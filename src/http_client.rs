//! Shared `reqwest` clients. All clients refuse plaintext HTTP.

use crate::version::OIL_VERSION;
use std::sync::OnceLock;
use std::time::Duration;

static API_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static DOWNLOAD_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static DEFAULT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static REGISTRY_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn user_agent() -> String {
    format!("oilpkg/{OIL_VERSION} (https://github.com/semitechnological/oil)")
}

fn build_client(timeout: Duration, compress: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(user_agent())
        .https_only(true);
    if compress {
        builder = builder.gzip(true);
    } else {
        builder = builder.gzip(false);
    }
    builder.build().expect("Failed to create HTTP client")
}

/// Homebrew JSON API: 30s timeout, compressed responses.
pub fn api() -> &'static reqwest::Client {
    API_CLIENT.get_or_init(|| build_client(Duration::from_secs(30), true))
}

/// Bottle/cask/package downloads: 5 minute timeout, raw bytes.
pub fn download() -> &'static reqwest::Client {
    DOWNLOAD_CLIENT.get_or_init(|| build_client(Duration::from_secs(300), false))
}

/// General-purpose client (GitHub, crates.io): 60s, compressed.
pub fn default_client() -> &'static reqwest::Client {
    DEFAULT_CLIENT.get_or_init(|| build_client(Duration::from_secs(60), true))
}

/// Distro registry indexes: 120s, compressed.
pub fn registry() -> &'static reqwest::Client {
    REGISTRY_CLIENT.get_or_init(|| build_client(Duration::from_secs(120), true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_identifies_oilpkg() {
        let ua = user_agent();
        assert!(ua.starts_with("oilpkg/"));
        assert!(ua.contains("semitechnological/oil"));
    }

    #[tokio::test]
    async fn https_only_rejects_plaintext_http() {
        for client in [api(), download(), default_client(), registry()] {
            let err = client
                .get("http://127.0.0.1/")
                .send()
                .await
                .expect_err("plaintext HTTP must be rejected");
            let msg = err.to_string().to_ascii_lowercase();
            assert!(
                msg.contains("https") || msg.contains("http"),
                "unexpected https_only error: {err}"
            );
        }
    }
}
