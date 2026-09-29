//! Print the structural leg's CALL-MODE witness (#2758) for one fixture —
//! the gate.sh feeder for the extracted `call-modes` checker.
//!
//! Usage: emit_call_modes <fixture-rel-path>
//!
//! Lowers the fixture through the product front, emits the whole program
//! with the witness sink armed, and prints the first emission pass's
//! `<signatures>|<sites>` stream (every pass's is judged by the witness
//! floor test). Exit 2 if nothing was recorded.

use std::io::Write;

fn fail(msg: &str) -> ! {
    let _ = writeln!(std::io::stderr(), "{msg}");
    std::process::exit(2);
}

fn main() {
    let Some(rel) = std::env::args().nth(1) else { fail("usage: emit_call_modes <fixture-rel-path>") };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(almide_corpus::resolve(&root, &rel)).unwrap_or_else(|e| fail(&format!("read {rel}: {e}")));
    let ir = almide_spine::s5::lower_to_ir(&rel, &text).unwrap_or_else(|e| fail(&format!("front: {e}")));
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir);
    let _ = almide_wasm::witness::take_by_pass();
    match almide_wasm::witness::take_modes().into_iter().next() {
        Some((_, w)) => {
            let _ = writeln!(std::io::stdout(), "{w}");
        }
        None => fail(&format!("no call-mode witness recorded for {rel}")),
    }
}
