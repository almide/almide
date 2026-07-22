/// Desugar a STATEMENT/let-bind effect-`!` (`Unwrap`) into a NESTED-MATCH continuation — the standard
/// monadic do-desugar — so a CAN-ERR effect-`!` propagates without a mid-function early-return (which the
/// v1 MIR has no Op for):
///
///   { before; let x = f()!; after }  →  { before; match f() { err(e) => err(e), ok(x) => { after } } }
///   { before;     f()!    ; after }  →  { before; match f() { err(e) => err(e), ok(_) => { after } } }
///
/// The continuation (`after`) nests in the Ok-arm; the Err-arm reconstructs `err(e)` at the enclosing
/// fn's `Result[_, String]` type. The fn's tail becomes the (nested) match, which the EXISTING
/// heap-result-`match` tail lowering already handles — VERIFIED to byte-match for scalar/String/Value/
/// record Ok (porta.start's every shape). Call-count-INVARIANT (`f()` appears once before and after; the
/// continuation nests in ONE arm — no duplication; `err(e)` is a constructor, not a call), so `mir == ir`
/// holds without the count gate re-running it. Only a TOP-LEVEL stmt `!` (a `let x = f()!` Bind value or
/// a bare `f()!` Expr stmt); a tail `f()!` is the fn's return (tail.rs pass-through), and a `!` nested in
/// an operand is handled by `desugar_callarg_unwrap`. Closes the porta.start / dojo effect-monad wall
/// WITHOUT the major Return-op subsystem.
pub fn desugar_effect_unwrap(
    body: &IrExpr,
    unit_main: bool,
    ret_is_result: bool,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let mut next_var = max_var_id(body) + 1;
    desugar_effect_unwrap_inner(body, &mut next_var, unit_main, ret_is_result)
}

/// A tiny read-scan: does the expression tree reference `var`? (The unit-discard
/// normalization's "never read" gate.)
struct VarUse<'a> {
    var: VarId,
    found: &'a mut bool,
}
impl almide_ir::visit::IrVisitor for VarUse<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if matches!(&e.kind, IrExprKind::Var { id } if *id == self.var) {
            *self.found = true;
        }
        almide_ir::visit::walk_expr(self, e);
    }
}

fn desugar_effect_unwrap_inner(
    body: &IrExpr,
    next_var: &mut u32,
    unit_main: bool,
    ret_is_result: bool,
) -> Option<IrExpr> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::Ty;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        // An EXPRESSION-FORM body (`effect fn f(..) -> Result[..] = list.get(xs, i)!`) is kept
        // BARE by the frontend — no Block wrapper — so the tail-position machinery below never
        // saw it and the bare Option-`!` fell straight to the wrong-repr pass-through (the same
        // confirmed wrong-value bug the Block-tail case has). The body IS the tail: delegate the
        // bare-Unwrap case directly. Gated to exactly that shape — any other non-Block body
        // keeps today's behavior untouched.
        if matches!(&body.kind, IrExprKind::Unwrap { .. }) {
            return desugar_tail_effect_unwrap(body, next_var, unit_main, ret_is_result);
        }
        return None;
    };
    for (i, s) in stmts.iter().enumerate() {
        // A TOP-LEVEL effect-`!` — the WHOLE stmt value is `Unwrap { f() }`:
        //   let-bind `let x = f()!` → the Ok-arm binds `x` to the payload
        //   bare stmt `f()!`        → the Ok-arm discards the (Unit/None) payload (`_`)
        // The desugar is repr-AGNOSTIC: it admits Option-`!` and Result-`!` uniformly — the same
        // `err(e) => err(e), ok(x) => cont` skeleton lowers through the shared variant-match path for
        // both (Option=none/some, Result=err/ok positionally, one len-as-tag repr). HOLE-1 (the
        // record-Ok recursive-drop leak) is gated NOT here but at the match-LOWERING mechanism
        // (`try_lower_variant_value_match` excludes a record-Ok subject from the str-result/
        // `heap_elem_lists` tracking via `is_record_result_ty`, so it rolls back → walls cleanly),
        // because identifying a record-Ok payload needs the record-layout registry the desugar lacks.
        // `Try` is the frontend's auto-`?` on an UN-annotated bind of a declared-Result
        // effect call (`let v = declared_result()`) — the same monadic coercion as a
        // spelled-out `!`, so both desugar identically. (A Try left unhandled emitted a
        // bare dst-less `(call $f)` whose Result handle stayed on the wasm stack —
        // effect_assign_unwrap's `unannotated=` leg, 2026-07-03.)
        let (inner, ok_pat) = match &s.kind {
            IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => (
                    (**expr).clone(),
                    IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: *var, ty: ty.clone() }) },
                ),
                _ => continue,
            },
            IrStmtKind::Expr { expr } => match &expr.kind {
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                    ((**expr).clone(), IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) })
                }
                _ => continue,
            },
            _ => continue,
        };
        // A UNIT Ok payload bound to a NEVER-READ var (`let _ = fs.write(p, s)!` — the
        // frontend gives `_` a real VarId) is semantically a discard: normalize it to the
        // Wildcard arm (exactly the statement-`!` shape above), so the statement
        // result-match parser dispatches it instead of declining on a Unit-typed bind.
        // Gated on ty == Unit AND the continuation (rest stmts + tail) never referencing
        // the var — a genuinely-read unit var keeps its bind.
        let ok_pat = match ok_pat {
            IrPattern::Ok { inner: b } => match *b {
                IrPattern::Bind { var, ty: bty }
                    if matches!(bty, Ty::Unit)
                        && !stmts[i + 1..].iter().any(|s| {
                            let mut f = false;
                            almide_ir::visit::walk_stmt(
                                &mut VarUse { var, found: &mut f },
                                s,
                            );
                            f
                        })
                        && !tail.as_deref().is_some_and(|t| {
                            let mut f = false;
                            almide_ir::visit::IrVisitor::visit_expr(
                                &mut VarUse { var, found: &mut f },
                                t,
                            );
                            f
                        }) =>
                {
                    IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) }
                }
                other => IrPattern::Ok { inner: Box::new(other) },
            },
            p => p,
        };
        // An Option-`!` admits BOTH scalar and heap Some payloads: a heap payload binds
        // as a @12 BORROW over the tracked subject (the heap_elem_lists discipline the
        // Option match machinery already proves — matrix_misc's `list.get(chunks, 0)!`
        // Matrix payload); an untracked/unliftable shape still walls honestly at the
        // match layer (rollback → the untracked-subject wall, never wrong bytes).
        if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, a) = &inner.ty {
            if a.len() != 1 {
                continue;
            }
        }
        // The continuation = the rest of the block `{ stmts[i+1..]; tail }`, typed as the whole body
        // (it produces the fn's return). RECURSE so a LATER `!` in the continuation also desugars.
        let cont = IrExpr {
            kind: IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.clone() },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        };
        let cont =
            desugar_effect_unwrap_inner(&cont, next_var, unit_main, ret_is_result).unwrap_or(cont);
        // `build_unwrap_match` declines a non-convertible err-type mismatch (the propagated
        // Result's err differs from the fn's and is not the List[String]→String join class):
        // leave the `!` bind in place so it walls honestly downstream, never a punned payload.
        let Some(m) = build_unwrap_match(inner, ok_pat, cont, body, next_var, unit_main) else {
            continue;
        };
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(m)) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    // No TOP-LEVEL stmt-`!` in this block — RECURSE INTO THE TAIL. A tail `if`/`match`/block is a
    // RETURN position whose arm bodies may carry a stmt-`!` (porta_init's `else { … fs.write(p,c)!;
    // … }`, signal_instance's nested if-arms). `desugar_tail_effect_unwrap` navigates that control
    // flow and applies the SAME gated stmt-`!` rewrite inside each arm/tail block.
    if let Some(t) = tail.as_deref() {
        if let Some(nt) = desugar_tail_effect_unwrap(t, next_var, unit_main, ret_is_result) {
            return Some(IrExpr {
                kind: IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(nt)) },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            });
        }
    }
    None
}

/// Build the nested-match `match inner { err(e) => err(e), ok(<ok_pat>) => <cont> }` at the enclosing
/// fn's `Result[_, String]` return type (`body.ty`). The err-arm is a REAL move-out `err(e) => err(e)`
/// (NOT a strip): a fresh `e: String` bound by the Err-pattern, reconstructed as `ResultErr` at the
/// return type. The continuation nests ONLY in the ok-arm. Shared by the stmt-position and the
/// tail/return-position (`desugar_tail_effect_unwrap`) rewrites.
/// Build the UNIT-MAIN fail-arm body: v0's main wrapper prints `Error: <msg>` to STDERR and
/// exits 1 — main is declared `-> Unit` (the void convention: an `err(…)` arm value would be
/// silently DISCARDED), so the arm aborts through the prim floor instead:
/// `prim.die(prim.handle(<full line>))` ($__die = STDERR write + proc_exit(1), the
/// int_rotate/math_int precedent). `msg` is either the static line (Option-`!`: "Error:
/// none\n") or a bound payload Var (Result-`!`: a `let $m = "Error: " + e + "\n"` bind
/// precedes the die and the die consumes $m's handle — the arm's scope-end drop after the
/// never-returning die balances the cert, exactly the checked-overflow abort shape).
/// Build the whole unit-main die BLOCK for a DYNAMIC String payload `e`:
/// `{ let $m = "Error: " + e + "\n"; prim.die(prim.handle($m)) }` — the v0 main-err line.
fn build_main_die_line(payload: IrExpr, at: &IrExpr, next_var: &mut u32) -> IrExpr {
    use almide_ir::BinOp;
    use almide_lang::types::Ty;
    let m_var = VarId(*next_var);
    *next_var += 1;
    let lit = |v: &str| IrExpr {
        kind: IrExprKind::LitStr { value: v.into() },
        ty: Ty::String,
        span: at.span.clone(),
        def_id: None,
    };
    let concat = |a: IrExpr, b: IrExpr| IrExpr {
        kind: IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(a), right: Box::new(b) },
        ty: Ty::String,
        span: at.span.clone(),
        def_id: None,
    };
    let line = concat(concat(lit("Error: "), payload), lit("\n"));
    let m_ref = IrExpr {
        kind: IrExprKind::Var { id: m_var },
        ty: Ty::String,
        span: at.span.clone(),
        def_id: None,
    };
    // SPLIT form (`let $h = prim.handle($m); prim.die($h)`): a NESTED `prim.handle(<Var>)`
    // inside a match ARM declines at the arm's prim lowering (the top-level form is fine) —
    // the split is the shape the arm path proves.
    let h_var = VarId(*next_var);
    *next_var += 1;
    let handle_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_lang::intern::sym("prim"),
                func: almide_lang::intern::sym("handle"),
                def_id: None,
            },
            args: vec![m_ref],
            type_args: Vec::new(),
        },
        ty: Ty::Int,
        span: at.span.clone(),
        def_id: None,
    };
    let h_ref =
        IrExpr { kind: IrExprKind::Var { id: h_var }, ty: Ty::Int, span: at.span.clone(), def_id: None };
    let die_call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_lang::intern::sym("prim"),
                func: almide_lang::intern::sym("die"),
                def_id: None,
            },
            args: vec![h_ref],
            type_args: Vec::new(),
        },
        ty: Ty::Unit,
        span: at.span.clone(),
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: m_var,
                        ty: Ty::String,
                        value: line,
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: at.span.clone(),
                },
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: h_var,
                        ty: Ty::Int,
                        value: handle_call,
                        mutability: almide_ir::Mutability::Let,
                    },
                    span: at.span.clone(),
                },
            ],
            expr: Some(Box::new(die_call)),
        },
        ty: Ty::Unit,
        span: at.span.clone(),
        def_id: at.def_id,
    }
}

/// UNIT-MAIN auto-? residue: the FRONTEND builds `match f() { ok(v) => …, err(e) => err(e) }`
/// directly (no `Unwrap` node reaches the MIR desugar), and in the VOID main the `err(e)` arm
/// value is DISCARDED — the failure path silently exited 0 (v0: `Error: <msg>` + exit 1).
/// Rewrite every bare-`ResultErr` arm body under a UNIT-typed match to the same die block the
/// `!` desugar builds. Sound: a user cannot type-check `err(e)` as a Unit match arm, so every
/// such arm IS the auto-? artifact (main's early error return). Gated to main by the caller.
pub fn desugar_unit_main_err_arms(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::Ty;
    struct Rw {
        next_var: u32,
        changed: bool,
    }
    impl IrMutVisitor for Rw {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Match { arms, .. } = &mut e.kind else { return };
            if !matches!(e.ty, Ty::Unit) {
                return;
            }
            for arm in arms.iter_mut() {
                if let IrExprKind::ResultErr { expr } = &arm.body.kind {
                    if matches!(expr.ty, Ty::String) {
                        let payload = (**expr).clone();
                        let at = arm.body.clone();
                        arm.body = build_main_die_line(payload, &at, &mut self.next_var);
                        self.changed = true;
                    }
                }
            }
        }
    }
    let mut rw = Rw { next_var: max_var_id(body) + 1, changed: false };
    let mut out = body.clone();
    rw.visit_expr_mut(&mut out);
    rw.changed.then_some(out)
}

fn build_main_die(msg: IrExpr, body: &IrExpr) -> IrExpr {
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let handle = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("prim"), func: sym("handle"), def_id: None },
            args: vec![msg],
            type_args: Vec::new(),
        },
        ty: Ty::Int,
        span: body.span.clone(),
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: sym("prim"), func: sym("die"), def_id: None },
            args: vec![handle],
            type_args: Vec::new(),
        },
        ty: Ty::Unit,
        span: body.span.clone(),
        def_id: None,
    }
}

fn build_unwrap_match(
    inner: IrExpr,
    ok_pat: almide_ir::IrPattern,
    cont: IrExpr,
    body: &IrExpr,
    next_var: &mut u32,
    unit_main: bool,
) -> Option<IrExpr> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    // An OPTION subject (`int.to_int8_checked(v)!` — the sized_conversion family): the fail
    // arm is `none => err("none")` (v0's unwrap-of-none message, oracle: `Error: none`,
    // exit 1), and the continuation binds under the SOME pattern — Option's len-as-tag
    // polarity is OPPOSITE Result's (Some = len 1, Err = len 1), so reusing the Ok/Err
    // skeleton would fire the fail arm on the SUCCESS value.
    if matches!(&inner.ty, Ty::Applied(TypeConstructorId::Option, _)) {
        let none_body = if unit_main {
            // main is void — abort with v0's whole line instead of a discarded err().
            build_main_die(
                IrExpr {
                    kind: IrExprKind::LitStr { value: "Error: none\n".into() },
                    ty: Ty::String,
                    span: body.span.clone(),
                    def_id: None,
                },
                body,
            )
        } else {
            IrExpr {
                kind: IrExprKind::ResultErr {
                    expr: Box::new(IrExpr {
                        kind: IrExprKind::LitStr { value: "none".into() },
                        ty: Ty::String,
                        span: body.span.clone(),
                        def_id: None,
                    }),
                },
                ty: body.ty.clone(),
                span: body.span.clone(),
                def_id: body.def_id,
            }
        };
        let none_arm = IrMatchArm { pattern: IrPattern::None, guard: None, body: none_body };
        let some_pat = match ok_pat {
            IrPattern::Ok { inner } => IrPattern::Some { inner },
            other => other,
        };
        let some_arm = IrMatchArm { pattern: some_pat, guard: None, body: cont };
        return Some(IrExpr {
            kind: IrExprKind::Match { subject: Box::new(inner), arms: vec![none_arm, some_arm] },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    let e_var = VarId(*next_var);
    *next_var += 1;
    // TYPE-DRIVEN err propagation (2026-07-17): bind the Err payload at ITS OWN type — the
    // old unconditional `Ty::String` bind type-punned every non-String err. A same-type err
    // passes through unchanged; a List[String] err into a String-err fn joins ", " (exactly
    // v0's `.map_err(|errs| errs.join(", "))?` — `result.collect/collect_map(..)!`); any
    // other mismatch DECLINES so the `!` walls honestly downstream. The SAME conversion
    // lives in `desugar_let_unwrap` — the lowering chain reaches that one first (via
    // `desugar_heap_branches`), this one first in the counted `desugar_all` order — so the
    // call counts agree on both sides (mir == ir, the caps-gate contract).
    let inner_err_ty = match &inner.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[1].clone(),
        _ => Ty::String,
    };
    let fn_err_ty = match &body.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => a[1].clone(),
        // main / a lifted effect body: the synthetic Result errs String.
        _ => Ty::String,
    };
    let e_ref = IrExpr {
        kind: IrExprKind::Var { id: e_var },
        ty: inner_err_ty.clone(),
        span: body.span.clone(),
        def_id: None,
    };
    let payload = if inner_err_ty == fn_err_ty {
        e_ref
    } else if matches!(&inner_err_ty,
            Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::String))
        && matches!(fn_err_ty, Ty::String)
    {
        IrExpr {
            kind: IrExprKind::Call {
                target: almide_ir::CallTarget::Module {
                    module: almide_lang::intern::sym("list"),
                    func: almide_lang::intern::sym("join"),
                    def_id: None,
                },
                args: vec![
                    e_ref,
                    IrExpr {
                        kind: IrExprKind::LitStr { value: ", ".into() },
                        ty: Ty::String,
                        span: body.span.clone(),
                        def_id: None,
                    },
                ],
                type_args: vec![],
            },
            ty: Ty::String,
            span: body.span.clone(),
            def_id: None,
        }
    } else {
        return None;
    };
    let err_body = if unit_main {
        // main is void — build the full line (`let $m = "Error: " + e + "\n"`) then abort.
        // `payload` is String here by construction (main's fn_err defaults String, so a
        // same-type pass-through IS a String and the join class produces one).
        build_main_die_line(payload, body, next_var)
    } else {
        IrExpr {
            kind: IrExprKind::ResultErr { expr: Box::new(payload) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        }
    };
    let err_arm = IrMatchArm {
        pattern: IrPattern::Err {
            inner: Box::new(IrPattern::Bind { var: e_var, ty: inner_err_ty }),
        },
        guard: None,
        body: err_body,
    };
    let ok_arm = IrMatchArm { pattern: ok_pat, guard: None, body: cont };
    Some(IrExpr {
        kind: IrExprKind::Match { subject: Box::new(inner), arms: vec![err_arm, ok_arm] },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// Recurse the effect-`!` desugar into RETURN/TAIL positions — an `if`/`match` arm body or a nested
/// block tail — so a stmt-`!` inside a branch (porta_init's `else { … fs.write(p, c)!; … }`,
/// signal_instance's nested if/else arm blocks) desugars to the same nested-match continuation. The
/// err-arm `err(e) => err(e)` propagates to the ENCLOSING fn's `Result[_, String]` return; the
/// continuation nests only in the ok-arm. Each arm/tail recurses INDEPENDENTLY (no duplication), so
/// `count_ir_calls` stays exact (`f()` once, continuation in one arm, `err(e)` is a ctor). HOLE-1
/// (admitted Ok-payload reprs only) is enforced inside the block path (`desugar_effect_unwrap_inner`).
/// Returns `None` if no `!` is reachable in a return position of `tail` (the body keeps its form).
fn desugar_tail_effect_unwrap(
    tail: &IrExpr,
    next_var: &mut u32,
    unit_main: bool,
    ret_is_result: bool,
) -> Option<IrExpr> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    match &tail.kind {
        // A nested block — its own stmts/tail may carry a stmt-`!`.
        IrExprKind::Block { .. } => {
            desugar_effect_unwrap_inner(tail, next_var, unit_main, ret_is_result)
        }
        // A BARE tail-position OPTION-`!` (`{ let o = r!; o! }`'s continuation, or a plainly
        // declared-Result fn's `{ let o: Option[Int] = some(42); o! }` / `{ …; list.get(xs, i)! }`)
        // — WITHOUT this desugar it falls through to `tail.rs`'s raw pass-through, which is a
        // correct no-op ONLY for a RESULT operand (same repr the fn already returns); an OPTION
        // operand has a DIFFERENT repr, so the pass-through silently returned the RAW Option
        // handle AS the Result — a CONFIRMED live wrong-value bug (v0 prints the payload, v1
        // printed `Error: `), reachable with NO auto-wrap involved at all. Desugar it into the
        // real none/some match (`match o { none => err("none"), some(v) => ok(v) }` — v0's
        // unwrap-of-none message), typed at the SYNTHESIZED `Result[T, String]` (from the Option's
        // own payload type — NOT `tail.ty`/`body.ty`, whose values proved unreliable across the
        // desugar fixpoint's interleaved probe/lowering streams in two prior reverted attempts;
        // `ret_is_result` is threaded EXPLICITLY from `lower_body_into` exactly like `unit_main`,
        // so the gate is a per-fn FACT, not a fragile tree-local type). A RESULT operand keeps
        // the (correct) pass-through; a non-Result-ABI fn (`unwrap_option_some`'s declared `->
        // Int`, where the raw payload IS the return) keeps its pre-existing path via the gate.
        IrExprKind::Unwrap { expr }
            if ret_is_result
                && matches!(&expr.ty,
                    Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1) =>
        {
            let Ty::Applied(TypeConstructorId::Option, a) = &expr.ty else { unreachable!() };
            let payload_ty = a[0].clone();
            let result_ty = Ty::result(payload_ty.clone(), Ty::String);
            let v = VarId(*next_var);
            *next_var += 1;
            let none_arm = IrMatchArm {
                pattern: IrPattern::None,
                guard: None,
                body: IrExpr {
                    kind: IrExprKind::ResultErr {
                        expr: Box::new(IrExpr {
                            kind: IrExprKind::LitStr { value: "none".into() },
                            ty: Ty::String,
                            span: tail.span.clone(),
                            def_id: None,
                        }),
                    },
                    ty: result_ty.clone(),
                    span: tail.span.clone(),
                    def_id: None,
                },
            };
            let some_arm = IrMatchArm {
                pattern: IrPattern::Some {
                    inner: Box::new(IrPattern::Bind { var: v, ty: payload_ty.clone() }),
                },
                guard: None,
                body: IrExpr {
                    kind: IrExprKind::ResultOk {
                        expr: Box::new(IrExpr {
                            kind: IrExprKind::Var { id: v },
                            ty: payload_ty,
                            span: tail.span.clone(),
                            def_id: None,
                        }),
                    },
                    ty: result_ty.clone(),
                    span: tail.span.clone(),
                    def_id: None,
                },
            };
            Some(IrExpr {
                kind: IrExprKind::Match {
                    subject: Box::new((**expr).clone()),
                    arms: vec![none_arm, some_arm],
                },
                ty: result_ty,
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_tail_effect_unwrap(then, next_var, unit_main, ret_is_result);
            let ne = desugar_tail_effect_unwrap(else_, next_var, unit_main, ret_is_result);
            if nt.is_none() && ne.is_none() {
                return None;
            }
            Some(IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(nt.unwrap_or_else(|| (**then).clone())),
                    else_: Box::new(ne.unwrap_or_else(|| (**else_).clone())),
                },
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        IrExprKind::Match { subject, arms } => {
            let mut changed = false;
            let new_arms: Vec<almide_ir::IrMatchArm> = arms
                .iter()
                .map(|a| match desugar_tail_effect_unwrap(&a.body, next_var, unit_main, ret_is_result) {
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
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            })
        }
        _ => None,
    }
}
