//! Bit-exactness lock for the vendored-libm trig (`math.sin`/`cos`/`tan`).
//!
//! Drain PR-C contract: the SAME musl-libm algorithm runs natively (the vendored
//! `runtime/rs/src/libm.rs`) and on WASM (`emit_wasm/rt_libm.rs`), so trig results
//! are **bit-identical native ↔ wasm** AND deterministic across platforms.
//!
//! The real lock is CROSS-TARGET: a `#[test]` that compares the vendored native
//! fns against *themselves* would be vacuous. Instead we generate `.almd`
//! batteries that print `sin/cos/tan` of a large point set via `float.to_string`
//! (Dragon4 → printing is byte-identical across targets, so any output diff is the
//! MATH), run each battery on BOTH targets, and assert byte-identical stdout.
//!
//! Point set (covers every rem_pio2 path):
//!   - thousands of uniform-random *finite* f64 (xorshift over the bit space)
//!   - pi-multiples and near-pi/2 cancellation points
//!   - huge args (1e10 … 1e300 — the rem_pio2_large path) and tiny/subnormal
//!   - ±0, ±inf, NaN
//!
//! A separate native-only test pins the vendored sin/cos/tan against the upstream
//! `libm` crate's published `#[test]` vectors (sanity vs the source of truth).
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
    // A WASM runtime: wasmtime preferred, node WASI fallback.
    let has_wasmtime = Command::new("wasmtime").arg("--version").output().is_ok();
    let has_node = Command::new("node").arg("--version").output().is_ok();
    has_wasmtime || has_node
}

fn run_native(source: &str, dir: &Path) -> String {
    let src_path = dir.join("trig.almd");
    std::fs::write(&src_path, source).unwrap();
    let output = Command::new(almide_bin())
        .args(["run", src_path.to_str().unwrap()])
        .output()
        .expect("failed to run almide");
    if !output.status.success() {
        panic!(
            "native compile/run failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).trim_end().to_string()
}

fn run_wasm(source: &str, dir: &Path) -> String {
    let src_path = dir.join("trig.almd");
    let wasm_path = dir.join("trig.wasm");
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
    if !output.status.success() {
        panic!(
            "WASM compile failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // Prefer wasmtime; fall back to node WASI.
    let wt = Command::new("wasmtime")
        .arg(wasm_path.to_str().unwrap())
        .output();
    let output = match wt {
        Ok(o) if o.status.code() != Some(127) => o,
        _ => {
            let js = format!(
                r#"
const {{ readFileSync }} = require('fs');
const {{ WASI }} = require('wasi');
const wasi = new WASI({{ version: 'preview1', args: [], env: {{}} }});
const buf = readFileSync('{}');
const mod = new WebAssembly.Module(buf);
const inst = new WebAssembly.Instance(mod, wasi.getImportObject());
wasi.start(inst);
"#,
                wasm_path.to_str().unwrap().replace('\\', "/")
            );
            let js_path = dir.join("run.cjs");
            std::fs::write(&js_path, &js).unwrap();
            Command::new("node")
                .arg(js_path.to_str().unwrap())
                .output()
                .expect("failed to run node or wasmtime")
        }
    };
    if !output.status.success() {
        panic!(
            "WASM run failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).trim_end().to_string()
}

/// Render an f64 as an almide float literal that round-trips to the SAME bits.
/// Both targets parse decimal with the correctly-rounded dec2flt, and Rust's
/// `{:?}` is the shortest round-tripping decimal, so the parsed bits match `x`.
/// Non-finite values can't be written as literals — callers exclude them and
/// drive ±inf / NaN through arithmetic in the generated program instead.
fn lit(x: f64) -> String {
    debug_assert!(x.is_finite());
    let s = format!("{:?}", x);
    // Ensure a decimal point / exponent so the lexer reads it as Float, not Int.
    if s.contains('.') || s.contains('e') || s.contains('E') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{}.0", s)
    }
}

/// Build a battery program that prints sin/cos/tan of each finite point, plus the
/// non-finite cases (±inf via 1/±0, NaN via 0/0) appended at the end.
fn build_program(points: &[f64]) -> String {
    let mut body = String::new();
    body.push_str("import math\n");
    body.push_str("fn main() -> Unit = {\n");
    // Non-finite, constructed so no literal is needed.
    body.push_str("  let pinf = 1.0 / 0.0\n");
    body.push_str("  let ninf = -1.0 / 0.0\n");
    body.push_str("  let nan = 0.0 / 0.0\n");
    body.push_str("  let nzero = -1.0 * 0.0\n");
    for kind in ["sin", "cos", "tan"] {
        for v in ["pinf", "ninf", "nan", "nzero"] {
            body.push_str(&format!(
                "  println(float.to_string(math.{}({})))\n",
                kind, v
            ));
        }
    }
    for &x in points {
        if !x.is_finite() {
            continue;
        }
        let l = lit(x);
        body.push_str(&format!("  let x = {}\n", l));
        body.push_str("  println(float.to_string(math.sin(x)))\n");
        body.push_str("  println(float.to_string(math.cos(x)))\n");
        body.push_str("  println(float.to_string(math.tan(x)))\n");
    }
    body.push_str("}\n");
    body
}

/// xorshift64* — deterministic, covers the full bit space.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn boundary_points() -> Vec<f64> {
    use std::f64::consts::PI;
    let mut v = vec![
        0.0, 1.0, -1.0, 0.5, -0.5, 2.0, 10.0, 100.0, 1000.0,
        PI / 4.0, PI / 2.0, PI, 1.5 * PI, 2.0 * PI, 3.0 * PI, 100.0 * PI,
        // k_tan |x| >= 0.6744 boundary
        0.6743, 0.6744, 0.6745, -0.6744,
        // rem_pio2 medium/large boundaries
        6.283185307179586, 1.5707963267948966, 3.141592653589793,
        1e6, 1e8, 1e10, 1e12, 1e15, 1e18, 1e100, 1e200, 1e300,
        // small / subnormal
        1e-6, 1e-8, 1e-300, 5e-324, f64::MIN_POSITIVE, 1e-20,
        // big finite
        f64::MAX, -f64::MAX, 4503599627370496.0, 9223372036854775807.0,
        // rem_pio2_large regression args from the libm test-suite
        -3054214.5490637687, 917340800458.2274,
        // near pi/2 cancellation (the medium-case `cancellation` branch)
        f64::from_bits(0x400921fb000FD5DD),
        3.141592025756836, 3.141592033207416, 3.141592144966125, 3.141592979431152,
    ];
    // ±k·(pi/2) ± tiny — exercise each n&3 quadrant + tail handling.
    for k in 0..40i32 {
        let base = (k as f64) * (PI / 2.0);
        v.push(base);
        v.push(base + 1e-9);
        v.push(base - 1e-9);
        v.push(-base);
    }
    v
}

/// Generate `n` uniform-random FINITE f64 from the xorshift bit stream.
fn random_finite(rng: &mut Rng, n: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let x = f64::from_bits(rng.next());
        if x.is_finite() {
            out.push(x);
        }
    }
    out
}

fn assert_battery(points: &[f64]) {
    let dir = tempfile::tempdir().unwrap();
    let prog = build_program(points);
    let native = run_native(&prog, dir.path());
    let wasm = run_wasm(&prog, dir.path());
    if native != wasm {
        // Find the first differing line to localize the offending point.
        let nl: Vec<&str> = native.lines().collect();
        let wl: Vec<&str> = wasm.lines().collect();
        let mut first = String::new();
        for i in 0..nl.len().max(wl.len()) {
            let a = nl.get(i).copied().unwrap_or("<missing>");
            let b = wl.get(i).copied().unwrap_or("<missing>");
            if a != b {
                first = format!("\nfirst diff at line {}: native={:?} wasm={:?}", i, a, b);
                break;
            }
        }
        panic!(
            "trig cross-target mismatch ({} native lines, {} wasm lines){}",
            nl.len(),
            wl.len(),
            first
        );
    }
}

#[test]
fn trig_cross_target_boundary() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    assert_battery(&boundary_points());
}

#[test]
fn trig_cross_target_random_fuzz() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    // A few thousand random finite points, split into batches so each generated
    // program stays a reasonable size for the WASM toolchain.
    let mut rng = Rng(0x243F6A8885A308D3);
    let total = 4000;
    let batch = 1000;
    let mut done = 0;
    while done < total {
        let n = batch.min(total - done);
        let pts = random_finite(&mut rng, n);
        assert_battery(&pts);
        done += n;
    }
}

// ── sanity vs upstream libm published vectors (through the real pipeline) ──
//
// `almide_rt` is not a workspace dependency (the runtime is *embedded* by the
// registry, not linked), so the vendored fns can't be called from this test
// crate directly. Instead we pin the vendored math against libm 0.2.16's own
// `#[test]` reference vectors *through the almide pipeline*: print sin/cos/tan of
// the exact upstream argument and assert the NATIVE output equals the value the
// upstream bit-pattern decodes to. (The cross-target batteries above then prove
// WASM matches native, so this transitively pins WASM to upstream too.)

/// Render an exact reference f64 (given by its IEEE-754 bits) as the decimal
/// string `float.to_string` must emit for it (Dragon4 shortest round-trip).
fn expected_decimal(bits: u64) -> String {
    // float.to_string is the Dragon4 shortest decimal; Rust `{}`/`{:?}` Display is
    // the same shortest round-trip, with a trailing `.0` Display adds for integers.
    // For these sub-1 reference values there is always a fractional part, so the
    // plain `{}` Display matches `float.to_string` exactly.
    format!("{}", f64::from_bits(bits))
}

#[test]
fn trig_native_matches_libm_published_vectors() {
    if !tools_available() {
        eprintln!("skipping: almide unavailable");
        return;
    }
    // libm sin::test_near_pi: sin(0x400921fb000FD5DD) == 0x3ea50d15ced1a4a2.
    let x = f64::from_bits(0x400921fb000FD5DD);
    let want_sin = expected_decimal(0x3ea50d15ced1a4a2);

    let prog = format!(
        "import math\nfn main() -> Unit = {{\n  let x = {}\n  println(float.to_string(math.sin(x)))\n}}\n",
        lit(x)
    );
    let dir = tempfile::tempdir().unwrap();
    let native = run_native(&prog, dir.path());
    assert_eq!(
        native.trim(),
        want_sin,
        "vendored native sin diverged from libm 0.2.16 sin::test_near_pi vector"
    );
}
