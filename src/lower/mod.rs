/// AST + Types → Typed IR lowering pass.
///
/// Input:    Program + expr_types + TypeEnv
/// Output:   IrProgram
/// Owns:     desugaring (pipe→call, UFCS, interpolation, operators→BinOp), VarId assignment
/// Does NOT: type inference (trusts checker), codegen decisions (trusts codegen)
///
/// Principles:
/// 1. **Checker is the source of truth** — every expression's type comes from
///    expr_types (populated by the constraint-based checker). Lower never
///    guesses types or falls back to Unknown.
/// 2. **No type inference** — lower is a mechanical translation, not a type
///    checker. If a type is missing from expr_types, that's a checker bug.
/// 3. **Desugar once** — pipes, UFCS, string interpolation, operators are
///    desugared here and nowhere else.
/// 4. **VarId for everything** — all variable references become VarId lookups.
///    No string-based variable resolution in codegen.

use std::collections::HashMap;
use crate::ast;
use crate::intern::{Sym, sym};
use crate::ir::*;
use crate::types::{Ty, TypeEnv};

mod expressions;
mod calls;
mod statements;
mod types;
mod derive;
mod derive_codec;

use expressions::lower_expr;
use types::resolve_type_expr;
use derive::generate_auto_derives;

// ── Context ─────────────────────────────────────────────────────

pub struct LowerCtx<'a> {
    pub var_table: VarTable,
    scopes: Vec<HashMap<Sym, VarId>>,
    expr_types: &'a HashMap<crate::ast::ExprId, Ty>,
    env: &'a TypeEnv,
    /// Default argument expressions for functions: fn_name → vec of defaults (index-aligned with params, None for required)
    fn_defaults: HashMap<Sym, Vec<Option<ast::Expr>>>,
    /// Type names that derive each convention: convention_name → set of type names
    type_conventions: HashMap<Sym, std::collections::HashSet<Sym>>,
    /// Protocol bounds for generic type parameters in scope: TypeVar name → list of protocol names
    /// Set during function lowering for protocol-bounded generics.
    protocol_bounds: HashMap<Sym, Vec<Sym>>,
    lambda_id_counter: u32,
}

impl<'a> LowerCtx<'a> {
    pub fn new(expr_types: &'a HashMap<crate::ast::ExprId, Ty>, env: &'a TypeEnv) -> Self {
        LowerCtx {
            var_table: VarTable::new(),
            scopes: vec![HashMap::new()],
            expr_types,
            env,
            fn_defaults: HashMap::new(),
            type_conventions: HashMap::new(),
            protocol_bounds: HashMap::new(),
            lambda_id_counter: 0,
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

    /// Get the type of an expression from the checker's expr_types.
    /// Falls back to field resolution for Member expressions and UFCS call
    /// return types that the checker couldn't determine (e.g., chained method
    /// calls on lambda parameters before constraint solving).
    pub(super) fn expr_ty(&self, expr: &ast::Expr) -> Ty {
        let ty = self.expr_types.get(&expr.id()).cloned().unwrap_or(Ty::Unknown);
        if ty == Ty::Unknown {
            // Resolve Member field types from the parent's known record type
            if let ast::Expr::Member { object, field, .. } = expr {
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
        // Resolve UFCS call return types: obj.method(args) when receiver type is known.
        // The checker may store Unknown or a bogus Fn type for chained method calls
        // on lambda parameters (constraints solve the receiver as callable rather than
        // as a collection method). Re-derive the type from the stdlib signature.
        if ty == Ty::Unknown || matches!(&ty, Ty::Fn { .. }) {
            if let ast::Expr::Call { callee, .. } = expr {
                if let ast::Expr::Member { object, field, .. } = callee.as_ref() {
                    let obj_ty = self.expr_ty(object);
                    if let Some(module) = crate::check::calls::builtin_module_for_type(&obj_ty) {
                        let key = format!("{}.{}", module, field);
                        if let Some(sig) = self.env.functions.get(&sym(&key)) {
                            return sig.ret.clone();
                        }
                        // Stdlib functions may not be in env.functions (TOML-defined).
                        // Infer a return type from the module so UFCS chaining works.
                        if crate::stdlib::resolve_ufcs_candidates(field).contains(&module) {
                            return Self::infer_stdlib_return_type(module, field);
                        }
                    }
                }
            }
        }
        ty
    }

    /// Infer the return type of a stdlib function by module and method name.
    /// Used when the function signature isn't in env.functions (TOML-defined stdlib).
    /// Returns a type with the correct "kind" for downstream UFCS resolution.
    fn infer_stdlib_return_type(module: &str, method: &str) -> Ty {
        match module {
            "list" => match method {
                "join" => Ty::String,
                "len" | "count" => Ty::Int,
                "any" | "all" | "contains" | "is_empty" => Ty::Bool,
                "find" | "first" | "last" => Ty::option(Ty::Unknown),
                "find_index" | "index_of" => Ty::option(Ty::Int),
                "fold" | "reduce" | "sum" | "product" => Ty::Unknown,
                _ => Ty::list(Ty::Unknown),
            },
            "string" => match method {
                "len" | "count" => Ty::Int,
                "contains" | "starts_with" | "ends_with" | "is_empty"
                    | "is_digit" | "is_alpha" | "is_alphanumeric"
                    | "is_whitespace" | "is_upper" | "is_lower" => Ty::Bool,
                "split" | "lines" | "chars" | "to_bytes" => Ty::list(Ty::String),
                "index_of" | "last_index_of" => Ty::option(Ty::Int),
                "codepoint" => Ty::Int,
                _ => Ty::String,
            },
            "map" => match method {
                "len" | "count" => Ty::Int,
                "contains" | "is_empty" => Ty::Bool,
                "keys" | "values" => Ty::list(Ty::Unknown),
                "entries" => Ty::list(Ty::Tuple(vec![Ty::Unknown, Ty::Unknown])),
                _ => Ty::map_of(Ty::Unknown, Ty::Unknown),
            },
            "option" => match method {
                "is_some" | "is_none" => Ty::Bool,
                "to_list" => Ty::list(Ty::Unknown),
                _ => Ty::option(Ty::Unknown),
            },
            "result" => match method {
                "is_ok" | "is_err" => Ty::Bool,
                _ => Ty::result(Ty::Unknown, Ty::Unknown),
            },
            "int" => Ty::String, // to_string, to_hex
            "float" => match method {
                "is_nan" | "is_infinite" => Ty::Bool,
                "round" | "floor" | "ceil" => Ty::Int,
                _ => Ty::Float,
            },
            _ => Ty::Unknown,
        }
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
        IrExpr { kind, ty, span }
    }
}

// ── Public API ──────────────────────────────────────────────────

pub fn lower_program(prog: &ast::Program, expr_types: &HashMap<crate::ast::ExprId, Ty>, env: &TypeEnv) -> IrProgram {
    let mut ctx = LowerCtx::new(expr_types, env);

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

    for decl in &prog.decls {
        match decl {
            ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, generics, extern_attrs, visibility, .. } => {
                let f = lower_fn(&mut ctx, name, params, body, effect, r#async, span, generics, extern_attrs, visibility, None);
                functions.push(f);
            }
            // Extern fn without body: include in IR with Hole body (codegen emits `use` import)
            ast::Decl::Fn { name, params, body: None, effect, r#async, span, generics, extern_attrs, visibility, .. } if !extern_attrs.is_empty() => {
                let hole_body = ast::Expr::Hole { id: ast::ExprId(0), span: span.clone(), resolved_type: None };
                let f = lower_fn(&mut ctx, name, params, &hole_body, effect, r#async, span, generics, extern_attrs, visibility, None);
                functions.push(f);
            }
            ast::Decl::Type { name, ty, deriving, visibility, generics, .. } => {
                type_decls.push(types::lower_type_decl(&mut ctx, name, ty, deriving, visibility, generics.as_ref()));
            }
            ast::Decl::TopLet { name, ty: _, value, .. } => {
                let val_ty = ctx.env.top_lets.get(name).cloned().unwrap_or_else(|| ctx.expr_ty(value));
                let var = ctx.define_var(name, val_ty.clone(), Mutability::Let, None);
                let ir_value = lower_expr(&mut ctx, value);
                let kind = classify_top_let_kind(&ir_value);
                top_lets.push(IrTopLet { var, ty: val_ty, value: ir_value, kind });
            }
            ast::Decl::Test { name, body, .. } => {
                let test_fn = lower_test(&mut ctx, name, body);
                functions.push(test_fn);
            }
            ast::Decl::Impl { for_, methods, .. } => {
                for m in methods {
                    if let ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, generics, extern_attrs, visibility, .. } = m {
                        // Prefix method name with type name: "show" → "Dog.show"
                        let convention_name = format!("{}.{}", for_, name);
                        let f = lower_fn(&mut ctx, &convention_name, params, body, effect, r#async, span, generics, extern_attrs, visibility, None);
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
    let effect_fn_names: std::collections::HashSet<crate::intern::Sym> = env.functions.iter()
        .filter(|(_, sig)| sig.is_effect)
        .map(|(name, _)| *name)
        .collect();

    let mut program = IrProgram { functions, top_lets, type_decls, var_table: ctx.var_table, modules: Vec::new(), type_registry: crate::types::TypeConstructorRegistry::new(), effect_fn_names, effect_map: Default::default(), codegen_annotations: Default::default() };

    // Register user-defined types in the type constructor registry (HKT foundation)
    for td in &program.type_decls {
        let arity = td.generics.as_ref().map_or(0, |g| g.len());
        program.type_registry.register_user_type(&*td.name, arity);
    }

    compute_use_counts(&mut program); // After auto-derive so derived functions get correct use_counts
    demote_unused_mut(&mut program);

    // Verify no inference TypeVars remain in IR (ICE if any found)
    verify_no_inference_typevars(&program);

    program
}

/// Verify no inference TypeVars (?N) remain in the IR.
/// Any remaining TypeVar indicates a type checker bug — the codegen cannot
/// reliably generate correct code without concrete types.
fn verify_no_inference_typevars(program: &IrProgram) {
    use crate::types::Ty;
    fn has_typevar(ty: &Ty) -> bool {
        match ty {
            Ty::TypeVar(name) => name.starts_with('?'), // Only inference vars, not generic params
            Ty::Unknown => false,
            Ty::Applied(_, args) => args.iter().any(has_typevar),
            Ty::Tuple(elems) => elems.iter().any(has_typevar),
            Ty::Fn { params, ret } => params.iter().any(has_typevar) || has_typevar(ret),
            Ty::Named(_, args) => args.iter().any(has_typevar),
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().any(|(_, t)| has_typevar(t)),
            _ => false,
        }
    }
    let verbose = std::env::var("ALMIDE_DEBUG_TYPEVARS").is_ok();
    fn count_expr(expr: &IrExpr, fn_name: &str, loc: &str, verbose: bool) -> usize {
        let mut c = 0;
        if has_typevar(&expr.ty) {
            c += 1;
            if verbose { eprintln!("  [TypeVar] {} in {}: {:?}", loc, fn_name, expr.ty); }
        }
        match &expr.kind {
            IrExprKind::Call { args, .. } => { for a in args { c += count_expr(a, fn_name, "call-arg", verbose); } }
            IrExprKind::Lambda { body, params, .. } => {
                for (_, ty) in params { if has_typevar(ty) { c += 1; if verbose { eprintln!("  [TypeVar] lambda-param in {}: {:?}", fn_name, ty); } } }
                c += count_expr(body, fn_name, "lambda-body", verbose);
            }
            IrExprKind::BinOp { left, right, .. } => { c += count_expr(left, fn_name, "binop-left", verbose); c += count_expr(right, fn_name, "binop-right", verbose); }
            IrExprKind::Match { subject, arms, .. } => {
                c += count_expr(subject, fn_name, "match-subject", verbose);
                for arm in arms { c += count_expr(&arm.body, fn_name, "match-arm", verbose); }
            }
            IrExprKind::If { cond, then, else_, .. } => {
                c += count_expr(cond, fn_name, "if-cond", verbose);
                c += count_expr(then, fn_name, "if-then", verbose);
                c += count_expr(else_, fn_name, "if-else", verbose);
            }
            IrExprKind::Block { stmts, expr, .. } => {
                for s in stmts {
                    if let IrStmtKind::Bind { ty, value, .. } = &s.kind {
                        if has_typevar(ty) { c += 1; if verbose { eprintln!("  [TypeVar] bind-ty in {}: {:?}", fn_name, ty); } }
                        c += count_expr(value, fn_name, "bind-val", verbose);
                    } else if let IrStmtKind::Expr { expr: e } = &s.kind {
                        c += count_expr(e, fn_name, "stmt-expr", verbose);
                    }
                }
                if let Some(e) = expr { c += count_expr(e, fn_name, "block-tail", verbose); }
            }
            _ => {}
        }
        c
    }
    let mut count = 0;
    for func in &program.functions {
        count += count_expr(&func.body, &func.name, "body", verbose);
        for p in &func.params { if has_typevar(&p.ty) { count += 1; if verbose { eprintln!("  [TypeVar] param in {}: {:?}", func.name, p.ty); } } }
        if has_typevar(&func.ret_ty) { count += 1; if verbose { eprintln!("  [TypeVar] ret_ty in {}: {:?}", func.name, func.ret_ty); } }
    }
    for i in 0..program.var_table.len() {
        let info = program.var_table.get(VarId(i as u32));
        if has_typevar(&info.ty) {
            count += 1;
            if verbose { eprintln!("  [TypeVar] var {} '{}': {:?}", i, info.name, info.ty); }
        }
    }
    if count > 0 {
        eprintln!("[ICE] {} unresolved inference TypeVar(s) in IR after lowering. This is a type checker bug.", count);
        eprintln!("[ICE] Codegen may produce incorrect code. Set ALMIDE_DEBUG_TYPEVARS=1 for details.");
    }
}

pub fn lower_module(
    name: &str,
    prog: &ast::Program,
    expr_types: &HashMap<crate::ast::ExprId, Ty>,
    env: &TypeEnv,
    versioned_name: Option<String>,
) -> IrModule {
    let ir_prog = lower_program(prog, expr_types, env);
    IrModule {
        name: sym(name),
        versioned_name: versioned_name.map(|v| sym(&v)),
        type_decls: ir_prog.type_decls,
        functions: ir_prog.functions,
        top_lets: ir_prog.top_lets,
        var_table: ir_prog.var_table,
    }
}

// ── Function lowering ───────────────────────────────────────────

fn lower_fn(
    ctx: &mut LowerCtx,
    name: &str, params: &[ast::Param], body: &ast::Expr,
    effect: &Option<bool>, r#async: &Option<bool>, span: &Option<ast::Span>,
    generics: &Option<Vec<ast::GenericParam>>, extern_attrs: &[ast::ExternAttr],
    visibility: &ast::Visibility, _module_prefix: Option<&str>,
) -> IrFunction {
    ctx.push_scope();

    // Set up protocol bounds for this function's generics
    let saved_pb = std::mem::take(&mut ctx.protocol_bounds);
    if let Some(gs) = generics {
        for g in gs {
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() {
                    ctx.protocol_bounds.insert(g.name, bounds.clone());
                }
            }
        }
    }

    let mut ir_params = Vec::new();
    for p in params {
        let ty = resolve_type_expr(&p.ty);
        let var = ctx.define_var(&p.name, ty.clone(), Mutability::Let, span.clone());
        let default = p.default.as_ref().map(|d| Box::new(lower_expr(ctx, d)));
        ir_params.push(IrParam {
            var, ty: ty.clone(), name: p.name,
            borrow: ParamBorrow::Own, open_record: None, default,
        });
    }

    let ret_ty = if let Some(sig) = ctx.env.functions.get(&sym(name)) {
        sig.ret.clone()
    } else {
        ctx.expr_ty(body)
    };

    let ir_body = lower_expr(ctx, body);
    ctx.protocol_bounds = saved_pb;
    ctx.pop_scope();

    let is_effect = effect.unwrap_or(false);
    let is_async = r#async.unwrap_or(false);
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };

    IrFunction {
        name: sym(name), params: ir_params, ret_ty, body: ir_body,
        is_effect, is_async, is_test: false,
        generics: generics.clone(), extern_attrs: extern_attrs.to_vec(), visibility: vis,
    }
}

fn lower_test(ctx: &mut LowerCtx, name: &str, body: &ast::Expr) -> IrFunction {
    ctx.push_scope();
    let ir_body = lower_expr(ctx, body);
    ctx.pop_scope();
    IrFunction {
        name: sym(name), params: vec![], ret_ty: Ty::Unit, body: ir_body,
        is_effect: true, is_async: false, is_test: true,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
    }
}
