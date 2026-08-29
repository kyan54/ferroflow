//! Fetches a subscription URL's raw body over HTTP(S). Kept separate from
//! `parse` so the pure parsing logic stays testable without network access
//! -- this module can't reasonably be unit-tested itself (that would mean
//! mocking an HTTP server), so it's kept as small and boring as possible:
//! build a client, GET, check status, read the body.
//!
//! Uses `reqwest` with the `rustls-tls` backend (not `native-tls`/OpenSSL) so
//! Linux CI doesn't need system OpenSSL dev packages installed.

use std::time::Duration;

use thiserror::Error;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Failure modes for `fetch_subscription`, each carrying enough context to
/// be surfaced directly to a user as a toast/error message.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("could not build an HTTP client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("request to '{url}' failed: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("subscription URL '{url}' returned HTTP {status}")]
    Status { url: String, status: reqwest::StatusCode },
    #[error("failed to read response body from '{url}': {source}")]
    Body { url: String, source: reqwest::Error },
}

/// Fetches `url` and returns its raw response body as text. Does not decode
/// or parse the body at all -- see `crate::parse` for that, once you have
/// the raw text in hand.
pub async fn fetch_subscription(url: &str) -> Result<String, FetchError> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(FetchError::ClientBuild)?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|source| FetchError::Request { url: url.to_string(), source })?;

    let response = response.error_for_status().map_err(|source| {
        // `error_for_status` consumes `self` but preserves the status code on
        // the resulting error, which is what we actually want to report --
        // reconstruct the status separately since `source.status()` is the
        // cleanest way to get it back out here.
        let status = source.status().unwrap_or(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        FetchError::Status { url: url.to_string(), status }
    })?;

    response.text().await.map_err(|source| FetchError::Body { url: url.to_string(), source })
}
