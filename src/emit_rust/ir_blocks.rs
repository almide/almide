use almide::ir::*;
use super::Emitter;

impl Emitter {
    pub(crate) fn gen_ir_stmt(&self, stmt: &IrStmt) -> String {
        match &stmt.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => {
                let name = self.ir_var_table().get(*var).name.clone();
                let val = self.gen_ir_expr(value);
                let mut_kw = if *mutability == Mutability::Var { "let mut" } else { "let" };
                // Emit type annotation for Result, Option, empty collections
                let ty_str = self.ir_ty_annotation(ty, Some(value));
                if let Some(t) = ty_str {
                    format!("{} {}: {} = {};", mut_kw, name, t, val)
                } else {
                    format!("{} {} = {};", mut_kw, name, val)
                }
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                if let IrPattern::RecordPattern { fields, .. } = pattern {
                    let val = self.gen_ir_expr(value);
                    let tmp = "__ds";
                    let mut lines = vec![format!("let {} = {};", tmp, val)];
                    for f in fields {
                        lines.push(format!("let {} = {}.{}.clone();", f.name, tmp, f.name));
                    }
                    lines.join("\n    ")
                } else {
                    format!("let {} = {};", self.gen_ir_pattern(pattern), self.gen_ir_expr(value))
                }
            }
            IrStmtKind::Assign { var, value } => {
                let name = self.ir_var_table().get(*var).name.clone();
                // Optimize: s = s ++ expr → push/almide_push_concat
                if let IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } = &value.kind {
                    if let IrExprKind::Var { id } = &left.kind {
                        if *id == *var {
                            if let IrExprKind::List { elements } = &right.kind {
                                if elements.len() == 1 {
                                    let elem = self.gen_ir_arg(&elements[0]);
                                    return format!("{}.push({});", name, elem);
                                }
                            }
                            let r = self.gen_ir_expr(right);
                            return format!("{}.almide_push_concat({});", name, r);
                        }
                    }
                }
                format!("{} = {};", name, self.gen_ir_expr(value))
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                let name = self.ir_var_table().get(*target).name.clone();
                let idx = self.gen_ir_expr(index);
                let val = self.gen_ir_expr(value);
                let target_ty = &self.ir_var_table().get(*target).ty;
                if matches!(target_ty, almide::types::Ty::Map(_, _)) {
                    format!("{}.insert({}, {});", name, idx, val)
                } else if self.fast_mode {
                    format!("unsafe {{ *{}.get_unchecked_mut({} as usize) = {}; }}", name, idx, val)
                } else {
                    format!("{}[{} as usize] = {};", name, idx, val)
                }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let name = self.ir_var_table().get(*target).name.clone();
                let val = self.gen_ir_expr(value);
                format!("{}.{} = {};", name, field, val)
            }
            IrStmtKind::Guard { cond, else_ } => {
                let c = self.gen_ir_expr(cond);
                if matches!(&else_.kind, IrExprKind::Break) {
                    format!("if !({}) {{ break; }}", c)
                } else if matches!(&else_.kind, IrExprKind::Continue) {
                    format!("if !({}) {{ continue; }}", c)
                } else {
                    let e = self.gen_ir_expr(else_);
                    if e.contains("return ") {
                        format!("if !({}) {{ {}; }}", c, e)
                    } else {
                        format!("if !({}) {{ return {}; }}", c, e)
                    }
                }
            }
            IrStmtKind::Expr { expr } => {
                format!("{};", self.gen_ir_expr(expr))
            }
            IrStmtKind::Comment { text } => {
                text.clone()
            }
        }
    }

    pub(crate) fn gen_ir_match(&self, subject: &IrExpr, arms: &[IrMatchArm]) -> String {
        let has_result_patterns = arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::Ok { .. } | IrPattern::Err { .. }));
        // Suppress auto-? when matching on ok/err
        let prev = self.skip_auto_q.get();
        if has_result_patterns {
            self.skip_auto_q.set(true);
        }
        let subj = self.gen_ir_expr(subject);
        self.skip_auto_q.set(prev);
        // Check for string patterns
        let has_string_in_option = arms.iter().any(|arm| Self::ir_has_string_in_option_pattern(&arm.pattern));
        let has_bare_string = arms.iter().any(|arm| Self::ir_has_bare_string_literal(&arm.pattern));

        // Check if subject is a borrowed param
        let subj_is_borrowed = if let IrExprKind::Var { id } = &subject.kind {
            let name = &self.ir_var_table().get(*id).name;
            self.borrowed_params.contains_key(name)
        } else {
            false
        };

        // Clone variable subjects to avoid use-after-move when matched multiple times
        let subj_is_var = matches!(&subject.kind, IrExprKind::Var { .. });
        let subj_needs_clone = subj_is_var && !Self::is_copy_ty(&subject.ty);

        let subj_expr = if has_string_in_option {
            format!("{}.as_deref()", subj)
        } else if has_bare_string && !subj_is_borrowed {
            format!("{}.as_str()", subj)
        } else if subj_needs_clone {
            format!("{}.clone()", subj)
        } else {
            subj
        };

        let mut lines = vec![format!("match {} {{", subj_expr)];
        for arm in arms {
            let pat = self.gen_ir_pattern(&arm.pattern);
            let guard = arm.guard.as_ref().map(|g| format!(" if {}", self.gen_ir_expr(g))).unwrap_or_default();
            let body = self.gen_ir_expr(&arm.body);
            let derefs = self.collect_ir_box_derefs(&arm.pattern);
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

    pub(crate) fn gen_ir_pattern(&self, pat: &IrPattern) -> String {
        match pat {
            IrPattern::Wildcard => "_".to_string(),
            IrPattern::Bind { var } => {
                self.ir_var_table().get(*var).name.clone()
            }
            IrPattern::Literal { expr } => self.gen_ir_pattern_literal(expr),
            IrPattern::None => "None".to_string(),
            IrPattern::Some { inner } => format!("Some({})", self.gen_ir_pattern(inner)),
            IrPattern::Ok { inner } => format!("Ok({})", self.gen_ir_pattern(inner)),
            IrPattern::Err { inner } => format!("Err({})", self.gen_ir_pattern(inner)),
            IrPattern::Constructor { name, args } => {
                let qualified = if let Some(enum_name) = self.generic_variant_constructors.get(name) {
                    format!("{}::{}", enum_name, name)
                } else {
                    name.clone()
                };
                if args.is_empty() {
                    qualified
                } else {
                    let ps: Vec<String> = args.iter().enumerate().map(|(i, p)| {
                        let inner = self.gen_ir_pattern(p);
                        if self.boxed_variant_args.contains(&(name.clone(), i)) {
                            if let IrPattern::Bind { .. } = p {
                                format!("__boxed_{}", inner)
                            } else {
                                inner
                            }
                        } else {
                            inner
                        }
                    }).collect();
                    format!("{}({})", qualified, ps.join(", "))
                }
            }
            IrPattern::Tuple { elements } => {
                let ps: Vec<String> = elements.iter().map(|p| self.gen_ir_pattern(p)).collect();
                format!("({})", ps.join(", "))
            }
            IrPattern::RecordPattern { name, fields, rest } => {
                let mut fs: Vec<String> = fields.iter().map(|f| {
                    if let Some(p) = &f.pattern {
                        format!("{}: {}", f.name, self.gen_ir_pattern(p))
                    } else {
                        f.name.clone()
                    }
                }).collect();
                if *rest { fs.push("..".to_string()); }
                let qualified = if let Some(enum_name) = self.generic_variant_constructors.get(name) {
                    format!("{}::{}", enum_name, name)
                } else {
                    name.clone()
                };
                format!("{} {{ {} }}", qualified, fs.join(", "))
            }
        }
    }

    fn gen_ir_pattern_literal(&self, expr: &IrExpr) -> String {
        match &expr.kind {
            IrExprKind::LitStr { value } => format!("\"{}\"", value),
            IrExprKind::LitInt { value } => format!("{}i64", value),
            IrExprKind::LitFloat { value } => value.to_string(),
            IrExprKind::LitBool { value } => value.to_string(),
            _ => self.gen_ir_expr(expr),
        }
    }

    pub(crate) fn gen_ir_block(&self, stmts: &[IrStmt], final_expr: Option<&IrExpr>) -> String {
        let mut lines = vec!["{".to_string()];
        for stmt in stmts {
            lines.push(format!("    {}", self.gen_ir_stmt(stmt)));
        }
        if let Some(expr) = final_expr {
            lines.push(format!("    {}", self.gen_ir_expr(expr)));
        }
        lines.push("}".to_string());
        lines.join("\n")
    }

    fn ir_is_ok_unit(&self, expr: &IrExpr) -> bool {
        matches!(&expr.kind, IrExprKind::ResultOk { expr } if matches!(&expr.kind, IrExprKind::Unit))
    }

    pub(crate) fn gen_ir_do_block(&self, stmts: &[IrStmt], final_expr: Option<&IrExpr>) -> String {
        let has_guard = stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { .. }));
        if has_guard {
            let has_ok_unit_guard = stmts.iter().any(|s| {
                if let IrStmtKind::Guard { else_, .. } = &s.kind { self.ir_is_ok_unit(else_) } else { false }
            });
            let mut lines = vec!["{ loop {".to_string()];
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Guard { cond, else_ } => {
                        let c = self.gen_ir_expr(cond);
                        if matches!(&else_.kind, IrExprKind::Unit) || self.ir_is_ok_unit(else_) || matches!(&else_.kind, IrExprKind::Break) {
                            lines.push(format!("    if !({}) {{ break; }}", c));
                        } else if matches!(&else_.kind, IrExprKind::Continue) {
                            lines.push(format!("    if !({}) {{ continue; }}", c));
                        } else {
                            let e = self.gen_ir_expr(else_);
                            if e.contains("return ") {
                                lines.push(format!("    if !({}) {{ {}; }}", c, e));
                            } else {
                                lines.push(format!("    if !({}) {{ return {}; }}", c, e));
                            }
                        }
                    }
                    _ => lines.push(format!("    {}", self.gen_ir_stmt(stmt))),
                }
            }
            if let Some(expr) = final_expr {
                lines.push(format!("    {};", self.gen_ir_expr(expr)));
            }
            lines.push("}".to_string());
            if has_ok_unit_guard && self.in_effect {
                lines.push("Ok::<(), String>(()) }".to_string());
            } else {
                lines.push("}".to_string());
            }
            lines.join("\n")
        } else {
            let prev = self.in_do_block.get();
            if self.in_effect {
                self.in_do_block.set(true);
            }
            let result = if self.in_effect && final_expr.is_some() {
                let expr = final_expr.expect("guarded by is_some()");
                let inner = self.gen_ir_expr(expr);
                let mut lines = vec!["{".to_string()];
                for stmt in stmts {
                    lines.push(format!("    {}", self.gen_ir_stmt(stmt)));
                }
                lines.push(format!("    Ok({})", inner));
                lines.push("}".to_string());
                lines.join("\n")
            } else {
                self.gen_ir_block(stmts, final_expr)
            };
            self.in_do_block.set(prev);
            result
        }
    }

    /// Check if any arm has string literals inside Option/Result patterns
    fn ir_has_string_in_option_pattern(pat: &IrPattern) -> bool {
        match pat {
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
                matches!(&**inner, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. }))
                    || Self::ir_has_string_in_option_pattern(inner)
            }
            _ => false,
        }
    }

    /// Check if pattern is a bare string literal
    fn ir_has_bare_string_literal(pat: &IrPattern) -> bool {
        matches!(pat, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. }))
    }

    /// Collect auto-deref let bindings for boxed recursive variant fields
    fn collect_ir_box_derefs(&self, pat: &IrPattern) -> Vec<String> {
        match pat {
            IrPattern::Constructor { name, args } => {
                let mut derefs = Vec::new();
                for (i, p) in args.iter().enumerate() {
                    if self.boxed_variant_args.contains(&(name.clone(), i)) {
                        if let IrPattern::Bind { var } = p {
                            let var_name = self.ir_var_table().get(*var).name.clone();
                            derefs.push(format!("let {} = *__boxed_{};", var_name, var_name));
                        }
                    }
                    derefs.extend(self.collect_ir_box_derefs(p));
                }
                derefs
            }
            _ => Vec::new(),
        }
    }

    /// Generate Rust type annotation for IR types that need it.
    /// Only annotates types that Rust cannot infer (empty collections, Option/Result in some contexts).
    fn ir_ty_annotation(&self, ty: &almide::types::Ty, value: Option<&IrExpr>) -> Option<String> {
        use almide::types::Ty;
        let is_empty_collection = value.map_or(false, |v| match &v.kind {
            IrExprKind::List { elements } => elements.is_empty(),
            IrExprKind::EmptyMap => true,
            IrExprKind::Call { target: CallTarget::Module { func, .. }, args, .. } => {
                func == "new" && args.is_empty()
            }
            _ => false,
        });
        match ty {
            // Annotate Result when both type params are known (not Unknown)
            // Skip in effect/do-block context: auto-? may unwrap the Result, making
            // the annotation incorrect (annotated type vs actual unwrapped type)
            Ty::Result(ok, err) => {
                if matches!(ok.as_ref(), Ty::Unknown) || matches!(err.as_ref(), Ty::Unknown) {
                    None
                } else if (self.in_effect || self.in_do_block.get()) && value.map_or(false, |v| self.ir_value_gets_auto_q(v)) {
                    // Skip: auto-? unwraps the Result, so annotation would be wrong
                    None
                } else {
                    Some(format!("Result<{}, {}>", self.ir_ty_to_rust(ok), self.ir_ty_to_rust(err)))
                }
            }
            Ty::Option(inner) => {
                let inner_str = self.ir_ty_to_rust(inner);
                Some(format!("Option<{}>", inner_str))
            }
            // Annotate List when empty or inner type is unknown
            Ty::List(inner) if is_empty_collection || matches!(inner.as_ref(), Ty::Unknown | Ty::TypeVar(_)) => {
                Some(format!("Vec<{}>", self.ir_ty_to_rust(inner)))
            }
            // Annotate Map when empty or types are unknown
            Ty::Map(k, v) if is_empty_collection || matches!(k.as_ref(), Ty::Unknown | Ty::TypeVar(_)) || matches!(v.as_ref(), Ty::Unknown | Ty::TypeVar(_)) => {
                Some(format!("HashMap<{}, {}>", self.ir_ty_to_rust(k), self.ir_ty_to_rust(v)))
            }
            // Annotate user-defined generic types (e.g., Either<String, i64>)
            Ty::Named(_, args) if !args.is_empty() => {
                Some(self.ir_ty_to_rust(ty))
            }
            _ => None,
        }
    }

    /// Check if an IR value expression would get auto-? appended by codegen,
    /// making the actual runtime type the unwrapped T instead of Result<T, E>.
    fn ir_value_gets_auto_q(&self, expr: &IrExpr) -> bool {
        match &expr.kind {
            IrExprKind::Try { .. } => true,
            IrExprKind::Call { target, .. } => {
                let name = match target {
                    CallTarget::Named { name } => name.as_str(),
                    CallTarget::Module { func, .. } => func.as_str(),
                    _ => return false,
                };
                if self.in_effect && !self.in_test {
                    if self.effect_fns.contains(&name.to_string()) { return true; }
                }
                if self.in_do_block.get() {
                    if self.result_fns.contains(&name.to_string()) || self.effect_fns.contains(&name.to_string()) { return true; }
                }
                false
            }
            // If any branch gets auto-?, the whole if/block produces unwrapped T
            IrExprKind::If { then, else_, .. } => {
                self.ir_value_gets_auto_q(then) || self.ir_value_gets_auto_q(else_)
            }
            IrExprKind::Block { expr: Some(e), .. } | IrExprKind::DoBlock { expr: Some(e), .. } => {
                self.ir_value_gets_auto_q(e)
            }
            _ => false,
        }
    }

    /// Convert IR Ty to Rust type string
    pub(crate) fn ir_ty_to_rust(&self, ty: &almide::types::Ty) -> String {
        use almide::types::Ty;
        match ty {
            Ty::Int => "i64".to_string(),
            Ty::Float => "f64".to_string(),
            Ty::String => "String".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Unit => "()".to_string(),
            Ty::List(inner) => format!("Vec<{}>", self.ir_ty_to_rust(inner)),
            Ty::Option(inner) => format!("Option<{}>", self.ir_ty_to_rust(inner)),
            Ty::Result(ok, err) => format!("Result<{}, {}>", self.ir_ty_to_rust(ok), self.ir_ty_to_rust(err)),
            Ty::Map(k, v) => format!("HashMap<{}, {}>", self.ir_ty_to_rust(k), self.ir_ty_to_rust(v)),
            Ty::Tuple(elems) => {
                let ts: Vec<String> = elems.iter().map(|e| self.ir_ty_to_rust(e)).collect();
                format!("({})", ts.join(", "))
            }
            Ty::Named(name, args) => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let ts: Vec<String> = args.iter().map(|a| self.ir_ty_to_rust(a)).collect();
                    format!("{}<{}>", name, ts.join(", "))
                }
            }
            Ty::TypeVar(_) => "_".to_string(),
            Ty::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| self.ir_ty_to_rust(p)).collect();
                format!("impl Fn({}) -> {} + Clone", ps.join(", "), self.ir_ty_to_rust(ret))
            }
            _ => "_".to_string(),
        }
    }
}
