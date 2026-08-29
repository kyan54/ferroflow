//! Real implementation goes here: Unix socket accept loop, `SO_PEERCRED`
//! auth against the allow-list, `Command` dispatch, fork+setuid+ambient-caps
//! spawn of sing-box. Mirrors `helper-linux/helper.go` in the Electron repo
//! for *behavior*; this is a from-scratch Rust design for the wire format.

#[cfg(target_os = "linux")]
pub async fn run() -> anyhow::Result<()> {
    unimplemented!("helper-linux: service::run")
}
