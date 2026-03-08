use crate::ast::*;
use super::Emitter;

impl Emitter {
    fn has_string_literal_in_option_pattern(arms: &[MatchArm]) -> bool {
        fn check_pattern(pat: &Pattern) -> bool {
            match pat {
                Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => {
                    check_pattern(inner)
                }
                Pattern::Literal { value } => matches!(&**value, Expr::String { .. }),
                _ => false,
            }
        }
        arms.iter().any(|arm| check_pattern(&arm.pattern))
    }

    pub(crate) fn gen_match(&self, subject: &Expr, arms: &[MatchArm]) -> String {
        let subj = self.gen_expr(subject);
        let needs_deref = Self::has_string_literal_in_option_pattern(arms);
        let subj_expr = if needs_deref {
            format!("{}.as_deref()", subj)
        } else {
            subj
        };
        let mut lines = vec![format!("match {} {{", subj_expr)];
        for arm in arms {
            let pat = self.gen_pattern(&arm.pattern);
            let guard = arm.guard.as_ref().map(|g| format!(" if {}", self.gen_expr(g))).unwrap_or_default();
            let body = self.gen_expr(&arm.body);
            lines.push(format!("    {}{} => {{ {} }}", pat, guard, body));
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
                if args.is_empty() {
                    format!("{}()", name)
                } else {
                    let ps: Vec<String> = args.iter().map(|p| self.gen_pattern(p)).collect();
                    format!("{}({})", name, ps.join(", "))
                }
            }
            Pattern::RecordPattern { name, fields } => {
                let fs: Vec<String> = fields.iter().map(|f| {
                    if let Some(p) = &f.pattern {
                        format!("{}: {}", f.name, self.gen_pattern(p))
                    } else {
                        f.name.clone()
                    }
                }).collect();
                format!("{} {{ {} }}", name, fs.join(", "))
            }
        }
    }

    fn gen_pattern_literal(&self, expr: &Expr) -> String {
        match expr {
            Expr::String { value } => format!("\"{}\"", value),
            Expr::Int { raw, .. } => format!("{}i64", raw),
            Expr::Float { value } => value.to_string(),
            Expr::Bool { value } => value.to_string(),
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
        matches!(expr, Expr::Ok { expr } if matches!(expr.as_ref(), Expr::Unit))
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
                    Stmt::Guard { cond, else_ } => {
                        let c = self.gen_expr(cond);
                        if matches!(else_, Expr::Unit) || self.is_ok_unit(else_) {
                            lines.push(format!("    if !({}) {{ break; }}", c));
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
                let expr = final_expr.unwrap();
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
            Stmt::Let { name, value, .. } => {
                // Use gen_arg to clone Ident values, preventing move issues
                let val = match value {
                    Expr::If { cond, then, else_ } => {
                        let c = self.gen_expr(cond);
                        let t = self.gen_arg(then);
                        let e = self.gen_arg(else_);
                        format!("if {} {{ {} }} else {{ {} }}", c, t, e)
                    }
                    _ => self.gen_expr(value),
                };
                format!("let {} = {};", name, val)
            }
            Stmt::LetDestructure { fields, value } => {
                format!("let ({}) = {};", fields.join(", "), self.gen_expr(value))
            }
            Stmt::Var { name, value, .. } => {
                format!("let mut {} = {};", name, self.gen_expr(value))
            }
            Stmt::Assign { name, value } => {
                format!("{} = {};", name, self.gen_expr(value))
            }
            Stmt::Guard { cond, else_ } => {
                let c = self.gen_expr(cond);
                let e = self.gen_expr(else_);
                if e.contains("return ") {
                    format!("if !({}) {{ {}; }}", c, e)
                } else {
                    format!("if !({}) {{ return {}; }}", c, e)
                }
            }
            Stmt::Expr { expr } => {
                format!("{};", self.gen_expr(expr))
            }
        }
    }
}
