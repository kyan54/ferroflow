//! One-time install/uninstall flow for the LaunchDaemon.
//!
//! This module never shells out to `osascript` itself — elevation is the
//! *caller's* job (the unprivileged Tauri app), because this helper binary
//! has no business prompting for its own admin rights, and because a
//! long-running root daemon binary is the wrong place to be linking against
//! whatever UI/AppleScript plumbing a one-shot admin prompt needs. This
//! module only builds strings; it never touches the filesystem.
//!
//! Handoff contract for whatever calls `build_install_script`:
//!   1. Persist the returned `InstallPlan::token` as its own client-side
//!      copy (e.g. next to the app's config), so it can authenticate to the
//!      socket once the daemon comes up — this module has no opinion on
//!      *where*, since that's app-data-layout territory, not helper
//!      territory.
//!   2. Write `InstallPlan::script` to a private, unpredictably-named file
//!      (0700, owned by the invoking user, created with an exclusive-create
//!      flag) and run it via
//!      `osascript -e 'do shell script "/bin/bash <path>" with
//!      administrator privileges'`. Mirrors
//!      `HelperManager.runRootScript`/`osaShellArg` in the sibling Electron
//!      app (`FlowZ/src/main/services/HelperManager.ts`) — see that
//!      function's comments for the TOCTOU rationale (predictable path +
//!      writable-by-others directory would let another local process swap
//!      the script's contents between write and root-exec).
//!   3. Delete the script file afterwards either way (success or failure).
//!
//! UNVERIFIED end-to-end: nobody has run this against a real `launchd` yet
//! (no macOS box to test on — see the crate-level caveat in `service.rs`).
//! The plist shape and the `launchctl bootstrap`/`bootout`/`enable`
//! invocations are carried over near-verbatim from
//! `HelperManager.ts`'s `buildInstallScript`/`buildUninstallScript`, which
//! *has* shipped in the sibling Electron app — but the label/paths/binary
//! here are new (`com.ferroflow.helper`, not `com.flowz.helper`; no
//! `--singbox`/`--confdir`/`--support`/`--coredir` CLI flags, since this
//! daemon takes its per-run paths over the socket instead) and have not
//! themselves been bootstrapped on a Mac.

#![cfg(target_os = "macos")]

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::paths::{log_path, support_dir, HELPER_DEST, LABEL, PLIST_PATH};

/// Everything a caller needs to actually perform the install: the token to
/// persist locally for future socket auth, and the root-executed script
/// that wires up the daemon with that same token baked in.
pub struct InstallPlan {
    /// Freshly generated shared secret, already embedded in `script` (which
    /// writes it to the root-owned `helper.token` file). The caller must
    /// separately persist this value as its own client-side copy — see the
    /// module doc comment's handoff contract, step 1.
    pub token: String,
    /// Bash script to run as root via
    /// `osascript -e '... with administrator privileges'`. See the module
    /// doc comment for the full handoff contract.
    pub script: String,
}

/// Builds the one-shot install script (plist + binary + token, all
/// root-owned) for the LaunchDaemon. `helper_binary_src` is the path to
/// this same `ferroflow-helper-macos` binary as bundled with the app (e.g.
/// inside `<App>.app/Contents/Resources/`) — the *script* `cp`s it into
/// place; this function does not read or touch it itself (it has no root
/// yet to do so with).
pub fn build_install_script(helper_binary_src: &Path) -> InstallPlan {
    let token = generate_token();
    let script = render_install_script(helper_binary_src, &token);
    InstallPlan { token, script }
}

/// Builds the uninstall script: `bootout` the daemon and remove every file
/// the install script created. No token needed — nothing to persist.
/// Deliberately has no `set -e` (unlike the install script): uninstall
/// should keep going and clean up as much as it can even if one step
/// fails, matching `helper.go`/`HelperManager.ts`'s "best effort" framing
/// and `service::handle_uninstall`'s own log-and-continue behavior for the
/// same operations reached via the socket.
pub fn build_uninstall_script() -> String {
    format!(
        r#"#!/bin/bash
PLIST={plist}
launchctl bootout system "$PLIST" 2>/dev/null || true
rm -f "$PLIST" {helper_dest}
rm -rf {support_dir}
echo uninstalled-ok
"#,
        plist = shell_quote(PLIST_PATH),
        helper_dest = shell_quote(HELPER_DEST),
        support_dir = shell_quote(&support_dir().to_string_lossy()),
    )
}

fn render_install_script(helper_binary_src: &Path, token: &str) -> String {
    let plist_xml = render_plist();
    format!(
        r#"#!/bin/bash
set -e
umask 077
SRC={src}
DEST={dest}
SUPPORT={support}
PLIST={plist_path}
mkdir -p /Library/PrivilegedHelperTools "$SUPPORT"
# `umask 077` above would otherwise leave a freshly-created $SUPPORT at
# 700, which blocks the unprivileged app from even traversing the
# directory to reach the socket. The socket itself stays 0666 regardless
# (see `service::run`) — the shared token is the real trust boundary, not
# filesystem permissions on the socket or its parent directory.
chmod 755 /Library/PrivilegedHelperTools "$SUPPORT"
cp "$SRC" "$DEST"
chown root:wheel "$DEST"
chmod 755 "$DEST"
printf '%s' {token} > "$SUPPORT/helper.token"
chown root:wheel "$SUPPORT/helper.token"
chmod 600 "$SUPPORT/helper.token"
cat > "$PLIST" <<'FERROFLOW_PLIST_EOF'
{plist_xml}
FERROFLOW_PLIST_EOF
chown root:wheel "$PLIST"
chmod 644 "$PLIST"
launchctl bootout system "$PLIST" 2>/dev/null || true
launchctl enable system/{label} 2>/dev/null || true
launchctl bootstrap system "$PLIST"
echo installed-ok
"#,
        src = shell_quote(&helper_binary_src.to_string_lossy()),
        dest = shell_quote(HELPER_DEST),
        support = shell_quote(&support_dir().to_string_lossy()),
        plist_path = shell_quote(PLIST_PATH),
        token = shell_quote(token),
        plist_xml = plist_xml,
        label = LABEL,
    )
}

/// `ProgramArguments` is just `[HELPER_DEST]` — unlike `helper.go`, this
/// daemon takes no CLI flags. The core binary path is a per-`Start`-request
/// field in the protocol rather than something locked in at install time
/// (see `service::handle_start`'s doc comment for the security trade-off
/// that implies). `StandardOutPath`/`StandardErrorPath` point at a log file
/// under the support directory so a crash-on-launch is debuggable without
/// digging through the unified log.
fn render_plist() -> String {
    let log_path = log_path().to_string_lossy().into_owned();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{helper_dest}</string>
  </array>
  <key>KeepAlive</key><true/>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>{log_path}</string>
  <key>StandardErrorPath</key><string>{log_path}</string>
</dict>
</plist>"#,
        label = xml_escape(LABEL),
        helper_dest = xml_escape(HELPER_DEST),
        log_path = xml_escape(&log_path),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Single-quotes `s` for safe embedding in the generated bash script as one
/// word, escaping any embedded single quotes the standard POSIX-shell way:
/// close the quote, emit a separately-quoted literal `'`, reopen —
/// i.e. `'...'"'"'...'`. Mirrors `shq()` in `HelperManager.ts`. Needed
/// because `helper_binary_src` in particular is an app-controlled path that
/// may contain spaces (e.g. anything under "Application Support") or, in
/// principle, other shell metacharacters.
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

/// 32 random bytes from the kernel CSPRNG, hex-encoded — plenty for a
/// bearer token nobody needs to type, and avoids pulling in the `rand`
/// crate for one call site. `/dev/urandom` never blocks on modern Darwin
/// (it draws from the same CSPRNG as `/dev/random`, unlike historic Linux
/// behavior where the distinction mattered more).
fn generate_token() -> String {
    let mut buf = [0u8; 32];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .unwrap_or_else(|err| {
            // Extremely unlikely (would mean /dev/urandom is missing or
            // sandboxed away) but an install should never silently hand out
            // a predictable token — fail loudly instead.
            panic!("failed to read /dev/urandom for helper token: {err}")
        });

    let mut hex = String::with_capacity(buf.len() * 2);
    for byte in buf {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
