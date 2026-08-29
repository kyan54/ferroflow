//! macOS privileged helper daemon: runs as a root `launchd` LaunchDaemon
//! (see `install.rs` for how it gets installed), listens on a Unix domain
//! socket (`helper_proto::endpoints::MACOS_SOCKET`), authenticates every
//! request except `Ping`/`Version` against a shared token file
//! (`MACOS_TOKEN_FILE`), and is the only thing on the box allowed to
//! start/stop the managed sing-box binary.
//!
//! Behavior reference: `FlowZ/helper/helper.go` and
//! `FlowZ/src/main/services/HelperManager.ts` in the sibling Electron repo
//! (same responsibilities — root daemon, one-time elevated install, then
//! zero-prompt socket control — but this is a from-scratch design against
//! `helper_proto`'s NDJSON wire format, not a port of the Go line protocol).
//!
//! CAVEAT (read before trusting this file): written and reviewed on a
//! Windows dev machine with no macOS available, so none of this has been
//! compiled or run for real. `#[cfg(target_os = "macos")]` keeps it out of
//! the non-mac workspace build entirely; first real exercise is expected to
//! be a macOS CI runner. Specific unverified spots are called out inline
//! (search this crate for "UNVERIFIED").

#![cfg(target_os = "macos")]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use helper_proto::{endpoints, read_message, write_message, Command, Request, Response, PROTO_VERSION};
use nix::errno::Errno;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command as ChildCommand;
use tokio::sync::Mutex;

use crate::paths::{self, PLIST_PATH};

/// How long `Stop`/`Uninstall` wait for a clean exit after SIGTERM before
/// escalating to SIGKILL. Comfortably fits inside launchd's own default
/// ~20s SIGTERM-to-SIGKILL grace window for the case where *we* are the one
/// being `bootout`'d (see `spawn_signal_reaper`), while still giving
/// sing-box a real chance to tear down TUN/routes cleanly instead of
/// dropping them.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// One sing-box instance we started and are responsible for reaping.
///
/// Deliberately just a bare pid, not the `tokio::process::Child` handle
/// itself: the handle is instead moved into the reaper task spawned by
/// `handle_start` and lives there for the child's whole lifetime, so
/// `.wait()` (which needs `&mut Child`) never has to fight anyone else for
/// access. Everything else (`Stop`/`Cleanup`/`FreePort`/liveness checks)
/// only ever needs to signal or query by pid, which needs no ownership of
/// the `Child` at all — see `process_alive`.
struct ManagedProcess {
    pid: u32,
}

type SharedState = Arc<Mutex<Option<ManagedProcess>>>;

pub async fn run() -> anyhow::Result<()> {
    let dir = paths::support_dir();
    std::fs::create_dir_all(dir)?;
    // `create_dir_all` doesn't change permissions on a directory that
    // already existed from a previous install; this directory must stay
    // traversable (0755) by the unprivileged app even though everything
    // created *under* it — the token file, in particular — sets its own,
    // stricter permissions explicitly.
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))?;

    let socket_path = Path::new(endpoints::MACOS_SOCKET);
    // A previous run's socket file left on disk (crash, or a `launchd`
    // relaunch racing a slow `bootout`) would otherwise make `bind` fail
    // with `AddrInUse`.
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    // 0666: per `helper_proto`'s module doc comment, the shared token is
    // the real trust boundary here, not filesystem permissions on the
    // socket — any local process can open it, but only one holding a valid
    // token gets anything past `Ping`/`Version` out of it.
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o666))?;

    let state: SharedState = Arc::new(Mutex::new(None));

    spawn_signal_reaper(state.clone());

    tracing::info!("helper-macos listening on {}", socket_path.display());
    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!("accept failed: {err}");
                continue;
            }
        };
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_conn(stream, state).await {
                tracing::debug!("connection closed with error: {err}");
            }
        });
    }
}

/// Installs a SIGTERM/SIGINT handler and returns immediately — the actual
/// wait-and-reap happens in the task it spawns.
///
/// Why this exists: `launchd` sends SIGTERM (then SIGKILL ~20s later if the
/// process hasn't exited) both on `launchctl bootout` and on system
/// shutdown. Without this handler, a bootout or shutdown would leave a
/// running root sing-box behind as an orphan, still holding onto the TUN
/// device and any bound ports. Mirrors the v3 signal reaper in
/// `helper.go`.
fn spawn_signal_reaper(state: SharedState) {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!("failed to install SIGTERM handler: {err}");
                return;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(err) => {
                tracing::error!("failed to install SIGINT handler: {err}");
                return;
            }
        };

        tokio::select! {
            _ = sigterm.recv() => tracing::info!("received SIGTERM, reaping managed child before exit"),
            _ = sigint.recv() => tracing::info!("received SIGINT, reaping managed child before exit"),
        }

        if let Some(managed) = state.lock().await.take() {
            terminate(managed).await;
        }
        std::process::exit(0);
    });
}

async fn handle_conn(stream: UnixStream, state: SharedState) -> Result<(), helper_proto::ProtoError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let request: Request = read_message(&mut reader).await?;
    let response = dispatch(request, &state).await;
    write_message(&mut write_half, &response).await
}

async fn dispatch(request: Request, state: &SharedState) -> Response {
    let Request { token, command } = request;

    // `Ping`/`Version` are the two documented exceptions in
    // `helper_proto::Request` — everything else needs a valid token.
    if !matches!(command, Command::Ping | Command::Version) {
        match check_token(token.as_deref()).await {
            Ok(true) => {}
            Ok(false) => return Response::err("auth", "missing or invalid token"),
            Err(err) => {
                tracing::error!("failed to read token file: {err}");
                return Response::err("auth", "helper token file unreadable");
            }
        }
    }

    match command {
        Command::Ping => Response::ok(json!({ "pong": true })),
        Command::Version => Response::ok(json!({ "version": PROTO_VERSION })),
        Command::Status => handle_status(state).await,
        Command::Start { config_path, core_path } => handle_start(state, config_path, core_path).await,
        Command::Stop => handle_stop(state).await,
        Command::Cleanup => handle_cleanup(state).await,
        Command::InstallCore { path, sha256 } => handle_install_core(path, sha256).await,
        Command::FreePort { port } => handle_free_port(port).await,
        Command::Uninstall => handle_uninstall(state).await,
    }
}

/// Re-reads the token file on every request instead of caching it in
/// memory (and instead of a cache + filesystem-watch scheme). Simplicity
/// over cleverness: the file is a few dozen bytes on local disk, this
/// daemon is not on anyone's hot path, and re-reading means a token
/// rotation (reinstall/repair) takes effect on the very next request with
/// no explicit cache-invalidation step to get wrong. Compared in constant
/// time so response timing can't be used as a guessing oracle.
async fn check_token(supplied: Option<&str>) -> std::io::Result<bool> {
    let supplied = match supplied {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(false),
    };
    let stored = match tokio::fs::read_to_string(endpoints::MACOS_TOKEN_FILE).await {
        Ok(s) => s,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    Ok(constant_time_eq(supplied.trim().as_bytes(), stored.trim().as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn handle_status(state: &SharedState) -> Response {
    match state.lock().await.as_ref() {
        Some(managed) => Response::ok(json!({
            "running": process_alive(managed.pid),
            "pid": managed.pid,
        })),
        None => Response::ok(json!({ "running": false })),
    }
}

/// `kill(pid, 0)` sends no signal but still validates that the pid exists
/// and is signalable by us — the standard Unix idiom for "is this process
/// alive". Used instead of `Child::try_wait()` because the `Child` handle
/// itself lives inside the reaper task spawned by `handle_start`, not in
/// `SharedState` (see `ManagedProcess`'s doc comment for why).
fn process_alive(pid: u32) -> bool {
    signal::kill(Pid::from_raw(pid as i32), None::<Signal>).is_ok()
}

async fn handle_start(state: &SharedState, config_path: String, core_path: String) -> Response {
    let mut guard = state.lock().await;

    if let Some(managed) = guard.as_ref() {
        if process_alive(managed.pid) {
            // Mirrors helper.go's idempotent "OK already <pid>": calling
            // Start twice in a row (e.g. a client retry) isn't an error.
            return Response::ok(json!({ "started": true, "pid": managed.pid, "already_running": true }));
        }
        // Tracked pid is gone — the reaper task just hasn't caught up to
        // clear `state` yet, or it died between requests. Fall through and
        // start a fresh one.
    }

    // Fixed following review, to match helper-linux's guard: the
    // request's `core_path` is deliberately IGNORED (kept in the wire
    // shape only so `helper_proto::Command::Start` stays one shape across
    // all three helpers) and the binary to run is always the locked path
    // `InstallCore` verified and wrote — never a path the caller names.
    // Without this, a compromised-but-still-token-holding app process
    // could ask this root daemon to spawn an arbitrary binary as root.
    // `config_path` stays caller-supplied: it's a data file consumed by a
    // fixed, already-trusted binary, not an executable, so it isn't a
    // privilege-escalation vector the way `core_path` was.
    let _ = &core_path; // intentionally unused — see comment above
    let locked_core_path = paths::core_binary_path();
    if !locked_core_path.is_file() {
        return Response::err(
            "core_not_installed",
            format!(
                "no verified sing-box binary at '{}' — run InstallCore first",
                locked_core_path.display()
            ),
        );
    }

    let mut command = ChildCommand::new(&locked_core_path);
    command
        .arg("run")
        .arg("-c")
        .arg(&config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return Response::err(
                "start",
                format!(
                    "failed to spawn '{} run -c {config_path}': {err}",
                    locked_core_path.display()
                ),
            )
        }
    };

    let pid = match child.id() {
        Some(pid) => pid,
        None => return Response::err("start", "child process exited before it could be tracked"),
    };

    *guard = Some(ManagedProcess { pid });
    drop(guard); // release the lock before spawning the reaper task below

    let reaper_state = state.clone();
    tokio::spawn(async move {
        // Owns the `Child` for its whole remaining lifetime so `.wait()`
        // can reap it without ever needing exclusive access to
        // `SharedState` — see `ManagedProcess`'s doc comment.
        match child.wait().await {
            Ok(status) => tracing::info!("managed sing-box (pid {pid}) exited: {status}"),
            Err(err) => tracing::warn!("failed to wait for sing-box (pid {pid}): {err}"),
        }
        let mut guard = reaper_state.lock().await;
        if matches!(guard.as_ref(), Some(p) if p.pid == pid) {
            *guard = None;
        }
    });

    Response::ok(json!({ "started": true, "pid": pid, "already_running": false }))
}

async fn handle_stop(state: &SharedState) -> Response {
    match state.lock().await.take() {
        Some(managed) => {
            let pid = managed.pid;
            // Backgrounded rather than awaited here: the grace period in
            // `terminate` can take up to `STOP_GRACE`, and blocking this
            // reply that long (or holding `state`'s lock that long) would
            // make `Stop` needlessly stall out concurrent `Ping`/`Status`
            // calls. The state slot is already cleared via `take()` above,
            // so a `Start` racing right behind this `Stop` can't collide
            // with the process being torn down.
            tokio::spawn(terminate(managed));
            Response::ok(json!({ "stopped": true, "pid": pid }))
        }
        None => Response::ok(json!({ "stopped": false, "was_running": false })),
    }
}

/// SIGTERM, then poll for up to `STOP_GRACE` before escalating to SIGKILL.
/// Gives sing-box a chance to tear down its TUN device/routes cleanly
/// instead of leaving them dangling. Mirrors `terminateChild` in
/// `helper.go`, minus its `childDone` channel — polling `process_alive`
/// every 200ms is simpler to reason about with no macOS box to verify
/// against, and at a 5-second timescale is no less correct.
async fn terminate(managed: ManagedProcess) {
    let pid = Pid::from_raw(managed.pid as i32);
    if let Err(err) = signal::kill(pid, Signal::SIGTERM) {
        if err != Errno::ESRCH {
            tracing::warn!("SIGTERM to pid {} failed: {err}", managed.pid);
        }
        return; // already gone (ESRCH), or an error SIGKILL would hit too
    }

    let deadline = tokio::time::Instant::now() + STOP_GRACE;
    while tokio::time::Instant::now() < deadline {
        if !process_alive(managed.pid) {
            return; // exited on its own within the grace window
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    if process_alive(managed.pid) {
        tracing::warn!("pid {} still alive after {STOP_GRACE:?}, sending SIGKILL", managed.pid);
        let _ = signal::kill(pid, Signal::SIGKILL);
    }
}

async fn handle_cleanup(state: &SharedState) -> Response {
    if let Some(managed) = state.lock().await.take() {
        // Cleanup means "make sure nothing is left running", not a graceful
        // shutdown — skip straight to SIGKILL rather than `terminate`'s
        // SIGTERM-then-grace-period dance.
        let _ = signal::kill(Pid::from_raw(managed.pid as i32), Signal::SIGKILL);
    }

    // Best-effort sweep for orphans from a *previous* run of this daemon —
    // e.g. it was `kill -9`'d directly rather than reaped via the SIGTERM
    // handler in `run()`, so whatever it was managing never got cleared
    // (that in-memory state died with the old process). helper.go can
    // `pkill -f "<exact locked-in path> run"` because its sing-box path is
    // fixed at install time; this protocol hands `core_path` over
    // per-request instead (see `handle_start`'s doc comment), so there's no
    // single fixed path to match on here. This matches on the binary name
    // only, which is coarser — it could in principle kill an unrelated
    // process that happens to be named `sing-box` and invoked with a `run`
    // argument — but matches the task brief's explicit "best-effort"
    // framing for this pass.
    let pkill = tokio::task::spawn_blocking(|| {
        std::process::Command::new("/usr/bin/pkill")
            .args(["-9", "-f", "sing-box run"])
            .output()
    })
    .await;
    if let Err(err) = pkill {
        tracing::warn!("cleanup pkill task panicked: {err}");
    }

    // TODO (phase 2 / `net` crate territory per the task brief): undo any
    // route/DNS/system-proxy state a crashed sing-box left behind. Not
    // attempted here.
    Response::ok(json!({ "cleaned": true }))
}

async fn handle_install_core(path: String, expected_sha256: String) -> Response {
    let data = match tokio::fs::read(&path).await {
        Ok(data) => data,
        Err(err) => return Response::err("install_core", format!("failed to read {path}: {err}")),
    };

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual_sha256 = hex_encode(&hasher.finalize());
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256.trim()) {
        return Response::err(
            "install_core",
            format!("sha256 mismatch: expected {expected_sha256}, got {actual_sha256}"),
        );
    }

    let dest_dir = paths::core_dir();
    if let Err(err) = tokio::fs::create_dir_all(&dest_dir).await {
        return Response::err("install_core", format!("failed to create {}: {err}", dest_dir.display()));
    }
    if let Err(err) = tokio::fs::set_permissions(&dest_dir, std::fs::Permissions::from_mode(0o755)).await {
        tracing::warn!("failed to chmod {}: {err}", dest_dir.display());
    }

    let dest = paths::core_binary_path();
    let tmp = dest.with_extension("new");
    if let Err(err) = tokio::fs::write(&tmp, &data).await {
        return Response::err("install_core", format!("failed to write {}: {err}", tmp.display()));
    }
    if let Err(err) = tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Response::err("install_core", format!("failed to chmod {}: {err}", tmp.display()));
    }
    // Rename, not copy-then-delete: atomic on the same filesystem, so a
    // concurrent reader can never observe a partially-written `sing-box`.
    if let Err(err) = tokio::fs::rename(&tmp, &dest).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Response::err("install_core", format!("failed to install to {}: {err}", dest.display()));
    }

    // Gatekeeper SIGKILLs an unsigned/quarantined binary the moment it's
    // exec'd if it still carries the com.apple.quarantine xattr from
    // wherever the app downloaded/staged it — clear that and ad-hoc
    // re-sign, matching `installCore` in `helper.go`. Both are external
    // tools, not Rust code, so run them off the async reactor thread.
    let dest_for_blocking = dest.clone();
    let sign_result = tokio::task::spawn_blocking(move || {
        let xattr = std::process::Command::new("/usr/bin/xattr")
            .arg("-cr")
            .arg(&dest_for_blocking)
            .output();
        let codesign = std::process::Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(&dest_for_blocking)
            .output();
        (xattr, codesign)
    })
    .await;

    match sign_result {
        Ok((xattr_res, codesign_res)) => {
            log_tool_result("xattr -cr", xattr_res);
            log_tool_result("codesign", codesign_res);
        }
        Err(err) => tracing::warn!("xattr/codesign blocking task panicked: {err}"),
    }

    Response::ok(json!({ "installed": true, "path": dest.to_string_lossy() }))
}

fn log_tool_result(name: &str, result: std::io::Result<std::process::Output>) {
    match result {
        Ok(output) if !output.status.success() => {
            tracing::warn!("{name} exited with {}: {}", output.status, String::from_utf8_lossy(&output.stderr));
        }
        Ok(_) => {}
        Err(err) => tracing::warn!("failed to launch {name}: {err}"),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

async fn handle_free_port(port: u16) -> Response {
    let lsof = tokio::task::spawn_blocking(move || {
        // Chained `.arg()` calls rather than a single `.args([...])` array:
        // the array-literal form needs every element to be the exact same
        // type, and "-ti"/"-sTCP:LISTEN" (`&str`) don't unify with
        // `format!(...)` (`String`) the way `.arg()`'s per-call generic
        // `impl AsRef<OsStr>` does.
        std::process::Command::new("/usr/sbin/lsof")
            .arg("-ti")
            .arg(format!("tcp:{port}"))
            .arg("-sTCP:LISTEN")
            .output()
    })
    .await;

    let output = match lsof {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => return Response::err("free_port", format!("failed to run lsof: {err}")),
        Err(err) => return Response::err("free_port", format!("lsof task panicked: {err}")),
    };

    // lsof exits non-zero when nothing matches the filter — that's "the
    // port is already free", not an error, so don't gate on
    // `output.status` here, only on whether it printed any pids.
    let pids: Vec<i32> = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();

    if pids.is_empty() {
        return Response::ok(json!({ "freed": true, "already_free": true }));
    }

    // Kills whatever's listening unconditionally. Deliberately simpler than
    // helper.go's `freeport`, which shells out to `ps -o comm=` first and
    // only kills a pid whose command name contains "sing-box", reporting
    // anything else back by name instead of touching it. The task brief
    // for this pass explicitly calls for the simpler version; if that
    // safety check turns out to matter in practice, port it over as a
    // follow-up rather than assuming this pass silently already has it.
    let mut killed = Vec::new();
    let mut failed = Vec::new();
    for pid in pids {
        match signal::kill(Pid::from_raw(pid), Signal::SIGKILL) {
            Ok(()) => killed.push(pid),
            Err(Errno::ESRCH) => {} // already gone between lsof and here
            Err(err) => failed.push(format!("pid {pid}: {err}")),
        }
    }

    if failed.is_empty() {
        Response::ok(json!({ "freed": true, "already_free": false, "killed_pids": killed }))
    } else {
        Response::err("free_port", format!("killed {killed:?}; failed to kill: {}", failed.join(", ")))
    }
}

async fn handle_uninstall(state: &SharedState) -> Response {
    if let Some(managed) = state.lock().await.take() {
        terminate(managed).await;
    }

    for path in [endpoints::MACOS_SOCKET, endpoints::MACOS_TOKEN_FILE] {
        remove_best_effort(Path::new(path));
    }
    remove_best_effort(Path::new(PLIST_PATH));

    // `launchctl bootout` on our own service label sends this very process
    // SIGTERM — almost certainly before this async fn's caller (`dispatch`
    // -> `handle_conn`) has finished flushing the `Ok` response below out
    // to the socket, if called inline here. Deferred to a detached task
    // with a short delay instead, so the response has time to actually
    // leave this connection's write buffer first.
    //
    // UNVERIFIED: the exact timing of "bootout signals the running
    // instance" on a real launchd. The delay here is a conservative guess
    // (a local Unix-socket round trip should take microseconds, not
    // milliseconds) and has not been measured on a Mac. If a client
    // reports never seeing `Uninstall`'s response, or `cargo clippy`/CI
    // surfaces something about this task, this is the first place to look.
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let result = tokio::task::spawn_blocking(|| {
            std::process::Command::new("/bin/launchctl")
                .args(["bootout", "system", PLIST_PATH])
                .output()
        })
        .await;
        match result {
            Ok(res) => log_tool_result("launchctl bootout", res),
            Err(err) => tracing::warn!("launchctl bootout task panicked: {err}"),
        }
    });

    Response::ok(json!({ "uninstalled": true }))
}

fn remove_best_effort(path: &Path) {
    if let Err(err) = std::fs::remove_file(path) {
        if err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("failed to remove {}: {err}", path.display());
        }
    }
}
