//! ChainSourceBorrowPass: a consumed chain source that is really a borrow.
//!
//! A chain's source is CONSUMED (`.into_iter()`) when a lambda the element
//! reaches needs it owned (`BorrowInsertion`'s element verdict, #2287). Two
//! shapes then turn out to need no owned value at all, because what remains
//! after fusion is an iteration and `.iter().cloned()` serves it from a
//! borrow (#2098):
//!
//! - the `Clone` clone insertion put in front of the source so the consumer
//!   could not take the caller's value (`(v.clone()).into_iter()` — dead by
//!   construction once the consumer is an inlined iteration): stripped, the
//!   source is borrowed. Every projection root qualifies (`t.sizes` off a
//!   `&Table` too), not only a bare variable;
//! - a `&[T]` param used bare as the source: a slice does not `into_iter()`
//!   into owned elements, so it is borrowed as the reference it is.
//!
//! The one way a borrow could still be wrong is a callback (or a fold seed)
//! that MUTATES the same variable while the chain walks it, so that is
//! exactly the condition checked — on the final chain, after any merge. A
//! shared cell (`var` captured and written, `shared_mut_vars`) is exempt:
//! the chain reads it through the cell's snapshot (`.get()`), never a live
//! borrow, so the write cannot reach the walk. Runs after `CloneInsertion`,
//! where the `Clone` comes from.

use std::collections::HashSet;

use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::*;

use super::pass::{NanoPass, PassResult, Target};
use super::use_kind::written_vars;

#[derive(Debug)]
pub struct ChainSourceBorrowPass;

impl NanoPass for ChainSourceBorrowPass {
    fn name(&self) -> &str { "ChainSourceBorrow" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["StreamFusion", "CloneInsertion"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["RustLowering"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        let none = HashSet::new();
        let cells = program.codegen_annotations.shared_mut_vars.clone();
        for f in &mut program.functions {
            let slices = slice_params(&f.params);
            changed |= borrow_sources(&mut f.body, &slices, &cells);
        }
        for tl in &mut program.top_lets { changed |= borrow_sources(&mut tl.value, &none, &cells); }
        for m in &mut program.modules {
            for f in &mut m.functions {
                let slices = slice_params(&f.params);
                changed |= borrow_sources(&mut f.body, &slices, &cells);
            }
            for tl in &mut m.top_lets { changed |= borrow_sources(&mut tl.value, &none, &cells); }
        }
        PassResult { program, changed }
    }
}

fn slice_params(params: &[IrParam]) -> HashSet<VarId> {
    params.iter().filter(|p| p.borrow == ParamBorrow::RefSlice).map(|p| p.var).collect()
}

fn borrow_sources(body: &mut IrExpr, slices: &HashSet<VarId>, cells: &HashSet<VarId>) -> bool {
    struct V<'a> { slices: &'a HashSet<VarId>, cells: &'a HashSet<VarId>, changed: bool }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            self.changed |= borrow_source(expr, self.slices, self.cells);
        }
    }
    let mut v = V { slices, cells, changed: false };
    v.visit_expr_mut(body);
    v.changed
}

/// The variable a place expression reads through (`v`, `t.sizes`, `p.0`).
fn root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => root(object),
        IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. } => root(expr),
        _ => None,
    }
}

fn borrow_source(expr: &mut IrExpr, slices: &HashSet<VarId>, cells: &HashSet<VarId>) -> bool {
    let IrExprKind::IterChain { source, consume, steps, collector } = &mut expr.kind else { return false };
    // A source the borrow pass already left borrowed (#2287) needs only the
    // clone the clone pass put in front of a live local stripped: the
    // iteration borrows, the copy is read by no one.
    let (id, strip_clone) = match &source.kind {
        IrExprKind::Clone { expr: inner } => match root(inner) { Some(id) => (id, true), None => return false },
        IrExprKind::Var { id } if *consume && slices.contains(id) => (*id, false),
        _ => return false,
    };
    let init = match &*collector {
        IterCollector::Fold { init, .. } => Some(&**init),
        _ => None,
    };
    // A borrow is wrong when something evaluated INSIDE the chain expression
    // still reaches the same variable — and there are two ways to reach it,
    // not one. Writing it was checked here from the start; MOVING it was not
    // (#2377), so `list.fold(xs, list.is_empty(list.sort_by(xs, f)), g)`
    // borrowed `xs` for the walk and handed the same `xs` to `sort_by` by
    // value, which is `error[E0505]` under a "this is an Almide bug" banner
    // on a program 0.62.0 compiled. The seed is evaluated while the
    // receiver's borrow is live, so a move in it conflicts exactly as a write
    // does. `moves_var` is the clone pass's own predicate, shared rather than
    // restated. A shared cell stays exempt for the same reason as before: the
    // chain reads it through the cell's snapshot, so neither reach is live.
    let reaches_source = !cells.contains(&id)
        && steps.iter().filter_map(IterStep::lambda).chain(collector.lambda()).chain(init)
            .any(|e| written_vars(e).contains(&id) || super::pass_clone_interp::moves_var(e, id));
    if reaches_source {
        return false;
    }
    if strip_clone {
        let taken = std::mem::replace(&mut source.kind, IrExprKind::Unit);
        let IrExprKind::Clone { expr: inner } = taken else { unreachable!("checked above") };
        *source = inner;
    }
    *consume = false;
    true
}
