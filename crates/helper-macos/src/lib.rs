//! macOS privileged helper — library half of this crate.
//!
//! Split into a lib (this file) + a thin `main.rs` bin, mirroring
//! `src-tauri`'s `ferroflow_lib`/`ferroflow` split, so `install`'s
//! one-time-setup logic is reusable by something other than this binary
//! later (e.g. `core-manager` or `src-tauri` directly, once the "app calls
//! this to build the osascript install script" wiring described in
//! `install.rs`'s doc comment is hooked up) without needing to shell out to
//! this binary for anything other than actually running as the daemon.
//!
//! Only meaningful when built for `*-apple-darwin`; on any other host both
//! modules below compile to nothing, so `cargo check --workspace` still
//! passes from Windows/Linux dev machines (see `main.rs` for the
//! non-macOS stub entry point).

#[cfg(target_os = "macos")]
mod paths;

#[cfg(target_os = "macos")]
pub mod install;
#[cfg(target_os = "macos")]
pub mod service;
