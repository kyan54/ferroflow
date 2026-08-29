//! Linux privileged helper — library half of this crate.
//!
//! Split into a lib (this file) + a thin `main.rs` bin, mirroring
//! `helper-macos`'s `lib.rs`/`main.rs` split, so `install`'s pure
//! script-builder functions are callable from `src-tauri` (to drive the
//! one-time `pkexec` install flow) without needing to shell out to this
//! binary for anything other than actually running as the daemon.
//!
//! Only meaningful when built for Linux; on any other host both modules
//! below compile to nothing, so `cargo check --workspace` still passes
//! from Windows/macOS dev machines (see `main.rs` for the non-Linux stub
//! entry point).

#[cfg(target_os = "linux")]
pub mod install;
#[cfg(target_os = "linux")]
pub mod paths;
#[cfg(target_os = "linux")]
pub mod service;
