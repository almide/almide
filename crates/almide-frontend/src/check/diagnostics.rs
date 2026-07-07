/// Diagnostic hints: type conversion suggestions for error messages.

use crate::types::Ty;
use super::Checker;

impl Checker {
    /// Returns (description, template) where template uses `{}` for the source expression.
    /// e.g. ("Int to String", "int.to_string({})")
    pub(crate) fn conversion_template(expected: &Ty, actual: &Ty) -> Option<(&'static str, &'static str)> {
        match (actual, expected) {
            (Ty::Int, Ty::String) => Some(("Int to String", "int.to_string({})")),
            (Ty::Float, Ty::String) => Some(("Float to String", "float.to_string({})")),
            (Ty::Bool, Ty::String) => Some(("Bool to String", "to_string({})")),
            (Ty::String, Ty::Int) => Some(("String to Int", "int.parse({})")),
            (Ty::String, Ty::Float) => Some(("String to Float", "float.parse({})")),
            (Ty::Float, Ty::Int) => Some(("Float to Int", "float.to_int({})")),
            (Ty::Int, Ty::Float) => Some(("Int to Float", "int.to_float({})")),
            _ => None,
        }
    }

    /// Int-only `math.*` builtins whose generic Float→Int hint (`float.to_int`)
    /// would silently truncate. When one of these gets a Float argument, the
    /// caller points at the Float-preserving sibling instead (#740).
    pub(crate) fn math_float_sibling(fn_name: &str) -> Option<&'static str> {
        match fn_name {
            "math.abs" => Some("float.abs"),
            "math.pow" => Some("math.fpow"),
            "math.max" => Some("math.fmax"),
            "math.min" => Some("math.fmin"),
            _ => None,
        }
    }

    pub(crate) fn suggest_conversion(expected: &Ty, actual: &Ty) -> Option<String> {
        match (actual, expected) {
            (Ty::Int, Ty::String) => Some("use `int.to_string(x)` to convert Int to String".to_string()),
            (Ty::Float, Ty::String) => Some("use `float.to_string(x)` to convert Float to String".to_string()),
            (Ty::Bool, Ty::String) => Some("use `to_string(x)` to convert Bool to String".to_string()),
            (Ty::String, Ty::Int) => Some("use `int.parse(s)` to convert String to Int (returns Result[Int, String])".to_string()),
            (Ty::String, Ty::Float) => Some("use `float.parse(s)` to convert String to Float (returns Result[Float, String])".to_string()),
            (Ty::Float, Ty::Int) => Some("use `float.to_int(x)` to convert Float to Int (truncates)".to_string()),
            (Ty::Int, Ty::Float) => Some("use `int.to_float(x)` to convert Int to Float".to_string()),
            // Unit where a List was expected: three common causes.
            // - `list.push/pop/clear` mutate and return Unit
            // - `for x in xs { ... }` is a side-effect loop returning Unit
            // - `println(...)` / other effect calls returning Unit
            (Ty::Unit, Ty::Applied(crate::types::TypeConstructorId::List, _)) =>
                Some("Got Unit where a List was expected. \
                      `list.push`/`pop`/`clear` mutate and return Unit — use `xs + [item]` for an immutable append. \
                      `for x in xs { ... }` is a side-effect loop (Unit); for element transforms, use `list.map(xs, (x) => ...)`.".to_string()),
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
