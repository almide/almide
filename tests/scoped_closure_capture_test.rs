//! A `let`-bound closure that cannot outlive its fn and only reads what it
//! captures is a SCOPE (#3455): it renders as the non-`move` `&|..| ..`, the
//! params it reads stay borrowed, and no caller clones its argument to make
//! the call. Called inside a fused fold / map / filter, called directly,
//! sharing a capture with a sibling closure, reading a record param's field,
//! or lent to a user fn's `&dyn Fn` slot — all borrowed. A closure that is
//! returned, stored in a list or record, handed to `fan`, that mutates a
//! captured `var`, or whose capture is moved after it, keeps the owned
//! `move` capture it always had. Both legs print the same thing; the
//! perf cell pins that a call no longer copies the list.
use std::process::Command;

const PROGRAM: &str = r#"type Cfg = { base: Int, name: String }
type Holder = { run: (Int) -> Int }

fn via_closure(xs: List[Int], n: Int) -> Int = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  list.range(0, n) |> list.fold(0, (a, i) => a + f(i))
}

fn in_map(xs: List[Int], n: Int) -> List[Int] = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  list.range(0, n) |> list.map((i) => f(i) * 2)
}

fn in_filter(xs: List[Int], n: Int) -> List[Int] = {
  let keep = (i: Int) => (list.get(xs, i) ?? 0) > 2
  list.range(0, n) |> list.filter((i) => keep(i))
}

fn direct(xs: List[Int]) -> Int = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  f(0) + f(1) + f(2)
}

fn shared(xs: List[Int], n: Int) -> Int = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  let g = (i: Int) => list.len(xs) - i
  list.range(0, n) |> list.fold(0, (a, i) => a + f(i) + g(i))
}

fn field(c: Cfg, n: Int) -> Int = {
  let h = (i: Int) => c.base + i + string.len(c.name)
  h(n) + h(n + 1)
}

fn apply(g: (Int) -> Int, x: Int) -> Int = g(x) + g(x + 1)

fn lent(xs: List[Int]) -> Int = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  apply(f, 1)
}

fn returned(xs: List[Int]) -> (Int) -> Int = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  f
}

fn stored_list(xs: List[Int]) -> Int = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  let fs = [f, f]
  fs |> list.fold(0, (a, g) => a + g(1))
}

fn stored_record(xs: List[Int]) -> Holder = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  { run: f }
}

effect fn fanned(xs: List[Int]) -> List[Int] = {
  let f = (i: Int) => ok(list.get(xs, i) ?? 0)
  fan.map([0, 1, 2], f)!
}

effect fn mutating(xs: List[Int]) -> Int = {
  var acc = 0
  let add = (i: Int) => { acc = acc + (list.get(xs, i) ?? 0) }
  add(0)
  add(1)
  acc
}

fn moved_after(xs: List[Int]) -> List[Int] = {
  let f = (i: Int) => list.get(xs, i) ?? 0
  let n = f(0)
  xs + [n]
}

effect fn main() -> Unit = {
  let xs = [1, 2, 3, 4, 5]
  let c = { base: 10, name: "c" }
  let show = (ys: List[Int]) => list.join(ys |> list.map((v) => int.to_string(v)), ",")
  println("${via_closure(xs, 3)} ${show(in_map(xs, 3))} ${show(in_filter(xs, 5))} ${direct(xs)} ${shared(xs, 3)} ${field(c, 1)} ${lent(xs)}")
  println("${returned(xs)(2)} ${stored_list(xs)} ${stored_record(xs).run(3)} ${show(fanned(xs)!)} ${mutating(xs)!} ${show(moved_after(xs))}")
  println("${list.len(xs)} ${c.name}")
}
"#;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn fn_sig_and_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust.find(&format!("pub fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    rust[start..].split("\n}").next().unwrap()
}

fn emit(src: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_non_escaping_read_only_bound_closure_borrows_what_it_captures() {
    let rust = emit(PROGRAM);
    // The scope: the param stays borrowed, the closure is the non-`move`
    // `&|..| ..` with no capture bind, and calling it clones no handle.
    for (name, sig) in [
        ("via_closure", "pub fn via_closure(xs: &[i64], n: i64)"),
        ("in_map", "pub fn in_map(xs: &[i64], n: i64)"),
        ("in_filter", "pub fn in_filter(xs: &[i64], n: i64)"),
        ("direct", "pub fn direct(xs: &[i64])"),
        ("shared", "pub fn shared(xs: &[i64], n: i64)"),
        ("field", "pub fn field(c: &Cfg, n: i64)"),
        ("lent", "pub fn lent(xs: &[i64])"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig), "{name}:\n{s}");
        assert!(s.contains(": _ = &|") && !s.contains("move |") && !s.contains("__cap_") && !s.contains("Rc::new"), "{name} must bind a borrowing scope closure:\n{s}");
        assert!(!s.contains(".clone())("), "{name} must call the scope closure directly:\n{s}");
    }
    // Two closures sharing one capture both borrow it.
    let s = fn_sig_and_body(&rust, "shared");
    assert_eq!(s.matches(": _ = &|i: i64|").count(), 2, "{s}");

    // The closure escapes, or its capture cannot stay borrowed: the owned
    // `move` capture stays, and so does the owned param.
    for (name, sig) in [
        ("returned", "pub fn returned(xs: Vec<i64>)"),
        ("stored_list", "pub fn stored_list(xs: Vec<i64>)"),
        ("stored_record", "pub fn stored_record(xs: Vec<i64>)"),
        ("fanned", "pub fn fanned(xs: Vec<i64>)"),
        ("mutating", "pub fn mutating(xs: Vec<i64>)"),
        ("moved_after", "pub fn moved_after(xs: Vec<i64>)"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig), "{name}:\n{s}");
        assert!(s.contains("move |") && !s.contains(": _ = &|"), "{name} must keep the owned capture:\n{s}");
    }

    // The caller passes the borrowed scopes' list without a clone; the
    // escaping ones still clone it, since `xs` stays live.
    let main = fn_sig_and_body(&rust, "__almide_main");
    for name in ["via_closure", "in_map", "in_filter", "direct", "shared", "lent"] {
        assert!(main.contains(&format!("{name}(&xs")), "{name} call must lend xs:\n{main}");
    }
    assert!(main.contains("field(&c, "), "{main}");
    for name in ["returned", "stored_list", "stored_record", "moved_after"] {
        assert!(main.contains(&format!("{name}(xs.clone()")), "{name} call keeps its clone:\n{main}");
    }

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().unwrap();
        assert!(run.status.success(), "{target}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(
            String::from_utf8_lossy(&run.stdout).trim(),
            "6 2,4,6 2,3,4 6 18 25 5\n3 4 4 1,2,3 3 1,2,3,4,5,1\n5 c",
            "{target}"
        );
    }
}

/// The ablation restores the owned capture: the verdict above is this rule's.
#[test]
fn the_ablation_restores_the_owned_capture() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust")
        .env("ALMIDE_SCOPED_CLOSURE_OFF", "1").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).into_owned();
    let s = fn_sig_and_body(&rust, "via_closure");
    assert!(s.starts_with("pub fn via_closure(xs: Vec<i64>, n: i64)") && s.contains("move |"), "{s}");
}

/// The perf cell: a 200k-element list, the closure called through `N` and
/// `4N` calls of the fn holding it. Each call used to clone the list into the
/// closure's owned capture, so the allocation count grew with the calls; a
/// scope closure borrows it, so the count does not move.
#[test]
fn a_call_through_a_scope_closure_does_not_copy_the_list() {
    let prog = |calls: i64| format!(r#"fn via_closure(xs: List[Int], n: Int) -> Int = {{
  let f = (i: Int) => list.get(xs, i) ?? 0
  f(n) + f(n + 1)
}}

fn main() -> Unit = {{
  let xs = list.range(0, 200000)
  var t = 0
  var k = 0
  while k < {calls} {{
    t = t + via_closure(xs, k)
    k = k + 1
  }}
  println("${{t}}")
}}
"#);
    let allocs = |calls: i64| -> u64 {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cell.almd");
        std::fs::write(&file, prog(calls)).unwrap();
        let out = Command::new(almide_bin()).arg("run").arg(&file).env("ALMIDE_ALLOC_COUNT", "1").output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no alloc line:\n{stderr}"));
        line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).unwrap().parse().unwrap()
    };
    let (n, n4) = (allocs(500), allocs(2000));
    assert_eq!(n, n4, "allocations grew with the calls ({n} at 500, {n4} at 2000): a call copies the list");
}
