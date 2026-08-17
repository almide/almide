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
        },
        // A comma, a closing brace, a quote and a space — each one broke the
        // old splitter in a different way.
        LockedDep {
            name: "hostile".into(),
            git: "https://github.com/almide/hostile".into(),
            ref_name: "feat/x,y}z\"w v2".into(),
            commit: "bbbb2222".into(),
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
    }
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
