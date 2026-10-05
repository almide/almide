//! #3413: a list pattern nested below a custom-variant constructor pattern
//! (`Op(_, [_, b])`, a boxed recursive payload `Pair(Op(_, [x]), _)`, a
//! record-payload field, a tuple element's case) panicked the native build
//! with a ListPatternLowering postcondition violation: the length-test chain
//! only reached a list pattern at the top of an arm. Guards here read the
//! list's binders and the outer binders together. Kept OUT of the spec corpus
//! on purpose: the incumbent brick walls a custom-variant match on a list
//! payload, and a corpus function it walls is a walled-real baseline entry —
//! this runs both legs directly instead (the option-payload forms are pinned
//! by spec/wasm_cross/list_pattern_in_option_payload.almd).

use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

const PROGRAM: &str = r#"type Expr =
  | Op(Int, List[Expr])
  | Lit(Int)
  | Pair(Expr, Expr)
  | Named { label: String, args: List[Int] }

type Bag = | Bag(List[Int])

fn arity(e: Expr) -> Int = match e {
  Op(_, []) => 0,
  Op(k, [Lit(x)]) if x > k => 10 + x,
  Op(_, [_, Lit(b)]) => 200 + b,
  Op(_, [Lit(a), ..rest]) if list.len(rest) > 1 => 300 + a,
  Op(_, _) => -1,
  Lit(n) => n,
  Pair(_, _) => -2,
  Named { label: l, args: [first, ..] } if string.len(l) > 1 => first,
  Named { .. } => -3,
}

fn deep(e: Expr) -> Int = match e {
  Pair(Op(_, [Lit(b)]), _) => b,
  Pair(Op(k, [_, Lit(c)]), Lit(d)) if c + d > k => c + d,
  _ => 0,
}

fn in_tuple(e: Expr, n: Int) -> Int = match (e, n) {
  (Op(_, [Lit(a), Lit(b)]), 1) => a + b,
  (Op(k, [Lit(a)]), m) if a > m => k,
  _ => -1,
}

fn bag(b: Bag) -> Int = match b {
  Bag([]) => 0,
  Bag([x, ..]) => x,
}

fn after_guard(xs: List[Int], skip: Bool) -> Int = match xs {
  [] => 0,
  _ if skip => -1,
  [x] => x,
  _ => 99,
}

fn main() -> Unit = {
  println(int.to_string(arity(Op(1, []))))
  println(int.to_string(arity(Op(1, [Lit(5)]))))
  println(int.to_string(arity(Op(9, [Lit(5)]))))
  println(int.to_string(arity(Op(1, [Lit(5), Lit(6)]))))
  println(int.to_string(arity(Op(1, [Lit(7), Lit(6), Lit(5)]))))
  println(int.to_string(arity(Op(1, [Op(2, []), Lit(6), Lit(5)]))))
  println(int.to_string(arity(Lit(4))))
  println(int.to_string(arity(Named { label: "ab", args: [6, 7] })))
  println(int.to_string(arity(Named { label: "a", args: [6, 7] })))
  println(int.to_string(deep(Pair(Op(0, [Lit(8)]), Lit(1)))))
  println(int.to_string(deep(Pair(Op(5, [Lit(0), Lit(3)]), Lit(4)))))
  println(int.to_string(deep(Pair(Op(9, [Lit(0), Lit(3)]), Lit(4)))))
  println(int.to_string(in_tuple(Op(0, [Lit(2), Lit(3)]), 1)))
  println(int.to_string(in_tuple(Op(6, [Lit(2)]), 1)))
  println(int.to_string(in_tuple(Op(6, [Lit(2)]), 5)))
  println(int.to_string(bag(Bag([]))))
  println(int.to_string(bag(Bag([12, 1]))))
  println(int.to_string(after_guard([], true)))
  println(int.to_string(after_guard([4], true)))
  println(int.to_string(after_guard([4], false)))
  println(int.to_string(after_guard([4, 5], false)))
}
"#;

const EXPECTED: &str = "0\n15\n-1\n206\n307\n-1\n4\n6\n-3\n8\n7\n0\n5\n6\n-1\n0\n12\n0\n-1\n4\n99\n";

fn run(target: Option<&str>) -> (i32, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("nested_list_variant.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(&src);
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("spawn almide run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn list_patterns_below_a_variant_lower_on_the_native_leg() {
    let (code, stdout, stderr) = run(None);
    assert_eq!(code, 0, "native run failed; stderr:\n{stderr}");
    assert_eq!(stdout, EXPECTED);
}

#[test]
fn list_patterns_below_a_variant_lower_on_the_wasm_leg() {
    let (code, stdout, stderr) = run(Some("wasm"));
    if stderr.contains("wasmtime") && code != 0 && stdout.is_empty() {
        eprintln!("skipping wasm leg: no wasm host here\n{stderr}");
        return;
    }
    assert_eq!(code, 0, "wasm run failed; stderr:\n{stderr}");
    assert_eq!(stdout, EXPECTED);
}
