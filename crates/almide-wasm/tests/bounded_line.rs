//! #2312 shape 1: a line whose every part has a static length bound, built
//! OUTERMOST in `main`, never links the line buffer's grow path — and so,
//! in a program that does not otherwise allocate, not `$alloc` and its
//! C-197 out-of-memory abort either. A build that is not outermost (inside
//! a called fn) keeps the checked appends. The soundness condition is
//! written in `src/line_bounded.rs`; output parity is the wasm_cross sweep.
//!
//! Slots are fixed function indices (five imports first): `F_ALLOC` = 11,
//! `F_APPEND_COPY` = 7, `F_LINE_GROW` = 39.

use wasmparser::{Parser, Payload};

const IMPORTS: usize = 5;
const F_APPEND_COPY: usize = 7;
const F_ALLOC: usize = 11;
const F_LINE_GROW: usize = 39;
/// `unreachable; end` with an empty locals vector.
const STUB_LEN: usize = 3;

fn body_sizes(bytes: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("parse") {
            let r = body.range();
            out.push((r.end - r.start) as usize);
        }
    }
    out
}

fn shipped(src: &str, slot: usize) -> bool {
    let ir = almide_spine::s5::lower_to_ir("bounded", src).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    wasmparser::validate(&bytes).expect("valid");
    body_sizes(&bytes)[slot - IMPORTS] > STUB_LEN
}

const FIB: &str = "fn fib(n: Int) -> Int = if n <= 1 then n else fib(n - 1) + fib(n - 2)\n";

#[test]
fn a_bounded_outermost_line_links_neither_the_grow_path_nor_the_allocator() {
    for main in [
        r#"fn main() -> Unit = println("${fib(20)}")"#,
        r#"fn main() -> Unit = println(int.to_string(fib(20)))"#,
        r#"fn main() -> Unit = println("${int.to_string(fib(20))}")"#,
        r#"fn main() -> Unit = println("fib=${fib(20)} big=${fib(20) > 5}!")"#,
        r#"fn main() -> Unit = { let v = fib(20)
  println("a${v}")
  eprintln("b${v}") }"#,
    ] {
        let src = format!("{FIB}{main}\n");
        for (slot, name) in [(F_LINE_GROW, "$line_grow"), (F_ALLOC, "$alloc"), (F_APPEND_COPY, "$append_copy")] {
            assert!(!shipped(&src, slot), "{name} shipped for a bounded outermost line:\n{src}");
        }
    }
}

/// The other side of the condition: the same build inside a called fn is
/// not outermost — something may already be open in the region when it
/// runs — so it keeps the room check and the grow path.
#[test]
fn a_build_that_is_not_outermost_keeps_the_room_check() {
    let src = format!(
        "{FIB}fn show(n: Int) -> String = \"n=${{n}}\"\nfn main() -> Unit = println(show(fib(20)))\n"
    );
    assert!(shipped(&src, F_LINE_GROW), "a build in a called fn must keep $line_grow:\n{src}");
}

/// An unbounded part (a String of any length) ends the room-free prefix:
/// the build goes back to the checked appends.
#[test]
fn an_unbounded_part_keeps_the_room_check() {
    let src = format!("{FIB}fn main() -> Unit = {{ let s = string.repeat(\"x\", fib(20))\n  println(\"${{fib(3)}}:${{s}}\") }}\n");
    assert!(shipped(&src, F_LINE_GROW), "an unbounded part must keep $line_grow:\n{src}");
}

