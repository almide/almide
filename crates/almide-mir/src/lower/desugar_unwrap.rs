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
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let mut next_var = max_var_id(body) + 1;
    desugar_effect_unwrap_inner(body, &mut next_var, unit_main)
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

fn desugar_effect_unwrap_inner(body: &IrExpr, next_var: &mut u32, unit_main: bool) -> Option<IrExpr> {
    use almide_ir::{IrMatchArm, IrPattern};
    use almide_lang::types::Ty;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
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
        let cont = desugar_effect_unwrap_inner(&cont, next_var, unit_main).unwrap_or(cont);
        let m = build_unwrap_match(inner, ok_pat, cont, body, next_var, unit_main);
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
        if let Some(nt) = desugar_tail_effect_unwrap(t, next_var, unit_main) {
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
) -> IrExpr {
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
        return IrExpr {
            kind: IrExprKind::Match { subject: Box::new(inner), arms: vec![none_arm, some_arm] },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        };
    }
    let e_var = VarId(*next_var);
    *next_var += 1;
    let err_body = if unit_main {
        // main is void — build the full line (`let $m = "Error: " + e + "\n"`) then abort.
        let e_ref = IrExpr {
            kind: IrExprKind::Var { id: e_var },
            ty: Ty::String,
            span: body.span.clone(),
            def_id: None,
        };
        build_main_die_line(e_ref, body, next_var)
    } else {
        IrExpr {
            kind: IrExprKind::ResultErr {
                expr: Box::new(IrExpr {
                    kind: IrExprKind::Var { id: e_var },
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
    let err_arm = IrMatchArm {
        pattern: IrPattern::Err { inner: Box::new(IrPattern::Bind { var: e_var, ty: Ty::String }) },
        guard: None,
        body: err_body,
    };
    let ok_arm = IrMatchArm { pattern: ok_pat, guard: None, body: cont };
    IrExpr {
        kind: IrExprKind::Match { subject: Box::new(inner), arms: vec![err_arm, ok_arm] },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    }
}

/// Recurse the effect-`!` desugar into RETURN/TAIL positions — an `if`/`match` arm body or a nested
/// block tail — so a stmt-`!` inside a branch (porta_init's `else { … fs.write(p, c)!; … }`,
/// signal_instance's nested if/else arm blocks) desugars to the same nested-match continuation. The
/// err-arm `err(e) => err(e)` propagates to the ENCLOSING fn's `Result[_, String]` return; the
/// continuation nests only in the ok-arm. Each arm/tail recurses INDEPENDENTLY (no duplication), so
/// `count_ir_calls` stays exact (`f()` once, continuation in one arm, `err(e)` is a ctor). HOLE-1
/// (admitted Ok-payload reprs only) is enforced inside the block path (`desugar_effect_unwrap_inner`).
/// Returns `None` if no `!` is reachable in a return position of `tail` (the body keeps its form).
fn desugar_tail_effect_unwrap(tail: &IrExpr, next_var: &mut u32, unit_main: bool) -> Option<IrExpr> {
    match &tail.kind {
        // A nested block — its own stmts/tail may carry a stmt-`!`.
        IrExprKind::Block { .. } => desugar_effect_unwrap_inner(tail, next_var, unit_main),
        IrExprKind::If { cond, then, else_ } => {
            let nt = desugar_tail_effect_unwrap(then, next_var, unit_main);
            let ne = desugar_tail_effect_unwrap(else_, next_var, unit_main);
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
                .map(|a| match desugar_tail_effect_unwrap(&a.body, next_var, unit_main) {
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

/// HOLE-1 GATE: is the `!`-subject's `Result[Ok, Err]` type one whose Ok payload the nested-match
/// continuation can BIND + DROP soundly in this brick? Admit ONLY: `Err == String` (the err-arm
/// reconstructs `err(e: String)`) AND Ok ∈ { scalar / Unit (the `result_heap_err_bind` len-as-tag
/// path, Err-String drop only), String / Value (the `str_heap_bind` flat / value-result drops),
/// List[Value] + the four tuple-Ok shapes (their dedicated RECURSIVE result-drops) }. A heap RECORD /
/// Option-of-record / List[String] / List[Int] / nested Ok payload has NO real recursive drop here —
/// the match-lowering's type dispatch would fall through to the FLAT `heap_elem_lists`/`DropListStr`
/// cert (control_p2.rs:330-332), which LEAKS that payload's nested heap. So REFUSE it: the fn then
/// walls cleanly on the raw `!` (an honest `Unsupported` > a gate-invisible leak).
fn effect_unwrap_admitted(
    result_ty: &Ty,
    layouts: &crate::lower::VariantLayouts,
) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(TypeConstructorId::Result, a) = result_ty else {
        return false;
    };
    if a.len() != 2 || !matches!(a[1], Ty::String) {
        return false;
    }
    let ok = &a[0];
    // scalar / Unit Ok — result_heap_err_bind (the Ok side frees nothing; only Err-String drops).
    if !is_heap_ty(ok) {
        return true;
    }
    // String / Value Ok — the existing str_heap_bind flat / DropResultValue drops.
    if matches!(ok, Ty::String) || is_value_ty(ok) {
        return true;
    }
    // List[<non-heap scalar>] (List[Int]/Float/Bool/…) or `Bytes` Ok — a FLAT block of INLINE
    // scalars (fs.read_bytes' `Result[List[Int], String]`, the `load` shape). The by-type dispatch
    // (control_p2.rs:334-376) routes it to the `else => heap_elem_lists` flat `DropListStr`, which
    // frees the block with NO nested-heap leak — the elements are inline scalars, identical
    // soundness to the existing String-Ok flat path. GATE HARD: a `List[String]/List[record]/
    // List[Value]/nested` element IS heap, so the flat drop would LEAK every element — those keep
    // walling (`is_heap_ty(&le[0])` excludes them; a `List[Value]` Ok takes the dedicated recursive
    // `is_result_listval_ty` branch below, never this flat one).
    if let Ty::Applied(TypeConstructorId::List, le) = ok {
        if le.len() == 1 && !is_heap_ty(&le[0]) {
            return true;
        }
    }
    if matches!(ok, Ty::Applied(TypeConstructorId::Bytes, _) | Ty::Bytes) {
        return true;
    }
    // RECORD-Ok (`Result[FileStat, String]` — fs.stat): the match layer routes a
    // recursive-drop record through `resrec:` (result_ok_record_drop_fn → DropWrapperRec)
    // and a scalar-only record through the flat @12 DropListStr — both exact
    // (control_p2's HOLE-1 machinery). A Named that is a REGISTERED VARIANT stays
    // walled (a variant payload's drop is the variant machinery, not the record path).
    if matches!(ok, Ty::Record { .. }) {
        return true;
    }
    if matches!(ok, Ty::Named(..)) && !layouts.field_is_variant(ok) {
        return true;
    }
    // List[Value] Ok + the (String,Int)/(Value,Int)/(List[String],Int)/(List[Value],Int) tuple-Ok
    // shapes — each has a dedicated RECURSIVE result-drop the match-lowering routes to soundly.
    is_result_listval_ty(result_ty)
        || is_str_int_result_ty(result_ty)
        || is_value_int_result_ty(result_ty)
        || is_list_str_int_result_ty(result_ty)
        || is_list_value_int_result_ty(result_ty)
}

/// Does `e` carry a STATEMENT/let-position effect-`!` (an `Unwrap` bound to a `let` or run as an
/// `Expr` stmt, or a tail/return-position `!`) anywhere a branch reaches, AND is EVERY such `!`'s Ok
/// payload HOLE-1-admitted (`effect_unwrap_admitted`)? Returns `(has_unwrap, all_admitted)`. Used by
/// the statement-control continuation-lift to fire ONLY when a branch genuinely needs the lift (some
/// `!` is present) and the lift's downstream tail effect-unwrap will lower soundly (no unproven-drop
/// Ok payload). A NON-admitted `!` flips `all_admitted` so the lift refuses → the raw `!` walls
/// cleanly (an honest `Unsupported` > a gate-invisible leak).
fn collect_arm_unwrap_admit(
    e: &IrExpr,
    has: &mut bool,
    all_admitted: &mut bool,
    layouts: &crate::lower::VariantLayouts,
) {
    match &e.kind {
        IrExprKind::Unwrap { expr } => {
            *has = true;
            if !effect_unwrap_admitted(&expr.ty, layouts) {
                *all_admitted = false;
            }
            collect_arm_unwrap_admit(expr, has, all_admitted, layouts);
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                match &s.kind {
                    IrStmtKind::Bind { value, .. } => collect_arm_unwrap_admit(value, has, all_admitted, layouts),
                    IrStmtKind::Expr { expr } => collect_arm_unwrap_admit(expr, has, all_admitted, layouts),
                    IrStmtKind::Assign { value, .. } => collect_arm_unwrap_admit(value, has, all_admitted, layouts),
                    _ => {}
                }
            }
            if let Some(t) = expr {
                collect_arm_unwrap_admit(t, has, all_admitted, layouts);
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_arm_unwrap_admit(cond, has, all_admitted, layouts);
            collect_arm_unwrap_admit(then, has, all_admitted, layouts);
            collect_arm_unwrap_admit(else_, has, all_admitted, layouts);
        }
        IrExprKind::Match { subject, arms } => {
            collect_arm_unwrap_admit(subject, has, all_admitted, layouts);
            for a in arms {
                collect_arm_unwrap_admit(&a.body, has, all_admitted, layouts);
            }
        }
        _ => {}
    }
}

/// Push the continuation `{ after_stmts; tail }` into a single branch ARM, FLATTENING the arm's own
/// block so a stmt-position `!` in the arm becomes a TOP-LEVEL stmt of the produced block (reachable
/// by `desugar_effect_unwrap_inner` / `desugar_let_unwrap` once the enclosing branch is in tail
/// position). The arm's own tail expr (if any, and not a trivial Unit) is demoted to an `Expr` stmt
/// before the continuation. Typed at the enclosing fn's return `result_ty`.
fn append_arm_continuation(
    arm: &IrExpr,
    after_stmts: &[IrStmt],
    tail: &Option<Box<IrExpr>>,
    result_ty: &Ty,
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();
    match &arm.kind {
        IrExprKind::Block { stmts: bs, expr: be } => {
            stmts.extend(bs.iter().cloned());
            if let Some(be) = be {
                if !matches!(be.kind, IrExprKind::Unit) {
                    stmts.push(IrStmt {
                        kind: IrStmtKind::Expr { expr: (**be).clone() },
                        span: be.span.clone(),
                    });
                }
            }
        }
        // A trivial Unit arm (`else ()`) contributes no statement — just the continuation.
        IrExprKind::Unit => {}
        _ => stmts.push(IrStmt {
            kind: IrStmtKind::Expr { expr: arm.clone() },
            span: arm.span.clone(),
        }),
    }
    stmts.extend(after_stmts.iter().cloned());
    IrExpr {
        kind: IrExprKind::Block { stmts, expr: tail.clone() },
        ty: result_ty.clone(),
        span: arm.span.clone(),
        def_id: None,
    }
}

/// STATEMENT-CONTROL continuation-lift (the #76 statement-control continuation-lift, deferred from
/// the tail-only `desugar_tail_effect_unwrap`): a block `{ before; S; after }` where `S` is a UNIT
/// `Expr`-statement `if`/`match` whose arm transitively carries a stmt/let effect-`!` (`Unwrap`) and
/// `after` (the remaining stmts + tail) is NON-EMPTY. The stmt-`!` cannot desugar in place (the v1
/// MIR has no mid-function early-return Op) and `desugar_tail_effect_unwrap` only navigates TAIL
/// control flow — so `S` in statement position with a continuation is unreachable. LIFT it: push
/// `after` into EACH of `S`'s arm tails (the proven `desugar_let_bound_heap_branch` tail-DUPLICATION
/// discipline), turning `S` into the block TAIL. The existing tail effect-unwrap then rewrites the
/// `!` into `match f() { err(e) => err(e), ok(x) => { after } }` — the err-arm returning early from
/// the fn, the continuation nesting ONLY in the ok-arm. Lives in the SHARED `desugar_heap_branches`
/// so the duplicated `after` is counted 1:1 by `count_ir_calls` in BOTH the caps gate and the
/// lowering (mir == ir). HOLE-1: fire only when every triggering `!`-subject's Ok payload has a
/// proven drop (`effect_unwrap_admitted`); otherwise leave `S` untouched so the raw `!` walls.
fn desugar_stmt_control_unwrap(
    body: &IrExpr,
    layouts: &crate::lower::VariantLayouts,
) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    for (i, s) in stmts.iter().enumerate() {
        // S = a UNIT `Expr`-statement that is an `if`/`match` (a branch run for effect).
        let IrStmtKind::Expr { expr: s_expr } = &s.kind else {
            continue;
        };
        if !matches!(&s_expr.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) {
            continue;
        }
        if !matches!(s_expr.ty, Ty::Unit) {
            continue;
        }
        // `after` must be NON-EMPTY (a real continuation to lift past `S`).
        let after_stmts = &stmts[i + 1..];
        if after_stmts.is_empty() && tail.is_none() {
            continue;
        }
        // Fire ONLY for a branch that carries a stmt/let `!`, and ONLY when every such `!`'s Ok
        // payload is HOLE-1-admitted (else the lift would feed an unprovable-drop Ok into the tail
        // effect-unwrap — refuse so the raw `!` walls cleanly).
        let (mut has, mut all_admitted) = (false, true);
        collect_arm_unwrap_admit(s_expr, &mut has, &mut all_admitted, layouts);
        if !has || !all_admitted {
            continue;
        }
        // Push `after` into each arm's tail. The lifted branch is typed at the fn return `body.ty`.
        let new_s = match &s_expr.kind {
            IrExprKind::If { cond, then, else_ } => IrExpr {
                kind: IrExprKind::If {
                    cond: cond.clone(),
                    then: Box::new(append_arm_continuation(then, after_stmts, tail, &body.ty)),
                    else_: Box::new(append_arm_continuation(else_, after_stmts, tail, &body.ty)),
                },
                ty: body.ty.clone(),
                span: s_expr.span.clone(),
                def_id: s_expr.def_id,
            },
            IrExprKind::Match { subject, arms } => {
                let new_arms: Vec<almide_ir::IrMatchArm> = arms
                    .iter()
                    .map(|a| almide_ir::IrMatchArm {
                        pattern: a.pattern.clone(),
                        guard: a.guard.clone(),
                        body: append_arm_continuation(&a.body, after_stmts, tail, &body.ty),
                    })
                    .collect();
                IrExpr {
                    kind: IrExprKind::Match { subject: subject.clone(), arms: new_arms },
                    ty: body.ty.clone(),
                    span: s_expr.span.clone(),
                    def_id: s_expr.def_id,
                }
            }
            _ => unreachable!(),
        };
        return Some(IrExpr {
            kind: IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(new_s)) },
            ty: body.ty.clone(),
            span: body.span.clone(),
            def_id: body.def_id,
        });
    }
    None
}

/// `{ …; let r = e!; ok(r) }` ≡ `{ …; e }` — the UNWRAP-REWRAP IDENTITY. `e!` unwraps `Ok(x)→x` /
/// propagates `Err`, and `ok(r)` re-wraps it UNCHANGED, so the trailing `let r = e!; ok(r)` collapses to
/// `e` (a `Result`-typed tail). This is the layer-3 simplifier for porta read_message's Content-Length
/// arms (`{ … let r = parse_and_wrap(body)!; ok(r) }` → `{ … parse_and_wrap(body) }`): the `!`-in-a-
/// heap-result-`if`-arm becomes a bare tail-call arm the real-recursive arm path already lowers. Gated:
/// the `Bind` is the LAST stmt, the tail is exactly `ok(Var r)` over that var, and `e` is `Result`-typed
/// (so the re-wrap is a true identity). Recurses via the shared pipeline (`desugar_nested_branch_arms`),
/// so a deeply-nested arm is reached. Pure IR→IR; NO certificate/Coq change.
pub fn desugar_unwrap_rewrap_identity(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &body.kind else {
        return None;
    };
    // The tail must be `ok(Var r)`.
    let IrExprKind::ResultOk { expr: tail_inner } = &tail.kind else {
        return None;
    };
    let IrExprKind::Var { id: r } = &tail_inner.kind else {
        return None;
    };
    // The LAST stmt must be `let r = e!` (`Bind` of an `Unwrap`) over the SAME var.
    let last = stmts.last()?;
    let IrStmtKind::Bind { var, value, .. } = &last.kind else {
        return None;
    };
    if var != r {
        return None;
    }
    let IrExprKind::Unwrap { expr: e } = &value.kind else {
        return None;
    };
    // `e` must be `Result`-typed (`e!` then `ok(r)` is the identity only when `e` is the SAME `Result`).
    if !e.ty.is_result() {
        return None;
    }
    // `r` is bound at the last stmt and used only in the tail `ok(r)` — removing the bind + collapsing
    // the tail to `e` references it nowhere, so the rewrite is sound.
    let mut new_stmts = stmts[..stmts.len() - 1].to_vec();
    let new_tail = (**e).clone();
    // Drop a now-empty trailing block to just `e` would change the node kind needlessly; keep the Block
    // wrapper (its tail is `e`), which lowers identically.
    let _ = &mut new_stmts;
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: new_stmts, expr: Some(Box::new(new_tail)) },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}

/// `{ …; let v = e!; rest }` — an unwrap-`!` bound to a let (an EFFECT-fn early-return on Err) →
/// `{ …; match e { ok(v) => { rest }, err($x) => err($x) } }`. The `!` IS exactly this: evaluate `e`,
/// bind the Ok payload to `v` and continue, else return the Err from the enclosing fn. Pushing the
/// continuation into the ok-arm makes the `match` the block TAIL, so the err-arm `err($x)` IS the
/// function's return (byte-identical to v0's `?`-style propagation). A SCALAR Ok payload then lowers
/// via the proven scalar-payload value-match; a HEAP Ok payload stays the Camp-4 frontier. Eliminates
/// the unwrap-bound-to-let wall — the top cross-repo wall reason (toml, base64 decode_chunks, porta).
pub fn desugar_let_unwrap(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // A single-var `let v = e!` (Bind-of-Unwrap) OR a destructure `let (a, b) = e!` (BindDestructure
    // whose VALUE is Result-typed — the `!` lowers to a bare Result-typed Call here, not a kept Unwrap
    // node, so key on the TYPE). Both early-return the `Err(E)` and bind/destructure the `Ok(T)`.
    enum Target {
        Single { var: VarId, ty: Ty },
        Destructure { pattern: almide_ir::IrPattern },
    }
    let (i, target, inner) = stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
            // `!` (Unwrap) and `?` (Try) both propagate the `Err(E)` in an effect fn — the SAME
            // early-return this desugar builds. The derive-generated field binds arrive as `Try`
            // (`let _e0 = value.as_int(..)?`) — handle both so they lower to the match, not a
            // heap-result Try left for `lower_call_args` to wall.
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                Some((i, Target::Single { var: *var, ty: ty.clone() }, (**expr).clone()))
            }
            _ => None,
        },
        IrStmtKind::BindDestructure { pattern, value } => {
            // `let (a,b) = e!` / `let (a,b) = e?`. The `!`/`?` is EITHER kept as an `Unwrap`/`Try`
            // node (inner = its Result expr) OR already stripped to a bare Result-typed value (inner
            // = the value) — handle all three so a destructure-let-unwrap never reaches lowering as a
            // Result-destructured-as-a-tuple. The derived variant decode's `let (_tag, _payload) =
            // value.tagged_variant(v)?` is exactly the kept-`Try` case.
            let inner = match &value.kind {
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => Some((**expr).clone()),
                _ if value.ty.is_result() => Some(value.clone()),
                _ => None,
            }?;
            Some((i, Target::Destructure { pattern: pattern.clone() }, inner))
        }
        _ => None,
    })?;
    // The unwrapped expr must be a `Result[T, E]` — `!` early-returns its `Err(E)`, binds `Ok(T)`.
    let (ok_ty, err_ty) = match &inner.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 => (a[0].clone(), a[1].clone()),
        _ => return None,
    };
    let result_ty = body.ty.clone();
    let fresh = VarId(max_var_id(body) + 1);
    let mk = |kind: IrExprKind, ty: Ty| IrExpr {
        kind,
        ty,
        span: body.span.clone(),
        def_id: body.def_id,
    };
    // ok(<bind>) => { <rest> }. A destructure becomes `ok($p2) => { let (a,b) = $p2; <rest> }` — the
    // hand-written direct-match form that already lowers (a Result destructured directly as a tuple
    // otherwise silently miscompiled: the wrapper @12/@16 was read as the tuple fields).
    let (ok_pattern, cont_stmts): (almide_ir::IrPattern, Vec<IrStmt>) = match target {
        // A UNIT Ok payload bound to a NEVER-READ var (`let _ = fs.write(p, s)!` — the
        // frontend gives `_` a real VarId): normalize to the Wildcard arm (exactly the
        // bare-stmt `!` shape), so the statement result-match parser dispatches it
        // instead of declining on a Unit-typed bind. A genuinely-read var keeps its bind.
        Target::Single { var, ty }
            if matches!(ty, Ty::Unit)
                && !stmts[i + 1..].iter().any(|s| {
                    let mut f = false;
                    almide_ir::visit::walk_stmt(&mut VarUse { var, found: &mut f }, s);
                    f
                })
                && !tail.as_deref().is_some_and(|tl| {
                    let mut f = false;
                    almide_ir::visit::IrVisitor::visit_expr(
                        &mut VarUse { var, found: &mut f },
                        tl,
                    );
                    f
                }) =>
        {
            (almide_ir::IrPattern::Wildcard, stmts[i + 1..].to_vec())
        }
        Target::Single { var, ty } => {
            (almide_ir::IrPattern::Bind { var, ty }, stmts[i + 1..].to_vec())
        }
        Target::Destructure { pattern } => {
            let p2 = VarId(max_var_id(body) + 2);
            let destr = IrStmt {
                kind: IrStmtKind::BindDestructure {
                    pattern,
                    value: mk(IrExprKind::Var { id: p2 }, ok_ty.clone()),
                },
                span: body.span.clone(),
            };
            let mut cs = vec![destr];
            cs.extend(stmts[i + 1..].iter().cloned());
            (almide_ir::IrPattern::Bind { var: p2, ty: ok_ty.clone() }, cs)
        }
    };
    let cont = mk(
        IrExprKind::Block { stmts: cont_stmts, expr: tail.clone() },
        result_ty.clone(),
    );
    let ok_arm = almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Ok { inner: Box::new(ok_pattern) },
        guard: None,
        body: cont,
    };
    // err($x) => err($x)  (the propagated error IS the function result)
    let err_var = mk(IrExprKind::Var { id: fresh }, err_ty.clone());
    let err_body = mk(IrExprKind::ResultErr { expr: Box::new(err_var) }, result_ty.clone());
    let err_arm = almide_ir::IrMatchArm {
        pattern: almide_ir::IrPattern::Err {
            inner: Box::new(almide_ir::IrPattern::Bind { var: fresh, ty: err_ty }),
        },
        guard: None,
        body: err_body,
    };
    let match_expr = mk(
        IrExprKind::Match { subject: Box::new(inner), arms: vec![ok_arm, err_arm] },
        result_ty.clone(),
    );
    Some(mk(
        IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(match_expr)) },
        result_ty,
    ))
}

// ─────────── effect-`!` inside a `for` loop body → loop-carried error flag ───────────
//
// A `for x in xs { … e! … }` in a function returning `Result[T, E]` cannot early-return the
// `Err(E)` from inside the loop in the FLAT certificate (a mid-loop `return` makes the loop's
// owned iterable + iteration transients conditionally dropped — a double-drop the flat checker
// rejects). This is the effect-monad-in-loop frontier.
//
// REWRITE (a PURE IR→IR desugar — "desugar-before-both", no new MIR op, no certificate/Coq
// change; it reuses the PROVEN loop-carried scalar/heap slot reassignment + heap-result-`if`):
//
//   { <pre>; for x in xs { BODY }; <post> }
//     ↓
//   { <pre>;
//     var __ef = false;            // err flag (scalar loop-carried)
//     var __ev = <empty E>;        // err accumulator (heap loop-carried slot, reassigned on Err)
//     for x in xs { if not __ef then { BODY' } else () };
//     if __ef then err(__ev) else { <post> } }
//
// where BODY' replaces each `let v = e!` / `e!` with `match e { ok(v) => <rest>, err($x) =>
// { __ef = true; __ev = $x } }`. BYTE-IDENTICAL to early-return: once `__ef` is set the per-
// iteration `if not __ef` guard skips ALL remaining effects (no later `e!`/`println` runs), the
// loop terminates by exhausting `xs` (a BOUNDED iteration — this is why it is `for` ONLY: a
// `while` could spin forever once its progress update is skipped), and the post-loop dispatch
// yields the propagated `Err`. Per-iteration transients are dropped by the normal iter-scope
// (there is NO early exit), so no special leak handling is needed.
//
// FAIL-SAFE: any `!` the rewrite cannot place a clean continuation behind is left untouched, so
// the function still WALLS at lowering (never a silent miscompile).

/// Does `e` contain an effect-`!` (`Unwrap`) anywhere in its subtree?
fn expr_has_unwrap(e: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct U(bool);
    impl IrVisitor for U {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Unwrap { .. }) {
                self.0 = true;
            }
            walk_expr(self, e);
        }
    }
    let mut u = U(false);
    u.visit_expr(e);
    u.0
}

/// Does statement `s` contain an effect-`!` anywhere in its subtree?
fn stmt_has_unwrap(s: &IrStmt) -> bool {
    use almide_ir::visit::{walk_stmt, IrVisitor};
    struct U(bool);
    impl IrVisitor for U {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Unwrap { .. }) {
                self.0 = true;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut u = U(false);
    walk_stmt(&mut u, s);
    u.0
}

/// `let v = if c then e! else d` — an unwrap-`!` INSIDE an `if` arm (base64 decode_chunks:
/// `let v1 = if remaining > 1 then char_to_val(..)! else 0`). LIFT the `!` out of the arm: wrap each
/// NON-unwrap arm in `ok(..)` and strip `!` from each unwrap arm, so the `if` becomes a `Result[T,E]`,
/// bind it to `$r`, then `let v = $r!` — a plain let-unwrap the [`desugar_let_unwrap`] pass then
/// handles (and the let-bound heap-result `if` `$r` the pre-TCO pass tail-duplicates). Composes the
/// two existing desugars to reach the unwrap-in-if-arm shape. v0's `!` early-returns identically.
pub fn desugar_if_arm_unwrap(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    let (i, bind_var, bind_ty, cond, then_e, else_e) =
        stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
            IrStmtKind::Bind { var, ty, value, .. } => match &value.kind {
                IrExprKind::If { cond, then, else_ }
                    if matches!(&then.kind, IrExprKind::Unwrap { .. })
                        || matches!(&else_.kind, IrExprKind::Unwrap { .. }) =>
                {
                    Some((i, *var, ty.clone(), (**cond).clone(), (**then).clone(), (**else_).clone()))
                }
                _ => None,
            },
            _ => None,
        })?;
    // The `Result[T, E]` type the arms unify to — take it from an unwrap arm's operand.
    let res_ty = match (&then_e.kind, &else_e.kind) {
        (IrExprKind::Unwrap { expr }, _) | (_, IrExprKind::Unwrap { expr }) => expr.ty.clone(),
        _ => return None,
    };
    let mk = |kind: IrExprKind, ty: Ty| IrExpr {
        kind,
        ty,
        span: body.span.clone(),
        def_id: body.def_id,
    };
    // strip `!` from an unwrap arm (already Result[T,E]); wrap a plain arm in `ok(..)`.
    let conv = |arm: IrExpr| -> IrExpr {
        match arm.kind {
            IrExprKind::Unwrap { expr } => *expr,
            _ => mk(IrExprKind::ResultOk { expr: Box::new(arm) }, res_ty.clone()),
        }
    };
    let new_if = mk(
        IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(conv(then_e)),
            else_: Box::new(conv(else_e)),
        },
        res_ty.clone(),
    );
    let r_var = VarId(max_var_id(body) + 1);
    let r_bind = IrStmt {
        kind: IrStmtKind::Bind {
            var: r_var,
            ty: res_ty.clone(),
            value: new_if,
            mutability: almide_ir::Mutability::Let,
        },
        span: None,
    };
    let v_value = mk(
        IrExprKind::Unwrap { expr: Box::new(mk(IrExprKind::Var { id: r_var }, res_ty)) },
        bind_ty.clone(),
    );
    let v_bind = IrStmt {
        kind: IrStmtKind::Bind {
            var: bind_var,
            ty: bind_ty,
            value: v_value,
            mutability: almide_ir::Mutability::Let,
        },
        span: None,
    };
    let mut new_stmts = stmts[..i].to_vec();
    new_stmts.push(r_bind);
    new_stmts.push(v_bind);
    new_stmts.extend_from_slice(&stmts[i + 1..]);
    Some(mk(IrExprKind::Block { stmts: new_stmts, expr: tail.clone() }, body.ty.clone()))
}

