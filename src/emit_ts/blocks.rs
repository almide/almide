use crate::ast::*;
use super::TsEmitter;

impl TsEmitter {
    pub(crate) fn gen_match(&self, subject: &Expr, arms: &[MatchArm]) -> String {
        let subj = self.gen_expr(subject);
        let tmp = "__m";

        let err_arm = arms.iter().find(|a| matches!(&a.pattern, Pattern::Err { .. }));

        if let Some(err_arm) = err_arm {
            let ok_arms: Vec<&MatchArm> = arms.iter().filter(|a| !matches!(&a.pattern, Pattern::Err { .. })).collect();
            let err_body = if Self::needs_iife(&err_arm.body) {
                format!("(() => {})()", self.gen_expr(&err_arm.body))
            } else {
                self.gen_expr(&err_arm.body)
            };
            let err_binding = if let Pattern::Err { inner } = &err_arm.pattern {
                if let Pattern::Ident { name } = inner.as_ref() {
                    Some(name.clone())
                } else { None }
            } else { None };

            let catch_return = if let Some(ref binding) = err_binding {
                format!("const {} = __e instanceof Error ? __e.message : String(__e); return {};", binding, err_body)
            } else {
                format!("return {};", err_body)
            };

            let mut lines = vec![format!("(() => {{ let {}; try {{ {} = {}; }} catch (__e) {{ {} }}", tmp, tmp, subj, catch_return)];
            for arm in &ok_arms {
                self.emit_match_arm(&mut lines, tmp, arm);
            }
            lines.push("  throw new Error(\"match exhausted\");".to_string());
            lines.push("})()".to_string());
            return lines.join("\n");
        }

        let mut lines = vec![format!("(({}) => {{", tmp)];
        for arm in arms {
            self.emit_match_arm(&mut lines, tmp, arm);
        }
        lines.push("  throw new Error(\"match exhausted\");".to_string());
        lines.push(format!("}})({})", subj));
        lines.join("\n")
    }

    fn emit_match_arm(&self, lines: &mut Vec<String>, tmp: &str, arm: &MatchArm) {
        let (cond, bindings) = self.gen_pattern_cond(tmp, &arm.pattern);
        let bind_str: String = bindings.iter()
            .map(|b| format!("    const {} = {};", b.0, b.1))
            .collect::<Vec<_>>()
            .join("\n");
        let body_str = if Self::needs_iife(&arm.body) {
            format!("(() => {})()", self.gen_expr(&arm.body))
        } else {
            self.gen_expr(&arm.body)
        };

        if let Some(guard) = &arm.guard {
            let guard_str = self.gen_expr(guard);
            if !bind_str.is_empty() {
                lines.push(format!("  {{ {}\n    if ({} && {}) return {}; }}", bind_str, cond, guard_str, body_str));
            } else {
                lines.push(format!("  if ({} && {}) return {};", cond, guard_str, body_str));
            }
        } else if !bind_str.is_empty() {
            lines.push(format!("  if ({}) {{ {}\n    return {}; }}", cond, bind_str, body_str));
        } else {
            lines.push(format!("  if ({}) return {};", cond, body_str));
        }
    }

    fn gen_pattern_cond(&self, expr: &str, pattern: &Pattern) -> (String, Vec<(String, String)>) {
        match pattern {
            Pattern::Wildcard => ("true".to_string(), vec![]),
            Pattern::Ident { name } => ("true".to_string(), vec![(name.clone(), expr.to_string())]),
            Pattern::Literal { value } => {
                (format!("{} === {}", expr, self.gen_expr(value)), vec![])
            }
            Pattern::None => (format!("{} === null", expr), vec![]),
            Pattern::Some { inner } => {
                let (inner_cond, bindings) = self.gen_pattern_cond(expr, inner);
                let cond = if inner_cond == "true" {
                    format!("{} !== null", expr)
                } else {
                    format!("{} !== null && {}", expr, inner_cond)
                };
                (cond, bindings)
            }
            Pattern::Ok { inner } => self.gen_pattern_cond(expr, inner),
            Pattern::Err { .. } => ("false".to_string(), vec![]),
            Pattern::Constructor { name, args } => {
                if args.is_empty() {
                    (format!("{}?.tag === {}", expr, Self::json_string(name)), vec![])
                } else {
                    let mut conds = vec![format!("{}?.tag === {}", expr, Self::json_string(name))];
                    let mut bindings = vec![];
                    for (i, arg) in args.iter().enumerate() {
                        let sub_expr = format!("{}._{}", expr, i);
                        let (sub_cond, sub_bindings) = self.gen_pattern_cond(&sub_expr, arg);
                        if sub_cond != "true" {
                            conds.push(sub_cond);
                        }
                        bindings.extend(sub_bindings);
                    }
                    (conds.join(" && "), bindings)
                }
            }
            Pattern::RecordPattern { name, fields } => {
                let mut conds = vec![format!("{}?.tag === {}", expr, Self::json_string(name))];
                let mut bindings = vec![];
                for f in fields {
                    let field_expr = format!("{}.{}", expr, f.name);
                    if let Some(p) = &f.pattern {
                        let (sub_cond, sub_bindings) = self.gen_pattern_cond(&field_expr, p);
                        if sub_cond != "true" {
                            conds.push(sub_cond);
                        }
                        bindings.extend(sub_bindings);
                    } else {
                        bindings.push((f.name.clone(), field_expr));
                    }
                }
                (conds.join(" && "), bindings)
            }
        }
    }

    pub(crate) fn gen_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>, indent: usize) -> String {
        let ind = "  ".repeat(indent + 1);
        let mut lines = Vec::new();

        // Detect let-match inlining pattern for Result erasure
        if let Some(fe) = final_expr {
            if let Expr::Match { subject, arms } = fe {
                if let Expr::Ident { name: subj_name } = subject.as_ref() {
                    if !stmts.is_empty() {
                        if let Stmt::Let { name: last_name, value, .. } = &stmts[stmts.len() - 1] {
                            if last_name == subj_name && arms.iter().any(|a| matches!(&a.pattern, Pattern::Err { .. })) {
                                for i in 0..stmts.len() - 1 {
                                    lines.push(format!("{}{}", ind, self.gen_stmt(&stmts[i])));
                                }
                                // Inline value into match subject
                                let inlined_match = self.gen_match(value, arms);
                                lines.push(format!("{}return {};", ind, inlined_match));
                                return format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent));
                            }
                        }
                    }
                }
            }
        }

        for stmt in stmts {
            lines.push(format!("{}{}", ind, self.gen_stmt(stmt)));
        }
        if let Some(fe) = final_expr {
            match fe {
                Expr::DoBlock { stmts: ds, expr: de } => {
                    lines.push(format!("{}{}", ind, self.gen_do_block(ds, de.as_deref(), indent + 1)));
                }
                _ => {
                    lines.push(format!("{}return {};", ind, self.gen_expr(fe)));
                }
            }
        }
        format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
    }

    pub(crate) fn gen_do_block(&self, stmts: &[Stmt], final_expr: Option<&Expr>, indent: usize) -> String {
        let has_guard = stmts.iter().any(|s| matches!(s, Stmt::Guard { .. }));
        let ind = "  ".repeat(indent + 1);
        let mut lines = Vec::new();

        for stmt in stmts {
            if has_guard {
                if let Stmt::Guard { cond, else_ } = stmt {
                    let c = self.gen_expr(cond);
                    if Self::is_unit(else_) {
                        lines.push(format!("{}if (!({})) {{ break; }}", ind, c));
                    } else {
                        lines.push(format!("{}if (!({})) {{ return {}; }}", ind, c, self.gen_expr(else_)));
                    }
                    continue;
                }
            }
            lines.push(format!("{}{}", ind, self.gen_stmt(stmt)));
        }

        if has_guard {
            if let Some(fe) = final_expr {
                lines.push(format!("{}{};", ind, self.gen_expr(fe)));
            }
            format!("while (true) {{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
        } else {
            if let Some(fe) = final_expr {
                lines.push(format!("{}return {};", ind, self.gen_expr(fe)));
            }
            format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
        }
    }

    pub(crate) fn gen_stmt(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let { name, value, .. } => {
                format!("const {} = {};", Self::sanitize(name), self.gen_expr(value))
            }
            Stmt::LetDestructure { fields, value } => {
                format!("const {{ {} }} = {};", fields.join(", "), self.gen_expr(value))
            }
            Stmt::Var { name, value, .. } => {
                format!("let {} = {};", Self::sanitize(name), self.gen_expr(value))
            }
            Stmt::Assign { name, value } => {
                format!("{} = {};", Self::sanitize(name), self.gen_expr(value))
            }
            Stmt::Guard { cond, else_ } => {
                let c = self.gen_expr(cond);
                self.gen_guard_stmt(&c, else_)
            }
            Stmt::Expr { expr } => {
                format!("{};", self.gen_expr(expr))
            }
        }
    }

    fn gen_guard_stmt(&self, cond: &str, else_: &Expr) -> String {
        match else_ {
            Expr::Block { stmts, expr } | Expr::DoBlock { stmts, expr } => {
                let body_stmts: Vec<String> = stmts.iter()
                    .map(|s| format!("  {}", self.gen_stmt(s)))
                    .collect();
                let final_part = expr.as_ref()
                    .map(|e| format!("  return {};", self.gen_expr(e)))
                    .unwrap_or_default();
                let body = [body_stmts.join("\n"), final_part]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("if (!({})) {{\n{}\n}}", cond, body)
            }
            _ => format!("if (!({})) {{ return {}; }}", cond, self.gen_expr(else_)),
        }
    }
}
