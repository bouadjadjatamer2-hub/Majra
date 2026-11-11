use crate::settings::{data_dir, Settings, SettingsStore};
use majra_core::{
    bridge::{BridgeOffer, BRIDGE_API, BRIDGE_PORTS},
    list_interfaces as core_list_interfaces,
    probe_url_with,
    test_interface as core_test_interface,
    DownloadManager, DownloadRequest, DownloadSnapshot, InterfaceInfo, ProbeResult,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

/// Offers the extension confirmed but the user hasn't started yet. The browser's cookies
/// stay here, never in the webview.
#[derive(Default)]
pub struct OfferStore {
    offers: Mutex<HashMap<String, BridgeOffer>>,
}

impl OfferStore {
    pub fn insert(&self, offer: BridgeOffer) {
        self.offers.lock().unwrap().insert(offer.id.clone(), offer);
    }

    pub fn get(&self, id: &str) -> Option<BridgeOffer> {
        self.offers.lock().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        self.offers.lock().unwrap().remove(id);
    }
}

pub struct AppState {
    pub manager: Arc<DownloadManager>,
    pub offers: Arc<OfferStore>,
    pub settings: Arc<SettingsStore>,
    pub bridge_port: Arc<AtomicU16>,
    /// True once `shutdown /s /t` has been issued for the current batch. Starting or
    /// resuming a download clears it, so the next batch can arm it again.
    pub shutdown_armed: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub running: bool,
    pub port: u16,
    pub ports: Vec<u16>,
    pub api: u32,
}

fn message(error: majra_core::MajraError) -> String {
    error.to_string()
}

#[tauri::command]
pub fn list_interfaces() -> Vec<InterfaceInfo> {
    core_list_interfaces()
}

#[tauri::command]
pub async fn test_interface(address: String) -> Result<u64, String> {
    core_test_interface(&address).await
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.get()
}

#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let saved = state.settings.save(settings)?;
    state.manager.set_max_active(saved.max_active);
    crate::settings::apply_proxy(&saved);

    // The start-at-login entry lives in the operating system, not in settings.json. A
    // locked-down machine can refuse it; the preference is still saved so the user sees
    // the same state after a restart.
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let result = if saved.start_at_login {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        if let Err(error) = result {
            eprintln!("majra: could not update start-at-login: {error}");
        }
    }
    Ok(saved)
}

/// Read the clipboard for the optional "watch for copied links" feature. Only the webview
/// asks for this, and only while the user has the setting switched on.
#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .map_err(|error| error.to_string())
}

/// Undo a pending `shutdown /s /t` started by the scheduler or "shut down when finished".
#[tauri::command]
pub fn cancel_shutdown() -> Result<(), String> {
    crate::power::cancel_shutdown()
}

#[tauri::command]
pub fn settings_path(state: State<'_, AppState>) -> String {
    state.settings.path().display().to_string()
}

#[tauri::command]
pub fn data_folder() -> String {
    data_dir().display().to_string()
}

#[tauri::command]
pub fn bridge_status(state: State<'_, AppState>) -> BridgeStatus {
    let port = state.bridge_port.load(Ordering::Relaxed);
    BridgeStatus {
        running: port != 0,
        port,
        ports: BRIDGE_PORTS.to_vec(),
        api: BRIDGE_API,
    }
}

fn open_path(path: &PathBuf, reveal: bool) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer");
        if reveal {
            command.arg(format!("/select,\"{}\"", path.display()));
        } else {
            command.arg(path);
        }
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        if reveal {
            command.arg("-R").arg(path);
        } else {
            command.arg(path);
        }
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = reveal;
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    open_path(&PathBuf::from(path), false)
}

#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    open_path(&PathBuf::from(path), true)
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|error| error.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("only http and https links can be opened".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    command
        .arg(parsed.as_str())
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn probe_link(
    state: State<'_, AppState>,
    url: String,
    offer_id: Option<String>,
) -> Result<ProbeResult, String> {
    let offer = offer_id
        .as_deref()
        .and_then(|id| state.offers.get(id))
        .filter(|offer| offer.url == url);
    let browser = offer.as_ref().and_then(|offer| offer.browser.clone());
    let mut probe = probe_url_with(&url, browser.as_ref())
        .await
        .map_err(message)?;
    if let Some(name) = offer.and_then(|offer| offer.file_name) {
        probe.file_name = name;
    }
    Ok(probe)
}

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    mut request: DownloadRequest,
    offer_id: Option<String>,
) -> Result<String, String> {
    state
        .shutdown_armed
        .store(false, std::sync::atomic::Ordering::Relaxed);
    if let Some(id) = offer_id {
        if let Some(offer) = state
            .offers
            .get(&id)
            .filter(|offer| offer.url == request.url)
        {
            request.browser = offer.browser.clone();
            if request.file_name.is_none() {
                request.file_name = offer.file_name.clone();
            }
            // The extension's probe is reused: a signed or one-time link must not be
            // fetched a second time before the transfer starts. Its requested URL is
            // normalized, so it is stamped with the exact link the window sent back.
            let mut probe = offer.probe.clone();
            probe.requested_url = request.url.clone();
            let result = state
                .manager
                .start_with_probe(request, probe)
                .await
                .map_err(message);
            if result.is_ok() {
                state.offers.remove(&id);
            }
            return result;
        }
    }
    state.manager.start(request).await.map_err(message)
}

#[tauri::command]
pub fn release_offer(state: State<'_, AppState>, id: String) {
    state.offers.remove(&id);
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.pause(&id).await.map_err(message)
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .shutdown_armed
        .store(false, std::sync::atomic::Ordering::Relaxed);
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
pub async fn set_max_active(state: State<'_, AppState>, value: u32) -> Result<u32, String> {
    state.manager.set_max_active(value);
    let saved = state.settings.set_max_active(value)?;
    Ok(saved.max_active)
}
