//! The ops whose native runtime takes its source either way (#3398): the
//! list range ops and the whole-map reads.
//!
//! `list.take(xs, n)` and its family only READ their source, so they borrow it
//! (`&[T]`, #3397): a source read again afterwards is never cloned whole. Where
//! the source is NOT read again, the borrow costs a clone of every element the
//! op keeps — 2× on a `list.drop(xs, 1)` tail recursion over strings. Each op
//! here has an `_owned` twin in `runtime/rs/src/list.rs` (`map.rs` for the map
//! reads) that takes the source by value and moves the kept elements out.
//!
//! Who decides which entry a call site gets:
//! - CloneInsertion (and TailCallOpt, for the loop params whose moves it owns)
//!   leave the source a bare `Var` where their last-use analysis says a move
//!   is legal, and a `Borrow` everywhere else.
//! - BorrowLowering spells the result: a source that is an owned value (that
//!   bare `Var`, or a temporary) calls the `_owned` twin by value; one that is a
//!   reference in Rust (a by-reference param or binder, a global, a shared
//!   cell) stays borrowed.
//!
//! A later argument that reads the source again (`list.slice(xs, 1,
//! list.len(xs))`) would make every source a non-last use. Where those reads
//! are only scalar borrows ([`SCALAR_SOURCE_READS`]), OwnedSourceHoist binds
//! the later arguments first (`pass_owned_source_hoist.rs`, #3519), so the
//! source is the call's last read and the two deciders above may move it.
//!
//! Gated by `tests/list_read_only_source_borrow_test.rs`.

use almide_ir::{IrExpr, IrExprKind, IrStmtKind, VarId};
use almide_lang::types::Ty;

/// `(borrowing runtime symbol, owned twin)`.
pub const OWNED_SOURCE_TWINS: &[(&str, &str)] = &[
    ("almide_rt_list_take", "almide_rt_list_take_owned"),
    ("almide_rt_list_drop", "almide_rt_list_drop_owned"),
    ("almide_rt_list_take_while", "almide_rt_list_take_while_owned"),
    ("almide_rt_list_drop_while", "almide_rt_list_drop_while_owned"),
    ("almide_rt_list_find", "almide_rt_list_find_owned"),
    ("almide_rt_list_slice", "almide_rt_list_slice_owned"),
    ("almide_rt_list_take_end", "almide_rt_list_take_end_owned"),
    ("almide_rt_list_drop_end", "almide_rt_list_drop_end_owned"),
    // Whole-map reads: the twin moves every key / value out of a source that
    // is not read again instead of cloning each one (`runtime/rs/src/map.rs`).
    ("almide_rt_map_keys", "almide_rt_map_keys_owned"),
    ("almide_rt_map_values", "almide_rt_map_values_owned"),
    ("almide_rt_map_entries", "almide_rt_map_entries_owned"),
    ("almide_rt_map_map_values", "almide_rt_map_map_values_owned"),
];

/// The owned twin of a borrowing range op, if it has one.
pub fn owned_twin(symbol: &str) -> Option<&'static str> {
    OWNED_SOURCE_TWINS.iter().find(|(b, _)| *b == symbol).map(|(_, o)| *o)
}

/// Is `symbol` a range op whose first argument may be moved instead of
/// borrowed?
pub fn takes_source_either_way(symbol: &str) -> bool {
    owned_twin(symbol).is_some()
}

/// Runtime reads that borrow a list and return a Copy scalar: the borrow ends
/// when the call returns, so a read bound to a `let` holds nothing of the
/// list, and the list may move right after it (#3519).
pub const SCALAR_SOURCE_READS: &[&str] = &["almide_rt_list_len", "almide_rt_list_is_empty"];

/// The var a source `Borrow` reads (`&p`), if that is its shape.
pub fn borrowed_var(arg: &IrExpr) -> Option<VarId> {
    match &arg.kind {
        IrExprKind::Borrow { expr, as_str: false, mutable: false } => match &expr.kind {
            IrExprKind::Var { id } => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

/// `Some(reads_src)` when `e` is a later argument that may be bound before a
/// move of `src`: a scalar built from literals, scalar vars other than `src`,
/// operators, `if` over those, and [`SCALAR_SOURCE_READS`] of `src` itself.
/// `None` for anything else — a call, a lambda, a block, any other read of
/// `src` — which keeps
/// today's borrow. Nothing here has an effect, so binding it early is
/// unobservable.
pub fn scalar_read_of(e: &IrExpr, src: VarId) -> Option<bool> {
    if !matches!(e.ty, Ty::Int | Ty::Float | Ty::Bool) { return None; }
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } => Some(false),
        IrExprKind::Var { id } => (*id != src).then_some(false),
        IrExprKind::UnOp { operand, .. } => scalar_read_of(operand, src),
        IrExprKind::BinOp { left, right, .. } => Some(scalar_read_of(left, src)? | scalar_read_of(right, src)?),
        IrExprKind::If { cond, then, else_ } => {
            Some(scalar_read_of(cond, src)? | scalar_read_of(then, src)? | scalar_read_of(else_, src)?)
        }
        IrExprKind::RuntimeCall { symbol, args } if SCALAR_SOURCE_READS.contains(&symbol.as_str()) => match args.as_slice() {
            [only] if borrowed_var(only) == Some(src) => Some(true),
            _ => None,
        },
        _ => None,
    }
}

/// The source var of `{ let t = <scalar read>; …; op(&src, …) }` — the block
/// OwnedSourceHoist builds (or one written that way): its lets are
/// [`scalar_read_of`] the source, so the op's source is the block's only
/// read of it that can move. TailCallOpt's census counts it as one source read.
pub fn hoisted_source(e: &IrExpr) -> Option<VarId> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &e.kind else { return None };
    let IrExprKind::RuntimeCall { symbol, args } = &tail.kind else { return None };
    if !takes_source_either_way(symbol.as_str()) { return None; }
    let src = borrowed_var(args.first()?)?;
    let scalar_let = |s: &almide_ir::IrStmt| matches!(&s.kind, IrStmtKind::Bind { value, .. } if scalar_read_of(value, src).is_some());
    let rest_free = args[1..].iter().all(|a| scalar_read_of(a, src) == Some(false));
    (!stmts.is_empty() && stmts.iter().all(scalar_let) && rest_free).then_some(src)
}
