//! The rooted fs family by descriptor (#4084, C-392): a symlink anywhere
//! under the root is refused by the call itself, so a directory swapped for a
//! symlink between a check and the use cannot steer the call out of the root.
//!
//! What these prove, and what they do not:
//! - `a_dir_swapped_for_a_symlink_mid_walk_*` rewrite the tree INSIDE the
//!   walk (the `hook` runs between the walk's own steps), at every point a
//!   concurrent attacker could. The old resolve-then-open path, asked at the
//!   same moment, reads the outside file — the test asserts that first, so
//!   the window is real — while the descriptor walk either refuses (the swap
//!   landed before the walk reached the segment) or reads the original
//!   directory it already holds (the swap landed after). It never answers
//!   the outside bytes.
//! - The per-segment walk is what runs on macOS; on Linux the default path is
//!   openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS), exercised by the other
//!   tests here when they run on Linux (CI), and the walk by `walk_only`.
#![cfg(unix)]

use almide_rt_core::fs_beneath_core::*;
use std::path::Path;

/// `root/sub/deep/a.txt` = "inside", `outside/sub/deep/a.txt` = "outside".
fn tree() -> (tempfile::TempDir, String, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(t.path()).unwrap();
    for (top, text) in [("root", "inside"), ("outside", "outside")] {
        std::fs::create_dir_all(base.join(top).join("sub/deep")).unwrap();
        std::fs::write(base.join(top).join("sub/deep/a.txt"), text).unwrap();
    }
    let root = base.join("root").to_str().unwrap().to_string();
    (t, root, base)
}

/// Replace `root/sub` by a symlink to `outside/sub` (the original kept aside).
fn swap(base: &Path) {
    std::fs::rename(base.join("root/sub"), base.join("root/sub.orig")).unwrap();
    std::os::unix::fs::symlink(base.join("outside/sub"), base.join("root/sub")).unwrap();
}

fn escapes<T: std::fmt::Debug>(r: Result<T, FsBeneathError>) -> bool {
    matches!(r, Err(FsBeneathError::Escape))
}

#[test]
fn a_dir_swapped_for_a_symlink_mid_walk_is_never_followed() {
    // The swap lands at each step of the walk in turn: before the root's
    // child `sub` is opened (hook 0), after `sub` is held (1), after `deep`
    // is held (2).
    for at in 0..=2usize {
        let (_t, root, base) = tree();
        let mut swapped = false;
        let r = fs_beneath_read_with(&root, "sub/deep/a.txt", true, &mut |i| {
            if i == at && !swapped {
                swap(&base);
                swapped = true;
                // The window is real: a path-based read at this instant —
                // the pre-#4084 check-then-use — reaches the outside file.
                assert_eq!(std::fs::read_to_string(format!("{root}/sub/deep/a.txt")).unwrap(), "outside");
            }
        });
        assert!(swapped, "the hook never ran at {at}");
        match r {
            Err(FsBeneathError::Escape) => assert_eq!(at, 0, "refused although the walk already held sub"),
            Ok(bytes) => {
                assert_ne!(at, 0, "the walk followed the symlink it met");
                assert_eq!(bytes, b"inside", "the walk left the directory it held");
            }
            Err(FsBeneathError::Io(e)) => panic!("swap at {at}: {e}"),
        }
    }
}

#[test]
fn a_symlink_at_any_segment_is_refused_by_every_call() {
    let (_t, root, base) = tree();
    swap(&base);
    std::os::unix::fs::symlink(base.join("outside/sub/deep/a.txt"), base.join("root/flink")).unwrap();
    assert!(escapes(fs_beneath_read(&root, "sub/deep/a.txt")));
    assert!(escapes(fs_beneath_read_with(&root, "sub/deep/a.txt", true, &mut |_| {})));
    assert!(escapes(fs_beneath_read(&root, "flink")));
    assert!(escapes(fs_beneath_write(&root, "sub/deep/new.txt", b"x")));
    assert!(escapes(fs_beneath_write(&root, "flink", b"x")));
    assert!(escapes(fs_beneath_list(&root, "sub")));
    assert!(escapes(fs_beneath_list(&root, "sub/deep")));
    assert!(escapes(fs_beneath_exists(&root, "flink")));
    assert!(escapes(fs_beneath_exists(&root, "sub/deep/a.txt")));
    assert!(escapes(fs_beneath_remove(&root, "flink")));
    assert!(escapes(fs_beneath_remove(&root, "sub/deep/a.txt")));
    assert_eq!(std::fs::read_to_string(base.join("outside/sub/deep/a.txt")).unwrap(), "outside");
    assert!(!base.join("outside/sub/deep/new.txt").exists());
    assert!(base.join("root/flink").exists(), "the refused remove took the link");
}

#[test]
fn a_path_without_symlinks_is_the_plain_call() {
    let (_t, root, base) = tree();
    assert_eq!(fs_beneath_read_text(&root, "sub/deep/a.txt").unwrap(), "inside");
    fs_beneath_write(&root, "sub/b.txt", b"new").unwrap();
    assert_eq!(std::fs::read_to_string(base.join("root/sub/b.txt")).unwrap(), "new");
    assert_eq!(fs_beneath_list(&root, "sub").unwrap(), vec!["b.txt".to_string(), "deep".to_string()]);
    assert_eq!(fs_beneath_list(&root, "").unwrap(), vec!["sub".to_string()]);
    assert!(fs_beneath_exists(&root, "sub/b.txt").unwrap());
    assert!(!fs_beneath_exists(&root, "sub/none").unwrap());
    assert!(!fs_beneath_exists(&root, "none/x").unwrap());
    fs_beneath_remove(&root, "sub/b.txt").unwrap();
    assert!(!base.join("root/sub/b.txt").exists());
    fs_beneath_remove(&root, "sub/deep/a.txt").unwrap();
    fs_beneath_remove(&root, "sub/deep").unwrap();
    assert!(!base.join("root/sub/deep").exists());
    // The plain call's own errors.
    assert!(matches!(fs_beneath_read(&root, "none/x"), Err(FsBeneathError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound));
    assert!(matches!(fs_beneath_read(&root, "sub"), Err(FsBeneathError::Io(e)) if e.raw_os_error() == Some(21)));
    // A segment the Almide side never sends is refused, not trusted.
    for rel in ["../outside/sub/deep/a.txt", "sub/../../outside", "/etc/hosts", "sub//deep", "./sub"] {
        assert!(escapes(fs_beneath_read(&root, rel)), "{rel}");
    }
    assert_eq!(fs_beneath_joined("/r", ""), "/r");
    assert_eq!(fs_beneath_joined("/r", "a/b"), "/r/a/b");
}

/// A thread flips `root/sub` between a directory and a symlink to the
/// outside while this one reads through it as fast as it can: no read ever
/// answers the outside bytes. Probabilistic where the hook test above is
/// exact; it pins the same property end to end on whichever path (openat2 or
/// the walk) the platform takes.
#[test]
fn a_concurrent_flip_never_leaks_the_outside() {
    let (_t, root, base) = tree();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flipper = {
        let (base, stop) = (base.clone(), stop.clone());
        std::thread::spawn(move || {
            let (sub, aside) = (base.join("root/sub"), base.join("root/sub.real"));
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                std::fs::rename(&sub, &aside).unwrap();
                std::os::unix::fs::symlink(base.join("outside/sub"), &sub).unwrap();
                std::fs::remove_file(&sub).unwrap();
                std::fs::rename(&aside, &sub).unwrap();
            }
        })
    };
    let (mut inside, mut refused) = (0, 0);
    for _ in 0..20_000 {
        match fs_beneath_read(&root, "sub/deep/a.txt") {
            Ok(b) => {
                assert_eq!(b, b"inside", "a read followed the flipped symlink out");
                inside += 1;
            }
            Err(_) => refused += 1,
        }
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    flipper.join().unwrap();
    assert!(inside + refused == 20_000);
}
