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
            Expr::TypeName { name, .. } => name.clone(),
            Expr::Unit { .. } => "undefined".to_string(),
            Expr::None { .. } => "null".to_string(),
            Expr::Some { expr, .. } => self.gen_expr(expr),
            Expr::Ok { expr, .. } => self.gen_expr(expr),
            Expr::Err { expr, .. } => self.gen_err(expr),
            Expr::List { elements, .. } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            Expr::Record { fields, .. } => { // name ignored in TS — records are plain objects
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ {} }}", fs.join(", "))
            }
            Expr::SpreadRecord { base, fields, .. } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ ...{}, {} }}", self.gen_expr(base), fs.join(", "))
            }
            Expr::Call { callee, args, .. } => self.gen_call(callee, args),
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
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
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
            Expr::Hole { .. } => if self.js_mode { "null /* hole */".to_string() } else { "null as any /* hole */".to_string() },
            Expr::Todo { message, .. } => format!("__throw({})", Self::json_string(message)),
            Expr::Placeholder { .. } => "__placeholder__".to_string(),
        }
    }

    pub(crate) fn gen_err(&self, expr: &Expr) -> String {
        match expr {
            Expr::Call { callee, args, .. } => {
                let callee_str = if let Expr::TypeName { name, .. } = callee.as_ref() {
                    Self::pascal_to_message(name)
                } else {
                    self.gen_expr(callee)
                };
                let arg = if !args.is_empty() { self.gen_expr(&args[0]) } else { "\"\"".to_string() };
                format!("__throw({} + \": \" + {})", Self::json_string(&callee_str), arg)
            }
            Expr::TypeName { name, .. } => {
                let msg = Self::pascal_to_message(name);
                format!("__throw({})", Self::json_string(&msg))
            }
            Expr::String { value, .. } => {
                format!("__throw({})", Self::json_string(value))
            }
            _ => {
                format!("__throw(String({}))", self.gen_expr(expr))
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

    fn resolve_ufcs_module(method: &str) -> Option<String> {
        // UFCS is only for stdlib modules, so always prefix with __
        crate::stdlib::resolve_ufcs_module(method).map(|m| format!("__almd_{}", m))
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

    pub(crate) fn gen_call(&self, callee: &Expr, args: &[Expr]) -> String {
        // UFCS: expr.method(args) => __module.method(expr, args)
        if let Expr::Member { object, field, .. } = callee {
            if let Expr::Ident { name, .. } = object.as_ref() {
                let is_user_module = self.user_modules.contains(&name.to_string());
                let is_module = is_user_module || crate::stdlib::is_stdlib_module(name);
                if !is_module {
                    // UFCS: non-module receiver
                    if let Some(module) = Self::resolve_ufcs_module(field) {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("{}.{}({})", module, Self::sanitize(field), all_args.join(", "));
                    }
                    // len/contains: try both, default to list for identifiers
                    if field == "len" || field == "contains" {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("__almd_list.{}({})", Self::sanitize(field), all_args.join(", "));
                    }
                }
            } else {
                // Non-ident object (e.g. call result, member chain)
                let is_module_chain = self.is_module_chain(object);

                if !is_module_chain {
                    if let Some(module) = Self::resolve_ufcs_module(field) {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("{}.{}({})", module, Self::sanitize(field), all_args.join(", "));
                    }
                    if field == "len" || field == "contains" {
                        let obj_str = self.gen_expr(object);
                        let mut all_args = vec![obj_str];
                        all_args.extend(args.iter().map(|a| self.gen_expr(a)));
                        return format!("__almd_list.{}({})", Self::sanitize(field), all_args.join(", "));
                    }
                }
            }
        }

        let callee_str = self.gen_expr(callee);
        // Special case: assert_eq(x, err(e))
        if callee_str == "assert_eq" && args.len() == 2 {
            if let Expr::Err { expr: err_expr, .. } = &args[1] {
                return format!("__assert_throws(() => {}, {})", self.gen_expr(&args[0]), self.gen_err_message(err_expr));
            }
            if let Expr::Err { expr: err_expr, .. } = &args[0] {
                return format!("__assert_throws(() => {}, {})", self.gen_expr(&args[1]), self.gen_err_message(err_expr));
            }
        }
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        format!("{}({})", callee_str, args_str.join(", "))
    }

    pub(crate) fn gen_err_message(&self, expr: &Expr) -> String {
        match expr {
            Expr::String { value, .. } => Self::json_string(value),
            Expr::Call { callee, args, .. } if matches!(callee.as_ref(), Expr::TypeName { .. }) => {
                if let Expr::TypeName { name, .. } = callee.as_ref() {
                    let msg = Self::pascal_to_message(name);
                    format!("{} + \": \" + {}", Self::json_string(&msg), self.gen_expr(&args[0]))
                } else {
                    format!("String({})", self.gen_expr(expr))
                }
            }
            Expr::TypeName { name, .. } => Self::json_string(&Self::pascal_to_message(name)),
            _ => format!("String({})", self.gen_expr(expr)),
        }
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
