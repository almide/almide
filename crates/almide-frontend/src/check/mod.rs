/// Almide type checker: AST → TypeMap (constraint-based type inference).
///
/// Input:    &mut Program (with canonicalized TypeEnv)
/// Output:   TypeMap (ExprId→Ty), diagnostics
/// Owns:     type inference (constraint collect → solve), exhaustiveness, type errors
/// Does NOT: auto-unwrap (codegen's job), code generation, optimization
///
/// Architecture:
///   Pass 1: Walk AST, assign fresh type variables to TypeMap, collect constraints (infer.rs)
///   Pass 2: Solve constraints via unification (solving.rs)
///   Pass 3: Resolve TypeVars in TypeMap values (mod.rs)
///
/// Split into:
///   mod.rs          — Checker struct, public API, declaration checking
///   types.rs        — TyVarId, Constraint, resolve_vars
///   infer.rs        — Expression/statement inference
///   calls.rs        — Function call resolution
///   registration.rs — Function/type/protocol declaration registration
///   solving.rs      — Constraint solving (unification)
///   diagnostics.rs  — Error hint helpers

mod types;
mod infer;
pub(crate) mod calls;
mod builtin_calls;
mod static_dispatch;
mod solving;
mod diagnostics;
mod exhaustiveness;

use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use crate::import_table::{ImportTable, build_import_table};
use almide_base::intern::sym;
use crate::types::{Ty, TypeEnv};
use types::{TyVarId, Constraint, FixHint, UnionFind, resolve_ty};

pub(crate) fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

pub struct Checker {
    pub env: TypeEnv,
    pub type_map: crate::types::TypeMap,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
    pub(crate) current_span: Option<crate::ast::Span>,
    /// Span of the current call's callee expression (the identifier
    /// / member reference). Set by `check_named_call_spanned` so E002
    /// can emit a `try_replace` range pointing exactly at the name
    /// token rather than the whole call. Cleared after each callee.
    pub(crate) callee_span_hint: Option<crate::ast::Span>,
    pub(crate) constraints: Vec<Constraint>,
    pub(crate) uf: UnionFind,
}

impl Checker {
    /// Create a Checker from a pre-populated TypeEnv (from canonicalize_program).
    pub fn from_env(env: TypeEnv) -> Self {
        Checker {
            env, type_map: crate::types::TypeMap::new(),
            diagnostics: Vec::new(),
            source_file: None, source_text: None,
            current_span: None,
            callee_span_hint: None,
            constraints: Vec::new(), uf: UnionFind::new(),
        }
    }

    /// Push a diagnostic, automatically attaching the current expression's span.
    pub(crate) fn emit(&mut self, mut diag: Diagnostic) {
        if diag.line.is_none() {
            if let Some(span) = &self.current_span {
                if let Some(file) = &self.source_file {
                    diag.file = Some(file.clone());
                }
                diag.line = Some(span.line);
                diag.col = Some(span.col);
                if span.end_col > span.col {
                    diag.end_col = Some(span.end_col);
                }
            }
        }
        self.diagnostics.push(diag);
    }

    pub(crate) fn fresh_var(&mut self) -> Ty {
        let id = self.uf.fresh();
        Ty::TypeVar(sym(&format!("?{}", id)))
    }

    /// Let-polymorphism: instantiate で TypeVar("?N") を fresh var に置換
    /// 同じ let binding を2回参照する時、各参照で独立した型変数を使う
    pub(crate) fn instantiate_ty(&mut self, ty: &Ty) -> Ty {
        let mut mapping: std::collections::HashMap<u32, TyVarId> = std::collections::HashMap::new();
        self.instantiate_inner(ty, &mut mapping)
    }

    fn instantiate_inner(&mut self, ty: &Ty, mapping: &mut std::collections::HashMap<u32, TyVarId>) -> Ty {
        // Inference variables (?N) must NOT be freshened — they need to stay
        // linked to the original constraint.
        if matches!(ty, Ty::TypeVar(name) if name.starts_with('?')) {
            return ty.clone();
        }
        // Recursively instantiate all children
        ty.map_children_mut(&mut |child| self.instantiate_inner(child, mapping))
    }

    pub(crate) fn constrain(&mut self, expected: Ty, actual: Ty, context: impl Into<String>) {
        self.constrain_with_hint(expected, actual, context, None);
    }

    pub(crate) fn constrain_with_hint(
        &mut self,
        expected: Ty,
        actual: Ty,
        context: impl Into<String>,
        fix_hint: Option<FixHint>,
    ) {
        let ctx = context.into();
        self.unify_infer(&expected, &actual);
        self.constraints.push(Constraint {
            expected, actual, context: ctx,
            span: self.current_span,
            fix_hint,
        });
    }

    pub fn set_source(&mut self, file: &str, text: &str) { self.source_file = Some(file.into()); self.source_text = Some(text.into()); }

    // ── Main entry point ──

    /// Type-check a program whose environment was pre-populated by `canonicalize_program`.
    /// Skips import table building and declaration registration — inference only.
    pub fn infer_program(&mut self, program: &mut ast::Program) -> Vec<Diagnostic> {
        for decl in program.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        resolve_type_map(&mut self.type_map, &self.uf);
        // Unused import warnings
        for imp in &program.imports {
            let (path, alias, span) = match imp {
                ast::Decl::Import { path, alias, span, .. } => (path, alias, span),
                _ => continue,
            };
            let import_name = alias.as_ref().cloned()
                .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
            if import_name.is_empty()
                || self.env.import_table.used.contains(&sym(&import_name))
                || import_name.starts_with('_')
                || path.first().map(|s| s.as_str()) == Some("self")
            { continue; }
            let line = span.as_ref().map(|s| s.line).unwrap_or(0);
            self.diagnostics.push(Diagnostic::warning(
                format!("unused import '{}'", import_name),
                format!("Remove the import or prefix with '_' to suppress: _{}", import_name),
                format!("import at line {}", line),
            ));
        }
        std::mem::take(&mut self.diagnostics)
    }

    /// Type-check a module's declarations. Populates type_map for all expressions.
    /// Temporarily registers unprefixed declarations for intra-module resolution,
    /// then cleans them up.
    pub fn infer_module(&mut self, prog: &mut ast::Program, module_name: &str) {
        // Isolate module's constraint solving and type map from the main program
        let saved_constraints = std::mem::take(&mut self.constraints);
        let saved_uf = std::mem::replace(&mut self.uf, UnionFind::new());
        self.type_map.clear();

        // Build module's import table
        let self_name = self.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(module_name);
        let saved_import_table = std::mem::replace(&mut self.env.import_table, ImportTable::new());
        let (mod_table, diags) = build_import_table(prog, Some(import_table_name), &self.env.user_modules);
        self.env.import_table = mod_table;
        self.diagnostics.extend(diags);

        // Temporarily register unprefixed declarations for intra-module resolution
        let snapshot = self.env.snapshot_keys();
        crate::canonicalize::registration::register_decls(
            &mut self.env, &mut self.diagnostics, &prog.decls, None,
        );

        // Infer + solve + resolve
        for decl in prog.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        resolve_type_map(&mut self.type_map, &self.uf);

        // Restore
        self.constraints = saved_constraints;
        self.uf = saved_uf;
        self.env.import_table = saved_import_table;
        self.env.restore_keys(&snapshot);
    }

    // ── Declaration checking ──

    /// Push generic type vars, structural bounds, and protocol bounds into the environment.
    fn enter_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.insert(sym(&g.name), Ty::TypeVar(sym(&g.name)));
            if let Some(bte) = &g.structural_bound {
                let bt = self.resolve_type_expr(bte);
                self.env.structural_bounds.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
            }
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() {
                    self.env.generic_protocol_bounds.insert(sym(&g.name), bounds.iter().map(|b| sym(b)).collect());
                }
            }
        }
    }

    /// Remove generic type vars, structural bounds, and protocol bounds from the environment.
    fn exit_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.remove(&sym(&g.name));
            self.env.structural_bounds.remove(&sym(&g.name));
            self.env.generic_protocol_bounds.remove(&sym(&g.name));
        }
    }

    /// Constrain an effect fn body against its return type signature.
    /// Effect fns accept: Unit body (control-flow returns), unwrapped T, or full Result[T, E].
    fn constrain_effect_body(&mut self, name: &str, ret_ty: &Ty, body_ty: Ty) {
        let body_resolved = resolve_ty(&body_ty, &self.uf);
        if body_resolved == Ty::Unit { return; } // while loops, guard patterns return via control flow
        if let Ty::Applied(crate::types::TypeConstructorId::Result, args) = ret_ty {
            // ret_ty is Result[T, E]: body can be Result[T, E] or unwrapped T
            if args.len() >= 1 {
                let ok = &args[0];
                if body_resolved.is_result() {
                    self.constrain(ret_ty.clone(), body_ty, format!("fn '{}'", name));
                } else {
                    self.constrain(ok.clone(), body_ty, format!("fn '{}'", name));
                }
                return;
            }
        }
        // ret_ty is non-Result (e.g. String): body can be T or Result[T, E] (auto-unwrapped)
        if let Ty::Applied(crate::types::TypeConstructorId::Result, ref args) = body_resolved {
            if args.len() >= 1 {
                self.constrain(ret_ty.clone(), args[0].clone(), format!("fn '{}'", name));
                return;
            }
        }
        self.constrain(ret_ty.clone(), body_ty, format!("fn '{}'", name));
    }

    fn check_fn_decl(
        &mut self,
        name: &str,
        params: &mut [ast::Param],
        return_type: &ast::TypeExpr,
        body: &mut ast::Expr,
        effect: &Option<bool>,
        generics: &mut Option<Vec<ast::GenericParam>>,
    ) {
        self.env.push_scope();
        self.enter_generics(generics);
        for p in params.iter_mut() {
            let ty = self.resolve_type_expr(&p.ty);
            self.env.define_var(&p.name, ty.clone());
            self.env.param_vars.insert(sym(&p.name));
            if let Some(ref mut default_expr) = p.default {
                let dty = self.infer_expr(default_expr);
                self.constrain(ty, dty, format!("default arg '{}'", p.name));
            }
        }
        let ret_ty = self.resolve_type_expr(return_type);
        let prev = (self.env.current_ret.take(), self.env.can_call_effect, self.env.auto_unwrap, self.env.lambda_depth);
        let is_effect = effect.unwrap_or(false);
        self.env.current_ret = Some(ret_ty.clone());
        self.env.can_call_effect = is_effect;
        self.env.auto_unwrap = is_effect;
        self.env.lambda_depth = 0;
        let body_ity = self.infer_expr(body);
        if effect.unwrap_or(false) {
            self.constrain_effect_body(name, &ret_ty, body_ity);
        } else {
            // Capture the trailing `let` binding name (if any) to specialize
            // the Unit-leak E001 try: snippet downstream.
            let hint = trailing_let_name(body).map(FixHint::LastLetName);
            self.constrain_with_hint(ret_ty, body_ity, format!("fn '{}'", name), hint);
        }
        self.env.current_ret = prev.0; self.env.can_call_effect = prev.1; self.env.auto_unwrap = prev.2; self.env.lambda_depth = prev.3;
        self.exit_generics(generics);
        self.env.pop_scope();
    }

    fn check_decl(&mut self, decl: &mut ast::Decl) {
        match decl {
            ast::Decl::Fn { name, params, return_type, body: Some(body), effect, generics, .. } => {
                self.check_fn_decl(name, params, return_type, body, effect, generics);
            }
            ast::Decl::Test { body, .. } => {
                self.env.push_scope();
                let prev_call = self.env.can_call_effect; self.env.can_call_effect = true;
                let prev_test = self.env.in_test_block; self.env.in_test_block = true;
                self.infer_expr(body);
                self.env.in_test_block = prev_test;
                self.env.can_call_effect = prev_call;
                self.env.pop_scope();
            }
            ast::Decl::TopLet { name, value, .. } => {
                let ity = self.infer_expr(value);
                let resolved = resolve_ty(&ity, &self.uf);
                // Update env.top_lets with the fully inferred type
                if matches!(self.env.top_lets.get(&sym(name)), Some(Ty::Unknown) | None) {
                    self.env.top_lets.insert(sym(name), resolved);
                }
            }
            ast::Decl::Impl { methods, .. } => {
                for m in methods.iter_mut() {
                    self.check_decl(m);
                }
            }
            ast::Decl::Type { ty, .. } => {
                // Infer types for default value expressions in variant record fields
                infer_default_exprs(self, ty);
            }
            _ => {}
        }
    }

    // ── Exhaustiveness ──

}

/// Infer types for default value expressions in type declarations.
/// Prevents ICE "missing type for expr" during lowering.
fn infer_default_exprs(checker: &mut Checker, ty: &mut ast::TypeExpr) {
    if let ast::TypeExpr::Variant { cases } = ty {
        for case in cases {
            if let ast::VariantCase::Record { fields, .. } = case {
                for field in fields {
                    if let Some(ref mut default_expr) = field.default {
                        checker.infer_expr(default_expr);
                    }
                }
            }
        }
    }
}

impl Checker {

    pub(crate) fn check_match_exhaustiveness(&mut self, subject_ty: &Ty, arms: &[ast::MatchArm]) {
        let missing = exhaustiveness::check_exhaustiveness(subject_ty, arms, &self.env);
        if !missing.is_empty() {
            let list = missing
                .iter()
                .map(|m| m.pattern.clone())
                .collect::<Vec<_>>()
                .join(", ");
            let resolved = self.env.resolve_named(subject_ty);
            let hint = if missing.len() == 1 && missing[0].pattern == "_" {
                let ty_name = match &resolved {
                    Ty::Int => "Int",
                    Ty::Float => "Float",
                    Ty::String => "String",
                    _ => "this type",
                };
                format!("match on {} requires a catch-all '_' pattern", ty_name)
            } else {
                // Paste-ready arms: indent + join with newlines so the LLM
                // (or user) can copy the block straight into the source.
                // `_ => todo()` is appended as a fallback for incremental
                // compilation, mirroring Rust's `unimplemented!()` idiom.
                let arms_block = missing
                    .iter()
                    .map(|m| format!("  {}", m.arm_template))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "add arms for {}:\n{}\nOr use `_ => todo()` to compile incrementally.",
                    list, arms_block
                )
            };
            self.emit(Diagnostic::error(
                format!("non-exhaustive match: missing {}", list),
                hint,
                "match",
            ).with_code("E010"));
        }
    }

    // ── Type resolution ──

    pub fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
        crate::canonicalize::resolve::resolve_type_expr(te, Some(&self.env.types))
    }

    pub(crate) fn resolve_field_type(&mut self, ty: &Ty, field: &str) -> Ty {
        let resolved = self.env.resolve_named(ty);
        match &resolved {
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown),
            Ty::TypeVar(tv) => {
                // First check existing structural bounds
                if let Some(bound) = self.env.structural_bounds.get(tv).cloned() {
                    let result = self.resolve_field_type(&bound, field);
                    if !matches!(result, Ty::Unknown) {
                        return result;
                    }
                }
                // Search env.types for record types with this field.
                // Only unify if exactly one candidate exists (unambiguous).
                let field_sym = almide_base::intern::sym(field);
                let mut candidates: Vec<(almide_base::intern::Sym, Ty)> = Vec::new();
                for (_name, reg_ty) in &self.env.types {
                    match reg_ty {
                        Ty::Record { fields } | Ty::OpenRecord { fields } => {
                            if let Some((_, fty)) = fields.iter().find(|(n, _)| *n == field_sym) {
                                candidates.push((*_name, fty.clone()));
                            }
                        }
                        _ => {}
                    }
                }
                if candidates.len() == 1 {
                    let (type_name, field_ty) = candidates.pop().unwrap();
                    let named = Ty::Named(type_name, vec![]);
                    self.unify_infer(ty, &named);
                    field_ty
                } else {
                    Ty::Unknown
                }
            }
            _ => Ty::Unknown,
        }
    }
}

/// Resolve inferred TypeVars in the type map after constraint solving.
fn resolve_type_map(type_map: &mut crate::types::TypeMap, uf: &UnionFind) {
    for ty in type_map.values_mut() {
        *ty = resolve_ty(ty, uf);
    }
}

/// If `expr` is a block whose value comes from a trailing `let` binding
/// (i.e. no tail expression, last statement is `Stmt::Let { name, .. }`),
/// return that binding name. This is the top dojo E001 anti-pattern:
/// `fn f() -> Int = { let x = ...  }` — the fn returns Unit because a
/// bare `let` evaluates to Unit, not to the bound value.
fn trailing_let_name(expr: &ast::Expr) -> Option<String> {
    let ast::ExprKind::Block { stmts, expr: tail } = &expr.kind else { return None };
    if tail.is_some() { return None; }
    match stmts.last()? {
        ast::Stmt::Let { name, .. } | ast::Stmt::Var { name, .. } => Some(name.to_string()),
        _ => None,
    }
}
