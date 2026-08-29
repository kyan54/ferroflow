//! Owns the sing-box core lifecycle: config generation and start/stop/status
//! across two backends. `ProxyModeType::SystemProxy`/`Manual` spawn sing-box
//! as a plain unprivileged child process (`process::ProcessHandle`), same as
//! always. `ProxyModeType::Tun` routes through `helper-client` instead — a
//! plain process can't create a TUN interface, so that mode asks whichever
//! platform's privileged helper (`helper-windows`/`helper-macos`/
//! `helper-linux`) to spawn its own install-time-verified sing-box binary
//! under its elevated identity (root/SYSTEM/ambient caps) and own the child.
//! The gRPC status/connections stream (`daemon.StartedService`, sing-box
//! 1.14+) is still a later pass — see `docs/ipc-contract.md` for the Tauri
//! command surface this crate backs.

pub mod config;
pub mod process;
pub mod tun;

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::time::{SystemTime, UNIX_EPOCH};

use helper_client::HelperClient;
use shared_types::{
    AppError, AppResult, ProxyErrorCode, ProxyModeType, ProxyStatus, RoutingRule, ServerConfig,
};
use tokio::sync::Mutex;

use process::ProcessHandle;

/// Overrides sing-box binary discovery (`CoreManager::new`'s default
/// `locate_binary`). Checked first, before the `.dev-bin` dev convenience
/// and the `PATH` fallback.
const BINARY_PATH_ENV: &str = "FERROFLOW_SINGBOX_PATH";

/// Which backend a `RunningCore` is tracked against. `Local` owns the child
/// process directly (today's behavior); `Helper`-backed runs have no local
/// `ProcessHandle` at all — the helper process owns the actual child, this
/// side only remembers that a run is in flight via the helper plus the
/// bookkeeping (`server_id`/`start_time_millis`/`config_path`) shared with
/// the `Local` case.
enum Backend {
    Local(Box<ProcessHandle>),
    Helper,
}

struct RunningCore {
    backend: Backend,
    server_id: String,
    start_time_millis: i64,
    config_path: PathBuf,
}

pub struct CoreManager {
    binary_path: PathBuf,
    running: Mutex<Option<RunningCore>>,
    /// Shared-token auth for talking to the Windows/macOS helper (`None` on
    /// Linux, where `SO_PEERCRED` is the trust boundary — see
    /// `helper-client`'s module doc comment). Plain `std::sync::Mutex`
    /// rather than tokio's: this is only ever read/written across brief,
    /// non-async sections (a clone into a `HelperClient::new` call), never
    /// held across an `.await`.
    helper_token: StdMutex<Option<String>>,
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
        Self {
            binary_path: binary_path.into(),
            running: Mutex::new(None),
            helper_token: StdMutex::new(None),
        }
    }

    /// Sets (or clears, with `None`) the token used to authenticate to the
    /// Windows/macOS privileged helper for subsequent `Tun`-mode
    /// `start`/`stop`/`status` calls. Callers persist/load this token
    /// themselves (see `docs/ipc-contract.md`'s "Helper install flow") and
    /// push it in here once available — a helper install that happens after
    /// this `CoreManager` was constructed doesn't require rebuilding it.
    pub fn set_helper_token(&self, token: Option<String>) {
        *self.helper_token.lock().unwrap() = token;
    }

    fn helper_client(&self) -> HelperClient {
        HelperClient::new(self.helper_token.lock().unwrap().clone())
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

    /// Starts sing-box against `server` for `mode_type`:
    /// - `SystemProxy`/`Manual`: unchanged behavior — builds the local
    ///   `mixed`-inbound config, writes it to a temp file, spawns
    ///   `<binary> run -c <config>` as a plain child (`Backend::Local`).
    /// - `Tun`: requires the privileged helper to be installed and
    ///   responding (`HelperClient::is_available`); if not, returns
    ///   `Err(AppError{code:"helper_unavailable",..})` rather than silently
    ///   falling back to an unprivileged (and non-functional, for TUN) local
    ///   process. If available, builds the TUN config, writes it to a temp
    ///   file, and asks the helper to start it (`Backend::Helper`) — the
    ///   helper spawns its own install-time-verified sing-box binary under
    ///   its elevated identity, ignoring the `core_path` this passes.
    ///
    /// If a core is already running under either backend (e.g. switching
    /// servers or modes), it's stopped first rather than leaking a second
    /// process/helper-managed run.
    ///
    /// `rules` is the caller's current `UserConfig.rules` list, threaded
    /// straight through to `config::build_config`/`build_tun_config` — see
    /// `config::build_route_rules` for how disabled/empty-values rules are
    /// filtered out and mapped to sing-box `route.rules` entries.
    pub async fn start(
        &self,
        server: &ServerConfig,
        mode_type: ProxyModeType,
        rules: &[RoutingRule],
    ) -> AppResult<ProxyStatus> {
        let mut guard = self.running.lock().await;

        if let Some(existing) = guard.take() {
            self.stop_running(existing).await;
        }

        match mode_type {
            ProxyModeType::SystemProxy | ProxyModeType::Manual => {
                let port = pick_local_port().map_err(|e| {
                    AppError::new("port_in_use", format!("failed to allocate a local proxy port: {e}"))
                })?;

                let cfg = config::build_config(server, port, rules);
                let config_path = write_temp_config(&cfg).map_err(|e| {
                    AppError::new("config_invalid", format!("failed to write sing-box config: {e}"))
                })?;

                let handle = ProcessHandle::spawn(&self.binary_path, &config_path).map_err(|e| {
                    // Config file is orphaned on this path (spawn never happened
                    // to consume it) — best-effort cleanup so temp dir doesn't
                    // accumulate.
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
                    backend: Backend::Local(Box::new(handle)),
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
            ProxyModeType::Tun => {
                let helper = self.helper_client();
                if !helper.is_available().await {
                    return Err(AppError::new(
                        "helper_unavailable",
                        "TUN mode requires the privileged helper, which is not installed or not responding",
                    ));
                }

                let cfg = config::build_tun_config(server, rules);
                let config_path = write_temp_config(&cfg).map_err(|e| {
                    AppError::new("config_invalid", format!("failed to write sing-box config: {e}"))
                })?;

                if let Err(e) = helper.start(config_path.to_string_lossy(), "").await {
                    let _ = std::fs::remove_file(&config_path);
                    return Err(AppError::new(
                        "helper_start_failed",
                        format!("helper failed to start sing-box: {e}"),
                    ));
                }

                let start_time_millis = now_millis();

                *guard = Some(RunningCore {
                    backend: Backend::Helper,
                    server_id: server.id.clone(),
                    start_time_millis,
                    config_path,
                });

                Ok(ProxyStatus {
                    running: true,
                    // The helper's `Start` response isn't parsed for a pid in
                    // this pass (its shape varies slightly per platform and
                    // isn't load-bearing for MVP) — `status()` still reports
                    // liveness correctly by asking the helper directly.
                    pid: None,
                    start_time: Some(start_time_millis),
                    uptime_secs: Some(0),
                    error: None,
                    error_code: None,
                    current_server_id: Some(server.id.clone()),
                })
            }
        }
    }

    /// Stops whichever backend is tracked, if any, and cleans up its temp
    /// config file. A no-op (not an error) if nothing is running.
    pub async fn stop(&self) -> AppResult<ProxyStatus> {
        let mut guard = self.running.lock().await;
        if let Some(running) = guard.take() {
            self.stop_running(running).await;
        }
        Ok(ProxyStatus::default())
    }

    /// Shared stop logic for both the explicit `stop()` call and the
    /// "stop the previous run first" step in `start()`. Best-effort: errors
    /// are logged, never propagated — the caller is about to discard this
    /// `RunningCore` either way.
    async fn stop_running(&self, mut running: RunningCore) {
        match &mut running.backend {
            Backend::Local(handle) => {
                if let Err(e) = handle.stop().await {
                    tracing::warn!("error stopping sing-box process: {e}");
                }
            }
            Backend::Helper => {
                if let Err(e) = self.helper_client().stop().await {
                    tracing::warn!("error stopping sing-box via helper: {e}");
                }
            }
        }
        let _ = std::fs::remove_file(&running.config_path);
    }

    /// Reports whether the tracked run is still alive, with uptime since
    /// `start()`. If it died on its own (crash, rejected config, killed
    /// externally, or — for the helper backend — the helper reports
    /// not-running), this detects it, clears the tracked state, and reports
    /// the exit as an error rather than silently going stale.
    pub async fn status(&self) -> AppResult<ProxyStatus> {
        let mut guard = self.running.lock().await;
        let Some(running) = guard.as_mut() else {
            return Ok(ProxyStatus::default());
        };

        match &mut running.backend {
            Backend::Local(handle) => {
                if handle.is_alive() {
                    let uptime_secs = ((now_millis() - running.start_time_millis).max(0) / 1000) as u64;
                    Ok(ProxyStatus {
                        running: true,
                        pid: handle.pid(),
                        start_time: Some(running.start_time_millis),
                        uptime_secs: Some(uptime_secs),
                        error: None,
                        error_code: None,
                        current_server_id: Some(running.server_id.clone()),
                    })
                } else {
                    let exit_desc = handle.exit_description();
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
            Backend::Helper => {
                // `Status` returns `{"running": bool, "pid": ...}` (per
                // helper-windows/helper-macos/helper-linux's `handle_status`/
                // `status`/`cmd_status`) — a request failure (helper
                // unreachable, crashed) is treated the same as "not running"
                // rather than surfacing a distinct error shape here.
                let is_running = match self.helper_client().status().await {
                    Ok(value) => value.get("running").and_then(|v| v.as_bool()).unwrap_or(false),
                    Err(e) => {
                        tracing::warn!("error querying helper status: {e}");
                        false
                    }
                };

                if is_running {
                    let uptime_secs = ((now_millis() - running.start_time_millis).max(0) / 1000) as u64;
                    Ok(ProxyStatus {
                        running: true,
                        pid: None,
                        start_time: Some(running.start_time_millis),
                        uptime_secs: Some(uptime_secs),
                        error: None,
                        error_code: None,
                        current_server_id: Some(running.server_id.clone()),
                    })
                } else {
                    let config_path = running.config_path.clone();
                    *guard = None;
                    let _ = std::fs::remove_file(&config_path);

                    Ok(ProxyStatus {
                        running: false,
                        error: Some("sing-box (helper-managed) is no longer running".into()),
                        error_code: Some(ProxyErrorCode::CoreStartFailed),
                        ..ProxyStatus::default()
                    })
                }
            }
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

    fn test_server() -> ServerConfig {
        ServerConfig {
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
        }
    }

    #[tokio::test]
    async fn start_with_missing_binary_returns_core_start_failed() {
        let manager = CoreManager::with_binary_path("definitely-not-a-real-binary-xyz");
        let server = test_server();
        let err = manager.start(&server, ProxyModeType::SystemProxy, &[]).await.unwrap_err();
        assert_eq!(err.code, "core_start_failed");
    }

    #[tokio::test]
    async fn start_manual_mode_also_uses_local_process() {
        // `Manual` shares the `SystemProxy` code path (both spawn a local
        // process) — verify it doesn't accidentally get routed to the
        // helper branch, which would fail differently (no helper running in
        // this test environment) and mask a routing bug.
        let manager = CoreManager::with_binary_path("definitely-not-a-real-binary-xyz");
        let server = test_server();
        let err = manager.start(&server, ProxyModeType::Manual, &[]).await.unwrap_err();
        assert_eq!(err.code, "core_start_failed");
    }

    #[tokio::test]
    async fn start_tun_mode_without_helper_returns_helper_unavailable() {
        // No helper is installed/running in this test environment (nor in
        // CI), so `HelperClient::is_available()` should reliably report
        // `false` here — this exercises exactly that branch and asserts
        // `start()` refuses TUN mode outright rather than silently falling
        // back to an unprivileged local process (which can't actually
        // create a TUN interface).
        let manager = CoreManager::with_binary_path("does-not-matter-for-this-path");
        let server = test_server();
        let err = manager.start(&server, ProxyModeType::Tun, &[]).await.unwrap_err();
        assert_eq!(err.code, "helper_unavailable");
    }

    #[tokio::test]
    async fn set_helper_token_is_stored_and_cleared() {
        let manager = CoreManager::with_binary_path("does-not-matter-for-this-path");
        manager.set_helper_token(Some("secret".into()));
        assert_eq!(manager.helper_token.lock().unwrap().as_deref(), Some("secret"));
        manager.set_helper_token(None);
        assert_eq!(*manager.helper_token.lock().unwrap(), None);
    }

    /// Real end-to-end smoke test against an actual sing-box binary, for the
    /// `SystemProxy` backend (`Backend::Local`) — the path this rework was
    /// most at risk of regressing, since `start`/`stop`/`status` all got
    /// restructured to branch on `Backend`. Not run by default (`#[ignore]`)
    /// since it needs a real binary at `<workspace root>/.dev-bin/sing-box[.exe]`
    /// (dev convenience, gitignored — see `locate_binary`'s doc comment) that
    /// CI doesn't provision. Run manually with:
    /// `cargo test -p core-manager --all-targets -- --ignored real_singbox`
    #[tokio::test]
    #[ignore = "needs a real sing-box binary at <workspace root>/.dev-bin/"]
    async fn real_singbox_local_backend_start_status_stop_lifecycle() {
        let binary_name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
        let binary =
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.dev-bin")).join(binary_name);
        assert!(binary.is_file(), "expected a real sing-box binary at {}", binary.display());

        let manager = CoreManager::with_binary_path(binary);
        let server = test_server();

        let started = manager
            .start(&server, ProxyModeType::SystemProxy, &[])
            .await
            .expect("start should succeed against a real sing-box binary");
        assert!(started.running);
        assert!(started.pid.is_some());
        assert_eq!(started.current_server_id.as_deref(), Some("s1"));

        // Give sing-box a moment to finish initializing before polling
        // status — a bogus outbound server (test_server()'s "example.com"
        // trojan target) doesn't stop sing-box from starting; it only
        // matters once traffic actually tries to go through the proxy.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let status = manager.status().await.expect("status should succeed");
        assert!(status.running, "sing-box should still be running: {status:?}");
        assert_eq!(status.pid, started.pid);

        let stopped = manager.stop().await.expect("stop should succeed");
        assert!(!stopped.running);

        let status_after_stop = manager.status().await.expect("status should succeed");
        assert!(!status_after_stop.running);
    }
}
