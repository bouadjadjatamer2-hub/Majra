use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Preferences that affect the backend. Theme, language and the compact layout stay in
/// the webview's own storage because they never leave the UI process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Empty means the operating system's Downloads folder.
    pub default_destination: String,
    pub default_connections: u32,
    pub max_active: u32,
    pub extract_by_default: bool,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub confirm_remove: bool,
    /// Run Majra when this Windows account signs in.
    pub start_at_login: bool,
    /// Shut the machine down once the last active download finishes.
    pub shutdown_when_done: bool,
    /// Ask Windows Defender to scan each finished file.
    pub scan_with_defender: bool,
    /// Watch the clipboard for a copied link and offer it as a new download.
    pub clipboard_watch: bool,
    /// A daily window in which downloads are allowed to run.
    pub schedule_enabled: bool,
    /// `HH:MM`, local time.
    pub schedule_start: String,
    /// `HH:MM`, local time.
    pub schedule_stop: String,
    /// pause | exit | shutdown
    pub schedule_stop_action: String,
    /// system | none | manual
    pub proxy_mode: String,
    pub proxy_url: String,
    pub proxy_username: String,
    pub proxy_password: String,
    /// Pick the connection count from the adapter's link speed instead of a fixed number.
    pub auto_tune: bool,
    /// Keep Windows (and the disk) awake while a download is running.
    pub keep_awake: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_destination: String::new(),
            default_connections: 8,
            max_active: 3,
            extract_by_default: false,
            close_to_tray: true,
            start_minimized: false,
            confirm_remove: true,
            start_at_login: false,
            shutdown_when_done: false,
            scan_with_defender: false,
            clipboard_watch: false,
            schedule_enabled: false,
            schedule_start: "02:00".into(),
            schedule_stop: "07:00".into(),
            schedule_stop_action: "pause".into(),
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            auto_tune: true,
            keep_awake: true,
        }
    }
}

fn normalize_hhmm(value: &str) -> Option<String> {
    let (hour, minute) = value.split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{hour:02}:{minute:02}"))
}

fn one_of(value: &str, allowed: &[&str], fallback: &str) -> String {
    if allowed.contains(&value) {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

impl Settings {
    /// Keep a hand-edited or older settings file inside the range the engine supports.
    pub fn sanitized(mut self) -> Self {
        self.default_destination = self.default_destination.trim().to_string();
        self.default_connections = self.default_connections.clamp(1, 32);
        self.max_active = self.max_active.clamp(1, 8);
        self.schedule_start =
            normalize_hhmm(&self.schedule_start).unwrap_or_else(|| "02:00".into());
        self.schedule_stop =
            normalize_hhmm(&self.schedule_stop).unwrap_or_else(|| "07:00".into());
        self.schedule_stop_action =
            one_of(&self.schedule_stop_action, &["pause", "exit", "shutdown"], "pause");
        self.proxy_mode = one_of(&self.proxy_mode, &["system", "none", "manual"], "system");
        self.proxy_url = self.proxy_url.trim().to_string();
        self.proxy_username = self.proxy_username.trim().to_string();
        self
    }
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("app.majra.desktop")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub struct SettingsStore {
    path: PathBuf,
    value: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load() -> Self {
        Self::open(settings_path())
    }

    pub fn open(path: PathBuf) -> Self {
        let value = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .unwrap_or_default()
            .sanitized();
        Self {
            path,
            value: Mutex::new(value),
        }
    }

    pub fn get(&self) -> Settings {
        self.value.lock().unwrap().clone()
    }

    pub fn save(&self, next: Settings) -> Result<Settings, String> {
        let next = next.sanitized();
        write_json(&self.path, &next)?;
        *self.value.lock().unwrap() = next.clone();
        Ok(next)
    }

    /// Keep the parallel-download limit in the same file as every other preference.
    pub fn set_max_active(&self, value: u32) -> Result<Settings, String> {
        let mut next = self.get();
        next.max_active = value.clamp(1, 8);
        self.save(next)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Push the saved proxy choice into the core, which every probe and transfer client reads.
pub fn apply_proxy(settings: &Settings) {
    use majra_core::proxy::{ProxyConfig, ProxyMode};
    let mode = match settings.proxy_mode.as_str() {
        "none" => ProxyMode::None,
        "manual" => ProxyMode::Manual,
        _ => ProxyMode::System,
    };
    majra_core::proxy::set(ProxyConfig {
        mode,
        url: settings.proxy_url.clone(),
        username: settings.proxy_username.clone(),
        password: settings.proxy_password.clone(),
    });
}

fn write_json(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json).map_err(|error| error.to_string())?;
    let _ = fs::remove_file(path);
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_a_restart_and_clamp_bad_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        {
            let store = SettingsStore::open(path.clone());
            let saved = store
                .save(Settings {
                    default_destination: "  D:\\Media  ".into(),
                    default_connections: 99,
                    max_active: 0,
                    extract_by_default: true,
                    close_to_tray: false,
                    start_minimized: true,
                    confirm_remove: false,
                    schedule_enabled: true,
                    schedule_start: "1:5".into(),
                    schedule_stop: "25:99".into(),
                    schedule_stop_action: "explode".into(),
                    proxy_mode: "manual".into(),
                    proxy_url: "  socks5://127.0.0.1:1080  ".into(),
                    ..Settings::default()
                })
                .expect("save");
            assert_eq!(saved.default_connections, 32);
            assert_eq!(saved.max_active, 1);
            assert_eq!(saved.default_destination, "D:\\Media");
            assert_eq!(saved.schedule_start, "01:05");
            assert_eq!(saved.schedule_stop, "07:00");
            assert_eq!(saved.schedule_stop_action, "pause");
            assert_eq!(saved.proxy_url, "socks5://127.0.0.1:1080");
        }

        let store = SettingsStore::open(path);
        let settings = store.get();
        assert!(settings.extract_by_default);
        assert!(!settings.close_to_tray);
        assert!(settings.start_minimized);
        assert!(!settings.confirm_remove);
        assert!(settings.schedule_enabled);
        assert_eq!(settings.proxy_mode, "manual");
    }

    #[test]
    fn an_unknown_proxy_mode_falls_back_to_the_system_route() {
        let settings = Settings {
            proxy_mode: "tunnel".into(),
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.proxy_mode, "system");
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        fs::write(&path, b"{ nope").expect("write");
        let store = SettingsStore::open(path);
        assert_eq!(store.get().max_active, 3);
    }
}
