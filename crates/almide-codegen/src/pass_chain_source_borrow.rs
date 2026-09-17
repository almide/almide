//! ChainSourceBorrowPass: a consumed chain source that is really a borrow.
//!
//! Fusion builds a chain whose source is CONSUMED when the twin's slot was
//! (`@consume(xs)`): `.into_iter()`. Two shapes then turn out to need no
//! owned value at all, because what remains after fusion is an iteration
//! and `.iter().cloned()` serves it from a borrow (#2098):
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
//! exactly the condition checked — on the final chain, after any merge.
//! Runs after `CloneInsertion`, where the `Clone` comes from.

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
        for f in &mut program.functions {
            let slices = slice_params(&f.params);
            changed |= borrow_sources(&mut f.body, &slices);
        }
        for tl in &mut program.top_lets { changed |= borrow_sources(&mut tl.value, &none); }
        for m in &mut program.modules {
            for f in &mut m.functions {
                let slices = slice_params(&f.params);
                changed |= borrow_sources(&mut f.body, &slices);
            }
            for tl in &mut m.top_lets { changed |= borrow_sources(&mut tl.value, &none); }
        }
        PassResult { program, changed }
    }
}

fn slice_params(params: &[IrParam]) -> HashSet<VarId> {
    params.iter().filter(|p| p.borrow == ParamBorrow::RefSlice).map(|p| p.var).collect()
}

fn borrow_sources(body: &mut IrExpr, slices: &HashSet<VarId>) -> bool {
    struct V<'a> { slices: &'a HashSet<VarId>, changed: bool }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
            self.changed |= borrow_source(expr, self.slices);
        }
    }
    let mut v = V { slices, changed: false };
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

fn borrow_source(expr: &mut IrExpr, slices: &HashSet<VarId>) -> bool {
    let IrExprKind::IterChain { source, consume, steps, collector } = &mut expr.kind else { return false };
    if !*consume {
        return false;
    }
    let (id, strip_clone) = match &source.kind {
        IrExprKind::Clone { expr: inner } => match root(inner) { Some(id) => (id, true), None => return false },
        IrExprKind::Var { id } if slices.contains(id) => (*id, false),
        _ => return false,
    };
    let init = match &*collector {
        IterCollector::Fold { init, .. } => Some(&**init),
        _ => None,
    };
    let writes_source = steps.iter().filter_map(IterStep::lambda).chain(collector.lambda()).chain(init)
        .any(|e| written_vars(e).contains(&id));
    if writes_source {
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
