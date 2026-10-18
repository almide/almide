//! #3522: `call(..) ?? fallback` on an effect fn declared `-> Option[T]`.
//!
//! The call is `Result[Option[T], String]` (ADR-0002 §D6), so `??` unwraps the
//! `Result` and wants an `Option[T]` fallback. A fallback of type `T` drew the
//! generic E001 ("expected Option[T] but got T"), never naming the missing
//! `!`, and the `??` stayed `Option[T]`, so the binding's next use raised a
//! second E001. The repair is `call(..)! ?? fallback`: `!` propagates the
//! `Err`, then `??` defaults the none. That E001 now names it, and the `??` is
//! typed `T` — the type the repaired program gives it — so the later uses see
//! no cascade.

use std::collections::HashSet;
use crate::ast::{self, ExprKind};
use crate::types::Ty;
use super::Checker;
use super::types::resolve_ty;

/// Start positions of the calls to an effect fn declared `-> Option[T]`.
#[derive(Clone, Default)]
pub(crate) struct EffectOptionCalls(HashSet<(usize, usize)>);

impl Checker {
    /// The named call being checked resolves to an effect fn declared
    /// `-> Option[T]`. Keyed by the start of the innermost expression being
    /// inferred: the call itself, or its callee path, which starts with it.
    pub(crate) fn note_effect_option_call(&mut self) {
        if let Some(s) = self.inferring_span() {
            self.effect_option_calls.0.insert((s.line, s.col));
        }
    }

    /// `inner ?? fallback`, `inner` typed `resolved`, `fallback` typed `ft`:
    /// when `inner` is such a call and the fallback is the call's `T` (not its
    /// `Option[T]`), report the E001 naming the `!` and return `T`. `None`
    /// leaves the `??` to the general rule.
    pub(super) fn report_qq_missing_bang(&mut self, inner: &ast::Expr, fallback: &ast::Expr, resolved: &Ty, ft: &Ty) -> Option<Ty> {
        let mut call = inner;
        while let ExprKind::Paren { expr } = &call.kind {
            call = expr;
        }
        let span = call.span?;
        let ExprKind::Call { callee, .. } = &call.kind else { return None };
        if !self.effect_option_calls.0.contains(&(span.line, span.col)) {
            return None;
        }
        let option = resolved.result_ok_ty()?;
        let payload = option.option_inner()?;
        let ft = resolve_ty(ft, &self.uf);
        if matches!(ft, Ty::Unknown | Ty::Never | Ty::TypeVar(_)) || ft.compatible(&option) || !payload.compatible(&ft) {
            return None;
        }
        let callee = callee.span.and_then(|s| self.source_slice(s)).unwrap_or_else(|| "the callee".to_string());
        // The call's own text, when its span is one line that is the whole call
        // (a multi-line call's span does not slice to it).
        let call_text = self.source_slice(span)
            .filter(|t| t.starts_with(callee.as_str()) && t.ends_with(')') && Self::fix_anchor_ends_expression(t));
        let fallback_text = fallback.span.and_then(|s| self.source_slice(s))
            .filter(|t| Self::fix_anchor_ends_expression(t) || (t.len() > 1 && t.starts_with('"') && t.ends_with('"')))
            .unwrap_or_else(|| "fallback".to_string());
        let call_shown = call_text.clone().unwrap_or_else(|| format!("{callee}(..)"));
        let hint = format!(
            "`{callee}` is an effect fn (its call is `{}`): propagate its error with `!` before `??` — `{call_shown}! ?? {fallback_text}`",
            resolved.display()
        );
        let mut diag = super::err(
            format!("type mismatch in ?? fallback: expected {} but got {}", option.display(), ft.display()),
            hint,
            "?? fallback",
        ).with_code("E001");
        // The `!` is the one spelling of the callee's declared `-> Option[T]`
        // (#2653's rule), where an effect fn body can propagate it.
        if call_text.is_some() && self.env.auto_unwrap && self.env.lambda_depth == 0 {
            diag = diag.with_machine_fix(span.line, span.end_col, span.end_col, "!");
        }
        let saved = self.current_span;
        self.current_span = fallback.span.or(saved);
        self.emit(diag);
        self.current_span = saved;
        Some(payload)
    }
}
