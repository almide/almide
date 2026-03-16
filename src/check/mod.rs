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

    /// Let-polymorphism: instantiate で TypeVar("?N") を fresh var に置換
    /// 同じ let binding を2回参照する時、各参照で独立した型変数を使う
    pub(crate) fn instantiate_ty(&mut self, ty: &Ty) -> InferTy {
        let mut mapping: std::collections::HashMap<u32, TyVarId> = std::collections::HashMap::new();
        self.instantiate_inner(ty, &mut mapping)
    }

    fn instantiate_inner(&mut self, ty: &Ty, mapping: &mut std::collections::HashMap<u32, TyVarId>) -> InferTy {
        match ty {
            Ty::TypeVar(name) if name.starts_with('?') => {
                if let Ok(id) = name[1..].parse::<u32>() {
                    let fresh_id = mapping.entry(id).or_insert_with(|| {
                        let fv = TyVarId(self.next_tyvar);
                        self.next_tyvar += 1;
                        fv
                    });
                    InferTy::Var(*fresh_id)
                } else {
                    InferTy::from_ty(ty)
                }
            }
            Ty::List(inner) => InferTy::List(Box::new(self.instantiate_inner(inner, mapping))),
            Ty::Option(inner) => InferTy::Option(Box::new(self.instantiate_inner(inner, mapping))),
            Ty::Result(ok, err) => InferTy::Result(
                Box::new(self.instantiate_inner(ok, mapping)),
                Box::new(self.instantiate_inner(err, mapping)),
            ),
            Ty::Map(k, v) => InferTy::Map(
                Box::new(self.instantiate_inner(k, mapping)),
                Box::new(self.instantiate_inner(v, mapping)),
            ),
            Ty::Tuple(elems) => InferTy::Tuple(elems.iter().map(|e| self.instantiate_inner(e, mapping)).collect()),
            Ty::Fn { params, ret } => InferTy::Fn {
                params: params.iter().map(|p| self.instantiate_inner(p, mapping)).collect(),
                ret: Box::new(self.instantiate_inner(ret, mapping)),
            },
            other => InferTy::from_ty(other),
        }
    }

    pub(crate) fn constrain(&mut self, expected: InferTy, actual: InferTy, context: impl Into<String>) {
        let ctx = context.into();
        // Eagerly unify to propagate type info into lambda bodies
        self.unify_infer(&expected, &actual);
        self.constraints.push(Constraint { expected, actual, context: ctx });
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
        // Unused import warnings
        for imp in &program.imports {
            let (path, alias, span) = match imp {
                ast::Decl::Import { path, alias, span, .. } => (path, alias, span),
                _ => continue,
            };
            let import_name = alias.as_ref().cloned()
                .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
            if import_name.is_empty()
                || self.env.used_modules.contains(&import_name)
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
                    (Ty::Named(na, args_a), Ty::Named(nb, args_b)) if na == nb => {
                        // HM: structurally unify type constructor arguments
                        args_a.len() == args_b.len()
                            && args_a.iter().zip(args_b.iter()).all(|(ta, tb)|
                                self.unify_infer(&InferTy::from_ty(ta), &InferTy::from_ty(tb)))
                            || (args_a.is_empty() || args_b.is_empty()) // backward compat: empty args = no constraint
                    }
                    // Resolve Named types for structural comparison
                    (Ty::Named(_, _), _) => {
                        let resolved = self.env.resolve_named(a);
                        if resolved != *a { self.unify_infer(&InferTy::from_ty(&resolved), &InferTy::from_ty(b)) }
                        else { a.compatible(b) }
                    }
                    (_, Ty::Named(_, _)) => {
                        let resolved = self.env.resolve_named(b);
                        if resolved != *b { self.unify_infer(&InferTy::from_ty(a), &InferTy::from_ty(&resolved)) }
                        else { a.compatible(b) }
                    }
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

    /// Collect structural bounds from generic params: Record → OpenRecord conversion.
    fn collect_structural_bounds(&self, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<String, Ty> {
        let mut sb = HashMap::new();
        let gs = match generics { Some(gs) => gs, None => return sb };
        for g in gs {
            let bte = match &g.structural_bound { Some(bte) => bte, None => continue };
            let bt = self.resolve_type_expr(bte);
            sb.insert(g.name.clone(), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
        }
        sb
    }

    /// Build a prefixed key: "module.name" or just "name".
    fn prefixed_key(prefix: Option<&str>, name: &str) -> String {
        prefix.map(|p| format!("{}.{}", p, name)).unwrap_or_else(|| name.to_string())
    }

    fn register_fn_sig(&mut self, name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
                        effect: &Option<bool>, r#async: &Option<bool>, generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        let sb = self.collect_structural_bounds(generics);
        for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
        let ptys: Vec<(String, Ty)> = params.iter().map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty))).collect();
        let ret = self.resolve_type_expr(return_type);
        for gn in &gnames { self.env.types.remove(gn); }
        let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
        let key = Self::prefixed_key(prefix, name);
        if prefix.is_none() && is_effect { self.env.effect_fns.insert(name.to_string()); }
        let min_p = params.iter().take_while(|p| p.default.is_none()).count();
        self.env.functions.insert(key.clone(), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb });
        if min_p < params.len() {
            self.env.fn_min_params.insert(key, min_p);
        }
    }

    fn validate_derives(&mut self, derives: &[String], type_name: &str) {
        let valid = ["Eq", "Repr", "Ord", "Hash", "Codec", "Encode", "Decode"];
        for d in derives {
            if !valid.contains(&d.as_str()) {
                self.diagnostics.push(err(
                    format!("unknown derive convention '{}' on type '{}'", d, type_name),
                    format!("Valid conventions: {}", valid.join(", ")),
                    format!("type {}", type_name),
                ));
            }
        }
    }

    fn register_derive_sigs(&mut self, derives: &[String], type_name: &str) {
        let type_ty = Ty::Named(type_name.to_string(), vec![]);
        let value_ty = Ty::Named("Value".to_string(), vec![]);
        let empty_sb = std::collections::HashMap::new();
        for d in derives {
            match d.as_str() {
                "Eq" => {
                    let fn_key = format!("{}.eq", type_name);
                    if !self.env.functions.contains_key(&fn_key) {
                        self.env.functions.insert(fn_key, FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone() });
                    }
                }
                "Repr" => {
                    let fn_key = format!("{}.repr", type_name);
                    if !self.env.functions.contains_key(&fn_key) {
                        self.env.functions.insert(fn_key, FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone() });
                    }
                }
                "Codec" => {
                    let encode_key = format!("{}.encode", type_name);
                    if !self.env.functions.contains_key(&encode_key) {
                        self.env.functions.insert(encode_key, FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone() });
                    }
                    let decode_key = format!("{}.decode", type_name);
                    if !self.env.functions.contains_key(&decode_key) {
                        self.env.functions.insert(decode_key, FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::Result(Box::new(type_ty.clone()), Box::new(Ty::String)), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone() });
                    }
                }
                _ => {}
            }
        }
    }

    fn register_type_decl(&mut self, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<String>>,
                           generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        if let Some(derives) = deriving {
            self.validate_derives(derives, name);
        }
        let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
        let mut resolved = self.resolve_type_expr(ty);
        for gn in &gnames { self.env.types.remove(gn); }
        if prefix.is_none() {
            if let Ty::Variant { name: ref mut vn, ref cases } = resolved {
                *vn = name.to_string();
                for case in cases { self.env.constructors.insert(case.name.clone(), (name.to_string(), case.clone())); }
            }
        }
        let key = Self::prefixed_key(prefix, name);
        self.env.types.insert(key.clone(), resolved);
        if let Some(derives) = deriving {
            self.register_derive_sigs(derives, name);
        }
    }

    fn register_decls(&mut self, decls: &[ast::Decl], prefix: Option<&str>) {
        for decl in decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, generics, .. } => {
                    self.register_fn_sig(name, params, return_type, effect, r#async, generics, prefix);
                }
                ast::Decl::Type { name, ty, deriving, generics, .. } => {
                    self.register_type_decl(name, ty, deriving, generics, prefix);
                }
                ast::Decl::TopLet { name, ty, value, .. } => {
                    let rt = ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.infer_literal_type(value));
                    let key = Self::prefixed_key(prefix, name);
                    self.env.top_lets.insert(key, rt);
                }
                _ => {}
            }
        }
    }

    // ── Declaration checking ──

    /// Push generic type vars and structural bounds into the environment.
    fn enter_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.insert(g.name.clone(), Ty::TypeVar(g.name.clone()));
            let bte = match &g.structural_bound { Some(bte) => bte, None => continue };
            let bt = self.resolve_type_expr(bte);
            self.env.structural_bounds.insert(g.name.clone(), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
        }
    }

    /// Remove generic type vars and structural bounds from the environment.
    fn exit_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() { self.env.types.remove(&g.name); self.env.structural_bounds.remove(&g.name); }
    }

    /// Constrain an effect fn body against its return type signature.
    /// Effect fns accept: Unit body (control-flow returns), unwrapped T, or full Result[T, E].
    fn constrain_effect_body(&mut self, name: &str, ret_ty: &Ty, body_ity: InferTy) {
        let body_ty = body_ity.to_ty(&self.solutions);
        if body_ty == Ty::Unit { return; } // do blocks, while loops, guard patterns return via control flow
        if let Ty::Result(ok, _) = ret_ty {
            if matches!(&body_ty, Ty::Result(_, _)) {
                self.constrain(InferTy::from_ty(ret_ty), body_ity, format!("fn '{}'", name));
            } else {
                self.constrain(InferTy::from_ty(ok), body_ity, format!("fn '{}'", name));
            }
            return;
        }
        self.constrain(InferTy::from_ty(ret_ty), body_ity, format!("fn '{}'", name));
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
            self.env.param_vars.insert(p.name.clone());
            if let Some(ref mut default_expr) = p.default {
                let dty = self.infer_expr(default_expr);
                self.constrain(InferTy::from_ty(&ty), dty, format!("default arg '{}'", p.name));
            }
        }
        let ret_ty = self.resolve_type_expr(return_type);
        let prev = (self.env.current_ret.take(), self.env.in_effect);
        self.env.current_ret = Some(ret_ty.clone());
        self.env.in_effect = effect.unwrap_or(false);
        let body_ity = self.infer_expr(body);
        if effect.unwrap_or(false) {
            self.constrain_effect_body(name, &ret_ty, body_ity);
        } else {
            self.constrain(InferTy::from_ty(&ret_ty), body_ity, format!("fn '{}'", name));
        }
        self.env.current_ret = prev.0; self.env.in_effect = prev.1;
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
                let prev = self.env.in_effect; self.env.in_effect = true;
                self.infer_expr(body);
                self.env.in_effect = prev;
                self.env.pop_scope();
            }
            ast::Decl::TopLet { name, value, .. } => {
                let ity = self.infer_expr(value);
                let resolved = ity.to_ty(&self.solutions);
                // Update env.top_lets with the fully inferred type
                if matches!(self.env.top_lets.get(name.as_str()), Some(Ty::Unknown) | None) {
                    self.env.top_lets.insert(name.clone(), resolved);
                }
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
