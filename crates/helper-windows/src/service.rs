//! Real implementation goes here: named-pipe accept loop (with SDDL ACL),
//! token auth, `Command` dispatch, SCM service registration
//! (`windows-service`), self-uninstall (spawn a detached sidecar so the
//! running service can delete its own locked exe / SCM entry — see
//! `selfuninstall.go` in the Electron repo for why that indirection is
//! needed).

#[cfg(windows)]
pub async fn run_console() -> anyhow::Result<()> {
    unimplemented!("helper-windows: service::run_console")
}

#[cfg(windows)]
pub fn run_as_service() -> anyhow::Result<()> {
    unimplemented!("helper-windows: service::run_as_service")
}
