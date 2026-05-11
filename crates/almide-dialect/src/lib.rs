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
#[cfg(feature = "llvm")]
pub mod emit_llvm;

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

    fn count_in_op(kind: &ops::OpKind, counts: &mut std::collections::HashMap<ValueId, usize>) {
        use ops::OpKind::*;
        match kind {
            BinOp { lhs, rhs, .. } => { *counts.entry(*lhs).or_default() += 1; *counts.entry(*rhs).or_default() += 1; }
            UnOp { operand, .. } => { *counts.entry(*operand).or_default() += 1; }
            CallOp { args, .. } | IntrinsicCallOp { args, .. } => { for a in args { *counts.entry(*a).or_default() += 1; } }
            IfOp { cond, then_region, else_region } => {
                *counts.entry(*cond).or_default() += 1;
                count_in_blocks(then_region, counts);
                count_in_blocks(else_region, counts);
            }
            MatchOp { subject, arms } => {
                *counts.entry(*subject).or_default() += 1;
                for arm in arms { count_in_blocks(&arm.body, counts); }
            }
            ListOp { elements } | TupleOp { elements } => { for e in elements { *counts.entry(*e).or_default() += 1; } }
            MapOp { entries } => { for (k, v) in entries { *counts.entry(*k).or_default() += 1; *counts.entry(*v).or_default() += 1; } }
            RecordOp { fields, .. } => { for (_, v) in fields { *counts.entry(*v).or_default() += 1; } }
            MemberOp { object, .. } | TupleIndexOp { object, .. } => { *counts.entry(*object).or_default() += 1; }
            IndexOp { object, index } | MapAccessOp { object, key: index } => {
                *counts.entry(*object).or_default() += 1; *counts.entry(*index).or_default() += 1;
            }
            ResultOkOp { value } | ResultErrOp { value } | OptionSomeOp { value }
            | TryOp { value } | UnwrapOp { value } => { *counts.entry(*value).or_default() += 1; }
            UnwrapOrOp { value, fallback } => { *counts.entry(*value).or_default() += 1; *counts.entry(*fallback).or_default() += 1; }
            LambdaOp { body, .. } => { count_in_blocks(body, counts); }
            FanOp { regions } => { for r in regions { count_in_blocks(r, counts); } }
            ForOp { iterable, body, .. } => { *counts.entry(*iterable).or_default() += 1; count_in_blocks(body, counts); }
            WhileOp { cond_region, body } => { count_in_blocks(cond_region, counts); count_in_blocks(body, counts); }
            _ => {}
        }
    }

    for f in &module.functions {
        count_in_blocks(&f.body, &mut counts);
    }
    for g in &module.globals {
        count_in_blocks(&g.init, &mut counts);
    }
    counts
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
