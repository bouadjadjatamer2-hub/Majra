//! End-to-end tests for the transfer core against a local HTTP range server.
//! They verify the two things that matter most: the bytes on disk are exact, and a
//! paused download resumes without skipping or duplicating data.

use majra_core::{
    DownloadEvent, DownloadManager, DownloadRequest, DownloadSnapshot, DownloadStatus,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

#[derive(Clone, Copy)]
struct ServerConfig {
    ranges: bool,
    throttle: Option<(usize, Duration)>,
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
}

async fn start_server(payload: Arc<Vec<u8>>, config: ServerConfig) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let payload = payload.clone();
            tokio::spawn(async move { serve(stream, payload, config).await });
        }
    });
    (addr, handle)
}

async fn serve(mut stream: TcpStream, payload: Arc<Vec<u8>>, config: ServerConfig) {
    let mut head = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        let Ok(n) = stream.read(&mut buf).await else {
            return;
        };
        if n == 0 {
            return;
        }
        head.extend_from_slice(&buf[..n]);
        if head.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if head.len() > 64 * 1024 {
            return;
        }
    }

    let text = String::from_utf8_lossy(&head);
    let range = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("range") {
            return None;
        }
        let spec = value.trim().strip_prefix("bytes=")?;
        let (start, end) = spec.split_once('-')?;
        Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?))
    });

    let total = payload.len() as u64;
    let (start, end, partial) = match range {
        Some((start, end)) if config.ranges && total > 0 => {
            (start.min(total - 1), end.min(total - 1), true)
        }
        _ => (0, total.saturating_sub(1), false),
    };

    let header = if partial {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nETag: \"majra-test\"\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
            end - start + 1
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nETag: \"majra-test\"\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n"
        )
    };
    if stream.write_all(header.as_bytes()).await.is_err() {
        return;
    }

    let body = &payload[start as usize..=end as usize];
    match config.throttle {
        Some((chunk, delay)) => {
            for piece in body.chunks(chunk) {
                if stream.write_all(piece).await.is_err() {
                    return;
                }
                tokio::time::sleep(delay).await;
            }
        }
        None => {
            let _ = stream.write_all(body).await;
        }
    }
    let _ = stream.shutdown().await;
}

fn snapshot_of(event: DownloadEvent) -> DownloadSnapshot {
    match event {
        DownloadEvent::Progress { snapshot }
        | DownloadEvent::Done { snapshot }
        | DownloadEvent::Paused { snapshot }
        | DownloadEvent::Cancelled { snapshot }
        | DownloadEvent::Error { snapshot } => snapshot,
    }
}

async fn wait_status(
    rx: &mut broadcast::Receiver<DownloadEvent>,
    id: &str,
    status: DownloadStatus,
) -> DownloadSnapshot {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timed out waiting for the download")
            .expect("event stream closed");
        let snapshot = snapshot_of(event);
        if snapshot.id == id && snapshot.status == status {
            return snapshot;
        }
    }
}

async fn wait_progress(
    rx: &mut broadcast::Receiver<DownloadEvent>,
    id: &str,
) -> DownloadSnapshot {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timed out waiting for progress")
            .expect("event stream closed");
        let snapshot = snapshot_of(event);
        if snapshot.id == id
            && snapshot.status == DownloadStatus::Downloading
            && snapshot.downloaded_bytes > 0
        {
            return snapshot;
        }
    }
}

/// With one slot, a second download must wait in the queue and start only when the
/// first one stops, the way IDM schedules its list.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn second_download_waits_in_queue_until_a_slot_frees() {
    let data = Arc::new(payload(8 * 1024 * 1024));
    let (addr, server) = start_server(
        data.clone(),
        ServerConfig {
            ranges: true,
            throttle: Some((16 * 1024, Duration::from_millis(2))),
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let first_target = dir.path().join("first.bin");
    let second_target = dir.path().join("second.bin");

    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    manager.set_max_active(1);
    assert_eq!(manager.max_active(), 1);
    let mut rx = manager.subscribe();

    let first = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/first.bin"),
            destination: first_target.display().to_string(),
            connections: 2,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start first");
    let second = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/second.bin"),
            destination: second_target.display().to_string(),
            connections: 2,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start second");

    let queued = wait_status(&mut rx, &second, DownloadStatus::Queued).await;
    assert_eq!(queued.downloaded_bytes, 0);
    assert_eq!(manager.active_count(), 1, "only the first row runs");
    assert!(
        !second_target.exists(),
        "a queued download must not create its file yet"
    );

    // Pausing the running row frees the slot and the queued one takes it.
    manager.pause(&first).await.expect("pause first");
    wait_status(&mut rx, &first, DownloadStatus::Paused).await;
    wait_status(&mut rx, &second, DownloadStatus::Downloading).await;
    let done = wait_status(&mut rx, &second, DownloadStatus::Completed).await;
    assert_eq!(done.downloaded_bytes, data.len() as u64);
    assert_eq!(std::fs::read(&second_target).expect("read"), *data);

    // The paused row can resume now that nothing else is running.
    manager.resume(&first).await.expect("resume first");
    let done = wait_status(&mut rx, &first, DownloadStatus::Completed).await;
    assert_eq!(done.downloaded_bytes, data.len() as u64);
    assert_eq!(std::fs::read(&first_target).expect("read"), *data);
    server.abort();
}

/// Raising the limit must fill every free slot, not just one, and lowering it must not
/// start anything while the running rows still exceed it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn raising_the_limit_fills_every_free_slot() {
    let data = Arc::new(payload(16 * 1024 * 1024));
    let (addr, server) = start_server(
        data,
        ServerConfig {
            ranges: true,
            throttle: Some((16 * 1024, Duration::from_millis(10))),
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    manager.set_max_active(1);
    let mut rx = manager.subscribe();

    let start = |name: &str| {
        let manager = manager.clone();
        let url = format!("http://{addr}/{name}");
        let destination = dir.path().join(name).display().to_string();
        async move {
            manager
                .start(DownloadRequest {
                    url,
                    destination,
                    connections: 2,
                    interface_ids: Vec::new(),
                    block_size: None,
                    extract: false,
                })
                .await
                .expect("start")
        }
    };

    let first = start("raise-a.bin").await;
    let second = start("raise-b.bin").await;
    let third = start("raise-c.bin").await;
    wait_status(&mut rx, &second, DownloadStatus::Queued).await;
    wait_status(&mut rx, &third, DownloadStatus::Queued).await;
    assert_eq!(manager.active_count(), 1);

    manager.set_max_active(3);
    wait_status(&mut rx, &second, DownloadStatus::Downloading).await;
    wait_status(&mut rx, &third, DownloadStatus::Downloading).await;
    assert_eq!(manager.active_count(), 3);

    manager.set_max_active(1);
    assert_eq!(
        manager.active_count(),
        3,
        "lowering the limit never stops a running row"
    );
    let fourth = start("raise-d.bin").await;
    wait_status(&mut rx, &fourth, DownloadStatus::Queued).await;
    assert_eq!(manager.active_count(), 3);

    for id in [&first, &second, &third, &fourth] {
        manager.cancel(id).await.ok();
        manager.forget(id);
    }
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multi_connection_download_matches_bytes() {
    let data = Arc::new(payload(4 * 1024 * 1024));
    let (addr, server) = start_server(
        data.clone(),
        ServerConfig {
            ranges: true,
            throttle: None,
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("multi.bin");

    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/multi.bin"),
            destination: target.display().to_string(),
            connections: 4,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start");

    let done = wait_status(&mut rx, &id, DownloadStatus::Completed).await;
    assert_eq!(done.downloaded_bytes, data.len() as u64);
    assert_eq!(done.connections.len(), 4);
    let written = std::fs::read(&target).expect("read file");
    assert_eq!(written, *data, "downloaded bytes differ from the source");
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_then_resume_keeps_integrity() {
    let data = Arc::new(payload(24 * 1024 * 1024));
    let (addr, server) = start_server(
        data.clone(),
        ServerConfig {
            ranges: true,
            throttle: Some((16 * 1024, Duration::from_millis(2))),
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("resume.bin");

    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/resume.bin"),
            destination: target.display().to_string(),
            connections: 4,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start");

    let progress = wait_progress(&mut rx, &id).await;
    assert!(progress.downloaded_bytes < progress.total_bytes);

    manager.pause(&id).await.expect("pause");
    let paused = wait_status(&mut rx, &id, DownloadStatus::Paused).await;
    assert!(paused.downloaded_bytes > 0);
    assert!(paused.downloaded_bytes < paused.total_bytes);
    assert!(target.exists());
    let state_file = dir.path().join("resume.bin.majra.json");
    assert!(state_file.exists(), "pause must persist resume state");

    manager.resume(&id).await.expect("resume");
    let done = wait_status(&mut rx, &id, DownloadStatus::Completed).await;
    assert_eq!(done.downloaded_bytes, data.len() as u64);
    let written = std::fs::read(&target).expect("read file");
    assert_eq!(written, *data, "resumed bytes differ from the source");
    assert!(!state_file.exists(), "completed download drops its state file");
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_after_pause_removes_partial_file() {
    let data = Arc::new(payload(16 * 1024 * 1024));
    let (addr, server) = start_server(
        data,
        ServerConfig {
            ranges: true,
            throttle: Some((16 * 1024, Duration::from_millis(2))),
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("cancel.bin");

    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/cancel.bin"),
            destination: target.display().to_string(),
            connections: 4,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start");

    wait_progress(&mut rx, &id).await;
    manager.pause(&id).await.expect("pause");
    wait_status(&mut rx, &id, DownloadStatus::Paused).await;
    assert!(target.exists());

    manager.cancel(&id).await.expect("cancel");
    let cancelled = manager.snapshot(&id).expect("snapshot");
    assert_eq!(cancelled.status, DownloadStatus::Cancelled);
    assert!(!target.exists(), "cancel must remove the partial file");
    assert!(
        !dir.path().join("cancel.bin.majra.json").exists(),
        "cancel must remove the resume state"
    );
    assert!(manager.forget(&id));
    assert!(manager.snapshot(&id).is_none());
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_without_ranges_falls_back_to_one_stream() {
    let data = Arc::new(payload(2 * 1024 * 1024));
    let (addr, server) = start_server(
        data.clone(),
        ServerConfig {
            ranges: false,
            throttle: None,
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("single.bin");

    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/single.bin"),
            destination: target.display().to_string(),
            connections: 8,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start");

    let done = wait_status(&mut rx, &id, DownloadStatus::Completed).await;
    assert_eq!(done.downloaded_bytes, data.len() as u64);
    let written = std::fs::read(&target).expect("read file");
    assert_eq!(written, *data);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn history_survives_a_restart_and_resumes() {
    let data = Arc::new(payload(8 * 1024 * 1024));
    let (addr, server) = start_server(
        data.clone(),
        ServerConfig {
            ranges: true,
            throttle: Some((16 * 1024, Duration::from_millis(2))),
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("history.bin");
    let catalog_path = dir.path().join("downloads.json");

    let manager = DownloadManager::with_catalog_path(catalog_path.clone());
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/history.bin"),
            destination: target.display().to_string(),
            connections: 2,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
        })
        .await
        .expect("start");

    wait_progress(&mut rx, &id).await;
    manager.pause(&id).await.expect("pause");
    let paused = wait_status(&mut rx, &id, DownloadStatus::Paused).await;
    let partial = paused.downloaded_bytes;
    assert!(partial > 0 && partial < data.len() as u64);
    drop(manager);

    // A new manager reads the catalog: the row is back, paused, with its byte count.
    let manager = DownloadManager::with_catalog_path(catalog_path);
    let restored = manager.list();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, id);
    assert_eq!(restored[0].status, DownloadStatus::Paused);
    assert_eq!(restored[0].downloaded_bytes, partial);

    let mut rx = manager.subscribe();
    manager.resume(&id).await.expect("resume restored");
    let done = wait_status(&mut rx, &id, DownloadStatus::Completed).await;
    assert_eq!(done.downloaded_bytes, data.len() as u64);
    assert_eq!(std::fs::read(&target).expect("read"), *data);
    server.abort();
}

/// Minimal stored-only ZIP writer for tests. Sizes live in the local header, which is the
/// shape Majra's progressive extractor can consume before the central directory arrives.
fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32fast::hash(data);
        let size = data.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    let central_size = central.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn extracts_zip_while_the_download_is_running() {
    let big = payload(200 * 1024);
    let small = payload(300 * 1024);
    let archive = Arc::new(stored_zip(&[
        ("alpha.bin", big.as_slice()),
        ("folder/beta.bin", small.as_slice()),
        ("../escape.txt", &b"must not land outside"[..]),
    ]));
    // One connection plus small blocks makes the order deterministic: alpha.bin lands in
    // the first blocks, so the extractor can finish it while the rest still arrives.
    let (addr, server) = start_server(
        archive,
        ServerConfig {
            ranges: true,
            throttle: Some((2048, Duration::from_millis(3))),
        },
    )
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("bundle.zip");

    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url: format!("http://{addr}/bundle.zip"),
            destination: target.display().to_string(),
            connections: 1,
            interface_ids: Vec::new(),
            block_size: Some(128 * 1024),
            extract: true,
        })
        .await
        .expect("start");

    let mut overlapped = false;
    let done = loop {
        let event = rx.recv().await.expect("event");
        match event {
            DownloadEvent::Progress { snapshot } => {
                if snapshot.status == DownloadStatus::Downloading
                    && snapshot
                        .extraction
                        .as_ref()
                        .is_some_and(|extraction| extraction.entries_done > 0)
                {
                    overlapped = true;
                }
            }
            DownloadEvent::Done { snapshot } => break snapshot,
            DownloadEvent::Error { snapshot } => {
                panic!("download failed: {:?}", snapshot.error);
            }
            _ => {}
        }
    };

    let extraction = done.extraction.expect("extraction snapshot");
    assert_eq!(done.id, id);
    assert_eq!(extraction.status, "done", "{:?}", extraction.error);
    assert_eq!(extraction.entries_total, Some(3));
    assert_eq!(extraction.entries_done, 3, "the traversal entry is processed");
    assert!(overlapped, "extraction must start before the download ends");

    let root = dir.path().join("bundle");
    assert_eq!(std::fs::read(root.join("alpha.bin")).expect("alpha"), big);
    assert_eq!(
        std::fs::read(root.join("folder").join("beta.bin")).expect("beta"),
        small
    );
    assert!(
        !dir.path().join("escape.txt").exists(),
        "zip-slip entry escaped the extraction folder"
    );
    server.abort();
}
