//! PeepholePass: detect idiomatic list-operation patterns and replace with
//! specialized IR nodes (ListSwap, ListReverse, ListRotateLeft, ListCopySlice).
//!
//! Target: all targets (target-independent optimization on IR).
//! Runs AFTER CloneInsertionPass so it sees the final ownership structure.

use crate::ir::*;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct PeepholePass;

impl NanoPass for PeepholePass {
    fn name(&self) -> &str { "Peephole" }
    fn targets(&self) -> Option<Vec<Target>> { None } // all targets
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if rewrite_expr(&mut func.body) { changed = true; }
        }
        for tl in &mut program.top_lets {
            if rewrite_expr(&mut tl.value) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if rewrite_expr(&mut func.body) { changed = true; }
            }
            for tl in &mut module.top_lets {
                if rewrite_expr(&mut tl.value) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

fn rewrite_expr(expr: &mut IrExpr) -> bool {
    let mut changed = false;
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            if rewrite_stmts(stmts) { changed = true; }
            if let Some(e) = tail { if rewrite_expr(e) { changed = true; } }
        }
        IrExprKind::If { cond, then, else_ } => {
            if rewrite_expr(cond) { changed = true; }
            if rewrite_expr(then) { changed = true; }
            if rewrite_expr(else_) { changed = true; }
        }
        IrExprKind::Match { subject, arms } => {
            if rewrite_expr(subject) { changed = true; }
            for arm in arms {
                if let Some(g) = &mut arm.guard { if rewrite_expr(g) { changed = true; } }
                if rewrite_expr(&mut arm.body) { changed = true; }
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            if rewrite_expr(iterable) { changed = true; }
            if rewrite_stmts(body) { changed = true; }
        }
        IrExprKind::While { cond, body } => {
            if rewrite_expr(cond) { changed = true; }
            if rewrite_stmts(body) { changed = true; }
        }
        IrExprKind::Lambda { body, .. } => {
            if rewrite_expr(body) { changed = true; }
        }
        _ => {}
    }

    // Detect: for i in 0..n { xs[i] = ys[i] } → ListCopySlice
    if let IrExprKind::ForIn { var, var_tuple, iterable, body } = &expr.kind {
        if var_tuple.is_none() && body.len() == 1 {
            if let Some(copy) = try_detect_copy_loop(*var, iterable, &body[0]) {
                *expr = copy;
                return true;
            }
        }
    }

    changed
}

fn rewrite_stmts(stmts: &mut Vec<IrStmt>) -> bool {
    // First recurse into sub-exprs of each stmt
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match &mut stmt.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
            | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
                if rewrite_expr(value) { changed = true; }
            }
            IrStmtKind::IndexAssign { index, value, .. } => {
                if rewrite_expr(index) { changed = true; }
                if rewrite_expr(value) { changed = true; }
            }
            IrStmtKind::MapInsert { key, value, .. } => {
                if rewrite_expr(key) { changed = true; }
                if rewrite_expr(value) { changed = true; }
            }
            IrStmtKind::Guard { cond, else_ } => {
                if rewrite_expr(cond) { changed = true; }
                if rewrite_expr(else_) { changed = true; }
            }
            IrStmtKind::Expr { expr } => {
                if rewrite_expr(expr) { changed = true; }
            }
            IrStmtKind::ListSwap { a, b, .. } => {
                if rewrite_expr(a) { changed = true; }
                if rewrite_expr(b) { changed = true; }
            }
            IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
                if rewrite_expr(end) { changed = true; }
            }
            IrStmtKind::ListCopySlice { len, .. } => {
                if rewrite_expr(len) { changed = true; }
            }
            _ => {}
        }
    }

    // Multi-stmt peephole: work on indices, collect results
    let orig = std::mem::take(stmts);
    let mut result = Vec::with_capacity(orig.len());
    let len = orig.len();
    // Convert to indexable slice, consume via into_iter at the end
    let slice = &orig;
    let mut i = 0;
    while i < len {
        // 3-stmt patterns
        if i + 2 < len {
            if let Some(s) = try_detect_reverse_block(&slice[i], &slice[i + 1], &slice[i + 2]) {
                result.push(s); i += 3; changed = true; continue;
            }
            if let Some(s) = try_detect_rotate(&slice[i], &slice[i + 1], &slice[i + 2]) {
                result.push(s); i += 3; changed = true; continue;
            }
            if let Some(s) = try_detect_swap(&slice[i], &slice[i + 1], &slice[i + 2]) {
                result.push(s); i += 3; changed = true; continue;
            }
        }

        // Self-assignment elimination
        if let IrStmtKind::Assign { var, value } = &slice[i].kind {
            let is_self = match &value.kind {
                IrExprKind::Var { id } => id == var,
                IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if id == var),
                _ => false,
            };
            if is_self { i += 1; changed = true; continue; }
        }

        result.push(orig[i].clone());
        i += 1;
    }

    *stmts = result;
    changed
}

// ── Pattern detectors ──────────────────────────────────────────

/// swap: let tmp = xs[a]; xs[a] = xs[b]; xs[b] = tmp
fn try_detect_swap(s1: &IrStmt, s2: &IrStmt, s3: &IrStmt) -> Option<IrStmt> {
    let IrStmtKind::Bind { var: tmp_var, value: bind_val, .. } = &s1.kind else { return None; };
    let IrExprKind::IndexAccess { object: obj1, index: idx_a } = &bind_val.kind else { return None; };
    let IrExprKind::Var { id: xs_id } = &obj1.kind else { return None; };

    let IrStmtKind::IndexAssign { target: xs2, index: idx_a2, value: assign_val } = &s2.kind else { return None; };
    if xs2 != xs_id { return None; }
    let IrExprKind::IndexAccess { object: obj2, index: idx_b } = &assign_val.kind else { return None; };
    let IrExprKind::Var { id: xs3 } = &obj2.kind else { return None; };
    if xs3 != xs_id { return None; }

    let IrStmtKind::IndexAssign { target: xs4, index: idx_b2, value: tmp_val } = &s3.kind else { return None; };
    if xs4 != xs_id { return None; }
    let IrExprKind::Var { id: tmp_id } = &tmp_val.kind else { return None; };
    if tmp_id != tmp_var { return None; }

    // Verify index structural equality via Debug
    if format!("{:?}", idx_a) != format!("{:?}", idx_a2) { return None; }
    if format!("{:?}", idx_b) != format!("{:?}", idx_b2) { return None; }

    Some(IrStmt {
        kind: IrStmtKind::ListSwap { target: *xs_id, a: (**idx_a).clone(), b: (**idx_b).clone() },
        span: s1.span,
    })
}

/// reverse block: var lo=0; var hi=end; while(lo<hi) { swap(xs,lo,hi); lo++; hi-- }
fn try_detect_reverse_block(s1: &IrStmt, s2: &IrStmt, s3: &IrStmt) -> Option<IrStmt> {
    let IrStmtKind::Bind { var: lo_id, value: lo_val, mutability: Mutability::Var, .. } = &s1.kind else { return None; };
    let IrExprKind::LitInt { value: 0 } = &lo_val.kind else { return None; };

    let IrStmtKind::Bind { var: hi_id, value: hi_val, mutability: Mutability::Var, .. } = &s2.kind else { return None; };

    let IrStmtKind::Expr { expr: while_expr } = &s3.kind else { return None; };
    let IrExprKind::While { cond, body } = &while_expr.kind else { return None; };

    let IrExprKind::BinOp { op: BinOp::Lt, left, right } = &cond.kind else { return None; };
    if !matches!(&left.kind, IrExprKind::Var { id } if id == lo_id) { return None; }
    if !matches!(&right.kind, IrExprKind::Var { id } if id == hi_id) { return None; }

    if body.len() != 5 { return None; }

    // body[3]: lo = lo + 1
    let IrStmtKind::Assign { var: inc_var, value: inc_val } = &body[3].kind else { return None; };
    if inc_var != lo_id { return None; }
    let IrExprKind::BinOp { op: BinOp::AddInt, left: il, right: ir } = &inc_val.kind else { return None; };
    if !matches!(&il.kind, IrExprKind::Var { id } if id == lo_id) { return None; }
    if !matches!(&ir.kind, IrExprKind::LitInt { value: 1 }) { return None; }

    // body[4]: hi = hi - 1
    let IrStmtKind::Assign { var: dec_var, value: dec_val } = &body[4].kind else { return None; };
    if dec_var != hi_id { return None; }
    let IrExprKind::BinOp { op: BinOp::SubInt, left: dl, right: dr } = &dec_val.kind else { return None; };
    if !matches!(&dl.kind, IrExprKind::Var { id } if id == hi_id) { return None; }
    if !matches!(&dr.kind, IrExprKind::LitInt { value: 1 }) { return None; }

    // body[0..3]: swap pattern
    let IrStmtKind::Bind { var: tmp_var, value: bind_val, .. } = &body[0].kind else { return None; };
    let IrExprKind::IndexAccess { object, index: swap_lo } = &bind_val.kind else { return None; };
    let IrExprKind::Var { id: xs_id } = &object.kind else { return None; };
    if !matches!(&swap_lo.kind, IrExprKind::Var { id } if id == lo_id) { return None; }

    let IrStmtKind::IndexAssign { target: xs2, index: a_lo, value: a_val } = &body[1].kind else { return None; };
    if xs2 != xs_id { return None; }
    if !matches!(&a_lo.kind, IrExprKind::Var { id } if id == lo_id) { return None; }
    let IrExprKind::IndexAccess { object: o2, index: a_hi } = &a_val.kind else { return None; };
    if !matches!(&o2.kind, IrExprKind::Var { id } if id == xs_id) { return None; }
    if !matches!(&a_hi.kind, IrExprKind::Var { id } if id == hi_id) { return None; }

    let IrStmtKind::IndexAssign { target: xs3, index: b_hi, value: tmp_val } = &body[2].kind else { return None; };
    if xs3 != xs_id { return None; }
    if !matches!(&b_hi.kind, IrExprKind::Var { id } if id == hi_id) { return None; }
    if !matches!(&tmp_val.kind, IrExprKind::Var { id } if id == tmp_var) { return None; }

    Some(IrStmt {
        kind: IrStmtKind::ListReverse { target: *xs_id, end: hi_val.clone() },
        span: s1.span,
    })
}

/// rotate: p0=xs[0]; for i in 0..r { xs[i]=xs[i+1] }; xs[r]=p0
fn try_detect_rotate(s1: &IrStmt, s2: &IrStmt, s3: &IrStmt) -> Option<IrStmt> {
    let IrStmtKind::Bind { var: p0_var, value: bind_val, .. } = &s1.kind else { return None; };
    let IrExprKind::IndexAccess { object: obj1, index: idx0 } = &bind_val.kind else { return None; };
    let IrExprKind::Var { id: xs_id } = &obj1.kind else { return None; };
    let IrExprKind::LitInt { value: 0 } = &idx0.kind else { return None; };

    let IrStmtKind::Expr { expr: for_expr } = &s2.kind else { return None; };
    let IrExprKind::ForIn { var: loop_var, iterable, body, var_tuple } = &for_expr.kind else { return None; };
    if var_tuple.is_some() { return None; }
    let IrExprKind::Range { start, end, inclusive } = &iterable.kind else { return None; };
    if *inclusive { return None; }
    if !matches!(&start.kind, IrExprKind::LitInt { value: 0 }) { return None; }
    if body.len() != 1 { return None; }

    let IrStmtKind::IndexAssign { target: xs2, index: assign_idx, value: assign_val } = &body[0].kind else { return None; };
    if xs2 != xs_id { return None; }
    if !matches!(&assign_idx.kind, IrExprKind::Var { id } if id == loop_var) { return None; }

    let IrExprKind::IndexAccess { object: obj2, index: plus1 } = &assign_val.kind else { return None; };
    if !matches!(&obj2.kind, IrExprKind::Var { id } if id == xs_id) { return None; }
    let IrExprKind::BinOp { op: BinOp::AddInt, left: pl, right: pr } = &plus1.kind else { return None; };
    if !matches!(&pl.kind, IrExprKind::Var { id } if id == loop_var) { return None; }
    if !matches!(&pr.kind, IrExprKind::LitInt { value: 1 }) { return None; }

    let IrStmtKind::IndexAssign { target: xs3, index: r_idx, value: p0_val } = &s3.kind else { return None; };
    if xs3 != xs_id { return None; }
    if !matches!(&p0_val.kind, IrExprKind::Var { id } if id == p0_var) { return None; }

    // r_idx should match end
    if format!("{:?}", r_idx) != format!("{:?}", end) { return None; }

    Some(IrStmt {
        kind: IrStmtKind::ListRotateLeft { target: *xs_id, end: (**end).clone() },
        span: s1.span,
    })
}

/// copy loop: for i in 0..n { xs[i] = ys[i] }
fn try_detect_copy_loop(loop_var: VarId, iterable: &IrExpr, body_stmt: &IrStmt) -> Option<IrExpr> {
    let IrExprKind::Range { start, end, inclusive } = &iterable.kind else { return None; };
    if *inclusive { return None; }
    if !matches!(&start.kind, IrExprKind::LitInt { value: 0 }) { return None; }

    let IrStmtKind::IndexAssign { target: xs_id, index, value } = &body_stmt.kind else { return None; };
    if !matches!(&index.kind, IrExprKind::Var { id } if *id == loop_var) { return None; }

    let IrExprKind::IndexAccess { object, index: val_idx } = &value.kind else { return None; };
    let IrExprKind::Var { id: ys_id } = &object.kind else { return None; };
    if !matches!(&val_idx.kind, IrExprKind::Var { id } if *id == loop_var) { return None; }
    if xs_id == ys_id { return None; }

    // Emit as a Block containing a single ListCopySlice stmt, returning Unit
    Some(IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![IrStmt {
                kind: IrStmtKind::ListCopySlice { dst: *xs_id, src: *ys_id, len: (**end).clone() },
                span: None,
            }],
            expr: Some(Box::new(IrExpr { kind: IrExprKind::Unit, ty: crate::types::Ty::Unit, span: None })),
        },
        ty: crate::types::Ty::Unit,
        span: None,
    })
}
