//! #4196 (bigint half): `bigint.parse` / `bigint.to_string` cost time quadratic
//! in the digit count (320,000 decimal digits: 18 s native, 19 s wasm; 160,000
//! hex digits parsed and printed back: over 60 s), and `bigint.pow(b, huge)`
//! ran until the CPU limit.
//!
//! The fix, gated here:
//! - a power-of-two radix (2, 4, 8, 16, 32) moves bits straight between the
//!   digits and the limbs, in linear time and with no limit;
//! - any other radix keeps the quadratic conversion and is limited to 4300
//!   digits (Python's default limit, CVE-2020-10735): `parse_radix` answers
//!   `err("integer string conversion exceeds the limit of 4300 digits")` after
//!   one linear scan, and `to_string_radix` aborts with the same text, refusing
//!   a magnitude of 2^(4300 * bit_length(radix)) or more before any division;
//! - `pow` refuses (aborts with `pow result too large`) a result sure to pass
//!   30 * 2^28 bits, the 2^28-limb list ceiling `bshl` meets, before computing.
//!
//! The scaling is pinned by a DOUBLING RATIO: the cost at 2n over the cost at
//! n is about 2 for a linear conversion and about 4 for a quadratic one. On
//! the stock wasm artifact the cost is wasmtime FUEL (one unit per wasm
//! instruction), found by bisection, so the verdict does not move with the
//! machine's load; natively it is the best of three wall-clock runs. Every run
//! has a budget (fuel cap / timeout) a quadratic conversion exceeds many times
//! over, so a regression fails instead of hanging.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SOURCE: &str = r#"import bigint
import env

effect fn main() -> Unit = {
  let args = env.args()
  let n = int.parse(list.last(args) ?? "0") ?? 0
  match list.get(args, list.len(args) - 2) ?? "" {
    "hex" => {
      let b = bigint.parse_radix(string.repeat("f7", n / 2), 16) ?? bigint.from(0)
      let t = bigint.to_string_radix(b, 16)
      println("${string.len(t)} ${string.slice(t, 0, 4)} ${(bigint.parse_radix(t, 16) ?? bigint.from(0)) == b}")
    },
    "bin" => {
      let b = bigint.parse_radix(string.repeat("10", n / 2), 2) ?? bigint.from(0)
      println("${string.len(bigint.to_string_radix(b, 2))} ${bigint.bit_length(b)}")
    },
    "dec" => match bigint.parse(string.repeat("7", n)) {
      ok(b) => println(int.to_string(bigint.bit_length(b))),
      err(e) => println(e),
    },
    "decout" => println(bigint.to_string(bigint.bshl(bigint.from(1), n))),
    "pow" => println(int.to_string(bigint.bit_length(bigint.pow(bigint.from(7), n)))),
    _ => println("usage"),
  }
}
"#;

const LIMIT_TEXT: &str = "integer string conversion exceeds the limit of 4300 digits";

fn expected(mode: &str, n: u64) -> String {
    match mode {
        "hex" => format!("{n} f7f7 true\n"),
        "bin" => format!("{n} {n}\n"),
        "dec" => format!("{LIMIT_TEXT}\n"),
        _ => unreachable!(),
    }
}

/// The scratch dir holding the program, its native binary and its stock wasm.
fn build(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("almide-bigint-linear-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("lin.almd"), SOURCE).expect("write");
    for args in [
        &["build", "lin.almd", "-o", "lin"][..],
        &["build", "lin.almd", "--target", "wasm", "-o", "lin.wasm"][..],
    ] {
        let o = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("almide");
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    dir
}

/// Runs `cmd` to completion within `limit`, killing it past that.
fn run_within(mut cmd: Command, limit: Duration) -> Option<(Duration, i32, String, String)> {
    let start = Instant::now();
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    loop {
        if child.try_wait().expect("wait").is_some() {
            let took = start.elapsed();
            let o = child.wait_with_output().expect("output");
            return Some((
                took,
                o.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&o.stdout).into_owned(),
                String::from_utf8_lossy(&o.stderr).into_owned(),
            ));
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Best of three native runs; panics on a wrong answer or a run past 20 s.
fn native_time(dir: &Path, mode: &str, n: u64) -> Duration {
    (0..3)
        .map(|_| {
            let mut cmd = Command::new(dir.join("lin"));
            cmd.args([mode, &n.to_string()]);
            let (took, code, out, err) = run_within(cmd, Duration::from_secs(20))
                .unwrap_or_else(|| panic!("native {mode} {n} ran past 20 s — quadratic again?"));
            assert_eq!(
                (code, out.as_str()),
                (0, expected(mode, n).as_str()),
                "native {mode} {n}: {err}"
            );
            took
        })
        .min()
        .unwrap()
}

fn has_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok();
    if !ok {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH"
        );
        eprintln!("skipping the wasm leg: wasmtime is not on PATH");
    }
    ok
}

/// One stock run under `fuel`: `None` when the fuel ran out.
fn fueled(dir: &Path, mode: &str, n: u64, fuel: u64) -> Option<(i32, String, String)> {
    let o = Command::new("wasmtime")
        .args([
            "run",
            "-W",
            &format!("fuel={fuel}"),
            "lin.wasm",
            mode,
            &n.to_string(),
        ])
        .current_dir(dir)
        .output()
        .expect("wasmtime");
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    if !o.status.success() && err.contains("fuel") {
        return None;
    }
    Some((
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        err,
    ))
}

/// The fuel of the 2n run of the hex mode is about 350 million; a quadratic
/// conversion of the same digits needs tens of billions.
const FUEL_CAP: u64 = 2_000_000_000;

/// The instructions one stock run needs, to within 1/64, by bisection.
fn fuel_needed(dir: &Path, mode: &str, n: u64) -> u64 {
    let (code, out, err) = fueled(dir, mode, n, FUEL_CAP).unwrap_or_else(|| {
        panic!("wasm {mode} {n} needs more than {FUEL_CAP} instructions — quadratic again?")
    });
    assert_eq!(
        (code, out.as_str()),
        (0, expected(mode, n).as_str()),
        "wasm {mode} {n}: {err}"
    );
    let (mut lo, mut hi) = (0, FUEL_CAP);
    while hi - lo > hi / 64 {
        let mid = lo + (hi - lo) / 2;
        if fueled(dir, mode, n, mid).is_some() {
            hi = mid
        } else {
            lo = mid
        }
    }
    hi
}

#[test]
fn power_of_two_text_and_the_decimal_refusal_scale_linearly() {
    let dir = build("ratio");
    for mode in ["hex", "bin", "dec"] {
        let (a, b) = (
            native_time(&dir, mode, 1_000_000),
            native_time(&dir, mode, 2_000_000),
        );
        let ratio = b.as_secs_f64() / a.as_secs_f64().max(1e-3);
        assert!(
            ratio < 3.0,
            "native {mode}: 2M digits took {b:?}, 1M took {a:?} (ratio {ratio:.2}) — quadratic again?"
        );
    }
    if has_wasmtime() {
        for mode in ["hex", "bin", "dec"] {
            let (a, b) = (
                fuel_needed(&dir, mode, 100_000),
                fuel_needed(&dir, mode, 200_000),
            );
            let ratio = b as f64 / a as f64;
            assert!(
                ratio < 2.6,
                "wasm {mode}: {b} instructions at 200k digits, {a} at 100k (ratio {ratio:.2}) — quadratic again?"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The refusals come before the work: printing a 4,000,001-bit value in
/// decimal, or raising 7 to i64::MAX, aborts at once on both legs.
#[test]
fn decimal_print_and_pow_refuse_before_computing() {
    let dir = build("refuse");
    let cases = [
        ("decout", 4_000_000u64, LIMIT_TEXT),
        ("pow", i64::MAX as u64, "pow result too large"),
    ];
    for (mode, n, text) in cases {
        let mut cmd = Command::new(dir.join("lin"));
        cmd.args([mode, &n.to_string()]);
        let (_, code, out, err) = run_within(cmd, Duration::from_secs(20))
            .unwrap_or_else(|| panic!("native {mode} ran past 20 s"));
        assert!(
            code == 1 && out.is_empty() && err.contains(text),
            "native {mode}: exit {code}, {out:.80} / {err:.200}"
        );
        if has_wasmtime() {
            let (code, out, err) = fueled(&dir, mode, n, 200_000_000)
                .unwrap_or_else(|| panic!("wasm {mode} needs more than 200M instructions"));
            assert!(
                code == 1 && out.is_empty() && err.contains(text),
                "wasm {mode}: exit {code}, {out:.80} / {err:.200}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
