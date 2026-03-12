use crate::ast::*;
use super::Emitter;

impl Emitter {
    /// Check if arms contain string literals inside Option/Result wrappers (some("x"), ok("x"))
    fn has_string_in_option_pattern(arms: &[MatchArm]) -> bool {
        fn check_inner(pat: &Pattern) -> bool {
            match pat {
                Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => {
                    matches!(&**inner, Pattern::Literal { value } if matches!(&**value, Expr::String { .. }))
                        || check_inner(inner)
                }
                _ => false,
            }
        }
        arms.iter().any(|arm| check_inner(&arm.pattern))
    }

    /// Check if arms contain bare string literal patterns ("hello", "/")
    fn has_bare_string_literal(arms: &[MatchArm]) -> bool {
        arms.iter().any(|arm| matches!(&arm.pattern, Pattern::Literal { value } if matches!(&**value, Expr::String { .. })))
    }

    /// Check if arms contain ok/err patterns (indicating Result matching)
    fn has_result_patterns(arms: &[MatchArm]) -> bool {
        arms.iter().any(|arm| matches!(&arm.pattern, Pattern::Ok { .. } | Pattern::Err { .. }))
    }

    pub(crate) fn gen_match(&self, subject: &Expr, arms: &[MatchArm]) -> String {
        // Suppress auto-? when matching on ok/err (caller handles Result explicitly)
        let prev = self.skip_auto_q.get();
        if Self::has_result_patterns(arms) {
            self.skip_auto_q.set(true);
        }
        let subj = self.gen_expr(subject);
        self.skip_auto_q.set(prev);
        // Check if the subject is a borrowed param (already &str)
        let subj_is_borrowed = matches!(subject, Expr::Ident { name, .. } if self.borrowed_params.contains_key(name));
        let subj_expr = if Self::has_string_in_option_pattern(arms) {
            // match option { some("x") => ... } needs as_deref()
            format!("{}.as_deref()", subj)
        } else if Self::has_bare_string_literal(arms) && !subj_is_borrowed {
            // match string { "/" => ... } needs as_str() (but not if already &str)
            format!("{}.as_str()", subj)
        } else {
            subj
        };
        let mut lines = vec![format!("match {} {{", subj_expr)];
        for arm in arms {
            let pat = self.gen_pattern(&arm.pattern);
            let guard = arm.guard.as_ref().map(|g| format!(" if {}", self.gen_expr(g))).unwrap_or_default();
            let body = self.gen_expr(&arm.body);
            let derefs = self.collect_box_derefs(&arm.pattern);
            if derefs.is_empty() {
                lines.push(format!("    {}{} => {{ {} }}", pat, guard, body));
            } else {
                let deref_str = derefs.join(" ");
                lines.push(format!("    {}{} => {{ {} {} }}", pat, guard, deref_str, body));
            }
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    pub(crate) fn gen_pattern(&self, pat: &Pattern) -> String {
        match pat {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Ident { name } => name.clone(),
            Pattern::Literal { value } => self.gen_pattern_literal(value),
            Pattern::None => "None".to_string(),
            Pattern::Some { inner } => format!("Some({})", self.gen_pattern(inner)),
            Pattern::Ok { inner } => format!("Ok({})", self.gen_pattern(inner)),
            Pattern::Err { inner } => format!("Err({})", self.gen_pattern(inner)),
            Pattern::Constructor { name, args } => {
                // For generic variant constructors, qualify with enum name
                let qualified = if let Some(enum_name) = self.generic_variant_constructors.get(name) {
                    format!("{}::{}", enum_name, name)
                } else {
                    name.clone()
                };
                if args.is_empty() {
                    qualified
                } else {
                    let ps: Vec<String> = args.iter().enumerate().map(|(i, p)| {
                        let inner = self.gen_pattern(p);
                        // For boxed recursive fields, rename binding so we can auto-deref later
                        if self.boxed_variant_args.contains(&(name.clone(), i)) {
                            if let Pattern::Ident { .. } = p {
                                format!("__boxed_{}", inner)
                            } else {
                                inner // non-ident patterns (wildcard etc) don't need deref
                            }
                        } else {
                            inner
                        }
                    }).collect();
                    format!("{}({})", qualified, ps.join(", "))
                }
            }
            Pattern::Tuple { elements } => {
                let ps: Vec<String> = elements.iter().map(|p| self.gen_pattern(p)).collect();
                format!("({})", ps.join(", "))
            }
            Pattern::RecordPattern { name, fields, rest } => {
                let mut fs: Vec<String> = fields.iter().map(|f| {
                    if let Some(p) = &f.pattern {
                        format!("{}: {}", f.name, self.gen_pattern(p))
                    } else {
                        f.name.clone()
                    }
                }).collect();
                if *rest { fs.push("..".to_string()); }
                // For generic variant constructors, qualify with enum name
                let qualified = if let Some(enum_name) = self.generic_variant_constructors.get(name) {
                    format!("{}::{}", enum_name, name)
                } else {
                    name.clone()
                };
                format!("{} {{ {} }}", qualified, fs.join(", "))
            }
        }
    }

    /// Collect auto-deref let bindings needed for boxed recursive variant fields.
    fn collect_box_derefs(&self, pat: &Pattern) -> Vec<String> {
        match pat {
            Pattern::Constructor { name, args } => {
                let mut derefs = Vec::new();
                for (i, p) in args.iter().enumerate() {
                    if self.boxed_variant_args.contains(&(name.clone(), i)) {
                        if let Pattern::Ident { name: var_name } = p {
                            derefs.push(format!("let {} = *__boxed_{};", var_name, var_name));
                        }
                    }
                    // Recurse into nested patterns
                    derefs.extend(self.collect_box_derefs(p));
                }
                derefs
            }
            _ => Vec::new(),
        }
    }

    fn gen_pattern_literal(&self, expr: &Expr) -> String {
        match expr {
            Expr::String { value, .. } => format!("\"{}\"", value),
            Expr::Int { raw, .. } => format!("{}i64", raw),
            Expr::Float { value, .. } => value.to_string(),
            Expr::Bool { value, .. } => value.to_string(),
            _ => self.gen_expr(expr),
        }
    }

    pub(crate) fn gen_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>) -> String {
        let mut lines = vec!["{".to_string()];
        for stmt in stmts {
            lines.push(format!("    {}", self.gen_stmt(stmt)));
        }
        if let Some(expr) = final_expr {
            lines.push(format!("    {}", self.gen_expr(expr)));
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    fn is_ok_unit(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Ok { expr, .. } if matches!(expr.as_ref(), Expr::Unit { .. }))
    }

    pub(crate) fn gen_do_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>) -> String {
        let has_guard = stmts.iter().any(|s| matches!(s, Stmt::Guard { .. }));
        if has_guard {
            // Check if any guard uses ok(()) as else branch (indicates Result context)
            let has_ok_unit_guard = stmts.iter().any(|s| {
                if let Stmt::Guard { else_, .. } = s { self.is_ok_unit(else_) } else { false }
            });
            let mut lines = vec!["{ loop {".to_string()];
            for stmt in stmts {
                match stmt {
                    Stmt::Guard { cond, else_, .. } => {
                        let c = self.gen_expr(cond);
                        if matches!(else_, Expr::Unit { .. }) || self.is_ok_unit(else_) || matches!(else_, Expr::Break { .. }) {
                            lines.push(format!("    if !({}) {{ break; }}", c));
                        } else if matches!(else_, Expr::Continue { .. }) {
                            lines.push(format!("    if !({}) {{ continue; }}", c));
                        } else {
                            let e = self.gen_expr(else_);
                            if e.contains("return ") {
                                lines.push(format!("    if !({}) {{ {}; }}", c, e));
                            } else {
                                lines.push(format!("    if !({}) {{ return {}; }}", c, e));
                            }
                        }
                    }
                    _ => lines.push(format!("    {}", self.gen_stmt(stmt))),
                }
            }
            if let Some(expr) = final_expr {
                lines.push(format!("    {};", self.gen_expr(expr)));
            }
            lines.push("}".to_string());
            // After the loop, provide the appropriate trailing value
            if has_ok_unit_guard && self.in_effect {
                lines.push("Ok::<(), String>(()) }".to_string());
            } else {
                lines.push("}".to_string());
            }
            lines.join("\n")
        } else {
            // In a do block inside a Result-returning function, enable auto-unwrap
            let prev = self.in_do_block.get();
            if self.in_effect {
                self.in_do_block.set(true);
            }
            // Wrap the final expression in Ok() if we're in effect context
            let result = if self.in_effect && final_expr.is_some() {
                let expr = final_expr.expect("guarded by is_some()");
                let inner = self.gen_expr(expr);
                let mut lines = vec!["{".to_string()];
                for stmt in stmts {
                    lines.push(format!("    {}", self.gen_stmt(stmt)));
                }
                lines.push(format!("    Ok({})", inner));
                lines.push("}".to_string());
                lines.join("\n")
            } else {
                self.gen_block(stmts, final_expr)
            };
            self.in_do_block.set(prev);
            result
        }
    }

    pub(crate) fn gen_stmt(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let { name, value, ty, .. } => {
                // Use gen_arg to clone Ident values, preventing move issues
                let val = match value {
                    Expr::If { cond, then, else_, .. } => {
                        let c = self.gen_expr(cond);
                        let t = self.gen_arg(then);
                        let e = self.gen_arg(else_);
                        format!("if {} {{ {} }} else {{ {} }}", c, t, e)
                    }
                    _ => self.gen_expr(value),
                };
                // Emit type annotation when present (Rust needs it for Result, Option, empty collections, etc.)
                if let Some(t) = ty {
                    let rust_ty = self.gen_type(t);
                    return format!("let {}: {} = {};", name, rust_ty, val);
                }
                format!("let {} = {};", name, val)
            }
            Stmt::LetDestructure { pattern, value, .. } => {
                // Record destructure: emit field-access bindings (Rust has no anonymous record destructuring)
                if let Pattern::RecordPattern { fields, .. } = pattern {
                    let val = self.gen_expr(value);
                    let tmp = "__ds";
                    let mut lines = vec![format!("let {} = {};", tmp, val)];
                    for f in fields {
                        lines.push(format!("let {} = {}.{}.clone();", f.name, tmp, f.name));
                    }
                    lines.join("\n    ")
                } else {
                    format!("let {} = {};", self.gen_pattern(pattern), self.gen_expr(value))
                }
            }
            Stmt::Var { name, value, .. } => {
                format!("let mut {} = {};", name, self.gen_expr(value))
            }
            Stmt::Assign { name, value, .. } => {
                // Optimize: s = s ++ expr → s.almide_push_concat(expr)
                // Avoids clone + alloc; works for both String and Vec via trait dispatch
                if let Expr::Binary { op, left, right, .. } = value {
                    if op == "++" {
                        if let Expr::Ident { name: ref lname, .. } = **left {
                            if lname == name {
                                let r = self.gen_expr(right);
                                return format!("{}.almide_push_concat({});", name, r);
                            }
                        }
                    }
                }
                format!("{} = {};", name, self.gen_expr(value))
            }
            Stmt::IndexAssign { target, index, value, .. } => {
                let idx = self.gen_expr(index);
                let val = self.gen_expr(value);
                format!("{}[{} as usize] = {};", target, idx, val)
            }
            Stmt::FieldAssign { target, field, value, .. } => {
                let val = self.gen_expr(value);
                format!("{}.{} = {};", target, field, val)
            }
            Stmt::Guard { cond, else_, .. } => {
                let c = self.gen_expr(cond);
                if matches!(else_, Expr::Break { .. }) {
                    format!("if !({}) {{ break; }}", c)
                } else if matches!(else_, Expr::Continue { .. }) {
                    format!("if !({}) {{ continue; }}", c)
                } else {
                    let e = self.gen_expr(else_);
                    if e.contains("return ") {
                        format!("if !({}) {{ {}; }}", c, e)
                    } else {
                        format!("if !({}) {{ return {}; }}", c, e)
                    }
                }
            }
            Stmt::Expr { expr, .. } => {
                format!("{};", self.gen_expr(expr))
            }
            Stmt::Comment { text } => {
                text.clone()
            }
        }
    }
}
