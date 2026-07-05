/// AST + TypeMap → Typed IR lowering pass.
///
/// Input:    Program + TypeEnv + TypeMap (ExprId→Ty, populated by checker)
/// Output:   IrProgram
/// Owns:     desugaring (pipe→call, UFCS, interpolation, operators→BinOp), VarId assignment
/// Does NOT: type inference (trusts checker), codegen decisions (trusts codegen)
///
/// Principles:
/// 1. **Checker is the source of truth** — every expression's type comes from
///    the TypeMap (populated by the constraint-based checker). Lower never
///    guesses types or falls back to Unknown.
/// 2. **No type inference** — lower is a mechanical translation, not a type
///    checker. If an ExprId is missing from the TypeMap, that's a checker bug.
/// 3. **Desugar once** — pipes, UFCS, string interpolation, operators are
///    desugared here and nowhere else.
/// 4. **VarId for everything** — all variable references become VarId lookups.
///    No string-based variable resolution in codegen.

use std::collections::HashMap;
use almide_lang::ast;
use almide_base::intern::{Sym, sym};
use almide_ir::*;
use crate::types::{Ty, TypeEnv, TypeMap};

mod expressions;
mod calls;
mod statements;
mod types;
mod derive;
mod derive_codec;
mod auto_try;

use expressions::lower_expr;
use types::resolve_type_expr;
use derive::generate_auto_derives;

// ── Context ─────────────────────────────────────────────────────

pub struct LowerCtx<'a> {
    pub var_table: VarTable,
    scopes: Vec<HashMap<Sym, VarId>>,
    env: &'a TypeEnv,
    type_map: &'a TypeMap,
    fn_defaults: HashMap<Sym, Vec<Option<ast::Expr>>>,
    type_conventions: HashMap<Sym, std::collections::HashSet<Sym>>,
    /// `Type.convention` names the user wrote explicitly (vs auto-derived).
    explicit_convention_fns: std::collections::HashSet<Sym>,
    protocol_bounds: HashMap<Sym, Vec<Sym>>,
    lambda_id_counter: u32,
    /// Maps const param name → VarId for value parameter lowering.
    pub const_param_vars: HashMap<Sym, VarId>,
    /// Definition table for cross-package resolution.
    pub def_table: almide_ir::DefTable,
    /// Maps qualified name (e.g. "snaidhm.web.gpu.STORAGE") → DefId.
    pub def_map: HashMap<Sym, almide_ir::DefId>,
    /// The module currently being lowered (its prefix), or None for the root
    /// program. Used to pin a struct-literal constructor to its qualified
    /// canonical name `mod.Type` (#433), mirroring `lower_type_decl`.
    pub current_module: Option<Sym>,
    /// Vars whose binding carried an EXPLICIT `Result[..]` annotation
    /// (`let r: Result[Int, String] = step()`). auto_try keeps these as
    /// Result instead of inserting `?`. Only the annotation distinguishes
    /// them in the IR: an un-annotated `let v = boom()` where boom DECLARES
    /// `-> Result[..]` has the identical Bind.ty but must auto-unwrap (#485).
    pub annotated_result_vars: std::collections::HashSet<VarId>,
}

impl<'a> LowerCtx<'a> {
    pub fn new(env: &'a TypeEnv, type_map: &'a TypeMap) -> Self {
        LowerCtx {
            var_table: VarTable::new(),
            scopes: vec![HashMap::new()],
            env,
            type_map,
            fn_defaults: HashMap::new(),
            type_conventions: HashMap::new(),
            explicit_convention_fns: std::collections::HashSet::new(),
            protocol_bounds: HashMap::new(),
            lambda_id_counter: 0,
            const_param_vars: HashMap::new(),
            def_table: env.def_table.clone(),
            def_map: env.def_map.iter().map(|(k, v)| (*k, *v)).collect(),
            current_module: None,
            annotated_result_vars: std::collections::HashSet::new(),
        }
    }

    /// Find a convention function (e.g., "Dog.eq") for a given type and convention name.
    /// Returns the fully qualified function name if:
    /// - The function is explicitly defined in env.functions, OR
    /// - The type declares `deriving <Convention>` (auto-derive will generate the function)
    /// A convention method the user wrote EXPLICITLY (not one auto-derive will
    /// synthesize). String interpolation of a record/variant uses this — when no
    /// explicit `repr` exists it falls through to the codegen `AlmideRepr` impl,
    /// the canonical Almide-literal form (quoted strings, Display floats), so a
    /// `deriving Repr` record and a plain record interpolate identically.
    pub(super) fn find_explicit_convention_fn(&self, ty: &Ty, convention: &str) -> Option<Sym> {
        if let Ty::Named(type_name, _) = ty {
            let fn_name = sym(&format!("{}.{}", type_name, convention));
            if self.explicit_convention_fns.contains(&fn_name) {
                return Some(fn_name);
            }
        }
        None
    }

    pub(super) fn find_convention_fn(&self, ty: &Ty, convention: &str) -> Option<Sym> {
        if let Ty::Named(type_name, _) = ty {
            let fn_name = sym(&format!("{}.{}", type_name, convention));
            // Check explicit definition
            if self.env.functions.contains_key(&fn_name) {
                return Some(fn_name);
            }
            // Check if auto-derive will generate it
            let conv_upper = match convention {
                "eq" => "Eq", "repr" => "Repr", "ord" => "Ord", "hash" => "Hash",
                _ => return None,
            };
            if self.type_conventions.get(&sym(conv_upper)).map_or(false, |types| types.contains(type_name)) {
                return Some(fn_name);
            }
        }
        None
    }

    pub(super) fn next_lambda_id(&mut self) -> u32 {
        let id = self.lambda_id_counter;
        self.lambda_id_counter += 1;
        id
    }

    pub(super) fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    pub(super) fn pop_scope(&mut self) {
        debug_assert!(self.scopes.len() > 1, "scope underflow");
        self.scopes.pop();
    }

    pub(super) fn define_var(&mut self, name: &str, ty: Ty, mutability: Mutability, span: Option<ast::Span>) -> VarId {
        let s = sym(name);
        let id = self.var_table.alloc(s, ty, mutability, span);
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(s, id);
        }
        id
    }

    pub(super) fn lookup_var(&self, name: &str) -> Option<VarId> {
        let s = sym(name);
        for scope in self.scopes.iter().rev() {
            if let Some(&id) = scope.get(&s) {
                return Some(id);
            }
        }
        None
    }

    /// Get the type of an expression from the TypeMap.
    /// Falls back to literal defaults, field resolution for Member expressions,
    /// and UFCS call return types that the checker couldn't determine.
    pub(super) fn expr_ty(&self, expr: &ast::Expr) -> Ty {
        let ty = self.type_map.get(&expr.id).cloned().unwrap_or_else(|| {
            // Fallback for expressions not in the type map (e.g., pattern literals)
            match &expr.kind {
                ast::ExprKind::Int { .. } => Ty::Int,
                ast::ExprKind::Float { .. } => Ty::Float,
                ast::ExprKind::String { .. } | ast::ExprKind::InterpolatedString { .. } => Ty::String,
                ast::ExprKind::Bool { .. } => Ty::Bool,
                ast::ExprKind::Unit => Ty::Unit,
                ast::ExprKind::None => Ty::option(Ty::Unknown),
                _ => Ty::Unknown,
            }
        });
        if ty == Ty::Unknown {
            // Resolve Member field types from the parent's known record type
            if let ast::ExprKind::Member { object, field, .. } = &expr.kind {
                let parent_ty = self.expr_ty(object);
                let resolved = self.env.resolve_named(&parent_ty);
                match &resolved {
                    Ty::Record { fields } | Ty::OpenRecord { fields } =>
                        return fields.iter().find(|(n, _)| n == field)
                            .map(|(_, t)| t.clone())
                            .unwrap_or(Ty::Unknown),
                    _ => {}
                }
            }
        }
        ty
    }

    /// Resolve a field type on a known object type.
    pub(super) fn resolve_field_ty(&self, obj_ty: &Ty, field: &str) -> Ty {
        match obj_ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
            }
            Ty::Named(name, _) => {
                if let Some(def) = self.env.types.get(name) {
                    self.resolve_field_ty(def, field)
                } else { Ty::Unknown }
            }
            Ty::TypeVar(tv) => {
                if let Some(bound) = self.env.structural_bounds.get(tv) {
                    self.resolve_field_ty(bound, field)
                } else { Ty::Unknown }
            }
            _ => Ty::Unknown,
        }
    }

    pub(super) fn mk(&self, kind: IrExprKind, ty: Ty, span: Option<ast::Span>) -> IrExpr {
        IrExpr { kind, ty, span, def_id: None }
    }

    pub(super) fn mk_def(&self, kind: IrExprKind, ty: Ty, span: Option<ast::Span>, def_id: DefId) -> IrExpr {
        IrExpr { kind, ty, span, def_id: Some(def_id) }
    }
}

// ── Public API ──────────────────────────────────────────────────

pub fn lower_program(prog: &ast::Program, env: &TypeEnv, type_map: &TypeMap) -> IrProgram {
    lower_program_with_prefix(prog, env, type_map, None)
}

fn lower_program_with_prefix(prog: &ast::Program, env: &TypeEnv, type_map: &TypeMap, module_prefix: Option<&str>) -> IrProgram {
    let mut ctx = LowerCtx::new(env, type_map);
    ctx.current_module = module_prefix.map(sym);

    // Register cross-package top-level lets that weren't in register_decls
    // (dependency packages populate env.top_lets during project fetch).
    for (qual_name, ty) in &env.top_lets {
        if ctx.def_map.contains_key(qual_name) { continue; }
        let qual = qual_name.as_str();
        if let Some(dot_pos) = qual.rfind('.') {
            let module = &qual[..dot_pos];
            let name = &qual[dot_pos + 1..];
            let package = module.split('.').next().unwrap_or(module);
            let def_id = ctx.def_table.alloc(
                sym(package), sym(module), sym(name),
                almide_ir::DefKind::TopLet, ty.clone(),
            );
            ctx.def_map.insert(*qual_name, def_id);
        }
    }

    // Collect type conventions (deriving Eq, Repr, etc.)
    for decl in &prog.decls {
        if let ast::Decl::Type { name, deriving: Some(derives), .. } = decl {
            for conv in derives {
                ctx.type_conventions.entry(*conv).or_default().insert(*name);
            }
        }
    }

    // Collect convention methods the user wrote EXPLICITLY (a dotted `fn X.repr`
    // or an `impl` method), as opposed to ones auto-derive will synthesize. The
    // interpolation `repr` dispatch uses this so a `deriving Repr` record falls
    // through to the codegen `AlmideRepr` impl (canonical literal form) while a
    // hand-written `fn X.repr` still overrides it.
    for decl in &prog.decls {
        match decl {
            ast::Decl::Fn { name, body: Some(_), .. } if name.as_str().contains('.') => {
                ctx.explicit_convention_fns.insert(*name);
            }
            ast::Decl::Impl { for_, methods, .. } => {
                for m in methods {
                    if let ast::Decl::Fn { name, body: Some(_), .. } = m {
                        ctx.explicit_convention_fns.insert(sym(&format!("{}.{}", for_, name)));
                    }
                }
            }
            _ => {}
        }
    }

    // Collect function default arguments for call-site expansion
    for decl in &prog.decls {
        if let ast::Decl::Fn { name, params, .. } = decl {
            if params.iter().any(|p| p.default.is_some()) {
                let defaults: Vec<Option<ast::Expr>> = params.iter()
                    .map(|p| p.default.as_ref().map(|d| *d.clone()))
                    .collect();
                ctx.fn_defaults.insert(*name, defaults);
            }
        }
    }

    let mut functions = Vec::new();
    let mut top_lets = Vec::new();
    let mut type_decls = Vec::new();

    // Pre-pass: register every top-level `let` binding in the root scope so that
    // forward references from earlier function bodies resolve to the correct
    // VarId. Without this, the lookup misses, the resolver falls back to the
    // error-recovery `VarId(0)`, and the reference silently aliases the first
    // variable allocated globally (typically a local in the first lowered fn).
    for decl in &prog.decls {
        if let ast::Decl::TopLet { name, value, mutable, .. } = decl {
            let prefixed_key = module_prefix
                .map(|p| almide_base::intern::sym(&format!("{}.{}", p, name.as_str())));
            let val_ty = prefixed_key
                .and_then(|k| ctx.env.top_lets.get(&k).cloned())
                .or_else(|| ctx.env.top_lets.get(name).cloned())
                .unwrap_or_else(|| ctx.expr_ty(value));
            let mutability = if *mutable { Mutability::Var } else { Mutability::Let };
            ctx.define_var(name, val_ty, mutability, None);
        }
    }

    // Pre-pass: collect file-scoped test where clauses
    let file_test_wheres: Vec<ast::TestWhere> = prog.decls.iter().filter_map(|d| {
        if let ast::Decl::TestWhereDef { clauses, .. } = d { Some(clauses.clone()) } else { None }
    }).flatten().collect();

    for (decl_idx, decl) in prog.decls.iter().enumerate() {
        let doc = prog.doc_map.get(decl_idx).cloned().flatten();
        let blank_lines = prog.blank_lines_map.get(decl_idx).copied().unwrap_or(0);

        match decl {
            ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, generics, extern_attrs, export_attrs, attrs, visibility, .. } => {
                let mut f = lower_fn(&mut ctx, name, params, body, effect, r#async, span, generics, extern_attrs, export_attrs, attrs, visibility, module_prefix);
                f.doc = doc;
                f.blank_lines_before = blank_lines;
                functions.push(f);
            }
            // Body-less fn: included in IR with a Hole body when it has
            // an `@extern(...)` binding (codegen emits `use` import) or
            // a generic `@inline_rust(...)` / `@wasm_intrinsic(...)`
            // attribute (stdlib unification: body is declarative only,
            // codegen skips emission and substitutes a template at call
            // sites). Either case keeps the signature in IR so callers
            // type-check against a real IrFunction.
            ast::Decl::Fn { name, params, body: None, effect, r#async, span, generics, extern_attrs, export_attrs, attrs, visibility, .. }
                if !extern_attrs.is_empty()
                    || attrs.iter().any(|a| matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic")) =>
            {
                let hole_body = ast::Expr::new(ast::ExprId(0), span.clone(), ast::ExprKind::Hole);
                let mut f = lower_fn(&mut ctx, name, params, &hole_body, effect, r#async, span, generics, extern_attrs, export_attrs, attrs, visibility, module_prefix);
                f.doc = doc;
                f.blank_lines_before = blank_lines;
                functions.push(f);
            }
            ast::Decl::Type { name, ty, deriving, visibility, generics, .. } => {
                let mut td = types::lower_type_decl(&mut ctx, name, ty, deriving, visibility, generics.as_ref(), module_prefix);
                td.doc = doc;
                td.blank_lines_before = blank_lines;
                type_decls.push(td);
            }
            ast::Decl::TopLet { name, ty: _, value, mutable, .. } => {
                let var = ctx.lookup_var(name).expect("top-level let pre-registered");
                let val_ty = ctx.var_table.get(var).ty.clone();
                let ir_value = lower_expr(&mut ctx, value);
                let kind = classify_top_let_kind(&ir_value);
                let tl_def_id = ctx.def_map.get(&sym(name)).copied();
                top_lets.push(IrTopLet { var, ty: val_ty, value: ir_value, kind, mutable: *mutable, doc, blank_lines_before: blank_lines, def_id: tl_def_id });
            }
            ast::Decl::TestWhereDef { .. } => {} // collected in pre-pass below
            ast::Decl::Test { name, body, where_clauses, .. } => {
                let cases: Vec<_> = where_clauses.iter()
                    .filter_map(|wc| match wc { ast::TestWhere::Case { name, bindings } => Some((name.clone(), bindings.clone())), _ => None })
                    .collect();
                let mut top_binds: Vec<_> = file_test_wheres.clone();
                top_binds.extend(where_clauses.iter()
                    .filter(|wc| !matches!(wc, ast::TestWhere::Case { .. }))
                    .cloned());
                if cases.is_empty() {
                    let test_fn = lower_test_with_where(&mut ctx, name, body, &top_binds);
                    functions.push(test_fn);
                } else {
                    for (case_name, case_binds) in &cases {
                        let full_name = format!("{} / {}", name, case_name);
                        let mut merged = top_binds.clone();
                        merged.extend(case_binds.iter().cloned());
                        let test_fn = lower_test_with_where(&mut ctx, &full_name, body, &merged);
                        functions.push(test_fn);
                    }
                }
            }
            ast::Decl::Impl { for_, methods, .. } => {
                for m in methods {
                    if let ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, generics, extern_attrs, export_attrs, attrs, visibility, .. } = m {
                        // Prefix method name with type name: "show" → "Dog.show"
                        let convention_name = format!("{}.{}", for_, name);
                        let f = lower_fn(&mut ctx, &convention_name, params, body, effect, r#async, span, generics, extern_attrs, export_attrs, attrs, visibility, None);
                        functions.push(f);
                    }
                }
            }
            _ => {}
        }
    }

    // Auto-derive: generate convention functions for types that declare deriving but lack custom impl
    let mut auto_derived = generate_auto_derives(&mut ctx, &type_decls, &functions);
    // Stamp every generated convention fn with a synthetic `@derived` marker.
    // This is the authoritative signal that a function is compiler-generated:
    // downstream passes (e.g. borrow inference, #647) must not name-match
    // `encode`/`decode`/`eq`/... to recognise derives — the generator is the
    // single source of truth, so it records the fact structurally here.
    for f in &mut auto_derived {
        f.attrs.push(ast::Attribute { name: sym("derived"), args: vec![], span: None });
    }
    functions.extend(auto_derived);

    // Collect effect fn names from TypeEnv (user-defined + stdlib)
    let effect_fn_names: std::collections::HashSet<almide_base::intern::Sym> = env.functions.iter()
        .filter(|(_, sig)| sig.is_effect)
        .map(|(name, _)| *name)
        .collect();

    let annotated_result_vars = std::mem::take(&mut ctx.annotated_result_vars);
    let mut program = IrProgram { functions, top_lets, type_decls, var_table: ctx.var_table, def_table: ctx.def_table, modules: Vec::new(), type_registry: crate::types::TypeConstructorRegistry::new(), effect_fn_names, effect_map: Default::default(), codegen_annotations: Default::default(), used_stdlib_modules: Default::default() };

    // Register user-defined types in the type constructor registry (HKT foundation)
    for td in &program.type_decls {
        let arity = td.generics.as_ref().map_or(0, |g| g.len());
        program.type_registry.register_user_type(&*td.name, arity);
    }

    compute_use_counts(&mut program); // After auto-derive so derived functions get correct use_counts
    demote_unused_mut(&mut program);

    // Resolve any remaining inference TypeVars to Unknown (prevents codegen ICE)
    resolve_inference_typevars(&mut program);

    // Auto-? insertion: wrap Result-typed calls in Try nodes.
    // This bridges the gap between checker (auto_unwrap strips Result
    // from bindings) and IR (Call nodes carry Result types).
    // #558: callees whose FIRST parameter is Result/Option must NOT have that
    // arg auto-?'d (it would unwrap the very value the callee consumes —
    // `error.context(inner(), msg)`, `result.unwrap_or(r, d)`, …). Derive the
    // set from the signature table instead of a hardcoded module-name list.
    let first_arg_unwraps: std::collections::HashSet<almide_base::intern::Sym> = env.functions.iter()
        .filter_map(|(k, sig)| {
            let first_is_opt_result = sig.params.first()
                .map_or(false, |(_, t)| t.is_result() || matches!(t, almide_lang::types::Ty::Applied(almide_lang::types::TypeConstructorId::Option, _)));
            if first_is_opt_result { Some(*k) } else { None }
        })
        .collect();
    auto_try::insert_auto_try(&mut program, &annotated_result_vars, &first_arg_unwraps);

    // Collect stdlib modules used in root functions/top_lets.
    // ir_link extends this with modules from dependencies.
    program.used_stdlib_modules = collect_stdlib_modules(&program);

    program
}

include!("mod_p2.rs");
include!("mod_p3.rs");
