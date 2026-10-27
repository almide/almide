//! BoxTakeLoweringPass: the reads of a recursive type's boxed field take
//! their final spelling (#4251, #4252).
//!
//! Target: Rust only. Runs last, on the final IR.
//!
//! A field that closes a cycle of a recursive type is an `AlmideBox`: a `Box`
//! whose drop, clone and `==` never take one native frame per level. Unlike a
//! `Box`, it cannot move its value out through `*b` (it has a drop), so every
//! read decides here whether it needs the PLACE behind the box or MOVES the
//! value out:
//!
//! - a pattern binder of a boxed position (`BoxDeref`'s `Deref(x)`): a place
//!   read stays `(*x)`; a move becomes `almide_box_take(x)`;
//! - a recursive record's boxed field (`r.f`, typed by the record): a place
//!   read is wrapped in `Deref` and renders `(*r.f)`; one left bare renders
//!   `almide_box_take(r.f)` — the move.
//!
//! A position reads a place when its parent only looks at the value: a
//! clone, a borrow, a field / index / method receiver, a comparison, an
//! interpolated part, a macro argument (`println!` and kin take references).
//! Every other position consumes its operand, as it does for a `Box`.

use std::collections::HashSet;
use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct BoxTakeLoweringPass;

impl NanoPass for BoxTakeLoweringPass {
    fn name(&self) -> &str { "BoxTakeLowering" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Reads the final spellings: every Clone / Borrow / Deref the ownership
    /// passes place is already in the IR.
    fn depends_on(&self) -> Vec<&'static str> {
        vec!["BoxDeref", "BorrowLowering", "SliceBinders", "StrMapKey", "ThreadShareMarks"]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let cx = Cx {
            binders: program.codegen_annotations.box_binders.iter().copied().collect(),
            fields: program.codegen_annotations.boxed_record_fields.clone(),
        };
        if cx.binders.is_empty() && cx.fields.is_empty() {
            return PassResult { program, changed: false };
        }
        for func in &mut program.functions {
            func.body = cx.lower(std::mem::take(&mut func.body), false);
        }
        for tl in &mut program.top_lets {
            tl.value = cx.lower(std::mem::take(&mut tl.value), false);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = cx.lower(std::mem::take(&mut func.body), false);
            }
            for tl in &mut module.top_lets {
                tl.value = cx.lower(std::mem::take(&mut tl.value), false);
            }
        }
        PassResult { program, changed: true }
    }
}

struct Cx {
    binders: HashSet<VarId>,
    fields: HashSet<(String, String)>,
}

impl Cx {
    fn boxed_member(&self, e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Member { object, field } => match &object.ty {
                almide_lang::types::Ty::Named(n, _) => self.fields.contains(&(n.to_string(), field.to_string())),
                _ => false,
            },
            _ => false,
        }
    }

    fn box_binder_read(&self, e: &IrExpr) -> Option<VarId> {
        match &e.kind {
            IrExprKind::Deref { expr } => match &expr.kind {
                IrExprKind::Var { id } if self.binders.contains(id) => Some(*id),
                _ => None,
            },
            _ => None,
        }
    }

    /// Rewrite `e`, read as a place (`place`) or consumed.
    fn lower(&self, e: IrExpr, place: bool) -> IrExpr {
        let IrExpr { kind, ty, span, def_id } = e;
        let kind = match kind {
            IrExprKind::Deref { expr } if matches!(&expr.kind, IrExprKind::Var { id } if self.binders.contains(id)) => {
                if place {
                    IrExprKind::Deref { expr }
                } else {
                    IrExprKind::InlineRust {
                        template: "almide_box_take({b})".to_string(),
                        args: vec![(almide_base::intern::sym("b"), *expr)],
                    }
                }
            }
            // `*l.clone()` consumed: a copy of the box's value, read through
            // the place (`(*l).clone()`) — the box itself cannot be moved out of.
            IrExprKind::Deref { expr } if !place && matches!(&expr.kind, IrExprKind::Clone { expr: inner }
                if matches!(&inner.kind, IrExprKind::Var { id } if self.binders.contains(id))) =>
            {
                let IrExprKind::Clone { expr: var } = expr.kind else { unreachable!() };
                let deref = IrExpr { kind: IrExprKind::Deref { expr: var }, ty: ty.clone(), span, def_id: None };
                IrExprKind::Clone { expr: Box::new(deref) }
            }
            IrExprKind::Member { object, field } => {
                // A non-`Copy` field MOVED out of a box binder's record (the
                // clone pass's last read of the binder): the binder's value is
                // taken whole first — a box cannot be moved out of in part.
                let take_object = !place
                    && self.box_binder_read(&object).is_some()
                    && !almide_ir::top_let_storage::clone_free(&ty);
                let object = Box::new(self.lower(*object, !take_object));
                let member = IrExpr { kind: IrExprKind::Member { object, field }, ty: ty.clone(), span, def_id };
                if place && self.boxed_member(&member) {
                    IrExprKind::Deref { expr: Box::new(member) }
                } else {
                    member.kind
                }
            }
            IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(self.lower(*expr, true)) },
            IrExprKind::Borrow { expr, as_str, mutable } => {
                IrExprKind::Borrow { expr: Box::new(self.lower(*expr, true)), as_str, mutable }
            }
            IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(self.lower(*expr, true)) },
            IrExprKind::TupleIndex { object, index } => {
                IrExprKind::TupleIndex { object: Box::new(self.lower(*object, true)), index }
            }
            IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
                object: Box::new(self.lower(*object, true)),
                index: Box::new(self.lower(*index, false)),
            },
            IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
                object: Box::new(self.lower(*object, true)),
                key: Box::new(self.lower(*key, true)),
            },
            IrExprKind::Call { target: CallTarget::Method { object, method }, args, type_args } => IrExprKind::Call {
                target: CallTarget::Method { object: Box::new(self.lower(*object, true)), method },
                args: args.into_iter().map(|a| self.lower(a, false)).collect(),
                type_args,
            },
            IrExprKind::BinOp { op, left, right } if is_comparison(&op) => IrExprKind::BinOp {
                op,
                left: Box::new(self.lower(*left, true)),
                right: Box::new(self.lower(*right, true)),
            },
            IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
                parts: parts.into_iter().map(|p| match p {
                    IrStringPart::Expr { expr } => IrStringPart::Expr { expr: self.lower(expr, true) },
                    lit => lit,
                }).collect(),
            },
            IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
                name,
                args: args.into_iter().map(|a| self.lower(a, true)).collect(),
            },
            // A chain that does not consume its source iterates a borrow of it.
            IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
                source: Box::new(self.lower(*source, !consume)),
                consume,
                steps: steps.into_iter().map(|s| s.map_exprs(&mut |c| self.lower(c, false))).collect(),
                collector: collector.map_exprs(&mut |c| self.lower(c, false)),
            },
            IrExprKind::Match { subject, arms } => {
                // A match MOVES its subject when an arm binds out of it; a
                // binder the arms still read keeps the place (a by-value
                // match of a box binder the arms reuse never moves on any leg).
                let keep_place = self.subject_reread(&subject, &arms) || !arms.iter().any(|a| binds_any(&a.pattern));
                IrExprKind::Match {
                    subject: Box::new(self.lower(*subject, keep_place)),
                    arms: arms.into_iter().map(|a| IrMatchArm {
                        pattern: a.pattern,
                        guard: a.guard.map(|g| self.lower(g, false)),
                        body: self.lower(a.body, false),
                    }).collect(),
                }
            }
            other => {
                return IrExpr { kind: other, ty, span, def_id }.map_children(&mut |c| self.lower(c, false));
            }
        };
        IrExpr { kind, ty, span, def_id }
    }

    /// Does an arm read the variable the subject takes its box from?
    fn subject_reread(&self, subject: &IrExpr, arms: &[IrMatchArm]) -> bool {
        let root = match self.box_binder_read(subject) {
            Some(id) => Some(id),
            None if self.boxed_member(subject) => root_var(subject),
            None => return false,
        };
        let Some(root) = root else { return true };
        arms.iter().any(|a| mentions(&a.body, root) || a.guard.as_ref().is_some_and(|g| mentions(g, root)))
    }
}

fn is_comparison(op: &BinOp) -> bool {
    matches!(op, BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte)
}

/// The variable a member path starts from (`r` of `r.a.b`).
fn root_var(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } | IrExprKind::Deref { expr: object }
        | IrExprKind::Clone { expr: object } | IrExprKind::Borrow { expr: object, .. } => root_var(object),
        _ => None,
    }
}

fn binds_any(p: &IrPattern) -> bool {
    match p {
        IrPattern::Bind { .. } | IrPattern::As { .. } => true,
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => false,
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => binds_any(inner),
        IrPattern::Constructor { args, .. } => args.iter().any(binds_any),
        IrPattern::Tuple { elements } => elements.iter().any(binds_any),
        IrPattern::List { elements, rest } => elements.iter().any(binds_any) || rest.as_ref().is_some_and(|r| binds_any(r)),
        IrPattern::RecordPattern { fields, .. } => fields.iter().any(|f| f.pattern.as_ref().is_none_or(binds_any)),
    }
}

fn mentions(e: &IrExpr, id: VarId) -> bool {
    struct V { id: VarId, hit: bool }
    impl almide_ir::visit::IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.hit { return; }
            if matches!(&e.kind, IrExprKind::Var { id } if *id == self.id) {
                self.hit = true;
                return;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut v = V { id, hit: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut v, e);
    v.hit
}
