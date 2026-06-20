//! Render a REAL `.almd` program to a COMPLETE wasm module via the v1 MIR renderer
//! (`render_wasm_program`) — the EXECUTION-side counterpart to emit_cert_from_source
//! (the verification side). Goal: a real program runs through the v1 pipeline and
//! matches v0 byte-identical — ③ execution parity, the path to v0 replacement
//! (docs/roadmap/active/v1-kgi-kpi.md, Gap 3). Functions outside the MIR-lowering
//! subset are reported to stderr (the honest boundary), the rest rendered.
//!
//!   render_program <file.almd>   → emits the wat module to stdout

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_mir::render_wasm::try_render_wasm_program;
use almide_mir::MirProgram;
use almide_optimize::{mono, optimize};
use std::collections::HashMap;

fn die(msg: String) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// Lower `.almd` source to a linked `IrProgram` (`parse → check → lower → optimize →
/// mono → ir_link`) — the SAME frontend cut point emit_cert_from_source uses.
fn source_to_ir(source: &str) -> almide_ir::IrProgram {
    let tokens = Lexer::tokenize(source);
    let mut prog = match Parser::new(tokens).parse() {
        Ok(p) => p,
        Err(e) => die(format!("parse error: {e:?}")),
    };
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .map(|d| d.message.clone())
        .collect();
    if !errors.is_empty() {
        die(format!("type errors: {errors:?}"));
    }
    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
    optimize::optimize_program(&mut ir);
    mono::monomorphize(&mut ir);
    ir_link::ir_link(&mut ir);
    ir
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| die("usage: render_program <file.almd>".into()));
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| die(format!("cannot read {path}: {e}")));
    let ir = source_to_ir(&source);

    // Top-level `let` globals (VarId -> Ty), union of program + module top_lets.
    let mut globals: HashMap<almide_ir::VarId, almide_lang::types::Ty> = HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
    }
    for m in &ir.modules {
        for tl in &m.top_lets {
            globals.insert(tl.var, tl.ty.clone());
        }
    }

    // The record-layout registry (type name → fields) for the VALUE MODEL: a record
    // literal / `r.x` typed `Ty::Named` resolves its field offsets here. Built from the
    // program's type declarations (+ each module's).
    let mut record_layouts = almide_mir::lower::build_record_layouts(&ir.type_decls);
    for m in &ir.modules {
        record_layouts.extend(almide_mir::lower::build_record_layouts(&m.type_decls));
    }

    // The variant-layout registry (type name → tag + per-constructor fields) for custom
    // ADTs, the value-model sibling of `record_layouts`. A variant construct / `match`
    // resolves its tag + field slots here.
    let mut variant_layouts = almide_mir::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        let m_vl = almide_mir::lower::build_variant_layouts(&m.type_decls);
        variant_layouts.by_type.extend(m_vl.by_type);
        variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
    }

    // PROGRAM pre-pass: inline mutual-recursive tail siblings so the parser loops become direct
    // self-recursion (exposed to the append-accumulator TCO). Guarded: only where it makes a walled
    // function lower (no regression). Semantics-preserving.
    let inlined_fns =
        almide_mir::lower::inline_mutual_tail_recursion(&ir.functions, &globals, &record_layouts);

    let mut functions = Vec::new();
    let mut walled = Vec::new();
    for func in &inlined_fns {
        // lower_function_all_with_types returns the main function plus any lambda-lifted
        // auxiliaries (index 0 is main); all go into the module so the function table
        // covers them. The record registry is threaded so `Ty::Named` records materialize.
        match almide_mir::lower::lower_function_all_with_layouts(
            func,
            &globals,
            &record_layouts,
            &variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            Err(e) => walled.push(format!("{}: {e:?}", func.name.as_str())),
        }
    }
    if !walled.is_empty() {
        eprintln!(
            "[render_program] {} of {} function(s) outside the lowering subset (NOT rendered):",
            walled.len(),
            ir.functions.len()
        );
        for w in &walled {
            eprintln!("  {w}");
        }
    }

    // Auto-link the self-hosted stdlib runtime (the registry — int.to_string, string.concat,
    // …) when an entry is called but not defined, renaming its impl fn to the call name. A linked
    // impl may itself call ANOTHER registry entry (e.g. `list.to_string_f` → `float.to_string`), so
    // iterate to a FIXPOINT: keep linking until a full pass adds nothing. (The test harness
    // `lower_source` gets this transitive closure for free via its recursive auto-link; this loop
    // is the example-side equivalent.)
    loop {
        let before = functions.len();
        for (source, entries) in almide_mir::render_wasm::self_host_runtime() {
            let any_called = entries.iter().any(|(_, call)| {
                functions.iter().any(|f| {
                    f.ops.iter().any(|op| matches!(op, almide_mir::Op::CallFn { name, .. } if name == call))
                })
            });
            let any_defined =
                entries.iter().any(|(_, call)| functions.iter().any(|f| &f.name == call));
            if any_called && !any_defined {
                let rt = source_to_ir(source);
                for f in &rt.functions {
                    if let Ok(mut mir) = almide_mir::lower::lower_function(f, &globals) {
                        if let Some((_, call)) = entries.iter().find(|(impl_fn, _)| &mir.name == impl_fn) {
                            mir.name = call.to_string();
                        }
                        functions.push(mir);
                    }
                }
            }
        }
        // Dedup by name (a recursively-linked impl carries its own helper copies, e.g. print_str);
        // keep the first definition — identical source ⇒ a no-op merge.
        let mut seen = std::collections::HashSet::new();
        functions.retain(|f| seen.insert(f.name.clone()));
        if functions.len() == before {
            break;
        }
    }

    // Auto-link the self-hosted runtime: `println(s)` lowers to a `PrintStr` call
    // rendered as `(call $print_str ...)`, so a program that prints needs the
    // Almide-written `print_str` (compiled through this same pipeline). Include the
    // bundled runtime unless the program already defines it — the v1 runtime-linking
    // step (the self-host vision: no hand-written wasm for print).
    if !functions.iter().any(|f| f.name == "print_str") {
        let rt = source_to_ir(include_str!("../../../stdlib/print_str.almd"));
        for f in &rt.functions {
            if let Ok(mir) = almide_mir::lower::lower_function(f, &globals) {
                functions.push(mir);
            }
        }
    }

    // If `main` itself was WALLED out of the lowering subset (it needs a capability,
    // a RawPtr with no scalar Repr, etc.), there is no `$main` in `functions` — yet
    // render_wasm_program unconditionally emits `(func (export "_start") (call $main))`.
    // That dangling `$main` is invalid wasm. Wall the WHOLE program cleanly instead of
    // emitting a main-less module, so the sweep categorizes it as a wall (not a RUNERR).
    if !functions.iter().any(|f| f.name == "main") {
        die("[render_program] Unsupported: main is outside the MIR-lowering subset".into());
    }

    // Wall any UNLINKED stdlib/runtime call: a `(call $name)` to a function that is
    // neither defined here (user / auto-linked self-host / print_str) nor a preamble
    // runtime fn would be a dangling reference (invalid wasm). Reject cleanly instead of
    // emitting it — conservative and structurally sound (it only removes a bad module).
    match try_render_wasm_program(&MirProgram { functions }) {
        Ok(wat) => print!("{wat}"),
        Err(e) => die(format!("[render_program] {e:?}")),
    }
}
