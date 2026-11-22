//! #4902: `list.push` on a list field of a `mut` record parameter grows the
//! field's own list on the certified native render — the field is taken out
//! of the parameter and extended — instead of copying the whole list into a
//! fresh one on every push (a build loop was quadratic). Whether the pushed
//! element reads the record's other fields does not matter. A `+` on the field
//! whose result is a new list while the field is read again afterwards keeps
//! the copy: the field still holds its own list.
//!
//! The pushed element never becomes a one-element list first (#4902, second
//! half): whatever the element type — a tuple, a string, a list, a variant —
//! the `+` pushes the element itself onto the taken field (`vN_e0`, the
//! element's binding, is never wrapped as `vec![vN_e0]`). The control keeps
//! the one-element list where the `+` result is a new list the program reads
//! beside the field it copied.

fn certified(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source_certified(&src).expect("renders on v1").0)
        .expect("spawn")
        .join()
        .expect("join")
}

fn body<'a>(rust: &'a str, f: &str) -> &'a str {
    let at = rust.find(&format!("fn almd_{f}(")).unwrap_or_else(|| panic!("no `{f}`:\n{rust}"));
    let b = &rust[at..];
    &b[..b.find("\n}\n").unwrap_or(b.len())]
}

const PROGRAM: &str = "type R = { slots: List[(Int, Int)], next: Int }\n\
     fn push_root(mut h: R, x: Int) -> Unit = list.push(h.slots, (x, h.next))\n\
     fn push_const(mut h: R, x: Int) -> Unit = list.push(h.slots, (x, 7))\n\
     fn grown(mut h: R, x: Int) -> Int = {\n  let g = h.slots + [(x, 7)]\n  list.len(g) + list.len(h.slots)\n}\n\
     fn main() -> Unit = {\n  var h: R = { slots: [], next: 7 }\n  for i in 0..<3 {\n    push_root(h, i)\n    push_const(h, i)\n    h.next = h.next + 1\n  }\n  println(int.to_string(grown(h, 1) + list.len(h.slots)))\n}\n";

#[test]
fn a_push_on_a_field_of_a_mut_parameter_grows_the_field_in_place() {
    let rust = certified(PROGRAM);
    for f in ["push_root", "push_const"] {
        let b = body(&rust, f);
        assert!(b.contains("std::mem::take(&mut v0."), "`{f}` does not take the field it pushes to:\n{b}");
        assert!(!b.contains("extend_from_slice"), "`{f}` copies the field's list:\n{b}");
    }
}

const ELEMENTS: &str = "type S = { names: List[String], n: Int }\n\
     type L = { rows: List[List[Int]], n: Int }\n\
     type Ev = | Hit(Int) | Miss(String)\n\
     type E = { evs: List[Ev], n: Int }\n\
     type R = { slots: List[(Int, Int)], next: Int }\n\
     fn push_tuple(mut h: R, x: Int) -> Unit = list.push(h.slots, (x, h.next))\n\
     fn push_str(mut s: S, x: String) -> Unit = list.push(s.names, x)\n\
     fn push_list(mut l: L, x: Int) -> Unit = list.push(l.rows, [x, l.n])\n\
     fn push_variant(mut e: E, x: Int) -> Unit = list.push(e.evs, Hit(x + e.n))\n\
     fn beside(h: R, x: Int) -> Int = {\n  let g = h.slots + [(x, 1)]\n  list.len(g) + list.len(h.slots)\n}\n\
     fn main() -> Unit = {\n  var h: R = { slots: [], next: 7 }\n  var s = S { names: [], n: 0 }\n  var l: L = { rows: [], n: 2 }\n  var e: E = { evs: [], n: 3 }\n  for i in 0..<3 {\n    push_tuple(h, i)\n    push_str(s, \"k${i}\")\n    push_list(l, i)\n    push_variant(e, i)\n  }\n  println(\"${h.slots} ${s.names} ${l.rows} ${list.len(e.evs)} ${beside(h, 9)}\")\n}\n";

#[test]
fn a_one_element_push_onto_a_taken_field_pushes_the_element_itself() {
    let rust = certified(ELEMENTS);
    for f in ["push_tuple", "push_str", "push_list", "push_variant"] {
        let b = body(&rust, f);
        assert!(b.contains("std::mem::take(&mut v0."), "`{f}` does not take the field it pushes to:\n{b}");
        assert!(b.contains(".push("), "`{f}` does not push:\n{b}");
        assert!(!b.contains("_e0]"), "`{f}` builds a one-element list to append:\n{b}");
        assert!(!b.contains("extend_from_slice"), "`{f}` copies the field's list:\n{b}");
    }
}

#[test]
fn a_one_element_concat_beside_a_field_read_again_keeps_the_copy() {
    let rust = certified(ELEMENTS);
    let b = body(&rust, "beside");
    assert!(!b.contains("std::mem::take"), "`beside` takes the list its param still holds:\n{b}");
    assert!(b.contains("extend_from_slice"), "`beside` no longer copies the left list:\n{b}");
}

#[test]
fn a_concat_on_a_field_read_again_afterwards_still_copies() {
    let rust = certified(PROGRAM);
    let b = body(&rust, "grown");
    assert!(!b.contains("std::mem::take"), "`grown` takes the list its field still holds:\n{b}");
    assert!(b.contains("extend_from_slice"), "`grown` no longer builds a fresh list:\n{b}");
}
