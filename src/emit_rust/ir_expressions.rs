use almide::ir::*;
use almide::types::Ty;
use super::Emitter;

impl Emitter {
    pub(crate) fn gen_ir_expr(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::LitInt { value } => format!("{}i64", value),
            IrExprKind::LitFloat { value } => format!("{:?}f64", value),
            IrExprKind::LitStr { value } => format!("{:?}.to_string()", value),
            IrExprKind::LitBool { value } => format!("{}", value),
            IrExprKind::Unit => "()".to_string(),

            IrExprKind::Var { id } => {
                let info = self.ir_var_table().get(*id);
                let name = crate::emit_common::sanitize(&info.name);
                // Top-level let: LazyLock needs deref+clone
                if let Some(&needs_deref) = self.top_let_names.get(&info.name) {
                    if needs_deref {
                        return format!("(*{}).clone()", name);
                    }
                }
                name
            }

            IrExprKind::BinOp { op, left, right } => self.gen_ir_binary(*op, left, right),
            IrExprKind::UnOp { op, operand } => {
                let o = self.gen_ir_expr(operand);
                match op {
                    UnOp::Not => format!("!({})", o),
                    UnOp::NegInt | UnOp::NegFloat => format!("-({})", o),
                }
            }

            IrExprKind::If { cond, then, else_ } => {
                let c = self.gen_ir_expr(cond);
                let t = self.gen_ir_expr(then);
                let e = self.gen_ir_expr(else_);
                format!("if {} {{ {} }} else {{ {} }}", c, t, e)
            }

            IrExprKind::Match { subject, arms } => self.gen_ir_match(subject, arms),
            IrExprKind::Block { stmts, expr } => self.gen_ir_block(stmts, expr.as_deref()),
            IrExprKind::DoBlock { stmts, expr } => self.gen_ir_do_block(stmts, expr.as_deref()),

            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_ir_stmt(s)))
                    .collect();
                let binding = if let Some(tuple_vars) = var_tuple {
                    let names: Vec<String> = tuple_vars.iter()
                        .map(|v| self.ir_var_table().get(*v).name.clone())
                        .collect();
                    format!("({})", names.join(", "))
                } else {
                    self.ir_var_table().get(*var).name.clone()
                };
                // Optimize: for i in range → native Rust range
                if let IrExprKind::Range { start, end, inclusive } = &iterable.kind {
                    let s = self.gen_ir_expr(start);
                    let e = self.gen_ir_expr(end);
                    let range = if *inclusive {
                        format!("{s}..={e}")
                    } else {
                        format!("{s}..{e}")
                    };
                    format!("for {binding} in {range} {{\n{}\n}}", stmts_str.join("\n"))
                } else {
                    let iter_str = self.gen_ir_expr(iterable);
                    format!("for {binding} in ({iter_str}).clone() {{\n{}\n}}", stmts_str.join("\n"))
                }
            }

            IrExprKind::While { cond, body } => {
                let c = self.gen_ir_expr(cond);
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_ir_stmt(s)))
                    .collect();
                format!("while {} {{\n{}\n}}", c, stmts_str.join("\n"))
            }
            IrExprKind::Break => "break".to_string(),
            IrExprKind::Continue => "continue".to_string(),

            IrExprKind::Call { target, args, type_args } => self.gen_ir_call(target, args, type_args),

            IrExprKind::List { elements } => {
                let elems: Vec<String> = elements.iter().map(|e| {
                    let code = self.gen_ir_expr(e);
                    if matches!(&e.kind, IrExprKind::Var { .. }) {
                        format!("{}.clone()", code)
                    } else {
                        code
                    }
                }).collect();
                format!("vec![{}]", elems.join(", "))
            }

            IrExprKind::EmptyMap => {
                "HashMap::new()".to_string()
            }

            IrExprKind::MapLiteral { entries } => {
                let pairs: Vec<String> = entries.iter().map(|(k, v)| {
                    let kc = self.gen_ir_arg(k);
                    let vc = self.gen_ir_arg(v);
                    format!("({}, {})", kc, vc)
                }).collect();
                format!("vec![{}].into_iter().collect::<HashMap<_, _>>()", pairs.join(", "))
            }

            IrExprKind::Record { name, fields } => {
                if let Some(struct_name) = name {
                    let fs: Vec<String> = fields.iter().map(|(fname, fval)| {
                        let val = self.gen_ir_arg(fval);
                        if self.boxed_variant_record_fields.contains(&(struct_name.clone(), fname.clone())) {
                            format!("{}: Box::new({})", fname, val)
                        } else {
                            format!("{}: {}", fname, val)
                        }
                    }).collect();
                    let qualified = if let Some(enum_name) = self.generic_variant_constructors.get(struct_name) {
                        format!("{}::{}", enum_name, struct_name)
                    } else {
                        struct_name.clone()
                    };
                    format!("{} {{ {} }}", qualified, fs.join(", "))
                } else {
                    let fs: Vec<String> = fields.iter().map(|(fname, fval)| {
                        format!("{}: {}", fname, self.gen_ir_expr(fval))
                    }).collect();
                    let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                    let struct_name = self.anon_record_name(&field_names);
                    format!("{} {{ {} }}", struct_name, fs.join(", "))
                }
            }

            IrExprKind::SpreadRecord { base, fields } => {
                let base_expr = self.gen_ir_expr(base);
                let mut assigns: Vec<String> = Vec::new();
                for (fname, fval) in fields {
                    assigns.push(format!("__spread.{} = {};", fname, self.gen_ir_expr(fval)));
                }
                format!("{{ let mut __spread = {}.clone(); {} __spread }}", base_expr, assigns.join(" "))
            }

            IrExprKind::Tuple { elements } => {
                // Collect all VarId references across all tuple elements
                let mut var_counts: std::collections::HashMap<VarId, usize> = std::collections::HashMap::new();
                for e in elements {
                    Self::count_vars_in_expr(e, &mut var_counts);
                }
                // For vars used more than once: clone at the first top-level Var position
                let mut cloned: std::collections::HashSet<VarId> = std::collections::HashSet::new();
                let parts: Vec<String> = elements.iter().map(|e| {
                    if let IrExprKind::Var { id } = &e.kind {
                        if *var_counts.get(id).unwrap_or(&0) > 1 && cloned.insert(*id) {
                            let name = crate::emit_common::sanitize(&self.ir_var_table().get(*id).name);
                            return format!("{}.clone()", name);
                        }
                    }
                    self.gen_ir_expr(e)
                }).collect();
                format!("({})", parts.join(", "))
            }

            IrExprKind::Range { start, end, inclusive } => {
                let s = self.gen_ir_expr(start);
                let e = self.gen_ir_expr(end);
                if *inclusive {
                    format!("({s}..={e}).collect::<Vec<i64>>()")
                } else {
                    format!("({s}..{e}).collect::<Vec<i64>>()")
                }
            }

            IrExprKind::Member { object, field } => {
                let obj = self.gen_ir_expr(object);
                if Self::is_copy_ty(&expr.ty) {
                    format!("{}.{}", obj, field)
                } else {
                    format!("{}.{}.clone()", obj, field)
                }
            }

            IrExprKind::TupleIndex { object, index } => {
                let obj = self.gen_ir_expr(object);
                format!("({}).{}", obj, index)
            }

            IrExprKind::IndexAccess { object, index } => {
                let obj = self.gen_ir_expr(object);
                let idx = self.gen_ir_expr(index);
                if matches!(object.ty, almide::types::Ty::Map(_, _)) {
                    // Map index: m[key] → map.get(m, key) → Option<V>
                    format!("almide_rt_map_get(&{}, &{})", obj, idx)
                } else if self.fast_mode {
                    format!("unsafe {{ *{}.get_unchecked({} as usize) }}", obj, idx)
                } else {
                    format!("{}[{} as usize]", obj, idx)
                }
            }

            IrExprKind::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|(var, _ty)| {
                    self.ir_var_table().get(*var).name.clone()
                }).collect();
                let b = self.gen_ir_expr(body);
                format!("move |{}| {{ {} }}", ps.join(", "), b)
            }

            IrExprKind::StringInterp { parts } => {
                let mut fmt = String::new();
                let mut args = Vec::new();
                for part in parts {
                    match part {
                        IrStringPart::Lit { value } => {
                            for c in value.chars() {
                                match c {
                                    '{' => fmt.push_str("{{"),
                                    '}' => fmt.push_str("}}"),
                                    '"' => fmt.push_str("\\\""),
                                    '\\' => fmt.push_str("\\\\"),
                                    _ => fmt.push(c),
                                }
                            }
                        }
                        IrStringPart::Expr { expr } => {
                            // Use Debug ({:?}) only for compound types that lack Display:
                            // List, Option, Result, Map, Tuple, Record, Variant
                            // Everything else uses Display ({}) — including String, Int, Float, Bool, Unknown
                            let use_debug = Self::needs_debug_format(&expr.ty);
                            if use_debug {
                                fmt.push_str("{:?}");
                            } else {
                                fmt.push_str("{}");
                            }
                            args.push(self.gen_ir_expr(expr));
                        }
                    }
                }
                if args.is_empty() {
                    format!("\"{}\".to_string()", fmt)
                } else {
                    format!("format!(\"{}\", {})", fmt, args.join(", "))
                }
            }

            IrExprKind::ResultOk { expr } => {
                if self.in_do_block.get() {
                    self.gen_ir_expr(expr)
                } else if self.in_effect {
                    format!("Ok({})", self.gen_ir_expr(expr))
                } else if matches!(&expr.kind, IrExprKind::Unit) {
                    "()".to_string()
                } else {
                    format!("Ok({})", self.gen_ir_expr(expr))
                }
            }
            IrExprKind::ResultErr { expr } => {
                let msg = self.gen_ir_expr(expr);
                let needs_to_string = matches!(&expr.kind,
                    IrExprKind::LitStr { .. } | IrExprKind::StringInterp { .. });
                let val = if needs_to_string {
                    format!("{}.to_string()", msg)
                } else {
                    msg
                };
                if self.in_effect && !self.in_test && !self.in_do_block.get() {
                    format!("return Err({})", val)
                } else {
                    format!("Err({})", val)
                }
            }
            IrExprKind::OptionSome { expr } => format!("Some({})", self.gen_ir_expr(expr)),
            IrExprKind::OptionNone => "None".to_string(),
            IrExprKind::Try { expr } => {
                if self.in_effect {
                    format!("({}?)", self.gen_ir_expr(expr))
                } else {
                    self.gen_ir_expr(expr)
                }
            }
            IrExprKind::Await { expr } => {
                let inner = self.gen_ir_expr(expr);
                if self.in_effect {
                    format!("almide_block_on({})?", inner)
                } else {
                    format!("almide_block_on({})", inner)
                }
            }
            IrExprKind::Hole => "todo!()".to_string(),
            IrExprKind::Todo { message } => format!("todo!(\"{}\")", message),
        }
    }

    pub(crate) fn gen_ir_binary(&self, op: BinOp, left: &IrExpr, right: &IrExpr) -> String {
        match op {
            BinOp::AddInt | BinOp::SubInt => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                let sym = if matches!(op, BinOp::AddInt) { "+" } else { "-" };
                format!("({} {} {})", l, sym, r)
            }
            BinOp::AddFloat | BinOp::SubFloat => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                let sym = if matches!(op, BinOp::AddFloat) { "+" } else { "-" };
                format!("({} {} {})", l, sym, r)
            }
            BinOp::MulInt => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                // Check for bigint context
                if self.ir_is_bigint(left) || self.ir_is_bigint(right) {
                    format!("(({}).wrapping_mul({}))", l, r)
                } else {
                    format!("({} * {})", l, r)
                }
            }
            BinOp::MulFloat => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("({} * {})", l, r)
            }
            BinOp::DivInt | BinOp::DivFloat => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("({} / {})", l, r)
            }
            BinOp::ModInt => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                if self.ir_is_bigint(left) || self.ir_is_bigint(right) {
                    // Check if % 2^64
                    if self.ir_is_pow2_64(right) {
                        let inner = self.gen_ir_expr_u64_wrapping(left);
                        format!("(({}) as i64)", inner)
                    } else {
                        let lu = self.gen_ir_expr_u64_wrapping(left);
                        let ru = self.gen_ir_expr_u64_wrapping(right);
                        format!("(({} % {}) as i64)", lu, ru)
                    }
                } else {
                    format!("({} % {})", l, r)
                }
            }
            BinOp::ModFloat => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("({} % {})", l, r)
            }
            BinOp::PowFloat => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("({}.powf({}))", l, r)
            }
            BinOp::XorInt => {
                if self.ir_is_bigint(left) || self.ir_is_bigint(right) {
                    let l = self.gen_ir_expr_u64_wrapping(left);
                    let r = self.gen_ir_expr_u64_wrapping(right);
                    format!("(({} ^ {}) as i64)", l, r)
                } else {
                    let l = self.gen_ir_expr(left);
                    let r = self.gen_ir_expr(right);
                    format!("({} ^ {})", l, r)
                }
            }
            BinOp::ConcatStr | BinOp::ConcatList => {
                let l = self.gen_ir_arg(left);
                let r = self.gen_ir_arg(right);
                format!("AlmideConcat::concat({}, {})", l, r)
            }
            BinOp::Eq => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("almide_eq!({}, {})", l, r)
            }
            BinOp::Neq => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("almide_ne!({}, {})", l, r)
            }
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                let sym = match op {
                    BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=",
                    _ => unreachable!(),
                };
                format!("({} {} {})", l, sym, r)
            }
            BinOp::And => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("({} && {})", l, r)
            }
            BinOp::Or => {
                let l = self.gen_ir_expr(left);
                let r = self.gen_ir_expr(right);
                format!("({} || {})", l, r)
            }
        }
    }

    pub(crate) fn gen_ir_call(&self, target: &CallTarget, args: &[IrExpr], type_args: &[almide::types::Ty]) -> String {
        match target {
            CallTarget::Named { name } => {
                match name.as_str() {
                    "println" => {
                        let arg = self.gen_ir_expr(&args[0]);
                        return format!("println!(\"{{}}\", {})", arg);
                    }
                    "eprintln" => {
                        let arg = self.gen_ir_expr(&args[0]);
                        return format!("eprintln!(\"{{}}\", {})", arg);
                    }
                    "err" => {
                        let msg = self.gen_ir_expr(&args[0]);
                        return format!("return Err(({}).to_string())", msg);
                    }
                    "assert_eq" => {
                        let a = self.gen_ir_expr(&args[0]);
                        let b = self.gen_ir_expr(&args[1]);
                        let msg = if args.len() >= 3 { Some(self.gen_ir_expr(&args[2])) } else { None };
                        if matches!(&args[1].kind, IrExprKind::List { elements } if elements.is_empty()) {
                            return format!("{{ let __v = {}; assert!(__v.is_empty(), \"expected empty list but got {{:?}}\", __v); }}", a);
                        }
                        if matches!(&args[0].kind, IrExprKind::List { elements } if elements.is_empty()) {
                            return format!("{{ let __v = {}; assert!(__v.is_empty(), \"expected empty list but got {{:?}}\", __v); }}", b);
                        }
                        if let Some(m) = msg {
                            return format!("assert_eq!({}, {}, \"{{}}\", {})", a, b, m);
                        }
                        return format!("assert_eq!({}, {})", a, b);
                    }
                    "assert_ne" => {
                        let a = self.gen_ir_expr(&args[0]);
                        let b = self.gen_ir_expr(&args[1]);
                        let msg = if args.len() >= 3 { Some(self.gen_ir_expr(&args[2])) } else { None };
                        if let Some(m) = msg {
                            return format!("assert_ne!({}, {}, \"{{}}\", {})", a, b, m);
                        }
                        return format!("assert_ne!({}, {})", a, b);
                    }
                    "assert" => {
                        let a = self.gen_ir_expr(&args[0]);
                        let msg = if args.len() >= 2 { Some(self.gen_ir_expr(&args[1])) } else { None };
                        if let Some(m) = msg {
                            return format!("assert!({}, \"{{}}\", {})", a, m);
                        }
                        return format!("assert!({})", a);
                    }
                    "unwrap_or" => {
                        let a = self.gen_ir_expr(&args[0]);
                        let b = self.gen_ir_expr(&args[1]);
                        return format!("({}).unwrap_or({})", a, b);
                    }
                    _ => {}
                }

                // Turbofish for generic calls with explicit type args
                let turbofish = if !type_args.is_empty() {
                    let tys: Vec<String> = type_args.iter().map(|t| self.ir_ty_to_rust(t)).collect();
                    format!("::<{}>", tys.join(", "))
                } else {
                    String::new()
                };

                // Generic variant unit constructor
                if self.generic_variant_unit_ctors.contains(name) {
                    let args_str: Vec<String> = args.iter().enumerate().map(|(i, a)| {
                        self.gen_ir_arg_for(a, name, i)
                    }).collect();
                    let call = format!("{}{}({})", name, turbofish, args_str.join(", "));
                    return self.ir_maybe_auto_q(&call, name);
                }

                let callee_str = crate::emit_common::sanitize(name);
                // Check for variant constructor Box wrapping
                let is_variant_ctor = name.chars().next().map_or(false, |c| c.is_uppercase());
                // Unit variant constructor (no args, uppercase name) — emit without parens
                if is_variant_ctor && args.is_empty() {
                    return callee_str;
                }
                let args_str: Vec<String> = args.iter().enumerate().map(|(i, a)| {
                    let arg_str = self.gen_ir_arg_for(a, name, i);
                    if is_variant_ctor
                        && self.boxed_variant_args.contains(&(name.clone(), i))
                        && !self.generic_variant_constructors.contains_key(name)
                    {
                        format!("Box::new({})", arg_str)
                    } else {
                        arg_str
                    }
                }).collect();
                let call = format!("{}{}({})", callee_str, turbofish, args_str.join(", "));
                self.ir_maybe_auto_q(&call, name)
            }

            CallTarget::Module { module, func } => {
                let args_str: Vec<String> = args.iter().map(|a| self.gen_ir_expr(a)).collect();
                self.gen_ir_module_call(module, func, &args_str, args)
            }

            CallTarget::Method { object, method } => {
                let obj = self.gen_ir_expr(object);
                // Built-in method calls that map to Rust method syntax
                if method == "unwrap_or" && args.len() == 1 {
                    let default = self.gen_ir_expr(&args[0]);
                    return format!("({}).unwrap_or({})", obj, default);
                }
                let rest: Vec<String> = args.iter().map(|a| self.gen_ir_expr(a)).collect();
                let mut all_args = vec![obj];
                all_args.extend(rest);
                format!("{}({})", crate::emit_common::sanitize(method), all_args.join(", "))
            }

            CallTarget::Computed { callee } => {
                let callee_str = self.gen_ir_expr(callee);
                let args_str: Vec<String> = args.iter().map(|a| self.gen_ir_arg(a)).collect();
                format!("({})({})", callee_str, args_str.join(", "))
            }
        }
    }

    fn gen_ir_module_call(&self, module: &str, func: &str, args_str: &[String], ir_args: &[IrExpr]) -> String {
        // User modules take priority over stdlib
        let resolved_mod = self.module_aliases.get(module)
            .cloned()
            .unwrap_or_else(|| module.to_string());
        if self.user_modules.contains(&resolved_mod) {
            let rust_mod = resolved_mod.replace('.', "_");
            let safe_func = crate::emit_common::sanitize(func);
            // Apply borrow inference using qualified name (module.func)
            let qualified = format!("{}.{}", resolved_mod, func);
            let borrow_args: Vec<String> = ir_args.iter().enumerate().map(|(i, a)| {
                self.gen_ir_arg_for(a, &qualified, i)
            }).collect();
            let call = format!("{}::{}({})", rust_mod, safe_func, borrow_args.join(", "));
            if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
                return format!("{}?", call);
            }
            return call;
        }
        // Try auto-generated codegen with lambda inlining from IR args
        let in_effect = self.in_effect || self.in_do_block.get();
        let inline_lambda_fn = |idx: usize, arity: usize| -> (Vec<String>, String) {
            self.ir_inline_lambda(&ir_args[idx], arity)
        };
        if let Some(expr) = almide::generated::emit_rust_calls::gen_generated_call(module, func, args_str, in_effect, &inline_lambda_fn) {
            return expr;
        }
        // Fallback: user-defined module call
        let rust_mod = resolved_mod.replace('.', "_");
        let call = format!("{}::{}({})", rust_mod, func, args_str.join(", "));
        if self.in_effect && (self.effect_fns.contains(&func.to_string()) || self.result_fns.contains(&func.to_string())) {
            format!("{}?", call)
        } else {
            call
        }
    }

    /// Extract lambda parameter names and body code from an IR expression.
    fn ir_inline_lambda(&self, expr: &IrExpr, arity: usize) -> (Vec<String>, String) {
        if let IrExprKind::Lambda { params, body } = &expr.kind {
            let names: Vec<String> = params.iter().map(|(var, _)| {
                self.ir_var_table().get(*var).name.clone()
            }).collect();
            let body_str = self.gen_ir_expr(body);
            (names, body_str)
        } else {
            // Not a lambda — wrap as function reference
            let f = self.gen_ir_expr(expr);
            if arity == 1 {
                (vec!["__x".to_string()], format!("({})((__x).clone())", f))
            } else {
                (vec!["__a".to_string(), "__b".to_string()], format!("({})(__a, __b.clone())", f))
            }
        }
    }

    /// Returns true if the Almide type maps to a Copy type in Rust.
    /// Primitives (i64, f64, bool, ()) are Copy.
    /// Option<T> and tuples are Copy when their inner types are Copy.
    pub(crate) fn is_copy_ty(ty: &almide::types::Ty) -> bool {
        use almide::types::Ty;
        match ty {
            Ty::Int | Ty::Float | Ty::Bool | Ty::Unit => true,
            Ty::Option(inner) => Self::is_copy_ty(inner),
            Ty::Tuple(elems) => elems.iter().all(|e| Self::is_copy_ty(e)),
            _ => false,
        }
    }

    /// Count all variable references in an expression (for multi-use detection in tuples)
    fn count_vars_in_expr(expr: &IrExpr, counts: &mut std::collections::HashMap<VarId, usize>) {
        match &expr.kind {
            IrExprKind::Var { id } => { *counts.entry(*id).or_insert(0) += 1; }
            IrExprKind::Call { args, .. } => {
                for a in args { Self::count_vars_in_expr(a, counts); }
            }
            IrExprKind::BinOp { left, right, .. } => {
                Self::count_vars_in_expr(left, counts);
                Self::count_vars_in_expr(right, counts);
            }
            IrExprKind::UnOp { operand, .. } => Self::count_vars_in_expr(operand, counts),
            IrExprKind::Member { object, .. } => Self::count_vars_in_expr(object, counts),
            IrExprKind::IndexAccess { object, index } => {
                Self::count_vars_in_expr(object, counts);
                Self::count_vars_in_expr(index, counts);
            }
            IrExprKind::If { cond, then, else_, .. } => {
                Self::count_vars_in_expr(cond, counts);
                Self::count_vars_in_expr(then, counts);
                Self::count_vars_in_expr(else_, counts);
            }
            IrExprKind::Tuple { elements } | IrExprKind::List { elements } => {
                for e in elements { Self::count_vars_in_expr(e, counts); }
            }
            IrExprKind::Record { fields, .. } => {
                for (_, v) in fields { Self::count_vars_in_expr(v, counts); }
            }
            IrExprKind::MapLiteral { entries } => {
                for (k, v) in entries {
                    Self::count_vars_in_expr(k, counts);
                    Self::count_vars_in_expr(v, counts);
                }
            }
            _ => {}
        }
    }

    /// Count how many times a VarId is referenced in an IR expression tree
    fn count_var_uses(var: VarId, expr: &IrExpr) -> usize {
        match &expr.kind {
            IrExprKind::Var { id } => if *id == var { 1 } else { 0 },
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } |
            IrExprKind::LitBool { .. } | IrExprKind::LitStr { .. } |
            IrExprKind::Unit | IrExprKind::Hole | IrExprKind::Todo { .. } |
            IrExprKind::Break | IrExprKind::Continue | IrExprKind::OptionNone |
            IrExprKind::EmptyMap => 0,
            IrExprKind::BinOp { left, right, .. } => {
                Self::count_var_uses(var, left) + Self::count_var_uses(var, right)
            }
            IrExprKind::UnOp { operand, .. } => Self::count_var_uses(var, operand),
            IrExprKind::Call { args, .. } => {
                args.iter().map(|a| Self::count_var_uses(var, a)).sum()
            }
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
                elements.iter().map(|e| Self::count_var_uses(var, e)).sum()
            }
            IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
                fields.iter().map(|(_, v)| Self::count_var_uses(var, v)).sum()
            }
            IrExprKind::MapLiteral { entries } => {
                entries.iter().map(|(k, v)| Self::count_var_uses(var, k) + Self::count_var_uses(var, v)).sum()
            }
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
                Self::count_var_uses(var, object)
            }
            IrExprKind::IndexAccess { object, index } => {
                Self::count_var_uses(var, object) + Self::count_var_uses(var, index)
            }
            IrExprKind::If { cond, then, else_, .. } => {
                Self::count_var_uses(var, cond) + Self::count_var_uses(var, then) + Self::count_var_uses(var, else_)
            }
            IrExprKind::Match { subject, arms } => {
                Self::count_var_uses(var, subject) + arms.iter().map(|a| Self::count_var_uses(var, &a.body)).sum::<usize>()
            }
            IrExprKind::Block { stmts, expr, .. } | IrExprKind::DoBlock { stmts, expr, .. } => {
                let s: usize = stmts.iter().map(|st| Self::count_var_uses_stmt(var, st)).sum();
                s + expr.as_ref().map_or(0, |e| Self::count_var_uses(var, e))
            }
            IrExprKind::ForIn { body, iterable, .. } => {
                Self::count_var_uses(var, iterable) + body.iter().map(|st| Self::count_var_uses_stmt(var, st)).sum::<usize>()
            }
            IrExprKind::While { cond, body } => {
                Self::count_var_uses(var, cond) + body.iter().map(|st| Self::count_var_uses_stmt(var, st)).sum::<usize>()
            }
            IrExprKind::Lambda { body, .. } => Self::count_var_uses(var, body),
            IrExprKind::StringInterp { parts } => {
                parts.iter().map(|p| match p {
                    IrStringPart::Expr { expr } => Self::count_var_uses(var, expr),
                    _ => 0,
                }).sum()
            }
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e } |
            IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e } |
            IrExprKind::Await { expr: e } => Self::count_var_uses(var, e),
            IrExprKind::Range { start, end, .. } => {
                Self::count_var_uses(var, start) + Self::count_var_uses(var, end)
            }
        }
    }

    fn count_var_uses_stmt(var: VarId, stmt: &IrStmt) -> usize {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } => Self::count_var_uses(var, value),
            IrStmtKind::BindDestructure { value, .. } => Self::count_var_uses(var, value),
            IrStmtKind::Assign { value, .. } => Self::count_var_uses(var, value),
            IrStmtKind::IndexAssign { index, value, .. } => {
                Self::count_var_uses(var, index) + Self::count_var_uses(var, value)
            }
            IrStmtKind::FieldAssign { value, .. } => Self::count_var_uses(var, value),
            IrStmtKind::Guard { cond, else_ } => {
                Self::count_var_uses(var, cond) + Self::count_var_uses(var, else_)
            }
            IrStmtKind::Expr { expr } => Self::count_var_uses(var, expr),
            IrStmtKind::Comment { .. } => 0,
        }
    }

    /// Auto-append ? for effect fn calls
    fn ir_maybe_auto_q(&self, call: &str, name: &str) -> String {
        if self.in_effect && !self.in_test && !self.skip_auto_q.get() {
            if self.effect_fns.contains(&name.to_string()) {
                return format!("{}?", call);
            }
        }
        if self.in_do_block.get() {
            if self.result_fns.contains(&name.to_string()) || self.effect_fns.contains(&name.to_string()) {
                return format!("{}?", call);
            }
        }
        call.to_string()
    }

    /// Generate IR expression as function argument — clone Idents to avoid move
    pub(crate) fn gen_ir_arg(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::Var { id } => {
                let info = self.ir_var_table().get(*id);
                let name = crate::emit_common::sanitize(&info.name);
                // Top-level let: const needs no clone, LazyLock needs deref+clone
                if let Some(&needs_deref) = self.top_let_names.get(&info.name) {
                    if needs_deref {
                        return format!("(*{}).clone()", name);
                    }
                    return name;
                }
                // Single-use: safe to move
                if self.single_use_vars.contains(&info.name) {
                    if let Some(borrow_ty) = self.borrowed_params.get(&info.name) {
                        return Self::borrow_to_owned(&name, borrow_ty);
                    }
                    return name;
                }
                // Multi-use: clone
                if let Some(borrow_ty) = self.borrowed_params.get(&info.name) {
                    Self::borrow_to_owned(&name, borrow_ty)
                } else {
                    format!("{}.clone()", name)
                }
            }
            _ => self.gen_ir_expr(expr),
        }
    }

    /// Generate argument for a specific callee, considering borrow inference
    pub(crate) fn gen_ir_arg_for(&self, expr: &IrExpr, callee_name: &str, param_idx: usize) -> String {
        // Try qualified name first (module.func) for module-internal calls
        let qualified;
        let lookup_name = if let Some(ref module) = self.current_module {
            qualified = format!("{}.{}", module, callee_name);
            if self.borrow_info.fn_params.contains_key(&qualified) {
                &qualified
            } else {
                callee_name
            }
        } else {
            callee_name
        };
        let ownership = self.borrow_info.param_ownership(lookup_name, param_idx);
        if ownership == super::borrow::ParamOwnership::Borrow {
            match &expr.kind {
                IrExprKind::Var { id } => {
                    let info = self.ir_var_table().get(*id);
                    if self.borrowed_params.contains_key(&info.name) {
                        self.gen_ir_expr(expr)
                    } else {
                        format!("&{}", self.gen_ir_expr(expr))
                    }
                }
                _ => {
                    let e = self.gen_ir_expr(expr);
                    format!("&({})", e)
                }
            }
        } else {
            self.gen_ir_arg(expr)
        }
    }

    /// Access the IR var table (panics if IR is not available)
    pub(crate) fn ir_var_table(&self) -> &almide::ir::VarTable {
        &self.ir_program.as_ref().expect("IR must be available for IR codegen").var_table
    }

    /// Check if an IR expression involves big integers
    fn ir_is_bigint(&self, expr: &IrExpr) -> bool {
        match &expr.kind {
            IrExprKind::LitInt { value } => (*value as u128) > i64::MAX as u128,
            IrExprKind::BinOp { op, left, right, .. }
                if matches!(op, BinOp::XorInt | BinOp::MulInt | BinOp::ModInt) =>
            {
                self.ir_is_bigint(left) || self.ir_is_bigint(right)
            }
            _ => false,
        }
    }

    fn ir_is_pow2_64(&self, _expr: &IrExpr) -> bool {
        // 2^64 = 18446744073709551616, but as i64 this wraps
        // In IR, large literals are stored as i64, so check for the wrapped value
        false // IR stores i64, so 2^64 can't be represented — handled at AST level
    }

    fn gen_ir_expr_u64_wrapping(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::BinOp { op, left, right, .. }
                if matches!(op, BinOp::XorInt | BinOp::MulInt | BinOp::ModInt) =>
            {
                let l = self.gen_ir_expr_u64_wrapping(left);
                let r = self.gen_ir_expr_u64_wrapping(right);
                match op {
                    BinOp::MulInt => format!("(({}).wrapping_mul({}))", l, r),
                    BinOp::XorInt => format!("(({}) ^ ({}))", l, r),
                    BinOp::ModInt => format!("(({}) % ({}))", l, r),
                    _ => format!("(({}) {} ({}))", l, "?", r),
                }
            }
            IrExprKind::LitInt { value } => format!("{}u64", *value as u64),
            _ => {
                let e = self.gen_ir_expr(expr);
                format!("(({}) as u64)", e)
            }
        }
    }

    /// Count variable uses in an IR expression tree for single-use analysis.
    pub(crate) fn count_ir_var_uses(expr: &IrExpr, counts: &mut std::collections::HashMap<VarId, usize>) {
        match &expr.kind {
            IrExprKind::Var { id } => {
                *counts.entry(*id).or_insert(0) += 1;
            }
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
            | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
            | IrExprKind::Hole | IrExprKind::Todo { .. }
            | IrExprKind::Break | IrExprKind::Continue
            | IrExprKind::EmptyMap => {}

            IrExprKind::BinOp { left, right, .. } => {
                Self::count_ir_var_uses(left, counts);
                Self::count_ir_var_uses(right, counts);
            }
            IrExprKind::UnOp { operand, .. } => {
                Self::count_ir_var_uses(operand, counts);
            }
            IrExprKind::If { cond, then, else_ } => {
                Self::count_ir_var_uses(cond, counts);
                Self::count_ir_var_uses(then, counts);
                Self::count_ir_var_uses(else_, counts);
            }
            IrExprKind::Match { subject, arms } => {
                Self::count_ir_var_uses(subject, counts);
                for arm in arms {
                    Self::count_ir_var_uses(&arm.body, counts);
                }
            }
            IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
                for s in stmts { Self::count_ir_var_uses_in_stmt(s, counts); }
                if let Some(e) = expr { Self::count_ir_var_uses(e, counts); }
            }
            IrExprKind::Call { target, args, .. } => {
                match target {
                    CallTarget::Method { object, .. } => Self::count_ir_var_uses(object, counts),
                    CallTarget::Computed { callee } => Self::count_ir_var_uses(callee, counts),
                    _ => {}
                }
                for a in args { Self::count_ir_var_uses(a, counts); }
            }
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
                for e in elements { Self::count_ir_var_uses(e, counts); }
            }
            IrExprKind::Record { fields, .. } => {
                for (_, e) in fields { Self::count_ir_var_uses(e, counts); }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                Self::count_ir_var_uses(base, counts);
                for (_, e) in fields { Self::count_ir_var_uses(e, counts); }
            }
            IrExprKind::MapLiteral { entries } => {
                for (k, v) in entries {
                    Self::count_ir_var_uses(k, counts);
                    Self::count_ir_var_uses(v, counts);
                }
            }
            IrExprKind::Range { start, end, .. } => {
                Self::count_ir_var_uses(start, counts);
                Self::count_ir_var_uses(end, counts);
            }
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
                Self::count_ir_var_uses(object, counts);
            }
            IrExprKind::IndexAccess { object, index } => {
                Self::count_ir_var_uses(object, counts);
                Self::count_ir_var_uses(index, counts);
            }
            IrExprKind::ForIn { iterable, body, .. } => {
                Self::count_ir_var_uses(iterable, counts);
                let mut loop_counts = std::collections::HashMap::new();
                for s in body { Self::count_ir_var_uses_in_stmt(s, &mut loop_counts); }
                for (id, _) in loop_counts { *counts.entry(id).or_insert(0) += 2; }
            }
            IrExprKind::While { cond, body } => {
                Self::count_ir_var_uses(cond, counts);
                let mut loop_counts = std::collections::HashMap::new();
                for s in body { Self::count_ir_var_uses_in_stmt(s, &mut loop_counts); }
                for (id, _) in loop_counts { *counts.entry(id).or_insert(0) += 2; }
            }
            IrExprKind::Lambda { body, .. } => {
                let mut lambda_counts = std::collections::HashMap::new();
                Self::count_ir_var_uses(body, &mut lambda_counts);
                for (id, _) in lambda_counts { *counts.entry(id).or_insert(0) += 2; }
            }
            IrExprKind::StringInterp { parts } => {
                for part in parts {
                    if let IrStringPart::Expr { expr } = part {
                        Self::count_ir_var_uses(expr, counts);
                    }
                }
            }
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
            | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
            | IrExprKind::Await { expr } => {
                Self::count_ir_var_uses(expr, counts);
            }
        }
    }

    fn count_ir_var_uses_in_stmt(stmt: &IrStmt, counts: &mut std::collections::HashMap<VarId, usize>) {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
            | IrStmtKind::Assign { value, .. } => {
                Self::count_ir_var_uses(value, counts);
            }
            IrStmtKind::IndexAssign { index, value, .. } => {
                Self::count_ir_var_uses(index, counts);
                Self::count_ir_var_uses(value, counts);
            }
            IrStmtKind::FieldAssign { value, .. } => {
                Self::count_ir_var_uses(value, counts);
            }
            IrStmtKind::Expr { expr } => {
                Self::count_ir_var_uses(expr, counts);
            }
            IrStmtKind::Guard { cond, else_ } => {
                Self::count_ir_var_uses(cond, counts);
                Self::count_ir_var_uses(else_, counts);
            }
            IrStmtKind::Comment { .. } => {}
        }
    }

    /// Analyze IR function body to find variables used exactly once
    pub(crate) fn analyze_ir_single_use(&mut self, body: &IrExpr, param_var_ids: &[(VarId, almide::types::Ty)]) {
        let mut counts: std::collections::HashMap<VarId, usize> = std::collections::HashMap::new();
        Self::count_ir_var_uses(body, &mut counts);
        self.single_use_vars.clear();
        let param_ids: std::collections::HashSet<VarId> = param_var_ids.iter().map(|(id, _)| *id).collect();
        let var_table = &self.ir_program.as_ref().expect("IR").var_table;
        for (id, count) in &counts {
            if *count == 1 && !param_ids.contains(id) {
                self.single_use_vars.insert(var_table.get(*id).name.clone());
            }
        }
    }

    fn needs_debug_format(ty: &Ty) -> bool {
        match ty {
            Ty::List(_) | Ty::Option(_) | Ty::Result(_, _) |
            Ty::Map(_, _) | Ty::Tuple(_) | Ty::Record { .. } |
            Ty::Variant { .. } => true,
            Ty::Named(name, _) => {
                // Built-in type names use Display, user-defined types use Debug
                !matches!(name.as_str(), "String" | "Int" | "Float" | "Bool")
            }
            _ => false,
        }
    }
}
