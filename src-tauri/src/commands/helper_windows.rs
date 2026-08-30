//! Tauri command implementations for the Windows privileged helper
//! (`crates/helper-windows`, installed as the `FerroFlowHelper` LocalSystem
//! Windows Service). Compiled on Windows only -- the `#[cfg(target_os =
//! "windows")] mod helper_windows;` gate lives in `commands/mod.rs`, wired
//! up separately from this file so three parallel per-platform passes
//! (this one, `helper_macos.rs`, `helper_linux.rs`) don't collide on a
//! shared file. Nothing in here needs an internal `cfg` guard as a result.
//!
//! `get_status`/`install`/`uninstall` intentionally do not touch
//! `AppState` or persist the token anywhere -- that's the dispatcher's
//! job once it's wired up (it owns deciding *where* the token/config live
//! and feeding the persisted token back into `core-manager`). This module
//! only knows how to talk to the one already-running helper (via
//! `helper-client`) and how to drive the elevated one-shot installer
//! binary (via `ShellExecuteExW`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use helper_client::HelperClient;
use rand::RngCore;
use shared_types::{AppError, AppResult, HelperPlatform, HelperStatus};
use tauri::{AppHandle, Manager};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED, HANDLE};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

/// Matches `helper-windows`'s `[[bin]] name` in `crates/helper-windows/Cargo.toml`
/// (`ferroflow-helper-windows`), with the `.exe` extension a bundled/dev
/// binary actually has on disk.
const HELPER_BINARY_NAME: &str = "ferroflow-helper-windows.exe";

/// Dev-only override, checked first so a local build doesn't need a full
/// Tauri bundle to exercise the install flow. Mirrors `core-manager`'s
/// `BINARY_PATH_ENV`/`locate_binary` convention for the sing-box binary
/// (see `crates/core-manager/src/lib.rs`), just for the helper binary
/// instead.
const HELPER_PATH_ENV: &str = "FERROFLOW_HELPER_PATH";

/// After `--install` returns (which itself waits for `service.start()` to
/// return -- see `crates/helper-windows/src/install.rs`), the SCM still
/// needs a moment to actually transition the service into a state where
/// it's accepting pipe connections. Poll a few times rather than failing
/// fast on the first `ping`.
const READY_POLL_ATTEMPTS: u32 = 5;
const READY_POLL_DELAY: Duration = Duration::from_millis(300);

/// `SW_SHOWNORMAL` -- not pulled in via `windows-sys`'s
/// `Win32_UI_WindowsAndMessaging` feature (which would be the "proper"
/// source for this constant) purely to avoid adding a fourth `windows-sys`
/// feature for one integer literal `ShellExecuteExW` barely uses (the
/// elevated installer has no window of its own to show/hide).
const SW_SHOWNORMAL: i32 = 1;

/// Best-effort "is the helper installed and healthy right now". Not an
/// error case when it isn't -- a fresh install with no helper yet is the
/// expected steady state before the user ever clicks "install", so this
/// always returns `Ok`, never `Err`, regardless of whether the helper
/// responds.
pub async fn get_status(token: Option<String>) -> AppResult<HelperStatus> {
    let client = HelperClient::new(token);

    if client.ping().await.is_err() {
        return Ok(HelperStatus {
            platform: HelperPlatform::Windows,
            installed: false,
            ready: false,
            version: None,
            needs_repair: false,
        });
    }

    // Best-effort: a helper that can `Ping` but not `Version` is still
    // "installed and ready" as far as this command is concerned, just
    // without a version string to display.
    let version =
        client.version().await.ok().and_then(|v| v.get("version").map(|v| v.to_string()));

    Ok(HelperStatus {
        platform: HelperPlatform::Windows,
        installed: true,
        ready: true,
        version,
        needs_repair: false,
    })
}

/// Elevated one-time install: locate the bundled helper binary, generate a
/// fresh shared token, hand it to `<helper> --install --token-file <path>`
/// under UAC, wait for that to finish, and confirm the resulting service
/// is actually reachable before declaring success.
///
/// Returns the freshly generated token alongside the resulting status --
/// persisting it (and feeding it into `core-manager`) is the caller's job,
/// not this function's; see the module doc comment.
pub async fn install(app: &AppHandle) -> AppResult<(HelperStatus, Option<String>)> {
    let binary = locate_helper_binary(Some(app)).map_err(|tried| {
        AppError::new(
            "helper_binary_missing",
            format!(
                "could not find the FerroFlow helper binary; tried: {}",
                tried.join(", ")
            ),
        )
    })?;

    let token = generate_token();

    // A fixed, PID-based name under `%TEMP%` rather than a cryptographically
    // unpredictable one: `%TEMP%` is already ACL'd to the current user by
    // Windows, so another local, unprivileged process reading or racing
    // this file is already outside this program's threat model -- the same
    // risk profile `helper-macos`'s `install.rs` doc comment accepts for
    // its own temp install script (see that module's handoff-contract
    // comment on TOCTOU). The *destination* the elevated process writes the
    // token to (`C:\ProgramData\FerroFlow\helper.token`) is the one that
    // actually gets locked down, via `icacls` in
    // `crates/helper-windows/src/install.rs::lock_down_token_file`.
    let token_file = std::env::temp_dir().join(format!("ferroflow-helper-token-{}.tmp", std::process::id()));
    std::fs::write(&token_file, &token).map_err(|err| {
        AppError::new("helper_install_failed", format!("failed to write temp token file: {err}"))
    })?;

    // Windows filenames can never contain `"`, so this naive quoting is
    // always safe for a path -- no need for full command-line escaping.
    let params = format!("--install --token-file \"{}\"", token_file.display());
    let elevated_result = run_elevated(&binary, &params);

    // Best-effort cleanup regardless of how the elevated run went; the
    // installer only ever *reads* this file (see
    // `crates/helper-windows/src/install.rs::install`), so it's safe to
    // remove it as soon as that process has exited either way.
    let _ = std::fs::remove_file(&token_file);

    let exit_code = elevated_result?;
    if exit_code != 0 {
        return Err(AppError::new(
            "helper_install_failed",
            format!("installer exited with code {exit_code}"),
        ));
    }

    let client = HelperClient::new(Some(token.clone()));
    let mut ready = false;
    for attempt in 0..READY_POLL_ATTEMPTS {
        if client.ping().await.is_ok() {
            ready = true;
            break;
        }
        if attempt + 1 < READY_POLL_ATTEMPTS {
            tokio::time::sleep(READY_POLL_DELAY).await;
        }
    }

    if !ready {
        return Err(AppError::new(
            "helper_install_unreachable",
            "the installer reported success but the helper service did not become reachable \
             over its named pipe",
        ));
    }

    Ok((
        HelperStatus {
            platform: HelperPlatform::Windows,
            installed: true,
            ready: true,
            // Freshly installed: the version is known (it's whatever this
            // build of the helper binary reports) but querying it here
            // would just be an extra round trip the caller can do itself
            // via `get_status` if it wants the string populated.
            version: None,
            needs_repair: false,
        },
        Some(token),
    ))
}

/// Elevated uninstall: locate the helper binary and run
/// `<helper> --uninstall` under UAC. Deliberately forgiving -- matching
/// the "best effort, keep going" semantics of the underlying uninstall
/// paths elsewhere in this codebase (see
/// `crates/helper-macos/src/install.rs::build_uninstall_script`'s doc
/// comment, and `crates/helper-windows/src/install.rs::uninstall`'s
/// log-and-continue handling of a missing service). A cancelled UAC
/// prompt, a missing binary, or a non-zero exit code are all logged and
/// swallowed rather than surfaced as `Err` -- this function always
/// resolves to "not installed" from the caller's point of view, since
/// that's the safe assumption once uninstall has been attempted at all.
pub async fn uninstall() -> AppResult<HelperStatus> {
    let not_installed = HelperStatus {
        platform: HelperPlatform::Windows,
        installed: false,
        ready: false,
        version: None,
        needs_repair: false,
    };

    // No `AppHandle` is available here (see this function's signature),
    // so unlike `install`, binary discovery can't fall back to
    // `resource_dir()` -- only the env override and the dev-bin
    // convenience path. In practice the packaged app's dispatcher is
    // expected to already know the helper is installed before calling
    // this (e.g. from a prior `get_status`), so the dev/env paths cover
    // the cases this crate can actually test.
    let binary = match locate_helper_binary(None) {
        Ok(path) => path,
        Err(tried) => {
            tracing::warn!(
                "uninstall: could not find the FerroFlow helper binary (tried: {}); leaving \
                 any installed service as-is",
                tried.join(", ")
            );
            return Ok(not_installed);
        }
    };

    match run_elevated(&binary, "--uninstall") {
        Ok(0) => {}
        Ok(code) => tracing::warn!("helper --uninstall exited with code {code}"),
        Err(err) => tracing::warn!("failed to run elevated helper --uninstall: {}", err.message),
    }

    Ok(not_installed)
}

/// Binary discovery, shared by `install` and `uninstall`, mirroring
/// `core-manager`'s `locate_binary` convention (see
/// `crates/core-manager/src/lib.rs::locate_binary`) one step at a time:
/// env var override, then a `.dev-bin` convenience path, then (when an
/// `AppHandle` is available) Tauri's bundled-resource directory, under a
/// `helper/` subfolder (staged there by `npm run build:helper`/
/// `scripts/build-helper.mjs` into `src-tauri/resources/helper/`, which
/// `bundle.resources` maps to `helper/` inside the bundle -- see that
/// script's doc comment for why this can't just be a bare-named workspace
/// binary Cargo builds for free). Returns the list of paths actually
/// tried, in order, on failure so the caller can build a useful error
/// message.
fn locate_helper_binary(app: Option<&AppHandle>) -> Result<PathBuf, Vec<String>> {
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

    if let Some(app) = app {
        if let Ok(resource_dir) = app.path().resource_dir() {
            let candidate = resource_dir.join("helper").join(HELPER_BINARY_NAME);
            if candidate.is_file() {
                return Ok(candidate);
            }
            tried.push(candidate.display().to_string());
        }
    }

    Err(tried)
}

/// 32 random bytes, hex-encoded -- the established shared-token format in
/// this codebase (see `crates/helper-macos/src/install.rs::generate_token`,
/// which reaches the same 32-byte/hex shape via `/dev/urandom` since it has
/// no `rand` dependency to reuse; there's no OS-CSPRNG equivalent this
/// crate would rather reach for directly, so `rand::thread_rng()` is used
/// here instead).
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Runs `<binary> <params>` elevated (UAC) via `ShellExecuteExW` with
/// `lpVerb = "runas"`, waits for it to exit, and returns its real exit
/// code. Preferred over shelling out to
/// `powershell -Command "Start-Process -Verb RunAs -Wait"` for two
/// reasons: quoting a path that might contain spaces through an extra
/// layer of PowerShell/cmd argument parsing is fragile, and
/// `ShellExecuteExW` + `SEE_MASK_NOCLOSEPROCESS` hands back a real process
/// handle for `GetExitCodeProcess` instead of inferring success from
/// PowerShell's own wrapper exit code.
///
/// A cancelled UAC prompt surfaces as `ShellExecuteExW` itself failing
/// with `ERROR_CANCELLED` (Windows' documented behavior when the user
/// declines the elevation prompt) -- callers get that back as a distinct
/// `helper_install_cancelled` error rather than a generic failure.
fn run_elevated(binary: &Path, params: &str) -> AppResult<u32> {
    let file_wide = to_wide(&binary.to_string_lossy());
    let params_wide = to_wide(params);
    let verb_wide = to_wide("runas");

    // SAFETY: `SHELLEXECUTEINFOW` is a plain-old-data struct; zero-
    // initializing it and then only setting the fields `ShellExecuteExW`
    // actually reads for this call (`cbSize`, `fMask`, `lpVerb`, `lpFile`,
    // `lpParameters`, `nShow`) is the documented pattern for this API. The
    // three `_wide` buffers above are kept alive for this whole function
    // (they're not dropped until after the call below), so the raw
    // pointers stored into `info` stay valid for the duration of the FFI
    // call.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb_wide.as_ptr();
    info.lpFile = file_wide.as_ptr();
    info.lpParameters = params_wide.as_ptr();
    info.nShow = SW_SHOWNORMAL;

    // SAFETY: `info` is a validly-initialized `SHELLEXECUTEINFOW` with
    // `cbSize` set correctly and every pointer field it uses pointing at
    // a live, NUL-terminated UTF-16 buffer for the duration of this call.
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        // SAFETY: plain FFI call, no preconditions beyond having just
        // observed a failure from the previous call on this thread.
        let err = unsafe { GetLastError() };
        if err == ERROR_CANCELLED {
            return Err(AppError::new(
                "helper_install_cancelled",
                "the elevation (UAC) prompt was cancelled",
            ));
        }
        return Err(AppError::new(
            "helper_elevation_failed",
            format!("ShellExecuteExW failed (error {err})"),
        ));
    }

    let process: HANDLE = info.hProcess;
    if process.is_null() {
        return Err(AppError::new(
            "helper_elevation_failed",
            "ShellExecuteExW reported success but returned no process handle \
             (SEE_MASK_NOCLOSEPROCESS)",
        ));
    }

    // SAFETY: `process` was just confirmed non-null and came directly from
    // a successful `ShellExecuteExW` call with `SEE_MASK_NOCLOSEPROCESS`
    // set, so it's a valid, owned process handle this function is
    // responsible for closing (done below, on every path).
    unsafe { WaitForSingleObject(process, INFINITE) };

    let mut exit_code: u32 = 0;
    // SAFETY: `process` is still the same valid handle; `exit_code` is a
    // live, correctly-sized output parameter.
    let got_exit_code = unsafe { GetExitCodeProcess(process, &mut exit_code) };
    // SAFETY: closes the handle exactly once, after both uses above are
    // done with it.
    unsafe { CloseHandle(process) };

    if got_exit_code == 0 {
        return Err(AppError::new(
            "helper_elevation_failed",
            "failed to retrieve the elevated process's exit code",
        ));
    }

    Ok(exit_code)
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
