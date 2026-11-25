//! A process that left a work folder is asked for by raea-runtime-pid (the system itself, never a `kill` on PATH): its
//! tests, here beside the sweep that uses it.
#[cfg(test)]
mod tests {
    use raea_runtime_pid::running;

    /// A child that waits until its stdin closes, on any system.
    fn child() -> std::process::Child {
        #[cfg(windows)]
        let mut c = std::process::Command::new("cmd");
        #[cfg(windows)]
        c.args(["/c", "more"]);
        #[cfg(not(windows))]
        let mut c = std::process::Command::new("cat");
        c.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap()
    }

    #[test]
    fn a_process_runs_until_it_ends() {
        assert_eq!(running(std::process::id()), Some(true));
        let mut c = child();
        let pid = c.id();
        assert_eq!(running(pid), Some(true));
        drop(c.stdin.take());
        c.wait().unwrap();
        // reaped, so its PID names nothing (until the system hands it out again, which takes far longer than this)
        assert_eq!(running(pid), Some(false));
    }

    /// The sweep each render starts with (cache.rs) removes a work folder only once its render's process is gone
    #[test]
    fn the_sweep_keeps_a_running_renders_folder() {
        let dir = std::env::temp_dir().join(format!("raea-pid-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut live = child();
        let mut ended = child();
        drop(ended.stdin.take());
        ended.wait().unwrap();
        let keep = [format!("raea-cli-b{}", live.id()), format!("raea-cli-r{}-ThreadId(2)", live.id()), "raea-cli-bx".into()];
        let go = [format!("raea-cli-b{}", ended.id()), format!("raea-cli-r{}-ThreadId(2)", ended.id())];
        for n in keep.iter().chain(&go) {
            std::fs::create_dir_all(dir.join(n)).unwrap();
        }
        crate::cache::sweep_stale_work_in(&dir, std::time::SystemTime::now());
        let left: Vec<bool> = keep.iter().chain(&go).map(|n| dir.join(n).exists()).collect();
        drop(live.stdin.take());
        live.wait().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(left, [true, true, true, false, false], "kept: {keep:?}, removed: {go:?}");
    }

    /// Two hours untouched removes a folder only when its render isn't known to run: a long render stops touching its
    /// folder's top level well before it ends (#31)
    #[test]
    fn the_age_rule_spares_a_running_render() {
        let dir = std::env::temp_dir().join(format!("raea-pid-age-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut live = child();
        let mut ended = child();
        drop(ended.stdin.take());
        ended.wait().unwrap();
        let names = [
            format!("raea-cli-b{}", live.id()),
            format!("raea-cli-r{}-ThreadId(2)", std::process::id()),
            format!("raea-cli-b{}", ended.id()),
            "raea-cli-bx".to_string(),
        ];
        for n in &names {
            std::fs::create_dir_all(dir.join(n)).unwrap();
        }
        // three hours from now, every folder is two hours untouched
        crate::cache::sweep_stale_work_in(&dir, std::time::SystemTime::now() + std::time::Duration::from_secs(3 * 3600));
        let left: Vec<bool> = names.iter().map(|n| dir.join(n).exists()).collect();
        drop(live.stdin.take());
        live.wait().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        // a running render's (another process's, and this one's) stay; an ended one's and one naming no process go
        assert_eq!(left, [true, true, false, false], "{names:?}");
    }

    #[test]
    fn a_pid_naming_no_one_process_is_unknown_or_gone() {
        // 0 and pids past i32::MAX are groups to kill(2), never "running"
        assert_ne!(running(0), Some(true));
        assert_ne!(running(u32::MAX), Some(true));
    }
}
