//! Unprivileged client for talking to whichever platform's running
//! privileged helper (`helper-macos`/`helper-windows`/`helper-linux`) over
//! `helper_proto`. One transport connection per request, matching every
//! helper's "read one `Request`, write one `Response`, close" server loop.
//!
//! Used by `core-manager` (to route privileged start/stop through the
//! helper instead of a plain child process) and by `src-tauri`'s
//! `commands::helper` module (install/uninstall/status), so this crate has
//! no Tauri dependency and no opinion on *where* the token gets persisted
//! — callers pass whatever token they have (or `None` on Linux, where
//! `SO_PEERCRED` is the trust boundary and no token exists).

use std::time::Duration;

use helper_proto::{read_message, write_message, Command, ProtoError, Request, Response};
use tokio::io::BufReader;

#[derive(Debug, thiserror::Error)]
pub enum HelperClientError {
    #[error("could not reach the helper: {0}")]
    Connect(#[source] std::io::Error),
    #[error("protocol error talking to the helper: {0}")]
    Proto(#[from] ProtoError),
    #[error("helper returned an error ({code}): {message}")]
    Helper { code: String, message: String },
    #[error("timed out waiting for the helper to respond")]
    Timeout,
}

/// How long a single request (connect + one round trip) is allowed to
/// take before this client gives up. Generous: `Start`/`InstallCore` do
/// real filesystem/process work on the other end, not just an in-memory
/// lookup.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub struct HelperClient {
    /// `Some` on macOS/Windows (shared-token auth). Always `None` on
    /// Linux — see the module doc comment.
    token: Option<String>,
}

impl HelperClient {
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }

    /// Cheap best-effort "is a helper installed and responding right now".
    /// Swallows every error (connection refused, timeout, wrong token —
    /// none of those distinguish "not installed" from "installed but
    /// unhealthy" in a way this method's boolean return could usefully
    /// convey; callers that need the distinction should call `status()`
    /// directly instead).
    pub async fn is_available(&self) -> bool {
        self.ping().await.is_ok()
    }

    pub async fn ping(&self) -> Result<(), HelperClientError> {
        self.send(Command::Ping).await.map(|_| ())
    }

    pub async fn version(&self) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::Version).await
    }

    pub async fn status(&self) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::Status).await
    }

    /// `core_path` is sent for wire-shape compatibility only -- every
    /// helper implementation ignores it and always spawns its own
    /// install-time-verified binary (see each helper's `handle_start`/
    /// `start_core`/`cmd_start` for why: trusting a caller-supplied
    /// executable path would let a compromised app process ask a
    /// privileged helper to run arbitrary code). Pass anything non-empty;
    /// `String::new()` is fine.
    pub async fn start(
        &self,
        config_path: impl Into<String>,
        core_path: impl Into<String>,
    ) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::Start { config_path: config_path.into(), core_path: core_path.into() }).await
    }

    pub async fn stop(&self) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::Stop).await
    }

    pub async fn cleanup(&self) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::Cleanup).await
    }

    pub async fn install_core(
        &self,
        path: impl Into<String>,
        sha256: impl Into<String>,
    ) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::InstallCore { path: path.into(), sha256: sha256.into() }).await
    }

    pub async fn free_port(&self, port: u16) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::FreePort { port }).await
    }

    pub async fn uninstall(&self) -> Result<serde_json::Value, HelperClientError> {
        self.send(Command::Uninstall).await
    }

    async fn send(&self, command: Command) -> Result<serde_json::Value, HelperClientError> {
        let request = Request { token: self.token.clone(), command };
        let response = tokio::time::timeout(REQUEST_TIMEOUT, transport::send(request))
            .await
            .map_err(|_elapsed| HelperClientError::Timeout)??;
        match response {
            Response::Ok { data } => Ok(data),
            Response::Err { code, message } => Err(HelperClientError::Helper { code, message }),
        }
    }
}

#[cfg(unix)]
mod transport {
    use super::*;
    use tokio::net::UnixStream;

    #[cfg(target_os = "macos")]
    const SOCKET_PATH: &str = helper_proto::endpoints::MACOS_SOCKET;
    #[cfg(target_os = "linux")]
    const SOCKET_PATH: &str = helper_proto::endpoints::LINUX_SOCKET;

    pub(super) async fn send(request: Request) -> Result<Response, HelperClientError> {
        let mut stream = UnixStream::connect(SOCKET_PATH).await.map_err(HelperClientError::Connect)?;
        write_message(&mut stream, &request).await?;
        let mut reader = BufReader::new(stream);
        Ok(read_message(&mut reader).await?)
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use tokio::net::windows::named_pipe::ClientOptions;
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;

    const PIPE_PATH: &str = helper_proto::endpoints::WINDOWS_PIPE;
    const CONNECT_ATTEMPTS: u32 = 10;
    const RETRY_DELAY: Duration = Duration::from_millis(100);

    pub(super) async fn send(request: Request) -> Result<Response, HelperClientError> {
        let mut client = connect_with_retry().await?;
        write_message(&mut client, &request).await?;
        let mut reader = BufReader::new(client);
        Ok(read_message(&mut reader).await?)
    }

    /// The server accepts one connection at a time per pipe instance; a
    /// client that connects in the brief window between one instance being
    /// consumed and the next `ServerOptions::create` call sees
    /// `ERROR_PIPE_BUSY`, not a hard failure -- retry a few times before
    /// giving up. Mirrors the standard Win32 named-pipe client pattern
    /// (`WaitNamedPipe` in C; tokio's named-pipe client docs recommend the
    /// same retry-on-busy loop since it has no built-in equivalent).
    async fn connect_with_retry() -> Result<tokio::net::windows::named_pipe::NamedPipeClient, HelperClientError> {
        for attempt in 0..CONNECT_ATTEMPTS {
            match ClientOptions::new().open(PIPE_PATH) {
                Ok(client) => return Ok(client),
                Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    if attempt + 1 == CONNECT_ATTEMPTS {
                        return Err(HelperClientError::Connect(err));
                    }
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(err) => return Err(HelperClientError::Connect(err)),
            }
        }
        unreachable!("loop above always returns before exhausting CONNECT_ATTEMPTS")
    }
}
