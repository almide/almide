use crate::ast;
use crate::types::{Ty, VariantPayload};
use super::{Checker, err};

impl Checker {
    pub(crate) fn check_stmt(&mut self, stmt: &mut ast::Stmt) {
        // Update current line/col from statement span for better error positions
        let stmt_span = match stmt {
            ast::Stmt::Let { span, .. }
            | ast::Stmt::LetDestructure { span, .. }
            | ast::Stmt::Var { span, .. }
            | ast::Stmt::Assign { span, .. }
            | ast::Stmt::IndexAssign { span, .. }
            | ast::Stmt::FieldAssign { span, .. }
            | ast::Stmt::Guard { span, .. }
            | ast::Stmt::Expr { span, .. } => *span,
            ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => None,
        };
        let prev_line = self.current_decl_line;
        let prev_col = self.current_decl_col;
        if let Some(s) = stmt_span {
            self.current_decl_line = Some(s.line);
            self.current_decl_col = Some(s.col);
        }
        match stmt {
            ast::Stmt::Let { name, ty, value, span, .. } => {
                let expected_ty = ty.as_ref().map(|te| self.resolve_type_expr(te));
                let vt = self.check_expr_with(value, expected_ty.as_ref());
                let vt = if self.env.in_do_block {
                    match vt {
                        Ty::Result(ok, _) => *ok,
                        other => other,
                    }
                } else { vt };
                let dt = if let Some(te) = ty {
                    let t = expected_ty.clone().unwrap_or_else(|| self.resolve_type_expr(te));
                    // Resolve Named types (e.g., Container[Int] → Record { items: List[Int], label: String })
                    // so structural comparison works with anonymous record literals
                    let t_resolved = self.env.resolve_named(&t);
                    if !t_resolved.compatible(&vt) {
                        self.push_diagnostic(err(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, t.display()),
                            "Change the type annotation or the value",
                            format!("let {} = ...", name),
                        ));
                    }
                    t
                } else { vt };
                if let Some(s) = span {
                    self.env.define_var_at(name, dt, s.line, s.col);
                } else {
                    self.env.define_var(name, dt);
                }
            }
            ast::Stmt::Var { name, ty, value, span, .. } => {
                let expected_ty = ty.as_ref().map(|te| self.resolve_type_expr(te));
                let vt = self.check_expr_with(value, expected_ty.as_ref());
                let dt = if let Some(te) = ty {
                    let t = expected_ty.clone().unwrap_or_else(|| self.resolve_type_expr(te));
                    let t_resolved = self.env.resolve_named(&t);
                    if !t_resolved.compatible(&vt) {
                        self.push_diagnostic(err(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, t.display()),
                            "Change the type annotation or the value",
                            format!("var {} = ...", name),
                        ));
                    }
                    t
                } else { vt };
                if let Some(s) = span {
                    self.env.define_var_at(name, dt, s.line, s.col);
                } else {
                    self.env.define_var(name, dt);
                }
                self.env.mutable_vars.insert(name.clone());
            }
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let vt = self.check_expr(value);
                let resolved = self.env.resolve_named(&vt);
                self.check_pattern(pattern, &resolved);
            }
            ast::Stmt::Assign { name, value, .. } => {
                let vt = self.check_expr(value);
                if let Some(var_ty) = self.env.lookup_var(name).cloned() {
                    if !self.env.mutable_vars.contains(name) {
                        let hint = if self.env.param_vars.contains(name) {
                            format!("'{}' is a function parameter (immutable). Use a local copy: var {0}_ = {0}", name)
                        } else {
                            format!("Use 'var {0} = ...' instead of 'let {0} = ...' to declare a mutable variable", name)
                        };
                        let mut diag = err(
                            format!("cannot reassign immutable binding '{}'", name),
                            hint,
                            format!("{} = ...", name),
                        );
                        // Show declaration site as secondary span
                        if let Some((decl_line, decl_col)) = self.env.var_decl_loc(name) {
                            diag.secondary.push(crate::diagnostic::SecondarySpan {
                                line: decl_line,
                                col: Some(decl_col),
                                label: format!("'{}' declared as immutable here", name),
                            });
                        }
                        self.push_diagnostic(diag);
                    } else if !var_ty.compatible(&vt) {
                        let mut diag = err(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, var_ty.display()),
                            "Assignment must match the variable's declared type",
                            format!("{} = ...", name),
                        );
                        if let Some((decl_line, decl_col)) = self.env.var_decl_loc(name) {
                            diag.secondary.push(crate::diagnostic::SecondarySpan {
                                line: decl_line,
                                col: Some(decl_col),
                                label: format!("declared as {} here", var_ty.display()),
                            });
                        }
                        self.push_diagnostic(diag);
                    }
                }
            }
            ast::Stmt::IndexAssign { target, index, value, .. } => {
                self.check_expr(index);
                self.check_expr(value);
                if self.env.lookup_var(target).is_none() {
                    self.push_diagnostic(err(
                        format!("undefined variable '{}' in index assignment", target),
                        "Declare the variable with 'var' before assigning to its elements",
                        format!("{}[...] = ...", target),
                    ));
                }
            }
            ast::Stmt::FieldAssign { target, value, .. } => {
                self.check_expr(value);
                if self.env.lookup_var(target).is_none() {
                    self.push_diagnostic(err(
                        format!("undefined variable '{}' in field assignment", target),
                        "Declare the variable with 'var' before assigning to its fields",
                        format!("{}.field = ...", target),
                    ));
                }
            }
            ast::Stmt::Guard { cond, else_, .. } => {
                let ct = self.check_expr(cond);
                if !ct.compatible(&Ty::Bool) {
                    self.push_diagnostic(err(
                        format!("guard condition has type {} but expected Bool", ct.display()),
                        "Guard conditions must be Bool", "guard statement",
                    ));
                }
                self.check_expr(else_);
            }
            ast::Stmt::Expr { expr, .. } => {
                self.check_expr(expr);
                // Warn about discarded return values from immutable update functions
                self.check_discarded_mutation(expr);
            }
            ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
        }
        self.current_decl_line = prev_line;
        self.current_decl_col = prev_col;
    }

    pub(crate) fn check_pattern(&mut self, pattern: &ast::Pattern, subject_ty: &Ty) {
        match pattern {
            ast::Pattern::Wildcard => {}
            ast::Pattern::Ident { name } => { self.env.define_var(name, subject_ty.clone()); }
            ast::Pattern::Literal { .. } => {}
            ast::Pattern::Some { inner } => {
                let it = match subject_ty { Ty::Option(t) => *t.clone(), _ => Ty::Unknown };
                self.check_pattern(inner, &it);
            }
            ast::Pattern::None => {}
            ast::Pattern::Ok { inner } => {
                let it = match subject_ty { Ty::Result(t, _) => *t.clone(), _ => Ty::Unknown };
                self.check_pattern(inner, &it);
            }
            ast::Pattern::Err { inner } => {
                let it = match subject_ty { Ty::Result(_, e) => *e.clone(), _ => Ty::Unknown };
                self.check_pattern(inner, &it);
            }
            ast::Pattern::Constructor { name, args } => {
                if let Some((_, case)) = self.env.constructors.get(name).cloned() {
                    if let VariantPayload::Tuple(field_tys) = &case.payload {
                        for (pat, ty) in args.iter().zip(field_tys.iter()) { self.check_pattern(pat, ty); }
                    } else {
                        for pat in args { self.check_pattern(pat, &Ty::Unknown); }
                    }
                } else {
                    for pat in args { self.check_pattern(pat, &Ty::Unknown); }
                }
            }
            ast::Pattern::Tuple { elements } => {
                let tys = self.resolve_tuple_elements(subject_ty, elements.len(), "tuple pattern");
                for (pat, ty) in elements.iter().zip(tys.iter()) {
                    self.check_pattern(pat, ty);
                }
            }
            ast::Pattern::RecordPattern { name, fields, .. } => {
                // Look up field types from variant constructor or subject record type
                let field_type_map: Vec<(String, Ty)> = if !name.is_empty() {
                    if let Some((_, case)) = self.env.constructors.get(name).cloned() {
                        if let VariantPayload::Record(rec_fields) = case.payload {
                            rec_fields.into_iter().map(|(n, t, _)| (n, t)).collect()
                        } else { vec![] }
                    } else { vec![] }
                } else if let Ty::Record { fields: rec_fields } = subject_ty {
                    rec_fields.clone()
                } else { vec![] };
                for field in fields {
                    let ft = field_type_map.iter()
                        .find(|(n, _)| n == &field.name)
                        .map(|(_, t)| t.clone())
                        .unwrap_or(Ty::Unknown);
                    if let Some(ref pat) = field.pattern {
                        self.check_pattern(pat, &ft);
                    } else {
                        self.env.define_var(&field.name, ft);
                    }
                }
            }
        }
    }

    /// Functions whose return value should never be discarded (immutable update pattern).
    const IMMUTABLE_UPDATE_FNS: &'static [(&'static str, &'static str)] = &[
        ("list", "set"), ("list", "swap"), ("list", "push"), ("list", "insert"),
        ("list", "remove"), ("list", "remove_at"), ("list", "sort"), ("list", "reverse"),
        ("list", "map"), ("list", "filter"), ("list", "take"), ("list", "drop"), ("list", "slice"),
        ("map", "set"), ("map", "remove"),
        ("string", "replace"), ("string", "replace_first"),
        ("string", "trim"), ("string", "to_lower"), ("string", "to_upper"),
    ];

    fn is_immutable_update(module: &str, func: &str) -> bool {
        Self::IMMUTABLE_UPDATE_FNS.iter().any(|(m, f)| *m == module && *f == func)
    }

    fn mutation_hint(module: &str, func: &str) -> String {
        match (module, func) {
            ("list", "set") | ("list", "swap") | ("list", "insert") | ("list", "remove_at") =>
                format!("{}.{}() returns a new list — assign it: xs = {}.{}(xs, ...)", module, func, module, func),
            ("list", "push") =>
                "list.push() returns a new list — assign it: xs = list.push(xs, item)".to_string(),
            ("list", "sort") | ("list", "reverse") =>
                format!("{}.{}() returns a new list — assign it: xs = {}.{}(xs)", module, func, module, func),
            ("list", "map") | ("list", "filter") | ("list", "take") | ("list", "drop") =>
                format!("{}.{}() returns a new list — assign the result", module, func),
            ("map", "set") =>
                "map.set() returns a new map — assign it: m = map.set(m, key, value)".to_string(),
            ("map", "remove") =>
                "map.remove() returns a new map — assign it: m = map.remove(m, key)".to_string(),
            _ =>
                format!("{}.{}() returns a new value — the result is discarded here", module, func),
        }
    }

    /// Check if a statement-level expression discards the return value of an immutable update function.
    /// Handles both `list.set(xs, i, v)` (module call) and `xs.set(i, v)` (UFCS).
    fn check_discarded_mutation(&mut self, expr: &ast::Expr) {
        use crate::diagnostic::Diagnostic;
        if let ast::Expr::Call { callee, .. } = expr {
            if let ast::Expr::Member { object, field, .. } = callee.as_ref() {
                let func = field.as_str();
                // Direct module call: list.set(xs, i, v)
                if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                    if Self::is_immutable_update(module, func) {
                        self.push_diagnostic(Diagnostic::warning(
                            format!("return value of {}.{}() is unused", module, func),
                            Self::mutation_hint(module, func),
                            format!("{}.{}()", module, func),
                        ));
                        return;
                    }
                }
                // UFCS call: xs.push(42) — resolve module from receiver type
                let receiver_ty = self.infer_receiver_type(object);
                let module = match &receiver_ty {
                    Some(Ty::List(_)) => Some("list"),
                    Some(Ty::Map(_, _)) => Some("map"),
                    Some(Ty::String) => Some("string"),
                    _ => None,
                };
                if let Some(module) = module {
                    if Self::is_immutable_update(module, func) {
                        self.push_diagnostic(Diagnostic::warning(
                            format!("return value of .{}() is unused", func),
                            Self::mutation_hint(module, func),
                            format!(".{}()", func),
                        ));
                    }
                }
            }
        }
    }

    /// Quick type inference for a receiver expression (for UFCS lost mutation detection).
    fn infer_receiver_type(&self, expr: &ast::Expr) -> Option<Ty> {
        match expr {
            ast::Expr::Ident { name, .. } => self.env.lookup_var(name).cloned(),
            ast::Expr::List { .. } => Some(Ty::List(Box::new(Ty::Unknown))),
            ast::Expr::String { .. } | ast::Expr::InterpolatedString { .. } => Some(Ty::String),
            _ => None,
        }
    }
}
