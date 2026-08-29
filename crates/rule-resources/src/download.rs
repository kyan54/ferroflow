//! Downloads a single rule-set `.srs` file over HTTP(S) to a local path.
//! Mirrors `subscription::fetch`'s shape closely (same `reqwest` +
//! `rustls-tls` dependency choice, same error-context style) but this one
//! writes the body to disk (atomically) and hashes it, rather than just
//! returning text -- `.srs` files are small binary blobs, not something to
//! parse as a string.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// These files are tiny (typically well under a megabyte), but reaching
/// `raw.githubusercontent.com` can be slow or blocked outright depending on
/// network conditions (the whole reason the GitHub-acceleration-prefix
/// feature exists) -- 15s gives a slow path a real chance without hanging
/// the UI indefinitely.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

/// Failure modes for `download`, each carrying the failed URL so a user can
/// see exactly what didn't resolve -- useful for diagnosing
/// GitHub-blocked-in-China scenarios, which is the whole reason the
/// GitHub-acceleration-prefix feature exists in the first place.
#[derive(Debug, Error)]
pub enum RuleResourceError {
    #[error("could not build an HTTP client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("request to '{url}' failed: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("rule-set download '{url}' returned HTTP {status}")]
    Status { url: String, status: reqwest::StatusCode },
    #[error("failed to read response body from '{url}': {source}")]
    Body { url: String, source: reqwest::Error },
    #[error("failed to write downloaded rule-set to '{path}': {source}")]
    Io { path: String, source: std::io::Error },
}

/// Result of a successful `download` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedResource {
    pub size_bytes: u64,
    pub sha256: String,
    /// RFC3339 timestamp string of when this download completed.
    pub downloaded_at: String,
}

/// Fetches `url` and writes the response body to `dest_path`, atomically
/// (writes to `<dest_path>.tmp` first, then renames over `dest_path` --
/// avoids ever leaving a half-written file at the real path if the process
/// is interrupted mid-download or mid-write). Creates `dest_path`'s parent
/// directory if it doesn't exist yet.
///
/// Returns the downloaded size, a hex-encoded SHA-256 of the content, and
/// the completion timestamp -- `UserConfig::rule_resources` stores these
/// verbatim (see `shared_types::RuleResourceInfo`).
pub async fn download(url: &str, dest_path: &Path) -> Result<DownloadedResource, RuleResourceError> {
    let client =
        reqwest::Client::builder().timeout(DOWNLOAD_TIMEOUT).build().map_err(RuleResourceError::ClientBuild)?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|source| RuleResourceError::Request { url: url.to_string(), source })?;

    let response = response.error_for_status().map_err(|source| {
        let status = source.status().unwrap_or(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        RuleResourceError::Status { url: url.to_string(), status }
    })?;

    let bytes =
        response.bytes().await.map_err(|source| RuleResourceError::Body { url: url.to_string(), source })?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256 = hex::encode(hasher.finalize());
    let size_bytes = bytes.len() as u64;

    write_atomic(dest_path, &bytes)
        .map_err(|source| RuleResourceError::Io { path: dest_path.display().to_string(), source })?;

    Ok(DownloadedResource { size_bytes, sha256, downloaded_at: crate::time::now_rfc3339() })
}

fn write_atomic(dest_path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut tmp_os = dest_path.as_os_str().to_os_string();
    tmp_os.push(".tmp");
    let tmp_path = PathBuf::from(tmp_os);

    std::fs::write(&tmp_path, bytes)?;
    std::fs::rename(&tmp_path, dest_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_writes_expected_content_and_cleans_up_tmp() {
        let mut path = std::env::temp_dir();
        path.push(format!("ferroflow-rule-resources-write-atomic-test-{}.srs", std::process::id()));
        let tmp_path = {
            let mut p = path.as_os_str().to_os_string();
            p.push(".tmp");
            PathBuf::from(p)
        };
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&tmp_path);

        write_atomic(&path, b"hello rule-set").expect("write_atomic should succeed");

        assert_eq!(std::fs::read(&path).unwrap(), b"hello rule-set");
        assert!(!tmp_path.exists(), "the .tmp file should be renamed away, not left behind");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_atomic_creates_missing_parent_directory() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("ferroflow-rule-resources-nested-{}", std::process::id()));
        let path = dir.join("geosite-netflix.srs");
        let _ = std::fs::remove_dir_all(&dir);

        write_atomic(&path, b"content").expect("write_atomic should create the parent dir");
        assert!(path.is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real, live download of a real, small upstream file
    /// (`geosite-netflix.srs`, confirmed to exist and return HTTP 200 with
    /// real binary content) -- the one integration test in this crate that
    /// actually touches the network. Not run by default (`#[ignore]`), same
    /// convention as `core-manager`'s `real_singbox_*` tests: run manually
    /// with `cargo test -p rule-resources --all-targets -- --ignored`.
    #[tokio::test]
    #[ignore = "hits the real network (raw.githubusercontent.com) -- run manually"]
    async fn real_download_of_geosite_netflix_srs() {
        use crate::catalog::{resource_url, ResourceCategory};

        let url = resource_url(ResourceCategory::Geosite, "netflix", None);

        let mut path = std::env::temp_dir();
        path.push(format!("ferroflow-rule-resources-real-download-test-{}.srs", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let result = download(&url, &path).await.expect("real download of geosite-netflix.srs should succeed");

        assert!(path.is_file(), "downloaded file should exist at {}", path.display());
        let on_disk = std::fs::metadata(&path).unwrap().len();
        assert_eq!(on_disk, result.size_bytes);
        assert!(result.size_bytes > 0, "geosite-netflix.srs should not be empty");
        assert_eq!(result.sha256.len(), 64, "sha256 should be a 64-char hex string");
        assert!(!result.downloaded_at.is_empty());

        let _ = std::fs::remove_file(&path);
    }
}
