//! `ALMIDE_WASM_ALLOC_COUNT` (#2407) at the CLI: `almide run --target wasm`
//! prints `__ALMD_WASM_ALLOC allocs=N reused=N bytes=N frees=N heap_end=N`
//! on stderr when the switch is set — the wasm twin of the native
//! `__ALMD_ALLOC` line (#2228) — and nothing at all when it is not. The
//! in-crate test (crates/almide-wasm/tests/alloc_count.rs) holds the
//! counts themselves; this one holds the delivery.

use std::process::Command;

const ONE: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3]
  println("${list.len(xs)}")
}
"#;

const TWO: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3]
  let ys = [4, 5, 6]
  println("${list.len(xs) + list.len(ys)}")
}
"#;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = root.join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn run(src: &std::path::Path, armed: bool) -> (String, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.args(["run", "--target", "wasm"]).arg(src).env_remove("ALMIDE_WASM_ALLOC_COUNT");
    if armed {
        cmd.env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("spawn almide run --target wasm");
    assert!(out.status.success(), "run failed: {}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn field(line: &str, key: &str) -> u64 {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("malformed report `{line}`: no `{key}=N`"))
}

#[test]
fn the_switch_prints_the_report_and_its_absence_prints_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let one = dir.path().join("one.almd");
    let two = dir.path().join("two.almd");
    std::fs::write(&one, ONE).unwrap();
    std::fs::write(&two, TWO).unwrap();

    let (out, err) = run(&one, false);
    assert_eq!(out, "3\n");
    assert!(!err.contains("__ALMD_WASM_ALLOC"), "unarmed: no report line, got: {err}");

    let (out, err) = run(&one, true);
    assert_eq!(out, "3\n", "the report never touches stdout");
    let line1 = err.lines().find(|l| l.starts_with("__ALMD_WASM_ALLOC ")).unwrap_or_else(|| {
        panic!("no `__ALMD_WASM_ALLOC` line on stderr with the switch set:\n{err}")
    });
    let (_, line2) = run(&two, true);
    let line2 = line2.lines().find(|l| l.starts_with("__ALMD_WASM_ALLOC ")).expect("report").to_string();
    assert_eq!(field(&line2, "allocs"), field(line1, "allocs") + 1, "one extra list literal: {line1} -> {line2}");
    assert_eq!(field(&line2, "bytes"), field(line1, "bytes") + 24);
    assert_eq!(field(&line2, "frees"), field(line1, "frees") + 1);
    assert!(field(line1, "heap_end") > 0, "the watermark rides the same line: {line1}");
}
