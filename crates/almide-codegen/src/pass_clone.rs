//! ClonePass: insert Clone IR nodes for heap-type variables in Rust.
//!
//! **Last-use optimization**: tracks remaining uses per variable.
//! At the final use of a variable, ownership is transferred (move) instead of cloning.
//! Inside loops, clones are always inserted (the body executes multiple times).
//! At branches (if/match), remaining counts are merged conservatively (min).

use std::collections::{HashSet, HashMap};
use almide_ir::*;
use almide_base::{Span, Sym};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct CloneInsertionPass;

impl NanoPass for CloneInsertionPass {
    fn name(&self) -> &str { "CloneInsertion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        compute_use_counts(&mut program);
        let top_let_vars: HashSet<VarId> = program.top_lets.iter().map(|tl| tl.var).collect();

        // Compute syntactic counts (no loop/lambda bumps) for remaining tracking
        let syntactic = compute_syntactic_counts_program(&program);

        let always_marks = program.codegen_annotations.always_clone_vars.clone();
        let (always, eligible) = split_clone_ids(&program.var_table, &top_let_vars, &syntactic, &always_marks);
        let mut remaining = build_remaining(&eligible, &syntactic);

        for func in &mut program.functions {
            // Reset remaining for each function (vars are function-scoped)
            reset_remaining(&mut remaining, &eligible, &syntactic);
            func.body = insert_clones_live(std::mem::take(&mut func.body), &mut CloneCtx { always: &always, eligible: &eligible, remaining: &mut remaining, in_loop: false });
        }
        for tl in &mut program.top_lets {
            reset_remaining(&mut remaining, &eligible, &syntactic);
            tl.value = insert_clones_live(std::mem::take(&mut tl.value), &mut CloneCtx { always: &always, eligible: &eligible, remaining: &mut remaining, in_loop: false });
        }

        let IrProgram { modules, var_table, .. } = &mut program;
        for module in modules.iter_mut() {
            let module_top_lets: HashSet<VarId> = module.top_lets.iter().map(|tl| tl.var).collect();
            let module_syntactic = compute_syntactic_counts_module(module);
            let (m_always, m_eligible) = split_clone_ids(var_table, &module_top_lets, &module_syntactic, &always_marks);
            let mut m_remaining = build_remaining(&m_eligible, &module_syntactic);

            for func in module.functions.iter_mut() {
                reset_remaining(&mut m_remaining, &m_eligible, &module_syntactic);
                func.body = insert_clones_live(std::mem::take(&mut func.body), &mut CloneCtx { always: &m_always, eligible: &m_eligible, remaining: &mut m_remaining, in_loop: false });
            }
            for tl in module.top_lets.iter_mut() {
                reset_remaining(&mut m_remaining, &m_eligible, &module_syntactic);
                tl.value = insert_clones_live(std::mem::take(&mut tl.value), &mut CloneCtx { always: &m_always, eligible: &m_eligible, remaining: &mut m_remaining, in_loop: false });
            }
        }
        PassResult { program, changed: true }
    }
}

// ── Syntactic use-count (no loop/lambda bumps) ─────────────────────

fn compute_syntactic_counts_program(program: &IrProgram) -> HashMap<VarId, u32> {
    let mut counts = HashMap::new();
    for func in &program.functions {
        count_syntactic(&func.body, &mut counts);
    }
    for tl in &program.top_lets {
        count_syntactic(&tl.value, &mut counts);
    }
    counts
}

fn compute_syntactic_counts_module(module: &IrModule) -> HashMap<VarId, u32> {
    let mut counts = HashMap::new();
    for func in &module.functions {
        count_syntactic(&func.body, &mut counts);
    }
    for tl in &module.top_lets {
        count_syntactic(&tl.value, &mut counts);
    }
    counts
}

/// Counts every syntactic `Var` use by riding the exhaustive `IrVisitor` walk —
/// so no node kind (incl. `IterChain`/`RcWrap`/`TailCall`, present here because
/// StreamFusion/TCO run before this pass) can silently drop a subtree and
/// under-count a var, which would desync the `remaining` last-use tracking.
struct SyntacticCounter<'a> {
    counts: &'a mut HashMap<VarId, u32>,
}

impl IrVisitor for SyntacticCounter<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Var { id } = &expr.kind {
            *self.counts.entry(*id).or_insert(0) += 1;
        }
        walk_expr(self, expr); // exhaustive recursion into all children
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        // An in-place mutation `a[i]=v` / `a.f=v` / `a[k]=v` reads-and-writes `a`,
        // but the target is a bare `VarId` field — NOT a `Var` expr node — so the
        // expr walk above never sees it. Count it explicitly: this makes the
        // mutation a *use* of `a`, so when an alias `var b = a` precedes it, the
        // bind is no longer `a`'s last use → the eligible-move path clones at the
        // bind instead of moving, and the later in-place write operates on owned
        // `a` (not a moved value → no E0382). Without this, shapes B/C/I above
        // emit `let mut b = a;`/`a.clone(); f(a);` then mutate the moved `a`.
        match &stmt.kind {
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => {
                *self.counts.entry(*target).or_insert(0) += 1;
            }
            _ => {}
        }
        walk_stmt(self, stmt); // exhaustive recursion into the stmt's expr children
    }
}

fn count_syntactic(expr: &IrExpr, counts: &mut HashMap<VarId, u32>) {
    SyntacticCounter { counts }.visit_expr(expr);
}

// ── Clone ID classification ────────────────────────────────────────

fn needs_clone(ty: &Ty) -> bool {
    // §4 stage 2c (#531): derived from THE copy-ness classifier — see the
    // projection table in almide_ir::top_let_storage.
    !almide_ir::top_let_storage::clone_free(ty)
}

/// Split clone candidates into "always clone" and "eligible for last-use move".
fn split_clone_ids(
    vt: &VarTable,
    top_let_vars: &HashSet<VarId>,
    syntactic: &HashMap<VarId, u32>,
    always_clone_marks: &HashSet<VarId>,
) -> (HashSet<VarId>, HashSet<VarId>) {
    let mut always = HashSet::new();
    let mut eligible = HashSet::new();

    for i in 0..vt.len() {
        let id = VarId(i as u32);
        let info = vt.get(id);
        if !needs_clone(&info.ty) { continue; }

        let name = almide_base::intern::resolve(info.name);
        if top_let_vars.contains(&id) || matches!(&info.ty, Ty::Fn { .. } | Ty::TypeVar(_))
            || always_clone_marks.contains(&id)
            || info.module_origin.is_some()
        {
            // `module_origin` marks a module top-let Var (decl side set in
            // lower/mod.rs, use side in lower/expressions.rs) whose Rust
            // storage is a `static LazyLock<T>` rendered by the walker as
            // `(*ALMIDE_RT_<MOD>_<NAME>)`. A static can never be moved
            // from, so every consuming use must clone — the same `always`
            // class as same-file `top_let_vars`. The use site allocates a
            // fresh VarId with a clean name, so neither the set lookup nor
            // any name prefix can catch it.
            always.insert(id);
        } else {
            let syn = syntactic.get(&id).copied().unwrap_or(0);
            if syn > 1 {
                // Multiple syntactic uses → eligible for last-use optimization
                eligible.insert(id);
            } else if info.use_count > 1 {
                // Single syntactic use but bumped (loop/lambda) → always clone
                always.insert(id);
            }
            // syn <= 1 && use_count <= 1: single use, no loop → move by default
        }
    }
    (always, eligible)
}

fn build_remaining(eligible: &HashSet<VarId>, syntactic: &HashMap<VarId, u32>) -> HashMap<VarId, u32> {
    eligible.iter().map(|&id| (id, syntactic.get(&id).copied().unwrap_or(0))).collect()
}

fn reset_remaining(remaining: &mut HashMap<VarId, u32>, eligible: &HashSet<VarId>, syntactic: &HashMap<VarId, u32>) {
    for &id in eligible {
        remaining.insert(id, syntactic.get(&id).copied().unwrap_or(0));
    }
}

// ── Clone insertion with last-use tracking ─────────────────────────

/// Bundles the four values threaded unchanged through every recursive call
/// in `insert_clones_live` / `insert_clone_stmts_live` (and their arm
/// helpers), so each fn stays at or under the `max-params` limit. `in_loop`
/// flips to `true` for a nested loop body/cond — built as a fresh `CloneCtx`
/// reborrowing `remaining` (same shape as `HoistCtx` in pass_licm_p2.rs).
struct CloneCtx<'a> {
    always: &'a HashSet<VarId>,
    eligible: &'a HashSet<VarId>,
    remaining: &'a mut HashMap<VarId, u32>,
    in_loop: bool,
}

fn make_clone(id: VarId, ty: Ty, span: Option<Span>) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Clone {
            expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None }),
        },
        ty, span, def_id: None,
    }
}

/// `Var { id }` arm of [`insert_clones_live`] — the core decision point for
/// clone-vs-move on a variable reference. Merges the two former match-guard
/// arms (`always`/`eligible`); an id tracked by neither falls through
/// unchanged, same as the exhaustive `other` catch-all's no-op on a
/// childless node.
fn insert_clones_var(id: VarId, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    if ctx.always.contains(&id) {
        return make_clone(id, ty, span);
    }
    if ctx.eligible.contains(&id) {
        if let Some(r) = ctx.remaining.get_mut(&id) {
            *r = r.saturating_sub(1);
            if *r == 0 && !ctx.in_loop {
                // Last use outside a loop → move (no clone)
                return IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None };
            }
        }
        return make_clone(id, ty, span);
    }
    IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None }
}

/// `If { cond, then, else_ }` arm of [`insert_clones_live`]: save/restore/min
/// for branches (the branch that consumed more `remaining` wins — conservative).
fn insert_clones_if(cond: IrExpr, then: IrExpr, else_: IrExpr, ctx: &mut CloneCtx) -> IrExprKind {
    let new_cond = insert_clones_live(cond, ctx);
    let saved = ctx.remaining.clone();
    let new_then = insert_clones_live(then, ctx);
    let then_remaining = std::mem::replace(ctx.remaining, saved);
    let new_else = insert_clones_live(else_, ctx);
    for &id in ctx.eligible.iter() {
        let t = then_remaining.get(&id).copied().unwrap_or(0);
        let e = ctx.remaining.get(&id).copied().unwrap_or(0);
        ctx.remaining.insert(id, t.min(e));
    }
    IrExprKind::If {
        cond: Box::new(new_cond),
        then: Box::new(new_then),
        else_: Box::new(new_else),
    }
}

/// `Match { subject, arms }` arm of [`insert_clones_live`]: same save/min
/// strategy as [`insert_clones_if`], generalized to N arms.
fn insert_clones_match(subject: IrExpr, arms: Vec<IrMatchArm>, ctx: &mut CloneCtx) -> IrExprKind {
    let new_subject = insert_clones_live(subject, ctx);
    let saved = ctx.remaining.clone();
    let mut min_remaining = HashMap::new();
    let mut new_arms = Vec::with_capacity(arms.len());

    for (i, arm) in arms.into_iter().enumerate() {
        *ctx.remaining = saved.clone();
        let new_guard = arm.guard.map(|g| insert_clones_live(g, ctx));
        let new_body = insert_clones_live(arm.body, ctx);
        new_arms.push(IrMatchArm { pattern: arm.pattern, guard: new_guard, body: new_body });

        if i == 0 {
            min_remaining = ctx.remaining.clone();
        } else {
            for &id in ctx.eligible.iter() {
                let cur = ctx.remaining.get(&id).copied().unwrap_or(0);
                let prev = min_remaining.get(&id).copied().unwrap_or(0);
                min_remaining.insert(id, cur.min(prev));
            }
        }
    }
    *ctx.remaining = min_remaining;
    IrExprKind::Match { subject: Box::new(new_subject), arms: new_arms }
}

/// `ForIn { var, var_tuple, iterable, body }` arm of [`insert_clones_live`]:
/// the iterable is NOT in the loop, the body IS.
fn insert_clones_for_in(var: VarId, var_tuple: Option<Vec<VarId>>, iterable: IrExpr, body: Vec<IrStmt>, ctx: &mut CloneCtx) -> IrExprKind {
    let new_iterable = insert_clones_live(iterable, ctx);
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true };
    let new_body = insert_clone_stmts_live(body, &mut loop_ctx);
    IrExprKind::ForIn { var, var_tuple, iterable: Box::new(new_iterable), body: new_body }
}

/// `While { cond, body }` arm of [`insert_clones_live`]: cond and body are
/// both in the loop.
fn insert_clones_while(cond: IrExpr, body: Vec<IrStmt>, ctx: &mut CloneCtx) -> IrExprKind {
    let mut loop_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: true };
    let new_cond = insert_clones_live(cond, &mut loop_ctx);
    let new_body = insert_clone_stmts_live(body, &mut loop_ctx);
    IrExprKind::While { cond: Box::new(new_cond), body: new_body }
}

/// `Call { target, args, type_args }` arm of [`insert_clones_live`].
fn insert_clones_call(target: CallTarget, args: Vec<IrExpr>, type_args: Vec<Ty>, ctx: &mut CloneCtx) -> IrExprKind {
    let args = args.into_iter().map(|a| insert_clones_live(a, ctx)).collect();
    let target = match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(insert_clones_live(*object, ctx)), method,
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(insert_clones_live(*callee, ctx)),
        },
        other => other,
    };
    IrExprKind::Call { target, args, type_args }
}

/// `IndexAccess { object, index }` arm of [`insert_clones_live`]: borrow the
/// container, clone the element.
fn insert_clones_index_access(object: IrExpr, index: IrExpr, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    let mut processed_object = insert_clones_live(object, ctx);
    // Strip top-level Clone from container (indexing borrows)
    if let IrExprKind::Clone { expr } = processed_object.kind {
        processed_object = *expr;
    }
    let processed_index = insert_clones_live(index, ctx);
    let access = IrExpr {
        kind: IrExprKind::IndexAccess {
            object: Box::new(processed_object),
            index: Box::new(processed_index),
        },
        ty: ty.clone(), span, def_id: None,
    };
    if needs_clone(&ty) {
        return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span, def_id: None };
    }
    access
}

/// `MapAccess { object, key }` arm of [`insert_clones_live`]: borrow the
/// container, clone the element.
fn insert_clones_map_access(object: IrExpr, key: IrExpr, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    let mut processed_object = insert_clones_live(object, ctx);
    if let IrExprKind::Clone { expr } = processed_object.kind {
        processed_object = *expr;
    }
    let processed_key = insert_clones_live(key, ctx);
    let access = IrExpr {
        kind: IrExprKind::MapAccess {
            object: Box::new(processed_object),
            key: Box::new(processed_key),
        },
        ty: ty.clone(), span, def_id: None,
    };
    if needs_clone(&ty) {
        return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span, def_id: None };
    }
    access
}

/// `Member { object, field }` arm of [`insert_clones_live`]. Mirrors
/// IndexAccess/MapAccess: the container is borrowed (Record may be a `&T`
/// after BorrowInference), and a heap-typed field can't be moved out
/// through the reference. Wrap the access in Clone when the field itself
/// needs cloning.
fn insert_clones_member(object: IrExpr, field: Sym, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    let mut processed_object = insert_clones_live(object, ctx);
    if let IrExprKind::Clone { expr } = processed_object.kind {
        processed_object = *expr;
    }
    let access = IrExpr {
        kind: IrExprKind::Member {
            object: Box::new(processed_object),
            field,
        },
        ty: ty.clone(), span, def_id: None,
    };
    if needs_clone(&ty) {
        return IrExpr { kind: IrExprKind::Clone { expr: Box::new(access) }, ty, span, def_id: None };
    }
    access
}

fn insert_clones_live(expr: IrExpr, ctx: &mut CloneCtx) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Var { id } => return insert_clones_var(id, ty, span, ctx),

        // ── Block: sequential statements ───────────────────────────
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: insert_clone_stmts_live(stmts, ctx),
            expr: expr.map(|e| Box::new(insert_clones_live(*e, ctx))),
        },

        IrExprKind::If { cond, then, else_ } => insert_clones_if(*cond, *then, *else_, ctx),
        IrExprKind::Match { subject, arms } => insert_clones_match(*subject, arms, ctx),
        IrExprKind::ForIn { var, var_tuple, iterable, body } => insert_clones_for_in(var, var_tuple, *iterable, body, ctx),
        IrExprKind::While { cond, body } => insert_clones_while(*cond, body, ctx),

        // ── Lambda: body recurses normally ─────────────────────────
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(insert_clones_live(*body, ctx)), lambda_id,
        },

        IrExprKind::Call { target, args, type_args } => insert_clones_call(target, args, type_args, ctx),
        IrExprKind::RuntimeCall { symbol, args } => {
            let args = args.into_iter().map(|a| insert_clones_live(a, ctx)).collect();
            IrExprKind::RuntimeCall { symbol, args }
        }

        IrExprKind::IndexAccess { object, index } => return insert_clones_index_access(*object, *index, ty, span, ctx),
        IrExprKind::MapAccess { object, key } => return insert_clones_map_access(*object, *key, ty, span, ctx),

        // ── Simple recursion cases ─────────────────────────────────
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(insert_clones_live(*left, ctx)),
            right: Box::new(insert_clones_live(*right, ctx)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(insert_clones_live(*operand, ctx)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_clones_live(e, ctx)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, ctx))).collect(),
        },
        IrExprKind::Member { object, field } => return insert_clones_member(*object, field, ty, span, ctx),
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(insert_clones_live(*expr, ctx)), field,
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_clones_live(expr, ctx) },
                other => other,
            }).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_clones_live(*expr, ctx)),
            fallback: Box::new(insert_clones_live(*fallback, ctx)),
        },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(insert_clones_live(*expr, ctx)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_clones_live(e, ctx)).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => {
            // Fields are evaluated before the spread base in Rust struct literals
            let new_fields: Vec<_> = fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, ctx))).collect();
            let new_base = insert_clones_live(*base, ctx);
            IrExprKind::SpreadRecord { base: Box::new(new_base), fields: new_fields }
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(insert_clones_live(*start, ctx)),
            end: Box::new(insert_clones_live(*end, ctx)),
            inclusive,
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_clones_live(e, ctx)).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (insert_clones_live(k, ctx), insert_clones_live(v, ctx))).collect(),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(insert_clones_live(*object, ctx)), index,
        },
        IrExprKind::Borrow { expr, as_str, mutable } => {
            let mut inner = insert_clones_live(*expr, ctx);
            // Strip clone inside borrow: &x.clone() → &x (borrow doesn't consume ownership)
            if let IrExprKind::Clone { expr: unwrapped } = inner.kind {
                inner = *unwrapped;
            }
            IrExprKind::Borrow { expr: Box::new(inner), as_str, mutable }
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew {
            expr: Box::new(insert_clones_live(*expr, ctx)),
        },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec {
            expr: Box::new(insert_clones_live(*expr, ctx)),
        },
        IrExprKind::Await { expr } => IrExprKind::Await {
            expr: Box::new(insert_clones_live(*expr, ctx)),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| insert_clones_live(a, ctx)).collect(),
        },
        // Default: recurse into every child through the exhaustive `map_children`
        // chokepoint, so no un-listed node kind (`IterChain`/`RcWrap`/`TailCall`/
        // future variants) silently drops its subtree — that was the DIV2-sibling
        // (clone insertion blind to closures fused inside a chain). Leaf kinds have
        // no children and pass through unchanged.
        other => {
            let e = IrExpr { kind: other, ty: ty.clone(), span, def_id: None };
            return e.map_children(&mut |child| insert_clones_live(child, ctx));
        }
    };

    IrExpr { kind, ty, span, def_id: None }
}

/// Account an in-place-mutation `target` as a use, mirroring the +1 that
/// `SyntacticCounter::visit_stmt` recorded. Only `eligible` (last-use-move) vars
/// track `remaining`; `always`/move-by-default vars don't appear there.
fn count_target_use(target: VarId, eligible: &HashSet<VarId>, remaining: &mut HashMap<VarId, u32>) {
    if eligible.contains(&target) {
        if let Some(r) = remaining.get_mut(&target) {
            *r = r.saturating_sub(1);
        }
    }
}

fn insert_clone_stmts_live(stmts: Vec<IrStmt>, ctx: &mut CloneCtx) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_clones_live(value, ctx),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_clones_live(value, ctx) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_clones_live(expr, ctx) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_clones_live(cond, ctx), else_: insert_clones_live(else_, ctx),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: insert_clones_live(value, ctx),
            },
            // In-place mutations: process the sub-exprs first (they may consume
            // vars), THEN account the target as a use of `target` itself —
            // `count_target_use` decrements `remaining[target]` to match the +1
            // that `SyntacticCounter::visit_stmt` added, keeping last-use tracking
            // consistent for any later use of `target`. The target binding is NOT
            // cloned/moved (the statement writes through it in place); this is a
            // pure counter decrement.
            IrStmtKind::IndexAssign { target, index, value } => {
                let index = insert_clones_live(index, ctx);
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::IndexAssign { target, index, value }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::FieldAssign { target, field, value }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                let key = insert_clones_live(key, ctx);
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::MapInsert { target, key, value }
            }
            // Default: recurse every expr child via the exhaustive `map_exprs`
            // chokepoint so no un-listed stmt kind (`ListSwap`/`ListReverse`/… —
            // which `count_syntactic` already counts) drops its expr subtree.
            other => IrStmt { kind: other, span: s.span }
                .map_exprs(&mut |e| insert_clones_live(e, ctx))
                .kind,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
