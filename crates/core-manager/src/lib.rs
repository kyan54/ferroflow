//! Owns the sing-box child process lifecycle: config generation and
//! start/stop/status as a plain unprivileged process. Helper integration
//! (privileged spawn for TUN mode) and the gRPC status/connections stream
//! (`daemon.StartedService`, sing-box 1.14+) are later passes — see
//! `docs/ipc-contract.md` for the Tauri command surface this crate backs.

pub mod config;
pub mod process;

use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use shared_types::{AppError, AppResult, ProxyErrorCode, ProxyStatus, ServerConfig};
use tokio::sync::Mutex;

use process::ProcessHandle;

/// Overrides sing-box binary discovery (`CoreManager::new`'s default
/// `locate_binary`). Checked first, before the `.dev-bin` dev convenience
/// and the `PATH` fallback.
const BINARY_PATH_ENV: &str = "FERROFLOW_SINGBOX_PATH";

struct RunningCore {
    handle: ProcessHandle,
    server_id: String,
    start_time_millis: i64,
    config_path: PathBuf,
}

pub struct CoreManager {
    binary_path: PathBuf,
    running: Mutex<Option<RunningCore>>,
}

impl CoreManager {
    /// Resolves the sing-box binary via `locate_binary` (env var / dev
    /// convenience / `PATH`). Use `with_binary_path` to pin an explicit
    /// path instead (tests, or once the app has its own binary discovery).
    pub fn new() -> Self {
        Self::with_binary_path(Self::locate_binary())
    }

    /// Builds a `CoreManager` that spawns exactly this binary path.
    pub fn with_binary_path(binary_path: impl Into<PathBuf>) -> Self {
        Self { binary_path: binary_path.into(), running: Mutex::new(None) }
    }

    /// Binary discovery order: `FERROFLOW_SINGBOX_PATH` env var (if set and
    /// non-empty) -> `./.dev-bin/sing-box[.exe]` (local dev convenience,
    /// see `.gitignore`) -> bare `sing-box[.exe]`, relying on `PATH`. Never
    /// fails here — a bad path surfaces as a `core_start_failed` AppError
    /// from `start()` when the spawn itself fails.
    fn locate_binary() -> PathBuf {
        if let Ok(path) = std::env::var(BINARY_PATH_ENV) {
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }

        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let dev_bin = PathBuf::from(".dev-bin").join(binary_name);
        if dev_bin.is_file() {
            return dev_bin;
        }

        PathBuf::from(binary_name)
    }

    /// Starts sing-box against `server`: generates the config, writes it to
    /// a temp file, spawns `<binary> run -c <config>`, and tracks the
    /// child. If a core is already running (e.g. switching servers), it's
    /// stopped first rather than leaking a second process.
    pub async fn start(&self, server: &ServerConfig) -> AppResult<ProxyStatus> {
        let mut guard = self.running.lock().await;

        if let Some(mut existing) = guard.take() {
            let _ = existing.handle.stop().await;
            let _ = std::fs::remove_file(&existing.config_path);
        }

        let port = pick_local_port().map_err(|e| {
            AppError::new("port_in_use", format!("failed to allocate a local proxy port: {e}"))
        })?;

        let cfg = config::build_config(server, port);
        let config_path = write_temp_config(&cfg).map_err(|e| {
            AppError::new("config_invalid", format!("failed to write sing-box config: {e}"))
        })?;

        let handle = ProcessHandle::spawn(&self.binary_path, &config_path).map_err(|e| {
            // Config file is orphaned on this path (spawn never happened to
            // consume it) — best-effort cleanup so temp dir doesn't accumulate.
            let _ = std::fs::remove_file(&config_path);
            AppError::new(
                "core_start_failed",
                format!(
                    "failed to spawn sing-box binary at '{}': {e}",
                    self.binary_path.display()
                ),
            )
        })?;

        let pid = handle.pid();
        let start_time_millis = now_millis();

        *guard = Some(RunningCore {
            handle,
            server_id: server.id.clone(),
            start_time_millis,
            config_path,
        });

        Ok(ProxyStatus {
            running: true,
            pid,
            start_time: Some(start_time_millis),
            uptime_secs: Some(0),
            error: None,
            error_code: None,
            current_server_id: Some(server.id.clone()),
        })
    }

    /// Stops the tracked sing-box process, if any, and cleans up its temp
    /// config file. A no-op (not an error) if nothing is running.
    pub async fn stop(&self) -> AppResult<ProxyStatus> {
        let mut guard = self.running.lock().await;
        if let Some(mut running) = guard.take() {
            if let Err(e) = running.handle.stop().await {
                tracing::warn!("error stopping sing-box process: {e}");
            }
            let _ = std::fs::remove_file(&running.config_path);
        }
        Ok(ProxyStatus::default())
    }

    /// Reports whether the tracked child is still alive, with uptime since
    /// `start()`. If the process died on its own (crash, rejected config,
    /// killed externally), this detects it, clears the tracked state, and
    /// reports the exit as an error rather than silently going stale.
    pub async fn status(&self) -> AppResult<ProxyStatus> {
        let mut guard = self.running.lock().await;
        let Some(running) = guard.as_mut() else {
            return Ok(ProxyStatus::default());
        };

        if running.handle.is_alive() {
            let uptime_secs = ((now_millis() - running.start_time_millis).max(0) / 1000) as u64;
            Ok(ProxyStatus {
                running: true,
                pid: running.handle.pid(),
                start_time: Some(running.start_time_millis),
                uptime_secs: Some(uptime_secs),
                error: None,
                error_code: None,
                current_server_id: Some(running.server_id.clone()),
            })
        } else {
            let exit_desc = running.handle.exit_description();
            let config_path = running.config_path.clone();
            *guard = None;
            let _ = std::fs::remove_file(&config_path);

            Ok(ProxyStatus {
                running: false,
                error: Some(exit_desc.unwrap_or_else(|| "sing-box exited unexpectedly".into())),
                error_code: Some(ProxyErrorCode::CoreStartFailed),
                ..ProxyStatus::default()
            })
        }
    }
}

impl Default for CoreManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Picks a free local TCP port by binding ephemeral (`:0`) and reading back
/// the OS-assigned port, then dropping the listener before sing-box binds
/// it. Small TOCTOU window (another process could grab it first) is
/// acceptable for MVP — there's no concurrent-start scenario yet.
fn pick_local_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.local_addr().map(|addr| addr.port())
}

/// Writes `config` to a fresh temp file and returns its path. Filename
/// includes this process's pid + a nanosecond timestamp, which is unique
/// enough for "one file per start() call on one machine" without pulling
/// in a UUID crate for a single call site.
fn write_temp_config(config: &serde_json::Value) -> std::io::Result<PathBuf> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let mut path = std::env::temp_dir();
    path.push(format!("ferroflow-singbox-{}-{}.json", std::process::id(), nanos));
    std::fs::write(&path, serde_json::to_vec_pretty(config)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_local_port_returns_nonzero() {
        let port = pick_local_port().expect("should allocate a port");
        assert_ne!(port, 0);
    }

    #[test]
    fn locate_binary_respects_env_override() {
        // SAFETY: test-only env mutation, single-threaded within this test
        // (no other test in this module touches this var).
        unsafe {
            std::env::set_var(BINARY_PATH_ENV, "C:/somewhere/sing-box.exe");
        }
        let path = CoreManager::locate_binary();
        unsafe {
            std::env::remove_var(BINARY_PATH_ENV);
        }
        assert_eq!(path, PathBuf::from("C:/somewhere/sing-box.exe"));
    }

    #[tokio::test]
    async fn status_with_nothing_running_is_default() {
        let manager = CoreManager::with_binary_path("does-not-exist");
        let status = manager.status().await.unwrap();
        assert!(!status.running);
        assert!(status.pid.is_none());
    }

    #[tokio::test]
    async fn start_with_missing_binary_returns_core_start_failed() {
        let manager = CoreManager::with_binary_path("definitely-not-a-real-binary-xyz");
        let server = ServerConfig {
            id: "s1".into(),
            name: "test".into(),
            protocol: shared_types::Protocol::Trojan,
            address: "example.com".into(),
            port: 443,
            uuid: None,
            password: Some("pw".into()),
            encryption: None,
            flow: None,
            tls: None,
        };
        let err = manager.start(&server).await.unwrap_err();
        assert_eq!(err.code, "core_start_failed");
    }
}
