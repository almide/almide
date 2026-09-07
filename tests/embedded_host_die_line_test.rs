//! #1912: a self-hosted guard's `prim.die` prints its one `Error: <msg>`
//! line and ends on `unreachable`; the embedded host (`almide run --target
//! wasm`) must not add `Error: wasm trap: …` after it — stock wasmtime and
//! native show the one line. A GENUINE trap (an unnamed one) still names
//! itself, and a named non-unreachable abort keeps its own line.

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

fn run_wasm(src: &str, name: &str) -> (i32, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(name);
    std::fs::write(&path, src).unwrap();
    let out = Command::new(almide_bin())
        .args(["run"])
        .arg(&path)
        .args(["--target", "wasm"])
        .output()
        .expect("spawn almide run --target wasm");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn a_named_die_is_one_stderr_line_on_the_embedded_host() {
    let (code, out, err) = run_wasm(
        "fn main() -> Unit = {\n  println(\"before\")\n  println(\"${int.clamp(3, 3, 1)}\")\n  println(\"after\")\n}\n",
        "die.almd",
    );
    assert_eq!(code, 1);
    assert_eq!(out.trim(), "before");
    assert_eq!(err.trim(), "Error: clamp requires min <= max", "no trap line after a named die");
}

#[test]
fn an_unnamed_trap_still_names_itself() {
    let (code, _out, err) = run_wasm(
        "fn main() -> Unit = {\n  let xs = [1, 2]\n  println(\"${xs[5]}\")\n}\n",
        "oob.almd",
    );
    assert_eq!(code, 1);
    assert!(err.trim().starts_with("Error: "), "{err}");
    assert_eq!(err.trim().lines().count(), 1, "one line: {err}");
}
