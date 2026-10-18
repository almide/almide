//! #3515: an E006 on a FALLIBLE effect callee, repaired in one round.
//!
//! `fn body() -> String = fs.read_text("x")` drew E006 ("mark the caller
//! `effect fn`") and, on the same expression, E001 (`expected String but got
//! Result[String, String]`). Following the E006 hint alone led to E041 (the
//! missing `!`), a second round; the E001's "fix the expression type" pointed
//! away from both steps.
//!
//! The E006 is emitted while its call is still being inferred, before the
//! call's type and its consumer are known. So `report_effect_isolation`
//! leaves a [`Pending`] keyed by the expression being inferred, and the call's
//! own `infer_expr` frame settles it on the way out ([`Checker::leave_isolation_frame`]):
//! when the call's type is a `Result` and nothing around it already consumes
//! the `Result` (`!`, `??`, `?`, a `match` subject), the generic hint gains the
//! `!` step and the call becomes a [`Site`]. A failed constraint whose actual
//! side IS that call's `Result` against its `ok` type, reported from inside the
//! call's span, is the E006's residue — the same record-at-the-source,
//! consult-at-the-cascade shape as `Checker::errored_slots` (#3505).

use crate::ast::{self, ExprKind};
use crate::types::Ty;
use super::Checker;
use super::types::resolve_ty;

/// The hint of the generic E006 branch (`calls.rs`): the only text the `!`
/// step is added to. The lambda / metered-region / argv-reader branches say
/// something else and are left as they are.
pub(super) const GENERIC_HINT: &str = "Mark the calling function as `effect fn`";

/// An E006 emitted for the call whose `infer_expr` frame is `at`.
#[derive(Clone)]
struct Pending {
    diag: usize,
    at: ast::Span,
    callee: String,
}

/// A call whose E006 hint names the `!` (the generic hint, augmented, or the
/// argv-reader hint, which already did), with the `Result` it produces.
#[derive(Clone)]
struct Site {
    diag: usize,
    span: ast::Span,
    ty: Ty,
    file: Option<String>,
}

/// The per-`infer_expr` state the two halves share.
#[derive(Clone, Default)]
pub(crate) struct IsolationCascade {
    /// The expression whose `infer_expr` frame is innermost right now.
    inferring: Option<ast::Span>,
    /// The operand of the innermost `!` / `??` / `?` / `match` subject: a
    /// call here already consumes its `Result`.
    consumed: Option<ast::Span>,
    pending: Option<Pending>,
    sites: Vec<Site>,
}

/// What [`Checker::enter_isolation_frame`] displaced, restored on leave.
pub(crate) struct IsolationFrame {
    inferring: Option<ast::Span>,
    consumed: Option<ast::Span>,
}

/// The operand whose `Result` `kind` consumes, through any parentheses.
fn consumed_operand(kind: &ExprKind) -> Option<&ast::Expr> {
    let mut operand = match kind {
        ExprKind::Unwrap { expr } | ExprKind::Try { expr } | ExprKind::ToOption { expr } => expr,
        ExprKind::UnwrapOr { expr, .. } => expr,
        ExprKind::Match { subject, .. } => subject,
        _ => return None,
    };
    while let ExprKind::Paren { expr } = &operand.kind {
        operand = expr;
    }
    Some(operand)
}

/// `line:col` of `inner` falls inside `outer`'s one-line extent.
fn within(inner: ast::Span, outer: ast::Span) -> bool {
    inner.line == outer.line && inner.col >= outer.col && inner.col < outer.end_col.max(outer.col + 1)
}

impl Checker {
    /// `infer_expr` entry: `expr` becomes the innermost frame.
    pub(crate) fn enter_isolation_frame(&mut self, expr: &ast::Expr) -> IsolationFrame {
        let state = &mut self.isolation_cascade;
        let consumed = match consumed_operand(&expr.kind) {
            Some(operand) => std::mem::replace(&mut state.consumed, operand.span),
            None => state.consumed,
        };
        IsolationFrame { inferring: std::mem::replace(&mut state.inferring, expr.span), consumed }
    }

    /// The span of the innermost expression being inferred.
    pub(crate) fn inferring_span(&self) -> Option<ast::Span> {
        self.isolation_cascade.inferring
    }

    /// `report_effect_isolation` just pushed diagnostic `diag` for a call of
    /// `callee` outside any lambda or metered region.
    pub(crate) fn defer_isolation_hint(&mut self, diag: usize, callee: &str) {
        let state = &mut self.isolation_cascade;
        state.pending = state.inferring.map(|at| Pending { diag, at, callee: callee.to_string() });
    }

    /// `infer_expr` exit: settle the E006 this call raised, if any.
    pub(crate) fn leave_isolation_frame(&mut self, expr: &ast::Expr, ity: &Ty, frame: IsolationFrame) {
        let state = &mut self.isolation_cascade;
        state.inferring = frame.inferring;
        let consumed = std::mem::replace(&mut state.consumed, frame.consumed);
        let ExprKind::Call { args, .. } = &expr.kind else { return };
        let Some(span) = expr.span else { return };
        if state.pending.as_ref().is_none_or(|p| p.at != span) {
            return;
        }
        let Some(p) = state.pending.take() else { return };
        let ty = resolve_ty(ity, &self.uf);
        if !ty.is_result() || consumed == Some(span) {
            return;
        }
        let Some(diag) = self.diagnostics.get_mut(p.diag) else { return };
        if diag.hint == GENERIC_HINT {
            let parens = if args.is_empty() { "()" } else { "(..)" };
            diag.hint = format!(
                "{GENERIC_HINT}, and propagate the call's error with `!`: `{}{parens}!` \
                 (the call's type is `{}`)",
                p.callee, ty.display()
            );
        }
        let file = self.source_file.clone();
        self.isolation_cascade.sites.push(Site { diag: p.diag, span, ty, file });
    }

    /// The failed constraint at `at` is the residue of an E006 whose hint
    /// already names the `!`: its actual side is that call's `Result`, its
    /// expected side the `Result`'s `ok` type, and it was reported from inside
    /// the call. Anything else — another type, another place, an E006 a
    /// rollback removed — is reported as before.
    pub(super) fn is_isolation_cascade(&self, at: Option<ast::Span>, exp: &Ty, act: &Ty) -> bool {
        let Some(at) = at else { return false };
        if act.result_ok_ty().as_ref() != Some(exp) {
            return false;
        }
        self.isolation_cascade.sites.iter().any(|site| {
            within(at, site.span)
                && site.file == self.source_file
                && self.with_slot_defaults(&resolve_ty(&site.ty, &self.uf)) == *act
                && self.diagnostics.get(site.diag).is_some_and(|d| d.code == Some("E006"))
        })
    }
}
