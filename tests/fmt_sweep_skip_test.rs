//! #4186: `almide fmt <dir>` sweeps the project's own sources, never version
//! control (`.git/hooks`), build output (`target/`) or vendored dependencies
//! (`node_modules/`). The sweep used to rewrite every `.almd` under them. A
//! directory named on the command line is still formatted.

use std::path::Path;
use std::process::Command;

const MESSY: &str = "fn f(a: Int) -> Int =     a  +  1\n";

fn almide_fmt(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("fmt")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run almide fmt")
}

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for rel in [".git/hooks/x.almd", "target/x.almd", "node_modules/p/x.almd", "src/x.almd"] {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, MESSY).unwrap();
    }
    dir
}

#[test]
fn a_directory_sweep_formats_only_the_project_sources() {
    let dir = tree();
    let out = almide_fmt(dir.path(), &["."]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    for rel in [".git/hooks/x.almd", "target/x.almd", "node_modules/p/x.almd"] {
        assert_eq!(
            std::fs::read_to_string(dir.path().join(rel)).unwrap(),
            MESSY,
            "`almide fmt .` rewrote {rel}"
        );
    }
    // Control: the project's own file IS formatted by the same sweep.
    assert_ne!(std::fs::read_to_string(dir.path().join("src/x.almd")).unwrap(), MESSY, "the sweep formatted nothing");
}

#[test]
fn a_named_skipped_directory_is_still_formatted() {
    let dir = tree();
    let out = almide_fmt(dir.path(), &[".git/hooks"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_ne!(std::fs::read_to_string(dir.path().join(".git/hooks/x.almd")).unwrap(), MESSY);
}
