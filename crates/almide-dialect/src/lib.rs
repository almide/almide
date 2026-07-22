//! Almide MLIR dialect schema — pure-Rust, FFI-free.
//!
//! Models MLIR's Region/Block/Operation hierarchy as Rust types.
//! When the real MLIR backend arrives (melior FFI), these types
//! become the source-of-truth schema: the converter produces them,
//! and a thin FFI layer serializes them into MLIR C API calls.
//!
//! Design:
//!   IrProgram → lower::lower_program() → Module (this crate)
//!   Module contains FuncOp, each containing Blocks of Operations.

pub mod types;
pub mod ops;
pub mod lower;
pub mod verify;
pub mod dump;
pub mod emit_rust;

use almide_base::intern::Sym;

/// A Module is the top-level container (corresponds to `mlir::ModuleOp`).
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Option<Sym>,
    pub functions: Vec<ops::FuncOp>,
    pub type_decls: Vec<ops::TypeDeclOp>,
    pub globals: Vec<ops::GlobalOp>,
}

/// SSA value reference — every computed value gets a unique ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);

/// Block label for control flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

/// A Block contains a sequence of Operations and block arguments.
#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub args: Vec<(ValueId, types::DialectType)>,
    pub ops: Vec<ops::Operation>,
    pub terminator: ops::Terminator,
}

/// Collect use-counts for all ValueIds in a Module.
pub fn compute_use_counts(module: &Module) -> std::collections::HashMap<ValueId, usize> {
    let mut counts: std::collections::HashMap<ValueId, usize> = std::collections::HashMap::new();

    for f in &module.functions {
        count_in_blocks(&f.body, &mut counts);
    }
    for g in &module.globals {
        count_in_blocks(&g.init, &mut counts);
    }
    counts
}

fn count_in_blocks(blocks: &[Block], counts: &mut std::collections::HashMap<ValueId, usize>) {
    for block in blocks {
        for op in &block.ops {
            count_in_op(&op.kind, counts);
        }
        match &block.terminator {
            ops::Terminator::Yield(v) | ops::Terminator::Return(v) => { *counts.entry(*v).or_default() += 1; }
            ops::Terminator::CondBranch { cond, .. } => { *counts.entry(*cond).or_default() += 1; }
            ops::Terminator::Branch(_, args) => { for a in args { *counts.entry(*a).or_default() += 1; } }
            _ => {}
        }
    }
}

/// Count ValueId references produced by a single op: dispatch to the
/// non-recursive group (records direct refs only) or the region-recursive
/// group (records direct refs, then walks nested blocks).
fn count_in_op(kind: &ops::OpKind, counts: &mut std::collections::HashMap<ValueId, usize>) {
    use ops::OpKind::*;
    match kind {
        BinOp { .. } | UnOp { .. } | CallOp { .. } | IntrinsicCallOp { .. }
        | ListOp { .. } | TupleOp { .. } | MapOp { .. } | RecordOp { .. }
        | MemberOp { .. } | TupleIndexOp { .. } | IndexOp { .. } | MapAccessOp { .. }
        | ResultOkOp { .. } | ResultErrOp { .. } | OptionSomeOp { .. } | TryOp { .. }
        | UnwrapOp { .. } | UnwrapOrOp { .. } | AllocVar { .. } | LoadVar { .. } | StoreVar { .. } =>
            count_in_op_scalar(kind, counts),
        IfOp { .. } | MatchOp { .. } | LambdaOp { .. } | FanOp { .. } | ForOp { .. } | WhileOp { .. } =>
            count_in_op_recursive(kind, counts),
        _ => {}
    }
}

fn count_in_op_scalar(kind: &ops::OpKind, counts: &mut std::collections::HashMap<ValueId, usize>) {
    use ops::OpKind::*;
    match kind {
        CallOp { .. } | IntrinsicCallOp { .. } | ListOp { .. } | TupleOp { .. }
        | MapOp { .. } | RecordOp { .. } => count_in_op_scalar_collection(kind, counts),
        _ => count_in_op_scalar_refs(kind, counts),
    }
}

/// Scalar op kinds whose ValueId references live in `Vec`/list-like fields —
/// each needs a loop to walk.
fn count_in_op_scalar_collection(kind: &ops::OpKind, counts: &mut std::collections::HashMap<ValueId, usize>) {
    use ops::OpKind::*;
    match kind {
        CallOp { args, .. } | IntrinsicCallOp { args, .. } => { for a in args { *counts.entry(*a).or_default() += 1; } }
        ListOp { elements } | TupleOp { elements } => { for e in elements { *counts.entry(*e).or_default() += 1; } }
        MapOp { entries } => { for (k, v) in entries { *counts.entry(*k).or_default() += 1; *counts.entry(*v).or_default() += 1; } }
        RecordOp { fields, .. } => { for (_, v) in fields { *counts.entry(*v).or_default() += 1; } }
        _ => {}
    }
}

/// Scalar op kinds whose ValueId references are direct fields (no loop needed).
fn count_in_op_scalar_refs(kind: &ops::OpKind, counts: &mut std::collections::HashMap<ValueId, usize>) {
    use ops::OpKind::*;
    match kind {
        BinOp { lhs, rhs, .. } => { *counts.entry(*lhs).or_default() += 1; *counts.entry(*rhs).or_default() += 1; }
        UnOp { operand, .. } => { *counts.entry(*operand).or_default() += 1; }
        MemberOp { object, .. } | TupleIndexOp { object, .. } => { *counts.entry(*object).or_default() += 1; }
        IndexOp { object, index } | MapAccessOp { object, key: index } => {
            *counts.entry(*object).or_default() += 1; *counts.entry(*index).or_default() += 1;
        }
        ResultOkOp { value } | ResultErrOp { value } | OptionSomeOp { value }
        | TryOp { value } | UnwrapOp { value } => { *counts.entry(*value).or_default() += 1; }
        UnwrapOrOp { value, fallback } => { *counts.entry(*value).or_default() += 1; *counts.entry(*fallback).or_default() += 1; }
        AllocVar { init, .. } => { *counts.entry(*init).or_default() += 1; }
        LoadVar { slot } => { *counts.entry(*slot).or_default() += 1; }
        StoreVar { slot, value } => { *counts.entry(*slot).or_default() += 1; *counts.entry(*value).or_default() += 1; }
        _ => {}
    }
}

fn count_in_op_recursive(kind: &ops::OpKind, counts: &mut std::collections::HashMap<ValueId, usize>) {
    use ops::OpKind::*;
    match kind {
        IfOp { cond, then_region, else_region } => {
            *counts.entry(*cond).or_default() += 1;
            count_in_blocks(then_region, counts);
            count_in_blocks(else_region, counts);
        }
        MatchOp { subject, arms } => {
            *counts.entry(*subject).or_default() += 1;
            for arm in arms { count_in_blocks(&arm.body, counts); }
        }
        LambdaOp { body, .. } => { count_in_blocks(body, counts); }
        FanOp { regions } => { for r in regions { count_in_blocks(r, counts); } }
        ForOp { iterable, body, .. } => { *counts.entry(*iterable).or_default() += 1; count_in_blocks(body, counts); }
        WhileOp { cond_region, body } => { count_in_blocks(cond_region, counts); count_in_blocks(body, counts); }
        _ => {}
    }
}

/// Generator for fresh ValueIds and BlockIds.
#[derive(Debug, Default)]
pub struct IdGen {
    next_value: u32,
    next_block: u32,
}

impl IdGen {
    pub fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    pub fn fresh_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        id
    }
}
