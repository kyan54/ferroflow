//! One-time install flow.
//!
//! Builds the systemd unit text and a POSIX-`sh` script that installs the
//! helper binary, seeds the root-owned managed core dir, authorizes a uid,
//! writes + enables the unit. **Deliberately does not invoke `pkexec`
//! itself** — that's the caller's job (the Tauri command layer / a future
//! `core-manager` install path, not wired up yet). The expected handoff,
//! mirroring `LinuxServiceHelper.install()` / `runPkexecScript()` in the
//! Electron original:
//!
//! 1. Caller writes the string returned by [`build_install_script`] to a
//!    temp file with mode 0o755.
//! 2. Caller runs `pkexec /bin/sh <path>` (a single authorization prompt).
//! 3. Caller deletes the temp file and inspects the exit code: `0` success,
//!    `126` polkit auth cancelled / no authentication agent, `127` no
//!    `pkexec` installed, anything else → stderr has the reason.
//! 4. Caller polls `Command::Ping` on the socket until the daemon answers
//!    (systemd needs a moment to actually start the unit after `enable
//!    --now`).
//!
//! None of this file has been exercised — see the crate-level report for
//! what's unverified.

// `build_install_script`/`build_unit` have no caller yet (see module docs:
// wiring up the Tauri-side pkexec handoff is a follow-up), which would
// otherwise be a `dead_code` warning in this bin-only crate (unlike a lib
// crate, `pub` here doesn't imply "part of an external API" to the linter).
// `build_uninstall_script` IS already called, from `service.rs`.
#![allow(dead_code)]

use std::path::Path;

use crate::paths::{CORE_BIN, CORE_DIR, HELPER_DEST, INSTALL_DIR, RUNTIME_DIR, STATE_DIR, UNIT_NAME, UNIT_PATH};

/// The systemd unit for the helper daemon itself.
///
/// Runs as root (no `User=`): it must *be* root in order to `setuid(2)` down
/// to whichever user calls `Start`, and to write the root-owned managed core
/// dir on `InstallCore`. The sing-box child's ambient capabilities are
/// granted per-process by `service.rs` (`caps::raise` in the forked child),
/// not by anything declared in this unit.
///
/// `CapabilityBoundingSet=` is deliberately left unset (full default bounding
/// set): the ambient-capability dance in `service.rs::cmd_start` needs
/// `CAP_NET_ADMIN`/`CAP_NET_RAW`/`CAP_NET_BIND_SERVICE` to still be in this
/// process's bounding set at the moment it raises them into its Inheritable
/// set (a capability can only enter Inheritable if it's in Bounding) — a
/// tighter `CapabilityBoundingSet=` would need to explicitly re-list exactly
/// those three, and getting that wrong silently breaks `Start` in a way
/// that's easy to miss until it's tested on a real box. Left as a follow-up
/// once this has actually run on Linux (this mirrors the Go original's own
/// documented decision, `helper-linux/main.go`'s unit-building comment: "留待
/// 真机验证后加").
///
/// `KillMode=process` (systemd's default is `control-group`) is load-bearing
/// for `Command::Uninstall`: that handler spawns a short-lived detached
/// cleanup script *after* this unit receives `systemctl stop`, and the
/// default control-group kill mode would tear the whole cgroup — including
/// that script — down together with the daemon. See `service.rs::cmd_uninstall`.
pub fn build_unit() -> String {
    format!(
        "[Unit]\n\
         Description=FerroFlow privileged network helper\n\
         Documentation=https://github.com/kyan54/ferroflow\n\
         After=network.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={HELPER_DEST}\n\
         RuntimeDirectory=ferroflow\n\
         RuntimeDirectoryMode=0755\n\
         KillMode=process\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n"
    )
}

/// Builds the root-run install/repair script.
///
/// `helper_binary_src` is the helper binary to install (this crate's own
/// build output, `ferroflow-helper-linux`). `uid` is the caller (the
/// unprivileged user the app is running as) to add to the allow-list —
/// **merge-appended, never overwritten**, so a second user installing or
/// repairing on a shared machine can't silently de-authorize the first.
/// `bundled_core_src`, if given, seeds the managed core dir with a
/// known-good `sing-box` **only when the managed dir doesn't already have an
/// executable there** — a repair/reinstall must never clobber a core that
/// was updated since via the authenticated `InstallCore` socket call.
pub fn build_install_script(helper_binary_src: &Path, uid: u32, bundled_core_src: Option<&Path>) -> String {
    let authfile = helper_proto::endpoints::LINUX_AUTHFILE;
    let unit_body = build_unit();

    let mut script = String::new();
    script.push_str("#!/bin/sh\nset -e\n");
    script.push_str(&format!(
        "install -D -o root -g root -m 0755 {} {}\n",
        sh_quote(&helper_binary_src.to_string_lossy()),
        sh_quote(HELPER_DEST),
    ));
    script.push_str(&format!("mkdir -p {}\n", sh_quote(CORE_DIR)));
    script.push_str(&format!("chown root:root {}\n", sh_quote(CORE_DIR)));
    script.push_str(&format!("chmod 0755 {}\n", sh_quote(CORE_DIR)));
    if let Some(bundled) = bundled_core_src {
        script.push_str(&format!(
            "if [ ! -x {core_bin} ]; then install -o root -g root -m 0755 {bundled} {core_bin}; fi\n",
            core_bin = sh_quote(CORE_BIN),
            bundled = sh_quote(&bundled.to_string_lossy()),
        ));
    }
    script.push_str(&format!("mkdir -p {}\n", sh_quote(STATE_DIR)));
    script.push_str(&format!("chmod 0755 {}\n", sh_quote(STATE_DIR)));
    script.push_str(&format!(
        "touch {authfile}\nchmod 0644 {authfile}\n",
        authfile = sh_quote(authfile)
    ));
    // `uid` is a u32 we generated ourselves (never attacker-controlled shell
    // text), so bare interpolation here is safe — same reasoning as the Go
    // original's equivalent line.
    script.push_str(&format!(
        "grep -qxF '{uid}' {authfile} || printf '%s\\n' '{uid}' >> {authfile}\n",
        authfile = sh_quote(authfile)
    ));
    script.push_str(&format!(
        "cat > {unit_path} <<'FERROFLOW_UNIT_EOF'\n{unit_body}FERROFLOW_UNIT_EOF\n",
        unit_path = sh_quote(UNIT_PATH),
    ));
    script.push_str(&format!("chmod 0644 {}\n", sh_quote(UNIT_PATH)));
    script.push_str("systemctl daemon-reload\n");
    script.push_str(&format!("systemctl enable --now {UNIT_NAME}\n"));
    script.push_str("echo ferroflow-helper-install-ok\n");
    script
}

/// Builds the root-run uninstall script: symmetric with
/// [`build_install_script`] — stop the unit, delete it, delete everything
/// under the managed install/state/runtime dirs. `INSTALL_DIR` is exclusively
/// ours (never shared with e.g. a future `.deb`'s `/opt` tree) so `rm -rf` is
/// safe. Reused by both the external pkexec uninstall flow and (with a
/// `sleep` spliced in — see `service.rs::cmd_uninstall`) the socket-triggered
/// `Command::Uninstall` self-teardown.
pub fn build_uninstall_script() -> String {
    format!(
        "#!/bin/sh\n\
         systemctl disable --now {unit} 2>/dev/null || true\n\
         rm -f {unit_path}\n\
         rm -rf {install} {state} {runtime}\n\
         systemctl daemon-reload 2>/dev/null || true\n\
         echo ferroflow-helper-uninstall-ok\n",
        unit = UNIT_NAME,
        unit_path = sh_quote(UNIT_PATH),
        install = sh_quote(INSTALL_DIR),
        state = sh_quote(STATE_DIR),
        runtime = sh_quote(RUNTIME_DIR),
    )
}

/// Minimal POSIX-`sh` single-quote escaping (`'...'`, with embedded `'`
/// turned into `'\''`). Only ever called with filesystem paths this process
/// controls (never raw client input), so this is intentionally not a
/// general-purpose shell-quoting library.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}
