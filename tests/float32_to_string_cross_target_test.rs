//! Lock for the Float32 printer (stdlib/float_to_string.almd's f32 Schubfach,
//! C-372) against native Rust's `f32` Display: `${x}` and `float32.to_string`
//! must print the shortest decimal that round-trips to the same binary32.
//!
//! The program replays a deterministic xorshift32 stream INSIDE itself and
//! prints both forms for:
//!   * every biased exponent (0..=255) with significands {0, 1, 2, 3, mid,
//!     max-1, max}, each with the sign bit clear and set — every power of two
//!     (the asymmetric interval), the subnormal boundary, MIN/MAX, ±0, ±inf,
//!     NaN;
//!   * the exact-tie shapes, where the magnitude rounds up (2^-12 prints
//!     0.00024414063, 2^20 + 0.25 prints 1048576.3);
//!   * ALMIDE_F32_SWEEP_N (default 100,000) pseudo-random bit patterns.
//!
//! The oracle is Rust's `format!("{}", f32)` (+ the `.0` suffix of an integral
//! value for `to_string`). Skips cleanly when the `almide` binary or a WASM
//! runtime is unavailable.

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
        && Command::new("wasmtime").arg("--version").output().is_ok()
}

fn run(source: &str, dir: &Path, wasm: bool) -> String {
    let src = dir.join("f32.almd");
    std::fs::write(&src, source).unwrap();
    let mut args = vec!["run", src.to_str().unwrap()];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    let out = Command::new(almide_bin()).args(&args).output().expect("run almide");
    assert!(out.status.success(), "run failed (wasm={wasm}):\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const SEED: u32 = 0x9E37_79B9;

fn xorshift(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

fn sweep_n() -> u64 {
    std::env::var("ALMIDE_F32_SWEEP_N").ok().and_then(|s| s.parse().ok()).unwrap_or(100_000)
}

fn fixed() -> Vec<u32> {
    let mut v = Vec::new();
    for e in 0..256u32 {
        for m in [0u32, 1, 2, 3, 1 << 22, (1 << 23) - 2, (1 << 23) - 1] {
            v.push((e << 23) | m);
            v.push((1 << 31) | (e << 23) | m);
        }
    }
    v.push((2.0f32).powi(-12).to_bits());
    v.push(1048576.25f32.to_bits());
    v
}

fn build_program(n: u64) -> String {
    let mut s = String::from(
        "fn show(b: Int) -> Unit = {\n  let g: Float32 = float.to_float32(int.bits_to_f32(b))\n  println(\"${g} ${float32.to_string(g)}\")\n}\n\nfn main() -> Unit = {\n",
    );
    for b in fixed() {
        s.push_str(&format!("  show({b})\n"));
    }
    s.push_str(&format!("  var x = {SEED}\n"));
    s.push_str(&format!("  for _ in 0..<{n} {{\n"));
    // xorshift32 on the low 32 bits of the i64 carrier
    s.push_str("    let a = int.band(int.bxor(x, int.bshl(x, 13)), 4294967295)\n");
    s.push_str("    let b = int.bxor(a, int.bshr(a, 17))\n");
    s.push_str("    x = int.band(int.bxor(b, int.bshl(b, 5)), 4294967295)\n");
    s.push_str("    show(x)\n");
    s.push_str("  }\n}\n");
    s
}

fn oracle(n: u64) -> String {
    let mut out = String::new();
    let mut show = |b: u32| {
        let g = f32::from_bits(b);
        let d = format!("{g}");
        let whole = g.is_finite() && g.fract() == 0.0 && !d.contains('.');
        out.push_str(&format!("{d} {}\n", if whole { format!("{d}.0") } else { d.clone() }));
    };
    for b in fixed() {
        show(b);
    }
    let mut x = SEED;
    for _ in 0..n {
        x = xorshift(x);
        show(x);
    }
    out
}

fn check(wasm: bool) {
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    let n = sweep_n();
    let dir = tempfile::tempdir().unwrap();
    let got = run(&build_program(n), dir.path(), wasm);
    let want = oracle(n);
    if got != want {
        let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
        let i = (0..g.len().max(w.len())).find(|&i| g.get(i) != w.get(i)).unwrap();
        panic!("wasm={wasm}: first mismatch at line {} of {}: got {:?} want {:?}", i + 1, w.len(), g.get(i), w.get(i));
    }
}

#[test]
fn float32_display_wasm_matches_rust_over_the_sweep() {
    check(true);
}

#[test]
fn float32_display_native_matches_rust_over_the_sweep() {
    check(false);
}
