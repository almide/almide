//! Print one structural-leg ownership witness (#1696 phase A2) — the
//! gate.sh feeder for the EXTRACTED kernel-proven checker, mirroring
//! `almide-mir`'s `emit_cert_from_source` for the incumbent leg.
//!
//! Usage: emit_structural_witness <fixture-rel-path> <fn-name>
//!
//! Lowers the fixture through the SAME front the product leg uses
//! (`almide_spine::s5`), arms the witness sink, emits the whole program,
//! and prints the named function's certificate (v0 event streams, one
//! object per line) to stdout. Exit 2 if the function was not witnessed —
//! a straightline-gate regression the gate must fail loudly on, never
//! skip.

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(rel), Some(fn_name)) = (args.next(), args.next()) else {
        eprintln!("usage: emit_structural_witness <fixture-rel-path> <fn-name>");
        std::process::exit(2);
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(almide_corpus::resolve(&root, &rel))
        .unwrap_or_else(|e| {
            eprintln!("read {rel}: {e}");
            std::process::exit(2);
        });
    let ir = almide_spine::s5::lower_to_ir(&rel, &text).unwrap_or_else(|e| {
        eprintln!("front: {e}");
        std::process::exit(2);
    });
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir);
    for (name, cert) in almide_wasm::witness::take() {
        if name == fn_name {
            print!("{cert}");
            return;
        }
    }
    eprintln!("fn {fn_name} was not witnessed in {rel} — the straightline gate declined it (a phase-A coverage regression)");
    std::process::exit(2);
}
