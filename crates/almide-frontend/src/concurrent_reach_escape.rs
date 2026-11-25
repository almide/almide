//! #4903 (ADR-0020 §3.1, rule (b)): a fn with a concurrent slot is called
//! only by name.
//!
//! Slot inference follows a parameter into a concurrent slot and moves the
//! slot to the fn's callers, so a call `w(bump)` holds `bump` to E008. That
//! holds only while the call names `w`. A `let` alias is followed to what it
//! was bound to (`let g = w; g(bump)` is a call of `w`; `let run = (f) =>
//! fan.map(xs, f)` gives the lambda `run` its own slots). Every other use of
//! such a fn — an argument, a list element, a record field, a `var` — makes a
//! call the walk cannot resolve, so its concurrent argument would go
//! unchecked: each such use is refused.
//!
//! "Such a fn" is anything whose call has a concurrent slot: a `fan.<op>`, a
//! stdlib fn declaring `@concurrent`, a user fn (this module's or another's)
//! with an inferred or declared slot, and a lambda with an inferred slot.

use super::{lookup, Analyzer, Callee, FnSummary, Mode, Scope, SlotKind};
use super::fan_surface;
use almide_base::intern::{sym, Sym};
use almide_lang::ast::{Expr, ExprKind, Span};

/// A use of a fn with a concurrent slot other than as a callee.
#[derive(Clone, Debug)]
pub(crate) struct EscapeFinding {
    /// The fn, as written (`w`, `m.w`, `fan.map`), or `this lambda`.
    pub name: String,
    /// The concurrent surface its slot flows into (`fan.map`, `http.serve`).
    pub surface: String,
    pub span: Option<Span>,
}

fn surface_of(k: &SlotKind) -> String {
    match k {
        SlotKind::FanBlock { surface } | SlotKind::FanCallback { surface } | SlotKind::Handler { surface, .. } => {
            surface.clone()
        }
    }
}

impl<'a> Analyzer<'a> {
    /// At a value position in the final discovery round: refuse `e` when it
    /// names a fn with a concurrent slot.
    pub(super) fn check_escape(&mut self, e: &'a Expr, scope: &Scope<'a>) {
        if let Some((name, surface)) = self.slotted(e, scope) {
            self.escape_findings.push(EscapeFinding { name, surface, span: e.span });
        }
    }

    /// `(name, surface)` when `e` names a fn whose call has a concurrent slot.
    fn slotted(&self, e: &Expr, scope: &Scope<'a>) -> Option<(String, String)> {
        let callee = match &e.kind {
            ExprKind::Ident { .. } | ExprKind::Lambda { .. } => self.resolve_callee(e, scope),
            ExprKind::Member { object, field } => self.module_member(object, *field, scope)?,
            _ => return None,
        };
        if let Callee::Fan(op) = callee {
            let surface = fan_surface(op.as_str());
            return Some((surface.clone(), surface));
        }
        let slots = self.call_slots(&callee, &[], scope);
        let (_, kind) = slots.first()?;
        let name = match callee {
            Callee::Local(f) | Callee::Ufcs(f) => f.to_string(),
            Callee::Ext(m, f) | Callee::Stdlib(m, f) | Callee::StdlibMethod(m, f) => format!("{m}.{f}"),
            // A lambda named through a `let` is called by that name.
            Callee::Lambda(_) => match &e.kind {
                ExprKind::Ident { name } => name.to_string(),
                _ => "this lambda".to_string(),
            },
            Callee::Fan(_) | Callee::Method | Callee::Value | Callee::Unknown => return None,
        };
        Some((name, surface_of(kind)))
    }

    /// The fn a module-qualified `object.field` names, if it is one: never a
    /// record field or a method of a value.
    pub(super) fn module_member(&self, object: &Expr, field: Sym, scope: &Scope<'a>) -> Option<Callee> {
        if let ExprKind::Ident { name } = &object.kind {
            if lookup(scope, *name).is_some() || self.fns.contains_key(name) {
                return None;
            }
            if name.as_str() == "fan" {
                return Some(Callee::Fan(field));
            }
            if let Some(m) = self.module_of_alias(*name) {
                return Some(self.module_callee(m, field));
            }
            if almide_lang::stdlib_info::is_stdlib_module(name.as_str()) {
                return Some(Callee::Stdlib(*name, field));
            }
            return None;
        }
        let path = Self::member_path(object)?;
        let m = self.module_of_alias(sym(&path))?;
        Some(self.module_callee(m, field))
    }

    /// The cross-module facts of a top-level `let`: the slots of the fn or
    /// lambda it is bound to, and what its initializer reaches.
    pub(super) fn top_let_summary(&mut self, init: &'a Expr) -> FnSummary {
        let empty: Scope<'a> = Vec::new();
        let slots = match self.resolve_callee(init, &empty) {
            c @ (Callee::Local(_) | Callee::Ufcs(_) | Callee::Lambda(_) | Callee::Ext(..) | Callee::Stdlib(..) | Callee::StdlibMethod(..)) => {
                self.call_slots(&c, &[], &empty)
            }
            Callee::Fan(_) | Callee::Method | Callee::Value | Callee::Unknown => Vec::new(),
        };
        let reach = if self.shape.may_reach {
            let saved = std::mem::take(&mut self.reached);
            let saved_inits = std::mem::take(&mut self.visited_inits);
            let body = self.fresh_body();
            let mut scope: Scope<'a> = Vec::new();
            self.walk(init, &mut scope, Mode::Reach { body, live: false });
            let found = std::mem::replace(&mut self.reached, saved);
            self.visited_inits = saved_inits;
            found.into_iter().next()
        } else {
            None
        };
        FnSummary { slots, reach }
    }
}
