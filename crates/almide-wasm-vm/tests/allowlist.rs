//! REQ-VM-2: the VM's closed instruction set is EXACTLY the set the shipped
//! artifact can contain — every instruction the structural emitter and the
//! `to_wasi` shims can write, and nothing else. Both directions are checked
//! against the emitter's source, so a new instruction in the emitter fails
//! here until the VM implements it (or the emitter stops using it), and an
//! instruction the emitter no longer writes cannot linger in the VM.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use almide_wasm_vm::ir::{CONTROL_AND_VARIABLE, LOADS, STORES};
use almide_wasm_vm::numeric::NUMERIC;

/// Methods whose names match the instruction pattern but that build types,
/// not code. Shrink-only: a new entry needs a reason.
const NOT_INSTRUCTIONS: &[&str] = &[
    "table_type", // wasm_encoder::reencode — a table's type, in the table section
];

/// The instruction-sink method for a text-format name: `i32.add` →
/// `i32_add`, and the four Rust keywords take a trailing underscore.
fn sink_method(text: &str) -> String {
    match text {
        "loop" | "if" | "else" | "return" => format!("{text}_"),
        _ => text.replace('.', "_"),
    }
}

fn vm_set() -> BTreeSet<String> {
    let names = NUMERIC
        .iter()
        .map(|o| o.name)
        .chain(LOADS.iter().map(|o| o.name))
        .chain(STORES.iter().map(|o| o.name))
        .chain(CONTROL_AND_VARIABLE.iter().copied());
    names.map(sink_method).collect()
}

fn looks_like_instruction(ident: &str) -> bool {
    const PREFIXES: &[&str] =
        &["i32_", "i64_", "f32_", "f64_", "v128_", "memory_", "table_", "local_", "global_", "ref_"];
    const CONTROL: &[&str] = &[
        "block", "loop_", "if_", "else_", "end", "br", "br_if", "br_table", "return_", "call", "call_indirect",
        "return_call", "return_call_indirect", "drop", "select", "unreachable", "nop", "try_table", "throw",
        "data_drop", "elem_drop",
    ];
    PREFIXES.iter().any(|p| ident.starts_with(p)) || CONTROL.contains(&ident)
}

/// Every `.ident(` in a source text whose ident looks like an instruction.
fn scan(text: &str, into: &mut BTreeSet<String>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'.' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_lowercase() || b[j].is_ascii_digit() || b[j] == b'_') {
                j += 1;
            }
            if j > start && j < b.len() && b[j] == b'(' {
                let ident = &text[start..j];
                if looks_like_instruction(ident) && !NOT_INSTRUCTIONS.contains(&ident) {
                    into.insert(ident.to_string());
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("source dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn emitter_set() -> BTreeSet<String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crates dir");
    let mut files = Vec::new();
    rust_files(&crates.join("almide-wasm/src"), &mut files);
    // the to_wasi transform's shims: code appended to every shipped artifact
    // (its own crate since #2554, re-exported as `almide_wasm_run::wasi`)
    files.push(crates.join("almide-wasi/src/lib.rs"));
    files.push(crates.join("almide-wasi/src/wasi_shims.rs"));
    let mut set = BTreeSet::new();
    for f in &files {
        scan(&std::fs::read_to_string(f).expect("readable source"), &mut set);
    }
    set
}

#[test]
fn the_closed_set_is_exactly_what_the_shipped_artifact_can_contain() {
    let vm = vm_set();
    let emitter = emitter_set();
    let missing: Vec<_> = emitter.difference(&vm).collect();
    let extra: Vec<_> = vm.difference(&emitter).collect();
    assert!(
        missing.is_empty(),
        "the emitter or the to_wasi shims write instructions the VM does not accept: {missing:?}"
    );
    assert!(extra.is_empty(), "the VM accepts instructions no shipped artifact can contain: {extra:?}");
    assert_eq!(vm.len(), 127, "the closed set's size (the numeric table, the memory rows, control and variables)");
}

#[test]
fn the_vm_set_has_no_duplicate_names() {
    let total = NUMERIC.len() + LOADS.len() + STORES.len() + CONTROL_AND_VARIABLE.len();
    assert_eq!(vm_set().len(), total, "a name appears in two tables");
}
