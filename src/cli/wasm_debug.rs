//! `almide build --target wasm --debug` (#1315): the DWARF line table.
//!
//! The emitter records which `.almd` line each instruction came from while
//! [`almide_wasm::debug_lines::DebugLinesGuard`] is set; after the stock-WASI
//! transform, the record is re-anchored on the bytes that ship and appended
//! as `.debug_*` custom sections. Without `--debug` none of this runs and the
//! artifact is byte-identical.

use crate::err;

/// The recording guard for a `--debug` build, or the refusal of a
/// combination whose bytes the line table could not describe.
pub(super) fn debug_build_guard(component: bool, wasm_opt: bool) -> almide_wasm::debug_lines::DebugLinesGuard {
    // wasm-opt rewrites the module outside the renderer, and a component
    // wraps it: the addresses the table names would not be the shipped ones.
    if component || wasm_opt {
        let flag = if component { "--component" } else { "--wasm-opt" };
        err(&format!(
            "error: --debug describes the core module the renderer ships, and {flag} rewrites it\n  \
             hint: drop {flag} for a debug build"
        ));
        std::process::exit(2);
    }
    almide_wasm::debug_lines::DebugLinesGuard::set()
}

/// `shipped` (the `to_wasi` form of `emitted`, with its defined-function
/// map) plus the line table, when this build records one.
pub(super) fn with_debug_lines(file: &str, emitted: &[u8], shipped: (Vec<u8>, Vec<Option<u32>>)) -> Vec<u8> {
    let (shipped, def_map) = shipped;
    let Some(table) = almide_wasm::debug_lines::take_table().filter(|_| almide_wasm::debug_lines::on()) else {
        return shipped;
    };
    let comp_dir = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_else(|_| ".".into());
    let input = almide_wasm::debug_lines::DwarfIn {
        emitted,
        shipped: &shipped,
        def_map: Some(&def_map),
        table: &table,
        entry_file: file,
        comp_dir: &comp_dir,
        producer: concat!("almide ", env!("CARGO_PKG_VERSION")),
    };
    match almide_wasm::debug_lines::attach_dwarf(&input) {
        Ok((out, stats)) => {
            err(&format!(
                "debug: DWARF line table for {} functions ({} rows, {} bytes of .debug_* sections{})",
                stats.functions,
                stats.rows,
                stats.bytes,
                if stats.skipped > 0 { format!("; {} pruned or unaligned functions left out", stats.skipped) } else { String::new() }
            ));
            out
        }
        Err(e) => {
            err(&format!("error: the DWARF line table could not be written — this is an Almide bug: {e}"));
            std::process::exit(1);
        }
    }
}
