//! Shared filesystem locations for the LaunchDaemon, used by both the
//! running `service` and the `install`/`uninstall` script builders. Kept in
//! one place so the two halves of this crate can't silently drift apart
//! (e.g. the daemon binding a socket somewhere the install script never
//! `chmod 755`'d up to).

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};

/// LaunchDaemon label — the plist `Label` key and the base of the
/// `launchctl` service target (`system/<LABEL>`).
pub(crate) const LABEL: &str = "com.ferroflow.helper";

/// Where the plist is installed. `launchctl bootstrap system "$PLIST"` /
/// `bootout system "$PLIST"` both take this path directly (see
/// `install::build_install_script`'s doc comment for why the two-argument
/// `bootout system "$PLIST"` form is used instead of `bootout
/// system/<LABEL>` — both are valid `launchctl` syntax, this one matches
/// what the sibling Electron app's `HelperManager.ts` already ships).
pub(crate) const PLIST_PATH: &str = "/Library/LaunchDaemons/com.ferroflow.helper.plist";

/// Where the install script copies the helper binary itself (root:wheel,
/// 0755) — the `/Library/PrivilegedHelperTools/<label>` convention Apple's
/// SMJobBless and the sibling Electron app's Go helper both use.
pub(crate) const HELPER_DEST: &str = "/Library/PrivilegedHelperTools/com.ferroflow.helper";

/// Directory holding the socket, the token file, the `InstallCore`
/// destination, and (once installed) the daemon's own log file. Derived
/// from `helper_proto::endpoints::MACOS_SOCKET` rather than duplicated as a
/// separate string literal, so this crate and `helper-proto` can't drift
/// apart on where things live.
pub(crate) fn support_dir() -> &'static Path {
    Path::new(helper_proto::endpoints::MACOS_SOCKET)
        .parent()
        .expect("MACOS_SOCKET is defined as an absolute path with a parent directory")
}

/// Locked-down directory `InstallCore` writes the verified sing-box binary
/// into. Not part of `helper_proto::endpoints` because it's an
/// implementation detail of this crate's `InstallCore` handling, not a
/// transport endpoint the client needs to know about.
pub(crate) fn core_dir() -> PathBuf {
    support_dir().join("core")
}

/// Final, locked path of the verified sing-box binary after `InstallCore`.
pub(crate) fn core_binary_path() -> PathBuf {
    core_dir().join("sing-box")
}

/// Path the plist's `StandardOutPath`/`StandardErrorPath` point at (see
/// `install::render_plist`) — a crash-on-launch is otherwise very hard to
/// debug, since a LaunchDaemon's stdio normally just goes to `/dev/null`.
pub(crate) fn log_path() -> PathBuf {
    support_dir().join("helper.log")
}
