//! #3644 — `matrix.fma` / `matrix.fma3` cost the OVERLAP of their operands,
//! not the largest operand, on both legs.
//!
//! Native zips the operands (`a.iter().zip(b.iter())`, twice), so a call
//! touches only the cells the result has. The wasm leg linked the composition
//! `add(scale(a, ka), scale(b, kb))` (stdlib/matrix_fused.almd), and `scale`
//! walks and allocates a whole copy of each operand before `add` truncates to
//! the overlap: `fma(1×1, n×n)` cost O(n²) there. 200 calls took 0.08 s at
//! n = 1000 and 0.33 s at n = 2000 on wasm, against a flat ~0.02 s natively.
//!
//! The values are pinned by spec/wasm_cross/matrix_composition_family.almd
//! (the overlap shapes, the unfused rounding, NaN / ±inf / -0.0 as bits);
//! this file pins the cost. The program times only the fma loop (it reads the
//! monotonic clock around it) with a 1×1 operand against an n×n one, at n and
//! 2n: a whole-operand pass takes about 4× per call, an overlap pass about
//! 1×; the bound 2.0 sits between them.
//!
//! Every subprocess runs under a wall-clock cap. The cell is release-only (a
//! debug compiler's embedded wasm engine would time itself); CI's
//! release-only root step names this file.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCALING: &str = r#"import env

effect fn run(n: Int, d: Int) -> Unit = {
  let one = matrix.from_lists([[1.5]])
  let big = matrix.ones(n, n)
  let t0 = datetime.monotonic_ns()!
  var acc = 0.0
  for i in 0..<d {
    let r = matrix.fma(one, 2.0, big, 3.0)
    let r3 = matrix.fma3(big, 1.0, one, 1.0, big, 0.5)
    acc = acc + matrix.get(r, 0, 0) + matrix.get(r3, 0, 0)
  }
  let t1 = datetime.monotonic_ns()!
  println("${acc} ${t1 - t0}")
}

effect fn main() -> Unit = {
  let args = env.args()
  let n = int.parse(list.get(args, 0) ?? "0") ?? 0
  let d = int.parse(list.get(args, 1) ?? "0") ?? 0
  run(n, d)!
}
"#;

/// Wall-clock cap on one subprocess (a compile included).
const PER_RUN_LIMIT: Duration = Duration::from_secs(180);

/// `almide run [--target wasm] --release <file> -- <args>` under the cap.
fn run_capped(file: &Path, wasm: bool, args: &[&str]) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg("--release").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    if !args.is_empty() {
        c.arg("--").args(args);
    }
    let mut child = c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn almide run");
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        so.read_to_string(&mut s).ok();
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        se.read_to_string(&mut s).ok();
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break Some(st);
        }
        if start.elapsed() > PER_RUN_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (out, err) = (to.join().unwrap(), te.join().unwrap());
    let leg = if wasm { "wasm" } else { "native" };
    let Some(status) = status else {
        panic!("{leg} {args:?}: still running after {PER_RUN_LIMIT:?} — killed (a whole-operand fma again?)");
    };
    (status.code().unwrap_or(-1), out, err)
}

/// Nanoseconds per loop round at size `n`: the best of three runs of the
/// program's own clock around the loop.
fn ns_per_round(file: &Path, wasm: bool, n: u64, d: u64) -> f64 {
    let (n_s, d_s) = (n.to_string(), d.to_string());
    // fma(1.5*2 + 1*3) = 6 and fma3(1 + 1.5 + 0.5) = 3 per round
    let want = format!("{}", 9 * d);
    (0..3)
        .map(|_| {
            let (code, out, err) = run_capped(file, wasm, &[&n_s, &d_s]);
            assert_eq!(code, 0, "wasm={wasm} n={n}: exit {code}\n{err}");
            let fields: Vec<&str> = out.split_whitespace().collect();
            assert_eq!(fields.len(), 2, "wasm={wasm} n={n}: unexpected output {out:?}");
            assert_eq!(fields[0], want, "wasm={wasm} n={n}: the fma cells moved");
            fields[1].parse::<f64>().expect("elapsed ns") / d as f64
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "times the fma loop: runs in CI's release-only root step")]
fn a_larger_operand_outside_the_overlap_costs_nothing() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("matrix_fma_scaling.almd");
    std::fs::write(&file, SCALING).expect("write source");
    let (n, d) = (300u64, 2_000u64);
    for wasm in [false, true] {
        let small = ns_per_round(&file, wasm, n, d);
        let large = ns_per_round(&file, wasm, 2 * n, d);
        let ratio = large / small.max(1.0);
        assert!(
            ratio < 2.0,
            "wasm={wasm}: an fma round took {large:.0} ns against a {}x{} operand and {small:.0} ns against {n}x{n} — ratio {ratio:.1}, the whole operand is walked again",
            2 * n,
            2 * n
        );
    }
}
