//! `[permissions] fs` by descriptor (#4084, C-393): a call under the scope
//! runs beneath the granted root's held descriptor, so a tree rewritten
//! between the scope's decision and the call cannot steer the call out.
//!
//! What these prove, and what they do not:
//! - `a_dir_swapped_for_a_symlink_after_the_decision_*` place the swap
//!   exactly in the window the old guard left open: the scope decides
//!   (`fs_scoped_admits`, the check every guard makes first, which keeps its
//!   resolution for the call), then `data/sub` becomes a symlink to the
//!   outside, then the call runs. The path-based call at that instant — the
//!   pre-#4084 check-then-call — reaches the outside (asserted first, so the
//!   window is real); every scoped call is refused instead, and nothing
//!   outside is read, written, created, listed, renamed or removed.
//! - The descriptor walk itself against swaps between its own steps is
//!   tests/fs_beneath_core.rs; on Linux the walk is one openat2, exercised
//!   when these run there (CI).
//! - One test fn: the scope is process-wide, and cargo runs test fns on
//!   parallel threads.
#![cfg(unix)]

use almide_rt_core::fs_beneath_core::*;
use std::path::Path;

/// `data/sub/{a.txt,d/x.txt}` = "inside", `outside/sub/{a.txt,d/x.txt}` =
/// "outside"; the scope grants `data`.
fn tree() -> (tempfile::TempDir, std::path::PathBuf) {
    let t = tempfile::tempdir().expect("test fixture setup");
    let base = std::fs::canonicalize(t.path()).expect("test fixture setup");
    for (top, text) in [("data", "inside"), ("outside", "outside")] {
        std::fs::create_dir_all(base.join(top).join("sub/d")).expect("test fixture setup");
        std::fs::write(base.join(top).join("sub/a.txt"), text).expect("test fixture setup");
        std::fs::write(base.join(top).join("sub/d/x.txt"), text).expect("test fixture setup");
    }
    (t, base)
}

/// Replace `data/sub` by a symlink to `outside/sub`.
fn swap(base: &Path) {
    std::fs::rename(base.join("data/sub"), base.join("data/sub.orig")).expect("test fixture setup");
    std::os::unix::fs::symlink(base.join("outside/sub"), base.join("data/sub")).expect("test fixture setup");
}

fn refused<T: std::fmt::Debug>(r: std::io::Result<T>) -> bool {
    matches!(&r, Err(e) if fs_scoped_refused_in(&e.to_string()))
}

/// The outside tree is exactly as [`tree`] made it.
fn outside_untouched(base: &Path) {
    let o = base.join("outside/sub");
    let mut names: Vec<_> = std::fs::read_dir(&o).expect("test fixture setup").map(|e| e.expect("test fixture setup").file_name()).collect();
    names.sort();
    assert_eq!(names, ["a.txt", "d"], "an entry appeared or vanished outside");
    assert_eq!(std::fs::read_to_string(o.join("a.txt")).expect("test fixture setup"), "outside");
    assert_eq!(std::fs::read_to_string(o.join("d/x.txt")).expect("test fixture setup"), "outside");
}

#[test]
fn a_dir_swapped_for_a_symlink_after_the_decision_is_never_followed() {
    // Each call: the scope decides on the intact tree, the swap lands, the
    // call runs. A fresh tree per call, so one call's refusal cannot hide
    // another's escape.
    type Call = fn(&str, &str) -> std::io::Result<String>;
    let calls: &[(&str, &str, Call)] = &[
        ("read", "sub/a.txt", |p, _| almide_sfs::read_to_string(p)),
        ("read bytes", "sub/a.txt", |p, _| almide_sfs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned())),
        ("open", "sub/d/x.txt", |p, _| {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut almide_sfs::File::open(p)?, &mut s)?;
            Ok(s)
        }),
        ("write", "sub/a.txt", |p, _| almide_sfs::write(p, "pwned").map(|()| String::new())),
        ("write new", "sub/new.txt", |p, _| almide_sfs::write(p, "pwned").map(|()| String::new())),
        ("append", "sub/a.txt", |p, _| {
            let mut f = almide_sfs::OpenOptions::new().append(true).create(true).open(p)?;
            std::io::Write::write_all(&mut f, b"pwned").map(|()| String::new())
        }),
        ("mkdir_p", "sub/d/e/f", |p, _| almide_sfs::create_dir_all(p).map(|()| String::new())),
        ("list", "sub/d", |p, _| almide_sfs::read_dir(p).map(|r| r.map(|e| e.expect("test fixture setup").file_name().to_string_lossy().into_owned()).collect())),
        ("metadata", "sub/a.txt", |p, _| almide_sfs::metadata(p).map(|m| m.len().to_string())),
        ("remove", "sub/a.txt", |p, _| almide_sfs::remove_file(p).map(|()| String::new())),
        ("remove_all", "sub/d", |p, _| almide_sfs::remove_dir_all(p).map(|()| String::new())),
        ("rename", "sub/a.txt", |p, q| almide_sfs::rename(p, q).map(|()| String::new())),
        ("copy", "sub/a.txt", |p, q| almide_sfs::copy(p, q).map(|n| n.to_string())),
    ];
    for (name, rel, call) in calls {
        let (_t, base2) = tree();
        let data = base2.join("data").to_str().expect("test fixture setup").to_string();
        fs_scoped_install(Some(&[data]));
        let path = base2.join("data").join(rel).to_str().expect("test fixture setup").to_string();
        let dst = base2.join("data/copied.txt").to_str().expect("test fixture setup").to_string();
        assert!(fs_scoped_admits(&path), "{name}: the intact tree is inside the scope");
        swap(&base2);
        if *rel == "sub/a.txt" {
            // The window is real: a path-based read now reaches the outside.
            assert_eq!(std::fs::read_to_string(&path).expect("test fixture setup"), "outside");
        }
        let r = call(&path, &dst);
        assert!(refused(r.as_ref().map(|_| ()).map_err(|e| std::io::Error::new(e.kind(), e.to_string()))), "{name}: not refused: {r:?}");
        outside_untouched(&base2);
        assert!(!base2.join("data/copied.txt").exists(), "{name}");
    }

    // The predicates answer false, never the outside's answer.
    let (_t, base3) = tree();
    fs_scoped_install(Some(&[base3.join("data").to_str().expect("test fixture setup").to_string()]));
    let a = base3.join("data/sub/a.txt").to_str().expect("test fixture setup").to_string();
    assert!(fs_scoped_admits(&a));
    swap(&base3);
    assert!(!almide_sfs::exists(&a) && !almide_sfs::is_file(&a));

    // Without a swap the scoped calls are the plain calls, symlinks inside
    // the grant followed as the decision resolved them.
    let (_t, base4) = tree();
    fs_scoped_install(Some(&[base4.join("data").to_str().expect("test fixture setup").to_string()]));
    std::os::unix::fs::symlink(base4.join("data/sub"), base4.join("data/alias")).expect("test fixture setup");
    let q = |rel: &str| base4.join("data").join(rel).to_str().expect("test fixture setup").to_string();
    assert_eq!(almide_sfs::read_to_string(q("alias/a.txt")).expect("test fixture setup"), "inside");
    almide_sfs::create_dir_all(q("alias/n/m")).expect("test fixture setup");
    assert!(base4.join("data/sub/n/m").is_dir());
    almide_sfs::write(q("sub/n/m/f"), "1").expect("test fixture setup");
    let mut f = almide_sfs::OpenOptions::new().append(true).create(true).open(q("sub/n/m/f")).expect("test fixture setup");
    std::io::Write::write_all(&mut f, b"2").expect("test fixture setup");
    assert_eq!(std::fs::read_to_string(base4.join("data/sub/n/m/f")).expect("test fixture setup"), "12");
    assert_eq!(almide_sfs::copy(q("sub/n/m/f"), q("sub/g")).expect("test fixture setup"), 2);
    almide_sfs::rename(q("sub/g"), q("sub/h")).expect("test fixture setup");
    assert!(almide_sfs::is_file(q("sub/h")) && !almide_sfs::exists(q("sub/g")));
    assert!(almide_sfs::is_symlink(q("alias")) && almide_sfs::is_dir(q("alias")));
    // A no-follow call acts on the link itself.
    almide_sfs::remove_file(q("alias")).expect("test fixture setup");
    assert!(base4.join("data/sub/a.txt").exists() && !base4.join("data/alias").exists());
    almide_sfs::remove_dir_all(q("sub/n")).expect("test fixture setup");
    assert!(!base4.join("data/sub/n").exists());
    // The plain call's own errors, byte for byte.
    let plain = std::fs::read_to_string(base4.join("data/none/x")).unwrap_err().to_string();
    assert_eq!(almide_sfs::read_to_string(q("none/x")).unwrap_err().to_string(), plain);
    let plain = std::fs::create_dir_all(base4.join("data/sub/a.txt")).unwrap_err().to_string();
    assert_eq!(almide_sfs::create_dir_all(q("sub/a.txt")).unwrap_err().to_string(), plain);
    let plain = std::fs::create_dir_all(base4.join("data/sub/a.txt/z")).unwrap_err().to_string();
    assert_eq!(almide_sfs::create_dir_all(q("sub/a.txt/z")).unwrap_err().to_string(), plain);
    let plain = std::fs::read(base4.join("data/sub/a.txt/../a.txt")).unwrap_err().to_string();
    assert_eq!(almide_sfs::read(q("sub/a.txt/../a.txt")).unwrap_err().to_string(), plain);
    // Outside every root: refused, nothing touched.
    assert!(refused(almide_sfs::read_to_string(base4.join("outside/sub/a.txt"))));
    assert!(refused(almide_sfs::write(base4.join("data/../outside/sub/new"), "x")));
    assert!(!base4.join("outside/sub/new").exists());
    fs_scoped_install(None);
    assert_eq!(almide_sfs::read_to_string(base4.join("outside/sub/a.txt")).expect("test fixture setup"), "outside");
}
