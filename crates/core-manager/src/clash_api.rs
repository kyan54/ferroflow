//! Thin HTTP client for the sing-box Clash API endpoints this app needs --
//! `GET /connections`, `DELETE /connections/{id}`, `DELETE /connections`.
//! This is a well-documented, stable sing-box feature (enabled per-run via
//! `config::build_config`/`build_tun_config`'s `experimental.clash_api`
//! block), not anything this app invents.
//!
//! No auth: the Clash API is enabled with no `secret` configured and bound
//! to `127.0.0.1` only (see `config.rs`) -- a deliberate MVP simplification
//! documented in `docs/ipc-contract.md`'s "Live connections" section, not an
//! oversight. Revisit if the local port is ever considered a meaningful
//! attack surface.

use std::time::Duration;

use shared_types::ConnectionsSnapshot;
use thiserror::Error;

/// Loopback-only, no concurrent-start scenario to race against -- a short
/// timeout is safe and appropriate here: this is a call to a process we just
/// confirmed is running (`CoreManager::running` holds a live `RunningCore`),
/// so it should never hang.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Failure modes for the three functions below, each carrying enough context
/// to be surfaced directly as an `AppError` message (see
/// `CoreManager::list_connections`/`close_connection`/`close_all_connections`).
#[derive(Debug, Error)]
pub enum ClashApiError {
    #[error("could not build an HTTP client: {0}")]
    ClientBuild(reqwest::Error),
    #[error("request to '{url}' failed: {source}")]
    Request { url: String, source: reqwest::Error },
    #[error("'{url}' returned HTTP {status}")]
    Status { url: String, status: reqwest::StatusCode },
    #[error("failed to parse response from '{url}': {source}")]
    Deserialize { url: String, source: reqwest::Error },
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

fn client() -> Result<reqwest::Client, ClashApiError> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(ClashApiError::ClientBuild)
}

/// `GET /connections` -- current connection list plus cumulative
/// upload/download totals since sing-box started.
pub async fn get_connections(port: u16) -> Result<ConnectionsSnapshot, ClashApiError> {
    let url = format!("{}/connections", base_url(port));
    let client = client()?;

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|source| ClashApiError::Request { url: url.clone(), source })?;

    let status = response.status();
    if !status.is_success() {
        return Err(ClashApiError::Status { url, status });
    }

    response
        .json::<ConnectionsSnapshot>()
        .await
        .map_err(|source| ClashApiError::Deserialize { url, source })
}

/// `DELETE /connections/{id}` -- closes one connection. Any 2xx response is
/// treated as success (sing-box returns `204 No Content`).
pub async fn close_connection(port: u16, id: &str) -> Result<(), ClashApiError> {
    let url = format!("{}/connections/{}", base_url(port), id);
    let client = client()?;

    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|source| ClashApiError::Request { url: url.clone(), source })?;

    let status = response.status();
    if !status.is_success() {
        return Err(ClashApiError::Status { url, status });
    }

    Ok(())
}

/// `DELETE /connections` -- closes every current connection.
pub async fn close_all_connections(port: u16) -> Result<(), ClashApiError> {
    let url = format!("{}/connections", base_url(port));
    let client = client()?;

    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|source| ClashApiError::Request { url: url.clone(), source })?;

    let status = response.status();
    if !status.is_success() {
        return Err(ClashApiError::Status { url, status });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// Real end-to-end check: starts an actual sing-box instance via
    /// `CoreManager` (mirroring `real_singbox_local_backend_start_status_stop_lifecycle`
    /// in `lib.rs`) and confirms its Clash API actually comes up and answers
    /// `GET /connections` with well-formed JSON. Not asserting anything
    /// about real traffic -- `test_server()`'s bogus trojan target never
    /// receives a connection here, so 0 totals and an empty connection list
    /// are the expected (and only meaningful) outcome. Not run by default
    /// (`#[ignore]`), needs a real binary at
    /// `<workspace root>/.dev-bin/sing-box[.exe]`. Run manually with:
    /// `cargo test -p core-manager --all-targets -- --ignored real_singbox`
    #[tokio::test]
    #[ignore = "needs a real sing-box binary at <workspace root>/.dev-bin/"]
    async fn real_singbox_clash_api_list_connections_returns_snapshot() {
        use crate::CoreManager;
        use shared_types::{Protocol, ProxyModeType, ServerConfig};

        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let binary =
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.dev-bin")).join(binary_name);
        assert!(binary.is_file(), "expected a real sing-box binary at {}", binary.display());

        let manager = CoreManager::with_binary_path(binary);
        let server = ServerConfig {
            id: "s1".into(),
            name: "test".into(),
            protocol: Protocol::Trojan,
            address: "example.com".into(),
            port: 443,
            uuid: None,
            password: Some("pw".into()),
            encryption: None,
            flow: None,
            tls: None,
        };

        let started = manager
            .start(&server, ProxyModeType::SystemProxy, &[])
            .await
            .expect("start should succeed against a real sing-box binary");
        assert!(started.running);

        // Give sing-box a moment to finish bringing up the Clash API
        // listener before querying it.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let snapshot =
            manager.list_connections().await.expect("clash api should respond with a snapshot");
        // No real traffic flowed (bogus outbound target, nothing dialed it)
        // -- 0 is the expected value, this just confirms the API came up and
        // returned well-formed JSON at all.
        assert_eq!(snapshot.download_total, 0);
        assert_eq!(snapshot.upload_total, 0);
        assert!(snapshot.connections.is_empty());

        manager.stop().await.expect("stop should succeed");
    }
}
