/// Almide type checker — inserted between parser and emitter.
/// Every error includes an actionable hint so LLMs can auto-repair.

mod expressions;
mod calls;
mod operators;
mod statements;

use crate::ast;
use crate::diagnostic::Diagnostic;
use crate::stdlib;
use crate::types::{Ty, TypeEnv, FnSig, VariantCase, VariantPayload};

pub struct Checker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
    current_decl_line: Option<usize>,
}

pub(crate) fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

impl Checker {
    pub fn new() -> Self {
        let mut c = Checker {
            env: TypeEnv::new(),
            diagnostics: Vec::new(),
            source_file: None,
            source_text: None,
            current_decl_line: None,
        };
        c.register_stdlib();
        c
    }

    pub fn set_source(&mut self, file: &str, text: &str) {
        self.source_file = Some(file.to_string());
        self.source_text = Some(text.to_string());
    }

    /// Extract the line number from a declaration's span.
    fn decl_line(&self, decl: &ast::Decl) -> Option<usize> {
        match decl {
            ast::Decl::Fn { span, .. }
            | ast::Decl::Test { span, .. }
            | ast::Decl::Type { span, .. }
            | ast::Decl::Module { span, .. }
            | ast::Decl::Import { span, .. }
            | ast::Decl::Trait { span, .. }
            | ast::Decl::Impl { span, .. }
            | ast::Decl::Strict { span, .. } => span.map(|s| s.line),
        }
    }

    pub(crate) fn push_diagnostic(&mut self, mut d: Diagnostic) {
        if let Some(ref file) = self.source_file {
            if d.file.is_none() {
                d.file = Some(file.clone());
            }
        }
        if d.line.is_none() {
            d.line = self.current_decl_line;
        }
        self.diagnostics.push(d);
    }

    /// Register function and type declarations into the environment.
    /// When `prefix` is Some, keys are prefixed (e.g. "module.func") for imported modules.
    /// When `prefix` is None, registers as local declarations with variant constructors and effect tracking.
    fn register_decls(&mut self, decls: &[ast::Decl], prefix: Option<&str>, is_external: bool) {
        for decl in decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, visibility, .. } => {
                    if prefix.is_some() {
                        let hidden = match visibility {
                            ast::Visibility::Local => true,
                            ast::Visibility::Mod => is_external,
                            ast::Visibility::Public => false,
                        };
                        if hidden {
                            if let Some(p) = prefix {
                                self.env.local_symbols.insert(format!("{}.{}", p, name));
                            }
                            continue;
                        }
                    }
                    let param_tys: Vec<(String, Ty)> = params.iter()
                        .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                        .collect();
                    let ret = self.resolve_type_expr(return_type);
                    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
                    let key = match prefix {
                        Some(p) => format!("{}.{}", p, name),
                        None => name.clone(),
                    };
                    if prefix.is_none() && is_effect {
                        self.env.effect_fns.insert(name.clone());
                    }
                    self.env.functions.insert(key, FnSig { params: param_tys, ret, is_effect });
                }
                ast::Decl::Type { name, ty, visibility, .. } => {
                    if prefix.is_some() {
                        let hidden = match visibility {
                            ast::Visibility::Local => true,
                            ast::Visibility::Mod => is_external,
                            ast::Visibility::Public => false,
                        };
                        if hidden {
                            if let Some(p) = prefix {
                                self.env.local_symbols.insert(format!("{}.{}", p, name));
                            }
                            continue;
                        }
                    }
                    let mut resolved = self.resolve_type_expr(ty);
                    if prefix.is_none() {
                        if let Ty::Variant { name: ref mut vname, ref cases } = resolved {
                            *vname = name.clone();
                            for case in cases {
                                self.env.constructors.insert(case.name.clone(), (name.clone(), case.clone()));
                            }
                        }
                    }
                    let key = match prefix {
                        Some(p) => format!("{}.{}", p, name),
                        None => name.clone(),
                    };
                    self.env.types.insert(key, resolved);
                }
                _ => {}
            }
        }
    }

    /// Register an imported module's exported functions and types.
    pub fn register_module(&mut self, mod_name: &str, prog: &ast::Program, pkg_id: Option<&crate::project::PkgId>) {
        let is_external = pkg_id.is_some();
        if let Some(pid) = pkg_id {
            let internal_name = pid.mod_name();
            self.env.user_modules.insert(internal_name.clone());
            self.env.module_aliases.insert(mod_name.to_string(), internal_name.clone());
            self.register_decls(&prog.decls, Some(&internal_name), is_external);
        } else {
            self.env.user_modules.insert(mod_name.to_string());
            self.register_decls(&prog.decls, Some(mod_name), is_external);
        }
    }

    pub fn check_program(&mut self, prog: &mut ast::Program) -> Vec<Diagnostic> {
        self.register_decls(&prog.decls, None, false);
        for decl in prog.decls.iter_mut() {
            self.check_decl(decl);
        }

        // Warn about unused imports
        for imp in &prog.imports {
            if let ast::Decl::Import { path, alias, .. } = imp {
                // For self imports, the accessible name is the alias or the last path segment
                let is_self_import = path.first().map(|s| s.as_str()) == Some("self");
                let accessible_name = if let Some(a) = alias {
                    a.as_str()
                } else if is_self_import && path.len() >= 2 {
                    path.last().map(|s| s.as_str()).unwrap_or(&path[0])
                } else {
                    path[0].as_str()
                };
                let display_path = path.join(".");
                if !self.env.used_modules.contains(accessible_name) {
                    let line = self.find_import_line_by_path(&display_path);
                    let mut d = Diagnostic::warning(
                        format!("unused import '{}'", display_path),
                        format!("Remove 'import {}' if it is not needed", display_path),
                        format!("import {}", display_path),
                    );
                    if let Some(ref file) = self.source_file {
                        d.file = Some(file.clone());
                    }
                    d.line = line;
                    self.diagnostics.push(d);
                }
            }
        }

        self.diagnostics.clone()
    }

    pub(crate) fn warn_unused_vars_in_scope(&mut self, context: &str) {
        let unused: Vec<String> = if let Some(scope) = self.env.scopes.last() {
            scope.keys()
                .filter(|v| !v.starts_with('_') && *v != "self" && !self.env.used_vars.contains(*v))
                .cloned()
                .collect()
        } else {
            vec![]
        };
        for var_name in unused {
            self.push_diagnostic(Diagnostic::warning(
                format!("unused variable '{}'", var_name),
                format!("Prefix with '_' to suppress: _{}", var_name),
                context.to_string(),
            ));
        }
    }

    fn find_import_line_by_path(&self, path: &str) -> Option<usize> {
        let source = self.source_text.as_ref()?;
        let pattern = format!("import {}", path);
        for (i, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed == pattern || trimmed.starts_with(&format!("{} ", pattern)) {
                return Some(i + 1);
            }
        }
        None
    }

    pub(crate) fn check_decl(&mut self, decl: &mut ast::Decl) {
        self.current_decl_line = self.decl_line(decl);
        match decl {
            ast::Decl::Fn { name, params, return_type, body, effect, .. } => {
                self.env.push_scope();
                let mut local_vars: Vec<String> = Vec::new();
                for p in params {
                    let ty = self.resolve_type_expr(&p.ty);
                    self.env.define_var(&p.name, ty);
                    local_vars.push(p.name.clone());
                }
                let ret_ty = self.resolve_type_expr(return_type);
                let prev_ret = self.env.current_ret.take();
                let prev_effect = self.env.in_effect;
                self.env.current_ret = Some(ret_ty.clone());
                self.env.in_effect = effect.unwrap_or(false);
                let body_ty = self.check_expr(body);
                let is_effect = effect.unwrap_or(false);
                let effective_ret = if is_effect {
                    match &ret_ty {
                        Ty::Result(ok_ty, _) => *ok_ty.clone(),
                        _ => ret_ty.clone(),
                    }
                } else {
                    ret_ty.clone()
                };
                if !body_ty.compatible(&effective_ret) && !body_ty.compatible(&ret_ty) {
                    self.push_diagnostic(err(
                        format!("function '{}' declared to return {} but body has type {}", name, ret_ty.display(), body_ty.display()),
                        "Change the return type or fix the body expression",
                        format!("fn {}", name),
                    ));
                }
                // Warn about unused variables (skip _ prefixed)
                self.warn_unused_vars_in_scope(&format!("fn {}", name));
                self.env.current_ret = prev_ret;
                self.env.in_effect = prev_effect;
                self.env.pop_scope();
            }
            ast::Decl::Test { body, .. } => {
                self.env.push_scope();
                let prev = self.env.in_effect;
                let prev_test = self.env.in_test;
                self.env.in_effect = true;
                self.env.in_test = true;
                self.check_expr(body);
                self.env.in_effect = prev;
                self.env.in_test = prev_test;
                self.env.pop_scope();
            }
            _ => {}
        }
    }

    pub(crate) fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
        match te {
            ast::TypeExpr::Simple { name } => match name.as_str() {
                "Int" => Ty::Int, "Float" => Ty::Float, "String" => Ty::String,
                "Bool" => Ty::Bool, "Unit" => Ty::Unit, "Path" => Ty::String,
                other => Ty::Named(other.to_string()),
            },
            ast::TypeExpr::Generic { name, args } => {
                let ra: Vec<Ty> = args.iter().map(|a| self.resolve_type_expr(a)).collect();
                match name.as_str() {
                    "List" if ra.len() == 1 => Ty::List(Box::new(ra[0].clone())),
                    "Option" if ra.len() == 1 => Ty::Option(Box::new(ra[0].clone())),
                    "Result" if ra.len() == 2 => Ty::Result(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Map" if ra.len() == 2 => Ty::Map(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Set" => Ty::List(Box::new(ra.first().cloned().unwrap_or(Ty::Unknown))),
                    _ => Ty::Named(name.clone()),
                }
            }
            ast::TypeExpr::Record { fields } => Ty::Record {
                fields: fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect(),
            },
            ast::TypeExpr::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|p| self.resolve_type_expr(p)).collect(),
                ret: Box::new(self.resolve_type_expr(ret)),
            },
            ast::TypeExpr::Tuple { elements } => Ty::Tuple(
                elements.iter().map(|e| self.resolve_type_expr(e)).collect(),
            ),
            ast::TypeExpr::Newtype { inner } => self.resolve_type_expr(inner),
            ast::TypeExpr::Variant { cases } => {
                let cs: Vec<VariantCase> = cases.iter().map(|c| match c {
                    ast::VariantCase::Unit { name } => VariantCase { name: name.clone(), payload: VariantPayload::Unit },
                    ast::VariantCase::Tuple { name, fields } => VariantCase {
                        name: name.clone(),
                        payload: VariantPayload::Tuple(fields.iter().map(|f| self.resolve_type_expr(f)).collect()),
                    },
                    ast::VariantCase::Record { name, fields } => VariantCase {
                        name: name.clone(),
                        payload: VariantPayload::Record(fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect()),
                    },
                }).collect();
                Ty::Variant { name: String::new(), cases: cs }
            }
        }
    }

    /// Check whether a match expression covers all cases of the subject type.
    /// Reports a warning with the specific missing cases.
    pub(crate) fn check_match_exhaustiveness(&mut self, subject_ty: &Ty, arms: &[ast::MatchArm]) {
        let resolved = self.env.resolve_named(subject_ty);

        // Determine required cases from the subject type
        let required_cases: Vec<String> = match &resolved {
            Ty::Variant { cases, .. } => {
                cases.iter().map(|c| c.name.clone()).collect()
            }
            Ty::Option(_) => vec!["some".to_string(), "none".to_string()],
            Ty::Result(_, _) => vec!["ok".to_string(), "err".to_string()],
            Ty::Bool => vec!["true".to_string(), "false".to_string()],
            _ => return, // Not a finite enum-like type; skip check
        };

        if required_cases.is_empty() {
            return;
        }

        // Collect covered cases from arms (arms with guards don't guarantee coverage)
        let mut has_wildcard = false;
        let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();

        for arm in arms {
            if arm.guard.is_some() {
                continue; // Guarded arms don't guarantee coverage
            }
            self.collect_covered_cases(&arm.pattern, &mut covered, &mut has_wildcard);
        }

        if has_wildcard {
            return; // Wildcard or variable binding covers everything
        }

        let missing: Vec<&String> = required_cases.iter()
            .filter(|c| !covered.contains(*c))
            .collect();

        if !missing.is_empty() {
            let missing_list = missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            let hint = if missing.len() == 1 {
                format!("Add a '{}' arm, or use '_' as a catch-all", missing_list)
            } else {
                format!("Add arms for {}, or use '_' as a catch-all", missing_list)
            };
            self.push_diagnostic(Diagnostic::warning(
                format!("non-exhaustive match: missing {}", missing_list),
                hint,
                "match expression",
            ));
        }
    }

    fn collect_covered_cases(&self, pattern: &ast::Pattern, covered: &mut std::collections::HashSet<String>, has_wildcard: &mut bool) {
        match pattern {
            ast::Pattern::Wildcard | ast::Pattern::Ident { .. } => {
                *has_wildcard = true;
            }
            ast::Pattern::Constructor { name, .. } => {
                covered.insert(name.clone());
            }
            ast::Pattern::Some { .. } => { covered.insert("some".to_string()); }
            ast::Pattern::None => { covered.insert("none".to_string()); }
            ast::Pattern::Ok { .. } => { covered.insert("ok".to_string()); }
            ast::Pattern::Err { .. } => { covered.insert("err".to_string()); }
            ast::Pattern::Literal { value } => {
                // For Bool exhaustiveness: track true/false literals
                match value.as_ref() {
                    ast::Expr::Bool { value: true, .. } => { covered.insert("true".to_string()); }
                    ast::Expr::Bool { value: false, .. } => { covered.insert("false".to_string()); }
                    _ => {}
                }
            }
            ast::Pattern::Tuple { .. } => {}
            ast::Pattern::RecordPattern { name, .. } => {
                covered.insert(name.clone());
            }
        }
    }

    fn register_stdlib(&mut self) {
        for name in stdlib::builtin_effect_fns() {
            self.env.effect_fns.insert(name.to_string());
        }
    }
}
