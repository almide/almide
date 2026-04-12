/// Diagnostic hints: type conversion suggestions for error messages.

use crate::types::Ty;
use super::Checker;

impl Checker {
    pub(crate) fn suggest_conversion(expected: &Ty, actual: &Ty) -> Option<String> {
        match (actual, expected) {
            (Ty::Int, Ty::String) => Some("use `int.to_string(x)` to convert Int to String".to_string()),
            (Ty::Float, Ty::String) => Some("use `float.to_string(x)` to convert Float to String".to_string()),
            (Ty::Bool, Ty::String) => Some("use `to_string(x)` to convert Bool to String".to_string()),
            (Ty::String, Ty::Int) => Some("use `int.parse(s)` to convert String to Int (returns Result[Int, String])".to_string()),
            (Ty::String, Ty::Float) => Some("use `float.parse(s)` to convert String to Float (returns Result[Float, String])".to_string()),
            (Ty::Float, Ty::Int) => Some("use `to_int(x)` to convert Float to Int (truncates)".to_string()),
            (Ty::Int, Ty::Float) => Some("use `to_float(x)` to convert Int to Float".to_string()),
            // list.push/pop/clear return Unit; suggest `+` for immutable list building
            (Ty::Unit, Ty::Applied(crate::types::TypeConstructorId::List, _)) =>
                Some("`list.push` mutates a var and returns Unit. For immutable lists, use `+` to build: `xs + [item]`".to_string()),
            // Option[Unit] vs Option[List[T]] — same pattern wrapped in Option
            (Ty::Applied(crate::types::TypeConstructorId::Option, a_args),
             Ty::Applied(crate::types::TypeConstructorId::Option, e_args))
                if a_args.first() == Some(&Ty::Unit)
                && matches!(e_args.first(), Some(Ty::Applied(crate::types::TypeConstructorId::List, _))) =>
                Some("`list.push` returns Unit. Use `+` for immutable list building: `some(xs + [item])` instead of `some(list.push(xs, item))`".to_string()),
            _ => None,
        }
    }

    pub(crate) fn hint_with_conversion(base_hint: &str, expected: &Ty, actual: &Ty) -> String {
        if let Some(conv) = Self::suggest_conversion(expected, actual) {
            return format!("{}. Or {}", base_hint, conv);
        }
        // For function types, compare return types for conversion hints
        if let (Ty::Fn { ret: ret_a, .. }, Ty::Fn { ret: ret_e, .. }) = (actual, expected) {
            if let Some(conv) = Self::suggest_conversion(ret_e, ret_a) {
                return format!("{}. {}", base_hint, conv);
            }
        }
        base_hint.to_string()
    }
}
