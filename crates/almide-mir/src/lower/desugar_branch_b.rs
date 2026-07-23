
/// Recurse the heap-branch desugar INTO an `if`/`match` arm and a block TAIL. After a let-bound
/// duplication the body becomes `Block{prefix; if c then {<nested branch>} else {…}}`, whose arm
/// blocks may still hide a call-arg `if` (`(value.str(if…), end)`) or another let-bound branch (the
/// block_scalar two-`if` shape). Normalizing those HERE — inside the SHARED `desugar_heap_branches`
/// both `lower_body_into` and the `count_ir_calls` caps gate call — keeps the duplicated calls 1:1
/// (mir == ir); doing it lowering-side only (in `lower_heap_result_arm`) would double-count.
/// Outer name router — each arm's body moved to a named helper (codopsy cc), same
/// "outer router unchanged, arm body to helper" split used throughout this crate.
/// Arm SELECTION (pattern + guard, in the same order) is untouched; each helper
/// re-matches `&body.kind` itself and falls back to `None` (never panics) if that
/// invariant is ever violated by a future edit — a defensive, not a load-bearing,
/// fallback.
fn desugar_nested_branch_arms(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    match &body.kind {
        IrExprKind::If { .. } => desugar_nested_branch_if_arm(body, next_var, layouts),
        IrExprKind::Match { .. } => desugar_nested_branch_match_arm(body, next_var, layouts),
        IrExprKind::Block { expr: Some(_), .. } => {
            desugar_nested_branch_block_arm(body, next_var, layouts)
        }
        // A `list.map`/`flat_map`/… CALL carrying an INLINE-LAMBDA arg whose body hides a LET-BOUND
        // heap `if`/`match` (the bindgen `gen_pack_variant` outer flat_map's `let cond = if idx==0
        // then "if" else "elseif"`). Apply ONLY the let-bound-branch tail-duplication INSIDE the
        // lambda body (`desugar_lambda_let_branches`) — NOT the full `desugar_heap_branches_inner`
        // fixpoint, whose other passes (match-subject-hoist, call-arg `if`/unwrap lifts) are tuned for
        // the FUNCTION-body lowering path and would mangle an already-lowerable defunc-lambda shape (a
        // `match {…} + [""]` body regressed when run through the full fixpoint). This stays a STRICT
        // no-op for a lambda WITHOUT a let-bound heap-branch (julia `gen_variant_types` is untouched).
        // Applied BEFORE both the defunc lowering and the `count_ir_calls` gate (desugar-before-both:
        // both see the IDENTICAL duplicated lambda body, mir==ir 1:1 by construction). Params/lambda_id
        // are preserved so the defunc inliner binds the same params.
        // SKIP a `list.fold` lambda: the tuple-accumulator fold (`try_lower_defunc_tuple_acc_fold`)
        // lowers its OWN multi-statement body — a `let store = if/match` INTERIOR stmt is materialized
        // by `lower_bind` (the heap-result-`if` merged-owned-value path) reading the slots directly.
        // Tail-DUPLICATING that `let store = if` here would turn the body `{ let store=if; (acc+[store],
        // n+1) }` into `if c then {(acc+[A],n+1)} else {…}` — an if-of-TUPLES the tuple-fold gate (which
        // requires a `(c0, c1)` tuple tail) cannot match → it declines to the self-host `fold_hacc`.
        // (map/flat_map's str-acc DOES handle the if-of-tuples-equivalent via its unit-append `if`, so
        // those keep the desugar.)
        IrExprKind::Call { target, .. }
            if !matches!(target,
                CallTarget::Module { module, func, .. }
                    if module.as_str() == "list" && func.as_str() == "fold") =>
        {
            desugar_nested_branch_call_arm(body)
        }
        // A `(<flat_map call>) + [tail]` ConcatList/ConcatStr — the bindgen `gen_pack_variant` /
        // `gen_variant_struct` outer shape `(cases |> list.flat_map(…)) + ["${indent}}"]`. The HOF
        // call whose lambda hides a let-bound heap-branch sits in a BinOp OPERAND, unreachable by the
        // `Call`/`Block`/arm cases above. Recurse into BOTH operands so the flat_map's lambda-let-if is
        // tail-duplicated (otherwise the outer flat_map declines → the concat walls `heap-result BinOp`).
        IrExprKind::BinOp { .. } => desugar_nested_branch_binop_arm(body, next_var, layouts),
        _ => None,
    }
}

fn desugar_nested_branch_if_arm(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let IrExprKind::If { cond, then, else_ } = &body.kind else { return None };
    let nt = desugar_heap_branches_inner(then, next_var, layouts);
    let ne = desugar_heap_branches_inner(else_, next_var, layouts);
    if nt.is_none() && ne.is_none() {
        return None;
    }
    Some(IrExpr {
        kind: IrExprKind::If {
            cond: cond.clone(),
            then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
            else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
        },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

fn desugar_nested_branch_match_arm(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let IrExprKind::Match { subject, arms } = &body.kind else { return None };
    let mut changed = false;
    let new_arms: Vec<almide_ir::IrMatchArm> = arms
        .iter()
        .map(|a| match desugar_heap_branches_inner(&a.body, next_var, layouts) {
            Some(nb) => {
                changed = true;
                almide_ir::IrMatchArm {
                    pattern: a.pattern.clone(),
                    guard: a.guard.clone(),
                    body: nb,
                }
            }
            None => a.clone(),
        })
        .collect();
    if !changed {
        return None;
    }
    Some(IrExpr {
        kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// Recurse into BOTH the block's `let`-bind STMT values AND its tail. A HOF call binding
/// `let case_lines = cases |> list.flat_map((entry) => { … let cond = if … ; … })` hides a
/// let-bound heap `if` inside the lambda arg — only reachable by descending the bind value.
/// The stmt-value recursion uses the FOCUSED `desugar_lambda_let_branches` (let-bound-branch
/// duplication ONLY, into nested if/match/block/lambda) — NOT the full `desugar_heap_branches
/// _inner` fixpoint, whose function-body-tuned passes regress an already-lowerable bind value
/// (julia `gen_variant_types`'s `let case_lines = <flat_map of match {…}+[""]>` walled when run
/// through the full fixpoint). The tail KEEPS the full fixpoint (the existing nested-arm path).
fn desugar_nested_branch_block_arm(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else { return None };
    let mut changed = false;
    let new_stmts: Vec<IrStmt> = stmts
        .iter()
        .map(|s| match &s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => {
                match desugar_lambda_let_branches(value) {
                    Some(nv) => {
                        changed = true;
                        IrStmt {
                            kind: IrStmtKind::Bind {
                                var: *var,
                                mutability: *mutability,
                                ty: ty.clone(),
                                value: nv,
                            },
                            span: s.span.clone(),
                        }
                    }
                    None => s.clone(),
                }
            }
            _ => s.clone(),
        })
        .collect();
    let nt = desugar_heap_branches_inner(tail, next_var, layouts);
    if nt.is_some() {
        changed = true;
    }
    if !changed {
        return None;
    }
    Some(IrExpr {
        kind: IrExprKind::Block {
            stmts: new_stmts,
            expr: Some(Box::new(nt.unwrap_or_else(|| (**tail).clone()))),
        },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

fn desugar_nested_branch_call_arm(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Call { target, args, type_args } = &body.kind else { return None };
    let mut changed = false;
    let new_args: Vec<IrExpr> = args
        .iter()
        .map(|a| match &a.kind {
            IrExprKind::Lambda { params, body: lam_body, lambda_id } => {
                match desugar_lambda_let_branches(lam_body) {
                    Some(nb) => {
                        changed = true;
                        IrExpr {
                            kind: IrExprKind::Lambda {
                                params: params.clone(),
                                body: Box::new(nb),
                                lambda_id: *lambda_id,
                            },
                            ty: a.ty.clone(),
                            span: a.span.clone(),
                            def_id: a.def_id,
                        }
                    }
                    None => a.clone(),
                }
            }
            _ => a.clone(),
        })
        .collect();
    if !changed {
        return None;
    }
    Some(IrExpr {
        kind: IrExprKind::Call {
            target: target.clone(),
            args: new_args,
            type_args: type_args.clone(),
        },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

fn desugar_nested_branch_binop_arm(
    body: &IrExpr,
    next_var: &mut u32,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let IrExprKind::BinOp { op, left, right } = &body.kind else { return None };
    let nl = desugar_nested_branch_arms(left, next_var, layouts);
    let nr = desugar_nested_branch_arms(right, next_var, layouts);
    if nl.is_none() && nr.is_none() {
        return None;
    }
    Some(IrExpr {
        kind: IrExprKind::BinOp {
            op: *op,
            left: Box::new(nl.unwrap_or_else(|| (**left).clone())),
            right: Box::new(nr.unwrap_or_else(|| (**right).clone())),
        },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// FOCUSED let-bound-heap-branch desugar for a DEFUNC-LAMBDA BODY (`(entry) => { … let cond = if …;
/// … }`). Applies ONLY the let-bound-branch tail-duplication (`desugar_let_bound_heap_branch`),
/// recursing through the body's nested structure (Block stmts + tail, if/match arms, and INNER HOF
/// lambdas) — NOT the full `desugar_heap_branches_inner` fixpoint, whose match-subject-hoist /
/// call-arg lift passes are tuned for the function-body lowering and REGRESS an already-lowerable
/// defunc-lambda shape (a `match {…} + [""]` body). A STRICT no-op when the lambda body has no
/// let-bound heap-branch. Returns `Some(rewritten)` when a duplication fired, `None` otherwise.
///
/// SOUNDNESS: identical to `desugar_let_bound_heap_branch` — a PURE IR→IR tail-duplication (each arm
/// binds its value + runs the continuation, per-arm `i…d` balance, only one arm runs = v0-identical,
/// NO cert/Coq change). Applied desugar-before-both (the shared desugar runs over the lambda body for
/// BOTH the defunc lowering and the caps `count_ir_calls` gate), so mir==ir 1:1 holds by construction.
fn desugar_lambda_let_branches(body: &IrExpr) -> Option<IrExpr> {
    // Fixpoint: apply the let-bound-branch duplication at THIS position until it stops firing (the
    // continuation may expose a second let-bound branch — the bounded gate caps the depth), then
    // recurse structurally so a duplication INSIDE an arm / inner lambda is reached too.
    let mut cur: Option<IrExpr> = None;
    loop {
        let src = cur.as_ref().unwrap_or(body);
        if let Some(r) = desugar_let_bound_heap_branch(src) {
            cur = Some(r);
            continue;
        }
        // Inline a SINGLE-USE let-bound match subject INSIDE the lambda body too (`(case) => { let p =
        // json.get(case,"payload"); match p { … } }`) — turning the let-bound-Var variant-match subject
        // into the inline-subject form the str-acc handler C1-lowers (vs the funcref-dropping C2-lift).
        // Value-preserving (single use ⇒ one eval); STRICT-gated so it is a no-op otherwise.
        if let Some(r) = desugar_inline_single_use_match_subject(src) {
            cur = Some(r);
            continue;
        }
        break;
    }
    let src_owned = cur;
    let src = src_owned.as_ref().unwrap_or(body);
    // Recurse structurally (let-branch desugar only) into the parts that may host a defunc lambda or a
    // nested let-bound branch. A change anywhere — here or the top-level duplication above — yields Some.
    let recursed = match &src.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let mut changed = false;
            let new_stmts: Vec<IrStmt> = stmts
                .iter()
                .map(|s| match &s.kind {
                    IrStmtKind::Bind { var, mutability, ty, value } => {
                        match desugar_lambda_let_branches(value) {
                            Some(nv) => {
                                changed = true;
                                IrStmt {
                                    kind: IrStmtKind::Bind {
                                        var: *var,
                                        mutability: *mutability,
                                        ty: ty.clone(),
                                        value: nv,
                                    },
                                    span: s.span.clone(),
                                }
                            }
                            None => s.clone(),
                        }
                    }
                    _ => s.clone(),
                })
                .collect();
            let nt = desugar_lambda_let_branches(tail);
            if nt.is_some() {
                changed = true;
            }
            if changed {
                Some(IrExpr {
                    kind: IrExprKind::Block {
                        stmts: new_stmts,
                        expr: Some(Box::new(nt.unwrap_or_else(|| (**tail).clone()))),
                    },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            } else {
                None
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_lambda_let_branches(then);
            let ne = desugar_lambda_let_branches(else_);
            if nt.is_none() && ne.is_none() {
                None
            } else {
                Some(IrExpr {
                    kind: IrExprKind::If {
                        cond: cond.clone(),
                        then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                        else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                    },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            }
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_lambda_let_branches(&a.body) {
                    Some(nb) => {
                        changed = true;
                        almide_ir::IrMatchArm {
                            pattern: a.pattern.clone(),
                            guard: a.guard.clone(),
                            body: nb,
                        }
                    }
                    None => a.clone(),
                })
                .collect();
            if changed {
                Some(IrExpr {
                    kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            } else {
                None
            }
        }
        // An inner HOF call (`get_arr(pl,"fields") |> list.flat_map((pe) => { … })`): recurse into its
        // lambda args so a let-bound branch in a NESTED loop body is reached too. SKIP a `list.fold`
        // lambda — the tuple-accumulator fold lowers its OWN multi-statement body (a `let store =
        // if/match` interior stmt materialized by `lower_bind`); tail-duplicating it here would turn the
        // `(acc+[store], n+step)` tuple body into an if-of-tuples the tuple-fold gate cannot match.
        IrExprKind::Call { target, args, type_args }
            if !matches!(target,
                CallTarget::Module { module, func, .. }
                    if module.as_str() == "list" && func.as_str() == "fold") =>
        {
            let mut changed = false;
            let new_args: Vec<IrExpr> = args
                .iter()
                .map(|a| match &a.kind {
                    IrExprKind::Lambda { params, body: lam_body, lambda_id } => {
                        match desugar_lambda_let_branches(lam_body) {
                            Some(nb) => {
                                changed = true;
                                IrExpr {
                                    kind: IrExprKind::Lambda {
                                        params: params.clone(),
                                        body: Box::new(nb),
                                        lambda_id: *lambda_id,
                                    },
                                    ty: a.ty.clone(),
                                    span: a.span.clone(),
                                    def_id: a.def_id,
                                }
                            }
                            None => a.clone(),
                        }
                    }
                    _ => a.clone(),
                })
                .collect();
            if changed {
                Some(IrExpr {
                    kind: IrExprKind::Call {
                        target: target.clone(),
                        args: new_args,
                        type_args: type_args.clone(),
                    },
                    ty: src.ty.clone(),
                    span: src.span.clone(),
                    def_id: src.def_id,
                })
            } else {
                None
            }
        }
        _ => None,
    };
    // Some if EITHER the top-level duplication fired OR a structural recursion changed something.
    match (src_owned, recursed) {
        (_, Some(r)) => Some(r),
        (Some(s), None) => Some(s),
        (None, None) => None,
    }
}

pub fn desugar_let_bound_heap_branch(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // Find the first heap let-bound `if`/`match` bind — OR a SCALAR-typed one whose
    // arms carry an error operator (`let v = if c then boom(x)! else boom(y)!` — the
    // effect-fn auto-`?` puts a `Try` INSIDE each arm; the scalar path silently
    // stripped it and bound the raw Result handle, the via_if class the proven
    // checker's leak line exposed). Tail-duplicating pushes the continuation into
    // each arm, where the ordinary monadic `!` desugar handles the Try faithfully.
    fn arm_has_error_op(e: &IrExpr) -> bool {
        fn direct(e: &IrExpr) -> bool {
            match &e.kind {
                IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } => true,
                IrExprKind::Block { expr: Some(t), .. } => direct(t),
                _ => false,
            }
        }
        match &e.kind {
            IrExprKind::If { then, else_, .. } => direct(then) || direct(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| direct(&a.body)),
            _ => false,
        }
    }
    let (i, bind_var, bind_ty, branch) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. }
            if (is_heap_ty(ty) || arm_has_error_op(value))
                && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) =>
        {
            Some((i, *var, ty.clone(), value))
        }
        _ => None,
    })?;
    // BOUNDED-DUPLICATION gate: refuse when the continuation itself carries another unresolved
    // heap let-bound `if`/`match`.
    // BOUNDED-DUPLICATION: the continuation is copied into BOTH arms, so each remaining heap let-bound
    // `if`/`match` in `rest` doubles the leaf-arm count as the fixpoint resolves them one at a time. A
    // FEW are fine (block_scalar = 2: `let joined = if…; let tmp = if…(value.str arg, ANF-lifted)`), so
    // allow up to 3 (≤ 2^4 = 16 leaves) and refuse beyond that to keep the duplication bounded. The
    // wasm-bindgen `generate_esm` shape stacks 4 top-level optional-list `if`s (matrix/bytes/import_shim/
    // shim_noop, each `if cond then [LITERALS] else []`); the FIRST sees rest = 3, so the old `> 2` gate
    // bailed → the merged let-bound `if` walled. Raising to `> 3` lets the proven per-arm-balanced
    // tail-duplication resolve all 4 (each leaf independently binds + drops its own list — corpus-wall
    // re-verifies every arm). The leaves are mostly literal-list concats (cheap), so 16× is tolerable for
    // a build-time codegen tool; deeper stacks still WALL rather than blow up.
    let rest_branch_binds = stmts[i + 1..]
        .iter()
        .filter(|s| {
            matches!(
                &s.kind,
                IrStmtKind::Bind { ty, value, .. }
                    if is_heap_ty(ty)
                        && matches!(&value.kind, IrExprKind::If { .. } | IrExprKind::Match { .. })
            )
        })
        .count();
    if rest_branch_binds > 3 {
        return None;
    }
    let result_ty = &body.ty;
    let rest_stmts: Vec<IrStmt> = stmts[i + 1..].to_vec();
    let rest_tail: Option<Box<IrExpr>> = tail.clone();
    // Push the continuation `{ rest }` behind the per-arm bind of `bind_var`. A literal-pattern
    // `match` (or `if`) reduces to a nested `if` chain via `desugar_match_to_if` and uses
    // `wrap_branch_arms`; a CUSTOM-VARIANT / non-literal `match` (`match s.shape { Circle(_) =>
    // "circle", … }`) — which `desugar_match_to_if` declines — keeps its `Match` and pushes the
    // continuation into each arm via `wrap_match_arms` (the proven tail custom-variant match then
    // runs each arm). Both are call-count-invariant, so `mir == ir` holds.
    let rewritten_branch = match &branch.kind {
        IrExprKind::If { .. } => LowerCtx::wrap_branch_arms(
            branch, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
        ),
        IrExprKind::Match { subject, arms } => {
            match LowerCtx::default().desugar_match_to_if(subject, arms, &branch.ty) {
                Some(if_branch) => LowerCtx::wrap_branch_arms(
                    &if_branch, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
                ),
                None => LowerCtx::wrap_match_arms(
                    subject, arms, bind_var, &bind_ty, &rest_stmts, &rest_tail, result_ty,
                ),
            }
        }
        _ => return None,
    };
    // The prefix statements `stmts[0..i]` stay; the rewritten branch is the new block TAIL.
    let prefix: Vec<IrStmt> = stmts[..i].to_vec();
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: prefix, expr: Some(Box::new(rewritten_branch)) },
        ty: result_ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}


/// A UNIT statement-`if` whose arm(s) CONDITIONALLY REASSIGN one heap var declared earlier in
/// the SAME block (`var r = err(..); if c then { r = ok(42) } else { () }; …r…` — the lp5
/// shape) is SSA-IFIED into a fresh LET-bound value-`if`
/// (`let r' = if c then ok(42) else r; …r'…`): each assigning arm's assign becomes the arm's
/// TAIL value, a non-assigning arm keeps its statements and yields the old `r`, and every
/// LATER reference to `r` substitutes to `r'`. The let-bound heap `if` then flows through the
/// EXISTING tail-duplication + heap-result-`if` machinery (per-arm construction + release
/// parity — the `pick` Var-arm `Dup` precedent), so the conditional value merges BY VALUE
/// instead of by slot mutation — mod_p3's in-frame heap-assign elision (which silently
/// DROPPED the reassignment: probe `pick(true)` printed v0 `ok:42` vs v1 `err:normal`, the
/// B127-recorded lp5 LIVE WRONG-VALUE bug) is never reached for this shape. Guards: exactly
/// ONE heap var assigned across the two arms; at most one assign per arm, only as the arm's
/// LAST statement (deep-scanned: no nested assigns to it anywhere in either arm); `r` bound
/// EARLIER IN THIS BLOCK (an outer-block `r` would leave un-substituted references outside
/// this block's remainder); no LATER assign to `r` after the `if`. Anything else keeps its
/// existing path (the honest wall / the loop machinery). Count-invariant: the assign's value
/// moves to tail position, the substituted reads stay reads — no call added or removed — and
/// the pass runs inside the SHARED `desugar_heap_branches` fixpoint, so `mir == ir` holds.
pub fn desugar_unit_if_heap_reassign(body: &IrExpr, next_var: &mut u32) -> Option<IrExpr> {
    use almide_ir::{substitute_var_in_expr, substitute_var_in_stmt, Mutability};
    fn assigns_to(e: &IrExpr, var: VarId) -> bool {
        use almide_ir::visit::IrVisitor;
        struct S {
            var: VarId,
            found: bool,
        }
        impl IrVisitor for S {
            fn visit_stmt(&mut self, s: &IrStmt) {
                if matches!(&s.kind, IrStmtKind::Assign { var, .. } if *var == self.var) {
                    self.found = true;
                }
                almide_ir::visit::walk_stmt(self, s);
            }
        }
        let mut s = S { var, found: false };
        s.visit_expr(e);
        s.found
    }
    fn heap_assigned_vars(e: &IrExpr, out: &mut std::collections::HashSet<VarId>) {
        use almide_ir::visit::IrVisitor;
        struct S<'a>(&'a mut std::collections::HashSet<VarId>);
        impl IrVisitor for S<'_> {
            fn visit_stmt(&mut self, s: &IrStmt) {
                if let IrStmtKind::Assign { var, value } = &s.kind {
                    if is_heap_ty(&value.ty) {
                        self.0.insert(*var);
                    }
                }
                almide_ir::visit::walk_stmt(self, s);
            }
        }
        S(out).visit_expr(e);
    }
    // Split an arm: `Some(Some((prefix_stmts, assigned_value)))` when its LAST stmt is the
    // arm's single deep assign to `r`; `Some(None)` when the arm never assigns `r` (kept
    // whole); `None` declines the whole rewrite.
    fn is_unit_tail(t: &Option<Box<IrExpr>>) -> bool {
        match t.as_deref() {
            None => true,
            Some(e) => matches!(&e.kind, IrExprKind::Unit),
        }
    }
    // The "last stmt must be the ONLY assign to r in the arm (deep)" check, named
    // (codopsy cc) — a sibling nested fn to `arm_split`, in the same block, so it
    // shares that block's other nested-fn items (`assigns_to`) exactly as the inline
    // loop did.
    fn any_pre_stmt_assigns(pre: &[IrStmt], r: VarId) -> bool {
        pre.iter().any(|s| {
            let probe = IrExpr {
                kind: IrExprKind::Block { stmts: vec![s.clone()], expr: Option::None },
                ty: Ty::Unit,
                span: Option::None,
                def_id: Option::None,
            };
            assigns_to(&probe, r)
        })
    }
    fn arm_split(arm: &IrExpr, r: VarId) -> Option<Option<(Vec<IrStmt>, IrExpr)>> {
        let deep_assigns = {
            let mut set = std::collections::HashSet::new();
            heap_assigned_vars(arm, &mut set);
            set.contains(&r)
        };
        if !deep_assigns {
            return Some(None);
        }
        let IrExprKind::Block { stmts, expr } = &arm.kind else { return None };
        if !is_unit_tail(expr) {
            return None;
        }
        let Some((last, pre)) = stmts.split_last() else { return None };
        let IrStmtKind::Assign { var, value } = &last.kind else { return None };
        if *var != r {
            return None;
        }
        if any_pre_stmt_assigns(pre, r) {
            return None;
        }
        Some(Some((pre.to_vec(), value.clone())))
    }
    // The per-candidate-`if` guard cascade, named (codopsy cc) — a sibling nested fn,
    // in the same block as `arm_split`/`assigns_to`/`heap_assigned_vars` so it shares
    // them. Returns the single heap var the arms conditionally reassign + its type,
    // or `None` if `stmts[i]` doesn't qualify (any guard below declines) — the SAME
    // sequence of checks, in the SAME order, as the original inline `continue` chain.
    fn qualifying_reassign_target(
        stmts: &[IrStmt],
        tail: &Option<Box<IrExpr>>,
        i: usize,
        cond: &IrExpr,
        then: &IrExpr,
        else_: &IrExpr,
    ) -> Option<(VarId, Ty)> {
        let mut assigned = std::collections::HashSet::new();
        heap_assigned_vars(then, &mut assigned);
        heap_assigned_vars(else_, &mut assigned);
        if assigned.len() != 1 {
            return None;
        }
        let r = *assigned.iter().next().expect("assigned.len() == 1, checked immediately above");
        // `r` must be bound earlier in THIS block.
        let rty = stmts[..i].iter().find_map(|b| match &b.kind {
            IrStmtKind::Bind { var, ty, .. } if *var == r => Some(ty.clone()),
            _ => Option::None,
        })?;
        if !is_heap_ty(&rty) {
            return None;
        }
        // The condition is evaluated before the arms — it must not assign r.
        if assigns_to(cond, r) {
            return None;
        }
        // No LATER assign to r after this if.
        let later = IrExpr {
            kind: IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.clone() },
            ty: Ty::Unit,
            span: Option::None,
            def_id: Option::None,
        };
        if assigns_to(&later, r) {
            return None;
        }
        Some((r, rty))
    }
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    for (i, s) in stmts.iter().enumerate() {
        let IrStmtKind::Expr { expr: ife } = &s.kind else { continue };
        let IrExprKind::If { cond, then, else_ } = &ife.kind else { continue };
        let Some((r, rty)) = qualifying_reassign_target(stmts, tail, i, cond, then, else_)
        else {
            continue;
        };
        let (Some(then_split), Some(else_split)) = (arm_split(then, r), arm_split(else_, r))
        else {
            continue;
        };
        let rprime = VarId(*next_var);
        *next_var += 1;
        let old_r = IrExpr {
            kind: IrExprKind::Var { id: r },
            ty: rty.clone(),
            span: ife.span.clone(),
            def_id: Option::None,
        };
        let mk_arm = |split: Option<(Vec<IrStmt>, IrExpr)>, orig: &IrExpr| -> IrExpr {
            match split {
                Some((pre, val)) => IrExpr {
                    kind: IrExprKind::Block { stmts: pre, expr: Some(Box::new(val)) },
                    ty: rty.clone(),
                    span: orig.span.clone(),
                    def_id: Option::None,
                },
                Option::None => {
                    // Keep the arm's own statements (its effects run), yield the old r.
                    let kept = match &orig.kind {
                        IrExprKind::Block { stmts, .. } => stmts.clone(),
                        IrExprKind::Unit => vec![],
                        _ => vec![IrStmt {
                            kind: IrStmtKind::Expr { expr: orig.clone() },
                            span: orig.span.clone(),
                        }],
                    };
                    IrExpr {
                        kind: IrExprKind::Block {
                            stmts: kept,
                            expr: Some(Box::new(old_r.clone())),
                        },
                        ty: rty.clone(),
                        span: orig.span.clone(),
                        def_id: Option::None,
                    }
                }
            }
        };
        let new_if = IrExpr {
            kind: IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(mk_arm(then_split, then)),
                else_: Box::new(mk_arm(else_split, else_)),
            },
            ty: rty.clone(),
            span: ife.span.clone(),
            def_id: Option::None,
        };
        let bind = IrStmt {
            kind: IrStmtKind::Bind {
                var: rprime,
                mutability: Mutability::Let,
                ty: rty.clone(),
                value: new_if,
            },
            span: s.span.clone(),
        };
        let rp_ref = IrExpr {
            kind: IrExprKind::Var { id: rprime },
            ty: rty,
            span: Option::None,
            def_id: Option::None,
        };
        let mut out = stmts[..i].to_vec();
        out.push(bind);
        for later_s in &stmts[i + 1..] {
            out.push(substitute_var_in_stmt(later_s, r, &rp_ref));
        }
        let new_tail =
            tail.as_deref().map(|t| Box::new(substitute_var_in_expr(t, r, &rp_ref)));
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: out, expr: new_tail },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    None
}
