//! A native test run builds its test crates concurrently, each in a crate dir
//! of its own. A test crate that needs a runtime crate (zlib, http, …) builds
//! through `cargo test --no-run`, and an inherited `CARGO_TARGET_DIR` sent all
//! of those builds to ONE target dir, where every one links the same
//! `almide_out-<hash>` test binary. Each file then took whichever binary was
//! there when its build ended: several files ran one sibling's tests — and a
//! sibling's passing tests read as theirs. The build now pins the crate's own
//! target dir, as the bin and cdylib builds already did.
//!
//! The fixture: twelve such files, built at once under a shared
//! `CARGO_TARGET_DIR`, each holding one test named for the file. A file that
//! ran another file's binary fails its run (the completion ledger does not
//! hold the test it selected), so the run must pass whole. Negative control:
//! `ALMIDE_BIN` at a build without the pin (44843fe3a) fails this on an
//! n2-standard-32 in every run tried.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

const FILES: usize = 12;

fn test_file(i: usize) -> String {
    format!(
        "import zlib\n\n\
         effect fn probe() -> String = match zlib.deflate(bytes.from_string(\"{i}\")) {{\n  ok(_) => \"ok\",\n  err(_) => \"err\",\n}}\n\n\
         fn own() -> Int = {i}\n\n\
         test \"file {i} runs its own test\" {{\n  let r = probe()!\n  assert(r == \"ok\" or r == \"err\")\n  assert_eq(own(), {i})\n}}\n"
    )
}

#[test]
fn each_file_runs_its_own_tests_under_a_shared_cargo_target_dir() {
    let dir = tempfile::Builder::new().prefix("shared_target").tempdir().expect("tempdir");
    let tests = dir.path().join("tests");
    let tmp = dir.path().join("tmp");
    let shared = dir.path().join("shared-target");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();
    for i in 0..FILES {
        std::fs::write(tests.join(format!("f{i:02}_test.almd")), test_file(i)).unwrap();
    }
    let out = Command::new(almide_bin())
        .args(["test", "tests", "--target", "native"])
        .current_dir(dir.path())
        .env("TMPDIR", &tmp)
        .env("CARGO_TARGET_DIR", &shared)
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "a file ran a binary that was not its own?\n{text}");
    assert!(text.contains(&format!("All {FILES} test file(s) passed")), "{text}");
    assert!(text.contains(&format!("{FILES} tests in {FILES} files")), "{text}");
}
