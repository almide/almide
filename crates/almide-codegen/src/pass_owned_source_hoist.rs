//! OwnedSourceHoist Nanopass: bind a range op's later arguments before its
//! source, where they read the source only as a scalar (#3519).
//!
//! Target: Rust only.
//!
//! `list.slice(xs, 1, list.len(xs))` reads `xs` twice in one call, so neither
//! read is the last one: CloneInsertion (and TailCallOpt for a loop param)
//! keeps the source a `Borrow` and the op clones every element it keeps, even
//! when `xs` is dead after the call. Rust evaluates arguments left to right,
//! so moving `xs` into the first argument and then reading `len(&xs)` would
//! not compile either. This pass binds the later arguments first:
//!
//! ```text
//! almide_rt_list_slice(&xs, 1, almide_rt_list_len(&xs))
//!   → { let t = almide_rt_list_len(&xs); almide_rt_list_slice(&xs, 1, t) }
//! ```
//!
//! after which the source is the block's last read of `xs`, and the existing
//! deciders (`owned_source.rs`) move it into the owned twin wherever `xs` is
//! not read again — and keep the borrow wherever it is (this pass runs before
//! that decision, so a borrowed call keeps its bound temps: the same
//! evaluation, spelled with a `let`).
//!
//! Only when every later argument is [`scalar_read_of`] the source: literals,
//! scalar vars, operators, `if`, and `list.len` / `list.is_empty` of the source.
//! None of them has an effect, and the bound ones keep their relative order
//! (literals and plain vars stay in place — evaluating them is a constant), so
//! the reorder against the source's `&xs` is unobservable. Anything else in a
//! later argument (a call, a lambda, a block, another read of the source)
//! leaves the call exactly as it was.

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use super::owned_source::{borrowed_var, scalar_read_of, takes_source_either_way};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct OwnedSourceHoistPass;

impl NanoPass for OwnedSourceHoistPass {
    fn name(&self) -> &str { "OwnedSourceHoist" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    /// Reads the source `Borrow` BorrowInsertion puts on a range op, and
    /// shapes the calls whose source the two last-use deciders then move.
    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowInsertion"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["TailCallOpt", "CloneInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let IrProgram { functions, top_lets, modules, var_table, .. } = &mut program;
        let mut v = HoistVisitor { var_table, changed: false };
        for func in functions.iter_mut() {
            v.visit_expr_mut(&mut func.body);
        }
        for tl in top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
        for module in modules.iter_mut() {
            for func in module.functions.iter_mut() {
                v.visit_expr_mut(&mut func.body);
            }
            for tl in module.top_lets.iter_mut() {
                v.visit_expr_mut(&mut tl.value);
            }
        }
        let changed = v.changed;
        PassResult { program, changed }
    }
}

struct HoistVisitor<'a> {
    var_table: &'a mut VarTable,
    changed: bool,
}

impl IrMutVisitor for HoistVisitor<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        if hoistable(expr) {
            self.hoist(expr);
            self.changed = true;
        }
    }
}

/// A range op on `&src` whose later arguments are all scalar, and at least one
/// of them reads `src`.
fn hoistable(expr: &IrExpr) -> bool {
    let IrExprKind::RuntimeCall { symbol, args } = &expr.kind else { return false };
    if !takes_source_either_way(symbol.as_str()) { return false; }
    let Some(src) = args.first().and_then(borrowed_var) else { return false };
    let reads: Option<Vec<bool>> = args[1..].iter().map(|a| scalar_read_of(a, src)).collect();
    reads.is_some_and(|r| r.contains(&true))
}

/// Literals and plain vars stay in the call: evaluating them is a constant.
fn stays_inline(arg: &IrExpr) -> bool {
    matches!(arg.kind, IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } | IrExprKind::Var { .. })
}

impl HoistVisitor<'_> {
    /// `op(&src, a, b)` → `{ let t = b; op(&src, a, t) }`, binding every
    /// later argument that is not [`stays_inline`], in order.
    fn hoist(&mut self, expr: &mut IrExpr) {
        let IrExprKind::RuntimeCall { args, .. } = &mut expr.kind else { return };
        let mut stmts = Vec::new();
        for arg in args[1..].iter_mut().filter(|a| !stays_inline(a)) {
            let var = self.var_table.alloc_fresh("__src_arg", arg.ty.clone(), Mutability::Let, None);
            let value = std::mem::replace(arg, IrExpr { kind: IrExprKind::Var { id: var }, ty: arg.ty.clone(), span: arg.span, def_id: None });
            stmts.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: value.ty.clone(), value }, span: None });
        }
        let call = std::mem::take(expr);
        *expr = IrExpr { ty: call.ty.clone(), span: call.span, def_id: None, kind: IrExprKind::Block { stmts, expr: Some(Box::new(call)) } };
    }
}
