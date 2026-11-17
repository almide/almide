//! Growth gates for the stdlib costs the 2026-11 re-inspection found above
//! their declared order (`proofs/stdlib-inspection.toml`). Each mode of the
//! program below runs one fixed function at a size `n` taken from the command
//! line, and the test pins its DOUBLING RATIO: the cost at 2n over the cost at
//! n is about 2 for linear work and about 4 for quadratic work.
//!
//! Natively the cost is the best of five runs of the child's own CPU time
//! (wait4's rusage), n and 2n interleaved so a burst of load lands on both.
//! On the stock wasm artifact it is wasmtime FUEL (one unit per wasm
//! instruction), found by bisection, so the verdict does not move with the
//! machine's load. Every run has a budget (timeout / fuel cap) the quadratic
//! shape exceeds many times over, so a regression fails instead of hanging.
//!
//! The modes, one per fix:
//! - `parse_radix` (#4765): `int.parse_radix` walked its digits with a
//!   `[b, ..rest]` match, copying the rest per digit; the fixed-width
//!   `parse_radix` fns delegate to it.
//! - `heap` (#4777): `heap.push` took `var slots = h.slots`, a second
//!   reference, so its first swap copied the whole slot list on every push
//!   (and `heap.from_list`, n pushes, was quadratic). The sifts now swap in
//!   the heap's own list; `from_list` heapifies bottom-up. n log n overall.
//! - `bigdiv` (#4783): dividing by a one-limb BigInt prepended each quotient
//!   limb, copying the quotient per limb (`bigint.floor_div` / `modulo` and
//!   every division that takes the single-limb path).
//! - `bytes_join` (#4785): wasm links the self-hosted `bytes.__join` for a
//!   `list.join` over `List[Bytes]`; it folded `__concat`, copying the
//!   accumulator per element. It now sizes the result once and fills it.
//! - `concat_rows` (#4770), `zip_with` (#4786), `f16` (#4775): the matrix
//!   self-hosts (and native concat_rows) built their result by appending to
//!   an accumulator that each step copied; one pass each now.
//! - `deque` (#4773): `deque.drop`, `drop_end` and `set` round-tripped
//!   through a List, so a sole owner's rebind copied the whole deque.
//! - `deque_pop` (#4778): a value-returning `list.pop` straight on a field of
//!   the `mut` deque copied that list per pop on wasm.
//! - `regex_replace` (#4779): the wasm self-host appended each match to a
//!   String accumulator, copying the result per match.
//! - `value_get` (#4757): wasm's `$vfield` scanned an Object's pairs on every
//!   `value.get` / `get_*` read, so reading each of n keys once was
//!   quadratic; it now asks the box's key index, as native reads its own.
#[path = "support/cpu_time.rs"]
mod cpu_time;
use cpu_time::reap_cpu_time;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SOURCE: &str = r#"import bigint
import env
import regex

effect fn main() -> Unit = {
  let args = env.args()
  let n = int.parse(list.last(args) ?? "0") ?? 0
  match list.get(args, list.len(args) - 2) ?? "" {
    "parse_radix" => {
      let digits = string.repeat("0", n) + "7f"
      let sized = match uint16.parse_radix(digits, 16) {
        ok(v) => uint16.to_string(v),
        err(e) => e,
      }
      println("${int.parse_radix(digits, 16) ?? 0} ${sized}")
    },
    "heap" => {
      var h = heap.from_list(list.range(0, n) |> list.map((i) => (i * 7919) % 100003))
      for i in 0..<n {
        heap.push(h, (i * 104729) % 100003)
      }
      var acc = 0
      for _ in 0..<(2 * n) {
        acc = (acc + (heap.pop(h) ?? 0)) % 1000003
      }
      println("${acc} ${heap.len(h)}")
    },
    "bigdiv" => {
      let x = bigint.bshl(bigint.from(1), n)
      let d = bigint.from(1234567)
      println("${bigint.bit_length(bigint.floor_div(x, d))} ${bigint.to_string(bigint.modulo(x, d))}")
    },
    "bytes_join" => {
      let parts = list.range(0, n) |> list.map((i) => bytes.from_list([i % 256, 7]))
      let j = list.join(parts, bytes.from_list([0]))
      println("${bytes.len(j)} ${bytes.get(j, bytes.len(j) - 1) ?? -1}")
    },
    "concat_rows" => {
      let ms = list.range(0, n) |> list.map((i) => matrix.from_lists([[1.5, 2.0], [0.5, 3.0]]))
      println("${matrix.rows(matrix.concat_rows(ms))}")
    },
    "zip_with" => {
      let a = matrix.zeros(n, 2)
      let c = matrix.zip_with(a, matrix.zeros(n, 2), (x, y) => x + y + 1.0)
      println("${matrix.rows(c)} ${matrix.get(c, n - 1, 1) == some(1.0)}")
    },
    "f16" => println("${bytes.len(matrix.to_bytes_f16_le(matrix.zeros(n, 2)))}"),
    "deque" => {
      var d = deque.from_list(list.range(0, n))
      deque.push_front(d, -1)
      for k in 0..<(n / 4) {
        d = deque.drop(d, 1)
        d = deque.drop_end(d, 1)
        d = deque.set(d, 0, k)
      }
      println("${deque.len(d)} ${deque.first(d) ?? -1}")
    },
    "deque_pop" => {
      var d = deque.from_list(list.range(0, n))
      var acc = 0
      for _ in 0..<(n / 4) {
        acc = acc + (deque.pop(d) ?? 0) - (deque.pop_front(d) ?? 0)
      }
      println("${acc} ${deque.len(d)}")
    },
    "regex_replace" => {
      let r = regex.replace(regex.new("(é)"), string.repeat("aé", n), "x")
      println("${string.len(r)} ${string.slice(r, 0, 4)}")
    },
    "value_get" => {
      let v = value.object(list.range(0, n) |> list.map((i) => ("k${i}", value.int(i))))
      let hits = list.range(0, n) |> list.fold(0, (acc, i) => acc + (value.get_int(v, "k${i}") ?? 0) + (if option.is_some(value.get(v, "k${n - 1 - i}")) then 1 else 0))
      println("${hits} ${option.is_none(value.get(v, "absent"))}")
    },
    _ => println("usage"),
  }
}
"#;

/// One mode: its sizes on each leg and the line it must print at `n`.
struct Case {
    mode: &'static str,
    native_n: u64,
    wasm_n: u64,
    expected: fn(u64) -> String,
}

const CASES: &[Case] = &[
    Case { mode: "parse_radix", native_n: 2_000_000, wasm_n: 100_000, expected: |_| "127 127\n".to_string() },
    Case { mode: "heap", native_n: 200_000, wasm_n: 5_000, expected: heap_expected },
    Case { mode: "bytes_join", native_n: 2_000_000, wasm_n: 100_000, expected: |n| format!("{} 7\n", 3 * n - 1) },
    Case { mode: "concat_rows", native_n: 400_000, wasm_n: 10_000, expected: |n| format!("{}\n", 2 * n) },
    Case { mode: "zip_with", native_n: 2_000_000, wasm_n: 20_000, expected: |n| format!("{n} true\n") },
    Case { mode: "f16", native_n: 2_000_000, wasm_n: 20_000, expected: |n| format!("{}\n", 4 * n) },
    Case { mode: "deque", native_n: 400_000, wasm_n: 40_000, expected: |n| format!("{} {}\n", n + 1 - 2 * (n / 4), n / 4 - 1) },
    Case { mode: "deque_pop", native_n: 2_000_000, wasm_n: 100_000, expected: |n| format!("{} {}\n", (n / 4) * (n - n / 4), n - 2 * (n / 4)) },
    Case { mode: "regex_replace", native_n: 1_000_000, wasm_n: 20_000, expected: |n| format!("{} axax\n", 2 * n) },
    Case { mode: "value_get", native_n: 400_000, wasm_n: 20_000, expected: |n| format!("{} true\n", n * (n - 1) / 2 + n) },
    Case { mode: "bigdiv", native_n: 32_000_000, wasm_n: 200_000, expected: bigdiv_expected },
];

/// The `heap` mode's line: every pushed value popped in ascending order.
fn heap_expected(n: u64) -> String {
    let mut all: Vec<u64> = (0..n).map(|i| (i * 7919) % 100003).chain((0..n).map(|i| (i * 104729) % 100003)).collect();
    all.sort_unstable();
    let acc = all.iter().fold(0, |acc, v| (acc + v) % 1000003);
    format!("{acc} 0\n")
}

/// The `bigdiv` mode's line: 2^n / 1234567 has n - 20 bits (the divisor is
/// between 2^20 and 2^21), and 2^n mod 1234567.
fn bigdiv_expected(n: u64) -> String {
    let rem = (0..n).fold(1u64, |r, _| r * 2 % 1234567);
    format!("{} {rem}\n", n - 20)
}

/// The scratch dir holding the program, its native binary and its stock wasm.
fn build(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-reinspect-linear-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("lin.almd"), SOURCE).expect("write");
    for args in [
        &["build", "lin.almd", "-o", "lin"][..],
        &["build", "lin.almd", "--target", "wasm", "-o", "lin.wasm"][..],
    ] {
        let o = Command::new(env!("CARGO_BIN_EXE_almide")).args(args).current_dir(&dir).output().expect("almide");
        assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
    }
    dir
}

/// Runs `cmd` to completion within `limit`, killing it past that: its CPU
/// time (`reap_cpu_time`), exit code, stdout and stderr.
fn run_within(mut cmd: Command, limit: Duration) -> Option<(Duration, i32, String, String)> {
    let start = Instant::now();
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn");
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = r.read_to_end(&mut b);
            String::from_utf8_lossy(&b).into_owned()
        })
    };
    let out = drain(Box::new(child.stdout.take().expect("piped")));
    let err = drain(Box::new(child.stderr.take().expect("piped")));
    let (code, took) = reap_cpu_time(child, start, limit)?;
    Some((took, code, out.join().expect("stdout"), err.join().expect("stderr")))
}

/// One native run's CPU time; panics on a wrong answer or a run past 30 s.
fn native_time(dir: &Path, case: &Case, n: u64) -> Duration {
    let mut cmd = Command::new(dir.join("lin"));
    cmd.args([case.mode, &n.to_string()]);
    let (took, code, out, err) = run_within(cmd, Duration::from_secs(30))
        .unwrap_or_else(|| panic!("native {} {n} ran past 30 s — quadratic again?", case.mode));
    assert_eq!((code, out.as_str()), (0, (case.expected)(n).as_str()), "native {} {n}: {err}", case.mode);
    took
}

fn has_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok();
    if !ok {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        eprintln!("skipping the wasm leg: wasmtime is not on PATH");
    }
    ok
}

/// One stock run under `fuel`: `None` when the fuel ran out.
fn fueled(dir: &Path, case: &Case, n: u64, fuel: u64) -> Option<(i32, String, String)> {
    let o = Command::new("wasmtime")
        .args(["run", "-W", &format!("fuel={fuel}"), "lin.wasm", case.mode, &n.to_string()])
        .current_dir(dir)
        .output()
        .expect("wasmtime");
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    if !o.status.success() && err.contains("fuel") {
        return None;
    }
    Some((o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned(), err))
}

/// Every 2n run needs well under this; the quadratic shapes need many times it.
const FUEL_CAP: u64 = 4_000_000_000;

/// The instructions one stock run needs, to within 1/64, by bisection.
fn fuel_needed(dir: &Path, case: &Case, n: u64) -> u64 {
    let (code, out, err) = fueled(dir, case, n, FUEL_CAP)
        .unwrap_or_else(|| panic!("wasm {} {n} needs more than {FUEL_CAP} instructions — quadratic again?", case.mode));
    assert_eq!((code, out.as_str()), (0, (case.expected)(n).as_str()), "wasm {} {n}: {err}", case.mode);
    let (mut lo, mut hi) = (0, FUEL_CAP);
    while hi - lo > hi / 64 {
        let mid = lo + (hi - lo) / 2;
        if fueled(dir, case, n, mid).is_some() {
            hi = mid
        } else {
            lo = mid
        }
    }
    hi
}

#[test]
fn the_reinspected_stdlib_fns_scale_linearly() {
    let dir = build("ratio");
    for case in CASES {
        let n = case.native_n;
        let (a, b) = (0..5).fold((Duration::MAX, Duration::MAX), |(a, b), _| {
            (a.min(native_time(&dir, case, n)), b.min(native_time(&dir, case, 2 * n)))
        });
        let ratio = b.as_secs_f64() / a.as_secs_f64().max(1e-3);
        eprintln!("native {}: {b:?} of CPU at 2n, {a:?} at n = {n} (ratio {ratio:.2})", case.mode);
        assert!(ratio < 3.0, "native {}: 2n took {b:?} of CPU, n = {n} took {a:?} (ratio {ratio:.2}) — quadratic again?", case.mode);
    }
    if has_wasmtime() {
        for case in CASES {
            let n = case.wasm_n;
            let (a, b) = (fuel_needed(&dir, case, n), fuel_needed(&dir, case, 2 * n));
            let ratio = b as f64 / a as f64;
            eprintln!("wasm {}: {b} instructions at 2n, {a} at n = {n} (ratio {ratio:.2})", case.mode);
            assert!(ratio < 2.6, "wasm {}: {b} instructions at 2n, {a} at n = {n} (ratio {ratio:.2}) — quadratic again?", case.mode);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
