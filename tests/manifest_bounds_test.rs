//! almide.toml / almide.lock are untrusted input from a cloned repository.
//!
//! #4194: they are read only when they are regular files of at most 1 MiB
//! (`project::PROJECT_FILE_MAX_BYTES`), so a file linked to `/dev/zero` is an
//! error instead of an allocation that takes the machine's memory, and the
//! unknown-table warnings cost one scan of the file, not one per warning.
//!
//! #4126: a key or string from the manifest is never printed raw. A key with
//! a terminal escape is refused, and the refusal quotes it escaped; a package
//! or dependency string holding one is refused before any command prints it.

use std::path::Path;
use std::process::{Command, Output};

const PACKAGE: &str = "[package]\nname = \"x\"\nversion = \"0.1.0\"\n";

fn project(manifest: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("almide.toml"), manifest).unwrap();
    std::fs::write(dir.path().join("app.almd"), "fn main() -> Unit = ()\n").unwrap();
    dir
}

fn check(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["check", "app.almd"])
        .current_dir(dir)
        .output()
        .expect("run almide check")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[cfg(unix)]
#[test]
fn a_lock_or_manifest_linked_to_dev_zero_is_refused_not_read() {
    let dir = project(PACKAGE);
    std::os::unix::fs::symlink("/dev/zero", dir.path().join("almide.lock")).unwrap();
    let o = check(dir.path());
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("almide.lock is not a regular file"), "{}", stderr(&o));

    let dir = project(PACKAGE);
    std::fs::remove_file(dir.path().join("almide.toml")).unwrap();
    std::os::unix::fs::symlink("/dev/zero", dir.path().join("almide.toml")).unwrap();
    let o = check(dir.path());
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("almide.toml is not a regular file"), "{}", stderr(&o));
}

/// The size cap, on a file the old reader accepted: a lock of valid TOML
/// (comments) just over 1 MiB is refused; a symlink to a regular manifest is
/// still followed.
#[test]
fn a_lock_over_the_cap_is_refused_and_a_symlinked_manifest_is_read() {
    let dir = project(PACKAGE);
    let line = "# padding padding padding padding padding padding padding padding\n";
    let lock = line.repeat((1 << 20) / line.len() + 1);
    std::fs::write(dir.path().join("almide.lock"), lock).unwrap();
    let o = check(dir.path());
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("larger than 1048576 bytes"), "{}", stderr(&o));

    #[cfg(unix)]
    {
        let dir = project(PACKAGE);
        let real = dir.path().join("real.toml");
        std::fs::rename(dir.path().join("almide.toml"), &real).unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("almide.toml")).unwrap();
        let o = check(dir.path());
        assert!(o.status.success(), "a symlink to a regular manifest must still be read: {}", stderr(&o));
    }
}

/// N unknown tables cost O(N): quadrupling N must not cost anything like the
/// twelve times a per-warning rescan did (#4194 measured 0.49 s at 20 000
/// and 5.8 s at 80 000).
#[test]
fn unknown_table_warnings_scale_linearly() {
    let run = |n: usize| {
        let tables: String = (0..n).map(|i| format!("[t{i}]\nk=1\n")).collect();
        let dir = project(&format!("{PACKAGE}{tables}"));
        let start = std::time::Instant::now();
        let o = check(dir.path());
        assert!(o.status.success(), "{}", stderr(&o));
        start.elapsed().as_secs_f64()
    };
    // Warm the binary and the page cache, so the first timed run is not the
    // one paying for them.
    let _ = run(1_000);
    let small = run(20_000);
    let large = run(80_000);
    // Linear is ~4x; the per-warning rescan measured ~12x (0.49 s → 5.8 s).
    assert!(large < small * 6.0 + 0.3, "4x the tables took {large:.2}s against {small:.2}s");
}

#[test]
fn a_manifest_key_with_a_terminal_escape_is_refused_and_quoted_escaped() {
    let dir = project(&format!("{PACKAGE}\"\\u001b]0;pwn\\u0007\\u001b[2Kfake\" = 1\n"));
    let o = check(dir.path());
    let e = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{}", e);
    assert!(!o.stderr.contains(&0x1b) && !o.stdout.contains(&0x1b), "a raw ESC reached the terminal: {e:?}");
    assert!(e.contains("\\u{1b}]0;pwn\\u{7}"), "{}", e);
}

#[test]
fn a_package_or_dependency_string_with_a_control_character_is_refused() {
    for manifest in [
        "[package]\nname = \"x\\u001b[2K\"\nversion = \"0.1.0\"\n".to_string(),
        format!("{PACKAGE}[dependencies]\ndep = {{ path = \"../d\\u0007\" }}\n"),
    ] {
        let dir = project(&manifest);
        let o = check(dir.path());
        assert_eq!(o.status.code(), Some(1), "{manifest}: {}", stderr(&o));
        assert!(stderr(&o).contains("holds a control or bidi character"), "{}", stderr(&o));
        assert!(!o.stderr.contains(&0x1b) && !o.stderr.contains(&0x07), "{:?}", stderr(&o));
    }
    // A description may hold a line break.
    let dir = project(&format!("{PACKAGE}description = \"two\\nlines\"\n"));
    let o = check(dir.path());
    assert!(o.status.success(), "{}", stderr(&o));
}

#[test]
fn a_lock_string_with_a_control_character_is_refused() {
    let dir = project(PACKAGE);
    std::fs::write(
        dir.path().join("almide.lock"),
        "dep = { git = \"https://example.invalid/d\", ref = \"v1\", commit = \"\\u001b[2Kabc\" }\n",
    )
    .unwrap();
    let o = check(dir.path());
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(!o.stderr.contains(&0x1b), "{:?}", stderr(&o));
}
