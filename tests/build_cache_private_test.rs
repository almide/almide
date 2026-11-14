//! #4180: the native build caches live under a private per-user base,
//! `<temp>/almide-<euid>`, and a cached binary runs only while its bytes match
//! the digest recorded when it was built.
//!
//! The caches used to sit at fixed names directly in the temp dir, created
//! with `create_dir_all`, and a hit was "the file exists". On Linux the temp
//! dir is the shared `/tmp`, so another local user could create the dir
//! first, plant a binary under its predictable name, and have it run as you.
//!
//! Each refusal has its control: the same run on an untouched cache is a hit.
#![cfg(unix)]

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn base(tmp: &Path) -> PathBuf {
    // SAFETY: geteuid has no preconditions.
    tmp.join(format!("almide-{}", unsafe { libc::geteuid() }))
}

fn run(td: &Path, program: &Path) -> Output {
    let tmp = td.join("tmp");
    Command::new(almide())
        .arg("run")
        .arg(program)
        .current_dir(td)
        .env("HOME", td.join("home"))
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .env_remove("ALMIDE_RUN_PROJECT_DIR")
        .output()
        .expect("spawn almide run")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(td.path().join("tmp")).unwrap();
    std::fs::create_dir_all(td.path().join("home")).unwrap();
    let program = td.path().join("hello.almd");
    std::fs::write(&program, "effect fn main() -> Unit = println(\"hello from the source\")\n").unwrap();
    (td, program)
}

/// A shell script that leaves a marker and prints a line, standing in for a
/// planted binary.
fn plant(path: &Path, marker: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\ntouch '{}'\necho PLANTED\n", marker.display())).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The cached binaries anywhere under the base: files named
/// `almide-<16 hex digits>` (the content-keyed name, without the digest).
fn cached_binaries(base: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                walk(&p, out);
            } else if ft.is_file() {
                let n = p.file_name().unwrap().to_string_lossy().into_owned();
                let hex = n.strip_prefix("almide-").unwrap_or("");
                if hex.len() == 16 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                    out.push(p);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(base, &mut out);
    out.sort();
    out
}

/// The recorded digest is the binary's BLAKE3, in a `.blake3` sidecar (#4682).
/// The suffix names the algorithm, so a pre-#4682 `.sha256` sidecar is never
/// compared against a BLAKE3 digest: its binary is a miss and is rebuilt
/// (`a_pre_blake3_cache_entry_is_rebuilt_not_run`).
fn assert_recorded_digest(bin: &Path) {
    let expected = blake3::hash(&std::fs::read(bin).unwrap()).to_hex().to_string();
    let recorded = bin.with_file_name(format!("{}.blake3", bin.file_name().unwrap().to_string_lossy()));
    assert_eq!(std::fs::read_to_string(recorded).unwrap().trim(), expected);
}

#[test]
fn a_base_others_can_write_is_refused_and_nothing_in_it_runs() {
    let (td, program) = setup();
    let tmp = td.path().join("tmp");
    let base = base(&tmp);
    std::fs::DirBuilder::new().mode(0o777).create(&base).unwrap();
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o777)).unwrap();
    // Plant a binary under every name a build of this program could look up:
    // the name is unknown here, so plant the directory a hit would read.
    let marker = td.path().join("planted-ran");
    plant(&base.join("run/target/debug/almide-planted"), &marker);
    let o = run(td.path(), &program);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(out.contains("refusing the build cache"), "{out}");
    assert!(out.contains("group or other users can reach it"), "{out}");
    assert!(!marker.exists(), "nothing under a refused base may run:\n{out}");
}

#[test]
fn a_base_that_is_a_symlink_is_refused() {
    let (td, program) = setup();
    let tmp = td.path().join("tmp");
    let elsewhere = td.path().join("elsewhere");
    std::fs::DirBuilder::new().mode(0o700).create(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, base(&tmp)).unwrap();
    let o = run(td.path(), &program);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    assert!(out.contains("not a directory"), "{out}");
}

#[test]
fn a_cached_binary_that_no_longer_matches_its_digest_is_rebuilt_not_run() {
    let (td, program) = setup();
    let tmp = td.path().join("tmp");
    // A first run builds and caches the program.
    let o = run(td.path(), &program);
    assert!(String::from_utf8_lossy(&o.stdout).contains("hello from the source"), "{}", text(&o));
    let base = base(&tmp);
    let meta = std::fs::metadata(&base).unwrap();
    assert_eq!(meta.permissions().mode() & 0o077, 0, "the base is created private");
    let bins = cached_binaries(&base);
    assert_eq!(bins.len(), 1, "one cached binary under the base: {bins:?}");
    assert_recorded_digest(&bins[0]);

    // Control: an untouched cache is a hit and prints the program's output.
    let o = run(td.path(), &program);
    assert!(String::from_utf8_lossy(&o.stdout).contains("hello from the source"), "{}", text(&o));

    // Replace the cached binary's bytes: the digest no longer matches.
    let marker = td.path().join("planted-ran");
    plant(&bins[0], &marker);
    let o = run(td.path(), &program);
    let out = text(&o);
    assert!(out.contains("hello from the source"), "the program is rebuilt from source:\n{out}");
    assert!(!out.contains("PLANTED") && !marker.exists(), "a binary that fails its digest must not run:\n{out}");
    assert_recorded_digest(&bins[0]);
}

#[test]
fn a_pre_blake3_cache_entry_is_rebuilt_not_run() {
    let (td, program) = setup();
    let tmp = td.path().join("tmp");
    let o = run(td.path(), &program);
    assert!(String::from_utf8_lossy(&o.stdout).contains("hello from the source"), "{}", text(&o));
    let bins = cached_binaries(&base(&tmp));
    assert_eq!(bins.len(), 1, "one cached binary under the base: {bins:?}");
    // Turn the entry into what a pre-#4682 compiler left: a planted binary
    // with a `.sha256` sidecar that matches it, and no `.blake3` sidecar.
    let marker = td.path().join("planted-ran");
    plant(&bins[0], &marker);
    let name = bins[0].file_name().unwrap().to_string_lossy().into_owned();
    std::fs::remove_file(bins[0].with_file_name(format!("{name}.blake3"))).unwrap();
    use sha2::{Digest, Sha256};
    let legacy: String = Sha256::digest(std::fs::read(&bins[0]).unwrap()).iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(bins[0].with_file_name(format!("{name}.sha256")), legacy).unwrap();
    let o = run(td.path(), &program);
    let out = text(&o);
    assert!(out.contains("hello from the source"), "the program is rebuilt from source:\n{out}");
    assert!(!out.contains("PLANTED") && !marker.exists(), "a legacy sidecar is not a hit:\n{out}");
    assert_recorded_digest(&bins[0]);
}
