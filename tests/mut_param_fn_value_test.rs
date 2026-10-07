//! #3456: a fn with a `mut` parameter cannot be used as a function value
//! (E096). A function type has no place for the write-back a `mut` parameter
//! promises (C-226), so before E096 `let f = bump; f(s)` type-checked, lost
//! the write on wasm (printed 0, not 2) and failed rustc E0596 natively.
//!
//! Every way a name becomes a value is refused here, one cell per shape, and
//! the forms that keep the write-back (a call, UFCS, a pipe, a lambda naming
//! the `var`) still check. The runtime half — those forms write back
//! byte-identically on both targets — is
//! `spec/wasm_cross/mut_param_fn_value_alternatives.almd` (C-226).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const PRELUDE: &str = "type St = { count: Int }\n\
fn bump(mut s: St) -> Unit = {\n  s.count = s.count + 1\n}\n\
fn show(mut x: Int) -> Int = {\n  x = x + 1\n  x\n}\n\
fn St.add(mut self, n: Int) -> Unit = {\n  self.count = self.count + n\n}\n\
fn push_twice[T](mut xs: List[T], x: T) -> Unit = {\n  list.push(xs, x)\n  list.push(xs, x)\n}\n";

/// A package with the given `src/<name>.almd` files; `check src/main.almd`.
fn check_files(files: &[(&str, String)]) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"mutfnv\"\nversion = \"0.1.0\"\n").expect("manifest");
    for (name, body) in files {
        std::fs::write(src.join(format!("{name}.almd")), body).expect("module");
    }
    let out = Command::new(almide()).current_dir(dir.path()).args(["check", "src/main.almd"]).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn check_main(body: &str) -> (bool, String) {
    check_files(&[("main", format!("{PRELUDE}{body}"))])
}

fn assert_e096(cell: &str, (ok, text): (bool, String), name: &str, param: &str) {
    let want = format!("error[E096]: fn '{name}' has a `mut` parameter '{param}' and cannot be used as a function value");
    assert!(!ok && text.contains(&want), "{cell}: expected `{want}`, got:\n{text}");
}

#[test]
fn a_let_binding_is_e096() {
    let r = check_main("effect fn main() -> Unit = {\n  var s = St { count: 0 }\n  let f = bump\n  f(s)\n  println(int.to_string(s.count))\n}\n");
    assert_e096("let", r, "bump", "s");
}

#[test]
fn an_argument_to_a_stdlib_hof_is_e096() {
    let r = check_main("effect fn main() -> Unit = println(list.join([1, 2] |> list.map(show) |> list.map((i) => int.to_string(i)), \",\"))\n");
    assert_e096("stdlib hof argument", r, "show", "x");
}

#[test]
fn an_argument_to_a_user_hof_is_e096() {
    let r = check_main("fn apply(f: (St) -> Unit, s: St) -> Unit = f(s)\n\
effect fn main() -> Unit = {\n  var s = St { count: 0 }\n  apply(bump, s)\n  println(int.to_string(s.count))\n}\n");
    assert_e096("user hof argument", r, "bump", "s");
}

#[test]
fn a_record_field_is_e096() {
    let r = check_main("type H = { f: (St) -> Unit }\n\
effect fn main() -> Unit = {\n  var s = St { count: 0 }\n  let h = H { f: bump }\n  (h.f)(s)\n  println(int.to_string(s.count))\n}\n");
    assert_e096("record field", r, "bump", "s");
}

#[test]
fn a_list_element_is_e096() {
    let r = check_main("effect fn main() -> Unit = {\n  let fs = [bump, bump]\n  println(int.to_string(list.len(fs)))\n}\n");
    assert_e096("list element", r, "bump", "s");
}

#[test]
fn a_return_value_is_e096() {
    let r = check_main("fn pick() -> (St) -> Unit = bump\n\
effect fn main() -> Unit = {\n  var s = St { count: 0 }\n  let f = pick()\n  f(s)\n  println(int.to_string(s.count))\n}\n");
    assert_e096("return", r, "bump", "s");
}

#[test]
fn a_generic_fn_is_e096() {
    let r = check_main("effect fn main() -> Unit = {\n  var xs: List[Int] = []\n  let f = push_twice\n  f(xs, 1)\n  println(int.to_string(list.len(xs)))\n}\n");
    assert_e096("generic", r, "push_twice", "xs");
}

#[test]
fn a_stdlib_member_is_e096_without_the_drop_mut_advice() {
    let (ok, text) = check_main("effect fn main() -> Unit = {\n  var xs = [1]\n  let f = list.push\n  f(xs, 2)\n  println(int.to_string(list.len(xs)))\n}\n");
    assert_e096("stdlib member", (ok, text.clone()), "list.push", "xs");
    assert!(text.contains("`(x) => list.push(xs, x)`"), "stdlib member: the lambda fix-it is missing:\n{text}");
    assert!(!text.contains("drop `mut`"), "stdlib member: a stdlib fn's `mut` cannot be dropped:\n{text}");
}

#[test]
fn a_user_module_member_is_e096() {
    let r = check_files(&[
        ("counter", "type C = { n: Int }\nfn bump(mut c: C) -> Unit = {\n  c.n = c.n + 1\n}\n".to_string()),
        ("main", "import self.counter\n\
effect fn main() -> Unit = {\n  var c = counter.C { n: 0 }\n  let f = counter.bump\n  f(c)\n  println(int.to_string(c.n))\n}\n".to_string()),
    ]);
    assert_e096("module member", r, "counter.bump", "c");
}

/// A method taken by its type (`St.add`) never was a value: it stays refused.
#[test]
fn a_method_value_stays_refused() {
    let (ok, text) = check_main("effect fn main() -> Unit = {\n  var s = St { count: 0 }\n  let f = St.add\n  f(s, 1)\n  println(int.to_string(s.count))\n}\n");
    assert!(!ok && text.contains("error["), "method value must not check, got:\n{text}");
}

/// The hint names the call and the lambda with the fn's own parameters.
#[test]
fn the_hint_names_the_direct_call_and_the_lambda() {
    let (_, text) = check_main("effect fn main() -> Unit = {\n  let f = bump\n  println(\"x\")\n}\n");
    assert!(
        text.contains("Call it directly (`bump(s)` with `var s`), or pass a lambda that names the `var` (`() => bump(s)`)")
            && text.contains("drop `mut` from 's'"),
        "{text}"
    );
}

/// The forms that keep the write-back still check: a call, UFCS, a pipe, the
/// type-qualified method call and a lambda that names the `var`.
#[test]
fn calls_and_lambdas_that_name_the_var_still_check() {
    let (ok, text) = check_main("effect fn main() -> Unit = {\n  var s = St { count: 0 }\n  bump(s)\n  s.add(1)\n  St.add(s, 1)\n  s |> bump\n\
  let tick = () => bump(s)\n  tick()\n  var xs: List[Int] = []\n  push_twice(xs, 1)\n  let p = (x: Int) => push_twice(xs, x)\n  p(2)\n\
  let ys = [1, 2] |> list.map((n) => { s.add(n); n })\n  println(\"${int.to_string(s.count)} ${int.to_string(list.len(xs) + list.len(ys))}\")\n}\n");
    assert!(ok, "the call forms must check, got:\n{text}");
}
