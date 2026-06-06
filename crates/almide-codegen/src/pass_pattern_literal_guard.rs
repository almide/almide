//! PatternLiteralGuard Nanopass: hoist payload-nested string literals into guards.
//!
//! Target: Rust only.
//!
//! A string literal that is the WHOLE match subject reconciles `&str`-vs-`String`
//! by deref-ing the subject (`match &*s { "good" => .. }`, handled by
//! `MatchSubjectPass`). But a string literal nested INSIDE a constructor payload —
//! `ok("good")`, `err("bad")`, `Word("hi")`, `Pair("a", _)`, `some(some("x"))` —
//! has no subject to deref: rustc sees a `&str` pattern against a `String` field
//! and rejects with E0308. (`some("x")` on `Option<String>` was the one payload
//! case `MatchSubjectPass` special-cased via `.as_deref()`; this pass subsumes it
//! so there is ONE way a string literal matches a String, at any depth.)
//!
//! The fix mirrors what the wasm emitter already does (contract C-036): match the
//! container tag structurally, then compare the inner value by `==`. Here we
//! rewrite each payload-nested `Literal { LitStr }` into a fresh `Bind` and AND an
//! equality condition (`__lit_N == "good"`) onto the arm's guard. A literal arm
//! therefore still REFINES (guard-false falls through to later arms, exactly like
//! the wasm tag+value check), so arm order and reachability are byte-identical
//! native == wasm.
//!
//! Only String literals are hoisted: Int/Bool/Float literals are valid Rust
//! patterns in payload position (`ok(0)`, `some(5)`, `Flag(true)`) and need no
//! reconciliation. A top-level `Literal` arm (the whole pattern) is likewise left
//! for `MatchSubjectPass` — this pass only descends INTO containers.
//!
//! Traversal goes through the canonical `IrMutVisitor`/`walk_expr_mut`, so a
//! `Match` nested under any wrapper node is always reached (no silent subtree
//! drop — see docs/roadmap/active/codegen-traversal-totality.md).

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct PatternLiteralGuardPass;

impl NanoPass for PatternLiteralGuardPass {
    fn name(&self) -> &str { "PatternLiteralGuard" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let IrProgram { functions, top_lets, modules, var_table, .. } = &mut program;
        let mut v = LiteralGuardVisitor { var_table, counter: 0 };
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
        PassResult { program, changed: true }
    }
}

struct LiteralGuardVisitor<'a> {
    var_table: &'a mut VarTable,
    counter: u32,
}

impl IrMutVisitor for LiteralGuardVisitor<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Post-order: rewrite nested matches in subjects/bodies/guards first.
        walk_expr_mut(self, expr);
        if let IrExprKind::Match { arms, .. } = &mut expr.kind {
            for arm in arms.iter_mut() {
                self.rewrite_arm(arm);
            }
        }
    }
}

impl LiteralGuardVisitor<'_> {
    /// Hoist every payload-nested string literal in `arm.pattern` into a fresh
    /// binding + an equality condition, then fold all conditions (and any
    /// pre-existing guard) into the arm's guard.
    fn rewrite_arm(&mut self, arm: &mut IrMatchArm) {
        let mut conds: Vec<IrExpr> = Vec::new();
        // The arm pattern itself (depth 0) is NOT a payload position: a top-level
        // `Literal` string is handled by MatchSubjectPass. Descend into its
        // children only.
        descend_children(&mut arm.pattern, &mut |child| {
            self.hoist_str_literals(child, &mut conds);
        });

        if conds.is_empty() {
            return;
        }

        // Fold conditions left-to-right with `&&`, then AND the original guard
        // FIRST so an existing user guard short-circuits before the synthetic
        // literal checks (preserves evaluation-order intent and matches the wasm
        // tag→value→guard sequencing).
        let mut acc = if let Some(g) = arm.guard.take() {
            let mut it = conds.into_iter();
            let first = it.next().expect("conds is non-empty");
            let combined = it.fold(first, and_expr);
            and_expr(g, combined)
        } else {
            let mut it = conds.into_iter();
            let first = it.next().expect("conds is non-empty");
            it.fold(first, and_expr)
        };
        // Normalize the accumulator type (defensive; and_expr already sets Bool).
        acc.ty = Ty::Bool;
        arm.guard = Some(acc);
    }

    /// If `pat` is a `Literal { LitStr }`, replace it with a fresh `Bind` and push
    /// `Var == literal` onto `conds`. Otherwise recurse into its children. Int /
    /// Bool / Float literals are left intact (valid Rust payload patterns).
    fn hoist_str_literals(&mut self, pat: &mut IrPattern, conds: &mut Vec<IrExpr>) {
        if let IrPattern::Literal { expr } = pat {
            if matches!(&expr.kind, IrExprKind::LitStr { .. }) {
                // A string literal pattern equals a `String` payload, so the
                // bound var and the comparison both have type String.
                let lit = std::mem::replace(expr, IrExpr::default());
                let name = almide_base::intern::sym(&format!("__lit_guard_{}", self.counter));
                self.counter += 1;
                let var = self.var_table.alloc(name, Ty::String, Mutability::Let, None);
                *pat = IrPattern::Bind { var, ty: Ty::String };
                conds.push(eq_expr(
                    IrExpr { kind: IrExprKind::Var { id: var }, ty: Ty::String, span: None, def_id: None },
                    lit,
                ));
                return;
            }
        }
        descend_children(pat, &mut |child| self.hoist_str_literals(child, conds));
    }
}

/// Apply `f` to each immediate sub-pattern of `pat`. Total over `IrPattern` —
/// a new container variant must be added here (mirrors `render_pattern`).
fn descend_children(pat: &mut IrPattern, f: &mut dyn FnMut(&mut IrPattern)) {
    match pat {
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => f(inner),
        IrPattern::Constructor { args, .. } => args.iter_mut().for_each(|a| f(a)),
        IrPattern::Tuple { elements } | IrPattern::List { elements } => {
            elements.iter_mut().for_each(|e| f(e))
        }
        IrPattern::RecordPattern { fields, .. } => {
            for fp in fields.iter_mut() {
                if let Some(p) = fp.pattern.as_mut() {
                    f(p);
                }
            }
        }
        IrPattern::Wildcard
        | IrPattern::Bind { .. }
        | IrPattern::Literal { .. }
        | IrPattern::None => {}
    }
}

fn eq_expr(left: IrExpr, right: IrExpr) -> IrExpr {
    IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::Eq,
            left: Box::new(left),
            right: Box::new(right),
        },
        ty: Ty::Bool,
        span: None,
        def_id: None,
    }
}

fn and_expr(left: IrExpr, right: IrExpr) -> IrExpr {
    IrExpr {
        kind: IrExprKind::BinOp {
            op: BinOp::And,
            left: Box::new(left),
            right: Box::new(right),
        },
        ty: Ty::Bool,
        span: None,
        def_id: None,
    }
}
