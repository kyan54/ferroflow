//! Windows process/network primitives backing the `Start`/`Stop`/
//! `FreePort`/`Cleanup` dispatch in `service.rs`: a Job Object orphan
//! safety net, `GetExtendedTcpTable`-based listener lookup, and process
//! termination/image-path matching. Ported *in spirit* (not code — this is
//! a clean-room Rust design per `helper-proto`'s doc comment) from
//! `helper-win/winproc.go` in the Electron repo; see that file for the
//! original design rationale this mirrors.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_TCP_STATE_LISTEN,
    TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, CREATE_NEW_PROCESS_GROUP,
    CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};

/// Passed to `tokio::process::Command::creation_flags` when spawning
/// sing-box: new process group (so a console-mode helper could in
/// principle deliver CTRL_BREAK for graceful shutdown — see the extensive
/// comment in the Go `sendCtrlBreak`/`startSingbox` about why that's a
/// service-mode no-op, we still set the flag for parity/console dev use)
/// plus no console window (LocalSystem services shouldn't be popping up
/// black windows).
pub const CHILD_CREATION_FLAGS: u32 = CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;

/// Lazily-created Job Object that every spawned sing-box child is assigned
/// into, with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set. If this helper
/// process dies without cleanly stopping its child first (crash, `sc stop`
/// racing a hard kill, etc.) the kernel closes our handle to the job on
/// process exit and kills every process still assigned to it — so a
/// LocalSystem sing-box orphan can never outlive the helper, even though
/// service-mode has no console to deliver CTRL_BREAK to for a graceful
/// shutdown. Mirrors the Go helper's `ensureJob`/`hJob` (see
/// `helper-win/winproc.go`).
struct Job(HANDLE);
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
static JOB: OnceLock<Option<Job>> = OnceLock::new();

fn job_handle() -> Option<HANDLE> {
    JOB.get_or_init(create_job).as_ref().map(|j| j.0)
}

fn create_job() -> Option<Job> {
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        tracing::warn!(
            "CreateJobObjectW failed ({}); orphaned sing-box processes won't be caught by the \
             job-object safety net if the helper crashes",
            std::io::Error::last_os_error()
        );
        return None;
    }

    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        tracing::warn!(
            "SetInformationJobObject failed ({}); job created but KILL_ON_JOB_CLOSE not set",
            std::io::Error::last_os_error()
        );
        unsafe { CloseHandle(handle) };
        return None;
    }
    Some(Job(handle))
}

/// Best-effort: assign `pid` into the shared job so it's covered by the
/// kill-on-close safety net. Failure doesn't block `Start` — the explicit
/// `Stop`/`Cleanup` path is still the primary way children get reaped.
pub fn assign_to_job(pid: u32) {
    let Some(job) = job_handle() else { return };
    if pid == 0 {
        return;
    }
    let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
    if process.is_null() {
        return;
    }
    unsafe {
        let _ = AssignProcessToJobObject(job, process);
        CloseHandle(process);
    }
}

/// `OpenProcess(PROCESS_TERMINATE) + TerminateProcess`. Used by `FreePort`
/// and `Cleanup` to hard-kill a pid without a graceful-shutdown attempt
/// (service mode has no console to deliver CTRL_BREAK to, see
/// `CHILD_CREATION_FLAGS` doc comment).
pub fn terminate_pid(pid: u32) -> std::io::Result<()> {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    let ok = unsafe { TerminateProcess(handle, 1) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Full image path (`QueryFullProcessImageNameW`) of `pid`, or `None` if it
/// can't be queried (already exited, access denied, etc).
fn process_image_path(pid: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..size as usize]))
}

fn paths_equal_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Does the process at `pid` have an image path equal (case-insensitively —
/// Windows paths are case-insensitive) to `locked_path`? Used so
/// `FreePort`/`Cleanup` only ever kill processes matching a path this
/// helper itself started sing-box from, never an arbitrary same-named
/// process a client points us at.
pub fn image_matches(pid: u32, locked_path: &str) -> bool {
    match process_image_path(pid) {
        Some(path) => paths_equal_ci(&path, locked_path),
        None => false,
    }
}

fn basename(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Enumerate all running processes (`CreateToolhelp32Snapshot`) and
/// hard-kill every one whose full image path matches `locked_path`
/// (case-insensitively). Returns the number killed. Used by `Cleanup` as a
/// best-effort orphan sweep beyond whatever child this helper instance
/// happens to still be tracking (e.g. after a helper restart lost the
/// handle). Mirrors the Go helper's `killAllSingbox`.
pub fn kill_all_matching_image(locked_path: &str) -> usize {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot as isize == -1 {
        return 0;
    }

    let target_base = basename(locked_path);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut killed = 0usize;
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
    while ok != 0 {
        let pid = entry.th32ProcessID;
        if pid != 0 && pid != 4 {
            let exe_end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let exe_name = String::from_utf16_lossy(&entry.szExeFile[..exe_end]);
            // Cheap basename pre-filter (avoids opening a handle to every
            // process on the box) before the authoritative full-path check.
            if exe_name.eq_ignore_ascii_case(target_base) && image_matches(pid, locked_path)
                && terminate_pid(pid).is_ok() {
                    killed += 1;
                }
        }
        entry = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        ok = unsafe { Process32NextW(snapshot, &mut entry) };
    }

    unsafe { CloseHandle(snapshot) };
    killed
}

/// `GetExtendedTcpTable(TCP_TABLE_OWNER_PID_LISTENER)` over IPv4 + IPv6,
/// returning the deduplicated set of pids with a socket in the LISTEN
/// state on `port`. Mirrors the Go helper's `listenPidsForPort` (same
/// underlying Win32 API; the struct layouts happen to match exactly since
/// both come from the same Win32 metadata).
pub fn listen_pids_for_port(port: u16) -> std::io::Result<Vec<u32>> {
    let mut pids = Vec::new();
    collect_v4(port, &mut pids)?;
    collect_v6(port, &mut pids)?;
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn get_tcp_table(family: u32) -> std::io::Result<Vec<u8>> {
    let mut size: u32 = 0;
    // First call just to learn the required buffer size.
    let err = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            1,
            family,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if err != ERROR_INSUFFICIENT_BUFFER && err != 0 {
        return Err(std::io::Error::from_raw_os_error(err as i32));
    }
    if size == 0 {
        return Ok(Vec::new());
    }

    let mut buf = vec![0u8; size as usize];
    let err = unsafe {
        GetExtendedTcpTable(
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
            1,
            family,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if err != 0 {
        return Err(std::io::Error::from_raw_os_error(err as i32));
    }
    Ok(buf)
}

fn collect_v4(port: u16, out: &mut Vec<u32>) -> std::io::Result<()> {
    let buf = get_tcp_table(AF_INET as u32)?;
    // Table is `{ dwNumEntries: u32, table: [MIB_TCPROW_OWNER_PID; N] }`
    // with no padding between the two on x86/x64 (4-byte aligned rows) —
    // same layout Windows hands back regardless of which typed binding
    // reads it. Guard against the empty-table case (`size == 4`, i.e. just
    // the header with zero rows) before ever indexing past it.
    if buf.len() < 4 {
        return Ok(());
    }
    let n = u32::from_ne_bytes(buf[0..4].try_into().unwrap()) as usize;
    if n == 0 {
        return Ok(());
    }
    let row_size = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
    let available = (buf.len() - 4) / row_size;
    let n = n.min(available);
    let rows = unsafe {
        std::slice::from_raw_parts(buf.as_ptr().add(4) as *const MIB_TCPROW_OWNER_PID, n)
    };
    for row in rows {
        if row.dwState as i32 == MIB_TCP_STATE_LISTEN && local_port_from_net_order(row.dwLocalPort) == port {
            out.push(row.dwOwningPid);
        }
    }
    Ok(())
}

fn collect_v6(port: u16, out: &mut Vec<u32>) -> std::io::Result<()> {
    let buf = get_tcp_table(AF_INET6 as u32)?;
    if buf.len() < 4 {
        return Ok(());
    }
    let n = u32::from_ne_bytes(buf[0..4].try_into().unwrap()) as usize;
    if n == 0 {
        return Ok(());
    }
    let row_size = std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>();
    let available = (buf.len() - 4) / row_size;
    let n = n.min(available);
    let rows = unsafe {
        std::slice::from_raw_parts(buf.as_ptr().add(4) as *const MIB_TCP6ROW_OWNER_PID, n)
    };
    for row in rows {
        if row.dwState as i32 == MIB_TCP_STATE_LISTEN && local_port_from_net_order(row.dwLocalPort) == port {
            out.push(row.dwOwningPid);
        }
    }
    Ok(())
}

/// `dwLocalPort` comes back as a 32-bit host-order word whose low 16 bits
/// hold the port in *network* byte order — i.e. we need an `ntohs` on just
/// the bottom half. Matches the Go helper's `localPortFromNetOrder`.
fn local_port_from_net_order(p: u32) -> u16 {
    u16::from_be((p & 0xFFFF) as u16)
}
