//! RangeCountingVarsPass — the native twin of the wasm leg's #1400 analysis
//! (`almide-mir::lower::range_counting_vars`), for the v3 codegen fallback.
//!
//! `let r = 0..<n; for i in r { … }` renders through the `range_expr`
//! template, which materializes the real `Vec<i64>` at the bind — 8 bytes per
//! element, `try_reserve`d up front. An INLINE `for i in 0..<n` head renders
//! the bare `start..end` and counts. The wasm leg stopped materializing the
//! head-only BOUND spelling in #1400; the v1 native trust-spine shares that
//! MIR analysis, so on the default native path the two legs agreed. But v1
//! walls on any function it cannot lower, and the WHOLE program then falls
//! back to this codegen — where every bound range still materialized.
//! #1857 (fuzz-nightly, seed 539646620663 index 1415): a `main` walled on
//! `list.range` (a sibling range that IS indexed and measured) carried a
//! head-only `let empty = -2147483648..<0` — 16 GiB of `Vec<i64>` natively,
//! two locals on wasm; 33.6 s vs 0.7 s, byte-identical output.
//!
//! This pass marks the binders the walker may keep as a bare
//! `std::ops::Range<i64>` (each head iterates `r.clone()` — a Range is two
//! scalars, so a second head, a nested head, or a head inside a closure all
//! read the SAME bounds the bind evaluated once). The admission rule mirrors
//! the MIR scan exactly, so the v3 fallback never counts where the v1 spine
//! materializes, and adds two guards the Rust binding shape wants:
//!   • the initializer is a `Range` with a `LitInt` start,
//!   • the bind is a `let` (a `var` range would need `mut` and a reassign
//!     path this pass does not model),
//!   • it is never rebound, reassigned, or index-assigned,
//!   • it IS read as a single-variable `for-in` head at least once,
//!   • and every read of it is such a head.
//! Runs at pipeline END (after every pass that reshapes a loop head or wraps
//! a `Var`), so the set names exactly the IR the walker renders.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct RangeCountingVarsPass;

impl NanoPass for RangeCountingVarsPass {
    fn name(&self) -> &str { "RangeCountingVars" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    fn depends_on(&self) -> Vec<&'static str> { vec!["CloneInsertion", "IrLinkFlatten"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut vars: HashSet<VarId> = HashSet::new();
        for func in &program.functions {
            vars.extend(range_counting_vars(&func.body));
        }
        for module in &program.modules {
            for func in &module.functions {
                vars.extend(range_counting_vars(&func.body));
            }
        }
        program.codegen_annotations.range_counting_vars = vars;
        PassResult { program, changed: false }
    }
}

/// The `let`-bound range vars in one function body whose every read is a
/// single-variable `for-in` head. See the module doc for the admission rule.
pub(crate) fn range_counting_vars(body: &IrExpr) -> HashSet<VarId> {
    #[derive(Default)]
    struct Scan {
        /// Binds that look eligible so far.
        cand: HashSet<VarId>,
        /// Read somewhere that is NOT a for-in head, or rebound / reassigned.
        disqualified: HashSet<VarId>,
        /// Read AS a for-in head at least once — a candidate with no head has
        /// no loop to carry its bounds, and stays on the materializing path.
        head_read: HashSet<VarId>,
    }
    impl Scan {
        /// Every `Var` read inside `e` disqualifies — the sub-expressions of
        /// a for-in that are NOT the head.
        fn disqualify_reads(&mut self, e: &IrExpr) {
            struct Reads<'a>(&'a mut HashSet<VarId>);
            impl IrVisitor for Reads<'_> {
                fn visit_expr(&mut self, e: &IrExpr) {
                    if let IrExprKind::Var { id } = &e.kind {
                        self.0.insert(*id);
                    }
                    walk_expr(self, e);
                }
            }
            Reads(&mut self.disqualified).visit_expr(e);
        }
    }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            match &stmt.kind {
                IrStmtKind::Bind { var, value, mutability, .. } => {
                    let lit_start_range = matches!(&value.kind, IrExprKind::Range { start, .. }
                        if matches!(start.kind, IrExprKind::LitInt { .. }));
                    if lit_start_range && matches!(mutability, Mutability::Let) {
                        // A SECOND bind of the same VarId is a different
                        // binding wearing one id; take neither.
                        if !self.cand.insert(*var) {
                            self.disqualified.insert(*var);
                        }
                    } else {
                        self.disqualified.insert(*var);
                    }
                    walk_stmt(self, stmt);
                }
                IrStmtKind::Assign { var, .. } => {
                    self.disqualified.insert(*var);
                    walk_stmt(self, stmt);
                }
                IrStmtKind::IndexAssign { target, .. } => {
                    self.disqualified.insert(*target);
                    walk_stmt(self, stmt);
                }
                _ => walk_stmt(self, stmt),
            }
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::ForIn { var_tuple, iterable, body, .. } = &e.kind {
                // The HEAD is the one position that does not disqualify — and
                // only for the single-variable form.
                match (&iterable.kind, var_tuple.is_none()) {
                    (IrExprKind::Var { id }, true) => {
                        self.head_read.insert(*id);
                    }
                    _ => self.disqualify_reads(iterable),
                }
                for s in body {
                    self.visit_stmt(s);
                }
                return;
            }
            if let IrExprKind::Var { id } = &e.kind {
                self.disqualified.insert(*id);
            }
            walk_expr(self, e);
        }
    }
    let mut s = Scan::default();
    s.visit_expr(body);
    s.cand
        .into_iter()
        .filter(|v| !s.disqualified.contains(v) && s.head_read.contains(v))
        .collect()
}
