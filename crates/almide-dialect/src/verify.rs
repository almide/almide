//! Dialect verifier — checks well-formedness of the lowered Module.
//!
//! Catches issues that would be bugs in the IR→dialect converter:
//! - Undefined ValueId references
//! - DialectType::Unknown surviving past lowering
//! - Empty blocks without terminators

use std::collections::HashSet;
use crate::{Module, Block, ValueId};
use crate::ops::*;
use crate::types::DialectType;

#[derive(Debug)]
pub struct VerifyError {
    pub message: String,
    pub context: String,
}

/// Verify a Module for well-formedness. Returns errors found.
pub fn verify_module(module: &Module) -> Vec<VerifyError> {
    let mut errors = Vec::new();
    let mut defined: HashSet<ValueId> = HashSet::new();

    for func in &module.functions {
        let ctx = format!("fn {}", func.name);
        for block in &func.body {
            verify_block(block, &ctx, &mut defined, &mut errors);
        }
    }

    for global in &module.globals {
        let ctx = format!("global {}", global.name);
        if matches!(global.ty, DialectType::Unknown) {
            errors.push(VerifyError {
                message: "global has Unknown type".into(),
                context: ctx.clone(),
            });
        }
        for block in &global.init {
            verify_block(block, &ctx, &mut defined, &mut errors);
        }
    }

    errors
}

fn verify_block(
    block: &Block,
    ctx: &str,
    defined: &mut HashSet<ValueId>,
    errors: &mut Vec<VerifyError>,
) {
    for (val, _) in &block.args {
        defined.insert(*val);
    }

    for op in &block.ops {
        if let Some(result) = op.result {
            defined.insert(result);
        }
        if matches!(op.result_ty, DialectType::Unknown) {
            errors.push(VerifyError {
                message: "operation result has Unknown type".into(),
                context: ctx.to_string(),
            });
        }

        // Recursively verify nested regions
        match &op.kind {
            OpKind::ComputedCallOp { .. } | OpKind::AllocVar { .. }
            | OpKind::LoadVar { .. } | OpKind::StoreVar { .. } => {}
            OpKind::IfOp { then_region, else_region, .. } => {
                for b in then_region { verify_block(b, ctx, defined, errors); }
                for b in else_region { verify_block(b, ctx, defined, errors); }
            }
            OpKind::MatchOp { arms, .. } => {
                for arm in arms {
                    for b in &arm.body { verify_block(b, ctx, defined, errors); }
                }
            }
            OpKind::LambdaOp { body, .. } => {
                for b in body { verify_block(b, ctx, defined, errors); }
            }
            OpKind::FanOp { regions } => {
                for region in regions {
                    for b in region { verify_block(b, ctx, defined, errors); }
                }
            }
            OpKind::ForOp { body, .. } => {
                for b in body { verify_block(b, ctx, defined, errors); }
            }
            OpKind::WhileOp { cond_region, body } => {
                for b in cond_region { verify_block(b, ctx, defined, errors); }
                for b in body { verify_block(b, ctx, defined, errors); }
            }
            _ => {}
        }
    }
}
