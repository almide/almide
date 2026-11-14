//! A subtracting reduction gives the same value at every build level (#4674).
//!
//! `almide run` printed `-1` on arm64 macOS for
//! `list.range(0, 1000) |> list.map((x) => x / 2) |> list.fold(1, (acc, x) => acc - x)`
//! where `almide build --release` and wasm printed `-249499`. The program's
//! Rust was the same at both levels; the loop unroller of the backend the
//! toolchain ships (LLVM 22) split the reduction over four accumulators and
//! joined them with subtractions, and at opt-level 1 no other loop pass takes
//! the loop first. The split is on by default only for the arm64 macOS CPU,
//! so the fix is an option every native build passes at every level
//! (`NATIVE_LLVM_ARGS`, src/cli/native_target.rs).
//!
//! What this test holds, for every length 0..=2048 and nine reducers (the
//! issue's, a fold without the map, `for` and `while` loops over a `var`, a
//! multiplying and a remainder body, a materialized list, a float, and an
//! adding control):
//! - the default level (`almide run`), `--release`, `almide test` and wasm
//!   print the value computed here;
//! - the same on the cargo route with the unroller's split FORCED ON by an
//!   inherited `RUSTFLAGS` — the leg that fails without the fix on every
//!   architecture, x86-64 included: Almide's option comes after the inherited
//!   ones and overrides them.
//!
//! The oracle sums with additions only and subtracts once, so a build of this
//! test file with the same defect still computes the right expectation.
//!
//! The wasm leg skips when wasmtime is not installed.

use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_LEN: i64 = 2048;

const REDUCERS: &str = r#"fn r_map_fold(n: Int) -> Int =
  list.range(0, n) |> list.map((x) => x / 2) |> list.fold(1, (acc, x) => acc - x)

fn r_fold(n: Int) -> Int = list.range(0, n) |> list.fold(1, (acc, x) => acc - x / 2)

fn r_for(n: Int) -> Int = {
  var s = 1
  for x in 0..<n {
    s = s - x / 2
  }
  s
}

fn r_while(n: Int) -> Int = {
  var s = 1
  var x = 0
  while x < n {
    s = s - x / 2
    x = x + 1
  }
  s
}

fn r_mul(n: Int) -> Int =
  list.range(0, n) |> list.map((x) => x * 3) |> list.fold(7, (acc, x) => acc - x)

fn r_mod(n: Int) -> Int = list.range(0, n) |> list.fold(0, (acc, x) => acc - x % 7)

fn r_list(xs: List[Int]) -> Int = xs |> list.fold(1, (acc, x) => acc - x)

fn r_float(n: Int) -> Int =
  float.to_bits(list.range(0, n) |> list.fold(1.0, (acc, x) => acc - float.from(x) / 2.0))

fn r_add(n: Int) -> Int =
  list.range(0, n) |> list.map((x) => x / 2) |> list.fold(1, (acc, x) => acc + x)

fn row(n: Int) -> String = {
  let squares = list.range(0, n) |> list.map((x) => x * x)
  "${n} ${r_map_fold(n)} ${r_fold(n)} ${r_for(n)} ${r_while(n)} ${r_mul(n)} ${r_mod(n)} ${r_list(squares)} ${r_float(n)} ${r_add(n)}"
}
"#;

fn program() -> String {
    format!("{REDUCERS}\nfn main() -> Unit = {{\n  for n in 0..<{} {{\n    println(row(n))\n  }}\n}}\n", MAX_LEN + 1)
}

/// The rows the program prints. Every sum is accumulated with `+` and
/// subtracted from the start value once.
fn oracle() -> String {
    let mut out = String::new();
    for n in 0..=MAX_LEN {
        let (mut halves, mut triples, mut rems, mut squares) = (0i64, 0i64, 0i64, 0i64);
        for x in 0..n {
            halves += x / 2;
            triples += x * 3;
            rems += x % 7;
            squares += x * x;
        }
        // Every partial float sum is a multiple of 0.5 below 2^53: exact.
        let float_bits = (1.0f64 - halves_exact(n)).to_bits() as i64;
        out.push_str(&format!(
            "{n} {} {} {} {} {} {} {} {float_bits} {}\n",
            1 - halves,
            1 - halves,
            1 - halves,
            1 - halves,
            7 - triples,
            -rems,
            1 - squares,
            1 + halves,
        ));
    }
    out
}

/// 0/2 + 1/2 + … + (n-1)/2 in floats: n(n-1)/4.
fn halves_exact(n: i64) -> f64 {
    (n * (n - 1)) as f64 / 4.0
}

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-unrolled-reduction-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

fn write_program(dir: &Path) -> PathBuf {
    let src = dir.join("reduce.almd");
    std::fs::write(&src, program()).expect("write program");
    src
}

/// The first row where `got` and the oracle differ, for a readable failure.
fn assert_rows(leg: &str, got: &str) {
    let want = oracle();
    if got == want {
        return;
    }
    let diff = got.lines().zip(want.lines()).find(|(g, w)| g != w);
    match diff {
        Some((g, w)) => panic!("{leg}: first wrong length\n  got:  {g}\n  want: {w}\n(columns: n map_fold fold for while mul mod list float add)"),
        None => panic!("{leg}: printed {} rows, expected {}", got.lines().count(), want.lines().count()),
    }
}

fn stdout_of(leg: &str, cmd: &mut Command) -> String {
    let out = cmd.output().unwrap_or_else(|e| panic!("{leg}: spawn: {e}"));
    assert!(out.status.success(), "{leg} failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_default_level_subtracts_like_the_release_level() {
    let dir = scratch("levels");
    let src = write_program(&dir);
    assert_rows("almide run", &stdout_of("almide run", almide().args(["run", src.to_str().unwrap()])));

    let bin = dir.join(if cfg!(windows) { "reduce.exe" } else { "reduce" });
    let built = almide()
        .args(["build", src.to_str().unwrap(), "--release", "-o", bin.to_str().unwrap()])
        .output()
        .expect("spawn almide build");
    assert!(built.status.success(), "almide build --release failed:\n{}", String::from_utf8_lossy(&built.stderr));
    assert_rows("almide build --release", &stdout_of("the release binary", &mut Command::new(&bin)));
}

#[test]
fn wasm_subtracts_like_native() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("skip: wasmtime is not installed");
        return;
    }
    let dir = scratch("wasm");
    let src = write_program(&dir);
    assert_rows("almide run --target wasm", &stdout_of("almide run --target wasm", almide().args(["run", src.to_str().unwrap(), "--target", "wasm"])));
}

/// `almide test` builds at its own level (the test body unoptimized against
/// the runtime at opt-level 1, or the whole crate at opt-level 1).
#[test]
fn a_test_run_subtracts_like_the_release_level() {
    let dir = scratch("test");
    let src = dir.join("reduce_test.almd");
    let want = oracle();
    let rows: Vec<&str> = want.lines().filter(|l| ["0 ", "4 ", "100 ", "128 ", "1000 ", "2048 "].iter().any(|p| l.starts_with(p))).collect();
    assert_eq!(rows.len(), 6);
    let checks: String = rows.iter().map(|r| {
        let n = r.split(' ').next().unwrap();
        format!("  assert_eq(row({n}), \"{r}\")\n")
    }).collect();
    std::fs::write(&src, format!("{REDUCERS}\ntest \"subtracting reductions\" {{\n{checks}}}\n")).expect("write test");
    let out = almide().args(["test", src.to_str().unwrap()]).output().expect("spawn almide test");
    assert!(
        out.status.success(),
        "almide test failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The leg every architecture fails without the fix: the cargo route reads
/// `RUSTFLAGS`, and these turn the unroller's split on (with the runtime
/// unrolling and the count the arm64 macOS tuning picks). Almide's own option
/// is appended after them, so the loop keeps one accumulator.
#[test]
fn an_inherited_flag_cannot_turn_the_split_back_on() {
    let dir = scratch("forced");
    let src = write_program(&dir);
    let tmp = dir.join("tmp");
    std::fs::create_dir_all(&tmp).expect("tmp");
    let forced = "-C llvm-args=-unroll-runtime -C llvm-args=-unroll-count=4 -C llvm-args=-unroll-add-parallel-reductions=true";
    let got = stdout_of(
        "almide run (cargo route, split forced on)",
        almide()
            .args(["--allow-switch", "ALMIDE_NO_RTLIB", "run", src.to_str().unwrap()])
            .env("ALMIDE_NO_RTLIB", "1")
            .env("RUSTFLAGS", forced)
            .env("TMPDIR", &tmp),
    );
    // The cargo route was taken: only it reads the inherited flags.
    let through_cargo = std::fs::read_dir(&tmp)
        .expect("tmp")
        .filter_map(|e| e.ok())
        .any(|e| e.path().join("run").join("Cargo.toml").is_file());
    assert!(through_cargo, "the forced leg did not build through cargo: it proves nothing");
    assert_rows("almide run (cargo route, split forced on)", &got);
}
