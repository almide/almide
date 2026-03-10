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
                    return self.gen_module_call(&dotted, func, args);
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
                    return self.gen_module_call(segments[0], func, args);
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
                return self.gen_module_call(resolved, field, &new_args);
            }
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
                    if matches!(&args[1], Expr::List { elements, .. } if elements.is_empty()) {
                        return format!("assert!(({}).is_empty(), \"expected empty list but got {{:?}}\", {})", a, a);
                    }
                    if matches!(&args[0], Expr::List { elements, .. } if elements.is_empty()) {
                        return format!("assert!(({}).is_empty(), \"expected empty list but got {{:?}}\", {})", b, b);
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
        let args_str: Vec<String> = args.iter().map(|a| self.gen_arg(a)).collect();
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

    pub(crate) fn gen_module_call(&self, module: &str, func: &str, args: &[Expr]) -> String {
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        // User modules take priority over stdlib (e.g. user's "math" module shadows stdlib "math")
        let resolved_mod = self.module_aliases.get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string());
        if self.user_modules.contains(&resolved_mod) {
            // Use gen_arg (clone Idents) for user module calls to avoid move errors
            let user_args_str: Vec<String> = args.iter().map(|a| self.gen_arg(a)).collect();
            let rust_mod = resolved_mod.replace('.', "_");
            let safe_func = crate::emit_common::sanitize(func);
            let call = format!("{}::{}({})", rust_mod, safe_func, user_args_str.join(", "));
            if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
                return format!("{}?", call);
            }
            return call;
        }
        match module {
            "fs" => match func {
                "read_text" => format!("almide_rt_fs_read_text(&*{})?", args_str[0]),
                "write" => format!("almide_rt_fs_write(&*{}, &*{})?", args_str[0], args_str[1]),
                "write_bytes" => format!("almide_rt_fs_write_bytes(&*{}, &{})?", args_str[0], args_str[1]),
                "read_bytes" => format!("almide_rt_fs_read_bytes(&*{})?", args_str[0]),
                "exists?" | "exists_hdlm_qm_" => format!("almide_rt_fs_exists(&*{})", args_str[0]),
                "mkdir_p" => format!("almide_rt_fs_mkdir_p(&*{})?", args_str[0]),
                "append" => format!("almide_rt_fs_append(&*{}, &*{})?", args_str[0], args_str[1]),
                "read_lines" => format!("almide_rt_fs_read_lines(&*{})?", args_str[0]),
                "remove" => format!("almide_rt_fs_remove(&*{})?", args_str[0]),
                "list_dir" => format!("almide_rt_fs_list_dir(&*{})?", args_str[0]),
                "is_dir?" | "is_dir_hdlm_qm_" => format!("almide_rt_fs_is_dir(&*{})", args_str[0]),
                "is_file?" | "is_file_hdlm_qm_" => format!("almide_rt_fs_is_file(&*{})", args_str[0]),
                "copy" => format!("almide_rt_fs_copy(&*{}, &*{})?", args_str[0], args_str[1]),
                "rename" => format!("almide_rt_fs_rename(&*{}, &*{})?", args_str[0], args_str[1]),
                "walk" => format!("almide_rt_fs_walk(&*{})?", args_str[0]),
                "stat" => format!("almide_rt_fs_stat(&*{})?", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for fs.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "string" => match func {
                "trim" => format!("almide_rt_string_trim(&*{})", args_str[0]),
                "split" => format!("almide_rt_string_split(&*{}, &*{})", args_str[0], args_str[1]),
                "join" => format!("almide_rt_string_join(&{}, &*{})", args_str[0], args_str[1]),
                "len" => format!("almide_rt_string_len(&*{})", args_str[0]),
                "contains" | "contains?" | "contains_hdlm_qm_" => format!("almide_rt_string_contains(&*{}, &*{})", args_str[0], args_str[1]),
                "starts_with?" | "starts_with_hdlm_qm_" | "starts_with" => format!("almide_rt_string_starts_with(&*{}, &*{})", args_str[0], args_str[1]),
                "ends_with?" | "ends_with_hdlm_qm_" | "ends_with" => format!("almide_rt_string_ends_with(&*{}, &*{})", args_str[0], args_str[1]),
                "slice" => {
                    if args_str.len() == 3 {
                        format!("almide_rt_string_slice(&*{}, {}, Some({}))", args_str[0], args_str[1], args_str[2])
                    } else {
                        format!("almide_rt_string_slice(&*{}, {}, None)", args_str[0], args_str[1])
                    }
                }
                "pad_left" => format!("almide_rt_string_pad_left(&*{}, {})", args_str[0], args_str[1]),
                "to_bytes" => format!("almide_rt_string_to_bytes(&*{})", args_str[0]),
                "to_upper" => format!("almide_rt_string_to_upper(&*{})", args_str[0]),
                "to_lower" => format!("almide_rt_string_to_lower(&*{})", args_str[0]),
                "to_int" => format!("almide_rt_string_to_int(&*{})?", args_str[0]),
                "replace" => format!("almide_rt_string_replace(&*{}, &*{}, &*{})", args_str[0], args_str[1], args_str[2]),
                "char_at" => format!("almide_rt_string_char_at(&*{}, {})", args_str[0], args_str[1]),
                "lines" => format!("almide_rt_string_lines(&*{})", args_str[0]),
                "chars" => format!("almide_rt_string_chars(&*{})", args_str[0]),
                "index_of" => format!("almide_rt_string_index_of(&*{}, &*{})", args_str[0], args_str[1]),
                "repeat" => format!("almide_rt_string_repeat(&*{}, {})", args_str[0], args_str[1]),
                "from_bytes" => format!("almide_rt_string_from_bytes(&{{ let __bytes: Vec<i64> = {}; __bytes }})", args_str[0]),
                "is_digit?" | "is_digit_hdlm_qm_" => format!("almide_rt_string_is_digit(&*{})", args_str[0]),
                "is_alpha?" | "is_alpha_hdlm_qm_" => format!("almide_rt_string_is_alpha(&*{})", args_str[0]),
                "is_alphanumeric?" | "is_alphanumeric_hdlm_qm_" => format!("almide_rt_string_is_alphanumeric(&*{})", args_str[0]),
                "is_whitespace?" | "is_whitespace_hdlm_qm_" => format!("almide_rt_string_is_whitespace(&*{})", args_str[0]),
                "pad_right" => format!("almide_rt_string_pad_right({}, {}, &*{})", args_str[0], args_str[1], args_str[2]),
                "trim_start" => format!("almide_rt_string_trim_start(&*{})", args_str[0]),
                "trim_end" => format!("almide_rt_string_trim_end(&*{})", args_str[0]),
                "count" => format!("almide_rt_string_count(&*{}, &*{})", args_str[0], args_str[1]),
                "is_empty?" | "is_empty_hdlm_qm_" => format!("almide_rt_string_is_empty(&*{})", args_str[0]),
                "reverse" => format!("almide_rt_string_reverse(&*{})", args_str[0]),
                "strip_prefix" => format!("almide_rt_string_strip_prefix(&*{}, &*{})", args_str[0], args_str[1]),
                "strip_suffix" => format!("almide_rt_string_strip_suffix(&*{}, &*{})", args_str[0], args_str[1]),
                "replace_first" => format!("almide_rt_string_replace_first(&*{}, &*{}, &*{})", args_str[0], args_str[1], args_str[2]),
                "last_index_of" => format!("almide_rt_string_last_index_of(&*{}, &*{})", args_str[0], args_str[1]),
                "to_float" => format!("almide_rt_string_to_float(&*{})?", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for string.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "list" => {
                match func {
                    "len" => format!("almide_rt_list_len(&{})", args_str[0]),
                    "get" => format!("almide_rt_list_get(&{}, {})", args_str[0], args_str[1]),
                    "get_or" => format!("almide_rt_list_get_or(&{}, {}, {})", args_str[0], args_str[1], args_str[2]),
                    "sort" => format!("almide_rt_list_sort(&{})", args_str[0]),
                    "reverse" => format!("almide_rt_list_reverse(&{})", args_str[0]),
                    "any" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_any(&{}, |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "all" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_all(&{}, |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "contains" => format!("almide_rt_list_contains(&{}, &{})", args_str[0], args_str[1]),
                    "each" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_each(&{}, |{}| {{ {} ; }})", args_str[0], names[0], body)
                    }
                    "map" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        if self.in_effect && body.contains("?") {
                            format!("almide_rt_list_map_effect(({}).clone(), |{}| -> Result<_, String> {{ Ok({{ {} }}) }})?", args_str[0], names[0], body)
                        } else {
                            format!("almide_rt_list_map(({}).clone(), |{}| {{ {} }})", args_str[0], names[0], body)
                        }
                    }
                    "filter" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_filter(({}).clone(), |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "find" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_find(({}).clone(), |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "fold" => {
                        let (names, body) = self.inline_lambda(&args[2], 2);
                        let init = &args_str[1];
                        if self.in_effect && body.contains("?") {
                            format!("almide_rt_list_fold_effect(({}).clone(), {}, |{}, {}| -> Result<_, String> {{ Ok({{ {} }}) }})?", args_str[0], init, names[0], names[1], body)
                        } else {
                            let acc_typed = if init.starts_with("Ok(") || init.starts_with("Err(") {
                                format!("{}: Result<_, String>", names[0])
                            } else {
                                names[0].clone()
                            };
                            format!("almide_rt_list_fold(({}).clone(), {}, |{}, {}| {{ {} }})", args_str[0], init, acc_typed, names[1], body)
                        }
                    }
                    "enumerate" => format!("almide_rt_list_enumerate(({}).clone())", args_str[0]),
                    "zip" => format!("almide_rt_list_zip(({}).clone(), ({}).clone())", args_str[0], args_str[1]),
                    "flatten" => format!("almide_rt_list_flatten(({}).clone())", args_str[0]),
                    "take" => format!("almide_rt_list_take(({}).clone(), {})", args_str[0], args_str[1]),
                    "drop" => format!("almide_rt_list_drop(({}).clone(), {})", args_str[0], args_str[1]),
                    "sort_by" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_sort_by(({}).clone(), |{}| {{ {} }})", args_str[0], names[0], body)
                    }
                    "unique" => format!("almide_rt_list_unique(&{})", args_str[0]),
                    "index_of" => format!("almide_rt_list_index_of(&{}, &{})", args_str[0], args_str[1]),
                    "last" => format!("almide_rt_list_last(&{})", args_str[0]),
                    "chunk" => format!("almide_rt_list_chunk(&{}, {})", args_str[0], args_str[1]),
                    "sum" => format!("almide_rt_list_sum(&{})", args_str[0]),
                    "product" => format!("almide_rt_list_product(&{})", args_str[0]),
                    "first" => format!("almide_rt_list_first(&{})", args_str[0]),
                    "is_empty?" | "is_empty_hdlm_qm_" => format!("almide_rt_list_is_empty(&{})", args_str[0]),
                    "flat_map" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        if self.in_effect && body.contains("?") {
                            format!("almide_rt_list_flat_map_effect(({}).clone(), |{}| -> Result<Vec<_>, String> {{ Ok({{ {} }}) }})?", args_str[0], names[0], body)
                        } else {
                            format!("almide_rt_list_flat_map(({}).clone(), |{}| {{ {} }})", args_str[0], names[0], body)
                        }
                    }
                    "min" => format!("almide_rt_list_min(&{})", args_str[0]),
                    "max" => format!("almide_rt_list_max(&{})", args_str[0]),
                    "join" => format!("almide_rt_list_join(&{}, &*{})", args_str[0], args_str[1]),
                    "filter_map" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_filter_map(({}).clone(), |{}| {{ {} }})", args_str[0], names[0], body)
                    }
                    "take_while" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_take_while(({}).clone(), |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "drop_while" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_drop_while(({}).clone(), |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "count" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_count(&{}, |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "partition" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_partition(({}).clone(), |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                    }
                    "reduce" => {
                        let (names, body) = self.inline_lambda(&args[1], 2);
                        format!("almide_rt_list_reduce(({}).clone(), |{}, {}| {{ {} }})", args_str[0], names[0], names[1], body)
                    }
                    "group_by" => {
                        let (names, body) = self.inline_lambda(&args[1], 1);
                        format!("almide_rt_list_group_by(({}).clone(), |{}| {{ {} }})", args_str[0], names[0], body)
                    }
                    _ => { eprintln!("internal error: no Rust codegen for list.{}() — this is a compiler bug", func); std::process::exit(70); },
                }
            },
            "map" => match func {
                "new" => "almide_rt_map_new()".to_string(),
                "get" => format!("almide_rt_map_get(&{}, &{})", args_str[0], args_str[1]),
                "get_or" => format!("almide_rt_map_get_or(&{}, &{}, {})", args_str[0], args_str[1], args_str[2]),
                "set" => format!("almide_rt_map_set(&{}, {}, {})", args_str[0], args_str[1], args_str[2]),
                "contains" => format!("almide_rt_map_contains(&{}, &{})", args_str[0], args_str[1]),
                "remove" => format!("almide_rt_map_remove(&{}, &{})", args_str[0], args_str[1]),
                "keys" => format!("almide_rt_map_keys(&{})", args_str[0]),
                "values" => format!("almide_rt_map_values(&{})", args_str[0]),
                "len" => format!("almide_rt_map_len(&{})", args_str[0]),
                "entries" => format!("almide_rt_map_entries(&{})", args_str[0]),
                "from_list" => {
                    let (names, body) = self.inline_lambda(&args[1], 1);
                    format!("almide_rt_map_from_list(({}).clone(), |{}| {{ {} }})", args_str[0], names[0], body)
                }
                "merge" => format!("almide_rt_map_merge(&{}, &{})", args_str[0], args_str[1]),
                "is_empty?" | "is_empty_hdlm_qm_" => format!("almide_rt_map_is_empty(&{})", args_str[0]),
                "map_values" => {
                    let (names, body) = self.inline_lambda(&args[1], 1);
                    format!("almide_rt_map_map_values(&{}, |{}| {{ let {} = {}.clone(); {} }})", args_str[0], names[0], names[0], names[0], body)
                }
                "filter" => {
                    let (names, body) = self.inline_lambda(&args[1], 2);
                    format!("almide_rt_map_filter(&{}, |{}, {}| {{ let {} = {}.clone(); let {} = {}.clone(); {} }})", args_str[0], names[0], names[1], names[0], names[0], names[1], names[1], body)
                }
                "from_entries" => format!("almide_rt_map_from_entries(({}).clone())", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for map.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "int" => match func {
                "to_hex" => format!("almide_rt_int_to_hex({})", args_str[0]),
                "to_string" => format!("almide_rt_int_to_string({})", args_str[0]),
                "parse" => format!("almide_rt_int_parse(&*{})", args_str[0]),
                "parse_hex" => format!("almide_rt_int_parse_hex(&*{})", args_str[0]),
                "abs" => format!("almide_rt_int_abs({})", args_str[0]),
                "min" => format!("almide_rt_int_min({}, {})", args_str[0], args_str[1]),
                "max" => format!("almide_rt_int_max({}, {})", args_str[0], args_str[1]),
                // bitwise operations
                "band" => format!("almide_rt_int_band({}, {})", args_str[0], args_str[1]),
                "bor" => format!("almide_rt_int_bor({}, {})", args_str[0], args_str[1]),
                "bxor" => format!("almide_rt_int_bxor({}, {})", args_str[0], args_str[1]),
                "bshl" => format!("almide_rt_int_bshl({}, {})", args_str[0], args_str[1]),
                "bshr" => format!("almide_rt_int_bshr({}, {})", args_str[0], args_str[1]),
                "bnot" => format!("almide_rt_int_bnot({})", args_str[0]),
                // wrapping arithmetic
                "wrap_add" => format!("almide_rt_int_wrap_add({}, {}, {})", args_str[0], args_str[1], args_str[2]),
                "wrap_mul" => format!("almide_rt_int_wrap_mul({}, {}, {})", args_str[0], args_str[1], args_str[2]),
                "rotate_right" => format!("almide_rt_int_rotate_right({}, {}, {})", args_str[0], args_str[1], args_str[2]),
                "rotate_left" => format!("almide_rt_int_rotate_left({}, {}, {})", args_str[0], args_str[1], args_str[2]),
                "to_u32" => format!("almide_rt_int_to_u32({})", args_str[0]),
                "to_u8" => format!("almide_rt_int_to_u8({})", args_str[0]),
                "clamp" => format!("almide_rt_int_clamp({}, {}, {})", args_str[0], args_str[1], args_str[2]),
                _ => { eprintln!("internal error: no Rust codegen for int.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "float" => match func {
                "to_string" => format!("almide_rt_float_to_string({})", args_str[0]),
                "to_int" => format!("almide_rt_float_to_int({})", args_str[0]),
                "round" => format!("almide_rt_float_round({})", args_str[0]),
                "floor" => format!("almide_rt_float_floor({})", args_str[0]),
                "ceil" => format!("almide_rt_float_ceil({})", args_str[0]),
                "abs" => format!("almide_rt_float_abs({})", args_str[0]),
                "sqrt" => format!("almide_rt_float_sqrt({})", args_str[0]),
                "parse" => format!("almide_rt_float_parse(&*{})?", args_str[0]),
                "from_int" => format!("almide_rt_float_from_int({})", args_str[0]),
                "min" => format!("almide_rt_float_min({}, {})", args_str[0], args_str[1]),
                "max" => format!("almide_rt_float_max({}, {})", args_str[0], args_str[1]),
                "clamp" => format!("almide_rt_float_clamp({}, {}, {})", args_str[0], args_str[1], args_str[2]),
                _ => { eprintln!("internal error: no Rust codegen for float.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "env" => match func {
                "unix_timestamp" => "almide_rt_env_unix_timestamp()".to_string(),
                "args" => "almide_rt_env_args()".to_string(),
                "get" => format!("almide_rt_env_get(&*{})", args_str[0]),
                "set" => format!("almide_rt_env_set(&*{}, &*{})", args_str[0], args_str[1]),
                "cwd" => "almide_rt_env_cwd()?".to_string(),
                "millis" => "almide_rt_env_millis()".to_string(),
                "sleep_ms" => format!("almide_rt_env_sleep_ms({})", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for env.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "process" => match func {
                "exec" => format!("almide_rt_process_exec(&*{}, &{{ let __a: Vec<String> = {}; __a }})", args_str[0], args_str[1]),
                "exit" => format!("almide_rt_process_exit({})", args_str[0]),
                "stdin_lines" => "almide_rt_process_stdin_lines()?".to_string(),
                "exec_status" => format!("almide_rt_process_exec_status(&*{}, &{{ let __a: Vec<String> = {}; __a }})?", args_str[0], args_str[1]),
                _ => { eprintln!("internal error: no Rust codegen for process.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "io" => match func {
                "read_line" => "almide_rt_io_read_line()?".to_string(),
                "print" => format!("almide_rt_io_print(&*{})?", args_str[0]),
                "read_all" => "almide_rt_io_read_all()?".to_string(),
                _ => { eprintln!("internal error: no Rust codegen for io.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "json" => match func {
                "parse" => format!("almide_json_parse(&{})?", args_str[0]),
                "stringify" => format!("almide_json_stringify(&{})", args_str[0]),
                "get" => format!("almide_json_get(&{}, &{})", args_str[0], args_str[1]),
                "get_string" => format!("almide_json_get_string(&{}, &{})", args_str[0], args_str[1]),
                "get_int" => format!("almide_json_get_int(&{}, &{})", args_str[0], args_str[1]),
                "get_bool" => format!("almide_json_get_bool(&{}, &{})", args_str[0], args_str[1]),
                "get_array" => format!("almide_json_get_array(&{}, &{})", args_str[0], args_str[1]),
                "keys" => format!("almide_json_keys(&{})", args_str[0]),
                "to_string" => format!("almide_json_to_string(&{})", args_str[0]),
                "to_int" => format!("almide_json_to_int(&{})", args_str[0]),
                "from_string" => format!("JStr({})", args_str[0]),
                "from_int" => format!("JInt({})", args_str[0]),
                "from_bool" => format!("JBool({})", args_str[0]),
                "null" => "JNull".to_string(),
                "array" => format!("JArray({})", args_str[0]),
                "from_map" => format!("JObject({})", args_str[0]),
                "get_float" => format!("almide_json_get_float(&{}, &{})", args_str[0], args_str[1]),
                "from_float" => format!("almide_json_from_float({})", args_str[0]),
                "stringify_pretty" => format!("almide_json_stringify_pretty(&{})", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for json.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "http" => match func {
                "serve" => {
                    let (names, body) = self.inline_lambda(&args[1], 1);
                    // serve returns Result<(), String> which matches effect fn main's return type
                    format!("{{ almide_http_serve({}, |{}| -> Result<AlmideHttpResponse, String> {{ Ok({{ {} }}) }})?; Ok(()) }}", args_str[0], names[0], body)
                }
                "response" => format!("AlmideHttpResponse::new({}, {}.to_string())", args_str[0], args_str[1]),
                "json" => format!("AlmideHttpResponse::json({}, {}.to_string())", args_str[0], args_str[1]),
                "with_headers" => format!("AlmideHttpResponse::with_headers({}, {}.to_string(), {})", args_str[0], args_str[1], args_str[2]),
                "get" => format!("almide_http_get(&{})?", args_str[0]),
                "post" => format!("almide_http_post(&{}, &{})?", args_str[0], args_str[1]),
                _ => { eprintln!("internal error: no Rust codegen for http.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "math" => match func {
                "min" => format!("almide_rt_math_min({}, {})", args_str[0], args_str[1]),
                "max" => format!("almide_rt_math_max({}, {})", args_str[0], args_str[1]),
                "abs" => format!("almide_rt_math_abs({})", args_str[0]),
                "pow" => format!("almide_rt_math_pow({}, {})", args_str[0], args_str[1]),
                "pi" => "almide_rt_math_pi()".to_string(),
                "e" => "almide_rt_math_e()".to_string(),
                "sin" => format!("almide_rt_math_sin({} as f64)", args_str[0]),
                "cos" => format!("almide_rt_math_cos({} as f64)", args_str[0]),
                "tan" => format!("almide_rt_math_tan({} as f64)", args_str[0]),
                "log" => format!("almide_rt_math_log({} as f64)", args_str[0]),
                "exp" => format!("almide_rt_math_exp({} as f64)", args_str[0]),
                "sqrt" => format!("almide_rt_math_sqrt({} as f64)", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for math.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "random" => match func {
                "int" => format!("almide_rt_random_int({}, {})?", args_str[0], args_str[1]),
                "float" => "almide_rt_random_float()?".to_string(),
                "choice" => format!("almide_rt_random_choice(&{})?", args_str[0]),
                "shuffle" => format!("almide_rt_random_shuffle(({}).clone())?", args_str[0]),
                _ => { eprintln!("internal error: no Rust codegen for random.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            "regex" => match func {
                "match?" | "match_hdlm_qm_" => format!("almide_regex_is_match(&{}, &{})", args_str[0], args_str[1]),
                "full_match?" | "full_match_hdlm_qm_" => format!("almide_regex_full_match(&{}, &{})", args_str[0], args_str[1]),
                "find" => format!("almide_regex_find(&{}, &{})", args_str[0], args_str[1]),
                "find_all" => format!("almide_regex_find_all(&{}, &{})", args_str[0], args_str[1]),
                "replace" => format!("almide_regex_replace(&{}, &{}, &{})", args_str[0], args_str[1], args_str[2]),
                "replace_first" => format!("almide_regex_replace_first(&{}, &{}, &{})", args_str[0], args_str[1], args_str[2]),
                "split" => format!("almide_regex_split(&{}, &{})", args_str[0], args_str[1]),
                "captures" => format!("almide_regex_captures(&{}, &{})", args_str[0], args_str[1]),
                _ => { eprintln!("internal error: no Rust codegen for regex.{}() — this is a compiler bug", func); std::process::exit(70); },
            },
            _ => {
                let resolved = self.module_aliases.get(module)
                    .cloned()
                    .unwrap_or_else(|| module.to_string());
                let rust_mod = resolved.replace('.', "_");
                let call = format!("{}::{}({})", rust_mod, func, args_str.join(", "));
                // Auto-propagate ? for user module effect/Result functions
                if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
                    format!("{}?", call)
                } else {
                    call
                }
            }
        }
    }
}
