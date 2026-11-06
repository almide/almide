//! #4503: a list `+` over lists whose elements own heap (`List[List[Int]]`,
//! `List[String]`) on the certified native render.
//!
//! koka_hqueens read 1.47x the standard codegen's instructions once its self
//! tail calls became loops (#4492): the loop slot `xss` started as a copy of
//! the borrowed param (`xss + []`, every element list cloned per call), and
//! the prepend `[x] + xss` cloned every element list of the dying slot again
//! each round. Now a param the body consumes by that `+` is taken over
//! (moved in by the caller), and a dying right operand's elements move into
//! the result. Each rule is checked both ways: where it applies and where it
//! must not.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
        .unwrap_or_else(|e| panic!("the program walls: {e}"))
}

/// The header line and body of the rendered fn whose name contains `name`.
fn rendered_fn(out: &str, name: &str) -> (String, String) {
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with("fn ") && l.contains(&format!("_{name}(")))
        .unwrap_or_else(|| panic!("{name} is rendered:\n{out}"));
    let end = (at + 1..lines.len()).find(|&k| lines[k].starts_with("fn ")).unwrap_or(lines.len());
    (lines[at].to_string(), lines[at + 1..end].join("\n"))
}

fn run_native(src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The hqueens shape: a self tail call prepending to a `List[List[Int]]`
/// carry. The carry param is taken over (no `&` in the header, no copy of
/// it at entry) and the prepend moves the dying slot's element lists.
#[test]
fn a_prepend_to_a_list_of_lists_carry_moves_its_elements() {
    let src = r#"
fn push_front(n: Int, xss: List[List[Int]]) -> List[List[Int]] = if n <= 0 then xss
else push_front(n - 1, [[n, n]] + xss)

effect fn main() -> Unit = {
  let r = push_front(1000, [[0]])
  let first = list.get(r, 0) ?? []
  let last = list.get(r, 1000) ?? []
  println("${list.len(r)} ${list.len(first)} ${list.get(first, 0) ?? 0} ${list.get(last, 0) ?? 9}")
}
"#;
    let out = render(src);
    let (header, body) = rendered_fn(&out, "push_front");
    assert!(!header.contains('&'), "the consumed carry param is a move param:\n{header}\n{body}");
    assert!(body.contains("r.extend("), "the prepend moves the dying slot's elements:\n{header}\n{body}");
    assert_eq!(run_native(src), "1001 2 1 0\n");
}

/// The same `+` whose right operand is read again keeps cloning its
/// elements, and a param read after the `+` stays borrowed.
#[test]
fn a_right_operand_read_after_the_concat_is_cloned_and_its_param_borrowed() {
    let src = r#"
fn wrap(xss: List[List[Int]]) -> Int = {
  let ys = [[7]] + xss
  list.len(ys) + list.len(xss)
}

effect fn main() -> Unit = {
  let xss = [[1], [2, 3]]
  println("${wrap(xss)} ${list.len(xss)}")
}
"#;
    let out = render(src);
    let (header, body) = rendered_fn(&out, "wrap");
    assert!(header.contains('&'), "a param read after the `+` stays borrowed:\n{header}\n{body}");
    assert!(!body.contains("r.extend("), "a live right operand is cloned, not moved:\n{header}\n{body}");
    assert!(body.contains("extend_from_slice"), "a live right operand is cloned:\n{header}\n{body}");
    assert_eq!(run_native(src), "5 2\n");
}

/// A param the body consumes as the LEFT side of a heap-element `+` is a
/// move param too (the `xss + []` slot start), and the concat grows the
/// caller's buffer in place.
#[test]
fn a_list_of_lists_param_grown_by_concat_is_taken_over() {
    let src = r#"
fn add_back(xss: List[List[Int]]) -> List[List[Int]] = xss + [[9]]

effect fn main() -> Unit = {
  let a = add_back([[1], [2]])
  println("${list.len(a)} ${list.get(list.get(a, 2) ?? [], 0) ?? 0}")
}
"#;
    let out = render(src);
    let (header, body) = rendered_fn(&out, "add_back");
    assert!(!header.contains('&'), "the grown param is a move param:\n{header}\n{body}");
    assert_eq!(run_native(src), "3 9\n");
}

/// A dying right operand of SCALAR elements keeps `extend_from_slice`
/// (a plain block copy; moving its `i64`s buys nothing).
#[test]
fn a_dying_scalar_right_operand_keeps_the_slice_copy() {
    let src = r#"
fn front(n: Int) -> Int = {
  let xs = [1, 2, 3, n]
  let ys = [n] + xs
  list.len(ys)
}

effect fn main() -> Unit = println("${front(5)}")
"#;
    let out = render(src);
    let (header, body) = rendered_fn(&out, "front");
    assert!(!body.contains("r.extend("), "scalar elements are block-copied:\n{header}\n{body}");
    assert_eq!(run_native(src), "5\n");
}
