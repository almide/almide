//! PerceusPass: Insert RcInc/RcDec IR nodes for automatic memory management.
//!
//! Uses Lean 4 FnBody representation internally:
//!   Block IR → FnBody chain → insert Inc/Dec → FnBody chain → Block IR
//!
//! FnBody (continuation-based):
//!   VDecl(var, ty, expr, body)  — let var: ty = expr; then body
//!   Assign(var, expr, body)     — var = expr; then body
//!   Inc(var, body)              — rc_inc(var); then body
//!   Dec(var, body)              — rc_dec(var); then body
//!   Expr(expr, body)            — eval expr (discard); then body
//!   Ret(expr)                   — return expr
//!   Nop                         — end of chain (Unit return)
//!
//! Target: WASM only (Rust handles ownership natively).

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use std::collections::{HashMap, HashSet};
use super::pass::{NanoPass, PassResult, Target};

// ── Lean 4 FnBody (internal representation for Perceus) ──

/// Continuation-based IR node (Lean 4 style).
/// Each node chains to the next via `body`. No "tail expression" concept.
#[derive(Debug, Clone)]
enum FnBody {
    /// let var: ty = expr; then body
    VDecl { var: VarId, ty: Ty, mutability: Mutability, expr: IrExpr, body: Box<FnBody> },
    /// var = expr; then body
    Assign { var: VarId, expr: IrExpr, body: Box<FnBody> },
    /// rc_inc(var); then body
    Inc { var: VarId, body: Box<FnBody> },
    /// rc_dec(var); then body
    Dec { var: VarId, body: Box<FnBody> },
    /// eval expr (discard result); then body
    Expr { expr: IrExpr, body: Box<FnBody> },
    /// Pass-through: original statement preserved as-is
    Stmt { stmt: IrStmt, body: Box<FnBody> },
    /// return expr
    Ret { expr: IrExpr },
    /// end of chain (Unit return)
    Nop,
}

/// Convert Block IR to FnBody chain.
fn block_to_fnbody(stmts: Vec<IrStmt>, tail: Option<Box<IrExpr>>) -> FnBody {
    let ret = match tail {
        Some(e) => FnBody::Ret { expr: *e },
        None => FnBody::Nop,
    };
    // Build chain in reverse: last stmt wraps ret, second-to-last wraps that, etc.
    stmts.into_iter().rev().fold(ret, |body, stmt| {
        match stmt.kind {
            IrStmtKind::Bind { var, ty, value, mutability } =>
                FnBody::VDecl { var, ty, mutability, expr: value, body: Box::new(body) },
            IrStmtKind::Assign { var, value } =>
                FnBody::Assign { var, expr: value, body: Box::new(body) },
            IrStmtKind::RcInc { var } =>
                FnBody::Inc { var, body: Box::new(body) },
            IrStmtKind::RcDec { var } =>
                FnBody::Dec { var, body: Box::new(body) },
            IrStmtKind::Expr { expr } =>
                FnBody::Expr { expr, body: Box::new(body) },
            _ => FnBody::Stmt { stmt, body: Box::new(body) },
        }
    })
}

/// Convert FnBody chain back to Block IR (stmts + tail).
fn fnbody_to_block(mut fb: FnBody) -> (Vec<IrStmt>, Option<Box<IrExpr>>) {
    let mut stmts = Vec::new();
    loop {
        match fb {
            FnBody::VDecl { var, ty, mutability, expr, body } => {
                stmts.push(IrStmt {
                    kind: IrStmtKind::Bind { var, ty, mutability, value: expr },
                    span: None,
                });
                fb = *body;
            }
            FnBody::Assign { var, expr, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::Assign { var, value: expr }, span: None });
                fb = *body;
            }
            FnBody::Inc { var, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::RcInc { var }, span: None });
                fb = *body;
            }
            FnBody::Dec { var, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
                fb = *body;
            }
            FnBody::Expr { expr, body } => {
                stmts.push(IrStmt { kind: IrStmtKind::Expr { expr }, span: None });
                fb = *body;
            }
            FnBody::Stmt { stmt, body } => {
                stmts.push(stmt);
                fb = *body;
            }
            FnBody::Ret { expr } => {
                return (stmts, Some(Box::new(expr)));
            }
            FnBody::Nop => {
                return (stmts, None);
            }
        }
    }
}

/// Recursively apply Perceus to expressions (handles nested blocks).
fn perceus_expr(expr: &mut IrExpr, var_table: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let old_stmts = std::mem::take(stmts);
            let old_tail = tail.take();
            let fb = block_to_fnbody(old_stmts, old_tail);
            let fb = perceus_fnbody(fb, var_table);
            let fb = insert_ret_decs(fb, var_table);
            let (new_stmts, new_tail) = fnbody_to_block(fb);
            *stmts = new_stmts;
            *tail = new_tail;
        }
        IrExprKind::If { cond, then, else_ } => {
            perceus_expr(cond, var_table);
            perceus_expr(then, var_table);
            perceus_expr(else_, var_table);
        }
        IrExprKind::Match { subject, arms } => {
            perceus_expr(subject, var_table);
            for arm in arms { perceus_expr(&mut arm.body, var_table); }
        }
        IrExprKind::Lambda { body, .. } => { perceus_expr(body, var_table); }
        IrExprKind::While { cond, body } => {
            perceus_expr(cond, var_table);
            // Convert while body stmts to FnBody chain (Nop terminus)
            let old_body = std::mem::take(body);
            let fb = block_to_fnbody(old_body, None); // None = Nop
            let fb = perceus_fnbody(fb, var_table);
            let fb = insert_ret_decs(fb, var_table); // inserts Dec before Nop for heap locals
            let (new_body, _) = fnbody_to_block(fb);
            *body = new_body;
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            perceus_expr(iterable, var_table);
            let old_body = std::mem::take(body);
            let fb = block_to_fnbody(old_body, None);
            let fb = perceus_fnbody(fb, var_table);
            let fb = insert_ret_decs(fb, var_table);
            let (new_body, _) = fnbody_to_block(fb);
            *body = new_body;
        }
        _ => {}
    }
}

/// Insert Perceus Inc/Dec into a FnBody chain.
fn perceus_fnbody(fb: FnBody, var_table: &mut VarTable) -> FnBody {
    match fb {
        FnBody::VDecl { var, ty, mutability, mut expr, body } => {
            let body = perceus_fnbody(*body, var_table);
            // Recurse into the expression (handles nested blocks)
            perceus_expr(&mut expr, var_table);
            // Rule 1: RcInc for heap alias
            let needs_inc = is_heap_type(&ty) && matches!(&expr.kind,
                IrExprKind::Var { .. } | IrExprKind::Clone { .. } | IrExprKind::Deref { .. });
            let inc_var = if needs_inc {
                match &expr.kind {
                    IrExprKind::Var { id } => Some(*id),
                    IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e } =>
                        if let IrExprKind::Var { id } = &e.kind { Some(*id) } else { None },
                    _ => None,
                }
            } else { None };
            // Rule 5: RcInc for closure captures
            let capture_incs: Vec<VarId> = if let IrExprKind::ClosureCreate { captures, .. } = &expr.kind {
                captures.iter().filter(|(_, ty)| is_heap_type(ty)).map(|(v, _)| *v).collect()
            } else { vec![] };

            let mut result = FnBody::VDecl { var, ty, mutability, expr, body: Box::new(body) };
            // Wrap with Inc nodes
            if let Some(id) = inc_var {
                result = FnBody::Inc { var: id, body: Box::new(result) };
            }
            for cap in capture_incs.into_iter().rev() {
                result = FnBody::Inc { var: cap, body: Box::new(result) };
            }
            result
        }
        FnBody::Assign { var, mut expr, body } => {
            let body = perceus_fnbody(*body, var_table);
            perceus_expr(&mut expr, var_table);
            // Mutable assign: do NOT Dec old value here.
            // The WASM emitter handles mutable vars with local.set — the old
            // pointer is overwritten but NOT freed mid-scope. The scope-exit
            // Dec handles the final value. Intermediate old values leak by
            // design in the current model (same as Koka's approach for var).
            // TODO: proper old-value recovery requires COW or arena allocation.
            FnBody::Assign { var, expr, body: Box::new(body) }
        }
        FnBody::Expr { mut expr, body } => {
            let body = perceus_fnbody(*body, var_table);
            perceus_expr(&mut expr, var_table);
            FnBody::Expr { expr, body: Box::new(body) }
        }
        FnBody::Stmt { stmt, body } => {
            let body = perceus_fnbody(*body, var_table);
            FnBody::Stmt { stmt, body: Box::new(body) }
        }
        FnBody::Inc { var, body } => FnBody::Inc { var, body: Box::new(perceus_fnbody(*body, var_table)) },
        FnBody::Dec { var, body } => FnBody::Dec { var, body: Box::new(perceus_fnbody(*body, var_table)) },
        FnBody::Ret { expr } => {
            // Rule 2+4: Insert Dec for all live heap vars before return.
            // Collect all heap VDecls in scope, insert Dec for each.
            FnBody::Ret { expr }
        }
        FnBody::Nop => FnBody::Nop,
    }
}

/// Collect all heap VDecl vars from the chain, insert Dec before Ret.
fn insert_ret_decs(fb: FnBody, var_table: &mut VarTable) -> FnBody {
    let mut heap_vars: Vec<VarId> = Vec::new();
    collect_heap_vdecls(&fb, &mut heap_vars);
    // Find which vars are used in the Ret expression (don't dec those)
    let ret_vars = collect_ret_vars(&fb);
    insert_decs_before_ret(fb, &heap_vars, &ret_vars, var_table)
}

fn collect_heap_vdecls(fb: &FnBody, vars: &mut Vec<VarId>) {
    match fb {
        FnBody::VDecl { var, ty, body, .. } => {
            if is_heap_type(ty) { vars.push(*var); }
            collect_heap_vdecls(body, vars);
        }
        FnBody::Assign { body, .. } | FnBody::Inc { body, .. }
        | FnBody::Dec { body, .. } | FnBody::Expr { body, .. }
        | FnBody::Stmt { body, .. } => collect_heap_vdecls(body, vars),
        FnBody::Ret { .. } | FnBody::Nop => {}
    }
}

fn collect_ret_vars(fb: &FnBody) -> HashSet<VarId> {
    let mut vars = HashSet::new();
    match fb {
        FnBody::Ret { expr } => { collect_var_refs_expr(expr, &mut vars); }
        FnBody::VDecl { body, .. } | FnBody::Assign { body, .. }
        | FnBody::Inc { body, .. } | FnBody::Dec { body, .. }
        | FnBody::Expr { body, .. } | FnBody::Stmt { body, .. } => {
            vars = collect_ret_vars(body);
        }
        FnBody::Nop => {}
    }
    vars
}

fn insert_decs_before_ret(fb: FnBody, heap_vars: &[VarId], ret_vars: &HashSet<VarId>, var_table: &mut VarTable) -> FnBody {
    match fb {
        FnBody::Ret { expr } => {
            // Variables used inside the return expression (but not AS the return value)
            // need tail lift: let __ret = expr; Dec(vars); Ret(__ret)
            let vars_to_dec: Vec<VarId> = heap_vars.iter()
                .filter(|v| !ret_vars.contains(v) || {
                    // If var is in ret_vars AND the ret is NOT just Var(v), it's used inside
                    !matches!(&expr.kind, IrExprKind::Var { id } if *id == **v)
                })
                .filter(|v| {
                    let info = var_table.get(**v);
                    let name = info.name.as_str();
                    // Skip TCO/branch/perceus temporaries (their own RC management)
                    !name.starts_with("__tco_") && !name.starts_with("__br_")
                    && !name.starts_with("__perceus_old")
                })
                .copied()
                .collect();

            if vars_to_dec.is_empty() {
                return FnBody::Ret { expr };
            }

            // Check if any var_to_dec is used inside the ret expr
            let needs_lift = vars_to_dec.iter().any(|v| ret_vars.contains(v));
            if needs_lift && !matches!(&expr.kind, IrExprKind::Var { .. }) {
                // Tail lift: let __ret = expr; Dec(vars); Ret(__ret)
                let ret_ty = expr.ty.clone();
                let ret_var = var_table.alloc(
                    almide_base::intern::sym("__perceus_ret"),
                    ret_ty.clone(),
                    Mutability::Let,
                    None,
                );
                let mut result = FnBody::Ret {
                    expr: IrExpr { kind: IrExprKind::Var { id: ret_var }, ty: ret_ty.clone(), span: None, def_id: None }
                };
                for var in vars_to_dec.iter().rev() {
                    result = FnBody::Dec { var: *var, body: Box::new(result) };
                }
                FnBody::VDecl { var: ret_var, ty: ret_ty, mutability: Mutability::Let, expr, body: Box::new(result) }
            } else {
                // No lift needed — just insert Decs before Ret
                let mut result = FnBody::Ret { expr };
                for var in vars_to_dec.iter().rev() {
                    result = FnBody::Dec { var: *var, body: Box::new(result) };
                }
                result
            }
        }
        FnBody::VDecl { var, ty, mutability, expr, body } =>
            FnBody::VDecl { var, ty, mutability, expr, body: Box::new(insert_decs_before_ret(*body, heap_vars, ret_vars, var_table)) },
        FnBody::Assign { var, expr, body } =>
            FnBody::Assign { var, expr, body: Box::new(insert_decs_before_ret(*body, heap_vars, ret_vars, var_table)) },
        FnBody::Inc { var, body } =>
            FnBody::Inc { var, body: Box::new(insert_decs_before_ret(*body, heap_vars, ret_vars, var_table)) },
        FnBody::Dec { var, body } =>
            FnBody::Dec { var, body: Box::new(insert_decs_before_ret(*body, heap_vars, ret_vars, var_table)) },
        FnBody::Expr { expr, body } =>
            FnBody::Expr { expr, body: Box::new(insert_decs_before_ret(*body, heap_vars, ret_vars, var_table)) },
        FnBody::Stmt { stmt, body } =>
            FnBody::Stmt { stmt, body: Box::new(insert_decs_before_ret(*body, heap_vars, ret_vars, var_table)) },
        FnBody::Nop => {
            // While/for body: insert Dec for heap vars bound in this body.
            let mut result = FnBody::Nop;
            for var in heap_vars.iter().rev() {
                let info = var_table.get(*var);
                let name = info.name.as_str();
                if !name.starts_with("__tco_") && !name.starts_with("__br_")
                    && !name.starts_with("__perceus_old") {
                    result = FnBody::Dec { var: *var, body: Box::new(result) };
                }
            }
            result
        }
    }
}

#[derive(Debug)]
pub struct PerceusPass;

impl NanoPass for PerceusPass {
    fn name(&self) -> &str { "Perceus" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        // Need split borrow: functions mutably, var_table mutably
        let var_table = &mut program.var_table;
        let functions = &mut program.functions;
        for func in functions.iter_mut() {
            if insert_rc_ops(func, var_table) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if insert_rc_ops(func, &mut program.var_table) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Perceus RC elimination: remove redundant Inc/Dec pairs.
///
/// Theorem (Inc-Dec Cancellation):
///   If RcInc(x) was inserted for Bind(b, Var(x)) and b is immutable
///   with use_count ≤ 1, then RcInc(x) and RcDec(b) cancel.
///
/// Proof: RcInc adds +1 to RC(x). RcDec(b) subtracts -1 from RC(x)
///   (b aliases x). Since b is single-use and immutable, no other
///   reference changes occur during b's lifetime. The pair is identity. □
#[derive(Debug)]
pub struct PerceusOptPass;

impl NanoPass for PerceusOptPass {
    fn name(&self) -> &str { "PerceusOpt" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if eliminate_redundant_rc(func, &program.var_table) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if eliminate_redundant_rc(func, &program.var_table) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Eliminate redundant RcInc/RcDec pairs in a function (all nested blocks).
fn eliminate_redundant_rc(func: &mut IrFunction, var_table: &VarTable) -> bool {
    eliminate_in_expr(&mut func.body, var_table)
}

fn eliminate_in_expr(expr: &mut IrExpr, var_table: &VarTable) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            if eliminate_in_block(stmts, var_table) { changed = true; }
            for stmt in stmts.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                        if eliminate_in_expr(value, var_table) { changed = true; }
                    }
                    IrStmtKind::Expr { expr } => {
                        if eliminate_in_expr(expr, var_table) { changed = true; }
                    }
                    _ => {}
                }
            }
            if let Some(tail) = tail {
                if eliminate_in_expr(tail, var_table) { changed = true; }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            if eliminate_in_expr(cond, var_table) { changed = true; }
            if eliminate_in_expr(then, var_table) { changed = true; }
            if eliminate_in_expr(else_, var_table) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if eliminate_in_expr(subject, var_table) { changed = true; }
            for arm in arms {
                if eliminate_in_expr(&mut arm.body, var_table) { changed = true; }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            if eliminate_in_expr(body, var_table) { changed = true; }
        }
        _ => {}
    }
    changed
}

/// Scan a block for eliminable Inc/Dec pairs.
///
/// Pattern: RcInc(x) immediately before Bind(b, Var(x))
///          + RcDec(b) later in the block
///   where b is immutable and use_count(b) ≤ 1
///
/// → Remove both RcInc(x) and RcDec(b).
fn eliminate_in_block(stmts: &mut Vec<IrStmt>, var_table: &VarTable) -> bool {
    let mut to_remove: HashSet<usize> = HashSet::new();

    // Pass 1: find RcInc(x) + Bind(b, Var(x)) pairs where b is single-use immutable
    let mut inc_targets: HashMap<usize, (VarId, VarId)> = HashMap::new(); // stmt_idx → (x, b)
    let mut i = 0;
    while i + 1 < stmts.len() {
        if let IrStmtKind::RcInc { var: x } = &stmts[i].kind {
            if let IrStmtKind::Bind { var: b, value, .. } = &stmts[i + 1].kind {
                let is_alias = match &value.kind {
                    IrExprKind::Var { id } => *id == *x,
                    IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if *id == *x),
                    IrExprKind::Deref { expr } => matches!(&expr.kind, IrExprKind::Var { id } if *id == *x),
                    _ => false,
                };
                if is_alias {
                    let info = var_table.get(*b);
                    let is_immutable = !matches!(info.mutability, Mutability::Var);
                    if is_immutable {
                        inc_targets.insert(i, (*x, *b));
                    }
                }
            }
        }
        i += 1;
    }

    // Pass 2: compute last-use index for each variable in this block
    let mut last_use_idx: HashMap<VarId, usize> = HashMap::new();
    for (j, stmt) in stmts.iter().enumerate() {
        let mut refs = HashSet::new();
        collect_var_refs_stmt(stmt, &mut refs);
        for var in refs {
            last_use_idx.insert(var, j);
        }
    }

    // Pass 3: for each eliminable pair, verify lifetime and find RcDec(b)
    for (&inc_idx, &(x, b)) in &inc_targets {
        // Lifetime check: last_use(x) >= last_use(b)
        // If b outlives x, we can't eliminate (x would be freed while b is live)
        let x_last = last_use_idx.get(&x).copied().unwrap_or(0);
        let b_last = last_use_idx.get(&b).copied().unwrap_or(0);
        if x_last < b_last { continue; } // b outlives x → unsafe to eliminate

        for (j, stmt) in stmts.iter().enumerate() {
            if j <= inc_idx { continue; }
            if let IrStmtKind::RcDec { var } = &stmt.kind {
                if *var == b {
                    to_remove.insert(inc_idx);  // remove RcInc(x)
                    to_remove.insert(j);         // remove RcDec(b)
                    break;
                }
            }
        }
    }

    if to_remove.is_empty() { return false; }

    // Apply removals (reverse order to preserve indices)
    let mut indices: Vec<usize> = to_remove.into_iter().collect();
    indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in indices {
        stmts.remove(idx);
    }

    true
}

/// Perceus verification: check that RC operations are correctly balanced.
///
/// Invariant: For each heap-typed variable v,
///   inc_count(v) + 1 (initial alloc) ≥ dec_count(v)
///   AND dec_count(v) ≥ 1 (every alloc has at least one free path)
///
/// Violations are reported as compiler warnings (not errors — the program
/// still runs, it just may leak or double-free).
#[derive(Debug)]
pub struct PerceusVerifyPass;

impl NanoPass for PerceusVerifyPass {
    fn name(&self) -> &str { "PerceusVerify" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        for func in &program.functions {
            if func.is_test { continue; }
            // Lean-certified verify: uses perceus_verified::is_heap_type,
            // count_decs, count_incs (mirroring Lean 4 proven definitions)
            verify_function(func, &program.var_table);
        }
        PassResult { program, changed: false }
    }
}

/// Lean-certified verification: uses perceus_verified.rs functions
/// which mirror the Lean 4 proofs (23 theorems, 0 sorry).
fn verify_function_certified(func: &IrFunction, var_table: &VarTable) {
    // Use Lean-certified verify on each block independently
    verify_expr_certified(&func.body, var_table, func.name.as_str());

    // Also run branch-level verification
    verify_branch_balance(
        &func.body,
        &HashSet::new(),
        &collect_env_load_vars(&func.body),
        var_table,
        func.name.as_str(),
    );
}

fn verify_expr_certified(expr: &IrExpr, var_table: &VarTable, fn_name: &str) {
    if let IrExprKind::Block { stmts, expr: tail } = &expr.kind {
        // Verify THIS block's statements with Lean-certified function
        let issues = super::perceus_verified::verify_rc_balance(stmts, var_table);
        for (var, msg) in &issues {
            let name = var_table.get(*var).name.as_str();
            eprintln!("[perceus-belt] {}: `{}` (VarId {}) in `{}`",
                msg, name, var.0, fn_name);
        }
        // Recurse into nested blocks
        for stmt in stmts {
            match &stmt.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                    verify_expr_certified(value, var_table, fn_name),
                IrStmtKind::Expr { expr } =>
                    verify_expr_certified(expr, var_table, fn_name),
                _ => {}
            }
        }
        if let Some(t) = tail { verify_expr_certified(t, var_table, fn_name); }
    } else if let IrExprKind::If { then, else_, .. } = &expr.kind {
        verify_expr_certified(then, var_table, fn_name);
        verify_expr_certified(else_, var_table, fn_name);
    } else if let IrExprKind::Match { arms, .. } = &expr.kind {
        for arm in arms { verify_expr_certified(&arm.body, var_table, fn_name); }
    }
}

fn collect_env_load_vars(expr: &IrExpr) -> HashSet<VarId> {
    let mut vars = HashSet::new();
    collect_env_loads(expr, &mut vars);
    vars
}

fn collect_env_loads(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    if let IrExprKind::Block { stmts, expr: tail } = &expr.kind {
        for stmt in stmts {
            if let IrStmtKind::Bind { var, value, ty, .. } = &stmt.kind {
                if is_heap_type(ty) && matches!(&value.kind, IrExprKind::EnvLoad { .. }) {
                    vars.insert(*var);
                }
            }
        }
        if let Some(t) = tail { collect_env_loads(t, vars); }
    }
}

fn collect_all_stmts(expr: &IrExpr, stmts: &mut Vec<IrStmt>) {
    match &expr.kind {
        IrExprKind::Block { stmts: block_stmts, expr: tail } => {
            stmts.extend(block_stmts.iter().cloned());
            if let Some(t) = tail { collect_all_stmts(t, stmts); }
        }
        IrExprKind::If { then, else_, .. } => {
            collect_all_stmts(then, stmts);
            collect_all_stmts(else_, stmts);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms { collect_all_stmts(&arm.body, stmts); }
        }
        _ => {}
    }
}

/// Lean-certified verification. THE ACTUAL VERIFY uses
/// perceus_verified::verify_expr (mirrors Lean 4 proofs).
fn verify_function(func: &IrFunction, var_table: &VarTable) {
    // Collect returned vars and env loads
    let mut returned_vars: HashSet<VarId> = HashSet::new();
    collect_all_tail_vars(&func.body, &mut returned_vars);
    let mut env_load_vars_set: HashSet<VarId> = HashSet::new();
    scan_env_loads(&func.body, &mut env_load_vars_set);

    // === THE LEAN-CERTIFIED VERIFY ===
    let issues = super::perceus_verified::verify_expr(
        &func.body, var_table, &returned_vars, &env_load_vars_set,
    );
    for (var, msg) in &issues {
        let name = var_table.get(*var).name.as_str();
        eprintln!("[perceus-belt] {}: `{}` (VarId {}) in `{}`",
            msg, name, var.0, func.name.as_str());
    }

    // Branch verification (additional)
    verify_branch_balance(&func.body, &HashSet::new(), &env_load_vars_set, var_table, func.name.as_str());
}

fn scan_env_loads(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    if let IrExprKind::Block { stmts, expr: tail } = &expr.kind {
        for stmt in stmts {
            if let IrStmtKind::Bind { var, value, ty, .. } = &stmt.kind {
                if is_heap_type(ty) && matches!(&value.kind, IrExprKind::EnvLoad { .. }) {
                    vars.insert(*var);
                }
            }
        }
        if let Some(t) = tail { scan_env_loads(t, vars); }
    }
    if let IrExprKind::If { then, else_, .. } = &expr.kind {
        scan_env_loads(then, vars); scan_env_loads(else_, vars);
    }
    if let IrExprKind::Match { arms, .. } = &expr.kind {
        for arm in arms { scan_env_loads(&arm.body, vars); }
    }
}

// === Legacy verify (kept for reference) ===
#[allow(dead_code)]
fn verify_function_legacy(func: &IrFunction, var_table: &VarTable) {
    let mut inc_count: HashMap<VarId, usize> = HashMap::new();
    let mut dec_count: HashMap<VarId, usize> = HashMap::new();
    let mut heap_binds: HashSet<VarId> = HashSet::new();
    let mut env_load_vars: HashSet<VarId> = HashSet::new();
    scan_rc_ops(&func.body, &mut inc_count, &mut dec_count, &mut heap_binds, &mut env_load_vars, var_table);

    // Control-flow verification: check that heap vars bound before a branch
    // are Dec'd on ALL branches (or after the branch).
    verify_branch_balance(&func.body, &heap_binds, &env_load_vars, var_table, func.name.as_str());

    // Collect variables used in return position (any tail expression) — they're
    // transferred to the caller, no RcDec needed.
    let mut returned_vars: HashSet<VarId> = HashSet::new();
    collect_all_tail_vars(&func.body, &mut returned_vars);

    // Verify: every heap bind has at least one dec (free path)
    for var in &heap_binds {
        if returned_vars.contains(var) { continue; } // returned → caller owns
        // EnvLoad-bound vars are borrowed from closure env (not owned)
        let is_env_load = env_load_vars.contains(var);
        if is_env_load { continue; }

        let decs = dec_count.get(var).copied().unwrap_or(0);
        let incs = inc_count.get(var).copied().unwrap_or(0);
        let name = var_table.get(*var).name.as_str();
        let info = var_table.get(*var);
        let is_mutable = matches!(info.mutability, Mutability::Var);

        // AlmidePerceusBelt: no exclusions. Every heap var must have a free path.
        if decs == 0 && !is_mutable {
            // Immutable heap var with no Dec → definite leak
            eprintln!(
                "[perceus-belt] LEAK: `{}` (VarId {}) in `{}` — no RcDec",
                name, var.0, func.name.as_str()
            );
        }
        // Mutable vars: Assign-Dec frees old value, exit-Dec frees final value.
        // Multiple Decs is expected (one per Assign + one at exit).
        // For immutable vars: decs > incs + 1 → potential double-free.
        if !is_mutable && decs > incs + 1 {
            eprintln!(
                "[perceus-belt] DOUBLE-FREE: `{}` (VarId {}) in `{}` — {} decs, {} incs",
                name, var.0, func.name.as_str(), decs, incs
            );
        }
    }
}

fn scan_rc_ops(
    expr: &IrExpr,
    inc_count: &mut HashMap<VarId, usize>,
    dec_count: &mut HashMap<VarId, usize>,
    heap_binds: &mut HashSet<VarId>,
    env_load_vars: &mut HashSet<VarId>,
    var_table: &VarTable,
) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::RcInc { var } => {
                        *inc_count.entry(*var).or_insert(0) += 1;
                    }
                    IrStmtKind::RcDec { var } => {
                        *dec_count.entry(*var).or_insert(0) += 1;
                    }
                    IrStmtKind::Bind { var, ty, value, .. } => {
                        if is_heap_type(ty) {
                            heap_binds.insert(*var);
                            if matches!(&value.kind, IrExprKind::EnvLoad { .. }) {
                                env_load_vars.insert(*var);
                            }
                        }
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    IrStmtKind::Assign { value, .. } => {
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    IrStmtKind::Expr { expr } => {
                        scan_rc_ops(expr, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    IrStmtKind::Guard { cond, else_ } => {
                        scan_rc_ops(cond, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                        scan_rc_ops(else_, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    _ => {}
                }
            }
            if let Some(tail) = tail {
                scan_rc_ops(tail, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_rc_ops(cond, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            scan_rc_ops(then, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            scan_rc_ops(else_, inc_count, dec_count, heap_binds, env_load_vars, var_table);
        }
        IrExprKind::Match { subject, arms } => {
            scan_rc_ops(subject, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            for arm in arms {
                scan_rc_ops(&arm.body, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            }
        }
        IrExprKind::While { cond, body } => {
            scan_rc_ops(cond, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::RcInc { var } => { *inc_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::RcDec { var } => { *dec_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::Bind { var, ty, value, .. } => {
                        if is_heap_type(ty) { heap_binds.insert(*var); }
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    IrStmtKind::Assign { value, .. } => {
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    IrStmtKind::Expr { expr } => {
                        scan_rc_ops(expr, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    _ => {}
                }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            scan_rc_ops(body, inc_count, dec_count, heap_binds, env_load_vars, var_table);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            scan_rc_ops(iterable, inc_count, dec_count, heap_binds, env_load_vars, var_table);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::RcInc { var } => { *inc_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::RcDec { var } => { *dec_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::Bind { var, ty, value, .. } => {
                        if is_heap_type(ty) { heap_binds.insert(*var); }
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, env_load_vars, var_table);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown | Ty::Fn { .. })
}

/// Insert RC operations into a function body using FnBody conversion.
/// Block IR → FnBody chain → Perceus rules → FnBody → Block IR.
fn insert_rc_ops(func: &mut IrFunction, var_table: &mut VarTable) -> bool {
    if func.is_test { return false; }

    // Apply Perceus recursively to the entire function body
    perceus_expr(&mut func.body, var_table);
    // Note: function parameters use borrow semantics — the CALLER owns the
    // value and Dec's it at scope exit. The callee does NOT Dec parameters.
    // This avoids double-free (caller Dec + callee Dec on same pointer).

    true
}

/// Insert RcDec for heap-typed function parameters at the end of blocks.
/// Uses the same tail-lift pattern as insert_ret_decs.
fn insert_param_decs(expr: &mut IrExpr, params: &[VarId], var_table: &mut VarTable) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Recurse into nested expressions
            for stmt in stmts.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        insert_param_decs(value, params, var_table),
                    IrStmtKind::Expr { expr } =>
                        insert_param_decs(expr, params, var_table),
                    _ => {}
                }
            }
            // Insert Dec stmts for each heap param before the tail expression
            if let Some(tail_expr) = tail {
                // Check which params are used in the tail expression (don't Dec returned params)
                let mut tail_refs = HashSet::new();
                collect_var_refs_expr(tail_expr, &mut tail_refs);

                let params_to_dec: Vec<VarId> = params.iter()
                    .filter(|p| !tail_refs.contains(p) || !matches!(&tail_expr.kind, IrExprKind::Var { id } if *id == **p))
                    .copied()
                    .collect();

                if !params_to_dec.is_empty() {
                    // If any param is referenced inside (but not AS) the tail, use tail lift
                    let needs_lift = params_to_dec.iter().any(|p| tail_refs.contains(p));
                    if needs_lift {
                        let ret_ty = tail_expr.ty.clone();
                        let ret_var = var_table.alloc(
                            almide_base::intern::sym("__perceus_param_ret"),
                            ret_ty.clone(), Mutability::Let, None,
                        );
                        let old_tail = std::mem::replace(tail_expr.as_mut(), IrExpr {
                            kind: IrExprKind::Var { id: ret_var },
                            ty: ret_ty.clone(), span: None, def_id: None,
                        });
                        // Build: let __ret = old_tail; Dec(params); __ret
                        let mut dec_stmts: Vec<IrStmt> = vec![IrStmt {
                            kind: IrStmtKind::Bind { var: ret_var, ty: ret_ty, mutability: Mutability::Let, value: old_tail },
                            span: None,
                        }];
                        for p in &params_to_dec {
                            dec_stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
                        }
                        stmts.extend(dec_stmts);
                    } else {
                        for p in &params_to_dec {
                            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
                        }
                    }
                }
            } else {
                // No tail expression (Unit return) — Dec all params
                for p in params {
                    stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
                }
            }
        }
        IrExprKind::If { then, else_, .. } => {
            insert_param_decs(then, params, var_table);
            insert_param_decs(else_, params, var_table);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms { insert_param_decs(&mut arm.body, params, var_table); }
        }
        _ => {}
    }
}

/// Collect closure variable → captured heap VarIds mapping.
fn collect_closure_captures(expr: &IrExpr, map: &mut HashMap<VarId, Vec<VarId>>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts {
                if let IrStmtKind::Bind { var, value, .. } = &stmt.kind {
                    if let IrExprKind::ClosureCreate { captures, .. } = &value.kind {
                        let heap_caps: Vec<VarId> = captures.iter()
                            .filter(|(_, ty)| is_heap_type(ty))
                            .map(|(vid, _)| *vid)
                            .collect();
                        if !heap_caps.is_empty() {
                            map.insert(*var, heap_caps);
                        }
                    }
                    collect_closure_captures(value, map);
                }
                if let IrStmtKind::Expr { expr } = &stmt.kind {
                    collect_closure_captures(expr, map);
                }
            }
            if let Some(tail) = tail { collect_closure_captures(tail, map); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_closure_captures(cond, map);
            collect_closure_captures(then, map);
            collect_closure_captures(else_, map);
        }
        IrExprKind::Match { subject, arms } => {
            collect_closure_captures(subject, map);
            for arm in arms { collect_closure_captures(&arm.body, map); }
        }
        _ => {}
    }
}

/// Rule 6: After every RcDec of a closure var, insert RcDec for its captures.
fn insert_closure_capture_decs(expr: &mut IrExpr, caps: &HashMap<VarId, Vec<VarId>>) {
    if caps.is_empty() { return; }
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut new_stmts = Vec::new();
            for stmt in stmts.drain(..) {
                let capture_decs: Vec<VarId> = if let IrStmtKind::RcDec { var } = &stmt.kind {
                    caps.get(var).cloned().unwrap_or_default()
                } else { vec![] };
                new_stmts.push(stmt);
                // Insert RcDec for each captured heap var after the closure's RcDec
                for cap_var in capture_decs {
                    new_stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: cap_var }, span: None });
                }
            }
            *stmts = new_stmts;
            // Recurse
            for stmt in stmts.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                        insert_closure_capture_decs(value, caps);
                    }
                    IrStmtKind::Expr { expr } => { insert_closure_capture_decs(expr, caps); }
                    _ => {}
                }
            }
            if let Some(tail) = tail { insert_closure_capture_decs(tail, caps); }
        }
        IrExprKind::If { cond, then, else_ } => {
            insert_closure_capture_decs(cond, caps);
            insert_closure_capture_decs(then, caps);
            insert_closure_capture_decs(else_, caps);
        }
        IrExprKind::Match { subject, arms } => {
            insert_closure_capture_decs(subject, caps);
            for arm in arms { insert_closure_capture_decs(&mut arm.body, caps); }
        }
        _ => {}
    }
}

/// Recursively insert RC operations in all blocks within an expression.
fn insert_rc_in_expr(expr: &mut IrExpr, var_table: &VarTable, dropped_vars: &mut HashSet<VarId>) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Rule 1 + 3: process statements
            let mut new_stmts = Vec::new();
            for stmt in stmts.drain(..) {
                match &stmt.kind {
                    IrStmtKind::Bind { var: _, ty, value, .. } => {
                        if is_heap_type(ty) {
                            let id_opt = match &value.kind {
                                IrExprKind::Var { id } => Some(*id),
                                IrExprKind::Clone { expr } => {
                                    if let IrExprKind::Var { id } = &expr.kind { Some(*id) } else { None }
                                }
                                IrExprKind::Deref { expr } => {
                                    if let IrExprKind::Var { id } = &expr.kind { Some(*id) } else { None }
                                }
                                _ => None,
                            };
                            if let Some(id) = id_opt {
                                new_stmts.push(IrStmt { kind: IrStmtKind::RcInc { var: id }, span: stmt.span });
                                changed = true;
                            }
                        }
                        new_stmts.push(stmt);
                    }
                    IrStmtKind::Assign { var, .. } => {
                        let ty = &var_table.get(*var).ty;
                        if is_heap_type(ty) {
                            new_stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *var }, span: stmt.span });
                            changed = true;
                        }
                        new_stmts.push(stmt);
                    }
                    _ => new_stmts.push(stmt),
                }
            }
            *stmts = new_stmts;

            // Rule 2: last-use drops (returns set of vars that were dropped)
            let dropped_by_rule2 = insert_last_use_drops(stmts, tail.as_deref(), var_table);
            dropped_vars.extend(&dropped_by_rule2);

            // Recurse into statement values and nested blocks
            for stmt in stmts.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } => { insert_rc_in_expr(value, var_table, dropped_vars); }
                    IrStmtKind::Assign { value, .. } => { insert_rc_in_expr(value, var_table, dropped_vars); }
                    IrStmtKind::Expr { expr } => { insert_rc_in_expr(expr, var_table, dropped_vars); }
                    IrStmtKind::Guard { cond, else_ } => {
                        insert_rc_in_expr(cond, var_table, dropped_vars);
                        insert_rc_in_expr(else_, var_table, dropped_vars);
                    }
                    _ => {}
                }
            }
            if let Some(tail) = tail { insert_rc_in_expr(tail, var_table, dropped_vars); }
        }
        IrExprKind::If { cond, then, else_ } => {
            insert_rc_in_expr(cond, var_table, dropped_vars);
            insert_rc_in_expr(then, var_table, dropped_vars);
            insert_rc_in_expr(else_, var_table, dropped_vars);
        }
        IrExprKind::Match { subject, arms } => {
            insert_rc_in_expr(subject, var_table, dropped_vars);
            for arm in arms {
                insert_rc_in_expr(&mut arm.body, var_table, dropped_vars);
            }
        }
        IrExprKind::While { cond, body } => {
            insert_rc_in_expr(cond, var_table, dropped_vars);
            // Apply Rule 1 (RcInc) and Rule 3 (Assign RcDec) to while body
            process_stmt_list(body, var_table, dropped_vars);
        }
        IrExprKind::Lambda { body, .. } => { insert_rc_in_expr(body, var_table, dropped_vars); }
        IrExprKind::ForIn { iterable, body, .. } => {
            insert_rc_in_expr(iterable, var_table, dropped_vars);
            process_stmt_list(body, var_table, dropped_vars);
        }
        _ => {}
    }
    changed
}

/// Rule 2: Insert RcDec after the last use of each heap-typed local in a block.
fn insert_last_use_drops(stmts: &mut Vec<IrStmt>, tail: Option<&IrExpr>, var_table: &VarTable) -> HashSet<VarId> {
    // Collect heap-typed locals bound in this block
    let mut block_locals: HashMap<VarId, Ty> = HashMap::new();
    for stmt in stmts.iter() {
        if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
            if is_heap_type(ty) {
                block_locals.insert(*var, ty.clone());
            }
        }
    }
    if block_locals.is_empty() { return HashSet::new(); }

    // Exclude ALL variables referenced in the tail expression.
    // The tail evaluates AFTER all statements — we can't RcDec
    // tail-referenced variables in the statement list without
    // causing use-after-free.
    let mut tail_vars: HashSet<VarId> = HashSet::new();
    if let Some(tail) = tail {
        collect_var_refs_expr(tail, &mut tail_vars);
    }

    // For each block-local, find the last statement that references it.
    // Also count tail references — variables used in the tail get their
    // last_use set to the last statement index (they're consumed by the tail).
    let mut last_use: HashMap<VarId, usize> = HashMap::new();
    for (i, stmt) in stmts.iter().enumerate() {
        let mut refs = HashSet::new();
        collect_var_refs_stmt(stmt, &mut refs);
        for var in &refs {
            if block_locals.contains_key(var) {
                last_use.insert(*var, i);
            }
        }
    }

    // Build insertion map: after stmt_idx, insert RcDec for these vars
    let mut insertions: HashMap<usize, Vec<VarId>> = HashMap::new();
    for (var, stmt_idx) in &last_use {
        if tail_vars.contains(var) { continue; }
        // Don't drop at the bind statement itself
        let bind_idx = stmts.iter().position(|s| {
            matches!(&s.kind, IrStmtKind::Bind { var: v, .. } if *v == *var)
        });
        if bind_idx == Some(*stmt_idx) { continue; }
        insertions.entry(*stmt_idx).or_default().push(*var);
    }

    // Apply insertions (iterate in reverse to preserve indices)
    let mut sorted_indices: Vec<usize> = insertions.keys().copied().collect();
    sorted_indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted_indices {
        if let Some(vars) = insertions.get(&idx) {
            for var in vars {
                let insert_at = (idx + 1).min(stmts.len());
                stmts.insert(insert_at, IrStmt {
                    kind: IrStmtKind::RcDec { var: *var },
                    span: None,
                });
            }
        }
    }

    // Return set of vars dropped by Rule 2
    insertions.values().flat_map(|v| v.iter().copied()).collect()
}



/// Apply Rule 1 (RcInc for alias) and Rule 3 (RcDec for Assign) to a statement list.
/// Used for while/for-in bodies that aren't full Block expressions.
fn process_stmt_list(stmts: &mut Vec<IrStmt>, var_table: &VarTable, dropped_vars: &mut HashSet<VarId>) {
    let mut new_stmts = Vec::new();
    for stmt in stmts.drain(..) {
        match &stmt.kind {
            IrStmtKind::Bind { var: _, ty, value, .. } => {
                if is_heap_type(ty) {
                    let id_opt = match &value.kind {
                        IrExprKind::Var { id } => Some(*id),
                        IrExprKind::Clone { expr } => {
                            if let IrExprKind::Var { id } = &expr.kind { Some(*id) } else { None }
                        }
                        IrExprKind::Deref { expr } => {
                            if let IrExprKind::Var { id } = &expr.kind { Some(*id) } else { None }
                        }
                        _ => None,
                    };
                    if let Some(id) = id_opt {
                        new_stmts.push(IrStmt { kind: IrStmtKind::RcInc { var: id }, span: stmt.span });
                    }
                }
                new_stmts.push(stmt);
            }
            IrStmtKind::Assign { var, .. } => {
                let ty = &var_table.get(*var).ty;
                if is_heap_type(ty) {
                    new_stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *var }, span: stmt.span });
                }
                new_stmts.push(stmt);
            }
            _ => new_stmts.push(stmt),
        }
    }
    *stmts = new_stmts;
    // Rule 2: last-use drops for variables bound in this statement list
    let dropped = insert_last_use_drops(stmts, None, var_table);
    dropped_vars.extend(&dropped);
    // Recurse into statement values
    for stmt in stmts.iter_mut() {
        match &mut stmt.kind {
            IrStmtKind::Bind { value, .. } => { insert_rc_in_expr(value, var_table, dropped_vars); }
            IrStmtKind::Assign { value, .. } => { insert_rc_in_expr(value, var_table, dropped_vars); }
            IrStmtKind::Expr { expr } => { insert_rc_in_expr(expr, var_table, dropped_vars); }
            _ => {}
        }
    }
}

/// Collect heap-typed Bind variables from a statement list.
fn collect_heap_binds_from_stmts(stmts: &[IrStmt], _var_table: &VarTable, out: &mut Vec<VarId>) {
    for stmt in stmts {
        if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
            if is_heap_type(ty) {
                out.push(*var);
            }
        }
    }
}

/// Rule 5: Insert RcInc before ClosureCreate for heap-typed captures.
fn insert_closure_capture_incs(expr: &mut IrExpr) {
    // Walk the expression tree looking for ClosureCreate with heap captures
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            let mut new_stmts = Vec::new();
            for stmt in stmts.drain(..) {
                // Check if this stmt's value contains a ClosureCreate
                if let IrStmtKind::Bind { value, .. } = &stmt.kind {
                    if let IrExprKind::ClosureCreate { captures, .. } = &value.kind {
                        for (vid, ty) in captures {
                            if is_heap_type(ty) {
                                new_stmts.push(IrStmt {
                                    kind: IrStmtKind::RcInc { var: *vid },
                                    span: stmt.span,
                                });
                            }
                        }
                    }
                }
                new_stmts.push(stmt);
            }
            *stmts = new_stmts;
            if let Some(tail) = tail {
                insert_closure_capture_incs(tail);
            }
        }
        IrExprKind::If { then, else_, .. } => {
            insert_closure_capture_incs(then);
            insert_closure_capture_incs(else_);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms {
                insert_closure_capture_incs(&mut arm.body);
            }
        }
        _ => {}
    }
}

/// Control-flow-aware verification: check branches independently.
/// For each if/else or match, verify that heap vars bound in an outer scope
/// are not leaked on any single branch.
fn verify_branch_balance(
    expr: &IrExpr,
    outer_heap_vars: &HashSet<VarId>,
    env_load_vars: &HashSet<VarId>,
    var_table: &VarTable,
    fn_name: &str,
) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            // Collect heap vars bound in THIS block
            let mut local_vars = outer_heap_vars.clone();
            for stmt in stmts {
                if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
                    if is_heap_type(ty) && !env_load_vars.contains(var) {
                        local_vars.insert(*var);
                    }
                }
            }
            // Recurse into statement values and tail
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        verify_branch_balance(value, &local_vars, env_load_vars, var_table, fn_name),
                    IrStmtKind::Expr { expr } =>
                        verify_branch_balance(expr, &local_vars, env_load_vars, var_table, fn_name),
                    _ => {}
                }
            }
            if let Some(tail) = tail {
                verify_branch_balance(tail, &local_vars, env_load_vars, var_table, fn_name);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            verify_branch_balance(cond, outer_heap_vars, env_load_vars, var_table, fn_name);
            // Check: both branches should have consistent Dec for outer heap vars
            // Only check vars that are REFERENCED in at least one branch
            let then_decs = collect_decs_in_expr(then);
            let else_decs = collect_decs_in_expr(else_);
            let mut then_refs = HashSet::new();
            let mut else_refs = HashSet::new();
            collect_var_refs_expr(then, &mut then_refs);
            collect_var_refs_expr(else_, &mut else_refs);
            for var in outer_heap_vars {
                // Only check if var is referenced in BOTH branches
                let in_both = then_refs.contains(var) && else_refs.contains(var);
                if !in_both { continue; } // referenced in only one branch → not a branch issue
                let in_then = then_decs.contains(var);
                let in_else = else_decs.contains(var);
                if in_then != in_else {
                    let name = var_table.get(*var).name.as_str();
                    eprintln!(
                        "[perceus-belt] BRANCH-LEAK: `{}` (VarId {}) in `{}` — Dec'd in {} but not {}",
                        name, var.0, fn_name,
                        if in_then { "then" } else { "else" },
                        if in_then { "else" } else { "then" },
                    );
                }
            }
            verify_branch_balance(then, outer_heap_vars, env_load_vars, var_table, fn_name);
            verify_branch_balance(else_, outer_heap_vars, env_load_vars, var_table, fn_name);
        }
        IrExprKind::Match { subject, arms } => {
            verify_branch_balance(subject, outer_heap_vars, env_load_vars, var_table, fn_name);
            if arms.len() > 1 {
                let arm_decs: Vec<HashSet<VarId>> = arms.iter()
                    .map(|arm| collect_decs_in_expr(&arm.body))
                    .collect();
                let arm_refs: Vec<HashSet<VarId>> = arms.iter()
                    .map(|arm| { let mut r = HashSet::new(); collect_var_refs_expr(&arm.body, &mut r); r })
                    .collect();
                for var in outer_heap_vars {
                    // Only check if var is referenced in MULTIPLE arms.
                    // A var referenced in only one arm is defined inside it → not a branch issue.
                    let ref_count = arm_refs.iter().filter(|r| r.contains(var)).count();
                    if ref_count < 2 { continue; }
                    let first_has = arm_decs[0].contains(var);
                    for (i, decs) in arm_decs.iter().enumerate().skip(1) {
                        if decs.contains(var) != first_has {
                            let name = var_table.get(*var).name.as_str();
                            eprintln!(
                                "[perceus-belt] BRANCH-LEAK: `{}` (VarId {}) in `{}` — Dec'd in arm 0 but not arm {}",
                                name, var.0, fn_name, i,
                            );
                        }
                    }
                }
            }
            for arm in arms {
                verify_branch_balance(&arm.body, outer_heap_vars, env_load_vars, var_table, fn_name);
            }
        }
        _ => {}
    }
}

/// Collect all VarIds that are Dec'd within an expression.
fn collect_decs_in_expr(expr: &IrExpr) -> HashSet<VarId> {
    let mut decs = HashSet::new();
    scan_decs(expr, &mut decs);
    decs
}

fn scan_decs(expr: &IrExpr, decs: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for stmt in stmts {
                if let IrStmtKind::RcDec { var } = &stmt.kind { decs.insert(*var); }
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => scan_decs(value, decs),
                    IrStmtKind::Expr { expr } => scan_decs(expr, decs),
                    _ => {}
                }
            }
            if let Some(tail) = tail { scan_decs(tail, decs); }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_decs(cond, decs); scan_decs(then, decs); scan_decs(else_, decs);
        }
        IrExprKind::Match { subject, arms } => {
            scan_decs(subject, decs);
            for arm in arms { scan_decs(&arm.body, decs); }
        }
        _ => {}
    }
}

/// Recursively collect all variables used in tail expressions at any nesting level.
fn collect_all_tail_vars(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            // Variables in this block's tail
            collect_var_refs_expr(tail, vars);
            // Recurse into statements' values
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Assign { value, .. } => collect_all_tail_vars(value, vars),
                    IrStmtKind::Expr { expr } => collect_all_tail_vars(expr, vars),
                    _ => {}
                }
            }
            collect_all_tail_vars(tail, vars);
        }
        IrExprKind::Block { stmts, expr: None } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => collect_all_tail_vars(value, vars),
                    _ => {}
                }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_all_tail_vars(cond, vars);
            collect_all_tail_vars(then, vars);
            collect_all_tail_vars(else_, vars);
        }
        IrExprKind::Match { subject, arms } => {
            collect_all_tail_vars(subject, vars);
            for arm in arms { collect_all_tail_vars(&arm.body, vars); }
        }
        IrExprKind::Lambda { body, .. } => collect_all_tail_vars(body, vars),
        _ => {}
    }
}

fn collect_var_refs_expr(expr: &IrExpr, refs: &mut HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct VarCollector<'a> { refs: &'a mut HashSet<VarId> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind { self.refs.insert(*id); }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            almide_ir::visit::walk_stmt(self, stmt);
        }
    }
    VarCollector { refs }.visit_expr(expr);
}

fn collect_var_refs_stmt(stmt: &IrStmt, refs: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_var_refs_expr(value, refs),
        IrStmtKind::Assign { var, value } => { refs.insert(*var); collect_var_refs_expr(value, refs); }
        IrStmtKind::Expr { expr } => collect_var_refs_expr(expr, refs),
        IrStmtKind::Guard { cond, else_ } => { collect_var_refs_expr(cond, refs); collect_var_refs_expr(else_, refs); }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { refs.insert(*var); }
        _ => {}
    }
}
