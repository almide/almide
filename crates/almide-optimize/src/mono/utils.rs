use std::collections::HashMap;
use almide_lang::types::Ty;

/// Key for a monomorphized instance: (function_name, concrete_type_suffix).
pub(super) type MonoKey = (String, String);

/// Info about a structurally-bounded type parameter in a function.
pub(super) struct BoundedParam {
    /// Index of the parameter in the function signature
    pub(super) param_idx: usize,
    /// Name of the type variable (e.g., "T")
    pub(super) type_var: String,
}

/// Generate a mangled suffix from type variable bindings.
pub(super) fn mangle_suffix(bindings: &HashMap<String, Ty>) -> String {
    let mut entries: Vec<(&String, &Ty)> = bindings.iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());
    entries.iter().map(|(_, ty)| mangle_ty(ty)).collect::<Vec<_>>().join("_")
}

pub(super) fn mangle_ty(ty: &Ty) -> String {
    if let Some(name) = mangle_scalar_ty_name(ty) {
        return name.to_string();
    }
    match ty {
        Ty::Named(name, args) => {
            if args.is_empty() { name.to_string() }
            else {
                let arg_strs: Vec<String> = args.iter().map(mangle_ty).collect();
                format!("{}_{}", name, arg_strs.join("_"))
            }
        }
        Ty::Record { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
            names.sort();
            names.join("_")
        }
        Ty::Applied(almide_lang::types::TypeConstructorId::List, args) if args.len() == 1 => format!("List_{}", mangle_ty(&args[0])),
        Ty::Applied(id, args) => {
            let name = id.to_string();
            if args.is_empty() { name } else {
                let arg_strs: Vec<String> = args.iter().map(mangle_ty).collect();
                format!("{}_{}", name, arg_strs.join("_"))
            }
        }
        _ => "Unknown".into(),
    }
}

/// Mangled name for a fixed-name scalar `Ty`. Returns `None` for the
/// structural/compound variants (`Named`, `Record`, `Applied`, ...) that
/// `mangle_ty` handles itself.
fn mangle_scalar_ty_name(ty: &Ty) -> Option<&'static str> {
    Some(match ty {
        Ty::Int => "Int",
        Ty::Float => "Float",
        Ty::Int8 => "Int8",
        Ty::Int16 => "Int16",
        Ty::Int32 => "Int32",
        Ty::UInt8 => "UInt8",
        Ty::UInt16 => "UInt16",
        Ty::UInt32 => "UInt32",
        Ty::UInt64 => "UInt64",
        Ty::Float32 => "Float32",
        Ty::String => "String",
        Ty::Bool => "Bool",
        Ty::Bytes => "Bytes",
        Ty::Matrix => "Matrix",
        Ty::Unit => "Unit",
        _ => return None,
    })
}

/// Extract the concrete type name from a Ty for protocol method rewriting.
pub(super) fn ty_to_name(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Named(name, _) => Some(name.to_string()),
        Ty::Int => Some("Int".into()),
        Ty::Float => Some("Float".into()),
        Ty::Int8 => Some("Int8".into()),
        Ty::Int16 => Some("Int16".into()),
        Ty::Int32 => Some("Int32".into()),
        Ty::UInt8 => Some("UInt8".into()),
        Ty::UInt16 => Some("UInt16".into()),
        Ty::UInt32 => Some("UInt32".into()),
        Ty::UInt64 => Some("UInt64".into()),
        Ty::Float32 => Some("Float32".into()),
        Ty::String => Some("String".into()),
        Ty::Bool => Some("Bool".into()),
        Ty::Bytes => Some("Bytes".into()),
        Ty::Matrix => Some("Matrix".into()),
        Ty::Unit => Some("Unit".into()),
        _ => None,
    }
}

/// Check if a type contains a specific TypeVar anywhere in its structure.
/// Uses Ty::any_child_recursive for uniform traversal.
pub(super) fn ty_contains_typevar(ty: &Ty, name: &str) -> bool {
    ty.any_child_recursive(&|t| match t {
        Ty::TypeVar(n) => n == name,
        Ty::Named(n, args) => n == name && args.is_empty(),
        _ => false,
    })
}

pub(super) fn has_typevar(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| {
        matches!(t, Ty::TypeVar(_))
            || matches!(t, Ty::Named(n, args) if args.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
    })
}
