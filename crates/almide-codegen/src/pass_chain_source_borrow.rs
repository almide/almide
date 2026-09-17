//! ChainSourceBorrowPass: the `Clone` the clone pass put in front of an
//! enumerate-adapted chain source is DEAD by construction (#2098) — it was
//! placed so the consuming runtime call could not take the caller's value,
//! and fusion removed the consumer. `borrow_adapted_source` (kept beside the
//! fusion pass, which decides the chain's shape) turns that source back into
//! a borrow, `.iter().cloned()`, unless a callback writes the variable while
//! the chain walks it. Runs after `CloneInsertion`, which is where the
//! `Clone` comes from; the fusion pass itself now runs before the borrow
//! pass and never sees one.

use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::Ty;

use super::pass::{NanoPass, PassResult, Target};
use super::pass_stream_fusion::borrow_adapted_source;

#[derive(Debug)]
pub struct ChainSourceBorrowPass;

impl NanoPass for ChainSourceBorrowPass {
    fn name(&self) -> &str { "ChainSourceBorrow" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["StreamFusion", "CloneInsertion"] }
    fn run_before(&self) -> Vec<&'static str> { vec!["RustLowering"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        struct V { changed: bool }
        impl IrMutVisitor for V {
            fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
                walk_expr_mut(self, expr);
                if let IrExprKind::IterChain { consume: true, .. } = &expr.kind {
                    let taken = std::mem::replace(expr, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
                    *expr = borrow_adapted_source(taken);
                    if let IrExprKind::IterChain { consume: false, .. } = &expr.kind { self.changed = true; }
                }
            }
        }
        let mut v = V { changed: false };
        for f in &mut program.functions { v.visit_expr_mut(&mut f.body); }
        for tl in &mut program.top_lets { v.visit_expr_mut(&mut tl.value); }
        for m in &mut program.modules {
            for f in &mut m.functions { v.visit_expr_mut(&mut f.body); }
            for tl in &mut m.top_lets { v.visit_expr_mut(&mut tl.value); }
        }
        let changed = v.changed;
        PassResult { program, changed }
    }
}
