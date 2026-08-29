//! SDDL -> `SECURITY_ATTRIBUTES` conversion for the named pipe ACL.
//!
//! Defense in depth on top of the shared-token auth (see the module doc in
//! `helper_proto`): even a local, non-admin process that somehow guessed
//! or leaked the token still has to be running as SYSTEM or an
//! interactively-logged-on user to even open a handle to the pipe.
//!
//! `D:(A;;FA;;;SY)(A;;GRGW;;;IU)`:
//!   - `D:`                    DACL follows (no explicit owner/group/SACL
//!                             — owner defaults to SYSTEM as creator).
//!   - `(A;;FA;;;SY)`          Allow FILE_ALL_ACCESS to SYSTEM (SY) — the
//!                             service itself.
//!   - `(A;;GRGW;;;IU)`        Allow GENERIC_READ|GENERIC_WRITE to
//!                             Interactive Users (IU) — enough to connect
//!                             and exchange NDJSON lines, not enough to
//!                             touch the pipe's ACL/handle-count.
//! An explicit DACL means no implicit Everyone grant, so non-interactive
//! callers (services, scheduled tasks, network logons) are refused at the
//! OS level regardless of whether they have the token. Same SDDL and
//! rationale as the Electron app's Go helper (`helper-win/service.go`).
//!
//! Chose IU over BU/AU deliberately: the FerroFlow app runs in the current
//! interactive desktop session, and IU covers exactly that without
//! widening to remote/service/network logons.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

pub const PIPE_SDDL: &str = "D:(A;;FA;;;SY)(A;;GRGW;;;IU)";

/// Owns both the `SECURITY_ATTRIBUTES` struct and the security descriptor
/// it points to, so callers get a single RAII value whose `as_raw_ptr()`
/// is valid for the lifetime of this struct. The descriptor is allocated
/// by `ConvertStringSecurityDescriptorToSecurityDescriptorW` (documented
/// as `LocalAlloc`-backed) and must be freed with `LocalFree` — done in
/// `Drop`.
pub struct PipeSecurity {
    descriptor: PSECURITY_DESCRIPTOR,
    attrs: SECURITY_ATTRIBUTES,
}

impl PipeSecurity {
    pub fn from_sddl(sddl: &str) -> std::io::Result<Self> {
        let wide: Vec<u16> = std::ffi::OsStr::new(sddl)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };

        Ok(Self { descriptor, attrs })
    }

    /// Raw pointer suitable for
    /// `ServerOptions::create_with_security_attributes_raw`. Only valid
    /// for the lifetime of `self`.
    pub fn as_raw_ptr(&mut self) -> *mut c_void {
        &mut self.attrs as *mut SECURITY_ATTRIBUTES as *mut c_void
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        if !self.descriptor.is_null() {
            unsafe {
                LocalFree(self.descriptor);
            }
        }
    }
}
