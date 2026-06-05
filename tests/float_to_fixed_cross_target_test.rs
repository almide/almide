//! Lock for `float.to_fixed` (WASM Dragon4 fixed-precision formatter).
//!
//! Drain PR-C Phase B: the wasm `__float_to_fixed` (rt_dragon.rs) reproduces the
//! native oracle `format!("{:.N}", x)` byte-for-byte — exact binary value, round-
//! half-to-EVEN, no 10^N i64 overflow. Two locks:
//!   1. CROSS-TARGET: print to_fixed of many (value, N) on native AND wasm and
//!      assert byte-identical stdout.
//!   2. NATIVE-vs-ORACLE: the native output must equal Rust `format!("{:.N}")`
//!      for the same inputs (the cross-target lock then carries it to wasm).
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
    let src_path = dir.join("tofixed.almd");
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
    String::from_utf8_lossy(&output.stdout).trim_end().to_string()
}

fn run_wasm(source: &str, dir: &Path) -> String {
    let src_path = dir.join("tofixed.almd");
    let wasm_path = dir.join("tofixed.wasm");
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
    assert!(
        output.status.success(),
        "WASM run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim_end().to_string()
}

fn lit(x: f64) -> String {
    debug_assert!(x.is_finite());
    let s = format!("{:?}", x);
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

/// Build a program that prints to_fixed(value, n) for each finite case.
fn build_program(cases: &[(f64, usize)]) -> String {
    let mut body = String::new();
    body.push_str("fn p(v: Float, n: Int) -> Unit = println(float.to_fixed(v, n))\n");
    body.push_str("fn main() -> Unit = {\n");
    for &(x, n) in cases {
        if !x.is_finite() {
            continue;
        }
        body.push_str(&format!("  p({}, {})\n", lit(x), n));
    }
    body.push_str("}\n");
    body
}

/// The Rust oracle: `format!("{:.N}", x)` per case (one line each).
fn oracle(cases: &[(f64, usize)]) -> String {
    let mut out = String::new();
    for &(x, n) in cases {
        if !x.is_finite() {
            continue;
        }
        out.push_str(&format!("{:.*}\n", n, x));
    }
    out.trim_end().to_string()
}

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

fn boundary_cases() -> Vec<(f64, usize)> {
    vec![
        // round-half-to-even on the exact binary value
        (0.5, 0), (1.5, 0), (2.5, 0), (3.5, 0), (255.5, 0), (256.5, 0), (99.5, 0),
        // exact truncation/rounding
        (0.125, 2), (0.125, 3), (0.0625, 3), (0.0625, 4),
        (2.675, 2), (1.005, 2), (0.045, 2), (0.35, 1), (0.05, 1),
        (9.995, 2), (0.999999, 2),
        // trailing zeros + plain
        (8.0, 3), (0.0, 2), (-0.0, 2), (2.0, 0), (123.456, 2), (123.456, 0), (-123.456, 2),
        // huge magnitudes
        (1e20, 0), (1e20, 3), (123456789.98765433, 5), (1e100, 0), (1e300, 2),
        // large N: exact binary expansion (old impl trapped at N>=19)
        (0.1, 17), (0.1, 19), (0.1, 20), (0.1, 25), (0.1, 40), (1.0 / 3.0, 25),
        // tiny values -> zeros (and the round-up edge near 0.5*10^-N)
        (0.0001, 5), (0.000001, 5), (1e-10, 5), (4.9e-3, 2), (5.1e-3, 2),
        // sign on zero-magnitude
        (-0.4, 0), (-0.05, 1),
    ]
}

#[test]
fn to_fixed_native_matches_format_macro() {
    if !tools_available() {
        eprintln!("skipping: almide unavailable");
        return;
    }
    let cases = boundary_cases();
    let dir = tempfile::tempdir().unwrap();
    let native = run_native(&build_program(&cases), dir.path());
    assert_eq!(
        native,
        oracle(&cases),
        "native float.to_fixed diverged from Rust format!(\"{{:.N}}\")"
    );
}

#[test]
fn to_fixed_cross_target_boundary() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    let cases = boundary_cases();
    let dir = tempfile::tempdir().unwrap();
    let native = run_native(&build_program(&cases), dir.path());
    let wasm = run_wasm(&build_program(&cases), dir.path());
    assert_eq!(native, wasm, "to_fixed cross-target mismatch on boundary cases");
    // and both equal the oracle
    assert_eq!(native, oracle(&cases), "to_fixed native != Rust format! oracle");
}

#[test]
fn to_fixed_cross_target_random_fuzz() {
    if !tools_available() {
        eprintln!("skipping: almide or wasm runtime unavailable");
        return;
    }
    let mut rng = Rng(0xABAD_1DEA_C0DE_F00D);
    let ns = [0usize, 0, 1, 2, 3, 5, 8, 15, 20, 30];
    let total = 1000;
    let batch = 150;
    let mut done = 0;
    while done < total {
        let n = batch.min(total - done);
        let mut cases = Vec::with_capacity(n);
        while cases.len() < n {
            let x = f64::from_bits(rng.next());
            if !x.is_finite() {
                continue;
            }
            let nd = ns[(rng.next() as usize) % ns.len()];
            cases.push((x, nd));
        }
        let dir = tempfile::tempdir().unwrap();
        let native = run_native(&build_program(&cases), dir.path());
        let wasm = run_wasm(&build_program(&cases), dir.path());
        // Localize the first diff if any.
        if native != wasm || native != oracle(&cases) {
            let nl: Vec<&str> = native.lines().collect();
            let wl: Vec<&str> = wasm.lines().collect();
            let ol = oracle(&cases);
            let ol: Vec<&str> = ol.lines().collect();
            for i in 0..nl.len() {
                let a = nl.get(i).copied().unwrap_or("<none>");
                let b = wl.get(i).copied().unwrap_or("<none>");
                let c = ol.get(i).copied().unwrap_or("<none>");
                if a != b || a != c {
                    panic!(
                        "to_fixed fuzz mismatch for case {:?}: native={:?} wasm={:?} oracle={:?}",
                        cases[i], a, b, c
                    );
                }
            }
            panic!("to_fixed fuzz length mismatch");
        }
        done += n;
    }
}
