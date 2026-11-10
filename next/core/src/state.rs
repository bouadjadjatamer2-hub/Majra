use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use tokio::fs;

use crate::error::Result;
use crate::probe::ProbeResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeState {
    pub url: String,
    pub final_url: String,
    pub total_bytes: u64,
    pub block_size: u64,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub completed: Vec<u64>,
}

fn matches(state: &ResumeState, probe: &ProbeResult, total_bytes: u64, block_size: u64) -> bool {
    state.url == probe.requested_url
        && state.final_url == probe.final_url
        && state.total_bytes == total_bytes
        && state.block_size == block_size
        && state.etag == probe.etag
        && state.last_modified == probe.last_modified
}

/// Returns the block indices that are safe to skip. A validator mismatch discards the state:
/// the file on the server changed, so old blocks are not the same bytes.
pub async fn load_completed(
    path: &Path,
    probe: &ProbeResult,
    total_bytes: u64,
    block_size: u64,
) -> HashSet<u64> {
    let Ok(raw) = fs::read_to_string(path).await else {
        return HashSet::new();
    };
    let Ok(state) = serde_json::from_str::<ResumeState>(&raw) else {
        return HashSet::new();
    };
    if matches(&state, probe, total_bytes, block_size) {
        state.completed.into_iter().collect()
    } else {
        HashSet::new()
    }
}

pub async fn save(
    path: &Path,
    probe: &ProbeResult,
    total_bytes: u64,
    block_size: u64,
    completed: &HashSet<u64>,
) -> Result<()> {
    let mut completed = completed.iter().copied().collect::<Vec<_>>();
    completed.sort_unstable();
    let state = ResumeState {
        url: probe.requested_url.clone(),
        final_url: probe.final_url.clone(),
        total_bytes,
        block_size,
        etag: probe.etag.clone(),
        last_modified: probe.last_modified.clone(),
        completed,
    };
    let json = serde_json::to_vec(&state)?;
    let tmp = path.with_extension("json.tmp");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(&tmp, json).await?;
    fs::rename(&tmp, path).await?;
    Ok(())
}

pub async fn remove(path: &Path) {
    let _ = fs::remove_file(path).await;
}
