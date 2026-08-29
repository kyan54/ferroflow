//! Real implementation goes here: Unix socket accept loop, token auth,
//! `Command` dispatch (spawn/kill sing-box as root, install-core with
//! sha256 re-verification, route add/del restricted to an interface
//! allow-list, DNS cache flush). Mirrors `helper/helper.go` in the
//! Electron repo (`FlowZ/helper/helper.go`) — same responsibilities, but
//! this is a from-scratch Rust design (see `helper-proto` docs), not a
//! line-for-line port of the Go wire format.

#[cfg(target_os = "macos")]
pub async fn run() -> anyhow::Result<()> {
    unimplemented!("helper-macos: service::run")
}
