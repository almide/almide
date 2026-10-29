//! Security table row 10 (#4084, #4293): the rooted fs family and
//! `[permissions] fs` cannot leave their root, with no path-based window, on
//! every platform the toolchain builds for. CI runs this file on Linux
//! (workspace), on i686-unknown-linux-gnu (`fs-beneath-32bit`), on macOS
//! (`test-arm`) and on Windows (`test-wasm-windows`).
//!
//! - `rooted_calls_never_leave_their_root`: `..`, absolute paths, and on
//!   Windows a drive, `\\?\`, device or UNC prefix and the `:stream` syntax
//!   are refused before anything is opened; a symlink (on Windows also a
//!   junction) met at any segment is refused by the call itself; and a
//!   directory swapped for a link BETWEEN the walk's own steps is never
//!   followed — the path-based read at that instant reaches the outside
//!   (asserted first, so the window is real).
//! - `scoped_calls_never_leave_their_grant`: the same under `[permissions]
//!   fs`, with the swap landing after the scope's decision and before the
//!   call, plus the plain calls the scope must still make.
//! - `a_granted_root_itself_is_acted_on_by_descriptor`: an operation on the
//!   granted root ITSELF runs on the root's held descriptor, or from its
//!   parent's after the entry there is checked to be the held root (device
//!   and inode; on Windows volume serial and file index), so a root replaced
//!   after the scope opened it is refused and the replacement left alone.
//! - Where the platform has no descriptor calls (neither Linux, macOS nor
//!   Windows) every rooted and scoped call below a root is refused instead.
//!
//! The scope is process-wide and cargo runs test fns on parallel threads, so
//! every test that installs it (and every rooted call, whose root is opened
//! through the scope) holds `SERIAL`.

use almide_rt_core::fs_beneath_core::*;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

fn refused<T: std::fmt::Debug>(r: std::io::Result<T>) -> bool {
    matches!(&r, Err(e) if fs_scoped_refused_in(&e.to_string()))
}

#[test]
fn the_platform_constant_names_the_descriptor_platforms() {
    let expected = cfg!(any(target_os = "linux", target_os = "macos", windows));
    assert_eq!(FS_BENEATH_SUPPORTED, expected);
    assert!(fs_scope_platform_check(false).is_ok(), "no scope, nothing to refuse");
    assert_eq!(fs_scope_platform_check(true).is_ok(), expected, "the build-time check must refuse exactly where the calls would");
}

/// `p` with its links resolved, in the form a path-based call accepts with
/// `/` joins (on Windows without the `\\?\` prefix canonicalize adds).
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn real(p: &Path) -> PathBuf {
    let c = std::fs::canonicalize(p).expect("test fixture setup");
    #[cfg(windows)]
    if let Some(rest) = c.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        if rest.as_bytes().get(1) == Some(&b':') {
            return PathBuf::from(rest);
        }
    }
    c
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn s(p: &Path) -> String {
    p.to_str().expect("test fixture setup").to_string()
}

/// A directory link at `at` to `target`: a symlink; on Windows a directory
/// symlink, or a junction where the process may not make symlinks.
#[cfg(unix)]
fn link_dir(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).expect("test fixture setup");
}

#[cfg(windows)]
fn link_dir(target: &Path, at: &Path) {
    if let Err(e) = std::os::windows::fs::symlink_dir(target, at) {
        // ERROR_PRIVILEGE_NOT_HELD: a junction is the reparse point any
        // user may make, and the walk refuses it the same way.
        assert_eq!(e.raw_os_error(), Some(1314), "test fixture setup: {e}");
        junction(target, at);
    }
    assert!(std::fs::symlink_metadata(at).is_ok(), "test fixture setup: the link {at:?} was not made");
}

#[cfg(windows)]
fn junction(target: &Path, at: &Path) {
    let st = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(at)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("test fixture setup");
    assert!(st.success() && at.is_dir(), "test fixture setup: mklink /J {at:?} {target:?}");
}

/// A file symlink at `at` to `target`; `false` (with a note) only where the
/// Windows process may not make symlinks, which has no junction for files.
#[cfg(unix)]
fn link_file(target: &Path, at: &Path) -> bool {
    std::os::unix::fs::symlink(target, at).expect("test fixture setup");
    true
}

#[cfg(windows)]
fn link_file(target: &Path, at: &Path) -> bool {
    match std::os::windows::fs::symlink_file(target, at) {
        Ok(()) => {
            assert!(std::fs::symlink_metadata(at).is_ok(), "test fixture setup: the link {at:?} was not made");
            true
        }
        Err(e) if e.raw_os_error() == Some(1314) => {
            eprintln!("note: no symlink privilege; the file-link refusal is covered on unix only in this run");
            false
        }
        Err(e) => panic!("test fixture setup: {e}"),
    }
}

/// `data/sub/{a.txt,deep/a.txt}` = "inside", `outside/sub/{a.txt,deep/a.txt}`
/// = "outside", `outside/secret.txt` = "secret".
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn tree() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().expect("test fixture setup");
    let base = real(t.path());
    for (top, text) in [("data", "inside"), ("outside", "outside")] {
        std::fs::create_dir_all(base.join(top).join("sub").join("deep")).expect("test fixture setup");
        std::fs::write(base.join(top).join("sub").join("a.txt"), text).expect("test fixture setup");
        std::fs::write(base.join(top).join("sub").join("deep").join("a.txt"), text).expect("test fixture setup");
    }
    std::fs::write(base.join("outside").join("secret.txt"), "secret").expect("test fixture setup");
    (t, base)
}

/// The outside tree is exactly as [`tree`] made it.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn outside_untouched(base: &Path) {
    let o = base.join("outside");
    let names = |d: &Path| {
        let mut v: Vec<String> = std::fs::read_dir(d).expect("test fixture setup").map(|e| e.expect("test fixture setup").file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    assert_eq!(names(&o), ["secret.txt", "sub"], "an entry appeared or vanished outside");
    assert_eq!(names(&o.join("sub")), ["a.txt", "deep"], "an entry appeared or vanished outside");
    assert_eq!(std::fs::read_to_string(o.join("secret.txt")).expect("test fixture setup"), "secret");
    assert_eq!(std::fs::read_to_string(o.join("sub").join("a.txt")).expect("test fixture setup"), "outside");
    assert_eq!(std::fs::read_to_string(o.join("sub").join("deep").join("a.txt")).expect("test fixture setup"), "outside");
}

/// Replace `data/sub` by a link to `outside/sub` (the original kept aside).
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn swap(base: &Path, link: fn(&Path, &Path)) {
    std::fs::rename(base.join("data").join("sub"), base.join("data").join("sub.orig")).expect("test fixture setup");
    link(&base.join("outside").join("sub"), &base.join("data").join("sub"));
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn escapes<T: std::fmt::Debug>(r: Result<T, FsBeneathError>) -> bool {
    matches!(r, Err(FsBeneathError::Escape))
}

/// Every rooted call on `rel` answers `Escape`.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn every_rooted_call_escapes(root: &str, rel: &str) {
    assert!(escapes(fs_beneath_read(root, rel)), "read {rel:?}");
    assert!(escapes(fs_beneath_read_with(root, rel, true, &mut |_| {})), "walked read {rel:?}");
    assert!(escapes(fs_beneath_write(root, rel, b"pwned")), "write {rel:?}");
    assert!(escapes(fs_beneath_list(root, rel)), "list {rel:?}");
    assert!(escapes(fs_beneath_exists(root, rel)), "exists {rel:?}");
    assert!(escapes(fs_beneath_remove(root, rel)), "remove {rel:?}");
}

/// The directory links this platform has: a symlink (or its junction
/// stand-in), and on Windows an explicit junction too.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn dir_links() -> Vec<(&'static str, fn(&Path, &Path))> {
    #[allow(unused_mut)]
    let mut v: Vec<(&'static str, fn(&Path, &Path))> = vec![("symlink", link_dir)];
    #[cfg(windows)]
    v.push(("junction", junction));
    v
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn rooted_calls_never_leave_their_root() {
    let _g = serial();
    let (_t, base) = tree();
    let data = base.join("data");
    let root = s(&data);

    // The plain calls, made by descriptor.
    assert_eq!(fs_beneath_read_text(&root, "sub/a.txt").expect("read under the root"), "inside");
    assert_eq!(fs_beneath_read(&root, "sub/deep/a.txt").expect("read under the root"), b"inside");
    fs_beneath_write(&root, "sub/new.txt", b"new").expect("write under the root");
    fs_beneath_write(&root, "sub/new.txt", b"newer").expect("rewrite under the root");
    assert_eq!(std::fs::read_to_string(data.join("sub").join("new.txt")).expect("written"), "newer");
    assert_eq!(fs_beneath_list(&root, "sub").expect("list under the root"), ["a.txt", "deep", "new.txt"]);
    assert_eq!(fs_beneath_list(&root, "").expect("list the root").len(), 1);
    assert!(fs_beneath_exists(&root, "sub/new.txt").expect("exists"));
    assert!(!fs_beneath_exists(&root, "sub/none.txt").expect("exists"));
    fs_beneath_remove(&root, "sub/new.txt").expect("remove under the root");
    assert!(!data.join("sub").join("new.txt").exists());
    assert!(matches!(fs_beneath_read(&root, "sub/none.txt"), Err(FsBeneathError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound));

    // `..` and absolute paths, refused before anything is opened.
    let secret = base.join("outside").join("secret.txt");
    let mut rels = vec![
        "..".to_string(),
        "../outside/secret.txt".to_string(),
        "sub/../../outside/secret.txt".to_string(),
        s(&secret),
        s(&secret).replace('\\', "/"),
    ];
    // Windows: a drive, verbatim, device or UNC prefix, a backslash walk
    // and the alternate-data-stream syntax.
    #[cfg(windows)]
    rels.extend([
        format!(r"\\?\{}", s(&secret)),
        format!("//?/{}", s(&secret).replace('\\', "/")),
        format!(r"\\.\{}", s(&secret)),
        r"\\localhost\c$\Windows\win.ini".to_string(),
        "C:secret.txt".to_string(),
        r"sub\..\..\outside\secret.txt".to_string(),
        "sub/a.txt:evil".to_string(),
        "sub/a.txt::$DATA".to_string(),
        "sub:evil".to_string(),
    ]);
    for rel in &rels {
        every_rooted_call_escapes(&root, rel);
    }
    #[cfg(windows)]
    assert!(std::fs::read(data.join("sub").join("a.txt:evil")).is_err(), "a refused write created a stream");
    assert_eq!(std::fs::read_to_string(data.join("sub").join("a.txt")).expect("untouched"), "inside");
    outside_untouched(&base);

    // A link met at any segment: refused by the call itself, the link left.
    for (kind, link) in dir_links() {
        let at = format!("l{kind}");
        link(&base.join("outside").join("sub"), &data.join(&at));
        for rel in [at.clone(), format!("{at}/a.txt"), format!("{at}/deep/a.txt"), format!("{at}/deep")] {
            every_rooted_call_escapes(&root, &rel);
        }
        assert!(escapes(fs_beneath_write(&root, &format!("{at}/new.txt"), b"pwned")), "{kind}");
        assert!(std::fs::symlink_metadata(data.join(&at)).is_ok(), "the refused remove took the {kind}");
        outside_untouched(&base);
    }
    if link_file(&secret, &data.join("lfile")) {
        every_rooted_call_escapes(&root, "lfile");
        assert!(std::fs::symlink_metadata(data.join("lfile")).is_ok(), "the refused remove took the link");
        outside_untouched(&base);
    }

    // A directory swapped for a link between the walk's own steps: before
    // `sub` is opened (hook 0), after `sub` is held (1), after `deep` (2).
    for (kind, link) in dir_links() {
        for at in 0..=2usize {
            let (_t, base) = tree();
            let root = s(&base.join("data"));
            let mut swapped = false;
            let r = fs_beneath_read_with(&root, "sub/deep/a.txt", true, &mut |i| {
                if i == at && !swapped {
                    swap(&base, link);
                    swapped = true;
                    // The window is real: a path-based read at this instant
                    // — the pre-#4084 check-then-use — reaches the outside.
                    let p = base.join("data").join("sub").join("deep").join("a.txt");
                    assert_eq!(std::fs::read_to_string(p).expect("test fixture setup"), "outside");
                }
            });
            assert!(swapped, "the hook never ran at {at}");
            match r {
                Err(FsBeneathError::Escape) => assert_eq!(at, 0, "{kind}: refused although the walk already held sub"),
                Ok(bytes) => {
                    assert_ne!(at, 0, "{kind}: the walk followed the link it met");
                    assert_eq!(bytes, b"inside", "{kind}: the walk left the directory it held");
                }
                Err(FsBeneathError::Io(e)) => panic!("{kind} swap at {at}: {e}"),
            }
            outside_untouched(&base);
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn scoped_calls_never_leave_their_grant() {
    let _g = serial();

    // The plain calls the scope must still make, by descriptor.
    let (_t, base) = tree();
    let data = base.join("data");
    fs_scoped_install(Some(&[s(&data)][..]));
    let q = |rel: &str| s(&data.join(rel));
    assert_eq!(almide_sfs::read_to_string(q("sub/a.txt")).expect("read under the grant"), "inside");
    almide_sfs::create_dir_all(q("sub/n/m")).expect("create_dir_all under the grant");
    assert!(data.join("sub").join("n").join("m").is_dir());
    almide_sfs::write(q("sub/n/m/f"), "1").expect("write under the grant");
    let mut f = almide_sfs::OpenOptions::new().append(true).create(true).open(q("sub/n/m/f")).expect("append under the grant");
    std::io::Write::write_all(&mut f, b"2").expect("append under the grant");
    drop(f);
    assert_eq!(std::fs::read_to_string(data.join("sub").join("n").join("m").join("f")).expect("written"), "12");
    assert_eq!(almide_sfs::copy(q("sub/n/m/f"), q("sub/g")).expect("copy under the grant"), 2);
    almide_sfs::rename(q("sub/g"), q("sub/h")).expect("rename under the grant");
    assert!(almide_sfs::is_file(q("sub/h")) && !almide_sfs::exists(q("sub/g")));
    assert_eq!(almide_sfs::metadata(q("sub/h")).expect("metadata").len(), 2);
    let mut names: Vec<_> = almide_sfs::read_dir(q("sub")).expect("list").map(|e| e.expect("entry").file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["a.txt", "deep", "h", "n"]);
    almide_sfs::create_new_file(q("sub/fresh")).expect("create_new under the grant");
    assert_eq!(almide_sfs::create_new_file(q("sub/fresh")).expect_err("exists").kind(), std::io::ErrorKind::AlreadyExists);
    almide_sfs::remove_file(q("sub/fresh")).expect("remove under the grant");
    almide_sfs::remove_dir_all(q("sub/n")).expect("remove_dir_all under the grant");
    assert!(!data.join("sub").join("n").exists() && !data.join("sub").join("fresh").exists());

    // Outside the grant, by `..`, by an absolute path or through a link the
    // decision resolves: refused, nothing touched.
    link_dir(&base.join("outside").join("sub"), &data.join("ldir"));
    assert!(refused(almide_sfs::read_to_string(base.join("outside").join("secret.txt"))));
    assert!(refused(almide_sfs::read_to_string(q("../outside/secret.txt"))));
    assert!(refused(almide_sfs::write(q("../outside/new.txt"), "x")));
    assert!(refused(almide_sfs::read_to_string(q("ldir/a.txt"))));
    assert!(refused(almide_sfs::write(q("ldir/new.txt"), "x")));
    assert!(refused(almide_sfs::create_dir_all(q("ldir/x/y"))));
    // A no-follow call acts on the link itself, never through it.
    almide_sfs::remove_dir_all(q("ldir")).expect("remove the link itself");
    assert!(std::fs::symlink_metadata(data.join("ldir")).is_err());
    #[cfg(windows)]
    {
        // The alternate-data-stream syntax and a junction.
        assert!(refused(almide_sfs::write(q("sub/a.txt:evil"), "x")), "a :stream write must be refused");
        assert!(refused(almide_sfs::read(q("sub/a.txt:$DATA"))), "a :stream read must be refused");
        assert!(std::fs::read(data.join("sub").join("a.txt:evil")).is_err(), "a refused write created a stream");
        junction(&base.join("outside").join("sub"), &data.join("jdir"));
        assert!(refused(almide_sfs::read_to_string(q("jdir/a.txt"))));
        assert!(refused(almide_sfs::write(q("jdir/new.txt"), "x")));
    }
    outside_untouched(&base);

    // A directory swapped for a link after the scope's decision and before
    // the call. A fresh tree per call, so one refusal cannot hide another's
    // escape.
    type Call = fn(&str, &str) -> std::io::Result<String>;
    let calls: &[(&str, &str, Call)] = &[
        ("read", "sub/a.txt", |p, _| almide_sfs::read_to_string(p)),
        ("read bounded", "sub/a.txt", |p, _| almide_sfs::read_to_string_bounded(p)),
        ("write", "sub/a.txt", |p, _| almide_sfs::write(p, "pwned").map(|()| String::new())),
        ("write new", "sub/new.txt", |p, _| almide_sfs::write(p, "pwned").map(|()| String::new())),
        ("append", "sub/a.txt", |p, _| {
            let mut f = almide_sfs::OpenOptions::new().append(true).create(true).open(p)?;
            std::io::Write::write_all(&mut f, b"pwned").map(|()| String::new())
        }),
        ("mkdir_p", "sub/deep/e/f", |p, _| almide_sfs::create_dir_all(p).map(|()| String::new())),
        ("list", "sub/deep", |p, _| almide_sfs::read_dir(p).map(|r| r.map(|e| e.expect("entry").file_name().to_string_lossy().into_owned()).collect())),
        ("metadata", "sub/a.txt", |p, _| almide_sfs::metadata(p).map(|m| m.len().to_string())),
        ("remove", "sub/a.txt", |p, _| almide_sfs::remove_file(p).map(|()| String::new())),
        ("remove_all", "sub/deep", |p, _| almide_sfs::remove_dir_all(p).map(|()| String::new())),
        ("rename", "sub/a.txt", |p, q| almide_sfs::rename(p, q).map(|()| String::new())),
        ("copy", "sub/a.txt", |p, q| almide_sfs::copy(p, q).map(|n| n.to_string())),
    ];
    for (kind, link) in dir_links() {
        for (name, rel, call) in calls {
            let (_t, base) = tree();
            let data = base.join("data");
            fs_scoped_install(Some(&[s(&data)][..]));
            let path = s(&data.join(rel));
            let dst = s(&data.join("copied.txt"));
            assert!(fs_scoped_admits(&path), "{kind} {name}: the intact tree is inside the scope");
            swap(&base, link);
            if *rel == "sub/a.txt" {
                assert_eq!(std::fs::read_to_string(&path).expect("test fixture setup"), "outside", "the window is real");
            }
            let r = call(&path, &dst);
            assert!(refused(r.as_ref().map(|_| ()).map_err(|e| std::io::Error::new(e.kind(), e.to_string()))), "{kind} {name}: not refused: {r:?}");
            outside_untouched(&base);
            assert!(!data.join("copied.txt").exists(), "{kind} {name}");
        }
    }
    fs_scoped_install(None);
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn a_granted_root_itself_is_acted_on_by_descriptor() {
    let _g = serial();
    let t = tempfile::tempdir().expect("test fixture setup");
    let base = real(t.path());
    let data = base.join("data");
    std::fs::create_dir_all(data.join("sub")).expect("test fixture setup");
    std::fs::write(data.join("a.txt"), "inside").expect("test fixture setup");
    let root = s(&data);
    fs_scoped_install(Some(&[root.clone()][..]));
    let sub = |rel: &str| s(&data.join(rel));

    // The scope opens and holds the root.
    assert_eq!(almide_sfs::read_to_string(sub("a.txt")).expect("read under the root"), "inside");

    // The metadata of an entry the process may not open, by descriptor. (As
    // root the open succeeds and the fallback is not reached; the answer must
    // be the same either way.)
    #[cfg(unix)]
    {
        let locked = data.join("locked.txt");
        std::fs::write(&locked, "12345").expect("test fixture setup");
        std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).expect("test fixture setup");
        let m = almide_sfs::metadata(sub("locked.txt")).expect("metadata of an unreadable file");
        assert!(m.is_file() && m.len() == 5, "unreadable file described: {m:?}");
        std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o644)).expect("test fixture setup");
    }

    // The root itself, while it is the held directory.
    assert!(almide_sfs::metadata(&root).expect("root metadata").is_dir());
    almide_sfs::create_dir_all(&root).expect("create_dir_all of an existing root is done");

    // The root replaced after the scope opened it: the entry at the root's
    // name is no longer the held directory. Every op on the root's NAME is
    // refused and the replacement survives; the reads answer the held one.
    std::fs::rename(&data, base.join("data.orig")).expect("test fixture setup");
    std::fs::create_dir_all(data.join("other")).expect("test fixture setup");
    std::fs::write(data.join("other").join("keep.txt"), "replacement").expect("test fixture setup");
    assert!(refused(almide_sfs::remove_dir_all(&root)), "remove_dir_all of a replaced root must be refused");
    assert!(refused(almide_sfs::remove_dir(&root)), "remove_dir of a replaced root must be refused");
    assert!(refused(almide_sfs::rename(&root, sub("other/moved"))), "rename of a replaced root must be refused");
    assert_eq!(std::fs::read_to_string(data.join("other").join("keep.txt")).expect("the replacement survives"), "replacement");
    let names: Vec<_> = almide_sfs::read_dir(&root).expect("list the held root").map(|e| e.expect("entry").file_name()).collect();
    assert!(names.iter().any(|n| n == "a.txt") && !names.iter().any(|n| n == "other"), "the root's listing is the held directory's: {names:?}");
    // A second listing of the held root starts over (its duplicate shares
    // the first one's position).
    let again: Vec<_> = almide_sfs::read_dir(&root).expect("list the held root again").map(|e| e.expect("entry").file_name()).collect();
    assert_eq!(names, again, "a second listing of the held root must answer the same entries");

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

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
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
