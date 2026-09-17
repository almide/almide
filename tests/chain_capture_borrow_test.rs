//! A list-combinator callback the stream-fusion pass inlines is a SCOPE, not a
//! closure (#2278, the residue of #2069): fusion runs before the borrow pass,
//! so a `&T` / `&str` / `&[T]` param the callback only reads stays borrowed,
//! and the chain step renders without `move` and without a `__cap` bind —
//! it borrows what it reads for the chain's duration. A closure that
//! outlives its call (returned, handed to a user fn's fn-typed slot, the
//! fallible `!` twin) still captures, and its captures still own. Both legs
//! print the same thing.
use std::process::Command;

const PROGRAM: &str = r#"type Table = { names: List[String], sizes: List[Int] }

fn mapped(t: Table) -> List[Int] = list.map(t.sizes, (x) => x + list.len(t.names))

fn filtered(t: Table, k: Int) -> List[String] = list.filter(t.names, (n) => string.len(n) > k)

fn folded(t: Table) -> Int = list.fold(t.sizes, 0, (a, x) => a + x * list.len(t.names))

fn any_long(t: Table, k: Int) -> Bool = list.any(t.names, (n) => string.len(n) > k)

fn sliced(xs: List[Int], ys: List[Int]) -> List[Int] = list.map(ys, (y) => y + list.len(xs))

fn prefixed(p: String, xs: List[String]) -> List[String] = list.map(xs, (x) => p + x)

fn each(t: Table) -> List[Table] = list.map(t.sizes, (x) => t)

fn saved(t: Table) -> (Int) -> Int = (i) => i + list.len(t.names)

fn twice(f: (Int) -> Int, x: Int) -> Int = f(f(x))

fn applied(t: Table) -> Int = twice((i) => i + list.len(t.names), 1)

fn counts(tables: List[String], classes: List[String]) -> List[Int] = {
  let all = list.flat_map(tables, (t) => [t, t])
  list.map(classes, (c) => list.len(list.filter(all, (x) => x == c)))
}

fn main() -> Unit = {
  let t = Table { names: ["a", "bb", "ccc"], sizes: [1, 2] }
  println(int.to_string(list.len(mapped(t)) + list.len(filtered(t, 1)) + folded(t)))
  println(if any_long(t, 2) then "true" else "false")
  println(int.to_string(list.len(sliced([1, 2, 3], [4])) + list.len(prefixed("p", ["x"]))))
  println(int.to_string(list.len(each(t)) + saved(t)(1) + applied(t)))
  println(int.to_string(list.len(t.names)))
  println(int.to_string(list.sum(counts(["a", "b"], ["a", "c"]))))
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
fn an_inlined_chain_callback_borrows_the_param_it_reads() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let out = Command::new(almide_bin()).arg(&file).arg("--target").arg("rust").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).into_owned();

    // Every combinator family: the param stays `&T`, the step is a plain
    // borrowing closure — no `move`, no `__cap` bind, no clone of `t`.
    for (name, sig) in [
        ("mapped", "pub fn mapped(t: &Table)"),
        ("filtered", "pub fn filtered(t: &Table, k: i64)"),
        ("folded", "pub fn folded(t: &Table)"),
        ("any_long", "pub fn any_long(t: &Table, k: i64)"),
        ("sliced", "pub fn sliced(xs: &[i64], ys: &[i64])"),
        ("prefixed", "pub fn prefixed(p: &str, xs: Vec<String>)"),
    ] {
        let s = fn_sig_and_body(&rust, name);
        assert!(s.starts_with(sig), "{name}:\n{s}");
        assert!(!s.contains("move |"), "{name} step must not move:\n{s}");
        assert!(!s.contains("__cap_"), "{name} step must not bind a capture:\n{s}");
    }
    // A step that returns the param per element clones it there whether
    // the param is owned or not, so the param stays borrowed.
    let s = fn_sig_and_body(&rust, "each");
    assert!(s.starts_with("pub fn each(t: &Table)") && s.contains("t.clone()"), "{s}");
    // A single-use local read inside a chain step (the source of a nested
    // chain whose predicate only compares) is borrowed there — never moved:
    // the step runs per element, and a move out of a captured variable is
    // E0507 (tools/almide-gates hit it).
    let s = fn_sig_and_body(&rust, "counts");
    assert!(s.contains("(all).iter()") || s.contains("all.clone()"), "the captured local must not move:\n{s}");
    // A closure that outlives its call captures, and the capture owns.
    assert!(fn_sig_and_body(&rust, "saved").starts_with("pub fn saved(t: Table)"), "{}", fn_sig_and_body(&rust, "saved"));
    assert!(fn_sig_and_body(&rust, "applied").starts_with("pub fn applied(t: Table)"), "{}", fn_sig_and_body(&rust, "applied"));

    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().unwrap();
        assert!(run.status.success(), "{target}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "13\ntrue\n2\n13\n3\n2", "{target}");
    }
}
