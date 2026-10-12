//! `fs.read_lines`, `fs.list_dir` and `fs.walk` on a path that does not
//! exist leave no heap block live on wasm. Their shared result assembly
//! (`fs_result_string_list`) allocated the Ok list before knowing the
//! status, and the Err path returned without releasing it: one block per
//! failed call. Found by the fuzzer's sandboxed effect campaign (tier E1).
use std::process::Command;

#[test]
fn a_failed_list_returning_fs_call_releases_its_empty_list_on_wasm() {
    for call in ["fs.read_lines(\"missing\")", "fs.list_dir(\"missing\")", "fs.walk(\"missing\")"] {
        let dir = tempfile::tempdir().expect("scratch");
        let file = dir.path().join("leak.almd");
        let src = format!("import fs\neffect fn main() -> Unit = {{\n  println(\"${{result.is_ok({call})}}\")\n}}\n");
        std::fs::write(&file, src).expect("source");
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run")
            .arg(&file)
            .args(["--target", "wasm"])
            .current_dir(dir.path())
            .env("ALMIDE_WASM_ALLOC_COUNT", "1")
            .output()
            .expect("almide run");
        assert!(out.status.success(), "{call}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "false\n", "{call}");
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().find(|l| l.contains("__ALMD_WASM_ALLOC")).expect("the alloc count line");
        assert!(line.contains(" live=0 "), "{call} left blocks live: {line}");
    }
}
