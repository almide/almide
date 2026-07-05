//! Emit a flight-grade witness for a REAL `.almd` program — the G1 end-to-end
//! PCC path (weekly indicator ①: a real source program taken to the proven
//! checker, not a hand-built MIR). Unlike `emit_cert.rs` (which constructs MIR
//! scenarios by hand), this drives the program through the EXISTING frontend
//! pipeline to linked IR, then through `almide_mir::lower` to MIR, and projects
//! the witness. `proofs/gate.sh` pipes the result into the KERNEL-PROVEN checker.
//!
//!   emit_cert_from_source <file.almd> [function=main] [ownership|names|caps]
//!
//! The lowering is the value-semantics subset (lower.rs); a program outside it
//! exits with an explicit `Unsupported` reason (flight-grade totality — never a
//! silent skip). That honest boundary is the point: the gate certifies exactly
//! the programs that lower, and says so for the rest.

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_mir::certificate::{
    cap_witness_string, name_witness_string, ownership_certificate, transitive_cap_witness_string,
};
use almide_optimize::{mono, optimize};
use std::collections::BTreeMap;

fn die(msg: String) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// Lower `.almd` source to a linked `IrProgram` at the pre-codegen cut point
/// (`parse → check → lower → optimize → mono → ir_link`) — the SAME public
/// frontend functions almide-interp uses, so the IR fed to MIR lowering is the
/// real compiler's, not a bespoke one.
fn source_to_ir(source: &str) -> almide_ir::IrProgram {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(tokens);
    let mut prog = match parser.parse() {
        Ok(p) => p,
        Err(e) => die(format!("parse error: {e:?}")),
    };
    if !parser.errors.is_empty() {
        die(format!("parse errors: {:?}", parser.errors));
    }
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
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| {
        die("usage: emit_cert_from_source <file.almd> [function] [ownership|names|caps]".into())
    });
    let func_name = args.next().unwrap_or_else(|| "main".to_string());
    let property = args.next().unwrap_or_else(|| "ownership".to_string());

    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| die(format!("cannot read {path}: {e}")));
    let ir = source_to_ir(&source);

    // Top-level `let` globals (VarId -> declared Ty), union of program- and
    // module-level top_lets — the declared set the lowering uses to admit a global
    // reference instead of walling it.
    let mut globals: std::collections::HashMap<almide_ir::VarId, almide_lang::types::Ty> =
        std::collections::HashMap::new();
    // The globals' INITIALIZERS too — so the emitted CERT covers the same heap-global
    // materialization render_program executes (the global lowers to its real const value).
    let mut global_inits: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
        std::collections::HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    for m in &ir.modules {
        for tl in &m.top_lets {
            globals.insert(tl.var, tl.ty.clone());
            global_inits.insert(tl.var, tl.value.clone());
        }
    }

    let func = ir
        .functions
        .iter()
        .find(|f| f.name.as_str() == func_name)
        // `mir` mode iterates by substring (mono renames generics, e.g. `unbox$String`), so it
        // does not need an exact-name `func`; fall back to the first function to avoid the die.
        .or_else(|| ir.functions.first())
        .unwrap_or_else(|| die(format!("no function `{func_name}` in {path}")));

    if property == "ir" {
        // Debug aid: dump the real linked-IR body the lowering sees.
        eprintln!("{:#?}", func.body);
        return;
    }

    if property == "drops" {
        // Debug aid: print the GENERATED recursive-drop fn source (ADT brick 5b).
        print!("{}", almide_mir::lower::generate_variant_drop_sources(&ir.type_decls));
        return;
    }

    if property == "mir" {
        // Debug aid: dump the MIR ops the variant-aware lowering produces (for ALL functions
        // whose name contains `func_name`, since mono specializes generics like `unbox$String`).
        let mut record_layouts = almide_mir::lower::build_record_layouts(&ir.type_decls);
        let mut variant_layouts = almide_mir::lower::build_variant_layouts(&ir.type_decls);
        for m in &ir.modules {
            record_layouts.extend(almide_mir::lower::build_record_layouts(&m.type_decls));
            let vl = almide_mir::lower::build_variant_layouts(&m.type_decls);
            variant_layouts.by_type.extend(vl.by_type);
            variant_layouts.ctor_to_type.extend(vl.ctor_to_type);
        }
        for f in &ir.functions {
            if !f.name.as_str().contains(func_name.as_str()) {
                continue;
            }
            match almide_mir::lower::lower_function_all_with_globals(
                f, &globals, &global_inits, &record_layouts, &variant_layouts,
            ) {
                Ok(mirs) => {
                    for mir in &mirs {
                        eprintln!("=== {} ===", mir.name);
                        eprintln!("ownership cert: {}", ownership_certificate(mir));
                        for (i, op) in mir.ops.iter().enumerate() {
                            eprintln!("  [{i}] {op:?}");
                        }
                    }
                }
                Err(e) => eprintln!("=== {} WALL: {e:?} ===", f.name.as_str()),
            }
        }
        return;
    }

    // The single ownership+layout DECISION: real linked IR → MIR. Outside the
    // value-semantics subset this is an explicit Unsupported (honest boundary).
    let mir = almide_mir::lower::lower_function(func, &globals)
        .unwrap_or_else(|e| die(format!("lowering `{func_name}` is out of subset: {e:?}")));

    match property.as_str() {
        "ownership" => print!("{}", ownership_certificate(&mir)),
        "names" => print!("{}", name_witness_string(&mir)),
        "caps" => print!("{}", cap_witness_string(&mir)),
        // Transitive capability witness: needs the WHOLE program, so a callee's
        // reachable caps are accounted at the call site (per-call-site rule).
        "tcaps" => {
            let mut program: BTreeMap<String, almide_mir::MirFunction> = BTreeMap::new();
            for f in &ir.functions {
                if let Ok(m) = almide_mir::lower::lower_function(f, &globals) {
                    program.insert(f.name.as_str().to_string(), m);
                }
            }
            print!("{}", transitive_cap_witness_string(&mir, &program));
        }
        other => {
            die(format!("unknown property: {other} (try: ownership | names | caps | tcaps)"))
        }
    }
}
