//! MatchSubject Nanopass: insert ownership transforms on match subjects.
//!
//! Target: Rust only.
//!
//! Rust's `match` on `String` requires `&*s` (`.as_str()`) to match a top-level
//! `&str` literal pattern: `match &*s { "good" => .. }`. This pass inserts the
//! `Borrow { as_str: true }` IR node on such subjects so the walker never checks
//! types.
//!
//! String literals nested INSIDE a payload (`some("x")`, `ok("x")`, `Word("x")`)
//! cannot be reconciled by a subject deref — they are hoisted into `==` guards by
//! `PatternLiteralGuardPass`, which runs earlier. This pass therefore only ever
//! handles the top-level-String-subject case.
//!
//! Traversal goes through the canonical `IrMutVisitor`/`walk_expr_mut` (exhaustive,
//! wildcard-free) rather than a hand-rolled `match expr.kind { …; _ => {} }`, so a
//! `Match` nested under any wrapper or future node kind is always reached — no
//! silent subtree drop (see docs/roadmap/active/codegen-traversal-totality.md).

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct MatchSubjectPass;

impl NanoPass for MatchSubjectPass {
    fn name(&self) -> &str { "MatchSubject" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = MatchSubjectVisitor;
        for func in &mut program.functions {
            v.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            v.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                v.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                v.visit_expr_mut(&mut tl.value);
            }
        }
        PassResult { program, changed: true }
    }
}

/// Post-order rewrite: descend into every child first via the exhaustive
/// `walk_expr_mut`, then — once a `Match`'s subject and arms are themselves
/// rewritten — insert its `.as_str()`/`.as_deref()` ownership transform.
struct MatchSubjectVisitor;

impl IrMutVisitor for MatchSubjectVisitor {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        if let IrExprKind::Match { subject, arms } = &mut expr.kind {
            transform_match_subject(subject, arms);
        }
    }
}

/// Insert `.as_str()` or `.as_deref()` on match subjects when needed.
fn transform_match_subject(subject: &mut Box<IrExpr>, arms: &[IrMatchArm]) {
    // String subject with string literal patterns → wrap with Borrow { as_str: true }
    if matches!(&subject.ty, Ty::String) {
        let has_str_pat = arms.iter().any(|a| {
            matches!(&a.pattern, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. }))
        });
        if has_str_pat {
            let inner = std::mem::replace(
                subject.as_mut(),
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            **subject = IrExpr {
                kind: IrExprKind::Borrow { expr: Box::new(inner), as_str: true, mutable: false },
                ty: Ty::String, // type is still String for downstream
                span: subject.span, def_id: None,
            };
        }
    }

    // A `Some("literal")` payload was previously reconciled here by wrapping the
    // subject in `.as_deref()` (Option<String> -> Option<&str>). That path is now
    // owned by `PatternLiteralGuardPass`, which runs FIRST and rewrites EVERY
    // payload-nested string literal — `Some("x")` included — into `Some(__s) if
    // __s == "x"`. So by the time MatchSubject runs, no `Some(Literal)` survives
    // and a single mechanism handles string-literal payloads at any depth (see
    // contract C-036). The top-level `String` subject above keeps its `&*s` deref:
    // there is no payload to bind, and `match &*s { "good" => .. }` is the
    // idiomatic Rust form.
}
