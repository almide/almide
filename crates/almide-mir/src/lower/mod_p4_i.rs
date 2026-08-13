// Continuation of `lower/mod_p4.rs` — the #1400 `let`-bound-range analysis.
// Split out to keep mod_p4.rs under the 800-line codopsy max-lines threshold;
// pure text move, same module scope via `include!` (no privacy boundary —
// the include!-continuation pattern the rest of this crate already uses).

/// #1400: the `let`-bound RANGE vars whose every use is a `for-in` HEAD, so the
/// bind can stay a pair of scalars instead of a materialized `list.range` block.
///
/// `let r = 0..<n; for i in r { … }` materializes the real list on the wasm leg
/// (`lower_bind_heap_range`, #1272's fix for a deferred-empty bind that iterated
/// ZERO times). Native never materializes — it iterates lazily — so a range longer
/// than memory prints the right answer natively and dies with `Error: out of memory`
/// on wasm. C-238 promises those legs agree.
///
/// The counting loop already exists and is already correct: an INLINE
/// `for i in 0..<4294967295` runs `try_lower_scalar_for_range` on both legs and
/// prints `9223372030412324865`. Only the BOUND spelling took the heap path. This
/// scan finds the vars that can take the counting loop instead.
///
/// Deliberately conservative — a var is admitted only when EVERY one of these holds,
/// so the deferral can never resurrect #1272:
///   • its bind initializer is a `Range` with a `LitInt` start
///     (`try_lower_scalar_for_range`'s own precondition),
///   • it is never rebound or reassigned,
///   • it IS read as a `for-in` head at least once (otherwise there is no loop to
///     carry the bounds, and deferring would merely delete the bind),
///   • and every READ of it is the `iterable` of a `for-in` with no tuple pattern.
/// A single read anywhere else (`r[i]`, `list.len(r)`, an argument, a tail) keeps the
/// var on today's materializing path, unchanged.
pub(crate) fn range_counting_vars(body: &IrExpr) -> HashMap<VarId, IrExpr> {
    use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
    #[derive(Default)]
    struct Scan {
        /// var → its Range initializer, for binds that look eligible so far.
        cand: HashMap<VarId, IrExpr>,
        /// Vars read somewhere that is NOT a for-in head (or rebound/reassigned).
        disqualified: std::collections::HashSet<VarId>,
        /// Vars read AS a for-in head at least once. A candidate with no head read
        /// has no loop to carry its bounds — deferring it would just delete the
        /// bind, so it stays on the materializing path (this is also what keeps
        /// the `#1272` regression pin, a bind with no reader at all, honest).
        head_read: std::collections::HashSet<VarId>,
    }
    impl Scan {
        /// Every `Var` read inside `e` disqualifies — used for the sub-expressions
        /// of a for-in that are NOT the head.
        fn disqualify_reads(&mut self, e: &IrExpr) {
            struct Reads<'a>(&'a mut std::collections::HashSet<VarId>);
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
                IrStmtKind::Bind { var, value, .. } => {
                    if matches!(&value.kind, IrExprKind::Range { start, .. }
                        if matches!(start.kind, IrExprKind::LitInt { .. }))
                    {
                        // A SECOND bind of the same VarId is a different binding
                        // wearing one id; take neither.
                        if self.cand.insert(*var, value.clone()).is_some() {
                            self.disqualified.insert(*var);
                        }
                    }
                    walk_stmt(self, stmt);
                }
                IrStmtKind::Assign { var, .. } => {
                    self.disqualified.insert(*var);
                    walk_stmt(self, stmt);
                }
                _ => walk_stmt(self, stmt),
            }
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::ForIn { var_tuple, iterable, body, .. } = &e.kind {
                // The HEAD is the one position that does not disqualify — and only
                // for the single-variable form the counting loop covers.
                let head_ok = var_tuple.is_none()
                    && matches!(&iterable.kind, IrExprKind::Var { .. });
                match (&iterable.kind, head_ok) {
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
    s.cand.retain(|v, _| !s.disqualified.contains(v) && s.head_read.contains(v));
    s.cand
}
