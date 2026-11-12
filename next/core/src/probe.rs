use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

use crate::browser::{base_headers, BrowserContext};
use crate::error::{MajraError, Result};

const PROBE_BYTES: u64 = 16 * 1024;
const MAX_REDIRECTS: u8 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub requested_url: String,
    pub final_url: String,
    pub supports_ranges: bool,
    pub total_bytes: Option<u64>,
    pub file_name: String,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub fn client_builder() -> reqwest::ClientBuilder {
    crate::proxy::apply(
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(45))
            // Keep a pool per adapter: a retried block reuses its TLS connection instead
            // of paying a fresh handshake, and a dead peer is noticed by keep-alive
            // probes instead of holding a worker forever.
            .tcp_keepalive(Duration::from_secs(30))
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32),
    )
}

fn header(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Parse `bytes 0-16383/123456` (or `bytes * /123456`) into the total size.
fn content_range_total(value: &str) -> Option<u64> {
    let (_, total) = value.rsplit_once('/')?;
    let total = total.trim();
    if total == "*" {
        return None;
    }
    total.parse().ok()
}

fn filename_from_disposition(value: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for part in value.split(';') {
        let part = part.trim();
        let Some((key, raw)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.eq_ignore_ascii_case("filename*") {
            // RFC 5987: charset'language'percent-encoded-name
            let raw = raw.trim().trim_matches('"');
            let encoded = raw.splitn(3, '\'').nth(2).unwrap_or(raw);
            let decoded = percent_encoding::percent_decode_str(encoded)
                .decode_utf8_lossy()
                .to_string();
            let name = sanitize_file_name(&decoded);
            if !name.is_empty() {
                extended = Some(name);
            }
        } else if key.eq_ignore_ascii_case("filename") {
            let raw = raw.trim().trim_matches('"');
            let decoded = percent_encoding::percent_decode_str(raw)
                .decode_utf8_lossy()
                .to_string();
            let name = sanitize_file_name(&decoded);
            if !name.is_empty() {
                plain = Some(name);
            }
        }
    }
    // The extended form is the standards-compliant one, so it wins when both appear.
    extended.or(plain)
}

fn filename_from_url(url: &Url) -> Option<String> {
    let last = url.path_segments()?.next_back()?;
    let decoded = percent_encoding::percent_decode_str(last)
        .decode_utf8_lossy()
        .to_string();
    let name = sanitize_file_name(&decoded);
    (!name.is_empty()).then_some(name)
}

/// Keep one plain file name: no directories, no control characters, no Windows device names.
pub fn sanitize_file_name(raw: &str) -> String {
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_matches('.')
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .collect::<String>();
    let base = base.trim();
    if base.is_empty() {
        return String::new();
    }
    let stem = base.split('.').next().unwrap_or(base).to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        format!("_{base}")
    } else {
        base.to_string()
    }
}

/// One small ranged GET is enough to learn the size, whether ranges work, and the validators.
/// HEAD is deliberately not used: many file hosts reject it or omit the fields we need.
pub async fn probe_url(url: &str) -> Result<ProbeResult> {
    probe_url_with(url, None).await
}

/// The same probe, sent as the browser that handed the link over. Redirects are followed
/// by hand so every hop gets only the cookies that belong to it, never the previous site's.
pub async fn probe_url_with(url: &str, browser: Option<&BrowserContext>) -> Result<ProbeResult> {
    let parsed = Url::parse(url)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(MajraError::Message(
            "only http and https links are supported in this build".into(),
        ));
    }

    let client = client_builder()
        .redirect(reqwest::redirect::Policy::none())
        // A probe must never outlive the user's patience. Thirty seconds is enough for a
        // slow host to answer, and short enough that the dialog reports the failure
        // instead of looking frozen.
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut current = parsed.clone();
    let mut redirects = 0u8;
    let response = loop {
        let mut request = client.get(current.clone()).header(
            reqwest::header::RANGE,
            format!("bytes=0-{}", PROBE_BYTES - 1),
        );
        for (name, value) in base_headers(browser, &current) {
            request = request.header(name, value);
        }
        let response = request.send().await?;
        if response.status().is_redirection() {
            let Some(location) = header(&response, "location") else {
                return Err(MajraError::Http(response.status().as_u16()));
            };
            redirects += 1;
            if redirects > MAX_REDIRECTS {
                return Err(MajraError::Message("too many redirects".into()));
            }
            let next = current.join(&location)?;
            if !matches!(next.scheme(), "http" | "https") {
                return Err(MajraError::Message(
                    "the server redirected to a non-HTTP address".into(),
                ));
            }
            current = next;
            continue;
        }
        break response;
    };

    let status = response.status();
    let final_url = current.to_string();
    let content_type = header(&response, "content-type");
    let etag = header(&response, "etag");
    let last_modified = header(&response, "last-modified");
    let disposition = header(&response, "content-disposition");
    let content_range = header(&response, "content-range");

    let (supports_ranges, total_bytes) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        let total = content_range
            .as_deref()
            .and_then(content_range_total)
            .or_else(|| response.content_length());
        (true, total)
    } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let total = content_range.as_deref().and_then(content_range_total);
        (true, total)
    } else if status.is_success() {
        (false, response.content_length())
    } else {
        return Err(MajraError::Http(status.as_u16()));
    };

    let file_name = disposition
        .as_deref()
        .and_then(filename_from_disposition)
        .or_else(|| {
            Url::parse(&final_url)
                .ok()
                .and_then(|url| filename_from_url(&url))
        })
        .or_else(|| filename_from_url(&parsed))
        .unwrap_or_else(|| "download.bin".to_string());

    // Headers are a promise; the first bytes are proof. Reading one probe-sized piece
    // here turns a host that accepts the request and then goes silent into an immediate,
    // understandable error instead of a row that sits at zero for minutes.
    if status == reqwest::StatusCode::PARTIAL_CONTENT || status.is_success() {
        use futures_util::StreamExt;
        let read = async move {
            let mut stream = response.bytes_stream();
            let mut received = 0usize;
            while received < PROBE_BYTES as usize {
                match stream.next().await {
                    Some(Ok(chunk)) => received += chunk.len(),
                    _ => break,
                }
            }
        };
        if tokio::time::timeout(Duration::from_secs(20), read)
            .await
            .is_err()
        {
            return Err(MajraError::Message(
                "the server accepted the request but sent no data".into(),
            ));
        }
    }

    Ok(ProbeResult {
        requested_url: url.to_string(),
        final_url,
        supports_ranges,
        total_bytes,
        file_name,
        content_type,
        etag,
        last_modified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range() {
        assert_eq!(content_range_total("bytes 0-16383/9000"), Some(9000));
        assert_eq!(content_range_total("bytes 0-16383/*"), None);
        assert_eq!(content_range_total("garbage"), None);
    }

    #[test]
    fn strips_directories_and_reserved_names() {
        assert_eq!(sanitize_file_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_file_name(r"C:\temp\file.zip"), "file.zip");
        assert_eq!(sanitize_file_name("CON"), "_CON");
        assert_eq!(sanitize_file_name(".."), "");
    }

    #[test]
    fn parses_disposition() {
        assert_eq!(
            filename_from_disposition("attachment; filename=\"report final.pdf\""),
            Some("report final.pdf".into())
        );
        assert_eq!(
            filename_from_disposition(
                "attachment; filename=\"old.pdf\"; filename*=UTF-8''%D9%85%D9%84%D9%81.pdf"
            ),
            Some("ملف.pdf".into())
        );
    }
}
