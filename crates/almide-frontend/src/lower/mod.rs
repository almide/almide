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
    /// True while lowering a `test` block body. The assert family desugars to
    /// the ALS-T18 abort form OUTSIDE tests only — test blocks keep the
    /// harness assertion forms (cargo / the wasm test runner report them).
    pub in_test: bool,
    /// Vars whose binding carried an EXPLICIT `Result[..]` annotation
    /// (`let r: Result[Int, String] = step()`). auto_try keeps these as
    /// Result instead of inserting `?`. Only the annotation distinguishes
    /// them in the IR: an un-annotated `let v = boom()` where boom DECLARES
    /// `-> Result[..]` has the identical Bind.ty but must auto-unwrap (#485).
    pub annotated_result_vars: std::collections::HashSet<VarId>,
    /// Functions synthesized during expression lowering (fan.bounded outlining).
    pub synthesized_fns: Vec<almide_ir::IrFunction>,
    /// Counter for synthesized fan.bounded function names.
    pub bounded_counter: u32,
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
            in_test: false,
            annotated_result_vars: std::collections::HashSet::new(),
            synthesized_fns: Vec::new(),
            bounded_counter: 0,
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
            // The set above only holds the program being lowered, so a custom
            // `repr` defined in ANOTHER module was silently skipped and the
            // value fell through to the derived `AlmideRepr` impl — printing
            // the variant name with no error anywhere (#1087).
            let held = crate::canonicalize::registration::convention_fn_key(self.env, &type_name.to_string(), convention)?;
            if self.env.explicit_convention_fns.contains(&held) {
                return crate::canonicalize::registration::convention_emit_key(self.env, &type_name.to_string(), convention);
            }
        }
        None
    }

    pub(super) fn find_convention_fn(&self, ty: &Ty, convention: &str) -> Option<Sym> {
        if let Ty::Named(type_name, _) = ty {
            let fn_name = sym(&format!("{}.{}", type_name, convention));
            // Check explicit definition, then the bare spelling a derived
            // method is registered under (#1087).
            if let Some(key) = crate::canonicalize::registration::convention_emit_key(self.env, &type_name.to_string(), convention) {
                return Some(key);
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

    register_cross_package_top_lets(&mut ctx, env);
    collect_type_conventions(&mut ctx, prog);
    collect_explicit_convention_fns(&mut ctx, prog);
    collect_fn_defaults(&mut ctx, prog);

    let mut functions = Vec::new();
    let mut top_lets = Vec::new();
    let mut type_decls = Vec::new();

    preregister_top_lets(&mut ctx, prog, module_prefix);
    lower_decls(&mut ctx, prog, module_prefix, &mut functions, &mut top_lets, &mut type_decls);
    append_auto_derives(&mut ctx, &type_decls, &mut functions);

    let annotated_result_vars = std::mem::take(&mut ctx.annotated_result_vars);
    let mut program = build_ir_program(ctx, functions, top_lets, type_decls, env);
    finalize_ir_program(&mut program, env, &annotated_result_vars);
    normalize_effect_fn_types(&mut program);

    program
}

/// #1055: rewrite every `effect (A) -> B` fn TYPE in the IR to its runtime
/// carrier `(A) -> Result[B, String]`. The checker keeps the effect form for
/// its diagnostics; downstream (v0 codegen, the v1 renders, almide-interp)
/// then see EXACTLY the shape the landed D3 fallible-slot machinery already
/// handles — no backend learns a new type. Recursive, so a nested effect fn
/// type inside a container or another fn type normalizes too.
fn normalize_effect_fn_types(program: &mut IrProgram) {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;

    fn norm(ty: &Ty) -> Ty {
        let mapped = ty.map_children(&mut |c: &Ty| norm(c));
        match mapped {
            Ty::Fn { params, ret, is_effect: true } => Ty::Fn {
                params,
                ret: Box::new(Ty::Applied(TypeConstructorId::Result, vec![*ret, Ty::String])),
                is_effect: false,
            },
            other => other,
        }
    }

    fn has_effect_fn(ty: &Ty) -> bool {
        if let Ty::Fn { is_effect: true, .. } = ty {
            return true;
        }
        ty.children().into_iter().any(has_effect_fn)
    }

    struct Norm;
    impl IrMutVisitor for Norm {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            if has_effect_fn(&expr.ty) {
                expr.ty = norm(&expr.ty);
            }
            walk_expr_mut(self, expr);
        }
    }

    let mut v = Norm;
    for f in program.functions.iter_mut().chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut())) {
        for p in f.params.iter_mut() {
            if has_effect_fn(&p.ty) {
                p.ty = norm(&p.ty);
            }
        }
        if has_effect_fn(&f.ret_ty) {
            f.ret_ty = norm(&f.ret_ty);
        }
        v.visit_expr_mut(&mut f.body);
    }
    for entry in program.var_table.entries.iter_mut() {
        if has_effect_fn(&entry.ty) {
            entry.ty = norm(&entry.ty);
        }
    }
}

// Register cross-package top-level lets that weren't in register_decls
// (dependency packages populate env.top_lets during project fetch).
fn register_cross_package_top_lets(ctx: &mut LowerCtx, env: &TypeEnv) {
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
}

// Collect type conventions (deriving Eq, Repr, etc.)
fn collect_type_conventions(ctx: &mut LowerCtx, prog: &ast::Program) {
    for decl in &prog.decls {
        if let ast::Decl::Type { name, deriving: Some(derives), .. } = decl {
            for conv in derives {
                ctx.type_conventions.entry(*conv).or_default().insert(*name);
            }
        }
    }
}

// Collect convention methods the user wrote EXPLICITLY (a dotted `fn X.repr`),
// as opposed to ones auto-derive will synthesize. The
// interpolation `repr` dispatch uses this so a `deriving Repr` record falls
// through to the codegen `AlmideRepr` impl (canonical literal form) while a
// hand-written `fn X.repr` still overrides it.
fn collect_explicit_convention_fns(ctx: &mut LowerCtx, prog: &ast::Program) {
    for decl in &prog.decls {
        match decl {
            ast::Decl::Fn { name, body: Some(_), .. } if name.as_str().contains('.') => {
                ctx.explicit_convention_fns.insert(*name);
            }
            _ => {}
        }
    }
}

// Collect function default arguments for call-site expansion
fn collect_fn_defaults(ctx: &mut LowerCtx, prog: &ast::Program) {
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
}

// Pre-pass: register every top-level `let` binding in the root scope so that
// forward references from earlier function bodies resolve to the correct
// VarId. Without this, the lookup misses, the resolver falls back to the
// error-recovery `VarId(0)`, and the reference silently aliases the first
// variable allocated globally (typically a local in the first lowered fn).
fn preregister_top_lets(ctx: &mut LowerCtx, prog: &ast::Program, module_prefix: Option<&str>) {
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
}

// Main decl loop: lowers every top-level declaration into `functions` /
// `top_lets` / `type_decls`. Each output Vec is only ever pushed to — no arm
// reads back what an earlier arm or iteration wrote — so this is a safe
// accumulator-output extraction.
fn lower_decls(
    ctx: &mut LowerCtx,
    prog: &ast::Program,
    module_prefix: Option<&str>,
    functions: &mut Vec<IrFunction>,
    top_lets: &mut Vec<IrTopLet>,
    type_decls: &mut Vec<IrTypeDecl>,
) {
    // Pre-pass: collect file-scoped test where clauses
    let file_test_wheres: Vec<ast::TestWhere> = prog.decls.iter().filter_map(|d| {
        if let ast::Decl::TestWhereDef { clauses, .. } = d { Some(clauses.clone()) } else { None }
    }).flatten().collect();

    for (decl_idx, decl) in prog.decls.iter().enumerate() {
        let doc = prog.doc_map.get(decl_idx).cloned().flatten();
        let blank_lines = prog.blank_lines_map.get(decl_idx).copied().unwrap_or(0);

        match decl {
            ast::Decl::Fn { name, params, body: Some(body), effect, span, generics, extern_attrs, export_attrs, attrs, visibility, return_type, .. } => {
                let mut f = lower_fn(ctx, &FnToLower {
                    name, params, body: body, effect, span, generics,
                    extern_attrs, export_attrs, attrs, visibility, module_prefix,
                });
                // ADR-0002 Phase 1b (#1103): a `-> T!` fn's VALUE tail lifts
                // into ok(...) — the same ergonomics an effect fn's lifted
                // body has. Done HERE, before the IR splits to the three
                // consumers (codegen / mir / interp), so one wrap serves all.
                // Result-typed exits (Phase 1a's pass-through / ok / err
                // bodies) are left untouched — the wrap is type-driven.
                if matches!(return_type, ast::TypeExpr::Generic { name: g, .. } if g.as_str() == "!") {
                    // ADR-0012 D2 (#1193): the resolver already mapped the
                    // marker to Result[T, E], so the declared E is read off
                    // f.ret_ty — `T!` yields String there, `T!E` yields E.
                    let err_ty = match &f.ret_ty {
                        Ty::Applied(crate::types::TypeConstructorId::Result, a) if a.len() == 2 => {
                            a[1].clone()
                        }
                        _ => Ty::String,
                    };
                    f.body = wrap_fallible_value_tail(f.body, &err_ty);
                }
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
            ast::Decl::Fn { name, params, body: None, effect, span, generics, extern_attrs, export_attrs, attrs, visibility, .. }
                if !extern_attrs.is_empty()
                    || attrs.iter().any(|a| matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic")) =>
            {
                let hole_body = ast::Expr::new(ast::ExprId(0), span.clone(), ast::ExprKind::Hole);
                let mut f = lower_fn(ctx, &FnToLower {
                    name, params, body: &hole_body, effect, span, generics,
                    extern_attrs, export_attrs, attrs, visibility, module_prefix,
                });
                f.doc = doc;
                f.blank_lines_before = blank_lines;
                functions.push(f);
            }
            ast::Decl::Type { name, ty, deriving, visibility, generics, .. } => {
                let mut td = types::lower_type_decl(ctx, &types::TypeToLower {
                    name, ty, deriving, visibility, generics: generics.as_ref(), module_prefix,
                });
                td.doc = doc;
                td.blank_lines_before = blank_lines;
                type_decls.push(td);
            }
            ast::Decl::TopLet { name, ty: _, value, mutable, .. } => {
                let var = ctx.lookup_var(name).expect("top-level let pre-registered");
                let val_ty = ctx.var_table.get(var).ty.clone();
                let ir_value = lower_expr(ctx, value);
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
                    let test_fn = lower_test_with_where(ctx, name, body, &top_binds);
                    functions.push(test_fn);
                } else {
                    for (case_name, case_binds) in &cases {
                        let full_name = format!("{} / {}", name, case_name);
                        let mut merged = top_binds.clone();
                        merged.extend(case_binds.iter().cloned());
                        let test_fn = lower_test_with_where(ctx, &full_name, body, &merged);
                        functions.push(test_fn);
                    }
                }
            }
            _ => {}
        }
    }
}

// Auto-derive: generate convention functions for types that declare deriving
// but lack a custom impl, then append them to `functions`.
fn append_auto_derives(ctx: &mut LowerCtx, type_decls: &[IrTypeDecl], functions: &mut Vec<IrFunction>) {
    let mut auto_derived = generate_auto_derives(ctx, type_decls, functions);
    // Stamp every generated convention fn with a synthetic `@derived` marker.
    // This is the authoritative signal that a function is compiler-generated:
    // downstream passes (e.g. borrow inference, #647) must not name-match
    // `encode`/`decode`/`eq`/... to recognise derives — the generator is the
    // single source of truth, so it records the fact structurally here.
    for f in &mut auto_derived {
        f.attrs.push(ast::Attribute { name: sym("derived"), args: vec![], span: None });
    }
    functions.extend(auto_derived);
}

// Assemble the IrProgram from the lowered pieces and register user-defined
// types in the type constructor registry (HKT foundation). Consumes `ctx` by
// value — nothing after this point needs it, its var_table/def_table move
// straight into the program.
fn build_ir_program(mut ctx: LowerCtx, functions: Vec<IrFunction>, top_lets: Vec<IrTopLet>, type_decls: Vec<IrTypeDecl>, env: &TypeEnv) -> IrProgram {
    // Collect effect fn names from TypeEnv (user-defined + stdlib)
    let effect_fn_names: std::collections::HashSet<almide_base::intern::Sym> = env.functions.iter()
        .filter(|(_, sig)| sig.is_effect)
        .map(|(name, _)| *name)
        .collect();

    let mut functions = functions;
    let mut effect_fn_names = effect_fn_names;
    for f in &ctx.synthesized_fns {
        if f.is_effect {
            effect_fn_names.insert(f.name);
        }
    }
    functions.append(&mut ctx.synthesized_fns);
    let mut program = IrProgram { functions, top_lets, type_decls, var_table: ctx.var_table, def_table: ctx.def_table, modules: Vec::new(), type_registry: crate::types::TypeConstructorRegistry::new(), effect_fn_names, effect_map: Default::default(), codegen_annotations: Default::default(), used_stdlib_modules: Default::default() };

    // Register user-defined types in the type constructor registry (HKT foundation)
    for td in &program.type_decls {
        let arity = td.generics.as_ref().map_or(0, |g| g.len());
        program.type_registry.register_user_type(&*td.name, arity);
    }

    program
}

// Post-processing passes shared by both lower_program and module lowering:
// use-count/mut demotion, TypeVar resolution, auto-`?` insertion, and stdlib
// module collection.
fn finalize_ir_program(program: &mut IrProgram, env: &TypeEnv, annotated_result_vars: &std::collections::HashSet<VarId>) {
    compute_use_counts(program); // After auto-derive so derived functions get correct use_counts
    demote_unused_mut(program);

    // Resolve any remaining inference TypeVars to Unknown (prevents codegen ICE)
    resolve_inference_typevars(program);

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
    auto_try::insert_auto_try(program, annotated_result_vars, &first_arg_unwraps);

    // Collect stdlib modules used in root functions/top_lets.
    // ir_link extends this with modules from dependencies.
    program.used_stdlib_modules = collect_stdlib_modules(program);
}

include!("module_lowering.rs");
include!("test_lowering.rs");

/// ADR-0002 Phase 1b (#1103): lift a `-> T!` fn's VALUE exits into `ok(...)`.
/// Type-driven, mirroring codegen's `wrap_tail_in_ok` discipline: an exit
/// whose IR type is already Result (Phase 1a's pass-through / `ok` / `err`
/// bodies, a call to a Result-returning fn) passes through untouched; a
/// T-typed exit wraps in `ResultOk` at `Result[T, String]`. Runs in the
/// FRONTEND lowering so all three IR consumers (codegen, mir, interp) see
/// the same lifted body.
/// The `E` of the enclosing fallible signature rides in `err_ty` — ADR-0012
/// D2 (#1193): a `-> T!E` fn's lifted `ok(...)` tail must carry
/// `Result[T, E]`, not the historical `Result[T, String]` (the codegen's
/// turbofish prints this node's type verbatim, so a String here was a rustc
/// E0308 on every typed-E fn). Bare `T!` callers pass `Ty::String`.
pub(crate) fn wrap_fallible_value_tail(body: IrExpr, err_ty: &Ty) -> IrExpr {
    let ty = body.ty.clone();
    let span = body.span.clone();
    match body.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let stmts = stmts
                .into_iter()
                .map(|stmt| {
                    let sspan = stmt.span.clone();
                    match stmt.kind {
                        IrStmtKind::Guard { cond, else_ } if !else_.ty.is_result() => IrStmt {
                            kind: IrStmtKind::Guard { cond, else_: wrap_fallible_value_tail(else_, err_ty) },
                            span: sspan,
                        },
                        other => IrStmt { kind: other, span: sspan },
                    }
                })
                .collect();
            let wrapped = wrap_fallible_value_tail(*tail, err_ty);
            let wty = wrapped.ty.clone();
            IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(wrapped)) }, ty: wty, span, def_id: None }
        }
        IrExprKind::If { cond, then, else_ } => {
            let then = Box::new(wrap_fallible_value_tail(*then, err_ty));
            let else_ = Box::new(wrap_fallible_value_tail(*else_, err_ty));
            let wty = then.ty.clone();
            IrExpr { kind: IrExprKind::If { cond, then, else_ }, ty: wty, span, def_id: None }
        }
        IrExprKind::Match { subject, arms } => {
            let arms: Vec<IrMatchArm> = arms
                .into_iter()
                .map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard,
                    body: wrap_fallible_value_tail(arm.body, err_ty),
                })
                .collect();
            let wty = arms
                .first()
                .map(|a| a.body.ty.clone())
                .unwrap_or_else(|| Ty::result(ty, err_ty.clone()));
            IrExpr { kind: IrExprKind::Match { subject, arms }, ty: wty, span, def_id: None }
        }
        // Already Result — Phase 1a's forms stay untouched.
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } => body,
        _ if ty.is_result() => body,
        _ => {
            let result_ty = Ty::result(ty.clone(), err_ty.clone());
            IrExpr {
                kind: IrExprKind::ResultOk { expr: Box::new(IrExpr { kind: body.kind, ty, span: span.clone(), def_id: None }) },
                ty: result_ty,
                span,
                def_id: None,
            }
        }
    }
}
