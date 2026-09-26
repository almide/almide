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
//! renders it with its `Debug` text (`map_err`), which is a real conversion.
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
        let Some(channel) = self.bang_channel_err_ty() else { return };
        if matches!(channel, Ty::String | Ty::Unknown | Ty::TypeVar(_)) {
            return;
        }
        let shown = match classify_operand_err(&resolve_ty(operand, &self.uf), plain_is_effect_call) {
            OperandErr::Unjudged => return,
            OperandErr::ImplicitString(what) => format!("{what} fails with `String`"),
            OperandErr::Declared(op_err) => {
                if self.unify_infer(&channel, &op_err) {
                    return;
                }
                let what = if plain_is_effect_call { "this effect call" } else { "this `Result`" };
                format!("{what} fails with `{}`", resolve_ty(&op_err, &self.uf).display())
            }
        };
        let fn_err = channel.display();
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

    /// The error type a `!` in the current fn body propagates into: a
    /// `Result`-returning fn's `E`, or `String` for an effect fn with any other
    /// return (its lowered channel). `None` when there is no error channel.
    fn bang_channel_err_ty(&self) -> Option<Ty> {
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
