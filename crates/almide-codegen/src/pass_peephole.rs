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
        // First recurse into sub-exprs of each stmt
        for stmt in stmts.iter_mut() {
            match &mut stmt.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
                | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
                    self.visit_expr_mut(value);
                }
                IrStmtKind::IndexAssign { index, value, .. } => {
                    self.visit_expr_mut(index);
                    self.visit_expr_mut(value);
                }
                IrStmtKind::MapInsert { key, value, .. } => {
                    self.visit_expr_mut(key);
                    self.visit_expr_mut(value);
                }
                IrStmtKind::Guard { cond, else_ } => {
                    self.visit_expr_mut(cond);
                    self.visit_expr_mut(else_);
                }
                IrStmtKind::Expr { expr } => {
                    self.visit_expr_mut(expr);
                }
                IrStmtKind::ListSwap { a, b, .. } => {
                    self.visit_expr_mut(a);
                    self.visit_expr_mut(b);
                }
                IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
                    self.visit_expr_mut(end);
                }
                IrStmtKind::ListCopySlice { len, .. } => {
                    self.visit_expr_mut(len);
                }
                // No recursable sub-exprs (Comment / RcInc / RcDec).
                IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
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
                if let Some(s) = try_detect_vec_init(&slice[i], &slice[i + 1], &slice[i + 2]) {
                    result.push(s); i += 3; self.changed = true; continue;
                }
                if let Some(s) = try_detect_reverse_block(&slice[i], &slice[i + 1], &slice[i + 2]) {
                    result.push(s); i += 3; self.changed = true; continue;
                }
                if let Some(s) = try_detect_rotate(&slice[i], &slice[i + 1], &slice[i + 2]) {
                    result.push(s); i += 3; self.changed = true; continue;
                }
                if let Some(s) = try_detect_swap(&slice[i], &slice[i + 1], &slice[i + 2]) {
                    result.push(s); i += 3; self.changed = true; continue;
                }
            }

            // Self-assignment elimination
            if let IrStmtKind::Assign { var, value } = &slice[i].kind {
                let is_self = match &value.kind {
                    IrExprKind::Var { id } => id == var,
                    IrExprKind::Clone { expr } => matches!(&expr.kind, IrExprKind::Var { id } if id == var),
                    _ => false,
                };
                if is_self { i += 1; self.changed = true; continue; }
            }

            result.push(orig[i].clone());
            i += 1;
        }

        *stmts = result;
    }
}

// ── Pattern detectors ──────────────────────────────────────────

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
    let IrStmtKind::Assign { var: assign_var, value: assign_val } = &body[0].kind else { return None; };
    if assign_var != x_var { return None; }

    // Unwrap the concat: ConcatList(left, right) where left contains x and right contains __licm
    let (left, right) = match &assign_val.kind {
        IrExprKind::BinOp { op: BinOp::ConcatList, left, right } => (left.as_ref(), right.as_ref()),
        _ => return None,
    };

    // left should be Var(x) or Clone(Var(x))
    let left_var = match &left.kind {
        IrExprKind::Var { id } => *id,
        IrExprKind::Clone { expr } => match &expr.kind { IrExprKind::Var { id } => *id, _ => return None },
        _ => return None,
    };
    if left_var != *x_var { return None; }

    // right should be Var(__licm) or Clone(Var(__licm))
    let right_var = match &right.kind {
        IrExprKind::Var { id } => *id,
        IrExprKind::Clone { expr } => match &expr.kind { IrExprKind::Var { id } => *id, _ => return None },
        _ => return None,
    };
    if right_var != *licm_var { return None; }

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
            expr: Some(Box::new(IrExpr { kind: IrExprKind::Unit, ty: almide_lang::types::Ty::Unit, span: None, def_id: None })),
        },
        ty: almide_lang::types::Ty::Unit,
        span: None, def_id: None,
    })
}
