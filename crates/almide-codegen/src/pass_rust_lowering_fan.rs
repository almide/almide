//! `fan` parallel routing — the RustLowering step that gives `fan` its native
//! parallelism (#2044). Not a pass of its own: the owner's ruling (2026-09-08)
//! retired the implicit `AutoParallelPass` (which threaded pure
//! `list.map/filter/any/all` anywhere, and was dead — it matched a
//! `Call { Named }` StdlibLowering no longer emits). Implicit `|>` chains are
//! FUSED sequentially (StreamFusion); parallelism exists only through the
//! explicit `fan.map` / `fan { … }`, and this module is where those two
//! spellings are routed to the thread-per-core runtime twins.
//!
//! Runs first inside RustLoweringPass — before its closure boxing, so the
//! boxing step sees the routed symbol: `fan.*` args stay raw, and the
//! `almide_rt_list_par_*` twins' `F: Fn` last arg is un-boxed by the
//! generated `takes_raw_fn_last_arg` registry. Two shapes are rewritten,
//! both only where the source already asked for a fan-out — nothing outside
//! a `fan` construct is ever parallelized:
//!
//! * `fan.map(xs, (x) => …)` — still a `Call { Module { fan, map } }` at this
//!   point (StdlibLowering leaves `fan.*` to the walker, which renders it as
//!   `almide_rt_fan_map`) — becomes `Module { fan, map_par }`, rendered as
//!   `almide_rt_fan_map_par`.
//! * `list.map/filter/any/all` UNDER a `fan { … }` block — the
//!   `RuntimeCall { almide_rt_list_map }` StdlibLowering emits (a legacy
//!   `Call { Named }` spelling is matched too) — becomes the
//!   `almide_rt_list_par_*` twin.
//!
//! A call qualifies only when the callback is a pure lambda LITERAL (no effect
//! fn calls, no mutable captures — an effect callback keeps its side effects in
//! list order on the sequential runtime) AND every type that would cross a
//! thread is Send-safe: the element type, the result type, and the type of each
//! captured variable (from `var_table`). Send-safe = Int/Float/Bool/Unit, the
//! sized numerics, and tuples / records / Option of those; anything with an
//! `Rc` in its native repr (List/Map/Set/Bytes/String — String is excluded
//! ahead of ADR-0013's refcounted repr — closure values) falls back to the
//! sequential twin. The parallel runtime joins by index and reports the first
//! Err in list order, so the output is byte-identical to the sequential one.
//!
//! Rust target only. Uses `std::thread::scope` in the runtime — no external crates.

use almide_ir::*;
use almide_base::intern::{Sym, sym};
use almide_lang::types::Ty;
use almide_lang::types::constructor::TypeConstructorId as TC;

/// Effect-fn-names collection phase of `route_fan_parallel`, extracted
/// verbatim (cog>30 decomposition, sequential-phase pattern — no state
/// shared with the mutable-vars phase below).
fn collect_effect_fn_names(program: &IrProgram) -> std::collections::HashSet<Sym> {
    let mut effect_fns: std::collections::HashSet<Sym> = std::collections::HashSet::new();
    for func in &program.functions {
        if func.is_effect {
            effect_fns.insert(func.name);
        }
    }
    for module in &program.modules {
        let mod_ident = module.versioned_name
            .map(|v| v.to_string())
            .unwrap_or_else(|| module.name.to_string())
            .replace('.', "_");
        for func in &module.functions {
            if func.is_effect {
                // Module-QUALIFIED plus the mangled runtime symbol
                // StdlibLowering renames call targets to — never the bare
                // name, which could only ever collide with a root fn or
                // another module's fn (#1597's wrong-source family). The
                // Named-call purity check sees the MANGLED spelling by
                // this point in the pipeline, so the old bare insert was
                // checking a name no call site carries.
                effect_fns.insert(sym(&format!("{}.{}", module.name, func.name)));
                effect_fns.insert(sym(&format!(
                    "almide_rt_{}_{}",
                    mod_ident,
                    func.name.as_str().replace('.', "_")
                )));
            }
        }
    }
    effect_fns
}

/// Mutable-var-IDs collection phase of `route_fan_parallel`, extracted
/// verbatim (cog>30 decomposition).
fn collect_mutable_var_ids(program: &IrProgram) -> std::collections::HashSet<VarId> {
    let mut mutable_vars = std::collections::HashSet::new();
    for i in 0..program.var_table.len() {
        let id = VarId(i as u32);
        if program.var_table.get(id).mutability == Mutability::Var {
            mutable_vars.insert(id);
        }
    }
    mutable_vars
}

/// Route every qualifying `fan.map` / under-fan list op in `program` to its
/// parallel twin. Rust target only (the caller, RustLoweringPass, is). Returns
/// whether any call was rewritten.
pub fn route_fan_parallel(program: &mut IrProgram) -> bool {
    let purity = super::pass_stream_fusion::Purity::for_fan(program);
    // Collect effect function names for purity analysis
    let effect_fns = collect_effect_fn_names(program);
    // Collect mutable variable IDs from var_table
    let mutable_vars = collect_mutable_var_ids(program);
    // Record decls from the root AND every module: a module's `type P = {…}`
    // is spelled `<module>.P` in `Ty::Named`, and a captured record of
    // scalars is Send-safe wherever it was declared.
    let type_decls: Vec<IrTypeDecl> = program.type_decls.iter()
        .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()))
        .cloned()
        .collect();
    let IrProgram { functions, top_lets, modules, var_table, .. } = program;
    let changed = std::cell::Cell::new(false);
    let ctx = ParCtx { purity: &purity, effect_fns: &effect_fns, mutable_vars: &mutable_vars, var_table, type_decls: &type_decls, changed: &changed };

    // `mem::take` (the pass_clone idiom) — `rewrite_expr` takes the body
    // by value, so cloning it first cost a whole-AST copy per function.
    for func in functions.iter_mut() {
        func.body = rewrite_expr(std::mem::take(&mut func.body), &ctx, false);
    }
    for tl in top_lets.iter_mut() {
        tl.value = rewrite_expr(std::mem::take(&mut tl.value), &ctx, false);
    }
    // Post-unification every VarId lives in `program.var_table`;
    // `mutable_vars` already covers module-local bindings too.
    for module in modules.iter_mut() {
        for func in &mut module.functions {
            func.body = rewrite_expr(std::mem::take(&mut func.body), &ctx, false);
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_expr(std::mem::take(&mut tl.value), &ctx, false);
        }
    }
    changed.get()
}

/// Everything the rewrite consults, threaded by reference through the walk.
struct ParCtx<'a> {
    purity: &'a super::pass_stream_fusion::Purity,
    effect_fns: &'a std::collections::HashSet<Sym>,
    mutable_vars: &'a std::collections::HashSet<VarId>,
    /// Captured vars are typed from here (post-UnifyVarTables: one table).
    var_table: &'a VarTable,
    /// Root + module type decls, for resolving a `Ty::Named` record.
    type_decls: &'a [IrTypeDecl],
    /// Set when a call is routed to its parallel twin.
    changed: &'a std::cell::Cell<bool>,
}

/// Map sequential runtime names to their parallel counterparts.
fn parallel_name(name: &str) -> Option<&'static str> {
    match name {
        "almide_rt_list_map" => Some("almide_rt_list_par_map"),
        "almide_rt_list_filter" => Some("almide_rt_list_par_filter"),
        "almide_rt_list_any" => Some("almide_rt_list_par_any"),
        "almide_rt_list_all" => Some("almide_rt_list_par_all"),
        _ => None,
    }
}

/// Send-safe = a type whose NATIVE repr carries no `Rc`: it can cross a
/// `thread::scope` boundary by value. Scalars, and tuples / records / Option
/// of Send-safe types. `Ty::Named` resolves through the program's record and
/// alias decls (a generic instantiation, a variant, or an unknown name is
/// declined). String is declined on purpose: today it is a plain `String`,
/// but ADR-0013 stage 2 makes it refcounted, and the fan runtime must not
/// change meaning under it. Depth-bounded so a recursive alias cannot loop.
fn is_send_safe_ty(ty: &Ty, decls: &[IrTypeDecl], depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    let safe = |t: &Ty| is_send_safe_ty(t, decls, depth + 1);
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::Unit
        | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        | Ty::Float32 | Ty::Float64 => true,
        Ty::Tuple(elems) => elems.iter().all(safe),
        Ty::Record { fields } | Ty::OpenRecord { fields } => fields.iter().all(|(_, t)| safe(t)),
        Ty::Applied(TC::Option, args) | Ty::Applied(TC::Tuple, args) => args.iter().all(safe),
        Ty::Named(name, args) if args.is_empty() => {
            decls.iter().find(|d| d.name == *name).is_some_and(|d| match &d.kind {
                IrTypeDeclKind::Record { fields } => fields.iter().all(|f| safe(&f.ty)),
                IrTypeDeclKind::Alias { target } => safe(target),
                IrTypeDeclKind::Variant { .. } => false,
            })
        }
        _ => false,
    }
}

/// The `B` of a `fan.map` callback's `Result[B, String]` return type.
fn result_ok_ty(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(TC::Result, args) => args.first(),
        _ => None,
    }
}

/// Check if a lambda body is pure: no effect fn calls, no mutable variable captures.
fn is_pure_lambda(
    body: &IrExpr,
    params: &[(VarId, Ty)],
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    let param_ids: std::collections::HashSet<VarId> = params.iter().map(|(id, _)| *id).collect();
    is_pure_expr(body, &param_ids, effect_fns, mutable_vars)
}

/// `IrExprKind::Call` / `IrExprKind::TailCall` case of `is_pure_expr`,
/// extracted verbatim (cog>30 decomposition, pattern 2 — every arm
/// independently returns a `bool`, no state shared between arms).
fn is_pure_call(
    target: &CallTarget,
    args: &[IrExpr],
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    match target {
        CallTarget::Named { name } => {
            if effect_fns.contains(name) { return false; }
            // Stdlib effect functions (fs, http, etc.)
            if let Some(rest) = name.strip_prefix("almide_rt_") {
                let module = rest.split('_').next().unwrap_or("");
                if matches!(module, "fs" | "http" | "env" | "process" | "time") {
                    return false;
                }
            }
        }
        CallTarget::Module { module, func, .. } => {
            if matches!(&**module, "fs" | "http" | "env" | "process" | "time") {
                return false;
            }
            // A USER module's effect fn is impure too — the old arm never
            // consulted effect_fns for Module targets, so only the five
            // stdlib modules were screened.
            if effect_fns.contains(&sym(&format!("{}.{}", module, func))) {
                return false;
            }
        }
        _ => {}
    }
    args.iter().all(|a| is_pure_expr(a, local_vars, effect_fns, mutable_vars))
}

/// `IrExprKind::RuntimeCall` case of `is_pure_expr`, extracted verbatim
/// (cog>30 decomposition).
fn is_pure_runtime_call(
    symbol: &Sym,
    args: &[IrExpr],
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    let name = symbol.as_str();
    if let Some(rest) = name.strip_prefix("almide_rt_") {
        let module = rest.split('_').next().unwrap_or("");
        if matches!(module, "fs" | "http" | "env" | "process" | "time") {
            return false;
        }
    }
    args.iter().all(|a| is_pure_expr(a, local_vars, effect_fns, mutable_vars))
}

/// `IrExprKind::Match` case of `is_pure_expr`, extracted verbatim (cog>30
/// decomposition).
fn is_pure_match(
    subject: &IrExpr,
    arms: &[IrMatchArm],
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    is_pure_expr(subject, local_vars, effect_fns, mutable_vars) &&
    arms.iter().all(|arm| {
        let mut arm_vars = local_vars.clone();
        collect_pattern_bindings(&arm.pattern, &mut arm_vars);
        arm.guard.as_ref().is_none_or(|g| is_pure_expr(g, &arm_vars, effect_fns, mutable_vars)) &&
        is_pure_expr(&arm.body, &arm_vars, effect_fns, mutable_vars)
    })
}

/// `IrExprKind::Block` case of `is_pure_expr`, extracted verbatim (cog>30
/// decomposition). The mid-loop `return false` only exits this helper (the
/// same "one arm, one value" shape as the original inlined arm), not the
/// caller — safe, matches the `check_needs_ownership`-style guard.
fn is_pure_block(
    stmts: &[IrStmt],
    expr: &Option<Box<IrExpr>>,
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    let mut block_vars = local_vars.clone();
    for stmt in stmts {
        if !is_pure_stmt(stmt, &block_vars, effect_fns, mutable_vars) {
            return false;
        }
        collect_stmt_bindings(stmt, &mut block_vars);
    }
    expr.as_ref().is_none_or(|e| is_pure_expr(e, &block_vars, effect_fns, mutable_vars))
}

/// Recursively check expression purity.
/// A pure expression:
/// - Contains no calls to effect functions
/// - Does not reference mutable variables outside its own lambda params
/// - Contains no Assign statements
fn is_pure_expr(
    expr: &IrExpr,
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    let pure = |e: &IrExpr| is_pure_expr(e, local_vars, effect_fns, mutable_vars);
    match &expr.kind {
        // Variable reference: a local is always fine; a CAPTURED variable is
        // impure only if it is mutable.
        IrExprKind::Var { id } => local_vars.contains(id) || !mutable_vars.contains(id),

        // ── Always pure, no children ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Hole | IrExprKind::OptionNone
        | IrExprKind::FnRef { .. } | IrExprKind::EmptyMap
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } => true,

        // ── Never pure ──
        //
        // Macro invocations and rendered calls are conservative; loops in a
        // lambda body could mutate; `fan` is concurrent by definition.
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::Fan { .. }
        | IrExprKind::Todo { .. } => false,

        // ── Calls: purity is decided by the target ──
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            is_pure_call(target, args, local_vars, effect_fns, mutable_vars)
        }
        // Resolved runtime call: purity follows the same effect-module rules as
        // Module calls. `almide_rt_fs_*` / `almide_rt_http_*` / etc. are effects
        // and block parallelization.
        IrExprKind::RuntimeCall { symbol, args } => {
            is_pure_runtime_call(symbol, args, local_vars, effect_fns, mutable_vars)
        }

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionalChain { expr: e, .. }
        | IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::RcWrap { expr: e, .. }
        | IrExprKind::ToVec { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e } => pure(e),

        // ── Two children ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::UnwrapOr { expr: a, fallback: b }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::Range { start: a, end: b, .. } => pure(a) && pure(b),

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => pure(cond) && pure(then) && pure(else_),

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            xs.iter().all(pure)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, e)| pure(e)),
        IrExprKind::SpreadRecord { base, fields } => {
            pure(base) && fields.iter().all(|(_, e)| pure(e))
        }

        // ── Shapes with their own traversal ──
        IrExprKind::Match { subject, arms } => {
            is_pure_match(subject, arms, local_vars, effect_fns, mutable_vars)
        }
        IrExprKind::Block { stmts, expr } => {
            is_pure_block(stmts, expr, local_vars, effect_fns, mutable_vars)
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| pure(k) && pure(v))
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Lit { .. } => true,
            IrStringPart::Expr { expr } => pure(expr),
        }),
        // Nested lambda: its params are locals for its own body.
        IrExprKind::Lambda { params, body, .. } => {
            let mut inner_vars = local_vars.clone();
            for (id, _) in params {
                inner_vars.insert(*id);
            }
            is_pure_expr(body, &inner_vars, effect_fns, mutable_vars)
        }
    }
}

/// Check statement purity.
fn is_pure_stmt(
    stmt: &IrStmt,
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => is_pure_expr(value, local_vars, effect_fns, mutable_vars),
        IrStmtKind::BindDestructure { value, .. } => is_pure_expr(value, local_vars, effect_fns, mutable_vars),
        // Assignment to a variable: impure (mutation)
        IrStmtKind::Assign { .. } | IrStmtKind::IndexAssign { .. } |
        IrStmtKind::MapInsert { .. } | IrStmtKind::FieldAssign { .. } |
        IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. } |
        IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => false,
        IrStmtKind::Expr { expr } => is_pure_expr(expr, local_vars, effect_fns, mutable_vars),
        IrStmtKind::Guard { cond, else_ } => {
            is_pure_expr(cond, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(else_, local_vars, effect_fns, mutable_vars)
        }
        IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => true,
        IrStmtKind::Comment { .. } => true,
    }
}

/// Collect variable bindings introduced by a pattern.
fn collect_pattern_bindings(pattern: &IrPattern, vars: &mut std::collections::HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { vars.insert(*var); }
        IrPattern::As { var, inner, .. } => {
            vars.insert(*var);
            collect_pattern_bindings(inner, vars);
        }
        IrPattern::Constructor { args, .. } => {
            for p in args { collect_pattern_bindings(p, vars); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_bindings(p, vars); }
            }
        }
        IrPattern::Tuple { elements } | IrPattern::List { elements, .. } => {
            for p in elements { collect_pattern_bindings(p, vars); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_bindings(inner, vars);
        }
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => {}
    }
}

/// Collect variable bindings from a statement (for block scope tracking).
fn collect_stmt_bindings(stmt: &IrStmt, vars: &mut std::collections::HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, .. } => { vars.insert(*var); }
        IrStmtKind::BindDestructure { pattern, .. } => {
            collect_pattern_bindings(pattern, vars);
        }
        _ => {}
    }
}

// ── IR rewriting ────────────────────────────────────────────────

/// `in_fan`: the node sits under a `fan { … }` block (not inside a lambda body
/// — a closure may run anywhere, and a `list.map` nested in a parallel
/// callback must not fan out again).
fn rewrite_expr(expr: IrExpr, ctx: &ParCtx, in_fan: bool) -> IrExpr {
    let IrExpr { kind, ty, span, .. } = expr;
    match kind {
        IrExprKind::Fan { exprs } => IrExpr {
            kind: IrExprKind::Fan { exprs: exprs.into_iter().map(|e| rewrite_expr(e, ctx, true)).collect() },
            ty, span, def_id: None,
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(rewrite_expr(*body, ctx, false)), lambda_id },
            ty, span, def_id: None,
        },
        // `fan.map(xs, f)`: the explicit fan-out; qualifies on its own.
        IrExprKind::Call { target: CallTarget::Module { module, func, def_id }, args, type_args }
            if module.as_str() == "fan" && func.as_str() == "map" =>
        {
            let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a, ctx, false)).collect();
            let elem_ok = args.get(1).and_then(|f| result_ok_ty(lambda_ret_ty(f)?));
            let func = if fan_qualifies(args.get(1), elem_ok, ctx) { ctx.changed.set(true); sym("map_par") } else { func };
            IrExpr {
                kind: IrExprKind::Call { target: CallTarget::Module { module, func, def_id }, args, type_args },
                ty, span, def_id: None,
            }
        }
        // `list.map/filter/any/all` under a fan block, in either spelling.
        IrExprKind::RuntimeCall { symbol, args } if in_fan && parallel_name(&symbol).is_some() => {
            let (symbol, args) = rewrite_list_op(symbol, args, ctx);
            IrExpr { kind: IrExprKind::RuntimeCall { symbol, args }, ty, span, def_id: None }
        }
        IrExprKind::Call { target: CallTarget::Named { name }, args, type_args }
            if in_fan && parallel_name(&name).is_some() =>
        {
            let (name, args) = rewrite_list_op(name, args, ctx);
            IrExpr { kind: IrExprKind::Call { target: CallTarget::Named { name }, args, type_args }, ty, span, def_id: None }
        }
        // Every other node: rewrite each child and rebuild. `map_children` is
        // the single wildcard-free traversal primitive (it lists every
        // `IrExprKind`, so adding a variant is a compile error there), which is
        // why this pass needs no per-variant arms of its own — including
        // statement bodies, which `IrStmt::map_exprs` covers. `def_id` is
        // dropped, as it always was here.
        kind => IrExpr { kind, ty, span, def_id: None }
            .map_children(&mut |e| rewrite_expr(e, ctx, in_fan)),
    }
}

/// The `list.map(xs, f)`-shaped call under a fan block: rewrite the args, then
/// swap the callee for its parallel twin when the lambda qualifies. The result
/// element type is the lambda body's own type (`B` for map, Bool for the rest).
fn rewrite_list_op(name: Sym, args: Vec<IrExpr>, ctx: &ParCtx) -> (Sym, Vec<IrExpr>) {
    let par_name = parallel_name(&name).expect("caller checked parallel_name");
    let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_expr(a, ctx, false)).collect();
    let lambda = args.last();
    let ret = lambda.and_then(lambda_ret_ty);
    let name = if fan_qualifies(lambda, ret, ctx) { ctx.changed.set(true); sym(par_name) } else { name };
    (name, args)
}

/// The lambda literal in a callback slot, looking through the `Clone`
/// CloneInsertionPass may have wrapped it in.
fn lambda_literal(arg: &IrExpr) -> Option<(&[(VarId, Ty)], &IrExpr)> {
    let kind = match &arg.kind {
        IrExprKind::Clone { expr } => &expr.kind,
        other => other,
    };
    match kind {
        IrExprKind::Lambda { params, body, .. } => Some((params, body)),
        _ => None,
    }
}

/// The callback's return type — the lambda body's own type.
fn lambda_ret_ty(arg: &IrExpr) -> Option<&Ty> {
    lambda_literal(arg).map(|(_, body)| &body.ty)
}

/// The decision: a pure lambda literal whose element type (its parameter),
/// result element type, and every captured variable's type are Send-safe.
/// Anything else — a stored closure value, a named fn value, an effect
/// callback, an `Rc`-shaped type anywhere on the thread boundary — keeps the
/// sequential runtime.
fn fan_qualifies(arg: Option<&IrExpr>, result_elem: Option<&Ty>, ctx: &ParCtx) -> bool {
    let Some((params, body)) = arg.and_then(lambda_literal) else { return false };
    let Some(result_elem) = result_elem else { return false };
    // A plain `fn` may print or reach effects transitively. Its declaration
    // alone is not a proof that callback order is unobservable. Recompute
    // the shared analysis on this pass input, independently of fusion.
    if !ctx.purity.pure(body) || !is_pure_lambda(body, params, ctx.effect_fns, ctx.mutable_vars) {
        return false;
    }
    if !params.iter().all(|(_, t)| is_send_safe_ty(t, ctx.type_decls, 0)) {
        return false;
    }
    if !is_send_safe_ty(result_elem, ctx.type_decls, 0) {
        return false;
    }
    let param_ids: std::collections::HashSet<VarId> = params.iter().map(|(id, _)| *id).collect();
    free_vars::free_vars(body, &param_ids).iter().all(|id| {
        (id.0 as usize) < ctx.var_table.len()
            && is_send_safe_ty(&ctx.var_table.get(*id).ty, ctx.type_decls, 0)
    })
}
