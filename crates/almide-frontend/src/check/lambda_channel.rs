//! A lambda's failure channel is the join of its `!` operands (ADR-0021 D1).
//!
//! A `!` inside a lambda propagates into the lambda's OWN channel
//! `Result[T, ε]` (#489, ADR-0009 D5). ε used to be fixed at `String`, so a
//! typed `!` operand erased its error into its text and the HOF call that
//! took the callback failed with `String` — the type then depended on how the
//! callback was spelled (#2601). ε is now decided like this:
//!
//! 1. **Context first.** ε is an open variable while the program is solved, so
//!    whatever the lambda flows into decides it: a typed slot `(A) -> B!E`, a
//!    `let` annotation, a fn's declared return, the `)!` of a `-> T!E` fn, an
//!    `err(e) => err(e)` arm, or a consumer that reads the error as a
//!    `String`. A consumer that needs `String` still gets it — every typed
//!    error converts to `String` (its repr text), so nothing accepted with the
//!    fixed `String` channel stops being accepted.
//! 2. **Then the join.** A channel no context decided takes the join of the
//!    operands' error types over `E ≤ String`: their common `E` when every
//!    operand agrees, `String` otherwise (an `Option`'s `none` and a
//!    non-`Result` effect call fail with `String`). An operand whose error
//!    type is still unresolved when the lambda closes makes the join `String`.
//! 3. **Then each `!` is judged** against the decided ε, as a fn body's `!` is
//!    against the fn's error type (`bang_error_channel`, ADR-0003): an equal
//!    error propagates unchanged, any error propagates into a `String`
//!    channel, and anything else into a typed ε is E022 at that `!`.
//!
//! The join always has an answer (`String` is the top), so the rule adds no
//! inference failure. An effect-slot lambda keeps its `String` carrier.
//!
//! The E022 of step 3 (ADR-0021 D3) says where ε came from when a typed slot
//! or the enclosing fn's `)!` decided it, names the operand that DOES agree
//! when the callback's operands disagree among themselves (D3-2), and shows
//! the conversion at the odd `!` — `op |> result.map_err((e) => Case(e))!`,
//! with the case filled in (and offered as a fix-it) when exactly one case of
//! ε carries the odd operand's error type.
use super::{Checker, err};
use super::types::resolve_ty;
use crate::ast::Span;
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use almide_base::intern::Sym;

/// What one `!` inside the lambda fails with.
#[derive(Clone, Debug)]
pub(crate) enum OperandErr {
    /// A `Result[_, E]` operand: `E` (possibly still a variable).
    Declared(Ty),
    /// An `Option`'s `none` or a non-`Result` effect call: always `String`.
    ImplicitString(&'static str),
    /// An error value the body RETURNS into the channel before its tail: a
    /// `guard … else err(..)`, a `guard let`'s else, or a value-join match's
    /// `err(..)` arm — the `E` of that `Result`.
    Returned(Ty),
    /// An operand whose type was still a bare variable at its `!` (a `let`-bound
    /// lambda's unannotated parameter): classified once the program has
    /// decided it. Its error type is unresolved when the lambda closes (D1-3).
    Deferred(Ty),
}

#[derive(Clone, Debug)]
pub(crate) struct Operand {
    pub err: OperandErr,
    pub span: Option<Span>,
    /// `span` is the operand's own span (not the enclosing statement's), so
    /// a fix-it may rewrite it.
    pub exact: bool,
}

/// One lambda's channel while its body is inferred, and after it closes.
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    pub eps: Ty,
    pub operands: Vec<Operand>,
    pub owner: Option<Sym>,
    /// Where a typed ε came from, when the checker saw it cheaply: the typed
    /// slot the lambda was passed to, or the enclosing fn's `)!` (D3-1).
    pub source: Option<String>,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct LambdaChannels {
    /// The lambdas being inferred, innermost last (`!` falls into the last).
    open: Vec<Channel>,
    /// `(ε, join)` for every closed fallible lambda: applied after every
    /// program constraint and BEFORE the `ok`/`err` slot defaults (#2599),
    /// so the join fills ε only where the program said nothing.
    defaults: Vec<(Ty, Ty)>,
    /// Closed fallible lambdas, judged once ε is decided.
    closed: Vec<Channel>,
    /// The typed slot the next lambda argument lands in (set by the call's
    /// argument loop, taken by the lambda when its channel opens).
    pending_source: Option<String>,
}

/// An operand as the program has decided it by now.
enum Now {
    /// Fails with this error type (`returned`: an `err(..)` the body returns).
    Err { ty: Ty, returned: bool },
    /// An `Option`'s `none` / a non-`Result` effect call: `String`.
    Implicit(&'static str),
    /// Nothing can be said (still open, or error recovery).
    Open,
}

impl Checker {
    /// A lambda starts inferring its body: its channel's ε is `eps`.
    pub(super) fn open_lambda_channel(&mut self, eps: Ty) {
        let owner = self.current_fn.as_ref().map(|f| f.0);
        let source = self.lambda_channels.pending_source.take();
        self.lambda_channels.open.push(Channel { eps, operands: Vec::new(), owner, source });
    }

    /// The call's argument loop: the lambda argument about to be inferred lands
    /// in `slot`. Returns the previous value, for the loop to restore.
    pub(super) fn swap_pending_lambda_source(&mut self, slot: Option<String>) -> Option<String> {
        std::mem::replace(&mut self.lambda_channels.pending_source, slot)
    }

    /// A `!` in the innermost lambda's body propagates `err` into its channel.
    pub(super) fn record_lambda_operand(&mut self, err: OperandErr, span: Option<Span>) {
        let exact = span.is_some();
        let span = span.or(self.current_span);
        if let Some(ch) = self.lambda_channels.open.last_mut() {
            ch.operands.push(Operand { err, span, exact });
        }
    }

    /// A fn body's `!` whose operand fails with a closed lambda's still-open
    /// ε pins it to the fn's error type: remember that as ε's source (D3-1).
    pub(super) fn note_fn_bang_channel_source(&mut self, operand: &Ty) {
        let Some(fn_err) = self.bang_channel_err_ty() else { return };
        if matches!(fn_err, Ty::String | Ty::Unknown) || super::types::is_inference_var(&fn_err).is_some() {
            return;
        }
        let Ty::Applied(TypeConstructorId::Result, args) = resolve_ty(operand, &self.uf) else { return };
        let Some(e) = args.get(1).map(|e| resolve_ty(e, &self.uf)) else { return };
        if super::types::is_inference_var(&e).is_none() {
            return;
        }
        for ch in &mut self.lambda_channels.closed {
            if ch.source.is_none() && resolve_ty(&ch.eps, &self.uf) == e {
                ch.source = Some("the enclosing fn's error type, which the call's `!` propagates into".to_string());
            }
        }
    }

    /// The lambda's body is inferred. `fallible`: it used its channel. The
    /// body's own `Result` tail (already unified with the channel) is part of
    /// the channel and needs no separate entry.
    pub(super) fn close_lambda_channel(&mut self, fallible: bool) {
        let Some(ch) = self.lambda_channels.open.pop() else { return };
        if !fallible {
            return;
        }
        // An operand whose error type is still open joins ε (its error IS the
        // channel's), and makes the join `String` (D1-3).
        let mut unresolved = false;
        let mut errs: Vec<Ty> = Vec::new();
        for op in &ch.operands {
            match self.operand_now(&op.err) {
                Now::Err { ty, .. } if matches!(ty, Ty::Unknown) => unresolved = true,
                Now::Err { ty, .. } if super::types::is_inference_var(&ty).is_some() => {
                    unresolved = true;
                    self.unify_infer(&ty, &ch.eps);
                }
                Now::Err { ty, .. } => errs.push(ty),
                Now::Implicit(_) => errs.push(Ty::String),
                Now::Open => unresolved = true,
            }
        }
        let join = match errs.split_first() {
            Some((first, rest)) if !unresolved && rest.iter().all(|e| e == first) => first.clone(),
            _ => Ty::String,
        };
        if super::types::is_inference_var(&resolve_ty(&ch.eps, &self.uf)).is_some() {
            self.lambda_channels.defaults.push((ch.eps.clone(), join));
        }
        self.record_decided_erasures(&ch);
        self.lambda_channels.closed.push(ch);
    }

    /// What `err` fails with, as far as the program has decided it now.
    fn operand_now(&self, err: &OperandErr) -> Now {
        match err {
            OperandErr::Declared(e) => Now::Err { ty: resolve_ty(e, &self.uf), returned: false },
            OperandErr::Returned(e) => Now::Err { ty: resolve_ty(e, &self.uf), returned: true },
            OperandErr::ImplicitString(what) => Now::Implicit(what),
            OperandErr::Deferred(op) => match resolve_ty(op, &self.uf) {
                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
                    Now::Err { ty: resolve_ty(&args[1], &self.uf), returned: false }
                }
                Ty::Applied(TypeConstructorId::Option, _) => Now::Implicit("an `Option`'s `none`"),
                _ => Now::Open,
            },
        }
    }

    /// #2601's E022 names the callback whose `!` erased a typed error: record
    /// every typed operand of a channel ALREADY decided as `String` here (the
    /// body's `String` tail or an effect-slot carrier pinned it).
    fn record_decided_erasures(&mut self, ch: &Channel) {
        if resolve_ty(&ch.eps, &self.uf) != Ty::String {
            return;
        }
        for op in &ch.operands {
            if let OperandErr::Declared(e) | OperandErr::Returned(e) = &op.err {
                let e = resolve_ty(e, &self.uf);
                if !matches!(e, Ty::String | Ty::Unknown) && super::types::is_inference_var(&e).is_none() {
                    self.lambda_err_erasures.push((e, op.span, ch.owner));
                }
            }
        }
    }

    /// The `(ε, join)` defaults still pending — for a diagnostic raised before
    /// they are applied, which should show ε as the join it will take.
    pub(super) fn lambda_channel_defaults(&self) -> &[(Ty, Ty)] {
        &self.lambda_channels.defaults
    }

    /// Fill every ε the program left open with its join (D1-2).
    pub(super) fn apply_lambda_channel_defaults(&mut self) {
        for (eps, join) in std::mem::take(&mut self.lambda_channels.defaults) {
            if super::types::is_inference_var(&resolve_ty(&eps, &self.uf)).is_some() {
                self.unify_infer(&eps, &join);
            }
        }
    }

    /// Judge every `!` of every closed lambda against its decided ε (D1-1).
    ///
    /// One E022 per odd operand, at that operand: each is its own place to
    /// convert, with its own fix-it. When some operand of the same callback
    /// DOES fail with ε, the message names both — the agreeing one and the odd
    /// one, with line and column (D3-2); a callback with several odd operands
    /// gets one E022 at each, every one naming the same agreeing operand.
    pub(super) fn judge_lambda_channels(&mut self) {
        for ch in std::mem::take(&mut self.lambda_channels.closed) {
            let eps = resolve_ty(&ch.eps, &self.uf);
            if matches!(eps, Ty::String | Ty::Unknown) || super::types::is_inference_var(&eps).is_some() {
                continue;
            }
            let mut agreeing: Option<Span> = None;
            let mut odd: Vec<(&Operand, Now)> = Vec::new();
            for op in &ch.operands {
                match self.operand_now(&op.err) {
                    Now::Err { ty, .. } if matches!(ty, Ty::Unknown) => {}
                    Now::Err { ty, returned } => {
                        if self.unify_infer(&eps, &ty) {
                            agreeing = agreeing.or(op.span);
                        } else {
                            odd.push((op, Now::Err { ty, returned }));
                        }
                    }
                    Now::Implicit(what) => odd.push((op, Now::Implicit(what))),
                    Now::Open => {}
                }
            }
            for (op, now) in odd {
                self.report_lambda_channel_mismatch(&eps, &ch, op, &now, agreeing);
            }
        }
    }

    /// A `guard`/`guard let` else or a value-join `err(..)` arm inside a lambda
    /// returns into the lambda's channel (the value is the lambda's result, the
    /// same as a `!`'s error): its error type joins ε. A plain `ok(..)` else
    /// says nothing about the error.
    pub(super) fn record_lambda_returned_err(&mut self, else_ty: &Ty, else_is_ok: bool, span: Option<Span>) {
        if self.env.lambda_depth == 0 || else_is_ok {
            return;
        }
        if let Ty::Applied(TypeConstructorId::Result, args) = resolve_ty(else_ty, &self.uf)
            && args.len() == 2
        {
            self.record_lambda_operand(OperandErr::Returned(args[1].clone()), span);
        }
    }

    fn report_lambda_channel_mismatch(&mut self, eps: &Ty, ch: &Channel, op: &Operand, now: &Now, agreeing: Option<Span>) {
        let eps_shown = eps.display();
        let (odd_ty, returned, implicit) = match now {
            Now::Err { ty, returned } => (self.with_slot_defaults(ty), *returned, None),
            Now::Implicit(what) => (Ty::String, false, Some(*what)),
            Now::Open => return,
        };
        let odd_shown = odd_ty.display();
        let source = ch.source.as_ref().map(|s| format!(" (from {s})")).unwrap_or_default();
        let at = |s: Option<Span>| s.map(|s| format!(" (line {}, col {})", s.line, s.col)).unwrap_or_default();
        let subject = format!("the callback's error type is `{eps_shown}`{source}");
        let head = if returned {
            "this error cannot leave the callback"
        } else {
            "operator '!' cannot propagate this error"
        };
        let message = match agreeing {
            // D3-2: the callback's own operands disagree — name both.
            Some(ok_at) => format!(
                "{head}: {subject}, but the callback's {} fail with different types — `{eps_shown}`{} and `{odd_shown}`{}",
                if returned { "`!`s and `err(..)`s" } else { "`!`s" },
                at(Some(ok_at)),
                at(op.span),
            ),
            None => {
                let what = match (implicit, returned) {
                    (Some(what), _) => what.to_string(),
                    (None, true) => "this `err(..)`".to_string(),
                    (None, false) => "this `Result`".to_string(),
                };
                format!("{head}: {subject}, but {what} fails with `{odd_shown}`")
            }
        };
        // The direction-specific conversion (ADR-0003 D2) at the odd operand.
        let case = self.unique_case_carrying(eps, &odd_ty);
        let case_shown = case.map(|c| c.to_string()).unwrap_or_else(|| "SomeCase".to_string());
        let op_text = op.span.filter(|_| op.exact && implicit.is_none() && !returned).and_then(|s| self.operand_text(s));
        let example = match &op_text {
            Some((text, wrap)) => {
                let converted = format!("{text} |> result.map_err((e) => {case_shown}(e))");
                if *wrap { format!("({converted})!") } else { format!("{converted}!") }
            }
            None => format!("result.map_err((e) => {case_shown}(e))"),
        };
        let fix = if agreeing.is_some() {
            format!("Convert the odd one at its `!` so every `!` in the callback fails with `{eps_shown}`")
        } else {
            "Convert this one at its `!`".to_string()
        };
        let hint = if returned {
            format!(
                "A callback's `err(..)` leaves through the callback's own failure channel, whose error type is \
                 `{eps_shown}`: return a `{eps_shown}` case here instead, e.g. `err({case_shown}(...))`"
            )
        } else if implicit.is_some() {
            format!(
                "A `!` inside a lambda propagates into the lambda's own failure channel, and every `!` in it must fail \
                 with the channel's error type `{eps_shown}` (`!` converts nothing into a typed error). {fix} — turn it \
                 into a `Result[_, {eps_shown}]` first — or handle it here with `match` or `?? default`"
            )
        } else {
            format!(
                "A `!` inside a lambda propagates into the lambda's own failure channel, and every `!` in it must fail \
                 with the channel's error type `{eps_shown}` (`!` converts nothing into a typed error). {fix}: \
                 `{example}` — or handle it here with `match` or `?? default`"
            )
        };
        let saved = self.current_span;
        if op.span.is_some() {
            self.current_span = op.span;
        }
        let mut diag = err(message, hint, if returned { "callback error" } else { "operator !" }).with_code("E022");
        if let (Some(s), Some((text, wrap)), Some(case)) = (op.span.filter(|_| op_text.is_some()), &op_text, case) {
            let converted = format!("{text} |> result.map_err((e) => {case}(e))");
            let replacement = if *wrap { format!("({converted})") } else { converted };
            diag = diag.with_suggested_fix(s.line, s.col, s.end_col, replacement);
        } else if op_text.is_some() {
            diag = diag.with_try(example);
        }
        self.emit(diag);
        self.current_span = saved;
    }

    /// The one case of `eps` (a variant) whose payload is exactly `(err)`, so
    /// `(e) => Case(e)` converts `err` into `eps`. `None` when there is no
    /// such case or more than one.
    fn unique_case_carrying(&self, eps: &Ty, err: &Ty) -> Option<Sym> {
        let Ty::Variant { cases, .. } = self.env.resolve_named(eps) else { return None };
        let mut hits = cases.iter().filter(|c| matches!(&c.payload, VariantPayload::Tuple(ts) if ts.len() == 1 && &ts[0] == err));
        let first = hits.next()?;
        if hits.next().is_some() { None } else { Some(first.name) }
    }

    /// The operand's source text, and whether the conversion must be wrapped
    /// in parentheses: `|>` binds looser than arithmetic, so `1 + op |> f` would
    /// pipe the sum. `None` when the span does not name one line of source.
    fn operand_text(&self, s: Span) -> Option<(String, bool)> {
        if s.end_col <= s.col {
            return None;
        }
        let text = self.source_slice(s)?;
        let balanced = |open: char, close: char| text.matches(open).count() == text.matches(close).count();
        if text.trim().is_empty() || !balanced('(', ')') || !balanced('[', ']') || !balanced('{', '}') {
            return None;
        }
        let before = self.source_slice(Span { line: s.line, col: 1, end_col: s.col }).unwrap_or_default();
        let before = before.trim_end();
        let wrap = before.ends_with(['+', '-', '*', '/', '%', '^', '<', '.'])
            || before.ends_with(" not")
            || before == "not";
        Some((text, wrap))
    }
}

/// `the slot `f: (Int) -> Int!E`` — the D3-1 source of a lambda argument whose
/// slot `(name, ty)` declares a typed error; `None` for any other slot (no
/// `Result` return, a generic or `String` error), which does not decide ε.
pub(super) fn slot_source(name: &Sym, ty: &Ty) -> Option<String> {
    let Ty::Fn { params, ret, is_effect: false } = ty else { return None };
    let Ty::Applied(TypeConstructorId::Result, args) = ret.as_ref() else { return None };
    let [ok, e] = args.as_slice() else { return None };
    if matches!(e, Ty::String | Ty::Unknown | Ty::TypeVar(_)) || e.has_unresolved_deep() {
        return None;
    }
    let ps: Vec<String> = params.iter().map(|p| p.display()).collect();
    Some(format!("the slot `{name}: ({}) -> {}!{}`", ps.join(", "), ok.display(), e.display()))
}

/// The operand classification a lambda's `!` records (mirrors
/// `bang_error_channel`'s): `None` for an operand nothing can be said about.
pub(super) fn classify_lambda_operand(op: &Ty, plain_is_effect_call: bool) -> Option<OperandErr> {
    match op {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => Some(OperandErr::Declared(args[1].clone())),
        Ty::Applied(TypeConstructorId::Option, _) => Some(OperandErr::ImplicitString("an `Option`'s `none`")),
        Ty::Unknown => None,
        Ty::TypeVar(_) => Some(OperandErr::Deferred(op.clone())),
        _ if plain_is_effect_call => Some(OperandErr::ImplicitString("an effect fn that does not return `Result`")),
        _ => None,
    }
}

impl Checker {
    /// #3464: a `fan.settle { }` arm that propagates with `!` is a channel
    /// scope of its own, as a `fan { }` arm is (#3462) and as the mapper
    /// form's callback is: the `!` ends the ARM with its Err, which becomes
    /// that arm's slot. ε is decided as a lambda's is (the join of the arm's
    /// `!`s, ADR-0021 D1). Returns the slot `Result[T, ε]`, or `None` when
    /// the arm has no `!` of its own (its slot keeps the plain rule).
    pub(super) fn infer_settle_arm_scope(&mut self, arm: &mut crate::ast::Expr) -> Option<Ty> {
        if !settle_arm_propagates(arm) {
            return None;
        }
        let ok = self.fresh_var();
        let eps = self.fresh_var();
        let chan = Ty::result(ok.clone(), eps.clone());
        let saved_ret = self.env.lambda_ret.replace(chan.clone());
        let saved_used = std::mem::replace(&mut self.env.lambda_prop_used, false);
        self.env.lambda_depth += 1;
        self.open_lambda_channel(eps);
        let t = self.infer_expr(arm);
        self.env.lambda_depth -= 1;
        let used = std::mem::replace(&mut self.env.lambda_prop_used, saved_used);
        self.env.lambda_ret = saved_ret;
        self.close_lambda_channel(used);
        let body = resolve_ty(&t, &self.uf);
        if body.is_result() {
            self.constrain(chan.clone(), t, "fan.settle arm");
        } else if body != Ty::Never {
            self.constrain(ok, t, "fan.settle arm");
        }
        Some(chan)
    }
}

/// Does the arm hold a `!` of its own — one not inside a lambda or a nested
/// fan form, which are scopes of their own?
fn settle_arm_propagates(arm: &crate::ast::Expr) -> bool {
    use crate::ast::{visit_expr, Expr, ExprKind};
    let mut bangs: Vec<*const Expr> = Vec::new();
    let mut nested: std::collections::HashSet<*const Expr> = std::collections::HashSet::new();
    visit_expr(arm, &mut |c| match c.kind {
        ExprKind::Unwrap { .. } => bangs.push(c),
        ExprKind::Lambda { .. } | ExprKind::Fan { .. } | ExprKind::FanSettle { .. } if !std::ptr::eq(c, arm) => {
            visit_expr(c, &mut |d| {
                if matches!(d.kind, ExprKind::Unwrap { .. }) {
                    nested.insert(d);
                }
            });
        }
        _ => {}
    });
    bangs.iter().any(|b| !nested.contains(b))
}
