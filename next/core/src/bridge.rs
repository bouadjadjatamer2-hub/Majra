//! The localhost bridge the browser extension talks to. It speaks the same protocol as
//! the old Majra app: `GET /ping`, `POST /add`, `POST /confirm` on the first free port.
//!
//! Nothing here starts a download on its own. `/add` checks the link with the browser's
//! own sign-in, `/confirm` hands the offer to the window, and the user still presses
//! Start. An unconfirmed offer expires, so a download never ends up in both places.

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{
    HeaderMap, HeaderName, HeaderValue, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE, VARY,
};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use crate::browser::BrowserContext;
use crate::probe::{probe_url_with, ProbeResult};

/// The extension tries every port, so a port another app holds needs no setting.
pub const BRIDGE_PORTS: [u16; 3] = [19288, 19289, 19290];
/// Raised when an older extension or Majra couldn't follow a change to /ping or /add.
pub const BRIDGE_API: u32 = 1;

const MAX_BODY_BYTES: usize = 64 * 1024;
/// The extension gives up a little after this, so nothing may be added once it has.
const PROBE_DEADLINE: Duration = Duration::from_secs(8);
/// An offer the extension never confirmed is dropped; the browser kept its copy.
const HANDOFF: Duration = Duration::from_secs(15);

/// One link the extension handed over. `browser` stays in the backend: the webview gets
/// every other field, never the cookies.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeOffer {
    pub id: String,
    pub url: String,
    pub file_name: Option<String>,
    pub page_url: Option<String>,
    pub from: Option<String>,
    pub probe: ProbeResult,
    #[serde(skip)]
    pub browser: Option<BrowserContext>,
}

struct Pending {
    offer: BridgeOffer,
    at: Instant,
}

#[derive(Default)]
struct Handoffs {
    entries: Mutex<HashMap<String, Pending>>,
}

impl Handoffs {
    fn insert(&self, offer: BridgeOffer) {
        let mut entries = self.entries.lock().unwrap();
        entries.retain(|_, pending| pending.at.elapsed() < HANDOFF);
        entries.insert(
            offer.id.clone(),
            Pending {
                offer,
                at: Instant::now(),
            },
        );
    }

    fn take(&self, token: &str) -> Option<BridgeOffer> {
        let mut entries = self.entries.lock().unwrap();
        let pending = entries.remove(token)?;
        (pending.at.elapsed() < HANDOFF).then_some(pending.offer)
    }
}

struct BridgeState {
    port: u16,
    version: String,
    handoffs: Handoffs,
    sender: UnboundedSender<BridgeOffer>,
}

/// Listen on the first free bridge port. Once one is taken, the extension can reach us
/// until the process exits. `active_port` is published for the settings screen; zero
/// means no bridge could bind.
pub async fn serve(
    version: String,
    sender: UnboundedSender<BridgeOffer>,
    active_port: Arc<AtomicU16>,
) {
    for port in BRIDGE_PORTS {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                active_port.store(port, Ordering::Relaxed);
                serve_listener(listener, port, version, sender).await;
                active_port.store(0, Ordering::Relaxed);
                return;
            }
            Err(_) => continue,
        }
    }
    active_port.store(0, Ordering::Relaxed);
    eprintln!("majra: browser bridge: ports 19288, 19289, 19290 are all taken");
}

/// The accept loop, split out so tests can bind an ephemeral port.
pub async fn serve_listener(
    listener: TcpListener,
    port: u16,
    version: String,
    sender: UnboundedSender<BridgeOffer>,
) {
    let state = Arc::new(BridgeState {
        port,
        version,
        handoffs: Handoffs::default(),
        sender,
    });
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let state = state.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |request| {
                let state = state.clone();
                async move { Ok::<_, Infallible>(handle(request, state).await) }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await;
        });
    }
}

type Cors = Option<Vec<(HeaderName, String)>>;

async fn handle(request: Request<Incoming>, state: Arc<BridgeState>) -> Response<Full<Bytes>> {
    let origin = header_str(request.headers(), "origin").map(str::to_owned);
    let host = header_str(request.headers(), "host").unwrap_or_default();
    let local =
        host == format!("127.0.0.1:{}", state.port) || host == format!("localhost:{}", state.port);
    // Host blocks DNS rebinding; a missing Origin is allowed because a page's POST always
    // sends one, and Firefox may leave it off an extension's.
    if !local
        || origin
            .as_deref()
            .is_some_and(|value| !is_extension_origin(value))
    {
        return empty(StatusCode::FORBIDDEN, None);
    }
    let cors = origin.as_deref().map(cors_headers);
    if request.method() == Method::OPTIONS {
        return empty(StatusCode::NO_CONTENT, cors);
    }
    // JSON only: a page can't send JSON here without a CORS preflight, which only an
    // extension's origin passes.
    if request.method() == Method::POST && !is_json(request.headers()) {
        return empty(StatusCode::UNSUPPORTED_MEDIA_TYPE, cors);
    }

    let method = request.method().clone();
    let path = request.uri().path().to_string();
    if method == Method::GET && path == "/ping" {
        return json_response(
            StatusCode::OK,
            json!({
                "app": "majra",
                "version": state.version,
                "api": BRIDGE_API
            }),
            cors,
        );
    }
    if method == Method::POST && path == "/add" {
        let Some(body) = read_json(request).await else {
            return empty(StatusCode::BAD_REQUEST, cors);
        };
        let Some(add) = AddRequest::parse(body) else {
            return empty(StatusCode::BAD_REQUEST, cors);
        };
        return add_response(add, state, cors).await;
    }
    if method == Method::POST && path == "/confirm" {
        let Some(body) = read_json(request).await else {
            return empty(StatusCode::BAD_REQUEST, cors);
        };
        let Some(token) = body.get("token").and_then(Value::as_str) else {
            return empty(StatusCode::BAD_REQUEST, cors);
        };
        let Some(offer) = state.handoffs.take(token) else {
            return empty(StatusCode::GONE, cors);
        };
        if state.sender.send(offer).is_err() {
            return empty(StatusCode::SERVICE_UNAVAILABLE, cors);
        }
        return json_response(StatusCode::OK, json!({}), cors);
    }
    empty(StatusCode::NOT_FOUND, cors)
}

async fn add_response(
    add: AddRequest,
    state: Arc<BridgeState>,
    cors: Cors,
) -> Response<Full<Bytes>> {
    // The new core has no torrent engine yet, so the browser keeps a magnet link.
    if add.url.to_ascii_lowercase().starts_with("magnet:") {
        return keep("magnet links are not supported yet", cors);
    }

    let checked = tokio::time::timeout(
        PROBE_DEADLINE,
        probe_url_with(&add.url, add.browser.as_ref()),
    )
    .await;
    let mut probe = match checked {
        Err(_) => return keep("the server took too long to answer", cors),
        Ok(Err(error)) => return keep(&error.to_string(), cors),
        Ok(Ok(probe)) => probe,
    };
    if let (Some(minimum), Some(total)) = (add.min_bytes, probe.total_bytes) {
        if total < minimum {
            return keep("smaller than the minimum size", cors);
        }
    }
    if let Some(name) = add.file_name.clone() {
        probe.file_name = name;
    }

    let id = Uuid::new_v4().to_string();
    let offer = BridgeOffer {
        id: id.clone(),
        url: add.url,
        file_name: add.file_name,
        page_url: add.page_url,
        from: add
            .browser
            .as_ref()
            .and_then(|browser| browser.name.clone()),
        probe,
        browser: add.browser,
    };
    state.handoffs.insert(offer);
    json_response(StatusCode::OK, json!({ "token": id }), cors)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRequest {
    url: String,
    #[serde(default)]
    browser: Option<BrowserContext>,
    #[serde(default)]
    page_url: Option<String>,
    #[serde(default)]
    min_bytes: Option<u64>,
    #[serde(default)]
    file_name: Option<String>,
}

impl AddRequest {
    fn parse(value: Value) -> Option<Self> {
        let request: Self = serde_json::from_value(value).ok()?;
        let lower = request.url.to_ascii_lowercase();
        let http = lower.starts_with("http://") || lower.starts_with("https://");
        let magnet = lower.starts_with("magnet:?");
        if request.url.len() > 8192 || !(http || magnet) {
            return None;
        }
        if let Some(browser) = &request.browser {
            browser.validated()?;
        }
        if let Some(page) = &request.page_url {
            let page = page.to_ascii_lowercase();
            if !(page.starts_with("http://") || page.starts_with("https://")) {
                return None;
            }
        }
        if let Some(name) = &request.file_name {
            if !valid_file_name(name) {
                return None;
            }
        }
        Some(request)
    }
}

fn valid_file_name(name: &str) -> bool {
    (1..=255).contains(&name.chars().count())
        && !name
            .chars()
            .any(|ch| ch.is_control() || ch == '/' || ch == '\\')
}

fn is_extension_origin(origin: &str) -> bool {
    let lower = origin.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("chrome-extension://")
        .or_else(|| lower.strip_prefix("moz-extension://"));
    let Some(rest) = rest else {
        return false;
    };
    !rest.is_empty()
        && rest
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

fn is_json(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let value = value.trim().to_ascii_lowercase();
    let Some(rest) = value.strip_prefix("application/json") else {
        return false;
    };
    rest.is_empty() || !rest.as_bytes()[0].is_ascii_alphanumeric()
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn cors_headers(origin: &str) -> Vec<(HeaderName, String)> {
    vec![
        (ACCESS_CONTROL_ALLOW_ORIGIN, origin.to_string()),
        (ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type".to_string()),
        (ACCESS_CONTROL_ALLOW_METHODS, "GET, POST".to_string()),
        (VARY, "Origin".to_string()),
    ]
}

fn with_cors(mut response: Response<Full<Bytes>>, cors: &Cors) -> Response<Full<Bytes>> {
    if let Some(headers) = cors {
        for (name, value) in headers {
            if let Ok(value) = HeaderValue::from_str(value) {
                response.headers_mut().insert(name.clone(), value);
            }
        }
    }
    response
}

fn empty(status: StatusCode, cors: Cors) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = status;
    with_cors(response, &cors)
}

fn json_response(status: StatusCode, body: Value, cors: Cors) -> Response<Full<Bytes>> {
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = Response::new(Full::new(Bytes::from(bytes)));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    with_cors(response, &cors)
}

fn keep(reason: &str, cors: Cors) -> Response<Full<Bytes>> {
    json_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({ "reason": reason }),
        cors,
    )
}

async fn read_json(request: Request<Incoming>) -> Option<Value> {
    let body = Limited::new(request.into_body(), MAX_BODY_BYTES);
    let bytes = body.collect().await.ok()?.to_bytes();
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_extension_origins_only() {
        assert!(is_extension_origin("chrome-extension://abcdefghijklmnop"));
        assert!(is_extension_origin("moz-extension://1234-abcd"));
        assert!(!is_extension_origin("https://example.com"));
        assert!(!is_extension_origin("chrome-extension://"));
        assert!(!is_extension_origin("chrome-extension://id/extra"));
        assert!(!is_extension_origin("file://extension"));
    }

    #[test]
    fn file_names_cannot_be_paths_or_headers() {
        assert!(valid_file_name("report final.pdf"));
        assert!(!valid_file_name(""));
        assert!(!valid_file_name("../secret"));
        assert!(!valid_file_name("a\\b"));
        assert!(!valid_file_name("a\r\nb"));
        assert!(!valid_file_name(&"a".repeat(256)));
    }

    #[test]
    fn json_needs_the_exact_media_type() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(is_json(&headers));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/jsonx"));
        assert!(!is_json(&headers));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        assert!(!is_json(&headers));
    }
}
