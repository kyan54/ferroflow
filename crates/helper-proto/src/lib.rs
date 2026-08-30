//! Wire protocol shared by `helper-macos`/`helper-windows`/`helper-linux`
//! (the privileged, always-on background services that own the sing-box
//! process and any operation that needs elevated rights) and `core-manager`
//! (the client, living in the Tauri app, unprivileged).
//!
//! This is a clean-room design, not wire-compatible with upstream FlowZ's
//! Go helpers — we don't need interop, only the same *authorization model*
//! (install once with one elevation prompt, then talk to the running
//! service with zero further prompts):
//!   - macOS:   LaunchDaemon, Unix domain socket, shared-token auth.
//!   - Windows: Windows Service (LocalSystem), named pipe with an ACL
//!              restricting connections to the interactive user + SYSTEM,
//!              plus the same shared-token auth as defense in depth.
//!   - Linux:   systemd service, Unix domain socket, `SO_PEERCRED`-based
//!              auth against a root-owned allow-list (no token needed —
//!              the kernel already tells us the peer's UID).
//!
//! Transport is one JSON object per line (NDJSON) instead of the
//! hand-rolled positional line-protocol upstream used, specifically so the
//! three helper crates (written by three different subagents) don't each
//! need to reimplement bespoke field-order parsing.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

pub const PROTO_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Shared-secret token (macOS/Windows). `None` on Linux, where the
    /// kernel-verified peer UID (`SO_PEERCRED`) is the trust boundary.
    #[serde(default)]
    pub token: Option<String>,
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Version,
    /// Is the managed sing-box process currently running.
    Status,
    /// Start sing-box with the given config file, as the helper's
    /// privileged identity (root on mac, LocalSystem on Windows, or the
    /// calling user + ambient net capabilities on Linux).
    Start { config_path: String, core_path: String },
    Stop,
    /// Kill any orphaned sing-box process + undo system-proxy/route state.
    Cleanup,
    /// Hot-swap the managed sing-box binary; helper re-verifies the hash
    /// before accepting it so a compromised app process can't smuggle in
    /// an arbitrary root-run binary.
    InstallCore { path: String, sha256: String },
    FreePort { port: u16 },
    Uninstall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Response {
    Ok { data: serde_json::Value },
    Err { code: String, message: String },
}

impl Response {
    pub fn ok(data: impl Serialize) -> Self {
        Self::Ok { data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null) }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Err { code: code.into(), message: message.into() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("connection closed before a full line was read")]
    ConnectionClosed,
}

pub async fn write_message<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    message: &T,
) -> Result<(), ProtoError> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_message<R, T>(reader: &mut BufReader<R>) -> Result<T, ProtoError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Err(ProtoError::ConnectionClosed);
    }
    Ok(serde_json::from_str(line.trim_end())?)
}

/// Well-known transport endpoint names, so `core-manager` and the three
/// helper crates agree on a location without a config file.
pub mod endpoints {
    pub const MACOS_SOCKET: &str = "/Library/Application Support/FerroFlow/helper.sock";
    pub const MACOS_TOKEN_FILE: &str = "/Library/Application Support/FerroFlow/helper.token";
    pub const LINUX_SOCKET: &str = "/run/ferroflow/helper.sock";
    pub const LINUX_AUTHFILE: &str = "/var/lib/ferroflow/authorized-uids";
    pub const WINDOWS_PIPE: &str = r"\\.\pipe\ferroflow-helper";
    pub const WINDOWS_TOKEN_FILE: &str = r"C:\ProgramData\FerroFlow\helper.token";
    /// Filename (not a full path -- always written next to whichever
    /// `ferroflow-helper-windows.exe` actually ran) that `--install`/
    /// `--uninstall` write their error message to on failure, so the
    /// unprivileged caller (which can't see an elevated child's
    /// stdout/stderr -- see `src-tauri/src/commands/helper_windows.rs`'s
    /// `run_elevated` doc comment) can read the real reason back instead
    /// of just an exit code.
    pub const WINDOWS_INSTALL_ERROR_LOG_NAME: &str = "helper-install-error.log";
}
