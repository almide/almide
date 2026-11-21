//! effect.e007-e008-fan-context (#4903, ADR-0020 §3.1 rule (b)): a fn with a
//! concurrent slot — a parameter that flows into `fan.map` — is called only by
//! name. A `let` alias is followed (`let g = w; g(cb)` holds `cb` to E008),
//! and every other use of it as a value is refused with E008, since the call
//! it then makes cannot be resolved and its concurrent argument would go
//! unchecked. The wrapper, the `var` and the callback are fresh names, so no
//! list in the compiler can know them. The controls are the same wrappers
//! with a callback that reaches no `var`, called by name or through an alias:
//! they check and run.
use std::process::{Command, Output};

#[path = "support/novel.rs"]
mod novel;

struct Names {
    w: String,
    v: String,
    cb: String,
}

fn names() -> Names {
    Names { w: novel::ident("wrap"), v: novel::ident("count"), cb: novel::ident("bump") }
}

/// The wrapper `w` (its `f` flows into `fan.map`), a top-level `var` and a
/// callback writing it, then `body` as `main`.
fn program(n: &Names, body: &str) -> String {
    format!(
        "effect fn {w}(f: (Int) -> Result[Int, String]) -> Result[List[Int], String] = fan.map([1, 2, 3], f)\n\n\
         var {v} = 0\n\n\
         fn {cb}(x: Int) -> Result[Int, String] = {{\n  {v} = {v} + x\n  ok(x)\n}}\n\n\
         effect fn apply(h: ((Int) -> Result[Int, String]) -> Result[List[Int], String], c: (Int) -> Result[Int, String]) -> Result[List[Int], String] = h(c)\n\n\
         effect fn main() -> Unit = {{\n{body}\n  println(\"${{{v}}}\")\n}}\n",
        w = n.w,
        v = n.v,
        cb = n.cb,
    )
}

fn almide(args: &[&str], src: &str) -> Output {
    let t = tempfile::tempdir().expect("scratch");
    let file = t.path().join("main.almd");
    std::fs::write(&file, src).expect("write source");
    Command::new(env!("CARGO_BIN_EXE_almide")).args(args).arg(&file).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit={:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// `almide check` refuses `src` with E008 naming `name`.
fn refused(src: &str, name: &str) {
    let o = almide(&["check"], src);
    let t = text(&o);
    assert!(!o.status.success(), "expected E008, the program checked:\n{src}\n{t}");
    assert!(t.contains("E008") && t.contains(name), "expected E008 naming `{name}`:\n{src}\n{t}");
}

#[test]
fn novel_a_let_alias_of_a_fan_wrapper_holds_its_callback_to_e008() {
    let n = names();
    refused(&program(&n, &format!("  let g = {w}\n  let r = g({cb})!\n  println(\"${{r}}\")", w = n.w, cb = n.cb)), &n.v);
    refused(&program(&n, &format!("  let g = ({w})\n  let k = g\n  let r = k({cb})!\n  println(\"${{r}}\")", w = n.w, cb = n.cb)), &n.v);
}

#[test]
fn novel_a_let_bound_lambda_wrapper_holds_its_callback_to_e008() {
    let n = names();
    let run = novel::ident("run");
    let body = format!(
        "  let {run} = (f: (Int) -> Result[Int, String]) => fan.map([1, 2, 3], f)\n  let r = {run}({cb})!\n  println(\"${{r}}\")",
        cb = n.cb
    );
    refused(&program(&n, &body), &n.v);
}

#[test]
fn novel_a_fan_wrapper_used_as_a_value_is_refused() {
    let n = names();
    let w = &n.w;
    let cb = &n.cb;
    // An argument, a list element, a tuple element, a `var`.
    refused(&program(&n, &format!("  let r = apply({w}, {cb})!\n  println(\"${{r}}\")")), w);
    refused(&program(&n, &format!("  let hs = [{w}]\n  println(\"${{list.len(hs)}}\")")), w);
    refused(&program(&n, &format!("  let t = ({w}, 1)\n  println(\"${{t.1}}\")")), w);
    refused(&program(&n, &format!("  var h = {w}\n  let r = h({cb})!\n  println(\"${{r}}\")")), w);
    // A `let` alias used as a value is the fn used as a value.
    refused(&program(&n, &format!("  let g = {w}\n  let r = apply(g, {cb})!\n  println(\"${{r}}\")")), "E008");
}

#[test]
fn novel_a_let_bound_lambda_wrapper_used_as_a_value_is_refused() {
    let n = names();
    let run = novel::ident("run");
    let body = format!(
        "  let {run} = (f: (Int) -> Result[Int, String]) => fan.map([1, 2, 3], f)\n  let r = apply({run}, {cb})!\n  println(\"${{r}}\")",
        cb = n.cb
    );
    refused(&program(&n, &body), &run);
}

#[test]
fn novel_control_the_wrapper_called_by_name_or_alias_with_a_var_free_callback_runs() {
    let n = names();
    let w = &n.w;
    for body in [
        format!("  let r = {w}((x) => ok(x * 2))!\n  println(\"${{r}}\")"),
        format!("  let g = {w}\n  let r = g((x) => ok(x * 2))!\n  println(\"${{r}}\")"),
    ] {
        let src = program(&n, &body);
        let o = almide(&["run"], &src);
        let t = text(&o);
        assert!(o.status.success(), "the control must run:\n{src}\n{t}");
        assert_eq!(String::from_utf8_lossy(&o.stdout), "[2, 4, 6]\n0\n", "{src}\n{t}");
    }
}

#[test]
fn novel_control_a_fn_without_a_concurrent_slot_is_a_value_as_before() {
    let n = names();
    let cb = &n.cb;
    let src = program(&n, &format!("  let hs = [{cb}]\n  println(\"${{list.len(hs)}}\")"));
    let o = almide(&["check"], &src);
    assert!(o.status.success(), "a fn with no concurrent slot is an ordinary value:\n{src}\n{}", text(&o));
}
