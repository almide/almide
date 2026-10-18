//! #3754: a `mut`-parameter call on a `var` that a closure in the same
//! statement also captures. The capture makes the var a shared cell on the
//! native leg, the mutating argument borrows that cell mutably, and the
//! borrow used to outlive the call — so the closure's read of the var, run
//! later in the same statement, panicked ("RefCell already mutably
//! borrowed") where wasm ran. The promise every leg keeps: arguments are
//! evaluated before the call, the mutation is complete when the call
//! returns, and a closure reads the var's value at the time it runs.
use std::process::Command;

const SRC: &str = r#"type Bag = { xs: List[Int], n: Int }

fn take(mut xs: List[Int]) -> Int = {
  let v = list.pop(xs) ?? 0
  v * 10
}

fn main() -> Unit = {
  var acc4: List[Bool] = [map.contains(["k0": false], "x"), false]
  let r6: List[Bool] = list.flat_map(option.to_list(list.pop(acc4)), ((x5) => acc4))
  println("${r6} ${acc4}")
  var xs: List[Int] = [1, 2, 3]
  let a = list.map(option.to_list(list.pop(xs)), (x) => xs)
  println("${a} ${xs}")
  var ys: List[Int] = [4, 5, 6]
  let b = list.map([take(ys)], (x) => list.len(ys) + x)
  println("${b} ${ys}")
  var h: Bag = { xs: [7, 8, 9], n: 0 }
  let c = list.filter_map(option.to_list(list.pop(h.xs)), (x) => list.last(h.xs))
  println("${c} ${h.xs}")
  var zs: List[Int] = [1]
  let d = list.flat_map([list.push(zs, 2)], (u) => zs)
  println("${d} ${zs}")
}
"#;

const EXPECTED: &str = "[false] [false]\n[[1, 2]] [1, 2]\n[62] [4, 5]\n[8] [7, 8]\n[1, 2] [1, 2]\n";

fn run(target: Option<&str>) -> (Option<i32>, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("mut_capture.almd");
    std::fs::write(&file, SRC).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if let Some(t) = target {
        c.args(["--target", t]);
    }
    let out = c.output().expect("run");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_mut_param_call_ends_its_borrow_before_a_capturing_closure_runs_on_both_legs() {
    let (native_code, native_out, native_err) = run(None);
    let (wasm_code, wasm_out, wasm_err) = run(Some("wasm"));
    assert_eq!(wasm_code, Some(0), "wasm: {wasm_err}");
    assert_eq!(wasm_out, EXPECTED, "wasm stdout");
    assert_eq!(native_code, wasm_code, "exit code differs; native stderr: {native_err}");
    assert_eq!(native_out, wasm_out, "stdout differs; native stderr: {native_err}");
}
