//! #4620 (owner ruling 2026-11-20): every native program fixes glibc's trim
//! and mmap thresholds at startup. glibc adapts both at run time, so a
//! program that frees and reallocates large blocks in a loop gave the top of
//! its heap back to the kernel and faulted it in again every round (the
//! fan-determinism probe p13: 26 s native against 4.3 s on wasm), and the
//! instructions a program ran varied between runs.
//!
//! Both native routes open `main` with the setting: the standard codegen
//! (`--target rust`, the fallback) and the certified render. On a glibc host
//! the built binary imports `mallopt`.

use std::process::Command;

const PROGRAM: &str = "effect fn main() -> Unit = {\n  let s = string.repeat(\"ab\", 100000)\n  println(\"${string.len(s)}\")\n}\n";
const SETTING: [&str; 2] = ["mallopt(-1, i32::MAX);", "mallopt(-3, 1 << 30);"];

fn main_of(rust: &str) -> &str {
    let at = rust.find("fn main()").expect("the source has a main");
    &rust[at..]
}

#[test]
fn the_standard_codegen_main_sets_the_allocator_policy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("policy.almd");
    std::fs::write(&file, PROGRAM).expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg(&file).args(["--target", "rust"]).output().expect("emit");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8(out.stdout).expect("utf8");
    for s in SETTING {
        assert!(main_of(&rust).contains(s), "the emitted main lacks `{s}`");
    }
}

#[test]
fn the_certified_render_main_sets_the_allocator_policy() {
    let rust = std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(|| almide_mir::pipeline::try_render_rust_source(PROGRAM))
        .expect("spawn")
        .join()
        .expect("join")
        .expect("the program renders on the certified lane");
    for s in SETTING {
        assert!(main_of(&rust).contains(s), "the rendered main lacks `{s}`");
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_native_build_imports_mallopt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("policy.almd");
    std::fs::write(&file, PROGRAM).expect("write");
    let bin = dir.path().join("policy");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("build").arg(&file).arg("-o").arg(&bin).output().expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&bin).expect("read the binary");
    assert!(bytes.windows(b"mallopt".len()).any(|w| w == b"mallopt"), "the binary does not import mallopt");
    let ran = Command::new(&bin).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "200000\n");
}
