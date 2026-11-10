//! `almide build` holds the shared build dir's lock from its cache check
//! through the copy to `-o` (#4269): writing the source, cargo and the copy
//! are one critical section, so a concurrent build, run or cache sweep of
//! the same dir waits for it instead of changing the binary under the copy.
//!
//! The negative control: a process holding the dir's lock. Before, a cache
//! hit took no lock at all and copied out while the holder could still be
//! rebuilding or evicting that binary. Now the build waits until the lock
//! is released, then installs the right program.
#![cfg(unix)]

use fs2::FileExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn scratch() -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4269-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("run")).expect("mkdir");
    std::fs::write(root.join("app.almd"), "fn main() -> Unit = println(\"built by the lock holder's peer\")\n").expect("write");
    root
}

fn build(root: &Path, out: &str) -> Command {
    // `ALMIDE_BIN` runs the same check against another compiler binary.
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let mut cmd = Command::new(bin);
    cmd.current_dir(root)
        .env("ALMIDE_RUN_PROJECT_DIR", root.join("run"))
        .args(["build", "app.almd", "-o", out])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}

fn runs_right(bin: &Path) {
    let out = Command::new(bin).output().expect("run the built binary");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "built by the lock holder's peer\n");
}

#[test]
fn a_build_waits_for_the_dir_lock_then_installs_its_own_program() {
    let root = scratch();
    // Warm the cache: the second build would be a lock-free hit before.
    let first = build(&root, "first").output().expect("spawn almide");
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    runs_right(&root.join("first"));

    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(root.join("run").join(".almide-build.lock"))
        .expect("open the dir's lockfile");
    lock.lock_exclusive().expect("hold the dir's lock");

    let mut child = build(&root, "second").spawn().expect("spawn almide");
    let held_until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < held_until {
        assert!(
            child.try_wait().expect("poll the build").is_none(),
            "the build finished while another process held the dir's lock"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!root.join("second").exists(), "the build installed while the lock was held");

    lock.unlock().expect("release the dir's lock");
    let out = child.wait_with_output().expect("wait for the build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    runs_right(&root.join("second"));
    let _ = std::fs::remove_dir_all(&root);
}
