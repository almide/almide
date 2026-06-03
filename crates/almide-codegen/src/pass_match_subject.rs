//! MatchSubject Nanopass: insert ownership transforms on match subjects.
//!
//! Target: Rust only.
//!
//! Rust's `match` on `String` requires `.as_str()` to match against `&str` literals.
//! `Option<String>` requires `.as_deref()` to match `Some("literal")` patterns.
//!
//! This pass inserts `Borrow { as_str: true }` or `Call { Method "as_deref" }` IR nodes
//! on match subjects, so the walker never needs to check types.
//!
//! Traversal goes through the canonical `IrMutVisitor`/`walk_expr_mut` (exhaustive,
//! wildcard-free) rather than a hand-rolled `match expr.kind { …; _ => {} }`, so a
//! `Match` nested under any wrapper or future node kind is always reached — no
//! silent subtree drop (see docs/roadmap/active/codegen-traversal-totality.md).

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use almide_lang::types::{Ty, TypeConstructorId};
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

    // Option<String> subject with Some("literal") patterns → wrap with method call .as_deref()
    // Also trigger when inner type is Unknown/TypeVar — if patterns contain string literals,
    // the subject is Option<String> even if not fully resolved at this point.
    if let Ty::Applied(TypeConstructorId::Option, args) = &subject.ty {
        let inner_is_string = args.len() == 1 && matches!(&args[0], Ty::String | Ty::Unknown | Ty::TypeVar(_));
        if inner_is_string {
            let has_some_str_pat = arms.iter().any(|a| {
                if let IrPattern::Some { inner } = &a.pattern {
                    matches!(inner.as_ref(), IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. }))
                } else { false }
            });
            if has_some_str_pat {
                let inner = std::mem::replace(
                    subject.as_mut(),
                    IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
                );
                let deref_ty = Ty::Applied(TypeConstructorId::Option, vec![Ty::String]);
                **subject = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Method {
                            object: Box::new(inner),
                            method: "as_deref".into(),
                        },
                        args: vec![],
                        type_args: vec![],
                    },
                    ty: deref_ty,
                    span: subject.span, def_id: None,
                };
            }
        }
    }
}
