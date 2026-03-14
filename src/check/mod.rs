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

/// Check if an expression is a compile-time constant (allowed as default field value).
fn is_const_expr(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Int { .. } | ast::Expr::Float { .. } | ast::Expr::String { .. }
        | ast::Expr::Bool { .. } | ast::Expr::Unit { .. } | ast::Expr::None { .. } => true,
        ast::Expr::List { elements, .. } => elements.is_empty(),
        ast::Expr::EmptyMap { .. } => true,
        ast::Expr::Unary { op, operand, .. } if op == "-" => is_const_expr(operand),
        _ => false,
    }
}

/// Check if an expression is a valid top-level constant expression.
/// More permissive than `is_const_expr`: allows references to other top-level lets
/// and arithmetic/string-concat on constants.
fn is_top_let_const_expr(expr: &ast::Expr, known_consts: &std::collections::HashSet<String>) -> bool {
    match expr {
        ast::Expr::Int { .. } | ast::Expr::Float { .. } | ast::Expr::String { .. }
        | ast::Expr::Bool { .. } | ast::Expr::Unit { .. } | ast::Expr::None { .. } => true,
        ast::Expr::List { elements, .. } => elements.is_empty(),
        ast::Expr::EmptyMap { .. } => true,
        ast::Expr::Unary { op, operand, .. } if op == "-" => is_top_let_const_expr(operand, known_consts),
        ast::Expr::Ident { name, .. } | ast::Expr::TypeName { name, .. } => known_consts.contains(name),
        ast::Expr::Binary { op, left, right, .. } => {
            let valid_op = matches!(op.as_str(), "+" | "-" | "*" | "/" | "%" | "++" );
            valid_op && is_top_let_const_expr(left, known_consts) && is_top_let_const_expr(right, known_consts)
        }
        ast::Expr::Paren { expr: inner, .. } => is_top_let_const_expr(inner, known_consts),
        _ => false,
    }
}

pub struct Checker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
    current_decl_line: Option<usize>,
    current_decl_col: Option<usize>,
    /// Build target (e.g. "rust", "ts", "wasm"). Used to gate platform modules.
    pub target: Option<String>,
    /// Full Ty for every expression, keyed by ExprId.
    pub expr_types: std::collections::HashMap<crate::ast::ExprId, Ty>,
    /// Counter for generating fresh ExprIds in sub-parsers (e.g. string interpolation).
    pub next_expr_id: u32,
}

/// Modules that require a native runtime (OS access). Not available on WASM.
const PLATFORM_MODULES: &[&str] = &["fs", "process", "io", "env", "http", "random"];

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
            current_decl_col: None,
            target: None,
            expr_types: std::collections::HashMap::new(),
            next_expr_id: 0,
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
            | ast::Decl::TopLet { span, .. }
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
        if d.col.is_none() {
            d.col = self.current_decl_col;
        }
        // Deduplicate: skip if same message + line + col already reported
        let dominated = self.diagnostics.iter().any(|existing| {
            existing.message == d.message && existing.line == d.line && existing.col == d.col
        });
        if !dominated {
            self.diagnostics.push(d);
        }
    }

    /// Register function and type declarations into the environment.
    /// When `prefix` is Some, keys are prefixed (e.g. "module.func") for imported modules.
    /// When `prefix` is None, registers as local declarations with variant constructors and effect tracking.
    fn register_decls(&mut self, decls: &[ast::Decl], prefix: Option<&str>, is_external: bool) {
        for decl in decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, visibility, generics, .. } => {
                    self.register_fn_decl(name, params, return_type, *effect, *r#async, visibility, generics, prefix, is_external);
                }
                ast::Decl::Type { name, ty, visibility, generics, .. } => {
                    self.register_type_decl(name, ty, visibility, generics, prefix, is_external);
                }
                ast::Decl::TopLet { name, ty, value, visibility, .. } => {
                    self.register_top_let_decl(name, ty.as_ref(), value, visibility, prefix, is_external);
                }
                _ => {}
            }
        }
    }

    fn is_hidden_by_visibility(&mut self, visibility: &ast::Visibility, prefix: Option<&str>, name: &str, is_external: bool) -> bool {
        if prefix.is_none() { return false; }
        let hidden = match visibility {
            ast::Visibility::Local => true,
            ast::Visibility::Mod => is_external,
            ast::Visibility::Public => false,
        };
        if hidden {
            if let Some(p) = prefix {
                self.env.local_symbols.insert(format!("{}.{}", p, name));
            }
        }
        hidden
    }

    fn register_fn_decl(&mut self, name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
                        effect: Option<bool>, r#async: Option<bool>, visibility: &ast::Visibility,
                        generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>, is_external: bool) {
        if self.is_hidden_by_visibility(visibility, prefix, name, is_external) { return; }
        let generic_names: Vec<String> = generics.as_ref()
            .map(|gs| gs.iter().map(|g| g.name.clone()).collect())
            .unwrap_or_default();
        let mut sb = std::collections::HashMap::new();
        if let Some(gs) = generics {
            for g in gs {
                if let Some(ref bound_te) = g.structural_bound {
                    let bound_ty = self.resolve_type_expr(bound_te);
                    let bound_ty = match bound_ty {
                        Ty::Record { fields } => Ty::OpenRecord { fields },
                        other => other,
                    };
                    sb.insert(g.name.clone(), bound_ty);
                }
            }
        }
        for gn in &generic_names {
            self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone()));
        }
        let param_tys: Vec<(String, Ty)> = params.iter()
            .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
            .collect();
        let ret = self.resolve_type_expr(return_type);
        for gn in &generic_names {
            self.env.types.remove(gn);
        }
        let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
        let key = match prefix {
            Some(p) => format!("{}.{}", p, name),
            None => name.to_string(),
        };
        if prefix.is_none() && is_effect {
            self.env.effect_fns.insert(name.to_string());
        }
        self.env.functions.insert(key, FnSig { params: param_tys, ret, is_effect, generics: generic_names, structural_bounds: sb });
    }

    fn register_type_decl(&mut self, name: &str, ty: &ast::TypeExpr, visibility: &ast::Visibility,
                          generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>, is_external: bool) {
        if self.is_hidden_by_visibility(visibility, prefix, name, is_external) { return; }
        let generic_names: Vec<String> = generics.as_ref()
            .map(|gs| gs.iter().map(|g| g.name.clone()).collect())
            .unwrap_or_default();
        for gn in &generic_names {
            self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone()));
        }
        let mut resolved = self.resolve_type_expr(ty);
        if let ast::TypeExpr::Variant { cases: ast_cases } = ty {
            for c in ast_cases {
                if let ast::VariantCase::Record { name: vname, fields } = c {
                    let mut seen_default = false;
                    for f in fields {
                        if f.default.is_some() {
                            seen_default = true;
                            if let Some(ref d) = f.default {
                                if !is_const_expr(d) {
                                    self.push_diagnostic(err(
                                        format!("default value for field '{}' in {} must be a compile-time constant", f.name, vname),
                                        "Allowed: literals, [], \"\", 0, true, false, none",
                                        "default field value",
                                    ));
                                }
                            }
                        } else if seen_default {
                            self.push_diagnostic(err(
                                format!("field '{}' in {} without default must come before fields with defaults", f.name, vname),
                                "Move fields without defaults to the top",
                                "default field value",
                            ));
                        }
                    }
                }
            }
        }
        for gn in &generic_names {
            self.env.types.remove(gn);
        }
        if prefix.is_none() {
            if let Ty::Variant { name: ref mut vname, ref cases } = resolved {
                *vname = name.to_string();
                for case in cases {
                    self.env.constructors.insert(case.name.clone(), (name.to_string(), case.clone()));
                }
            }
        }
        let key = match prefix {
            Some(p) => format!("{}.{}", p, name),
            None => name.to_string(),
        };
        self.env.types.insert(key, resolved);
    }

    fn register_top_let_decl(&mut self, name: &str, ty: Option<&ast::TypeExpr>, value: &ast::Expr,
                             visibility: &ast::Visibility, prefix: Option<&str>, is_external: bool) {
        if self.is_hidden_by_visibility(visibility, prefix, name, is_external) { return; }
        let resolved_ty = if let Some(te) = ty {
            self.resolve_type_expr(te)
        } else {
            self.infer_const_type(value)
        };
        let key = match prefix {
            Some(p) => format!("{}.{}", p, name),
            None => name.to_string(),
        };
        self.env.top_lets.insert(key, resolved_ty);
    }

    /// Infer the type of a constant expression (for top-level let without annotation).
    fn infer_const_type(&self, expr: &ast::Expr) -> Ty {
        match expr {
            ast::Expr::Int { .. } => Ty::Int,
            ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::String { .. } => Ty::String,
            ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::Unit { .. } => Ty::Unit,
            ast::Expr::None { .. } => Ty::Option(Box::new(Ty::Unknown)),
            ast::Expr::List { .. } => Ty::List(Box::new(Ty::Unknown)),
            ast::Expr::EmptyMap { .. } => Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            ast::Expr::Unary { op, operand, .. } if op == "-" => self.infer_const_type(operand),
            ast::Expr::Ident { name, .. } | ast::Expr::TypeName { name, .. } => {
                // Reference to another top-level let
                if let Some(ty) = self.env.top_lets.get(name) {
                    ty.clone()
                } else {
                    Ty::Unknown
                }
            }
            ast::Expr::Binary { op, left, right, .. } => {
                if op == "++" {
                    // Concat: could be String or List
                    let lt = self.infer_const_type(left);
                    if matches!(lt, Ty::String) { Ty::String } else { lt }
                } else {
                    // Arithmetic: infer from operands (Float wins over Int)
                    let lt = self.infer_const_type(left);
                    let rt = self.infer_const_type(right);
                    if matches!(lt, Ty::Float) || matches!(rt, Ty::Float) {
                        Ty::Float
                    } else {
                        lt
                    }
                }
            }
            ast::Expr::Paren { expr: inner, .. } => self.infer_const_type(inner),
            _ => Ty::Unknown,
        }
    }

    /// Register an imported module's exported functions and types.
    pub fn register_module(&mut self, mod_name: &str, prog: &ast::Program, pkg_id: Option<&crate::project::PkgId>, is_self_import: bool) {
        let is_external = !is_self_import;
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

    /// Type-check a module program's function bodies (for IR lowering).
    /// Returns a separate `expr_types` map to avoid id collisions with the main program.
    pub fn check_module_bodies(&mut self, prog: &mut ast::Program) -> std::collections::HashMap<crate::ast::ExprId, Ty> {
        let saved = std::mem::take(&mut self.expr_types);
        for decl in prog.decls.iter_mut() {
            self.check_decl(decl);
        }
        let module_types = std::mem::take(&mut self.expr_types);
        self.expr_types = saved;
        module_types
    }

    /// Register a user-level import alias (import pkg as alias).
    pub fn register_alias(&mut self, alias: &str, target: &str) {
        self.env.module_aliases.insert(alias.to_string(), target.to_string());
    }

    /// Set the build target (e.g. "wasm") to enable platform module gating.
    pub fn set_target(&mut self, target: &str) {
        self.target = Some(target.to_string());
    }

    fn is_wasm_target(&self) -> bool {
        self.target.as_ref().map_or(false, |t| t.starts_with("wasm"))
    }

    pub fn check_program(&mut self, prog: &mut ast::Program) -> Vec<Diagnostic> {
        // Check for platform module imports on WASM target
        if self.is_wasm_target() {
            for imp in &prog.imports {
                if let ast::Decl::Import { path, span, .. } = imp {
                    let mod_name = path.first().map(|s| s.as_str()).unwrap_or("");
                    if PLATFORM_MODULES.contains(&mod_name) {
                        let mut d = err(
                            format!("module '{}' is not available on WASM target", mod_name),
                            format!("'{}' requires OS access (file I/O, networking, etc.) which is not available in WebAssembly. Use only core modules (string, list, map, int, float, math, json, regex, path, time, args)", mod_name),
                            format!("import {}", path.join(".")),
                        );
                        if let Some(ref file) = self.source_file {
                            d.file = Some(file.clone());
                        }
                        d.line = span.map(|s| s.line);
                        self.diagnostics.push(d);
                    }
                }
            }
        }

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
                } else if (is_self_import && path.len() >= 2) || path.len() > 1 {
                    // import self.xxx or import pkg.sub → accessible as last segment
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
            ast::Decl::Fn { name, params, return_type, body, effect, extern_attrs, generics, .. } => {
                self.check_fn_decl_body(name, params, return_type, body, *effect, extern_attrs, generics);
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
            ast::Decl::TopLet { name, value, .. } => {
                // Validate that value is a constant expression
                let known_consts: std::collections::HashSet<String> = self.env.top_lets.keys().cloned().collect();
                if !is_top_let_const_expr(value, &known_consts) {
                    self.push_diagnostic(err(
                        format!("top-level 'let {}' value must be a constant expression", name),
                        "Allowed: literals, references to earlier top-level let values, arithmetic on constants, string concatenation (++)",
                        format!("let {}", name),
                    ));
                }
            }
            ast::Decl::Module { path, .. } => {
                self.push_diagnostic(Diagnostic::warning(
                    format!("'module {}' declaration is deprecated and will be removed in a future version", path.join(".")),
                    "Remove the 'module' declaration — file path determines the module name",
                    format!("module {}", path.join(".")),
                ));
            }
            _ => {}
        }
    }

    fn check_fn_decl_body(&mut self, name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
                          body: &mut Option<ast::Expr>, effect: Option<bool>,
                          extern_attrs: &[ast::ExternAttr], generics: &Option<Vec<ast::GenericParam>>) {
        // Warn about @extern type safety: compiler trusts declared signature without verification
        if !extern_attrs.is_empty() {
            self.push_diagnostic(crate::diagnostic::Diagnostic::warning(
                format!("function '{}' uses @extern — type signature is not verified at the FFI boundary", name),
                "Ensure the external function's actual types match the declared signature. Mismatches cause undefined behavior in Rust target.",
                format!("fn {}", name),
            ));
        }
        if body.is_none() && extern_attrs.is_empty() {
            self.push_diagnostic(err(
                format!("function '{}' has no body and no @extern declarations", name),
                "Add a body with '= expr' or add @extern annotations",
                format!("fn {}", name),
            ));
        }
        if body.is_none() {
            let has_rs = extern_attrs.iter().any(|a| a.target == "rs");
            let has_ts = extern_attrs.iter().any(|a| a.target == "ts");
            if !has_rs || !has_ts {
                let missing: Vec<&str> = [("rs", has_rs), ("ts", has_ts)]
                    .iter().filter(|(_, has)| !has).map(|(t, _)| *t).collect();
                self.push_diagnostic(err(
                    format!("function '{}' has no body and is missing @extern for: {}", name, missing.join(", ")),
                    "Add a body as fallback or add the missing @extern declarations",
                    format!("fn {}", name),
                ));
            }
        }
        if let Some(body) = body {
            self.env.push_scope();
            if let Some(gs) = generics {
                for g in gs {
                    self.env.types.insert(g.name.clone(), Ty::TypeVar(g.name.clone()));
                    if let Some(ref bound_te) = g.structural_bound {
                        let bound_ty = self.resolve_type_expr(bound_te);
                        // Structural bounds are open records: {name: String} means "has at least field name"
                        let bound_ty = match bound_ty {
                            Ty::Record { fields } => Ty::OpenRecord { fields },
                            other => other,
                        };
                        self.env.structural_bounds.insert(g.name.clone(), bound_ty);
                    }
                }
            }
            for p in params.iter() {
                let ty = self.resolve_type_expr(&p.ty);
                self.env.define_var(&p.name, ty);
                self.env.param_vars.insert(p.name.clone());
            }
            let ret_ty = self.resolve_type_expr(return_type);
            let prev_ret = self.env.current_ret.take();
            let prev_effect = self.env.in_effect;
            self.env.current_ret = Some(ret_ty.clone());
            self.env.in_effect = effect.unwrap_or(false);
            let body_ty = self.check_expr_with(body, Some(&ret_ty));
            let is_effect = effect.unwrap_or(false);
            let effective_ret = if is_effect {
                match &ret_ty {
                    Ty::Result(ok_ty, _) => *ok_ty.clone(),
                    _ => ret_ty.clone(),
                }
            } else {
                ret_ty.clone()
            };
            let resolved_body = self.env.resolve_named(&body_ty);
            let resolved_ret = self.env.resolve_named(&effective_ret);
            let resolved_ret_full = self.env.resolve_named(&ret_ty);
            if !resolved_body.compatible(&resolved_ret) && !resolved_body.compatible(&resolved_ret_full) && !body_ty.compatible(&effective_ret) && !body_ty.compatible(&ret_ty) {
                let hint = Self::hint_with_conversion(
                    "Change the return type or fix the body expression",
                    &ret_ty, &body_ty,
                );
                self.push_diagnostic(err(
                    format!("function '{}' declared to return {} but body has type {}", name, ret_ty.display(), body_ty.display()),
                    hint,
                    format!("fn {}", name),
                ));
            }
            self.warn_unused_vars_in_scope(&format!("fn {}", name));
            self.check_lost_list_return(name, params, &ret_ty, body);
            self.env.current_ret = prev_ret;
            self.env.in_effect = prev_effect;
            if let Some(gs) = generics {
                for g in gs {
                    self.env.types.remove(&g.name);
                    self.env.structural_bounds.remove(&g.name);
                }
            }
            self.env.param_vars.clear();
            self.env.pop_scope();
        }
    }

    pub(crate) fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
        match te {
            ast::TypeExpr::Simple { name } => match name.as_str() {
                "Int" => Ty::Int, "Float" => Ty::Float, "String" => Ty::String,
                "Bool" => Ty::Bool, "Unit" => Ty::Unit, "Path" => Ty::String,
                other => {
                    // Check if this name is a registered type (could be TypeVar from generics)
                    if let Some(ty) = self.env.types.get(other) {
                        ty.clone()
                    } else {
                        Ty::Named(other.to_string(), vec![])
                    }
                }
            },
            ast::TypeExpr::Generic { name, args } => {
                let ra: Vec<Ty> = args.iter().map(|a| self.resolve_type_expr(a)).collect();
                match name.as_str() {
                    "List" if ra.len() == 1 => Ty::List(Box::new(ra[0].clone())),
                    "Option" if ra.len() == 1 => Ty::Option(Box::new(ra[0].clone())),
                    "Result" if ra.len() == 2 => Ty::Result(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Map" if ra.len() == 2 => Ty::Map(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Set" => Ty::List(Box::new(ra.first().cloned().unwrap_or(Ty::Unknown))),
                    _ => Ty::Named(name.clone(), ra),
                }
            }
            ast::TypeExpr::Record { fields } => Ty::Record {
                fields: fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect(),
            },
            ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
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
            ast::TypeExpr::Union { members } => {
                let tys: Vec<Ty> = members.iter().map(|m| self.resolve_type_expr(m)).collect();
                Ty::union(tys)
            }
            ast::TypeExpr::Variant { cases } => {
                let cs: Vec<VariantCase> = cases.iter().map(|c| match c {
                    ast::VariantCase::Unit { name } => VariantCase { name: name.clone(), payload: VariantPayload::Unit },
                    ast::VariantCase::Tuple { name, fields } => VariantCase {
                        name: name.clone(),
                        payload: VariantPayload::Tuple(fields.iter().map(|f| self.resolve_type_expr(f)).collect()),
                    },
                    ast::VariantCase::Record { name, fields } => VariantCase {
                        name: name.clone(),
                        payload: VariantPayload::Record(fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty), f.default.clone())).collect()),
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
            self.push_diagnostic(Diagnostic::error(
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

    /// Validate tuple arity and return element types.
    /// Returns a Vec of types matching `expected` count.
    /// Emits a diagnostic if the tuple has a different number of elements.
    /// For non-tuple / Unknown types, returns `vec![Ty::Unknown; expected]` silently.
    pub(crate) fn resolve_tuple_elements(&mut self, ty: &Ty, expected: usize, context: impl Into<String>) -> Vec<Ty> {
        match ty {
            Ty::Tuple(elements) => {
                if elements.len() != expected {
                    self.push_diagnostic(err(
                        format!("tuple has {} elements but {} expected", elements.len(), expected),
                        format!("The value has type {}", ty.display()),
                        context,
                    ));
                }
                (0..expected).map(|i| elements.get(i).cloned().unwrap_or(Ty::Unknown)).collect()
            }
            _ => vec![Ty::Unknown; expected],
        }
    }

    fn register_stdlib(&mut self) {
        for name in stdlib::builtin_effect_fns() {
            self.env.effect_fns.insert(name.to_string());
        }
    }

    /// Suggest a type conversion function when a type mismatch occurs.
    /// Returns a hint string if a known conversion path exists between the types.
    pub(crate) fn suggest_conversion(expected: &Ty, actual: &Ty) -> Option<String> {
        match (actual, expected) {
            (Ty::Int, Ty::String) => Some("use `int.to_string(x)` to convert Int to String".to_string()),
            (Ty::Float, Ty::String) => Some("use `float.to_string(x)` to convert Float to String".to_string()),
            (Ty::Bool, Ty::String) => Some("use `to_string(x)` to convert Bool to String".to_string()),
            (Ty::String, Ty::Int) => Some("use `int.parse(s)` to convert String to Int (returns Result[Int, String])".to_string()),
            (Ty::String, Ty::Float) => Some("use `float.parse(s)` to convert String to Float (returns Result[Float, String])".to_string()),
            (Ty::Float, Ty::Int) => Some("use `to_int(x)` to convert Float to Int (truncates)".to_string()),
            (Ty::Int, Ty::Float) => Some("use `to_float(x)` to convert Int to Float".to_string()),
            _ => None,
        }
    }

    /// Build a hint string, appending a conversion suggestion if available.
    pub(crate) fn hint_with_conversion(base_hint: &str, expected: &Ty, actual: &Ty) -> String {
        if let Some(conv) = Self::suggest_conversion(expected, actual) {
            format!("{}. Or {}", base_hint, conv)
        } else {
            base_hint.to_string()
        }
    }

    /// Suggest similar names for "did you mean?" errors.
    pub(crate) fn suggest_similar(&self, name: &str, kind: &str) -> Option<String> {
        let candidates: Vec<&str> = match kind {
            "function" => self.env.functions.keys().map(|s| s.as_str())
                .chain(["println", "eprintln", "assert", "assert_eq", "assert_ne", "ok", "err", "some"].iter().copied())
                .collect(),
            "variable" => self.env.scopes.iter().rev()
                .flat_map(|s| s.keys().map(|k| k.as_str()))
                .collect(),
            _ => return None,
        };
        let mut best: Option<(&str, usize)> = None;
        let threshold = (name.len().max(1) * 2 / 5).max(1).min(3);
        for c in &candidates {
            let d = levenshtein(name, c);
            if d > 0 && d <= threshold {
                if best.is_none() || d < best.unwrap().1 {
                    best = Some((c, d));
                }
            }
        }
        best.map(|(s, _)| s.to_string())
    }

    /// Suggest similar module function names.
    pub(crate) fn suggest_module_fn(&self, module: &str, func: &str) -> Option<String> {
        let candidates = stdlib::module_functions(module);
        let mut best: Option<(&str, usize)> = None;
        // Allow up to 40% of the longer string's length as threshold (min 1, max 3)
        let threshold = (func.len().max(1) * 2 / 5).max(1).min(3);
        for c in &candidates {
            let d = levenshtein(func, c);
            if d > 0 && d <= threshold {
                if best.is_none() || d < best.unwrap().1 {
                    best = Some((c, d));
                }
            }
        }
        // Also check substring containment (e.g., "length" → "len" if func contains candidate)
        if best.is_none() {
            for c in &candidates {
                if func.contains(c) || c.contains(func) {
                    best = Some((c, 0));
                    break;
                }
            }
        }
        best.map(|(s, _)| s.to_string())
    }

    /// List mutation functions whose first arg is the collection being modified.
    const LIST_MUTATION_FNS: &'static [&'static str] = &[
        "set", "swap", "push", "insert", "remove_at", "sort", "reverse",
    ];

    /// Check if a function modifies list-typed parameters but doesn't return them.
    /// Suggests tuple return pattern when mutations would otherwise be lost.
    fn check_lost_list_return(&mut self, name: &str, params: &[ast::Param], ret_ty: &Ty, body: &ast::Expr) {
        // Collect list-typed parameter names
        let list_params: std::collections::HashSet<String> = params.iter()
            .filter(|p| matches!(self.resolve_type_expr(&p.ty), Ty::List(_)))
            .map(|p| p.name.clone())
            .collect();
        if list_params.is_empty() {
            return;
        }
        // Check if return type already contains a List
        if Self::ty_contains_list(ret_ty) {
            return;
        }
        // Walk body to find list mutation calls on parameters
        let mut mutated_params = std::collections::HashSet::new();
        Self::find_list_mutations(body, &list_params, &mut mutated_params);
        if mutated_params.is_empty() {
            return;
        }
        let param_names: Vec<&str> = mutated_params.iter().map(|s| s.as_str()).collect();
        let hint = if param_names.len() == 1 {
            let p = param_names[0];
            format!(
                "'{}' is modified via list.set/swap/push but not included in the return type. \
                 Return the modified list alongside the result: -> ({}, {})",
                p, "List[T]", ret_ty.display()
            )
        } else {
            format!(
                "{} are modified but not returned. Use a tuple return to include them.",
                param_names.join(", ")
            )
        };
        self.push_diagnostic(Diagnostic::warning(
            format!("function '{}' modifies list parameter(s) but doesn't return them", name),
            hint,
            format!("fn {}", name),
        ));
    }

    /// Check if a Ty contains a List anywhere (direct, in tuple, result, option, etc.)
    fn ty_contains_list(ty: &Ty) -> bool {
        match ty {
            Ty::List(_) => true,
            Ty::Tuple(elems) => elems.iter().any(Self::ty_contains_list),
            Ty::Result(ok, err) => Self::ty_contains_list(ok) || Self::ty_contains_list(err),
            Ty::Option(inner) => Self::ty_contains_list(inner),
            _ => false,
        }
    }

    /// Walk an expression tree to find `list.set(param, ...)` / `param.set(...)` calls.
    fn find_list_mutations(expr: &ast::Expr, list_params: &std::collections::HashSet<String>, out: &mut std::collections::HashSet<String>) {
        match expr {
            ast::Expr::Call { callee, args, .. } => {
                if let ast::Expr::Member { object, field, .. } = callee.as_ref() {
                    let func = field.as_str();
                    if Self::LIST_MUTATION_FNS.contains(&func) {
                        // Module call: list.set(param, ...)
                        if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                            if module == "list" {
                                if let Some(ast::Expr::Ident { name: arg0, .. }) = args.first() {
                                    if list_params.contains(arg0) {
                                        out.insert(arg0.clone());
                                    }
                                }
                            }
                        }
                        // UFCS: param.set(...)
                        if let ast::Expr::Ident { name: receiver, .. } = object.as_ref() {
                            if list_params.contains(receiver) {
                                out.insert(receiver.clone());
                            }
                        }
                    }
                }
                // Recurse into callee and args
                Self::find_list_mutations(callee, list_params, out);
                for a in args {
                    Self::find_list_mutations(a, list_params, out);
                }
            }
            ast::Expr::Block { stmts, expr, .. } | ast::Expr::DoBlock { stmts, expr, .. } => {
                for s in stmts {
                    Self::find_list_mutations_in_stmt(s, list_params, out);
                }
                if let Some(e) = expr {
                    Self::find_list_mutations(e, list_params, out);
                }
            }
            ast::Expr::If { cond, then, else_, .. } => {
                Self::find_list_mutations(cond, list_params, out);
                Self::find_list_mutations(then, list_params, out);
                Self::find_list_mutations(else_, list_params, out);
            }
            ast::Expr::Match { subject, arms, .. } => {
                Self::find_list_mutations(subject, list_params, out);
                for arm in arms {
                    Self::find_list_mutations(&arm.body, list_params, out);
                }
            }
            ast::Expr::ForIn { iterable, body, .. } => {
                Self::find_list_mutations(iterable, list_params, out);
                for s in body {
                    Self::find_list_mutations_in_stmt(s, list_params, out);
                }
            }
            ast::Expr::Binary { left, right, .. } | ast::Expr::Pipe { left, right, .. } => {
                Self::find_list_mutations(left, list_params, out);
                Self::find_list_mutations(right, list_params, out);
            }
            ast::Expr::Unary { operand, .. } | ast::Expr::Paren { expr: operand, .. }
            | ast::Expr::Try { expr: operand, .. } | ast::Expr::Await { expr: operand, .. }
            | ast::Expr::Some { expr: operand, .. } | ast::Expr::Ok { expr: operand, .. }
            | ast::Expr::Err { expr: operand, .. } => {
                Self::find_list_mutations(operand, list_params, out);
            }
            ast::Expr::Lambda { body, .. } => {
                Self::find_list_mutations(body, list_params, out);
            }
            ast::Expr::Tuple { elements, .. } | ast::Expr::List { elements, .. } => {
                for e in elements {
                    Self::find_list_mutations(e, list_params, out);
                }
            }
            ast::Expr::MapLiteral { entries, .. } => {
                for (k, v) in entries {
                    Self::find_list_mutations(k, list_params, out);
                    Self::find_list_mutations(v, list_params, out);
                }
            }
            ast::Expr::EmptyMap { .. } => {}
            ast::Expr::Member { object, .. } | ast::Expr::TupleIndex { object, .. } => {
                Self::find_list_mutations(object, list_params, out);
            }
            ast::Expr::Record { fields, .. } => {
                for f in fields {
                    Self::find_list_mutations(&f.value, list_params, out);
                }
            }
            ast::Expr::SpreadRecord { base, fields, .. } => {
                Self::find_list_mutations(base, list_params, out);
                for f in fields {
                    Self::find_list_mutations(&f.value, list_params, out);
                }
            }
            _ => {}
        }
    }

    fn find_list_mutations_in_stmt(stmt: &ast::Stmt, list_params: &std::collections::HashSet<String>, out: &mut std::collections::HashSet<String>) {
        match stmt {
            ast::Stmt::Let { value, .. } | ast::Stmt::Var { value, .. } => {
                Self::find_list_mutations(value, list_params, out);
            }
            ast::Stmt::Assign { value, .. } => {
                Self::find_list_mutations(value, list_params, out);
            }
            ast::Stmt::IndexAssign { index, value, .. } => {
                Self::find_list_mutations(index, list_params, out);
                Self::find_list_mutations(value, list_params, out);
            }
            ast::Stmt::FieldAssign { value, .. } => {
                Self::find_list_mutations(value, list_params, out);
            }
            ast::Stmt::Expr { expr, .. } => {
                Self::find_list_mutations(expr, list_params, out);
            }
            ast::Stmt::Guard { cond, else_, .. } => {
                Self::find_list_mutations(cond, list_params, out);
                Self::find_list_mutations(else_, list_params, out);
            }
            ast::Stmt::LetDestructure { value, .. } => {
                Self::find_list_mutations(value, list_params, out);
            }
            ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
        }
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (m, n) = (a.len(), b.len());
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
