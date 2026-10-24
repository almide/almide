//! C-417 (#4125): process.kill / is_alive validate before the syscall, so a
//! pid that names a process group or "every process" never reaches kill(2),
//! and a failed kill(2) answers a fixed errno text.
use almide_rt_core::process_core::{almide_proc_is_alive, almide_proc_kill};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALLS: AtomicUsize = AtomicUsize::new(0);

fn counting(_pid: i32, _sig: i32) -> Result<(), i32> {
    CALLS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[test]
fn refused_operands_never_reach_the_syscall() {
    let before = CALLS.load(Ordering::SeqCst);
    for pid in [0i64, -1, -42, i64::from(i32::MAX) + 1, i64::MIN] {
        assert_eq!(
            almide_proc_kill(pid, 0, counting),
            Err(format!(
                "process.kill({pid}, 0): pid must be in 1..=2147483647"
            ))
        );
        assert!(!almide_proc_is_alive(pid, counting));
    }
    for sig in [-1i64, 32, 64, i64::MAX] {
        assert_eq!(
            almide_proc_kill(7, sig, counting),
            Err(format!("process.kill(7, {sig}): signal must be in 0..=31"))
        );
    }
    assert_eq!(CALLS.load(Ordering::SeqCst), before);
    assert_eq!(almide_proc_kill(7, 0, counting), Ok(()));
    assert_eq!(CALLS.load(Ordering::SeqCst), before + 1);
}

#[test]
fn errno_texts_are_fixed() {
    assert_eq!(
        almide_proc_kill(7, 9, |_, _| Err(3)),
        Err("process.kill(7, 9): no such process".to_string())
    );
    assert_eq!(
        almide_proc_kill(7, 9, |_, _| Err(1)),
        Err("process.kill(7, 9): operation not permitted".to_string())
    );
    assert_eq!(
        almide_proc_kill(7, 9, |_, _| Err(22)),
        Err("process.kill(7, 9): invalid signal".to_string())
    );
    assert_eq!(
        almide_proc_kill(7, 9, |_, _| Err(99)),
        Err("process.kill(7, 9): signal failed (errno 99)".to_string())
    );
    // EPERM: the process exists (someone else's); ESRCH: it does not.
    assert!(almide_proc_is_alive(7, |_, _| Err(1)));
    assert!(!almide_proc_is_alive(7, |_, _| Err(3)));
}
