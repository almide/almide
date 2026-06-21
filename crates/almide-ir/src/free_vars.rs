//! Closure capture analysis — the single source of truth (Closure Architecture v2, P1).
//!
//! A closure's *captures* are the `VarId`s it references from an enclosing scope.
//! This used to be computed by three independent implementations (the WASM
//! lifting pass's `FreeVarCollector`, the WASM emitter's `ClosureScanner`, and
//! Rust's implicit `move`), which could subtly diverge — e.g. on binder handling
//! (`Match`/`ForIn`/`Block`) or capture order — and produced two near-duplicate
//! WASM env builders keyed off them. `free_vars` consolidates the codegen-path
//! analysis into one scope-tracking traversal that every consumer calls, so the
//! capture set for a given lambda is computed exactly once and identically.
//!
//! Scope-tracking: a lambda's own params, block bindings (`Bind`/`BindDestructure`),
//! match-arm patterns, and for-in loop vars are *locally bound* and excluded. A
//! `ClosureCreate`'s captures are treated as references (the enclosing vars a
//! nested closure needs). The result is **sorted by `VarId`** so any downstream
//! env layout is host-deterministic (mirrors the existing `captures.sort()` the
//! emitter relied on; see the Determinism Belt).
//!
//! Full design: docs/roadmap/active/closure-architecture-v2.md.

use std::collections::HashSet;
use crate::{IrExpr, IrExprKind, IrStmt, IrStmtKind, IrPattern, VarId};
use crate::visit::{IrVisitor, walk_expr, walk_stmt};

/// The free variables of `expr` relative to `bound` (typically a lambda's params
/// plus any already-in-scope binders), as a deterministically-sorted `Vec`.
///
/// Because `IrExprKind::Var` denotes *only* locals (params, `let`s, loop/match
/// binders) — globals and top-level functions are `Member`/`FnRef`/DefId nodes,
/// never `Var` — the free set is exactly the enclosing-scope locals the
/// expression references, i.e. its captures.
pub fn free_vars(expr: &IrExpr, bound: &HashSet<VarId>) -> Vec<VarId> {
    let mut c = FreeVarCollector { bound: bound.clone(), free: HashSet::new() };
    c.visit_expr(expr);
    let mut v: Vec<VarId> = c.free.into_iter().collect();
    v.sort_by_key(|id| id.0);
    v
}

/// Every `VarId` *bound* anywhere within `expr` — by a `let`/destructure (`Bind`/`BindDestructure`),
/// a `match`-arm pattern, or a `for-in` loop variable. The dual of [`free_vars`]: where `free_vars`
/// asks "which enclosing locals does this reference", `bound_vars` asks "which locals does this
/// introduce". The TCO rewrite uses it to tell a base case that closes over only carried params
/// (safe to recompute in the post-loop dispatch) from one that references a loop-body-local binding
/// (which is dead post-loop, so its base must be computed IN the loop and carried out).
pub fn bound_vars(expr: &IrExpr) -> HashSet<VarId> {
    let mut c = BoundVarCollector { bound: HashSet::new() };
    c.visit_expr(expr);
    c.bound
}

struct BoundVarCollector {
    bound: HashSet<VarId>,
}

impl IrVisitor for BoundVarCollector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Lambda { params, .. } => {
                for (v, _) in params {
                    self.bound.insert(*v);
                }
            }
            IrExprKind::Match { arms, .. } => {
                for arm in arms {
                    collect_pattern_bindings(&arm.pattern, &mut self.bound);
                }
            }
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.bound.insert(*var);
                if let Some(vt) = var_tuple {
                    self.bound.extend(vt.iter().copied());
                }
            }
            _ => {}
        }
        walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, .. } => {
                self.bound.insert(*var);
            }
            IrStmtKind::BindDestructure { pattern, .. } => {
                collect_pattern_bindings(pattern, &mut self.bound);
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

struct FreeVarCollector {
    bound: HashSet<VarId>,
    free: HashSet<VarId>,
}

impl IrVisitor for FreeVarCollector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Var { id } => {
                if !self.bound.contains(id) {
                    self.free.insert(*id);
                }
            }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (vid, _) in captures {
                    if !self.bound.contains(vid) {
                        self.free.insert(*vid);
                    }
                }
            }
            IrExprKind::Lambda { params, body, .. } => {
                let saved = self.bound.clone();
                for (v, _) in params {
                    self.bound.insert(*v);
                }
                self.visit_expr(body);
                self.bound = saved;
            }
            IrExprKind::Block { stmts, expr: tail } => {
                let saved = self.bound.clone();
                for stmt in stmts {
                    self.visit_stmt(stmt);
                    match &stmt.kind {
                        IrStmtKind::Bind { var, .. } => {
                            self.bound.insert(*var);
                        }
                        IrStmtKind::BindDestructure { pattern, .. } => {
                            collect_pattern_bindings(pattern, &mut self.bound);
                        }
                        _ => {}
                    }
                }
                if let Some(e) = tail {
                    self.visit_expr(e);
                }
                self.bound = saved;
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                for arm in arms {
                    let saved = self.bound.clone();
                    collect_pattern_bindings(&arm.pattern, &mut self.bound);
                    if let Some(g) = &arm.guard {
                        self.visit_expr(g);
                    }
                    self.visit_expr(&arm.body);
                    self.bound = saved;
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.visit_expr(iterable);
                let saved = self.bound.clone();
                self.bound.insert(*var);
                if let Some(vt) = var_tuple {
                    for v in vt {
                        self.bound.insert(*v);
                    }
                }
                for s in body {
                    self.visit_stmt(s);
                }
                self.bound = saved;
            }
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        // The *target* of an assignment is a `VarId` field, not a `Var` expr, so the
        // expr walk above never sees it. But a closure that writes a variable does
        // reference it — count the target free so a write-only capture (`xs[0] = 9`,
        // `s = …` with no read of `s`) is still recognized as captured. Without this
        // such a closure captured nothing and the write went nowhere.
        match &stmt.kind {
            IrStmtKind::Assign { var, .. } => {
                if !self.bound.contains(var) { self.free.insert(*var); }
            }
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => {
                if !self.bound.contains(target) { self.free.insert(*target); }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

/// Add every `VarId` a pattern binds to `bound`.
pub fn collect_pattern_bindings(pattern: &IrPattern, bound: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => {
            bound.insert(*var);
        }
        IrPattern::Constructor { args, .. } => {
            for a in args {
                collect_pattern_bindings(a, bound);
            }
        }
        IrPattern::Tuple { elements } => {
            for e in elements {
                collect_pattern_bindings(e, bound);
            }
        }
        IrPattern::Some { inner, .. } | IrPattern::Ok { inner, .. } | IrPattern::Err { inner, .. } => {
            collect_pattern_bindings(inner, bound);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern {
                    collect_pattern_bindings(p, bound);
                }
            }
        }
        _ => {}
    }
}
