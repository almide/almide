//! `expr!` must propagate an error of the enclosing fn's OWN error type (#2635).
//!
//! The lowered `?` converts nothing: the error it carries out has to already be
//! the fn's error type. Three operands carry a `String` error the writer never
//! spelled: an effect call whose declared return is not a `Result` (`-> Bool`,
//! `-> Unit`, `-> Int`, ...; the effect channel is `Result[T, String]`), a
//! `Result[T, String]`, and an `Option` (whose `none` becomes a manufactured
//! `String` error). In a fn whose error type is `String` all three flow; in a fn
//! whose error type is a user type (`-> Result[T, Failure]`, `-> T!Failure`) none
//! of them has a value of that type, and the program used to pass `almide check`
//! and die in rustc with E0277. There is no conversion from a `String` to an
//! arbitrary user type, so the mismatch is a check-time E022.
//!
//! A variant error flowing into a `String` fn stays accepted: the lowering
//! renders it with its repr text (`map_err` to `almide_repr`, the text
//! `"${e}"` shows — #2725), which is a real conversion.
use super::{Checker, err};
use super::types::resolve_ty;
use crate::types::{Ty, TypeConstructorId};

/// What a `!` operand fails with, as far as the error channel is concerned.
enum OperandErr {
    /// A `Result[_, E]` operand: `E`.
    Declared(Ty),
    /// An error the writer never spelled, always a `String`: an `Option`'s
    /// `none`, or the channel of an effect call that does not return `Result`.
    ImplicitString(&'static str),
    /// Not decidable here (an unresolved operand, or a plain value E034 reports).
    Unjudged,
}

impl Checker {
    /// Reject a `!` whose operand's error cannot become the enclosing fn's error
    /// type. `plain_is_effect_call`: the operand is a call to an effect fn (the
    /// checker types a non-`Result` one as `Result[T, String]` already; a
    /// plain operand only survives on the never-err path).
    pub(super) fn check_bang_error_channel(&mut self, operand: &Ty, plain_is_effect_call: bool) {
        let Some((fn_err, shown)) = self.channel_mismatch(operand, plain_is_effect_call) else { return };
        self.emit(err(
            format!("operator '!' cannot propagate this error: the fn's error type is `{fn_err}`, but {shown}"),
            format!(
                "`!` passes the error on unchanged, so it must already be a `{fn_err}`. \
                 Handle it here instead: `match` on the result and return `err(...)` with a `{fn_err}` case, \
                 use `?? default` for a fallback value, or `let _ = f()` to discard a Unit call's error"
            ),
            "operator !",
        ).with_code("E022"));
    }

    /// #3467: a `fan { }` arm's Err is the block's Err (C-199), which leaves
    /// the fn as a `!` would — so an arm that yields a `Result` (or is an
    /// effect call) must fail with the fn's own error type. An arm written
    /// `x!` was judged at its `!`. There is no conversion from `String` to a
    /// user type: the mismatch was rustc E0277 natively.
    pub(super) fn check_fan_arm_channel(&mut self, arm: &crate::ast::Expr, arm_ty: &Ty, is_effect_call: bool) {
        if self.env.in_test_block || matches!(arm.kind, crate::ast::ExprKind::Unwrap { .. }) {
            return;
        }
        let Some((fn_err, shown)) = self.channel_mismatch(arm_ty, is_effect_call) else { return };
        let saved = self.current_span;
        self.current_span = arm.span.or(saved);
        self.emit(err(
            format!("this fan arm's error cannot leave the block: the fn's error type is `{fn_err}`, but {shown}"),
            format!(
                "A fan block's first Err is the fn's Err, so every arm must fail with `{fn_err}`. \
                 Handle this arm's error inside it: `match` on the result and build a `{fn_err}`, \
                 or use `?? default` for a fallback value"
            ),
            "fan arm",
        ).with_code("E022"));
        self.current_span = saved;
    }

    /// The fn's error type and what the operand fails with instead, when the
    /// operand's error cannot become the fn's error type.
    fn channel_mismatch(&mut self, operand: &Ty, plain_is_effect_call: bool) -> Option<(String, String)> {
        let channel = self.bang_channel_err_ty()?;
        if matches!(channel, Ty::String | Ty::Unknown | Ty::TypeVar(_)) {
            return None;
        }
        let shown = match classify_operand_err(&resolve_ty(operand, &self.uf), plain_is_effect_call) {
            OperandErr::Unjudged => return None,
            OperandErr::ImplicitString(what) => format!("{what} fails with `String`"),
            OperandErr::Declared(op_err) => {
                if self.unify_infer(&channel, &op_err) || self.report_lambda_erasure(&channel, &op_err) {
                    return None;
                }
                let what = if plain_is_effect_call { "this effect call" } else { "this `Result`" };
                format!("{what} fails with `{}`", resolve_ty(&op_err, &self.uf).display())
            }
        };
        Some((channel.display(), shown))
    }

    /// #2601: the operand fails with `String` because a callback inside it
    /// erased a typed error — a `!` in a lambda propagates into the lambda's
    /// own failure channel, and that channel is `String` when the callback's
    /// `!`s (or its `Result` result) do not all fail with one type (ADR-0021
    /// D1). "Fails with `String`" was true and pointed away from the cause, so
    /// name the callback's `!`.
    fn report_lambda_erasure(&mut self, channel: &Ty, op_err: &Ty) -> bool {
        if resolve_ty(op_err, &self.uf) != Ty::String {
            return false;
        }
        let Some(mark) = self.bang_erasure_mark else { return false };
        let Some((erased, at, _)) = self.lambda_err_erasures.get(mark..).and_then(|es| es.first()).cloned() else {
            return false;
        };
        let fn_err = channel.display();
        let erased = erased.display();
        let at = at.map(|s| format!(" (line {}, col {})", s.line, s.col)).unwrap_or_default();
        self.emit(err(
            format!(
                "operator '!' cannot propagate this error: the fn's error type is `{fn_err}`, but the callback's `!`{at} \
                 turned its `{erased}` error into `String` — a `!` inside a lambda propagates into the lambda's own \
                 failure channel, which is `String` here because not everything in the callback fails with `{erased}`"
            ),
            format!(
                "A callback fails with `{erased}` when every `!` in it, and its `Result` result if it has one, fails \
                 with `{erased}`. Convert the odd one where it happens — `result.map_err((e) => SomeCase(e))` before \
                 its `!`, or a `{erased}` case in place of a `String` `err(..)` — so the call that takes the callback \
                 fails with `{erased}`."
            ),
            "operator !",
        ).with_code("E022"));
        true
    }

    /// The error type a `!` in the current fn body propagates into: a
    /// `Result`-returning fn's `E`, or `String` for an effect fn with any other
    /// return (its lowered channel). `None` when there is no error channel.
    pub(super) fn bang_channel_err_ty(&self) -> Option<Ty> {
        let ret = self.env.current_ret.as_ref().map(|r| resolve_ty(r, &self.uf));
        match ret {
            Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => {
                Some(resolve_ty(&args[1], &self.uf))
            }
            _ if self.env.auto_unwrap => Some(Ty::String),
            _ => None,
        }
    }
}

fn classify_operand_err(op: &Ty, plain_is_effect_call: bool) -> OperandErr {
    match op {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => OperandErr::Declared(args[1].clone()),
        Ty::Applied(TypeConstructorId::Option, _) => OperandErr::ImplicitString("an `Option`'s `none`"),
        Ty::Unknown | Ty::TypeVar(_) => OperandErr::Unjudged,
        _ if plain_is_effect_call => OperandErr::ImplicitString("an effect fn that does not return `Result`"),
        _ => OperandErr::Unjudged,
    }
}
