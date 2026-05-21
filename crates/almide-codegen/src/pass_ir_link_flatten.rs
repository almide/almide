// ── IR Link Flatten Pass ─────────────────────────────────────────────
//
// Final nanopass: merges program.modules into root functions/types/top_lets.
// MUST run after UnifyVarTablesPass (VarIds already unified).
// After this, program.modules is empty. Walker renders flat program.

use almide_ir::*;
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Target};
use std::collections::HashSet;

#[derive(Debug)]
pub struct IrLinkFlattenPass;

impl NanoPass for IrLinkFlattenPass {
    fn name(&self) -> &str { "IrLinkFlatten" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        if program.modules.is_empty() {
            return PassResult { program, changed: false };
        }

        let modules = std::mem::take(&mut program.modules);

        let mut emitted_types: HashSet<String> = program.type_decls.iter()
            .map(|td| td.name.as_str().to_string())
            .collect();

        for module in modules {
            let mod_ident = module.versioned_name
                .map(|v| v.to_string().replace('.', "_"))
                .unwrap_or_else(|| module.name.to_string().replace('.', "_"));

            // Merge type declarations (deduplicate by name)
            for td in module.type_decls {
                let name = td.name.as_str().to_string();
                if !emitted_types.contains(&name) {
                    emitted_types.insert(name);
                    program.type_decls.push(td);
                }
            }

            // Merge functions with prefixed names
            for mut func in module.functions {
                let clean_name = func.name.as_str()
                    .replace(' ', "_").replace('-', "_").replace('.', "_");
                let prefixed = format!("almide_rt_{}_{}", mod_ident, clean_name);
                func.name = sym(&prefixed);
                program.functions.push(func);
            }

            // Merge top_lets with prefixed names (single owner of prefix logic)
            let mod_upper = mod_ident.to_uppercase();
            for tl in module.top_lets {
                let old_name = program.var_table.get(tl.var).name;
                let expected_prefix = format!("ALMIDE_RT_{}_", mod_upper);
                if !old_name.as_str().to_uppercase().starts_with(&expected_prefix) {
                    let new_name = format!("ALMIDE_RT_{}_{}", mod_upper, old_name.as_str().to_uppercase());
                    program.var_table.entries[tl.var.0 as usize].name = sym(&new_name);
                }
                program.top_lets.push(tl);
            }
        }

        PassResult { program, changed: true }
    }
}
