//! #3118: `panic(msg)` aborts through ONE defined path on every leg, in every
//! position — `PANIC: <msg>` on stderr with no trailing newline, exit 1 (C-219).
//! Native used to lower `panic` to Rust's unwinding `panic!`, which the verified
//! native rung never reached for the shapes it walls (a guard else block, a
//! match arm returning a heap Option): those programs exited 101 with the
//! thread banner, while wasm and the interp printed `PANIC:` and exited 1. An
//! effect fn whose tail or arm was a `panic` did not even compile natively
//! (`Ok::<(), String>(panic)`, E0308).
//!
//! Each case builds one program from the shared positions below and takes ONE
//! abort; native, structural wasm and the reference interpreter must agree on
//! stdout, stderr and the exit code. The spec fixture
//! `spec/wasm_cross/panic_abort_positions.almd` pins the positions the MIR rung
//! also lowers; the guard and match-arm positions it walls live here, where no
//! walled-real row is needed.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

include!("wasm_runtime_test_parts/interp_leg.rs");

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

const POSITIONS: &str = r#"fn tail_pure(n: Int) -> Int = if n > 0 then n else panic("tail_pure ${n}")

effect fn tail_effect(n: Int) -> Int = {
  println("tail_effect ${n}")
  if n > 0 then n else panic("tail_effect ${n}")
}

fn guard_block_pure(xs: List[Int]) -> Int = {
  var n = 0
  for x in xs {
    guard x > 0 else {
      panic("guard_block_pure ${x}")
    }
    n = n + x
  }
  n
}

effect fn guard_block_effect(xs: List[Int]) -> Int = {
  var n = 0
  for x in xs {
    let tag = "x${x}"
    guard x > 0 else {
      println("leaving at ${tag}")
      panic("guard_block_effect ${tag}")
    }
    n = n + x
  }
  n
}

fn arm_pure(o: Option[Int]) -> Int = match o {
  some(v) => v,
  none => panic("arm_pure none"),
}

effect fn arm_effect(o: Option[String]) -> String = match o {
  some(v) => v,
  none => {
    println("arm_effect none")
    panic("arm_effect none")
  },
}

fn lambda_pure(xs: List[Int]) -> List[Int] = xs |> list.map((x) => if x >= 0 then x * 2 else panic("lambda_pure ${x}"))

fn loop_pure(m: Int) -> Int = {
  var i = 0
  var s = 0
  while i < 10 {
    i = i + 1
    if i == m then panic("loop_pure ${i}") else ()
    s = s + i
  }
  s
}

effect fn loop_effect(m: Int) -> Int = {
  var s = 0
  for i in 0..<10 {
    println("loop_effect ${i}")
    if i == m then panic("loop_effect ${i}") else ()
    s = s + i
  }
  s
}

fn value_pure(n: Int) -> String = {
  let label = if n < 0 then panic("value_pure ${n}") else "n=${n}"
  label + "!"
}
"#;

/// (position, the `main` statement that takes its abort, the panic message).
const CASES: &[(&str, &str, &str)] = &[
    ("tail_pure", r#"println("${tail_pure(-1)}")"#, "tail_pure -1"),
    ("tail_effect", r#"println("${tail_effect(-1)!}")"#, "tail_effect -1"),
    ("guard_block_pure", r#"println("${guard_block_pure([1, -2, 3])}")"#, "guard_block_pure -2"),
    ("guard_block_effect", r#"println("${guard_block_effect([1, -2, 3])!}")"#, "guard_block_effect x-2"),
    ("arm_pure", r#"println("${arm_pure(none)}")"#, "arm_pure none"),
    ("arm_effect", r#"println(arm_effect(none)!)"#, "arm_effect none"),
    ("lambda_pure", r#"println("${list.len(lambda_pure([1, -2, 3]))}")"#, "lambda_pure -2"),
    ("loop_pure", r#"println("${loop_pure(4)}")"#, "loop_pure 4"),
    ("loop_effect", r#"println("${loop_effect(2)!}")"#, "loop_effect 2"),
    ("value_pure", r#"println(value_pure(-5))"#, "value_pure -5"),
    ("main_stmt", r#"panic("main_stmt")"#, "main_stmt"),
];

fn program(call: &str, name: &str) -> String {
    format!("{POSITIONS}\neffect fn main() -> Unit = {{\n  println(\"start {name}\")\n  {call}\n  println(\"unreachable\")\n}}\n")
}

fn run(dir: &Path, wasm: bool) -> (i32, String, String) {
    let mut args = vec!["run", "p.almd"];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    let o = Command::new(almide()).current_dir(dir).args(&args).output().expect("almide run");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn panic_aborts_identically_on_native_wasm_and_interp_in_every_position() {
    let wasm = wasmtime_available();
    if !wasm {
        eprintln!("NOTE panic positions: no `wasmtime` on PATH — the wasm leg is skipped here");
    }
    let mut failures = Vec::new();
    for (name, call, msg) in CASES {
        let src = program(call, name);
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("p.almd"), &src).unwrap();
        let native = run(dir.path(), false);
        let (code, out, err) = &native;
        if *code != 1 || err != &format!("PANIC: {msg}") {
            failures.push(format!("{name}: native exit {code}, stderr {err:?}; want exit 1, {:?}", format!("PANIC: {msg}")));
        }
        if !out.starts_with(&format!("start {name}\n")) || out.contains("unreachable") {
            failures.push(format!("{name}: native stdout {out:?} does not stop at the abort"));
        }
        if wasm {
            let w = run(dir.path(), true);
            if w != native {
                failures.push(format!("{name}: wasm {w:?} differs from native {native:?}"));
            }
        }
        match run_interp_capture(&src) {
            InterpLeg::Ran(icode, iout, ierr) => {
                let trimmed = (*code, out.trim().to_string(), err.trim().to_string());
                if (icode, iout.clone(), ierr.clone()) != trimmed {
                    failures.push(format!("{name}: interp {:?} differs from native {trimmed:?}", (icode, iout, ierr)));
                }
            }
            InterpLeg::Skip(why) => failures.push(format!("{name}: the interp abstained ({why})")),
        }
    }
    assert!(failures.is_empty(), "panic abort divergences:\n{}", failures.join("\n"));
}
