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

/// Eliminate redundant RcInc/RcDec pairs in a function.
fn eliminate_redundant_rc(func: &mut IrFunction, var_table: &VarTable) -> bool {
    if let IrExprKind::Block { stmts, .. } = &mut func.body.kind {
        eliminate_in_block(stmts, var_table)
    } else {
        false
    }
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

fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Unknown | Ty::Fn { .. })
}

/// Insert RC operations into a function body.
fn insert_rc_ops(func: &mut IrFunction, var_table: &VarTable) -> bool {
    if func.name.as_str() == "main" || func.is_test { return false; }

    let mut changed = false;

    // Recursively process ALL blocks in the function body
    if insert_rc_in_expr(&mut func.body, var_table) { changed = true; }

    // Rule 4: Function-exit RcDec for multi-use heap locals (top-level only)
    if let IrExprKind::Block { stmts, .. } = &mut func.body.kind {
        let mut exit_vars = Vec::new();
        collect_heap_binds_from_stmts(stmts, var_table, &mut exit_vars);
        for var in exit_vars {
            let use_count = var_table.get(var).use_count;
            if use_count <= 1 { continue; }
            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            changed = true;
        }
    }

    // Rule 5: RcInc for closure captures
    insert_closure_capture_incs(&mut func.body);

    changed
}

/// Recursively insert RC operations in all blocks within an expression.
fn insert_rc_in_expr(expr: &mut IrExpr, var_table: &VarTable) -> bool {
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

            // Rule 2: last-use drops
            insert_last_use_drops(stmts, tail.as_deref(), var_table);

            // Recurse into statement values and nested blocks
            for stmt in stmts.iter_mut() {
                match &mut stmt.kind {
                    IrStmtKind::Bind { value, .. } => { insert_rc_in_expr(value, var_table); }
                    IrStmtKind::Assign { value, .. } => { insert_rc_in_expr(value, var_table); }
                    IrStmtKind::Expr { expr } => { insert_rc_in_expr(expr, var_table); }
                    IrStmtKind::Guard { cond, else_ } => {
                        insert_rc_in_expr(cond, var_table);
                        insert_rc_in_expr(else_, var_table);
                    }
                    _ => {}
                }
            }
            if let Some(tail) = tail { insert_rc_in_expr(tail, var_table); }
        }
        IrExprKind::If { cond, then, else_ } => {
            insert_rc_in_expr(cond, var_table);
            insert_rc_in_expr(then, var_table);
            insert_rc_in_expr(else_, var_table);
        }
        IrExprKind::Match { subject, arms } => {
            insert_rc_in_expr(subject, var_table);
            for arm in arms {
                insert_rc_in_expr(&mut arm.body, var_table);
            }
        }
        IrExprKind::While { cond, body } => {
            insert_rc_in_expr(cond, var_table);
            // Apply Rule 1 (RcInc) and Rule 3 (Assign RcDec) to while body
            process_stmt_list(body, var_table);
        }
        IrExprKind::Lambda { body, .. } => { insert_rc_in_expr(body, var_table); }
        IrExprKind::ForIn { iterable, body, .. } => {
            insert_rc_in_expr(iterable, var_table);
            process_stmt_list(body, var_table);
        }
        _ => {}
    }
    changed
}

/// Rule 2: Insert RcDec after the last use of each heap-typed local in a block.
fn insert_last_use_drops(stmts: &mut Vec<IrStmt>, tail: Option<&IrExpr>, var_table: &VarTable) {
    // Collect heap-typed locals bound in this block
    let mut block_locals: HashMap<VarId, Ty> = HashMap::new();
    for stmt in stmts.iter() {
        if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
            if is_heap_type(ty) {
                block_locals.insert(*var, ty.clone());
            }
        }
    }
    if block_locals.is_empty() { return; }

    // Variables used in tail expression — don't drop (they're returned)
    let mut tail_vars: HashSet<VarId> = HashSet::new();
    if let Some(tail) = tail {
        collect_var_refs_expr(tail, &mut tail_vars);
    }

    // For each block-local, find the last statement that references it
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
                stmts.insert(idx + 1, IrStmt {
                    kind: IrStmtKind::RcDec { var: *var },
                    span: None,
                });
            }
        }
    }
}

/// Apply Rule 1 (RcInc for alias) and Rule 3 (RcDec for Assign) to a statement list.
/// Used for while/for-in bodies that aren't full Block expressions.
fn process_stmt_list(stmts: &mut Vec<IrStmt>, var_table: &VarTable) {
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
    // Recurse into statement values
    for stmt in stmts.iter_mut() {
        match &mut stmt.kind {
            IrStmtKind::Bind { value, .. } => { insert_rc_in_expr(value, var_table); }
            IrStmtKind::Assign { value, .. } => { insert_rc_in_expr(value, var_table); }
            IrStmtKind::Expr { expr } => { insert_rc_in_expr(expr, var_table); }
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
