/// Almide type checker: AST → Typed AST (constraint-based type inference).
///
/// Input:    &mut Program
/// Output:   expr_types: HashMap<ExprId, Ty>, diagnostics
/// Owns:     type inference (constraint collect → solve), exhaustiveness, type errors
/// Does NOT: auto-unwrap (codegen's job), code generation, optimization
///
/// Architecture:
///   Pass 1: Walk AST, assign fresh type variables, collect constraints (infer.rs)
///   Pass 2: Solve constraints via unification (mod.rs)
///   Pass 3: Substitute solved types into expr_types (mod.rs)
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
mod registration;
mod solving;
mod diagnostics;

use std::collections::HashMap;
use crate::ast;
use crate::ast::ExprId;
use crate::diagnostic::Diagnostic;
use crate::intern::sym;
use crate::types::{Ty, TypeEnv, VariantCase, VariantPayload, ProtocolDef, ProtocolMethodSig};
use types::{TyVarId, Constraint, UnionFind, resolve_ty};

pub(crate) fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

pub struct Checker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
    pub expr_types: HashMap<ExprId, Ty>,
    /// Current expression span — set by infer_expr, used to annotate diagnostics
    pub(crate) current_span: Option<crate::ast::Span>,
    // Inference state
    pub(crate) infer_types: HashMap<ExprId, Ty>,
    pub(crate) constraints: Vec<Constraint>,
    pub(crate) uf: UnionFind,
}

impl Checker {
    pub fn new() -> Self {
        let mut checker = Checker {
            env: TypeEnv::new(), diagnostics: Vec::new(),
            source_file: None, source_text: None,
            expr_types: HashMap::new(), current_span: None,
            infer_types: HashMap::new(),
            constraints: Vec::new(), uf: UnionFind::new(),
        };
        checker.register_builtin_protocols();
        checker
    }

    /// Register built-in conventions (Eq, Repr, Ord, Hash, Codec, Encode, Decode) as protocols.
    fn register_builtin_protocols(&mut self) {
        let self_ty = Ty::TypeVar(sym("Self"));
        let value_ty = Ty::Named(sym("Value"), vec![]);

        // Eq: fn eq(a: Self, b: Self) -> Bool
        self.env.protocols.insert("Eq".into(), ProtocolDef {
            name: "Eq".into(),
            generics: vec![],
            methods: vec![ProtocolMethodSig {
                name: "eq".into(),
                params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
                ret: Ty::Bool,
                is_effect: false,
            }],
        });

        // Repr: fn repr(v: Self) -> String
        self.env.protocols.insert("Repr".into(), ProtocolDef {
            name: "Repr".into(),
            generics: vec![],
            methods: vec![ProtocolMethodSig {
                name: "repr".into(),
                params: vec![("v".into(), self_ty.clone())],
                ret: Ty::String,
                is_effect: false,
            }],
        });

        // Ord: fn cmp(a: Self, b: Self) -> Int
        self.env.protocols.insert("Ord".into(), ProtocolDef {
            name: "Ord".into(),
            generics: vec![],
            methods: vec![ProtocolMethodSig {
                name: "cmp".into(),
                params: vec![("a".into(), self_ty.clone()), ("b".into(), self_ty.clone())],
                ret: Ty::Int,
                is_effect: false,
            }],
        });

        // Hash: fn hash(v: Self) -> Int
        self.env.protocols.insert("Hash".into(), ProtocolDef {
            name: "Hash".into(),
            generics: vec![],
            methods: vec![ProtocolMethodSig {
                name: "hash".into(),
                params: vec![("v".into(), self_ty.clone())],
                ret: Ty::Int,
                is_effect: false,
            }],
        });

        // Codec: fn encode(v: Self) -> Value, fn decode(v: Value) -> Result[Self, String]
        self.env.protocols.insert("Codec".into(), ProtocolDef {
            name: "Codec".into(),
            generics: vec![],
            methods: vec![
                ProtocolMethodSig {
                    name: "encode".into(),
                    params: vec![("v".into(), self_ty.clone())],
                    ret: value_ty.clone(),
                    is_effect: false,
                },
                ProtocolMethodSig {
                    name: "decode".into(),
                    params: vec![("v".into(), value_ty.clone())],
                    ret: Ty::result(self_ty.clone(), Ty::String),
                    is_effect: false,
                },
            ],
        });

        // Encode: fn encode(v: Self) -> Value
        self.env.protocols.insert("Encode".into(), ProtocolDef {
            name: "Encode".into(),
            generics: vec![],
            methods: vec![ProtocolMethodSig {
                name: "encode".into(),
                params: vec![("v".into(), self_ty.clone())],
                ret: value_ty.clone(),
                is_effect: false,
            }],
        });

        // Decode: fn decode(v: Value) -> Result[Self, String]
        self.env.protocols.insert("Decode".into(), ProtocolDef {
            name: "Decode".into(),
            generics: vec![],
            methods: vec![ProtocolMethodSig {
                name: "decode".into(),
                params: vec![("v".into(), value_ty.clone())],
                ret: Ty::result(self_ty.clone(), Ty::String),
                is_effect: false,
            }],
        });
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
        let ctx = context.into();
        // Eagerly unify to propagate type info into lambda bodies
        self.unify_infer(&expected, &actual);
        self.constraints.push(Constraint { expected, actual, context: ctx });
    }

    pub fn set_source(&mut self, file: &str, text: &str) { self.source_file = Some(file.into()); self.source_text = Some(text.into()); }

    pub fn register_module(&mut self, name: &str, prog: &ast::Program, _pkg_id: Option<&crate::project::PkgId>, _is_self: bool) {
        self.env.user_modules.insert(name.into());
        self.register_decls(&prog.decls, Some(name));
    }

    pub fn register_alias(&mut self, alias: &str, target: &str) {
        self.env.module_aliases.insert(alias.into(), target.into());
    }

    // ── Main entry point ──

    pub fn check_program(&mut self, program: &mut ast::Program) -> Vec<Diagnostic> {
        // Register explicitly imported stdlib modules (Tier 2)
        for imp in &program.imports {
            if let ast::Decl::Import { path, alias, .. } = imp {
                let name = alias.as_ref().cloned()
                    .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
                if crate::stdlib::is_stdlib_module(&name) {
                    self.env.imported_stdlib.insert(sym(&name));
                }
            }
        }
        self.register_decls(&program.decls, None);
        for decl in program.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        for (id, ity) in &self.infer_types {
            self.expr_types.insert(*id, resolve_ty(ity, &self.uf));
        }
        // Unused import warnings
        for imp in &program.imports {
            let (path, alias, span) = match imp {
                ast::Decl::Import { path, alias, span, .. } => (path, alias, span),
                _ => continue,
            };
            let import_name = alias.as_ref().cloned()
                .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
            if import_name.is_empty()
                || self.env.used_modules.contains(&sym(&import_name))
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

    pub fn check_module_bodies(&mut self, prog: &mut ast::Program) -> HashMap<ExprId, Ty> {
        let saved = (std::mem::take(&mut self.expr_types), std::mem::take(&mut self.infer_types),
            std::mem::take(&mut self.constraints), std::mem::replace(&mut self.uf, UnionFind::new()));
        for decl in prog.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        for (id, ity) in &self.infer_types {
            self.expr_types.insert(*id, resolve_ty(ity, &self.uf));
        }
        let module_types = std::mem::take(&mut self.expr_types);
        self.expr_types = saved.0; self.infer_types = saved.1; self.constraints = saved.2; self.uf = saved.3;
        module_types
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
            self.constrain(ret_ty, body_ity, format!("fn '{}'", name));
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
                self.infer_expr(body);
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
        let resolved = self.env.resolve_named(subject_ty);
        let required: Vec<String> = match &resolved {
            Ty::Variant { cases, .. } => cases.iter().map(|c| c.name.to_string()).collect(),
            Ty::Applied(crate::types::TypeConstructorId::Option, _) => vec!["some".into(), "none".into()],
            Ty::Applied(crate::types::TypeConstructorId::Result, _) => vec!["ok".into(), "err".into()],
            Ty::Bool => vec!["true".into(), "false".into()],
            _ => return,
        };
        let mut covered = std::collections::HashSet::new();
        let mut has_wildcard = false;
        for arm in arms { if arm.guard.is_some() { continue; } self.collect_covered(&arm.pattern, &mut covered, &mut has_wildcard); }
        if has_wildcard { return; }
        let missing: Vec<&String> = required.iter().filter(|c| !covered.contains(*c)).collect();
        if !missing.is_empty() {
            let list = missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            self.emit(Diagnostic::error(format!("non-exhaustive match: missing {}", list), format!("Add arms for {}, or use '_'", list), "match").with_code("E010"));
        }
    }

    fn collect_covered(&self, pat: &ast::Pattern, covered: &mut std::collections::HashSet<String>, wildcard: &mut bool) {
        match pat {
            ast::Pattern::Wildcard | ast::Pattern::Ident { .. } => *wildcard = true,
            ast::Pattern::Constructor { name, .. } | ast::Pattern::RecordPattern { name, .. } => { covered.insert(name.to_string()); }
            ast::Pattern::Some { .. } => { covered.insert("some".into()); }
            ast::Pattern::None => { covered.insert("none".into()); }
            ast::Pattern::Ok { .. } => { covered.insert("ok".into()); }
            ast::Pattern::Err { .. } => { covered.insert("err".into()); }
            ast::Pattern::Literal { value } => { if let ast::Expr::Bool { value: v, .. } = value.as_ref() { covered.insert(if *v { "true" } else { "false" }.into()); } }
            _ => {}
        }
    }

    // ── Type resolution ──

    pub fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
        match te {
            ast::TypeExpr::Simple { name } => match name.as_str() {
                "Int" => Ty::Int, "Float" => Ty::Float, "String" => Ty::String,
                "Bool" => Ty::Bool, "Unit" => Ty::Unit, "Path" => Ty::String,
                other => self.env.types.get(&sym(other)).cloned().unwrap_or(Ty::Named(other.into(), vec![])),
            },
            ast::TypeExpr::Generic { name, args } => {
                let ra: Vec<Ty> = args.iter().map(|a| self.resolve_type_expr(a)).collect();
                match name.as_str() {
                    "List" => Ty::list(ra.first().cloned().unwrap_or(Ty::Unknown)),
                    "Option" => Ty::option(ra.first().cloned().unwrap_or(Ty::Unknown)),
                    "Result" if ra.len() >= 2 => Ty::result(ra[0].clone(), ra[1].clone()),
                    "Map" if ra.len() >= 2 => Ty::map_of(ra[0].clone(), ra[1].clone()),
                    _ => Ty::Named(sym(name), ra),
                }
            },
            ast::TypeExpr::Record { fields } => Ty::Record { fields: fields.iter().map(|f| (sym(&f.name), self.resolve_type_expr(&f.ty))).collect() },
            ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord { fields: fields.iter().map(|f| (sym(&f.name), self.resolve_type_expr(&f.ty))).collect() },
            ast::TypeExpr::Fn { params, ret } => Ty::Fn { params: params.iter().map(|p| self.resolve_type_expr(p)).collect(), ret: Box::new(self.resolve_type_expr(ret)) },
            ast::TypeExpr::Tuple { elements } => Ty::Tuple(elements.iter().map(|e| self.resolve_type_expr(e)).collect()),
            ast::TypeExpr::Union { members } => Ty::union(members.iter().map(|m| self.resolve_type_expr(m)).collect()),
            ast::TypeExpr::Variant { cases } => {
                let cs = cases.iter().map(|c| match c {
                    ast::VariantCase::Unit { name } => VariantCase { name: sym(name), payload: VariantPayload::Unit },
                    ast::VariantCase::Tuple { name, fields } => VariantCase { name: sym(name), payload: VariantPayload::Tuple(fields.iter().map(|f| self.resolve_type_expr(f)).collect()) },
                    ast::VariantCase::Record { name, fields } => VariantCase { name: sym(name), payload: VariantPayload::Record(fields.iter().map(|f| (sym(&f.name), self.resolve_type_expr(&f.ty), f.default.clone())).collect()) },
                }).collect();
                Ty::Variant { name: sym(""), cases: cs }
            },
        }
    }

    pub(crate) fn resolve_field_type(&self, ty: &Ty, field: &str) -> Ty {
        let resolved = self.env.resolve_named(ty);
        match &resolved {
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown),
            Ty::TypeVar(tv) => self.env.structural_bounds.get(tv).map(|b| self.resolve_field_type(b, field)).unwrap_or(Ty::Unknown),
            _ => Ty::Unknown,
        }
    }

    fn infer_literal_type(&self, expr: &ast::Expr) -> Ty {
        match expr {
            ast::Expr::Int { .. } => Ty::Int, ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::String { .. } => Ty::String, ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::Unit { .. } => Ty::Unit, _ => Ty::Unknown,
        }
    }
}
