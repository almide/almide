//! A borrowed string keeps its borrow where the body only reads it, and
//! converts where the body stores it — the two flows #2188 and #2189 broke.
//!
//! * A loop binder the body only compares is bound `&String` (`.iter()`,
//!   #1673); its `as_str` view must reach `&str` (`c.as_str()`), because
//!   `&*c` is `&String` and `&String >= &str` has no `PartialOrd` (#2188).
//! * A `var` re-assigned from a borrowed param stores an OWNED value, so the
//!   `&str` / `&[T]` converts exactly as a `let` initializer's does (#2189,
//!   the `Assign` twin of #624).
//!
//! Both shapes only reach rustc on the v0 leg — the v1 native render walls
//! on `count_above`'s `List[String]` param — so the run is pinned to that
//! leg explicitly as well as taken through the default route and wasm.
use std::process::Command;

const PROGRAM: &str = r#"fn has_digit(s: String) -> Bool = {
  var out = false
  for c in string.chars(s) {
    let hit = c >= "0" and c <= "9"
    if hit then { out = true } else ()
  }
  out
}

fn count_above(xs: List[String], floor: String) -> Int = {
  var n = 0
  for x in xs {
    if x > floor then { n = n + 1 } else ()
  }
  n
}

fn classify(xs: List[String]) -> Int = {
  var n = 0
  for x in xs {
    n = n + match x { "a" => 1, "b" => 10, _ => 100 }
  }
  n
}

fn starts_upper(xs: List[String]) -> Int = {
  var n = 0
  for x in xs {
    if string.starts_with(x, "A") then { n = n + 1 } else ()
  }
  n
}

fn keep(lit: String) -> String = {
  var model = ""
  if lit.len() > 0 then { model = lit } else ()
  model
}

fn keep_list(xs: List[Int]) -> List[Int] = {
  var out: List[Int] = []
  if list.len(xs) > 0 then { out = xs } else ()
  out
}

fn first_upper(s: String) -> String = {
  var found = ""
  for c in string.chars(s) {
    if found == "" and c >= "A" and c <= "Z" then { found = c } else ()
  }
  found
}

effect fn main() -> Unit = {
  println(if has_digit("a1b") then "digit" else "none")
  println(if has_digit("abc") then "digit" else "none")
  println(int.to_string(count_above(["b", "z", "a"], "m")))
  println(int.to_string(classify(["a", "b", "c"])))
  println(int.to_string(starts_upper(["Ab", "cd", "Ax"])))
  println(keep("openai/gpt-4o"))
  println(keep(""))
  println(int.to_string(list.len(keep_list([1, 2, 3]))))
  println(first_upper("abCdE"))
}
"#;

const EXPECTED: &str = "digit\nnone\n1\n111\n2\nopenai/gpt-4o\n\n3\nC";

/// #2194: a heap-typed `let`-bound `if` inside the loop body is lifted into a
/// tail helper (`optimize/branch_lift.rs`) whose params KEEP the enclosing
/// fn's VarIds — so the borrowed loop binder `key` reappears there as a `&str`
/// param. The `as_str` view keyed on `borrowed_loop_vars` alone rendered
/// `key.as_str()` on that param: `str::as_str` is unstable (E0658) on stable
/// rustc. Both heads (a list literal and a `List[String]` param) and both
/// shapes the reporter combined (`var` re-assigned under `if key == …` next
/// to the lifted `let`).
const LIFTED_PROGRAM: &str = r#"fn kinds_of_literal() -> List[String] = {
  var out: List[String] = []
  for key in ["tools", "context", "agents", "tasks"] {
    let child_kind = if key == "agents" then "agent" else "task"
    out = out + [child_kind]
  }
  out
}

fn kinds_of(keys: List[String]) -> List[String] = {
  var out: List[String] = []
  for key in keys {
    var relation = "uses"
    if key == "context" then { relation = "context" } else ()
    let child_kind = if key == "agents" then "agent" else "task"
    out = out + [child_kind + ":" + relation]
  }
  out
}

effect fn main() -> Unit = {
  println(kinds_of_literal() |> list.join(","))
  println(kinds_of(["tools", "context", "agents"]) |> list.join(","))
}
"#;

const LIFTED_EXPECTED: &str = "task,task,agent,task\ntask:uses,task:context,agent:uses";

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    rust.split(&format!("pub fn {name}(")).nth(1).unwrap_or_else(|| panic!("no fn {name}")).split("\n}").next().unwrap()
}

#[test]
fn borrowed_loop_binder_compares_as_str_and_borrowed_param_stores_owned() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, PROGRAM).unwrap();
    let bin = almide_bin();

    let emitted = Command::new(&bin).arg("emit").arg(&source).output().unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8(emitted.stdout).unwrap();
    // The read-only binders keep the per-element borrow: no clone per char.
    for name in ["has_digit", "count_above", "starts_upper"] {
        let body = fn_body(&rust, name);
        assert!(body.contains(".iter()") && !body.contains(".cloned()"), "{name} lost its borrowed binder: {body}");
        assert!(!body.contains(".clone()") && !body.contains(".to_string()"), "{name} clones: {body}");
    }
    // The stores convert the borrow into the owned binding's type.
    assert!(fn_body(&rust, "keep").contains("model = lit.to_string();"), "{}", fn_body(&rust, "keep"));
    assert!(fn_body(&rust, "keep_list").contains("out = xs.to_vec();"), "{}", fn_body(&rust, "keep_list"));

    run_on_every_leg(&bin, &source, EXPECTED);
}

/// `almide run` on the default native route, the pinned v0 leg, and wasm.
fn run_on_every_leg(bin: &str, source: &std::path::Path, expected: &str) {
    let runs: [(&str, &[&str], &[(&str, &str)]); 3] = [
        ("native", &[], &[]),
        ("native v0", &["--no-verified"], &[("ALMIDE_NO_VERIFIED_OK", "1")]),
        ("wasm", &["--target", "wasm"], &[]),
    ];
    for (label, args, envs) in runs {
        let mut cmd = Command::new(bin);
        cmd.arg("run").arg(source).args(args);
        for (k, v) in envs { cmd.env(k, v); }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{label}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim_end(), expected, "{label}");
    }
}

/// The lift runs in the shared optimizer (`build` / `run`), not in `emit`, so
/// the evidence is the build itself: on a22cfb7f the native leg failed rustc
/// with E0658 `str_as_str` on the helper's `key.as_str()`.
#[test]
fn borrowed_loop_binder_lifted_into_branch_helper_compares_without_as_str() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, LIFTED_PROGRAM).unwrap();
    run_on_every_leg(&almide_bin(), &source, LIFTED_EXPECTED);
}
