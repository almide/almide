//! A `mut` record param is `&mut T` for its whole body, so unlike a shared
//! borrow it is both borrowed and, wherever the body hands the value on,
//! consumed: inference never flips it to `Own`, and the clone pass's last-use
//! move leaves that occurrence a bare `Var`. rustc refused the by-value
//! positions with E0308 (`expected Table, found &mut Table`, #2266) while
//! `almide check` was green — the borrow lowering now owns the read at every
//! such position: an owned call slot, a constructor field or element, a
//! concatenation operand, a `let` initializer, the body's result.
//!
//! The evidence is the native build (the wasm leg never had the defect), so
//! the issue's program runs on both legs and the emitted Rust is pinned to the
//! owned spelling at each position.
use std::process::Command;

/// Issue #2266's program: three callers of a `mut` param, one per callee shape.
const ISSUE: &str = r#"type Table = { names: List[String], sizes: List[Int] }
type Holder = { t: Table, n: Int }

fn plain(t: Table) -> Int = list.len(t.names) + list.len(t.sizes)

fn captured(t: Table) -> Int = list.fold(list.map(t.sizes, (x) => x + list.len(t.names)), 0, (a, b) => a + b)

fn stored(t: Table, n: Int) -> Holder = Holder { t: t, n: n }

fn stored_then_read(t: Table) -> Int = { let h = stored(t, 1); h.n + plain(t) }

fn via_plain(mut t: Table) -> Int = { list.push(t.sizes, 1); plain(t) }
fn via_captured(mut t: Table) -> Int = { list.push(t.sizes, 1); captured(t) }
fn via_stored(mut t: Table) -> Int = { list.push(t.sizes, 1); stored_then_read(t) }

fn main() -> Unit = {
  var t = Table { names: ["a", "b"], sizes: [1] }
  println(int.to_string(via_plain(t) + via_captured(t) + via_stored(t)))
}
"#;

/// Every by-value position of a `mut` param the lowering must own.
const SHAPES: &str = r#"type Table = { names: List[String], sizes: List[Int] }
type Holder = { t: Table, n: Int }

fn captured(t: Table) -> Int = list.fold(list.map(t.sizes, (x) => x + list.len(t.names)), 0, (a, b) => a + b)

fn ret_tail(mut t: Table) -> Table = { list.push(t.sizes, 2); t }
fn into_record(mut t: Table) -> Holder = { list.push(t.sizes, 3); Holder { t: t, n: 1 } }
fn into_list(mut t: Table) -> List[Table] = { list.push(t.sizes, 4); [t] }
fn into_tuple(mut t: Table) -> (Table, Int) = { list.push(t.sizes, 5); (t, 7) }
fn into_let(mut t: Table) -> Int = { list.push(t.sizes, 6); let u = t; captured(u) }
fn concat_str(mut s: String) -> String = { s = s + "x"; s + "!" }
fn concat_list(mut xs: List[Int]) -> List[Int] = { list.push(xs, 9); xs + [10] }
fn scalar(mut n: Int) -> Int = { n = n + 1; captured(Table { names: [], sizes: [n] }) }

fn main() -> Unit = {
  var t = Table { names: ["a", "b"], sizes: [1] }
  var s = "s"
  var xs = [1]
  var n = 1
  println(int.to_string(list.len(ret_tail(t).sizes)))
  println(int.to_string(into_record(t).n + list.len(into_list(t)) + into_tuple(t).1 + into_let(t)))
  println(concat_str(s))
  println(int.to_string(list.len(concat_list(xs))))
  println(int.to_string(scalar(n)))
}
"#;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn write(tag: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-2266-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    file
}

fn run(file: &std::path::Path, extra: &[&str]) -> (bool, String, String) {
    let out = Command::new(almide_bin()).arg("run").arg(file).args(extra).output().expect("almide run");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn emit(file: &std::path::Path) -> String {
    let out = Command::new(almide_bin()).arg(file).arg("--target").arg("rust").output().expect("almide --target rust");
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    rust.split(&format!("pub fn {name}(")).nth(1).unwrap_or_else(|| panic!("no fn {name}")).split("\n}").next().unwrap()
}

#[test]
fn the_issue_program_builds_and_agrees_across_legs() {
    let file = write("issue", ISSUE);
    let (ok, out, err) = run(&file, &[]);
    assert!(ok, "native build must succeed (#2266 was E0308 here):\n{err}");
    assert_eq!(out, "20");
    let (ok, wasm_out, err) = run(&file, &["--target", "wasm"]);
    assert!(ok, "wasm run:\n{err}");
    assert_eq!(wasm_out, out, "the two legs must print the same line");
    let rust = emit(&file);
    assert!(fn_body(&rust, "via_captured").contains("captured(t.clone())"), "{}", fn_body(&rust, "via_captured"));
    // The shared-borrow callee keeps its reborrow: no clone where none is needed.
    assert!(fn_body(&rust, "via_plain").contains("plain(&t)"), "{}", fn_body(&rust, "via_plain"));
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn every_by_value_position_of_a_mut_param_owns_the_read() {
    let file = write("shapes", SHAPES);
    let (ok, out, err) = run(&file, &[]);
    assert!(ok, "native build must succeed:\n{err}");
    assert_eq!(out, "2\n42\nsx!\n3\n2");
    let rust = emit(&file);
    for (name, spelling) in [
        ("ret_tail", "t.clone()"),
        ("into_record", "t: t.clone()"),
        ("into_list", "vec![t.clone()]"),
        ("into_tuple", "(t.clone(), 7i64)"),
        ("into_let", "let u: Table = t.clone();"),
        ("concat_str", "concat(s.to_string(), \"!\""),
        ("concat_list", "concat(xs.to_vec(), vec![10i64])"),
    ] {
        let body = fn_body(&rust, name);
        assert!(body.contains(spelling), "{name}: expected `{spelling}` in\n{body}");
    }
    // A Copy scalar is read through `*n`, never cloned.
    let scalar = fn_body(&rust, "scalar");
    assert!(scalar.contains("*n") && !scalar.contains("n.clone()"), "{scalar}");
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
