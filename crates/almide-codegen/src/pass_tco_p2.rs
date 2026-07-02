/// Rewrite a TCO-eligible function body from recursive form to a loop.
fn rewrite_to_loop(
    func: &mut IrFunction,
    var_table: &mut VarTable,
    infer_bindings: &mut std::collections::BTreeSet<VarId>,
) -> HashSet<usize> {
    let fn_name = func.name.clone();
    // For effect fns returning Result[T, E], the TCO result variable should hold T
    // because the Rust codegen auto-unwraps Result via `?` operator.
    let ret_ty = if func.is_effect {
        match &func.ret_ty {
            Ty::Applied(TypeConstructorId::Result, args) if !args.is_empty() => args[0].clone(),
            _ => func.ret_ty.clone(),
        }
    } else {
        func.ret_ty.clone()
    };

    // Mark all param VarIds as mutable (they'll be reassigned in the loop).
    //
    // Historically all borrow annotations were reset to Own here, because a
    // param that persists across loop iterations needs an owned value. BUT
    // that causes massive clones in binary parsers: a self-recursive walker
    // over a 77MB Bytes buffer would clone the buffer on every iteration.
    //
    // For Bytes specifically, holding a `&Vec<u8>` across iterations is
    // safe — the reference targets something owned by the outermost caller,
    // and reassigning it inside the loop (to the same or a derived &Vec<u8>)
    // is a no-op lifetime-wise. Keep the borrow there; force ownership
    // everywhere else.
    let mut bytes_borrowed_params: HashSet<usize> = HashSet::new();
    let mut reverted_to_own: HashSet<usize> = HashSet::new();
    for (i, param) in func.params.iter_mut().enumerate() {
        var_table.entries[param.var.0 as usize].mutability = Mutability::Var;
        let keep_borrow = matches!(param.ty, Ty::Bytes)
            && !matches!(param.borrow, almide_ir::ParamBorrow::Own);
        if keep_borrow {
            bytes_borrowed_params.insert(i);
        } else {
            // If BorrowInsertion had previously marked this param as Borrow,
            // external call sites are now emitting &str / &Vec where we now
            // expect owned. Record so the call-site walker can strip wrappers.
            if !matches!(param.borrow, almide_ir::ParamBorrow::Own) {
                reverted_to_own.insert(i);
            }
            param.borrow = almide_ir::ParamBorrow::Own;
        }
    }
    TCO_BORROWED_PARAMS.with(|s| *s.borrow_mut() = bytes_borrowed_params.clone());

    // Allocate a result variable
    let result_var = var_table.alloc(
        "__tco_result".into(),
        ret_ty.clone(),
        Mutability::Var,
        None,
    );

    // Allocate temporaries for each param, each carrying the param's REAL type
    // (the ConcretizeTypes postcondition verifies no Unknown reaches codegen).
    // A borrow-preserved param's Rust representation is `&Vec<u8>` — a borrow
    // the `Ty` system cannot spell — so its temp is registered in
    // `infer_binding_tys` and the walker renders `let __tco_tmp_data = data;`
    // with a `_` annotation, letting Rust infer the borrow. (The old approach
    // smuggled `Ty::Unknown` through the temp's type, which the postcondition
    // gate rightly refused — nn vocab_at_loop/parse_tensors_loop.)
    let temps: Vec<(VarId, Ty)> = func.params.iter().enumerate().map(|(i, p)| {
        let tmp_ty = p.ty.clone();
        let tmp = var_table.alloc(
            format!("__tco_tmp_{}", p.name).into(),
            tmp_ty.clone(),
            Mutability::Let,
            None,
        );
        if bytes_borrowed_params.contains(&i) {
            infer_bindings.insert(tmp);
        }
        (tmp, tmp_ty)
    }).collect();

    // Collect param info for rewrite
    let params: Vec<(VarId, Ty)> = func.params.iter()
        .map(|p| (p.var, p.ty.clone()))
        .collect();

    // Close the tail-recursive heap-accumulator leak: when a self-tail-call
    // overwrites a heap param P with a FRESH allocation each iteration (a List /
    // Record / Map / String literal or a string concat — `tco_managed_params`),
    // the OLD value of P is dead the instant it is reassigned, and the new value
    // cannot alias it (a literal allocates a new block). Such a `dec_param` gets:
    //   - an entry `Inc` (the loop ADOPTS ownership of the caller's borrowed-in
    //     value so the first per-iteration `Dec` releases the loop's added ref,
    //     not the caller's still-live one — see `body_stmts` below),
    //   - a `Dec` before every loop reassignment (free the dead old value),
    //   - a `Dec` at every base case whose result is NON-heap (free the final
    //     value); a heap-typed base-case result might carry P out to the caller,
    //     so its Dec is suppressed (the caller frees it) — see `emit_base_case`.
    // Params reassigned via a bare carry (`f(P, …)`), a possibly-in-place-reusing
    // call (`f(list.drop(P,1))`), or any non-literal arg are left UNMANAGED: their
    // new value may alias the old, so a Dec could use-after-free. Conservative —
    // those keep the pre-existing (bounded-per-call) leak rather than risk a
    // double-free now that frees are live.
    let dec_params: Vec<VarId> = tco_managed_params(&func.body, &params, fn_name.as_str());
    if std::env::var("ALMIDE_TCO_DEBUG").is_ok() {
        let names = |vs: &[VarId]| vs.iter().map(|v| var_table.get(*v).name.as_str().to_string()).collect::<Vec<_>>();
        eprintln!("[tco] {} dec_params={:?}", fn_name.as_str(), names(&dec_params));
    }

    // Rewrite the body expression
    let old_body = std::mem::take(&mut func.body);
    let is_effect = func.is_effect;
    let rewritten = rewrite_tail_expr(old_body, &fn_name, &params, &temps, result_var, is_effect, &dec_params);

    // Build the default value for the result variable
    let default_val = default_for_type(&ret_ty);

    // Construct: { var __tco_result = default; while true { rewritten_body }; __tco_result }
    let bind_result = IrStmt {
        kind: IrStmtKind::Bind {
            var: result_var,
            mutability: Mutability::Var,
            ty: ret_ty.clone(),
            value: default_val,
        },
        span: None,
    };

    // The while body is a single Expr statement wrapping the rewritten body
    let while_body_stmt = IrStmt {
        kind: IrStmtKind::Expr { expr: rewritten },
        span: None,
    };

    // The while body is just the rewritten body. (A previous trailing
    // `RcDec __tco_tmp_*` here was both DEAD — every path through the rewritten
    // body ends in `continue` or `break`, so a statement after it is unreachable —
    // and WRONG: each temp is MOVED into its param (`param = __tco_tmp`), so
    // Dec-ing it would free the live loop value. The old-value Dec for heap params
    // is now emitted correctly at the reassignment / base-case sites; see
    // `dec_params` and `emit_tail_call_replacement` / `emit_base_case`.
    let while_body = vec![while_body_stmt];

    let while_expr = IrExpr {
        kind: IrExprKind::While {
            cond: Box::new(IrExpr {
                kind: IrExprKind::LitBool { value: true },
                ty: Ty::Bool,
                span: None, def_id: None,
            }),
            body: while_body,
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    let while_stmt = IrStmt {
        kind: IrStmtKind::Expr { expr: while_expr },
        span: None,
    };

    let tail_var = IrExpr {
        kind: IrExprKind::Var { id: result_var },
        ty: ret_ty.clone(),
        span: None, def_id: None,
    };

    // For effect fns, wrap the result in Ok() since the function returns Result
    let tail_expr = if func.is_effect {
        IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(tail_var) },
            ty: func.ret_ty.clone(),
            span: None, def_id: None,
        }
    } else {
        tail_var
    };

    // Entry Inc for dec-managed heap params: converts the borrowed-in param value
    // to callee-owned, so the uniform `Dec`-before-reassign (and base-case `Dec`)
    // balances on the FIRST iteration too (it releases this added ref, leaving the
    // caller's own ref intact for the caller to Dec). Without it, iteration 1 would
    // free the caller's value early.
    let mut body_stmts = vec![bind_result];
    for p in &dec_params {
        body_stmts.push(IrStmt { kind: IrStmtKind::RcInc { var: *p }, span: None });
    }
    body_stmts.push(while_stmt);

    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: body_stmts,
            expr: Some(Box::new(tail_expr)),
        },
        ty: func.ret_ty.clone(),
        span: func.body.span, def_id: None,
    };

    reverted_to_own
}

/// Heap-typed for TCO RC purposes (mirrors Perceus `is_heap_type`).
fn tco_is_heap(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. }
        | Ty::Unknown | Ty::Fn { .. })
}

/// A heap param P is "managed" (entry-Inc + per-reassign Dec + base-case Dec) iff,
/// in EVERY self-tail-call, the argument in P's position is a FRESH allocation:
/// a `List`/`Record`/`Map`/string literal or a string concat. A fresh allocation
/// is a brand-new heap block, so it can never alias P's OLD value — making
/// `Dec(old P)` before the reassignment provably free of use-after-free. (Any
/// heap SUBcomponent of P borrowed into the fresh value, e.g. `Acc{ tag: P.tag }`
/// — would be a move, but a field-read producing a heap value is Inc'd by Perceus
/// Rule-1 on aliasing, so the Dec only frees what is truly dead.) Bare carries,
/// in-place-reusing calls (`list.drop`/`map`/`filter` over a single-use source),
/// and any non-literal arg are conservatively UNMANAGED: their new value might BE
/// the old block, so a Dec could double-free now that frees are live.
///
/// WASM has no `Borrow`/`Clone` IR nodes (those passes are Rust-only), so the
/// fresh-allocation shape — not a borrow annotation — is the soundness signal.
fn tco_managed_params(body: &IrExpr, params: &[(VarId, Ty)], fn_name: &str) -> Vec<VarId> {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    // managed[i] starts true; a non-fresh self-call arg flips it false. We do NOT
    // pre-filter by `tco_is_heap(param.ty)`: at TCO time a record/variant param is
    // an unresolved `Ty::Named` (not yet `Ty::Record`), so a type test would miss
    // it. Instead we rely on the arg shape — `is_fresh_alloc` only matches
    // heap-allocating literals (List/Record/Map/String/concat), so "all args fresh"
    // already IMPLIES the param is heap. A non-heap param (e.g. `n` fed `n - 1`)
    // never has a fresh arg, so it is excluded automatically — and is thus never
    // handed a (corruption-prone) rc_dec on a scalar.
    let mut managed: Vec<bool> = vec![true; params.len()];
    struct V<'a> { fn_name: &'a str, managed: &'a mut Vec<bool>, saw_self_call: bool }
    impl IrVisitor for V<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind {
                if name.as_str() == self.fn_name {
                    self.saw_self_call = true;
                    for (i, arg) in args.iter().enumerate() {
                        if i < self.managed.len() && !is_fresh_alloc(arg) {
                            self.managed[i] = false;
                        }
                    }
                }
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
    }
    let mut v = V { fn_name, managed: &mut managed, saw_self_call: false };
    v.visit_expr(body);
    if !v.saw_self_call { return Vec::new(); }
    params.iter().zip(managed).filter_map(|((var, _), m)| m.then_some(*var)).collect()
}

/// A self-tail-call argument the LOOP can adopt ownership of. Literal heap
/// allocations are trivially owned. Since the Round-3 ownership discipline,
/// CALL-valued args are adoption-safe too: user fns return OWNED (the
/// return-alias dup, mechanism #6), and after this rewrite turns the arg
/// into a `bind temp = arg; assign param = temp` chain, an alias-shaped
/// temp receives the bind-level Inc at Perceus time — either way the loop
/// holds its own reference, so the per-iteration Dec is balanced. Scalar
/// results are excluded by type (a Dec on a scalar would corrupt), and
/// `Unknown` stays unmanaged (conservative).
fn is_fresh_alloc(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::List { .. } | IrExprKind::Record { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => true,
        IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. } => match &e.ty {
            Ty::Int | Ty::Float | Ty::Bool | Ty::Unit | Ty::Unknown => false,
            Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32
            | Ty::UInt64 | Ty::Float32 => false,
            _ => true,
        },
        IrExprKind::Block { expr: Some(tail), .. } => is_fresh_alloc(tail),
        _ => false,
    }
}

/// Rewrite an expression in tail position:
/// - Self-calls become: bind temps, assign params, continue
/// - If/Match: recurse into branches
/// - Block: recurse into trailing expr
/// - Anything else (base case): assign to result var, break
fn rewrite_tail_expr(
    expr: IrExpr,
    fn_name: &str,
    params: &[(VarId, Ty)],
    temps: &[(VarId, Ty)],
    result_var: VarId,
    is_effect: bool,
    dec_params: &[VarId],
) -> IrExpr {
    match expr.kind {
        // Self-recursive call in tail position -> reassign params and continue
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
            emit_tail_call_replacement(args, params, temps, result_var, dec_params)
        }

        // #557: `(self-call)?` / `(self-call)!` — a frontend auto-? wrapping a
        // tail self-call loop-converts IDENTICALLY to the bare self-call (the
        // `?` is subsumed by the loop; an Err can only originate at a base
        // case, never the recursion).
        IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner }
            if matches!(&inner.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name) =>
        {
            match inner.kind {
                IrExprKind::Call { args, .. } => emit_tail_call_replacement(args, params, temps, result_var, dec_params),
                _ => unreachable!("guard guarantees a self-call"),
            }
        }

        // Effect fn: the Ok(..) propagation wrapper is tail-transparent —
        // RECURSE into it so `Ok(Try(Call self))` reaches the tail-call arm and
        // `Ok(0)` reaches the base-case arm (#557). Previously this eagerly
        // emitted the whole inner as a base case, so a wasm-arm
        // `Ok(Try(Call self))` was mis-emitted as a base value (no loop).
        IrExprKind::ResultOk { expr: inner } if is_effect => {
            rewrite_tail_expr(*inner, fn_name, params, temps, result_var, is_effect, dec_params)
        }

        // If: recurse into both branches
        IrExprKind::If { cond, then, else_ } => {
            let new_then = rewrite_tail_expr(*then, fn_name, params, temps, result_var, is_effect, dec_params);
            let new_else = rewrite_tail_expr(*else_, fn_name, params, temps, result_var, is_effect, dec_params);
            IrExpr {
                kind: IrExprKind::If {
                    cond,
                    then: Box::new(new_then),
                    else_: Box::new(new_else),
                },
                ty: Ty::Unit,
                span: expr.span, def_id: None,
            }
        }

        // Match: recurse into arm bodies
        IrExprKind::Match { subject, arms } => {
            let new_arms = arms.into_iter().map(|arm| {
                IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard,
                    body: rewrite_tail_expr(arm.body, fn_name, params, temps, result_var, is_effect, dec_params),
                }
            }).collect();
            IrExpr {
                kind: IrExprKind::Match { subject, arms: new_arms },
                ty: Ty::Unit,
                span: expr.span, def_id: None,
            }
        }

        // Block: recurse into trailing expr
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let new_tail = rewrite_tail_expr(*tail, fn_name, params, temps, result_var, is_effect, dec_params);
            IrExpr {
                kind: IrExprKind::Block {
                    stmts,
                    expr: Some(Box::new(new_tail)),
                },
                ty: Ty::Unit,
                span: expr.span, def_id: None,
            }
        }

        // Base case: assign result and break.
        // Explicit-preserve: every remaining variant — including a Call that is
        // NOT a self-call, a ResultOk in a non-effect fn, and a Block with no
        // trailing expr (all of which fall through the guarded/partial arms
        // above) — is a base case emitted via emit_base_case. Same RHS the
        // catch-all had, total-by-construction.
        //
        // These patterns bind nothing (`{ .. }` / unit variants), so `expr`
        // is not partially moved and remains usable whole — exactly as `_` was.
        IrExprKind::Call { .. } | IrExprKind::ResultOk { .. }
        | IrExprKind::Block { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::List { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Tuple { .. }
        | IrExprKind::Range { .. } | IrExprKind::Member { .. }
        | IrExprKind::TupleIndex { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::Lambda { .. }
        | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. }
        | IrExprKind::OptionNone | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {
            emit_base_case(expr, result_var, dec_params)
        }
    }
}

/// Emit the replacement for a tail self-call:
/// ```text
/// let __tco_tmp_0 = arg0_expr
/// let __tco_tmp_1 = arg1_expr
/// param0 = __tco_tmp_0
/// param1 = __tco_tmp_1
/// continue
/// ```
/// Strip a Borrow wrapper from an expression (if present).
/// TCO params become owned, so borrow annotations from BorrowInsertion must be removed.
fn strip_borrow(expr: IrExpr) -> IrExpr {
    match expr.kind {
        IrExprKind::Borrow { expr: inner, .. } => *inner,
        _ => expr,
    }
}

fn emit_tail_call_replacement(
    args: Vec<IrExpr>,
    params: &[(VarId, Ty)],
    temps: &[(VarId, Ty)],
    _result_var: VarId,
    dec_params: &[VarId],
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();

    // F5 (#527): an IDENTITY CARRY — argument i is the bare Var of param i —
    // is a semantic no-op; emitting the temp anyway gave the temp bind a
    // Rule-1 alias-Inc with no Dec anywhere (the temp is Dec-exempt by name,
    // an unmanaged param has no per-iteration Dec): +1 rc per iteration, an
    // immortal param block and rc creep toward wrap on long loops. Skip both
    // the bind and the assign for those positions.
    let identity_carry: Vec<bool> = args.iter().enumerate().map(|(i, arg)| {
        matches!(&arg.kind, IrExprKind::Var { id } if *id == params[i].0)
    }).collect();

    // Bind temporaries to argument expressions.
    // Strip Borrow from arg unless this param position is kept-borrowed
    // (e.g. Bytes borrow preserved across iterations).
    for (i, arg) in args.into_iter().enumerate() {
        if identity_carry[i] { continue; }
        let (tmp_var, tmp_ty) = &temps[i];
        let keep = TCO_BORROWED_PARAMS.with(|s| s.borrow().contains(&i));
        let unwrapped = if keep { arg } else { strip_borrow(arg) };
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: *tmp_var,
                mutability: Mutability::Let,
                ty: tmp_ty.clone(),
                value: unwrapped,
            },
            span: None,
        });
    }

    // Free the OLD value of each dec-managed heap param BEFORE overwriting it.
    // The new values are already captured in the temps above (which only borrowed
    // the old params), so the old values are now dead and unaliased — Dec-ing them
    // is safe and is exactly the per-iteration free that closes the accumulator
    // leak. (Done after ALL temp binds so an arg that borrows several params still
    // reads their live old values first.)
    for p in dec_params {
        stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
    }

    // Assign params from temporaries (identity carries skipped — the param
    // already holds its value).
    for (i, (param_var, _)) in params.iter().enumerate() {
        if identity_carry[i] { continue; }
        let (tmp_var, tmp_ty) = &temps[i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign {
                var: *param_var,
                value: IrExpr {
                    kind: IrExprKind::Var { id: *tmp_var },
                    ty: tmp_ty.clone(),
                    span: None, def_id: None,
                },
            },
            span: None,
        });
    }

    // Continue the loop
    let continue_expr = IrExpr {
        kind: IrExprKind::Continue,
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(continue_expr)),
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    }
}

/// Emit the base case: assign to result variable and break.
/// ```text
/// __tco_result = expr
/// break
/// ```
fn emit_base_case(expr: IrExpr, result_var: VarId, dec_params: &[VarId]) -> IrExpr {
    // A heap-typed base-case result may MOVE a managed param out to the caller
    // (e.g. `then s` returns the accumulator). Freeing the param here would then
    // be a use-after-free on the returned value, so suppress the exit Dec for a
    // heap result and let the caller own it. A non-heap result (Int/Bool/…) cannot
    // carry a param out, so the params are dead after the assign and freeing the
    // final loop value here balances the entry `Inc`.
    let result_is_heap = tco_is_heap(&expr.ty);

    let assign = IrStmt {
        kind: IrStmtKind::Assign {
            var: result_var,
            value: expr,
        },
        span: None,
    };

    let mut stmts = vec![assign];
    if !result_is_heap {
        for p in dec_params {
            stmts.push(IrStmt { kind: IrStmtKind::RcDec { var: *p }, span: None });
        }
    }

    let break_expr = IrExpr {
        kind: IrExprKind::Break,
        ty: Ty::Unit,
        span: None, def_id: None,
    };

    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(break_expr)),
        },
        ty: Ty::Unit,
        span: None, def_id: None,
    }
}

/// Produce a default value for a given type (used to initialize the result variable).
/// The value is never observed — every control path assigns before reading — but
/// Rust's type checker requires a valid expression of the correct type.
fn default_for_type(ty: &Ty) -> IrExpr {
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        Ty::String => IrExprKind::LitStr { value: String::new() },
        Ty::Unit => IrExprKind::Unit,
        Ty::Applied(TypeConstructorId::Result, args) => {
            let inner_ty = args.first().cloned().unwrap_or(Ty::Unit);
            let inner = default_for_type(&inner_ty);
            IrExprKind::ResultOk { expr: Box::new(inner) }
        }
        Ty::Applied(TypeConstructorId::Option, _) => {
            IrExprKind::OptionNone
        }
        Ty::Applied(TypeConstructorId::List, _) => {
            IrExprKind::List { elements: vec![] }
        }
        Ty::Applied(TypeConstructorId::Map, _) => {
            IrExprKind::MapLiteral { entries: vec![] }
        }
        Ty::Tuple(elems) => {
            IrExprKind::Tuple {
                elements: elems.iter().map(|t| default_for_type(t)).collect(),
            }
        }
        // Named types and other complex types: cannot synthesize a default value.
        // TCO should not be applied to functions returning these types.
        // Return Unit as unreachable placeholder (guarded by can_default_init check).
        _ => IrExprKind::Unit,
    };
    IrExpr {
        kind,
        ty: ty.clone(),
        span: None, def_id: None,
    }
}

/// Returns true if we can produce a valid default value for this type.
/// Types that fail this check should not be TCO'd (the result variable
/// cannot be initialized without unsafe code).
fn can_default_init(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit => true,
        Ty::Applied(TypeConstructorId::Result, args) => {
            args.first().map_or(true, |inner| can_default_init(inner))
        }
        Ty::Applied(TypeConstructorId::Option, _) => true,
        Ty::Applied(TypeConstructorId::List, _) => true,
        Ty::Applied(TypeConstructorId::Map, _) => true,
        Ty::Tuple(elems) => elems.iter().all(|t| can_default_init(t)),
        _ => false,
    }
}
