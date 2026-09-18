//! A fn-typed param is borrowed unless its callable ESCAPES the call (#2288):
//! a user higher-order fn that only calls its callback takes it as
//! `&dyn Fn(A) -> B`, and a lambda literal handed to that slot is a scope —
//! passed as `&|x| …`, no `Rc::new`, no `move`, no capture bind, the outer
//! params it reads staying borrowed. A callback that is returned, stored in a
//! record, captured by a closure that outlives the call, or handed to a
//! runtime twin's owned slot keeps the `Rc<dyn Fn>` handle. A closure VALUE
//! at a borrowed slot is lent through its handle (`&*g`); a tail-recursive fn
//! carries the borrowed callable through its loop — unless a self-call REBINDS
//! the slot to a new callable (a CPS accumulator): that closure is built inside
//! one loop iteration and cannot outlive it, so the slot keeps the handle. Both
//! legs print the same thing.
use std::process::Command;

const PROGRAM: &str = r#"type Box = { f: (Int) -> Int }

fn apply(xs: List[Int], f: (Int) -> Int) -> List[Int] = xs |> list.map((x) => f(x))

fn dbl(x: Int) -> Int = x * 2

fn scaled(xs: List[Int], k: Int) -> List[Int] = apply(xs, (x) => x * k)

fn via_value(xs: List[Int], k: Int) -> List[Int] = {
  let g = (x) => x + k
  apply(xs, g)
}

fn forward(xs: List[Int], f: (Int) -> Int) -> List[Int] = apply(xs, f)

fn walk(f: (Int) -> Int, n: Int) -> Int = if n <= 0 then f(0) else walk(f, n - 1)

fn twice_call(f: (Int) -> Int, x: Int) -> Int = f(f(x))

fn apply_str(xs: List[String], f: (String) -> String) -> List[String] = xs |> list.map((x) => f(x))

fn suffixed(names: List[String], tag: String) -> List[String] = apply_str(names, (n) => n + tag)

fn stored(f: (Int) -> Int) -> Box = Box { f: f }

fn captured(f: (Int) -> Int) -> (Int) -> Int = (x) => f(x) + 1

fn unfused(xs: List[Int], f: (Int) -> Int) -> List[Int] = list.map(xs, f)

fn keep(f: (Int) -> Int) -> (Int) -> Int = f

fn cps(f: (Int) -> Int, n: Int) -> Int = if n <= 0 then f(0) else cps((x) => f(x + n), n - 1)

fn main() -> Unit = {
  let xs = [1, 2, 3]
  println("${scaled(xs, 10) |> list.sum} ${via_value(xs, 10) |> list.sum} ${forward(xs, dbl) |> list.sum} ${walk(dbl, 3)} ${twice_call(dbl, 3)}")
  println(list.join(suffixed(["a", "b"], "!"), ","))
  println("${(stored(dbl)).f(4)} ${captured(dbl)(5)} ${unfused(xs, dbl) |> list.sum} ${keep(dbl)(6)} ${cps(dbl, 3)}")
}
"#;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_sig_and_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    rust[start..].split("\n}").next().unwrap()
}

#[test]
fn a_fn_typed_param_is_borrowed_unless_its_callable_escapes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).into_owned();

    // The callee only calls its callback: a borrowed callable, called
    // directly (no `.clone()` of a handle).
    for (name, sig) in [
        ("apply", "pub fn apply(xs: &[i64], f: &dyn Fn(i64) -> i64)"),
        ("forward", "pub fn forward(xs: &[i64], f: &dyn Fn(i64) -> i64)"),
        ("twice_call", "pub fn twice_call(f: &dyn Fn(i64) -> i64, x: i64)"),
        ("apply_str", "pub fn apply_str(xs: Vec<String>, f: &dyn Fn(String) -> String)"),
        ("walk", "pub fn walk(mut f: &dyn Fn(i64) -> i64, mut n: i64)"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig), "{name}:\n{s}");
        assert!(!s.contains("Rc::new") && !s.contains("f.clone())("), "{name} must call the borrowed callable directly:\n{s}");
    }
    // A lambda literal at that slot is a scope: `&|x| …`, no box, no `move`,
    // no capture bind — and the outer param it reads stays borrowed.
    let s = fn_sig_and_body(&rust, "scaled");
    assert!(s.starts_with("pub fn scaled(xs: &[i64], k: i64)") && s.contains("apply(xs, &|x|") && !s.contains("move") && !s.contains("Rc::new"), "{s}");
    let s = fn_sig_and_body(&rust, "suffixed");
    assert!(s.starts_with("pub fn suffixed(names: Vec<String>, tag: &str)") && s.contains("apply_str(names, &|n|") && !s.contains("__cap_") && !s.contains("Rc::new"), "{s}");
    // A closure value is lent through its handle.
    let s = fn_sig_and_body(&rust, "via_value");
    assert!(s.contains("apply(xs, &(*g))"), "{s}");

    // The callable escapes: returned, stored, captured by an outliving
    // closure, handed to a runtime twin — the handle stays.
    for (name, sig) in [
        ("stored", "pub fn stored(f: std::rc::Rc<dyn Fn(i64) -> i64>)"),
        ("captured", "pub fn captured(f: std::rc::Rc<dyn Fn(i64) -> i64>)"),
        ("unfused", "pub fn unfused(xs: Vec<i64>, f: std::rc::Rc<dyn Fn(i64) -> i64>)"),
        ("keep", "pub fn keep(f: std::rc::Rc<dyn Fn(i64) -> i64>)"),
        ("cps", "pub fn cps(mut f: std::rc::Rc<dyn Fn(i64) -> i64>, mut n: i64)"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig), "{name}:\n{s}");
    }

    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().unwrap();
        assert!(run.status.success(), "{target}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "60 36 12 0 12\na!,b!\n8 11 12 12 12", "{target}");
    }
}
