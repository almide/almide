//! Bit-exactness lock for the vendored-libm exp/log/log2/log10 and float `**`.
//!
//! Drain PR-C Phase B: the SAME musl-libm algorithm runs natively (the vendored
//! `runtime/rs/src/libm.rs`) and on WASM (`emit_wasm/rt_libm.rs`), so the results
//! are **bit-identical native <-> wasm** AND deterministic across platforms.
//!
//! The lock is CROSS-TARGET: generate `.almd` batteries that print exp/log/log2/
//! log10 and `b ** e` of a large point set via `float.to_string` (Dragon4 ->
//! printing is byte-identical across targets, so any output diff is the MATH), run
//! each battery on BOTH targets, and assert byte-identical stdout. Boundary cases
//! pin the sweep-confirmed special values (log(0)=-inf, log(neg)=NaN, exp(-745)=
//! subnormal, exp overflow=inf, pow(-2,0.5)=NaN, pow(0,-1)=inf, pow(2,inf) no trap).
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
    let src_path = dir.join("explog.almd");
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
    let src_path = dir.join("explog.almd");
    let wasm_path = dir.join("explog.wasm");
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

/// Battery over the four unary fns. Each finite point prints exp/log/log2/log10.
fn build_unary_program(points: &[f64]) -> String {
    let mut body = String::new();
    body.push_str("import math\n");
    body.push_str("fn pe(x: Float) -> Unit = println(float.to_string(math.exp(x)))\n");
    body.push_str("fn pl(x: Float) -> Unit = println(float.to_string(math.log(x)))\n");
    body.push_str("fn p2(x: Float) -> Unit = println(float.to_string(math.log2(x)))\n");
    body.push_str("fn pt(x: Float) -> Unit = println(float.to_string(math.log10(x)))\n");
    body.push_str("fn main() -> Unit = {\n");
    for &x in points {
        if !x.is_finite() {
            continue;
        }
        let l = lit(x);
        body.push_str(&format!("  let x = {}\n", l));
        body.push_str("  pe(x)\n  pl(x)\n  p2(x)\n  pt(x)\n");
    }
    body.push_str("}\n");
    body
}

/// Battery over `b ** e`. Non-finite operands are constructed without literals.
fn build_pow_program(pairs: &[(f64, f64)]) -> String {
    let mut body = String::new();
    body.push_str("import math\n");
    body.push_str("fn pp(b: Float, e: Float) -> Unit = println(float.to_string(b ** e))\n");
    body.push_str("fn main() -> Unit = {\n");
    for &(b, e) in pairs {
        if !b.is_finite() || !e.is_finite() {
            continue;
        }
        body.push_str(&format!("  pp({}, {})\n", lit(b), lit(e)));
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

fn assert_unary(points: &[f64]) {
    let dir = tempfile::tempdir().unwrap();
    let prog = build_unary_program(points);
    let native = run_native(&prog, dir.path());
    let wasm = run_wasm(&prog, dir.path());
    assert_eq_localized(&native, &wasm, "exp/log/log2/log10");
}

fn assert_pow(pairs: &[(f64, f64)]) {
    let dir = tempfile::tempdir().unwrap();
    let prog = build_pow_program(pairs);
    let native = run_native(&prog, dir.path());
    let wasm = run_wasm(&prog, dir.path());
    assert_eq_localized(&native, &wasm, "pow");
}

fn assert_eq_localized(native: &str, wasm: &str, what: &str) {
    if native == wasm {
        return;
    }
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
        "{what} cross-target mismatch ({} native lines, {} wasm lines){}",
        nl.len(),
        wl.len(),
        first
    );
}

fn unary_boundary() -> Vec<f64> {
    vec![
        // exp range edges (overflow ~709.78, underflow ~-745.13 -> subnormal)
        0.0, 1.0, -1.0, 0.5, 2.5, 10.0, -10.0,
        709.0, 709.7827, 709.782712893383973096, 709.79, 710.0,
        -700.0, -745.0, -745.13321910194110842, -745.2, -746.0,
        1e-30, -1e-30, 1e-300, 1e300,
        // log domain: 0 -> -inf, negatives -> NaN, 1 -> 0, powers
        2.718281828459045, 1000000.0, 123.456,
        2.0, 4.0, 8.0, 1024.0, 100.0, 1000.0,
        // subnormal inputs (log scale-up path)
        5e-324, f64::MIN_POSITIVE, 2.2250738585072014e-308,
        f64::MAX, 1.5, 0.25, 0.75,
    ]
}

fn pow_boundary() -> Vec<(f64, f64)> {
    vec![
        // integer exponents
        (2.0, 10.0), (2.0, 0.0), (2.0, 1.0), (2.0, -1.0), (2.0, -3.0),
        (3.0, 4.0), (10.0, 3.0), (10.0, -3.0),
        // negative base + integer exponent parity
        (-2.0, 2.0), (-2.0, 3.0), (-2.0, 4.0), (-1.0, 9.0), (-1.0, 10.0),
        // negative base + non-integer -> NaN
        (-2.0, 0.5), (-1.0, 2.2), (-1.0, -1.14),
        // fractional exponents
        (2.0, 0.5), (2.0, 1.0 / 3.0), (3.0, 1.0 / 3.0), (9.0, 0.5),
        (0.5, 3.0), (100.0, 0.5), (std::f64::consts::E, 2.0),
        // zero / one
        (0.0, 2.0), (0.0, -1.0), (1.0, 100.0), (-0.0, 3.0), (-0.0, -3.0),
        // huge magnitudes (overflow/underflow + near-1 base)
        (10.0, 308.0), (10.0, 309.0), (10.0, -309.0),
        (1.0000001, 1e8), (0.9999999, 1e8), (2.0, 1024.0), (2.0, -1075.0),
    ]
}

#[test]
fn explog_cross_target_boundary() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    assert_unary(&unary_boundary());
}

#[test]
fn pow_cross_target_boundary() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    assert_pow(&pow_boundary());
}

#[test]
fn explog_cross_target_random_fuzz() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    // Batches kept modest so each generated program stays small for the toolchain.
    let mut rng = Rng(0x1234_5678_9ABC_DEF0);
    let total = 1200;
    let batch = 150;
    let mut done = 0;
    while done < total {
        let n = batch.min(total - done);
        let pts = random_finite(&mut rng, n);
        assert_unary(&pts);
        done += n;
    }
}

#[test]
fn pow_cross_target_random_fuzz() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    let mut rng = Rng(0x0FED_CBA9_8765_4321);
    let total = 1200;
    let batch = 150;
    let mut done = 0;
    while done < total {
        let n = batch.min(total - done);
        let a = random_finite(&mut rng, n);
        let b = random_finite(&mut rng, n);
        let pairs: Vec<(f64, f64)> = a.into_iter().zip(b).collect();
        assert_pow(&pairs);
        done += n;
    }
}
