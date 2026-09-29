//! #1490 item 3: `almide bench` — verify-then-time with a median
//! headline. The refusal is the load-bearing part: a workload whose
//! output drifts between runs is measuring different work, and the bench
//! must say so instead of averaging nonsense. The wasm leg keeps these
//! tests rustc-free (the native leg shares every line but the runner).

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

fn bench(source: &str, name: &str) -> (bool, String) {
    bench_args(source, name, &[])
}

fn bench_args(source: &str, name: &str, args: &[&str]) -> (bool, String) {
    let dir = std::env::temp_dir().join("almide-bench-test");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join(name);
    std::fs::write(&src, source).expect("write");
    let out = Command::new(almide_bin())
        .args(["bench", src.to_str().unwrap(), "--target", "wasm", "--runs", "3"])
        .args(if args.is_empty() { vec![] } else { std::iter::once("--").chain(args.iter().copied()).collect() })
        .output()
        .expect("spawn almide bench");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn deterministic_workload_reports_a_verified_median() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let (ok, err) = bench(
        "fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)\n\nfn main() -> Unit = {\n  println(int.to_string(fib(20)))\n}\n",
        "fib.almd",
    );
    assert!(ok, "bench failed:\n{err}");
    assert!(err.contains("median"), "no median headline:\n{err}");
    assert!(
        err.contains("output verified identical across all runs"),
        "the verification claim is missing:\n{err}"
    );
}

#[test]
fn drifting_output_is_refused_not_averaged() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let (ok, err) = bench(
        "import random\n\neffect fn main() -> Unit = {\n  println(int.to_string(random.int(0, 1000000)))\n}\n",
        "nondet.almd",
    );
    assert!(!ok, "a nondeterministic workload must refuse, got:\n{err}");
    assert!(
        err.contains("nondeterministic"),
        "the refusal must name the cause:\n{err}"
    );
}

/// #2980: the headline is `main` alone and the cold start is its own column,
/// so startup is visible without being the gated quantity.
#[test]
fn the_headline_is_main_alone_and_cold_start_is_its_own_column() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let (ok, err) = bench(
        "fn main() -> Unit = {\n  println(\"hi\")\n}\n",
        "hi.almd",
    );
    assert!(ok, "bench failed:\n{err}");
    assert!(err.contains("main() only"), "the headline must name its boundary:\n{err}");
    assert!(
        err.contains("cold start median") && err.contains("compile + instantiate + run"),
        "the cold-start column is missing:\n{err}"
    );
}

/// #2980: program arguments after `--` reach the benched program (the
/// ledger sizes small rows with them).
#[test]
fn program_args_after_the_separator_reach_the_program() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let src = "import env\nimport process\n\neffect fn main() -> Unit = {\n  let a = env.args()\n  if list.len(a) == 0 then process.exit(3) else println(\"sized\")\n}\n";
    let (ok, err) = bench_args(src, "sized.almd", &["7"]);
    assert!(ok, "with an argument the workload must run cleanly:\n{err}");
    let (ok, err) = bench_args(src, "sized.almd", &[]);
    assert!(!ok && err.contains("exited 3"), "without it the program must see no argument:\n{err}");
}
