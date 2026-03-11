use crate::ast::*;
use crate::stdlib;
use super::Emitter;

impl Emitter {
    pub(crate) fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
        stdlib::resolve_ufcs_module(method)
    }

    /// Extract lambda parameter names and body code from a lambda expression or function reference.
    pub(crate) fn inline_lambda(&self, lambda_arg: &Expr, arity: usize) -> (Vec<String>, String) {
        if let Expr::Lambda { params, body, .. } = lambda_arg {
            let names: Vec<String> = params.iter().map(|p| {
                if let Some(tuple_names) = &p.tuple_names {
                    format!("({})", tuple_names.join(", "))
                } else {
                    p.name.clone()
                }
            }).collect();
            let body_str = self.gen_expr(body);
            (names, body_str)
        } else {
            let f = self.gen_expr(lambda_arg);
            if arity == 1 {
                (vec!["__x".to_string()], format!("({})((__x).clone())", f))
            } else {
                (vec!["__a".to_string(), "__b".to_string()], format!("({})(__a, __b.clone())", f))
            }
        }
    }

    /// Flatten nested Member expressions into (segments, func).
    fn flatten_member_chain<'a>(expr: &'a Expr) -> Option<(Vec<&'a str>, &'a str)> {
        if let Expr::Member { object, field, .. } = expr {
            let mut segments = Vec::new();
            let mut current = object.as_ref();
            loop {
                match current {
                    Expr::Ident { name, .. } => {
                        segments.push(name.as_str());
                        break;
                    }
                    Expr::Member { object, field: seg, .. } => {
                        segments.push(seg.as_str());
                        current = object.as_ref();
                    }
                    _ => return None,
                }
            }
            segments.reverse();
            Some((segments, field.as_str()))
        } else {
            None
        }
    }

    pub(crate) fn gen_call(&self, callee: &Expr, args: &[Expr], type_args: Option<&Vec<crate::ast::TypeExpr>>) -> String {
        // Handle module calls — any depth of nesting
        if let Some((segments, func)) = Self::flatten_member_chain(callee) {
            // Resolve alias on the first segment
            let first = self.module_aliases.get(segments[0])
                .map(|s| s.as_str())
                .unwrap_or(segments[0]);

            // Try progressively longer module paths
            for i in (1..=segments.len()).rev() {
                let dotted = if i == 1 {
                    first.to_string()
                } else {
                    let rest: Vec<&str> = segments[1..i].to_vec();
                    format!("{}.{}", first, rest.join("."))
                };
                let is_user = self.user_modules.contains(&dotted);
                let is_stdlib = !is_user && stdlib::is_stdlib_module(&dotted);
                if is_user || is_stdlib {
                    return self.gen_module_call(&dotted, func, args, type_args);
                }
            }

            // Single segment — direct module call
            if segments.len() == 1 {
                let resolved_mod = self.module_aliases.get(segments[0])
                    .cloned()
                    .unwrap_or_else(|| segments[0].to_string());
                let is_user = self.user_modules.contains(&resolved_mod);
                let is_stdlib = !is_user && stdlib::is_stdlib_module(segments[0]);
                if is_user || is_stdlib {
                    return self.gen_module_call(segments[0], func, args, type_args);
                }
            }
        }

        if let Expr::Member { object, field, .. } = callee {
            // UFCS: receiver.method(args) => module.method(receiver, args)
            // Try type-based resolution first (correct for ambiguous methods like len, contains)
            let resolved = object.resolved_type()
                .and_then(|rt| crate::stdlib::resolve_ufcs_by_type(field, rt))
                .or_else(|| Self::resolve_ufcs_module(field));
            if let Some(resolved) = resolved {
                let mut new_args = vec![object.as_ref().clone()];
                new_args.extend(args.iter().cloned());
                return self.gen_module_call(resolved, field, &new_args, None);
            }
            // Fallback: user-defined UFCS — receiver.f(args) => f(receiver, args)
            let receiver = self.gen_expr(object);
            let rest: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
            let mut all_args = vec![receiver];
            all_args.extend(rest);
            return format!("{}({})", crate::emit_common::sanitize(field), all_args.join(", "));
        }

        // Handle built-in functions
        if let Expr::Ident { name, .. } = callee {
            match name.as_str() {
                "println" => {
                    let arg = self.gen_expr(&args[0]);
                    return format!("println!(\"{{}}\", {})", arg);
                }
                "eprintln" => {
                    let arg = self.gen_expr(&args[0]);
                    return format!("eprintln!(\"{{}}\", {})", arg);
                }
                "err" => {
                    let msg = self.gen_expr(&args[0]);
                    return format!("return Err(({}).to_string())", msg);
                }
                "assert_eq" => {
                    let a = self.gen_expr(&args[0]);
                    let b = self.gen_expr(&args[1]);
                    let msg = if args.len() >= 3 { Some(self.gen_expr(&args[2])) } else { None };
                    // If one side is an empty list, use .is_empty() check instead
                    // Use let-binding to avoid evaluating the expression twice
                    if matches!(&args[1], Expr::List { elements, .. } if elements.is_empty()) {
                        return format!("{{ let __v = {}; assert!(__v.is_empty(), \"expected empty list but got {{:?}}\", __v); }}", a);
                    }
                    if matches!(&args[0], Expr::List { elements, .. } if elements.is_empty()) {
                        return format!("{{ let __v = {}; assert!(__v.is_empty(), \"expected empty list but got {{:?}}\", __v); }}", b);
                    }
                    if let Some(m) = msg {
                        return format!("assert_eq!({}, {}, \"{{}}\", {})", a, b, m);
                    }
                    return format!("assert_eq!({}, {})", a, b);
                }
                "assert_ne" => {
                    let a = self.gen_expr(&args[0]);
                    let b = self.gen_expr(&args[1]);
                    let msg = if args.len() >= 3 { Some(self.gen_expr(&args[2])) } else { None };
                    if let Some(m) = msg {
                        return format!("assert_ne!({}, {}, \"{{}}\", {})", a, b, m);
                    }
                    return format!("assert_ne!({}, {})", a, b);
                }
                "assert" => {
                    let a = self.gen_expr(&args[0]);
                    let msg = if args.len() >= 2 { Some(self.gen_expr(&args[1])) } else { None };
                    if let Some(m) = msg {
                        return format!("assert!({}, \"{{}}\", {})", a, m);
                    }
                    return format!("assert!({})", a);
                }
                "unwrap_or" => {
                    let a = self.gen_expr(&args[0]);
                    let b = self.gen_expr(&args[1]);
                    return format!("({}).unwrap_or({})", a, b);
                }
                _ => {}
            }
        }

        // For generic variant unit constructors used as Call callee, get raw name
        // (gen_expr would add "()" which breaks turbofish syntax)
        let callee_str = match callee {
            Expr::TypeName { name, .. } if self.generic_variant_unit_ctors.contains(name) => name.clone(),
            Expr::Ident { name, .. } if self.generic_variant_unit_ctors.contains(name) => crate::emit_common::sanitize(name),
            _ => self.gen_expr(callee),
        };
        let turbofish = match type_args {
            Some(ta) if !ta.is_empty() => {
                let types: Vec<String> = ta.iter().map(|t| self.gen_type(t)).collect();
                format!("::<{}>", types.join(", "))
            }
            _ => String::new(),
        };
        // Use borrow-aware arg generation for known function names
        // When inside a module, qualify with module name for borrow lookup
        let callee_fn_name: Option<String> = match callee {
            Expr::Ident { name, .. } => {
                if let Some(ref mod_name) = self.current_module {
                    let qualified = format!("{}.{}", mod_name, name);
                    if self.borrow_info.fn_params.contains_key(&qualified) {
                        Some(qualified)
                    } else {
                        Some(name.clone())
                    }
                } else {
                    Some(name.clone())
                }
            }
            _ => None,
        };
        // Check if callee is a variant constructor that needs Box wrapping
        let variant_ctor_name: Option<&str> = match callee {
            Expr::TypeName { name, .. } => Some(name.as_str()),
            _ => None,
        };
        let args_str: Vec<String> = args.iter().enumerate().map(|(i, a)| {
            let arg_str = if let Some(ref fn_name) = callee_fn_name {
                self.gen_arg_for(a, fn_name, i)
            } else {
                self.gen_arg(a)
            };
            // Auto-wrap with Box::new for recursive variant constructor args
            // Skip for generic variants — their wrapper functions already do Box::new
            if let Some(ctor) = variant_ctor_name {
                if self.boxed_variant_args.contains(&(ctor.to_string(), i))
                    && !self.generic_variant_constructors.contains_key(ctor)
                {
                    return format!("Box::new({})", arg_str);
                }
            }
            arg_str
        }).collect();
        let call = format!("{}{}({})", callee_str, turbofish, args_str.join(", "));
        // Auto-propagate ? for effect fn calls within effect context (not in tests, not suppressed)
        if self.in_effect && !self.in_test && !self.skip_auto_q.get() {
            if let Expr::Ident { name, .. } = callee {
                if self.effect_fns.contains(name) {
                    return format!("{}?", call);
                }
            }
        }
        // In do blocks, auto-unwrap calls to Result-returning functions
        if self.in_do_block.get() {
            if let Expr::Ident { name, .. } = callee {
                if self.result_fns.contains(name) || self.effect_fns.contains(name) {
                    return format!("{}?", call);
                }
            }
        }
        call
    }

    pub(crate) fn gen_module_call(&self, module: &str, func: &str, args: &[Expr], type_args: Option<&Vec<crate::ast::TypeExpr>>) -> String {
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();

        // Build turbofish string from call-site type arguments
        let turbofish = match type_args {
            Some(ta) if !ta.is_empty() => {
                let types: Vec<String> = ta.iter().map(|t| self.gen_type(t)).collect();
                format!("::<{}>", types.join(", "))
            }
            _ => String::new(),
        };

        // User modules take priority over stdlib (e.g. user's "math" module shadows stdlib "math")
        let resolved_mod = self.module_aliases.get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string());
        if self.user_modules.contains(&resolved_mod) {
            // Use borrow-aware arg generation for user module calls
            let qualified = format!("{}.{}", resolved_mod, func);
            let user_args_str: Vec<String> = args.iter().enumerate()
                .map(|(i, a)| self.gen_arg_for(a, &qualified, i))
                .collect();
            let rust_mod = resolved_mod.replace('.', "_");
            let safe_func = crate::emit_common::sanitize(func);
            let call = format!("{}::{}{}({})", rust_mod, safe_func, turbofish, user_args_str.join(", "));
            if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
                return format!("{}?", call);
            }
            return call;
        }
        // Try auto-generated codegen first
        let in_effect = self.in_effect;
        let inline_lambda_fn = |idx: usize, arity: usize| -> (Vec<String>, String) {
            self.inline_lambda(&args[idx], arity)
        };
        if let Some(mut expr) = almide::generated::emit_rust_calls::gen_generated_call(module, func, &args_str, in_effect, &inline_lambda_fn) {
            // Insert turbofish into generated call expression (before the first '(')
            if !turbofish.is_empty() {
                if let Some(paren_pos) = expr.find('(') {
                    expr.insert_str(paren_pos, &turbofish);
                }
            }
            return expr;
        }
        // User-defined module call (all stdlib handled by generated code above)
        let resolved = self.module_aliases.get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string());
        let rust_mod = resolved.replace('.', "_");
        let call = format!("{}::{}{}({})", rust_mod, func, turbofish, args_str.join(", "));
        if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
            format!("{}?", call)
        } else {
            call
        }
    }

}
