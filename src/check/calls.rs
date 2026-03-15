/// Call type checking — resolves function calls, builtins, variant constructors.

use std::collections::HashMap;
use crate::ast;
use crate::types::Ty;
use super::types::InferTy;
use super::Checker;

impl Checker {
    pub(crate) fn check_call(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr]) -> InferTy {
        let arg_tys: Vec<InferTy> = args.iter_mut().map(|a| self.infer_expr(a)).collect();
        match callee {
            ast::Expr::Ident { name, .. } => self.check_named_call(name, &arg_tys),
            ast::Expr::TypeName { name, .. } => {
                if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
                    // Validate constructor argument types
                    self.check_constructor_args(name, &case, &arg_tys);
                    InferTy::Concrete(Ty::Named(type_name, vec![]))
                } else { InferTy::Concrete(Ty::Named(name.clone(), vec![])) }
            }
            // Module call: string.trim(s), list.map(xs, f), etc.
            ast::Expr::Member { object, field, .. } => {
                if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                    // Codec convenience: module.encode(t) → String when t has T.encode
                    if field == "encode" && arg_tys.len() == 1 {
                        let arg_concrete = arg_tys[0].to_ty(&self.solutions);
                        let has_codec = match &arg_concrete {
                            Ty::Named(name, _) => self.env.functions.contains_key(&format!("{}.encode", name)),
                            Ty::Record { .. } | Ty::Variant { .. } => {
                                self.env.types.iter().any(|(name, ty)| {
                                    ty == &arg_concrete && self.env.functions.contains_key(&format!("{}.encode", name))
                                })
                            }
                            _ => false,
                        };
                        if has_codec {
                            return InferTy::Concrete(Ty::String);
                        }
                    }
                    if crate::stdlib::is_stdlib_module(module) || self.env.user_modules.contains(module.as_str()) {
                        let key = format!("{}.{}", module, field);
                        return self.check_named_call(&key, &arg_tys);
                    }
                    // Check module aliases
                    if let Some(target) = self.env.module_aliases.get(module.as_str()).cloned() {
                        let key = format!("{}.{}", target, field);
                        return self.check_named_call(&key, &arg_tys);
                    }
                }
                // TypeName.method(args) → direct convention call (not UFCS)
                // e.g., Person.decode(v) → Person.decode(v), NOT Person.decode(Person, v)
                if let ast::Expr::TypeName { name: type_name, .. } = object.as_ref() {
                    let key = format!("{}.{}", type_name, field);
                    if self.env.functions.contains_key(&key) {
                        return self.check_named_call(&key, &arg_tys);
                    }
                }
                // UFCS method: obj.method(args) → module.method(obj, args)
                let obj_ty = self.infer_expr(object);
                let obj_concrete = obj_ty.to_ty(&self.solutions);
                // Built-in generic types → stdlib module UFCS
                let builtin_module = match &obj_concrete {
                    Ty::List(_) => Some("list"),
                    Ty::Map(_, _) => Some("map"),
                    Ty::String => Some("string"),
                    Ty::Int => Some("int"),
                    Ty::Float => Some("float"),
                    Ty::Result(_, _) => Some("result"),
                    Ty::Option(_) => Some("option"),
                    _ => None,
                };
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
        // Builtins that accept any types
        match name {
            "println" | "eprintln" => {
                // println/eprintln require String argument
                if let Some(first) = arg_tys.first() {
                    self.constrain(InferTy::Concrete(Ty::String), first.clone(), format!("call to {}()", name));
                }
                return InferTy::Concrete(Ty::Unit);
            }
            "assert" | "assert_eq" | "assert_ne" => return InferTy::Concrete(Ty::Unit),
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
                let sig = if let Some(sig) = self.env.functions.get(name).cloned() {
                    Some(sig)
                } else if name.contains('.') {
                    let parts: Vec<&str> = name.splitn(2, '.').collect();
                    if parts.len() == 2 { crate::stdlib::lookup_sig(parts[0], parts[1]) } else { None }
                } else {
                    None
                };

                if let Some(sig) = sig {
                    // Validate argument count
                    let min_params = if name.contains('.') {
                        let parts: Vec<&str> = name.splitn(2, '.').collect();
                        crate::stdlib::min_params(parts[0], parts[1]).unwrap_or(sig.params.len())
                    } else {
                        self.env.fn_min_params.get(name).copied().unwrap_or(sig.params.len())
                    };
                    if arg_tys.len() < min_params || arg_tys.len() > sig.params.len() {
                        self.diagnostics.push(super::err(
                            format!("{}() expects {} argument(s) but got {}", name, sig.params.len(), arg_tys.len()),
                            "Check the number of arguments", format!("call to {}()", name)));
                    }
                    // Validate argument types and infer generics
                    let mut bindings = HashMap::new();
                    let concrete_args: Vec<Ty> = arg_tys.iter().map(|a| a.to_ty(&self.solutions)).collect();
                    for ((pname, pty), aty) in sig.params.iter().zip(concrete_args.iter()) {
                        if let Ty::TypeVar(tv) = pty {
                            if let Some(bound) = sig.structural_bounds.get(tv) {
                                let resolved = self.env.resolve_named(aty);
                                if bound.compatible(&resolved) || bound.compatible(aty) {
                                    bindings.insert(tv.clone(), aty.clone());
                                } else {
                                    self.diagnostics.push(super::err(
                                        format!("argument '{}' does not satisfy bound {}: got {}", pname, bound.display(), aty.display()),
                                        "The argument must have the required fields".to_string(),
                                        format!("call to {}()", name)));
                                }
                            } else { crate::types::unify(pty, aty, &mut bindings); }
                        } else {
                            crate::types::unify(pty, aty, &mut bindings);
                            // Generate constraint for argument type checking
                            let expected_ty = if bindings.is_empty() { pty.clone() } else { crate::types::substitute(pty, &bindings) };
                            let expected_resolved = self.env.resolve_named(&expected_ty);
                            let aty_resolved = self.env.resolve_named(aty);
                            if expected_ty != Ty::Unknown && *aty != Ty::Unknown && !expected_resolved.compatible(&aty_resolved) && !expected_ty.compatible(aty) {
                                let hint = Self::hint_with_conversion(
                                    "Fix the argument type",
                                    &expected_ty, aty,
                                );
                                self.diagnostics.push(super::err(
                                    format!("argument '{}' expects {} but got {}", pname, expected_ty.display(), aty.display()),
                                    hint, format!("call to {}()", name)));
                            }
                        }
                    }
                    // Propagate resolved types back to inference variables (fixes lambda param inference)
                    for ((_, pty), aty) in sig.params.iter().zip(arg_tys.iter()) {
                        let expected = if bindings.is_empty() { pty.clone() } else { crate::types::substitute(pty, &bindings) };
                        if expected != Ty::Unknown {
                            self.constrain(InferTy::from_ty(&expected), aty.clone(), format!("call to {}()", name));
                        }
                    }
                    let ret = if bindings.is_empty() { sig.ret.clone() } else { crate::types::substitute(&sig.ret, &bindings) };
                    InferTy::from_ty(&ret)
                } else if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
                    self.check_constructor_args(name, &case, arg_tys);
                    InferTy::Concrete(Ty::Named(type_name, vec![]))
                } else if let Some(ty) = self.env.lookup_var(name).cloned() {
                    match &ty { Ty::Fn { ret, .. } => InferTy::from_ty(ret), _ => InferTy::from_ty(&ty) }
                } else {
                    self.diagnostics.push(super::err(format!("undefined function '{}'", name), "Check the function name", format!("call to {}()", name)));
                    InferTy::Concrete(Ty::Unknown)
                }
            }
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
}
