//! A daily download window: start the queue at `schedule_start`, stop it at
//! `schedule_stop`, then optionally pause, exit or shut the machine down.
//!
//! The window can cross midnight (`22:00` → `06:00`). Times are local and compared as
//! minutes since midnight, so the check costs nothing and needs no date arithmetic.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Local, Timelike};
use majra_core::DownloadManager;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::power;
use crate::settings::SettingsStore;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEvent {
    /// "start" or "stop"
    pub state: String,
    /// What the stop transition did: pause | exit | shutdown
    pub action: String,
}

/// `HH:MM` → minutes since midnight. The settings layer normalizes the value first, so a
/// failure here only means a hand-edited file slipped through.
pub fn parse_minutes(value: &str) -> Option<u32> {
    let (hour, minute) = value.split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

/// Inclusive start, exclusive stop. A window whose ends match is treated as empty rather
/// than as a full day, so an accidental `02:00` → `02:00` cannot start downloads forever.
pub fn in_window(now: u32, start: u32, stop: u32) -> bool {
    if start == stop {
        return false;
    }
    if start < stop {
        now >= start && now < stop
    } else {
        now >= start || now < stop
    }
}

fn now_minutes() -> u32 {
    let now = Local::now();
    now.hour() * 60 + now.minute()
}

/// Run forever. A transition is acted on once; the 20-second tick is only a clock, so a
/// suspended laptop still notices a window that it slept through.
pub async fn run(app: AppHandle, manager: Arc<DownloadManager>, settings: Arc<SettingsStore>) {
    let mut was_inside = false;
    loop {
        let saved = settings.get();
        let window = if saved.schedule_enabled {
            match (
                parse_minutes(&saved.schedule_start),
                parse_minutes(&saved.schedule_stop),
            ) {
                (Some(start), Some(stop)) => Some(in_window(now_minutes(), start, stop)),
                _ => None,
            }
        } else {
            None
        };

        match window {
            Some(true) if !was_inside => {
                manager.resume_paused().await;
                let _ = app.emit(
                    "majra://schedule",
                    ScheduleEvent {
                        state: "start".into(),
                        action: "resume".into(),
                    },
                );
            }
            Some(false) if was_inside => {
                manager.pause_all().await;
                let action = saved.schedule_stop_action.clone();
                let _ = app.emit(
                    "majra://schedule",
                    ScheduleEvent {
                        state: "stop".into(),
                        action: action.clone(),
                    },
                );
                match action.as_str() {
                    "exit" => app.exit(0),
                    "shutdown" => {
                        if let Err(error) = power::shutdown(60) {
                            eprintln!("majra: scheduled shutdown failed: {error}");
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        was_inside = window.unwrap_or(false);
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_daytime_window_has_clear_edges() {
        let start = parse_minutes("09:30").unwrap();
        let stop = parse_minutes("18:00").unwrap();
        assert!(!in_window(parse_minutes("09:29").unwrap(), start, stop));
        assert!(in_window(parse_minutes("09:30").unwrap(), start, stop));
        assert!(in_window(parse_minutes("17:59").unwrap(), start, stop));
        assert!(!in_window(parse_minutes("18:00").unwrap(), start, stop));
    }

    #[test]
    fn a_window_can_cross_midnight() {
        let start = parse_minutes("22:00").unwrap();
        let stop = parse_minutes("06:00").unwrap();
        assert!(in_window(parse_minutes("23:15").unwrap(), start, stop));
        assert!(in_window(parse_minutes("02:00").unwrap(), start, stop));
        assert!(!in_window(parse_minutes("12:00").unwrap(), start, stop));
    }

    #[test]
    fn equal_ends_do_not_mean_all_day() {
        let start = parse_minutes("02:00").unwrap();
        assert!(!in_window(parse_minutes("02:00").unwrap(), start, start));
        assert!(!in_window(parse_minutes("12:00").unwrap(), start, start));
    }

    #[test]
    fn invalid_times_are_rejected() {
        assert!(parse_minutes("24:00").is_none());
        assert!(parse_minutes("12:60").is_none());
        assert!(parse_minutes("noon").is_none());
    }
}
