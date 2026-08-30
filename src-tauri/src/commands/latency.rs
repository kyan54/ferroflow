//! Lightweight per-server latency probe -- a raw TCP-connect timing, **not**
//! a full proxy handshake through sing-box/the privileged helper. This is
//! the standard "how long does establishing a connection to
//! `server.address:server.port` take" technique proxy-client UIs use for a
//! quick per-node ping/latency indicator (see the Servers page's per-card
//! latency badge and "Test all" button), and deliberately doesn't require
//! sing-box or the helper to be running at all -- it's just
//! `tokio::net::TcpStream::connect` against the server's own endpoint,
//! wall-clock timed.
//!
//! A failed/timed-out probe is `Ok(None)`, not an `Err` -- "no response" is
//! a legitimate, expected result for an unreachable/misconfigured server,
//! not a command failure the frontend should show as an error toast (see
//! `servers_test_latency_all`'s doc comment for the same reasoning applied
//! to the batch variant).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use shared_types::{AppError, AppResult};
use tauri::State;
use tokio::net::TcpStream;
use tokio::task::JoinSet;

use crate::state::AppState;

/// Generous enough for a real-world TCP handshake to a far-away server
/// (including one behind a slow/loaded network path), short enough that one
/// unreachable server doesn't stall a "Test all" batch for long.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Times a raw TCP connect to `address:port`. `None` on timeout or any
/// connect error (DNS failure, connection refused, unreachable, ...) --
/// this function never fails, by design (see module doc comment).
async fn probe_tcp_connect(address: &str, port: u16) -> Option<u32> {
    let start = Instant::now();
    let result = tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect((address, port))).await;
    match result {
        Ok(Ok(_stream)) => Some(start.elapsed().as_millis() as u32),
        _ => None,
    }
}

#[tauri::command]
pub async fn server_test_latency(state: State<'_, AppState>, server_id: String) -> AppResult<Option<u32>> {
    let server = {
        let config = state.config.lock().unwrap();
        config.servers.iter().find(|s| s.id == server_id).cloned()
    };
    let Some(server) = server else {
        return Err(AppError::new("server_not_found", format!("no server with id {server_id}")));
    };
    Ok(probe_tcp_connect(&server.address, server.port).await)
}

/// Runs `probe_tcp_connect` concurrently across every server in the current
/// config (mirrors `core_manager::unlock::check_all`'s concurrent-probe
/// pattern, adapted to a dynamic-length list via `JoinSet` rather than a
/// fixed-arity `tokio::join!`) and returns one `server_id -> Option<ms>`
/// entry per server. Never fails outright -- an individual server's probe
/// failing is reflected as that entry's `None`, not a batch-wide `Err`.
#[tauri::command]
pub async fn servers_test_latency_all(state: State<'_, AppState>) -> AppResult<HashMap<String, Option<u32>>> {
    let servers = {
        let config = state.config.lock().unwrap();
        config.servers.clone()
    };

    let mut set = JoinSet::new();
    for server in servers {
        set.spawn(async move {
            let ms = probe_tcp_connect(&server.address, server.port).await;
            (server.id, ms)
        });
    }

    let mut results = HashMap::new();
    while let Some(joined) = set.join_next().await {
        if let Ok((id, ms)) = joined {
            results.insert(id, ms);
        }
    }
    Ok(results)
}
