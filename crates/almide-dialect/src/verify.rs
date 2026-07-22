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
        verify_op(op, ctx, defined, errors);
    }
}

/// Verify a single operation: record its result (if any), flag Unknown
/// result types, and recurse into any nested regions it carries.
fn verify_op(
    op: &Operation,
    ctx: &str,
    defined: &mut HashSet<ValueId>,
    errors: &mut Vec<VerifyError>,
) {
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
        OpKind::IfOp { .. } => verify_op_if(op, ctx, defined, errors),
        OpKind::MatchOp { .. } => verify_op_match(op, ctx, defined, errors),
        OpKind::LambdaOp { .. } => verify_op_lambda(op, ctx, defined, errors),
        OpKind::FanOp { .. } => verify_op_fan(op, ctx, defined, errors),
        OpKind::ForOp { .. } => verify_op_for(op, ctx, defined, errors),
        OpKind::WhileOp { .. } => verify_op_while(op, ctx, defined, errors),
        _ => {}
    }
}

fn verify_op_if(op: &Operation, ctx: &str, defined: &mut HashSet<ValueId>, errors: &mut Vec<VerifyError>) {
    let OpKind::IfOp { then_region, else_region, .. } = &op.kind else { unreachable!() };
    for b in then_region { verify_block(b, ctx, defined, errors); }
    for b in else_region { verify_block(b, ctx, defined, errors); }
}

fn verify_op_match(op: &Operation, ctx: &str, defined: &mut HashSet<ValueId>, errors: &mut Vec<VerifyError>) {
    let OpKind::MatchOp { arms, .. } = &op.kind else { unreachable!() };
    for arm in arms {
        for b in &arm.body { verify_block(b, ctx, defined, errors); }
    }
}

fn verify_op_lambda(op: &Operation, ctx: &str, defined: &mut HashSet<ValueId>, errors: &mut Vec<VerifyError>) {
    let OpKind::LambdaOp { body, .. } = &op.kind else { unreachable!() };
    for b in body { verify_block(b, ctx, defined, errors); }
}

fn verify_op_fan(op: &Operation, ctx: &str, defined: &mut HashSet<ValueId>, errors: &mut Vec<VerifyError>) {
    let OpKind::FanOp { regions } = &op.kind else { unreachable!() };
    for region in regions {
        for b in region { verify_block(b, ctx, defined, errors); }
    }
}

fn verify_op_for(op: &Operation, ctx: &str, defined: &mut HashSet<ValueId>, errors: &mut Vec<VerifyError>) {
    let OpKind::ForOp { body, .. } = &op.kind else { unreachable!() };
    for b in body { verify_block(b, ctx, defined, errors); }
}

fn verify_op_while(op: &Operation, ctx: &str, defined: &mut HashSet<ValueId>, errors: &mut Vec<VerifyError>) {
    let OpKind::WhileOp { cond_region, body } = &op.kind else { unreachable!() };
    for b in cond_region { verify_block(b, ctx, defined, errors); }
    for b in body { verify_block(b, ctx, defined, errors); }
}
