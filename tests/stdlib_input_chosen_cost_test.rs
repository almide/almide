//! Two stdlib functions whose running time an input chose.
//!
//! - #4211: `path.normalize` built its segment stack with `[seg] + acc` and
//!   `list.drop(acc, 1)`, a full copy per segment, so it was quadratic in the
//!   segment count on both targets: 160,000 `a/` plus 160,000 `../` took 4.1 s
//!   native and 28 s wasm. It now pushes and pops at the end of a `var` list.
//! - #4212: `math.factorial(n)` multiplied n times, so an input-chosen n of
//!   10^18 ran for centuries, although every n >= 66 wraps to 0 (v2(66!) = 64).
//!   It now answers 0 at once from 66 on.
//!
//! The wasm leg is pinned by wasmtime's FUEL (one unit per wasm instruction),
//! not by a clock. `path.normalize` runs at n and 2n with the 2n budget twice
//! the n budget, so a super-linear body fails the larger cell first; each
//! budget is about three times what the linear body spends (measured 212M at
//! 40,000 and 425M at 80,000). `math.factorial` gets one fixed budget for
//! arguments up to i64::MAX: no loop over n fits it. The native leg, which has
//! no fuel, gets a wall-clock bound (the probe is killed past it, so a body
//! that loops over n fails instead of hanging the suite). Every wasm answer is
//! differed against native, and every native answer against the expected text.
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn has_wasmtime() -> bool {
    let ok = Command::new("wasmtime").arg("--version").output().is_ok();
    if !ok {
        assert!(
            std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(),
            "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH"
        );
        eprintln!("skipping: wasmtime is not on PATH");
    }
    ok
}

fn normalize_src(n: usize) -> String {
    format!(
        r#"import path
fn main() -> Unit = {{
  println(path.normalize(string.repeat("a/", {n}) + string.repeat("../", {n}) + "b/./c"))
  let r = path.normalize("/" + string.repeat("..//./", {n}) + string.repeat("seg/", {n}))
  println("${{string.len(r)}} ${{string.slice(r, 0, 8)}}")
  let up = path.normalize(string.repeat("../", {n}) + "x")
  println("${{string.len(up)}} ${{string.slice(up, string.len(up) - 5, string.len(up))}}")
}}
"#
    )
}

fn normalize_want(n: usize) -> String {
    format!("b/c\n{} /seg/seg\n{} /../x\n", 4 * n, 3 * n + 1)
}

const FACTORIAL_SRC: &str = r#"fn main() -> Unit = {
  println(int.to_string(math.factorial(9223372036854775807)))
  println(int.to_string(math.factorial(4611686018427387904)))
  println(int.to_string(math.factorial(1000000000000000000)))
  println(int.to_string(math.factorial(66)))
  println(int.to_string(math.factorial(65)))
  println(int.to_string(math.factorial(20)))
  println(int.to_string(math.factorial(-9223372036854775807 - 1)))
}
"#;

// factorial(65) is 65! mod 2^64 as a signed Int: v2(65!) = 63 and the odd part
// is odd, so exactly bit 63 is set. factorial(66) is the first 0.
const FACTORIAL_WANT: &str = "0\n0\n0\n0\n-9223372036854775808\n2432902008176640000\n1\n";

fn almide(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn almide")
}

/// Native answer, and how long the built binary took; killed past `limit`.
fn native(dir: &Path, src: &str, limit: Duration) -> (String, Duration) {
    std::fs::write(dir.join("probe.almd"), src).expect("write");
    let b = almide(dir, &["build", "probe.almd", "-o", "probe.bin"]);
    assert!(
        b.status.success(),
        "native build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let t = Instant::now();
    let mut child = Command::new(dir.join("probe.bin"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn native");
    loop {
        if child.try_wait().expect("wait native").is_some() {
            break;
        }
        if t.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native probe still running after {limit:?} — cost chosen by the input again?");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let took = t.elapsed();
    let o = child.wait_with_output().expect("native output");
    assert!(
        o.status.success(),
        "native run failed:\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
    (String::from_utf8(o.stdout).expect("utf8"), took)
}

/// The stock wasm artifact's answer under a fuel budget.
fn stock_fueled(dir: &Path, src: &str, fuel: u64) -> Result<String, String> {
    std::fs::write(dir.join("probe.almd"), src).expect("write");
    let b = almide(
        dir,
        &[
            "build",
            "probe.almd",
            "--target",
            "wasm",
            "-o",
            "probe.wasm",
        ],
    );
    assert!(
        b.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&b.stderr)
    );
    let o = Command::new("wasmtime")
        .args(["run", "--dir=.", "-W"])
        .arg(format!("fuel={fuel}"))
        .arg("probe.wasm")
        .current_dir(dir)
        .output()
        .expect("spawn wasmtime");
    if o.status.success() {
        Ok(String::from_utf8(o.stdout).expect("utf8"))
    } else {
        Err(String::from_utf8_lossy(&o.stderr)
            .chars()
            .take(300)
            .collect())
    }
}

#[test]
fn path_normalize_is_linear_on_wasm() {
    if !has_wasmtime() {
        return;
    }
    let dir = tempfile::tempdir().expect("scratch");
    let (n, fuel_n) = (40_000, 650_000_000u64);
    for (size, fuel) in [(n, fuel_n), (2 * n, fuel_n * 2)] {
        let program = normalize_src(size);
        let (native_out, _) = native(dir.path(), &program, Duration::from_secs(30));
        assert_eq!(native_out, normalize_want(size), "native answer at {size}");
        match stock_fueled(dir.path(), &program, fuel) {
            Ok(got) => assert_eq!(got, native_out, "wasm differs from native at {size}"),
            Err(e) => panic!(
                "path.normalize at {size} ran out of its {fuel} fuel budget on wasm — quadratic again?\n{e}"
            ),
        }
    }
}

#[test]
fn path_normalize_stays_fast_on_native_at_the_issue_size() {
    // The issue's 320,000 took 17.5 s on native with the quadratic body.
    let dir = tempfile::tempdir().expect("scratch");
    let size = 320_000;
    let (got, took) = native(dir.path(), &normalize_src(size), Duration::from_secs(4));
    assert_eq!(got, normalize_want(size));
    assert!(took < Duration::from_secs(4), "took {took:?} at {size}");
}

#[test]
fn math_factorial_cost_does_not_follow_n() {
    let dir = tempfile::tempdir().expect("scratch");
    let (native_out, _) = native(dir.path(), FACTORIAL_SRC, Duration::from_secs(4));
    assert_eq!(native_out, FACTORIAL_WANT, "native answer");
    if !has_wasmtime() {
        return;
    }
    // A loop over n = 10^18 needs about 10^18 instructions; the bounded body
    // multiplies at most 65 times per call (the whole probe measured under
    // 20,000 fuel).
    match stock_fueled(dir.path(), FACTORIAL_SRC, 1_000_000) {
        Ok(got) => assert_eq!(got, native_out, "wasm differs from native"),
        Err(e) => {
            panic!("math.factorial ran out of its fuel budget on wasm — looping over n again?\n{e}")
        }
    }
}
