/// Static member resolution — fan.*, codec.*, module/alias dispatch, TypeName.method.

use crate::ast;
use crate::types::{Ty, TypeConstructorId};
use super::types::resolve_ty;
use super::Checker;

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

/// Extract the Result type from List[Fn() -> Result[T, E]] -> Result[T, E]
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

/// Extract the element's effective return type from List[Fn() -> Result[T, E]] -> T
fn unwrap_list_fn_return(list_ty: &Ty) -> Ty {
    match list_ty {
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            unwrap_fn_return(&args[0]).unwrap_or(Ty::Unknown)
        }
        _ => Ty::Unknown,
    }
}

impl Checker {
    /// Resolve a member call statically (module.func, alias, TypeName.method, codec).
    /// Returns Some(Ty) if resolved, None to fall through to UFCS/convention dispatch.
    pub(super) fn resolve_static_member(&mut self, object: &ast::Expr, field: &str, arg_tys: &[Ty]) -> Option<Ty> {
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

            // Codec convenience: json.encode(t) -> String when t has T.encode
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
