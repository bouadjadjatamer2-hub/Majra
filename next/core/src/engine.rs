use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::catalog::{Catalog, SavedDownload};
use crate::error::{MajraError, Result};
use crate::extract::{self, ExtractionSnapshot};
use crate::net::{self, InterfaceInfo, InterfaceKind};
use crate::plan::{self, Block, MAX_CONNECTIONS};
use crate::probe::{client_builder, probe_url, ProbeResult};
use crate::state;

const WRITE_CHUNK: usize = 256 * 1024;
const CHANNEL_CHUNKS: usize = 32;
const REPORT_MS: u64 = 250;
const CHECKPOINT_SECONDS: u64 = 2;
const MAX_RETRIES: u32 = 3;
/// Downloads running at once by default; the rest wait in the queue.
const DEFAULT_MAX_ACTIVE: usize = 3;
const MAX_ACTIVE_LIMIT: usize = 8;

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
    /// The adapters the user picked, kept so the catalog can rebuild the request.
    interface_ids: Vec<String>,
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
    extracted: Mutex<HashSet<String>>,
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

    /// How many downloads run at the same time. Everything above the limit waits in the
    /// queue, IDM style, and starts as soon as a slot frees up.
    pub fn set_max_active(&self, value: u32) {
        let value = (value as usize).clamp(1, MAX_ACTIVE_LIMIT);
        self.max_active.store(value, Ordering::Relaxed);
        self.fill_slots();
    }

    pub fn max_active(&self) -> usize {
        self.max_active.load(Ordering::Relaxed)
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
            if self.active_count() >= self.max_active() {
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
        self.start_inner(request, None).await
    }

    /// `reuse_id` keeps the row identity when a restored download resumes, so the list
    /// updates the same entry instead of showing a duplicate.
    async fn start_inner(
        &self,
        request: DownloadRequest,
        reuse_id: Option<String>,
    ) -> Result<String> {
        if request.url.trim().is_empty() {
            return Err(MajraError::Message("the link is empty".into()));
        }

        let probe = probe_url(&request.url).await?;
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

        let file_path = resolve_destination(&request.destination, &probe.file_name);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let state_path = state_path_for(&file_path);

        let completed = if probe.supports_ranges && total_bytes > 0 {
            state::load_completed(&state_path, &probe, total_bytes, block_size).await
        } else {
            HashSet::new()
        };

        let interfaces = selected_interfaces(&request.interface_ids);
        let stats = make_stats(&interfaces, connections);
        let clients = build_clients(&interfaces);
        let id = reuse_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = now_ms();
        let extract_root = extract_root_for(&file_path);
        let extraction = request
            .extract
            .then(|| ExtractionSnapshot::new(&extract_root));
        let snapshot = DownloadSnapshot {
            id: id.clone(),
            status: DownloadStatus::Downloading,
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
            interface_ids: request.interface_ids.clone(),
            snapshot: Mutex::new(snapshot),
            cancel: Mutex::new(CancellationToken::new()),
            cancel_kind: Mutex::new(CancelKind::None),
            task: Mutex::new(None),
            extract: request.extract,
            extract_root,
            prefix: Arc::new(AtomicU64::new(0)),
            ranges: Mutex::new(BTreeMap::new()),
            extracted: Mutex::new(HashSet::new()),
        });

        let start_now = {
            let _gate = self.admission.lock().unwrap();
            let free = self.active_count() < self.max_active();
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
        if record.snapshot.lock().unwrap().status == DownloadStatus::Queued {
            self.queue.lock().unwrap().retain(|queued| queued != id);
            {
                let mut snapshot = record.snapshot.lock().unwrap();
                snapshot.status = DownloadStatus::Paused;
                snapshot.error = None;
                snapshot.speed_bytes_per_sec = 0.0;
                snapshot.eta_seconds = None;
            }
            self.catalog.upsert(saved_from(&record));
            let snapshot = record.snapshot.lock().unwrap().clone();
            let _ = self.events.send(DownloadEvent::Paused { snapshot });
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
            let free = self.active_count_except(id) < self.max_active();
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
        };
        self.start_inner(request, Some(id.to_string())).await?;
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
        self.queue.lock().unwrap().retain(|queued| queued != id);
        let mut snapshot = record.snapshot.lock().unwrap().clone();
        if !matches!(
            snapshot.status,
            DownloadStatus::Queued | DownloadStatus::Paused | DownloadStatus::Error
        ) {
            return Ok(());
        }
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
        self.restored
            .lock()
            .unwrap()
            .get(id)
            .map(saved_snapshot)
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
    let raw = raw.trim();
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
    let last = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if last.contains('.') {
        path
    } else {
        path.join(file_name)
    }
}

fn state_path_for(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".majra.json");
    PathBuf::from(name)
}

/// Archives unpack into a folder named after the archive, next to the archive itself.
fn extract_root_for(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    let stem = file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("extracted");
    parent.join(stem)
}

fn auto_interface() -> InterfaceInfo {
    InterfaceInfo {
        id: "auto".into(),
        name: "Auto".into(),
        address: String::new(),
        kind: InterfaceKind::Other,
        is_default: true,
        network: None,
    }
}

fn selected_interfaces(ids: &[String]) -> Vec<InterfaceInfo> {
    if ids.is_empty() {
        return vec![auto_interface()];
    }
    let selected = net::list_interfaces()
        .into_iter()
        .filter(|interface| ids.iter().any(|id| id == &interface.id))
        .collect::<Vec<_>>();
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

fn build_clients(interfaces: &[InterfaceInfo]) -> Vec<reqwest::Client> {
    let default = client_builder()
        .build()
        .expect("the default HTTP client must build");
    interfaces
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
        .collect()
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

    let mut result = run_inner(&record, &cancel, &downloaded).await;
    if matches!(result, Err(MajraError::Cancelled)) {
        result = Ok(outcome(&record));
    }

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
    snapshot.downloaded_bytes = downloaded_now;

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
            let completed = record.completed.lock().unwrap().clone();
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
            let completed = record.completed.lock().unwrap().clone();
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
fn note_written(record: &Record, offset: u64, length: u64) {
    if length == 0 {
        return;
    }
    let mut ranges = record.ranges.lock().unwrap();
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

    loop {
        if !wait_prefix(&record, offset.saturating_add(30), &cancel, &stop).await {
            return;
        }

        // Read the header first, then wait for the whole entry. Reading it early would see
        // the preallocated zeros that sit beyond the contiguous prefix.
        let path = record.file_path.clone();
        let requirement = tokio::task::spawn_blocking(move || {
            extract::local_requirement(&path, offset)
        })
        .await;
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
    if !extract::looks_like_zip(&record.file_path) {
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
        let mut progress = |name: &str, done: u64, bytes: u64, total: u64| {
            update_extraction(&progress_record, |extraction| {
                extraction.entries_done = base_entries.saturating_add(done);
                extraction.entries_total = Some(total);
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
    let writer = tokio::spawn(writer_task(
        record.clone(),
        rx,
        downloaded.clone(),
        fatal.clone(),
    ));

    let queue = Arc::new(Mutex::new(record.pending_blocks()));
    let mut workers = Vec::new();
    for index in 0..record.stats.len() {
        let client = record.clients[index % record.clients.len()].clone();
        let stat = record.stats[index].clone();
        let queue = queue.clone();
        let tx = tx.clone();
        let record = record.clone();
        let cancel = cancel.clone();
        let fatal = fatal.clone();
        workers.push(tokio::spawn(async move {
            worker(record, client, stat, queue, tx, cancel, fatal).await
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

    let writer_result = writer.await;
    if let Some(error) = fatal.lock().unwrap().clone() {
        return Err(MajraError::Message(error));
    }
    if let Some(error) = worker_error {
        return Err(error);
    }
    writer_result
        .map_err(|error| MajraError::Message(format!("writer stopped: {error}")))??;

    if cancel.is_cancelled() {
        return Ok(outcome(record));
    }
    if downloaded.load(Ordering::Relaxed) < record.total_bytes {
        return Err(MajraError::ShortRead);
    }
    Ok(Outcome::Completed)
}

async fn worker(
    record: Arc<Record>,
    client: reqwest::Client,
    stat: Arc<ConnectionStat>,
    queue: Arc<Mutex<VecDeque<usize>>>,
    tx: mpsc::Sender<WriteChunk>,
    cancel: CancellationToken,
    fatal: Arc<Mutex<Option<String>>>,
) -> Result<()> {
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
            )
            .await
            {
                Ok(()) => {
                    record.completed.lock().unwrap().insert(block.index);
                    *stat.status.lock().unwrap() = "idle".into();
                    break;
                }
                Err(MajraError::Cancelled) => return Ok(()),
                Err(error) => {
                    if attempt >= MAX_RETRIES {
                        *fatal.lock().unwrap() = Some(format!(
                            "block {} failed after {} retries: {error}",
                            block.index,
                            MAX_RETRIES
                        ));
                        cancel.cancel();
                        return Ok(());
                    }
                    attempt += 1;
                    *stat.status.lock().unwrap() = format!("retry {attempt}");
                    tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(Duration::from_millis(300 * 2u64.pow(attempt))) => {}
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
) -> Result<()> {
    let response = client
        .get(url)
        .header(
            reqwest::header::RANGE,
            format!("bytes={}-{}", block.start, block.end),
        )
        .header(reqwest::header::ACCEPT, "*/*")
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(MajraError::RangeRejected);
    }

    let mut stream = response.bytes_stream();
    let mut offset = block.start;
    let mut buffer = Vec::with_capacity(WRITE_CHUNK);
    while let Some(item) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(MajraError::Cancelled);
        }
        let chunk = item?;
        if chunk.is_empty() {
            continue;
        }
        if offset + chunk.len() as u64 > block.end + 1 {
            return Err(MajraError::Message(
                "the server sent more bytes than the range asked for".into(),
            ));
        }
        buffer.extend_from_slice(&chunk);
        if buffer.len() >= WRITE_CHUNK {
            let data = Bytes::from(std::mem::replace(
                &mut buffer,
                Vec::with_capacity(WRITE_CHUNK),
            ));
            let length = data.len() as u64;
            tx.send(WriteChunk {
                offset,
                data,
                connection_id,
            })
            .await
            .map_err(|_| MajraError::Message("the disk writer stopped".into()))?;
            offset += length;
        }
    }
    if !buffer.is_empty() {
        let data = Bytes::from(buffer);
        let length = data.len() as u64;
        tx.send(WriteChunk {
            offset,
            data,
            connection_id,
        })
        .await
        .map_err(|_| MajraError::Message("the disk writer stopped".into()))?;
        offset += length;
    }
    if offset != block.end + 1 {
        return Err(MajraError::ShortRead);
    }
    Ok(())
}

async fn writer_task(
    record: Arc<Record>,
    mut rx: mpsc::Receiver<WriteChunk>,
    downloaded: Arc<AtomicU64>,
    fatal: Arc<Mutex<Option<String>>>,
) -> Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&record.file_path)
        .await?;

    // Drain every chunk that was already accepted, even after a pause: a block is only
    // marked complete once its bytes have left the channel, so discarding the tail here
    // would leave a hole in a block that resume then skips.
    while let Some(chunk) = rx.recv().await {
        let write = async {
            file.seek(std::io::SeekFrom::Start(chunk.offset)).await?;
            file.write_all(&chunk.data).await?;
            Ok::<(), std::io::Error>(())
        }
        .await;
        if let Err(error) = write {
            *fatal.lock().unwrap() = Some(format!("could not write the file: {error}"));
            // Nothing on disk can be trusted after a write failure: force a full retry.
            record.completed.lock().unwrap().clear();
            record.cancel.lock().unwrap().cancel();
            return Err(error.into());
        }
        let length = chunk.data.len() as u64;
        downloaded.fetch_add(length, Ordering::Relaxed);
        note_written(&record, chunk.offset, length);
        if let Some(stat) = record.stats.get(chunk.connection_id as usize) {
            stat.downloaded.fetch_add(length, Ordering::Relaxed);
        }
    }
    file.sync_all().await?;
    Ok(())
}

async fn run_single(
    record: &Arc<Record>,
    cancel: &CancellationToken,
    downloaded: &Arc<AtomicU64>,
) -> Result<Outcome> {
    record.ranges.lock().unwrap().clear();
    record.prefix.store(0, Ordering::Relaxed);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(&record.file_path)
        .await?;
    let response = record.clients[0]
        .get(&record.probe.final_url)
        .header(reqwest::header::ACCEPT, "*/*")
        .send()
        .await?;
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
            file.sync_all().await?;
            return Ok(outcome(record));
        }
        let chunk = item?;
        buffer.extend_from_slice(&chunk);
        if buffer.len() >= WRITE_CHUNK {
            file.write_all(&buffer).await?;
            let length = buffer.len() as u64;
            written += length;
            note_written(record, written - length, length);
            downloaded.fetch_add(length, Ordering::Relaxed);
            if let Some(stat) = record.stats.first() {
                stat.downloaded.fetch_add(length, Ordering::Relaxed);
            }
            buffer.clear();
        }
    }
    if !buffer.is_empty() {
        file.write_all(&buffer).await?;
        let length = buffer.len() as u64;
        written += length;
        note_written(record, written - length, length);
        downloaded.fetch_add(length, Ordering::Relaxed);
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

    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_millis(REPORT_MS)) => {}
        }

        let now = Instant::now();
        let current = downloaded.load(Ordering::Relaxed);
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
            let completed = record.completed.lock().unwrap().clone();
            let _ = state::save(
                &record.state_path,
                &record.probe,
                record.total_bytes,
                record.block_size,
                &completed,
            )
            .await;
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
                network: Some("192.168.1.0/24".into()),
            },
            InterfaceInfo {
                id: "eth|10.0.0.4".into(),
                name: "Ethernet".into(),
                address: "10.0.0.4".into(),
                kind: InterfaceKind::Ethernet,
                is_default: false,
                network: Some("10.0.0.0/8".into()),
            },
            InterfaceInfo {
                id: "usb|192.168.42.2".into(),
                name: "USB Tethering".into(),
                address: "192.168.42.2".into(),
                kind: InterfaceKind::Usb,
                is_default: false,
                network: Some("192.168.42.0/24".into()),
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
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
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
                network: None,
            },
            InterfaceInfo {
                id: "lo|127.0.0.2".into(),
                name: "Loop 2".into(),
                address: "127.0.0.2".into(),
                kind: InterfaceKind::Other,
                is_default: false,
                network: None,
            },
        ];

        for client in build_clients(&interfaces) {
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
}
