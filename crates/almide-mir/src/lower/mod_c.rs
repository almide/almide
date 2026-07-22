
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
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::BinOp { op, left, right } = &e.kind else { return };
            let is_matrix = |t: &Ty| {
                matches!(t, Ty::Matrix)
                    || matches!(t, Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Matrix, _))
            };
            // `m * k` / `k * m` (ScaleMatrix — one Matrix, one scalar) → matrix.scale
            // with the Matrix normalized to the FIRST arg (the self-host's signature).
            if matches!(op, almide_ir::BinOp::ScaleMatrix) {
                let (m, k) = if is_matrix(&left.ty) {
                    ((**left).clone(), (**right).clone())
                } else {
                    ((**right).clone(), (**left).clone())
                };
                e.kind = IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("matrix"),
                        func: sym("scale"),
                        def_id: None,
                    },
                    args: vec![m, k],
                    type_args: Vec::new(),
                };
                self.changed = true;
                return;
            }
            if !is_matrix(&left.ty) || !is_matrix(&right.ty) {
                return;
            }
            // The frontend's dispatch: `a * b` (both Matrix) → MulMatrix; `m * k` →
            // ScaleMatrix (handled by the two-typed arm below); `a + b`/`a - b` fall
            // through the NUMERIC arms as AddInt/SubInt (neither operand is Float),
            // so those are matched here by the MATRIX operand types, not the op class.
            let func = match op {
                almide_ir::BinOp::MulMatrix => "mul",
                almide_ir::BinOp::AddMatrix => "add",
                almide_ir::BinOp::SubMatrix => "sub",
                almide_ir::BinOp::AddInt | almide_ir::BinOp::AddFloat => "add",
                almide_ir::BinOp::SubInt | almide_ir::BinOp::SubFloat => "sub",
                almide_ir::BinOp::DivInt | almide_ir::BinOp::DivFloat => "div",
                almide_ir::BinOp::MulInt | almide_ir::BinOp::MulFloat => "mul",
                _ => return,
            };
            e.kind = IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("matrix"),
                    func: sym(func),
                    def_id: None,
                },
                args: vec![(**left).clone(), (**right).clone()],
                type_args: Vec::new(),
            };
            self.changed = true;
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
    let mut s = S { changed: false, next_var: crate::lower::max_var_id(body) + 1 };
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

fn lower_function_all_impl(
    func: &IrFunction,
    globals: &HashMap<VarId, Ty>,
    global_inits: &HashMap<VarId, IrExpr>,
    record_layouts: &RecordLayouts,
    variant_layouts: &VariantLayouts,
) -> Result<Vec<MirFunction>, LowerError> {
    // A body-less `@extern(wasm, module, name)` function lowers to a thin host-IMPORT
    // call (the browser dom/fetch/timer/console stubs) — its behavior IS the host's, so
    // it CALLS the import, never fabricates a value. Gated STRICTLY on target == "wasm"
    // (a `rust`/`rs` extern has no wasm host → `None` → it keeps walling as before).
    if let Some(import_fn) = try_lower_extern_wasm(func)? {
        return Ok(vec![import_fn]);
    }
    // A `mut` param's write-back rides v0's tuple-return + place-writeback
    // convention (C-131/C-132). The v1 lower has NO move-mode calling convention
    // yet: a mutation through the borrowed param COWs a copy and silently DROPS
    // the caller-visible write (`push9(v, 20)` left `v` unchanged on the verified
    // default while v0 pushed — the #790 mut_list_param row, main-reachable).
    // WALL the fn — v0 emits the correct convention on both targets.
    if !func.mutated_params.is_empty() {
        return Err(LowerError::Unsupported(format!(
            "fn `{}` mutates its `mut` param(s) — the move-mode write-back \
             convention (C-132) not in this brick",
            func.name
        )));
    }
    let mut ctx = LowerCtx {
        globals: globals.clone(),
        global_inits: global_inits.clone(),
        fn_name: func.name.as_str().to_string(),
        record_layouts: record_layouts.clone(),
        variant_layouts: variant_layouts.clone(),
        // An EXPLICIT `Result`/`Option` declared return is a REAL heap value the caller inspects
        // (e.g. `fs.write -> Result[Unit, String]`), so a `Result[Unit, _]` tail must NOT be voided
        // — see `LowerCtx::decl_ret_is_result`. A declared-`Unit` effect fn (the synthetic Result)
        // keeps the void convention.
        decl_ret_is_result: matches!(
            &func.ret_ty,
            Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::Result
                    | almide_lang::types::constructor::TypeConstructorId::Option,
                _
            )
        ),
        // STRICTLY-Result declared return (Option excluded — see the field doc) OR an
        // auto-wrapped scalar ABI: the bare-tail-Option-`!` desugar's gate.
        ret_is_result_abi: matches!(
            &func.ret_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)
        ) || crate::lower::AUTO_WRAP_ABI_FNS
            .with(|s| s.borrow().contains(func.name.as_str())),
        // The fn's effective err type — declared `Result[_, E]`'s E, `String` for the lifted
        // synthetic Result, None for a declared Option (its `!` pass-through is repr-identical).
        decl_fn_err: match &func.ret_ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, a)
                if a.len() == 2 =>
            {
                Some(a[1].clone())
            }
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, _) => None,
            _ => Some(Ty::String),
        },
        ..Default::default()
    };
    let params = ctx.bind_params(&func.params)?;
    // TCO: a tail-self-recursive heap-result function is rewritten to a scalar loop + post-loop
    // dispatch (the existing self-rec guard would otherwise wall it). The rewritten body lowers
    // through the ordinary statements+tail path; if it is out of the TCO subset, `None` keeps the
    // original body (which the self-rec guard walls as before — no regression).
    // PRE-DESUGAR before the TCO: a recursive body `{ let c = if k then A else B; recurse(acc + c) }`
    // has a let-bound heap-result `if` the loop-body lowering would wall. Tail-duplication
    // (`desugar_heap_branches`) pushes the continuation — INCLUDING the recursive call — into each arm,
    // yielding BRANCHED recursion `if k then recurse(acc+A) else recurse(acc+B)` that `tco_collect`
    // handles (it recurses both `if` arms). The let-bound `if` is ELIMINATED, so the loop body lowers.
    // `lower_body_into` desugars again (idempotent) for the non-TCO path; the caps gate counts the
    // SAME desugared tree (desugar-before-both), so mir == ir. Unblocks base64 encode/decode_chunks +
    // toml read_basic/parse_val (the let-bound-heap-`if`-in-a-loop frontier).
    let owned_body;
    let func_body: &IrExpr = if crate::lower::AUTO_WRAP_ABI_FNS
        .with(|s| s.borrow().contains(func.name.as_str()))
    {
        owned_body = IrExpr { ty: Ty::result(func.ret_ty.clone(), Ty::String), ..func.body.clone() };
        &owned_body
    } else {
        &func.body
    };
    // assert/assert_eq/assert_ne → the controlled-halt `if`/die shape (see
    // `desugar_assert_calls`). Desugar-before-both: every downstream consumer
    // (counting, TCO, lowering) sees the same tree.
    let assert_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_assert_calls(func_body) {
        assert_body = rewritten;
        &assert_body
    } else {
        func_body
    };
    // `m[k]` → `map.get(m, k)` (see `desugar_map_access_calls`) — same
    // desugar-before-both slot.
    let map_access_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_map_access_calls(func_body) {
        map_access_body = rewritten;
        &map_access_body
    } else {
        func_body
    };
    // `buf[i]` over Bytes → `bytes.index(buf, i)` (see `desugar_bytes_index_calls`).
    let bytes_index_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_bytes_index_calls(func_body) {
        bytes_index_body = rewritten;
        &bytes_index_body
    } else {
        func_body
    };
    // Matrix `a * b`/`+`/`-` → matrix.mul/add/sub (see `desugar_matrix_binops`) —
    // same desugar-before-both slot.
    let matrix_binop_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_matrix_binops(func_body) {
        matrix_binop_body = rewritten;
        &matrix_binop_body
    } else {
        func_body
    };
    // The C-127 piped HOF chain (`… |> option.map(λ) |> option.unwrap_or(d)`) →
    // its source-`let` decomposed form (see `desugar_hof_chain_anf`) — same
    // desugar-before-both slot.
    let hof_chain_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_hof_chain_anf(func_body) {
        hof_chain_body = rewritten;
        &hof_chain_body
    } else {
        func_body
    };
    // `buf[i] = v` over Bytes → `bytes.set_at(buf, i, v)` (see
    // `desugar_bytes_index_assign`) — same desugar-before-both slot.
    let bytes_index_assign_body;
    let func_body: &IrExpr =
        if let Some(rewritten) = desugar_bytes_index_assign(func_body, &func.params) {
            bytes_index_assign_body = rewritten;
            &bytes_index_assign_body
        } else {
            func_body
        };
    // `xs[a..b]` slice RuntimeCall → `list.slice(xs, a, b)` (see `desugar_list_slice_calls`).
    let list_slice_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_list_slice_calls(func_body) {
        list_slice_body = rewritten;
        &list_slice_body
    } else {
        func_body
    };
    // `p?.f` → the some/none match (see `desugar_optional_chain`).
    let opt_chain_body;
    let func_body: &IrExpr = if let Some(rewritten) = desugar_optional_chain(func_body) {
        opt_chain_body = rewritten;
        &opt_chain_body
    } else {
        func_body
    };
    // A RESULT-ABI fn (declared `Result[Unit, E]`, or a declared-Unit AUTO_WRAP lift) whose
    // effective TAIL is Unit-typed produces NO value on the unit path — the never-err strips
    // reduce a lifted tail call to a raw Unit effect call, and a declared-Result effect fn can
    // end on a bare effect stmt. But every CALL SITE consults the same name-keyed ABI
    // registries and `local.set`s the expected Result handle over the void callee — invalid
    // wasm (the #786 class: def and call sites disagree on the ABI). Materialize the missing
    // value: `body_unit` → `{ body_unit; ok(()) }`, so the def returns the real Result block
    // its classification promises (the proven alloc(i) + move-out(m) tail). A declared-Unit
    // main is NEITHER case (both gates miss), so the exit-code void convention is untouched.
    let ok_wrapped_body;
    let func_body: &IrExpr = if let Some(result_ty) = unit_tail_result_abi_ty(func, func_body) {
        ok_wrapped_body = wrap_unit_body_in_ok(func_body, result_ty);
        &ok_wrapped_body
    } else {
        func_body
    };
    crate::lower::dump_desugared_ir(func.name.as_str(), func_body, variant_layouts, record_layouts);
    let pre_tco = desugar_heap_branches(func_body, variant_layouts);
    let body_ref: &IrExpr = pre_tco.as_ref().unwrap_or(func_body);
    let tco_body = try_tco_rewrite(&ctx.fn_name, &func.params, body_ref);
    let final_body = tco_body.as_ref().unwrap_or(body_ref);
    // SHARED-CELL pre-scan (closures Rung 6, cells.rs): over the FINAL lowered tree,
    // so bind/read/write/capture all classify the same vars as cells. A pure scan —
    // no rewrite, so the counted tree is untouched.
    ctx.cell_vars = collect_cell_vars(final_body, &ctx.globals, &func.params);
    let ret = ctx.lower_body_into(final_body)?;
    // The function's EFFECT SIGNATURE → its declared capability bound. The v1 model
    // has one capability (Stdout); an `effect fn` declares it may reach the host, so
    // it admits the only modeled cap. A pure `fn` declares ∅ — so if it reached
    // Stdout (forbidden by the effect system) the proven `used ⊆ declared` checker
    // would REJECT it. The capability gate verifies `reachable ⊆ declared`, not just
    // "reaches nothing" — so an effectful function is now caps-VERIFIED against its
    // own declared bound, not merely excluded.
    // An `effect fn` declares it MAY reach the modeled host capabilities (the v1 effect system is
    // binary: pure vs host-reaching, not per-capability). So it admits Stdout, Entropy, CliArgs AND
    // FsRead — the `used ⊆ declared` checker then verifies its body stays within that bound. A pure
    // `fn` declares ∅, so reaching ANY cap (a `print`/`random.int`/`env.args`/`fs.read_text` from a
    // non-effect fn — already a frontend type error) would REJECT here too: the soundness floor (pure
    // stays pure) is unchanged; only the host-reaching set grows. (A per-capability effect signature
    // is a later precision refinement.)
    let declared_caps = if func.is_effect {
        vec![
            crate::Capability::Stdout,
            crate::Capability::Entropy,
            crate::Capability::CliArgs,
            crate::Capability::FsRead,
            crate::Capability::FsWrite,
            crate::Capability::Stdin,
        ]
    } else {
        Vec::new()
    };
    let lifted = std::mem::take(&mut ctx.lifted);
    let heap_slot_masks = ctx.record_masks.iter().map(|(v, m)| (*v, m.clone())).collect();
    let main = MirFunction {
        name: func.name.as_str().to_string(),
        params,
        ops: ctx.ops,
        ret,
        declared_caps,
        heap_slot_masks,
    };
    let mut all = vec![main];
    all.extend(lifted);
    // The synthesized recursive-eq helpers ride the same rail as lifted lambdas
    // (extra cluster functions; per-parent names, so no cross-fn collision).
    all.extend(std::mem::take(&mut ctx.synth_eq_fns));
    Ok(all)
}

mod binds;
mod layout;
mod tail;
mod control;
mod calls;

// The `??`-operand admission gate (a free fn in the private `control` module) — re-exported so the
// `classify_corpus` caps counter consults the SAME predicate the lowering uses (no count drift).
pub use control::unwrap_or_operand_admitted;


#[cfg(test)]
mod tests;

include!("drop_sources.rs");
include!("drop_sources_b.rs");
include!("drop_sources_c.rs");
include!("repr_sources.rs");
include!("repr_sources_b.rs");
include!("repr_sources_c.rs");
include!("repr_sources_d.rs");
include!("newtype_erase.rs");
include!("record_defaults.rs");
include!("desugar_guard.rs");
include!("desugar_guard_b.rs");
include!("cells.rs");
include!("inline_scalar_fns.rs");
include!("mod_p2.rs");
include!("mod_p2_b.rs");
include!("mod_p2_c.rs");
include!("mod_p3.rs");
include!("mod_p3_b.rs");
include!("mod_p3_c.rs");
include!("mod_p4.rs");
include!("mod_p4_b.rs");
include!("mod_p4_c.rs");
include!("mod_p4_d.rs");
include!("mod_p5.rs");
include!("mod_p5_b.rs");
// The desugar family (formerly one 4.8k-line mod_p6.rs), split by concern:
include!("desugar.rs");
include!("desugar_b.rs");
include!("desugar_unwrap.rs");
include!("desugar_unwrap_b.rs");
include!("desugar_loop.rs");
include!("desugar_loop_b.rs");
include!("desugar_branch.rs");
include!("desugar_branch_b.rs");
include!("desugar_fan.rs");
include!("desugar_match.rs");
include!("desugar_match_b.rs");
include!("desugar_match_c.rs");
include!("desugar_match_subject.rs");
include!("synth_eq.rs");
