//! PeepholePass: detect idiomatic list-operation patterns and replace with
//! specialized IR nodes (ListSwap, ListReverse, ListRotateLeft, ListCopySlice).
//!
//! Target: all targets (target-independent optimization on IR).
//! Runs AFTER CloneInsertionPass so it sees the final ownership structure.

use almide_ir::*;
use almide_ir::visit_mut::{IrMutVisitor, walk_expr_mut};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct PeepholePass;

impl NanoPass for PeepholePass {
    fn name(&self) -> &str { "Peephole" }
    fn targets(&self) -> Option<Vec<Target>> { None } // all targets
    // #559: now enforceable target-conditionally — CloneInsertion is Rust-only,
    // so on the wasm arm (where it is absent) the edge is vacuous, not a panic.
    fn depends_on(&self) -> Vec<&'static str> { vec!["CloneInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = Peephole { changed: false };
        for func in &mut program.functions {
            v.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            v.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                v.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                v.visit_expr_mut(&mut tl.value);
            }
        }
        PassResult { program, changed: v.changed }
    }
}

/// Post-order peephole rewriter.
///
/// Child recursion goes through the canonical, wildcard-free `walk_expr_mut`, so
/// the per-expression fusion/copy-loop rewrites are reached inside *every* node
/// kind (Record fields, `Try`/`Clone` wrappers, map literals, …). A partial
/// hand-rolled `match … { _ => {} }` would silently skip the unlisted kinds and
/// drop their subtrees — the native↔WASM divergence class.
///
/// The only kinds handled explicitly are those carrying a `Vec<IrStmt>`
/// (`Block` / `ForIn` / `While`): they route their statement vector through
/// `rewrite_stmts` so the cross-statement sequence detectors (vec-init / swap /
/// reverse / rotate idioms) keep running. Those arms early-return from the match
/// (no `walk_expr_mut`) so the statement bodies are visited exactly once.
struct Peephole {
    changed: bool,
}

impl IrMutVisitor for Peephole {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::Block { stmts, expr: tail } => {
                self.rewrite_stmts(stmts);
                if let Some(e) = tail { self.visit_expr_mut(e); }
            }
            IrExprKind::ForIn { iterable, body, .. } => {
                self.visit_expr_mut(iterable);
                self.rewrite_stmts(body);
            }
            IrExprKind::While { cond, body } => {
                self.visit_expr_mut(cond);
                self.rewrite_stmts(body);
            }
            // Every other kind: exhaustive child recursion via the IR visitor —
            // any future variant is traversed automatically.
            _ => walk_expr_mut(self, expr),
        }

        self.local_rewrite(expr);
    }
}

impl Peephole {
    /// Apply the single-expression peephole rewrites to `expr` after its children
    /// have already been rewritten (post-order). Sets `self.changed` on a hit.
    fn local_rewrite(&mut self, expr: &mut IrExpr) {
        // ── Fusion: unwrap_or(map.get(m, k), default) → map.get_or(m, k, default) ──
        if self.try_fuse_map_get_or(expr) { return; }
        // Detect: for i in 0..n { xs[i] = ys[i] } → ListCopySlice
        self.try_rewrite_copy_loop(expr);
    }

    /// `UnwrapOr` fusion check of `local_rewrite`, extracted verbatim
    /// (cog>30 decomposition, pattern 1 — independent "try this rewrite,
    /// return whether it fired" checks with no state shared between them
    /// other than `self.changed`, which only the firing check writes).
    /// Eliminates heap allocation for Option return in the common `??`
    /// pattern. Returns `true` iff `expr` was rewritten (both the `Call`
    /// and post-`IntrinsicLowering` `RuntimeCall` forms of `map.get`).
    fn try_fuse_map_get_or(&mut self, expr: &mut IrExpr) -> bool {
        let IrExprKind::UnwrapOr { expr: inner, fallback } = &expr.kind else { return false };
        if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &inner.kind {
            if module.as_str() == "map" && func.as_str() == "get" && args.len() == 2 {
                let mut new_args = args.clone();
                new_args.push(*fallback.clone());
                let ret_ty = expr.ty.clone();
                *expr = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: almide_base::intern::sym("map"),
                            func: almide_base::intern::sym("get_or"),
                            def_id: None,
                        },
                        args: new_args,
                        type_args: vec![],
                    },
                    ty: ret_ty,
                    span: expr.span,
                    def_id: None,
                };
                self.changed = true;
                return true;
            }
        }
        // Also handle RuntimeCall form (post-IntrinsicLowering)
        if let IrExprKind::RuntimeCall { symbol, args } = &inner.kind {
            let s = symbol.as_str();
            if (s == "almide_rt_map_get" || s.contains("map_get")) && !s.contains("get_or") && args.len() == 2 {
                let mut new_args = args.clone();
                new_args.push(*fallback.clone());
                let ret_ty = expr.ty.clone();
                *expr = IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: almide_base::intern::sym("map"),
                            func: almide_base::intern::sym("get_or"),
                            def_id: None,
                        },
                        args: new_args,
                        type_args: vec![],
                    },
                    ty: ret_ty,
                    span: expr.span,
                    def_id: None,
                };
                self.changed = true;
                return true;
            }
        }
        false
    }

    /// `ForIn` → `ListCopySlice` detection check of `local_rewrite`,
    /// extracted verbatim (cog>30 decomposition).
    fn try_rewrite_copy_loop(&mut self, expr: &mut IrExpr) {
        let IrExprKind::ForIn { var, var_tuple, iterable, body } = &expr.kind else { return };
        if var_tuple.is_none() && body.len() == 1 {
            if let Some(copy) = try_detect_copy_loop(*var, iterable, &body[0]) {
                *expr = copy;
                self.changed = true;
            }
        }
    }

    /// Cross-statement sequence analysis. Recurses each statement's sub-exprs
    /// through `visit_expr_mut` (so the per-expr rewrites still fire inside
    /// statements), then collapses the recognized multi-statement idioms.
    fn rewrite_stmts(&mut self, stmts: &mut Vec<IrStmt>) {
        for stmt in stmts.iter_mut() {
            self.visit_stmt_exprs(stmt);
        }
        *stmts = self.collapse_idioms(std::mem::take(stmts));
    }

    /// Recurse the per-expr rewrites into one statement's sub-expressions.
    /// Grouped by how many expressions the statement carries.
    fn visit_stmt_exprs(&mut self, stmt: &mut IrStmt) {
        match &mut stmt.kind {
            // One sub-expression.
            IrStmtKind::Bind { value: e, .. } | IrStmtKind::BindDestructure { value: e, .. }
            | IrStmtKind::Assign { value: e, .. } | IrStmtKind::FieldAssign { value: e, .. }
            | IrStmtKind::Expr { expr: e }
            | IrStmtKind::ListReverse { end: e, .. } | IrStmtKind::ListRotateLeft { end: e, .. }
            | IrStmtKind::ListCopySlice { len: e, .. } => self.visit_expr_mut(e),
            // Two sub-expressions, left to right.
            IrStmtKind::IndexAssign { index: a, value: b, .. }
            | IrStmtKind::MapInsert { key: a, value: b, .. }
            | IrStmtKind::Guard { cond: a, else_: b }
            | IrStmtKind::ListSwap { a, b, .. } => {
                self.visit_expr_mut(a);
                self.visit_expr_mut(b);
            }
            // No recursable sub-exprs (Comment / RcInc / RcDec).
            IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
        }
    }

    /// The multi-statement peephole: scan the list left to right, replacing any
    /// recognized idiom with its collapsed form and dropping self-assignments.
    fn collapse_idioms(&mut self, orig: Vec<IrStmt>) -> Vec<IrStmt> {
        let len = orig.len();
        let mut result = Vec::with_capacity(len);
        let mut i = 0;
        while i < len {
            if let Some(s) = self.try_three_stmt_idiom(&orig, i) {
                result.push(s);
                i += 3;
                continue;
            }
            if is_self_assign(&orig[i]) {
                i += 1;
                self.changed = true;
                continue;
            }
            result.push(orig[i].clone());
            i += 1;
        }
        result
    }

    /// The three-statement idioms, tried in a fixed order at position `i`.
    fn try_three_stmt_idiom(&mut self, stmts: &[IrStmt], i: usize) -> Option<IrStmt> {
        let (a, b, c) = (stmts.get(i)?, stmts.get(i + 1)?, stmts.get(i + 2)?);
        let hit = try_detect_vec_init(a, b, c)
            .or_else(|| try_detect_reverse_block(a, b, c))
            .or_else(|| try_detect_rotate(a, b, c))
            .or_else(|| try_detect_swap(a, b, c))?;
        self.changed = true;
        Some(hit)
    }
}

/// `x = x` / `x = x.clone()` — a no-op the earlier passes can leave behind.
fn is_self_assign(stmt: &IrStmt) -> bool {
    let IrStmtKind::Assign { var, value } = &stmt.kind else { return false };
    match &value.kind {
        IrExprKind::Var { id } => id == var,
        IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if id == var),
        _ => false,
    }
}

// ── Pattern detectors ──────────────────────────────────────────

/// `x = x + licm` — the accumulator append this idiom collapses. Either side
/// may be wrapped in a `Clone` inserted by CloneInsertion.
fn is_self_append(stmt: &IrStmt, x_var: &VarId, licm_var: &VarId) -> bool {
    let IrStmtKind::Assign { var: assign_var, value } = &stmt.kind else { return false };
    if assign_var != x_var {
        return false;
    }
    let IrExprKind::BinOp { op: BinOp::ConcatList, left, right } = &value.kind else { return false };
    read_var(left) == Some(*x_var) && read_var(right) == Some(*licm_var)
}

/// The variable a `Var` — or a `Clone` of one — reads.
fn read_var(expr: &IrExpr) -> Option<VarId> {
    match &expr.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Clone { expr } => match &expr.kind {
            IrExprKind::Var { id } => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

/// Vec init: `var x = []; let __licm = [val]; for _ in 0..n { x = x + __licm }`
/// → `var x = vec![val; n]` (O(n) instead of O(n²))
fn try_detect_vec_init(s1: &IrStmt, s2: &IrStmt, s3: &IrStmt) -> Option<IrStmt> {
    // s1: Bind { var: x, mutability: Var, value: List { [] } }
    let IrStmtKind::Bind { var: x_var, mutability: Mutability::Var, value: init_val, ty } = &s1.kind else { return None; };
    let IrExprKind::List { elements } = &init_val.kind else { return None; };
    if !elements.is_empty() { return None; }

    // s2: Bind { var: __licm, value: List { [val] } } OR value: Clone { List { [val] } }
    let IrStmtKind::Bind { var: licm_var, value: licm_val, .. } = &s2.kind else { return None; };
    let single_val = match &licm_val.kind {
        IrExprKind::List { elements } if elements.len() == 1 => &elements[0],
        _ => return None,
    };

    // s3: Expr { ForIn { var: _, iterable: Range { 0, n }, body: [Assign { var: x, value: Concat(x, __licm) }] } }
    let IrStmtKind::Expr { expr: for_expr } = &s3.kind else { return None; };
    let IrExprKind::ForIn { iterable, body, .. } = &for_expr.kind else { return None; };
    let IrExprKind::Range { start, end, inclusive: false } = &iterable.kind else { return None; };
    if !matches!(&start.kind, IrExprKind::LitInt { value: 0 }) { return None; }
    if body.len() != 1 { return None; }

    // body[0]: Assign { var: x, value: BinOp { ConcatList, Clone(x), Clone(__licm) } }
    if !is_self_append(&body[0], x_var, licm_var) {
        return None;
    }

    // Match! Replace with: Bind { var: x, value: RenderedCall { "vec![val; n as usize]" } }
    // We can't render here, so use a Call to a synthetic runtime function
    // Better: use List { elements } with n copies... no, that's not possible at IR level.
    // Use RenderedCall as a placeholder that the walker outputs verbatim.
    // But we need to render `val` and `n`. Use a hack: store them in the RenderedCall.
    // Actually, cleanest: emit `(0..n).map(|_| val).collect::<Vec<_>>()`  via IterChain!

    // Emit list.repeat(val, n) — target-agnostic, StdlibLowering handles Rust vs WASM
    let repeat_expr = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_base::intern::sym("list"),
                func: almide_base::intern::sym("repeat"),
                def_id: None,
            },
            args: vec![single_val.clone(), (**end).clone()],
            type_args: vec![],
        },
        ty: ty.clone(),
        span: s1.span, def_id: None,
    };

    Some(IrStmt {
        kind: IrStmtKind::Bind {
            var: *x_var,
            mutability: Mutability::Var,
            ty: ty.clone(),
            value: repeat_expr,
        },
        span: s1.span,
    })
}

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
    if !is_lt_of(cond, lo_id, hi_id) {
        return None;
    }
    if body.len() != 5 {
        return None;
    }
    // body[3] / body[4]: the cursors close in by one per iteration.
    if !is_step_by_one(&body[3], lo_id, BinOp::AddInt) {
        return None;
    }
    if !is_step_by_one(&body[4], hi_id, BinOp::SubInt) {
        return None;
    }
    // body[0..3]: the three-statement swap of xs[lo] and xs[hi].
    let xs_id = swapped_list(&body[0..3], lo_id, hi_id)?;

    Some(IrStmt {
        kind: IrStmtKind::ListReverse { target: xs_id, end: hi_val.clone() },
        span: s1.span,
    })
}

/// `lo < hi`, with exactly those two variables on those two sides.
fn is_lt_of(cond: &IrExpr, lo_id: &VarId, hi_id: &VarId) -> bool {
    let IrExprKind::BinOp { op: BinOp::Lt, left, right } = &cond.kind else { return false };
    matches!(&left.kind, IrExprKind::Var { id } if id == lo_id)
        && matches!(&right.kind, IrExprKind::Var { id } if id == hi_id)
}

/// `v = v ± 1` for the given variable and direction.
fn is_step_by_one(stmt: &IrStmt, v: &VarId, op: BinOp) -> bool {
    let IrStmtKind::Assign { var, value } = &stmt.kind else { return false };
    if var != v {
        return false;
    }
    let IrExprKind::BinOp { op: found, left, right } = &value.kind else { return false };
    *found == op
        && matches!(&left.kind, IrExprKind::Var { id } if id == v)
        && matches!(&right.kind, IrExprKind::LitInt { value: 1 })
}

/// The three-statement `xs[lo] ⇄ xs[hi]` swap:
///
/// ```text
/// let tmp = xs[lo]; xs[lo] = xs[hi]; xs[hi] = tmp
/// ```
///
/// Returns the list being swapped, which must be the same binding throughout.
fn swapped_list(stmts: &[IrStmt], lo_id: &VarId, hi_id: &VarId) -> Option<VarId> {
    let IrStmtKind::Bind { var: tmp_var, value: bind_val, .. } = &stmts[0].kind else { return None; };
    let IrExprKind::IndexAccess { object, index: swap_lo } = &bind_val.kind else { return None; };
    let IrExprKind::Var { id: xs_id } = &object.kind else { return None; };
    if !matches!(&swap_lo.kind, IrExprKind::Var { id } if id == lo_id) { return None; }

    let IrStmtKind::IndexAssign { target: xs2, index: a_lo, value: a_val } = &stmts[1].kind else { return None; };
    if xs2 != xs_id { return None; }
    if !matches!(&a_lo.kind, IrExprKind::Var { id } if id == lo_id) { return None; }
    let IrExprKind::IndexAccess { object: o2, index: a_hi } = &a_val.kind else { return None; };
    if !matches!(&o2.kind, IrExprKind::Var { id } if id == xs_id) { return None; }
    if !matches!(&a_hi.kind, IrExprKind::Var { id } if id == hi_id) { return None; }

    let IrStmtKind::IndexAssign { target: xs3, index: b_hi, value: tmp_val } = &stmts[2].kind else { return None; };
    if xs3 != xs_id { return None; }
    if !matches!(&b_hi.kind, IrExprKind::Var { id } if id == hi_id) { return None; }
    if !matches!(&tmp_val.kind, IrExprKind::Var { id } if id == tmp_var) { return None; }

    Some(*xs_id)
}

/// `for i in 0..end { <one statement> }` as a statement — the loop shape both
/// the rotate and the copy idioms are built on. Returns the loop variable, the
/// exclusive end, and the single-statement body.
fn simple_zero_range_loop(stmt: &IrStmt) -> Option<(VarId, &IrExpr, &[IrStmt])> {
    let IrStmtKind::Expr { expr: for_expr } = &stmt.kind else { return None };
    let IrExprKind::ForIn { var, iterable, body, var_tuple } = &for_expr.kind else { return None };
    if var_tuple.is_some() || body.len() != 1 {
        return None;
    }
    let IrExprKind::Range { start, end, inclusive: false } = &iterable.kind else { return None };
    if !matches!(&start.kind, IrExprKind::LitInt { value: 0 }) {
        return None;
    }
    Some((*var, end, body))
}

/// `xs[i] = xs[i + 1]` — one step of the rotate-left shift.
fn is_shift_left_by_one(stmt: &IrStmt, xs_id: &VarId, loop_var: &VarId) -> bool {
    let IrStmtKind::IndexAssign { target, index, value } = &stmt.kind else { return false };
    if target != xs_id || !matches!(&index.kind, IrExprKind::Var { id } if id == loop_var) {
        return false;
    }
    let IrExprKind::IndexAccess { object, index: plus1 } = &value.kind else { return false };
    if !matches!(&object.kind, IrExprKind::Var { id } if id == xs_id) {
        return false;
    }
    let IrExprKind::BinOp { op: BinOp::AddInt, left, right } = &plus1.kind else { return false };
    matches!(&left.kind, IrExprKind::Var { id } if id == loop_var)
        && matches!(&right.kind, IrExprKind::LitInt { value: 1 })
}

/// rotate: p0=xs[0]; for i in 0..r { xs[i]=xs[i+1] }; xs[r]=p0
fn try_detect_rotate(s1: &IrStmt, s2: &IrStmt, s3: &IrStmt) -> Option<IrStmt> {
    let IrStmtKind::Bind { var: p0_var, value: bind_val, .. } = &s1.kind else { return None; };
    let IrExprKind::IndexAccess { object: obj1, index: idx0 } = &bind_val.kind else { return None; };
    let IrExprKind::Var { id: xs_id } = &obj1.kind else { return None; };
    let IrExprKind::LitInt { value: 0 } = &idx0.kind else { return None; };

    let (loop_var, end, body) = simple_zero_range_loop(s2)?;
    if !is_shift_left_by_one(&body[0], xs_id, &loop_var) {
        return None;
    }

    let IrStmtKind::IndexAssign { target: xs3, index: r_idx, value: p0_val } = &s3.kind else { return None; };
    if xs3 != xs_id { return None; }
    if !matches!(&p0_val.kind, IrExprKind::Var { id } if id == p0_var) { return None; }

    // r_idx should match end
    if format!("{:?}", r_idx) != format!("{:?}", end) { return None; }

    Some(IrStmt {
        kind: IrStmtKind::ListRotateLeft { target: *xs_id, end: end.clone() },
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
            expr: Some(Box::new(IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None, def_id: None })),
        },
        ty: almide_lang::types::Ty::Unit,
        span: None, def_id: None,
    })
}
