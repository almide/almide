//! The CPU time a child process spends, for the doubling-ratio and growth
//! checks that time a native binary. Wall time also counts the time a loaded
//! runner spends running other tests' processes: a 4x linear cell read 13x on
//! a busy shard (#4428), a 2x doubling read 3.36 against a 3.0 limit (#4331).
//! The child's own user + system time (wait4's rusage) is what the body's
//! growth decides. Off unix it falls back to the wall time.

use std::process::Child;
use std::time::{Duration, Instant};

/// Reap `child` (spawned at `started`), waiting at most `limit` and killing
/// it past that (`None`). Returns its exit code (-1 for a signal) and its
/// user + system CPU time. The caller drains the child's pipes itself.
#[cfg(unix)]
pub fn reap_cpu_time(mut child: Child, started: Instant, limit: Duration) -> Option<(i32, Duration)> {
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: rusage is plain data that wait4 fills.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: valid pointers to the two locals above; the pid is this test's own child, not yet reaped.
        let r = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut ru) };
        if r == pid {
            break;
        }
        assert_eq!(r, 0, "wait4 failed: {}", std::io::Error::last_os_error());
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        // 1 ms: the cells this times take ~10 ms and up.
        std::thread::sleep(Duration::from_millis(1));
    }
    let tv = |t: libc::timeval| Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64);
    let code = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { -1 };
    Some((code, tv(ru.ru_utime) + tv(ru.ru_stime)))
}

#[cfg(not(unix))]
pub fn reap_cpu_time(mut child: Child, started: Instant, limit: Duration) -> Option<(i32, Duration)> {
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return Some((status.code().unwrap_or(-1), started.elapsed()));
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
