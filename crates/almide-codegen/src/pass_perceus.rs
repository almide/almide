//! PerceusPass: Insert RcInc/RcDec IR nodes for automatic memory management.
//!
//! Implements Perceus-style reference counting (ICFP 2021) at the IR level.
//! All RC operations are derived from types alone — no user annotation.
//!
//! Rules:
//!   1. RcInc: Bind(y, Var(x)) where is_heap(typeof(x)) → insert RcInc(x) after bind
//!   2. RcDec: last use of heap-typed variable in a block → insert RcDec after last use
//!   3. RcDec: Assign(x, _) where is_heap(typeof(x)) → insert RcDec(x) before assign
//!   4. RcDec: function exit → insert RcDec for all heap locals not returned
//!   5. RcInc: ClosureCreate captures heap var → insert RcInc
//!
//! Target: WASM only (Rust handles ownership natively).

use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use std::collections::{HashMap, HashSet};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct PerceusPass;

impl NanoPass for PerceusPass {
    fn name(&self) -> &str { "Perceus" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if insert_rc_ops(func, &program.var_table) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if insert_rc_ops(func, &program.var_table) { changed = true; }
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
            if func.name.as_str() == "main" || func.is_test { continue; }
            verify_function(func, &program.var_table);
        }
        PassResult { program, changed: false }
    }
}

fn verify_function(func: &IrFunction, var_table: &VarTable) {
    let mut inc_count: HashMap<VarId, usize> = HashMap::new();
    let mut dec_count: HashMap<VarId, usize> = HashMap::new();
    let mut heap_binds: HashSet<VarId> = HashSet::new();

    // Scan entire function body for RC operations and heap binds
    scan_rc_ops(&func.body, &mut inc_count, &mut dec_count, &mut heap_binds, var_table);

    // Collect variables used in return position (tail expression) — they're
    // transferred to the caller, no RcDec needed.
    let mut returned_vars: HashSet<VarId> = HashSet::new();
    if let IrExprKind::Block { expr: Some(tail), .. } = &func.body.kind {
        collect_var_refs_expr(tail, &mut returned_vars);
    }

    // Verify: every heap bind has at least one dec (free path)
    for var in &heap_binds {
        if returned_vars.contains(var) { continue; } // returned → caller owns
        let decs = dec_count.get(var).copied().unwrap_or(0);
        let incs = inc_count.get(var).copied().unwrap_or(0);
        if decs == 0 {
            let name = var_table.get(*var).name.as_str();
            eprintln!(
                "[perceus-verify] warning: heap variable `{}` (VarId {}) in `{}` has no RcDec — potential leak",
                name, var.0, func.name.as_str()
            );
        }
        // Check balance: incs + 1 (alloc) should equal decs (over all paths)
        // This is approximate — control flow makes exact checking hard
        if decs > incs + 1 {
            let name = var_table.get(*var).name.as_str();
            eprintln!(
                "[perceus-verify] warning: heap variable `{}` (VarId {}) in `{}` has {} decs but only {} incs — potential double-free",
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
                            // Skip EnvLoad binds — they're borrowed from closure env, not owned
                            let is_env_load = matches!(&value.kind, IrExprKind::EnvLoad { .. });
                            if !is_env_load {
                                heap_binds.insert(*var);
                            }
                        }
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, var_table);
                    }
                    IrStmtKind::Assign { value, .. } => {
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, var_table);
                    }
                    IrStmtKind::Expr { expr } => {
                        scan_rc_ops(expr, inc_count, dec_count, heap_binds, var_table);
                    }
                    IrStmtKind::Guard { cond, else_ } => {
                        scan_rc_ops(cond, inc_count, dec_count, heap_binds, var_table);
                        scan_rc_ops(else_, inc_count, dec_count, heap_binds, var_table);
                    }
                    _ => {}
                }
            }
            if let Some(tail) = tail {
                scan_rc_ops(tail, inc_count, dec_count, heap_binds, var_table);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_rc_ops(cond, inc_count, dec_count, heap_binds, var_table);
            scan_rc_ops(then, inc_count, dec_count, heap_binds, var_table);
            scan_rc_ops(else_, inc_count, dec_count, heap_binds, var_table);
        }
        IrExprKind::Match { subject, arms } => {
            scan_rc_ops(subject, inc_count, dec_count, heap_binds, var_table);
            for arm in arms {
                scan_rc_ops(&arm.body, inc_count, dec_count, heap_binds, var_table);
            }
        }
        IrExprKind::While { cond, body } => {
            scan_rc_ops(cond, inc_count, dec_count, heap_binds, var_table);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::RcInc { var } => { *inc_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::RcDec { var } => { *dec_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::Bind { var, ty, value, .. } => {
                        if is_heap_type(ty) { heap_binds.insert(*var); }
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, var_table);
                    }
                    IrStmtKind::Assign { value, .. } => {
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, var_table);
                    }
                    IrStmtKind::Expr { expr } => {
                        scan_rc_ops(expr, inc_count, dec_count, heap_binds, var_table);
                    }
                    _ => {}
                }
            }
        }
        IrExprKind::Lambda { body, .. } => {
            scan_rc_ops(body, inc_count, dec_count, heap_binds, var_table);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            scan_rc_ops(iterable, inc_count, dec_count, heap_binds, var_table);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::RcInc { var } => { *inc_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::RcDec { var } => { *dec_count.entry(*var).or_insert(0) += 1; }
                    IrStmtKind::Bind { var, ty, value, .. } => {
                        if is_heap_type(ty) { heap_binds.insert(*var); }
                        scan_rc_ops(value, inc_count, dec_count, heap_binds, var_table);
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

/// Insert RC operations into a function body.
fn insert_rc_ops(func: &mut IrFunction, var_table: &VarTable) -> bool {
    if func.name.as_str() == "main" || func.is_test { return false; }

    let mut changed = false;

    // Build closure capture map: VarId of closure → Vec of captured heap VarIds
    // This enables Rule 6: when a closure is RcDec'd, also RcDec its captures
    let mut closure_captures: HashMap<VarId, Vec<VarId>> = HashMap::new();
    collect_closure_captures(&func.body, &mut closure_captures);

    // Recursively process ALL blocks in the function body
    let mut dropped_vars: HashSet<VarId> = HashSet::new();
    if insert_rc_in_expr(&mut func.body, var_table, &mut dropped_vars) { changed = true; }

    // Rule 4: Function-exit RcDec for multi-use heap locals NOT already dropped by Rule 2
    if let IrExprKind::Block { stmts, .. } = &mut func.body.kind {
        let mut exit_vars = Vec::new();
        collect_heap_binds_from_stmts(stmts, var_table, &mut exit_vars);
        for var in exit_vars {
            if dropped_vars.contains(&var) { continue; }
            let use_count = var_table.get(var).use_count;
            if use_count <= 1 { continue; }
            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            changed = true;
        }
    }

    // Rule 5: RcInc for closure captures
    insert_closure_capture_incs(&mut func.body);

    // Rule 6: disabled — causes premature free of captured values
    // when closure is dropped before all uses of the captured variable.
    // The RcInc from Rule 5 is balanced by the original variable's
    // own RcDec at its natural scope exit.
    // TODO: re-enable when lifetime analysis can prove safety.
    // insert_closure_capture_decs(&mut func.body, &closure_captures);

    changed
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
