//! #3192: a `mut` param a closure captures and the fn writes is rebound onto a
//! write-back cell on native (#3154). Every shape of the fn's TAIL that reads
//! that cell — a call taking the param by `mut` or by value, a `!`-propagated
//! call, a `match` / `if` whose arms call with it, nested blocks, a loop, an
//! early `guard` exit — must build on native and print the same bytes on
//! native, structural wasm and the reference interpreter, for every way the
//! closure captures the param.
//!
//! The family's completeness rule: CAPTURES × TAILS, no omissions. Before the
//! fix the tail's temporary `RefMut` (`f(&mut *s.borrow_mut())`) outlived the
//! cell — a tail expression's temporaries drop after the block's locals under
//! edition 2021 — and rustc refused the program (E0597) for every cell whose
//! tail reads the param.
//!
//! The caller prints the param after the call, so the copy-in/write-back the
//! cell carries (#3103 ruling (A)) is observed on every exit, the `!` error
//! exit included. One program holds every cell and `main` runs the one its
//! first argument names, so native and wasm build ONCE; the interpreter runs
//! each cell from its own `main`.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

include!("wasm_runtime_test_parts/interp_leg.rs");

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// (capture, the body lines that capture `s` in a closure and write `s`).
const CAPTURES: &[(&str, &str)] = &[
    ("whole", "let ys = s.xs |> list.map((x) => x + weigh(s))\n  s.xs = ys"),
    ("field", "let ys = s.xs |> list.map((x) => x + s.n)\n  s.xs = ys"),
    ("writes", "let note = (k: Int) => bump(s, k)\n  note(3)"),
];

/// How a cell's result is observed by `main`.
#[derive(Clone, Copy)]
enum Ret {
    Unit,
    Int,
    /// An `effect fn -> Unit`; `main` matches its `ok` / `err`.
    Fallible,
}

/// (tail, return kind, the tail lines).
const TAILS: &[(&str, Ret, &str)] = &[
    ("mut_call", Ret::Unit, "bump(s, 1)"),
    ("mut_call_value", Ret::Int, "bumpv(s, 1)"),
    ("by_value", Ret::Int, "weigh(s)"),
    ("bang_ok", Ret::Fallible, "bump_e(s, 5)!"),
    ("bang_err", Ret::Fallible, "bump_e(s, 500)!"),
    ("match_arms", Ret::Unit, "match s.n {\n    0 => bump(s, 1),\n    _ => bump(s, 2),\n  }"),
    ("if_arms", Ret::Int, "if s.n > 2 then bumpv(s, 3) else bumpv(s, 4)"),
    ("nested_block", Ret::Unit, "{\n    let t = 2\n    {\n      bump(s, t)\n    }\n  }"),
    ("loop", Ret::Unit, "for i in 0..<3 {\n    bump(s, i)\n  }"),
    ("loop_then_call", Ret::Int, "for i in 0..<3 {\n    bump(s, i)\n  }\n  bumpv(s, 10)"),
    ("early_exit_taken", Ret::Int, "guard s.n < 0 else bumpv(s, 100)\n  bump(s, 1)\n  bumpv(s, 2)"),
    ("early_exit_passed", Ret::Int, "guard s.n < 1000 else bumpv(s, 100)\n  bump(s, 1)\n  bumpv(s, 2)"),
];

const PRELUDE: &str = r#"import process

type S = { n: Int, xs: List[Int] }

fn bump(mut s: S, k: Int) -> Unit = { s.n = s.n + k }

fn bumpv(mut s: S, k: Int) -> Int = {
  s.n = s.n + k
  s.n
}

fn weigh(s: S) -> Int = s.n * 100 + list.len(s.xs)

effect fn bump_e(mut s: S, k: Int) -> Unit = {
  s.n = s.n + k
  guard k < 100 else err("big ${int.to_string(k)}")
}

fn show(s: S) -> Unit =
  println("n=${int.to_string(s.n)} xs=${s.xs |> list.map((x) => int.to_string(x)) |> list.join(",")}")
"#;

struct Cell {
    name: String,
    fn_src: String,
    /// The `main` lines that run the cell on a fresh `s` and print it.
    call: String,
}

fn cells() -> Vec<Cell> {
    let mut out = Vec::new();
    for (cap, prefix) in CAPTURES {
        for (tail, ret, body) in TAILS {
            let name = format!("c_{cap}__{tail}");
            let (sig, call) = match ret {
                Ret::Unit => ("fn", format!("{name}(s)")),
                Ret::Int => ("fn", format!("println(\"ret ${{int.to_string({name}(s))}}\")")),
                Ret::Fallible => (
                    "effect fn",
                    format!("let o = match {name}(s) {{\n    ok(_) => \"ok\",\n    err(m) => \"err ${{m}}\",\n  }}\n  println(o)"),
                ),
            };
            let ty = if matches!(ret, Ret::Int) { "Int" } else { "Unit" };
            let fn_src = format!("{sig} {name}(mut s: S) -> {ty} = {{\n  {prefix}\n  {body}\n}}\n");
            let call = format!("var s = S {{ n: 1, xs: [1, 2] }}\n  {call}\n  show(s)");
            out.push(Cell { name, fn_src, call });
        }
    }
    out
}

fn program_with_main(cells: &[Cell], main: &str) -> String {
    let mut src = PRELUDE.to_string();
    for c in cells {
        src.push('\n');
        src.push_str(&c.fn_src);
    }
    src.push('\n');
    src.push_str(main);
    src
}

fn dispatch_main(cells: &[Cell]) -> String {
    let arms: String = cells.iter().map(|c| format!("    \"{}\" => {{\n  {}\n    }},\n", c.name, c.call)).collect();
    format!(
        "effect fn main() -> Unit = {{\n  let which = list.get(process.args(), 1) ?? \"\"\n  match which {{\n{arms}    _ => println(\"no such cell\"),\n  }}\n}}\n"
    )
}

fn run(dir: &Path, cell: &str, wasm: bool) -> (i32, String, String) {
    let mut args = vec!["run", "p.almd"];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    args.extend(["--", cell]);
    let o = Command::new(almide()).current_dir(dir).args(&args).output().expect("almide run");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn a_tail_reading_a_write_back_cell_builds_and_agrees_on_every_leg() {
    let wasm = wasmtime_available();
    if !wasm {
        eprintln!("NOTE mut-param cell tails: no `wasmtime` on PATH — the wasm leg is skipped here");
    }
    let cells = cells();
    assert_eq!(cells.len(), CAPTURES.len() * TAILS.len(), "the matrix lost a cell");
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("p.almd"), program_with_main(&cells, &dispatch_main(&cells))).unwrap();
    let mut failures = Vec::new();
    for c in &cells {
        let native = run(dir.path(), &c.name, false);
        if native.0 != 0 || !native.1.contains("n=") {
            failures.push(format!("{}: native did not run: {native:?}", c.name));
            continue;
        }
        if wasm {
            let w = run(dir.path(), &c.name, true);
            if w != native {
                failures.push(format!("{}: wasm {w:?} differs from native {native:?}", c.name));
            }
        }
        let main = format!("effect fn main() -> Unit = {{\n  {}\n}}\n", c.call);
        match run_interp_capture(&program_with_main(&cells, &main)) {
            InterpLeg::Ran(code, out, err) => {
                let want = (native.0, native.1.trim().to_string(), native.2.trim().to_string());
                if (code, out.clone(), err.clone()) != want {
                    failures.push(format!("{}: interp {:?} differs from native {want:?}", c.name, (code, out, err)));
                }
            }
            InterpLeg::Skip(why) => failures.push(format!("{}: the interp abstained ({why})", c.name)),
        }
    }
    assert!(failures.is_empty(), "mut-param cell tail divergences ({}):\n{}", failures.len(), failures.join("\n"));
}
