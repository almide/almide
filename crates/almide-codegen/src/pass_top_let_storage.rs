//! TopLetStoragePass — completeness-by-construction §4, Stage 1.
//!
//! Computes the unified top-let storage attribute ONCE (see
//! `almide_ir::top_let_storage` for the rules) and records it in
//! `codegen_annotations`. Pure analysis — no IR rewrite. Runs at the END of
//! both pipelines so VarIds and types are final; on the Rust target the
//! module top-lets are already flattened into `program.top_lets`
//! (IrLinkFlatten), on wasm they are still per-module — both shapes are
//! covered.
//!
//! TOTALITY: every VarId whose `VarInfo.module_origin` is set must resolve
//! to a declaration (it is either a decl itself or a synthetic use-site
//! reference). A miss is exactly the #500 silent-zero / #486 E0507 class —
//! the build is refused with a structured compiler-bug report instead of
//! letting the walker/emitter improvise.

use std::collections::HashMap;
use almide_ir::*;
use almide_ir::top_let_storage::{build_global_tables, top_let_inputs, GlobalInfo};
use crate::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct TopLetStoragePass;

impl NanoPass for TopLetStoragePass {
    fn name(&self) -> &str { "TopLetStorage" }

    fn targets(&self) -> Option<Vec<Target>> { None } // both targets

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut inputs: Vec<(bool, TopLetKind, VarId, bool)> = Vec::new();
        let mut init_order: Vec<VarId> = Vec::new();
        for tl in &program.top_lets {
            inputs.push(top_let_inputs(tl));
            init_order.push(tl.var);
        }
        for m in &program.modules {
            for tl in &m.top_lets {
                inputs.push(top_let_inputs(tl));
                init_order.push(tl.var);
            }
        }

        let (globals, alias, offenders) = build_global_tables(&inputs, &program.var_table);

        if !offenders.is_empty() {
            let mut msg = String::new();
            msg.push_str("error: [COMPILER BUG] unresolvable module-global reference(s)\n");
            msg.push_str(&format!(
                "  {} VarId(s) carry a module_origin but match no top-let declaration —\n",
                offenders.len()
            ));
            msg.push_str("  the walker/emitter would improvise storage for them (the #486/#500\n");
            msg.push_str("  silent-miscompile class), so the build is refused instead.\n");
            const MAX_LISTED: usize = 10;
            for o in offenders.iter().take(MAX_LISTED) {
                msg.push_str(&format!("    - {}\n", o));
            }
            if offenders.len() > MAX_LISTED {
                msg.push_str(&format!("    … and {} more\n", offenders.len() - MAX_LISTED));
            }
            msg.push_str("  Please report this at https://github.com/almide/almide/issues\n");
            eprint!("{}", msg);
            std::process::exit(1);
        }

        let globals: HashMap<VarId, GlobalInfo> = globals;
        program.codegen_annotations.globals = globals;
        program.codegen_annotations.global_alias = alias;
        program.codegen_annotations.global_init_order = init_order;
        PassResult { program, changed: false }
    }
}
