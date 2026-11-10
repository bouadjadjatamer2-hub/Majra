//! Small command-line harness for the transfer core. It uses the same `DownloadManager`
//! as the desktop app, so it is useful for real-world smoke tests and benchmarks:
//!
//! ```text
//! cargo run --release -p majra-core --example fetch -- <url> [destination] [connections] [extract]
//! ```
//!
//! Pass `extract` as the fourth argument to unpack a ZIP archive while it downloads.

use majra_core::{DownloadEvent, DownloadManager, DownloadRequest};
use std::time::Instant;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(url) = args.next() else {
        eprintln!("usage: fetch <url> [destination] [connections]");
        std::process::exit(2);
    };
    let destination = args.next().unwrap_or_default();
    let connections = args
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(8);
    let extract = args
        .next()
        .is_some_and(|value| value == "extract" || value == "1" || value == "true");

    let manager = DownloadManager::new();
    let mut events = manager.subscribe();
    let started = Instant::now();
    let id = match manager
        .start(DownloadRequest {
            url,
            destination,
            connections,
            interface_ids: Vec::new(),
            block_size: None,
            extract,
        })
        .await
    {
        Ok(id) => id,
        Err(error) => {
            eprintln!("could not start: {error}");
            std::process::exit(1);
        }
    };
    println!("started {id} with {connections} connection(s)");

    let mut next_report = 8 * 1024 * 1024u64;
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(error) => {
                eprintln!("event stream failed: {error}");
                std::process::exit(1);
            }
        };
        match event {
            DownloadEvent::Progress { snapshot } => {
                if snapshot.downloaded_bytes >= next_report {
                    next_report = snapshot.downloaded_bytes + 8 * 1024 * 1024;
                    println!(
                        "{:>10} / {:>10}  {:>7.1} MB/s",
                        snapshot.downloaded_bytes,
                        snapshot.total_bytes,
                        snapshot.speed_bytes_per_sec / (1024.0 * 1024.0)
                    );
                }
            }
            DownloadEvent::Done { snapshot } => {
                let seconds = started.elapsed().as_secs_f64().max(0.001);
                println!(
                    "done: {} bytes in {:.2}s ({:.1} MB/s) -> {}",
                    snapshot.downloaded_bytes,
                    seconds,
                    snapshot.downloaded_bytes as f64 / (1024.0 * 1024.0) / seconds,
                    snapshot.destination
                );
                if let Some(extraction) = snapshot.extraction {
                    println!(
                        "extraction: {} | {}/{} entries | {}/{} bytes -> {}",
                        extraction.status,
                        extraction.entries_done,
                        extraction
                            .entries_total
                            .map_or("?".to_string(), |value| value.to_string()),
                        extraction.bytes_written,
                        extraction
                            .bytes_total
                            .map_or("?".to_string(), |value| value.to_string()),
                        extraction.folder
                    );
                    if let Some(error) = extraction.error {
                        eprintln!("extraction error: {error}");
                    }
                }
                break;
            }
            DownloadEvent::Error { snapshot } => {
                eprintln!("failed: {}", snapshot.error.unwrap_or_default());
                std::process::exit(1);
            }
            DownloadEvent::Paused { .. } | DownloadEvent::Cancelled { .. } => {
                eprintln!("the download stopped before completion");
                std::process::exit(1);
            }
        }
    }
}
