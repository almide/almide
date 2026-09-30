//! Lock for float `%` on the wasm leg (stdlib/float_fmod.almd, #3080), which has
//! no float remainder instruction: its exact shift-and-subtract remainder must
//! give native Rust's `%` bit for bit — for Float, and for Float32, whose `%`
//! is the same exact remainder of the widened operands (C-371).
//!
//! The program replays a deterministic xorshift64 stream INSIDE itself and
//! prints `float.to_bits(x % y)` for every pair, over:
//!   * every biased exponent pairing class — each x exponent against y at the
//!     same, one below and far below exponent (the long shift loop), subnormals
//!     on both sides, and the specials (±0, ±inf, NaN, equal magnitudes);
//!   * ALMIDE_FMOD_SWEEP_N (default 20,000) pseudo-random bit-pattern pairs;
//!   * the same pairs rounded to Float32 and taken `%` as Float32.
//!
//! The oracle is computed here in Rust. Skips cleanly when the `almide`
//! binary or a WASM runtime is unavailable.

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
    let src = dir.join("fmod.almd");
    std::fs::write(&src, source).unwrap();
    let mut args = vec!["run", src.to_str().unwrap()];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    let out = Command::new(almide_bin()).args(&args).output().expect("run almide");
    assert!(out.status.success(), "run failed (wasm={wasm}):\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

fn xorshift(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn sweep_n() -> u64 {
    std::env::var("ALMIDE_FMOD_SWEEP_N").ok().and_then(|s| s.parse().ok()).unwrap_or(20_000)
}

/// The fixed pairs, as f64 bit patterns.
fn fixed_pairs() -> Vec<(u64, u64)> {
    let specials = [0u64, 1 << 63, 0x7FF0_0000_0000_0000, 0xFFF0_0000_0000_0000, 0x7FF8_0000_0000_0000, 1, 0x000F_FFFF_FFFF_FFFF];
    let mut v = Vec::new();
    for &a in &specials {
        for &b in &specials {
            v.push((a, b));
        }
        v.push((a, 1.5f64.to_bits()));
        v.push((1.5f64.to_bits(), a));
    }
    for e in (1..2047u64).step_by(7) {
        let x = (e << 52) | 0x000A_BCDE_F012_3456;
        for d in [0u64, 1, 60, 700] {
            if e > d {
                v.push((x, ((e - d) << 52) | 0x0003_1415_9265_3589));
            }
        }
        v.push((x, 0x0000_0000_0001_2345)); // a subnormal divisor
    }
    // equal magnitudes: a zero carrying x's sign
    v.push((7.0f64.to_bits(), (-7.0f64).to_bits()));
    v.push(((-7.0f64).to_bits(), 7.0f64.to_bits()));
    v
}

fn build_program(n: u64) -> String {
    let mut s = String::from(
        "fn show(a: Int, b: Int) -> Unit = {\n  let x = int.bits_to_float(a)\n  let y = int.bits_to_float(b)\n  println(int.to_string(float.to_bits(x % y)))\n  let p: Float32 = float.to_float32(x)\n  let q: Float32 = float.to_float32(y)\n  println(int.to_string(float.to_bits(float32.to_float64(p % q))))\n}\n\nfn main() -> Unit = {\n",
    );
    for (a, b) in fixed_pairs() {
        s.push_str(&format!("  show({}, {})\n", a as i64, b as i64));
    }
    s.push_str(&format!("  var x = 0 - {}\n", (SEED as i64).unsigned_abs()));
    s.push_str(&format!("  for _ in 0..<{n} {{\n"));
    // two xorshift64 steps per pair: (first, second)
    let step = "    let a1 = int.bxor(x, int.bshl(x, 13))\n    let b1 = int.bxor(a1, int.band(int.bshr(a1, 7), 144115188075855871))\n    x = int.bxor(b1, int.bshl(b1, 17))\n";
    s.push_str(step);
    s.push_str("    let first = x\n");
    s.push_str(step);
    s.push_str("    show(first, x)\n");
    s.push_str("  }\n}\n");
    s
}

fn canon(f: f64) -> u64 {
    if f.is_nan() { 0x7FF8_0000_0000_0000 } else { f.to_bits() }
}

fn oracle(n: u64) -> String {
    let mut out = String::new();
    let mut show = |a: u64, b: u64| {
        let (x, y) = (f64::from_bits(a), f64::from_bits(b));
        out.push_str(&format!("{}\n", canon(x % y) as i64));
        let (p, q) = (x as f32, y as f32);
        out.push_str(&format!("{}\n", canon((p % q) as f64) as i64));
    };
    for (a, b) in fixed_pairs() {
        show(a, b);
    }
    let mut x = SEED;
    for _ in 0..n {
        x = xorshift(x);
        let first = x;
        x = xorshift(x);
        show(first, x);
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
fn float_rem_wasm_matches_rust_over_the_sweep() {
    check(true);
}

#[test]
fn float_rem_native_matches_rust_over_the_sweep() {
    check(false);
}
