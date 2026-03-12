use crate::ast::*;
use super::TsEmitter;

impl TsEmitter {
    pub(crate) fn gen_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<i128>() {
                    if n > 9007199254740991 || n < -9007199254740991 {
                        return format!("{}n", raw);
                    }
                }
                raw.clone()
            }
            Expr::Float { value, .. } => format!("{}", value),
            Expr::String { value, .. } => Self::json_string(value),
            Expr::InterpolatedString { value, .. } => {
                // Re-parse ${expr} segments and re-emit through gen_expr for module mapping
                let mut result = String::from("`");
                let mut chars = value.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '$' && chars.peek() == Some(&'{') {
                        chars.next(); // skip {
                        let mut expr_str = String::new();
                        let mut depth = 1;
                        while let Some(ch) = chars.next() {
                            if ch == '{' { depth += 1; }
                            if ch == '}' { depth -= 1; if depth == 0 { break; } }
                            expr_str.push(ch);
                        }
                        let tokens = crate::lexer::Lexer::tokenize(&expr_str);
                        let mut parser = crate::parser::Parser::new(tokens);
                        if let Ok(parsed_expr) = parser.parse_single_expr() {
                            result.push_str(&format!("${{{}}}", self.gen_expr(&parsed_expr)));
                        } else {
                            result.push_str(&format!("${{{}}}", expr_str));
                        }
                    } else {
                        result.push(c);
                    }
                }
                result.push('`');
                result
            }
            Expr::Bool { value, .. } => format!("{}", value),
            Expr::Ident { name, .. } => Self::sanitize(name),
            Expr::TypeName { name, .. } => {
                if self.generic_variant_unit_ctors.contains(name) {
                    format!("{}()", name)
                } else {
                    name.clone()
                }
            }
            Expr::Unit { .. } => "undefined".to_string(),
            Expr::None { .. } => "null".to_string(),
            Expr::Some { expr, .. } => self.gen_expr(expr),
            Expr::Ok { expr, .. } => self.gen_expr(expr),
            Expr::Err { expr, .. } => self.gen_err(expr),
            Expr::List { elements, .. } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            Expr::Record { name, fields, .. } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                // If this is a variant record constructor, add a tag field
                if let Some(cname) = name.as_ref() {
                    if self.variant_constructors.contains(cname.as_str()) {
                        let mut all = vec![format!("tag: {}", Self::json_string(cname))];
                        all.extend(fs);
                        return format!("{{ {} }}", all.join(", "));
                    }
                }
                format!("{{ {} }}", fs.join(", "))
            }
            Expr::SpreadRecord { base, fields, .. } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ ...{}, {} }}", self.gen_expr(base), fs.join(", "))
            }
            Expr::Call { callee, args, type_args, .. } => self.gen_call_with_ta(callee, args, type_args.as_ref()),
            Expr::Member { object, field, .. } => {
                if let Expr::Ident { name, .. } = object.as_ref() {
                    let mapped = self.map_module(name);
                    format!("{}.{}", mapped, Self::sanitize(field))
                } else {
                    let obj = self.gen_expr(object);
                    format!("{}.{}", obj, Self::sanitize(field))
                }
            }
            Expr::TupleIndex { object, index, .. } => {
                let obj = self.gen_expr(object);
                format!("({})[{}]", obj, index)
            }
            Expr::Pipe { left, right, .. } => self.gen_pipe(left, right),
            Expr::If { cond, then, else_, .. } => {
                let t = self.gen_expr_value(then);
                let e = self.gen_expr_value(else_);
                format!("({} ? {} : {})", self.gen_expr(cond), t, e)
            }
            Expr::Match { subject, arms, .. } => self.gen_match(subject, arms),
            Expr::Block { stmts, expr: final_expr, .. } => {
                self.gen_block(stmts, final_expr.as_deref(), 0)
            }
            Expr::DoBlock { stmts, expr: final_expr, .. } => {
                self.gen_do_block(stmts, final_expr.as_deref(), 0)
            }
            Expr::Range { start, end, inclusive, .. } => {
                let s = self.gen_expr(start);
                let e = self.gen_expr(end);
                if *inclusive {
                    format!("Array.from({{length: ({e}) - ({s}) + 1}}, (_, i) => ({s}) + i)")
                } else {
                    format!("Array.from({{length: ({e}) - ({s})}}, (_, i) => ({s}) + i)")
                }
            }
            Expr::ForIn { var, var_tuple, iterable, body, .. } => {
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                let binding = if let Some(names) = var_tuple {
                    format!("[{}]", names.iter().map(|n| Self::sanitize(n)).collect::<Vec<_>>().join(", "))
                } else {
                    Self::sanitize(var)
                };
                // Optimize: for i in start..end → C-style for loop
                if let Expr::Range { start, end, inclusive, .. } = iterable.as_ref() {
                    let s = self.gen_expr(start);
                    let e = self.gen_expr(end);
                    let cmp = if *inclusive { "<=" } else { "<" };
                    format!("for (let {} = {}; {} {} {}; {}++) {{\n{}\n}}", binding, s, binding, cmp, e, binding, stmts_str.join("\n"))
                } else {
                    let iter_str = self.gen_expr(iterable);
                    format!("for (const {} of {}) {{\n{}\n}}", binding, iter_str, stmts_str.join("\n"))
                }
            }
            Expr::Lambda { params, body, .. } => {
                let ps: Vec<String> = params.iter().map(|p| {
                    if let Some(names) = &p.tuple_names {
                        format!("[{}]", names.join(", "))
                    } else {
                        p.name.clone()
                    }
                }).collect();
                format!("(({}) => {})", ps.join(", "), self.gen_expr(body))
            }
            Expr::Binary { op, left, right, .. } => self.gen_binary(op, left, right),
            Expr::Unary { op, operand, .. } => {
                if op == "not" {
                    format!("!({})", self.gen_expr(operand))
                } else {
                    format!("{}{}", op, self.gen_expr(operand))
                }
            }
            Expr::Paren { expr, .. } => format!("({})", self.gen_expr(expr)),
            Expr::Tuple { elements, .. } => {
                let parts: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", parts.join(", "))
            }
            Expr::Try { expr, .. } => self.gen_expr(expr),
            Expr::Await { expr, .. } => format!("await {}", self.gen_expr(expr)),
            Expr::Break { .. } => "break".to_string(),
            Expr::Continue { .. } => "continue".to_string(),
            Expr::Hole { .. } => if self.js_mode { "null /* hole */".to_string() } else { "null as any /* hole */".to_string() },
            Expr::Todo { message, .. } => format!("__throw({})", Self::json_string(message)),
            Expr::Placeholder { .. } => "__placeholder__".to_string(),
        }
    }

    pub(crate) fn gen_err(&self, expr: &Expr) -> String {
        // In effect fn, throw immediately (for auto-? propagation via try-catch)
        // In non-effect context (test blocks, pure fn), produce a value
        let msg = self.gen_err_msg_expr(expr);
        if self.in_effect.get() {
            format!("__throw({})", msg)
        } else {
            format!("new __Err({})", msg)
        }
    }

    /// Generate the error message expression string for err()
    pub(crate) fn gen_err_msg_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Call { callee, args, .. } => {
                let callee_str = if let Expr::TypeName { name, .. } = callee.as_ref() {
                    Self::pascal_to_message(name)
                } else {
                    self.gen_expr(callee)
                };
                let arg = if !args.is_empty() { self.gen_expr(&args[0]) } else { "\"\"".to_string() };
                format!("{} + \": \" + {}", Self::json_string(&callee_str), arg)
            }
            Expr::TypeName { name, .. } => {
                Self::json_string(&Self::pascal_to_message(name))
            }
            Expr::String { value, .. } => {
                Self::json_string(value)
            }
            _ => {
                format!("String({})", self.gen_expr(expr))
            }
        }
    }

    /// Generate an expression as a value, unwrapping simple blocks to avoid unnecessary IIFEs.
    /// A block with no statements and just a final expression can be inlined directly.
    pub(crate) fn gen_expr_value(&self, expr: &Expr) -> String {
        match expr {
            Expr::Block { stmts, expr: Some(final_expr), .. } if stmts.is_empty() => {
                self.gen_expr_value(final_expr)
            }
            other if Self::needs_iife(other) => {
                format!("(() => {})()", self.gen_expr(other))
            }
            _ => self.gen_expr(expr),
        }
    }

    /// Resolve UFCS module for the TS target.
    /// 1. Try compile-time resolution using the receiver's resolved_type
    /// 2. Fall back to runtime dispatch for ambiguous methods with Unknown type
    /// 3. Fall back to single-candidate resolution
    fn resolve_ufcs_for_ts(&self, object: &Expr, method: &str) -> Option<String> {
        let candidates = crate::stdlib::resolve_ufcs_candidates(method);
        if candidates.is_empty() {
            return None;
        }

        // Try type-based resolution first (zero runtime cost)
        if let Some(rt) = object.resolved_type() {
            if let Some(module) = crate::stdlib::resolve_ufcs_by_type(method, rt) {
                return Some(format!("__almd_{}", module));
            }
        }

        // Single candidate — use it directly
        if candidates.len() == 1 {
            return Some(format!("__almd_{}", candidates[0]));
        }

        // Ambiguous with unknown type — runtime dispatch
        // Build IIFE: ((__r) => typeof __r === 'string' ? ... : ...)(obj)
        // This is handled by gen_ufcs_dispatch, but we return a special marker
        // Actually, we can't return a module name here — we need the full expression.
        // Return None so the caller can handle it.
        None
    }

    /// Resolve UFCS with runtime dispatch for truly ambiguous cases (Unknown receiver type).
    /// Called only when resolve_ufcs_for_ts returns None but candidates exist.
    fn try_ufcs_runtime_dispatch(&self, object: &Expr, method: &str, args: &[Expr]) -> Option<String> {
        let candidates = crate::stdlib::resolve_ufcs_candidates(method);
        if candidates.len() > 1 {
            return Some(self.gen_ufcs_dispatch(method, &candidates, object, args));
        }
        None
    }

    /// Generate runtime dispatch for ambiguous UFCS methods (e.g. `len` in string/list/map).
    /// Returns `typeof __r === 'string' ? __almd_string.len(__r) : Array.isArray(__r) ? __almd_list.len(__r) : __almd_map.len(__r)`
    fn gen_ufcs_dispatch(&self, method: &str, candidates: &[&str], obj: &Expr, args: &[Expr]) -> String {
        let obj_str = self.gen_expr(obj);
        let sanitized = Self::sanitize(method);
        let extra_args: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();

        let tmp = "__r";

        // Build call strings using tmp directly (not via string replacement, which corrupts module names)
        let mut parts: Vec<String> = Vec::new();
        for &candidate in candidates {
            let module = self.map_module(candidate);
            let mut all_args = vec![tmp.to_string()];
            all_args.extend(extra_args.clone());
            let call = format!("{}.{}({})", module, sanitized, all_args.join(", "));
            parts.push(call);
        }

        // Single candidate — call directly with the real object (no tmp wrapper needed)
        if parts.len() == 1 {
            let module = self.map_module(candidates[0]);
            let mut all_args = vec![obj_str];
            all_args.extend(extra_args);
            return format!("{}.{}({})", module, sanitized, all_args.join(", "));
        }

        // Generate ternary chain wrapped in an IIFE
        let mut chain = String::new();
        let total = parts.len();
        for (i, call_str) in parts.iter().enumerate() {
            if i == total - 1 {
                chain.push_str(call_str);
            } else {
                let cond = match candidates[i] {
                    "string" => format!("typeof {} === 'string'", tmp),
                    "list" => format!("Array.isArray({})", tmp),
                    "map" => format!("typeof {} === 'object' && {} !== null && !Array.isArray({})", tmp, tmp, tmp),
                    _ => "true".to_string(),
                };
                chain.push_str(&format!("{} ? {} : ", cond, call_str));
            }
        }

        format!("(({}) => {})({})", tmp, chain, obj_str)
    }

    /// Check if an expression is a module chain (e.g. deeplib.http.client)
    fn is_module_chain(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Ident { name, .. } => {
                self.user_modules.contains(&name.to_string()) || crate::stdlib::is_stdlib_module(name)
            }
            Expr::Member { object, .. } => {
                // Check if the full path up to this point is a known module
                if let Some(path) = self.expr_to_module_path(expr) {
                    if self.user_modules.contains(&path) {
                        return true;
                    }
                }
                // Recurse: if the object is a module chain, this is likely a module member
                self.is_module_chain(object)
            }
            _ => false,
        }
    }

    /// Try to reconstruct a dotted module path from a member chain
    fn expr_to_module_path(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Ident { name, .. } => Some(name.clone()),
            Expr::Member { object, field, .. } => {
                self.expr_to_module_path(object).map(|base| format!("{}.{}", base, field))
            }
            _ => None,
        }
    }

    pub(crate) fn gen_call_with_ta(&self, callee: &Expr, args: &[Expr], _type_args: Option<&Vec<crate::ast::TypeExpr>>) -> String {
        // TypeScript/JS: type arguments are erased at runtime, so we ignore them
        // In TS mode we could emit `fn<T>()` but for calls it's not needed since TS infers
        self.gen_call(callee, args)
    }

    pub(crate) fn gen_call(&self, callee: &Expr, args: &[Expr]) -> String {
        // UFCS: expr.method(args) => __module.method(expr, args)
        if let Expr::Member { object, field, .. } = callee {
            let is_module = match object.as_ref() {
                Expr::Ident { name, .. } => {
                    self.user_modules.contains(&name.to_string()) || crate::stdlib::is_stdlib_module(name)
                }
                _ => self.is_module_chain(object),
            };

            if !is_module {
                // Try type-based UFCS resolution (compile-time, zero cost)
                if let Some(module) = self.resolve_ufcs_for_ts(object, field) {
                    let obj_str = self.gen_expr(object);
                    let mut all_args = vec![obj_str];
                    all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                    return format!("{}.{}({})", module, Self::sanitize(field), all_args.join(", "));
                }
                // Fallback: runtime dispatch for ambiguous methods with unknown receiver type
                if let Some(dispatched) = self.try_ufcs_runtime_dispatch(object, field, args) {
                    return dispatched;
                }
            }
        }

        // For generic unit constructors used as callees, use raw name to avoid double-call
        let callee_str = match callee {
            Expr::TypeName { name, .. } if self.generic_variant_unit_ctors.contains(name) => name.clone(),
            Expr::Ident { name, .. } if self.generic_variant_unit_ctors.contains(name) => Self::sanitize(name),
            _ => self.gen_expr(callee),
        };
        // assert_eq with err(): when one arg is err(), wrap the other in try-catch
        // so that effect fn calls that throw get compared as __Err values
        if callee_str == "assert_eq" && args.len() == 2 {
            if matches!(&args[1], Expr::Err { .. }) {
                let other = self.gen_expr(&args[0]);
                let err_val = self.gen_expr(&args[1]);
                return format!("(() => {{ let __v; try {{ __v = {}; }} catch (__e) {{ __v = new __Err(__e instanceof Error ? __e.message : String(__e)); }} assert_eq(__v, {}); }})()", other, err_val);
            }
            if matches!(&args[0], Expr::Err { .. }) {
                let other = self.gen_expr(&args[1]);
                let err_val = self.gen_expr(&args[0]);
                return format!("(() => {{ let __v; try {{ __v = {}; }} catch (__e) {{ __v = new __Err(__e instanceof Error ? __e.message : String(__e)); }} assert_eq({}, __v); }})()", other, err_val);
            }
        }
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        format!("{}({})", callee_str, args_str.join(", "))
    }


    pub(crate) fn gen_binary(&self, op: &str, left: &Expr, right: &Expr) -> String {
        let l = self.gen_expr(left);
        let r = self.gen_expr(right);
        let has_float = Self::expr_has_float(left) || Self::expr_has_float(right);
        match op {
            "and" => format!("({} && {})", l, r),
            "or" => format!("({} || {})", l, r),
            "==" => format!("__deep_eq({}, {})", l, r),
            "!=" => format!("!__deep_eq({}, {})", l, r),
            "++" => format!("__concat({}, {})", l, r),
            "^" if has_float => format!("Math.pow({}, {})", l, r),
            "^" => format!("__bigop(\"^\", {}, {})", l, r),
            "*" if !has_float => format!("__bigop(\"*\", {}, {})", l, r),
            "%" if !has_float => format!("__bigop(\"%\", {}, {})", l, r),
            "/" if !has_float => format!("__div({}, {})", l, r),
            _ => format!("({} {} {})", l, op, r),
        }
    }

    /// Check if an expression involves Float values using resolved type info from the checker.
    fn expr_has_float(expr: &Expr) -> bool {
        use crate::ast::ResolvedType;
        if expr.resolved_type() == Some(ResolvedType::Float) {
            return true;
        }
        // Fallback: literal detection for unchecked ASTs (e.g. tests without checker)
        matches!(expr, Expr::Float { .. })
    }

    pub(crate) fn gen_pipe(&self, left: &Expr, right: &Expr) -> String {
        let l = self.gen_expr(left);
        match right {
            Expr::Call { callee, args, .. } => {
                let has_placeholder = args.iter().any(|a| matches!(a, Expr::Placeholder { .. }));
                if has_placeholder {
                    let mapped_args: Vec<String> = args.iter().map(|a| {
                        if matches!(a, Expr::Placeholder { .. }) { l.clone() } else { self.gen_expr(a) }
                    }).collect();
                    let callee_str = self.gen_expr(callee);
                    format!("{}({})", callee_str, mapped_args.join(", "))
                } else {
                    let callee_str = self.gen_expr(callee);
                    let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                    if args_str.is_empty() {
                        format!("{}({})", callee_str, l)
                    } else {
                        format!("{}({}, {})", callee_str, l, args_str.join(", "))
                    }
                }
            }
            _ => format!("{}({})", self.gen_expr(right), l),
        }
    }
}
