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
    protocol_bounds: HashMap<Sym, Vec<Sym>>,
    lambda_id_counter: u32,
    /// Maps const param name → VarId for value parameter lowering.
    pub const_param_vars: HashMap<Sym, VarId>,
    /// Definition table for cross-package resolution.
    pub def_table: almide_ir::DefTable,
    /// Maps qualified name (e.g. "snaidhm.web.gpu.STORAGE") → DefId.
    pub def_map: HashMap<Sym, almide_ir::DefId>,
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
            protocol_bounds: HashMap::new(),
            lambda_id_counter: 0,
            const_param_vars: HashMap::new(),
            def_table: env.def_table.clone(),
            def_map: env.def_map.iter().map(|(k, v)| (*k, *v)).collect(),
        }
    }

    /// Find a convention function (e.g., "Dog.eq") for a given type and convention name.
    /// Returns the fully qualified function name if:
    /// - The function is explicitly defined in env.functions, OR
    /// - The type declares `deriving <Convention>` (auto-derive will generate the function)
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
                let mut td = types::lower_type_decl(&mut ctx, name, ty, deriving, visibility, generics.as_ref());
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
            ast::Decl::Test { name, body, .. } => {
                let test_fn = lower_test(&mut ctx, name, body);
                functions.push(test_fn);
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
    let auto_derived = generate_auto_derives(&mut ctx, &type_decls, &functions);
    functions.extend(auto_derived);

    // Collect effect fn names from TypeEnv (user-defined + stdlib)
    let effect_fn_names: std::collections::HashSet<almide_base::intern::Sym> = env.functions.iter()
        .filter(|(_, sig)| sig.is_effect)
        .map(|(name, _)| *name)
        .collect();

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
    auto_try::insert_auto_try(&mut program);

    // Collect stdlib modules used in root functions/top_lets.
    // ir_link extends this with modules from dependencies.
    program.used_stdlib_modules = collect_stdlib_modules(&program);

    program
}

/// Collect stdlib module names referenced by CallTarget::Module in the IR.
/// Scans all functions and modules (including transitive deps).
fn collect_stdlib_modules(program: &IrProgram) -> std::collections::HashSet<String> {
    let mut used = std::collections::HashSet::new();

    fn scan_expr(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
        match &expr.kind {
            IrExprKind::Call { target, args, .. } => {
                if let CallTarget::Module { module, .. } = target {
                    used.insert(module.to_string());
                }
                if let CallTarget::Method { object, .. } = target {
                    scan_expr(object, used);
                }
                for a in args { scan_expr(a, used); }
            }
            IrExprKind::RuntimeCall { symbol, args } => {
                // Extract module from runtime symbol: almide_rt_{module}_{fn}
                if let Some(rest) = symbol.as_str().strip_prefix("almide_rt_") {
                    if let Some(pos) = rest.find('_') {
                        used.insert(rest[..pos].to_string());
                    }
                }
                for a in args { scan_expr(a, used); }
            }
            IrExprKind::Block { stmts, expr: tail } => {
                for s in stmts { scan_stmt(s, used); }
                if let Some(e) = tail { scan_expr(e, used); }
            }
            IrExprKind::If { cond, then, else_ } => {
                scan_expr(cond, used); scan_expr(then, used); scan_expr(else_, used);
            }
            IrExprKind::Match { subject, arms } => {
                scan_expr(subject, used);
                for arm in arms {
                    if let Some(g) = &arm.guard { scan_expr(g, used); }
                    scan_expr(&arm.body, used);
                }
            }
            IrExprKind::Lambda { body, .. } => scan_expr(body, used),
            IrExprKind::ForIn { iterable, body, .. } => {
                scan_expr(iterable, used);
                for s in body { scan_stmt(s, used); }
            }
            IrExprKind::While { cond, body } => {
                scan_expr(cond, used);
                for s in body { scan_stmt(s, used); }
            }
            IrExprKind::BinOp { left, right, .. } => { scan_expr(left, used); scan_expr(right, used); }
            IrExprKind::UnOp { operand, .. } => scan_expr(operand, used),
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
                for e in elements { scan_expr(e, used); }
            }
            IrExprKind::Record { fields, .. } => { for (_, v) in fields { scan_expr(v, used); } }
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
            | IrExprKind::OptionSome { expr: e } | IrExprKind::Unwrap { expr: e }
            | IrExprKind::Try { expr: e } | IrExprKind::ToOption { expr: e }
            | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
            | IrExprKind::Member { object: e, .. } => scan_expr(e, used),
            IrExprKind::UnwrapOr { expr: e, fallback } => { scan_expr(e, used); scan_expr(fallback, used); }
            IrExprKind::StringInterp { parts } => {
                for p in parts { if let IrStringPart::Expr { expr } = p { scan_expr(expr, used); } }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                scan_expr(base, used);
                for (_, v) in fields { scan_expr(v, used); }
            }
            IrExprKind::IndexAccess { object, index } => { scan_expr(object, used); scan_expr(index, used); }
            IrExprKind::MapLiteral { entries } => {
                for (k, v) in entries { scan_expr(k, used); scan_expr(v, used); }
            }
            IrExprKind::Range { start, end, .. } => { scan_expr(start, used); scan_expr(end, used); }
            _ => {}
        }
    }
    fn scan_stmt(stmt: &IrStmt, used: &mut std::collections::HashSet<String>) {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } => scan_expr(value, used),
            IrStmtKind::Expr { expr } => scan_expr(expr, used),
            IrStmtKind::Assign { value, .. } => scan_expr(value, used),
            IrStmtKind::Guard { cond, else_ } => { scan_expr(cond, used); scan_expr(else_, used); }
            _ => {}
        }
    }

    for func in &program.functions { scan_expr(&func.body, &mut used); }
    for tl in &program.top_lets { scan_expr(&tl.value, &mut used); }
    for module in &program.modules {
        used.insert(module.name.to_string());
        for func in &module.functions { scan_expr(&func.body, &mut used); }
        for tl in &module.top_lets { scan_expr(&tl.value, &mut used); }
    }

    used
}

/// Verify no inference TypeVars (?N) remain in the IR.
/// Any remaining TypeVar indicates a type checker bug — the codegen cannot
/// reliably generate correct code without concrete types.
fn resolve_inference_typevars(program: &mut IrProgram) {
    use crate::types::Ty;
    fn has_typevar(ty: &Ty) -> bool {
        match ty {
            Ty::TypeVar(name) => name.starts_with('?'),
            Ty::Unknown => false,
            Ty::Applied(_, args) => args.iter().any(has_typevar),
            Ty::Tuple(elems) => elems.iter().any(has_typevar),
            Ty::Fn { params, ret } => params.iter().any(has_typevar) || has_typevar(ret),
            Ty::Named(_, args) => args.iter().any(has_typevar),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| has_typevar(t)),
            _ => false,
        }
    }
    fn resolve_ty(ty: &mut Ty) {
        match ty {
            Ty::TypeVar(name) if name.starts_with('?') => *ty = Ty::Unknown,
            Ty::Applied(_, args) => { for a in args { resolve_ty(a); } }
            Ty::Tuple(elems) => { for e in elems { resolve_ty(e); } }
            Ty::Fn { params, ret } => { for p in params { resolve_ty(p); } resolve_ty(ret); }
            Ty::Named(_, args) => { for a in args { resolve_ty(a); } }
            Ty::Record { fields } | Ty::OpenRecord { fields } => { for (_, t) in fields { resolve_ty(t); } }
            _ => {}
        }
    }
    fn resolve_expr(expr: &mut IrExpr) {
        resolve_ty(&mut expr.ty);
        match &mut expr.kind {
            IrExprKind::Call { args, .. } => { for a in args { resolve_expr(a); } }
            IrExprKind::Lambda { body, params, .. } => {
                for (_, ty) in params { resolve_ty(ty); }
                resolve_expr(body);
            }
            IrExprKind::BinOp { left, right, .. } => { resolve_expr(left); resolve_expr(right); }
            IrExprKind::Match { subject, arms, .. } => {
                resolve_expr(subject);
                for arm in arms { resolve_expr(&mut arm.body); }
            }
            IrExprKind::If { cond, then, else_, .. } => {
                resolve_expr(cond); resolve_expr(then); resolve_expr(else_);
            }
            IrExprKind::Block { stmts, expr, .. } => {
                for s in stmts { resolve_stmt(s); }
                if let Some(e) = expr { resolve_expr(e); }
            }
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
                for e in elements { resolve_expr(e); }
            }
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
            | IrExprKind::OptionSome { expr: e } | IrExprKind::Unwrap { expr: e }
            | IrExprKind::Try { expr: e } | IrExprKind::ToOption { expr: e }
            | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
            | IrExprKind::ToVec { expr: e } | IrExprKind::UnOp { operand: e, .. }
            | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e } => {
                resolve_expr(e);
            }
            IrExprKind::UnwrapOr { expr: e, fallback } => { resolve_expr(e); resolve_expr(fallback); }
            IrExprKind::Record { fields, .. } => { for (_, v) in fields { resolve_expr(v); } }
            IrExprKind::ForIn { iterable, body, .. } => {
                resolve_expr(iterable);
                for s in body { resolve_stmt(s); }
            }
            IrExprKind::While { cond, body } => {
                resolve_expr(cond);
                for s in body { resolve_stmt(s); }
            }
            IrExprKind::Member { object, .. } | IrExprKind::OptionalChain { expr: object, .. } => resolve_expr(object),
            IrExprKind::IndexAccess { object, index } | IrExprKind::Range { start: object, end: index, .. } => {
                resolve_expr(object); resolve_expr(index);
            }
            IrExprKind::StringInterp { parts } => {
                for p in parts { if let IrStringPart::Expr { expr } = p { resolve_expr(expr); } }
            }
            _ => {}
        }
    }
    fn resolve_stmt(stmt: &mut IrStmt) {
        match &mut stmt.kind {
            IrStmtKind::Bind { ty, value, .. } => { resolve_ty(ty); resolve_expr(value); }
            IrStmtKind::Assign { value, .. } => resolve_expr(value),
            IrStmtKind::Expr { expr } => resolve_expr(expr),
            IrStmtKind::Guard { cond, else_ } => { resolve_expr(cond); resolve_expr(else_); }
            _ => {}
        }
    }
    // Resolve all remaining inference TypeVars → Unknown
    for func in &mut program.functions {
        resolve_expr(&mut func.body);
        resolve_ty(&mut func.ret_ty);
        for p in &mut func.params { resolve_ty(&mut p.ty); }
    }
    for tl in &mut program.top_lets {
        resolve_expr(&mut tl.value);
        resolve_ty(&mut tl.ty);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            resolve_expr(&mut func.body);
            resolve_ty(&mut func.ret_ty);
            for p in &mut func.params { resolve_ty(&mut p.ty); }
        }
    }
    for i in 0..program.var_table.len() {
        resolve_ty(&mut program.var_table.entries[i].ty);
    }
}

pub fn lower_module(
    name: &str,
    prog: &ast::Program,
    env: &TypeEnv,
    type_map: &TypeMap,
    versioned_name: Option<String>,
) -> IrModule {
    let mut ir_prog = lower_program_with_prefix(prog, env, type_map, Some(name));
    // Set module_origin on top_let VarInfo — walker prefixes at emit time.
    // IR names stay clean (no ALMIDE_RT_ mangling in the IR).
    let mod_ident = versioned_name.as_deref().unwrap_or(name).replace('.', "_");
    for tl in &ir_prog.top_lets {
        ir_prog.var_table.entries[tl.var.0 as usize].module_origin = Some(mod_ident.clone());
    }
    // Collect exports: public functions, types, constants
    let mut exports = Vec::new();
    for func in &ir_prog.functions {
        if matches!(func.visibility, IrVisibility::Public) && !func.is_test {
            exports.push(IrExport::Function { name: func.name, is_effect: func.is_effect });
        }
    }
    for td in &ir_prog.type_decls {
        if matches!(td.visibility, IrVisibility::Public) {
            exports.push(IrExport::Type { name: td.name });
        }
    }
    for tl in &ir_prog.top_lets {
        let tl_name = ir_prog.var_table.get(tl.var).name;
        exports.push(IrExport::Constant { name: tl_name });
    }

    IrModule {
        name: sym(name),
        versioned_name: versioned_name.map(|v| sym(&v)),
        type_decls: std::mem::take(&mut ir_prog.type_decls),
        functions: std::mem::take(&mut ir_prog.functions),
        top_lets: std::mem::take(&mut ir_prog.top_lets),
        var_table: std::mem::take(&mut ir_prog.var_table),
        exports,
        imports: Vec::new(), // populated during import resolution (future)
    }
}

// ── Function lowering ───────────────────────────────────────────

fn lower_fn(
    ctx: &mut LowerCtx,
    name: &str, params: &[ast::Param], body: &ast::Expr,
    effect: &Option<bool>, r#async: &Option<bool>, span: &Option<ast::Span>,
    generics: &Option<Vec<ast::GenericParam>>, extern_attrs: &[ast::ExternAttr],
    export_attrs: &[ast::ExportAttr],
    attrs: &[ast::Attribute],
    visibility: &ast::Visibility, module_prefix: Option<&str>,
) -> IrFunction {
    ctx.push_scope();

    // Set up protocol bounds and const params for this function's generics
    let saved_pb = std::mem::take(&mut ctx.protocol_bounds);
    let saved_cp = std::mem::take(&mut ctx.const_param_vars);
    if let Some(gs) = generics {
        for g in gs {
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() {
                    // Check if this is a const param (scalar type bound)
                    let is_const = bounds.len() == 1
                        && crate::canonicalize::registration::SCALAR_TYPE_NAMES.contains(&bounds[0].as_str());
                    if !is_const {
                        ctx.protocol_bounds.insert(g.name, bounds.clone());
                    }
                }
            }
        }
    }

    let mut ir_params = Vec::new();

    // Add const params as implicit leading parameters
    if let Some(gs) = generics {
        for g in gs {
            if let Some(bounds) = &g.bounds {
                let is_const = bounds.len() == 1
                    && crate::canonicalize::registration::SCALAR_TYPE_NAMES.contains(&bounds[0].as_str());
                if is_const {
                    let param_ty = resolve_type_expr(&ast::TypeExpr::Simple { name: sym(&bounds[0]) });
                    let var = ctx.define_var(&g.name, param_ty.clone(), Mutability::Let, span.clone());
                    ctx.const_param_vars.insert(sym(&g.name), var);
                    ir_params.push(IrParam {
                        var, ty: param_ty, name: g.name,
                        borrow: ParamBorrow::Own, open_record: None, default: None,
                        attrs: Vec::new(),
                    });
                }
            }
        }
    }

    for p in params {
        let ty = resolve_type_expr(&p.ty);
        let var = ctx.define_var(&p.name, ty.clone(), Mutability::Let, span.clone());
        let default = p.default.as_ref().map(|d| Box::new(lower_expr(ctx, d)));
        ir_params.push(IrParam {
            var, ty: ty.clone(), name: p.name,
            borrow: ParamBorrow::Own, open_record: None, default,
            attrs: p.attrs.clone(),
        });
    }

    let ret_ty = {
        // For module functions, look up the module-prefixed name first (e.g., "option.unwrap_or")
        // to avoid picking up a user function with the same bare name.
        let prefixed = module_prefix.map(|p| format!("{}.{}", p, name));
        let sig = prefixed.as_ref()
            .and_then(|pn| ctx.env.functions.get(&sym(pn)))
            .or_else(|| ctx.env.functions.get(&sym(name)));
        if let Some(sig) = sig {
            sig.ret.clone()
        } else {
            ctx.expr_ty(body)
        }
    };

    let ir_body = lower_expr(ctx, body);
    ctx.protocol_bounds = saved_pb;
    ctx.const_param_vars = saved_cp;
    ctx.pop_scope();

    let is_effect = effect.unwrap_or(false);
    let is_async = r#async.unwrap_or(false);
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };

    // Strip const params from generics (they became runtime params above).
    // If only const params remain, generics becomes None (non-generic function).
    let stripped_generics = generics.as_ref().map(|gs| {
        let remaining: Vec<_> = gs.iter().filter(|g| {
            !g.bounds.as_ref().map_or(false, |bs| {
                bs.len() == 1 && crate::canonicalize::registration::SCALAR_TYPE_NAMES.contains(&bs[0].as_str())
            })
        }).cloned().collect();
        if remaining.is_empty() { None } else { Some(remaining) }
    }).flatten();

    // Resolve mut params: from `mut` keyword and @mutating(param_name) annotation
    let mut mutated_params: Vec<usize> = params.iter().enumerate()
        .filter(|(_, p)| p.is_mut)
        .map(|(i, _)| i)
        .collect();
    // Merge @mutating(param_name) indices (backward compat)
    for attr in attrs.iter().filter(|a| a.name.as_str() == "mutating") {
        for arg in &attr.args {
            if let almide_lang::ast::AttrValue::Ident { name: pname } = &arg.value {
                if let Some(idx) = params.iter().position(|p| p.name == *pname) {
                    if !mutated_params.contains(&idx) {
                        mutated_params.push(idx);
                    }
                }
            }
        }
    }

    IrFunction {
        name: sym(name), params: ir_params, ret_ty, body: ir_body,
        is_effect, is_async, is_test: false,
        generics: stripped_generics, extern_attrs: extern_attrs.to_vec(),
        export_attrs: export_attrs.to_vec(),
        attrs: attrs.to_vec(),
        visibility: vis,
        doc: None, blank_lines_before: 0,
        def_id: ctx.def_map.get(&sym(name)).copied(),
        mutated_params, module_origin: None,
    }
}

fn lower_test(ctx: &mut LowerCtx, name: &str, body: &ast::Expr) -> IrFunction {
    ctx.push_scope();
    let ir_body = lower_expr(ctx, body);
    ctx.pop_scope();
    IrFunction {
        name: sym(&format!("{}{}", almide_ir::TEST_NAME_PREFIX, name)),
        params: vec![], ret_ty: Ty::Unit, body: ir_body,
        is_effect: true, is_async: false, is_test: true,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], module_origin: None,
    }
}
