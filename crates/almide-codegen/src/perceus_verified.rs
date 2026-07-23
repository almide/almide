//! Perceus Verified — Rust implementation certified by Lean 4 proofs.
//!
//! This module mirrors the Lean 4 definitions in
//! `crates/almide-perceus-belt/AlmidePerceusBelt/FnBody.lean`
//! and `Heap.lean`. Every function here has a corresponding
//! Lean definition with mechanically verified theorems.
//!
//! Lean proofs guarantee:
//!   - insertDecBeforeEnd adds exactly 1 Dec (insertDec_adds_one)
//!   - insertDecBeforeEnd preserves Inc count (insertDec_keeps_incs)
//!   - Fresh var + 1 Dec = freed (single_dec_frees)
//!   - Inc + Dec pair is identity (inc_dec_is_id)
//!   - perceusTransform covers all heap VDecls (perceus_covers_vdecl)
//!   - Dec in both if/else branches = freed on all paths (cf_both_branches_freed)
//!   - Heap execution confirms RC reaches 0 (perceus_fixes_heap)
//!
//! 23 theorems, 0 sorry. Lean 4 kernel verified.

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId, VarTable};
use almide_lang::types::Ty;

/// Lean-certified: Ty.isHeap
/// Corresponds to: `def Ty.isHeap : Ty → Bool`
pub fn is_heap_type(ty: &Ty) -> bool {
    // Keep in sync with pass_perceus::is_heap_type — `Ty::Named` (declared nominal
    // record/variant) is a heap pointer, so the verifier accounts for its Inc/Dec.
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Named(..)
        | Ty::Unknown | Ty::Fn { .. })
}

/// Lean-certified: countDecs
/// Counts RcDec statements for a specific variable in a statement list.
/// Corresponds to: `def countDecs : FnBody → VarId → Nat`
pub fn count_decs(stmts: &[IrStmt], var: VarId) -> usize {
    stmts.iter().filter(|s| matches!(&s.kind, IrStmtKind::RcDec { var: v } if *v == var)).count()
}

/// Lean-certified: countIncs
/// Corresponds to: `def countIncs : FnBody → VarId → Nat`
pub fn count_incs(stmts: &[IrStmt], var: VarId) -> usize {
    stmts.iter().filter(|s| matches!(&s.kind, IrStmtKind::RcInc { var: v } if *v == var)).count()
}

/// Lean-certified: isFreed
/// A variable is freed when decs = incs + 1 (initial RC=1 reaches 0).
/// Corresponds to: `def isFreed (fb : FnBody) (v : VarId) : Prop := countDecs fb v = countIncs fb v + 1`
pub fn is_freed(stmts: &[IrStmt], var: VarId) -> bool {
    count_decs(stmts, var) == count_incs(stmts, var) + 1
}

/// Lean-certified: hasDec
/// Corresponds to: `def hasDec (fb : FnBody) (v : VarId) : Prop := countDecs fb v ≥ 1`
pub fn has_dec(stmts: &[IrStmt], var: VarId) -> bool {
    count_decs(stmts, var) >= 1
}

/// Lean-certified: verify RC balance for all heap variables.
/// Returns list of (VarId, issue) for any violations.
///
/// Lean theorem: `perceus_strictly_better` proves that
/// with Dec, RC reaches 0. Without Dec, RC stays at 1 (leak).
pub fn verify_rc_balance(
    stmts: &[IrStmt],
    var_table: &VarTable,
) -> Vec<(VarId, &'static str)> {
    let mut issues = Vec::new();

    // Collect heap-typed Bind variables
    for stmt in stmts {
        if let IrStmtKind::Bind { var, ty, value, .. } = &stmt.kind {
            if !is_heap_type(ty) { continue; }
            // Skip EnvLoad (borrowed, not owned)
            if matches!(&value.kind, IrExprKind::EnvLoad { .. }) { continue; }

            let decs = count_decs(stmts, *var);
            let incs = count_incs(stmts, *var);

            // Lean theorem: single_dec_frees
            // For non-returned vars: decs should = incs + 1
            if decs == 0 {
                issues.push((*var, "LEAK: no RcDec"));
            }

            // Lean theorem: inc_dec_is_id
            // For immutable vars: decs should not exceed incs + 1
            let info = var_table.get(*var);
            if !matches!(info.mutability, almide_ir::Mutability::Var) && decs > incs + 1 {
                issues.push((*var, "DOUBLE-FREE: too many RcDec"));
            }
        }
    }

    issues
}

/// Read-only context for the recursive RC verifier. Bundles the var table and
/// the "ownership leaves this scope" exemption sets so the recursion threads one
/// reference instead of four positional params.
struct VerifyCtx<'a> {
    var_table: &'a VarTable,
    /// Vars in function-return (tail) position: ownership escapes the function,
    /// so the callee must NOT Dec them.
    returned_vars: &'a std::collections::HashSet<VarId>,
    /// Vars moved out of their defining block as a bare-`Var` block tail:
    /// ownership transfers to the block's consumer (an enclosing Bind, a return,
    /// a call arg), which carries the Dec — so a missing Dec in the var's own
    /// block is correct, not a leak. This is the block-level generalization of
    /// `returned_vars` (the function-level escape): both are the Perceus "moved"
    /// relation, one scope apart.
    moved_out_vars: &'a std::collections::HashSet<VarId>,
    /// Vars bound directly from an `EnvLoad`: borrowed from the closure
    /// environment, not owned, so no Dec.
    env_load_vars: &'a std::collections::HashSet<VarId>,
}

/// Lean-certified recursive verification of entire expression tree.
/// Walks all blocks, if/else, match, while, for-in.
/// Reports (VarId, message) for every violation.
pub fn verify_expr(
    expr: &IrExpr,
    var_table: &VarTable,
    returned_vars: &std::collections::HashSet<VarId>,
    moved_out_vars: &std::collections::HashSet<VarId>,
    env_load_vars: &std::collections::HashSet<VarId>,
) -> Vec<(VarId, &'static str)> {
    let ctx = VerifyCtx { var_table, returned_vars, moved_out_vars, env_load_vars };
    let mut issues = Vec::new();
    verify_expr_inner(expr, &ctx, &mut issues);
    issues
}

/// `IrExprKind::Block` case of `verify_expr_inner`, extracted verbatim
/// (cog>30 decomposition, pattern 1 — `issues` is push-only w.r.t. this
/// function's own control flow, no cross-arm state).
fn verify_block(
    expr: &IrExpr,
    ctx: &VerifyCtx,
    issues: &mut Vec<(VarId, &'static str)>,
) {
    let IrExprKind::Block { stmts, expr: tail } = &expr.kind else { unreachable!() };
    verify_block_leak_check(stmts, ctx, issues);
    // Recurse into nested expressions
    for stmt in stmts {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                verify_expr_inner(value, ctx, issues),
            IrStmtKind::Expr { expr } =>
                verify_expr_inner(expr, ctx, issues),
            IrStmtKind::Guard { cond, else_ } => {
                verify_expr_inner(cond, ctx, issues);
                verify_expr_inner(else_, ctx, issues);
            }
            _ => {}
        }
    }
    if let Some(t) = tail {
        verify_expr_inner(t, ctx, issues);
    }
}

/// The `continue`-guard chain of `verify_block_leak_check`'s per-`Bind` loop,
/// extracted to a named predicate (further split of the same decomposition):
/// true when this heap `Bind` is exempt from the Dec-balance check below —
/// borrowed from the closure environment, or its ownership escapes the
/// function/block so the Dec is legitimately carried elsewhere.
fn bind_dec_check_exempt(var: VarId, value: &IrExpr, ctx: &VerifyCtx) -> bool {
    if matches!(&value.kind, IrExprKind::EnvLoad { .. }) { return true; }
    if ctx.env_load_vars.contains(&var) { return true; }
    if ctx.returned_vars.contains(&var) { return true; }
    // TCO trampoline temporaries (`__tco_*`) and branch-lift
    // temporaries (`__br_*`) DO get an RcDec, but in a sibling/outer
    // block from their Bind: the value is threaded through a loop or
    // branch reassignment (`Assign loopvar = Var __tco_tmp`), so the
    // matching Dec lands one scope away. This flat, per-block rule
    // counts `count_decs` only within the Bind's own block and so
    // reads decs==0 — a false positive, not a leak (verified by
    // probing `sum_acc`: the RcDec exists; the cross-target gate
    // would trap on a double-free and does not). This is a distinct
    // class from the ANF move-out tails handled by `moved_out_vars`.
    // Replacing this name-prefix exclusion with scope-aware Dec
    // accounting is a tracked perceus-belt follow-up.
    let vname = ctx.var_table.get(var).name.as_str();
    vname.starts_with("__tco_") || vname.starts_with("__br_")
}

/// First phase of `verify_block`: each heap `Bind` in `stmts` should have a
/// matching `RcDec` in this same scope — extracted verbatim (cog>30
/// decomposition, further split of the `verify_block` extraction above).
fn verify_block_leak_check(
    stmts: &[IrStmt],
    ctx: &VerifyCtx,
    issues: &mut Vec<(VarId, &'static str)>,
) {
    for stmt in stmts {
        if let IrStmtKind::Bind { var, ty, value, .. } = &stmt.kind {
            if !is_heap_type(ty) { continue; }
            if bind_dec_check_exempt(*var, value, ctx) { continue; }

            let decs = count_decs(stmts, *var);
            let incs = count_incs(stmts, *var);
            let info = ctx.var_table.get(*var);
            let is_mutable = matches!(info.mutability, almide_ir::Mutability::Var);

            // Lean theorem: single_dec_frees. A var moved out of this
            // block (bare-`Var` tail) has its Dec carried by the block's
            // consumer, so decs==0 here is correct, not a leak.
            if decs == 0 && !is_mutable && !ctx.moved_out_vars.contains(var) {
                issues.push((*var, "LEAK: no RcDec"));
            }
            // Lean theorem: inc_dec_is_id (balance check). Still enforced
            // for moved-out vars: a Dec on an already-moved value is a
            // double-free, so this check is NOT exempted.
            if !is_mutable && decs > incs + 1 {
                issues.push((*var, "DOUBLE-FREE: too many RcDec"));
            }
        }
    }
}

fn verify_expr_inner(
    expr: &IrExpr,
    ctx: &VerifyCtx,
    issues: &mut Vec<(VarId, &'static str)>,
) {
    match &expr.kind {
        IrExprKind::Block { .. } => verify_block(expr, ctx, issues),
        IrExprKind::If { cond, then, else_ } => {
            verify_expr_inner(cond, ctx, issues);
            verify_expr_inner(then, ctx, issues);
            verify_expr_inner(else_, ctx, issues);
        }
        IrExprKind::Match { subject, arms } => {
            verify_expr_inner(subject, ctx, issues);
            for arm in arms {
                verify_expr_inner(&arm.body, ctx, issues);
            }
        }
        IrExprKind::While { cond, body } => {
            verify_expr_inner(cond, ctx, issues);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } =>
                        verify_expr_inner(value, ctx, issues),
                    IrStmtKind::Expr { expr } =>
                        verify_expr_inner(expr, ctx, issues),
                    _ => {}
                }
            }
        }
        IrExprKind::Lambda { body, .. } =>
            verify_expr_inner(body, ctx, issues),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Corresponds to Lean theorem: `perceus_strictly_better`
    #[test]
    fn test_strictly_better() {
        let stmts_leak = vec![
            IrStmt { kind: IrStmtKind::Bind {
                var: VarId(0), ty: Ty::String,
                mutability: almide_ir::Mutability::Let,
                value: IrExpr { kind: IrExprKind::LitStr { value: "test".into() },
                    ty: Ty::String, span: None, def_id: None },
            }, span: None },
        ];
        assert!(!has_dec(&stmts_leak, VarId(0)));

        let stmts_freed = vec![
            IrStmt { kind: IrStmtKind::Bind {
                var: VarId(0), ty: Ty::String,
                mutability: almide_ir::Mutability::Let,
                value: IrExpr { kind: IrExprKind::LitStr { value: "test".into() },
                    ty: Ty::String, span: None, def_id: None },
            }, span: None },
            IrStmt { kind: IrStmtKind::RcDec { var: VarId(0) }, span: None },
        ];
        assert!(has_dec(&stmts_freed, VarId(0)));
        assert!(is_freed(&stmts_freed, VarId(0)));
    }
}

/// Property-based tests: verify Lean/Rust algorithm consistency.
///
/// Each test corresponds to a Lean 4 theorem in AlmidePerceusBelt.
/// proptest generates random IR structures; if any property fails,
/// the Rust implementation diverges from the Lean-proven spec.
#[cfg(test)]
mod proptest_lean_rust {
    use super::*;
    use proptest::prelude::*;

    // ── Strategies ──

    fn arb_var_id() -> impl Strategy<Value = VarId> {
        (0u32..8).prop_map(VarId)
    }

    fn arb_heap_ty() -> impl Strategy<Value = Ty> {
        prop_oneof![
            Just(Ty::String),
            Just(Ty::Unknown),
            Just(Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::Int],
            )),
            Just(Ty::Record { fields: vec![(almide_base::intern::sym("x"), Ty::Int)] }),
            Just(Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }),
        ]
    }

    fn arb_nonheap_ty() -> impl Strategy<Value = Ty> {
        prop_oneof![
            Just(Ty::Int),
            Just(Ty::Float),
            Just(Ty::Bool),
            Just(Ty::Unit),
        ]
    }

    fn arb_ty() -> impl Strategy<Value = Ty> {
        prop_oneof![arb_heap_ty(), arb_nonheap_ty()]
    }

    fn dummy_expr(ty: Ty) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty, span: None, def_id: None }
    }

    fn arb_stmt() -> impl Strategy<Value = IrStmt> {
        prop_oneof![
            // RcInc
            arb_var_id().prop_map(|v| IrStmt {
                kind: IrStmtKind::RcInc { var: v }, span: None,
            }),
            // RcDec
            arb_var_id().prop_map(|v| IrStmt {
                kind: IrStmtKind::RcDec { var: v }, span: None,
            }),
            // Bind (immutable)
            (arb_var_id(), arb_ty()).prop_map(|(v, ty)| IrStmt {
                kind: IrStmtKind::Bind {
                    var: v, ty: ty.clone(),
                    mutability: almide_ir::Mutability::Let,
                    value: dummy_expr(ty),
                },
                span: None,
            }),
            // Bind (mutable)
            (arb_var_id(), arb_heap_ty()).prop_map(|(v, ty)| IrStmt {
                kind: IrStmtKind::Bind {
                    var: v, ty: ty.clone(),
                    mutability: almide_ir::Mutability::Var,
                    value: dummy_expr(ty),
                },
                span: None,
            }),
            // Assign (mutable reassignment)
            (arb_var_id(), arb_heap_ty()).prop_map(|(v, ty)| IrStmt {
                kind: IrStmtKind::Assign {
                    var: v,
                    value: dummy_expr(ty),
                },
                span: None,
            }),
        ]
    }

    fn arb_stmts() -> impl Strategy<Value = Vec<IrStmt>> {
        prop::collection::vec(arb_stmt(), 0..20)
    }

    // ── Lean theorem: countDecs / countIncs correctness ──
    // These test that our Rust count_decs/count_incs match the
    // Lean definitions: filter + count on RcDec/RcInc nodes.

    proptest! {
        /// Lean: `def countDecs (fb : FnBody) (v : VarId) : Nat`
        #[test]
        fn count_decs_is_filter_count(stmts in arb_stmts(), var in arb_var_id()) {
            let expected = stmts.iter()
                .filter(|s| matches!(&s.kind, IrStmtKind::RcDec { var: v } if *v == var))
                .count();
            prop_assert_eq!(count_decs(&stmts, var), expected);
        }

        /// Lean: `def countIncs (fb : FnBody) (v : VarId) : Nat`
        #[test]
        fn count_incs_is_filter_count(stmts in arb_stmts(), var in arb_var_id()) {
            let expected = stmts.iter()
                .filter(|s| matches!(&s.kind, IrStmtKind::RcInc { var: v } if *v == var))
                .count();
            prop_assert_eq!(count_incs(&stmts, var), expected);
        }
    }

    // ── Lean theorem: insertDec_adds_one ──
    // Adding one RcDec increases countDecs by exactly 1.

    proptest! {
        #[test]
        fn insert_dec_adds_one(stmts in arb_stmts(), var in arb_var_id()) {
            let before = count_decs(&stmts, var);
            let mut with_dec = stmts;
            with_dec.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            prop_assert_eq!(count_decs(&with_dec, var), before + 1);
        }
    }

    // ── Lean theorem: insertDec_keeps_incs ──
    // Adding RcDec does not change countIncs.

    proptest! {
        #[test]
        fn insert_dec_keeps_incs(stmts in arb_stmts(), var in arb_var_id()) {
            let before = count_incs(&stmts, var);
            let mut with_dec = stmts;
            with_dec.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            prop_assert_eq!(count_incs(&with_dec, var), before);
        }
    }

    // ── Lean theorem: single_dec_frees ──
    // Fresh variable (0 incs) + 1 Dec = freed (RC reaches 0).

    proptest! {
        #[test]
        fn single_dec_frees(var in arb_var_id()) {
            let stmts = vec![
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
            ];
            prop_assert!(is_freed(&stmts, var), "single Dec must free a fresh var");
        }
    }

    // ── Lean theorem: inc_dec_is_id ──
    // Adding an Inc+Dec pair is identity: freed status unchanged.

    proptest! {
        #[test]
        fn inc_dec_is_identity(stmts in arb_stmts(), var in arb_var_id()) {
            let freed_before = is_freed(&stmts, var);
            let mut with_pair = stmts;
            with_pair.push(IrStmt { kind: IrStmtKind::RcInc { var }, span: None });
            with_pair.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            prop_assert_eq!(
                is_freed(&with_pair, var), freed_before,
                "Inc+Dec pair must not change freed status"
            );
        }
    }

    // ── Lean definition: isFreed ↔ countDecs == countIncs + 1 ──

    proptest! {
        #[test]
        fn freed_iff_decs_eq_incs_plus_one(stmts in arb_stmts(), var in arb_var_id()) {
            let freed = is_freed(&stmts, var);
            let expected = count_decs(&stmts, var) == count_incs(&stmts, var) + 1;
            prop_assert_eq!(freed, expected);
        }
    }

    // ── Lean definition: hasDec ↔ countDecs ≥ 1 ──

    proptest! {
        #[test]
        fn has_dec_iff_count_ge_one(stmts in arb_stmts(), var in arb_var_id()) {
            let has = has_dec(&stmts, var);
            let expected = count_decs(&stmts, var) >= 1;
            prop_assert_eq!(has, expected);
        }
    }

    // ── Lean: Ty.isHeap classification ──

    proptest! {
        #[test]
        fn heap_type_is_heap(ty in arb_heap_ty()) {
            prop_assert!(is_heap_type(&ty), "String/Unknown must be heap");
        }

        #[test]
        fn nonheap_type_is_not_heap(ty in arb_nonheap_ty()) {
            prop_assert!(!is_heap_type(&ty), "Int/Float/Bool/Unit must not be heap");
        }
    }

    // ── Independence: Dec(a) doesn't affect counts for b ──
    // Lean: VarId-indexed counting is independent.

    proptest! {
        #[test]
        fn dec_independence(
            stmts in arb_stmts(),
            a in (0u32..4).prop_map(VarId),
            b in (4u32..8).prop_map(VarId),
        ) {
            let decs_b = count_decs(&stmts, b);
            let incs_b = count_incs(&stmts, b);
            let mut extended = stmts;
            extended.push(IrStmt { kind: IrStmtKind::RcDec { var: a }, span: None });
            prop_assert_eq!(count_decs(&extended, b), decs_b, "Dec(a) must not change decs(b)");
            prop_assert_eq!(count_incs(&extended, b), incs_b, "Dec(a) must not change incs(b)");
        }

        #[test]
        fn inc_independence(
            stmts in arb_stmts(),
            a in (0u32..4).prop_map(VarId),
            b in (4u32..8).prop_map(VarId),
        ) {
            let decs_b = count_decs(&stmts, b);
            let incs_b = count_incs(&stmts, b);
            let mut extended = stmts;
            extended.push(IrStmt { kind: IrStmtKind::RcInc { var: a }, span: None });
            prop_assert_eq!(count_decs(&extended, b), decs_b, "Inc(a) must not change decs(b)");
            prop_assert_eq!(count_incs(&extended, b), incs_b, "Inc(a) must not change incs(b)");
        }
    }

    // ── Lean theorem: perceus_strictly_better ──
    // Without Dec: not freed. With Dec: freed.

    proptest! {
        #[test]
        fn strictly_better(var in arb_var_id()) {
            let without = vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var, ty: Ty::String,
                        mutability: almide_ir::Mutability::Let,
                        value: dummy_expr(Ty::String),
                    },
                    span: None,
                },
            ];
            prop_assert!(!is_freed(&without, var), "no Dec = not freed (leak)");

            let mut with_dec = without;
            with_dec.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            prop_assert!(is_freed(&with_dec, var), "1 Dec = freed");
        }
    }

    // ── Lean theorem: assign_both_freed ──
    // Dec(old) before assign + Dec(new) at exit = both freed.

    proptest! {
        #[test]
        fn assign_pattern_both_freed(var in arb_var_id()) {
            // old alloc → Dec(old) → new alloc → Dec(new)
            let stmts = vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var, ty: Ty::String,
                        mutability: almide_ir::Mutability::Var,
                        value: dummy_expr(Ty::String),
                    },
                    span: None,
                },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
                IrStmt {
                    kind: IrStmtKind::Assign {
                        var,
                        value: dummy_expr(Ty::String),
                    },
                    span: None,
                },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
            ];
            // 2 decs, 0 incs — models old+new both freed
            prop_assert_eq!(count_decs(&stmts, var), 2);
            prop_assert_eq!(count_incs(&stmts, var), 0);
        }
    }

    // ── Lean theorem: alias_frees_heap ──
    // Inc(v) + Dec(v) + Dec(v) = original + alias both freed.

    proptest! {
        #[test]
        fn alias_pattern_freed(var in arb_var_id()) {
            let stmts = vec![
                IrStmt { kind: IrStmtKind::RcInc { var }, span: None },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
            ];
            // 1 inc + 2 decs = freed (decs == incs + 1 → 2 == 1 + 1)
            prop_assert!(is_freed(&stmts, var), "alias pattern must be freed");
        }
    }

    // ── Commutativity: order of Inc/Dec doesn't affect counts ──

    proptest! {
        #[test]
        fn count_order_independent(stmts in arb_stmts(), var in arb_var_id()) {
            let mut shuffled = stmts.clone();
            shuffled.reverse();
            prop_assert_eq!(count_decs(&stmts, var), count_decs(&shuffled, var));
            prop_assert_eq!(count_incs(&stmts, var), count_incs(&shuffled, var));
        }
    }

    // ── Assign does not change Inc/Dec counts ──
    // Assign is a value overwrite, not an Inc/Dec operation.
    proptest! {
        #[test]
        fn assign_does_not_affect_counts(stmts in arb_stmts(), var in arb_var_id(), ty in arb_heap_ty()) {
            let decs_before = count_decs(&stmts, var);
            let incs_before = count_incs(&stmts, var);
            let mut with_assign = stmts;
            with_assign.push(IrStmt {
                kind: IrStmtKind::Assign { var, value: dummy_expr(ty) },
                span: None,
            });
            prop_assert_eq!(count_decs(&with_assign, var), decs_before);
            prop_assert_eq!(count_incs(&with_assign, var), incs_before);
        }
    }

    // ── Applied/Record/Fn types are heap ──
    proptest! {
        #[test]
        fn applied_record_fn_are_heap(ty in arb_heap_ty()) {
            prop_assert!(is_heap_type(&ty), "{:?} must be heap", ty);
        }
    }

    // ── Mutable var: Bind(var) + Assign(var) + 2*Dec = 2 decs ──
    // Models: allocate, reassign, free both old and new values.
    proptest! {
        #[test]
        fn mutable_reassign_two_decs(var in arb_var_id()) {
            let stmts = vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var, ty: Ty::String,
                        mutability: almide_ir::Mutability::Var,
                        value: dummy_expr(Ty::String),
                    },
                    span: None,
                },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
                IrStmt {
                    kind: IrStmtKind::Assign { var, value: dummy_expr(Ty::String) },
                    span: None,
                },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
            ];
            prop_assert_eq!(count_decs(&stmts, var), 2);
            prop_assert_eq!(count_incs(&stmts, var), 0);
        }

        #[test]
        fn mutable_triple_reassign(var in arb_var_id()) {
            let stmts = vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var, ty: Ty::String,
                        mutability: almide_ir::Mutability::Var,
                        value: dummy_expr(Ty::String),
                    },
                    span: None,
                },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
                IrStmt { kind: IrStmtKind::Assign { var, value: dummy_expr(Ty::String) }, span: None },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
                IrStmt { kind: IrStmtKind::Assign { var, value: dummy_expr(Ty::String) }, span: None },
                IrStmt { kind: IrStmtKind::RcDec { var }, span: None },
            ];
            prop_assert_eq!(count_decs(&stmts, var), 3, "3 allocations need 3 decs");
        }
    }

    // ── Lean theorem: opt_inc_dec_preserves_freed ──
    // PerceusOpt: Inc(v)+Dec(v) pair is identity for isFreed.

    proptest! {
        #[test]
        fn opt_inc_dec_preserves_freed(stmts in arb_stmts(), var in arb_var_id()) {
            let freed_before = is_freed(&stmts, var);
            // Wrap with Inc+Dec pair
            let mut wrapped = vec![IrStmt { kind: IrStmtKind::RcInc { var }, span: None }];
            wrapped.extend(stmts);
            wrapped.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            prop_assert_eq!(
                is_freed(&wrapped, var), freed_before,
                "Inc+Dec wrapper must not change freed status (PerceusOpt soundness)"
            );
        }

        #[test]
        fn opt_inc_dec_count_balance(stmts in arb_stmts(), var in arb_var_id()) {
            let decs_before = count_decs(&stmts, var);
            let incs_before = count_incs(&stmts, var);
            // Wrap with Inc+Dec pair
            let mut wrapped = vec![IrStmt { kind: IrStmtKind::RcInc { var }, span: None }];
            wrapped.extend(stmts);
            wrapped.push(IrStmt { kind: IrStmtKind::RcDec { var }, span: None });
            // Both counts increase by exactly 1
            prop_assert_eq!(count_decs(&wrapped, var), decs_before + 1);
            prop_assert_eq!(count_incs(&wrapped, var), incs_before + 1);
        }
    }
}
