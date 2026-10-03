// The call-argument ANF hoists: a MUTABLE-GLOBAL projection argument and a
// heap-result `if` argument are each decomposed into fresh `let` binds so the
// call sees a plain `Var` (#881 / #904 — the two committed walker twins share
// the loop-boundary discipline). Split out of desugar_b.rs (max-lines, #852);
// both passes moved verbatim.

/// `s(cached_items[i].content)` — a call argument that PROJECTS a heap value
/// out of a MUTABLE module-level global (an `IndexAccess`/`Member` chain
/// rooted at a mutable-global var) walls in STRICT value mode, while the SAME
/// projection decomposed into `let` steps lowers (the ceangal `get_item_text`
/// export, #881's last brick). ANF exactly that shape: each such argument's
/// projection chain is bound step-by-step into fresh `let`s and the argument
/// becomes the final temp's `Var`; the call moves into a Block carrying the
/// binds. Count-preserving (the same calls, merely let-bound) and rooted-at-a
/// -mutable-global only, so ordinary local projections keep their existing
/// lowering byte-for-byte.
pub(crate) fn desugar_mutable_global_projection_args(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};

    fn projection_root(e: &IrExpr) -> Option<VarId> {
        match &e.kind {
            IrExprKind::Member { object, .. } => projection_root(object),
            IrExprKind::IndexAccess { object, .. } => projection_root(object),
            IrExprKind::Var { id } => Some(*id),
            _ => None,
        }
    }
    fn qualifies(a: &IrExpr) -> bool {
        matches!(a.kind, IrExprKind::Member { .. } | IrExprKind::IndexAccess { .. })
            && crate::lower::is_heap_ty(&a.ty)
            && projection_root(a).is_some_and(crate::lower::is_mutable_global)
    }
    fn is_projection(e: &IrExpr) -> bool {
        matches!(e.kind, IrExprKind::Member { .. } | IrExprKind::IndexAccess { .. })
    }
    /// Bind every projection level into a temp (innermost first), returning
    /// the `Var` of the outermost temp.
    fn bind_chain(e: &IrExpr, next: &mut u32, binds: &mut Vec<IrStmt>) -> IrExpr {
        let rebuilt = match &e.kind {
            IrExprKind::Member { object, field } if is_projection(object) => IrExpr {
                kind: IrExprKind::Member {
                    object: Box::new(bind_chain(object, next, binds)),
                    field: *field,
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            },
            IrExprKind::IndexAccess { object, index } if is_projection(object) => IrExpr {
                kind: IrExprKind::IndexAccess {
                    object: Box::new(bind_chain(object, next, binds)),
                    index: index.clone(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            },
            _ => e.clone(),
        };
        let tmp = VarId(*next);
        *next += 1;
        binds.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: Mutability::Let,
                ty: e.ty.clone(),
                value: rebuilt,
            },
            span: e.span.clone(),
        });
        IrExpr { kind: IrExprKind::Var { id: tmp }, ty: e.ty.clone(), span: e.span.clone(), def_id: None }
    }

    /// Hoist the DEEPEST qualifying projection prefix inside `a`: the whole
    /// arg when it qualifies, else the heap-typed mg-rooted OBJECT under a
    /// scalar-tail projection (`float.to_string(items[0].elapsed)` — the arg
    /// is Float, but `items[0]` is the heap read that must bind first; the
    /// wasm_cross_pkg println concat, the ratchet regression).
    fn hoist_qualifying(a: &mut IrExpr, next: &mut u32, binds: &mut Vec<IrStmt>) -> bool {
        if qualifies(a) {
            let bound = bind_chain(a, next, binds);
            *a = bound;
            return true;
        }
        match &mut a.kind {
            IrExprKind::Member { object, .. } | IrExprKind::IndexAccess { object, .. } => {
                hoist_qualifying(object, next, binds)
            }
            _ => false,
        }
    }

    /// Statement-position hoist (the `desugar_heap_if_call_args` discipline —
    /// an IN-PLACE Block around the call turns the enclosing operand into a
    /// Block the arg/concat machinery declines, which re-walled the
    /// wasm_cross_pkg println): binds collect into the enclosing statement
    /// list; lambda bodies are their own hoist roots.
    fn hoist(e: &mut IrExpr, binds: &mut Vec<IrStmt>, next: &mut u32, changed: &mut bool) {
        if let IrExprKind::Lambda { body, .. } = &mut e.kind {
            if let Some(new_body) = desugar_mutable_global_projection_args(body) {
                **body = new_body;
                *changed = true;
            }
            return;
        }
        // A LOOP is a hoist BOUNDARY: `bump(items[i], d)` inside a for body
        // is LOOP-VARIANT (i changes, and `items` itself may be reassigned
        // mid-loop) — hoisting its reads before the loop froze one stale
        // handle for every iteration (p2's tick reading pre-reassign values:
        // a silent wrong value). The loop body's statements hoist WITHIN the
        // body instead.
        if let IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } = &mut e.kind {
            hoist_stmt_list(body, next, changed);
            // The While COND re-evaluates per iteration and has no
            // per-iteration statement slot — leave it untouched (the
            // pre-existing arg machinery lowers or walls it honestly).
            return;
        }
        almide_ir::visit_mut::walk_expr_mut(&mut HoistWalk { binds, next, changed }, e);
        let IrExprKind::Call { args, .. } = &mut e.kind else { return };
        for a in args.iter_mut() {
            if hoist_qualifying(a, next, binds) {
                *changed = true;
            }
        }
    }
    /// Hoist within a loop body's OWN statement list (binds land before their
    /// carrying statement, inside the loop).
    fn hoist_stmt_list(stmts: &mut Vec<IrStmt>, next: &mut u32, changed: &mut bool) {
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut s in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut s.kind {
                IrStmtKind::Bind { value, .. }
                | IrStmtKind::Assign { value, .. }
                | IrStmtKind::Expr { expr: value }
                | IrStmtKind::BindDestructure { value, .. } => {
                    hoist(value, &mut binds, next, changed)
                }
                IrStmtKind::IndexAssign { index, value, .. } => {
                    hoist(index, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::MapInsert { key, value, .. } => {
                    hoist(key, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::FieldAssign { value, .. } => hoist(value, &mut binds, next, changed),
                _ => {}
            }
            out.extend(binds);
            out.push(s);
        }
        *stmts = out;
    }
    struct HoistWalk<'a> {
        binds: &'a mut Vec<IrStmt>,
        next: &'a mut u32,
        changed: &'a mut bool,
    }
    impl almide_ir::IrMutVisitor for HoistWalk<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            hoist(e, self.binds, self.next, self.changed);
        }
    }
    fn rewrite_block(
        stmts: &mut Vec<IrStmt>,
        tail: &mut Option<Box<IrExpr>>,
        next: &mut u32,
        changed: &mut bool,
    ) {
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut s in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut s.kind {
                IrStmtKind::Bind { value, .. }
                | IrStmtKind::Assign { value, .. }
                | IrStmtKind::Expr { expr: value }
                | IrStmtKind::BindDestructure { value, .. } => {
                    hoist(value, &mut binds, next, changed)
                }
                IrStmtKind::IndexAssign { index, value, .. } => {
                    hoist(index, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::MapInsert { key, value, .. } => {
                    hoist(key, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::FieldAssign { value, .. } => hoist(value, &mut binds, next, changed),
                _ => {}
            }
            out.extend(binds);
            out.push(s);
        }
        *stmts = out;
        if let Some(t) = tail {
            let mut binds = Vec::new();
            hoist(t, &mut binds, next, changed);
            if !binds.is_empty() {
                stmts.extend(binds);
            }
        }
    }
    struct BlockWalk<'a> {
        next: &'a mut u32,
        changed: &'a mut bool,
    }
    impl almide_ir::IrMutVisitor for BlockWalk<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::visit_mut::walk_expr_mut(self, e);
            if let IrExprKind::Block { stmts, expr } = &mut e.kind {
                rewrite_block(stmts, expr, self.next, self.changed);
            }
        }
    }

    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let mut out = body.clone();
    almide_ir::IrMutVisitor::visit_expr_mut(
        &mut BlockWalk { next: &mut next, changed: &mut changed },
        &mut out,
    );
    // A non-Block body whose tail needs hoists: wrap it in a Block carrying them.
    if !matches!(out.kind, IrExprKind::Block { .. }) {
        let mut binds = Vec::new();
        hoist(&mut out, &mut binds, &mut next, &mut changed);
        if !binds.is_empty() {
            let ty = out.ty.clone();
            let span = out.span.clone();
            out = IrExpr {
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(out)) },
                ty,
                span,
                def_id: None,
            };
        }
    }
    changed.then_some(out)
}

/// `v.color(if item.done then gray else white)` — a call argument that IS a
/// heap-result `if` (record/variant/container-typed) walls the element/arg
/// lowering, while the SAME `if` bound by a `let` at STATEMENT position
/// lowers (the heap-branch desugar and `lower_bind_heap_if` both work on
/// statement-position binds — the ceangal todo_item element chain, #881).
/// HOIST such an argument to the nearest statement slot: every argument of
/// the carrying call is bound to a fresh `let` IN ARGUMENT ORDER (so effects
/// keep their sequence) and the call keeps only `Var`s. A non-Block function
/// body (or lambda body) that needs hoists is wrapped in a Block carrying
/// them. Lambda bodies hoist within THEIR OWN body — an argument `if` there
/// typically reads the lambda's params. STRING-typed if-args are excluded:
/// the existing arg machinery already lowers them, and rewriting would churn
/// working programs' bytes for no behavior change.
pub(crate) fn desugar_heap_if_call_args(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};

    fn qualifies(a: &IrExpr) -> bool {
        matches!(a.kind, IrExprKind::If { .. })
            && crate::lower::is_heap_ty(&a.ty)
            && !matches!(a.ty, Ty::String)
    }
    fn is_atom(a: &IrExpr) -> bool {
        matches!(
            a.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::Unit
        )
    }

    /// Rewrite `e` in place: any call carrying a qualifying if-arg gets ALL
    /// its non-atom arguments let-bound into `binds` (argument order kept)
    /// and replaced with `Var`s. Lambda bodies are their own hoist roots.
    fn hoist(e: &mut IrExpr, binds: &mut Vec<IrStmt>, next: &mut u32, changed: &mut bool) {
        if let IrExprKind::Lambda { body, .. } = &mut e.kind {
            if let Some(new_body) = desugar_heap_if_call_args(body) {
                **body = new_body;
                *changed = true;
            }
            return;
        }
        // A LOOP is a hoist BOUNDARY (the projection-hoist sibling's p2
        // lesson): an arg `if` inside a loop body is loop-variant — hoisting
        // it before the loop would freeze one arm choice (or one read) for
        // every iteration. The body's statements hoist WITHIN the body.
        if let IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } = &mut e.kind {
            hoist_stmt_list_heap_if(body, next, changed);
            return;
        }
        // Children first, so an inner call's hoists land before the outer's.
        almide_ir::visit_mut::walk_expr_mut(
            &mut HoistWalk { binds, next, changed },
            e,
        );
        let IrExprKind::Call { args, .. } = &mut e.kind else { return };
        if !args.iter().any(qualifies) {
            return;
        }
        *changed = true;
        for a in args.iter_mut() {
            if is_atom(a) {
                continue;
            }
            let tmp = VarId(*next);
            *next += 1;
            let bound = std::mem::replace(
                a,
                IrExpr {
                    kind: IrExprKind::Var { id: tmp },
                    ty: a.ty.clone(),
                    span: a.span.clone(),
                    def_id: None,
                },
            );
            binds.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: tmp,
                    mutability: Mutability::Let,
                    ty: bound.ty.clone(),
                    value: bound,
                },
                span: a.span.clone(),
            });
        }
    }
    struct HoistWalk<'a> {
        binds: &'a mut Vec<IrStmt>,
        next: &'a mut u32,
        changed: &'a mut bool,
    }
    impl almide_ir::IrMutVisitor for HoistWalk<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            hoist(e, self.binds, self.next, self.changed);
        }
    }

    fn hoist_stmt_list_heap_if(stmts: &mut Vec<IrStmt>, next: &mut u32, changed: &mut bool) {
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut s in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut s.kind {
                IrStmtKind::Bind { value, .. }
                | IrStmtKind::Assign { value, .. }
                | IrStmtKind::Expr { expr: value }
                | IrStmtKind::BindDestructure { value, .. } => {
                    hoist(value, &mut binds, next, changed)
                }
                IrStmtKind::IndexAssign { index, value, .. } => {
                    hoist(index, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::MapInsert { key, value, .. } => {
                    hoist(key, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::FieldAssign { value, .. } => hoist(value, &mut binds, next, changed),
                _ => {}
            }
            out.extend(binds);
            out.push(s);
        }
        *stmts = out;
    }
    fn rewrite_block(stmts: &mut Vec<IrStmt>, tail: &mut Option<Box<IrExpr>>, next: &mut u32, changed: &mut bool) {
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut s in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut s.kind {
                IrStmtKind::Bind { value, .. }
                | IrStmtKind::Assign { value, .. }
                | IrStmtKind::Expr { expr: value }
                | IrStmtKind::BindDestructure { value, .. } => {
                    hoist(value, &mut binds, next, changed)
                }
                IrStmtKind::IndexAssign { index, value, .. } => {
                    hoist(index, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::MapInsert { key, value, .. } => {
                    hoist(key, &mut binds, next, changed);
                    hoist(value, &mut binds, next, changed);
                }
                IrStmtKind::FieldAssign { value, .. } => hoist(value, &mut binds, next, changed),
                _ => {}
            }
            out.extend(binds);
            out.push(s);
        }
        *stmts = out;
        if let Some(t) = tail {
            let mut binds = Vec::new();
            hoist(t, &mut binds, next, changed);
            if !binds.is_empty() {
                stmts.extend(binds);
            }
        }
    }

    struct BlockWalk<'a> {
        next: &'a mut u32,
        changed: &'a mut bool,
    }
    impl almide_ir::IrMutVisitor for BlockWalk<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::visit_mut::walk_expr_mut(self, e);
            if let IrExprKind::Block { stmts, expr } = &mut e.kind {
                rewrite_block(stmts, expr, self.next, self.changed);
            }
        }
    }

    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let mut out = body.clone();
    almide_ir::IrMutVisitor::visit_expr_mut(&mut BlockWalk { next: &mut next, changed: &mut changed }, &mut out);
    // A non-Block body whose tail needs hoists: wrap it.
    if !matches!(out.kind, IrExprKind::Block { .. }) {
        let mut binds = Vec::new();
        hoist(&mut out, &mut binds, &mut next, &mut changed);
        if !binds.is_empty() {
            let ty = out.ty.clone();
            let span = out.span.clone();
            out = IrExpr {
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(out)) },
                ty,
                span,
                def_id: None,
            };
        }
    }
    changed.then_some(out)
}

/// `"a ${e0} ${ { s; r } } b"  ≡  { let t0 = e0; s; "a ${t0} ${r} b" }` — a string
/// interpolation whose part is a statement block (the C-132 write-back of a `mut`-param
/// call, `"${later(g)}"` → `{ let (r, b) = later(g); g = b; r }`) absorbs the block's
/// statements, so the interp the lowering sees has only plain operands; a call holding it
/// then absorbs the resulting block like any block argument ([`hoist_block_call_args`]).
///
/// Evaluation order is kept: an EARLIER part that calls something or reads a variable
/// the block writes is bound to a fresh `let` first, in order, so it still evaluates
/// before the block's statements ([`bind_earlier_interp_parts`] — a heap read through a
/// written variable declines instead, leaving the interp untouched); parts after the
/// block already evaluate after it. Returns whether `e` changed.
fn absorb_interp_block_parts(e: &mut IrExpr, vt: &mut almide_ir::VarTable) -> bool {
    let IrExprKind::StringInterp { parts } = &mut e.kind else { return false };
    let mut hoisted: Vec<almide_ir::IrStmt> = Vec::new();
    while let Some(bi) = first_interp_block_part(parts) {
        let Some(mut binds) = bind_earlier_interp_parts(parts, bi, vt) else { break };
        let almide_ir::IrStringPart::Expr { expr } = &mut parts[bi] else { unreachable!() };
        let IrExprKind::Block { stmts, expr: Some(tail) } = &mut expr.kind else { unreachable!() };
        binds.append(stmts);
        let tail = (**tail).clone();
        *expr = tail;
        hoisted.append(&mut binds);
    }
    if hoisted.is_empty() {
        return false;
    }
    let interp = std::mem::replace(
        e,
        IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
    );
    *e = IrExpr {
        ty: interp.ty.clone(),
        span: interp.span.clone(),
        def_id: None,
        kind: IrExprKind::Block { stmts: hoisted, expr: Some(Box::new(interp)) },
    };
    true
}

/// The index of the first interpolation part that is a block with statements.
fn first_interp_block_part(parts: &[almide_ir::IrStringPart]) -> Option<usize> {
    parts.iter().position(|p| {
        matches!(p, almide_ir::IrStringPart::Expr { expr }
            if matches!(&expr.kind, IrExprKind::Block { stmts, expr: Some(_) } if !stmts.is_empty()))
    })
}

/// Make every part before `bi` safe to evaluate after the block at `bi`, and return the
/// `let`s that do it, in order. Per non-literal part:
///   - it reads no variable the block writes and calls nothing → stays (re-reading it
///     after the block is unobservable);
///   - a SCALAR part, or a heap part that is itself a call (a fresh owned result) → bound
///     to a fresh `let`, so it is computed before the block;
///   - any other heap part (a field or element read through a written variable, an
///     operator over one) → `None`, nothing changed: a `let` of it would be a borrow
///     into a value the block's write may release.
fn bind_earlier_interp_parts(
    parts: &mut [almide_ir::IrStringPart],
    bi: usize,
    vt: &mut almide_ir::VarTable,
) -> Option<Vec<almide_ir::IrStmt>> {
    use almide_ir::{IrStmt, IrStmtKind, IrStringPart, Mutability};
    let (earlier, rest) = parts.split_at_mut(bi);
    let IrStringPart::Expr { expr: block } = &rest[0] else { return None };
    let written = stmts_written_vars(block);
    let mut plan = Vec::with_capacity(earlier.len());
    for p in earlier.iter() {
        let IrStringPart::Expr { expr: a } = p else {
            plan.push(false);
            continue;
        };
        let (reads_written, calls) = part_reads_and_calls(a, &written);
        let bind = match (reads_written || calls, crate::lower::is_heap_ty(&a.ty)) {
            (false, _) => false,
            (true, false) => true,
            (true, true) if matches!(a.kind, IrExprKind::Call { .. }) => true,
            (true, true) => return None,
        };
        plan.push(bind);
    }
    let mut binds = Vec::new();
    for (p, bind) in earlier.iter_mut().zip(plan) {
        let IrStringPart::Expr { expr: a } = p else { continue };
        if !bind {
            continue;
        }
        let ty = a.ty.clone();
        let span = a.span.clone();
        let var = vt.alloc(almide_base::intern::sym("__part"), ty.clone(), Mutability::Let, None);
        let value = std::mem::replace(
            a,
            IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
        );
        binds.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
    }
    Some(binds)
}

/// Whether `e` reads a variable in `written`, and whether it contains a call.
fn part_reads_and_calls(
    e: &IrExpr,
    written: &std::collections::HashSet<almide_ir::VarId>,
) -> (bool, bool) {
    struct R<'a> {
        written: &'a std::collections::HashSet<almide_ir::VarId>,
        reads: bool,
        calls: bool,
    }
    impl almide_ir::visit::IrVisitor for R<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Var { id } if self.written.contains(id) => self.reads = true,
                IrExprKind::Call { .. } => self.calls = true,
                _ => {}
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut r = R { written, reads: false, calls: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut r, e);
    (r.reads, r.calls)
}

/// The variables a block's statements write: assignment targets, field / index / map
/// write targets, and in-place mutator receivers.
fn stmts_written_vars(block: &IrExpr) -> std::collections::HashSet<almide_ir::VarId> {
    use almide_ir::IrStmtKind;
    struct W(std::collections::HashSet<almide_ir::VarId>);
    impl almide_ir::visit::IrVisitor for W {
        fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
            match &s.kind {
                IrStmtKind::Assign { var, .. } => {
                    self.0.insert(*var);
                }
                IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::FieldAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. } => {
                    self.0.insert(*target);
                }
                _ => {}
            }
            almide_ir::visit::walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if let Some(v) = crate::lower::inplace_mutated_receiver(e) {
                self.0.insert(v);
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut w = W(std::collections::HashSet::new());
    almide_ir::visit::IrVisitor::visit_expr(&mut w, block);
    w.0
}
