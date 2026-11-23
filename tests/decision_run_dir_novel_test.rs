//! env.run-project-dir (#4842): the `ALMIDE_RUN_PROJECT_DIR` override holds
//! binaries that are executed, so it is held to what the private cache base
//! is held to. A directory no test author named — writable by others, a
//! symlink, or owned by someone else — is refused before anything in it runs;
//! a missing one is made closed (0700) and used.
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/novel.rs"]
mod novel;

fn run(dir: &Path, program: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(program)
        .env("ALMIDE_RUN_PROJECT_DIR", dir)
        .output()
        .expect("spawn almide run")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn novel_an_override_others_could_write_into_is_refused() {
    let t = tempfile::tempdir().expect("scratch");
    let program = t.path().join("main.almd");
    std::fs::write(&program, "effect fn main() -> Unit = println(\"ran\")\n").expect("write");

    // Writable by group and others: refused, nothing built or run.
    let open = t.path().join(novel::ident("open"));
    std::fs::create_dir(&open).expect("mkdir");
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    let o = run(&open, &program);
    assert!(!o.status.success(), "a world-writable override was used:\n{}", text(&o));
    assert!(text(&o).contains("group or other users can write to it"), "{}", text(&o));
    assert!(!String::from_utf8_lossy(&o.stdout).contains("ran"));

    // A symlink to a closed directory: refused as a symlink.
    let target = t.path().join(novel::ident("target"));
    std::fs::create_dir(&target).expect("mkdir");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let link = t.path().join(novel::ident("link"));
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let o = run(&link, &program);
    assert!(!o.status.success(), "a symlinked override was used:\n{}", text(&o));
    assert!(text(&o).contains("it is not a directory (a symlink or a file)"), "{}", text(&o));

    // A missing one is made closed and used.
    let fresh = t.path().join(novel::ident("fresh"));
    let o = run(&fresh, &program);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "ran\n");
    let mode = std::fs::metadata(&fresh).expect("made").permissions().mode() & 0o777;
    assert_eq!(mode & 0o022, 0, "the override was made open to writers: {mode:o}");
}
