//! sing-box child-process spawn/monitor + the helper-proto client used to
//! ask the privileged helper to do it instead when TUN mode needs root/
//! SYSTEM/ambient-caps. Falls back to a per-run elevated spawn (UAC/
//! osascript/pkexec) when no helper is installed yet, matching
//! `PlatformPrivilegeService.ts` in the Electron app.

pub struct ProcessHandle;
