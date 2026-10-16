//! An expected effect-closure type reaches every lambda it shapes.
//!
//! A lambda checked against an `effect (A) -> B` type is an effect-fn body
//! (#1055): effect calls are allowed and the lambda types as the carrier
//! `(A) -> Result[B, String]`. The expectation used to reach a lambda only
//! when the lambda WAS the expression in the typed position — a call slot, a
//! `let` annotation, a fn's declared return. A lambda one step inside data —
//! `ok(some((req) => …))`, `[(next) => (req) => …]`, a record or map field, a
//! tuple element, the fallback of `??`, the value a callback returns — typed
//! pure while the slot it filled was the carrier, so check passed and the
//! native build died in rustc (E0308) while the wasm leg walled (#3749).
//!
//! [`Checker::expect_expr`] walks the expected type down the expression's
//! structure and records, per lambda, the fn type it is expected to have; the
//! lambda's inference reads the record (`take_lambda_expect`). Only an
//! expectation that holds an effect fn somewhere is walked, so a program with
//! no effect closures infers exactly as before.
use super::Checker;
use super::types::resolve_ty;
use crate::ast::{self, ExprKind};
use crate::types::{Ty, TypeConstructorId};

/// How deep the effect-fn search follows type aliases and records.
const MAX_DEPTH: usize = 8;

impl Checker {
    /// `t` with inference vars and a top-level alias resolved.
    fn expect_view(&self, t: &Ty) -> Ty {
        self.env.resolve_named(&resolve_ty(t, &self.uf))
    }

    /// Does `t` hold an `effect` fn type anywhere (through aliases)?
    pub(crate) fn holds_effect_fn(&self, t: &Ty) -> bool {
        self.holds_effect_fn_at(t, 0)
    }

    fn holds_effect_fn_at(&self, t: &Ty, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        match self.expect_view(t) {
            Ty::Fn { is_effect: true, .. } => true,
            other => other.children().iter().any(|c| self.holds_effect_fn_at(c, depth + 1)),
        }
    }

    /// Hand `expected` down `expr`'s structure to the lambdas it shapes. A
    /// lambda written directly in the position still gets the flags its
    /// caller arms; this reaches the ones inside data and branches.
    pub(crate) fn expect_expr(&mut self, expr: &ast::Expr, expected: &Ty) {
        if !self.holds_effect_fn(expected) {
            return;
        }
        let t = self.expect_view(expected);
        match &expr.kind {
            ExprKind::Lambda { .. } if matches!(t, Ty::Fn { .. }) => {
                self.lambda_expect.insert(expr.id, t);
            }
            ExprKind::Paren { expr: e } | ExprKind::Block { expr: Some(e), .. } => self.expect_expr(e, &t),
            ExprKind::If { then, else_, .. } => {
                self.expect_expr(then, &t);
                self.expect_expr(else_, &t);
            }
            ExprKind::Match { arms, .. } => arms.iter().for_each(|a| self.expect_expr(&a.body, &t)),
            _ => self.expect_data(expr, &t),
        }
    }

    /// The data forms of [`Self::expect_expr`]: a constructor, a collection
    /// literal, a tuple or a record, each against its matching type.
    fn expect_data(&mut self, expr: &ast::Expr, t: &Ty) {
        use TypeConstructorId::{List, Map, Option as Opt, Result as Res};
        match (&expr.kind, t) {
            (ExprKind::Some { expr: e }, Ty::Applied(Opt, a))
            | (ExprKind::Ok { expr: e }, Ty::Applied(Res, a)) => {
                if let Some(inner) = a.first() {
                    self.expect_expr(e, inner);
                }
            }
            (ExprKind::Err { expr: e }, Ty::Applied(Res, a)) => {
                if let Some(inner) = a.get(1) {
                    self.expect_expr(e, inner);
                }
            }
            (ExprKind::List { elements }, Ty::Applied(List, a)) if a.len() == 1 => {
                elements.iter().for_each(|e| self.expect_expr(e, &a[0]));
            }
            (ExprKind::MapLiteral { entries }, Ty::Applied(Map, a)) if a.len() == 2 => {
                for (k, v) in entries {
                    self.expect_expr(k, &a[0]);
                    self.expect_expr(v, &a[1]);
                }
            }
            (ExprKind::Tuple { elements }, Ty::Tuple(ts)) => {
                elements.iter().zip(ts).for_each(|(e, et)| self.expect_expr(e, et));
            }
            (ExprKind::Record { fields, .. }, Ty::Record { fields: decl } | Ty::OpenRecord { fields: decl }) => {
                self.expect_fields(fields, decl);
            }
            _ => {}
        }
    }

    fn expect_fields(&mut self, fields: &[ast::FieldInit], decl: &[(almide_base::intern::Sym, Ty)]) {
        for f in fields {
            if let Some((_, ft)) = decl.iter().find(|(n, _)| *n == f.name) {
                self.expect_expr(&f.value, ft);
            }
        }
    }

    /// A named record literal (`Box { h: (req) => … }`) is shaped by its own
    /// declaration.
    pub(crate) fn expect_named_record_fields(&mut self, name: almide_base::intern::Sym, fields: &[ast::FieldInit]) {
        if let Ty::Record { fields: decl } = self.env.resolve_named(&Ty::Named(name, vec![])) {
            self.expect_fields(fields, &decl);
        }
    }

    /// The fn type recorded for lambda `id` by [`Self::expect_expr`].
    pub(crate) fn take_lambda_expect(&mut self, id: ast::ExprId) -> Option<Ty> {
        self.lambda_expect.remove(&id)
    }
}

impl Checker {
    /// Fold the expectation recorded for lambda `id` into the flags its
    /// caller armed: an effect expectation makes it an effect body, the
    /// expected return replaces the caller's, and the expected params fill a
    /// hint the caller left unset. A param
    /// type still naming a generic is not a hint (it would detach the param
    /// from the union-find, #653). Returns the effect-slot verdict.
    pub(crate) fn merge_lambda_expect(
        &mut self,
        id: ast::ExprId,
        slot_effect: bool,
        ret_expect: &mut Option<Ty>,
        param_hint: &mut Option<Vec<Option<Ty>>>,
    ) -> bool {
        let Some(Ty::Fn { params, ret, is_effect }) = self.take_lambda_expect(id) else {
            return slot_effect;
        };
        // The recorded return is the slot's with the call's bindings applied
        // (`() -> A` read as `() -> HttpHandler`), so it wins over the raw one.
        *ret_expect = Some(*ret);
        if param_hint.is_none() && !params.iter().any(Ty::contains_typevar) {
            *param_hint = Some(params.into_iter().map(Some).collect());
        }
        slot_effect || is_effect
    }
}
