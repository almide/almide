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
            _ => None,
        }
    }

    pub(crate) fn hint_with_conversion(base_hint: &str, expected: &Ty, actual: &Ty) -> String {
        if let Some(conv) = Self::suggest_conversion(expected, actual) {
            format!("{}. Or {}", base_hint, conv)
        } else {
            base_hint.to_string()
        }
    }
}
