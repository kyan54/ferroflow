//! The running root systemd service: Unix-socket accept loop,
//! `SO_PEERCRED`-based auth (via `tokio::net::UnixStream::peer_cred`, no
//! shared token — see `helper_proto` module docs), `Command` dispatch, and
//! the fork+setuid+ambient-capabilities dance that lets `sing-box` run as an
//! unprivileged user while still being able to build a TUN device / bind
//! privileged ports.
//!
//! Mirrors `helper-linux/helper.go` in the Electron original for
//! *behavior*; this is a from-scratch Rust design for the wire format (see
//! `helper_proto`), not a port of the Go line-protocol.
//!
//! # What is and isn't verified
//!
//! This compiles clean (`cargo check`/`build --target
//! x86_64-unknown-linux-gnu` both succeed through codegen, `cargo clippy` is
//! silent — only linking fails here, for lack of a cross-linker on this
//! Windows dev box) and every `nix`/`caps`/`tokio` API used was checked
//! against docs.rs / upstream source rather than recalled from memory. None
//! of that means it's *correct at runtime*: it has never executed against a
//! real kernel. See the crate's top-level report (delivered alongside this
//! change, not in this file) for the full list of syscall-ordering
//! assumptions. The single most important one, repeated here because it's
//! the one thing a reviewer must not skim past: the
//! fork→keepcaps→setuid→raise-ambient-capabilities sequence in
//! [`cmd_start`]'s `pre_exec` closure is modeled directly on Go's standard
//! library (`syscall/exec_linux.go`'s handling of `SysProcAttr{Credential,
//! AmbientCaps}`), which is the field-proven reference implementation for
//! this exact pattern — but it has not been independently re-verified
//! against the kernel here.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use caps::{CapSet, Capability};
use helper_proto::{endpoints, read_message, write_message, Command, Request, Response, PROTO_VERSION};
use nix::sys::signal::{self, Signal};
use nix::sys::prctl;
use nix::unistd::{Gid, Pid, Uid};
use serde_json::json;
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::Mutex;

use crate::paths::{CORE_BIN, CORE_DIR};

/// The three capabilities sing-box needs to build a TUN device, use raw
/// sockets, and bind low ports — as an otherwise-unprivileged process.
/// Matches the set the Electron original grants via `setcap` (see
/// `helper.go`'s comment referencing `PlatformPrivilegeService.ts:191`); not
/// picked fresh here.
const AMBIENT_CAPS: [Capability; 3] =
    [Capability::CAP_NET_ADMIN, Capability::CAP_NET_RAW, Capability::CAP_NET_BIND_SERVICE];

/// How long `Stop`/`Cleanup`/shutdown wait for a `SIGTERM`'d core to exit
/// before escalating to `SIGKILL`.
const TERMINATE_GRACE: Duration = Duration::from_secs(5);

#[derive(Default)]
struct ChildState {
    child: Option<Child>,
    pid: Option<u32>,
}

type SharedState = Arc<Mutex<ChildState>>;

pub async fn run() -> anyhow::Result<()> {
    tracing::info!("helper-linux starting (pid={})", std::process::id());

    let listener = bind_socket().context("binding helper socket")?;
    let state: SharedState = Arc::new(Mutex::new(ChildState::default()));

    spawn_shutdown_handler(state.clone());

    tracing::info!("listening on {}", endpoints::LINUX_SOCKET);
    loop {
        let (stream, _addr) = listener.accept().await.context("accepting connection")?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_conn(stream, state).await {
                tracing::warn!("connection handling error: {err:#}");
            }
        });
    }
}

/// Binds the Unix socket, creating `/run/ferroflow/` first if it doesn't
/// already exist (normally systemd's `RuntimeDirectory=` does this for us —
/// see `install.rs::build_unit` — but we don't want to depend on that if the
/// daemon is ever launched outside systemd, e.g. manual testing).
///
/// The socket itself is `0666`: unlike macOS/Windows, Linux auth is not
/// socket-permission-based at all — `SO_PEERCRED` (checked per-connection in
/// [`handle_conn`]) against the root-owned allow-list is the real trust
/// boundary, so there is no reason to restrict who can even *connect*.
fn bind_socket() -> anyhow::Result<UnixListener> {
    let sock_path = Path::new(endpoints::LINUX_SOCKET);
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
        // 0755 so any local user can traverse into the runtime dir to reach
        // the socket; the directory itself grants no access to anything.
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("chmod {}", parent.display()))?;
    }
    // Remove a stale socket inode from a previous (crashed / killed) run —
    // otherwise bind() fails with AddrInUse even though nothing is listening.
    let _ = std::fs::remove_file(sock_path);

    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("bind {}", sock_path.display()))?;
    std::fs::set_permissions(sock_path, std::fs::Permissions::from_mode(0o666))
        .with_context(|| format!("chmod {}", sock_path.display()))?;
    Ok(listener)
}

/// Installs SIGTERM/SIGINT handlers so `systemctl stop` (or Ctrl-C in
/// `--console`-style manual runs) reaps the managed core before the process
/// exits, rather than abandoning it as an orphan holding ambient
/// `CAP_NET_ADMIN`.
fn spawn_shutdown_handler(state: SharedState) {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install SIGTERM handler: {e}");
                return;
            }
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("failed to install SIGINT handler: {e}");
                return;
            }
        };
        tokio::select! {
            _ = sigterm.recv() => tracing::info!("received SIGTERM"),
            _ = sigint.recv() => tracing::info!("received SIGINT"),
        }
        let taken = {
            let mut guard = state.lock().await;
            (guard.pid.take(), guard.child.take())
        };
        if let (Some(pid), Some(mut child)) = taken {
            tracing::info!("terminating managed core (pid={pid}) before exit");
            terminate(pid, &mut child).await;
        }
        std::process::exit(0);
    });
}

/// One request per connection: read one `Request` line, authorize + dispatch
/// it, write one `Response` line, close. (Matches the Electron original's
/// socket protocol exactly — connect, write, read, disconnect — rather than
/// keeping a connection open for multiple commands.)
async fn handle_conn(stream: UnixStream, state: SharedState) -> anyhow::Result<()> {
    // SO_PEERCRED: the kernel captured the connecting process's credentials
    // at connect(2) time and they cannot be spoofed by the peer, which is
    // exactly why Linux doesn't need the shared-token auth macOS/Windows use.
    let peer = stream.peer_cred().context("reading SO_PEERCRED")?;
    let peer_uid = peer.uid();
    let peer_gid = peer.gid();

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let request: Request = match read_message(&mut reader).await {
        Ok(r) => r,
        Err(err) => {
            tracing::debug!("dropping connection with unreadable request: {err}");
            return Ok(());
        }
    };
    // `token` is always `None` on Linux (see `helper_proto::Request` docs);
    // deliberately ignored rather than matched on, so a client that still
    // sends one (e.g. shared client code ported from macOS) isn't rejected.
    let _ = request.token;

    // Ping/Version answer before the allow-list check, same as the Go
    // original — a caller needs to be able to probe "is the daemon even up"
    // without already being authorized. Written as if/else rather than a
    // `match` on `&request.command` so that the final branch can move
    // `request.command` into `handle_command` without fighting the borrow
    // checker over a scrutinee reference.
    let response = if matches!(request.command, Command::Ping) {
        Response::ok(json!({ "pong": true, "uid": peer_uid }))
    } else if matches!(request.command, Command::Version) {
        Response::ok(json!({ "version": PROTO_VERSION }))
    } else if !is_authorized(peer_uid) {
        Response::err("unauthorized", format!("uid {peer_uid} is not in the allow-list"))
    } else {
        handle_command(request.command, peer_uid, peer_gid, &state).await
    };

    write_message(&mut write_half, &response).await.context("writing response")?;
    Ok(())
}

/// Checks `uid` against `helper_proto::endpoints::LINUX_AUTHFILE` (one UID or
/// username per line; blank lines and `#`-comments ignored). `root` is
/// always authorized without consulting the file — mirrors the Go original,
/// and covers e.g. the app itself being run as root in some deployments.
fn is_authorized(uid: u32) -> bool {
    if uid == 0 {
        return true;
    }
    let Ok(data) = std::fs::read_to_string(endpoints::LINUX_AUTHFILE) else {
        return false;
    };
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Ok(listed) = line.parse::<u32>() {
            if listed == uid {
                return true;
            }
            continue;
        }
        // Not numeric: treat as a username (getpwnam(3) via `nix::unistd::User`).
        if let Ok(Some(user)) = nix::unistd::User::from_name(line) {
            if user.uid.as_raw() == uid {
                return true;
            }
        }
    }
    false
}

async fn handle_command(
    command: Command,
    peer_uid: u32,
    peer_gid: u32,
    state: &SharedState,
) -> Response {
    match command {
        Command::Ping | Command::Version => unreachable!("handled in handle_conn before auth"),
        Command::Status => cmd_status(state).await,
        Command::Start { config_path, core_path } => {
            cmd_start(state, peer_uid, peer_gid, config_path, core_path).await
        }
        Command::Stop => cmd_stop(state).await,
        Command::Cleanup => cmd_cleanup(state, peer_uid).await,
        Command::InstallCore { path, sha256 } => cmd_install_core(peer_uid, path, sha256).await,
        Command::FreePort { port } => cmd_free_port(peer_uid, port).await,
        Command::Uninstall => cmd_uninstall(state).await,
    }
}

// ---------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------

async fn cmd_status(state: &SharedState) -> Response {
    let mut guard = state.lock().await;
    let running = still_alive(&mut guard);
    let pid = if running { guard.pid } else { None };
    Response::ok(json!({ "running": running, "pid": pid }))
}

/// Reaps `state.child` if it has already exited (via the non-blocking
/// `try_wait`), clearing `state.pid`/`state.child` in that case, and returns
/// whether it's still running. Centralizing this means `Status` and `Start`'s
/// "already running?" check can never disagree about a process that died
/// between two requests.
fn still_alive(state: &mut ChildState) -> bool {
    let Some(child) = state.child.as_mut() else {
        return false;
    };
    match child.try_wait() {
        Ok(None) => true,
        Ok(Some(status)) => {
            tracing::info!("managed core (pid={:?}) exited: {status}", state.pid);
            state.child = None;
            state.pid = None;
            false
        }
        Err(err) => {
            tracing::warn!("try_wait failed, assuming core is gone: {err}");
            state.child = None;
            state.pid = None;
            false
        }
    }
}

// ---------------------------------------------------------------------
// Start — the privileged part of this crate.
// ---------------------------------------------------------------------

async fn cmd_start(
    state: &SharedState,
    peer_uid: u32,
    peer_gid: u32,
    config_path: String,
    core_path: String,
) -> Response {
    // --- Guard 1 (security-critical): the core binary path is locked. ---
    // The client only ever *names* the binary; it must resolve to exactly
    // the root-owned managed path below, or this call would be "please grant
    // CAP_NET_ADMIN/CAP_NET_RAW/CAP_NET_BIND_SERVICE, as an ambient
    // capability, to any file I like" — a one-line local privilege
    // escalation. Deliberately a strict string comparison rather than
    // `canonicalize()` + compare: `CORE_DIR` is root:root 0755, so a
    // non-root peer cannot plant a file or symlink inside it in the first
    // place, which makes following symlinks unnecessary and (more
    // importantly) avoids a TOCTOU window between resolving the path and
    // execing it.
    if core_path != CORE_BIN {
        return Response::err(
            "core-path-denied",
            format!("core_path must be the managed binary at {CORE_BIN}"),
        );
    }
    if let Err(err) = std::fs::metadata(CORE_BIN) {
        return Response::err("core-missing", format!("{CORE_BIN}: {err}"));
    }

    // --- Guard 2 (defense-in-depth, not itself security-critical): the ---
    // --- config must belong to the caller. ---
    // sing-box runs *as peer_uid* after the setuid below, so the kernel's
    // ordinary file-permission check already stops it from reading a config
    // it doesn't own; this just turns "helper says OK, sing-box silently
    // dies because it can't open its config" into an error message at the
    // point that's actually useful. A TOCTOU race here (symlink-swap
    // between this stat and sing-box's later open) doesn't grant an
    // attacker anything they couldn't already get by just... owning that
    // uid, so unlike Guard 1 this doesn't need fstat-after-open hardening.
    match std::fs::metadata(&config_path) {
        Ok(meta) if meta.uid() == peer_uid => {}
        Ok(_) => {
            return Response::err(
                "config-not-owned",
                "config_path is not owned by the calling user".to_string(),
            )
        }
        Err(err) => return Response::err("config-not-found", format!("{config_path}: {err}")),
    }

    // Resolved before taking the lock below: it's a handful of synchronous
    // file reads (`/etc/passwd`, `/etc/group`) that don't depend on
    // `ChildState` at all, so there's no reason to make `Status`/`Stop` wait
    // on them.
    let groups = supplementary_groups(peer_uid);

    let mut guard = state.lock().await;
    if still_alive(&mut guard) {
        // Idempotent, matches the Go original's "OK already <pid>": a client
        // that races a node-switch stop+start doesn't need this to be an error.
        return Response::ok(json!({ "pid": guard.pid, "already_running": true }));
    }

    let mut command = TokioCommand::new(CORE_BIN);
    command.arg("run").arg("-c").arg(&config_path);
    // `helper_proto::Command::Start` has no log-path field (unlike the
    // Electron original, which redirects to a caller-specified log file) —
    // stdout/stderr are left inherited from this daemon, which under
    // systemd means they land in the journal alongside the helper's own
    // `tracing` output. stdin is nulled since inheriting the daemon's own
    // stdin (itself `/dev/null` under systemd) has no legitimate use here.
    command.stdin(std::process::Stdio::null());

    // SAFETY / correctness: this closure runs in the forked child, in the
    // window between fork() and execve() that POSIX only guarantees
    // async-signal-safe behavior in (see `CommandExt::pre_exec`'s own safety
    // docs). We restrict it to raw setuid-family syscalls (via `nix`) and
    // the `caps` crate's capget/capset/prctl wrappers, and signal failure by
    // panicking rather than constructing an `io::Error` — `pre_exec`'s docs
    // specifically call out that `Error::new`/`Error::other` allocate and
    // aren't safe to rely on in this window, and that `Command::spawn` calls
    // `std::panic::always_abort()` *before* invoking this closure precisely
    // so that panicking here is the supported way to bail out: it aborts
    // this forked child immediately, before it can reach execve() in a
    // half-dropped-privilege state.
    //
    // Sequence (mirrors Go's `syscall/exec_linux.go` handling of
    // `SysProcAttr{Credential, AmbientCaps}` — see module docs for why that's
    // the reference this is modeled on):
    //
    //   1. prctl(PR_SET_KEEPCAPS, 1) — MUST happen before setuid, while still
    //      root. Without it, the kernel wipes the permitted+effective
    //      capability sets the instant the effective UID transitions from 0
    //      to nonzero, leaving nothing in Permitted for step 5 to promote.
    //   2. setgroups(supplementary groups of the target uid) — must happen
    //      before dropping uid/gid: changing our *own* supplementary groups
    //      needs CAP_SETGID, which step 4 gives up.
    //   3. setgid(target_gid), 4. setuid(target_uid) — gid before uid is
    //      deliberate: once the uid is nonzero, changing gid needs a
    //      capability we've just relinquished.
    //   5. caps::raise(Inheritable, {NET_ADMIN,NET_RAW,NET_BIND_SERVICE}) —
    //      ambient capabilities can never exceed Permitted ∩ Inheritable.
    //      KEEPCAPS preserved Permitted across the setuid in steps 3-4, but
    //      Inheritable still needs these three explicitly added — a
    //      capability can enter Inheritable only if it's currently in
    //      Permitted (true here, thanks to KEEPCAPS) and in Bounding (true
    //      as long as the unit doesn't set a restrictive
    //      `CapabilityBoundingSet=`, see `install.rs::build_unit`).
    //   6. caps::raise(Ambient, ...) — promotes the same three into Ambient,
    //      which is what actually survives the execve below. sing-box is an
    //      ordinary unprivileged binary (no file capabilities / setuid bit),
    //      so on exec its new Permitted+Effective sets become *exactly* this
    //      Ambient set — not the much larger Permitted set this
    //      still-technically-root-a-moment-ago process is holding at this
    //      point, which is what makes the whole scheme safe: that transient
    //      over-broad Permitted set never reaches the sing-box image.
    //
    // UNVERIFIED: this exact sequence has not been run against a real
    // kernel from this Rust code. It is believed correct because it mirrors
    // Go's proven implementation step-for-step, but every step here is a
    // candidate for extra review scrutiny — see the crate's final report.
    unsafe {
        command.pre_exec(move || {
            if prctl::set_keepcaps(true).is_err() {
                panic!("helper-linux: prctl(PR_SET_KEEPCAPS) failed");
            }
            if nix::unistd::setgroups(&groups).is_err() {
                panic!("helper-linux: setgroups failed");
            }
            if nix::unistd::setgid(Gid::from_raw(peer_gid)).is_err() {
                panic!("helper-linux: setgid failed");
            }
            if nix::unistd::setuid(Uid::from_raw(peer_uid)).is_err() {
                panic!("helper-linux: setuid failed");
            }
            for cap in AMBIENT_CAPS {
                if caps::raise(None, CapSet::Inheritable, cap).is_err() {
                    panic!("helper-linux: raising inheritable capability failed");
                }
            }
            for cap in AMBIENT_CAPS {
                if caps::raise(None, CapSet::Ambient, cap).is_err() {
                    panic!("helper-linux: raising ambient capability failed");
                }
            }
            Ok(())
        });
    }

    match command.spawn() {
        Ok(child) => {
            let Some(pid) = child.id() else {
                return Response::err("start-failed", "spawned child has no pid".to_string());
            };
            guard.child = Some(child);
            guard.pid = Some(pid);
            tracing::info!("started managed core: pid={pid} uid={peer_uid}");
            Response::ok(json!({ "pid": pid }))
        }
        Err(err) => Response::err("start-failed", err.to_string()),
    }
}

/// Best-effort supplementary-group lookup for `uid`, parsing `/etc/group`
/// directly (no NSS/SSSD/LDAP) — the same tradeoff the Go original documents
/// in `helper.go`'s `supplementaryGroups`. If this can't resolve anything we
/// fall back to an empty list (equivalent to `setgroups(&[])`), which is
/// never worse than the pre-existing `setcap`-based status quo that grants
/// no supplementary groups at all.
///
/// Without this, the forked child would silently keep root's supplementary
/// groups (or lose them entirely, depending on ordering) instead of the
/// login user's — potentially breaking access to group-gated resources like
/// a `ssl-cert` group certificate.
fn supplementary_groups(uid: u32) -> Vec<Gid> {
    let Ok(Some(user)) = nix::unistd::User::from_uid(Uid::from_raw(uid)) else {
        return Vec::new();
    };
    let mut gids = vec![user.gid]; // primary group first, matches Go's os/user.GroupIds()
    let Ok(data) = std::fs::read_to_string("/etc/group") else {
        return gids;
    };
    for line in data.lines() {
        let mut fields = line.splitn(4, ':');
        let (Some(_name), Some(_passwd), Some(gid_str), Some(members)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if members.split(',').any(|m| m == user.name) {
            if let Ok(raw_gid) = gid_str.parse::<u32>() {
                let gid = Gid::from_raw(raw_gid);
                if !gids.contains(&gid) {
                    gids.push(gid);
                }
            }
        }
    }
    gids
}

// ---------------------------------------------------------------------
// Stop / Cleanup
// ---------------------------------------------------------------------

async fn cmd_stop(state: &SharedState) -> Response {
    let taken = {
        let mut guard = state.lock().await;
        (guard.pid.take(), guard.child.take())
    };
    let (pid, mut child) = match taken {
        (Some(pid), Some(child)) => (pid, child),
        _ => return Response::ok(json!({ "stopped": false, "already_stopped": true })),
    };
    terminate(pid, &mut child).await;
    Response::ok(json!({ "stopped": true, "pid": pid }))
}

/// SIGTERM, then SIGKILL if it hasn't exited within `TERMINATE_GRACE`. Root
/// can signal a process regardless of which uid it's running as (the kernel
/// permission check for `kill(2)` allows it whenever the sender's real or
/// effective uid is 0, or matches the target's), which is why this works
/// even though the managed core dropped to `peer_uid` in [`cmd_start`].
async fn terminate(pid: u32, child: &mut Child) {
    let nix_pid = Pid::from_raw(pid as i32);
    if let Err(err) = signal::kill(nix_pid, Signal::SIGTERM) {
        tracing::debug!("SIGTERM to pid={pid} failed (already gone?): {err}");
    }
    if tokio::time::timeout(TERMINATE_GRACE, child.wait()).await.is_err() {
        tracing::warn!("pid={pid} didn't exit within {TERMINATE_GRACE:?}, sending SIGKILL");
        let _ = signal::kill(nix_pid, Signal::SIGKILL);
        let _ = child.wait().await;
    }
}

async fn cmd_cleanup(state: &SharedState, peer_uid: u32) -> Response {
    let taken = {
        let mut guard = state.lock().await;
        (guard.pid.take(), guard.child.take())
    };
    if let (Some(pid), Some(mut child)) = taken {
        terminate(pid, &mut child).await;
    }
    // Best-effort sweep for orphans this daemon no longer has in memory
    // (e.g. it restarted since a core was started) — scoped to the caller's
    // own uid (never touches another user's processes, mirroring the Go
    // original's cross-user-kill guard) and to the managed core binary path
    // specifically, so it can't accidentally reap an unrelated process.
    let _ = tokio::task::spawn_blocking(move || {
        // `.args([...])` needs one homogeneous element type, hence building
        // the uid string first rather than inlining `&peer_uid.to_string()`
        // (that would mix `&str` and `&String` in one array literal).
        let uid_arg = peer_uid.to_string();
        std::process::Command::new("pkill")
            .args(["-9", "-u", uid_arg.as_str(), "-f", CORE_BIN])
            .status()
    })
    .await;
    Response::ok(json!({ "cleaned": true }))
}

// ---------------------------------------------------------------------
// FreePort
// ---------------------------------------------------------------------

async fn cmd_free_port(peer_uid: u32, port: u16) -> Response {
    match tokio::task::spawn_blocking(move || free_port_blocking(peer_uid, port)).await {
        Ok(response) => response,
        Err(err) => Response::err("freeport-failed", err.to_string()),
    }
}

/// Finds whatever is `LISTEN`ing on `port` (via `ss`, matching the Go
/// original rather than hand-parsing `/proc/net/tcp` — `ss` already handles
/// IPv4/IPv6/dual-stack and is far less error-prone to get right) and, if
/// it's a `sing-box` process **owned by `caller_uid`**, kills it. Never
/// touches a process owned by a different uid — reports it as `foreign`
/// instead, so the caller can surface "something else is using this port"
/// rather than the helper silently killing an unrelated program.
fn free_port_blocking(caller_uid: u32, port: u16) -> Response {
    let sport_arg = format!("sport = :{port}");
    let output = std::process::Command::new("ss")
        .args(["-H", "-ltnp", sport_arg.as_str()])
        .output();
    let Ok(output) = output else {
        // `ss` not installed: best-effort "assume free" rather than failing
        // the whole request over a missing diagnostic tool.
        return Response::ok(json!({ "freed": true }));
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let pids = extract_pids(&text);
    if pids.is_empty() {
        return Response::ok(json!({ "freed": true }));
    }

    let mut killed = Vec::new();
    let mut foreign = Vec::new();
    for pid in pids {
        match proc_owner_uid(pid) {
            Some(uid) if uid == caller_uid => {
                let comm = proc_comm(pid).unwrap_or_default();
                if comm.contains("sing-box") {
                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
                    killed.push(pid);
                } else {
                    foreign.push(if comm.is_empty() { format!("pid:{pid}") } else { comm });
                }
            }
            _ => foreign.push(format!("pid:{pid}")),
        }
    }
    if !foreign.is_empty() {
        return Response::ok(json!({ "freed": false, "foreign": foreign.join(" | ") }));
    }
    Response::ok(json!({ "freed": true, "killed": killed }))
}

/// Scans `ss -p` output for every `pid=NNN` token. Hand-rolled instead of
/// pulling in the `regex` crate for one call site — `ss`'s `-p` field format
/// (`users:(("name",pid=NNN,fd=N))`) is stable enough that this is safe.
fn extract_pids(text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(idx) = rest.find("pid=") {
        rest = &rest[idx + "pid=".len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(pid) = digits.parse::<u32>() {
            if !out.contains(&pid) {
                out.push(pid);
            }
        }
    }
    out
}

fn proc_owner_uid(pid: u32) -> Option<u32> {
    std::fs::metadata(format!("/proc/{pid}")).ok().map(|m| m.uid())
}

fn proc_comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
}

// ---------------------------------------------------------------------
// InstallCore
// ---------------------------------------------------------------------

async fn cmd_install_core(peer_uid: u32, path: String, sha256_hex: String) -> Response {
    if sha256_hex.len() != 64 || !sha256_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Response::err("bad-args", "sha256 must be a 64-character hex digest".to_string());
    }
    match tokio::task::spawn_blocking(move || install_core_blocking(peer_uid, path, sha256_hex)).await {
        Ok(response) => response,
        Err(err) => Response::err("install-failed", err.to_string()),
    }
}

/// Hot-swaps the managed `sing-box` binary. Re-verifies the sha256 itself
/// (never trusts the caller's claim that `path` already matches
/// `want_hash`) so a compromised/confused unprivileged client can't smuggle
/// an arbitrary root-run binary into `CORE_BIN` — that hash check is the
/// second half of the same privilege-escalation guard `cmd_start`'s
/// core-path lock relies on: together they mean the only way `CORE_BIN` ever
/// changes is "someone supplied bytes that hash to a value the caller
/// asserted in advance", not "someone pointed us at a file".
fn install_core_blocking(peer_uid: u32, path: String, want_hash: String) -> Response {
    use sha2::{Digest, Sha256};

    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(err) => return Response::err("read-failed", format!("{path}: {err}")),
    };
    // Defense-in-depth, same reasoning as the config-ownership check in
    // `cmd_start`: not itself what stops a bad binary from landing in
    // `CORE_BIN` (the hash check below is), just an early, clearer error.
    if peer_uid != 0 && meta.uid() != peer_uid {
        return Response::err(
            "not-owned",
            "source binary is not owned by the calling user".to_string(),
        );
    }

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(err) => return Response::err("read-failed", format!("{path}: {err}")),
    };
    let got_hash = hex::encode(Sha256::digest(&data));
    if !got_hash.eq_ignore_ascii_case(&want_hash) {
        return Response::err(
            "hash-mismatch",
            format!("expected sha256 {want_hash}, got {got_hash}"),
        );
    }

    if let Err(err) = std::fs::create_dir_all(CORE_DIR) {
        return Response::err("install-failed", format!("mkdir {CORE_DIR}: {err}"));
    }
    let _ = std::fs::set_permissions(CORE_DIR, std::fs::Permissions::from_mode(0o755));
    let _ = nix::unistd::chown(CORE_DIR, Some(Uid::from_raw(0)), Some(Gid::from_raw(0)));

    // Atomic same-filesystem write-then-rename so a reader (or `Start`)
    // never observes a partially-written binary.
    let tmp_path = format!("{CORE_BIN}.new");
    if let Err(err) = std::fs::write(&tmp_path, &data) {
        return Response::err("install-failed", format!("write {tmp_path}: {err}"));
    }
    if let Err(err) = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755)) {
        let _ = std::fs::remove_file(&tmp_path);
        return Response::err("install-failed", format!("chmod {tmp_path}: {err}"));
    }
    let _ = nix::unistd::chown(tmp_path.as_str(), Some(Uid::from_raw(0)), Some(Gid::from_raw(0)));
    if let Err(err) = std::fs::rename(&tmp_path, CORE_BIN) {
        let _ = std::fs::remove_file(&tmp_path);
        return Response::err("install-failed", format!("rename to {CORE_BIN}: {err}"));
    }

    tracing::info!("installed core at {CORE_BIN} (sha256={got_hash})");
    Response::ok(json!({ "installed": true, "sha256": got_hash }))
}

// ---------------------------------------------------------------------
// Uninstall
// ---------------------------------------------------------------------

/// Stops the managed core, then hands off the actual teardown (stopping +
/// deleting our own systemd unit, deleting our own installed files) to a
/// short-lived detached script, rather than doing it synchronously here.
///
/// This indirection exists for a different reason than the Windows helper's
/// analogous self-uninstall detour (there, it's "a running Windows service
/// can't delete its own locked .exe"; on Linux, unlinking our own running
/// binary is perfectly safe): here it's that `systemctl disable --now` on
/// *our own* unit will deliver us a SIGTERM essentially immediately, which
/// would race writing the `Response` for this very request back over the
/// socket. So: respond first, tear down after.
///
/// UNVERIFIED, and lower-confidence than the `Start` privilege-drop path:
/// relies on (a) `systemd-run --collect` succeeding in escaping this unit's
/// cgroup so `systemctl stop` doesn't reap the cleanup script along with the
/// daemon, with a plain detached fallback relying on (b) the generated
/// unit's `KillMode=process` (see `install.rs::build_unit`) for the same
/// property if `systemd-run` isn't available. Neither has been exercised
/// against a real systemd instance.
async fn cmd_uninstall(state: &SharedState) -> Response {
    let taken = {
        let mut guard = state.lock().await;
        (guard.pid.take(), guard.child.take())
    };
    if let (Some(pid), Some(mut child)) = taken {
        terminate(pid, &mut child).await;
    }

    let script_path = format!("{}/uninstall.sh", crate::paths::RUNTIME_DIR);
    // Splice a short delay in front of the generated script so the
    // `Response::ok` below has time to actually reach the client before we
    // start stopping the unit that's serving it.
    let script = crate::install::build_uninstall_script().replacen("#!/bin/sh\n", "#!/bin/sh\nsleep 1\n", 1);
    if let Err(err) = std::fs::write(&script_path, &script) {
        return Response::err("uninstall-failed", format!("staging uninstall script: {err}"));
    }
    let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));

    let escaped_cgroup = std::process::Command::new("systemd-run")
        .args(["--collect", "--unit=ferroflow-helper-uninstall", "/bin/sh", script_path.as_str()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok();
    if !escaped_cgroup {
        let _ = std::process::Command::new("/bin/sh")
            .arg(&script_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    Response::ok(json!({ "uninstalling": true }))
}
