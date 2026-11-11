//! Optional Windows Defender scan of a finished file.
//!
//! Defender is already installed on Windows 10 and 11, so this shells out to its own
//! command-line scanner instead of shipping an antivirus engine. A machine with Defender
//! disabled or removed reports `unavailable` and the download itself is unaffected.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub id: String,
    pub file_name: String,
    /// clean | threat | unavailable | failed
    pub status: String,
    pub detail: String,
}

impl ScanResult {
    fn new(id: &str, path: &Path, status: &str, detail: String) -> Self {
        Self {
            id: id.to_string(),
            file_name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download")
                .to_string(),
            status: status.to_string(),
            detail,
        }
    }
}

#[cfg(target_os = "windows")]
fn mpcmdrun() -> Option<PathBuf> {
    let program_files = std::env::var_os("ProgramFiles")?;
    let direct = PathBuf::from(&program_files)
        .join("Windows Defender")
        .join("MpCmdRun.exe");
    if direct.is_file() {
        return Some(direct);
    }

    // Newer builds put the scanner under a versioned Platform folder.
    let program_data = std::env::var_os("ProgramData")?;
    let platform = PathBuf::from(program_data)
        .join("Microsoft")
        .join("Windows Defender")
        .join("Platform");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(platform)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    versions
        .into_iter()
        .rev()
        .map(|version| version.join("MpCmdRun.exe"))
        .find(|candidate| candidate.is_file())
}

/// `MpCmdRun.exe` only accepts a native Windows path. A destination typed with forward
/// slashes works for writing, but the scanner answers exit code 2 for it, which is easy
/// to mistake for a detection. `canonicalize` gives the one form every tool agrees on.
#[cfg(target_os = "windows")]
fn scanner_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Exit code 2 means "a threat was left in place" *and* "bad arguments" depending on the
/// build, so the output text is the part that decides. A clean scan always says so.
fn classify(text: &str, code: Option<i32>) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("found no threats")
        || lower.contains("no threats found")
        || lower.contains("no malware")
    {
        return Some("clean");
    }
    if lower.contains("threat") || lower.contains("malware") || lower.contains("virus") {
        return Some("threat");
    }
    match code {
        Some(0) => Some("clean"),
        _ => None,
    }
}

pub fn scan(id: &str, path: &Path) -> ScanResult {
    if !path.is_file() {
        return ScanResult::new(id, path, "unavailable", "the file is no longer on disk".into());
    }

    #[cfg(target_os = "windows")]
    {
        let Some(scanner) = mpcmdrun() else {
            return ScanResult::new(
                id,
                path,
                "unavailable",
                "Windows Defender's scanner was not found".into(),
            );
        };
        let target = scanner_path(path);
        // -ScanType 3 is a custom scan of the path. -DisableRemediation keeps the file in
        // place; the user decides what to do with a detection.
        match Command::new(scanner)
            .args([
                "-Scan",
                "-ScanType",
                "3",
                "-File",
                &target.display().to_string(),
                "-DisableRemediation",
            ])
            .output()
        {
            Ok(output) => {
                let text = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                let code = output.status.code();
                match classify(&text, code) {
                    Some("clean") => {
                        ScanResult::new(id, path, "clean", "Windows Defender found nothing".into())
                    }
                    Some("threat") => ScanResult::new(
                        id,
                        path,
                        "threat",
                        "Windows Defender reported a threat".into(),
                    ),
                    _ => {
                        let first = text
                            .lines()
                            .map(str::trim)
                            .find(|line| !line.is_empty())
                            .unwrap_or("no output");
                        ScanResult::new(
                            id,
                            path,
                            "failed",
                            format!(
                                "Windows Defender exited with {}: {first}",
                                code.map(|code| code.to_string())
                                    .unwrap_or_else(|| "no code".into())
                            ),
                        )
                    }
                }
            }
            Err(error) => ScanResult::new(id, path, "failed", error.to_string()),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        ScanResult::new(
            id,
            path,
            "unavailable",
            "Windows Defender is only available on Windows".into(),
        )
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::classify;

    #[test]
    fn a_clean_scan_is_not_mistaken_for_a_threat() {
        let clean = "Scan starting...\nScan finished.\nScanning C:\\a\\readme (1).txt found no threats.";
        assert_eq!(classify(clean, Some(0)), Some("clean"));
        // Exit code 2 also means "bad arguments", so the text has the final word.
        assert_eq!(classify(clean, Some(2)), Some("clean"));
    }

    #[test]
    fn a_detection_is_reported_as_a_threat() {
        let found = "Scan starting...\nScan finished.\nScanning C:\\a\\bad.exe found 1 threat.";
        assert_eq!(classify(found, Some(2)), Some("threat"));
        assert_eq!(classify("Threat detected: Trojan:Win32/Test", Some(2)), Some("threat"));
    }

    #[test]
    fn an_unreadable_answer_is_left_as_a_failure() {
        assert_eq!(classify("", Some(2)), None);
        assert_eq!(classify("", None), None);
    }
}
