//! env.test-wasm-runner (#4842): `ALMIDE_TEST_WASM_RUNNER=wasmtime` runs the
//! wasm test lane on the wasmtime CLI, and that lane is bounded by the test
//! file's `[permissions] fs` as the embedded lane is: the CLI preopens the
//! granted roots, never `/`. The granted directory and the file outside it are
//! fresh names no test author wrote. The control runs the same read with no
//! `[permissions]`, where the lane must read the file, so the refusal is not a
//! file the lane could never see. Skips without wasmtime unless
//! `ALMIDE_EXPECT_TOOLS` is set.
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/novel.rs"]
mod novel;

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn lane(proj: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(proj)
        .args(["test", "src/r_test.almd", "--target", "wasm"])
        .env("ALMIDE_TEST_WASM_RUNNER", "wasmtime")
        .output()
        .expect("spawn almide")
}

fn project(root: &Path, manifest_tail: &str, outside: &Path, expect: &str) -> std::path::PathBuf {
    let proj = root.join(novel::path_segment());
    std::fs::create_dir_all(proj.join("src")).expect("mkdir src");
    std::fs::write(proj.join("almide.toml"), format!("[package]\nname = \"lane\"\nversion = \"0.1.0\"\n{manifest_tail}")).expect("write manifest");
    std::fs::write(
        proj.join("src/r_test.almd"),
        format!("import fs\ntest \"outside\" {{\n  assert_eq(fs.read_text(\"{}\") ?? \"refused\", \"{expect}\")\n}}\n", outside.display()),
    )
    .expect("write test");
    proj
}

#[test]
fn novel_the_wasmtime_lane_refuses_a_read_outside_a_fresh_grant() {
    let has = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if !has {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS is set but wasmtime is not on PATH");
        return;
    }
    let td = tempfile::TempDir::new().expect("scratch");
    let outside = td.path().join(format!("{}.txt", novel::ident("outside")));
    let marker = novel::ident("content");
    std::fs::write(&outside, &marker).expect("write outside file");

    let granted = novel::path_segment();
    let scoped = project(td.path(), &format!("\n[permissions]\nfs = [\"{granted}\"]\n"), &outside, "refused");
    std::fs::create_dir_all(scoped.join(&granted)).expect("mkdir grant");
    let o = lane(&scoped);
    assert!(o.status.success(), "the wasmtime lane read outside a fresh [permissions] fs grant:\n{}", text(&o));
    assert!(text(&o).contains("1 passed, 0 failed (of 1 files)") && !text(&o).contains("native fallback"), "the file did not run on the wasm lane:\n{}", text(&o));

    // Control: no grant, and the same lane reads the file.
    let open = project(td.path(), "", &outside, &marker);
    let o = lane(&open);
    assert!(o.status.success(), "control: the wasmtime lane could not read the file with no [permissions]:\n{}", text(&o));
}
