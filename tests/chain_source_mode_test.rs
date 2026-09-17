//! A fused chain's source is borrowed or consumed by what the steps do with
//! each ELEMENT, not by the combinator's `@consume(xs)` slot (#2287): a
//! `Copy` element, or a heap element every receiving lambda only reads,
//! leaves the source param `&[T]` and iterates it from a borrow — `.iter()`
//! with `&T` binders when no element is ever cloned, `.iter().cloned()` when
//! a `for` body still copies its binder; an element that is returned, built
//! into the collected list or concatenated keeps the source owned, so the
//! last-use caller moves and the chain moves each element out for free. A
//! `for` loop over a list is the same iteration and follows the same rule.
//! Both legs print the same thing.
use std::process::Command;

const PROGRAM: &str = r#"fn mapped(ns: List[Int]) -> List[Int] = ns |> list.map((n) => n * 2)

fn total(ns: List[Int]) -> Int = ns |> list.fold(0, (acc, n) => acc + n)

fn twice(xs: List[Int]) -> Int = list.len(mapped(xs)) + list.len(mapped(xs))

fn shouted(ws: List[String]) -> List[String] = ws |> list.map((w) => w + "!")

fn echo(ws: List[String]) -> List[String] = ws |> list.map((w) => w)

fn lens(ws: List[String]) -> List[Int] = ws |> list.map((w) => string.len(w))

fn interp(ws: List[String]) -> List[String] = ws |> list.map((w) => "<${w}>")

fn size(s: String) -> Int = string.len(s)

fn via(ws: List[String]) -> List[Int] = ws |> list.map((w) => size(w))

fn long(ws: List[String]) -> List[String] = ws |> list.filter((w) => string.len(w) > 1)

fn kept(ws: List[String]) -> Int = ws |> list.filter((w) => string.len(w) > 1) |> list.len

fn has(ws: List[String], t: String) -> Bool = ws |> list.any((w) => w == t)

fn loop_len(ws: List[String]) -> Int = {
  var n = 0
  for w in ws { n = n + string.len(w) }
  n
}

fn joined(ws: List[String]) -> String = {
  var s = ""
  for w in ws { s = s + w }
  s
}

fn main() -> Unit = {
  let ws = ["ab", "c"]
  println(int.to_string(twice([1, 2, 3]) + total([1, 2, 3])))
  println(list.join(shouted(ws), ",") + list.join(echo(ws), ","))
  println(int.to_string(list.sum(lens(ws)) + list.sum(via(ws)) + kept(ws) + loop_len(ws)))
  println(list.join(interp(ws), "") + list.join(long(ws), "") + joined(ws))
  println(if has(ws, "c") and not has(ws, "d") then "true" else "false")
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
fn a_chain_source_is_borrowed_unless_a_step_needs_the_element_owned() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).into_owned();

    // `Copy` elements: the source is a borrow whatever the step does, and a
    // caller that reads its list twice clones nothing.
    for (name, sig, iter) in [
        ("mapped", "pub fn mapped(ns: &[i64])", "(ns).iter().cloned()"),
        ("total", "pub fn total(ns: &[i64])", "(ns).iter().cloned()"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig) && s.contains(iter), "{name}:\n{s}");
    }
    let s = fn_sig_and_body(&rust, "twice");
    assert!(s.starts_with("pub fn twice(xs: &[i64])") && !s.contains("clone"), "{s}");

    // Heap elements every step only reads: the source is a borrow and the
    // steps bind `&T` — no element is cloned, not even for `format!`.
    for (name, sig) in [
        ("lens", "pub fn lens(ws: &[String])"),
        ("interp", "pub fn interp(ws: &[String])"),
        ("via", "pub fn via(ws: &[String])"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig) && s.contains("(ws).iter().map("), "{name}:\n{s}");
        assert!(!s.contains("clone"), "{name} must not clone an element it only reads:\n{s}");
    }
    // A filter-family step off `.iter()` receives `&&T`: its prepared
    // rebinding derefs once instead of cloning.
    let s = fn_sig_and_body(&rust, "kept");
    assert!(s.starts_with("pub fn kept(ws: &[String])") && s.contains("(ws).iter().filter(") && s.contains("= (*w);"), "{s}");
    assert!(!s.contains("clone"), "{s}");
    let s = fn_sig_and_body(&rust, "has");
    assert!(s.starts_with("pub fn has(ws: &[String], t: &str)") && s.contains("(ws).iter().any(") && s.contains("w.as_str()"), "{s}");

    // An element that leaves the chain owned — concatenated, returned as the
    // mapped value, collected by a filter — keeps the source consumed: the
    // caller's last use moves the list and each element moves out for free.
    for (name, sig) in [
        ("shouted", "pub fn shouted(ws: Vec<String>)"),
        ("echo", "pub fn echo(ws: Vec<String>)"),
        ("long", "pub fn long(ws: Vec<String>)"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig) && s.contains("(ws).into_iter()"), "{name}:\n{s}");
    }

    // A `for` loop is the same iteration: a body that only reads its binder
    // borrows the list; one that concatenates it needs it owned.
    let s = fn_sig_and_body(&rust, "loop_len");
    assert!(s.starts_with("pub fn loop_len(ws: &[String])") && s.contains("for w in ws.iter() {"), "{s}");
    let s = fn_sig_and_body(&rust, "joined");
    assert!(s.starts_with("pub fn joined(ws: Vec<String>)"), "{s}");

    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().unwrap();
        assert!(run.status.success(), "{target}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "12\nab!,c!ab,c\n10\n<ab><c>ababc\ntrue", "{target}");
    }
}
