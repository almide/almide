use crate::ir::*;
use super::TsEmitter;

impl TsEmitter {
    pub(crate) fn gen_ir_stmt(&self, stmt: &IrStmt) -> String {
        match &stmt.kind {
            IrStmtKind::Bind { var, mutability, value, .. } => {
                let name = Self::sanitize(&self.ir_var_table().get(*var).name);
                let val = self.gen_ir_expr(value);
                let is_call = matches!(&value.kind, IrExprKind::Call { .. });
                if self.in_test.get() && !self.in_effect.get() && is_call {
                    format!("var {}; try {{ {} = {}; }} catch (__e) {{ {} = new __Err(__e instanceof Error ? __e.message : String(__e)); }}", name, name, val, name)
                } else if *mutability == Mutability::Var {
                    format!("let {} = {};", name, val)
                } else {
                    format!("var {} = {};", name, val)
                }
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                format!("var {} = {};", self.gen_ir_destructure_pattern(pattern), self.gen_ir_expr(value))
            }
            IrStmtKind::Assign { var, value } => {
                let name = Self::sanitize(&self.ir_var_table().get(*var).name);
                format!("{} = {};", name, self.gen_ir_expr(value))
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                let name = Self::sanitize(&self.ir_var_table().get(*target).name);
                format!("{}[{}] = {};", name, self.gen_ir_expr(index), self.gen_ir_expr(value))
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let name = Self::sanitize(&self.ir_var_table().get(*target).name);
                format!("{}.{} = {};", name, field, self.gen_ir_expr(value))
            }
            IrStmtKind::Guard { cond, else_ } => {
                let c = self.gen_ir_expr(cond);
                self.gen_ir_guard(&c, else_)
            }
            IrStmtKind::Expr { expr } => {
                format!("{};", self.gen_ir_expr(expr))
            }
            IrStmtKind::Comment { text } => text.clone(),
        }
    }

    fn gen_ir_guard(&self, cond: &str, else_: &IrExpr) -> String {
        match &else_.kind {
            IrExprKind::Break => format!("if (!({})) {{ break; }}", cond),
            IrExprKind::Continue => format!("if (!({})) {{ continue; }}", cond),
            IrExprKind::ResultErr { expr } => {
                let msg = self.gen_ir_err_msg(expr);
                format!("if (!({})) {{ throw new Error({}); }}", cond, msg)
            }
            IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
                let body_stmts: Vec<String> = stmts.iter()
                    .map(|s| format!("  {}", self.gen_ir_stmt(s)))
                    .collect();
                let final_part = expr.as_ref()
                    .map(|e| format!("  return {};", self.gen_ir_expr(e)))
                    .unwrap_or_default();
                let body = [body_stmts.join("\n"), final_part]
                    .iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n");
                format!("if (!({})) {{\n{}\n}}", cond, body)
            }
            _ => format!("if (!({})) {{ return {}; }}", cond, self.gen_ir_expr(else_)),
        }
    }

    pub(crate) fn gen_ir_match(&self, subject: &IrExpr, arms: &[IrMatchArm]) -> String {
        let subj = self.gen_ir_expr(subject);
        let tmp = "__m";

        let err_arm = arms.iter().find(|a| matches!(&a.pattern, IrPattern::Err { .. }));

        if let Some(err_arm) = err_arm {
            let ok_arms: Vec<&IrMatchArm> = arms.iter().filter(|a| !matches!(&a.pattern, IrPattern::Err { .. })).collect();
            let err_body = self.gen_ir_expr_value(&err_arm.body);
            let err_binding = if let IrPattern::Err { inner } = &err_arm.pattern {
                if let IrPattern::Bind { var } = inner.as_ref() {
                    Some(self.ir_var_table().get(*var).name.clone())
                } else { None }
            } else { None };

            let catch_convert = format!("{} = new __Err(__e instanceof Error ? __e.message : String(__e));", tmp);
            let err_return = if let Some(ref binding) = err_binding {
                format!("const {} = {}.message; return {};", binding, tmp, err_body)
            } else {
                format!("return {};", err_body)
            };

            let type_ann = if self.js_mode { "" } else { ": any" };
            let mut lines = vec![format!(
                "(() => {{ let {}{}; try {{ {} = {}; }} catch (__e) {{ {} }} if ({} instanceof __Err) {{ {} }}",
                tmp, type_ann, tmp, subj, catch_convert, tmp, err_return
            )];
            for arm in &ok_arms {
                self.emit_ir_match_arm(&mut lines, tmp, arm);
            }
            if !ok_arms.last().map_or(false, |a| a.guard.is_none() && Self::ir_is_unconditional_pattern(&a.pattern)) {
                lines.push("  throw new Error(\"match exhausted\");".to_string());
            }
            lines.push("})()".to_string());
            return lines.join("\n");
        }

        let mut lines = vec![format!("(({}) => {{", tmp)];
        for arm in arms {
            self.emit_ir_match_arm(&mut lines, tmp, arm);
        }
        if !arms.last().map_or(false, |a| a.guard.is_none() && Self::ir_is_unconditional_pattern(&a.pattern)) {
            lines.push("  throw new Error(\"match exhausted\");".to_string());
        }
        lines.push(format!("}})({})", subj));
        lines.join("\n")
    }

    fn ir_is_unconditional_pattern(pat: &IrPattern) -> bool {
        matches!(pat, IrPattern::Wildcard | IrPattern::Bind { .. })
            || matches!(pat, IrPattern::Ok { inner } if Self::ir_is_unconditional_pattern(inner))
    }

    fn emit_ir_match_arm(&self, lines: &mut Vec<String>, tmp: &str, arm: &IrMatchArm) {
        let (cond, bindings) = self.gen_ir_pattern_cond(tmp, &arm.pattern);
        let bind_str: String = bindings.iter()
            .map(|b| format!("    const {} = {};", b.0, b.1))
            .collect::<Vec<_>>().join("\n");
        let body_str = self.gen_ir_expr_value(&arm.body);

        if let Some(guard) = &arm.guard {
            let guard_str = self.gen_ir_expr(guard);
            if !bind_str.is_empty() {
                lines.push(format!("  {{ {}\n    if ({} && {}) return {}; }}", bind_str, cond, guard_str, body_str));
            } else {
                lines.push(format!("  if ({} && {}) return {};", cond, guard_str, body_str));
            }
        } else if cond == "true" && bind_str.is_empty() {
            lines.push(format!("  return {};", body_str));
        } else if cond == "true" && !bind_str.is_empty() {
            lines.push(format!("  {{ {}\n    return {}; }}", bind_str, body_str));
        } else if !bind_str.is_empty() {
            lines.push(format!("  if ({}) {{ {}\n    return {}; }}", cond, bind_str, body_str));
        } else {
            lines.push(format!("  if ({}) return {};", cond, body_str));
        }
    }

    fn gen_ir_pattern_cond(&self, expr: &str, pattern: &IrPattern) -> (String, Vec<(String, String)>) {
        match pattern {
            IrPattern::Wildcard => ("true".to_string(), vec![]),
            IrPattern::Bind { var } => {
                let name = self.ir_var_table().get(*var).name.clone();
                ("true".to_string(), vec![(name, expr.to_string())])
            }
            IrPattern::Literal { expr: lit_expr } => {
                (format!("{} === {}", expr, self.gen_ir_expr(lit_expr)), vec![])
            }
            IrPattern::None => (format!("{} === null", expr), vec![]),
            IrPattern::Some { inner } => {
                let (inner_cond, bindings) = self.gen_ir_pattern_cond(expr, inner);
                let cond = if inner_cond == "true" {
                    format!("{} !== null", expr)
                } else {
                    format!("{} !== null && {}", expr, inner_cond)
                };
                (cond, bindings)
            }
            IrPattern::Ok { inner } => self.gen_ir_pattern_cond(expr, inner),
            IrPattern::Err { .. } => ("false".to_string(), vec![]),
            IrPattern::Constructor { name, args } => {
                if args.is_empty() {
                    (format!("{}?.tag === {}", expr, Self::json_string(name)), vec![])
                } else {
                    let mut conds = vec![format!("{}?.tag === {}", expr, Self::json_string(name))];
                    let mut bindings = vec![];
                    for (i, arg) in args.iter().enumerate() {
                        let sub_expr = format!("{}._{}", expr, i);
                        let (sub_cond, sub_bindings) = self.gen_ir_pattern_cond(&sub_expr, arg);
                        if sub_cond != "true" { conds.push(sub_cond); }
                        bindings.extend(sub_bindings);
                    }
                    (conds.join(" && "), bindings)
                }
            }
            IrPattern::Tuple { elements } => {
                let mut conds = vec![];
                let mut bindings = vec![];
                for (i, elem) in elements.iter().enumerate() {
                    let sub_expr = format!("{}[{}]", expr, i);
                    let (sub_cond, sub_bindings) = self.gen_ir_pattern_cond(&sub_expr, elem);
                    if sub_cond != "true" { conds.push(sub_cond); }
                    bindings.extend(sub_bindings);
                }
                let cond = if conds.is_empty() { "true".to_string() } else { conds.join(" && ") };
                (cond, bindings)
            }
            IrPattern::RecordPattern { name, fields, .. } => {
                let mut conds = vec![format!("{}?.tag === {}", expr, Self::json_string(name))];
                let mut bindings = vec![];
                for f in fields {
                    let field_expr = format!("{}.{}", expr, f.name);
                    if let Some(p) = &f.pattern {
                        let (sub_cond, sub_bindings) = self.gen_ir_pattern_cond(&field_expr, p);
                        if sub_cond != "true" { conds.push(sub_cond); }
                        bindings.extend(sub_bindings);
                    } else {
                        bindings.push((f.name.clone(), field_expr));
                    }
                }
                (conds.join(" && "), bindings)
            }
        }
    }

    pub(crate) fn gen_ir_block(&self, stmts: &[IrStmt], final_expr: Option<&IrExpr>, indent: usize) -> String {
        let ind = "  ".repeat(indent + 1);
        let mut lines = Vec::new();
        for stmt in stmts {
            lines.push(format!("{}{}", ind, self.gen_ir_stmt(stmt)));
        }
        if let Some(fe) = final_expr {
            match &fe.kind {
                IrExprKind::DoBlock { stmts: ds, expr: de } => {
                    lines.push(format!("{}{}", ind, self.gen_ir_do_block(ds, de.as_deref(), indent + 1)));
                }
                _ => {
                    lines.push(format!("{}return {};", ind, self.gen_ir_expr(fe)));
                }
            }
        }
        format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
    }

    pub(crate) fn gen_ir_do_block(&self, stmts: &[IrStmt], final_expr: Option<&IrExpr>, indent: usize) -> String {
        let has_guard = stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { .. }));
        let ind = "  ".repeat(indent + 1);
        let mut lines = Vec::new();

        for stmt in stmts {
            if has_guard {
                if let IrStmtKind::Guard { cond, else_ } = &stmt.kind {
                    let c = self.gen_ir_expr(cond);
                    if matches!(&else_.kind, IrExprKind::Unit | IrExprKind::ResultOk { .. } | IrExprKind::Break) {
                        lines.push(format!("{}if (!({})) {{ break; }}", ind, c));
                    } else if matches!(&else_.kind, IrExprKind::Continue) {
                        lines.push(format!("{}if (!({})) {{ continue; }}", ind, c));
                    } else {
                        lines.push(format!("{}if (!({})) {{ return {}; }}", ind, c, self.gen_ir_expr(else_)));
                    }
                    continue;
                }
            }
            lines.push(format!("{}{}", ind, self.gen_ir_stmt(stmt)));
            // Auto-propagate __Err in do blocks
            if let IrStmtKind::Bind { var, .. } = &stmt.kind {
                let san = Self::sanitize(&self.ir_var_table().get(*var).name);
                lines.push(format!("{}if ({} instanceof __Err) throw new Error({}.message);", ind, san, san));
            }
        }

        if has_guard {
            if let Some(fe) = final_expr {
                lines.push(format!("{}{};", ind, self.gen_ir_expr(fe)));
            }
            format!("while (true) {{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
        } else {
            if let Some(fe) = final_expr {
                lines.push(format!("{}return {};", ind, self.gen_ir_expr(fe)));
            }
            format!("{{\n{}\n{}}}", lines.join("\n"), "  ".repeat(indent))
        }
    }

    fn gen_ir_destructure_pattern(&self, pattern: &IrPattern) -> String {
        match pattern {
            IrPattern::Bind { var } => self.ir_var_table().get(*var).name.clone(),
            IrPattern::Wildcard => "_".to_string(),
            IrPattern::Tuple { elements } => {
                let ps: Vec<String> = elements.iter().map(|p| self.gen_ir_destructure_pattern(p)).collect();
                format!("[{}]", ps.join(", "))
            }
            IrPattern::RecordPattern { fields, .. } => {
                let fs: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                format!("{{ {} }}", fs.join(", "))
            }
            _ => "_".to_string(),
        }
    }
}
