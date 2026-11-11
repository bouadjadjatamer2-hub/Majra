//! One process-wide proxy choice, shared by the probe and every transfer client.
//!
//! The setting is deliberately small: a mode plus one manual URL. A per-download proxy
//! would multiply the number of clients and make the interface bonding harder to reason
//! about, while the usual reason to set a proxy (a school, an office, a metered link) is
//! the whole app rather than one file.

use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    /// Let reqwest use the environment and the operating system's own settings.
    System,
    /// Never use a proxy, even when the environment defines one.
    None,
    /// Use the URL below for every request.
    Manual,
}

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    /// `http://host:port`, `https://host:port`, `socks5://host:port` or `socks5h://host:port`.
    pub url: String,
    pub username: String,
    pub password: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: ProxyMode::System,
            url: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }
}

static PROXY: RwLock<ProxyConfig> = RwLock::new(ProxyConfig {
    mode: ProxyMode::System,
    url: String::new(),
    username: String::new(),
    password: String::new(),
});

pub fn set(config: ProxyConfig) {
    if let Ok(mut current) = PROXY.write() {
        *current = config;
    }
}

pub fn get() -> ProxyConfig {
    PROXY
        .read()
        .map(|config| config.clone())
        .unwrap_or_default()
}

/// Apply one explicit choice to a client that is about to be built. A malformed manual URL
/// falls back to the system route: a broken preference must not make every download fail.
pub fn apply_with(
    builder: reqwest::ClientBuilder,
    config: &ProxyConfig,
) -> reqwest::ClientBuilder {
    match config.mode {
        ProxyMode::System => builder,
        ProxyMode::None => builder.no_proxy(),
        ProxyMode::Manual => {
            let url = config.url.trim();
            if url.is_empty() {
                return builder;
            }
            match reqwest::Proxy::all(url) {
                Ok(mut proxy) => {
                    let username = config.username.trim();
                    if !username.is_empty() {
                        proxy = proxy.basic_auth(username, &config.password);
                    }
                    builder.proxy(proxy)
                }
                Err(error) => {
                    eprintln!("majra: ignoring invalid proxy URL '{url}': {error}");
                    builder
                }
            }
        }
    }
}

/// Apply the process-wide choice. Kept separate from `apply_with` so tests can check one
/// configuration without touching the global that other tests run against.
pub fn apply(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    apply_with(builder, &get())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(config: &ProxyConfig) -> reqwest::Client {
        apply_with(reqwest::Client::builder(), config)
            .build()
            .expect("client")
    }

    #[test]
    fn a_malformed_manual_url_still_builds_a_client() {
        let _ = client(&ProxyConfig {
            mode: ProxyMode::Manual,
            url: "not a proxy".into(),
            username: String::new(),
            password: String::new(),
        });
    }

    #[test]
    fn manual_socks_and_http_urls_are_accepted() {
        for url in ["http://127.0.0.1:8080", "socks5://127.0.0.1:1080"] {
            let _ = client(&ProxyConfig {
                mode: ProxyMode::Manual,
                url: url.into(),
                username: "user".into(),
                password: "secret".into(),
            });
        }
    }
}
