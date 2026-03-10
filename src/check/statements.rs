use crate::ast;
use crate::types::{Ty, VariantPayload};
use super::{Checker, err};

impl Checker {
    pub(crate) fn check_stmt(&mut self, stmt: &mut ast::Stmt) {
        // Update current line from statement span for better error positions
        let stmt_line = match stmt {
            ast::Stmt::Let { span, .. }
            | ast::Stmt::LetDestructure { span, .. }
            | ast::Stmt::Var { span, .. }
            | ast::Stmt::Assign { span, .. }
            | ast::Stmt::Guard { span, .. }
            | ast::Stmt::Expr { span, .. } => span.map(|s| s.line),
            ast::Stmt::Comment { .. } => None,
        };
        let prev_line = self.current_decl_line;
        if let Some(line) = stmt_line {
            self.current_decl_line = Some(line);
        }
        match stmt {
            ast::Stmt::Let { name, ty, value, .. } => {
                let vt = self.check_expr(value);
                let vt = if self.env.in_do_block {
                    match vt {
                        Ty::Result(ok, _) => *ok,
                        other => other,
                    }
                } else { vt };
                let dt = if let Some(te) = ty {
                    let t = self.resolve_type_expr(te);
                    if !t.compatible(&vt) {
                        self.push_diagnostic(err(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, t.display()),
                            "Change the type annotation or the value",
                            format!("let {} = ...", name),
                        ));
                    }
                    t
                } else { vt };
                self.env.define_var(name, dt);
            }
            ast::Stmt::Var { name, ty, value, .. } => {
                let vt = self.check_expr(value);
                let dt = if let Some(te) = ty {
                    let t = self.resolve_type_expr(te);
                    if !t.compatible(&vt) {
                        self.push_diagnostic(err(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, t.display()),
                            "Change the type annotation or the value",
                            format!("var {} = ...", name),
                        ));
                    }
                    t
                } else { vt };
                self.env.define_var(name, dt);
            }
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let vt = self.check_expr(value);
                let resolved = self.env.resolve_named(&vt);
                self.check_pattern(pattern, &resolved);
            }
            ast::Stmt::Assign { name, value, .. } => {
                let vt = self.check_expr(value);
                if let Some(var_ty) = self.env.lookup_var(name).cloned() {
                    if !var_ty.compatible(&vt) {
                        self.push_diagnostic(err(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, var_ty.display()),
                            "Assignment must match the variable's declared type",
                            format!("{} = ...", name),
                        ));
                    }
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
            ast::Stmt::Expr { expr, .. } => { self.check_expr(expr); }
            ast::Stmt::Comment { .. } => {}
        }
        self.current_decl_line = prev_line;
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
            ast::Pattern::RecordPattern { name, fields } => {
                // Look up field types from variant constructor or subject record type
                let field_type_map: Vec<(String, Ty)> = if !name.is_empty() {
                    if let Some((_, case)) = self.env.constructors.get(name).cloned() {
                        if let VariantPayload::Record(rec_fields) = case.payload {
                            rec_fields
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
}
