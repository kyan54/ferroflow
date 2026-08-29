//! Windows privileged helper core: named-pipe accept loop (with SDDL ACL,
//! see `pipe_acl.rs`), shared-token auth, `Command` dispatch, and both the
//! foreground (`--console`) and SCM-registered (`windows-service` crate)
//! run modes.
//!
//! Reference for *behavior* (not wire format — see the module doc in
//! `helper_proto`): `helper-win/{service,winproc,selfuninstall}.go` in the
//! Electron repo.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::BufReader;
use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{Mutex, Notify};

use helper_proto::{endpoints, read_message, write_message, Command, Request, Response, PROTO_VERSION};

use crate::pipe_acl::{PipeSecurity, PIPE_SDDL};
use crate::winproc;

const MANAGED_CORE_DIR: &str = r"C:\ProgramData\FerroFlow\core";
const MANAGED_CORE_FILE: &str = "sing-box.exe";

/// One sing-box instance this helper started and is tracking.
struct ManagedChild {
    child: Child,
    pid: u32,
    core_path: String,
}

struct HelperState {
    token: String,
    child: Mutex<Option<ManagedChild>>,
    /// Remembers the last `core_path` a `Start` was issued with, purely so
    /// `Cleanup`/`FreePort` have something to image-path-match against
    /// even if this helper process was restarted and lost the `Child`
    /// handle (see `winproc::kill_all_matching_image`). Best-effort, not
    /// persisted across restarts.
    last_core_path: Mutex<Option<String>>,
}

// ---------------------------------------------------------------------
// Token loading + constant-time comparison
// ---------------------------------------------------------------------

fn load_token() -> Result<String> {
    let path = endpoints::WINDOWS_TOKEN_FILE;
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "reading helper token from {path} -- has the helper been installed yet? \
             run `ferroflow-helper-windows.exe --install` from an elevated shell first"
        )
    })?;
    Ok(raw.trim().to_string())
}

/// `--console` dev/test convenience: `--install` is the only path that's
/// supposed to create the token (with a real SYSTEM+Administrators ACL,
/// see `install.rs::write_new_token`), but requiring an elevated
/// `--install` run before `--console` even starts would defeat the point
/// of having an unprivileged foreground dev mode. So: if the token is
/// already there (e.g. a real install), use it as-is; if not, generate an
/// unlocked one purely for this session. This token is **not** ACL-locked
/// down -- fine for local dev against a throwaway pipe, not how the
/// installed service behaves.
fn load_or_create_dev_token() -> Result<String> {
    match load_token() {
        Ok(token) => Ok(token),
        Err(_) => {
            let dir = Path::new(endpoints::WINDOWS_TOKEN_FILE)
                .parent()
                .context("computing token file parent dir")?;
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating {} for the dev token", dir.display()))?;
            let mut bytes = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
            let token = hex::encode(bytes);
            std::fs::write(endpoints::WINDOWS_TOKEN_FILE, &token).with_context(|| {
                format!("writing dev token to {}", endpoints::WINDOWS_TOKEN_FILE)
            })?;
            tracing::warn!(
                "no installed token found; generated an UNLOCKED dev-only token at {} \
                 (run --install for the real, ACL-restricted token)",
                endpoints::WINDOWS_TOKEN_FILE
            );
            Ok(token)
        }
    }
}

/// Constant-time-ish comparison: always walks the full length of the
/// shorter operand's worth of bytes... but since a length mismatch returns
/// immediately, this is *not* fully timing-attack-proof against
/// length-probing. That's an acceptable tradeoff here: the token is a
/// 256-bit random value delivered over a filesystem ACL'd to
/// SYSTEM+interactive-user, so the realistic threat this defends against
/// is a confused/buggy local client, not a remote timing adversary.
fn tokens_match(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------

async fn dispatch(state: &Arc<HelperState>, req: Request) -> Response {
    if !matches!(req.command, Command::Ping | Command::Version) {
        match &req.token {
            Some(t) if tokens_match(t, &state.token) => {}
            _ => return Response::err("unauthorized", "missing or invalid token"),
        }
    }

    match req.command {
        Command::Ping => Response::ok(json!({})),
        Command::Version => Response::ok(json!({ "version": PROTO_VERSION })),
        Command::Status => status(state).await,
        Command::Start { config_path, core_path } => start_core(state, config_path, core_path).await,
        Command::Stop => stop_core(state).await,
        Command::Cleanup => cleanup(state).await,
        Command::InstallCore { path, sha256 } => install_core(path, sha256).await,
        Command::FreePort { port } => free_port(state, port).await,
        Command::Uninstall => uninstall_cmd(state).await,
    }
}

async fn status(state: &Arc<HelperState>) -> Response {
    let mut guard = state.child.lock().await;
    let (running, pid, core_path) = match guard.as_mut() {
        Some(mc) => match mc.child.try_wait() {
            Ok(None) => (true, Some(mc.pid), Some(mc.core_path.clone())),
            Ok(Some(_)) | Err(_) => (false, None, None),
        },
        None => (false, None, None),
    };
    // The child exited on its own since the last check -- stop tracking it
    // so Start doesn't think one's already running.
    if !running {
        *guard = None;
    }
    Response::ok(json!({ "running": running, "pid": pid, "core_path": core_path }))
}

async fn start_core(state: &Arc<HelperState>, config_path: String, core_path: String) -> Response {
    let mut guard = state.child.lock().await;
    if let Some(mc) = guard.as_mut() {
        if matches!(mc.child.try_wait(), Ok(None)) {
            return Response::err(
                "already_running",
                format!("sing-box already running under this helper (pid {})", mc.pid),
            );
        }
    }

    // Fixed following cross-helper review (same guard added to
    // helper-macos/helper-linux): the request's `core_path` is
    // deliberately IGNORED and the binary spawned is always the locked
    // path `InstallCore` verified — never a path the caller names. A
    // compromised, still-token-holding app process must not be able to
    // ask this LocalSystem service to run an arbitrary binary.
    let _ = &core_path; // intentionally unused, kept only so the wire shape stays uniform
    let locked_core_path = Path::new(MANAGED_CORE_DIR).join(MANAGED_CORE_FILE);
    if !locked_core_path.is_file() {
        return Response::err(
            "core_not_installed",
            format!("no verified sing-box binary at '{}' — run InstallCore first", locked_core_path.display()),
        );
    }
    let locked_core_path_str = locked_core_path.to_string_lossy().to_string();

    let mut cmd = TokioCommand::new(&locked_core_path);
    cmd.args(["run", "-c", &config_path]);
    cmd.creation_flags(winproc::CHILD_CREATION_FLAGS);
    cmd.kill_on_drop(false);

    match cmd.spawn() {
        Ok(child) => {
            let pid = child.id().unwrap_or(0);
            winproc::assign_to_job(pid);
            *guard = Some(ManagedChild { child, pid, core_path: locked_core_path_str.clone() });
            *state.last_core_path.lock().await = Some(locked_core_path_str);
            Response::ok(json!({ "pid": pid }))
        }
        Err(e) => Response::err("core_start_failed", format!("failed to spawn {}: {e}", locked_core_path.display())),
    }
}

async fn stop_core(state: &Arc<HelperState>) -> Response {
    let mut guard = state.child.lock().await;
    match guard.take() {
        Some(mut mc) => {
            let _ = mc.child.start_kill();
            let _ = mc.child.wait().await;
            Response::ok(json!({ "stopped": true, "pid": mc.pid }))
        }
        None => Response::ok(json!({ "stopped": false, "reason": "not running" })),
    }
}

async fn cleanup(state: &Arc<HelperState>) -> Response {
    let mut guard = state.child.lock().await;
    let tracked_killed = if let Some(mut mc) = guard.take() {
        let _ = mc.child.start_kill();
        let _ = mc.child.wait().await;
        true
    } else {
        false
    };
    drop(guard);

    // Best-effort orphan sweep: catches a sing-box left behind by a
    // previous helper instance that crashed/was killed before it could
    // reap its own child (the Job Object safety net in `winproc.rs`
    // handles the "helper process itself dies" case at the kernel level;
    // this handles "helper restarted and no longer remembers the child").
    let hint = state.last_core_path.lock().await.clone();
    let orphans_killed = match hint.as_deref() {
        Some(path) => winproc::kill_all_matching_image(path),
        None => 0,
    };

    Response::ok(json!({
        "tracked_process_killed": tracked_killed,
        "orphans_killed": orphans_killed,
    }))
}

async fn install_core(path: String, expected_sha256: String) -> Response {
    let src = PathBuf::from(&path);
    let bytes = match tokio::fs::read(&src).await {
        Ok(b) => b,
        Err(e) => return Response::err("io_error", format!("failed reading {path}: {e}")),
    };

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(&expected_sha256) {
        return Response::err(
            "hash_mismatch",
            format!("refusing to install: expected sha256 {expected_sha256}, computed {actual}"),
        );
    }

    if let Err(e) = tokio::fs::create_dir_all(MANAGED_CORE_DIR).await {
        return Response::err("io_error", format!("failed creating {MANAGED_CORE_DIR}: {e}"));
    }
    let dest = Path::new(MANAGED_CORE_DIR).join(MANAGED_CORE_FILE);
    if let Err(e) = tokio::fs::write(&dest, &bytes).await {
        return Response::err("io_error", format!("failed writing {}: {e}", dest.display()));
    }

    Response::ok(json!({ "installed_path": dest.to_string_lossy(), "sha256": actual }))
}

async fn free_port(state: &Arc<HelperState>, port: u16) -> Response {
    let pids = match winproc::listen_pids_for_port(port) {
        Ok(p) => p,
        Err(e) => return Response::err("lookup_failed", format!("GetExtendedTcpTable failed: {e}")),
    };
    if pids.is_empty() {
        return Response::ok(json!({ "was_in_use": false, "listeners": Vec::<u32>::new(), "killed_pids": Vec::<u32>::new() }));
    }

    let hint = state.last_core_path.lock().await.clone();
    let tracked_pid = state.child.lock().await.as_ref().map(|mc| mc.pid);

    // Only ever kill a listener that is either the sing-box we're
    // currently tracking, or matches the image path of the last core we
    // started -- never an arbitrary process a client points us at just by
    // naming a port. If some *other* process holds the port, we report it
    // but leave it alone; the caller (core-manager) decides what to do
    // with an unrecognized occupant.
    let mut killed = Vec::new();
    for &pid in &pids {
        let is_ours = tracked_pid == Some(pid)
            || hint.as_deref().is_some_and(|h| winproc::image_matches(pid, h));
        if is_ours && winproc::terminate_pid(pid).is_ok() {
            killed.push(pid);
        }
    }

    if let Some(tp) = tracked_pid {
        if killed.contains(&tp) {
            *state.child.lock().await = None;
        }
    }

    Response::ok(json!({
        "was_in_use": true,
        "listeners": pids,
        "killed_pids": killed,
    }))
}

async fn uninstall_cmd(state: &Arc<HelperState>) -> Response {
    let _ = stop_core(state).await;
    let _ = std::fs::remove_file(endpoints::WINDOWS_TOKEN_FILE);
    let _ = std::fs::remove_dir_all(MANAGED_CORE_DIR);

    // TODO(selfuninstall): a running service can't delete its own on-disk
    // .exe (the file is locked) or reliably `sc delete` itself out from
    // under the SCM while it's the process servicing that exact call. The
    // Go helper solves this with a detached sidecar (`spawnSelfUninstall`
    // in `helper-win/selfuninstall.go`): a `DETACHED_PROCESS` cmd.exe that
    // outlives this process, waits for it to exit, then runs
    // `sc stop`/`sc delete` and removes the remaining files -- with a
    // careful hand-rolled command line (see `selfuninstall.go`'s comment
    // block) because `exec.Command`'s default argument-escaping mangles
    // the quoting `cmd.exe`'s `/c` parsing needs. Port that sidecar-spawn
    // approach here before wiring this command up for real end users; the
    // separate elevated `--uninstall` path in `install.rs` already does
    // the full job (it isn't self-locked, so it can `sc delete` directly)
    // and is the recommended uninstall path for this pass.
    Response::ok(json!({
        "stopped": true,
        "token_and_core_removed": true,
        "service_removed": false,
        "note": "service binary/SCM entry not removed by this command yet -- use --uninstall instead; see TODO in service.rs",
    }))
}

// ---------------------------------------------------------------------
// Connection handling
// ---------------------------------------------------------------------

async fn handle_connection(pipe: NamedPipeServer, state: Arc<HelperState>) -> Result<()> {
    let (rd, mut wr) = tokio::io::split(pipe);
    let mut reader = BufReader::new(rd);
    loop {
        let req: Request = match read_message(&mut reader).await {
            Ok(req) => req,
            Err(helper_proto::ProtoError::ConnectionClosed) => return Ok(()),
            Err(e) => {
                tracing::debug!("dropping connection after protocol error: {e}");
                return Ok(());
            }
        };
        let resp = dispatch(&state, req).await;
        if let Err(e) = write_message(&mut wr, &resp).await {
            tracing::debug!("failed writing response: {e}");
            return Ok(());
        }
    }
}

fn create_pipe_instance(first: bool) -> Result<NamedPipeServer> {
    let mut opts = ServerOptions::new();
    opts.pipe_mode(PipeMode::Byte);
    opts.first_pipe_instance(first);

    if first {
        let mut security = PipeSecurity::from_sddl(PIPE_SDDL)
            .context("converting pipe SDDL to a security descriptor")?;
        // SAFETY: `security` outlives this call, and its `as_raw_ptr()`
        // points at a fully-initialized `SECURITY_ATTRIBUTES` whose
        // `lpSecurityDescriptor` is a valid descriptor for the duration of
        // the syscall inside `create_with_security_attributes_raw`.
        unsafe {
            Ok(opts.create_with_security_attributes_raw(endpoints::WINDOWS_PIPE, security.as_raw_ptr())?)
        }
    } else {
        // Only the first instance may (or needs to) set the ACL -- later
        // instances of the same pipe name inherit it automatically.
        Ok(opts.create(endpoints::WINDOWS_PIPE)?)
    }
}

async fn accept_loop(state: Arc<HelperState>, shutdown: Arc<Notify>) -> Result<()> {
    let mut server = create_pipe_instance(true)?;
    tracing::info!(
        "FerroFlow helper listening on {}, proto v{}",
        endpoints::WINDOWS_PIPE,
        PROTO_VERSION
    );

    loop {
        tokio::select! {
            res = server.connect() => {
                res.context("named pipe connect failed")?;
                let connected = std::mem::replace(&mut server, create_pipe_instance(false)?);
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(connected, state).await {
                        tracing::warn!("connection handler error: {e}");
                    }
                });
            }
            _ = shutdown.notified() => {
                tracing::info!("shutdown requested, stopping pipe accept loop");
                break;
            }
        }
    }
    Ok(())
}

async fn reap_tracked_child(state: &Arc<HelperState>) {
    let mut guard = state.child.lock().await;
    if let Some(mut mc) = guard.take() {
        tracing::info!("reaping tracked sing-box child (pid {}) on shutdown", mc.pid);
        let _ = mc.child.start_kill();
        let _ = mc.child.wait().await;
    }
}

fn state_with_token(token: String) -> Arc<HelperState> {
    Arc::new(HelperState {
        token,
        child: Mutex::new(None),
        last_core_path: Mutex::new(None),
    })
}

/// Strict variant used by the SCM-registered service: the token must
/// already exist (from a prior `--install`) or this fails outright, since
/// a silently-generated token there would mean the real, ACL-restricted
/// service is running unauthenticated.
fn build_state() -> Result<Arc<HelperState>> {
    Ok(state_with_token(load_token()?))
}

// ---------------------------------------------------------------------
// Console (foreground, dev/test) mode
// ---------------------------------------------------------------------

pub async fn run_console() -> Result<()> {
    let state = state_with_token(load_or_create_dev_token()?);
    let shutdown = Arc::new(Notify::new());
    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            tracing::info!("ctrl-c received");
            shutdown_signal.notify_one();
        }
    });

    let result = accept_loop(state.clone(), shutdown).await;
    reap_tracked_child(&state).await;
    result
}

// ---------------------------------------------------------------------
// SCM (Windows Service) mode
// ---------------------------------------------------------------------

use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

windows_service::define_windows_service!(ffi_service_main, service_main);

pub fn run_as_service() -> Result<()> {
    service_dispatcher::start(crate::install::SERVICE_NAME, ffi_service_main).context(
        "failed to start the service control dispatcher -- if you're running this \
         interactively, use --console instead (SCM dispatch only works when actually \
         launched by the Service Control Manager)",
    )
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service_inner() {
        tracing::error!("helper-windows service run failed: {e}");
    }
}

fn run_service_inner() -> Result<()> {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(crate::install::SERVICE_NAME, event_handler)
        .context("registering service control handler")?;

    let report = |state: ServiceState, accepted: ServiceControlAccept, exit_code: ServiceExitCode| {
        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: state,
            controls_accepted: accepted,
            exit_code,
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
    };

    report(ServiceState::StartPending, ServiceControlAccept::empty(), ServiceExitCode::Win32(0))
        .context("reporting StartPending")?;

    let state = match build_state() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to initialize helper state: {e}");
            let _ = report(ServiceState::Stopped, ServiceControlAccept::empty(), ServiceExitCode::ServiceSpecific(1));
            return Err(e);
        }
    };

    let rt = tokio::runtime::Runtime::new().context("building tokio runtime")?;
    let shutdown = Arc::new(Notify::new());
    let shutdown_bridge = shutdown.clone();
    // Bridges the synchronous SCM control-handler callback (which fires on
    // an arbitrary system thread, not inside our tokio runtime) to the
    // async accept loop's shutdown signal.
    std::thread::spawn(move || {
        let _ = stop_rx.recv();
        shutdown_bridge.notify_one();
    });

    report(ServiceState::Running, ServiceControlAccept::STOP, ServiceExitCode::Win32(0))
        .context("reporting Running")?;

    let run_result = rt.block_on(accept_loop(state.clone(), shutdown));
    // Reap the tracked child *before* reporting Stopped -- otherwise, if
    // the SCM (or a watchdog) hard-kills this process right after seeing
    // Stopped, a sing-box started under LocalSystem is left running with
    // no owner. Mirrors the Go service's Execute() calling
    // reapChildOnExit() before closing the listener.
    rt.block_on(reap_tracked_child(&state));

    let exit_code = match &run_result {
        Ok(()) => ServiceExitCode::Win32(0),
        Err(_) => ServiceExitCode::ServiceSpecific(1),
    };
    report(ServiceState::Stopped, ServiceControlAccept::empty(), exit_code).context("reporting Stopped")?;

    run_result
}
