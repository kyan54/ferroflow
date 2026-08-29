//! Plain unprivileged sing-box child-process spawn/monitor.
//!
//! This is the non-elevated path only: `<binary> run -c <config>` as a
//! normal child process, tracked via `tokio::process::Child`. Privileged
//! spawn (helper integration for TUN mode, needing root/SYSTEM/ambient
//! caps) is a separate follow-up crate being built in parallel — this
//! module does not call into `helper-proto` or any helper crate.

use std::io;
use std::path::Path;
use std::process::Stdio;
#[cfg(unix)]
use std::time::Duration;

use tokio::process::{Child, ChildStderr, ChildStdout, Command};

/// How long `stop()` waits for a graceful exit after SIGTERM before
/// escalating to SIGKILL. Unix-only: Windows has no equivalent grace period
/// (see `stop()`).
#[cfg(unix)]
const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A running (or since-exited) sing-box child process. Owns the
/// `tokio::process::Child` handle; stdout/stderr are piped so sing-box
/// doesn't inherit/flash a console. `CoreManager::start` reads them via
/// `take_stdio` right after `spawn` returns and feeds each line into
/// `core_manager::logs::LogBuffer` -- the gRPC status/connections stream
/// (`daemon.StartedService`, sing-box 1.14+) is still a later pass, but
/// plain stdout/stderr log capture is not.
pub struct ProcessHandle {
    child: Child,
}

impl ProcessHandle {
    /// Spawns `binary run -c config_path` as a plain, unprivileged child.
    pub fn spawn(binary: &Path, config_path: &Path) -> io::Result<Self> {
        let mut cmd = Command::new(binary);
        cmd.arg("run").arg("-c").arg(config_path);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            // sing-box is a console subsystem binary; without this a
            // console window flashes (and lingers in the taskbar) behind
            // the Tauri window every time the proxy starts.
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let child = cmd.spawn()?;
        Ok(Self { child })
    }

    /// OS process id, while the child hasn't been reaped.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Takes ownership of the child's piped stdout/stderr handles, if not
    /// already taken -- `Command::stdout(Stdio::piped())`/`stderr` above
    /// guarantee both are `Some` the first time this is called (right after
    /// `spawn`), so callers get a real reader on both ends to feed into
    /// `core_manager::logs::spawn_line_reader`. Calling this more than once
    /// returns `None` for whichever side was already taken.
    pub fn take_stdio(&mut self) -> (Option<ChildStdout>, Option<ChildStderr>) {
        (self.child.stdout.take(), self.child.stderr.take())
    }

    /// Non-blocking liveness check. Also reaps the exit status as a side
    /// effect if the process has already died, so a subsequent `stop()`
    /// doesn't hang waiting on an already-exited child.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Best-effort human-readable exit status, if the process has already
    /// exited (i.e. the most recent `is_alive()` returned `false`).
    pub fn exit_description(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(format!("sing-box exited: {status}")),
            _ => None,
        }
    }

    /// Graceful stop: on Unix, SIGTERM followed by a grace period before
    /// SIGKILL if it didn't exit in time. On Windows there's no equivalent
    /// signal sing-box handles specially, so this goes straight to the
    /// process handle's `kill()` (`TerminateProcess`).
    pub async fn stop(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            if let Some(pid) = self.child.id() {
                // SAFETY: `pid` is this child's own pid, valid as long as
                // it hasn't been reaped (guaranteed — we haven't waited yet).
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
            }
            match tokio::time::timeout(GRACEFUL_STOP_TIMEOUT, self.child.wait()).await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(e)) => Err(e),
                Err(_elapsed) => {
                    // Didn't exit within the grace period — escalate.
                    self.child.kill().await
                }
            }
        }
        #[cfg(windows)]
        {
            self.child.kill().await?;
            let _ = self.child.wait().await;
            Ok(())
        }
    }
}
