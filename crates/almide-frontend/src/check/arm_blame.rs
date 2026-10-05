//! Which peer a match-arm / if-branch mismatch is reported AT (#2927).
//!
//! A join takes its type from one peer — the first arm, the `then` branch —
//! and reports every other peer that disagrees with it. When that first peer
//! is the one that is wrong, every CORRECT later peer is reported instead,
//! and the arm the writer has to edit is never named:
//!
//! ```text
//! effect fn apply(c: Cmd) -> Result[Level?, String] = match c {
//!   A => ok(level()),          // the mistake: level() without `!`
//!   B(_) => ok(none),          // was reported here, twice
//! }
//! ```
//!
//! When the construct sits in a tail position whose type is known (the fn's
//! declared return, carried through blocks, arms and branches as a
//! [`TailExpect`]), the peer the join is anchored on is the first one that
//! AGREES with that type, so the disagreeing peer is the one reported. With
//! no expectation (or none agreeing) the first-peer rule stays, and the
//! message says which peer fixed the type and where.

use almide_lang::ast::{self, ExprKind};
use crate::types::{Ty, TypeConstructorId};
use super::Checker;
use super::types::{resolve_ty, Constraint, FixHint, TailExpect, UnionFind};

impl TailExpect {
    /// Whether a peer of type `t` can be the value of this tail — as the
    /// declared type, or through the effect-body leniency.
    fn accepts(&self, t: &Ty, uf: &UnionFind) -> bool {
        let want = resolve_ty(&self.ty, uf);
        let got = resolve_ty(t, uf);
        if want.compatible(&got) {
            return true;
        }
        if !self.effect_body {
            return false;
        }
        match (&want, &got) {
            // `-> Result[T, E]`: a bare `T` tail is lifted into `ok`.
            (Ty::Applied(TypeConstructorId::Result, w), g) if w.len() == 2 => {
                !g.is_result() && w[0].compatible(g)
            }
            // `-> T`: a `Result[T, _]` tail is stripped (and reported as
            // implicit propagation elsewhere).
            (w, Ty::Applied(TypeConstructorId::Result, g)) if g.len() == 2 => w.compatible(&g[0]),
            _ => false,
        }
    }
}

/// The span a peer's value is reported at: a block's tail, not its `{`.
pub(super) fn value_leaf_span(e: &ast::Expr) -> Option<ast::Span> {
    match &e.kind {
        ExprKind::Block { expr: Some(tail), .. } => value_leaf_span(tail).or(e.span),
        _ => e.span,
    }
}

/// Where a whole body's mismatch with its declared return is reported: the
/// value that fixed the body's type — a block's tail, a match's first
/// value-producing arm, an if's `then` branch — descending to the leaf.
/// `None` for a block with no tail (the body ends in a statement): the
/// caller keeps the last statement's position, which is the one to edit.
pub(super) fn tail_report_span(e: &ast::Expr) -> Option<ast::Span> {
    match &e.kind {
        ExprKind::Block { expr: Some(tail), .. } => tail_report_span(tail),
        ExprKind::Block { expr: None, .. } => None,
        ExprKind::Match { arms, .. } => arms
            .iter()
            .map(|a| &a.body)
            .find(|b| !matches!(b.kind, ExprKind::Err { .. }))
            .map_or(e.span, |b| tail_report_span(b).or(b.span)),
        ExprKind::If { then, .. } => tail_report_span(then).or(then.span),
        _ => e.span,
    }
}

impl Checker {
    /// Pick the peer the join is anchored on: the first one the tail
    /// expectation accepts when the first peer is one it rejects, else the
    /// first. Returns the index and, when the choice was made by the
    /// expectation, the declared type to name in the report.
    pub(super) fn pick_join_anchor(&self, expect: Option<&TailExpect>, types: &[Ty]) -> (usize, Option<Ty>) {
        let Some(expect) = expect else { return (0, None) };
        let Some(first) = types.first() else { return (0, None) };
        let declared = Some(resolve_ty(&expect.ty, &self.uf));
        if expect.accepts(first, &self.uf) {
            return (0, declared);
        }
        match types.iter().skip(1).position(|t| expect.accepts(t, &self.uf)) {
            Some(i) => (i + 1, declared),
            None => (0, None),
        }
    }

    /// ADR-0002 D3 (#3385): in a LIFTING tail — a `-> T!` body — every value
    /// exit lifts into `ok(..)` on its own: the lowering
    /// (`wrap_fallible_value_tail`) wraps each branch / arm leaf by its own
    /// type. So
    /// peers that mix a plain `T` with an explicit `Result[T, E]` are not a
    /// mismatch; they are compared at the lifted level. One rule for `if`
    /// branches and `match` arms.
    ///
    /// Only a MIXED peer set lifts (at least one Result peer and one concrete
    /// non-Result peer): an all-value join keeps the #880 sized-peer rule and
    /// an all-Result join is untouched. `Never` peers (an `err(..)` arm, a
    /// `panic`) and still-open inference variables are left as they are — the
    /// lift is decided by a peer's type, and an open one has none yet. A
    /// lifted peer keeps its own (unresolved) type inside the `Result`, so a
    /// literal still narrows to the declared payload (`-> Int8!`).
    ///
    /// An effect fn that declares `-> Result[T, E]` itself is a lifting tail
    /// too (#3395, ADR-0002 D3: a `-> Result[T, String]` body has the same
    /// auto-ok of a tail `T`): the lowering lifts its value leaves the same
    /// way, so `if` and `match` follow one rule on both targets. An effect
    /// fn declaring `-> T` is not — its expectation is not a `Result`.
    /// Returns the peer types to join, or `None` when nothing lifts.
    pub(super) fn lift_mixed_tail_peers(&self, expect: Option<&TailExpect>, tys: &[Ty]) -> Option<Vec<Ty>> {
        let expect = expect.filter(|e| e.effect_body)?;
        let Ty::Applied(TypeConstructorId::Result, want) = resolve_ty(&expect.ty, &self.uf) else { return None };
        if want.len() != 2 {
            return None;
        }
        let resolved: Vec<Ty> = tys.iter().map(|t| resolve_ty(t, &self.uf)).collect();
        let liftable = |t: &Ty| {
            !t.is_result()
                && !matches!(t, Ty::Never | Ty::Unknown)
                && super::types::is_inference_var(t).is_none()
        };
        if !resolved.iter().any(Ty::is_result) || !resolved.iter().any(liftable) {
            return None;
        }
        Some(
            tys.iter()
                .zip(&resolved)
                .map(|(t, r)| if liftable(r) { Ty::result(t.clone(), want[1].clone()) } else { t.clone() })
                .collect(),
        )
    }

    /// The un-`!`ed Result call a blamed peer wraps in `ok(..)` / `some(..)`
    /// — `ok(level())` in an effect fn is `Result[Result[T, E], E]` where
    /// `Result[T, E]` was meant (ADR-0008: propagation is spelled `!`).
    pub(super) fn wrapped_unbanged_call(&self, body: &ast::Expr) -> Option<(ast::Span, String)> {
        if !self.env.can_call_effect || self.env.lambda_depth > 0 {
            return None;
        }
        let inner = match &body.kind {
            ExprKind::Block { expr: Some(tail), .. } => return self.wrapped_unbanged_call(tail),
            ExprKind::Ok { expr } | ExprKind::Some { expr } => expr,
            _ => return None,
        };
        if !matches!(inner.kind, ExprKind::Call { .. }) {
            return None;
        }
        let ty = resolve_ty(self.type_map.get(&inner.id)?, &self.uf);
        if !ty.is_result() {
            return None;
        }
        let span = inner.span?;
        let text = self.source_slice(span).filter(|t| !t.contains('\n')).unwrap_or_else(|| call_text(inner));
        Some((span, text))
    }

    /// Constrain one peer against the anchor, reported at the peer.
    pub(super) fn constrain_peer(
        &mut self,
        anchor: (&Ty, Option<ast::Span>),
        peer: (&Ty, Option<ast::Span>),
        context: &str,
        hint: Option<FixHint>,
    ) {
        self.unify_infer(anchor.0, peer.0);
        self.constraints.push(Constraint {
            expected: anchor.0.clone(),
            actual: peer.0.clone(),
            context: context.into(),
            span: peer.1.or(self.current_span),
            fix_hint: hint,
        });
    }

    /// The `ArmBlame` part of an E001: the headline's tail naming what fixed
    /// the join's type, and — for the missing-`!` shape — the hint and the
    /// `!` insertion. `None` when the constraint carries no `ArmBlame`.
    pub(super) fn arm_blame_report(
        &self,
        c: &Constraint,
        exp: &Ty,
        act: &Ty,
    ) -> Option<(String, Option<String>, Option<ast::Span>)> {
        let Some(FixHint::ArmBlame { anchor, declared, bang, real }) = &c.fix_hint else { return None };
        let what = if c.context == "match arm" { "arm" } else { "branch" };
        let at = anchor.map(|s| format!(" at line {}", s.line)).unwrap_or_default();
        let (shown_exp, act, tail) = match declared {
            Some(d) => (
                d.clone(),
                self.with_slot_defaults(&resolve_ty(real, &self.uf)),
                format!(" — {} is the declared return type, and the {what}{at} produces it", d.display()),
            ),
            None => (
                exp.clone(),
                act.clone(),
                format!(" — the {}{at} fixed the type to {}", if what == "arm" { "first arm" } else { "`then` branch" }, exp.display()),
            ),
        };
        let headline = format!(
            "type mismatch in {}: expected {} but got {}{}",
            c.context, shown_exp.display(), act.display(), tail
        );
        let bang = bang.as_ref().filter(|_| nests_expected(&shown_exp, &act));
        let hint = bang.map(|(_, call)| {
            format!(
                "add `!` to `{call}`: without it the call's own Result is wrapped a second time. \
                 Propagation is explicit (ADR-0008) — `{call}!` unwraps the value and returns the error"
            )
        });
        Some((headline, hint, bang.map(|(s, _)| *s)))
    }
}

/// A call's spelling when the source text is not at hand: `callee(..)`.
fn call_text(call: &ast::Expr) -> String {
    fn callee(e: &ast::Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident { name } => Some(name.to_string()),
            ExprKind::Member { object, field } => Some(format!("{}.{}", callee(object)?, field)),
            _ => None,
        }
    }
    let ExprKind::Call { callee: f, args, .. } = &call.kind else { return "the call".into() };
    let name = callee(f).unwrap_or_else(|| "f".into());
    if args.is_empty() { format!("{name}()") } else { format!("{name}(..)") }
}

/// `act` is `C[C[..]]`-shaped where `exp` is `C[..]` (or `C[..]` holds a
/// `Result` where `exp` holds a plain value): the one-layer-too-many shape
/// a missing `!` inside `ok(..)` / `some(..)` produces.
fn nests_expected(exp: &Ty, act: &Ty) -> bool {
    match (exp, act) {
        (Ty::Applied(e, ea), Ty::Applied(a, aa)) if e == a && !ea.is_empty() && !aa.is_empty() => {
            aa[0].is_result() && !ea[0].is_result() || nests_expected(&ea[0], &aa[0])
        }
        _ => false,
    }
}
