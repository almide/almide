//! almide.lock round-trip (#1465).
//!
//! The lock's write format was always valid TOML, but the reader was a
//! hand-rolled line splitter: `trim_end_matches('}')` ate every trailing
//! brace, `split(',')` broke a ref containing a comma, `trim_matches('"')`
//! mangled an embedded quote — and a line the splitter rejected was silently
//! DROPPED, so the dependency vanished from the lock set and was re-resolved
//! from the network. The reader is now the `toml` crate, the writer escapes
//! its strings, and a malformed entry is an error naming the dependency.

use almide::project::{parse_lock_file, write_lock_file, LockedDep};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-issue1465-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir.join("almide.lock")
}

/// Every byte a git ref can legally carry must survive write → parse.
#[test]
fn hostile_ref_round_trips() {
    let path = scratch("roundtrip");
    let deps = vec![
        LockedDep {
            name: "plain".into(),
            git: "https://github.com/almide/plain".into(),
            ref_name: "v0.1.0".into(),
            commit: "aaaa1111".into(),
            subdir: None,
        },
        // A comma, a closing brace, a quote and a space — each one broke the
        // old splitter in a different way.
        LockedDep {
            name: "hostile".into(),
            git: "https://github.com/almide/hostile".into(),
            ref_name: "feat/x,y}z\"w v2".into(),
            commit: "bbbb2222".into(),
            subdir: None,
        },
        // #3381: a package in a subdirectory of the repository.
        LockedDep {
            name: "ceangal".into(),
            git: "https://github.com/almide-graphics/ceangal2".into(),
            ref_name: "v0.1.0".into(),
            commit: "cccc3333".into(),
            subdir: Some("pkgs/ceangal".into()),
        },
    ];
    write_lock_file(&path, &deps).expect("write");
    let back = parse_lock_file(&path).expect("parse");
    assert_eq!(back.len(), deps.len(), "an entry vanished in the round trip");
    for want in &deps {
        let got = back
            .iter()
            .find(|b| b.name == want.name)
            .unwrap_or_else(|| panic!("entry '{}' vanished", want.name));
        assert_eq!(got.git, want.git);
        assert_eq!(got.ref_name, want.ref_name);
        assert_eq!(got.commit, want.commit);
        assert_eq!(got.subdir, want.subdir);
    }
    // An entry without `subdir` is written exactly as before #3381.
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(
        text.contains("plain = { git = \"https://github.com/almide/plain\", ref = \"v0.1.0\", commit = \"aaaa1111\" }\n"),
        "{text}"
    );
}

/// A malformed entry is an error naming the dependency — never a silently
/// smaller lock set.
#[test]
fn a_corrupt_lock_is_an_error_not_an_empty_set() {
    let path = scratch("corrupt");
    std::fs::write(&path, "broken = { git = , commit = \"x\" }\n").expect("write");
    let err = parse_lock_file(&path).expect_err("invalid TOML must not parse");
    assert!(err.contains("not valid TOML"), "unexpected error: {err}");

    std::fs::write(&path, "nogit = { ref = \"v1\", commit = \"abc\" }\n").expect("write");
    let err = parse_lock_file(&path).expect_err("missing git must be an error");
    assert!(
        err.contains("nogit") && err.contains("git"),
        "error must name the entry and the field: {err}"
    );
}

fn one_dep(commit: &str) -> Vec<LockedDep> {
    vec![LockedDep {
        name: "almai".into(),
        git: "https://github.com/almide/almai".into(),
        ref_name: "main".into(),
        commit: commit.into(),
        subdir: None,
    }]
}

/// #4472: every dependency resolution writes the lock, and `almide test`
/// resolves on every parallel test thread. The in-place write (truncate, then
/// write) let a concurrent reader see an empty lock — no entries — and resolve
/// a locked git dependency at its branch head. Writers alternating between two
/// commits while readers parse: every read must see one whole entry.
#[test]
fn a_concurrent_reader_never_sees_a_partial_lock() {
    let path = scratch("concurrent");
    write_lock_file(&path, &one_dep("aaaa1111")).expect("write");
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writers: Vec<_> = (0..4)
        .map(|w| {
            let (path, stop) = (path.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut i = 0u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let commit = if (i + w) % 2 == 0 { "aaaa1111" } else { "bbbb2222" };
                    write_lock_file(&path, &one_dep(commit)).expect("write");
                    i += 1;
                }
            })
        })
        .collect();
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                for _ in 0..2000 {
                    let got = parse_lock_file(&path).expect("a lock mid-write must still parse");
                    assert_eq!(got.len(), 1, "a reader saw a lock with {} entries", got.len());
                }
            })
        })
        .collect();
    for r in readers {
        r.join().expect("reader");
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for w in writers {
        w.join().expect("writer");
    }
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "almide.lock")
        .collect();
    assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
}

/// #4472: an unchanged lock is not rewritten, and a read-only lock is left
/// alone (the write fails, as the in-place write did) rather than replaced.
#[test]
fn an_unchanged_or_read_only_lock_is_not_rewritten() {
    let path = scratch("unchanged");
    write_lock_file(&path, &one_dep("aaaa1111")).expect("write");
    let mut perms = std::fs::metadata(&path).expect("meta").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms.clone()).expect("chmod");
    write_lock_file(&path, &one_dep("aaaa1111")).expect("identical content is a no-op");
    assert!(write_lock_file(&path, &one_dep("bbbb2222")).is_err(), "a read-only lock was replaced");
    let back = parse_lock_file(&path).expect("parse");
    assert_eq!(back[0].commit, "aaaa1111");
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&path, perms).expect("chmod back");
}
