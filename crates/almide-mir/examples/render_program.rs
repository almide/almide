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
use almide_mir::render_wasm::render_wasm_program;
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

    let mut functions = Vec::new();
    let mut walled = Vec::new();
    for func in &ir.functions {
        match almide_mir::lower::lower_function(func, &globals) {
            Ok(mir) => functions.push(mir),
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

    print!("{}", render_wasm_program(&MirProgram { functions }));
}
