//! Security table row 10: the rooted fs family and `[permissions] fs` leave
//! no path-based window, on every platform the toolchain builds for.
//!
//! - Where the descriptor calls exist (64-bit Linux and macOS,
//!   `FS_BENEATH_SUPPORTED`): an operation on a granted root ITSELF runs on
//!   the root's held descriptor, or from its parent's descriptor after the
//!   entry there is checked to be the held root, so a root replaced after the
//!   scope opened it is refused and the replacement is not touched; and the
//!   metadata of an entry the process may not open is taken by descriptor
//!   (no lstat of a path).
//! - Elsewhere (Windows, 32-bit — CI runs this file on windows-latest and on
//!   i686-unknown-linux-gnu): every rooted and scoped call below a root is
//!   refused with `Unsupported`, and the toolchain's build-time check refuses
//!   the scope. Nothing falls back to the old check-then-call.
//!
//! One test fn per platform half: the scope is process-wide.

use almide_rt_core::fs_beneath_core::*;

fn refused<T: std::fmt::Debug>(r: std::io::Result<T>) -> bool {
    matches!(&r, Err(e) if fs_scoped_refused_in(&e.to_string()))
}

#[test]
fn the_platform_constant_names_the_descriptor_platforms() {
    let expected = cfg!(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"));
    assert_eq!(FS_BENEATH_SUPPORTED, expected);
    assert!(fs_scope_platform_check(false).is_ok(), "no scope, nothing to refuse");
    assert_eq!(fs_scope_platform_check(true).is_ok(), expected, "the build-time check must refuse exactly where the calls would");
}

#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
#[test]
fn a_granted_root_itself_is_acted_on_by_descriptor() {
    let t = tempfile::tempdir().expect("test fixture setup");
    let base = std::fs::canonicalize(t.path()).expect("test fixture setup");
    let data = base.join("data");
    std::fs::create_dir_all(data.join("sub")).expect("test fixture setup");
    std::fs::write(data.join("a.txt"), "inside").expect("test fixture setup");
    let root = data.to_string_lossy().into_owned();
    fs_scoped_install(Some(&[root.clone()][..]));
    let sub = |rel: &str| data.join(rel).to_string_lossy().into_owned();

    // The scope opens and holds the root.
    assert_eq!(almide_sfs::read_to_string(sub("a.txt")).expect("read under the root"), "inside");

    // The metadata of an entry the process may not open, by descriptor. (As
    // root the open succeeds and the fallback is not reached; the answer must
    // be the same either way.)
    let locked = data.join("locked.txt");
    std::fs::write(&locked, "12345").expect("test fixture setup");
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).expect("test fixture setup");
    let m = almide_sfs::metadata(sub("locked.txt")).expect("metadata of an unreadable file");
    assert!(m.is_file() && m.len() == 5, "unreadable file described: {m:?}");
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o644)).expect("test fixture setup");

    // The root itself, while it is the held directory.
    assert!(almide_sfs::metadata(&root).expect("root metadata").is_dir());
    almide_sfs::create_dir_all(&root).expect("create_dir_all of an existing root is done");

    // The root replaced after the scope opened it: the entry at the root's
    // name is no longer the held directory. Every op on the root's NAME is
    // refused and the replacement survives; the reads answer the held one.
    std::fs::rename(&data, base.join("data.orig")).expect("test fixture setup");
    std::fs::create_dir_all(data.join("other")).expect("test fixture setup");
    std::fs::write(data.join("other/keep.txt"), "replacement").expect("test fixture setup");
    assert!(refused(almide_sfs::remove_dir_all(&root)), "remove_dir_all of a replaced root must be refused");
    assert!(refused(almide_sfs::remove_dir(&root)), "remove_dir of a replaced root must be refused");
    assert!(refused(almide_sfs::rename(&root, sub("other/moved"))), "rename of a replaced root must be refused");
    assert_eq!(std::fs::read_to_string(data.join("other/keep.txt")).expect("the replacement survives"), "replacement");
    let names: Vec<_> = almide_sfs::read_dir(&root).expect("list the held root").map(|e| e.expect("entry").file_name()).collect();
    assert!(names.iter().any(|n| n == "a.txt") && !names.iter().any(|n| n == "other"), "the root's listing is the held directory's: {names:?}");

    // Put the held root back: removing it by its name now works, and the
    // scope lets the descriptor go, so a root created again is the one used.
    std::fs::remove_dir_all(&data).expect("test fixture setup");
    std::fs::rename(base.join("data.orig"), &data).expect("test fixture setup");
    almide_sfs::remove_dir_all(&root).expect("remove_dir_all of the held root");
    assert!(!data.exists(), "the root itself is removed");
    almide_sfs::create_dir_all(&root).expect("create the root again");
    almide_sfs::write(sub("b.txt"), "again").expect("write under the new root");
    assert_eq!(std::fs::read_to_string(data.join("b.txt")).expect("written under the new root"), "again");
    fs_scoped_install(None);
}

#[cfg(not(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64")))]
#[test]
fn every_call_below_a_root_is_refused_where_descriptors_are_missing() {
    let t = tempfile::tempdir().expect("test fixture setup");
    let base = std::fs::canonicalize(t.path()).expect("test fixture setup");
    let data = base.join("data");
    std::fs::create_dir_all(data.join("sub")).expect("test fixture setup");
    std::fs::write(data.join("sub/a.txt"), "inside").expect("test fixture setup");
    let unsupported = |e: &std::io::Error| e.kind() == std::io::ErrorKind::Unsupported && e.to_string().contains(FS_BENEATH_UNSUPPORTED);
    let dir = data.to_string_lossy().into_owned();

    // The rooted family below its root.
    let rooted: Vec<(&str, Result<(), FsBeneathError>)> = vec![
        ("read", fs_beneath_read(&dir, "sub/a.txt").map(drop)),
        ("write", fs_beneath_write(&dir, "sub/a.txt", b"pwned")),
        ("list", fs_beneath_list(&dir, "sub").map(drop)),
        ("exists", fs_beneath_exists(&dir, "sub/a.txt").map(drop)),
        ("remove", fs_beneath_remove(&dir, "sub/a.txt")),
    ];
    for (name, r) in rooted {
        assert!(matches!(&r, Err(FsBeneathError::Io(e)) if unsupported(e)), "rooted {name} must be refused, got {r:?}");
    }
    assert_eq!(std::fs::read_to_string(data.join("sub/a.txt")).expect("untouched"), "inside");

    // Every scoped call, under an installed scope.
    fs_scoped_install(Some(&[dir.clone()][..]));
    let p = data.join("sub/a.txt").to_string_lossy().into_owned();
    let q = data.join("sub/b.txt").to_string_lossy().into_owned();
    let scoped: Vec<(&str, std::io::Result<()>)> = vec![
        ("read", almide_sfs::read(&p).map(drop)),
        ("write", almide_sfs::write(&p, "pwned")),
        ("metadata", almide_sfs::metadata(&p).map(drop)),
        ("read_dir", almide_sfs::read_dir(&dir).map(drop)),
        ("create_dir_all", almide_sfs::create_dir_all(&q)),
        ("remove_file", almide_sfs::remove_file(&p)),
        ("rename", almide_sfs::rename(&p, &q)),
        ("create_new_file", almide_sfs::create_new_file(&q).map(drop)),
    ];
    for (name, r) in scoped {
        assert!(matches!(&r, Err(e) if unsupported(e)), "scoped {name} must be refused, got {r:?}");
    }
    fs_scoped_install(None);
    assert_eq!(std::fs::read_to_string(data.join("sub/a.txt")).expect("untouched"), "inside");
    assert!(!data.join("sub/b.txt").exists());
}
