//! End-to-end tests for the localhost bridge, using the same HTTP requests the browser
//! extension makes. They cover the happy hand-off and every refusal that keeps a web page
//! (or another program) from starting a download.

use majra_core::bridge::{self, BridgeOffer};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const PAYLOAD_LEN: usize = 64 * 1024;

async fn start_file_server() -> (SocketAddr, Arc<Mutex<Vec<HashMap<String, String>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let recorder = recorder.clone();
            tokio::spawn(async move { serve(stream, recorder).await });
        }
    });
    (addr, seen)
}

async fn serve(mut stream: TcpStream, seen: Arc<Mutex<Vec<HashMap<String, String>>>>) {
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
    let mut headers = HashMap::new();
    for line in text.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    seen.lock().unwrap().push(headers.clone());

    let total = PAYLOAD_LEN as u64;
    let range = headers
        .get("range")
        .and_then(|value| value.strip_prefix("bytes="))
        .and_then(|spec| spec.split_once('-'))
        .and_then(|(start, end)| Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?)));
    let (start, end, partial) = match range {
        Some((start, end)) => (start.min(total - 1), end.min(total - 1), true),
        None => (0, total - 1, false),
    };
    let header = if partial {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nETag: \"bridge-test\"\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
            end - start + 1
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nETag: \"bridge-test\"\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n"
        )
    };
    if stream.write_all(header.as_bytes()).await.is_err() {
        return;
    }
    let body: Vec<u8> = (start..=end).map(|index| (index % 251) as u8).collect();
    let _ = stream.write_all(&body).await;
    let _ = stream.shutdown().await;
}

async fn start_bridge() -> (u16, mpsc::UnboundedReceiver<BridgeOffer>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = mpsc::unbounded_channel();
    tokio::spawn(bridge::serve_listener(
        listener,
        port,
        "0.1.0".into(),
        sender,
    ));
    (port, receiver)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client")
}

async fn post(port: u16, path: &str, body: Value, origin: &str) -> reqwest::Response {
    client()
        .post(format!("http://127.0.0.1:{port}{path}"))
        .header("origin", origin)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("request")
}

#[tokio::test]
async fn an_extension_hands_a_signed_link_over() {
    let (file, seen) = start_file_server().await;
    let (port, mut offers) = start_bridge().await;

    let ping: Value = client()
        .get(format!("http://127.0.0.1:{port}/ping"))
        .header("origin", "chrome-extension://abcdefghijklmnop")
        .send()
        .await
        .expect("ping")
        .json()
        .await
        .expect("ping json");
    assert_eq!(ping["app"], "majra");
    assert_eq!(ping["api"], 1);

    let url = format!("http://127.0.0.1:{}/file.zip", file.port());
    let response = post(
        port,
        "/add",
        json!({
            "url": url,
            "fileName": "Game Setup.zip",
            "pageUrl": "https://example.com/downloads",
            "minBytes": 1024,
            "browser": {
                "cookies": [{
                    "name": "session",
                    "value": "abc",
                    "domain": "127.0.0.1",
                    "hostOnly": true,
                    "path": "/",
                    "secure": false
                }],
                "referer": "https://example.com/downloads",
                "userAgent": "TestBrowser/9.9",
                "name": "Chrome"
            }
        }),
        "chrome-extension://abcdefghijklmnop",
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("add json");
    let token = body["token"].as_str().expect("token").to_string();

    // The probe reached the file host as the browser, not as Majra.
    let request = seen.lock().unwrap().last().cloned().expect("probe");
    assert_eq!(request.get("user-agent").unwrap(), "TestBrowser/9.9");
    assert_eq!(request.get("cookie").unwrap(), "session=abc");
    assert_eq!(
        request.get("referer").unwrap(),
        "https://example.com/downloads"
    );

    let response = post(
        port,
        "/confirm",
        json!({ "token": token }),
        "chrome-extension://abcdefghijklmnop",
    )
    .await;
    assert_eq!(response.status(), 200);

    let offer = tokio::time::timeout(Duration::from_secs(2), offers.recv())
        .await
        .expect("offer timeout")
        .expect("offer");
    assert_eq!(offer.url, url);
    assert_eq!(offer.file_name.as_deref(), Some("Game Setup.zip"));
    assert_eq!(offer.from.as_deref(), Some("Chrome"));
    assert_eq!(offer.probe.file_name, "Game Setup.zip");
    assert_eq!(offer.probe.total_bytes, Some(PAYLOAD_LEN as u64));
    assert!(offer.probe.supports_ranges);
    assert_eq!(
        offer
            .browser
            .expect("browser context")
            .user_agent
            .as_deref(),
        Some("TestBrowser/9.9")
    );
}

#[tokio::test]
async fn the_bridge_refuses_what_is_not_the_extension() {
    let (file, _seen) = start_file_server().await;
    let (port, mut offers) = start_bridge().await;
    let url = format!("http://127.0.0.1:{}/file.zip", file.port());
    let add = json!({ "url": url });

    // A web page's origin is refused.
    let response = post(port, "/add", add.clone(), "https://evil.example").await;
    assert_eq!(response.status(), 403);
    let response = client()
        .get(format!("http://127.0.0.1:{port}/ping"))
        .header("origin", "https://evil.example")
        .send()
        .await
        .expect("ping");
    assert_eq!(response.status(), 403);

    // A Host that isn't this loopback port is refused (DNS rebinding).
    let response = client()
        .post(format!("http://127.0.0.1:{port}/add"))
        .header("host", "evil.example")
        .header("origin", "chrome-extension://abcdefghijklmnop")
        .header("content-type", "application/json")
        .json(&add)
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 403);

    // A form POST can't reach it: JSON needs a preflight only an extension passes.
    let response = client()
        .post(format!("http://127.0.0.1:{port}/add"))
        .header("origin", "chrome-extension://abcdefghijklmnop")
        .header("content-type", "text/plain")
        .body(add.to_string())
        .send()
        .await
        .expect("request");
    assert_eq!(response.status(), 415);

    // A file below the browser's minimum size stays in the browser.
    let response = post(
        port,
        "/add",
        json!({ "url": url, "minBytes": PAYLOAD_LEN as u64 + 1 }),
        "chrome-extension://abcdefghijklmnop",
    )
    .await;
    assert_eq!(response.status(), 422);
    let body: Value = response.json().await.expect("reason json");
    assert!(body["reason"].as_str().unwrap().contains("smaller"));

    // The new core has no torrent engine, so a magnet link stays in the browser.
    let response = post(
        port,
        "/add",
        json!({ "url": "magnet:?xt=urn:btih:abcdef" }),
        "chrome-extension://abcdefghijklmnop",
    )
    .await;
    assert_eq!(response.status(), 422);

    // A token that was never issued is gone.
    let response = post(
        port,
        "/confirm",
        json!({ "token": "never-issued" }),
        "chrome-extension://abcdefghijklmnop",
    )
    .await;
    assert_eq!(response.status(), 410);
    assert!(offers.try_recv().is_err());
}
