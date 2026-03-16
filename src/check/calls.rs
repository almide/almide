/// Call type checking — resolves function calls, builtins, variant constructors.

use std::collections::HashMap;
use crate::ast;
use crate::types::Ty;
use super::types::InferTy;
use super::Checker;

/// Map a built-in type to its stdlib UFCS module name.
pub(crate) fn builtin_module_for_type(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::List(_) => Some("list"),
        Ty::Map(_, _) => Some("map"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
        Ty::Result(_, _) => Some("result"),
        Ty::Option(_) => Some("option"),
        _ => None,
    }
}

/// Check if two types are mismatched (neither Unknown, not compatible in either direction).
pub(crate) fn types_mismatch(expected: &Ty, actual: &Ty) -> bool {
    *expected != Ty::Unknown && *actual != Ty::Unknown
        && !expected.compatible(actual) && !actual.compatible(expected)
}

impl Checker {
    pub(crate) fn check_call(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr]) -> InferTy {
        self.check_call_with_type_args(callee, args, None)
    }

    pub(crate) fn check_call_with_type_args(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr], type_args: Option<&[Ty]>) -> InferTy {
        let arg_tys: Vec<InferTy> = args.iter_mut().map(|a| self.infer_expr(a)).collect();
        match callee {
            ast::Expr::Ident { name, .. } => {
                let name = name.clone();
                // Register callee's type for variables that hold function values
                // (Skip for builtins/functions — they don't need ExprId registration)
                if self.env.lookup_var(&name).is_some() {
                    let _ = self.infer_expr(callee);
                }
                self.check_named_call_with_type_args(&name, &arg_tys, type_args)
            }
            ast::Expr::TypeName { name, .. } => {
                if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
                    self.check_constructor_args(name, &case, &arg_tys);
                    // Instantiate parent type's generics with fresh inference vars
                    let generic_args = self.instantiate_type_generics(&type_name);
                    InferTy::Concrete(Ty::Named(type_name, generic_args))
                } else { InferTy::Concrete(Ty::Named(name.clone(), vec![])) }
            }
            // Module call: string.trim(s), list.map(xs, f), etc.
            ast::Expr::Member { object, field, .. } => {
                // Try static resolution: module.func, alias.func, TypeName.method, codec.encode
                if let Some(result) = self.resolve_static_member(object, field, &arg_tys) {
                    return result;
                }
                // UFCS method: obj.method(args) → module.method(obj, args)
                let obj_ty = self.infer_expr(object);
                let obj_concrete = obj_ty.to_ty(&self.solutions);
                // Built-in generic types → stdlib module UFCS
                let builtin_module = builtin_module_for_type(&obj_concrete);
                if let Some(module) = builtin_module {
                    let key = format!("{}.{}", module, field);
                    if self.env.functions.contains_key(&key)
                        || crate::stdlib::resolve_ufcs_candidates(field).contains(&module)
                    {
                        let mut all_args = vec![obj_ty];
                        all_args.extend(arg_tys.iter().cloned());
                        return self.check_named_call(&key, &all_args);
                    }
                }
                // Convention method: dog.repr() → Dog.repr(dog)
                let type_name_opt = match &obj_concrete {
                    Ty::Named(name, _) => Some(name.clone()),
                    Ty::Record { .. } | Ty::Variant { .. } => {
                        // Reverse lookup: find type name whose definition matches this structure
                        self.env.types.iter().find_map(|(name, ty)| {
                            if ty == &obj_concrete && name.chars().next().map_or(false, |c| c.is_uppercase()) {
                                Some(name.clone())
                            } else { None }
                        })
                    }
                    _ => None,
                };
                if let Some(type_name) = type_name_opt {
                    let convention_key = format!("{}.{}", type_name, field);
                    if self.env.functions.contains_key(&convention_key) {
                        let mut all_args = vec![obj_ty];
                        all_args.extend(arg_tys.iter().cloned());
                        return self.check_named_call(&convention_key, &all_args);
                    }
                }
                // UFCS: user-defined function obj.func(args) → func(obj, args)
                if self.env.functions.contains_key(field) {
                    let mut all_args = vec![obj_ty];
                    all_args.extend(arg_tys.iter().cloned());
                    return self.check_named_call(field, &all_args);
                }
                let ret = self.fresh_var();
                self.constrain(obj_ty, InferTy::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "method call");
                ret
            }
            _ => {
                let ct = self.infer_expr(callee);
                let ret = self.fresh_var();
                self.constrain(ct, InferTy::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "function call");
                ret
            }
        }
    }

    pub(crate) fn check_named_call(&mut self, name: &str, arg_tys: &[InferTy]) -> InferTy {
        self.check_named_call_with_type_args(name, arg_tys, None)
    }

    pub(crate) fn check_named_call_with_type_args(&mut self, name: &str, arg_tys: &[InferTy], type_args: Option<&[Ty]>) -> InferTy {
        // Builtins that accept any types
        match name {
            "println" | "eprintln" => {
                // println/eprintln require String argument
                if let Some(first) = arg_tys.first() {
                    self.constrain(InferTy::Concrete(Ty::String), first.clone(), format!("call to {}()", name));
                }
                return InferTy::Concrete(Ty::Unit);
            }
            "assert" => return InferTy::Concrete(Ty::Unit),
            "assert_eq" | "assert_ne" => {
                if arg_tys.len() >= 2 {
                    self.constrain(arg_tys[0].clone(), arg_tys[1].clone(), format!("call to {}()", name));
                }
                return InferTy::Concrete(Ty::Unit);
            }
            _ => {}
        }
        match name {
            "ok" => {
                let ok_ty = arg_tys.first().cloned().unwrap_or(InferTy::Concrete(Ty::Unit));
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Result(_, e)) => InferTy::from_ty(e),
                    _ => InferTy::Concrete(Ty::String),
                };
                InferTy::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            "err" => {
                let err_ty = arg_tys.first().cloned().unwrap_or(InferTy::Concrete(Ty::String));
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Result(o, _)) => InferTy::from_ty(o),
                    _ => InferTy::Concrete(Ty::Unit),
                };
                InferTy::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            "some" => InferTy::Option(Box::new(arg_tys.first().cloned().unwrap_or(self.fresh_var()))),
            "unwrap_or" if arg_tys.len() >= 2 => {
                let concrete = arg_tys[0].to_ty(&self.solutions);
                match &concrete {
                    Ty::Option(inner) => InferTy::from_ty(inner),
                    Ty::Result(ok, _) => InferTy::from_ty(ok),
                    _ => arg_tys[1].clone(),
                }
            }
            _ => {
                // Try stdlib lookup for module-qualified calls (e.g. "string.trim")
                let sig = self.env.functions.get(name).cloned().or_else(|| {
                    let (module, func) = name.split_once('.')?;
                    crate::stdlib::lookup_sig(module, func)
                });

                let Some(sig) = sig else {
                    // No function signature found — try constructor, variable, or report error
                    if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
                        self.check_constructor_args(name, &case, arg_tys);
                        let generic_args = self.instantiate_type_generics(&type_name);
                        return InferTy::Concrete(Ty::Named(type_name, generic_args));
                    }
                    if let Some(ty) = self.env.lookup_var(name).cloned() {
                        return match &ty {
                            Ty::Fn { params, ret } => {
                                for (aty, pty) in arg_tys.iter().zip(params.iter()) {
                                    self.constrain(InferTy::from_ty(pty), aty.clone(), format!("call to {}()", name));
                                }
                                InferTy::from_ty(ret)
                            }
                            _ => InferTy::from_ty(&ty)
                        };
                    }
                    self.diagnostics.push(super::err(format!("undefined function '{}'", name), "Check the function name", format!("call to {}()", name)));
                    return InferTy::Concrete(Ty::Unknown);
                };

                // Validate argument count
                let min_params = match name.split_once('.') {
                    Some((module, func)) => crate::stdlib::min_params(module, func).unwrap_or(sig.params.len()),
                    None => self.env.fn_min_params.get(name).copied().unwrap_or(sig.params.len()),
                };
                if arg_tys.len() < min_params || arg_tys.len() > sig.params.len() {
                    self.diagnostics.push(super::err(
                        format!("{}() expects {} argument(s) but got {}", name, sig.params.len(), arg_tys.len()),
                        "Check the number of arguments", format!("call to {}()", name)));
                }
                // Validate argument types and infer generics
                let mut bindings = HashMap::new();
                if let Some(ta) = type_args {
                    for (gname, gty) in sig.generics.iter().zip(ta.iter()) {
                        bindings.insert(gname.clone(), gty.clone());
                    }
                }
                let concrete_args: Vec<Ty> = arg_tys.iter().map(|a| a.to_ty(&self.solutions)).collect();
                for ((pname, pty), aty) in sig.params.iter().zip(concrete_args.iter()) {
                    self.unify_call_arg(name, pname, pty, aty, &sig.structural_bounds, &mut bindings);
                }
                // Propagate resolved types back to inference variables
                for ((_, pty), aty) in sig.params.iter().zip(arg_tys.iter()) {
                    let expected = if bindings.is_empty() { pty.clone() } else { crate::types::substitute(pty, &bindings) };
                    if expected != Ty::Unknown {
                        self.constrain(InferTy::from_ty(&expected), aty.clone(), format!("call to {}()", name));
                    }
                }
                let ret = if bindings.is_empty() { sig.ret.clone() } else { crate::types::substitute(&sig.ret, &bindings) };
                InferTy::from_ty(&ret)
            }
        }
    }

    /// Create fresh inference variables for a type's generic parameters.
    fn instantiate_type_generics(&mut self, type_name: &str) -> Vec<Ty> {
        // Count generics by finding TypeVars in the type definition
        if let Some(ty_def) = self.env.types.get(type_name).cloned() {
            let mut type_vars = Vec::new();
            crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_vars);
            type_vars.iter().map(|_| {
                let var = self.fresh_var();
                var.to_ty(&self.solutions)
            }).collect()
        } else {
            vec![]
        }
    }

    fn check_constructor_args(&mut self, name: &str, case: &crate::types::VariantCase, arg_tys: &[InferTy]) {
        if let crate::types::VariantPayload::Tuple(expected_tys) = &case.payload {
            if arg_tys.len() != expected_tys.len() {
                self.diagnostics.push(super::err(
                    format!("{}() expects {} argument(s) but got {}", name, expected_tys.len(), arg_tys.len()),
                    "Check the number of arguments", format!("constructor {}()", name)));
            }
            for (i, (aty, ety)) in arg_tys.iter().zip(expected_tys.iter()).enumerate() {
                let concrete_arg = aty.to_ty(&self.solutions);
                if concrete_arg != Ty::Unknown && !ety.compatible(&concrete_arg) {
                    self.diagnostics.push(super::err(
                        format!("{}() argument {} expects {} but got {}", name, i + 1, ety.display(), concrete_arg.display()),
                        "Fix the argument type", format!("constructor {}()", name)));
                }
            }
        }
    }

    /// Unify a single call argument against its parameter type, updating bindings.
    /// Reports diagnostics for structural bound violations and type mismatches.
    fn unify_call_arg(
        &mut self, fn_name: &str, param_name: &str,
        param_ty: &Ty, arg_ty: &Ty,
        structural_bounds: &HashMap<String, Ty>,
        bindings: &mut HashMap<String, Ty>,
    ) {
        if let Ty::TypeVar(tv) = param_ty {
            if let Some(bound) = structural_bounds.get(tv) {
                let resolved = self.env.resolve_named(arg_ty);
                if bound.compatible(&resolved) || bound.compatible(arg_ty) {
                    bindings.insert(tv.clone(), arg_ty.clone());
                } else {
                    self.diagnostics.push(super::err(
                        format!("argument '{}' does not satisfy bound {}: got {}", param_name, bound.display(), arg_ty.display()),
                        "The argument must have the required fields",
                        format!("call to {}()", fn_name)));
                }
            } else {
                crate::types::unify(param_ty, arg_ty, bindings);
            }
        } else {
            crate::types::unify(param_ty, arg_ty, bindings);
            let expected = if bindings.is_empty() { param_ty.clone() } else { crate::types::substitute(param_ty, bindings) };
            let expected_resolved = self.env.resolve_named(&expected);
            let arg_resolved = self.env.resolve_named(arg_ty);
            if types_mismatch(&expected_resolved, &arg_resolved) {
                self.diagnostics.push(super::err(
                    format!("argument '{}' expects {} but got {}", param_name, expected.display(), arg_ty.display()),
                    Self::hint_with_conversion("Fix the argument type", &expected, arg_ty),
                    format!("call to {}()", fn_name)));
            }
        }
    }

    /// Resolve a member call statically (module.func, alias, TypeName.method, codec).
    /// Returns Some(InferTy) if resolved, None to fall through to UFCS/convention dispatch.
    fn resolve_static_member(&mut self, object: &ast::Expr, field: &str, arg_tys: &[InferTy]) -> Option<InferTy> {
        let module_name = match object {
            ast::Expr::Ident { name, .. } => Some(name.as_str()),
            _ => None,
        };

        if let Some(module) = module_name {
            // Codec convenience: json.encode(t) → String when t has T.encode
            if field == "encode" && arg_tys.len() == 1 {
                let arg_concrete = arg_tys[0].to_ty(&self.solutions);
                if self.has_codec_encode(&arg_concrete) {
                    return Some(InferTy::Concrete(Ty::String));
                }
            }

            // Direct stdlib/user module call, or resolved alias
            let resolved_module = if crate::stdlib::is_stdlib_module(module) || self.env.user_modules.contains(module) {
                Some(module.to_string())
            } else {
                self.env.module_aliases.get(module).cloned()
            };
            if let Some(m) = resolved_module {
                return Some(self.check_named_call(&format!("{}.{}", m, field), arg_tys));
            }
        }

        // TypeName.method() — direct convention call
        if let ast::Expr::TypeName { name: type_name, .. } = object {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&key) {
                return Some(self.check_named_call(&key, arg_tys));
            }
        }

        None
    }

    /// Check if a type has a Codec encode function registered.
    fn has_codec_encode(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Named(name, _) => self.env.functions.contains_key(&format!("{}.encode", name)),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().any(|(name, t)| t == ty && self.env.functions.contains_key(&format!("{}.encode", name)))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_module_list() { assert_eq!(builtin_module_for_type(&Ty::List(Box::new(Ty::Int))), Some("list")); }
    #[test]
    fn builtin_module_string() { assert_eq!(builtin_module_for_type(&Ty::String), Some("string")); }
    #[test]
    fn builtin_module_int() { assert_eq!(builtin_module_for_type(&Ty::Int), Some("int")); }
    #[test]
    fn builtin_module_float() { assert_eq!(builtin_module_for_type(&Ty::Float), Some("float")); }
    #[test]
    fn builtin_module_map() { assert_eq!(builtin_module_for_type(&Ty::Map(Box::new(Ty::String), Box::new(Ty::Int))), Some("map")); }
    #[test]
    fn builtin_module_result() { assert_eq!(builtin_module_for_type(&Ty::Result(Box::new(Ty::Int), Box::new(Ty::String))), Some("result")); }
    #[test]
    fn builtin_module_option() { assert_eq!(builtin_module_for_type(&Ty::Option(Box::new(Ty::Int))), Some("option")); }
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
