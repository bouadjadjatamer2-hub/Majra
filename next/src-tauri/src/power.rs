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

/// Keep the machine and its disks awake while a transfer is running, so a long download
/// is not paused by sleep or by a disk that spun down. `SetThreadExecutionState` applies
/// to the calling thread, so a dedicated thread owns the state and flips it on request.
#[cfg(target_os = "windows")]
pub fn set_keep_awake(on: bool) {
    use std::sync::mpsc::{channel, Sender};
    use std::sync::OnceLock;
    use windows_sys::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };

    static KEEPER: OnceLock<Sender<bool>> = OnceLock::new();
    let sender = KEEPER.get_or_init(|| {
        let (sender, receiver) = channel::<bool>();
        std::thread::Builder::new()
            .name("majra-keep-awake".into())
            .spawn(move || {
                let mut current = false;
                for wanted in receiver {
                    if wanted == current {
                        continue;
                    }
                    current = wanted;
                    let flags = if wanted {
                        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
                    } else {
                        ES_CONTINUOUS
                    };
                    unsafe {
                        SetThreadExecutionState(flags);
                    }
                }
                unsafe {
                    SetThreadExecutionState(ES_CONTINUOUS);
                }
            })
            .ok();
        sender
    });
    let _ = sender.send(on);
}

#[cfg(not(target_os = "windows"))]
pub fn set_keep_awake(_on: bool) {}
