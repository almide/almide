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
fn mixed_zip_runs_identically_on_both_targets() {
    // GRADUATED (#1527 frontier row, the result.zip_fs twin): this shape used
    // to be the honest `result.zip_x` wall — the `_fs` twin now reads side a
    // len-as-tag and side b cap-as-tag, and the tag-aware `$__drop_res_fs`
    // route frees the pair's interior exactly (the #1530 cap harness holds
    // the churn flat). The old wall pin flips to a support pin: both targets
    // must print the SAME ok tuple — and the fallback value must NEVER
    // appear (the original #1154 mis-link printed exactly that).
    if !tool_available() {
        eprintln!("skipping: almide binary unavailable");
        return;
    }
    let p = write_fixture("mixed_zip.almd", MIXED_ZIP);

    let native = Command::new(almide_bin()).arg("run").arg(&p).output().unwrap();
    let native_out = String::from_utf8_lossy(&native.stdout).to_string();
    assert!(
        native_out.contains("r = (1, \"abc\")"),
        "native must take the ok path:\n{native_out}"
    );

    if !wasmtime_available() {
        eprintln!("skipping wasm half: wasmtime unavailable");
        return;
    }
    let wasm = Command::new(almide_bin())
        .arg("run").arg(&p).arg("--target").arg("wasm")
        .output().unwrap();
    let wasm_out = String::from_utf8_lossy(&wasm.stdout).to_string();
    assert!(
        !wasm_out.contains("-42.5"),
        "wasm must not silently take the err path on ok inputs:\n{wasm_out}"
    );
    assert_eq!(
        native_out, wasm_out,
        "the graduated mixed zip must be byte-identical across targets"
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
