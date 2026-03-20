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
///   types.rs  — TyVarId, Constraint, resolve_vars
///   infer.rs  — Expression/statement inference
///   calls.rs  — Function call resolution

mod types;
mod infer;
pub(crate) mod calls;

use std::collections::HashMap;
use crate::ast;
use crate::ast::ExprId;
use crate::diagnostic::Diagnostic;
use crate::types::{Ty, TypeEnv, FnSig, VariantCase, VariantPayload, ProtocolDef, ProtocolMethodSig};
use types::{TyVarId, Constraint, is_inference_var, resolve_vars};

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
    next_tyvar: u32,
    pub(crate) infer_types: HashMap<ExprId, Ty>,
    pub(crate) constraints: Vec<Constraint>,
    pub(crate) solutions: HashMap<TyVarId, Ty>,
}

impl Checker {
    pub fn new() -> Self {
        let mut checker = Checker {
            env: TypeEnv::new(), diagnostics: Vec::new(),
            source_file: None, source_text: None,
            expr_types: HashMap::new(), current_span: None,
            next_tyvar: 0, infer_types: HashMap::new(),
            constraints: Vec::new(), solutions: HashMap::new(),
        };
        checker.register_builtin_protocols();
        checker
    }

    /// Register built-in conventions (Eq, Repr, Ord, Hash, Codec, Encode, Decode) as protocols.
    fn register_builtin_protocols(&mut self) {
        let self_ty = Ty::TypeVar("Self".to_string());
        let value_ty = Ty::Named("Value".to_string(), vec![]);

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
        let id = self.next_tyvar;
        self.next_tyvar += 1;
        Ty::TypeVar(format!("?{}", id))
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
        self.register_decls(&program.decls, None);
        for decl in program.decls.iter_mut() { self.check_decl(decl); }
        self.solve_constraints();
        for (id, ity) in &self.infer_types {
            let ty = resolve_vars(ity, &self.solutions);
            self.expr_types.insert(*id, resolve_vars(&ty, &self.solutions));
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
            let ty = resolve_vars(ity, &self.solutions);
            self.expr_types.insert(*id, resolve_vars(&ty, &self.solutions));
        }
        let module_types = std::mem::take(&mut self.expr_types);
        self.expr_types = saved.0; self.infer_types = saved.1; self.constraints = saved.2; self.solutions = saved.3;
        module_types
    }

    // ── Constraint solving ──

    fn solve_constraints(&mut self) {
        for c in std::mem::take(&mut self.constraints) {
            if !self.unify_infer(&c.expected, &c.actual) {
                let exp = resolve_vars(&c.expected, &self.solutions);
                let act = resolve_vars(&c.actual, &self.solutions);
                if exp != Ty::Unknown && act != Ty::Unknown {
                    let hint = Self::hint_with_conversion(
                        "Fix the expression type or change the expected type",
                        &exp, &act,
                    );
                    self.emit(err(
                        format!("type mismatch in {}: expected {} but got {}", c.context, exp.display(), act.display()),
                        hint, c.context).with_code("E001"));
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

    fn unify_infer(&mut self, a: &Ty, b: &Ty) -> bool {
        // Handle inference variables
        if let Some(id_a) = is_inference_var(a) {
            if let Some(id_b) = is_inference_var(b) {
                if id_a == id_b { return true; }
            }
            if !self.occurs(id_a, b) { self.solutions.insert(id_a, b.clone()); }
            return true;
        }
        if let Some(id_b) = is_inference_var(b) {
            if !self.occurs(id_b, a) { self.solutions.insert(id_b, a.clone()); }
            return true;
        }
        match (a, b) {
            (Ty::Applied(id1, args1), Ty::Applied(id2, args2)) if id1 == id2 && args1.len() == args2.len() => {
                args1.iter().zip(args2.iter()).all(|(x, y)| self.unify_infer(x, y))
            }
            (Ty::Tuple(a), Ty::Tuple(b)) if a.len() == b.len() => a.iter().zip(b.iter()).all(|(x, y)| self.unify_infer(x, y)),
            (Ty::Fn { params: ap, ret: ar }, Ty::Fn { params: bp, ret: br }) if ap.len() == bp.len() =>
                ap.iter().zip(bp.iter()).all(|(x, y)| self.unify_infer(x, y)) && self.unify_infer(ar, br),
            _ => {
                if *a == Ty::Unknown || *b == Ty::Unknown { return true; }
                // Record structural unification: match fields by name
                match (a, b) {
                    (Ty::Record { fields: fa }, Ty::Record { fields: fb }) => {
                        fa.len() == fb.len() && fa.iter().all(|(n, t)| fb.iter().any(|(n2, t2)| n == n2 && self.unify_infer(t, t2)))
                    }
                    (Ty::OpenRecord { fields: req, .. }, Ty::Record { fields: actual })
                    | (Ty::OpenRecord { fields: req, .. }, Ty::OpenRecord { fields: actual, .. }) => {
                        req.iter().all(|(n, t)| actual.iter().any(|(n2, t2)| n == n2 && self.unify_infer(t, t2)))
                    }
                    (Ty::Named(na, args_a), Ty::Named(nb, args_b)) if na == nb => {
                        // HM: structurally unify type constructor arguments
                        args_a.len() == args_b.len()
                            && args_a.iter().zip(args_b.iter()).all(|(ta, tb)|
                                self.unify_infer(ta, tb))
                            || (args_a.is_empty() || args_b.is_empty()) // backward compat: empty args = no constraint
                    }
                    // Resolve Named types for structural comparison
                    (Ty::Named(_, _), _) => {
                        let resolved = self.env.resolve_named(a);
                        if resolved != *a { self.unify_infer(&resolved, b) }
                        else { a.compatible(b) }
                    }
                    (_, Ty::Named(_, _)) => {
                        let resolved = self.env.resolve_named(b);
                        if resolved != *b { self.unify_infer(a, &resolved) }
                        else { a.compatible(b) }
                    }
                    _ => a.compatible(b),
                }
            }
        }
    }

    fn occurs(&self, var: TyVarId, ty: &Ty) -> bool {
        match ty {
            Ty::TypeVar(name) if name.starts_with('?') => {
                if let Ok(id) = name[1..].parse::<u32>() {
                    id == var.0 || self.solutions.get(&TyVarId(id)).map_or(false, |s| self.occurs(var, s))
                } else { false }
            }
            Ty::Applied(_, args) => args.iter().any(|a| self.occurs(var, a)),
            Ty::Tuple(elems) => elems.iter().any(|e| self.occurs(var, e)),
            Ty::Fn { params, ret } => params.iter().any(|p| self.occurs(var, p)) || self.occurs(var, ret),
            _ => false,
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

    /// Collect protocol bounds from generic params: TypeVar name → list of protocol names.
    fn collect_protocol_bounds(&self, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<String, Vec<String>> {
        let mut pb = HashMap::new();
        let gs = match generics { Some(gs) => gs, None => return pb };
        for g in gs {
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() {
                    pb.insert(g.name.clone(), bounds.clone());
                }
            }
        }
        pb
    }

    fn register_fn_sig(&mut self, name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
                        effect: &Option<bool>, r#async: &Option<bool>, generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        let sb = self.collect_structural_bounds(generics);
        let pb = self.collect_protocol_bounds(generics);
        for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
        let ptys: Vec<(String, Ty)> = params.iter().map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty))).collect();
        let ret = self.resolve_type_expr(return_type);
        for gn in &gnames { self.env.types.remove(gn); }
        let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
        let key = Self::prefixed_key(prefix, name);
        if prefix.is_none() && is_effect { self.env.effect_fns.insert(name.to_string()); }
        let min_p = params.iter().take_while(|p| p.default.is_none()).count();
        self.env.functions.insert(key.clone(), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb });
        if min_p < params.len() {
            self.env.fn_min_params.insert(key, min_p);
        }
    }

    fn validate_protocols(&mut self, derives: &[String], type_name: &str) {
        for d in derives {
            if !self.env.protocols.contains_key(d.as_str()) {
                let valid: Vec<&str> = self.env.protocols.keys().map(|s| s.as_str()).collect();
                self.emit(err(
                    format!("unknown protocol '{}' on type '{}'", d, type_name),
                    format!("Known protocols: {}", {
                        let mut sorted = valid; sorted.sort(); sorted.join(", ")
                    }),
                    format!("type {}", type_name),
                ));
            }
        }
    }

    fn register_derive_sigs(&mut self, derives: &[String], type_name: &str) {
        let type_ty = Ty::Named(type_name.to_string(), vec![]);
        let value_ty = Ty::Named("Value".to_string(), vec![]);
        let empty_sb = std::collections::HashMap::new();
        let empty_pb = std::collections::HashMap::new();
        for d in derives {
            match d.as_str() {
                "Eq" => {
                    let fn_key = format!("{}.eq", type_name);
                    if !self.env.functions.contains_key(&fn_key) {
                        self.env.functions.insert(fn_key, FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                "Repr" => {
                    let fn_key = format!("{}.repr", type_name);
                    if !self.env.functions.contains_key(&fn_key) {
                        self.env.functions.insert(fn_key, FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                "Codec" => {
                    let encode_key = format!("{}.encode", type_name);
                    if !self.env.functions.contains_key(&encode_key) {
                        self.env.functions.insert(encode_key, FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                    let decode_key = format!("{}.decode", type_name);
                    if !self.env.functions.contains_key(&decode_key) {
                        self.env.functions.insert(decode_key, FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                _ => {}
            }
        }
    }

    /// Register a user-defined protocol declaration into env.protocols.
    fn register_protocol_decl(&mut self, name: &str, generics: &Option<Vec<ast::GenericParam>>, methods: &[ast::ProtocolMethod]) {
        let gnames: Vec<String> = generics.as_ref()
            .map(|gs| gs.iter().map(|g| g.name.clone()).collect())
            .unwrap_or_default();

        // Temporarily register `Self` as a TypeVar so resolve_type_expr handles it
        self.env.types.insert("Self".to_string(), Ty::TypeVar("Self".to_string()));
        for gn in &gnames {
            self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone()));
        }

        let method_sigs: Vec<ProtocolMethodSig> = methods.iter().map(|m| {
            let params: Vec<(String, Ty)> = m.params.iter()
                .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                .collect();
            let ret = self.resolve_type_expr(&m.return_type);
            ProtocolMethodSig {
                name: m.name.clone(),
                params,
                ret,
                is_effect: m.effect,
            }
        }).collect();

        self.env.types.remove("Self");
        for gn in &gnames {
            self.env.types.remove(gn);
        }

        self.env.protocols.insert(name.to_string(), ProtocolDef {
            name: name.to_string(),
            generics: gnames,
            methods: method_sigs,
        });
    }

    /// Validate that types declaring `: ProtocolName` have all required convention methods.
    /// Called after all declarations are registered so all `Type.method` functions are available.
    fn validate_protocol_impls(&mut self) {
        // Snapshot type_protocols to avoid borrow conflict
        let type_protocols: Vec<(String, Vec<String>)> = self.env.type_protocols.iter()
            .map(|(ty, protos)| (ty.clone(), protos.iter().cloned().collect()))
            .collect();

        for (type_name, protocol_names) in &type_protocols {
            for proto_name in protocol_names {
                let proto_def = match self.env.protocols.get(proto_name) {
                    Some(p) => p.clone(),
                    None => continue, // Unknown protocol — already reported by validate_protocols
                };

                for method_sig in &proto_def.methods {
                    let fn_key = format!("{}.{}", type_name, method_sig.name);
                    if !self.env.functions.contains_key(&fn_key) {
                        // Only emit error for user-defined protocols (not built-in ones
                        // which may be auto-derived)
                        let is_builtin = matches!(proto_name.as_str(),
                            "Eq" | "Repr" | "Ord" | "Hash" | "Codec" | "Encode" | "Decode");
                        if !is_builtin {
                            self.emit(err(
                                format!("type '{}' declares protocol '{}' but missing method '{}'",
                                    type_name, proto_name, method_sig.name),
                                format!("Add: fn {}.{}({}) -> {}",
                                    type_name, method_sig.name,
                                    method_sig.params.iter()
                                        .map(|(n, t)| {
                                            let display_ty = if *t == Ty::TypeVar("Self".to_string()) {
                                                type_name.clone()
                                            } else {
                                                t.display()
                                            };
                                            format!("{}: {}", n, display_ty)
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    {
                                        let ret = &method_sig.ret;
                                        if *ret == Ty::TypeVar("Self".to_string()) {
                                            type_name.clone()
                                        } else {
                                            ret.display()
                                        }
                                    }),
                                format!("type {} : {}", type_name, proto_name),
                            ));
                        }
                    }
                }
            }
        }
    }

    fn register_type_decl(&mut self, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<String>>,
                           generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        if let Some(derives) = deriving {
            self.validate_protocols(derives, name);
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
        self.env.types.insert(key.clone(), resolved.clone());
        // Also register with unprefixed name so field resolution works
        // (e.g., g.words where g: KeywordGroup from a module)
        if prefix.is_some() {
            self.env.types.insert(name.to_string(), resolved);
        }
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
                    // Track protocol conformances declared via `: ProtocolName`
                    if let Some(derives) = deriving {
                        for d in derives {
                            self.env.type_protocols
                                .entry(name.clone())
                                .or_insert_with(std::collections::HashSet::new)
                                .insert(d.clone());
                        }
                    }
                }
                ast::Decl::Protocol { name, generics, methods, .. } => {
                    self.register_protocol_decl(name, generics, methods);
                }
                ast::Decl::TopLet { name, ty, value, .. } => {
                    let rt = ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.infer_literal_type(value));
                    let key = Self::prefixed_key(prefix, name);
                    self.env.top_lets.insert(key, rt);
                }
                _ => {}
            }
        }
        // After all decls are registered, validate protocol implementations
        self.validate_protocol_impls();
    }

    // ── Declaration checking ──

    /// Push generic type vars, structural bounds, and protocol bounds into the environment.
    fn enter_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.insert(g.name.clone(), Ty::TypeVar(g.name.clone()));
            if let Some(bte) = &g.structural_bound {
                let bt = self.resolve_type_expr(bte);
                self.env.structural_bounds.insert(g.name.clone(), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
            }
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() {
                    self.env.generic_protocol_bounds.insert(g.name.clone(), bounds.clone());
                }
            }
        }
    }

    /// Remove generic type vars, structural bounds, and protocol bounds from the environment.
    fn exit_generics(&mut self, generics: &Option<Vec<ast::GenericParam>>) {
        let gs = match generics { Some(gs) => gs, None => return };
        for g in gs.iter() {
            self.env.types.remove(&g.name);
            self.env.structural_bounds.remove(&g.name);
            self.env.generic_protocol_bounds.remove(&g.name);
        }
    }

    /// Constrain an effect fn body against its return type signature.
    /// Effect fns accept: Unit body (control-flow returns), unwrapped T, or full Result[T, E].
    fn constrain_effect_body(&mut self, name: &str, ret_ty: &Ty, body_ty: Ty) {
        let body_resolved = resolve_vars(&body_ty, &self.solutions);
        if body_resolved == Ty::Unit { return; } // do blocks, while loops, guard patterns return via control flow
        if let Ty::Applied(crate::types::TypeConstructorId::Result, args) = ret_ty {
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
            self.env.param_vars.insert(p.name.clone());
            if let Some(ref mut default_expr) = p.default {
                let dty = self.infer_expr(default_expr);
                self.constrain(ty, dty, format!("default arg '{}'", p.name));
            }
        }
        let ret_ty = self.resolve_type_expr(return_type);
        let prev = (self.env.current_ret.take(), self.env.can_call_effect, self.env.auto_unwrap);
        let is_effect = effect.unwrap_or(false);
        self.env.current_ret = Some(ret_ty.clone());
        self.env.can_call_effect = is_effect;
        self.env.auto_unwrap = is_effect;
        let body_ity = self.infer_expr(body);
        if effect.unwrap_or(false) {
            self.constrain_effect_body(name, &ret_ty, body_ity);
        } else {
            self.constrain(ret_ty, body_ity, format!("fn '{}'", name));
        }
        self.env.current_ret = prev.0; self.env.can_call_effect = prev.1; self.env.auto_unwrap = prev.2;
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
                let resolved = resolve_vars(&ity, &self.solutions);
                // Update env.top_lets with the fully inferred type
                if matches!(self.env.top_lets.get(name.as_str()), Some(Ty::Unknown) | None) {
                    self.env.top_lets.insert(name.clone(), resolved);
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
            Ty::Variant { cases, .. } => cases.iter().map(|c| c.name.clone()).collect(),
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
                    "List" => Ty::list(ra.first().cloned().unwrap_or(Ty::Unknown)),
                    "Option" => Ty::option(ra.first().cloned().unwrap_or(Ty::Unknown)),
                    "Result" if ra.len() >= 2 => Ty::result(ra[0].clone(), ra[1].clone()),
                    "Map" if ra.len() >= 2 => Ty::map_of(ra[0].clone(), ra[1].clone()),
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
