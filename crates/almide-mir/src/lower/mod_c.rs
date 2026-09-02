
/// `buf[i]` over `Bytes` (a scalar `Int` element read) → `bytes.index(buf, i)` — the
/// CHECKED self-host byte read (aborts `Error: index out of bounds` + exit 1 exactly
/// like v0's `b[i]`; `bytes.read_u8`'s 0-for-OOB convention is a DIFFERENT api).
/// Same desugar-before-both slot as `desugar_map_access_calls`.
fn desugar_bytes_index_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::IndexAccess { object, index } = &e.kind else {
                return;
            };
            if !matches!(object.ty, Ty::Bytes) || !matches!(e.ty, Ty::Int) {
                return;
            }
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("bytes"),
                        func: sym("index"),
                        def_id: None,
                    },
                    args: vec![(**object).clone(), (**index).clone()],
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// A float-family BinOp over MATRIX operands (`a * b` / `a + b` / `a - b` on Matrix —
/// the numeric-protocol operators) → the registered `matrix.mul`/`add`/`sub` module
/// call. The scalar-binop path had NO operand gate on the arithmetic arms, so `a * b`
/// lowered as an f64 multiply of the two BLOCK HANDLES — a silent garbage Matrix on
/// the verified default (matrix_test's `*` row). Same desugar-before-both slot as
/// `desugar_map_access_calls` (the rewrite adds ONE counted Module call).
fn desugar_matrix_binops(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;

    fn is_matrix_ty(t: &Ty) -> bool {
        matches!(t, Ty::Matrix)
            || matches!(t, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Matrix, _))
    }

    /// `m * k` / `k * m` (ScaleMatrix — one Matrix, one scalar) → matrix.scale with the
    /// Matrix normalized to the FIRST arg (the self-host's signature). Split out of
    /// `matrix_binop_rewrite` below (codopsy cc) — a disjoint `op` case (`ScaleMatrix`
    /// only), tried first in the same order the original single function checked it.
    fn matrix_scale_rewrite(op: &almide_ir::BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
        if !matches!(op, almide_ir::BinOp::ScaleMatrix) {
            return None;
        }
        let (m, k) = if is_matrix_ty(&left.ty) {
            (left.clone(), right.clone())
        } else {
            (right.clone(), left.clone())
        };
        Some(IrExprKind::Call {
            target: CallTarget::Module { module: sym("matrix"), func: sym("scale"), def_id: None },
            args: vec![m, k],
            type_args: Vec::new(),
        })
    }

    // The frontend's dispatch: `a * b` (both Matrix) → MulMatrix; `m * k` → ScaleMatrix
    // (handled by `matrix_scale_rewrite` above); `a + b`/`a - b` fall through the
    // NUMERIC arms as AddInt/SubInt (neither operand is Float), so those are matched
    // here by the MATRIX operand types, not the op class. A NON-arithmetic op (the
    // wildcard) is out of subset — `None`, not a wall (the caller keeps the original
    // BinOp verbatim). Pure name lookup, split out of `matrix_binop_rewrite` (cc).
    fn matrix_binop_func_name(op: &almide_ir::BinOp) -> Option<&'static str> {
        Some(match op {
            almide_ir::BinOp::MulMatrix => "mul",
            almide_ir::BinOp::AddMatrix => "add",
            almide_ir::BinOp::SubMatrix => "sub",
            almide_ir::BinOp::AddInt | almide_ir::BinOp::AddFloat => "add",
            almide_ir::BinOp::SubInt | almide_ir::BinOp::SubFloat => "sub",
            almide_ir::BinOp::DivInt | almide_ir::BinOp::DivFloat => "div",
            almide_ir::BinOp::MulInt | almide_ir::BinOp::MulFloat => "mul",
            _ => return None,
        })
    }

    // Pure decision, no visitor state — `self.changed` below is an OUTPUT flag the
    // caller reads after the fact, never fed back into this decision, so (unlike a
    // real state-threading walker) the whole rewrite computation is safe to extract
    // as a free function of `(op, left, right)` returning the replacement `IrExprKind`.
    fn matrix_binop_rewrite(op: &almide_ir::BinOp, left: &IrExpr, right: &IrExpr) -> Option<IrExprKind> {
        if let Some(k) = matrix_scale_rewrite(op, left, right) {
            return Some(k);
        }
        if !is_matrix_ty(&left.ty) || !is_matrix_ty(&right.ty) {
            return None;
        }
        let func = matrix_binop_func_name(op)?;
        Some(IrExprKind::Call {
            target: CallTarget::Module { module: sym("matrix"), func: sym(func), def_id: None },
            args: vec![left.clone(), right.clone()],
            type_args: Vec::new(),
        })
    }

    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::BinOp { op, left, right } = &e.kind else { return };
            if let Some(new_kind) = matrix_binop_rewrite(op, left, right) {
                e.kind = new_kind;
                self.changed = true;
            }
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `buf[i] = v` over `Bytes` — the WRITE-side twin of `desugar_bytes_index_calls` —
/// → statement `bytes.set_at(buf, i, v)`, the CHECKED packed-byte store self-host
/// (whose receiver rides the #794 COW discipline: local var → MakeUnique, mut param
/// → write-through). Without this rewrite `IndexAssign` lowers as a uniform 8-byte
/// SLOT store (`+12+i*8` — never where `bytes.index` reads `+12+i`, and past a
/// packed block's end for i>3): `buf[2] = 0x42` silently vanished on the verified
/// default while corrupting the neighboring heap block. Bytes receivers are known
/// by TYPE: `Bytes`-typed params plus `Bind`s with `ty: Bytes`, seen in statement
/// order (VarIds are function-unique, so no scoping ambiguity).
fn desugar_bytes_index_assign(body: &IrExpr, params: &[IrParam]) -> Option<IrExpr> {
    use almide_ir::{walk_stmt_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        bytes_vars: HashSet<VarId>,
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
            walk_stmt_mut(self, stmt);
            if let IrStmtKind::Bind { var, ty: Ty::Bytes, .. } = &stmt.kind {
                self.bytes_vars.insert(*var);
                return;
            }
            let IrStmtKind::IndexAssign { target, index, value } = &stmt.kind else {
                return;
            };
            if !self.bytes_vars.contains(target) {
                return;
            }
            let recv = IrExpr {
                kind: IrExprKind::Var { id: *target },
                ty: Ty::Bytes,
                span: index.span.clone(),
                def_id: None,
            };
            let call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("bytes"),
                        func: sym("set_at"),
                        def_id: None,
                    },
                    args: vec![recv, index.clone(), value.clone()],
                    type_args: Vec::new(),
                },
                ty: Ty::Unit,
                span: index.span.clone(),
                def_id: None,
            };
            stmt.kind = IrStmtKind::Expr { expr: call };
            self.changed = true;
        }
    }
    let mut s = S {
        bytes_vars: params.iter().filter(|p| matches!(p.ty, Ty::Bytes)).map(|p| p.var).collect(),
        changed: false,
    };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `xs[a..b]` over a SCALAR-element list: the frontend struck the range slice
/// directly to `RuntimeCall{almide_rt_list_slice}` (expressions.rs), which the
/// v1 bind path can only defer to an EMPTY Opaque — `sub[0]` then walls. But
/// `almide_rt_list_slice` IS `list.slice`, and `list.slice` is SELF-HOSTED
/// (list_take_drop.almd) — rewrite the RuntimeCall back to the Module call so
/// it rides `lower_pure_module_value_call` and materializes a REAL list.
/// Same desugar-before-both slot as `desugar_map_access_calls`. Gated to a
/// `List[scalar]` result — the registered self-host is the scalar-element
/// `list_slice`; a heap-element slice keeps the (walling) deferred path.
/// `buf[a..b]` over `Bytes` (`RuntimeCall{almide_rt_bytes_slice}`) is the same
/// deferred-Opaque hole with a WORSE failure (the empty defer READS as len 0 —
/// `bytes.len(sub)` returned 0 silently) — rewrite to the self-hosted
/// `bytes.slice(b, start, end)` (bytes_core.almd, v0-clamping semantics).
fn desugar_list_slice_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::RuntimeCall { symbol, args } = &e.kind else {
                return;
            };
            if args.len() != 3 {
                return;
            }
            let (module, func) = match symbol.as_str() {
                "almide_rt_list_slice" if crate::lower::is_scalar_elem_list_ty(&e.ty) => {
                    ("list", "slice")
                }
                "almide_rt_bytes_slice" if matches!(e.ty, Ty::Bytes) => ("bytes", "slice"),
                _ => return,
            };
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym(module),
                        func: sym(func),
                        def_id: None,
                    },
                    args: args.clone(),
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// `p?.f` → `match p { some(__x) => some(__x.f), none => none }` — a PURE desugar
/// into the proven Option-match rails (variant-seeded subjects, payload binds,
/// heap-result arms), replacing the deferred-Opaque the OptionalChain node fell
/// to (its bound var then misread as `none`/garbage in any comparison — the
/// unwrap_operators optional-chain walls). Same desugar-before-both slot as
/// `desugar_map_access_calls`; the rewrite adds NO calls (Match/Member/Some are
/// call-free), so both counters see the identical call multiset. Fresh payload
/// vars mint past `max_var_id` (the desugar_unwrap discipline).
fn desugar_optional_chain(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        changed: bool,
        next_var: u32,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::OptionalChain { expr, field } = &e.kind else {
                return;
            };
            let Ty::Applied(TypeConstructorId::Option, a) = &expr.ty else {
                return;
            };
            if a.len() != 1 {
                return;
            }
            let payload_ty = a[0].clone();
            let x = VarId(self.next_var);
            self.next_var += 1;
            let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span: e.span.clone(), def_id: None };
            let field_ty = match &e.ty {
                Ty::Applied(TypeConstructorId::Option, fa) if fa.len() == 1 => fa[0].clone(),
                _ => return,
            };
            let x_read = mk(IrExprKind::Var { id: x }, payload_ty.clone());
            let member =
                mk(IrExprKind::Member { object: Box::new(x_read), field: *field }, field_ty);
            let some_body = mk(IrExprKind::OptionSome { expr: Box::new(member) }, e.ty.clone());
            let none_body = mk(IrExprKind::OptionNone, e.ty.clone());
            let arms = vec![
                almide_ir::IrMatchArm {
                    pattern: almide_ir::IrPattern::Some {
                        inner: Box::new(almide_ir::IrPattern::Bind { var: x, ty: payload_ty }),
                    },
                    guard: None,
                    body: some_body,
                },
                almide_ir::IrMatchArm { pattern: almide_ir::IrPattern::None, guard: None, body: none_body },
            ];
            // ANF-lift a non-Var subject (`match f() {…}` → `{ let __s = f(); match __s {…} }`):
            // the LET-BOUND Named call is what seeds the Option read-shape
            // (`materialized_options`), so the match branches on a TRACKED subject.
            let (stmts, subject) = if matches!(&expr.kind, IrExprKind::Var { .. }) {
                (Vec::new(), expr.clone())
            } else {
                let s_var = VarId(self.next_var);
                self.next_var += 1;
                let bind = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: s_var,
                        mutability: almide_ir::Mutability::Let,
                        ty: expr.ty.clone(),
                        value: (**expr).clone(),
                    },
                    span: e.span.clone(),
                };
                let subj = mk(IrExprKind::Var { id: s_var }, expr.ty.clone());
                (vec![bind], Box::new(subj))
            };
            let match_expr = mk(IrExprKind::Match { subject, arms }, e.ty.clone());
            *e = if stmts.is_empty() {
                match_expr
            } else {
                mk(IrExprKind::Block { stmts, expr: Some(Box::new(match_expr)) }, e.ty.clone())
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false, next_var: crate::lower::desugar_var_seed() };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// The `Result[Unit, E]` this fn's ABI promises when its body's effective TAIL is Unit-typed
/// (descending Block chains; an absent tail is Unit) — `None` when the tail carries a real
/// value or the fn is not Result-ABI. Declared `Result[Unit, E]` keeps its own `E`; a
/// declared-Unit AUTO_WRAP lift synthesizes `Result[Unit, String]` (the same type the
/// `owned_body` override stamps). Declared-Option and declared-Unit-non-AUTO_WRAP fns
/// (including a void-convention main) are excluded by construction.
fn unit_tail_result_abi_ty(func: &IrFunction, body: &IrExpr) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn tail_is_unit(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Block { expr: Some(t), .. } => tail_is_unit(t),
            IrExprKind::Block { expr: None, .. } => true,
            _ => matches!(e.ty, Ty::Unit),
        }
    }
    let result_ty = match &func.ret_ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[0], Ty::Unit) => {
            func.ret_ty.clone()
        }
        // A LIFTED (declared-Unit effect) fn whose CALLERS keep the Result expectation:
        // the AUTO_WRAP set, or any CAN-ERR lifted fn (∉ NEVER_ERR — e.g. an argument-
        // position `!` errs without tripping the stmt/tail AUTO_WRAP heuristics, so the
        // caller's `Try` is never stripped and it `local.set`s the promised handle).
        // The def must return that handle: same registry, same verdict, by construction.
        // `main` keeps the exit-code void convention (its caller is `_start`, not a
        // registry-classified call site).
        Ty::Unit
            if func.is_effect
                && func.name.as_str() != "main"
                && (crate::lower::AUTO_WRAP_ABI_FNS
                    .with(|s| s.borrow().contains(func.name.as_str()))
                    || !crate::lower::NEVER_ERR_LIFTED_FNS
                        .with(|s| s.borrow().contains(func.name.as_str()))) =>
        {
            Ty::result(Ty::Unit, Ty::String)
        }
        _ => return None,
    };
    tail_is_unit(body).then_some(result_ty)
}

/// The FULL ABI-effective body — BOTH root retypes the lowering applies before its
/// desugar ladder, in order: the `AUTO_WRAP_ABI_FNS` synthetic-Result retype
/// ([`auto_wrap_abi_body`]) and the unit-tail Result-ABI ok-wrap
/// (`unit_tail_result_abi_ty` + `wrap_unit_body_in_ok`). `pub` for the SAME
/// #1176 reason `auto_wrap_abi_body` is: the classify count-side must apply the
/// IDENTICAL retypes before `desugar_all`, or `desugar_loop_unwrap`'s
/// `Result[T, String]` root gate declines on the count side while the lowering
/// fires it — the rewrite's injected owned-copy concats then become MIR ops with
/// no counted IR node (a false `mir > ir` breach). The second retype's
/// registry-independent arm (a CAN-ERR declared-Unit effect fn ∉ NEVER_ERR) is
/// what the first alone misses: a `effect_cont_synth_*` continuation is
/// synthesized AFTER the registry fixpoint, so it sits in NEITHER set and the
/// lowering wraps it through exactly that arm (the fs_streaming false breach).
pub fn abi_effective_body(func: &IrFunction) -> Option<IrExpr> {
    let auto = auto_wrap_abi_body(func);
    let base: &IrExpr = auto.as_ref().unwrap_or(&func.body);
    if let Some(result_ty) = unit_tail_result_abi_ty(func, base) {
        return Some(wrap_unit_body_in_ok(base, result_ty));
    }
    auto
}

/// `{ stmts…; unit_tail }` → `{ stmts…; unit_tail; ok(()) }` — the old Unit tail becomes a
/// statement (the standard stmt-position effect shape), and the fn returns the real ok-Unit
/// Result block its ABI classification promises. Only the TOP-level Block is flattened; a
/// non-Block unit body becomes the single statement.
fn wrap_unit_body_in_ok(body: &IrExpr, result_ty: Ty) -> IrExpr {
    let (mut stmts, old_tail) = match &body.kind {
        IrExprKind::Block { stmts, expr } => (stmts.clone(), expr.as_deref().cloned()),
        _ => (Vec::new(), Some(body.clone())),
    };
    if let Some(t) = old_tail {
        stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: t }, span: None });
    }
    let ok_unit = IrExpr {
        kind: IrExprKind::ResultOk {
            expr: Box::new(IrExpr {
                kind: IrExprKind::Unit,
                ty: Ty::Unit,
                span: None,
                def_id: None,
            }),
        },
        ty: result_ty.clone(),
        span: None,
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(ok_unit)) },
        ty: result_ty,
        span: body.span.clone(),
        def_id: body.def_id,
    }
}

/// The desugar-before-both chain, as a pass table. Each pass returns `None` when
/// it does not apply; a pass that fires feeds its rewrite to the next, so the
/// whole chain is one fold. `None` overall means nothing rewrote the body.
///
/// - `desugar_assert_calls` — assert/assert_eq/assert_ne → the controlled-halt `if`/die shape.
/// - `desugar_map_access_calls` — `m[k]` → `map.get(m, k)`.
/// - `desugar_bytes_index_calls` — `buf[i]` over Bytes → `bytes.index(buf, i)`.
/// - `desugar_matrix_binops` — matrix `a * b` / `+` / `-` → matrix.mul/add/sub.
/// - `desugar_hof_chain_anf` — the C-127 piped HOF chain → its source-`let` form.
/// - `desugar_heap_if_call_args` — a HEAP-result `if` in argument position → let-decomposed (#881).
/// - `desugar_mutable_global_projection_args` / `desugar_bytes_index_assign` —
///   the two that need the fn's PARAMS to decide.
/// - `desugar_unwrap_or_unwrap_fallback` — `a ?? f(..)!` over a heap payload → `(match a { … })!` (#1375).
/// - `desugar_list_slice_calls`, `desugar_optional_chain` — the remaining surface forms.
fn apply_pre_lower_desugars(body: &IrExpr, params: &[almide_ir::IrParam]) -> Option<IrExpr> {
    // Per-function band reset: every desugar of the same body draws the same
    // chunk sequence, so the counting pass and the lowering pass re-derive
    // IDENTICAL trees (desugar-before-both), and the band never grows across
    // functions. In-lowering mints keep drawing from where the chain stopped.
    crate::lower::reset_desugar_var_band();
    type Pass = fn(&IrExpr) -> Option<IrExpr>;
    const PASSES: &[Pass] = &[
        desugar_assert_calls,
        desugar_map_access_calls,
        desugar_bytes_index_calls,
        desugar_matrix_binops,
        desugar_hof_chain_anf,
        desugar_heap_if_call_args,
        desugar_mutable_global_projection_args,
        // BEFORE the branch/unwrap desugars: the rewrite moves the fallback's `!` OUT of the
        // conditional arm into the `let x = e!` position `desugar_let_unwrap` then handles.
        desugar_unwrap_or_unwrap_fallback,
    ];
    let mut cur: Option<IrExpr> = None;
    for pass in PASSES {
        if let Some(next) = pass(cur.as_ref().unwrap_or(body)) {
            cur = Some(next);
        }
    }
    if let Some(next) = desugar_bytes_index_assign(cur.as_ref().unwrap_or(body), params) {
        cur = Some(next);
    }
    for pass in [desugar_list_slice_calls as Pass, desugar_optional_chain as Pass] {
        if let Some(next) = pass(cur.as_ref().unwrap_or(body)) {
            cur = Some(next);
        }
    }
    cur
}

include!("mod_c_tail.rs");
