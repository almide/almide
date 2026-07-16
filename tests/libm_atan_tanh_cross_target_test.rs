//! Bit-exactness lock for the vendored-libm `math.atan` / `math.tanh` (and,
//! through tanh, the `__libm_expm1` kernel).
//!
//! Same contract as libm_trig_cross_target_test: the SAME musl-libm algorithm
//! runs natively (`runtime/rs/src/libm_p4.rs`) and on WASM
//! (`emit_wasm/rt_numeric.rs::compile_math_atan/compile_math_tanh` +
//! `emit_wasm/rt_libm_p4.rs::compile_expm1`), so results are **bit-identical
//! native ↔ wasm** AND deterministic across platforms. We generate `.almd`
//! batteries printing atan/tanh over a point set that covers every branch,
//! run them on BOTH targets, and assert byte-identical stdout.
//!
//! Point set:
//!   - every atan subrange boundary (0.4375 / 0.6875 / 1.1875 / 2.4375, 2^-27,
//!     2^66) with ±eps neighbors, on both signs
//!   - every tanh/expm1 range split (log(5/3)/2, log(3)/2, 20, 0.5ln2, 1.5ln2,
//!     56ln2 overflow filter, 2^-54, subnormals)
//!   - thousands of uniform-random finite f64 (xorshift over the bit space)
//!   - ±0, ±inf, NaN (driven through arithmetic, not literals)
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
    let has_wasmtime = Command::new("wasmtime").arg("--version").output().is_ok();
    let has_node = Command::new("node").arg("--version").output().is_ok();
    has_wasmtime || has_node
}

fn run_native(source: &str, dir: &Path) -> String {
    let src_path = dir.join("atanh.almd");
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
    let src_path = dir.join("atanh.almd");
    let wasm_path = dir.join("atanh.wasm");
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
fn lit(x: f64) -> String {
    debug_assert!(x.is_finite());
    let s = format!("{:?}", x);
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

/// Battery program printing atan/tanh of each finite point, plus the
/// non-finite cases (±inf via 1/±0, NaN via 0/0) appended at the front.
fn build_program(points: &[f64]) -> String {
    let mut body = String::new();
    body.push_str("import math\n");
    body.push_str("fn main() -> Unit = {\n");
    body.push_str("  let pinf = 1.0 / 0.0\n");
    body.push_str("  let ninf = -1.0 / 0.0\n");
    body.push_str("  let nan = 0.0 / 0.0\n");
    body.push_str("  let nzero = -1.0 * 0.0\n");
    for kind in ["atan", "tanh"] {
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
        body.push_str("  println(float.to_string(math.atan(x)))\n");
        body.push_str("  println(float.to_string(math.tanh(x)))\n");
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
    let mut v = vec![
        // atan subrange boundaries (7/16, 11/16, 19/16, 39/16) ± neighbors
        0.4374, 0.4375, 0.4376, 0.6874, 0.6875, 0.6876, 1.1874, 1.1875, 1.1876,
        2.4374, 2.4375, 2.4376,
        // atan small-arg cutoff 2^-27 and the huge-arg cutoff 2^66
        7.450580596923828e-9, 7.5e-9, 7.4e-9, 7.378697629483821e19, 7.4e19, 7.3e19,
        // tanh range splits: log(5/3)/2 ≈ 0.2554, log(3)/2 ≈ 0.5493, |x| > 20
        0.2553, 0.2554, 0.2555, 0.5492, 0.5493, 0.5494, 19.9, 20.0, 20.1, 21.0,
        // expm1 branches through tanh(2x): 0.5ln2≈0.3466, 1.5ln2≈1.0397 (as 2x)
        0.1732, 0.1734, 0.5198, 0.5199, 0.52, 0.7, 1.0, 1.5,
        // expm1 overflow filter (56*ln2 ≈ 38.8 as 2x → 19.4) and k branches
        19.4, 19.41, 10.5, 15.25,
        // generic + signs
        0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0, 3.0, 10.0, 100.0, 1e6, 1e15,
        -0.3, -0.4375, -0.6875, -1.1875, -2.4375, -20.0, -25.0, -1e10,
        // tiny / subnormal (tanh subnormal branch, atan tiny return)
        1e-300, 5e-324, f64::MIN_POSITIVE, 1e-30, -5e-324, -1e-300,
        // big finite
        f64::MAX, -f64::MAX, 1e300, -1e300,
        // libm expm1 published vector argument (through tanh(0.55))
        0.55, 1.1,
    ];
    // dense neighbors around each atan reduction interval
    for k in 1..40i32 {
        let x = (k as f64) * 0.075;
        v.push(x);
        v.push(-x);
    }
    v
}

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
            "atan/tanh cross-target mismatch ({} native lines, {} wasm lines){}",
            nl.len(),
            wl.len(),
            first
        );
    }
}

#[test]
fn atan_tanh_cross_target_boundary() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    assert_battery(&boundary_points());
}

#[test]
fn atan_tanh_cross_target_random_fuzz() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let total = 3000;
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

#[test]
fn atan_tanh_native_matches_libm_published_vectors() {
    if !tools_available() {
        eprintln!("skipping: almide unavailable");
        return;
    }
    // libm atan::sanity + zero/infinity tests: atan(1) == FRAC_PI_4,
    // atan(0) == 0, atan(inf) == FRAC_PI_2 (== ATANHI[1]/[3] exactly).
    // libm expm1::sanity_check pins expm1(1.1) == 2.0041660239464334; through
    // tanh(0.55) = expm1(1.1)/(expm1(1.1)+2) that vector reaches this pipeline.
    let prog = "import math\nfn main() -> Unit = {\n  println(float.to_string(math.atan(1.0)))\n  println(float.to_string(math.atan(0.0)))\n  println(float.to_string(math.atan(1.0 / 0.0)))\n  println(float.to_string(math.tanh(0.55)))\n}\n";
    let dir = tempfile::tempdir().unwrap();
    let native = run_native(prog, dir.path());
    let lines: Vec<&str> = native.lines().collect();
    assert_eq!(lines[0], format!("{}", std::f64::consts::FRAC_PI_4));
    assert_eq!(lines[1], "0.0");
    assert_eq!(lines[2], format!("{}", std::f64::consts::FRAC_PI_2));
    // tanh(0.55) via the pinned expm1 vector: 0.55 > log(3)/2, so the
    // algorithm computes t = expm1(1.1) = 2.0041660239464334 (the upstream
    // sanity vector) and returns 1 - 2/(t + 2), computed in f64.
    let t = 2.0041660239464334_f64;
    assert_eq!(lines[3], format!("{}", 1.0 - 2.0 / (t + 2.0)));
}
