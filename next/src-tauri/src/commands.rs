use majra_core::{
    list_interfaces as core_list_interfaces, probe_url, DownloadManager, DownloadRequest,
    DownloadSnapshot, InterfaceInfo, ProbeResult,
};
use std::sync::Arc;
use tauri::State;

pub struct AppState {
    pub manager: Arc<DownloadManager>,
}

fn message(error: majra_core::MajraError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn list_interfaces() -> Vec<InterfaceInfo> {
    core_list_interfaces()
}

#[tauri::command]
pub async fn probe_link(url: String) -> Result<ProbeResult, String> {
    probe_url(&url).await.map_err(message)
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    request: DownloadRequest,
) -> Result<String, String> {
    state.manager.start(request).await.map_err(message)
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.pause(&id).await.map_err(message)
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.resume(&id).await.map_err(message)
}

#[tauri::command]
pub async fn cancel_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.cancel(&id).await.map_err(message)
}

#[tauri::command]
pub fn forget_download(state: State<'_, AppState>, id: String) -> bool {
    state.manager.forget(&id)
}

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadSnapshot> {
    state.manager.list()
}

#[tauri::command]
pub fn download_snapshot(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DownloadSnapshot>, String> {
    Ok(state.manager.snapshot(&id))
}

#[tauri::command]
pub fn set_max_active(state: State<'_, AppState>, value: u32) -> u32 {
    state.manager.set_max_active(value);
    state.manager.max_active() as u32
}
