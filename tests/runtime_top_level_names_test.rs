//! The native runtime is concatenated into ONE flat Rust file with the
//! user's program, so every top-level runtime item shares a namespace with
//! every user function (#1957). A bare runtime helper is a name the user may
//! not declare: `fn log` in a module collided with the vendored libm kernel
//! (`E0428: the name log is defined multiple times`) the moment the module
//! was compiled on the native leg. Two gates:
//!
//! 1. Every top-level `fn` / `struct` / `enum` / `type` / `trait` /
//!    `macro_rules!` the runtime emits is `almide`-prefixed (the libm kernels
//!    live inside `mod almide_libm`, whose body is skipped).
//! 2. A program declaring functions under the kernels' and helpers' old bare
//!    names compiles on the native (v0 codegen) leg and prints its answer.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn emit_rust(src: &str, name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-issue1957-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let almd = dir.join(format!("{name}.almd"));
    std::fs::write(&almd, src).expect("write");
    let out = Command::new(almide_bin())
        .args([almd.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("spawn almide");
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Top-level item names of the emitted file, with the libm module body
/// skipped (its kernels are scoped, not flat).
fn top_level_names(rust: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_libm = false;
    for line in rust.lines() {
        if line.starts_with("mod almide_libm {") {
            in_libm = true;
            continue;
        }
        if line.starts_with("} // mod almide_libm") {
            in_libm = false;
            continue;
        }
        if in_libm || line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        let rest = line.strip_prefix("pub ").unwrap_or(line);
        let rest = rest.strip_prefix("pub(crate) ").unwrap_or(rest);
        for kw in ["fn ", "struct ", "enum ", "type ", "trait ", "macro_rules! "] {
            if let Some(after) = rest.strip_prefix(kw) {
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    names.push(name);
                }
            }
        }
    }
    names
}

#[test]
fn every_top_level_runtime_item_is_almide_prefixed() {
    if !tools_available() {
        return;
    }
    let rust = emit_rust("fn main() -> Unit = println(\"x\")\n", "hello");
    let bare: Vec<String> = top_level_names(&rust)
        .into_iter()
        .filter(|n| !(n.starts_with("almide") || n.starts_with("Almide") || n.starts_with("__almide") || n == "main"))
        .collect();
    assert!(
        bare.is_empty(),
        "bare top-level runtime items (a user fn of the same name is E0428):\n{}",
        bare.join("\n")
    );
}

const SHADOWING_PROGRAM: &str = "\
fn log(text: String) -> Option[String] = if string.len(text) > 0 then some(string.to_upper(text)) else none
fn exp(n: Int) -> Int = n * 2
fn sin(s: String) -> String = s + \"!\"
fn pow(a: Int, b: Int) -> Int = a * b
fn key_slot(k: String) -> Int = string.len(k)
fn value_kind(n: Int) -> String = if n > 0 then \"pos\" else \"non-pos\"
fn civil_from_epoch(n: Int) -> Int = n + 1
fn rotate_mask(n: Int) -> Int = n - 1
effect fn main() -> Unit = println((log(\"abc\") ?? \"none\") + \" \" + int.to_string(exp(3)) + \" \" + sin(\"a\") + \" \" + int.to_string(pow(2, 5)) + \" \" + int.to_string(key_slot(\"xy\")) + \" \" + value_kind(1) + \" \" + int.to_string(civil_from_epoch(1)) + \" \" + int.to_string(rotate_mask(1)))
";

#[test]
fn user_functions_named_like_runtime_helpers_compile_on_the_native_leg() {
    if !tools_available() || Command::new("rustc").arg("--version").output().is_err() {
        return;
    }
    let rust = emit_rust(SHADOWING_PROGRAM, "shadow");
    let dir = std::env::temp_dir().join(format!("almide-issue1957-rustc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("shadow.rs");
    std::fs::write(&src, rust).expect("write");
    let bin = dir.join("shadow");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-O", "-o", bin.to_str().unwrap(), src.to_str().unwrap()])
        .output()
        .expect("spawn rustc");
    assert!(rustc.status.success(), "rustc rejected the emit:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let run = Command::new(&bin).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ABC 6 a! 10 2 pos 2 0\n");
    let _ = std::fs::remove_dir_all(&dir);
}
