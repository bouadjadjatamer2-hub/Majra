mod commands;

use commands::AppState;
use majra_core::DownloadManager;
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let manager = Arc::new(DownloadManager::new());
            app.manage(AppState {
                manager: manager.clone(),
            });

            // One aggregated stream to the webview. The core emits at 4 Hz; the UI never
            // sees a per-chunk message.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut events = manager.subscribe();
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            let _ = handle.emit("majra://download", event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_interfaces,
            commands::probe_link,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::forget_download,
            commands::list_downloads,
            commands::set_max_active,
            commands::set_speed_limit,
            commands::download_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("Majra failed to start");
}
