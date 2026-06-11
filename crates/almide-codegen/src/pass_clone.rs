//! ClonePass: insert Clone IR nodes for heap-type variables in Rust.
//!
//! **Last-use optimization**: tracks remaining uses per variable.
//! At the final use of a variable, ownership is transferred (move) instead of cloning.
//! Inside loops, clones are always inserted (the body executes multiple times).
//! At branches (if/match), remaining counts are merged conservatively (min).

use std::collections::{HashSet, HashMap};
use almide_ir::*;
use almide_base::Span;
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
            func.body = insert_clones_live(std::mem::take(&mut func.body), &always, &eligible, &mut remaining, false);
        }
        for tl in &mut program.top_lets {
            reset_remaining(&mut remaining, &eligible, &syntactic);
            tl.value = insert_clones_live(std::mem::take(&mut tl.value), &always, &eligible, &mut remaining, false);
        }

        let IrProgram { modules, var_table, .. } = &mut program;
        for module in modules.iter_mut() {
            let module_top_lets: HashSet<VarId> = module.top_lets.iter().map(|tl| tl.var).collect();
            let module_syntactic = compute_syntactic_counts_module(module);
            let (m_always, m_eligible) = split_clone_ids(var_table, &module_top_lets, &module_syntactic, &always_marks);
            let mut m_remaining = build_remaining(&m_eligible, &module_syntactic);

            for func in module.functions.iter_mut() {
                reset_remaining(&mut m_remaining, &m_eligible, &module_syntactic);
                func.body = insert_clones_live(std::mem::take(&mut func.body), &m_always, &m_eligible, &mut m_remaining, false);
            }
            for tl in module.top_lets.iter_mut() {
                reset_remaining(&mut m_remaining, &m_eligible, &module_syntactic);
                tl.value = insert_clones_live(std::mem::take(&mut tl.value), &m_always, &m_eligible, &mut m_remaining, false);
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

fn make_clone(id: VarId, ty: Ty, span: Option<Span>) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Clone {
            expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None }),
        },
        ty, span, def_id: None,
    }
}

fn insert_clones_live(
    expr: IrExpr,
    always: &HashSet<VarId>,
    eligible: &HashSet<VarId>,
    remaining: &mut HashMap<VarId, u32>,
    in_loop: bool,
) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        // ── Var: the core decision point ───────────────────────────
        IrExprKind::Var { id } if always.contains(&id) => {
            return make_clone(id, ty, span);
        }
        IrExprKind::Var { id } if eligible.contains(&id) => {
            if let Some(r) = remaining.get_mut(&id) {
                *r = r.saturating_sub(1);
                if *r == 0 && !in_loop {
                    // Last use outside a loop → move (no clone)
                    return IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None };
                }
            }
            return make_clone(id, ty, span);
        }

        // ── Block: sequential statements ───────────────────────────
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: insert_clone_stmts_live(stmts, always, eligible, remaining, in_loop),
            expr: expr.map(|e| Box::new(insert_clones_live(*e, always, eligible, remaining, in_loop))),
        },

        // ── If: save/restore/min for branches ──────────────────────
        IrExprKind::If { cond, then, else_ } => {
            let new_cond = insert_clones_live(*cond, always, eligible, remaining, in_loop);
            let saved = remaining.clone();
            let new_then = insert_clones_live(*then, always, eligible, remaining, in_loop);
            let then_remaining = std::mem::replace(remaining, saved);
            let new_else = insert_clones_live(*else_, always, eligible, remaining, in_loop);
            // Merge: take min (conservative — the branch that consumed more wins)
            for &id in eligible.iter() {
                let t = then_remaining.get(&id).copied().unwrap_or(0);
                let e = remaining.get(&id).copied().unwrap_or(0);
                remaining.insert(id, t.min(e));
            }
            IrExprKind::If {
                cond: Box::new(new_cond),
                then: Box::new(new_then),
                else_: Box::new(new_else),
            }
        }

        // ── Match: same strategy as If but N arms ──────────────────
        IrExprKind::Match { subject, arms } => {
            let new_subject = insert_clones_live(*subject, always, eligible, remaining, in_loop);
            let saved = remaining.clone();
            let mut min_remaining = HashMap::new();
            let mut new_arms = Vec::with_capacity(arms.len());

            for (i, arm) in arms.into_iter().enumerate() {
                *remaining = saved.clone();
                let new_guard = arm.guard.map(|g| insert_clones_live(g, always, eligible, remaining, in_loop));
                let new_body = insert_clones_live(arm.body, always, eligible, remaining, in_loop);
                new_arms.push(IrMatchArm { pattern: arm.pattern, guard: new_guard, body: new_body });

                if i == 0 {
                    min_remaining = remaining.clone();
                } else {
                    for &id in eligible.iter() {
                        let cur = remaining.get(&id).copied().unwrap_or(0);
                        let prev = min_remaining.get(&id).copied().unwrap_or(0);
                        min_remaining.insert(id, cur.min(prev));
                    }
                }
            }
            *remaining = min_remaining;
            IrExprKind::Match { subject: Box::new(new_subject), arms: new_arms }
        }

        // ── ForIn: iterable is NOT in loop, body IS ────────────────
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            let new_iterable = insert_clones_live(*iterable, always, eligible, remaining, in_loop);
            let new_body = insert_clone_stmts_live(body, always, eligible, remaining, true);
            IrExprKind::ForIn { var, var_tuple, iterable: Box::new(new_iterable), body: new_body }
        }

        // ── While: cond and body are in loop ───────────────────────
        IrExprKind::While { cond, body } => {
            let new_cond = insert_clones_live(*cond, always, eligible, remaining, true);
            let new_body = insert_clone_stmts_live(body, always, eligible, remaining, true);
            IrExprKind::While { cond: Box::new(new_cond), body: new_body }
        }

        // ── Lambda: body recurses normally ─────────────────────────
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(insert_clones_live(*body, always, eligible, remaining, in_loop)), lambda_id,
        },

        // ── Call ───────────────────────────────────────────────────
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| insert_clones_live(a, always, eligible, remaining, in_loop)).collect();
            let target = match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(insert_clones_live(*object, always, eligible, remaining, in_loop)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(insert_clones_live(*callee, always, eligible, remaining, in_loop)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            let args = args.into_iter().map(|a| insert_clones_live(a, always, eligible, remaining, in_loop)).collect();
            IrExprKind::RuntimeCall { symbol, args }
        }

        // ── IndexAccess: borrow container, clone element ───────────
        IrExprKind::IndexAccess { object, index } => {
            let mut processed_object = insert_clones_live(*object, always, eligible, remaining, in_loop);
            // Strip top-level Clone from container (indexing borrows)
            if let IrExprKind::Clone { expr } = processed_object.kind {
                processed_object = *expr;
            }
            let processed_index = insert_clones_live(*index, always, eligible, remaining, in_loop);
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
            return access;
        }

        // ── MapAccess: borrow container, clone element ─────────────
        IrExprKind::MapAccess { object, key } => {
            let mut processed_object = insert_clones_live(*object, always, eligible, remaining, in_loop);
            if let IrExprKind::Clone { expr } = processed_object.kind {
                processed_object = *expr;
            }
            let processed_key = insert_clones_live(*key, always, eligible, remaining, in_loop);
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
            return access;
        }

        // ── Simple recursion cases ─────────────────────────────────
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(insert_clones_live(*left, always, eligible, remaining, in_loop)),
            right: Box::new(insert_clones_live(*right, always, eligible, remaining, in_loop)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(insert_clones_live(*operand, always, eligible, remaining, in_loop)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| insert_clones_live(e, always, eligible, remaining, in_loop)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, always, eligible, remaining, in_loop))).collect(),
        },
        // Member access mirrors IndexAccess/MapAccess: the container is
        // borrowed (Record may be a `&T` after BorrowInference), and a
        // heap-typed field can't be moved out through the reference.
        // Wrap the access in Clone when the field itself needs cloning.
        IrExprKind::Member { object, field } => {
            let mut processed_object = insert_clones_live(*object, always, eligible, remaining, in_loop);
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
            return access;
        }
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)), field,
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: insert_clones_live(expr, always, eligible, remaining, in_loop) },
                other => other,
            }).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
            fallback: Box::new(insert_clones_live(*fallback, always, eligible, remaining, in_loop)),
        },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)) },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| insert_clones_live(e, always, eligible, remaining, in_loop)).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => {
            // Fields are evaluated before the spread base in Rust struct literals
            let new_fields: Vec<_> = fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, always, eligible, remaining, in_loop))).collect();
            let new_base = insert_clones_live(*base, always, eligible, remaining, in_loop);
            IrExprKind::SpreadRecord { base: Box::new(new_base), fields: new_fields }
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(insert_clones_live(*start, always, eligible, remaining, in_loop)),
            end: Box::new(insert_clones_live(*end, always, eligible, remaining, in_loop)),
            inclusive,
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| insert_clones_live(e, always, eligible, remaining, in_loop)).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (insert_clones_live(k, always, eligible, remaining, in_loop), insert_clones_live(v, always, eligible, remaining, in_loop))).collect(),
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(insert_clones_live(*object, always, eligible, remaining, in_loop)), index,
        },
        IrExprKind::Borrow { expr, as_str, mutable } => {
            let mut inner = insert_clones_live(*expr, always, eligible, remaining, in_loop);
            // Strip clone inside borrow: &x.clone() → &x (borrow doesn't consume ownership)
            if let IrExprKind::Clone { expr: unwrapped } = inner.kind {
                inner = *unwrapped;
            }
            IrExprKind::Borrow { expr: Box::new(inner), as_str, mutable }
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
        },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
        },
        IrExprKind::Await { expr } => IrExprKind::Await {
            expr: Box::new(insert_clones_live(*expr, always, eligible, remaining, in_loop)),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| insert_clones_live(a, always, eligible, remaining, in_loop)).collect(),
        },
        // Default: recurse into every child through the exhaustive `map_children`
        // chokepoint, so no un-listed node kind (`IterChain`/`RcWrap`/`TailCall`/
        // future variants) silently drops its subtree — that was the DIV2-sibling
        // (clone insertion blind to closures fused inside a chain). Leaf kinds have
        // no children and pass through unchanged.
        other => {
            let e = IrExpr { kind: other, ty: ty.clone(), span, def_id: None };
            return e.map_children(&mut |child| insert_clones_live(child, always, eligible, remaining, in_loop));
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

fn insert_clone_stmts_live(
    stmts: Vec<IrStmt>,
    always: &HashSet<VarId>,
    eligible: &HashSet<VarId>,
    remaining: &mut HashMap<VarId, u32>,
    in_loop: bool,
) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_clones_live(value, always, eligible, remaining, in_loop) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_clones_live(expr, always, eligible, remaining, in_loop) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_clones_live(cond, always, eligible, remaining, in_loop), else_: insert_clones_live(else_, always, eligible, remaining, in_loop),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: insert_clones_live(value, always, eligible, remaining, in_loop),
            },
            // In-place mutations: process the sub-exprs first (they may consume
            // vars), THEN account the target as a use of `target` itself —
            // `count_target_use` decrements `remaining[target]` to match the +1
            // that `SyntacticCounter::visit_stmt` added, keeping last-use tracking
            // consistent for any later use of `target`. The target binding is NOT
            // cloned/moved (the statement writes through it in place); this is a
            // pure counter decrement.
            IrStmtKind::IndexAssign { target, index, value } => {
                let index = insert_clones_live(index, always, eligible, remaining, in_loop);
                let value = insert_clones_live(value, always, eligible, remaining, in_loop);
                count_target_use(target, eligible, remaining);
                IrStmtKind::IndexAssign { target, index, value }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let value = insert_clones_live(value, always, eligible, remaining, in_loop);
                count_target_use(target, eligible, remaining);
                IrStmtKind::FieldAssign { target, field, value }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                let key = insert_clones_live(key, always, eligible, remaining, in_loop);
                let value = insert_clones_live(value, always, eligible, remaining, in_loop);
                count_target_use(target, eligible, remaining);
                IrStmtKind::MapInsert { target, key, value }
            }
            // Default: recurse every expr child via the exhaustive `map_exprs`
            // chokepoint so no un-listed stmt kind (`ListSwap`/`ListReverse`/… —
            // which `count_syntactic` already counts) drops its expr subtree.
            other => IrStmt { kind: other, span: s.span }
                .map_exprs(&mut |e| insert_clones_live(e, always, eligible, remaining, in_loop))
                .kind,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
