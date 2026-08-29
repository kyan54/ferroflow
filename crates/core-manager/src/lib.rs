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

pub mod clash_api;
pub mod config;
pub mod history;
pub mod logs;
pub mod process;
pub mod tun;
pub mod unlock;

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use helper_client::HelperClient;
use shared_types::{
    AppError, AppResult, ConnectionsSnapshot, ProxyErrorCode, ProxyModeType, ProxyStatus,
    RoutingRule, RuleOutbound, ServerConfig, UnlockResult,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

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
    /// Which mode this run was started under — `Backend::Local` alone
    /// doesn't distinguish `SystemProxy` from `Manual` (both spawn a plain
    /// local process), but only `SystemProxy` should ever have
    /// `system_proxy.disable()` called against it on stop/crash.
    mode_type: ProxyModeType,
    /// The local `mixed` inbound's port, when this run has one
    /// (`SystemProxy`/`Manual`) — `None` for `Tun`. Mirrored into
    /// `ProxyStatus::local_port`.
    local_port: Option<u16>,
    /// Port sing-box's Clash API (`experimental.clash_api.external_controller`)
    /// is listening on for this run. Unlike `local_port`, every run has one
    /// — traffic visibility doesn't depend on which inbound is active — so
    /// this is not `Option`.
    clash_api_port: u16,
    /// The background `history::HistoryRecorder` task for this run, if
    /// `connection_history_enabled` was `true` and a history path was
    /// configured at `start()` time — `None` otherwise (history disabled, or
    /// `set_history_path` was never called). Aborted in `stop_running` — see
    /// that method's doc comment for why a hard `.abort()` rather than a
    /// cooperative cancellation channel is fine here.
    history_task: Option<JoinHandle<()>>,
    /// Background `logs::spawn_line_reader` tasks forwarding this run's
    /// sing-box stdout/stderr into `log_buffer`, when `Backend::Local` and a
    /// buffer has been configured (`set_log_buffer`) — empty for
    /// `Backend::Helper` (no local child to read from here; the helper owns
    /// it) or if no buffer was ever set. Aborted in `stop_running` alongside
    /// `history_task`, same reasoning.
    log_tasks: Vec<JoinHandle<()>>,
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
    /// Owns actually pointing the OS at the local proxy for `SystemProxy`
    /// mode (`net::SystemProxyManager` is a zero-sized dispatcher, not
    /// per-instance state, so one shared here is fine).
    system_proxy: net::SystemProxyManager,
    /// Where `history::HistoryRecorder` should write finished-connection log
    /// lines, once configured — `None` until `set_history_path` is called
    /// (this type has no app-data-directory knowledge of its own, same
    /// reasoning as `helper_token` above) or if the caller explicitly clears
    /// it back to `None`. Read fresh at the top of every `start()` call, so
    /// a path set after construction applies to every run started after
    /// that point.
    history_dir_or_path: StdMutex<Option<PathBuf>>,
    /// The shared `logs::LogBuffer` sing-box stdout/stderr lines get pushed
    /// into, once configured — `None` until `set_log_buffer` is called.
    /// Unlike `history_dir_or_path`, this isn't app-data-path-dependent, but
    /// it still has to be a setter rather than built inside `new()`: the
    /// *same* `Arc<LogBuffer>` also needs to back `src-tauri`'s
    /// `tracing_subscriber::Layer`, which is installed before `CoreManager`
    /// is constructed at all (see `src-tauri`'s `run()`), so the one true
    /// instance is created there first and handed in here afterward.
    log_buffer: StdMutex<Option<Arc<logs::LogBuffer>>>,
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
            system_proxy: net::SystemProxyManager::new(),
            history_dir_or_path: StdMutex::new(None),
            log_buffer: StdMutex::new(None),
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

    /// Sets (or clears, with `None`) the file path `history::HistoryRecorder`
    /// appends finished-connection log lines to for subsequent `start()`
    /// calls where `connection_history_enabled` is `true`. Mirrors
    /// `set_helper_token` exactly: `CoreManager` is constructed with no
    /// app-data-directory knowledge of its own, so `src-tauri`'s setup hook
    /// calls this once `AppHandle`/`app_config_dir()` are available (see
    /// `state::init_history_path`) — a run started before that call simply
    /// gets no history recorder (see `start()`'s doc comment), same as a
    /// `Tun`-mode start before `set_helper_token` would fail outright.
    pub fn set_history_path(&self, path: Option<PathBuf>) {
        *self.history_dir_or_path.lock().unwrap() = path;
    }

    /// Sets the shared `logs::LogBuffer` that sing-box stdout/stderr lines
    /// get forwarded into for subsequent `start()` calls. See the field's
    /// doc comment for why this must be handed in from outside rather than
    /// built inside `new()`. Called once, from `AppState::new()`, right
    /// after the same `Arc` was used to build the app's tracing layer.
    pub fn set_log_buffer(&self, buffer: Arc<logs::LogBuffer>) {
        *self.log_buffer.lock().unwrap() = Some(buffer);
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
    ///
    /// `resource_paths` maps a downloaded rule-set resource's id (see
    /// `shared_types::RuleResourceInfo::id`) to its `.srs` file's absolute
    /// path on disk -- built by the caller (`src-tauri`'s `proxy_start`
    /// command) from `UserConfig.rule_resources` plus the app's known
    /// rule-resources storage directory convention. Threaded straight
    /// through to `config::build_config`/`build_tun_config`; an empty map is
    /// fine when no `RuleMatchType::RuleSet` rule is in play (every such
    /// rule is then skipped with a `tracing::warn!`, same as an id that's
    /// individually missing -- see `config::build_route_rules`).
    ///
    /// `connection_history_enabled` is the caller's current
    /// `UserConfig.connection_history_enabled` (opt-in, default `false`) —
    /// when `true` *and* a path has been configured via `set_history_path`,
    /// a `history::HistoryRecorder` background task is spawned against this
    /// run's `clash_api_port` right before returning. Neither condition
    /// alone is enough: `true` with no path configured (e.g. this call
    /// happens before `src-tauri`'s setup hook runs) spawns nothing rather
    /// than erroring, and a path configured with the flag `false` likewise
    /// spawns nothing — this is a deliberately best-effort, silent-skip
    /// feature, not one that can fail `start()` outright. Only applies to
    /// runs started *after* the flag was turned on; flipping it while a
    /// proxy is already running does not retroactively start logging that
    /// run (there is no live-reconfiguration path here, matching this
    /// codebase's stated preference for not over-engineering MVP features).
    ///
    /// `default_outbound` is the caller's current `UserConfig.default_outbound`
    /// -- threaded straight through to `config::build_config`/
    /// `build_tun_config` as the tag `route.final` resolves to. See that
    /// field's doc comment for why this exists (region presets needing a
    /// non-proxy catch-all).
    pub async fn start(
        &self,
        server: &ServerConfig,
        mode_type: ProxyModeType,
        rules: &[RoutingRule],
        resource_paths: &HashMap<String, PathBuf>,
        connection_history_enabled: bool,
        default_outbound: RuleOutbound,
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
                // Second independent ephemeral-port allocation for the Clash
                // API listener — `pick_local_port` binds `:0` and releases
                // the listener before returning, so back-to-back calls never
                // collide with each other (each gets a fresh OS-assigned
                // port), only (in principle) with something else grabbing
                // the port in the brief window between release and sing-box
                // actually binding it -- see `pick_local_port`'s doc comment.
                let clash_api_port = pick_local_port().map_err(|e| {
                    AppError::new(
                        "port_in_use",
                        format!("failed to allocate a local port for the Clash API: {e}"),
                    )
                })?;

                let cfg =
                    config::build_config(server, port, rules, resource_paths, clash_api_port, default_outbound);
                let config_path = write_temp_config(&cfg).map_err(|e| {
                    AppError::new("config_invalid", format!("failed to write sing-box config: {e}"))
                })?;

                let mut handle = ProcessHandle::spawn(&self.binary_path, &config_path).map_err(|e| {
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

                // Start forwarding this run's stdout/stderr into the shared
                // log buffer right away, before any of the fallible steps
                // below -- a config-rejection or system-proxy failure below
                // still logs whatever sing-box printed about it.
                let (stdout, stderr) = handle.take_stdio();
                let log_tasks = self.spawn_core_log_readers(stdout, stderr);

                // `SystemProxy` mode promises the OS actually routes through
                // this proxy -- if we can't make that true, fail loudly
                // rather than leave the user thinking they're covered while
                // sing-box quietly listens to nothing. `Manual` mode makes
                // no such promise (the user points their own apps at
                // `local_port` by hand), so it skips this entirely.
                if matches!(mode_type, ProxyModeType::SystemProxy) {
                    if let Err(e) = self.system_proxy.enable(port, port) {
                        let _ = handle.stop().await;
                        for task in log_tasks {
                            task.abort();
                        }
                        let _ = std::fs::remove_file(&config_path);
                        return Err(AppError::new(
                            "system_proxy_failed",
                            format!("sing-box started but enabling the system proxy failed: {e}"),
                        ));
                    }
                }

                let pid = handle.pid();
                let start_time_millis = now_millis();
                let history_task =
                    self.spawn_history_task_if_enabled(connection_history_enabled, clash_api_port);

                *guard = Some(RunningCore {
                    backend: Backend::Local(Box::new(handle)),
                    server_id: server.id.clone(),
                    start_time_millis,
                    config_path,
                    mode_type,
                    local_port: Some(port),
                    clash_api_port,
                    history_task,
                    log_tasks,
                });

                Ok(ProxyStatus {
                    running: true,
                    pid,
                    start_time: Some(start_time_millis),
                    uptime_secs: Some(0),
                    error: None,
                    error_code: None,
                    current_server_id: Some(server.id.clone()),
                    local_port: Some(port),
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

                let clash_api_port = pick_local_port().map_err(|e| {
                    AppError::new(
                        "port_in_use",
                        format!("failed to allocate a local port for the Clash API: {e}"),
                    )
                })?;

                let cfg =
                    config::build_tun_config(server, rules, resource_paths, clash_api_port, default_outbound);
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
                let history_task =
                    self.spawn_history_task_if_enabled(connection_history_enabled, clash_api_port);

                *guard = Some(RunningCore {
                    backend: Backend::Helper,
                    server_id: server.id.clone(),
                    start_time_millis,
                    config_path,
                    mode_type,
                    local_port: None,
                    clash_api_port,
                    history_task,
                    // No local child here to read stdout/stderr from -- the
                    // helper owns the actual sing-box process. Out of scope
                    // for this pass (see `logs` module doc comment).
                    log_tasks: Vec::new(),
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
                    local_port: None,
                })
            }
        }
    }

    /// Spawns a `history::HistoryRecorder` background task for a just-started
    /// run when `enabled` is `true` and a history path has been configured
    /// (`set_history_path`) — `None` (no task) in every other case. See
    /// `start()`'s doc comment for the full "both conditions required, never
    /// errors" reasoning.
    fn spawn_history_task_if_enabled(&self, enabled: bool, clash_api_port: u16) -> Option<JoinHandle<()>> {
        if !enabled {
            return None;
        }
        let path = self.history_dir_or_path.lock().unwrap().clone()?;
        Some(history::HistoryRecorder::spawn(clash_api_port, path))
    }

    /// Spawns `logs::spawn_line_reader` for whichever of `stdout`/`stderr`
    /// is `Some`, forwarding lines into the configured `log_buffer` -- an
    /// empty `Vec` (no tasks) if `set_log_buffer` was never called, mirroring
    /// `spawn_history_task_if_enabled`'s "no configuration, spawn nothing"
    /// convention rather than erroring.
    fn spawn_core_log_readers(
        &self,
        stdout: Option<tokio::process::ChildStdout>,
        stderr: Option<tokio::process::ChildStderr>,
    ) -> Vec<JoinHandle<()>> {
        let Some(buffer) = self.log_buffer.lock().unwrap().clone() else {
            return Vec::new();
        };
        let mut tasks = Vec::new();
        if let Some(stdout) = stdout {
            tasks.push(logs::spawn_line_reader(stdout, buffer.clone()));
        }
        if let Some(stderr) = stderr {
            tasks.push(logs::spawn_line_reader(stderr, buffer));
        }
        tasks
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
        self.disable_system_proxy_if_needed(&running);
        let _ = std::fs::remove_file(&running.config_path);

        // Hard `.abort()` rather than a cooperative cancellation channel
        // (e.g. a oneshot the loop selects against): this loop has no
        // critical section that needs graceful unwinding — worst case, an
        // abort lands mid-iteration and loses one in-flight Clash API fetch
        // or file write, which is an acceptable tradeoff for a best-effort
        // background logger. `.abort()` also takes effect at the task's next
        // `.await` point, which in practice is immediate here since nearly
        // the entire loop body is awaits (the `tokio::time::sleep` between
        // ticks included) — a cooperative channel would only be checked once
        // per multi-second tick, so `.abort()` is both simpler *and* faster
        // to actually stop. This also prevents a subsequent `start()` from
        // racing with a stale recorder still appending to the same file.
        if let Some(handle) = running.history_task.take() {
            handle.abort();
        }
        // Same reasoning as `history_task` above -- these are best-effort
        // background forwarders with no critical section, so a hard abort
        // (rather than a cooperative shutdown signal) is fine, and also
        // prevents a stale reader from a previous run racing a fresh
        // `start()`'s reader over the same shared `log_buffer` (harmless
        // either way, but pointless once the process it was reading from is
        // gone).
        for task in running.log_tasks.drain(..) {
            task.abort();
        }
    }

    /// Best-effort: reverses `start()`'s `system_proxy.enable()` call for a
    /// `SystemProxy`-mode run, whether it's ending via an explicit
    /// `stop()`/being superseded by a new `start()`, or because `status()`
    /// just detected it died on its own. Leaving the OS pointed at a proxy
    /// that's no longer running would silently break the user's system
    /// traffic, so this runs on every path that clears a `SystemProxy` run
    /// out of `self.running` — not just the happy-path stop.
    fn disable_system_proxy_if_needed(&self, running: &RunningCore) {
        if matches!(running.mode_type, ProxyModeType::SystemProxy) {
            if let Err(e) = self.system_proxy.disable() {
                tracing::warn!("error disabling system proxy: {e}");
            }
        }
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
                        local_port: running.local_port,
                    })
                } else {
                    let exit_desc = handle.exit_description();
                    let finished = guard.take().expect("just matched Some(running) above");
                    self.disable_system_proxy_if_needed(&finished);
                    let _ = std::fs::remove_file(&finished.config_path);

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
                        local_port: None,
                    })
                } else {
                    let finished = guard.take().expect("just matched Some(running) above");
                    self.disable_system_proxy_if_needed(&finished);
                    let _ = std::fs::remove_file(&finished.config_path);

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

    /// Queries sing-box's Clash API for the current connection list plus
    /// cumulative upload/download totals (`GET /connections` — see
    /// `clash_api::get_connections`). Fails with `proxy_not_running` if
    /// nothing is currently running, rather than picking a stale/bogus port.
    pub async fn list_connections(&self) -> AppResult<ConnectionsSnapshot> {
        let port = self.clash_api_port().await?;
        clash_api::get_connections(port).await.map_err(|e| {
            AppError::new("clash_api_error", format!("failed to query connections: {e}"))
        })
    }

    /// Closes one connection by id (`DELETE /connections/{id}` — see
    /// `clash_api::close_connection`). Fails with `proxy_not_running` if
    /// nothing is currently running.
    pub async fn close_connection(&self, id: &str) -> AppResult<()> {
        let port = self.clash_api_port().await?;
        clash_api::close_connection(port, id).await.map_err(|e| {
            AppError::new("clash_api_error", format!("failed to close connection '{id}': {e}"))
        })
    }

    /// Closes every current connection (`DELETE /connections` — see
    /// `clash_api::close_all_connections`). Fails with `proxy_not_running` if
    /// nothing is currently running.
    pub async fn close_all_connections(&self) -> AppResult<()> {
        let port = self.clash_api_port().await?;
        clash_api::close_all_connections(port).await.map_err(|e| {
            AppError::new("clash_api_error", format!("failed to close all connections: {e}"))
        })
    }

    /// Shared lookup for the three Clash API methods above: the currently
    /// tracked run's `clash_api_port`, or `proxy_not_running` if nothing is
    /// running. Locks `self.running` only briefly (a `u16` copy), not held
    /// across the subsequent HTTP call.
    async fn clash_api_port(&self) -> AppResult<u16> {
        let guard = self.running.lock().await;
        guard
            .as_ref()
            .map(|running| running.clash_api_port)
            .ok_or_else(|| AppError::new("proxy_not_running", "the proxy is not currently running"))
    }

    /// Public, infallible variant of `clash_api_port` for callers that want
    /// to branch on "is a Clash API even up right now" themselves rather
    /// than getting an `AppError` (see `commands::dashboard::dashboard_open`,
    /// which needs the port to build the sing-box dashboard window's
    /// connection info and has its own `proxy_not_running` error message).
    /// `None` means nothing is currently running, same condition as
    /// `clash_api_port`'s `Err` case.
    pub async fn current_clash_api_port(&self) -> Option<u16> {
        let guard = self.running.lock().await;
        guard.as_ref().map(|running| running.clash_api_port)
    }

    /// Probes the built-in streaming/AI-service catalog (see
    /// `unlock::check_all`) through the current run's local `mixed` inbound
    /// port. Fails with `AppError{code:"proxy_not_running"}` if nothing is
    /// running, or if the current run has no local inbound to probe through
    /// at all (`Tun` mode -- see `ProxyStatus::local_port`'s doc comment) --
    /// the lock is dropped before the actual probing starts, so this doesn't
    /// block a concurrent `start()`/`stop()` for however long the probes
    /// take.
    pub async fn check_unlock(&self) -> AppResult<Vec<UnlockResult>> {
        let guard = self.running.lock().await;
        let port = match guard.as_ref() {
            None => {
                return Err(AppError::new("proxy_not_running", "Start the proxy to check unlock status"));
            }
            Some(running) => match running.local_port {
                Some(port) => port,
                None => {
                    return Err(AppError::new(
                        "proxy_not_running",
                        "Unlock status needs the proxy running in System proxy or Manual mode -- TUN mode has no local port to probe through",
                    ));
                }
            },
        };
        drop(guard);

        Ok(unlock::check_all(port).await)
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
            wireguard_private_key: None,
            wireguard_peer_public_key: None,
            wireguard_pre_shared_key: None,
            wireguard_local_address: None,
        }
    }

    #[tokio::test]
    async fn start_with_missing_binary_returns_core_start_failed() {
        let manager = CoreManager::with_binary_path("definitely-not-a-real-binary-xyz");
        let server = test_server();
        let err = manager
            .start(&server, ProxyModeType::SystemProxy, &[], &HashMap::new(), false, RuleOutbound::Proxy)
            .await
            .unwrap_err();
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
        let err = manager
            .start(&server, ProxyModeType::Manual, &[], &HashMap::new(), false, RuleOutbound::Proxy)
            .await
            .unwrap_err();
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
        let err = manager
            .start(&server, ProxyModeType::Tun, &[], &HashMap::new(), false, RuleOutbound::Proxy)
            .await
            .unwrap_err();
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
            .start(&server, ProxyModeType::SystemProxy, &[], &HashMap::new(), false, RuleOutbound::Proxy)
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
