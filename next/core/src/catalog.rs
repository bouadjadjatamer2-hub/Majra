use crate::browser::BrowserContext;
use crate::engine::DownloadStatus;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One row kept between runs. It holds only what the list needs to come back and what a
/// resume needs to rebuild the request; the block map stays in the per-file `.majra.json`
/// state file next to the partial download.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDownload {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub destination: String,
    pub connections: u32,
    #[serde(default)]
    pub interface_ids: Vec<String>,
    #[serde(default)]
    pub extract: bool,
    /// The browser's sign-in, so a download paused before a restart can still resume.
    #[serde(default)]
    pub browser: Option<BrowserContext>,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    #[serde(default)]
    pub error: Option<String>,
    pub started_at: u64,
    #[serde(default)]
    pub completed_at: Option<u64>,
}

/// Small JSON list of known downloads. Progress writes are throttled so a fast transfer
/// does not turn into a disk write every two seconds; status changes are always written.
pub struct Catalog {
    path: PathBuf,
    entries: Mutex<Vec<SavedDownload>>,
    last_write: Mutex<Option<Instant>>,
}

const WRITE_INTERVAL: Duration = Duration::from_secs(5);

impl Catalog {
    /// `%APPDATA%\app.majra.desktop\downloads.json` on Windows. A missing or broken file
    /// starts an empty list: history is convenience, never a reason to fail a download.
    pub fn load() -> Self {
        let path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("app.majra.desktop")
            .join("downloads.json");
        Self::open(path)
    }

    pub fn open(path: PathBuf) -> Self {
        let entries = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<SavedDownload>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            entries: Mutex::new(entries),
            last_write: Mutex::new(None),
        }
    }

    pub fn all(&self) -> Vec<SavedDownload> {
        let mut entries = self.entries.lock().unwrap().clone();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.started_at));
        entries
    }

    pub fn get(&self, id: &str) -> Option<SavedDownload> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// Status changes and new rows: write immediately.
    pub fn upsert(&self, entry: SavedDownload) {
        self.update(entry, true);
    }

    /// Progress ticks: keep memory current, write at most every few seconds.
    pub fn touch(&self, entry: SavedDownload) {
        self.update(entry, false);
    }

    pub fn remove(&self, id: &str) {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|entry| entry.id != id);
        self.write(&entries);
        *self.last_write.lock().unwrap() = Some(Instant::now());
    }

    fn update(&self, entry: SavedDownload, force: bool) {
        let mut entries = self.entries.lock().unwrap();
        match entries.iter_mut().find(|known| known.id == entry.id) {
            Some(known) => *known = entry,
            None => entries.push(entry),
        }
        let due = self
            .last_write
            .lock()
            .unwrap()
            .map(|last| last.elapsed() >= WRITE_INTERVAL)
            .unwrap_or(true);
        if force || due {
            self.write(&entries);
            *self.last_write.lock().unwrap() = Some(Instant::now());
        }
    }

    fn write(&self, entries: &[SavedDownload]) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let Ok(json) = serde_json::to_vec_pretty(entries) else {
            return;
        };
        // Write beside the target and swap: a crash mid-write leaves the old list intact.
        let temporary = self.path.with_extension("json.tmp");
        if fs::write(&temporary, json).is_ok() {
            let _ = fs::remove_file(&self.path);
            if fs::rename(&temporary, &self.path).is_err() {
                let _ = fs::remove_file(&temporary);
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, status: DownloadStatus) -> SavedDownload {
        SavedDownload {
            id: id.into(),
            url: "https://example.com/file.zip".into(),
            file_name: "file.zip".into(),
            destination: "C:\\Downloads\\file.zip".into(),
            connections: 8,
            interface_ids: vec!["wifi|192.168.1.4".into()],
            extract: true,
            browser: None,
            total_bytes: 1000,
            downloaded_bytes: 250,
            status,
            error: None,
            started_at: 7,
            completed_at: None,
        }
    }

    #[test]
    fn survives_a_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("downloads.json");
        {
            let catalog = Catalog::open(path.clone());
            catalog.upsert(row("a", DownloadStatus::Paused));
            catalog.upsert(row("b", DownloadStatus::Completed));
        }

        let catalog = Catalog::open(path);
        let entries = catalog.all();
        assert_eq!(entries.len(), 2);
        let first = catalog.get("a").expect("row a");
        assert_eq!(first.downloaded_bytes, 250);
        assert!(first.extract);
        assert_eq!(first.interface_ids, vec!["wifi|192.168.1.4".to_string()]);
    }

    #[test]
    fn a_new_row_replaces_the_old_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let catalog = Catalog::open(dir.path().join("downloads.json"));
        catalog.upsert(row("a", DownloadStatus::Downloading));
        let mut finished = row("a", DownloadStatus::Completed);
        finished.downloaded_bytes = 1000;
        catalog.upsert(finished);

        assert_eq!(catalog.all().len(), 1);
        let entry = catalog.get("a").expect("row a");
        assert_eq!(entry.status, DownloadStatus::Completed);
        assert_eq!(entry.downloaded_bytes, 1000);
    }

    #[test]
    fn remove_drops_the_row_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("downloads.json");
        let catalog = Catalog::open(path.clone());
        catalog.upsert(row("a", DownloadStatus::Paused));
        catalog.remove("a");
        assert!(catalog.all().is_empty());
        assert!(Catalog::open(path).all().is_empty());
    }

    #[test]
    fn a_broken_file_starts_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("downloads.json");
        fs::write(&path, b"{ not json").expect("write");
        let catalog = Catalog::open(path);
        assert!(catalog.all().is_empty());
    }
}
