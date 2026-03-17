//! ClonePass: mark variables that need .clone() in Rust.
//!
//! Walks the IR and identifies variables of heap types (String, Vec, HashMap,
//! records, Option, Result) that are used in expression position.
//! Adds their VarIds to CodegenAnnotations.clone_vars.

use std::collections::HashSet;
use crate::ir::*;
use crate::types::Ty;
use super::annotations::CodegenAnnotations;
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct ClonePass;

impl NanoPass for ClonePass {
    fn name(&self) -> &str { "CloneInsertion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        // This pass doesn't modify IR — it populates annotations
        // Annotations are set separately via annotate()
    }
}

/// Collect all VarIds that need .clone() based on their type.
pub fn collect_clone_vars(program: &IrProgram) -> HashSet<VarId> {
    let mut clone_vars = HashSet::new();
    let vt = &program.var_table;

    for i in 0..vt.len() {
        let id = VarId(i as u32);
        let info = vt.get(id);
        if needs_clone(&info.ty) {
            clone_vars.insert(id);
        }
    }

    clone_vars
}

fn needs_clone(ty: &Ty) -> bool {
    matches!(ty,
        Ty::String | Ty::List(_) | Ty::Map(_, _) |
        Ty::Record { .. } | Ty::OpenRecord { .. } |
        Ty::Named(_, _) | Ty::Option(_) | Ty::Result(_, _) |
        Ty::Variant { .. }
    )
}
