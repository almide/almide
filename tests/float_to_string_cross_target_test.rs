//! Lock for `float.to_string`, the `${x}` form and `float.to_fixed` (the self-hosted
//! printers in stdlib/float_to_string.almd) against native Rust `format!` — C-023 /
//! C-011 / C-025.
//!
//! The wasm leg prints every value of a deterministic set in all three forms
//! (`to_fixed` at precision `bits & 7`, plus 40 for the exponent sweep and the
//! specials) and the output must be byte-identical to the oracle computed here in
//! Rust; the native leg is run over the same program as the cross-target half. The
//! set is generated INSIDE the program (the same xorshift64 stream and exponent sweep
//! are replayed here), so the source stays small while the sweep is large:
//!
//!   * every biased exponent (0..=2047) with significands {0, 1, 2, 3, mid, max-1, max},
//!     each with the sign bit clear and set — 28,672 values: every power of two, every
//!     subnormal boundary, MIN/MAX, +-0, +-inf, quiet/signalling NaN;
//!   * the decimal specials pinned by the printer's history: 0.1-style values, 1/3,
//!     2^53 +- 1, the 1e15..1e23 anchors, the 1669760939663944.25 tie (rounds UP, not to
//!     even), 20 * 2^-1074 (the one-digit "1e-322" whose shorter candidate Java skips);
//!   * `to_fixed` at the long precisions: MIN_VALUE's full 1074-digit expansion and past
//!     it, the 1200-digit differential-fuzz finding, the 4096 domain ceiling;
//!   * ALMIDE_FLOAT_SWEEP_N (default 100,000) xorshift64 bit patterns over the whole
//!     64-bit space — mostly huge and tiny magnitudes, the expensive end for a printer.
//!
//! Skips cleanly when the `almide` binary or a WASM runtime is unavailable.

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
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        return false;
    }
    Command::new("wasmtime").arg("--version").output().is_ok()
}

fn run_native(source: &str, dir: &Path) -> String {
    let src_path = dir.join("sweep.almd");
    std::fs::write(&src_path, source).unwrap();
    let output = Command::new(almide_bin())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .expect("failed to run almide");
    assert!(
        output.status.success(),
        "native run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_wasm(source: &str, dir: &Path) -> String {
    let src_path = dir.join("sweep.almd");
    let wasm_path = dir.join("sweep.wasm");
    std::fs::write(&src_path, source).unwrap();
    let output = Command::new(almide_bin())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build WASM");
    assert!(
        output.status.success(),
        "WASM build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new("wasmtime")
        .arg(wasm_path.to_str().unwrap())
        .output()
        .expect("failed to run wasmtime");
    assert!(
        output.status.success(),
        "WASM run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The native `float.to_string`: Rust Display plus the `.0` suffix on whole values
/// (runtime/rs/src/float.rs `almide_rt_float_to_string`).
fn oracle_to_string(x: f64) -> String {
    let s = format!("{}", x);
    if x.fract() == 0.0 && !s.contains('.') && !s.contains("inf") && !s.contains("NaN") {
        format!("{}.0", s)
    } else {
        s
    }
}

/// The `${x}` form (C-011): Rust Display as is — a whole value has no `.0`.
fn oracle_compound(x: f64) -> String {
    format!("{}", x)
}

const SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const SIGN: u64 = 1 << 63;
const MASK57: u64 = (1 << 57) - 1;

fn xorshift(x: u64) -> u64 {
    let a = x ^ (x << 13);
    let b = a ^ ((a >> 7) & MASK57);
    b ^ (b << 17)
}

/// The decimal specials, as source literals (parsed by the SAME correctly-rounded
/// parser on both legs) paired with the f64 the oracle formats.
fn specials() -> Vec<(&'static str, f64)> {
    vec![
        ("0.1", 0.1),
        ("0.2", 0.2),
        ("0.3", 0.3),
        ("0.1 + 0.2", 0.1 + 0.2),
        ("1.0 / 3.0", 1.0 / 3.0),
        ("2.0 / 3.0", 2.0 / 3.0),
        ("1.25", 1.25),
        ("100.0", 100.0),
        ("123456789.0", 123456789.0),
        ("9007199254740992.0", 9007199254740992.0),
        ("9007199254740993.0", 9007199254740993.0),
        ("9007199254740991.0", 9007199254740991.0),
        ("1e15", 1e15),
        ("1e16", 1e16),
        ("1e17", 1e17),
        ("1e21", 1e21),
        ("1e22", 1e22),
        ("1e23", 1e23),
        ("1e-7", 1e-7),
        ("1e-300", 1e-300),
        ("1e300", 1e300),
        ("1669760939663944.25", 1669760939663944.25),
        ("5e-324", 5e-324),
        ("1e-322", 1e-322),
        ("2.2250738585072014e-308", 2.2250738585072014e-308),
        ("1.7976931348623157e308", 1.7976931348623157e308),
        ("4.35", 4.35),
        ("2.675", 2.675),
        ("0.000001", 0.000001),
        ("123.456", 123.456),
        ("1234567.0", 1234567.0),
        ("0.5", 0.5),
        ("3.0e-5", 3.0e-5),
    ]
}

/// `to_fixed` at the precisions the fixed path has been wrong at before: the whole 1074-digit
/// exact expansion of MIN_VALUE, a precision past it (trailing zeros), the 1200-digit case of
/// the differential-fuzz finding (seed 1784965210163815000), and the 4096 domain ceiling.
fn long_fixed() -> Vec<(&'static str, usize)> {
    vec![
        ("int.bits_to_float(1)", 1074),
        ("int.bits_to_float(1)", 1100),
        ("1.5", 1200),
        ("0.1", 4096),
        ("1e300", 4096),
        ("2.2250738585072014e-308", 4096),
        ("0.0", 4096),
    ]
}

fn long_fixed_oracle() -> Vec<(f64, usize)> {
    vec![
        (f64::from_bits(1), 1074),
        (f64::from_bits(1), 1100),
        (1.5, 1200),
        (0.1, 4096),
        (1e300, 4096),
        (2.2250738585072014e-308, 4096),
        (0.0, 4096),
    ]
}

fn sweep_n() -> u64 {
    std::env::var("ALMIDE_FLOAT_SWEEP_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000)
}

/// The program: `show` prints both forms of one bit pattern; the exponent sweep, the
/// specials (each with both signs) and the xorshift stream, in that order.
fn build_program(n: u64) -> String {
    let mut src = String::new();
    src.push_str("fn show(b: Int) -> Unit = {\n");
    src.push_str("  let x = int.bits_to_float(b)\n");
    src.push_str("  println(float.to_string(x))\n");
    src.push_str("  println(\"${x}\")\n");
    src.push_str("  println(float.to_fixed(x, int.band(b, 7)))\n");
    src.push_str("}\n\n");
    src.push_str("fn both(b: Int) -> Unit = {\n");
    src.push_str("  show(b)\n");
    src.push_str("  show(int.bxor(b, int.bshl(1, 63)))\n");
    src.push_str("  println(float.to_fixed(int.bits_to_float(b), 40))\n");
    src.push_str("}\n\n");
    src.push_str("fn main() -> Unit = {\n");
    for (lit, nd) in long_fixed() {
        src.push_str(&format!("  println(float.to_fixed({}, {}))\n", lit, nd));
    }
    src.push_str(
        "  let mants = [0, 1, 2, 3, 2251799813685248, 4503599627370494, 4503599627370495]\n",
    );
    src.push_str("  for e in 0..<2048 {\n");
    src.push_str("    for m in mants { both(int.bor(int.bshl(e, 52), m)) }\n");
    src.push_str("  }\n");
    for (lit, _) in specials() {
        src.push_str(&format!("  both(float.to_bits({}))\n", lit));
    }
    // SEED has the sign bit set; spell it as a subtraction (no signed literal needed).
    src.push_str(&format!("  var x = 0 - {}\n", (SEED as i64).unsigned_abs()));
    src.push_str(&format!("  for _ in 0..<{} {{\n", n));
    src.push_str("    let a = int.bxor(x, int.bshl(x, 13))\n");
    src.push_str("    let b = int.bxor(a, int.band(int.bshr(a, 7), 144115188075855871))\n");
    src.push_str("    x = int.bxor(b, int.bshl(b, 17))\n");
    src.push_str("    show(x)\n");
    src.push_str("  }\n");
    src.push_str("}\n");
    src
}

/// The oracle output for the same program, one line per printed form.
fn oracle(n: u64) -> String {
    fn show(out: &mut String, bits: u64) {
        let x = f64::from_bits(bits);
        out.push_str(&oracle_to_string(x));
        out.push('\n');
        out.push_str(&oracle_compound(x));
        out.push('\n');
        out.push_str(&format!("{:.*}\n", (bits & 7) as usize, x));
    }
    fn both(out: &mut String, b: u64) {
        show(out, b);
        show(out, b ^ SIGN);
        out.push_str(&format!("{:.40}\n", f64::from_bits(b)));
    }
    let mut out = String::new();
    for (x, nd) in long_fixed_oracle() {
        out.push_str(&format!("{:.*}\n", nd, x));
    }
    let mants: [u64; 7] = [0, 1, 2, 3, 1 << 51, (1 << 52) - 2, (1 << 52) - 1];
    for e in 0..2048u64 {
        for &m in &mants {
            both(&mut out, (e << 52) | m);
        }
    }
    for (_, x) in specials() {
        both(&mut out, x.to_bits());
    }
    let mut x = SEED;
    for _ in 0..n {
        x = xorshift(x);
        show(&mut out, x);
    }
    out
}

fn first_diff(label: &str, got: &str, want: &str) {
    if got == want {
        return;
    }
    let gl: Vec<&str> = got.lines().collect();
    let wl: Vec<&str> = want.lines().collect();
    for i in 0..gl.len().max(wl.len()) {
        let g = gl.get(i).copied().unwrap_or("<none>");
        let w = wl.get(i).copied().unwrap_or("<none>");
        if g != w {
            panic!(
                "{label}: first mismatch at line {} ({} lines got, {} lines expected):\n  got:  {g:?}\n  want: {w:?}",
                i + 1,
                gl.len(),
                wl.len()
            );
        }
    }
    panic!("{label}: outputs differ only in trailing whitespace");
}

#[test]
fn to_string_wasm_matches_rust_display_over_the_sweep() {
    if !tools_available() {
        eprintln!("skipping: almide or wasmtime unavailable");
        return;
    }
    let n = sweep_n();
    let dir = tempfile::tempdir().unwrap();
    let wasm = run_wasm(&build_program(n), dir.path());
    first_diff(
        "wasm float.to_string / ${x} vs Rust format!",
        &wasm,
        &oracle(n),
    );
}

#[test]
fn to_string_native_matches_rust_display_over_the_sweep() {
    if !tools_available() {
        eprintln!("skipping: almide unavailable");
        return;
    }
    let n = sweep_n();
    let dir = tempfile::tempdir().unwrap();
    let native = run_native(&build_program(n), dir.path());
    first_diff(
        "native float.to_string / ${x} vs Rust format!",
        &native,
        &oracle(n),
    );
}
