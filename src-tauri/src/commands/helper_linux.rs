//! Linux Tauri commands for the privileged-helper install/status/uninstall
//! flow — the `commands::helper_linux` half of the three-platform dispatcher
//! described in `docs/ipc-contract.md`'s "Helper install flow" section.
//!
//! Linux has no shared-secret token: `helper-linux` authenticates callers via
//! `SO_PEERCRED` (kernel-verified peer uid) against a root-owned allow-list
//! file that the install script itself appends to (see
//! `crates/helper-linux/src/install.rs`). The `token`/`Option<String>` slots
//! on [`get_status`]/[`install`] exist only so this module's function shapes
//! match the Windows/macOS siblings for a shared dispatcher — on Linux they
//! are always `None`.
//!
//! This module is only ever compiled on Linux (a `#[cfg(target_os =
//! "linux")] mod helper_linux;` in `commands/mod.rs`, wired up separately),
//! so no internal `#[cfg]` gates are needed here.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use helper_client::HelperClient;
use shared_types::{AppError, AppResult, HelperPlatform, HelperStatus};
use tauri::Manager;

/// Dev/packaged-binary discovery env var, mirroring `core-manager`'s
/// `FERROFLOW_SINGBOX_PATH` convention (see `crates/core-manager/src/lib.rs`
/// `locate_binary`) and documented in `docs/ipc-contract.md`.
const HELPER_PATH_ENV: &str = "FERROFLOW_HELPER_PATH";

/// Name of the bundled helper binary, both in the dev `.dev-bin/` convention
/// dir and in the packaged app's resource dir.
const HELPER_BINARY_NAME: &str = "ferroflow-helper-linux";

/// How many times (and how far apart) to poll `Ping` after `systemctl enable
/// --now` before declaring the install a success — systemd needs a moment to
/// actually start the unit, per `install.rs`'s module doc comment.
const PING_POLL_ATTEMPTS: u32 = 5;
const PING_POLL_DELAY: Duration = Duration::from_millis(300);

/// Best-effort "is the helper installed and healthy right now".
///
/// `token` exists only for signature parity with the Windows/macOS
/// siblings — Linux auth never uses one, so this is always effectively
/// `None` in practice. A ping failure is the expected steady-state before
/// the first install, not an error: this returns `Ok` either way.
pub async fn get_status(token: Option<String>) -> AppResult<HelperStatus> {
    let client = HelperClient::new(token);

    if client.ping().await.is_err() {
        return Ok(HelperStatus {
            platform: HelperPlatform::Linux,
            installed: false,
            ready: false,
            version: None,
            needs_repair: false,
        });
    }

    // Best-effort: a `Version` failure right after a successful `Ping`
    // shouldn't demote an otherwise-healthy helper to "not installed".
    let version = client.version().await.ok().and_then(|data| data.get("version").map(ToString::to_string));

    Ok(HelperStatus { platform: HelperPlatform::Linux, installed: true, ready: true, version, needs_repair: false })
}

/// Runs the one-time (or repair) `pkexec` install flow: locate the bundled
/// helper binary, build the install script, run it under a single polkit
/// authorization prompt, then poll `Ping` until the freshly-enabled systemd
/// unit answers.
///
/// Returns `None` for the token slot (parity with the Windows/macOS
/// siblings' `Some(token)`) since Linux never has one.
pub async fn install(app: &tauri::AppHandle) -> AppResult<(HelperStatus, Option<String>)> {
    let helper_binary_path = locate_helper_binary(app)?;

    // SAFETY: `getuid(2)` takes no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };

    let script = helper_linux::install::build_install_script(&helper_binary_path, uid, None);
    let script_path = write_temp_script("ferroflow-helper-install", &script)
        .map_err(|e| AppError::new("helper_install_failed", format!("failed to write install script: {e}")))?;

    let run_result = run_pkexec(script_path.clone()).await;
    let _ = std::fs::remove_file(&script_path);
    let output = run_result
        .map_err(|e| AppError::new("helper_install_failed", format!("failed to run pkexec: {e}")))?;

    match output.status.code() {
        Some(0) => {}
        Some(126) => {
            return Err(AppError::new(
                "helper_install_cancelled",
                "the polkit authorization prompt was cancelled, or no authentication agent is available",
            ));
        }
        Some(127) => {
            return Err(AppError::new("pkexec_missing", "pkexec is not installed on this system"));
        }
        other => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::new(
                "helper_install_failed",
                format!("install script exited with {other:?}: {stderr}"),
            ));
        }
    }

    let client = HelperClient::new(None);
    let mut ready = false;
    for attempt in 0..PING_POLL_ATTEMPTS {
        if client.ping().await.is_ok() {
            ready = true;
            break;
        }
        if attempt + 1 < PING_POLL_ATTEMPTS {
            tokio::time::sleep(PING_POLL_DELAY).await;
        }
    }
    if !ready {
        tracing::warn!("helper install script succeeded but the daemon did not answer Ping in time");
    }

    Ok((
        HelperStatus { platform: HelperPlatform::Linux, installed: true, ready: true, version: None, needs_repair: false },
        None,
    ))
}

/// Runs the `pkexec`-driven uninstall script. Best-effort, matching this
/// codebase's established "uninstall always reports removed" convention: a
/// non-zero exit (including a cancelled auth prompt) is logged but does not
/// turn into an `Err` — the user asked to remove the helper, so from the
/// app's perspective it is gone either way.
///
/// Takes `_app` only so `commands/helper.rs`'s platform dispatch can call
/// `uninstall(&app)` uniformly across all three platforms -- this
/// implementation builds a self-contained removal script and never needs
/// to resolve the bundled binary's `resource_dir()` the way Windows's
/// `uninstall` does.
pub async fn uninstall(_app: &tauri::AppHandle) -> AppResult<HelperStatus> {
    let script = helper_linux::install::build_uninstall_script();

    match write_temp_script("ferroflow-helper-uninstall", &script) {
        Ok(script_path) => {
            let run_result = run_pkexec(script_path.clone()).await;
            let _ = std::fs::remove_file(&script_path);
            match run_result {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "helper uninstall script exited with {:?}: {stderr}",
                        output.status.code()
                    );
                }
                Err(e) => {
                    tracing::warn!("failed to run pkexec for helper uninstall: {e}");
                }
            }
        }
        Err(e) => {
            tracing::warn!("failed to write helper uninstall script: {e}");
        }
    }

    Ok(HelperStatus { platform: HelperPlatform::Linux, installed: false, ready: false, version: None, needs_repair: false })
}

/// Resolves the bundled helper binary, mirroring `core-manager`'s
/// `locate_binary` discovery order (see `crates/core-manager/src/lib.rs`)
/// and `docs/ipc-contract.md`'s "Binary discovery" convention:
///
/// 1. `FERROFLOW_HELPER_PATH` env var, used verbatim, if set and non-empty.
/// 2. `./.dev-bin/ferroflow-helper-linux` (dev convenience, gitignored).
/// 3. `<resource_dir>/helper/ferroflow-helper-linux` (packaged case --
///    staged there by `npm run build:helper`/`scripts/build-helper.mjs`
///    into `src-tauri/resources/helper/`, which `bundle.resources` maps to
///    `helper/` inside the bundle).
///
/// Returns an error naming every candidate tried if none exist as a file.
fn locate_helper_binary(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let mut tried = Vec::new();

    if let Ok(path) = std::env::var(HELPER_PATH_ENV) {
        if !path.is_empty() {
            let candidate = PathBuf::from(&path);
            if candidate.is_file() {
                return Ok(candidate);
            }
            tried.push(candidate.display().to_string());
        }
    }

    let dev_bin = PathBuf::from(".dev-bin").join(HELPER_BINARY_NAME);
    if dev_bin.is_file() {
        return Ok(dev_bin);
    }
    tried.push(dev_bin.display().to_string());

    if let Ok(resource_dir) = app.path().resource_dir() {
        let packaged = resource_dir.join("helper").join(HELPER_BINARY_NAME);
        if packaged.is_file() {
            return Ok(packaged);
        }
        tried.push(packaged.display().to_string());
    }

    Err(AppError::new(
        "helper_binary_missing",
        format!("could not find the ferroflow helper binary; tried: {}", tried.join(", ")),
    ))
}

/// Writes `contents` to a fresh temp file with mode 0o755 (executable by the
/// script's own shebang, matching what `pkexec /bin/sh <path>` and
/// `install.rs`'s handoff contract expect), using a filename salted with the
/// current pid and a timestamp so concurrent install/uninstall calls (or a
/// stale leftover from a previous run) can't collide. Shared by [`install`]
/// and [`uninstall`], which otherwise both need this identically.
///
/// Unlike the macOS flow's exclusive-create dance (see that module's own
/// doc comment), this doesn't need TOCTOU hardening: `pkexec` re-reads and
/// directly executes the named file under its own authorization dialog,
/// rather than trusting a path handed to an already-privileged process.
fn write_temp_script(prefix: &str, contents: &str) -> std::io::Result<PathBuf> {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("{prefix}-{}-{:x}.sh", std::process::id(), nanos));

    let mut file =
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o755).open(&path)?;
    file.write_all(contents.as_bytes())?;
    Ok(path)
}

/// Runs `pkexec /bin/sh <script_path>` off the async executor's thread (this
/// blocks on the polkit authorization dialog, which can sit open for as long
/// as the user takes to respond).
async fn run_pkexec(script_path: PathBuf) -> std::io::Result<std::process::Output> {
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("pkexec").arg("/bin/sh").arg(&script_path as &Path).output()
    })
    .await
    .unwrap_or_else(|join_err| {
        Err(std::io::Error::other(format!("pkexec task panicked: {join_err}")))
    })
}
