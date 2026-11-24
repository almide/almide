//! A self-append whose pieces include an Int, Bool or Float (`s = "${s}, ${n}"`)
//! grows `s` in place on v1 native, as a string-only one already did. It
//! rebuilt all of `s` through one `from_fmt` on every turn, quadratic in the
//! loop: a nested list's display (its synthesized `__native_display` loops
//! append `", " + int text` per element) ran 17 s native against 0.07 s wasm
//! at 400,000 inner Ints, the matrix.zeros/ones inspection findings.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

/// The body of the rendered fn `name` (up to the next top-level `}`).
fn body<'a>(code: &'a str, name: &str) -> &'a str {
    let at = code.find(&format!("fn almd_{name}(")).unwrap_or_else(|| panic!("no fn {name} in:\n{code}"));
    let f = &code[at..];
    &f[..f.find("\n}\n").map_or(f.len(), |e| e + 2)]
}

#[test]
fn a_self_append_with_an_int_piece_grows_the_string_in_place() {
    let code = render(
        "fn join_ints(xs: List[Int]) -> String = {\n\
         \x20 var s = \"\"\n\
         \x20 for n in xs {\n\
         \x20   s = \"${s}, ${n}\"\n\
         \x20 }\n\
         \x20 s\n\
         }\n\
         effect fn main() -> Unit = println(join_ints([1, 2, 3]))\n",
    );
    let f = body(&code, "join_ints");
    assert!(f.contains("unique_with(__extra)"), "the append is not in place:\n{f}");
    assert!(f.contains("let __t0 = AlmideString::from_fmt(format_args!(\"{}\""), "the Int is not formatted on its own:\n{f}");
}

#[test]
fn a_nested_list_display_appends_each_element_in_place() {
    let code = render("effect fn main() -> Unit = {\n  let l = list.repeat(list.repeat(0, 3), 2)\n  println(\"${l}\")\n}\n");
    let displays: Vec<&str> = code
        .match_indices("fn almd___native_display_")
        .map(|(i, _)| &code[i..i + code[i..].find("\n}\n").map_or(code.len() - i, |e| e + 2)])
        .collect();
    assert!(displays.len() >= 2, "expected the outer and the inner display:\n{code}");
    for d in displays {
        assert!(d.contains("unique_with(__extra)"), "a display loop rebuilds its text:\n{d}");
    }
}

/// Control: a chain whose head is read again afterwards cannot take the head's
/// buffer, so it still renders as one fresh `from_fmt`.
#[test]
fn a_chain_whose_head_is_read_later_still_builds_a_fresh_string() {
    let code = render(
        "fn both(s: String, n: Int) -> String = {\n\
         \x20 let t = \"${s}, ${n}\"\n\
         \x20 t + s\n\
         }\n\
         effect fn main() -> Unit = println(both(\"a\", 1))\n",
    );
    let f = body(&code, "both");
    assert!(!f.contains("unique_with(__extra)"), "a live head was grown in place:\n{f}");
}
