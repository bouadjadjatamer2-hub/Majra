//! Live regression probe for a slow CDN: a large release asset that delivers roughly one
//! megabyte per second in 1371-byte TLS chunks. It used to be killed by the stall
//! watchdog because no 1 MiB write landed inside the first-byte window.
//!
//! Run it on demand, pointing at any slow direct link:
//!
//! ```text
//! MAJRA_REPRO_URL=https://... MAJRA_REPRO_CONNECTIONS=32 \
//!   cargo test -p majra-core --test slow_host -- --ignored --nocapture
//! ```

use majra_core::{
    BrowserContext, BrowserCookie, DownloadEvent, DownloadManager, DownloadRequest,
    DownloadStatus,
};
use std::time::Duration;

fn cookies() -> Vec<BrowserCookie> {
    vec![BrowserCookie {
        name: "logged_in".into(),
        value: "yes".into(),
        domain: ".github.com".into(),
        host_only: false,
        path: "/".into(),
        secure: true,
    }]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn github_release_asset_downloads() {
    let url = std::env::var("MAJRA_REPRO_URL").expect("MAJRA_REPRO_URL");
    let connections: u32 = std::env::var("MAJRA_REPRO_CONNECTIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(32);
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("asset.bin");
    let manager = DownloadManager::with_catalog_path(dir.path().join("downloads.json"));
    let mut rx = manager.subscribe();
    let id = manager
        .start(DownloadRequest {
            url,
            destination: target.display().to_string(),
            connections,
            interface_ids: Vec::new(),
            block_size: None,
            extract: false,
            file_name: None,
            browser: Some(BrowserContext {
                cookies: cookies(),
                referer: Some("https://bouadjadjatamer2-hub.github.io/".into()),
                user_agent: Some(
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36"
                        .into(),
                ),
                name: Some("Chrome".into()),
            }),
        })
        .await
        .expect("start");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(900);
    loop {
        let event = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("timed out")
            .expect("closed");
        let snapshot = match event {
            DownloadEvent::Progress { snapshot }
            | DownloadEvent::Done { snapshot }
            | DownloadEvent::Paused { snapshot }
            | DownloadEvent::Cancelled { snapshot }
            | DownloadEvent::Error { snapshot } => snapshot,
        };
        if snapshot.id != id {
            continue;
        }
        if matches!(
            snapshot.status,
            DownloadStatus::Completed | DownloadStatus::Error | DownloadStatus::Paused
        ) {
            println!(
                "status={:?} downloaded={} total={} error={:?}",
                snapshot.status, snapshot.downloaded_bytes, snapshot.total_bytes, snapshot.error
            );
            assert_eq!(snapshot.status, DownloadStatus::Completed);
            let len = std::fs::metadata(&target).expect("metadata").len();
            assert_eq!(len, snapshot.total_bytes);
            break;
        }
    }
}
