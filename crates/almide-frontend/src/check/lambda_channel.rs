//! A lambda's failure channel is the join of its `!` operands (ADR-0021 D1).
//!
//! A `!` inside a lambda propagates into the lambda's OWN channel
//! `Result[T, ε]` (#489, ADR-0009 D5). ε used to be fixed at `String`, so a
//! typed `!` operand erased its error into Debug text and the HOF call that
//! took the callback failed with `String` — the type then depended on how the
//! callback was spelled (#2601). ε is now decided like this:
//!
//! 1. **Context first.** ε is an open variable while the program is solved, so
//!    whatever the lambda flows into decides it: a typed slot `(A) -> B!E`, a
//!    `let` annotation, a fn's declared return, the `)!` of a `-> T!E` fn, an
//!    `err(e) => err(e)` arm, or a consumer that reads the error as a
//!    `String`. A consumer that needs `String` still gets it — every typed
//!    error converts to `String` (Debug text), so nothing accepted with the
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
use super::{Checker, err};
use super::types::resolve_ty;
use crate::ast::Span;
use crate::types::{Ty, TypeConstructorId};
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
}

/// One lambda's channel while its body is inferred, and after it closes.
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    pub eps: Ty,
    pub operands: Vec<Operand>,
    pub owner: Option<Sym>,
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
}

impl Checker {
    /// A lambda starts inferring its body: its channel's ε is `eps`.
    pub(super) fn open_lambda_channel(&mut self, eps: Ty) {
        let owner = self.current_fn.as_ref().map(|f| f.0);
        self.lambda_channels.open.push(Channel { eps, operands: Vec::new(), owner });
    }

    /// A `!` in the innermost lambda's body propagates `err` into its channel.
    pub(super) fn record_lambda_operand(&mut self, err: OperandErr, span: Option<Span>) {
        let span = span.or(self.current_span);
        if let Some(ch) = self.lambda_channels.open.last_mut() {
            ch.operands.push(Operand { err, span });
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
            match &op.err {
                OperandErr::Declared(e) | OperandErr::Returned(e) => {
                    let r = resolve_ty(e, &self.uf);
                    if matches!(r, Ty::Unknown) {
                        unresolved = true;
                    } else if super::types::is_inference_var(&r).is_some() {
                        unresolved = true;
                        self.unify_infer(&r, &ch.eps);
                    } else {
                        errs.push(r);
                    }
                }
                OperandErr::ImplicitString(_) => errs.push(Ty::String),
                OperandErr::Deferred(_) => unresolved = true,
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
    pub(super) fn judge_lambda_channels(&mut self) {
        for ch in std::mem::take(&mut self.lambda_channels.closed) {
            let eps = resolve_ty(&ch.eps, &self.uf);
            if matches!(eps, Ty::String | Ty::Unknown) || super::types::is_inference_var(&eps).is_some() {
                continue;
            }
            for op in &ch.operands {
                let err = match &op.err {
                    OperandErr::Deferred(t) => match resolve_ty(t, &self.uf) {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => OperandErr::Declared(args[1].clone()),
                        Ty::Applied(TypeConstructorId::Option, _) => OperandErr::ImplicitString("an `Option`'s `none`"),
                        _ => continue,
                    },
                    other => other.clone(),
                };
                let shown = match &err {
                    OperandErr::ImplicitString(what) => format!("{what} fails with `String`"),
                    OperandErr::Deferred(_) => continue,
                    OperandErr::Declared(e) | OperandErr::Returned(e) => {
                        let r = resolve_ty(e, &self.uf);
                        if matches!(r, Ty::Unknown) || self.unify_infer(&eps, &r) {
                            continue;
                        }
                        let what = if matches!(err, OperandErr::Returned(_)) { "this `err(..)`" } else { "this `Result`" };
                        format!("{what} fails with `{}`", self.with_slot_defaults(&r).display())
                    }
                };
                self.report_lambda_channel_mismatch(&eps, &shown, op.span);
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

    fn report_lambda_channel_mismatch(&mut self, eps: &Ty, shown: &str, span: Option<Span>) {
        let eps = eps.display();
        let saved = self.current_span;
        if span.is_some() {
            self.current_span = span;
        }
        self.emit(err(
            if shown.starts_with("this `err(..)`") {
                format!("this error cannot leave the callback: the callback's error type is `{eps}`, but {shown}")
            } else {
                format!("operator '!' cannot propagate this error: the callback's error type is `{eps}`, but {shown}")
            },
            format!(
                "A `!` inside a lambda propagates into the lambda's own failure channel, and every `!` in it must fail \
                 with the channel's error type `{eps}` (`!` converts nothing into a typed error). Convert this one at \
                 its `!` — e.g. `result.map_err((e) => SomeCase(e))` before the `!` — or handle it here with `match` \
                 or `?? default`"
            ),
            if shown.starts_with("this `err(..)`") { "callback error" } else { "operator !" },
        ).with_code("E022"));
        self.current_span = saved;
    }
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
