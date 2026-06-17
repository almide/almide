// tests_part5 — SELF-HOSTED `list.to_string` (the `${list}` compound interp), fix-0276.
//
// A `${xs}` over a `List[T]` desugars (lower::interp_to_string_call) to an element-type-keyed call:
//   List[Int]    → list.to_string      List[Float] → list.to_string_f
//   List[Bool]   → list.to_string_b    List[String]→ list.to_string_s
// each a monomorphic self-host impl (stdlib/list_to_string*.almd) that byte-matches v0's AlmideRepr
// for the element type. An UNSUPPORTED element (nested List[List[_]], Map/Set/…) routes to the
// never-registered `list.to_string_x`, so the using fn WALLS cleanly — never a wrong byte. The
// goldens below are captured from `almide run` on the native v0 oracle.

#[test]
fn compound_list_interp_int_byte_matches_v0() {
    // v0: "[1, 2, 3]", "[]", "[42]", "[-5, 0, 100, -1234567]" (signed decimal elements, ", " joined).
    let src = "fn main() -> Unit = {\n  \
        let a: List[Int] = [1, 2, 3]\n  \
        let b: List[Int] = []\n  \
        let c: List[Int] = [42]\n  \
        let d: List[Int] = [0 - 5, 0, 100, 0 - 1234567]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\")\n  println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string"),
        "a List[Int] interp must auto-link list.to_string"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[Int] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_int", &render_wasm_program(&prog)) {
        assert_eq!(out, "[1, 2, 3]\n[]\n[42]\n[-5, 0, 100, -1234567]");
    }
}

#[test]
fn compound_list_interp_bool_byte_matches_v0() {
    // v0: "[true, false]", "[]", "[false, true, true]".
    let src = "fn main() -> Unit = {\n  \
        let a: List[Bool] = [true, false]\n  \
        let b: List[Bool] = []\n  \
        let c: List[Bool] = [false, true, true]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string_b"),
        "a List[Bool] interp must auto-link list.to_string_b"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[Bool] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_bool", &render_wasm_program(&prog)) {
        assert_eq!(out, "[true, false]\n[]\n[false, true, true]");
    }
}

#[test]
fn compound_list_interp_string_byte_matches_v0() {
    // v0: each element double-quoted + escaped (\ " \n \r \t), other bytes (incl. UTF-8) verbatim.
    // "[\"a\", \"b\", \"c\"]", "[]", and an escaping case.
    let src = "fn main() -> Unit = {\n  \
        let a: List[String] = [\"a\", \"b\", \"c\"]\n  \
        let b: List[String] = []\n  \
        let c: List[String] = [\"hi\", \"\"]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string_s"),
        "a List[String] interp must auto-link list.to_string_s"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[String] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_string", &render_wasm_program(&prog)) {
        assert_eq!(out, "[\"a\", \"b\", \"c\"]\n[]\n[\"hi\", \"\"]");
    }
}

#[test]
fn compound_list_interp_string_escapes_byte_match_v0() {
    // The escaping detail: a quote, a backslash, a tab, a newline each escape to a 2-byte sequence.
    // v0 (one line): [\"q\\\"x\", \"b\\\\s\", \"t\\tx\", \"n\\nx\"]
    let src = "fn main() -> Unit = {\n  \
        let c: List[String] = [\"q\\\"x\", \"b\\\\s\", \"t\\tx\", \"n\\nx\"]\n  \
        println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[String] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_str_esc", &render_wasm_program(&prog)) {
        // The bytes: opening [, then "q\"x", "b\\s", "t\tx", "n\nx" — backslash-escaped per element.
        assert_eq!(out, "[\"q\\\"x\", \"b\\\\s\", \"t\\tx\", \"n\\nx\"]");
    }
}

#[test]
fn compound_list_interp_float_byte_matches_v0_with_dot0_drop() {
    // THE TRAP: the compound-Float element is `format!("{}", f64)` — the SHORTEST decimal with the
    // trailing ".0" DROPPED for integer-valued floats (1.0 → "1", 100.0 → "100"), unlike scalar
    // float.to_string (which keeps ".0"). Non-integer floats are unchanged (2.5 → "2.5"). v0:
    //   [1, 2.5]                        (1.0 drops ".0", 2.5 kept)
    //   [1, 2, 2.5, 0.5, 100, 0.1, 0.3333333333333333, 123456789, 0.001, 1000000]
    let src = "fn main() -> Unit = {\n  \
        let a: List[Float] = [1.0, 2.5]\n  \
        let b: List[Float] = []\n  \
        let c: List[Float] = [1.0, 2.0, 2.5, 0.5, 100.0, 0.1, 0.3333333333333333, 123456789.0, 0.001, 1000000.0]\n  \
        println(\"${a}\")\n  println(\"${b}\")\n  println(\"${c}\") }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "list.to_string_f"),
        "a List[Float] interp must auto-link list.to_string_f"
    );
    // list.to_string_f calls float.to_string transitively — both must be linked (fixpoint).
    assert!(
        prog.functions.iter().any(|f| f.name == "float.to_string"),
        "list.to_string_f must transitively auto-link float.to_string"
    );
    assert!(
        crate::render_wasm::unlinked_call_names(&prog).is_empty(),
        "a List[Float] interp must be fully linkable, got {:?}",
        crate::render_wasm::unlinked_call_names(&prog)
    );
    if let Some(out) = build_and_run("compound_list_float", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "[1, 2.5]\n[]\n[1, 2, 2.5, 0.5, 100, 0.1, 0.3333333333333333, 123456789, 0.001, 1000000]"
        );
    }
}

#[test]
fn compound_list_interp_nested_walls_cleanly() {
    // A NESTED List[List[Int]] element is OUT of subset (v1 does not materialize the inner-list
    // handles): it routes to the never-registered `list.to_string_x`, so the fn WALLS (Unsupported)
    // rather than emit wrong bytes or invalid wasm. This is the all-or-nothing soundness boundary.
    use crate::lower::LowerError;
    use crate::render_wasm::{try_render_wasm_program, unlinked_call_names};
    let src = "fn main() -> Unit = {\n  \
        let xs: List[List[Int]] = [[1, 2], [3]]\n  \
        println(\"${xs}\") }\n";
    let prog = lower_source(src);
    assert!(
        unlinked_call_names(&prog).contains("list.to_string_x"),
        "a nested List interp must route to the unlinked list.to_string_x, got {:?}",
        unlinked_call_names(&prog)
    );
    match try_render_wasm_program(&prog) {
        Err(LowerError::Unsupported(_)) => {}
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("a nested-List interp must wall, not render to (possibly invalid) wasm"),
    }
}
