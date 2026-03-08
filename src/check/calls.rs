use crate::ast;
use crate::types::{Ty, VariantPayload};
use crate::stdlib;
use super::{Checker, err};

impl Checker {
    pub(crate) fn check_call(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr]) -> Ty {
        let arg_tys: Vec<Ty> = args.iter_mut().map(|a| self.check_expr(a)).collect();

        if let ast::Expr::Member { object, field, .. } = callee {
            if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                return self.check_module_call(module, field, &arg_tys);
            }
        }

        if let ast::Expr::Ident { name, .. } = callee {
            return self.check_direct_call(name, &arg_tys);
        }

        if let ast::Expr::TypeName { name, .. } = callee {
            return self.check_constructor_call(name, &arg_tys);
        }

        let ct = self.check_expr(callee);
        match &ct {
            Ty::Fn { ret, .. } => *ret.clone(),
            _ => Ty::Unknown,
        }
    }

    fn check_direct_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        match name {
            "println" | "eprintln" => {
                if arg_tys.len() != 1 {
                    self.push_diagnostic(err(
                        format!("{}() takes exactly 1 argument but got {}", name, arg_tys.len()),
                        format!("Use {}(\"message\")", name),
                        format!("{}()", name),
                    ));
                } else if !arg_tys[0].compatible(&Ty::String) {
                    self.push_diagnostic(err(
                        format!("{}() requires String but got {}", name, arg_tys[0].display()),
                        "Use int.to_string(n) to convert to String first",
                        format!("{}()", name),
                    ));
                }
                return Ty::Unit;
            }
            "assert" => {
                if arg_tys.len() == 1 && !arg_tys[0].compatible(&Ty::Bool) {
                    self.push_diagnostic(err(
                        format!("assert() requires Bool but got {}", arg_tys[0].display()),
                        "Pass a boolean condition to assert()", "assert()",
                    ));
                }
                return Ty::Unit;
            }
            "assert_eq" | "assert_ne" => return Ty::Unit,
            "ok" => return Ty::Result(Box::new(arg_tys.first().cloned().unwrap_or(Ty::Unit)), Box::new(Ty::Unknown)),
            "err" => return Ty::Result(Box::new(Ty::Unknown), Box::new(arg_tys.first().cloned().unwrap_or(Ty::Unknown))),
            "some" => return Ty::Option(Box::new(arg_tys.first().cloned().unwrap_or(Ty::Unknown))),
            _ => {}
        }

        if let Some(sig) = self.env.functions.get(name).cloned() {
            if sig.is_effect && !self.env.in_effect {
                self.push_diagnostic(err(
                    format!("cannot call effect function '{}' from a pure function", name),
                    "Mark the calling function as 'effect fn' to allow side effects",
                    format!("call to {}()", name),
                ));
            }
            if arg_tys.len() != sig.params.len() {
                let expected = sig.format_params();
                self.push_diagnostic(err(
                    format!("function '{}' expects {} argument(s) but got {}", name, sig.params.len(), arg_tys.len()),
                    format!("Expected: {}({})", name, expected),
                    format!("call to {}()", name),
                ));
            } else {
                for (i, ((pname, pty), aty)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                    if !pty.compatible(aty) {
                        self.push_diagnostic(err(
                            format!("argument '{}' (position {}) expects {} but got {}", pname, i + 1, pty.display(), aty.display()),
                            format!("Pass a value of type {}", pty.display()),
                            format!("call to {}()", name),
                        ));
                    }
                }
            }
            let ret = sig.ret.clone();
            if self.env.in_effect && !self.env.in_test && !self.env.skip_auto_unwrap {
                if let Ty::Result(ok_ty, _) = &ret {
                    return *ok_ty.clone();
                }
            }
            return ret;
        }

        if self.env.constructors.contains_key(name) {
            return self.check_constructor_call(name, arg_tys);
        }

        Ty::Unknown
    }

    fn check_constructor_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
            match &case.payload {
                VariantPayload::Unit => {
                    if !arg_tys.is_empty() {
                        self.push_diagnostic(err(
                            format!("constructor '{}' takes no arguments but got {}", name, arg_tys.len()),
                            format!("Use {} without parentheses", name),
                            format!("constructor {}", name),
                        ));
                    }
                }
                VariantPayload::Tuple(expected) => {
                    if arg_tys.len() != expected.len() {
                        let exp = expected.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ");
                        self.push_diagnostic(err(
                            format!("constructor '{}' expects {} argument(s) but got {}", name, expected.len(), arg_tys.len()),
                            format!("{}({})", name, exp),
                            format!("constructor {}", name),
                        ));
                    }
                }
                VariantPayload::Record(_) => {}
            }
            return Ty::Named(type_name);
        }
        Ty::Unknown
    }

    fn check_module_call(&mut self, module: &str, func: &str, arg_tys: &[Ty]) -> Ty {
        // Resolve module name through aliases (e.g. "json" -> "json_v2")
        let resolved_module = self.env.module_aliases.get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string());

        // Track module usage for unused import detection (use original name)
        if stdlib::is_stdlib_module(module) || self.env.user_modules.contains(&resolved_module) {
            self.env.used_modules.insert(module.to_string());
        }
        if let Some(sig) = stdlib::lookup_sig(module, func) {
            let min_params = stdlib::min_params(module, func).unwrap_or(sig.params.len());
            if arg_tys.len() < min_params || arg_tys.len() > sig.params.len() {
                let usage = sig.format_params();
                self.push_diagnostic(err(
                    format!("{}.{}() expects {} argument(s) but got {}", module, func, sig.params.len(), arg_tys.len()),
                    format!("Usage: {}.{}({})", module, func, usage),
                    format!("{}.{}()", module, func),
                ));
            } else {
                for (i, ((pname, pty), aty)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                    if !pty.compatible(aty) {
                        self.push_diagnostic(err(
                            format!("{}.{}() argument '{}' (position {}) expects {} but got {}", module, func, pname, i + 1, pty.display(), aty.display()),
                            format!("Pass a value of type {}", pty.display()),
                            format!("{}.{}()", module, func),
                        ));
                    }
                }
            }
            if sig.is_effect && !self.env.in_effect {
                self.push_diagnostic(err(
                    format!("{}.{}() is an effect function and cannot be called from a pure function", module, func),
                    "Mark the calling function as 'effect fn'",
                    format!("{}.{}()", module, func),
                ));
            }
            let ret = sig.ret.clone();
            // Stdlib calls always have hardcoded `?` in codegen, so always unwrap Result
            if self.env.in_effect {
                if let Ty::Result(ok_ty, _) = &ret {
                    return *ok_ty.clone();
                }
            }
            return ret;
        }

        // Check user-defined modules
        if self.env.user_modules.contains(&resolved_module) {
            let key = format!("{}.{}", resolved_module, func);
            if let Some(sig) = self.env.functions.get(&key).cloned() {
                if arg_tys.len() != sig.params.len() {
                    let usage = sig.format_params();
                    self.push_diagnostic(err(
                        format!("{}.{}() expects {} argument(s) but got {}", module, func, sig.params.len(), arg_tys.len()),
                        format!("Usage: {}.{}({})", module, func, usage),
                        format!("{}.{}()", module, func),
                    ));
                } else {
                    for (i, ((pname, pty), aty)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if !pty.compatible(aty) {
                            self.push_diagnostic(err(
                                format!("{}.{}() argument '{}' (position {}) expects {} but got {}", module, func, pname, i + 1, pty.display(), aty.display()),
                                format!("Pass a value of type {}", pty.display()),
                                format!("{}.{}()", module, func),
                            ));
                        }
                    }
                }
                if sig.is_effect && !self.env.in_effect {
                    self.push_diagnostic(err(
                        format!("{}.{}() is an effect function and cannot be called from a pure function", module, func),
                        "Mark the calling function as 'effect fn'",
                        format!("{}.{}()", module, func),
                    ));
                }
                let ret = sig.ret.clone();
                // In effect context, auto-unwrap Result (same as stdlib)
                if self.env.in_effect {
                    if let Ty::Result(ok_ty, _) = &ret {
                        return *ok_ty.clone();
                    }
                }
                return ret;
            }
        }

        Ty::Unknown
    }

    pub(crate) fn check_member_access(&mut self, obj_ty: &Ty, field: &str) -> Ty {
        let resolved = self.env.resolve_named(obj_ty);
        match &resolved {
            Ty::Record { fields } => {
                for (name, ty) in fields {
                    if name == field { return ty.clone(); }
                }
                let avail = fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
                self.push_diagnostic(err(
                    format!("record has no field '{}'", field),
                    format!("Available fields: {}", avail),
                    format!("field access .{}", field),
                ));
                Ty::Unknown
            }
            Ty::Unknown => Ty::Unknown,
            _ => Ty::Unknown,
        }
    }
}
