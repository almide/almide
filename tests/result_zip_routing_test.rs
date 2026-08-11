//! result.zip routing (#1154): the scalar self-host shim reads BOTH results'
//! tags len-as-tag, so it is linkable ONLY when both Ok payloads are scalar.
//! A heap-Ok argument on EITHER side must route to the UNLINKED `_x` — an
//! honest render wall — never to the shim (which took the err path on ok
//! inputs: fuzz seed 500705518628 index 738, a silent wrong output on wasm).
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// The wasm halves execute through wasmtime; environments without it (a bare
/// `cargo test --workspace` shard) skip them — the routing itself is still
/// pinned wherever the wasm toolchain exists (the Test WASM job, local runs
/// with /opt/homebrew on PATH).
fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok()
}

/// The night's shape: scalar-Ok FIRST argument, heap-Ok second. The first-arg
/// guard alone linked the shim for this one.
const MIXED_ZIP: &str = r#"fn main() -> Unit = {
  let y: Result[Float, String] = ok(1.0)
  let t: Result[String, String] = ok("abc")
  let r: (Float, String) = result.unwrap_or_else(result.zip(y, t), ((e) => (-42.5, "fallback")))
  println("r = ${r}")
}
"#;

const SCALAR_ZIP: &str = r#"fn main() -> Unit = {
  let y: Result[Int, String] = ok(7)
  let t: Result[Int, String] = ok(9)
  let r: (Int, Int) = result.unwrap_or_else(result.zip(y, t), ((e) => (-1, -2)))
  println("r = ${r}")
}
"#;

fn write_fixture(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("almide-zip-routing-test");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, src).unwrap();
    p
}

#[test]
fn mixed_zip_walls_on_wasm_and_runs_on_native() {
    if !tool_available() {
        eprintln!("skipping: almide binary unavailable");
        return;
    }
    let p = write_fixture("mixed_zip.almd", MIXED_ZIP);

    // Native: the properly-typed mono instance runs — the ok tuple, not the
    // fallback.
    let native = Command::new(almide_bin()).arg("run").arg(&p).output().unwrap();
    let stdout = String::from_utf8_lossy(&native.stdout);
    assert!(
        stdout.contains("r = (1, \"abc\")"),
        "native must take the ok path:\n{stdout}"
    );

    // Wasm: an honest wall — NEVER the shim's wrong value. (The wall fires at
    // RENDER time, before wasmtime, so this half needs no wasmtime either —
    // but a missing-wasmtime error would still confuse the assertions.)
    if !wasmtime_available() {
        eprintln!("skipping wasm half: wasmtime unavailable");
        return;
    }
    let wasm = Command::new(almide_bin())
        .arg("run").arg(&p).arg("--target").arg("wasm")
        .output().unwrap();
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&wasm.stdout),
        String::from_utf8_lossy(&wasm.stderr)
    );
    // The wall boilerplate itself says "silent fallback", so key on the
    // fallback tuple's VALUE, which only the mis-linked shim could print.
    assert!(
        !out.contains("-42.5"),
        "wasm must not silently take the err path on ok inputs:\n{out}"
    );
    assert!(
        out.contains("not yet supported") || out.contains("wall"),
        "wasm must wall the mixed zip instantiation honestly:\n{out}"
    );
}

#[test]
fn scalar_zip_still_links_the_shim_on_wasm() {
    if !tool_available() || !wasmtime_available() {
        eprintln!("skipping: almide binary or wasmtime unavailable");
        return;
    }
    let p = write_fixture("scalar_zip.almd", SCALAR_ZIP);
    let wasm = Command::new(almide_bin())
        .arg("run").arg(&p).arg("--target").arg("wasm")
        .output().unwrap();
    let stdout = String::from_utf8_lossy(&wasm.stdout);
    assert!(
        stdout.contains("r = (7, 9)"),
        "scalar zip must keep executing on wasm:\n{stdout}\n{}",
        String::from_utf8_lossy(&wasm.stderr)
    );
}
