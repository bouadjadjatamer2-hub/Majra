use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

use crate::browser::{base_headers, BrowserContext};
use crate::catalog::{Catalog, SavedDownload};
use crate::error::{MajraError, Result};
use crate::extract::{self, ExtractionSnapshot};
use crate::net::{self, InterfaceInfo, InterfaceKind};
use crate::plan::{self, Block, MAX_CONNECTIONS};
use crate::probe::{client_builder, probe_url_with, sanitize_file_name, ProbeResult};
use crate::state;

/// One write syscall's worth of data. Bigger chunks keep the writer out of the network's
/// way: at a gigabit the disk sees roughly a hundred writes a second, not four hundred.
const WRITE_CHUNK: usize = 1024 * 1024;
/// Chunks the workers may queue ahead of the writer before they wait for the disk.
const CHANNEL_CHUNKS: usize = 32;
const REPORT_MS: u64 = 250;
/// How often the resume map is written beside the partial file. Five seconds is far
/// more than a crash can lose and far less disk churn than a tick every other second.
const CHECKPOINT_SECONDS: u64 = 5;
/// Transient failures (a reset connection, a 503, a short body) get a few more tries than
/// before; permanent ones (403, 404, 416) fail on the first answer instead of burning time.
const MAX_RETRIES: u32 = 5;
/// A transfer that delivers nothing for this long is treated as hung: the current attempt
/// is cancelled and its missing blocks are requested again.
const STALL_SECONDS: u64 = 35;
/// Before the first byte there is nothing on screen to prove the app is alive, so a
/// silent server gets less patience than one that has already started sending.
const FIRST_BYTE_SECONDS: u64 = 20;
const MAX_STALL_RETRIES: u32 = 2;
/// Downloads running at once by default; the rest wait in the queue.
const DEFAULT_MAX_ACTIVE: usize = 0;
const MAX_ACTIVE_LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSnapshot {
    pub id: u32,
    pub interface_id: String,
    pub interface_name: String,
    pub downloaded_bytes: u64,
    pub speed_bytes_per_sec: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSnapshot {
    pub id: String,
    pub status: DownloadStatus,
    /// What the transfer is doing right now. `starting` until the first byte reaches
    /// the disk, then `downloading`; `reconnecting` after a stall restart, and
    /// `finalizing` while the last blocks are flushed and the file is closed.
    #[serde(default)]
    pub phase: String,
    pub url: String,
    pub file_name: String,
    pub destination: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub speed_bytes_per_sec: f64,
    pub eta_seconds: Option<u64>,
    pub connections: Vec<ConnectionSnapshot>,
    pub error: Option<String>,
    pub started_at: u64,
    pub completed_at: Option<u64>,
    /// Present when the request asked Majra to unpack the archive while downloading.
    pub extraction: Option<ExtractionSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    /// A full file path, or an existing folder. A folder gets the server's file name.
    pub destination: String,
    pub connections: u32,
    /// Empty means "let Windows choose the route".
    pub interface_ids: Vec<String>,
    pub block_size: Option<u64>,
    /// Unpack a ZIP archive while its bytes are still arriving.
    #[serde(default)]
    pub extract: bool,
    /// The name the browser picked, when only it saw a link's `download` attribute.
    #[serde(default)]
    pub file_name: Option<String>,
    /// The browser that handed the link over: its User-Agent, page and cookies.
    #[serde(default)]
    pub browser: Option<BrowserContext>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DownloadEvent {
    Progress { snapshot: DownloadSnapshot },
    Done { snapshot: DownloadSnapshot },
    Paused { snapshot: DownloadSnapshot },
    Cancelled { snapshot: DownloadSnapshot },
    Error { snapshot: DownloadSnapshot },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelKind {
    None,
    Paused,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Completed,
    Paused,
    Cancelled,
}

struct ConnectionStat {
    id: u32,
    interface_id: String,
    interface_name: String,
    downloaded: AtomicU64,
    status: Mutex<String>,
}

struct WriteChunk {
    offset: u64,
    data: Bytes,
    connection_id: u32,
}

struct Record {
    probe: ProbeResult,
    file_path: PathBuf,
    state_path: PathBuf,
    total_bytes: u64,
    block_size: u64,
    blocks: Vec<Block>,
    completed: Mutex<HashSet<u64>>,
    stats: Vec<Arc<ConnectionStat>>,
    clients: Vec<reqwest::Client>,
    /// An unbound client. If a selected adapter drops mid-transfer, its workers finish
    /// the block through this instead of failing the whole download.
    auto_client: reqwest::Client,
    /// False when the request already uses the Windows route; there is nothing to fall
    /// back to in that case.
    fallback_allowed: bool,
    /// The adapters the user picked, kept so the catalog can rebuild the request.
    interface_ids: Vec<String>,
    /// The browser's sign-in, kept for every request and for a resume after a restart.
    browser: Option<BrowserContext>,
    snapshot: Mutex<DownloadSnapshot>,
    cancel: Mutex<CancellationToken>,
    cancel_kind: Mutex<CancelKind>,
    task: Mutex<Option<JoinHandle<()>>>,
    /// Extraction pipeline. `prefix` is the contiguous byte prefix already on disk; the
    /// extractor can only consume bytes below it.
    extract: bool,
    extract_root: PathBuf,
    prefix: Arc<AtomicU64>,
    ranges: Mutex<BTreeMap<u64, u64>>,
    /// Byte ranges that the writer has actually written to the file. Checkpoints use this
    /// instead of `completed`: a block is only marked in `completed` once its bytes were
    /// handed to the writer, which can be a moment before they reach the disk.
    durable: Mutex<BTreeMap<u64, u64>>,
    extracted: Mutex<HashSet<String>>,
    /// Set when a server advertised range support but answered a range request with
    /// the whole file. `run_inner` then restarts the transfer as a single stream.
    range_rejected: AtomicBool,
}

impl Record {
    fn completed_bytes(&self) -> u64 {
        let completed = self.completed.lock().unwrap();
        self.blocks
            .iter()
            .filter(|block| completed.contains(&block.index))
            .map(Block::len)
            .sum()
    }

    /// Blocks whose every byte has reached the file. Safe to write into the resume state.
    fn durable_blocks(&self) -> HashSet<u64> {
        let ranges = self.durable.lock().unwrap();
        self.blocks
            .iter()
            .filter(|block| covers(&ranges, block.start, block.end + 1))
            .map(|block| block.index)
            .collect()
    }

    fn pending_blocks(&self) -> VecDeque<usize> {
        let completed = self.completed.lock().unwrap();
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| !completed.contains(&block.index))
            .map(|(index, _)| index)
            .collect()
    }

    /// Bytes covered by finished blocks, counted from the start of the file. A resumed
    /// download can reuse that prefix for extraction without waiting for new traffic.
    fn completed_prefix(&self) -> u64 {
        let completed = self.completed.lock().unwrap();
        let mut prefix = 0u64;
        for block in &self.blocks {
            if !completed.contains(&block.index) {
                break;
            }
            prefix += block.len();
        }
        prefix
    }
}

/// True when one of the merged ranges fully contains `[start, end)`.
fn covers(ranges: &BTreeMap<u64, u64>, start: u64, end: u64) -> bool {
    ranges
        .range(..=start)
        .next_back()
        .map_or(false, |(_, &range_end)| range_end >= end)
}

/// Merge a half-open byte range into a sorted, non-overlapping map.
fn merge_range(ranges: &mut BTreeMap<u64, u64>, mut start: u64, mut end: u64) {
    if let Some((&previous_start, &previous_end)) = ranges.range(..=start).next_back() {
        if previous_end >= start {
            start = previous_start;
            end = end.max(previous_end);
            ranges.remove(&previous_start);
        }
    }
    let overlapping = ranges
        .range(start..=end)
        .map(|(&key, _)| key)
        .collect::<Vec<_>>();
    for key in overlapping {
        if let Some(value) = ranges.remove(&key) {
            end = end.max(value);
        }
    }
    ranges.insert(start, end);
}

#[derive(Clone)]
pub struct DownloadManager {
    records: Arc<Mutex<HashMap<String, Arc<Record>>>>,
    /// Rows read from the catalog that have no live worker yet: restored after a restart.
    restored: Arc<Mutex<HashMap<String, SavedDownload>>>,
    catalog: Arc<Catalog>,
    /// Ids waiting for a free slot, oldest first. Only rows whose status is Queued sit here.
    queue: Arc<Mutex<VecDeque<String>>>,
    max_active: Arc<AtomicUsize>,
    /// Serialises "is there a free slot?" so two starts cannot both take the last one.
    admission: Arc<Mutex<()>>,
    events: broadcast::Sender<DownloadEvent>,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        Self::with_catalog(Catalog::load())
    }

    /// Tests and tools keep their history in a scratch file instead of the real one.
    pub fn with_catalog_path(path: PathBuf) -> Self {
        Self::with_catalog(Catalog::open(path))
    }

    fn with_catalog(catalog: Catalog) -> Self {
        let (events, _) = broadcast::channel(512);
        let restored = catalog
            .all()
            .into_iter()
            .filter(|entry| entry.status != DownloadStatus::Cancelled)
            .map(|entry| (entry.id.clone(), entry))
            .collect();
        Self {
            records: Arc::new(Mutex::new(HashMap::new())),
            restored: Arc::new(Mutex::new(restored)),
            catalog: Arc::new(catalog),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            max_active: Arc::new(AtomicUsize::new(DEFAULT_MAX_ACTIVE)),
            admission: Arc::new(Mutex::new(())),
            events,
        }
    }

    /// How many downloads run at the same time. Zero means no limit: every row starts
    /// immediately, which is what the app is expected to do unless the user asks for a
    /// queue. Any other value waits for a free slot, IDM style.
    pub fn set_max_active(&self, value: u32) {
        let value = (value as usize).min(MAX_ACTIVE_LIMIT);
        self.max_active.store(value, Ordering::Relaxed);
        self.fill_slots();
    }

    pub fn max_active(&self) -> usize {
        self.max_active.load(Ordering::Relaxed)
    }

    /// True when another row may start. Zero active limit means "as many as the user
    /// starts".
    fn has_free_slot(&self) -> bool {
        let limit = self.max_active();
        limit == 0 || self.active_count() < limit
    }

    /// Number of rows that are actually transferring right now.
    pub fn active_count(&self) -> usize {
        let records = self.records.lock().unwrap();
        records
            .values()
            .filter(|record| {
                let status = record.snapshot.lock().unwrap().status;
                matches!(status, DownloadStatus::Downloading)
            })
            .count()
    }

    fn active_count_except(&self, id: &str) -> usize {
        let records = self.records.lock().unwrap();
        records
            .iter()
            .filter(|(key, record)| {
                key.as_str() != id
                    && matches!(
                        record.snapshot.lock().unwrap().status,
                        DownloadStatus::Downloading
                    )
            })
            .count()
    }

    /// Start queued rows while the parallel limit has room. Called when a download stops,
    /// when the limit changes, and after a queued row is resumed. The admission gate makes
    /// the check-and-start atomic, so two workers finishing together cannot overshoot.
    fn fill_slots(&self) {
        loop {
            let _gate = self.admission.lock().unwrap();
            if !self.has_free_slot() {
                return;
            }
            let next = {
                let mut queue = self.queue.lock().unwrap();
                let mut found = None;
                while let Some(id) = queue.pop_front() {
                    if let Some(record) = self.records.lock().unwrap().get(&id).cloned() {
                        let queued = matches!(
                            record.snapshot.lock().unwrap().status,
                            DownloadStatus::Queued
                        );
                        if queued {
                            found = Some(record);
                            break;
                        }
                    }
                }
                found
            };
            let Some(record) = next else {
                return;
            };
            {
                let mut snapshot = record.snapshot.lock().unwrap();
                snapshot.status = DownloadStatus::Downloading;
                snapshot.error = None;
            }
            self.catalog.upsert(saved_from(&record));
            self.launch(record);
        }
    }

    /// Spawn the worker for a prepared record. The worker hands control back to
    /// `fill_slots` when it stops, so the queue keeps moving.
    fn launch(&self, record: Arc<Record>) {
        // Every launch is a fresh contact with the server, so the row shows "connecting"
        // until bytes actually land, not a stale phase from the previous attempt.
        set_phase(&record, "starting");
        let events = self.events.clone();
        let catalog = self.catalog.clone();
        let manager = self.clone();
        let worker = record.clone();
        let handle = tokio::spawn(async move {
            run(worker, events, catalog, manager).await;
        });
        *record.task.lock().unwrap() = Some(handle);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DownloadEvent> {
        self.events.subscribe()
    }

    pub async fn start(&self, request: DownloadRequest) -> Result<String> {
        self.start_inner(request, None, None).await
    }

    /// Start from a probe the browser bridge already made. The extension's link may be
    /// one-time or already signed, so it must not be fetched again just to plan the blocks.
    pub async fn start_with_probe(
        &self,
        request: DownloadRequest,
        probe: ProbeResult,
    ) -> Result<String> {
        self.start_inner(request, Some(probe), None).await
    }

    /// `reuse_id` keeps the row identity when a restored download resumes, so the list
    /// updates the same entry instead of showing a duplicate.
    async fn start_inner(
        &self,
        request: DownloadRequest,
        given_probe: Option<ProbeResult>,
        reuse_id: Option<String>,
    ) -> Result<String> {
        if request.url.trim().is_empty() {
            return Err(MajraError::Message("the link is empty".into()));
        }

        let browser = match request.browser.as_ref() {
            Some(context) => Some(context.validated().ok_or_else(|| {
                MajraError::Message(
                    "the browser's details for this download couldn't be read".into(),
                )
            })?),
            None => None,
        };
        let mut probe = match given_probe {
            Some(probe) if probe.requested_url == request.url => probe,
            _ => probe_url_with(&request.url, browser.as_ref()).await?,
        };
        if let Some(name) = request
            .file_name
            .as_deref()
            .map(sanitize_file_name)
            .filter(|name| !name.is_empty())
        {
            probe.file_name = name;
        }
        let total_bytes = probe.total_bytes.unwrap_or(0);
        let connections = request.connections.clamp(1, MAX_CONNECTIONS);
        let block_size = request
            .block_size
            .unwrap_or_else(|| plan::block_size_for(total_bytes, connections));
        let blocks = if probe.supports_ranges && total_bytes > 0 {
            plan::plan_blocks(total_bytes, block_size)
        } else {
            Vec::new()
        };

        let mut file_path = resolve_destination(&request.destination, &probe.file_name);
        // A fresh download never overwrites an existing file. A resume keeps its path
        // because its `.majra.json` state file proves the partial file is ours.
        if reuse_id.is_none() && file_path.exists() && !state_path_for(&file_path).exists() {
            file_path = unique_path(&file_path);
        }
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let state_path = state_path_for(&file_path);

        let completed = if probe.supports_ranges && total_bytes > 0 {
            let loaded = state::load_completed(&state_path, &probe, total_bytes, block_size).await;
            // The state file can outlive the file it describes (moved, deleted, truncated).
            // A block that reaches past the real file is not on disk, so it must be fetched
            // again instead of leaving a hole behind a "resumed" download.
            let file_len = tokio::fs::metadata(&file_path)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            loaded
                .into_iter()
                .filter(|index| {
                    blocks
                        .get(*index as usize)
                        .map_or(false, |block| block.end < file_len)
                })
                .collect()
        } else {
            HashSet::new()
        };

        let interfaces = selected_interfaces(&request.interface_ids);
        let stats = make_stats(&interfaces, connections);
        let (clients, auto_client) = build_clients(&interfaces);
        let fallback_allowed = interfaces.iter().any(|interface| interface.id != "auto");
        let id = reuse_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = now_ms();
        let extract_root = extract_root_for(&file_path);
        let extraction = request
            .extract
            .then(|| ExtractionSnapshot::new(&extract_root));
        let snapshot = DownloadSnapshot {
            id: id.clone(),
            status: DownloadStatus::Downloading,
            phase: "starting".into(),
            url: request.url.clone(),
            file_name: probe.file_name.clone(),
            destination: file_path.display().to_string(),
            total_bytes,
            downloaded_bytes: 0,
            speed_bytes_per_sec: 0.0,
            eta_seconds: None,
            connections: empty_connections(&stats),
            error: None,
            started_at: now,
            completed_at: None,
            extraction,
        };

        let mut durable = BTreeMap::new();
        for index in &completed {
            if let Some(block) = blocks.get(*index as usize) {
                merge_range(&mut durable, block.start, block.end + 1);
            }
        }

        let record = Arc::new(Record {
            probe,
            file_path,
            state_path,
            total_bytes,
            block_size,
            blocks,
            completed: Mutex::new(completed),
            stats,
            clients,
            auto_client,
            fallback_allowed,
            interface_ids: request.interface_ids.clone(),
            browser,
            snapshot: Mutex::new(snapshot),
            cancel: Mutex::new(CancellationToken::new()),
            cancel_kind: Mutex::new(CancelKind::None),
            task: Mutex::new(None),
            extract: request.extract,
            extract_root,
            prefix: Arc::new(AtomicU64::new(0)),
            ranges: Mutex::new(BTreeMap::new()),
            durable: Mutex::new(durable),
            extracted: Mutex::new(HashSet::new()),
            range_rejected: AtomicBool::new(false),
        });

        let start_now = {
            let _gate = self.admission.lock().unwrap();
            let free = self.has_free_slot();
            if !free {
                record.snapshot.lock().unwrap().status = DownloadStatus::Queued;
            }
            self.records
                .lock()
                .unwrap()
                .insert(id.clone(), record.clone());
            free
        };
        self.restored.lock().unwrap().remove(&id);

        // A resumed file already has bytes on disk; show them before the first report.
        let resumed = record.completed_bytes();
        if resumed > 0 {
            let mut snapshot = record.snapshot.lock().unwrap();
            snapshot.downloaded_bytes = resumed;
        }

        if start_now {
            self.launch(record.clone());
        } else {
            self.queue.lock().unwrap().push_back(id.clone());
        }
        self.catalog.upsert(saved_from(&record));

        // Publish the row now: a queued download has no reporter to do it for it.
        let snapshot = record.snapshot.lock().unwrap().clone();
        let _ = self.events.send(DownloadEvent::Progress { snapshot });

        Ok(id)
    }

    pub async fn pause(&self, id: &str) -> Result<()> {
        let record = match self.live(id) {
            Some(record) => record,
            None if self.restored.lock().unwrap().contains_key(id) => return Ok(()),
            None => return Err(MajraError::Message("download not found".into())),
        };
        // The admission gate makes "is it still queued?" and "mark it paused" atomic with
        // `fill_slots`, so a slot cannot start the row between the check and the pause.
        let paused_queued = {
            let _gate = self.admission.lock().unwrap();
            let mut snapshot = record.snapshot.lock().unwrap();
            if snapshot.status != DownloadStatus::Queued {
                false
            } else {
                self.queue.lock().unwrap().retain(|queued| queued != id);
                snapshot.status = DownloadStatus::Paused;
                snapshot.error = None;
                snapshot.speed_bytes_per_sec = 0.0;
                snapshot.eta_seconds = None;
                drop(snapshot);
                self.catalog.upsert(saved_from(&record));
                let snapshot = record.snapshot.lock().unwrap().clone();
                let _ = self.events.send(DownloadEvent::Paused { snapshot });
                true
            }
        };
        if paused_queued {
            return Ok(());
        }
        *record.cancel_kind.lock().unwrap() = CancelKind::Paused;
        record.cancel.lock().unwrap().cancel();
        let task = record.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
        Ok(())
    }

    pub async fn resume(&self, id: &str) -> Result<()> {
        let Some(record) = self.live(id) else {
            return self.resume_restored(id).await;
        };
        // A worker that just turned Error can still own the handle for a moment. Wait for
        // it before launching a second worker, or two workers would share one record.
        let previous = record.task.lock().unwrap().take();
        if let Some(previous) = previous {
            let _ = previous.await;
        }
        {
            let snapshot = record.snapshot.lock().unwrap();
            if !matches!(
                snapshot.status,
                DownloadStatus::Paused | DownloadStatus::Error | DownloadStatus::Queued
            ) {
                return Err(MajraError::Message(
                    "this download is not paused or failed".into(),
                ));
            }
        }
        self.queue.lock().unwrap().retain(|queued| queued != id);
        {
            let mut snapshot = record.snapshot.lock().unwrap();
            snapshot.status = DownloadStatus::Downloading;
            snapshot.error = None;
            snapshot.speed_bytes_per_sec = 0.0;
            snapshot.eta_seconds = None;
            snapshot.completed_at = None;
        }
        let token = CancellationToken::new();
        *record.cancel.lock().unwrap() = token;
        *record.cancel_kind.lock().unwrap() = CancelKind::None;

        // Resuming still respects the parallel limit: a full set sends the row to the
        // back of the queue instead of starting a fourth transfer.
        let start_now = {
            let _gate = self.admission.lock().unwrap();
            let limit = self.max_active();
            let free = limit == 0 || self.active_count_except(id) < limit;
            if !free {
                record.snapshot.lock().unwrap().status = DownloadStatus::Queued;
                self.queue.lock().unwrap().push_back(id.to_string());
            }
            free
        };
        self.catalog.upsert(saved_from(&record));
        if start_now {
            self.launch(record);
        } else {
            let snapshot = record.snapshot.lock().unwrap().clone();
            let _ = self.events.send(DownloadEvent::Progress { snapshot });
        }
        Ok(())
    }

    /// Pause every row that is transferring or waiting for a slot. The scheduler uses this
    /// at the end of its window; a toolbar action would take the same path.
    pub async fn pause_all(&self) {
        let ids: Vec<String> = self
            .list()
            .into_iter()
            .filter(|snapshot| {
                matches!(
                    snapshot.status,
                    DownloadStatus::Downloading | DownloadStatus::Queued
                )
            })
            .map(|snapshot| snapshot.id)
            .collect();
        for id in ids {
            let _ = self.pause(&id).await;
        }
    }

    /// Resume every paused or failed row, oldest first. Queued rows are already waiting and
    /// need no help. The parallel limit still applies, so this only fills the free slots.
    pub async fn resume_paused(&self) {
        let mut ids: Vec<(u64, String)> = self
            .list()
            .into_iter()
            .filter(|snapshot| {
                matches!(
                    snapshot.status,
                    DownloadStatus::Paused | DownloadStatus::Error
                )
            })
            .map(|snapshot| (snapshot.started_at, snapshot.id))
            .collect();
        ids.sort_by_key(|(started_at, _)| *started_at);
        for (_, id) in ids {
            let _ = self.resume(&id).await;
        }
    }

    /// True while at least one row is transferring or waiting for a free slot. Used by the
    /// "shut down when finished" check, which runs on every progress event, so this reads
    /// the status under the lock instead of cloning a snapshot per row.
    pub fn has_pending(&self) -> bool {
        let records = self.records.lock().unwrap();
        records.values().any(|record| {
            matches!(
                record.snapshot.lock().unwrap().status,
                DownloadStatus::Downloading | DownloadStatus::Queued
            )
        })
    }

    /// A row restored from the catalog has no worker. Rebuild its request and start it
    /// again with the same id, so the partial file and its block map are reused.
    async fn resume_restored(&self, id: &str) -> Result<()> {
        let saved = self
            .restored
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| MajraError::Message("download not found".into()))?;
        let request = DownloadRequest {
            url: saved.url,
            destination: saved.destination,
            connections: saved.connections,
            interface_ids: saved.interface_ids,
            block_size: None,
            extract: saved.extract,
            file_name: None,
            browser: saved.browser,
        };
        self.start_inner(request, None, Some(id.to_string()))
            .await?;
        Ok(())
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        let Some(record) = self.live(id) else {
            return self.cancel_restored(id).await;
        };
        *record.cancel_kind.lock().unwrap() = CancelKind::Cancelled;
        record.cancel.lock().unwrap().cancel();
        let task = record.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
            return Ok(());
        }

        // No worker is running: the download was queued, paused or failed. Mark it
        // cancelled and clear the partial file here, otherwise the row would stay queued.
        // The admission gate closes the window where `fill_slots` could start the row
        // between the status check and the cancel.
        let mut cancelled_snapshot = None;
        {
            let _gate = self.admission.lock().unwrap();
            let mut snapshot = record.snapshot.lock().unwrap();
            if matches!(
                snapshot.status,
                DownloadStatus::Queued | DownloadStatus::Paused | DownloadStatus::Error
            ) {
                self.queue.lock().unwrap().retain(|queued| queued != id);
                snapshot.status = DownloadStatus::Cancelled;
                snapshot.speed_bytes_per_sec = 0.0;
                snapshot.eta_seconds = None;
                snapshot.error = None;
                if let Some(extraction) = snapshot.extraction.as_mut() {
                    extraction.status = "cancelled".into();
                    extraction.current = None;
                }
                for connection in &mut snapshot.connections {
                    connection.status = "cancelled".into();
                    connection.speed_bytes_per_sec = 0.0;
                }
                cancelled_snapshot = Some(snapshot.clone());
            }
        }
        let Some(snapshot) = cancelled_snapshot else {
            // `fill_slots` won the race and a worker is starting; stop it normally.
            let task = record.task.lock().unwrap().take();
            if let Some(task) = task {
                let _ = task.await;
            }
            return Ok(());
        };
        let _ = record.task.lock().unwrap().take();
        *record.snapshot.lock().unwrap() = snapshot.clone();
        state::remove(&record.state_path).await;
        let _ = tokio::fs::remove_file(&record.file_path).await;
        let _ = self.events.send(DownloadEvent::Cancelled { snapshot });
        Ok(())
    }

    /// Cancel a row that only exists in the catalog. Its partial file and block map go too;
    /// a completed file is left on disk because cancel is not a delete.
    async fn cancel_restored(&self, id: &str) -> Result<()> {
        let saved = self
            .restored
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| MajraError::Message("download not found".into()))?;
        self.catalog.remove(id);

        if saved.status != DownloadStatus::Completed {
            let path = PathBuf::from(&saved.destination);
            state::remove(&state_path_for(&path)).await;
            let _ = tokio::fs::remove_file(&path).await;
        }

        let mut snapshot = saved_snapshot(&saved);
        snapshot.status = DownloadStatus::Cancelled;
        snapshot.error = None;
        let _ = self.events.send(DownloadEvent::Cancelled { snapshot });
        Ok(())
    }

    pub fn snapshot(&self, id: &str) -> Option<DownloadSnapshot> {
        if let Some(record) = self.live(id) {
            return Some(record.snapshot.lock().unwrap().clone());
        }
        self.restored.lock().unwrap().get(id).map(saved_snapshot)
    }

    pub fn list(&self) -> Vec<DownloadSnapshot> {
        let records = self.records.lock().unwrap();
        let mut snapshots = records
            .values()
            .map(|record| record.snapshot.lock().unwrap().clone())
            .collect::<Vec<_>>();
        for (id, saved) in self.restored.lock().unwrap().iter() {
            if !records.contains_key(id) {
                snapshots.push(saved_snapshot(saved));
            }
        }
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.started_at));
        snapshots
    }

    pub fn destination_of(&self, id: &str) -> Option<PathBuf> {
        if let Some(record) = self.live(id) {
            return Some(record.file_path.clone());
        }
        self.restored
            .lock()
            .unwrap()
            .get(id)
            .map(|saved| PathBuf::from(&saved.destination))
    }

    /// Drop a finished or cancelled record so a long session does not keep every
    /// snapshot (and its connection stats) in memory. The catalog row goes too.
    pub fn forget(&self, id: &str) -> bool {
        self.queue.lock().unwrap().retain(|queued| queued != id);
        let live = self.records.lock().unwrap().remove(id).is_some();
        let restored = self.restored.lock().unwrap().remove(id).is_some();
        if live || restored {
            self.catalog.remove(id);
        }
        live || restored
    }

    fn live(&self, id: &str) -> Option<Arc<Record>> {
        self.records.lock().unwrap().get(id).cloned()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The catalog row for a live record. Called after the snapshot lock is released.
fn saved_from(record: &Record) -> SavedDownload {
    let snapshot = record.snapshot.lock().unwrap();
    SavedDownload {
        id: snapshot.id.clone(),
        url: snapshot.url.clone(),
        file_name: snapshot.file_name.clone(),
        destination: record.file_path.display().to_string(),
        connections: record.stats.len() as u32,
        interface_ids: record.interface_ids.clone(),
        extract: record.extract,
        browser: record.browser.clone(),
        total_bytes: record.total_bytes,
        downloaded_bytes: snapshot.downloaded_bytes,
        status: snapshot.status,
        error: snapshot.error.clone(),
        started_at: snapshot.started_at,
        completed_at: snapshot.completed_at,
    }
}

/// A row read back at startup. A transfer that was running when the app closed comes
/// back as paused: nothing is fetching until the user asks for it.
fn saved_snapshot(saved: &SavedDownload) -> DownloadSnapshot {
    let status = match saved.status {
        DownloadStatus::Downloading | DownloadStatus::Queued => DownloadStatus::Paused,
        status => status,
    };
    DownloadSnapshot {
        id: saved.id.clone(),
        status,
        phase: String::new(),
        url: saved.url.clone(),
        file_name: saved.file_name.clone(),
        destination: saved.destination.clone(),
        total_bytes: saved.total_bytes,
        downloaded_bytes: saved.downloaded_bytes,
        speed_bytes_per_sec: 0.0,
        eta_seconds: None,
        connections: Vec::new(),
        error: saved.error.clone(),
        started_at: saved.started_at,
        completed_at: saved.completed_at,
        extraction: None,
    }
}

fn resolve_destination(raw: &str, file_name: &str) -> PathBuf {
    // Windows accepts `/` in its own file APIs but not every tool does (Defender's scanner
    // rejects it), so one shape is chosen here and kept for the whole download.
    let raw = if cfg!(windows) {
        raw.trim().replace('/', "\\")
    } else {
        raw.trim().to_string()
    };
    let raw = raw.as_str();
    if raw.is_empty() {
        return dirs::download_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(file_name);
    }
    let path = PathBuf::from(raw);
    if path.is_dir() || raw.ends_with('/') || raw.ends_with('\\') {
        return path.join(file_name);
    }
    if path.is_file() {
        return path;
    }
    // A path that does not exist yet: a dotted last segment is a file name, anything else
    // is a new folder. "D:\Media" stays a folder; "D:\Media\clip.mp4" stays a file.
    let last = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if last.contains('.') {
        path
    } else {
        path.join(file_name)
    }
}

/// `movie.mp4` -> `movie (1).mp4`, `movie (2).mp4`, ... until both the file and its
/// resume state are free. This keeps a second download from silently overwriting the
/// first one, which is the behavior a download manager user expects.
fn unique_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let (stem, extension) = match name.rfind('.') {
        Some(index) if index > 0 => (&name[..index], &name[index..]),
        _ => (name, ""),
    };
    for number in 1..10_000u32 {
        let candidate = parent.join(format!("{stem} ({number}){extension}"));
        if !candidate.exists() && !state_path_for(&candidate).exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}-{}{extension}", Uuid::new_v4()))
}

fn state_path_for(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".majra.json");
    PathBuf::from(name)
}

/// Archives unpack into a folder named after the archive, next to the archive itself.
/// A double suffix such as `.tar.gz` is removed as a whole, so the folder is `sample`,
/// not `sample.tar`.
fn extract_root_for(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    let name = file
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("extracted");
    let lower = name.to_ascii_lowercase();
    let stem = [".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".gz"]
        .iter()
        .find(|suffix| lower.ends_with(*suffix))
        .map(|suffix| &name[..name.len() - suffix.len()])
        .unwrap_or_else(|| {
            file.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("extracted")
        });
    parent.join(stem)
}

fn auto_interface() -> InterfaceInfo {
    InterfaceInfo {
        id: "auto".into(),
        name: "Auto".into(),
        address: String::new(),
        kind: InterfaceKind::Other,
        is_default: true,
        usable: false,
        network: None,
        link_speed_bps: None,
    }
}

fn selected_interfaces(ids: &[String]) -> Vec<InterfaceInfo> {
    choose_interfaces(net::list_interfaces(), ids)
}

/// Only adapters that Windows can route are accepted. A selected adapter that went down
/// since the dialog opened is skipped, and if nothing usable remains the request uses the
/// Windows route instead of failing every block.
fn choose_interfaces(all: Vec<InterfaceInfo>, ids: &[String]) -> Vec<InterfaceInfo> {
    if ids.is_empty() {
        return vec![auto_interface()];
    }
    let mut selected = all
        .into_iter()
        .filter(|interface| interface.usable && ids.iter().any(|id| id == &interface.id))
        .collect::<Vec<_>>();
    selected.dedup_by(|a, b| a.id == b.id);
    if selected.is_empty() {
        vec![auto_interface()]
    } else {
        selected
    }
}

fn make_stats(interfaces: &[InterfaceInfo], connections: u32) -> Vec<Arc<ConnectionStat>> {
    let interfaces = if interfaces.is_empty() {
        vec![auto_interface()]
    } else {
        interfaces.to_vec()
    };
    (0..connections)
        .map(|id| {
            let interface = &interfaces[id as usize % interfaces.len()];
            Arc::new(ConnectionStat {
                id,
                interface_id: interface.id.clone(),
                interface_name: interface.name.clone(),
                downloaded: AtomicU64::new(0),
                status: Mutex::new("idle".into()),
            })
        })
        .collect()
}

fn build_clients(interfaces: &[InterfaceInfo]) -> (Vec<reqwest::Client>, reqwest::Client) {
    let default = client_builder()
        .build()
        .expect("the default HTTP client must build");
    let clients = interfaces
        .iter()
        .map(|interface| {
            if interface.id == "auto" {
                return default.clone();
            }
            let Ok(address) = interface.address.parse::<IpAddr>() else {
                return default.clone();
            };
            client_builder()
                .local_address(address)
                .build()
                .unwrap_or_else(|_| default.clone())
        })
        .collect();
    (clients, default)
}

fn empty_connections(stats: &[Arc<ConnectionStat>]) -> Vec<ConnectionSnapshot> {
    stats
        .iter()
        .map(|stat| ConnectionSnapshot {
            id: stat.id,
            interface_id: stat.interface_id.clone(),
            interface_name: stat.interface_name.clone(),
            downloaded_bytes: 0,
            speed_bytes_per_sec: 0.0,
            status: "idle".into(),
        })
        .collect()
}

async fn run(
    record: Arc<Record>,
    events: broadcast::Sender<DownloadEvent>,
    catalog: Arc<Catalog>,
    manager: DownloadManager,
) {
    let cancel = record.cancel.lock().unwrap().clone();
    let downloaded = Arc::new(AtomicU64::new(record.completed_bytes()));
    let stop = CancellationToken::new();
    let reporter = tokio::spawn(reporter(
        record.clone(),
        downloaded.clone(),
        stop.clone(),
        events.clone(),
        catalog.clone(),
    ));

    // Extraction runs beside the transfer. It consumes only bytes below the contiguous
    // prefix, so it never reads a hole that a later block has not filled yet.
    let extract_stop = CancellationToken::new();
    let extractor = if record.extract {
        Some(tokio::spawn(progressive_extract(
            record.clone(),
            cancel.clone(),
            extract_stop.clone(),
        )))
    } else {
        None
    };

    // A dead socket can stay open without ever delivering another byte. Run each attempt
    // under a watchdog; when nothing reaches the disk for a while, drop that attempt and
    // ask for the missing blocks again. Completed blocks stay on disk, so this resumes
    // instead of starting over.
    let mut stall_retries = 0u32;
    let result = loop {
        let attempt_cancel = cancel.child_token();
        let stalled = Arc::new(AtomicBool::new(false));
        let watchdog = {
            let downloaded = downloaded.clone();
            let stalled = stalled.clone();
            let token = attempt_cancel.clone();
            tokio::spawn(async move {
                let mut last = downloaded.load(Ordering::Relaxed);
                let mut idle = Duration::ZERO;
                loop {
                    tokio::select! {
                        _ = token.cancelled() => break,
                        _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    }
                    let now = downloaded.load(Ordering::Relaxed);
                    if now > last {
                        last = now;
                        idle = Duration::ZERO;
                    } else {
                        idle += Duration::from_secs(2);
                    }
                    // A transfer that has not produced its first byte yet is far more
                    // likely to be dead than slow, so it gets the shorter window.
                    let limit = if now == 0 {
                        Duration::from_secs(FIRST_BYTE_SECONDS)
                    } else {
                        Duration::from_secs(STALL_SECONDS)
                    };
                    if idle >= limit {
                        stalled.store(true, Ordering::SeqCst);
                        token.cancel();
                        break;
                    }
                }
            })
        };
        let mut attempt = run_inner(&record, &attempt_cancel, &downloaded).await;
        watchdog.abort();
        if matches!(attempt, Err(MajraError::Cancelled)) {
            attempt = Ok(outcome(&record));
        }
        let hung = stalled.load(Ordering::SeqCst)
            && !cancel.is_cancelled()
            && matches!(*record.cancel_kind.lock().unwrap(), CancelKind::None);
        if hung && stall_retries < MAX_STALL_RETRIES {
            stall_retries += 1;
            set_phase(&record, "reconnecting");
            continue;
        }
        if hung {
            // The host answered the probe but stopped sending. Say so instead of leaving
            // a row that looks busy at 0 %; Resume starts from the bytes already on disk.
            attempt = Err(MajraError::Message(
                "the server stopped sending data. Press Resume to try again.".into(),
            ));
        }
        break attempt;
    };

    extract_stop.cancel();
    if let Some(handle) = extractor {
        let _ = handle.await;
    }

    if record.extract {
        match &result {
            Ok(Outcome::Completed) => finish_extract(&record, &cancel).await,
            Ok(Outcome::Paused) => set_extraction_status(&record, "paused"),
            Ok(Outcome::Cancelled) => set_extraction_status(&record, "cancelled"),
            Err(_) => set_extraction_status(&record, "error"),
        }
    }

    stop.cancel();
    let _ = reporter.await;

    let downloaded_now = downloaded.load(Ordering::Relaxed);
    let mut snapshot = record.snapshot.lock().unwrap().clone();
    let record_id = snapshot.id.clone();
    snapshot.speed_bytes_per_sec = 0.0;
    snapshot.eta_seconds = None;
    snapshot.downloaded_bytes = if record.total_bytes > 0 {
        downloaded_now.min(record.total_bytes)
    } else {
        downloaded_now
    };

    let cancelled = matches!(result, Ok(Outcome::Cancelled));
    let event = match result {
        Ok(Outcome::Completed) => {
            if record.total_bytes > 0 {
                snapshot.downloaded_bytes = record.total_bytes;
            }
            snapshot.status = DownloadStatus::Completed;
            snapshot.error = None;
            snapshot.completed_at = Some(now_ms());
            for connection in &mut snapshot.connections {
                connection.status = "done".into();
                connection.speed_bytes_per_sec = 0.0;
            }
            state::remove(&record.state_path).await;
            DownloadEvent::Done { snapshot }
        }
        Ok(Outcome::Paused) => {
            snapshot.status = DownloadStatus::Paused;
            snapshot.error = None;
            for connection in &mut snapshot.connections {
                connection.status = "paused".into();
                connection.speed_bytes_per_sec = 0.0;
            }
            let completed = record.durable_blocks();
            let _ = state::save(
                &record.state_path,
                &record.probe,
                record.total_bytes,
                record.block_size,
                &completed,
            )
            .await;
            DownloadEvent::Paused { snapshot }
        }
        Ok(Outcome::Cancelled) => {
            snapshot.status = DownloadStatus::Cancelled;
            snapshot.error = None;
            for connection in &mut snapshot.connections {
                connection.status = "cancelled".into();
                connection.speed_bytes_per_sec = 0.0;
            }
            state::remove(&record.state_path).await;
            let _ = tokio::fs::remove_file(&record.file_path).await;
            DownloadEvent::Cancelled { snapshot }
        }
        Err(error) => {
            snapshot.status = DownloadStatus::Error;
            snapshot.error = Some(error.to_string());
            for connection in &mut snapshot.connections {
                connection.status = "error".into();
                connection.speed_bytes_per_sec = 0.0;
            }
            let completed = record.durable_blocks();
            let _ = state::save(
                &record.state_path,
                &record.probe,
                record.total_bytes,
                record.block_size,
                &completed,
            )
            .await;
            DownloadEvent::Error { snapshot }
        }
    };

    *record.snapshot.lock().unwrap() = match &event {
        DownloadEvent::Progress { snapshot }
        | DownloadEvent::Done { snapshot }
        | DownloadEvent::Paused { snapshot }
        | DownloadEvent::Cancelled { snapshot }
        | DownloadEvent::Error { snapshot } => snapshot.clone(),
    };
    if cancelled {
        catalog.remove(&record_id);
    } else {
        catalog.upsert(saved_from(&record));
    }
    let _ = events.send(event);

    // This row stopped (finished, paused, failed or cancelled): queued rows may start.
    manager.fill_slots();
}

fn outcome(record: &Record) -> Outcome {
    match *record.cancel_kind.lock().unwrap() {
        CancelKind::Cancelled => Outcome::Cancelled,
        _ => Outcome::Paused,
    }
}

fn set_phase(record: &Record, phase: &str) {
    let mut snapshot = record.snapshot.lock().unwrap();
    if snapshot.phase != phase {
        snapshot.phase = phase.into();
    }
}

fn update_extraction(record: &Record, change: impl FnOnce(&mut ExtractionSnapshot)) {
    let mut snapshot = record.snapshot.lock().unwrap();
    if let Some(extraction) = snapshot.extraction.as_mut() {
        change(extraction);
    }
}

fn set_extraction_status(record: &Record, status: &str) {
    update_extraction(record, |extraction| {
        extraction.status = status.into();
        extraction.current = None;
        if status != "error" {
            extraction.error = None;
        }
    });
}

/// Record that `length` bytes at `offset` reached disk, then advance the contiguous prefix.
/// Returns only the bytes that were not already present, so a retried block does not
/// inflate the progress counter when it overwrites its own partial data.
fn note_written(record: &Record, offset: u64, length: u64) -> u64 {
    if length == 0 {
        return 0;
    }
    let mut ranges = record.ranges.lock().unwrap();
    let before = covered_bytes(&ranges);
    let mut start = offset;
    let mut end = offset.saturating_add(length);

    if let Some((&previous_start, &previous_end)) = ranges.range(..=start).next_back() {
        if previous_end >= start {
            start = previous_start;
            end = end.max(previous_end);
            ranges.remove(&previous_start);
        }
    }

    let mut absorbed = Vec::new();
    for (&next_start, &next_end) in ranges.range(start..) {
        if next_start > end {
            break;
        }
        end = end.max(next_end);
        absorbed.push(next_start);
    }
    for key in absorbed {
        ranges.remove(&key);
    }
    ranges.insert(start, end);
    let after = covered_bytes(&ranges);

    let mut prefix = record.prefix.load(Ordering::Relaxed);
    loop {
        let next = ranges.get(&prefix).copied();
        let Some(next) = next else {
            break;
        };
        ranges.remove(&prefix);
        prefix = next;
    }
    record.prefix.store(prefix, Ordering::Relaxed);
    after.saturating_sub(before)
}

fn covered_bytes(ranges: &BTreeMap<u64, u64>) -> u64 {
    ranges
        .iter()
        .map(|(start, end)| end.saturating_sub(*start))
        .sum()
}

async fn wait_prefix(
    record: &Record,
    needed: u64,
    cancel: &CancellationToken,
    stop: &CancellationToken,
) -> bool {
    loop {
        if cancel.is_cancelled() || stop.is_cancelled() {
            return false;
        }
        if record.prefix.load(Ordering::Relaxed) >= needed {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
}

async fn progressive_extract(
    record: Arc<Record>,
    cancel: CancellationToken,
    stop: CancellationToken,
) {
    set_extraction_status(&record, "running");
    let mut offset = 0u64;
    let mut entries_done = 0u64;
    let mut bytes_written = 0u64;

    // A ZIP local header is 30 bytes; a TAR header is 512. Wait for the larger of the two
    // once, then decide whether this is a format the progressive pass understands.
    if !wait_prefix(&record, 512, &cancel, &stop).await {
        return;
    }
    let kind_path = record.file_path.clone();
    let kind = tokio::task::spawn_blocking(move || extract::detect(&kind_path)).await;
    match kind {
        Ok(extract::ArchiveKind::Zip) | Ok(extract::ArchiveKind::Tar) => {}
        // A gzip stream cannot be resumed in the middle; the final pass unpacks it.
        _ => return,
    }

    loop {
        if !wait_prefix(&record, offset.saturating_add(30), &cancel, &stop).await {
            return;
        }

        // Read the header first, then wait for the whole entry. Reading it early would see
        // the preallocated zeros that sit beyond the contiguous prefix.
        let path = record.file_path.clone();
        let requirement =
            tokio::task::spawn_blocking(move || extract::local_requirement(&path, offset)).await;
        let needed = match requirement {
            Ok(Ok(Some(needed))) => needed,
            Ok(Ok(None)) => return,
            Ok(Err(error)) => {
                if !cancel.is_cancelled() && !stop.is_cancelled() {
                    update_extraction(&record, |extraction| {
                        extraction.status = "error".into();
                        extraction.error = Some(error.to_string());
                    });
                }
                return;
            }
            Err(error) => {
                if !cancel.is_cancelled() && !stop.is_cancelled() {
                    update_extraction(&record, |extraction| {
                        extraction.status = "error".into();
                        extraction.error = Some(format!("extraction stopped: {error}"));
                    });
                }
                return;
            }
        };
        if !wait_prefix(&record, needed, &cancel, &stop).await {
            return;
        }

        let path = record.file_path.clone();
        let root = record.extract_root.clone();
        let token = cancel.clone();
        let step =
            tokio::task::spawn_blocking(move || extract::local_step(&path, offset, &root, &token))
                .await;

        match step {
            Ok(Ok(extract::LocalStep::Extracted {
                next,
                name,
                written,
            })) => {
                record.extracted.lock().unwrap().insert(name.clone());
                entries_done += 1;
                bytes_written = bytes_written.saturating_add(written);
                offset = next;
                update_extraction(&record, |extraction| {
                    extraction.entries_done = entries_done;
                    extraction.bytes_written = bytes_written;
                    extraction.current = Some(name);
                });
            }
            Ok(Ok(extract::LocalStep::Skipped { next })) => {
                offset = next;
            }
            Ok(Ok(extract::LocalStep::Stop)) => return,
            Ok(Err(error)) => {
                if !cancel.is_cancelled() && !stop.is_cancelled() {
                    update_extraction(&record, |extraction| {
                        extraction.status = "error".into();
                        extraction.error = Some(error.to_string());
                    });
                }
                return;
            }
            Err(error) => {
                if !cancel.is_cancelled() && !stop.is_cancelled() {
                    update_extraction(&record, |extraction| {
                        extraction.status = "error".into();
                        extraction.error = Some(format!("extraction stopped: {error}"));
                    });
                }
                return;
            }
        }
    }
}

/// Final pass: read the central directory, fill the gaps the progressive pass left, and
/// report the exact entry count and uncompressed size.
async fn finish_extract(record: &Arc<Record>, cancel: &CancellationToken) {
    if !extract::is_archive(&record.file_path) {
        update_extraction(record, |extraction| {
            extraction.status = "skipped".into();
            extraction.current = None;
            extraction.error = None;
        });
        return;
    }
    let (base_entries, base_bytes) = {
        let snapshot = record.snapshot.lock().unwrap();
        match snapshot.extraction.as_ref() {
            Some(extraction) => (extraction.entries_done, extraction.bytes_written),
            None => return,
        }
    };
    let path = record.file_path.clone();
    let root = record.extract_root.clone();
    let skip = record.extracted.lock().unwrap().clone();
    let token = cancel.clone();
    let progress_record = record.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut progress = |name: &str, done: u64, bytes: u64, total: Option<u64>| {
            update_extraction(&progress_record, |extraction| {
                extraction.entries_done = base_entries.saturating_add(done);
                // A gzip stream has no index, so its total stays unknown until the end.
                if total.is_some() {
                    extraction.entries_total = total;
                }
                extraction.bytes_written = base_bytes.saturating_add(bytes);
                extraction.current = Some(name.to_string());
            });
        };
        extract::extract_remaining(&path, &root, &skip, &token, &mut progress)
    })
    .await;

    match result {
        Ok(Ok((total, _done, bytes, bytes_total))) => {
            update_extraction(record, |extraction| {
                extraction.status = "done".into();
                extraction.entries_total = Some(total);
                // Entries that were safely skipped (traversal, directories) still count as
                // processed; the bar must reach 100% when the pass is complete.
                extraction.entries_done = total;
                extraction.bytes_total = Some(bytes_total);
                extraction.bytes_written = base_bytes.saturating_add(bytes);
                extraction.current = None;
                extraction.error = None;
            });
        }
        Ok(Err(error)) => {
            update_extraction(record, |extraction| {
                extraction.status = "error".into();
                extraction.current = None;
                extraction.error = Some(error.to_string());
            });
        }
        Err(error) => {
            update_extraction(record, |extraction| {
                extraction.status = "error".into();
                extraction.current = None;
                extraction.error = Some(format!("extraction stopped: {error}"));
            });
        }
    }
}

async fn run_inner(
    record: &Arc<Record>,
    cancel: &CancellationToken,
    downloaded: &Arc<AtomicU64>,
) -> Result<Outcome> {
    if record.blocks.is_empty() {
        return run_single(record, cancel, downloaded).await;
    }

    // A resumed download already owns the leading blocks; seed the extractor with them so
    // it does not wait for traffic that will never be sent again.
    record.ranges.lock().unwrap().clear();
    record
        .prefix
        .store(record.completed_prefix(), Ordering::Relaxed);

    {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&record.file_path)
            .await?;
        if file.metadata().await?.len() != record.total_bytes {
            file.set_len(record.total_bytes).await?;
        }
    }

    let (tx, rx) = mpsc::channel::<WriteChunk>(CHANNEL_CHUNKS);
    let fatal: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let writer = {
        let record = record.clone();
        let fatal = fatal.clone();
        std::thread::Builder::new()
            .name("majra-disk-writer".into())
            .spawn(move || writer_task(record, rx, fatal))?
    };

    let queue = Arc::new(Mutex::new(record.pending_blocks()));
    let headers = Arc::new(base_headers(
        record.browser.as_ref(),
        &Url::parse(&record.probe.final_url)?,
    ));
    let mut workers = Vec::new();
    for index in 0..record.stats.len() {
        let context = WorkerContext {
            client: record.clients[index % record.clients.len()].clone(),
            fallback: record.auto_client.clone(),
            can_fallback: record.fallback_allowed,
            headers: headers.clone(),
            downloaded: downloaded.clone(),
        };
        let stat = record.stats[index].clone();
        let queue = queue.clone();
        let tx = tx.clone();
        let record = record.clone();
        let cancel = cancel.clone();
        let fatal = fatal.clone();
        workers.push(tokio::spawn(async move {
            worker(record, context, stat, queue, tx, cancel, fatal).await
        }));
    }
    drop(tx);

    let mut worker_error = None;
    for handle in workers {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => worker_error = Some(error),
            Err(error) => {
                worker_error = Some(MajraError::Message(format!("worker stopped: {error}")))
            }
        }
    }

    // Everything is in the channel now; the writer flushes the file before it exits, and
    // that flush is what "finalizing" covers on a slow disk.
    if !cancel.is_cancelled() {
        set_phase(record, "finalizing");
    }
    let writer_result = writer
        .join()
        .map_err(|_| MajraError::Message("the disk writer stopped unexpectedly".into()))?;
    if let Some(error) = fatal.lock().unwrap().clone() {
        return Err(MajraError::Message(error));
    }
    if let Some(error) = worker_error {
        return Err(error);
    }
    writer_result.map_err(|error| MajraError::Message(format!("writer stopped: {error}")))?;

    if record.range_rejected.load(Ordering::SeqCst) && !cancel.is_cancelled() {
        // The server ignored the range header. The only correct answer is a single
        // stream from byte zero, which is what a browser would do.
        record.range_rejected.store(false, Ordering::SeqCst);
        reset_single(record, downloaded);
        set_phase(record, "downloading");
        return run_single(record, cancel, downloaded).await;
    }

    if cancel.is_cancelled() {
        return Ok(outcome(record));
    }
    if downloaded.load(Ordering::Relaxed) < record.total_bytes {
        return Err(MajraError::ShortRead);
    }
    Ok(Outcome::Completed)
}

/// Transient failures are worth another request. A 403, 404 or 416 will answer the same
/// way every time, so retrying it only wastes seconds before the real error is shown.
fn retryable(error: &MajraError) -> bool {
    match error {
        MajraError::Network(_) | MajraError::ShortRead => true,
        MajraError::Http(code) => matches!(*code, 408 | 425 | 429 | 500..=599),
        _ => false,
    }
}

/// Exponential backoff with a little jitter, so a burst of workers that fail together do
/// not all come back at the same instant. Capped at eight seconds.
fn retry_delay(attempt: u32, salt: u64) -> Duration {
    let base = 300u64.saturating_mul(1u64 << attempt.min(5));
    let jitter = salt
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(u64::from(attempt).wrapping_mul(1_442_695_040_888_963_407))
        >> 33;
    Duration::from_millis((base + jitter % 250).min(8_000))
}

/// The per-worker HTTP client and the headers every range request carries. Bundled so a
/// worker keeps its arguments readable.
struct WorkerContext {
    client: reqwest::Client,
    fallback: reqwest::Client,
    can_fallback: bool,
    headers: Arc<Vec<(&'static str, String)>>,
    downloaded: Arc<AtomicU64>,
}

async fn worker(
    record: Arc<Record>,
    context: WorkerContext,
    stat: Arc<ConnectionStat>,
    queue: Arc<Mutex<VecDeque<usize>>>,
    tx: mpsc::Sender<WriteChunk>,
    cancel: CancellationToken,
    fatal: Arc<Mutex<Option<String>>>,
) -> Result<()> {
    // A worker starts on its chosen adapter. If that adapter loses its route, the same
    // block continues through the Windows route instead of burning every retry and
    // failing the whole download.
    let mut client = context.client.clone();
    let mut used_fallback = false;
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let next = { queue.lock().unwrap().pop_front() };
        let Some(index) = next else {
            *stat.status.lock().unwrap() = "done".into();
            return Ok(());
        };
        let block = record.blocks[index];
        *stat.status.lock().unwrap() = "downloading".into();
        let mut attempt = 0u32;
        loop {
            if cancel.is_cancelled() {
                return Ok(());
            }
            match download_block(
                &client,
                &record.probe.final_url,
                block,
                &tx,
                &cancel,
                stat.id,
                &context.headers,
                &context.downloaded,
            )
            .await
            {
                Ok(()) => {
                    record.completed.lock().unwrap().insert(block.index);
                    *stat.status.lock().unwrap() = "idle".into();
                    break;
                }
                Err(MajraError::Cancelled) => return Ok(()),
                Err(MajraError::RangeRejected) => {
                    // The probe saw "Accept-Ranges" but this host answers with the whole
                    // file anyway. Stop this worker; `run_inner` restarts as one stream.
                    record.range_rejected.store(true, Ordering::SeqCst);
                    *stat.status.lock().unwrap() = "done".into();
                    return Ok(());
                }
                Err(error) => {
                    if !used_fallback && context.can_fallback && matches!(error, MajraError::Network(_))
                    {
                        used_fallback = true;
                        client = context.fallback.clone();
                        *stat.status.lock().unwrap() = "fallback".into();
                        continue;
                    }
                    if !retryable(&error) || attempt >= MAX_RETRIES {
                        *fatal.lock().unwrap() = Some(if attempt > 0 {
                            format!(
                                "block {} failed after {} attempts: {error}",
                                block.index,
                                attempt + 1
                            )
                        } else {
                            format!("block {} failed: {error}", block.index)
                        });
                        cancel.cancel();
                        return Ok(());
                    }
                    attempt += 1;
                    *stat.status.lock().unwrap() = format!("retry {attempt}");
                    tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(retry_delay(attempt, block.index)) => {}
                    }
                }
            }
        }
    }
}

async fn download_block(
    client: &reqwest::Client,
    url: &str,
    block: Block,
    tx: &mpsc::Sender<WriteChunk>,
    cancel: &CancellationToken,
    connection_id: u32,
    headers: &[(&'static str, String)],
    downloaded: &Arc<AtomicU64>,
) -> Result<()> {
    let mut request = client.get(url).header(
        reqwest::header::RANGE,
        format!("bytes={}-{}", block.start, block.end),
    );
    for (name, value) in headers {
        request = request.header(*name, value.as_str());
    }
    let response = request.send().await?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(MajraError::RangeRejected);
    }

    let mut stream = response.bytes_stream();
    let mut offset = block.start;
    let mut buffer = Vec::with_capacity(WRITE_CHUNK);
    // Progress counts bytes the moment they arrive, not when a whole 1 MB write lands.
    // A slow host that trickles a kilobyte at a time is alive, and the window has to say
    // so; otherwise a big file from a slow server looks dead and the watchdog agrees.
    // A failed attempt rolls its bytes back, so a retry is never counted twice.
    let mut seen = 0u64;
    macro_rules! give_up {
        ($error:expr) => {{
            // Bytes already handed to the disk writer are still flushed when the
            // channel closes, so only the un-sent tail is rolled back here.
            let sent = offset.saturating_sub(block.start);
            downloaded.fetch_sub(seen.saturating_sub(sent), Ordering::Relaxed);
            return Err($error);
        }};
    }
    while let Some(item) = stream.next().await {
        if cancel.is_cancelled() {
            give_up!(MajraError::Cancelled);
        }
        let chunk = match item {
            Ok(chunk) => chunk,
            Err(error) => give_up!(error.into()),
        };
        if chunk.is_empty() {
            continue;
        }
        if offset + chunk.len() as u64 > block.end + 1 {
            give_up!(MajraError::Message(
                "the server sent more bytes than the range asked for".into(),
            ));
        }
        seen += chunk.len() as u64;
        downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        buffer.extend_from_slice(&chunk);
        if buffer.len() >= WRITE_CHUNK {
            let data = Bytes::from(std::mem::replace(
                &mut buffer,
                Vec::with_capacity(WRITE_CHUNK),
            ));
            let length = data.len() as u64;
            if tx
                .send(WriteChunk {
                    offset,
                    data,
                    connection_id,
                })
                .await
                .is_err()
            {
                give_up!(MajraError::Message("the disk writer stopped".into()));
            }
            offset += length;
        }
    }
    if !buffer.is_empty() {
        let data = Bytes::from(buffer);
        let length = data.len() as u64;
        if tx
            .send(WriteChunk {
                offset,
                data,
                connection_id,
            })
            .await
            .is_err()
        {
            give_up!(MajraError::Message("the disk writer stopped".into()));
        }
        offset += length;
    }
    if offset != block.end + 1 {
        give_up!(MajraError::ShortRead);
    }
    Ok(())
}

/// Writes are handed to one operating-system thread that owns the file. Positioned writes
/// mean no seek before every chunk, the async runtime never blocks on the disk, and a
/// gigabit of traffic costs about a hundred syscalls a second instead of four hundred.
fn writer_task(
    record: Arc<Record>,
    mut rx: mpsc::Receiver<WriteChunk>,
    fatal: Arc<Mutex<Option<String>>>,
) -> std::io::Result<()> {
    let file = open_partial(&record.file_path)?;

    // Drain every chunk that was already accepted, even after a pause: a block is only
    // marked complete once its bytes have left the channel, so discarding the tail here
    // would leave a hole in a block that resume then skips.
    while let Some(chunk) = rx.blocking_recv() {
        if let Err(error) = write_at(&file, &chunk.data, chunk.offset) {
            *fatal.lock().unwrap() = Some(format!("could not write the file: {error}"));
            // Nothing on disk can be trusted after a write failure: force a full retry.
            record.completed.lock().unwrap().clear();
            record.durable.lock().unwrap().clear();
            record.cancel.lock().unwrap().cancel();
            return Err(error);
        }
        let length = chunk.data.len() as u64;
        merge_range(
            &mut record.durable.lock().unwrap(),
            chunk.offset,
            chunk.offset + length,
        );
        // Progress was already counted when these bytes arrived; the writer only keeps
        // the durable map and the per-connection totals in step with the disk.
        let added = note_written(&record, chunk.offset, length);
        if let Some(stat) = record.stats.get(chunk.connection_id as usize) {
            stat.downloaded.fetch_add(added, Ordering::Relaxed);
        }
    }
    file.sync_all()
}

/// Open the partial file for the writer. On Windows the sequential-scan hint tells the
/// cache manager this file only grows forward, which keeps a multi-gigabyte transfer from
/// pushing everything else out of the standby list.
fn open_partial(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;
        options.custom_flags(FILE_FLAG_SEQUENTIAL_SCAN);
    }
    options.open(path)
}

/// One positioned write, repeated until the whole chunk is on disk. `seek_write`/`write_at`
/// take the offset with the buffer, so parallel blocks do not need to move a shared cursor.
fn write_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    use std::os::windows::fs::FileExt;
    #[cfg(unix)]
    use std::os::unix::fs::FileExt;

    let mut written = 0usize;
    while written < data.len() {
        #[cfg(target_os = "windows")]
        let count = file.seek_write(&data[written..], offset + written as u64)?;
        #[cfg(unix)]
        let count = file.write_at(&data[written..], offset + written as u64)?;
        #[cfg(not(any(target_os = "windows", unix)))]
        let count = {
            use std::io::{Seek, SeekFrom, Write};
            let mut handle = file.try_clone()?;
            handle.seek(SeekFrom::Start(offset + written as u64))?;
            handle.write(&data[written..])?
        };
        if count == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        written += count;
    }
    Ok(())
}

async fn run_single(
    record: &Arc<Record>,
    cancel: &CancellationToken,
    downloaded: &Arc<AtomicU64>,
) -> Result<Outcome> {
    let mut client = record.clients[0].clone();
    let mut used_fallback = false;
    let mut attempt = 0u32;
    loop {
        if cancel.is_cancelled() {
            return Ok(outcome(record));
        }
        match run_single_once(record, cancel, downloaded, &client).await {
            Err(MajraError::Network(_)) if !used_fallback && record.fallback_allowed => {
                used_fallback = true;
                client = record.auto_client.clone();
                reset_single(record, downloaded);
            }
            Err(error) if retryable(&error) && attempt < MAX_RETRIES => {
                attempt += 1;
                reset_single(record, downloaded);
                if let Some(stat) = record.stats.first() {
                    *stat.status.lock().unwrap() = format!("retry {attempt}");
                }
                tokio::select! {
                    _ = cancel.cancelled() => return Ok(outcome(record)),
                    _ = tokio::time::sleep(retry_delay(attempt, 0)) => {}
                }
            }
            other => return other,
        }
    }
}

/// A non-range server cannot resume, so a retry starts the file over. Reset the counters
/// that would otherwise show bytes from the abandoned attempt.
fn reset_single(record: &Record, downloaded: &AtomicU64) {
    downloaded.store(0, Ordering::Relaxed);
    for stat in &record.stats {
        stat.downloaded.store(0, Ordering::Relaxed);
        *stat.status.lock().unwrap() = "downloading".into();
    }
    record.ranges.lock().unwrap().clear();
    record.prefix.store(0, Ordering::Relaxed);
}

async fn run_single_once(
    record: &Arc<Record>,
    cancel: &CancellationToken,
    downloaded: &Arc<AtomicU64>,
    client: &reqwest::Client,
) -> Result<Outcome> {
    record.ranges.lock().unwrap().clear();
    record.prefix.store(0, Ordering::Relaxed);
    let headers = base_headers(
        record.browser.as_ref(),
        &Url::parse(&record.probe.final_url)?,
    );
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(&record.file_path)
        .await?;
    let mut request = client.get(&record.probe.final_url);
    for (name, value) in &headers {
        request = request.header(*name, value.as_str());
    }
    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(MajraError::Http(response.status().as_u16()));
    }

    let total = response
        .content_length()
        .or(record.probe.total_bytes)
        .unwrap_or(0);
    if total > 0 {
        file.set_len(total).await?;
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::with_capacity(WRITE_CHUNK);
    let mut written = 0u64;
    if let Some(stat) = record.stats.first() {
        *stat.status.lock().unwrap() = "downloading".into();
    }

    while let Some(item) = stream.next().await {
        if cancel.is_cancelled() {
            downloaded.fetch_sub(buffer.len() as u64, Ordering::Relaxed);
            file.sync_all().await?;
            return Ok(outcome(record));
        }
        let chunk = match item {
            Ok(chunk) => chunk,
            Err(error) => {
                downloaded.fetch_sub(buffer.len() as u64, Ordering::Relaxed);
                return Err(error.into());
            }
        };
        downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        buffer.extend_from_slice(&chunk);
        if buffer.len() >= WRITE_CHUNK {
            file.write_all(&buffer).await?;
            let length = buffer.len() as u64;
            let offset = written;
            written += length;
            note_written(record, offset, length);
            if let Some(stat) = record.stats.first() {
                stat.downloaded.fetch_add(length, Ordering::Relaxed);
            }
            buffer.clear();
        }
    }
    if !buffer.is_empty() {
        file.write_all(&buffer).await?;
        let length = buffer.len() as u64;
        let offset = written;
        written += length;
        note_written(record, offset, length);
        if let Some(stat) = record.stats.first() {
            stat.downloaded.fetch_add(length, Ordering::Relaxed);
        }
    }
    file.sync_all().await?;
    if let Some(stat) = record.stats.first() {
        *stat.status.lock().unwrap() = "done".into();
    }
    if total > 0 && written < total {
        return Err(MajraError::ShortRead);
    }
    if cancel.is_cancelled() {
        return Ok(outcome(record));
    }
    Ok(Outcome::Completed)
}

async fn reporter(
    record: Arc<Record>,
    downloaded: Arc<AtomicU64>,
    stop: CancellationToken,
    events: broadcast::Sender<DownloadEvent>,
    catalog: Arc<Catalog>,
) {
    let mut previous = downloaded.load(Ordering::Relaxed);
    let mut previous_time = Instant::now();
    let mut speed = 0.0f64;
    let mut previous_connections = record
        .stats
        .iter()
        .map(|stat| stat.downloaded.load(Ordering::Relaxed))
        .collect::<Vec<_>>();
    let mut last_checkpoint = Instant::now();
    let mut saved_blocks: HashSet<u64> = HashSet::new();

    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_millis(REPORT_MS)) => {}
        }

        let now = Instant::now();
        // Bytes are counted as they arrive, so a retried range can briefly overshoot the
        // total. The UI never shows more than 100 %.
        let current = {
            let value = downloaded.load(Ordering::Relaxed);
            if record.total_bytes > 0 {
                value.min(record.total_bytes)
            } else {
                value
            }
        };
        // The first byte is the moment the row stops being a promise and starts being a
        // transfer; until then the UI shows "connecting" instead of a dead 0 %.
        if current > 0 {
            let mut snapshot = record.snapshot.lock().unwrap();
            if matches!(snapshot.phase.as_str(), "starting" | "reconnecting") {
                snapshot.phase = "downloading".into();
            }
        }
        let elapsed = now.duration_since(previous_time).as_secs_f64().max(0.001);
        let instant = current.saturating_sub(previous) as f64 / elapsed;
        speed = if speed <= 0.0 {
            instant
        } else {
            speed * 0.72 + instant * 0.28
        };
        previous = current;
        previous_time = now;

        let total = record.total_bytes;
        let eta = if speed > 1024.0 && total > current {
            Some(((total - current) as f64 / speed).ceil() as u64)
        } else {
            None
        };

        let connections = record
            .stats
            .iter()
            .enumerate()
            .map(|(index, stat)| {
                let bytes = stat.downloaded.load(Ordering::Relaxed);
                let connection_speed =
                    bytes.saturating_sub(previous_connections[index]) as f64 / elapsed;
                previous_connections[index] = bytes;
                ConnectionSnapshot {
                    id: stat.id,
                    interface_id: stat.interface_id.clone(),
                    interface_name: stat.interface_name.clone(),
                    downloaded_bytes: bytes,
                    speed_bytes_per_sec: connection_speed,
                    status: stat.status.lock().unwrap().clone(),
                }
            })
            .collect::<Vec<_>>();

        let snapshot = {
            let mut snapshot = record.snapshot.lock().unwrap();
            snapshot.downloaded_bytes = current;
            snapshot.speed_bytes_per_sec = speed;
            snapshot.eta_seconds = eta;
            snapshot.connections = connections;
            snapshot.clone()
        };
        let _ = events.send(DownloadEvent::Progress { snapshot });

        if last_checkpoint.elapsed() >= Duration::from_secs(CHECKPOINT_SECONDS) {
            let completed = record.durable_blocks();
            // Only touch the disk when the safe-to-resume map actually grew. On a
            // stalled host this used to rewrite the same file every two seconds.
            if completed != saved_blocks {
                let _ = state::save(
                    &record.state_path,
                    &record.probe,
                    record.total_bytes,
                    record.block_size,
                    &completed,
                )
                .await;
                saved_blocks = completed;
            }
            catalog.touch(saved_from(&record));
            last_checkpoint = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_folder_and_file_are_distinguished() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = dir.path().to_path_buf();
        let folder_text = folder.display().to_string();

        // An existing directory always receives the server's file name.
        assert_eq!(
            resolve_destination(&folder_text, "movie.mkv"),
            folder.join("movie.mkv")
        );

        // A missing dotted last segment is a file path.
        let file = folder.join("clip.mp4");
        assert_eq!(
            resolve_destination(&file.display().to_string(), "movie.mkv"),
            file
        );

        // A missing un-dotted last segment is a new folder.
        let new_folder = folder.join("Media");
        assert_eq!(
            resolve_destination(&new_folder.display().to_string(), "movie.mkv"),
            new_folder.join("movie.mkv")
        );

        // A trailing separator is an explicit folder.
        let explicit = format!("{folder_text}\\");
        assert_eq!(
            resolve_destination(&explicit, "movie.mkv"),
            folder.join("movie.mkv")
        );
    }

    #[test]
    fn connections_are_spread_over_selected_interfaces() {
        let interfaces = vec![
            InterfaceInfo {
                id: "wifi|192.168.1.10".into(),
                name: "Wi-Fi".into(),
                address: "192.168.1.10".into(),
                kind: InterfaceKind::Wifi,
                is_default: false,
                usable: true,
                network: Some("192.168.1.0/24".into()),
                link_speed_bps: None,
            },
            InterfaceInfo {
                id: "eth|10.0.0.4".into(),
                name: "Ethernet".into(),
                address: "10.0.0.4".into(),
                kind: InterfaceKind::Ethernet,
                is_default: false,
                usable: true,
                network: Some("10.0.0.0/8".into()),
                link_speed_bps: None,
            },
            InterfaceInfo {
                id: "usb|192.168.42.2".into(),
                name: "USB Tethering".into(),
                address: "192.168.42.2".into(),
                kind: InterfaceKind::Usb,
                is_default: false,
                usable: true,
                network: Some("192.168.42.0/24".into()),
                link_speed_bps: None,
            },
        ];
        let stats = make_stats(&interfaces, 9);
        assert_eq!(stats.len(), 9);
        for (name, expected) in [("Wi-Fi", 3), ("Ethernet", 3), ("USB Tethering", 3)] {
            assert_eq!(
                stats
                    .iter()
                    .filter(|stat| stat.interface_name == name)
                    .count(),
                expected,
                "connections were not spread over {name}"
            );
        }
    }

    fn test_interface(id: &str, usable: bool) -> InterfaceInfo {
        InterfaceInfo {
            id: id.into(),
            name: id.into(),
            address: "192.0.2.10".into(),
            kind: InterfaceKind::Other,
            is_default: false,
            usable,
            network: Some("192.0.2.0/24".into()),
            link_speed_bps: None,
        }
    }

    #[test]
    fn unusable_adapters_are_never_used() {
        let all = vec![
            test_interface("wifi|192.0.2.10", true),
            test_interface("eth|192.0.2.11", false),
        ];
        let chosen = choose_interfaces(
            all.clone(),
            &["eth|192.0.2.11".to_string(), "wifi|192.0.2.10".to_string()],
        );
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].id, "wifi|192.0.2.10");

        let none = choose_interfaces(all, &["eth|192.0.2.11".to_string()]);
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].id, "auto");
    }

    #[test]
    #[cfg(windows)]
    fn a_typed_destination_gets_native_separators() {
        // The scanner rejects forward slashes with exit code 2, which used to look like a
        // detection, so the path is normalized before it ever reaches disk.
        let path = resolve_destination("D:/Media/Clips/", "clip.mp4");
        assert_eq!(path.to_string_lossy(), r"D:\Media\Clips\clip.mp4");
        let unc = resolve_destination("//server/share", "file.bin");
        assert_eq!(unc.to_string_lossy(), r"\\server\share\file.bin");
    }

    #[test]
    fn a_second_download_does_not_overwrite_the_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = dir.path().join("movie.mp4");
        std::fs::write(&first, b"old").expect("write");
        let second = unique_path(&first);
        assert_eq!(
            second.file_name().and_then(|name| name.to_str()),
            Some("movie (1).mp4")
        );
        std::fs::write(&second, b"new").expect("write");
        let third = unique_path(&first);
        assert_eq!(
            third.file_name().and_then(|name| name.to_str()),
            Some("movie (2).mp4")
        );
    }

    /// Binding a client to an interface address must change the source address the server
    /// sees. This is the mechanism the whole bonding feature rests on, so it is checked
    /// against a real socket pair rather than assumed.
    #[tokio::test]
    async fn each_client_binds_to_its_interface_address() {
        use std::net::Ipv4Addr;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let target = listener.local_addr().expect("addr");
        let peers = Arc::new(Mutex::new(Vec::<IpAddr>::new()));
        let seen = peers.clone();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, peer) = listener.accept().await.expect("accept");
                seen.lock().unwrap().push(peer.ip());
                let mut buffer = [0u8; 512];
                let _ = socket.read(&mut buffer).await;
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await;
                let _ = socket.shutdown().await;
            }
        });

        let interfaces = vec![
            InterfaceInfo {
                id: "lo|127.0.0.1".into(),
                name: "Loop 1".into(),
                address: "127.0.0.1".into(),
                kind: InterfaceKind::Other,
                is_default: false,
                usable: true,
                network: None,
                link_speed_bps: None,
            },
            InterfaceInfo {
                id: "lo|127.0.0.2".into(),
                name: "Loop 2".into(),
                address: "127.0.0.2".into(),
                kind: InterfaceKind::Other,
                is_default: false,
                usable: true,
                network: None,
                link_speed_bps: None,
            },
        ];

        for client in build_clients(&interfaces).0 {
            let response = client
                .get(format!("http://{target}/"))
                .send()
                .await
                .expect("request");
            let _ = response.bytes().await;
        }
        server.await.expect("server");

        let mut seen = peers.lock().unwrap().clone();
        seen.sort();
        assert_eq!(
            seen,
            vec![
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2))
            ],
            "a connection did not use its own interface address"
        );
    }

    #[test]
    fn permanent_http_errors_are_not_retried() {
        // A 403/404/416 answers the same way every time; retrying it only delays the
        // message the user needs.
        assert!(!retryable(&MajraError::Http(403)));
        assert!(!retryable(&MajraError::Http(404)));
        assert!(!retryable(&MajraError::Http(416)));
        assert!(!retryable(&MajraError::RangeRejected));
        assert!(!retryable(&MajraError::Cancelled));

        // Transient answers and a dropped connection are worth another request.
        assert!(retryable(&MajraError::Http(408)));
        assert!(retryable(&MajraError::Http(429)));
        assert!(retryable(&MajraError::Http(500)));
        assert!(retryable(&MajraError::Http(503)));
        assert!(retryable(&MajraError::ShortRead));
    }

    #[test]
    fn retry_delay_grows_and_stays_capped() {
        let first = retry_delay(1, 0);
        let later = retry_delay(4, 0);
        assert!(first >= Duration::from_millis(300), "{first:?}");
        assert!(later > first, "{later:?} should exceed {first:?}");
        // Jitter must never push the wait past the eight second ceiling.
        for attempt in 1..=8 {
            for salt in 0..64 {
                assert!(retry_delay(attempt, salt) <= Duration::from_secs(8));
            }
        }
    }

    #[test]
    fn merged_ranges_cover_only_what_was_written() {
        let mut ranges = BTreeMap::new();
        merge_range(&mut ranges, 100, 200);
        merge_range(&mut ranges, 200, 300);
        assert!(covers(&ranges, 100, 300));
        assert!(!covers(&ranges, 100, 301));
        assert!(!covers(&ranges, 99, 200));

        // An overlapping range is absorbed instead of producing two entries.
        merge_range(&mut ranges, 150, 250);
        assert_eq!(ranges.len(), 1);
        assert!(covers(&ranges, 100, 300));

        // A gap stays a gap.
        merge_range(&mut ranges, 400, 500);
        assert!(!covers(&ranges, 100, 500));
        assert!(covers(&ranges, 400, 500));
        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn durable_blocks_require_the_whole_block_on_disk() {
        let blocks = vec![
            Block {
                index: 0,
                start: 0,
                end: 99,
            },
            Block {
                index: 1,
                start: 100,
                end: 199,
            },
        ];
        let mut durable = BTreeMap::new();
        merge_range(&mut durable, 0, 100);

        let record = Record {
            probe: ProbeResult {
                requested_url: "http://example.test/file".into(),
                final_url: "http://example.test/file".into(),
                supports_ranges: true,
                total_bytes: Some(200),
                file_name: "file.bin".into(),
                content_type: None,
                etag: None,
                last_modified: None,
            },
            file_path: PathBuf::from("unused"),
            state_path: PathBuf::from("unused.json"),
            total_bytes: 200,
            block_size: 100,
            blocks,
            completed: Mutex::new(HashSet::from([0])),
            stats: Vec::new(),
            clients: Vec::new(),
            auto_client: client_builder().build().expect("client"),
            fallback_allowed: false,
            interface_ids: Vec::new(),
            browser: None,
            snapshot: Mutex::new(DownloadSnapshot {
                id: "test".into(),
                status: DownloadStatus::Queued,
                phase: String::new(),
                url: "http://example.test/file".into(),
                file_name: "file.bin".into(),
                destination: String::new(),
                total_bytes: 200,
                downloaded_bytes: 0,
                speed_bytes_per_sec: 0.0,
                eta_seconds: None,
                connections: Vec::new(),
                error: None,
                started_at: 0,
                completed_at: None,
                extraction: None,
            }),
            cancel: Mutex::new(CancellationToken::new()),
            cancel_kind: Mutex::new(CancelKind::None),
            task: Mutex::new(None),
            prefix: Arc::new(AtomicU64::new(0)),
            ranges: Mutex::new(BTreeMap::new()),
            durable: Mutex::new(durable),
            extracted: Mutex::new(HashSet::new()),
            range_rejected: AtomicBool::new(false),
            extract: false,
            extract_root: PathBuf::new(),
        };

        // Block 0 is fully written, block 1 has not been touched yet.
        assert_eq!(record.durable_blocks(), HashSet::from([0]));

        // Half of block 1 is not enough: a checkpoint must never claim it.
        merge_range(&mut record.durable.lock().unwrap(), 100, 150);
        assert_eq!(record.durable_blocks(), HashSet::from([0]));

        merge_range(&mut record.durable.lock().unwrap(), 150, 200);
        assert_eq!(record.durable_blocks(), HashSet::from([0, 1]));
    }
}
