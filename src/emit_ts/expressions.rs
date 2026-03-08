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
            Expr::Float { value } => format!("{}", value),
            Expr::String { value } => Self::json_string(value),
            Expr::InterpolatedString { value } => format!("`{}`", value),
            Expr::Bool { value } => format!("{}", value),
            Expr::Ident { name } => Self::sanitize(name),
            Expr::TypeName { name } => name.clone(),
            Expr::Unit => "undefined".to_string(),
            Expr::None => "null".to_string(),
            Expr::Some { expr } => self.gen_expr(expr),
            Expr::Ok { expr } => self.gen_expr(expr),
            Expr::Err { expr } => self.gen_err(expr),
            Expr::List { elements } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            Expr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ {} }}", fs.join(", "))
            }
            Expr::SpreadRecord { base, fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value)))
                    .collect();
                format!("{{ ...{}, {} }}", self.gen_expr(base), fs.join(", "))
            }
            Expr::Call { callee, args } => self.gen_call(callee, args),
            Expr::Member { object, field } => {
                let obj = self.gen_expr(object);
                format!("{}.{}", Self::map_module(&obj), Self::sanitize(field))
            }
            Expr::Pipe { left, right } => self.gen_pipe(left, right),
            Expr::If { cond, then, else_ } => {
                let t = if Self::needs_iife(then) {
                    format!("(() => {})()", self.gen_expr(then))
                } else {
                    self.gen_expr(then)
                };
                let e = if Self::needs_iife(else_) {
                    format!("(() => {})()", self.gen_expr(else_))
                } else {
                    self.gen_expr(else_)
                };
                format!("({} ? {} : {})", self.gen_expr(cond), t, e)
            }
            Expr::Match { subject, arms } => self.gen_match(subject, arms),
            Expr::Block { stmts, expr: final_expr } => {
                self.gen_block(stmts, final_expr.as_deref(), 0)
            }
            Expr::DoBlock { stmts, expr: final_expr } => {
                self.gen_do_block(stmts, final_expr.as_deref(), 0)
            }
            Expr::ForIn { var, iterable, body } => {
                let iter_str = self.gen_expr(iterable);
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                format!("for (const {} of {}) {{\n{}\n}}", Self::sanitize(var), iter_str, stmts_str.join("\n"))
            }
            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                format!("(({}) => {})", ps.join(", "), self.gen_expr(body))
            }
            Expr::Binary { op, left, right } => self.gen_binary(op, left, right),
            Expr::Unary { op, operand } => {
                if op == "not" {
                    format!("!({})", self.gen_expr(operand))
                } else {
                    format!("{}{}", op, self.gen_expr(operand))
                }
            }
            Expr::Paren { expr } => format!("({})", self.gen_expr(expr)),
            Expr::Try { expr } => self.gen_expr(expr),
            Expr::Await { expr } => format!("await {}", self.gen_expr(expr)),
            Expr::Hole => if self.js_mode { "null /* hole */".to_string() } else { "null as any /* hole */".to_string() },
            Expr::Todo { message } => format!("(() => {{ throw new Error({}); }})()", Self::json_string(message)),
            Expr::Placeholder => "__placeholder__".to_string(),
        }
    }

    pub(crate) fn gen_err(&self, expr: &Expr) -> String {
        match expr {
            Expr::Call { callee, args } => {
                let callee_str = if let Expr::TypeName { name } = callee.as_ref() {
                    Self::pascal_to_message(name)
                } else {
                    self.gen_expr(callee)
                };
                let arg = if !args.is_empty() { self.gen_expr(&args[0]) } else { "\"\"".to_string() };
                format!("(() => {{ throw new Error({} + \": \" + {}); }})()", Self::json_string(&callee_str), arg)
            }
            Expr::TypeName { name } => {
                let msg = Self::pascal_to_message(name);
                format!("(() => {{ throw new Error({}); }})()", Self::json_string(&msg))
            }
            Expr::String { value } => {
                format!("(() => {{ throw new Error({}); }})()", Self::json_string(value))
            }
            _ => {
                format!("(() => {{ throw new Error(String({})); }})()", self.gen_expr(expr))
            }
        }
    }

    fn resolve_ufcs_module(method: &str) -> Option<&'static str> {
        match method {
            // string methods
            "trim" | "split" | "join" | "pad_left" | "starts_with" | "starts_with_qm_"
            | "ends_with_qm_" | "slice" | "to_bytes" | "contains" | "to_upper" | "to_lower"
            | "to_int" | "replace" | "char_at" | "lines" => Some("__string"),
            // list methods
            "get" | "get_or" | "sort" | "reverse" | "each" | "map" | "filter" | "find" | "fold" | "any" | "all" => Some("__list"),
            // int methods
            "to_string" | "to_hex" => Some("__int"),
            // map methods
            "keys" | "values" | "entries" => Some("__map"),
            _ => None,
        }
    }

    pub(crate) fn gen_call(&self, callee: &Expr, args: &[Expr]) -> String {
        // UFCS: expr.method(args) => __module.method(expr, args)
        if let Expr::Member { object, field } = callee {
            if let Expr::Ident { name } = object.as_ref() {
                let is_module = matches!(name.as_str(), "string" | "list" | "int" | "float" | "fs" | "env" | "map");
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
                        return format!("__list.{}({})", Self::sanitize(field), all_args.join(", "));
                    }
                }
            } else {
                // Non-ident object (e.g. call result, member chain)
                let module_name = if let Expr::Member { object: inner_obj, .. } = object.as_ref() {
                    if let Expr::Ident { name } = inner_obj.as_ref() {
                        matches!(name.as_str(), "string" | "list" | "int" | "float" | "fs" | "env")
                    } else { false }
                } else { false };

                if !module_name {
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
                        return format!("__list.{}({})", Self::sanitize(field), all_args.join(", "));
                    }
                }
            }
        }

        let callee_str = self.gen_expr(callee);
        // Special case: assert_eq(x, err(e))
        if callee_str == "assert_eq" && args.len() == 2 {
            if let Expr::Err { expr: err_expr } = &args[1] {
                return format!("__assert_throws(() => {}, {})", self.gen_expr(&args[0]), self.gen_err_message(err_expr));
            }
            if let Expr::Err { expr: err_expr } = &args[0] {
                return format!("__assert_throws(() => {}, {})", self.gen_expr(&args[1]), self.gen_err_message(err_expr));
            }
        }
        let args_str: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
        format!("{}({})", callee_str, args_str.join(", "))
    }

    pub(crate) fn gen_err_message(&self, expr: &Expr) -> String {
        match expr {
            Expr::String { value } => Self::json_string(value),
            Expr::Call { callee, args } if matches!(callee.as_ref(), Expr::TypeName { .. }) => {
                if let Expr::TypeName { name } = callee.as_ref() {
                    let msg = Self::pascal_to_message(name);
                    format!("{} + \": \" + {}", Self::json_string(&msg), self.gen_expr(&args[0]))
                } else {
                    format!("String({})", self.gen_expr(expr))
                }
            }
            Expr::TypeName { name } => Self::json_string(&Self::pascal_to_message(name)),
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
            "^" if !has_float => format!("__bigop(\"^\", {}, {})", l, r),
            "*" if !has_float => format!("__bigop(\"*\", {}, {})", l, r),
            "%" if !has_float => format!("__bigop(\"%\", {}, {})", l, r),
            "/" if !has_float => format!("__div({}, {})", l, r),
            _ => format!("({} {} {})", l, op, r),
        }
    }

    /// Check if an expression involves Float values (heuristic for JS codegen).
    fn expr_has_float(expr: &Expr) -> bool {
        match expr {
            Expr::Float { .. } => true,
            Expr::Binary { left, right, .. } => {
                Self::expr_has_float(left) || Self::expr_has_float(right)
            }
            Expr::Paren { expr: inner } => Self::expr_has_float(inner),
            Expr::Call { callee, .. } => {
                // float.xxx() calls return Float
                if let Expr::Member { object, .. } = callee.as_ref() {
                    if let Expr::Ident { name } = object.as_ref() {
                        return name == "float";
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub(crate) fn gen_pipe(&self, left: &Expr, right: &Expr) -> String {
        let l = self.gen_expr(left);
        match right {
            Expr::Call { callee, args } => {
                let has_placeholder = args.iter().any(|a| matches!(a, Expr::Placeholder));
                if has_placeholder {
                    let mapped_args: Vec<String> = args.iter().map(|a| {
                        if matches!(a, Expr::Placeholder) { l.clone() } else { self.gen_expr(a) }
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
