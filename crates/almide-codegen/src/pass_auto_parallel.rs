//! AutoParallelPass: rewrite pure list operations to parallel variants.
//!
//! Runs AFTER StdlibLowering (which converts Module calls to Named calls).
//! Looks for Named calls to `almide_rt_list_{map,filter,any,all}` where
//! the lambda argument is pure (no effect fn calls, no mutable captures).
//! Rewrites the call target to `almide_rt_list_par_{map,filter,any,all}`.
//!
//! Rust target only. Uses `std::thread::scope` in the runtime — no external crates.

use almide_ir::*;
use almide_base::intern::{Sym, sym};
use super::pass::PassResult;
use almide_lang::types::Ty;
use super::pass::{NanoPass, Target};

/// Effect-fn-names collection phase of `AutoParallelPass::run`, extracted
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

/// Mutable-var-IDs collection phase of `AutoParallelPass::run`, extracted
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

#[derive(Debug)]
pub struct AutoParallelPass;

impl NanoPass for AutoParallelPass {
    fn name(&self) -> &str { "AutoParallel" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn depends_on(&self) -> Vec<&'static str> {
        vec!["StdlibLowering"]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Collect effect function names for purity analysis
        let effect_fns = collect_effect_fn_names(&program);
        // Collect mutable variable IDs from var_table
        let mutable_vars = collect_mutable_var_ids(&program);

        // `mem::take` (the pass_clone idiom) — `rewrite_expr` takes the body
        // by value, so cloning it first cost a whole-AST copy per function.
        for func in &mut program.functions {
            func.body = rewrite_expr(std::mem::take(&mut func.body), &effect_fns, &mutable_vars);
        }
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(std::mem::take(&mut tl.value), &effect_fns, &mutable_vars);
        }
        // Post-unification every VarId lives in `program.var_table`;
        // `mutable_vars` already covers module-local bindings too.
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = rewrite_expr(std::mem::take(&mut func.body), &effect_fns, &mutable_vars);
            }
            for tl in &mut module.top_lets {
                tl.value = rewrite_expr(std::mem::take(&mut tl.value), &effect_fns, &mutable_vars);
            }
        }
        PassResult { program, changed: true }
    }
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
            if name.starts_with("almide_rt_") {
                let rest = &name["almide_rt_".len()..];
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
        arm.guard.as_ref().map_or(true, |g| is_pure_expr(g, &arm_vars, effect_fns, mutable_vars)) &&
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
    expr.as_ref().map_or(true, |e| is_pure_expr(e, &block_vars, effect_fns, mutable_vars))
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

fn rewrite_expr(
    expr: IrExpr,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> IrExpr {
    // Target pattern: a Named call to a parallelizable list function.
    let is_target = matches!(
        &expr.kind,
        IrExprKind::Call { target: CallTarget::Named { name }, .. } if parallel_name(name).is_some()
    );
    if is_target {
        return rewrite_parallel_call(expr, effect_fns, mutable_vars);
    }
    // Every other node: rewrite each child and rebuild. `map_children` is the
    // single wildcard-free traversal primitive (it lists every `IrExprKind`, so
    // adding a variant is a compile error there), which is why this pass needs
    // no per-variant arms of its own — including statement bodies, which
    // `IrStmt::map_exprs` covers. `def_id` is dropped, as it always was here.
    let IrExpr { kind, ty, span, .. } = expr;
    IrExpr { kind, ty, span, def_id: None }
        .map_children(&mut |e| rewrite_expr(e, effect_fns, mutable_vars))
}

/// The `list.map(xs, f)`-shaped call this pass exists for: rewrite the args,
/// then swap the callee for its parallel twin when the lambda argument is pure.
fn rewrite_parallel_call(
    expr: IrExpr,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    let IrExprKind::Call { target: CallTarget::Named { name: orig_name }, args, type_args } = expr.kind
    else {
        unreachable!("rewrite_parallel_call on a non-parallelizable call")
    };
    let par_name = parallel_name(&orig_name).expect("caller checked parallel_name");

    // Recurse into args first
    let args: Vec<IrExpr> = args
        .into_iter()
        .map(|a| rewrite_expr(a, effect_fns, mutable_vars))
        .collect();

    // The lambda argument (last arg for map/filter/any/all) decides. If the
    // lambda is wrapped in Clone (from CloneInsertionPass), peek inside.
    let lambda = match args.last().map(|a| &a.kind) {
        Some(IrExprKind::Clone { expr }) => Some(&expr.kind),
        other => other,
    };
    let is_pure = matches!(
        lambda,
        Some(IrExprKind::Lambda { params, body, .. })
            if is_pure_lambda(body, params, effect_fns, mutable_vars)
    );
    let name = if is_pure { sym(par_name) } else { orig_name };
    IrExpr {
        kind: IrExprKind::Call { target: CallTarget::Named { name }, args, type_args },
        ty,
        span,
        def_id: None,
    }
}
