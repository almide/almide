/// Builtin call handling and type utilities for call checking.

use crate::types::{Ty, TypeConstructorId};
use super::types::resolve_ty;
use super::Checker;

/// Map a built-in type to its stdlib UFCS module name.
pub(crate) fn builtin_module_for_type(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Applied(TypeConstructorId::List, _) => Some("list"),
        Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
        Ty::Applied(TypeConstructorId::Set, _) => Some("set"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
        // Sized numeric types (Stage 3 of the sized-numeric-types arc).
        // Each has its own bundled module for UFCS conversion methods.
        Ty::Int8 => Some("int8"),
        Ty::Int16 => Some("int16"),
        Ty::Int32 => Some("int32"),
        Ty::UInt8 => Some("uint8"),
        Ty::UInt16 => Some("uint16"),
        Ty::UInt32 => Some("uint32"),
        Ty::UInt64 => Some("uint64"),
        Ty::Float32 => Some("float32"),
        Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
        Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
        _ => None,
    }
}

/// Check if an argument type mismatches a parameter slot.
///
/// Matches the "expected accepts actual" semantic: the provided value
/// must be assignable to the parameter. Asymmetric compat rules (e.g.
/// `Matrix.compatible(Matrix[T])` = true but the reverse is false)
/// depend on this directional check; the bidirectional variant used
/// before P5 Matrix[T] discrimination let narrowing assignments slip
/// through silently.
pub(crate) fn types_mismatch(expected: &Ty, actual: &Ty) -> bool {
    *expected != Ty::Unknown && *actual != Ty::Unknown
        && !expected.compatible(actual)
}

impl Checker {
    /// Try to resolve a call to a builtin function.
    /// Returns Some(Ty) if the name is a builtin, None otherwise.
    pub(super) fn check_builtin_call(&mut self, name: &str, arg_tys: &[Ty]) -> Option<Ty> {
        if let Some(ty) = self.check_builtin_output(name, arg_tys) { return Some(ty); }
        if let Some(ty) = self.check_builtin_ctor(name, arg_tys) { return Some(ty); }
        None
    }

    /// The output and abort builtins: `println`, `eprintln`, `panic` and the
    /// assert family.
    ///
    /// One group of `check_builtin_call`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group" — the router tries the groups in that
    /// order, so which rule a call gets is unchanged.
    pub(super) fn check_builtin_output(&mut self, name: &str, arg_tys: &[Ty]) -> Option<Ty> {
        match name {
            "println" | "eprintln" => {
                // println/eprintln require String argument
                if let Some(first) = arg_tys.first() {
                    self.constrain(Ty::String, first.clone(), format!("call to {}()", name));
                }
                Some(Ty::Unit)
            }
            "panic" => {
                if let Some(first) = arg_tys.first() {
                    self.constrain(Ty::String, first.clone(), "call to panic()".to_string());
                }
                Some(Ty::Never)
            }
            "assert" => Some(Ty::Unit),
            "assert_eq" | "assert_ne" => {
                if arg_tys.len() >= 2 {
                    self.constrain(arg_tys[0].clone(), arg_tys[1].clone(), format!("call to {}()", name));
                }
                Some(Ty::Unit)
            }
            _ => return None,
        }
    }

    /// The `Result` / `Option` constructors and `unwrap_or`.
    ///
    /// One group of `check_builtin_call`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group" — the router tries the groups in that
    /// order, so which rule a call gets is unchanged.
    pub(super) fn check_builtin_ctor(&mut self, name: &str, arg_tys: &[Ty]) -> Option<Ty> {
        if let Some(ty) = self.check_builtin_result_ctor(name, arg_tys) { return Some(ty); }
        if let Some(ty) = self.check_builtin_option_ctor(name, arg_tys) { return Some(ty); }
        None
    }

    /// `ok` and `err`.
    ///
    /// One group of `check_builtin_call`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group" — the router tries the groups in that
    /// order, so the dispatch is unchanged.
    pub(super) fn check_builtin_result_ctor(&mut self, name: &str, arg_tys: &[Ty]) -> Option<Ty> {
        match name {
            "ok" => {
                let ok_ty = arg_tys.first().cloned().unwrap_or(Ty::Unit);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => Ty::String,
                };
                Some(Ty::result(ok_ty, err_ty))
            }
            "err" => {
                let err_ty = arg_tys.first().cloned().unwrap_or(Ty::String);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => Ty::Unit,
                };
                Some(Ty::result(ok_ty, err_ty))
            }
            _ => return None,
        }
    }

    /// `some`, and `unwrap_or`'s fallback typing.
    ///
    /// One group of `check_builtin_call`'s arm table, arms verbatim and in source
    /// order. `None` means "not my group" — the router tries the groups in that
    /// order, so the dispatch is unchanged.
    pub(super) fn check_builtin_option_ctor(&mut self, name: &str, arg_tys: &[Ty]) -> Option<Ty> {
        match name {
            "some" => Some(Ty::option(arg_tys.first().cloned().unwrap_or_else(|| self.fresh_var()))),
            "unwrap_or" if arg_tys.len() >= 2 => {
                let concrete = resolve_ty(&arg_tys[0], &self.uf);
                Some(match &concrete {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => arg_tys[1].clone(),
                })
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_module_list() { assert_eq!(builtin_module_for_type(&Ty::list(Ty::Int)), Some("list")); }
    #[test]
    fn builtin_module_string() { assert_eq!(builtin_module_for_type(&Ty::String), Some("string")); }
    #[test]
    fn builtin_module_int() { assert_eq!(builtin_module_for_type(&Ty::Int), Some("int")); }
    #[test]
    fn builtin_module_float() { assert_eq!(builtin_module_for_type(&Ty::Float), Some("float")); }
    #[test]
    fn builtin_module_map() { assert_eq!(builtin_module_for_type(&Ty::map_of(Ty::String, Ty::Int)), Some("map")); }
    #[test]
    fn builtin_module_result() { assert_eq!(builtin_module_for_type(&Ty::result(Ty::Int, Ty::String)), Some("result")); }
    #[test]
    fn builtin_module_option() { assert_eq!(builtin_module_for_type(&Ty::option(Ty::Int)), Some("option")); }
    #[test]
    fn builtin_module_none() { assert_eq!(builtin_module_for_type(&Ty::Bool), None); }

    #[test]
    fn mismatch_same_type() { assert!(!types_mismatch(&Ty::Int, &Ty::Int)); }
    #[test]
    fn mismatch_different_types() { assert!(types_mismatch(&Ty::Int, &Ty::String)); }
    #[test]
    fn mismatch_unknown_permissive() {
        assert!(!types_mismatch(&Ty::Unknown, &Ty::Int));
        assert!(!types_mismatch(&Ty::Int, &Ty::Unknown));
    }
}
