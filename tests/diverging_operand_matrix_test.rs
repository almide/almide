//! #3144: a diverging expression in a VALUE position — every position × every
//! kind of diverging expression — runs the operands to its left, left to right
//! (C-192), then aborts at it (C-219), identically on native, structural wasm
//! and the reference interpreter.
//!
//! The family's completeness rule: a `Never`-typed expression is accepted
//! wherever a value is, so every cell of POSITIONS × DIVERGERS must build and
//! abort the same way on every leg. The one omission is intentional and is
//! asserted, not skipped: `bail(c)!` inside a lambda is E041 (a lambda is not
//! an effect context), the same rule as any other effect call there.
//!
//! Before the fix only `panic` / `process.exit` as an `if` / `match` arm, a
//! call argument, a list element or a `??` fallback ran on both legs; every
//! other cell failed somewhere: native rendered `Never` as `()`
//! (`format!("{} {}", f(), <panic>)` is E0277, `vec![g(), die("p")]` found `()`
//! where it wanted `i64`, `let n: Int = panic("p")` bound `let n: ()`), the
//! structural wasm leg walled `diverging-call-untyped` / `bind-ty:Never` /
//! `bind-ty:Record` / `call-fn:die:ret-ty:Never`, and the checker refused a
//! diverging operand of `+`, `-`, `and`, `not` and unary `-`.
//!
//! One program holds every cell as an `effect fn`, and `main` runs the one its
//! first argument names, so native and wasm build ONCE (the build cache serves
//! every later run). The interpreter runs each cell from its own `main`.

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

/// (position, the cell body with `{D}` for the diverging expression, the lines
/// the operands to its left print before the abort).
const POSITIONS: &[(&str, &str, &[&str])] = &[
    ("interp", "let s = \"${f()} ${{D}}\"\n  println(s)", &["f"]),
    ("interp_arg", "println(\"${f()} ${{D}}\")", &["f"]),
    ("binop_r", "let n = g() + {D}\n  println(int.to_string(n))", &["g"]),
    ("binop_l", "let n = {D} + g()\n  println(int.to_string(n))", &[]),
    ("concat", "let s = f() + {D}\n  println(s)", &["f"]),
    ("eq", "let b = g() == {D}\n  println(if b then \"t\" else \"f\")", &["g"]),
    ("and_r", "let b = g() > 0 and {D}\n  println(if b then \"t\" else \"f\")", &["g"]),
    ("or_l", "let b = {D} or g() > 0\n  println(if b then \"t\" else \"f\")", &[]),
    ("neg", "let n = -({D})\n  println(int.to_string(n))", &[]),
    ("call_arg", "let n = add(g(), {D})\n  println(int.to_string(n))", &["g"]),
    ("list", "let xs = [g(), {D}]\n  println(int.to_string(list.len(xs)))", &["g"]),
    ("map", "let m = [\"a\": g(), \"b\": {D}]\n  println(int.to_string(map.len(m)))", &["g"]),
    ("record", "let r = { a: g(), b: {D} }\n  println(int.to_string(r.a))", &["g"]),
    ("tuple", "let t = (g(), {D})\n  println(int.to_string(t.0))", &["g"]),
    ("if_cond", "let n = if {D} then 1 else 2\n  println(int.to_string(n))", &[]),
    ("if_then", "let n = if g() > 0 then {D} else 1\n  println(int.to_string(n))", &["g"]),
    ("if_else", "let n = if g() < 0 then 1 else {D}\n  println(int.to_string(n))", &["g"]),
    ("match_arm", "let n = match g() { 1 => {D}, _ => 0 }\n  println(int.to_string(n))", &["g"]),
    ("coalesce", "let n = int.parse(f()) ?? {D}\n  println(int.to_string(n))", &["f"]),
    ("let_annotated", "let n: Int = {D}\n  println(int.to_string(n))", &[]),
    ("lambda", "let xs = [g()] |> list.map((x) => \"${x} ${{D}}\")\n  println(list.join(xs, \",\"))", &["g"]),
];

/// (name, the diverging expression, exit code, stderr).
const DIVERGERS: &[(&str, &str, i32, &str)] = &[
    ("panic", "panic(\"p\")", 1, "PANIC: p"),
    ("exit", "process.exit(3)", 3, ""),
    ("never_fn", "die(\"p\")", 1, "PANIC: p"),
    ("never_effect_fn", "bail(4)!", 4, ""),
];

/// The one intentional omission: an effect call inside a lambda is E041.
fn omitted(pos: &str, div: &str) -> bool {
    pos == "lambda" && div == "never_effect_fn"
}

const PRELUDE: &str = r#"import process

fn f() -> String = {
  println("f")
  "x"
}

fn g() -> Int = {
  println("g")
  1
}

fn add(a: Int, b: Int) -> Int = a + b

fn die(m: String) -> Never = panic(m)

effect fn bail(c: Int) -> Never = process.exit(c)
"#;

fn cell_name(pos: &str, div: &str) -> String {
    format!("c_{pos}__{div}")
}

fn cells() -> Vec<(String, String, i32, &'static str, String)> {
    let mut out = Vec::new();
    for (pos, body, left) in POSITIONS {
        for (div, expr, code, err) in DIVERGERS {
            if omitted(pos, div) {
                continue;
            }
            let mut stdout = String::from("start\n");
            for l in *left {
                stdout.push_str(l);
                stdout.push('\n');
            }
            let fn_src = format!("effect fn {}() -> Unit = {{\n  {}\n}}\n", cell_name(pos, div), body.replace("{D}", expr));
            out.push((cell_name(pos, div), fn_src, *code, *err, stdout));
        }
    }
    out
}

fn program_with_main(cells: &[(String, String, i32, &str, String)], main: &str) -> String {
    let mut src = PRELUDE.to_string();
    for (_, fn_src, ..) in cells {
        src.push('\n');
        src.push_str(fn_src);
    }
    src.push('\n');
    src.push_str(main);
    src
}

fn dispatch_main(cells: &[(String, String, i32, &str, String)]) -> String {
    let mut arms = String::new();
    for (name, ..) in cells {
        arms.push_str(&format!("    \"{name}\" => {name}()!,\n"));
    }
    format!(
        "effect fn main() -> Unit = {{\n  let which = list.get(process.args(), 1) ?? \"\"\n  println(\"start\")\n  match which {{\n{arms}    _ => println(\"no such cell\"),\n  }}\n  println(\"unreachable\")\n}}\n"
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
fn a_diverging_value_aborts_identically_in_every_position_on_every_leg() {
    let wasm = wasmtime_available();
    if !wasm {
        eprintln!("NOTE diverging operands: no `wasmtime` on PATH — the wasm leg is skipped here");
    }
    let cells = cells();
    assert_eq!(cells.len(), POSITIONS.len() * DIVERGERS.len() - 1, "the matrix lost a cell");
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("p.almd"), program_with_main(&cells, &dispatch_main(&cells))).unwrap();
    let mut failures = Vec::new();
    for (name, _, code, err, stdout) in &cells {
        let want = (*code, stdout.clone(), err.to_string());
        let native = run(dir.path(), name, false);
        if native != want {
            failures.push(format!("{name}: native {native:?}, want {want:?}"));
        }
        if wasm {
            let w = run(dir.path(), name, true);
            if w != native {
                failures.push(format!("{name}: wasm {w:?} differs from native {native:?}"));
            }
        }
        let interp_src = program_with_main(
            &cells,
            &format!("effect fn main() -> Unit = {{\n  println(\"start\")\n  {name}()!\n  println(\"unreachable\")\n}}\n"),
        );
        match run_interp_capture(&interp_src) {
            InterpLeg::Ran(icode, iout, ierr) => {
                let trimmed = (*code, stdout.trim().to_string(), err.trim().to_string());
                if (icode, iout.clone(), ierr.clone()) != trimmed {
                    failures.push(format!("{name}: interp {:?} differs from {trimmed:?}", (icode, iout, ierr)));
                }
            }
            InterpLeg::Skip(why) => failures.push(format!("{name}: the interp abstained ({why})")),
        }
    }
    assert!(failures.is_empty(), "diverging-operand divergences ({}):\n{}", failures.len(), failures.join("\n"));
}

/// The omitted cell stays omitted for the stated reason: a lambda is not an
/// effect context, so `bail(4)!` there is E041 — not a silent gap.
#[test]
fn the_omitted_cell_is_refused_by_the_effect_rule() {
    let (_, body, _) = POSITIONS.iter().find(|(p, ..)| *p == "lambda").expect("lambda position");
    let src = format!(
        "{PRELUDE}\neffect fn main() -> Unit = {{\n  {}\n}}\n",
        body.replace("{D}", "bail(4)!")
    );
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("p.almd"), src).unwrap();
    let o = Command::new(almide()).current_dir(dir.path()).args(["check", "p.almd"]).output().expect("almide check");
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success() && err.contains("E041"), "want E041, got: {err}");
}
