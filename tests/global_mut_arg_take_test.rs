//! A module-level `var` handed to a `mut` parameter of a bundled stdlib fn
//! with an Almide body (`bytes.put_f32_le(buf, …)`) on the native leg (#4550).
//!
//! The call used the copy-in / write-back form of #2960 — the one a callee that
//! may read the same global needs — so every 4-byte write cloned the whole
//! buffer: an immediate-mode renderer writing 80-byte instances into a
//! module-level buffer ran ~1000x slower than on 0.68, where `bytes.set_f32_le`
//! was a runtime fn written inside the global's cell. A stdlib fn whose
//! arguments cannot carry user code in cannot read the global, so the call now
//! TAKES the block out of the cell and moves it back: no copy, the same values.

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
    "almide".to_string()
}

const SRC: &str = r#"
var buf = bytes.zeros(8)
var xs: List[Int] = [1, 2, 3]

fn write(o: Int, x: Float) -> Unit = {
  bytes.put_u8(buf, o, 7)
  bytes.put_f32_le(buf, o + 4, x)
}

effect fn main() -> Unit = {
  let snap = buf
  write(0, 1.5)
  for i in 0..<3 { bytes.put_u8(buf, 1, i + 1) }
  list.push(xs, list.len(xs) + 1)
  println("${bytes.get(buf, 0) ?? 0} ${bytes.get(buf, 1) ?? 0} ${bytes.get(snap, 0) ?? 0} ${bytes.get_f32_le(buf, 4) ?? 0.0} ${bytes.len(buf)} ${list.len(xs)}")
}
"#;

const WANT: &str = "7 3 0 1.5 8 4\n";

fn source_file(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-global-mut-take-{tag}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, SRC).unwrap();
    file
}

fn run(target: Option<&str>) -> String {
    let file = source_file(target.unwrap_or("native"));
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn stdlib_mut_arg_on_a_global_keeps_its_values_natively() {
    let out = run(None);
    assert!(out.contains(WANT), "native:\n{out}");
}

#[test]
fn stdlib_mut_arg_on_a_global_keeps_its_values_on_wasm() {
    let out = run(Some("wasm"));
    assert!(out.contains(WANT), "wasm:\n{out}");
}

/// The emitted Rust moves the buffer's `Rc` out of its cell for the stdlib call
/// (no clone, no fresh `Rc`) and never clones it in: a `let mut __almide_gp` copy-in of the global is the
/// per-write copy this test exists to keep out.
#[test]
fn stdlib_mut_arg_on_a_global_is_taken_not_copied() {
    let file = source_file("emit");
    let out = Command::new(almide_bin()).arg(&file).args(["--target", "rust"]).output().expect("spawn almide");
    let rust = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    // A global `mut` place call binds the callee's result as `__almide_gr`.
    let sites: Vec<&str> = rust.match_indices("let __almide_gr = almide_rt_bytes_put_").map(|(i, _)| {
        let start = rust[..i].rfind("({ let mut __almide_gp").unwrap_or(i);
        &rust[start..i]
    }).collect();
    assert!(!sites.is_empty(), "no put_* call site in the emitted Rust:\n{rust}");
    for s in &sites {
        assert!(s.contains("from_rc(") && s.contains("std::mem::replace("), "a put_* call on the global does not move its Rc in:\n{s}");
        assert!(!s.contains(".borrow()).clone()"), "a put_* call on the global copies it in:\n{s}");
    }
}
