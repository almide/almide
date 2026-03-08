use crate::ast::*;
use super::Emitter;

impl Emitter {
    pub(crate) fn gen_expr(&self, expr: &Expr) -> String {
        match expr {
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<u128>() {
                    if n > i64::MAX as u128 {
                        // Wrap to i64 — Almide Int is i64, large literals wrap automatically
                        format!("{}i64", n as i64)
                    } else {
                        format!("{}i64", raw)
                    }
                } else {
                    raw.clone()
                }
            }
            Expr::Float { value } => format!("{:?}f64", value),
            Expr::String { value } => format!("{:?}.to_string()", value),
            Expr::InterpolatedString { value } => {
                // Convert ${expr} to Rust format!("{}", expr) style
                let mut fmt = String::new();
                let mut args = Vec::new();
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
                        fmt.push_str("{}");
                        // Parse the interpolated expression and re-emit as Rust
                        let tokens = crate::lexer::Lexer::tokenize(&expr_str);
                        let mut parser = crate::parser::Parser::new(tokens);
                        if let Ok(parsed_expr) = parser.parse_single_expr() {
                            args.push(self.gen_expr(&parsed_expr));
                        } else {
                            args.push(expr_str);
                        }
                    } else if c == '{' {
                        fmt.push_str("{{");
                    } else if c == '}' {
                        fmt.push_str("}}");
                    } else {
                        fmt.push(c);
                    }
                }
                if args.is_empty() {
                    format!("\"{}\".to_string()", fmt)
                } else {
                    format!("format!(\"{}\", {})", fmt, args.join(", "))
                }
            }
            Expr::Bool { value } => format!("{}", value),
            Expr::Ident { name } => name.replace('?', "_qm_"),
            Expr::TypeName { name } => name.clone(),
            Expr::Unit => "()".to_string(),
            Expr::None => "None".to_string(),
            Expr::Some { expr } => format!("Some({})", self.gen_expr(expr)),
            Expr::Ok { expr } => {
                if self.in_do_block.get() {
                    // In do blocks, ok(expr) just unwraps to expr (since do auto-wraps in Ok)
                    self.gen_expr(expr)
                } else if self.in_effect {
                    format!("Ok({})", self.gen_expr(expr))
                } else if matches!(expr.as_ref(), Expr::Unit) {
                    "()".to_string()
                } else {
                    format!("Ok({})", self.gen_expr(expr))
                }
            }
            Expr::Err { expr } => {
                let msg = self.gen_expr(expr);
                if self.in_effect && !self.in_test && !self.in_do_block.get() {
                    format!("return Err({}.to_string())", msg)
                } else {
                    format!("Err({}.to_string())", msg)
                }
            }

            Expr::List { elements } => {
                let elems: Vec<String> = elements.iter().map(|e| self.gen_expr(e)).collect();
                format!("vec![{}]", elems.join(", "))
            }
            Expr::Record { fields } => {
                let fs: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, self.gen_expr(&f.value))).collect();
                format!("{{ {} }}", fs.join(", "))
            }

            Expr::Binary { op, left, right } => self.gen_binary(op, left, right),
            Expr::Unary { op, operand } => {
                let o = self.gen_expr(operand);
                match op.as_str() {
                    "not" => format!("!({})", o),
                    _ => format!("{}{}", op, o),
                }
            }

            Expr::If { cond, then, else_ } => {
                let c = self.gen_expr(cond);
                let t = self.gen_expr(then);
                let e = self.gen_expr(else_);
                format!("if {} {{ {} }} else {{ {} }}", c, t, e)
            }

            Expr::Call { callee, args } => self.gen_call(callee, args),

            Expr::Member { object, field } => {
                let obj = self.gen_expr(object);
                format!("{}.{}", obj, field)
            }

            Expr::Pipe { left, right } => {
                let l = self.gen_expr(left);
                match right.as_ref() {
                    Expr::Call { callee, args } => {
                        // Reconstruct as a full call with pipe-left as first arg
                        let mut all_args = Vec::new();
                        all_args.push(left.as_ref().clone());
                        all_args.extend(args.iter().cloned());
                        let full_call = Expr::Call {
                            callee: callee.clone(),
                            args: all_args,
                        };
                        self.gen_expr(&full_call)
                    }
                    _ => {
                        let r = self.gen_expr(right);
                        format!("{}({})", r, l)
                    }
                }
            }

            Expr::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                let b = self.gen_expr(body);
                format!("|{}| {{ {} }}", ps.join(", "), b)
            }

            Expr::Match { subject, arms } => self.gen_match(subject, arms),

            Expr::Block { stmts, expr } => self.gen_block(stmts, expr.as_deref()),
            Expr::DoBlock { stmts, expr } => self.gen_do_block(stmts, expr.as_deref()),
            Expr::ForIn { var, iterable, body } => {
                let iter_str = self.gen_expr(iterable);
                let stmts_str: Vec<String> = body.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                format!("for {var} in ({iter_str}).clone() {{\n{}\n}}", stmts_str.join("\n"))
            }

            Expr::Paren { expr } => format!("({})", self.gen_expr(expr)),
            Expr::Try { expr } => {
                // In effect fn: use ?, otherwise just eval
                if self.in_effect {
                    format!("({}?)", self.gen_expr(expr))
                } else {
                    self.gen_expr(expr)
                }
            }
            Expr::Hole => "todo!()".to_string(),
            Expr::Todo { message } => format!("todo!(\"{}\")", message),
            Expr::Placeholder => "_".to_string(),

            _ => format!("todo!(/* unsupported */)")
        }
    }

    /// Generate expression in u64 wrapping context (for hash/BigInt arithmetic)
    /// Almide's % 2^64 pattern maps to u64 wrapping arithmetic
    pub(crate) fn gen_expr_u64_wrapping(&self, expr: &Expr) -> String {
        match expr {
            Expr::Binary { op, left, right } if op == "^" || op == "*" || op == "%" => {
                // Check if % 2^64 — this is a no-op in u64 arithmetic
                if op == "%" && self.is_pow2_64(right) {
                    return self.gen_expr_u64_wrapping(left);
                }
                let l = self.gen_expr_u64_wrapping(left);
                let r = self.gen_expr_u64_wrapping(right);
                match op.as_str() {
                    "*" => format!("(({}).wrapping_mul({}))", l, r),
                    "^" => format!("(({}) ^ ({}))", l, r),
                    "%" => format!("(({}) % ({}))", l, r),
                    _ => format!("(({}) {} ({}))", l, op, r),
                }
            }
            Expr::Paren { expr: inner } => self.gen_expr_u64_wrapping(inner),
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<u128>() {
                    if n > u64::MAX as u128 {
                        format!("{}u64", n % (u64::MAX as u128 + 1))
                    } else {
                        format!("{}u64", raw)
                    }
                } else {
                    raw.clone()
                }
            }
            _ => {
                let e = self.gen_expr(expr);
                format!("(({}) as u64)", e)
            }
        }
    }

    pub(crate) fn is_pow2_64(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Int { raw, .. } => raw == "18446744073709551616",
            Expr::Paren { expr: inner } => self.is_pow2_64(inner),
            _ => false,
        }
    }

    pub(crate) fn is_bigint_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Int { raw, .. } => {
                if let Ok(n) = raw.parse::<u128>() { n > i64::MAX as u128 } else { false }
            }
            Expr::Binary { op, left, right } if op == "^" || op == "*" || op == "%" => {
                self.is_bigint_expr(left) || self.is_bigint_expr(right)
            }
            Expr::Paren { expr: inner } => self.is_bigint_expr(inner),
            _ => false,
        }
    }

    pub(crate) fn gen_binary(&self, op: &str, left: &Expr, right: &Expr) -> String {
        match op {
            "++" => {
                let l = self.gen_arg(left);
                let r = self.gen_arg(right);
                format!("AlmideConcat::concat({}, {})", l, r)
            }
            "^" | "%" => {
                let left_big = self.is_bigint_expr(left);
                let right_big = self.is_bigint_expr(right);
                let needs_wrapping = left_big || right_big;
                if needs_wrapping {
                    if op == "%" && self.is_pow2_64(right) {
                        let inner = self.gen_expr_u64_wrapping(left);
                        return format!("(({}) as i64)", inner);
                    }
                    let l = self.gen_expr_u64_wrapping(left);
                    let r = self.gen_expr_u64_wrapping(right);
                    format!("(({} {} {}) as i64)", l, op, r)
                } else {
                    let l = self.gen_expr(left);
                    let r = self.gen_expr(right);
                    format!("({} {} {})", l, op, r)
                }
            }
            "*" => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                if self.is_bigint_expr(left) || self.is_bigint_expr(right) {
                    format!("(({}).wrapping_mul({}))", l, r)
                } else {
                    format!("({} * {})", l, r)
                }
            }
            "==" => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                format!("almide_eq!({}, {})", l, r)
            }
            "!=" => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                format!("almide_ne!({}, {})", l, r)
            }
            "and" => { let l = self.gen_expr(left); let r = self.gen_expr(right); format!("({} && {})", l, r) }
            "or" => { let l = self.gen_expr(left); let r = self.gen_expr(right); format!("({} || {})", l, r) }
            _ => { let l = self.gen_expr(left); let r = self.gen_expr(right); format!("({} {} {})", l, op, r) }
        }
    }
}
