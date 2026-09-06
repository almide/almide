//! #1962: the RC runtime core (`$alloc` / `$free` / `$inc` / `$dec_flat` /
//! `$cow`) ships only when a shipped body reaches it. A program that never
//! allocates carries the five as index-stable 2-byte `unreachable` stubs; a
//! program that allocates keeps the real bodies. The slots are fixed
//! (`F_ALLOC` = 11, `F_FREE` = 33, `F_INC` = 34, `F_DEC_FLAT` = 35, `F_COW`
//! = 36 in the function index space, five imports first), so the code
//! section's body at `slot - 5` is the helper.

use wasmparser::{Parser, Payload};

const HELLO: &str = r#"fn main() -> Unit = println("Hello, world")
"#;

const ALLOCATES: &str = r#"fn main() -> Unit = {
  let xs = [1, 2, 3]
  println("${list.len(xs)}")
}
"#;

const IMPORTS: usize = 5;
const CORE_SLOTS: [u32; 5] = [11, 33, 34, 35, 36];

/// Byte length of every code-section body, in function-index order after
/// the imports.
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

fn emit(name: &str, src: &str) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir(name, src).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    wasmparser::validate(&bytes).expect("valid");
    bytes
}

/// `unreachable; end` with an empty locals vector is three bytes.
const STUB_LEN: usize = 3;

#[test]
fn a_program_that_never_allocates_stubs_the_rc_core() {
    let bytes = emit("hello", HELLO);
    let sizes = body_sizes(&bytes);
    for slot in CORE_SLOTS {
        let len = sizes[slot as usize - IMPORTS];
        assert!(
            len <= STUB_LEN,
            "core helper at slot {slot} shipped a real body ({len} bytes) for a non-allocating program (#1962)"
        );
    }
    assert!(
        bytes.len() < 1200,
        "hello without the RC core must be well under the 2.3 KB it carried; got {} bytes",
        bytes.len()
    );
}

#[test]
fn a_program_that_allocates_keeps_the_rc_core() {
    let bytes = emit("allocates", ALLOCATES);
    let sizes = body_sizes(&bytes);
    let alloc_len = sizes[11 - IMPORTS];
    assert!(
        alloc_len > STUB_LEN,
        "$alloc must ship its real body for an allocating program; got {alloc_len} bytes"
    );
}

/// The stubs are index-stable: pruning the core must not move `main` or
/// any other slot — the module still validates and its exported main runs
/// (the wasi_services suite runs hello end to end; here the shape only).
#[test]
fn pruning_keeps_the_slot_layout() {
    let hello = body_sizes(&emit("hello", HELLO));
    let alloc = body_sizes(&emit("allocates", ALLOCATES));
    // Both modules carry the full fixed helper block (42 - 5 slots) before
    // any program fn.
    assert!(hello.len() >= 42 - IMPORTS && alloc.len() >= 42 - IMPORTS);
}
