//! Almide dialect operations.
//!
//! Each variant of `OpKind` corresponds to an MLIR operation
//! in the `almide` dialect namespace.

use almide_base::intern::Sym;
use super::{ValueId, BlockId, Block};
use super::types::DialectType;

/// A single SSA operation.
#[derive(Debug, Clone)]
pub struct Operation {
    /// Result value (None for void ops like Store, Print).
    pub result: Option<ValueId>,
    /// Result type.
    pub result_ty: DialectType,
    /// The operation itself.
    pub kind: OpKind,
}

/// Operation kinds in the Almide dialect.
#[derive(Debug, Clone)]
pub enum OpKind {
    // ── Constants ──
    ConstInt(i64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstString(String),
    ConstUnit,

    // ── Arithmetic (type-dispatched at IR level, preserved here) ──
    BinOp {
        op: almide_ir::BinOp,
        lhs: ValueId,
        rhs: ValueId,
    },
    UnOp {
        op: almide_ir::UnOp,
        operand: ValueId,
    },

    // ── Control flow (structured, using regions) ──
    /// if-then-else with two single-block regions
    IfOp {
        cond: ValueId,
        then_region: Vec<Block>,
        else_region: Vec<Block>,
    },
    /// Pattern match — each arm is a region with a guard + body
    MatchOp {
        subject: ValueId,
        arms: Vec<MatchArm>,
    },

    // ── Calls ──
    /// Direct function call
    CallOp {
        callee: Sym,
        args: Vec<ValueId>,
    },
    /// Computed call (closure / fn variable)
    ComputedCallOp {
        callee: ValueId,
        args: Vec<ValueId>,
    },
    /// Runtime intrinsic call (stdlib)
    IntrinsicCallOp {
        symbol: Sym,
        args: Vec<ValueId>,
    },

    // ── Collections ──
    ListOp { elements: Vec<ValueId> },
    MapOp { entries: Vec<(ValueId, ValueId)> },
    EmptyMapOp,
    RecordOp {
        name: Option<Sym>,
        fields: Vec<(Sym, ValueId)>,
    },
    TupleOp { elements: Vec<ValueId> },

    // ── Access ──
    MemberOp { object: ValueId, field: Sym },
    TupleIndexOp { object: ValueId, index: usize },
    IndexOp { object: ValueId, index: ValueId },
    MapAccessOp { object: ValueId, key: ValueId },

    // ── Result / Option ──
    ResultOkOp { value: ValueId },
    ResultErrOp { value: ValueId },
    OptionSomeOp { value: ValueId },
    OptionNoneOp,
    TryOp { value: ValueId },
    UnwrapOp { value: ValueId },
    UnwrapOrOp { value: ValueId, fallback: ValueId },

    // ── Lambda ──
    LambdaOp {
        params: Vec<(ValueId, DialectType)>,
        body: Vec<Block>,
    },

    // ── Effect ──
    /// Fan (structured concurrency)
    FanOp { regions: Vec<Vec<Block>> },

    // ── Loops ──
    ForOp {
        var: ValueId,
        iterable: ValueId,
        body: Vec<Block>,
    },
    WhileOp {
        cond_region: Vec<Block>,
        body: Vec<Block>,
    },
}

/// Match arm with pattern + body region.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<ValueId>,
    pub body: Vec<Block>,
}

/// Simplified match patterns at dialect level.
#[derive(Debug, Clone)]
pub enum MatchPattern {
    Wildcard,
    /// Literal value in pattern position (not a ValueId reference).
    LitInt(i64),
    LitStr(String),
    LitBool(bool),
    Binding(ValueId),
    Variant { tag: Sym, bindings: Vec<ValueId> },
    Record { fields: Vec<(Sym, ValueId)> },
    Tuple(Vec<MatchPattern>),
}

/// Block terminators.
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Return a value from the enclosing function/region.
    Yield(ValueId),
    /// Return from function.
    Return(ValueId),
    /// Branch to another block.
    Branch(BlockId, Vec<ValueId>),
    /// Conditional branch.
    CondBranch {
        cond: ValueId,
        true_dest: BlockId,
        false_dest: BlockId,
    },
    /// No terminator (block continues to next).
    Fallthrough,
    /// Loop break.
    Break,
    /// Loop continue.
    Continue,
}

/// Function operation (top-level).
#[derive(Debug, Clone)]
pub struct FuncOp {
    pub name: Sym,
    pub params: Vec<(Sym, DialectType)>,
    pub ret_ty: DialectType,
    pub is_effect: bool,
    pub is_test: bool,
    pub body: Vec<Block>,
}

/// Type declaration at module level.
#[derive(Debug, Clone)]
pub struct TypeDeclOp {
    pub name: Sym,
    pub kind: TypeDeclKind,
}

#[derive(Debug, Clone)]
pub enum TypeDeclKind {
    Record { fields: Vec<(Sym, DialectType)> },
    Variant { cases: Vec<VariantCase> },
    Alias(DialectType),
}

#[derive(Debug, Clone)]
pub struct VariantCase {
    pub name: Sym,
    pub payload: Vec<DialectType>,
}

/// Global variable (top-level let).
#[derive(Debug, Clone)]
pub struct GlobalOp {
    pub name: Sym,
    pub ty: DialectType,
    pub init: Vec<Block>,
}
