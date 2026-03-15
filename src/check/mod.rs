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
///   mod.rs    — Checker struct, public API, solving, registration
///   types.rs  — InferTy, TyVarId, Constraint
///   infer.rs  — Expression/statement inference
///   calls.rs  — Function call resolution

mod types;
mod infer;
mod calls;

use std::collections::HashMap;
use crate::ast;
use crate::ast::ExprId;
use crate::diagnostic::Diagnostic;
use crate::types::{Ty, TypeEnv, FnSig, VariantCase, VariantPayload};
use types::{InferTy, TyVarId, Constraint};

pub(crate) fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

pub struct Checker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
    pub source_file: Option<String>,
    pub source_text: Option<String>,
    pub target: Option<String>,
    pub expr_types: HashMap<ExprId, Ty>,
    pub next_expr_id: u32,
    // Inference state
    next_tyvar: u32,
    pub(crate) infer_types: HashMap<ExprId, InferTy>,
    pub(crate) constraints: Vec<Constraint>,
    pub(crate) solutions: HashMap<TyVarId, InferTy>,
}

impl Checker {
    pub fn new() -> Self {
        Checker {
            env: TypeEnv::new(), diagnostics: Vec::new(),
            source_file: None, source_text: None, target: None,
            expr_types: HashMap::new(), next_expr_id: 0,
            next_tyvar: 0, infer_types: HashMap::new(),
            constraints: Vec::new(), solutions: HashMap::new(),
        }
    }

    pub(crate) fn fresh_var(&mut self) -> InferTy {
        let id = TyVarId(self.next_tyvar);
        self.next_tyvar += 1;
        InferTy::Var(id)
    }

    pub(crate) fn constrain(&mut self, expected: InferTy, actual: InferTy, context: impl Into<String>) {
        self.constraints.push(Constraint { expected, actual, context: context.into() });
    }

    pub fn set_source(&mut self, file: &str, text: &str) { self.source_file = Some(file.into()); self.source_text = Some(text.into()); }
    pub fn set_target(&mut self, target: &str) { self.target = Some(target.into()); }

    pub fn register_module(&mut self, name: &str, prog: &ast::Program, _pkg_id: Option<&crate::project::PkgId>, _is_self: bool) {
        self.env.user_modules.insert(name.into());
        self.register_decls(&prog.decls, Some(name));
    }

    pub fn register_alias(&mut self, alias: &str, target: &str) {
        self.env.module_aliases.insert(alias.into(), target.into());
    }

    // ── Main entry point ──

    pub fn check_program(&mut self, program: &mut ast::Program) -> Vec<Diagnostic> {
        self.register_decls(&program.decls, None);
        for decl in program.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        for (id, ity) in &self.infer_types {
            let ty = ity.to_ty(&self.solutions);
            self.expr_types.insert(*id, InferTy::resolve_inference_vars(&ty, &self.solutions));
        }
        std::mem::take(&mut self.diagnostics)
    }

    pub fn check_module_bodies(&mut self, prog: &mut ast::Program) -> HashMap<ExprId, Ty> {
        let saved = (std::mem::take(&mut self.expr_types), std::mem::take(&mut self.infer_types),
            std::mem::take(&mut self.constraints), std::mem::take(&mut self.solutions));
        for decl in prog.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        for (id, ity) in &self.infer_types {
            let ty = ity.to_ty(&self.solutions);
            self.expr_types.insert(*id, InferTy::resolve_inference_vars(&ty, &self.solutions));
        }
        let module_types = std::mem::take(&mut self.expr_types);
        self.expr_types = saved.0; self.infer_types = saved.1; self.constraints = saved.2; self.solutions = saved.3;
        module_types
    }

    // ── Constraint solving ──

    fn solve_constraints(&mut self) {
        for c in std::mem::take(&mut self.constraints) {
            if !self.unify_infer(&c.expected, &c.actual) {
                let exp = c.expected.to_ty(&self.solutions);
                let act = c.actual.to_ty(&self.solutions);
                if exp != Ty::Unknown && act != Ty::Unknown {
                    let hint = Self::hint_with_conversion(
                        "Fix the expression type or change the expected type",
                        &exp, &act,
                    );
                    self.diagnostics.push(err(
                        format!("type mismatch in {}: expected {} but got {}", c.context, exp.display(), act.display()),
                        hint, c.context));
                }
            }
        }
    }

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

    pub(crate) fn hint_with_conversion(base_hint: &str, expected: &Ty, actual: &Ty) -> String {
        if let Some(conv) = Self::suggest_conversion(expected, actual) {
            format!("{}. Or {}", base_hint, conv)
        } else {
            base_hint.to_string()
        }
    }

    fn unify_infer(&mut self, a: &InferTy, b: &InferTy) -> bool {
        match (a, b) {
            (InferTy::Var(id), other) | (other, InferTy::Var(id)) => {
                if let InferTy::Var(oid) = other { if id == oid { return true; } }
                if !self.occurs(*id, other) { self.solutions.insert(*id, other.clone()); }
                true
            }
            (InferTy::Concrete(a), InferTy::Concrete(b)) => {
                if *a == Ty::Unknown || *b == Ty::Unknown { return true; }
                // Record structural unification: match fields by name
                match (a, b) {
                    (Ty::Record { fields: fa }, Ty::Record { fields: fb }) => {
                        fa.len() == fb.len() && fa.iter().all(|(n, t)| fb.iter().any(|(n2, t2)| n == n2 && self.unify_infer(&InferTy::from_ty(t), &InferTy::from_ty(t2))))
                    }
                    (Ty::OpenRecord { fields: req, .. }, Ty::Record { fields: actual })
                    | (Ty::OpenRecord { fields: req, .. }, Ty::OpenRecord { fields: actual, .. }) => {
                        req.iter().all(|(n, t)| actual.iter().any(|(n2, t2)| n == n2 && self.unify_infer(&InferTy::from_ty(t), &InferTy::from_ty(t2))))
                    }
                    (Ty::Named(na, _), Ty::Named(nb, _)) if na == nb => true,
                    _ => a.compatible(b),
                }
            }
            (InferTy::List(a), InferTy::List(b)) => self.unify_infer(a, b),
            (InferTy::Option(a), InferTy::Option(b)) => self.unify_infer(a, b),
            (InferTy::Result(ao, ae), InferTy::Result(bo, be)) => self.unify_infer(ao, bo) && self.unify_infer(ae, be),
            (InferTy::Map(ak, av), InferTy::Map(bk, bv)) => self.unify_infer(ak, bk) && self.unify_infer(av, bv),
            (InferTy::Tuple(a), InferTy::Tuple(b)) if a.len() == b.len() => a.iter().zip(b.iter()).all(|(x, y)| self.unify_infer(x, y)),
            (InferTy::Fn { params: ap, ret: ar }, InferTy::Fn { params: bp, ret: br }) if ap.len() == bp.len() =>
                ap.iter().zip(bp.iter()).all(|(x, y)| self.unify_infer(x, y)) && self.unify_infer(ar, br),
            // Concrete ↔ structured
            (InferTy::Concrete(Ty::List(inner)), InferTy::List(b)) | (InferTy::List(b), InferTy::Concrete(Ty::List(inner))) =>
                self.unify_infer(&InferTy::from_ty(inner), b),
            (InferTy::Concrete(Ty::Option(inner)), InferTy::Option(b)) | (InferTy::Option(b), InferTy::Concrete(Ty::Option(inner))) =>
                self.unify_infer(&InferTy::from_ty(inner), b),
            (InferTy::Concrete(Ty::Result(ok, err)), InferTy::Result(bo, be)) | (InferTy::Result(bo, be), InferTy::Concrete(Ty::Result(ok, err))) =>
                self.unify_infer(&InferTy::from_ty(ok), bo) && self.unify_infer(&InferTy::from_ty(err), be),
            _ => false,
        }
    }

    fn occurs(&self, var: TyVarId, ty: &InferTy) -> bool {
        match ty {
            InferTy::Var(id) => *id == var || self.solutions.get(id).map_or(false, |s| self.occurs(var, s)),
            InferTy::List(inner) | InferTy::Option(inner) => self.occurs(var, inner),
            InferTy::Result(a, b) | InferTy::Map(a, b) => self.occurs(var, a) || self.occurs(var, b),
            InferTy::Tuple(elems) => elems.iter().any(|e| self.occurs(var, e)),
            InferTy::Fn { params, ret } => params.iter().any(|p| self.occurs(var, p)) || self.occurs(var, ret),
            InferTy::Concrete(_) => false,
        }
    }

    // ── Registration ──

    fn register_decls(&mut self, decls: &[ast::Decl], prefix: Option<&str>) {
        for decl in decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, generics, .. } => {
                    let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
                    let mut sb = HashMap::new();
                    if let Some(gs) = generics {
                        for g in gs {
                            if let Some(ref bte) = g.structural_bound {
                                let bt = self.resolve_type_expr(bte);
                                sb.insert(g.name.clone(), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
                            }
                        }
                    }
                    for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
                    let ptys: Vec<(String, Ty)> = params.iter().map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty))).collect();
                    let ret = self.resolve_type_expr(return_type);
                    for gn in &gnames { self.env.types.remove(gn); }
                    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
                    let key = prefix.map(|p| format!("{}.{}", p, name)).unwrap_or(name.clone());
                    if prefix.is_none() && is_effect { self.env.effect_fns.insert(name.clone()); }
                    self.env.functions.insert(key, FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb });
                }
                ast::Decl::Type { name, ty, deriving, generics, .. } => {
                    // Validate derive convention names
                    if let Some(derives) = deriving {
                        let valid = ["Eq", "Show", "Compare", "Hash", "Encode", "Decode"];
                        for d in derives {
                            if !valid.contains(&d.as_str()) {
                                self.diagnostics.push(err(
                                    format!("unknown derive convention '{}' on type '{}'", d, name),
                                    format!("Valid conventions: {}", valid.join(", ")),
                                    format!("type {}", name),
                                ));
                            }
                        }
                    }
                    let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
                    for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
                    let mut resolved = self.resolve_type_expr(ty);
                    for gn in &gnames { self.env.types.remove(gn); }
                    if prefix.is_none() {
                        if let Ty::Variant { name: ref mut vn, ref cases } = resolved {
                            *vn = name.clone();
                            for case in cases { self.env.constructors.insert(case.name.clone(), (name.clone(), case.clone())); }
                        }
                    }
                    let key = prefix.map(|p| format!("{}.{}", p, name)).unwrap_or(name.clone());
                    self.env.types.insert(key, resolved);
                }
                ast::Decl::TopLet { name, ty, value, .. } => {
                    let rt = ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.infer_literal_type(value));
                    let key = prefix.map(|p| format!("{}.{}", p, name)).unwrap_or(name.clone());
                    self.env.top_lets.insert(key, rt);
                }
                _ => {}
            }
        }
    }

    // ── Declaration checking ──

    fn check_decl(&mut self, decl: &mut ast::Decl) {
        match decl {
            ast::Decl::Fn { name, params, return_type, body: Some(body), effect, generics, .. } => {
                self.env.push_scope();
                if let Some(gs) = generics {
                    for g in gs {
                        self.env.types.insert(g.name.clone(), Ty::TypeVar(g.name.clone()));
                        if let Some(ref bte) = g.structural_bound {
                            let bt = self.resolve_type_expr(bte);
                            self.env.structural_bounds.insert(g.name.clone(), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
                        }
                    }
                }
                for p in params {
                    let ty = self.resolve_type_expr(&p.ty);
                    self.env.define_var(&p.name, ty);
                    self.env.param_vars.insert(p.name.clone());
                }
                let ret_ty = self.resolve_type_expr(return_type);
                let prev = (self.env.current_ret.take(), self.env.in_effect);
                self.env.current_ret = Some(ret_ty.clone());
                self.env.in_effect = effect.unwrap_or(false);
                let body_ity = self.infer_expr(body);
                // Signature-driven constraint:
                // - effect fn with Result[T, E] sig: accept body returning T (auto-wrapped) or Result[T, E] (explicit)
                // - non-effect: body must match signature exactly
                if effect.unwrap_or(false) {
                    if let Ty::Result(ok, _) = &ret_ty {
                        // Try unwrapped first (body returns T), fall back to full Result match
                        let unwrapped = InferTy::from_ty(ok);
                        let full = InferTy::from_ty(&ret_ty);
                        let body_ty = body_ity.to_ty(&self.solutions);
                        if matches!(&body_ty, Ty::Result(_, _)) {
                            self.constrain(full, body_ity, format!("fn '{}'", name));
                        } else {
                            self.constrain(unwrapped, body_ity, format!("fn '{}'", name));
                        }
                    } else {
                        self.constrain(InferTy::from_ty(&ret_ty), body_ity, format!("fn '{}'", name));
                    }
                } else {
                    self.constrain(InferTy::from_ty(&ret_ty), body_ity, format!("fn '{}'", name));
                }
                self.env.current_ret = prev.0; self.env.in_effect = prev.1;
                if let Some(gs) = generics { for g in gs { self.env.types.remove(&g.name); self.env.structural_bounds.remove(&g.name); } }
                self.env.pop_scope();
            }
            ast::Decl::Test { body, .. } => {
                self.env.push_scope();
                let prev = self.env.in_effect; self.env.in_effect = true;
                self.infer_expr(body);
                self.env.in_effect = prev;
                self.env.pop_scope();
            }
            _ => {}
        }
    }

    // ── Exhaustiveness ──

    pub(crate) fn check_match_exhaustiveness(&mut self, subject_ty: &Ty, arms: &[ast::MatchArm]) {
        let resolved = self.env.resolve_named(subject_ty);
        let required: Vec<String> = match &resolved {
            Ty::Variant { cases, .. } => cases.iter().map(|c| c.name.clone()).collect(),
            Ty::Option(_) => vec!["some".into(), "none".into()],
            Ty::Result(_, _) => vec!["ok".into(), "err".into()],
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
            self.diagnostics.push(Diagnostic::error(format!("non-exhaustive match: missing {}", list), format!("Add arms for {}, or use '_'", list), "match"));
        }
    }

    fn collect_covered(&self, pat: &ast::Pattern, covered: &mut std::collections::HashSet<String>, wildcard: &mut bool) {
        match pat {
            ast::Pattern::Wildcard | ast::Pattern::Ident { .. } => *wildcard = true,
            ast::Pattern::Constructor { name, .. } | ast::Pattern::RecordPattern { name, .. } => { covered.insert(name.clone()); }
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
                other => self.env.types.get(other).cloned().unwrap_or(Ty::Named(other.into(), vec![])),
            },
            ast::TypeExpr::Generic { name, args } => {
                let ra: Vec<Ty> = args.iter().map(|a| self.resolve_type_expr(a)).collect();
                match name.as_str() {
                    "List" => Ty::List(Box::new(ra.first().cloned().unwrap_or(Ty::Unknown))),
                    "Option" => Ty::Option(Box::new(ra.first().cloned().unwrap_or(Ty::Unknown))),
                    "Result" if ra.len() >= 2 => Ty::Result(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Map" if ra.len() >= 2 => Ty::Map(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    _ => Ty::Named(name.clone(), ra),
                }
            },
            ast::TypeExpr::Record { fields } => Ty::Record { fields: fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect() },
            ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord { fields: fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect() },
            ast::TypeExpr::Fn { params, ret } => Ty::Fn { params: params.iter().map(|p| self.resolve_type_expr(p)).collect(), ret: Box::new(self.resolve_type_expr(ret)) },
            ast::TypeExpr::Tuple { elements } => Ty::Tuple(elements.iter().map(|e| self.resolve_type_expr(e)).collect()),
            ast::TypeExpr::Newtype { inner } => self.resolve_type_expr(inner),
            ast::TypeExpr::Union { members } => Ty::union(members.iter().map(|m| self.resolve_type_expr(m)).collect()),
            ast::TypeExpr::Variant { cases } => {
                let cs = cases.iter().map(|c| match c {
                    ast::VariantCase::Unit { name } => VariantCase { name: name.clone(), payload: VariantPayload::Unit },
                    ast::VariantCase::Tuple { name, fields } => VariantCase { name: name.clone(), payload: VariantPayload::Tuple(fields.iter().map(|f| self.resolve_type_expr(f)).collect()) },
                    ast::VariantCase::Record { name, fields } => VariantCase { name: name.clone(), payload: VariantPayload::Record(fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty), f.default.clone())).collect()) },
                }).collect();
                Ty::Variant { name: String::new(), cases: cs }
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
