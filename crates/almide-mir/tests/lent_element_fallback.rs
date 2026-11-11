//! `list.get(ts, p) ?? "lit"` over a borrowed `List[String]` parameter: the
//! element Option is held as the lent element, and the join with a literal
//! fallback is a borrow when it is only compared or measured — the present
//! arm is the element itself and the absent arm the literal's own text. A
//! token test in a parser (`(list.get(ts, p) ?? "") == s`) allocated and
//! freed a copy of the element on every call before.
//!
//! The control: an element that leaves the function (returned, stored in a
//! list) is still an owned value, made by sharing the element's handle — not
//! by copying its bytes into a new block.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

const SOURCE: &str = r#"fn eq_at(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn bound_eq(ts: List[String], p: Int, s: String) -> Int = {
  let t = list.get(ts, p) ?? "?"
  if t == s or t == "(" then 1 else 2
}

fn guard_eq(ts: List[String], p: Int, s: String) -> Int = {
  let t = list.get(ts, p) ?? ""
  match t {
    "(" => 1,
    x if x == s => 3,
    _ => 2,
  }
}

fn len_at(ts: List[String], p: Int) -> Int = string.len(list.get(ts, p) ?? "none")

fn returned(ts: List[String], p: Int) -> String = list.get(ts, p) ?? "<end>"

fn stored(ts: List[String], p: Int) -> List[String] = {
  let t = list.get(ts, p) ?? "<end>"
  [t, t]
}

fn main() -> Unit = {
  let ts = ["A", "(", ")"]
  println("${eq_at(ts, 1, "(")} ${bound_eq(ts, 0, "?")} ${guard_eq(ts, 0, "A")} ${len_at(ts, 9)}")
  println("${returned(ts, 9)} ${list.join(stored(ts, 0), ",")}")
}
"#;

fn body<'a>(code: &'a str, name: &str) -> &'a str {
    let at = code.find(&format!("fn almd_{name}(")).unwrap_or_else(|| panic!("no fn {name} in:\n{code}"));
    let f = &code[at..];
    &f[..f.find("\n}\n").map_or(f.len(), |e| e + 2)]
}

#[test]
fn a_defaulted_element_that_is_only_inspected_is_read_through_a_borrow() {
    let code = render(SOURCE);
    for name in ["eq_at", "bound_eq", "guard_eq", "len_at"] {
        let b = body(&code, name);
        assert!(b.contains("almide_rt_list_get_ref("), "{name} must hold the lent element:\n{b}");
        assert!(!b.contains("AlmideString::from(") && !b.contains(".clone()"), "{name} copies the element:\n{b}");
        assert!(!b.contains("AlmideString::from_static("), "{name} builds its fallback as an owned value:\n{b}");
    }
}

#[test]
fn a_defaulted_element_that_escapes_is_an_owned_value_sharing_the_handle() {
    let code = render(SOURCE);
    for name in ["returned", "stored"] {
        let b = body(&code, name);
        // The join is owned: its fallback is the owned literal, and the
        // element side is one share of the element's handle.
        assert!(b.contains("AlmideString::from_static("), "{name} must own its fallback:\n{b}");
        assert!(b.contains("Some(v) => v.clone()"), "{name} must share the element's handle:\n{b}");
        assert!(!b.contains("AlmideString::from(v"), "{name} copies the element's bytes:\n{b}");
    }
    assert!(body(&code, "returned").contains("-> AlmideString"), "{}", body(&code, "returned"));
}
