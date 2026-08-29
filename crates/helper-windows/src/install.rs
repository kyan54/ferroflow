//! One-time `--install` / `--uninstall` path (needs an elevated/admin
//! shell to actually succeed — writing `C:\ProgramData\FerroFlow\...` and
//! registering a service both require admin rights). Not wired into the
//! Tauri app yet; that's `core-manager`'s job for a later pass (it would
//! shell out to this binary with `runas`/UAC once, then talk to the
//! running service over the pipe forever after).
//!
//! Mirrors the *behavior* of the Electron installer path
//! (`WindowsServiceHelper.ts` + the Go helper's own bootstrap), not the
//! wire format: generate the shared token, lock it down with an ACL,
//! register the SCM service (via the `windows-service` crate rather than
//! shelling out to `sc.exe create`), and start it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rand::RngCore;
use windows_service::service::{
    ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use helper_proto::endpoints;

pub const SERVICE_NAME: &str = "FerroFlowHelper";
const SERVICE_DISPLAY_NAME: &str = "FerroFlow Helper";
const SUPPORT_DIR: &str = r"C:\ProgramData\FerroFlow";

/// `helper-windows.exe --install`: generate the token, ACL it down to
/// SYSTEM + Administrators, register the service (auto-start, LocalSystem,
/// no arguments so it comes back up in service mode), and start it.
pub fn install() -> Result<()> {
    ensure_support_dir()?;
    let token_path = write_new_token()?;
    tracing::info!("wrote helper token to {}", token_path.display());

    let exe_path = std::env::current_exe().context("resolving current_exe for service registration")?;

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CREATE_SERVICE | ServiceManagerAccess::CONNECT,
    )
    .context("opening the Service Control Manager (are you running elevated?)")?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe_path,
        // No launch arguments: `main()` defaults to service mode when
        // called back by the SCM (only `--console`/`--install`/
        // `--uninstall` change that, and none of those are passed here).
        launch_arguments: vec![],
        dependencies: vec![],
        // `None` = run as LocalSystem, matching the Go helper and the
        // "always-available privileged access" requirement.
        account_name: None,
        account_password: None,
    };

    let service = match manager.create_service(
        &service_info,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS | ServiceAccess::CHANGE_CONFIG,
    ) {
        Ok(service) => service,
        Err(err) => {
            bail!(
                "failed to create service '{SERVICE_NAME}' ({err}). If it already exists, run \
                 --uninstall first."
            )
        }
    };

    service
        .start::<&str>(&[])
        .context("service created but failed to start")?;

    tracing::info!("service '{SERVICE_NAME}' installed and started");
    Ok(())
}

/// `helper-windows.exe --uninstall`: stop + delete the service, then clean
/// up the token/managed files. Reverses `install()`.
///
/// This *is* able to delete the service and its own on-disk files, unlike
/// the `Uninstall` pipe command (see the TODO in `service.rs::uninstall_cmd`)
/// — the difference is this runs as a plain elevated console process, not
/// as the running service itself, so nothing here is self-locked.
pub fn uninstall() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("opening the Service Control Manager (are you running elevated?)")?;

    match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
    ) {
        Ok(service) => {
            // Best-effort stop; it may already be stopped.
            let _ = service.stop();
            // Give the SCM a moment to actually transition it to Stopped
            // before we ask for delete — mirrors the Go self-uninstall
            // script's `ping`-as-sleep between `sc stop` and `sc delete`.
            std::thread::sleep(Duration::from_secs(2));
            service
                .delete()
                .context("failed to delete service (it may still be stopping)")?;
            tracing::info!("service '{SERVICE_NAME}' stopped and deleted");
        }
        Err(err) => {
            tracing::warn!("service '{SERVICE_NAME}' not found or couldn't be opened: {err}");
        }
    }

    let _ = std::fs::remove_file(endpoints::WINDOWS_TOKEN_FILE);
    let _ = std::fs::remove_dir_all(Path::new(SUPPORT_DIR).join("core"));
    tracing::info!("removed token file and managed core dir under {SUPPORT_DIR}");
    Ok(())
}

fn ensure_support_dir() -> Result<()> {
    std::fs::create_dir_all(SUPPORT_DIR)
        .with_context(|| format!("creating support dir {SUPPORT_DIR}"))?;
    Ok(())
}

/// Generate a fresh random token, write it to `WINDOWS_TOKEN_FILE`, and
/// lock the file down to SYSTEM + Administrators via `icacls` (simpler and
/// more robust across locale/OS-language than hand-rolling
/// `SetNamedSecurityInfo`; well-known SIDs `S-1-5-18` (SYSTEM) and
/// `S-1-5-32-544` (Administrators) sidestep localized group-name issues).
fn write_new_token() -> Result<PathBuf> {
    let token = generate_token();
    let path = PathBuf::from(endpoints::WINDOWS_TOKEN_FILE);
    std::fs::write(&path, &token).with_context(|| format!("writing token file {}", path.display()))?;
    lock_down_token_file(&path)?;
    Ok(path)
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn lock_down_token_file(path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    // /inheritance:r strips inherited ACEs (e.g. from ProgramData's
    // default "Users: read" ACE) so this really is SYSTEM+Administrators
    // only, not "that plus whatever ProgramData already granted".
    let status = std::process::Command::new("icacls")
        .arg(&path_str)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg("*S-1-5-18:F") // SYSTEM
        .arg("*S-1-5-32-544:F") // BUILTIN\Administrators
        .status()
        .context("spawning icacls to lock down the token file")?;
    if !status.success() {
        bail!("icacls exited with {status} while locking down {path_str}");
    }
    Ok(())
}
