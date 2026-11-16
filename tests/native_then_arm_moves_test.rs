//! A record rebuilt in the THEN-arm of an `if` moves the dying record's
//! fields on the native render, as the else-arm always did.
//!
//! `scan_after_copy` walked the ops after a copy in straight order, so from
//! the then-arm it read on into the else-arm, found the record's fields named
//! there, and kept them live: `d = deque.set(d, 0, k)` on a sole owner (its
//! front-slot arm) copied both of the deque's lists on every step, O(n²)
//! over a loop (found by the 2026-11 stdlib re-inspection, #4773). The
//! else-arm never runs on the then-arm's path, so the walk now resumes at its
//! `EndIf`. Inside a loop it does not: a later round may run either arm.

fn shipped_v1(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src).unwrap_or_else(|e| panic!("the v1 render walled: {e:?}\n{src}"))
}

fn fn_body<'a>(rust: &'a str, name: &str) -> &'a str {
    let at = rust.find(&format!("\nfn almd_{name}(")).unwrap_or_else(|| panic!("no fn {name} in\n{rust}"));
    let rest = &rust[at..];
    &rest[..rest.find("\n}\n").map_or(rest.len(), |e| e + 2)]
}

fn copies(body: &str) -> Vec<&str> {
    body.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .filter(|l| l.contains(".clone()") || l.contains(".to_vec()") || l.contains("clone_from"))
        .collect()
}

const PROGRAM: &str = r#"type D = { front: List[Int], back: List[Int] }

fn put(d: D, i: Int, x: Int) -> D = if i < list.len(d.front) then { ...d, front: list.set(d.front, i, x) }
else { ...d, back: list.set(d.back, i - list.len(d.front), x) }

effect fn main() -> Unit = {
  var d: D = { front: [1, 2], back: [3, 4] }
  for k in 0..<3 {
    d = put(d, 0, k)
    d = put(d, 3, k)
  }
  println("${d.front} ${d.back}")
}
"#;

#[test]
fn a_then_arm_rebuild_moves_the_dying_record() {
    let rust = shipped_v1(PROGRAM);
    let body = fn_body(&rust, "put");
    let c = copies(body);
    assert!(c.is_empty(), "the then-arm (or else-arm) rebuild copies the dying record:\n  {}\n{body}", c.join("\n  "));
}

#[test]
fn a_shared_record_is_still_copied_in_the_then_arm() {
    // The negative control: the caller keeps `d`, so a copy is made — in
    // `put`'s arms or at the call in `main` — and the check sees it.
    let shared = PROGRAM.replace("    d = put(d, 0, k)\n", "    let e = put(d, 0, k)\n    println(\"${e.front} ${d.front}\")\n");
    let rust = shipped_v1(&shared);
    let body = fn_body(&rust, "put");
    let main = &rust[rust.find("\nfn main()").expect("fn main")..];
    let main = &main[..main.find("\n}\n").map_or(main.len(), |e| e + 2)];
    assert!(
        !copies(body).is_empty() || !copies(main).is_empty(),
        "the shared control copies nothing — the check cannot tell a move from a copy:\n{body}\n{main}"
    );
}
