//! Memory: void watches its own physical footprint (what memguard measures: it kills any job over 6 GB, see the
//! swapwatch protocol) and sheds before it gets near. Soft (RAEA_VOID_MEM_SOFT_MB, default 3000): the frame cache
//! keeps only the program on the stage, at half its budget. Hard (RAEA_VOID_MEM_HARD_MB, default 4500): the frame
//! cache empties and listening stops, which frees its three models (~1.5 GB). The voice clip encoder's ~3 GB peak runs
//! in a child process (voice/tts.rs ENCODER) so it never counts here.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Soft,
    Hard,
}

/// This process's physical footprint in bytes (proc_pid_rusage's ri_phys_footprint, as Activity Monitor shows).
pub fn footprint() -> u64 {
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    let r = unsafe {
        libc::proc_pid_rusage(std::process::id() as i32, libc::RUSAGE_INFO_V4, &mut info as *mut _ as *mut libc::rusage_info_t)
    };
    if r == 0 { info.ri_phys_footprint } else { 0 }
}

fn mb(var: &str, default: u64) -> u64 {
    std::env::var(var).ok().and_then(|v| v.parse().ok()).unwrap_or(default) << 20
}

pub fn level(bytes: u64) -> Level {
    if bytes >= mb("RAEA_VOID_MEM_HARD_MB", 4500) {
        Level::Hard
    } else if bytes >= mb("RAEA_VOID_MEM_SOFT_MB", 3000) {
        Level::Soft
    } else {
        Level::Ok
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_its_own_footprint() {
        let b = super::footprint();
        assert!(b > 1 << 20 && b < 8 << 30, "{b}");
        assert_eq!(super::level(1 << 20), super::Level::Ok);
    }
}
