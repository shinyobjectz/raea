//! Whether a process still runs, asked of the system itself: a `kill` found on PATH can't be trusted (on Windows under
//! Git Bash it is MSYS's, which knows no Windows PID and called every live render dead, #29).

/// Whether process `pid` runs: Some(false) only when there's no such process, None when the system won't say (so a
/// caller deciding whether to delete what it left keeps it).
#[cfg(unix)]
pub fn running(pid: u32) -> Option<bool> {
    // 0 and anything past i32::MAX name process groups, not one process
    let p = i32::try_from(pid).ok().filter(|p| *p > 0)?;
    // signal 0 sends nothing: it only checks that the process exists and could be signalled
    if unsafe { libc::kill(p, 0) } == 0 {
        return Some(true);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => Some(false),
        // another user's process
        Some(libc::EPERM) => Some(true),
        _ => None,
    }
}

/// Whether process `pid` runs: Some(false) only when there's no such process, None when the system won't say (so a
/// caller deciding whether to delete what it left keeps it).
#[cfg(windows)]
pub fn running(pid: u32) -> Option<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            // no such process; access denied (another user's, a protected one) says nothing
            return (GetLastError() == ERROR_INVALID_PARAMETER).then_some(false);
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok.then_some(code == STILL_ACTIVE as u32)
    }
}

#[cfg(not(any(unix, windows)))]
pub fn running(_pid: u32) -> Option<bool> {
    None
}
