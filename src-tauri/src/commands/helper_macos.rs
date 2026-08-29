//! Tauri-facing glue for the macOS privileged helper: status polling plus
//! the install/uninstall flows that shell out to `osascript` for the one
//! admin-rights prompt.
//!
//! This module owns exactly the two things `crates/helper-macos::install`'s
//! module doc comment leaves to "whatever calls `build_install_script`":
//! writing the generated script to a private, unpredictably-named file and
//! running it elevated, then cleaning up. See that doc comment for the
//! TOCTOU reasoning behind the file-naming/permission requirements this
//! module follows.
//!
//! Deliberately free of `#[tauri::command]` attributes and of any
//! reference to `AppState`/`commands::mod`/`lib.rs` — those are wired up by
//! a small platform dispatcher added separately (see this file's own
//! task description), so three sibling platform modules (this one, plus
//! Windows and Linux equivalents) can be developed without three-way merge
//! conflicts on the shared files.

use std::fmt::Write as _;
use std::io::{Read, Write as _};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use helper_client::HelperClient;
use shared_types::{AppError, AppResult, HelperPlatform, HelperStatus};
use tauri::Manager;

/// Overrides helper-binary discovery entirely when set to a non-empty
/// value, mirroring `core-manager`'s `BINARY_PATH_ENV` pattern for the
/// sing-box binary — handy for pointing at a locally built helper without
/// needing a full app bundle.
const HELPER_BINARY_ENV: &str = "FERROFLOW_HELPER_PATH";
const HELPER_BINARY_NAME: &str = "ferroflow-helper-macos";

/// How many times to poll the freshly installed helper before giving up on
/// waiting for `launchd` to finish bootstrapping it. 5 attempts spaced
/// 300ms apart is ~1.5s of slack, which is generous for a `RunAtLoad`
/// daemon that's just a `tokio` binary starting up.
const POST_INSTALL_PING_ATTEMPTS: u32 = 5;
const POST_INSTALL_PING_DELAY: Duration = Duration::from_millis(300);

/// Cheap status check: ping the helper over its Unix socket and, on
/// success, best-effort ask for its version. A ping failure is the
/// expected steady-state before the helper has ever been installed (or
/// after it's been uninstalled) — not an error, just "not installed".
pub async fn get_status(token: Option<String>) -> AppResult<HelperStatus> {
    let client = HelperClient::new(token);

    if client.ping().await.is_err() {
        return Ok(HelperStatus {
            platform: HelperPlatform::Macos,
            installed: false,
            ready: false,
            version: None,
            needs_repair: false,
        });
    }

    let version = client.version().await.ok().and_then(|value| {
        value.get("version").map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    });

    Ok(HelperStatus {
        platform: HelperPlatform::Macos,
        installed: true,
        ready: true,
        version,
        needs_repair: false,
    })
}

/// Runs the full install flow: locate the bundled helper binary, build the
/// install script (`helper_macos::install::build_install_script`), run it
/// elevated via `osascript`, then poll the newly bootstrapped daemon.
///
/// Returns the resulting status plus the freshly generated shared token —
/// callers must persist that token themselves (see the handoff contract in
/// `helper_macos::install`'s module doc comment); this function has no
/// opinion on where.
pub async fn install(app: &tauri::AppHandle) -> AppResult<(HelperStatus, Option<String>)> {
    let helper_binary = locate_helper_binary(app)?;

    let helper_macos::install::InstallPlan { token, script } =
        helper_macos::install::build_install_script(&helper_binary);

    let script_path = write_private_script(&script).map_err(|e| {
        AppError::new("helper_install_failed", format!("failed to write install script to a private temp file: {e}"))
    })?;

    let run_result = run_privileged_script(&script_path).await;
    // Clean up regardless of whether the elevated run succeeded.
    let _ = std::fs::remove_file(&script_path);

    let output = run_result.map_err(|e| {
        AppError::new("helper_install_failed", format!("failed to invoke osascript: {e}"))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // A cancelled admin-privileges prompt surfaces here too (typically
        // with "User canceled" somewhere in stderr) — no separate error
        // code for that per the task spec, but stderr is included so it's
        // distinguishable from other failures.
        return Err(AppError::new(
            "helper_install_failed",
            format!(
                "install script exited with {:?}: {}",
                output.status.code(),
                stderr.trim()
            ),
        ));
    }

    // launchd needs a moment to actually bootstrap the daemon before the
    // socket is accepting connections; poll a few times rather than
    // declaring success (or failure) immediately off the script's exit.
    let client = HelperClient::new(Some(token.clone()));
    let mut became_ready = false;
    for attempt in 0..POST_INSTALL_PING_ATTEMPTS {
        if client.ping().await.is_ok() {
            became_ready = true;
            break;
        }
        if attempt + 1 < POST_INSTALL_PING_ATTEMPTS {
            tokio::time::sleep(POST_INSTALL_PING_DELAY).await;
        }
    }
    if !became_ready {
        let waited_ms = POST_INSTALL_PING_ATTEMPTS as u128 * POST_INSTALL_PING_DELAY.as_millis();
        tracing::warn!(
            "helper install script reported success but the helper did not respond to ping within {waited_ms}ms"
        );
    }

    Ok((
        HelperStatus {
            platform: HelperPlatform::Macos,
            installed: true,
            ready: true,
            version: None,
            needs_repair: false,
        },
        Some(token),
    ))
}

/// Runs the uninstall flow. Best-effort like the underlying script itself
/// (see `helper_macos::install::build_uninstall_script`'s doc comment): a
/// failed or cancelled elevated run is only logged, never returned as an
/// error, since the user asked to remove the helper and this reports it as
/// removed from the app's perspective either way — matching this
/// codebase's established "uninstall is best-effort" convention.
pub async fn uninstall() -> AppResult<HelperStatus> {
    let script = helper_macos::install::build_uninstall_script();

    match write_private_script(&script) {
        Ok(script_path) => {
            let run_result = run_privileged_script(&script_path).await;
            let _ = std::fs::remove_file(&script_path);

            match run_result {
                Ok(output) if !output.status.success() => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        "helper uninstall script exited with {:?}: {}",
                        output.status.code(),
                        stderr.trim()
                    );
                }
                Err(e) => {
                    tracing::warn!("failed to invoke osascript for helper uninstall: {e}");
                }
                Ok(_) => {}
            }
        }
        Err(e) => {
            tracing::warn!("failed to write uninstall script to a private temp file: {e}");
        }
    }

    Ok(HelperStatus {
        platform: HelperPlatform::Macos,
        installed: false,
        ready: false,
        version: None,
        needs_repair: false,
    })
}

/// Resolves the bundled `ferroflow-helper-macos` binary path, in order:
/// `FERROFLOW_HELPER_PATH` env var (verbatim, if set and non-empty) →
/// `./.dev-bin/ferroflow-helper-macos` (dev convenience, gitignored) →
/// `<resource_dir>/ferroflow-helper-macos` (packaged app case). Mirrors
/// `core-manager`'s `locate_binary`, except this returns an error instead
/// of a bare-name fallback, since there's no `$PATH` lookup that could
/// plausibly find a privileged-helper binary.
fn locate_helper_binary(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let mut tried = Vec::new();

    if let Ok(path) = std::env::var(HELPER_BINARY_ENV) {
        if !path.is_empty() {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(path);
            }
            tried.push(path.display().to_string());
        }
    }

    let dev_bin = PathBuf::from(".dev-bin").join(HELPER_BINARY_NAME);
    if dev_bin.is_file() {
        return Ok(dev_bin);
    }
    tried.push(dev_bin.display().to_string());

    match app.path().resource_dir() {
        Ok(resource_dir) => {
            let candidate = resource_dir.join(HELPER_BINARY_NAME);
            if candidate.is_file() {
                return Ok(candidate);
            }
            tried.push(candidate.display().to_string());
        }
        Err(e) => tried.push(format!("<resource_dir unavailable: {e}>")),
    }

    Err(AppError::new(
        "helper_binary_missing",
        format!("could not locate the {HELPER_BINARY_NAME} binary; tried: {}", tried.join(", ")),
    ))
}

/// Writes `contents` to a private, unpredictably-named file: 0700
/// permissions and exclusive-create, per the handoff contract in
/// `helper_macos::install`'s module doc comment (steps 2 and, symmetrically,
/// the uninstall script). The random suffix is generated the same way
/// `helper_macos::install::generate_token` generates the shared token —
/// kernel CSPRNG bytes, hex-encoded — since collisions (and thus a
/// `create_new` failure) should be astronomically unlikely rather than
/// something worth retrying.
fn write_private_script(contents: &str) -> std::io::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("ferroflow-helper-{}.sh", random_hex(16)));

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)?;
    file.write_all(contents.as_bytes())?;

    Ok(path)
}

/// `n` random bytes from the kernel CSPRNG, hex-encoded.
fn random_hex(n: usize) -> String {
    let mut buf = vec![0u8; n];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .unwrap_or_else(|err| panic!("failed to read /dev/urandom for temp script filename: {err}"));

    let mut hex = String::with_capacity(buf.len() * 2);
    for byte in buf {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Runs `script_path` as root via
/// `osascript -e 'do shell script "/bin/bash <path>" with administrator
/// privileges'`, per the handoff contract in `helper_macos::install`'s
/// module doc comment. Uses `tokio::process::Command` (not
/// `std::process::Command`) since this waits on however long the user
/// takes at the admin-credentials prompt, and this function runs on a
/// `tokio` worker thread.
async fn run_privileged_script(script_path: &Path) -> std::io::Result<std::process::Output> {
    let bash_invocation = format!("/bin/bash {}", shell_quote(&script_path.to_string_lossy()));
    let osa_script = format!(
        "do shell script \"{}\" with administrator privileges",
        applescript_escape(&bash_invocation)
    );

    tokio::process::Command::new("osascript").arg("-e").arg(osa_script).output().await
}

/// Single-quotes `s` for safe embedding in a shell command as one word,
/// escaping embedded single quotes the standard POSIX way. Equivalent to
/// (but a separate copy of, since that one is private to its crate)
/// `helper_macos::install`'s own `shell_quote`.
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\"'\"'");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Escapes `s` for embedding inside an AppleScript double-quoted string
/// literal (the outer string `osascript -e`'s `do shell script "..."`
/// argument uses). Our generated temp paths never contain `"` or `\`, but
/// this keeps the call correct even if that ever changes.
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
