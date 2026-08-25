
/// Collect stdlib module names referenced by CallTarget::Module in the IR.
/// Scans all functions and modules (including transitive deps).
fn collect_stdlib_modules(program: &IrProgram) -> std::collections::HashSet<String> {
    let mut used = std::collections::HashSet::new();

    // Router: dispatches to a group helper by expr kind. Each helper handles
    // an independent subset of `IrExprKind` and returns whether it matched —
    // `used` is a write-only accumulator (no arm ever reads back what an
    // earlier arm wrote), so grouping is behavior-preserving.
    fn scan_expr(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
        if scan_expr_calls(expr, used) { return; }
        if scan_expr_control(expr, used) { return; }
        scan_expr_containers(expr, used);
    }

    // Call-like nodes: the only arms that can add a module name to `used`.
    fn scan_expr_calls(expr: &IrExpr, used: &mut std::collections::HashSet<String>) -> bool {
        match &expr.kind {
            IrExprKind::Call { target, args, .. } => {
                if let CallTarget::Module { module, .. } = target {
                    used.insert(module.to_string());
                }
                if let CallTarget::Method { object, .. } = target {
                    scan_expr(object, used);
                }
                for a in args { scan_expr(a, used); }
                true
            }
            IrExprKind::RuntimeCall { symbol, args } => {
                // Extract module from runtime symbol: almide_rt_{module}_{fn}
                if let Some(rest) = symbol.as_str().strip_prefix("almide_rt_") {
                    if let Some(pos) = rest.find('_') {
                        used.insert(rest[..pos].to_string());
                    }
                }
                for a in args { scan_expr(a, used); }
                true
            }
            _ => false,
        }
    }

    // Control-flow nodes: recurse into sub-blocks/statements/arms.
    fn scan_expr_control(expr: &IrExpr, used: &mut std::collections::HashSet<String>) -> bool {
        if scan_expr_block_like(expr, used) { return true; }
        scan_expr_loop_like(expr, used)
    }

    // Block/If/Match: nodes that carry statement lists or arms.
    fn scan_expr_block_like(expr: &IrExpr, used: &mut std::collections::HashSet<String>) -> bool {
        match &expr.kind {
            IrExprKind::Block { stmts, expr: tail } => {
                for s in stmts { scan_stmt(s, used); }
                if let Some(e) = tail { scan_expr(e, used); }
                true
            }
            IrExprKind::If { cond, then, else_ } => {
                scan_expr(cond, used); scan_expr(then, used); scan_expr(else_, used);
                true
            }
            IrExprKind::Match { subject, arms } => {
                scan_expr(subject, used);
                for arm in arms {
                    if let Some(g) = &arm.guard { scan_expr(g, used); }
                    scan_expr(&arm.body, used);
                }
                true
            }
            _ => false,
        }
    }

    // Lambda/ForIn/While: nodes with a body and (for loops) an iterable/cond.
    fn scan_expr_loop_like(expr: &IrExpr, used: &mut std::collections::HashSet<String>) -> bool {
        match &expr.kind {
            IrExprKind::Lambda { body, .. } => { scan_expr(body, used); true }
            IrExprKind::ForIn { iterable, body, .. } => {
                scan_expr(iterable, used);
                for s in body { scan_stmt(s, used); }
                true
            }
            IrExprKind::While { cond, body } => {
                scan_expr(cond, used);
                for s in body { scan_stmt(s, used); }
                true
            }
            _ => false,
        }
    }

    // Plain container/wrapper nodes: straight recursive descent, no module
    // names to record here.
    fn scan_expr_containers(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
        if scan_expr_wrappers(expr, used) { return; }
        scan_expr_collections(expr, used);
    }

    // Single/dual-child wrapper nodes: unwrap-like variants, UnOp, IndexAccess,
    // Range, UnwrapOr — each recurses directly into its 1-2 sub-expressions.
    fn scan_expr_wrappers(expr: &IrExpr, used: &mut std::collections::HashSet<String>) -> bool {
        match &expr.kind {
            IrExprKind::UnOp { operand, .. } => { scan_expr(operand, used); true }
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
            | IrExprKind::OptionSome { expr: e } | IrExprKind::Unwrap { expr: e }
            | IrExprKind::Try { expr: e } | IrExprKind::ToOption { expr: e }
            | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
            | IrExprKind::Member { object: e, .. } => { scan_expr(e, used); true }
            IrExprKind::UnwrapOr { expr: e, fallback } => { scan_expr(e, used); scan_expr(fallback, used); true }
            IrExprKind::IndexAccess { object, index } => { scan_expr(object, used); scan_expr(index, used); true }
            IrExprKind::Range { start, end, .. } => { scan_expr(start, used); scan_expr(end, used); true }
            _ => false,
        }
    }

    // Collection-literal nodes: recurse over a list/map of sub-expressions.
    fn scan_expr_collections(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
        if scan_expr_seq_literals(expr, used) { return; }
        scan_expr_keyed_literals(expr, used);
    }

    // BinOp/List/Tuple/Fan/Record: sequence-shaped literals and BinOp.
    fn scan_expr_seq_literals(expr: &IrExpr, used: &mut std::collections::HashSet<String>) -> bool {
        match &expr.kind {
            IrExprKind::BinOp { left, right, .. } => { scan_expr(left, used); scan_expr(right, used); true }
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
                for e in elements { scan_expr(e, used); }
                true
            }
            IrExprKind::Record { fields, .. } => { for (_, v) in fields { scan_expr(v, used); } true }
            _ => false,
        }
    }

    // StringInterp/SpreadRecord/MapLiteral: keyed/mixed literal forms.
    fn scan_expr_keyed_literals(expr: &IrExpr, used: &mut std::collections::HashSet<String>) {
        match &expr.kind {
            IrExprKind::StringInterp { parts } => {
                for p in parts { if let IrStringPart::Expr { expr } = p { scan_expr(expr, used); } }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                scan_expr(base, used);
                for (_, v) in fields { scan_expr(v, used); }
            }
            IrExprKind::MapLiteral { entries } => {
                for (k, v) in entries { scan_expr(k, used); scan_expr(v, used); }
            }
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

/// Replace every inference TypeVar (?N) still in the IR with `Ty::Unknown`.
/// A surviving TypeVar indicates a type checker bug — codegen cannot reliably
/// generate correct code without concrete types, so it is erased here rather
/// than carried into the backends.
fn resolve_inference_typevars(program: &mut IrProgram) {
    use crate::types::Ty;
    fn resolve_ty(ty: &mut Ty) {
        match ty {
            Ty::TypeVar(name) if name.starts_with('?') => *ty = Ty::Unknown,
            Ty::Applied(_, args) => { for a in args { resolve_ty(a); } }
            Ty::Tuple(elems) => { for e in elems { resolve_ty(e); } }
            Ty::Fn { is_effect: _, params, ret } => { for p in params { resolve_ty(p); } resolve_ty(ret); }
            Ty::Named(_, args) => { for a in args { resolve_ty(a); } }
            Ty::Record { fields } | Ty::OpenRecord { fields } => { for (_, t) in fields { resolve_ty(t); } }
            _ => {}
        }
    }
    // Router: dispatches to a group helper by expr kind after resolving the
    // node's own `.ty`. Each helper handles an independent subset of
    // `IrExprKind` and returns whether it matched — every arm only WRITES
    // through its `&mut` sub-expressions (no arm reads back what an earlier
    // arm wrote), so grouping is behavior-preserving.
    fn resolve_expr(expr: &mut IrExpr) {
        resolve_ty(&mut expr.ty);
        if resolve_expr_calls(expr) { return; }
        if resolve_expr_control(expr) { return; }
        resolve_expr_containers(expr);
    }

    // Call/Lambda/BinOp: call-like and operator nodes.
    fn resolve_expr_calls(expr: &mut IrExpr) -> bool {
        match &mut expr.kind {
            IrExprKind::Call { args, .. } => { for a in args { resolve_expr(a); } true }
            IrExprKind::Lambda { body, params, .. } => {
                for (_, ty) in params { resolve_ty(ty); }
                resolve_expr(body);
                true
            }
            IrExprKind::BinOp { left, right, .. } => { resolve_expr(left); resolve_expr(right); true }
            _ => false,
        }
    }

    // Match/If/Block/ForIn/While: nodes that carry statement lists or arms.
    fn resolve_expr_control(expr: &mut IrExpr) -> bool {
        match &mut expr.kind {
            IrExprKind::Match { subject, arms, .. } => {
                resolve_expr(subject);
                for arm in arms { resolve_expr(&mut arm.body); }
                true
            }
            IrExprKind::If { cond, then, else_, .. } => {
                resolve_expr(cond); resolve_expr(then); resolve_expr(else_);
                true
            }
            IrExprKind::Block { stmts, expr, .. } => {
                for s in stmts { resolve_stmt(s); }
                if let Some(e) = expr { resolve_expr(e); }
                true
            }
            IrExprKind::ForIn { iterable, body, .. } => {
                resolve_expr(iterable);
                for s in body { resolve_stmt(s); }
                true
            }
            IrExprKind::While { cond, body } => {
                resolve_expr(cond);
                for s in body { resolve_stmt(s); }
                true
            }
            _ => false,
        }
    }

    // Plain container/wrapper nodes: straight recursive descent.
    fn resolve_expr_containers(expr: &mut IrExpr) {
        if resolve_expr_wrappers(expr) { return; }
        resolve_expr_collections(expr);
    }

    // Single-child unwrap-like variants and Member/OptionalChain.
    fn resolve_expr_wrappers(expr: &mut IrExpr) -> bool {
        match &mut expr.kind {
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
            | IrExprKind::OptionSome { expr: e } | IrExprKind::Unwrap { expr: e }
            | IrExprKind::Try { expr: e } | IrExprKind::ToOption { expr: e }
            | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
            | IrExprKind::ToVec { expr: e } | IrExprKind::UnOp { operand: e, .. }
            | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e } => {
                resolve_expr(e);
                true
            }
            IrExprKind::Member { object, .. } | IrExprKind::OptionalChain { expr: object, .. } => { resolve_expr(object); true }
            _ => false,
        }
    }

    // List/Tuple/Fan/UnwrapOr/Record/IndexAccess/Range/StringInterp:
    // multi-child collection-literal and dual-child nodes.
    fn resolve_expr_collections(expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
                for e in elements { resolve_expr(e); }
            }
            IrExprKind::UnwrapOr { expr: e, fallback } => { resolve_expr(e); resolve_expr(fallback); }
            IrExprKind::Record { fields, .. } => { for (_, v) in fields { resolve_expr(v); } }
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

/// A borrowed view of the `fn` declaration being lowered.
///
/// The fields used to be thirteen positional parameters, three adjacent pairs of
/// which shared a type: `effect` and `r#async` are both `&Option<bool>`, and
/// `extern_attrs` / `export_attrs` / `attrs` are all attribute slices. Swapping
/// any pair type-checks, so the positional form made a silent
/// `effect`/`async` transposition a live possibility at every call site. Named
/// fields make the same mistake a compile error.
pub(super) struct FnToLower<'a> {
    pub name: &'a str,
    pub params: &'a [ast::Param],
    pub body: &'a ast::Expr,
    pub effect: &'a Option<bool>,
    pub span: &'a Option<ast::Span>,
    pub generics: &'a Option<Vec<ast::GenericParam>>,
    pub extern_attrs: &'a [ast::ExternAttr],
    pub export_attrs: &'a [ast::ExportAttr],
    pub attrs: &'a [ast::Attribute],
    pub visibility: &'a ast::Visibility,
    pub module_prefix: Option<&'a str>,
}

fn lower_fn(ctx: &mut LowerCtx, decl: &FnToLower<'_>) -> IrFunction {
    let FnToLower {
        name, params, body, effect, span, generics,
        extern_attrs, export_attrs, attrs, visibility, module_prefix,
    } = *decl;
    ctx.push_scope();

    // Set up protocol bounds and const params for this function's generics
    let saved_pb = std::mem::take(&mut ctx.protocol_bounds);
    let saved_cp = std::mem::take(&mut ctx.const_param_vars);
    register_generic_protocol_bounds(ctx, generics);

    let mut ir_params = lower_const_param_ir_params(ctx, generics, span);
    // #1577: the fn's own generic letters shadow same-named TYPE bindings
    // while its signature resolves. Without the overlay, `fn go[Q](c: Q)`
    // with a visible type named `Q` (an imported module's — post-#1574 the
    // bare alias survives registration) resolved the param to the NOMINAL
    // type; mono then saw no type-var in any param, never specialized the
    // fn, and the call site dangled on both targets. Const-param letters
    // are VALUE names, not type vars, so they stay out of the overlay.
    let env_ref = ctx.env;
    let generic_overlay = generic_typevar_overlay(env_ref, generics);
    let sig_types = generic_overlay.as_ref().unwrap_or(&env_ref.types);
    ir_params.extend(lower_fn_value_params(ctx, name, params, module_prefix, span, sig_types));

    let ret_ty = resolve_fn_ret_ty(ctx, name, module_prefix, body);

    let ir_body = lower_expr(ctx, body);
    ctx.protocol_bounds = saved_pb;
    ctx.const_param_vars = saved_cp;
    ctx.pop_scope();

    let is_effect = effect.unwrap_or(false);
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };

    let stripped_generics = strip_const_param_generics(generics);
    let mutated_params = resolve_mutated_params(params, attrs);

    IrFunction {
        name: sym(name), params: ir_params, ret_ty, body: ir_body,
        is_effect, is_test: false,
        generics: stripped_generics, extern_attrs: extern_attrs.to_vec(),
        export_attrs: export_attrs.to_vec(),
        attrs: attrs.to_vec(),
        visibility: vis,
        doc: None, blank_lines_before: 0,
        def_id: ctx.def_map.get(&sym(name)).copied(),
        mutated_params, module_origin: None,
    }
}

/// The signature-resolution type map for a GENERIC fn: `env.types` copied
/// with each non-const generic letter mapped to `Ty::TypeVar` — the letter
/// SHADOWS a same-named visible type for the declaration, exactly as the
/// checker's enter_generics scopes it (#1577; the lowering twin of #1574's
/// registration rule). `None` for a fn with no type-var generics — the
/// caller then resolves against `env.types` directly, clone-free.
fn generic_typevar_overlay(env: &TypeEnv, generics: &Option<Vec<ast::GenericParam>>) -> Option<HashMap<Sym, Ty>> {
    let gs = generics.as_ref()?;
    let letters: Vec<Sym> = gs
        .iter()
        .filter(|g| {
            !g.bounds.as_ref().is_some_and(|bs| {
                bs.len() == 1
                    && crate::canonicalize::registration::SCALAR_TYPE_NAMES.contains(&bs[0].as_str())
            })
        })
        .map(|g| sym(&g.name))
        .collect();
    if letters.is_empty() {
        return None;
    }
    let mut overlay = env.types.clone();
    for l in letters {
        overlay.insert(l, Ty::TypeVar(l));
    }
    Some(overlay)
}

// Set up protocol bounds for this function's generics (non-const-param
// bounds only — const params are handled separately below).
fn register_generic_protocol_bounds(ctx: &mut LowerCtx, generics: &Option<Vec<ast::GenericParam>>) {
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
}

// Const generic params (a single scalar-type bound, e.g. `[N: Int]`) become
// implicit leading IrParams instead of type parameters.
fn lower_const_param_ir_params(ctx: &mut LowerCtx, generics: &Option<Vec<ast::GenericParam>>, span: &Option<ast::Span>) -> Vec<IrParam> {
    let mut ir_params = Vec::new();
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
                        borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None,
                        attrs: Vec::new(),
                    });
                }
            }
        }
    }
    ir_params
}

// A bare `self` first param is sugar for `self: Self` (see registration.rs
// and check/mod.rs's matching fixes). `Self` only stays an unresolved
// placeholder inside a `protocol { ... }` declaration; on a real
// convention method it must lower to the enclosing type, or codegen
// emits the literal (nonexistent) Rust type `Self`.
/// `sig_types` is the type map signature resolution runs against — the plain
/// `env.types` for a non-generic fn, or [`generic_typevar_overlay`]'s copy
/// with the fn's own generic letters mapped to `TypeVar` (#1577).
fn lower_fn_value_params(ctx: &mut LowerCtx, name: &str, params: &[ast::Param], module_prefix: Option<&str>, span: &Option<ast::Span>, sig_types: &HashMap<Sym, Ty>) -> Vec<IrParam> {
    let receiver_ty = name.split_once('.').map(|(ty_name, _)| Ty::Named(sym(ty_name), Vec::new()));
    let mut ir_params = Vec::new();
    for (i, p) in params.iter().enumerate() {
        let ty = if i == 0 && p.name.as_str() == "self"
            && matches!(&p.ty, ast::TypeExpr::Simple { name: tn } if tn.as_str() == "Self")
        {
            receiver_ty.clone().unwrap_or_else(||
                crate::canonicalize::resolve::resolve_type_expr_in(&p.ty, Some(sig_types), module_prefix))
        } else {
            crate::canonicalize::resolve::resolve_type_expr_in(&p.ty, Some(sig_types), module_prefix)
        };
        let var = ctx.define_var(&p.name, ty.clone(), Mutability::Let, span.clone());
        let default = p.default.as_ref().map(|d| Box::new(lower_expr(ctx, d)));
        ir_params.push(IrParam {
            var, ty: ty.clone(), name: p.name,
            borrow: ParamBorrow::Own, is_mut: p.is_mut, open_record: None, default,
            attrs: p.attrs.clone(),
        });
    }
    ir_params
}

// For module functions, look up the module-prefixed name first (e.g.,
// "option.unwrap_or") to avoid picking up a user function with the same
// bare name.
fn resolve_fn_ret_ty(ctx: &LowerCtx, name: &str, module_prefix: Option<&str>, body: &ast::Expr) -> Ty {
    let prefixed = module_prefix.map(|p| format!("{}.{}", p, name));
    let sig = prefixed.as_ref()
        .and_then(|pn| ctx.env.functions.get(&sym(pn)))
        .or_else(|| ctx.env.functions.get(&sym(name)));
    if let Some(sig) = sig {
        sig.ret.clone()
    } else {
        ctx.expr_ty(body)
    }
}

// Strip const params from generics (they became runtime params above).
// If only const params remain, generics becomes None (non-generic function).
fn strip_const_param_generics(generics: &Option<Vec<ast::GenericParam>>) -> Option<Vec<ast::GenericParam>> {
    generics.as_ref().map(|gs| {
        let remaining: Vec<_> = gs.iter().filter(|g| {
            !g.bounds.as_ref().map_or(false, |bs| {
                bs.len() == 1 && crate::canonicalize::registration::SCALAR_TYPE_NAMES.contains(&bs[0].as_str())
            })
        }).cloned().collect();
        if remaining.is_empty() { None } else { Some(remaining) }
    }).flatten()
}

// Resolve mut params: from `mut` keyword and @mutating(param_name) annotation
fn resolve_mutated_params(params: &[ast::Param], attrs: &[ast::Attribute]) -> Vec<usize> {
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
    mutated_params
}
