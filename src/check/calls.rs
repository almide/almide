/// Call type checking — resolves function calls, builtins, variant constructors.

use std::collections::HashMap;
use crate::ast;
/// Extract the effective return type from a function type, auto-unwrapping Result.
fn unwrap_fn_return(fn_ty: &Ty) -> Option<Ty> {
    if let Ty::Fn { ret, .. } = fn_ty {
        Some(match ret.as_ref() {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
            other => other.clone(),
        })
    } else {
        None
    }
}

/// Extract the Result type from List[Fn() -> Result[T, E]] → Result[T, E]
fn unwrap_list_fn_result_ty(list_ty: &Ty) -> Ty {
    match list_ty {
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            match &args[0] {
                Ty::Fn { ret, .. } => match ret.as_ref() {
                    r @ Ty::Applied(TypeConstructorId::Result, _) => r.clone(),
                    other => Ty::result(other.clone(), Ty::String),
                },
                _ => Ty::Unknown,
            }
        }
        _ => Ty::Unknown,
    }
}

/// Extract the element's effective return type from List[Fn() -> Result[T, E]] → T
fn unwrap_list_fn_return(list_ty: &Ty) -> Ty {
    match list_ty {
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            unwrap_fn_return(&args[0]).unwrap_or(Ty::Unknown)
        }
        _ => Ty::Unknown,
    }
}


use crate::types::{Ty, TypeConstructorId};
use super::types::resolve_ty;
use super::Checker;

/// Map a built-in type to its stdlib UFCS module name.
pub(crate) fn builtin_module_for_type(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Applied(TypeConstructorId::List, _) => Some("list"),
        Ty::Applied(TypeConstructorId::Map, _) => Some("map"),
        Ty::String => Some("string"),
        Ty::Int => Some("int"),
        Ty::Float => Some("float"),
        Ty::Applied(TypeConstructorId::Result, _) => Some("result"),
        Ty::Applied(TypeConstructorId::Option, _) => Some("option"),
        _ => None,
    }
}

/// Check if two types are mismatched (neither Unknown, not compatible in either direction).
pub(crate) fn types_mismatch(expected: &Ty, actual: &Ty) -> bool {
    *expected != Ty::Unknown && *actual != Ty::Unknown
        && !expected.compatible(actual) && !actual.compatible(expected)
}

impl Checker {
    pub(crate) fn check_call_with_type_args(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr], type_args: Option<&[Ty]>) -> Ty {
        let arg_tys: Vec<Ty> = args.iter_mut().map(|a| self.infer_expr(a)).collect();
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
                    Ty::Named(type_name, generic_args)
                } else { Ty::Named(name.clone(), vec![]) }
            }
            // Module call: string.trim(s), list.map(xs, f), etc.
            ast::Expr::Member { object, field, .. } => {
                // Try static resolution: module.func, alias.func, TypeName.method, codec.encode
                if let Some(result) = self.resolve_static_member(object, field, &arg_tys) {
                    return result;
                }
                // UFCS method: obj.method(args) → module.method(obj, args)
                let obj_ty = self.infer_expr(object);
                let obj_concrete = resolve_ty(&obj_ty, &self.uf);
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
                let type_name_opt = self.resolve_type_name(&obj_concrete);
                if let Some(type_name) = type_name_opt {
                    let convention_key = format!("{}.{}", type_name, field);
                    if self.env.functions.contains_key(&convention_key) {
                        let mut all_args = vec![obj_ty];
                        all_args.extend(arg_tys.iter().cloned());
                        return self.check_named_call(&convention_key, &all_args);
                    }
                }
                // Protocol method on TypeVar: item.show() where item: T, T: Showable
                if let Ty::TypeVar(tv) = &obj_concrete {
                    if let Some(proto_names) = self.env.generic_protocol_bounds.get(tv).cloned() {
                        for proto_name in &proto_names {
                            if let Some(proto_def) = self.env.protocols.get(proto_name).cloned() {
                                if let Some(method_sig) = proto_def.methods.iter().find(|m| m.name == *field) {
                                    // Resolve method return type: substitute Self → T (the TypeVar)
                                    let ret = self.substitute_self_in_ty(&method_sig.ret, &obj_concrete);
                                    return ret;
                                }
                            }
                        }
                    }
                }
                // UFCS: user-defined function obj.func(args) → func(obj, args)
                if self.env.functions.contains_key(field) {
                    let mut all_args = vec![obj_ty];
                    all_args.extend(arg_tys.iter().cloned());
                    return self.check_named_call(field, &all_args);
                }
                let ret = self.fresh_var();
                self.constrain(obj_ty, Ty::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "method call");
                ret
            }
            _ => {
                let ct = self.infer_expr(callee);
                let ret = self.fresh_var();
                self.constrain(ct, Ty::Fn { params: arg_tys.to_vec(), ret: Box::new(ret.clone()) }, "function call");
                ret
            }
        }
    }

    /// Resolve a concrete type to its declared type name.
    fn resolve_type_name(&self, ty: &Ty) -> Option<String> {
        match ty {
            Ty::Named(name, _) => Some(name.clone()),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().find_map(|(name, def)| {
                    (def == ty && name.starts_with(|c: char| c.is_uppercase())).then(|| name.clone())
                })
            }
            _ => None,
        }
    }

    /// Resolve a type to its name for protocol checking purposes.
    /// Handles Named types, Records/Variants (by looking up type definitions),
    /// and TypeVars (which are not concrete — returns None to skip checking).
    fn resolve_type_name_for_protocol(&self, ty: &Ty) -> Option<String> {
        match ty {
            Ty::Named(name, _) => Some(name.clone()),
            Ty::Record { .. } | Ty::Variant { .. } => {
                self.env.types.iter().find_map(|(name, def)| {
                    (def == ty && name.starts_with(|c: char| c.is_uppercase())).then(|| name.clone())
                })
            }
            // TypeVars and inference vars are not concrete — skip protocol checking
            Ty::TypeVar(_) | Ty::Unknown => None,
            _ => None,
        }
    }

    pub(crate) fn check_named_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        self.check_named_call_with_type_args(name, arg_tys, None)
    }

    pub(crate) fn check_named_call_with_type_args(&mut self, name: &str, arg_tys: &[Ty], type_args: Option<&[Ty]>) -> Ty {
        // Builtins that accept any types
        match name {
            "println" | "eprintln" => {
                // println/eprintln require String argument
                if let Some(first) = arg_tys.first() {
                    self.constrain(Ty::String, first.clone(), format!("call to {}()", name));
                }
                return Ty::Unit;
            }
            "assert" => return Ty::Unit,
            "assert_eq" | "assert_ne" => {
                if arg_tys.len() >= 2 {
                    self.constrain(arg_tys[0].clone(), arg_tys[1].clone(), format!("call to {}()", name));
                }
                return Ty::Unit;
            }
            _ => {}
        }
        match name {
            "ok" => {
                let ok_ty = arg_tys.first().cloned().unwrap_or(Ty::Unit);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => Ty::String,
                };
                Ty::result(ok_ty, err_ty)
            }
            "err" => {
                let err_ty = arg_tys.first().cloned().unwrap_or(Ty::String);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => Ty::Unit,
                };
                Ty::result(ok_ty, err_ty)
            }
            "some" => Ty::option(arg_tys.first().cloned().unwrap_or_else(|| self.fresh_var())),
            "unwrap_or" if arg_tys.len() >= 2 => {
                let concrete = resolve_ty(&arg_tys[0], &self.uf);
                match &concrete {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
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
                        return Ty::Named(type_name, generic_args);
                    }
                    if let Some(ty) = self.env.lookup_var(name).cloned() {
                        if let Ty::Fn { params, ret } = &ty {
                            arg_tys.iter().zip(params.iter()).for_each(|(aty, pty)| {
                                self.constrain(pty.clone(), aty.clone(), format!("call to {}()", name));
                            });
                            return ret.as_ref().clone();
                        }
                        return ty;
                    }
                    self.emit(super::err(format!("undefined function '{}'", name), "Check the function name", format!("call to {}()", name)).with_code("E002"));
                    return Ty::Unknown;
                };

                // Effect isolation: pure fn cannot call effect fn
                if sig.is_effect && !self.env.can_call_effect {
                    self.emit(super::err(
                        format!("cannot call effect function '{}' from a pure function", name),
                        "Mark the calling function as `effect fn`",
                        format!("call to {}()", name)).with_code("E006"));
                }

                // Validate argument count
                let min_params = match name.split_once('.') {
                    Some((module, func)) => crate::stdlib::min_params(module, func).unwrap_or(sig.params.len()),
                    None => self.env.fn_min_params.get(name).copied().unwrap_or(sig.params.len()),
                };
                if arg_tys.len() < min_params || arg_tys.len() > sig.params.len() {
                    self.emit(super::err(
                        format!("{}() expects {} argument(s) but got {}", name, sig.params.len(), arg_tys.len()),
                        "Check the number of arguments", format!("call to {}()", name)).with_code("E004"));
                }
                // Validate argument types and infer generics
                let mut bindings = HashMap::new();
                if let Some(ta) = type_args {
                    for (gname, gty) in sig.generics.iter().zip(ta.iter()) {
                        bindings.insert(gname.clone(), gty.clone());
                    }
                }
                let concrete_args: Vec<Ty> = arg_tys.iter().map(|a| resolve_ty(a, &self.uf)).collect();
                for ((pname, pty), aty) in sig.params.iter().zip(concrete_args.iter()) {
                    self.unify_call_arg(name, pname, pty, aty, &sig.structural_bounds, &mut bindings);
                }
                // Verify protocol bounds on generic type parameters
                for (tv_name, proto_names) in &sig.protocol_bounds {
                    if let Some(concrete_ty) = bindings.get(tv_name) {
                        let type_name = self.resolve_type_name_for_protocol(concrete_ty);
                        if let Some(type_name) = type_name {
                            for proto in proto_names {
                                let has_proto = self.env.type_protocols
                                    .get(&type_name)
                                    .map_or(false, |ps| ps.contains(proto));
                                if !has_proto {
                                    self.emit(super::err(
                                        format!("type '{}' does not implement protocol '{}'", type_name, proto),
                                        format!("Add `: {}` to the type declaration: type {}: {} = ...", proto, type_name, proto),
                                        format!("call to {}()", name)));
                                }
                            }
                        }
                    }
                }
                // Propagate resolved types back to inference variables
                for ((_, pty), aty) in sig.params.iter().zip(arg_tys.iter()) {
                    let expected = if bindings.is_empty() { pty.clone() } else { crate::types::substitute(pty, &bindings) };
                    if expected != Ty::Unknown {
                        self.constrain(expected, aty.clone(), format!("call to {}()", name));
                    }
                }
                // Instantiate unresolved generics with fresh vars
                let mut final_bindings = bindings.clone();
                for g in &sig.generics {
                    if !final_bindings.contains_key(g) {
                        final_bindings.insert(g.clone(), self.fresh_var());
                    }
                }
                let ret = if final_bindings.is_empty() { sig.ret.clone() } else { crate::types::substitute(&sig.ret, &final_bindings) };
                ret
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
                self.fresh_var()
            }).collect()
        } else {
            vec![]
        }
    }

    fn check_constructor_args(&mut self, name: &str, case: &crate::types::VariantCase, arg_tys: &[Ty]) {
        if let crate::types::VariantPayload::Tuple(expected_tys) = &case.payload {
            if arg_tys.len() != expected_tys.len() {
                self.emit(super::err(
                    format!("{}() expects {} argument(s) but got {}", name, expected_tys.len(), arg_tys.len()),
                    "Check the number of arguments", format!("constructor {}()", name)));
            }
            for (i, (aty, ety)) in arg_tys.iter().zip(expected_tys.iter()).enumerate() {
                let concrete_arg = resolve_ty(aty, &self.uf);
                if concrete_arg != Ty::Unknown && !ety.compatible(&concrete_arg) {
                    self.emit(super::err(
                        format!("{}() argument {} expects {} but got {}", name, i + 1, ety.display(), concrete_arg.display()),
                        "Fix the argument type", format!("constructor {}()", name)).with_code("E005"));
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
                    self.emit(super::err(
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
                self.emit(super::err(
                    format!("argument '{}' expects {} but got {}", param_name, expected.display(), arg_ty.display()),
                    Self::hint_with_conversion("Fix the argument type", &expected, arg_ty),
                    format!("call to {}()", fn_name)).with_code("E005"));
            }
        }
    }

    /// Resolve a member call statically (module.func, alias, TypeName.method, codec).
    /// Returns Some(Ty) if resolved, None to fall through to UFCS/convention dispatch.
    fn resolve_static_member(&mut self, object: &ast::Expr, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
        let module_name = match object {
            ast::Expr::Ident { name, .. } => Some(name.as_str()),
            _ => None,
        };

        if let Some(module) = module_name {
            // fan.map / fan.race — compiler-known concurrency primitives
            if module == "fan" {
                if !self.env.can_call_effect {
                    self.emit(super::err(
                        format!("fan.{}() can only be used inside an effect fn", field),
                        "Mark the enclosing function as `effect fn`",
                        format!("call to fan.{}()", field)));
                }
                match field {
                    "map" => {
                        // fan.map(xs, f) -> List[B] where xs: List[A], f: Fn(A) -> B
                        if arg_tys.len() != 2 {
                            self.emit(super::err(
                                format!("fan.map() expects 2 arguments but got {}", arg_tys.len()),
                                "Usage: fan.map(list, fn(item) => result)",
                                "call to fan.map()".to_string()));
                            return Some(Ty::Unknown);
                        }
                        let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                        let elem_ty = match &list_ty {
                            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                            _ => Ty::Unknown,
                        };
                        // Infer return type from f's return type
                        let fn_ty = resolve_ty(&arg_tys[1], &self.uf);
                        let result_elem = unwrap_fn_return(&fn_ty).unwrap_or_else(|| {
                            let ret_var = self.fresh_var();
                            self.constrain(arg_tys[1].clone(),
                                Ty::Fn { params: vec![elem_ty], ret: Box::new(ret_var.clone()) },
                                "fan.map callback");
                            resolve_ty(&ret_var, &self.uf)
                        });
                        return Some(Ty::list(result_elem));
                    }
                    "race" => {
                        // fan.race(thunks) -> T where thunks: List[Fn() -> T]
                        if arg_tys.len() != 1 {
                            self.emit(super::err(
                                format!("fan.race() expects 1 argument but got {}", arg_tys.len()),
                                "Usage: fan.race([fn() => a, fn() => b])",
                                "call to fan.race()".to_string()));
                            return Some(Ty::Unknown);
                        }
                        let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                        return Some(unwrap_list_fn_return(&list_ty));
                    }
                    "any" => {
                        // fan.any(thunks) -> T — first success, all fail = error
                        if arg_tys.len() != 1 {
                            self.emit(super::err(
                                format!("fan.any() expects 1 argument but got {}", arg_tys.len()),
                                "Usage: fan.any([() => a, () => b])",
                                "call to fan.any()".to_string()));
                            return Some(Ty::Unknown);
                        }
                        let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                        return Some(unwrap_list_fn_return(&list_ty));
                    }
                    "settle" => {
                        // fan.settle(thunks) -> List[Result[T, String]]
                        if arg_tys.len() != 1 {
                            self.emit(super::err(
                                format!("fan.settle() expects 1 argument but got {}", arg_tys.len()),
                                "Usage: fan.settle([() => a, () => b])",
                                "call to fan.settle()".to_string()));
                            return Some(Ty::Unknown);
                        }
                        let list_ty = resolve_ty(&arg_tys[0], &self.uf);
                        let inner_result = unwrap_list_fn_result_ty(&list_ty);
                        return Some(Ty::list(inner_result));
                    }
                    "timeout" => {
                        // fan.timeout(ms, thunk) -> T
                        if arg_tys.len() != 2 {
                            self.emit(super::err(
                                format!("fan.timeout() expects 2 arguments but got {}", arg_tys.len()),
                                "Usage: fan.timeout(5000, () => expr)",
                                "call to fan.timeout()".to_string()));
                            return Some(Ty::Unknown);
                        }
                        self.constrain(Ty::Int, arg_tys[0].clone(), "fan.timeout ms");
                        let fn_ty = resolve_ty(&arg_tys[1], &self.uf);
                        let result_ty = match &fn_ty {
                            Ty::Fn { ret, .. } => match ret.as_ref() {
                                Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                                other => other.clone(),
                            },
                            _ => Ty::Unknown,
                        };
                        return Some(Ty::result(result_ty, Ty::String));
                    }
                    _ => {
                        self.emit(super::err(
                            format!("unknown function 'fan.{}'", field),
                            "Available: fan.map, fan.race, fan.any, fan.settle, fan.timeout",
                            format!("call to fan.{}()", field)));
                        return Some(Ty::Unknown);
                    }
                }
            }

            // Codec convenience: json.encode(t) → String when t has T.encode
            if field == "encode" && arg_tys.len() == 1 {
                let arg_concrete = resolve_ty(&arg_tys[0], &self.uf);
                if self.has_codec_encode(&arg_concrete) {
                    return Some(Ty::String);
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

    /// Substitute Ty::TypeVar("Self") with a concrete type in a protocol method return type.
    fn substitute_self_in_ty(&self, ty: &Ty, replacement: &Ty) -> Ty {
        match ty {
            Ty::TypeVar(name) if name == "Self" => replacement.clone(),
            _ => ty.map_children(&|child| self.substitute_self_in_ty(child, replacement)),
        }
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
