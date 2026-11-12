mod commands;
mod power;
mod scan;
mod schedule;
mod settings;

use commands::{AppState, OfferStore};
use majra_core::{bridge, DownloadEvent, DownloadManager};
use settings::SettingsStore;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // A second launch (or a majra://open link from the extension) belongs to the running
    // copy: it comes back to the front instead of starting a second tray icon and bridge.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }));
    }
    builder
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // A portable build has no installer to write the scheme, so register it on
            // first run. A failure (a locked registry, a managed machine) only means the
            // extension's wake button can't start Majra; everything else still works.
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(error) = app.deep_link().register_all() {
                    eprintln!("majra: could not register majra:// links: {error}");
                }
            }

            let preferences = Arc::new(SettingsStore::load());
            let initial = preferences.get();
            settings::apply_proxy(&initial);
            let manager = Arc::new(DownloadManager::new());
            manager.set_max_active(initial.max_active);
            let offers = Arc::new(OfferStore::default());
            let bridge_port = Arc::new(AtomicU16::new(0));
            let shutdown_armed = Arc::new(AtomicBool::new(false));

            // Keep the operating system's startup entry in step with the saved choice.
            #[cfg(desktop)]
            {
                use tauri_plugin_autostart::ManagerExt;
                let result = if initial.start_at_login {
                    app.autolaunch().enable()
                } else {
                    app.autolaunch().disable()
                };
                if let Err(error) = result {
                    eprintln!("majra: could not sync start-at-login: {error}");
                }
            }

            app.manage(AppState {
                manager: manager.clone(),
                offers: offers.clone(),
                settings: preferences.clone(),
                bridge_port: bridge_port.clone(),
                shutdown_armed: shutdown_armed.clone(),
            });

            // One aggregated stream to the webview. The core emits at 4 Hz; the UI never
            // sees a per-chunk message. A finished row also feeds the two optional
            // after-download actions, so neither needs its own worker per download.
            let handle = app.handle().clone();
            let event_manager = manager.clone();
            let event_settings = preferences.clone();
            let event_armed = shutdown_armed.clone();
            tauri::async_runtime::spawn(async move {
                let mut events = event_manager.subscribe();
                loop {
                    match events.recv().await {
                        Ok(event) => {
                            let finished = match &event {
                                DownloadEvent::Done { snapshot } => Some((
                                    snapshot.id.clone(),
                                    PathBuf::from(&snapshot.destination),
                                )),
                                _ => None,
                            };
                            let _ = handle.emit("majra://download", event);

                            // Sleep is the one thing a long download cannot survive, so
                            // hold the machine awake only while something is moving or
                            // waiting to move.
                            if event_settings.get().keep_awake {
                                power::set_keep_awake(event_manager.has_pending());
                            } else {
                                power::set_keep_awake(false);
                            }

                            let Some((id, path)) = finished else {
                                continue;
                            };
                            if event_settings.get().scan_with_defender {
                                let scan_handle = handle.clone();
                                tauri::async_runtime::spawn(async move {
                                    let result = tokio::task::spawn_blocking(move || {
                                        scan::scan(&id, &path)
                                    })
                                    .await;
                                    if let Ok(result) = result {
                                        let _ = scan_handle.emit("majra://scan", result);
                                    }
                                });
                            }
                            if event_settings.get().shutdown_when_done
                                && !event_manager.has_pending()
                            {
                                // Let the queue admit the next row before arming shutdown.
                                let manager = event_manager.clone();
                                let settings = event_settings.clone();
                                let armed = event_armed.clone();
                                let shutdown_handle = handle.clone();
                                tauri::async_runtime::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                    if !settings.get().shutdown_when_done
                                        || manager.has_pending()
                                        || armed.swap(true, Ordering::SeqCst)
                                    {
                                        return;
                                    }
                                    match power::shutdown(60) {
                                        Ok(()) => {
                                            let _ = shutdown_handle.emit("majra://shutdown", 60u64);
                                        }
                                        Err(error) => {
                                            armed.store(false, Ordering::SeqCst);
                                            eprintln!("majra: shutdown failed: {error}");
                                        }
                                    }
                                });
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });

            // The browser bridge holds an offer until the extension confirms that its own
            // copy is gone, then this task shows the window and hands the link to the UI.
            let (sender, mut receiver) =
                tokio::sync::mpsc::unbounded_channel::<majra_core::bridge::BridgeOffer>();
            let handle = app.handle().clone();
            let store = offers.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(offer) = receiver.recv().await {
                    store.insert(offer.clone());
                    let _ = handle.emit("majra://offer", &offer);
                    show_main(&handle);
                }
            });
            let version = app.package_info().version.to_string();
            tauri::async_runtime::spawn(bridge::serve(version, sender, bridge_port));

            let scheduler_manager = manager.clone();
            let scheduler_settings = preferences.clone();
            let scheduler_handle = app.handle().clone();
            tauri::async_runtime::spawn(schedule::run(
                scheduler_handle,
                scheduler_manager,
                scheduler_settings,
            ));

            // The tray is the app's home once the window is closed: downloads and the
            // browser bridge keep running there.
            let open = MenuItem::with_id(app, "open", "Open Majra", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit Majra", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let hidden = window.clone();
                let handle = app.handle().clone();
                let preferences = preferences.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        if preferences.get().close_to_tray {
                            api.prevent_close();
                            let _ = hidden.hide();
                        } else {
                            handle.exit(0);
                        }
                    }
                });
                if initial.start_minimized {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_interfaces,
            commands::test_interface,
            commands::get_settings,
            commands::save_settings,
            commands::settings_path,
            commands::data_folder,
            commands::bridge_status,
            commands::open_folder,
            commands::reveal_path,
            commands::open_file,
            commands::open_url,
            commands::system_profile,
            commands::probe_link,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::forget_download,
            commands::list_downloads,
            commands::set_max_active,
            commands::download_snapshot,
            commands::release_offer,
            commands::read_clipboard,
            commands::cancel_shutdown,
        ])
        .run(tauri::generate_context!())
        .expect("Majra failed to start");
}
