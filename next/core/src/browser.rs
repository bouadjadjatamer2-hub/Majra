use serde::{Deserialize, Serialize};
use url::Url;

/// Some hosts (and the CDNs and bot filters in front of them) refuse a client that does
/// not name a browser. A download manager has to look like what the link came from.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

const MAX_COOKIES: usize = 500;
const MAX_TEXT: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub host_only: bool,
    pub path: String,
    pub secure: bool,
}

/// What the extension saw when the browser started the download: the exact User-Agent,
/// the page it came from, and the cookies that page's browser would send. It is kept in
/// the backend and never sent to the webview.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserContext {
    pub cookies: Vec<BrowserCookie>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn is_text(value: &str) -> bool {
    value.len() <= MAX_TEXT && !value.chars().any(char::is_control)
}

fn is_cookie_text(value: &str) -> bool {
    is_text(value) && !value.contains(';')
}

fn is_http_url(value: &str) -> bool {
    (value.starts_with("http://") || value.starts_with("https://")) && is_text(value)
}

impl BrowserContext {
    /// It comes from outside Majra (the extension, or a catalog file), so only known and
    /// safe fields are accepted. A line break in a value would start a header of its own.
    pub fn validated(&self) -> Option<Self> {
        if self.cookies.len() > MAX_COOKIES {
            return None;
        }
        if let Some(referer) = &self.referer {
            if !is_http_url(referer) {
                return None;
            }
        }
        if let Some(user_agent) = &self.user_agent {
            if !is_text(user_agent) {
                return None;
            }
        }
        if let Some(name) = &self.name {
            if !is_text(name) || name.chars().count() > 40 {
                return None;
            }
        }
        for cookie in &self.cookies {
            if !is_cookie_text(&cookie.name) || !is_cookie_text(&cookie.value) {
                return None;
            }
            if !is_text(&cookie.domain) || !is_text(&cookie.path) {
                return None;
            }
            if !cookie.path.starts_with('/') {
                return None;
            }
        }
        Some(self.clone())
    }

    pub fn user_agent(&self) -> &str {
        self.user_agent
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_USER_AGENT)
    }

    pub fn referer(&self) -> Option<&str> {
        self.referer.as_deref().filter(|value| !value.is_empty())
    }

    /// RFC 6265 §5.4, the way the old app built it: only the cookies that belong to this
    /// exact host, path and scheme. Called again for every redirect hop, so a site's
    /// session cookie never reaches the CDN it redirects to.
    pub fn cookie_header(&self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        let path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let secure = url.scheme() == "https";

        let mut matching = self
            .cookies
            .iter()
            .filter(|cookie| {
                let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
                if domain.is_empty() {
                    return false;
                }
                let domain_matches = if cookie.host_only {
                    host == domain
                } else {
                    host == domain || host.ends_with(&format!(".{domain}"))
                };
                if !domain_matches {
                    return false;
                }
                let cookie_path = if cookie.path.is_empty() {
                    "/"
                } else {
                    cookie.path.as_str()
                };
                let path_matches = path == cookie_path
                    || (path.starts_with(cookie_path)
                        && (cookie_path.ends_with('/')
                            || path[cookie_path.len()..].starts_with('/')));
                path_matches && (!cookie.secure || secure)
            })
            .collect::<Vec<_>>();

        if matching.is_empty() {
            return None;
        }
        // Longer paths first, as a browser sends them.
        matching.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.len()));
        Some(
            matching
                .into_iter()
                .map(|cookie| {
                    if cookie.name.is_empty() {
                        cookie.value.clone()
                    } else {
                        format!("{}={}", cookie.name, cookie.value)
                    }
                })
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

/// The headers every request for `url` carries. The cookie list is recomputed for each
/// redirect hop; the User-Agent and Referer are the browser's own.
pub fn base_headers(browser: Option<&BrowserContext>, url: &Url) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        (
            "User-Agent",
            browser
                .map(BrowserContext::user_agent)
                .unwrap_or(DEFAULT_USER_AGENT)
                .to_string(),
        ),
        ("Accept", "*/*".to_string()),
        ("Accept-Language", "en-US,en;q=0.9".to_string()),
    ];
    if let Some(referer) = browser.and_then(BrowserContext::referer) {
        headers.push(("Referer", referer.to_string()));
    }
    if let Some(cookie) = browser.and_then(|browser| browser.cookie_header(url)) {
        headers.push(("Cookie", cookie));
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, value: &str, domain: &str, host_only: bool, path: &str) -> BrowserCookie {
        BrowserCookie {
            name: name.into(),
            value: value.into(),
            domain: domain.into(),
            host_only,
            path: path.into(),
            secure: false,
        }
    }

    fn context(cookies: Vec<BrowserCookie>) -> BrowserContext {
        BrowserContext {
            cookies,
            referer: Some("https://example.com/downloads".into()),
            user_agent: Some("TestAgent/1.0".into()),
            name: Some("Chrome".into()),
        }
    }

    #[test]
    fn sends_only_the_cookies_that_belong_to_the_url() {
        let mut session = cookie("session", "abc", ".example.com", false, "/");
        session.secure = true;
        let context = context(vec![
            session,
            cookie("other", "nope", ".elsewhere.com", false, "/"),
            cookie("hostonly", "yes", "cdn.example.com", true, "/files"),
            cookie("wrongpath", "no", ".example.com", false, "/private"),
        ]);

        let header = context
            .cookie_header(&Url::parse("https://cdn.example.com/files/a.zip").unwrap())
            .expect("cookie header");
        assert_eq!(header, "hostonly=yes; session=abc");

        // A secure cookie never leaves HTTPS.
        let header = context
            .cookie_header(&Url::parse("http://cdn.example.com/files/a.zip").unwrap())
            .expect("cookie header");
        assert_eq!(header, "hostonly=yes");
    }

    #[test]
    fn a_host_only_cookie_never_follows_a_subdomain() {
        let context = context(vec![cookie("a", "1", "example.com", true, "/")]);
        assert!(context
            .cookie_header(&Url::parse("https://sub.example.com/").unwrap())
            .is_none());
        assert!(context
            .cookie_header(&Url::parse("https://example.com/").unwrap())
            .is_some());
    }

    #[test]
    fn longer_paths_come_first() {
        let context = context(vec![
            cookie("root", "1", "example.com", true, "/"),
            cookie("deep", "2", "example.com", true, "/files"),
        ]);
        let header = context
            .cookie_header(&Url::parse("https://example.com/files/a.zip").unwrap())
            .expect("cookie header");
        assert_eq!(header, "deep=2; root=1");
    }

    #[test]
    fn headers_carry_the_browser_identity() {
        let context = context(vec![cookie("session", "abc", "example.com", true, "/")]);
        let headers = base_headers(
            Some(&context),
            &Url::parse("https://example.com/a.zip").unwrap(),
        );
        assert!(headers.contains(&("User-Agent", "TestAgent/1.0".to_string())));
        assert!(headers.contains(&("Referer", "https://example.com/downloads".to_string())));
        assert!(headers.contains(&("Cookie", "session=abc".to_string())));

        let plain = base_headers(None, &Url::parse("https://example.com/a.zip").unwrap());
        assert!(plain.contains(&("User-Agent", DEFAULT_USER_AGENT.to_string())));
        assert!(!plain.iter().any(|(name, _)| *name == "Cookie"));
    }

    #[test]
    fn rejects_a_header_break_in_any_field() {
        let mut broken = context(vec![cookie("a", "1", "example.com", true, "/")]);
        broken.referer = Some("https://example.com/\r\nX-Evil: 1".into());
        assert!(broken.validated().is_none());

        let mut broken = context(vec![cookie("a", "1", "example.com", true, "/")]);
        broken.cookies[0].value = "1\r\nX-Evil: 1".into();
        assert!(broken.validated().is_none());

        let mut broken = context(vec![cookie("a", "1", "example.com", true, "/")]);
        broken.cookies[0].name = "a; b".into();
        assert!(broken.validated().is_none());
    }
}
