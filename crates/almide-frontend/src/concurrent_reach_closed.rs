//! E095 (ADR-0020 §5.2, #2698): the app passed to `http.serve` /
//! `http.serve_with_limits` is INSTANCE-CLOSED. Every name its executable
//! closure reads, and that no body inside it binds, is a top-level item.
//!
//! The app is evaluated in several instances — one per worker natively, one
//! per request on a wasi:http export host — and the enclosing fn's locals do
//! not exist in any of them. The bodies the closure reaches through top-level
//! fns and top-level `let`s cannot name a local (none is in their scope), so
//! the rule reduces to the app expression itself: a name it reads that
//! resolves to a binder of the enclosing fn (a `let`, a `var`, a parameter, a
//! pattern or loop binder) is the error. Binders inside the app — a lambda's
//! parameters, a `let` in its body, a nested lambda's own parameters — are
//! its own.
//!
//! The app slot is a concurrent slot, so it rides the same discovery as E008:
//! an app that is a parameter of the enclosing fn (`serve_on(port, h)`) moves
//! the slot to that fn's callers (§3.1), and each caller's argument is then
//! held to this rule in its own scope.

use super::{Analyzer, Mode, Scope, SlotKind};
use almide_base::intern::Sym;
use almide_lang::ast::{Expr, Span};

/// The serve surfaces whose declared `@concurrent` slot is the app.
const SERVE_SURFACES: &[&str] = &["http.serve", "http.serve_with_limits"];

/// One local an `http.serve` app reads.
#[derive(Clone, Debug)]
pub struct ClosureFinding {
    /// `http.serve` or `http.serve_with_limits`.
    pub surface: String,
    /// The user fn the app was passed to when the slot was inferred through
    /// a wrapper (`serve_on(8080, app)`).
    pub wrapper: Option<String>,
    /// The local the app reads.
    pub name: Sym,
    /// The top-level fn whose local it is (None in a `test` block or a
    /// top-level `let`).
    pub owner: Option<Sym>,
    /// The first read of `name` in the app.
    pub span: Option<Span>,
}

impl<'a> Analyzer<'a> {
    /// At a site in discovery: when its slot is the app of `http.serve` (or
    /// `serve_with_limits`), collect the locals the argument reads.
    pub(super) fn check_served_app(&mut self, app: &'a Expr, kind: &SlotKind, wrapper: Option<&str>, scope: &mut Scope<'a>) {
        let SlotKind::Handler { surface } = kind else { return };
        if !SERVE_SURFACES.contains(&surface.as_str()) {
            return;
        }
        let saved = std::mem::take(&mut self.closure_hits);
        self.walk(app, scope, Mode::Closed { base: scope.len() });
        let hits = std::mem::replace(&mut self.closure_hits, saved);
        for (name, span) in hits {
            self.closure_findings.push(ClosureFinding {
                surface: surface.clone(),
                wrapper: wrapper.map(str::to_string),
                name,
                owner: self.current_fn,
                span,
            });
        }
    }

    /// A name read in `Mode::Closed`: a hit when it resolves to a binder
    /// below `base` (outside the app). The first read of each name is kept.
    pub(super) fn closed_ref(&mut self, name: Sym, span: Option<Span>, scope: &Scope<'a>, base: usize) {
        let Some(pos) = scope.iter().rposition(|(n, _)| *n == name) else { return };
        if pos < base && !self.closure_hits.iter().any(|(n, _)| *n == name) {
            self.closure_hits.push((name, span));
        }
    }
}
