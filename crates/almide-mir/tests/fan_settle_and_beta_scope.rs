//! #3465: two MIR desugars that moved a `!` out of its scope.
//!
//! - `desugar_beta_reduce` inlined `((x) => f(x)!)(a)`, so the `!` exited the
//!   ENCLOSING fn instead of yielding the lambda's Err. It now declines, and
//!   the application walls.
//! - `desugar_fan_block` chained `let $v = arm!` binds, so an Err in arm 1
//!   skipped arm 2. C-199 (ADR-0024) says every arm runs and the lowest-index
//!   Err wins: the arms now all run, then the `!`s settle in list order.

fn lowered(src: &str) -> Vec<almide_mir::MirFunction> {
    almide_mir::pipeline::lowered_functions(src).expect("reaches the MIR lowering")
}

fn ops_of(funcs: &[almide_mir::MirFunction], name: &str) -> Vec<String> {
    let f = funcs.iter().find(|f| f.name.as_str() == name).unwrap_or_else(|| panic!("`{name}` did not lower"));
    assert!(almide_mir::verify_ownership(f).is_ok(), "`{name}` fails ownership verification");
    f.ops.iter().map(|op| format!("{op:?}")).collect()
}

/// The op index of the call to `callee`, and its result ValueId as printed.
fn call_of(ops: &[String], callee: &str) -> (usize, String) {
    let needle = format!("name: \"{callee}\"");
    let i = ops.iter().position(|o| o.starts_with("CallFn") && o.contains(&needle))
        .unwrap_or_else(|| panic!("no call to `{callee}`:\n{ops:#?}"));
    let dst = ops[i].split("dst: Some(").nth(1).and_then(|r| r.split(')').next()).expect("call has a dst");
    (i, format!("{dst})"))
}

const CHECK: &str = "effect fn check(x: Int) -> Int = {\n  if x > 0 then err(\"bad\") else ok(x)\n}\n";

#[test]
fn beta_reduce_declines_a_lambda_that_propagates() {
    // Inlined, the `!` exits `run` with "bad" and skips the match and the print.
    let src = format!(
        "{CHECK}effect fn run() -> Int = {{\n  let m = match ((x) => check(x)!)(5) {{\n    ok(v) => v,\n    err(_) => 7,\n  }}\n  println(\"after lambda m=${{m}}\")\n  ok(m)\n}}\n\
         effect fn main() -> Unit = {{\n  match run() {{\n    ok(v) => println(\"v=${{v}}\"),\n    err(e) => println(\"run failed: ${{e}}\"),\n  }}\n}}\n"
    );
    let funcs = lowered(&src);
    assert!(
        !funcs.iter().any(|f| f.name.as_str() == "run"),
        "`run` lowered with the lambda's `!` inlined into it; it must wall"
    );
}

#[test]
fn beta_reduce_still_inlines_a_lambda_without_bang() {
    let src = "fn run(n: Int) -> Int = ((x) => x + 1)(n)\nfn main() -> Unit = println(\"${run(4)}\")\n";
    let ops = ops_of(&lowered(src), "run");
    assert!(!ops.iter().any(|o| o.contains("Computed")), "the application was not inlined:\n{ops:#?}");
}

#[test]
fn fan_runs_every_arm_before_the_first_err_exits() {
    // `first` errs; `second` prints. C-199: `second` still runs.
    let src = format!(
        "{CHECK}effect fn second(x: Int) -> Int = {{\n  println(\"second ran\")\n  ok(x + 1)\n}}\n\
         effect fn main() -> Unit = {{\n  let (a, b) = fan {{ check(1), second(2) }}\n  println(\"${{a}} ${{b}}\")\n}}\n"
    );
    let ops = ops_of(&lowered(&src), "main");
    let (second_at, _) = call_of(&ops, "second");
    let die_at = ops.iter().position(|o| o.contains("kind: Die")).expect("main has its err exit");
    assert!(second_at < die_at, "arm 2 runs only after arm 1's Err exit was taken:\n{ops:#?}");
}

#[test]
fn fan_settles_the_lowest_index_err_first() {
    // Both Result arms err; the plain middle arm runs between them. Every call
    // precedes the first tag test, and arm 1's Result is tested first. (In `main`:
    // a non-main fn destructuring the fan walls on its tail match today.)
    let src = format!(
        "{CHECK}fn plain(x: Int) -> Int = x * 2\n\
         effect fn other(x: Int) -> Int = {{\n  if x > 0 then err(\"other\") else ok(x)\n}}\n\
         effect fn main() -> Unit = {{\n  let (a, b, c) = fan {{ check(1), plain(2), other(3) }}\n  println(\"${{a + b + c}}\")\n}}\n"
    );
    let ops = ops_of(&lowered(&src), "main");
    let (check_at, check_v) = call_of(&ops, "check");
    let (other_at, other_v) = call_of(&ops, "other");
    let reads = |v: &str| {
        ops.iter()
            .position(|o| o.starts_with("Prim { kind: Handle") && o.contains(&format!("args: [{v}]")))
            .unwrap_or_else(|| panic!("{v} is never tested:\n{ops:#?}"))
    };
    let (check_tested, other_tested) = (reads(&check_v), reads(&other_v));
    assert!(check_at < other_at, "arms run out of list order:\n{ops:#?}");
    assert!(other_at < check_tested, "arm 1 is settled before arm 3 ran:\n{ops:#?}");
    assert!(check_tested < other_tested, "arm 3's Err would win over arm 1's:\n{ops:#?}");
}
