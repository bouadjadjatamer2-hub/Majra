//! Machine actions the scheduler and "shut down when finished" share.

use std::process::Command;

/// Ask Windows to shut down after `delay_seconds`. The command returns immediately; the
/// delay is what gives the user a chance to cancel from the toast.
pub fn shutdown(delay_seconds: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let delay = delay_seconds.min(600).to_string();
        Command::new("shutdown")
            .args(["/s", "/t", delay.as_str()])
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = delay_seconds;
        Err("automatic shutdown is only wired on Windows".into())
    }
}

/// Undo a pending shutdown (`shutdown /a`). Harmless when nothing is pending.
pub fn cancel_shutdown() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("shutdown")
            .arg("/a")
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("automatic shutdown is only wired on Windows".into())
    }
}
