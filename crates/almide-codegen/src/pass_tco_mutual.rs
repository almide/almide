// MUTUAL tail-call elimination (#1043) — the cross-fn chain the per-fn loop
// rewrite cannot reach. Self tail-recursion is rewritten by `rewrite_to_loop`;
// a MUTUAL cycle (is_even/is_odd, ping/pong, a parser's state functions) was
// handed to LLVM, whose sibling-call optimization only fires at opt-level >= 1
// — a native SEMANTIC property (deep mutual recursion terminates) resting on a
// Cargo setting (C-178 held by `[profile.dev] opt-level = 1`).
//
// The rewrite: each tail-call SCC of size >= 2 collapses into ONE dispatcher
//
//   fn __mutual_tco_<n>_<first>(tag: Int, <member-0 slots>, <member-1 slots>, …) -> R {
//     var __mt_result = <default>
//     while true {
//       if tag == 0 { <body-0, tail calls → slot assigns + tag = j + continue> }
//       else        { <body-1, likewise> }
//     }
//     __mt_result
//   }
//
// and every member becomes a THIN WRAPPER `f(a, b) = __mutual_tco_…(k, …)`
// passing its own args in its own slots and zero-literals in the others. The
// wrappers preserve every external surface — non-tail call sites, function
// values, exports — so any shape this pass does NOT rewrite (a call under
// `?`, a call in non-tail position) still calls the wrapper and stays
// semantically correct; it merely keeps its stack frame. Safe-by-default.
//
// SCOPE (v1): ALL-SCALAR groups only — every member's params are
// Int/Float/Bool and the (necessarily shared) return type is
// Int/Float/Bool/Unit. Scalars are Copy: no borrow preservation, no
// per-iteration RcDec, no clone/move census — the entire ownership apparatus
// the self-TCO rewrite carries does not apply. Heap-typed groups stay
// delegated to LLVM (still documented at the opt-level pin) until this pass
// grows the same discipline.

/// Scalar gate for v1: params must be one of these; return may also be Unit.
fn mt_is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool)
}

fn mt_is_candidate(f: &IrFunction) -> bool {
    !f.is_effect
        && !f.is_test
        && f.generics.is_none()
        && f.extern_attrs.is_empty()
        && f.export_attrs.is_empty()
        && f.attrs.is_empty()
        // Module-qualified names (stdlib splices) render with a prefix scheme
        // this pass does not participate in; top-level user fns only.
        && !f.name.as_str().contains('.')
        && (mt_is_scalar(&f.ret_ty) || matches!(f.ret_ty, Ty::Unit))
        && f.params.iter().all(|p| mt_is_scalar(&p.ty))
}

/// Collect fn names called in TAIL position (body root; If arms; Match arm
/// bodies; a Block's trailing expr). Everything else is by definition not a
/// tail call and is left for the wrappers to serve.
fn mt_collect_tail_callees(expr: &IrExpr, out: &mut HashSet<almide_base::intern::Sym>) {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, .. } => {
            out.insert(*name);
        }
        IrExprKind::If { then, else_, .. } => {
            mt_collect_tail_callees(then, out);
            mt_collect_tail_callees(else_, out);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms {
                mt_collect_tail_callees(&arm.body, out);
            }
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            mt_collect_tail_callees(tail, out);
        }
        // Explicit-preserve (traversal-totality): every remaining variant is BY
        // DEFINITION not a tail position — a call under any of these keeps its
        // stack frame and (after the rewrite) targets the member's wrapper,
        // which is the conservative-correct default for a variant added later.
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
        | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

/// Tarjan strongly-connected components over the tail-call edges, iterative
/// (an explicit stack — the input is user code, so no recursion budget is
/// assumed). Returns SCCs in reverse-topological order; only size >= 2 groups
/// are rewritten (size-1 self-loops belong to `rewrite_to_loop`).
fn mt_sccs(n: usize, edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    #[derive(Clone, Copy)]
    struct NodeState {
        index: u32,
        lowlink: u32,
        on_stack: bool,
        visited: bool,
    }
    let mut st = vec![NodeState { index: 0, lowlink: 0, on_stack: false, visited: false }; n];
    let mut counter: u32 = 0;
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    for root in 0..n {
        if st[root].visited {
            continue;
        }
        // (node, next-edge cursor)
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&mut (v, ref mut cursor)) = work.last_mut() {
            if *cursor == 0 {
                st[v].visited = true;
                st[v].index = counter;
                st[v].lowlink = counter;
                counter += 1;
                st[v].on_stack = true;
                stack.push(v);
            }
            if let Some(&w) = edges[v].get(*cursor) {
                *cursor += 1;
                if !st[w].visited {
                    work.push((w, 0));
                } else if st[w].on_stack {
                    st[v].lowlink = st[v].lowlink.min(st[w].index);
                }
            } else {
                work.pop();
                if let Some(&(parent, _)) = work.last() {
                    let low = st[v].lowlink;
                    st[parent].lowlink = st[parent].lowlink.min(low);
                }
                if st[v].lowlink == st[v].index {
                    let mut scc = Vec::new();
                    loop {
                        let w = stack.pop().expect("tarjan stack underflow");
                        st[w].on_stack = false;
                        scc.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(scc);
                }
            }
        }
    }
    sccs
}

fn mt_scalar_zero(ty: &Ty) -> IrExpr {
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        _ => unreachable!("mt_is_candidate admits only scalar params"),
    };
    IrExpr { kind, ty: ty.clone(), span: None, def_id: None }
}

fn mt_var(id: VarId, ty: Ty) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None }
}

/// Entry point, called from `TailCallOptPass::run` BEFORE the per-fn self-TCO
/// walk (a rewritten member's wrapper body holds no self-call, so the two
/// passes never touch the same function twice).
pub(crate) fn run_mutual_tco(program: &mut IrProgram) {
    let IrProgram { functions, var_table, .. } = program;

    // Candidate index: name → position in `functions`.
    let mut idx_of: HashMap<almide_base::intern::Sym, usize> = HashMap::new();
    for (i, f) in functions.iter().enumerate() {
        if mt_is_candidate(f) {
            idx_of.insert(f.name, i);
        }
    }
    if idx_of.len() < 2 {
        return;
    }

    // Tail-call edges restricted to candidates, in a dense id space.
    let members: Vec<usize> = {
        let mut v: Vec<usize> = idx_of.values().copied().collect();
        v.sort_unstable();
        v
    };
    let dense_of: HashMap<usize, usize> =
        members.iter().enumerate().map(|(d, &i)| (i, d)).collect();
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); members.len()];
    for (&fi, &d) in &dense_of {
        let mut callees = HashSet::new();
        mt_collect_tail_callees(&functions[fi].body, &mut callees);
        for c in callees {
            if let Some(&ci) = idx_of.get(&c) {
                edges[d].push(dense_of[&ci]);
            }
        }
        edges[d].sort_unstable();
    }

    let mut scc_no = 0usize;
    for scc in mt_sccs(members.len(), &edges) {
        if scc.len() < 2 {
            continue;
        }
        let fis: Vec<usize> = scc.iter().map(|&d| members[d]).collect();
        // A tail call returns the callee's result unchanged, so intra-SCC ret
        // types agree by construction — but verify rather than trust.
        let ret_ty = functions[fis[0]].ret_ty.clone();
        if !fis.iter().all(|&i| functions[i].ret_ty == ret_ty) {
            continue;
        }
        rewrite_mutual_scc(functions, var_table, &fis, ret_ty, scc_no);
        scc_no += 1;
    }
}

fn rewrite_mutual_scc(
    functions: &mut Vec<IrFunction>,
    var_table: &mut VarTable,
    fis: &[usize],
    ret_ty: Ty,
    scc_no: usize,
) {
    let member_idx: HashMap<almide_base::intern::Sym, usize> =
        fis.iter().enumerate().map(|(k, &i)| (functions[i].name, k)).collect();

    let tag_var = var_table.alloc(
        almide_base::intern::sym(&format!("__mt_tag_{scc_no}")),
        Ty::Int,
        Mutability::Var,
        None,
    );
    let result_var = var_table.alloc(
        almide_base::intern::sym(&format!("__mt_result_{scc_no}")),
        ret_ty.clone(),
        Mutability::Var,
        None,
    );

    // Per-member slot vars (dispatcher params, reassigned on every jump) and
    // jump temps (bound before the slot assigns so an arg that reads several
    // current slots sees their pre-jump values — the simultaneous-assignment
    // discipline the self-TCO temps exist for). Names carry the member index:
    // is_even(n) / is_odd(n) must NOT both render a param `n` (the E0415
    // duplicate-binder class the egg fusion rename helper exists for).
    let mut slots: Vec<Vec<(VarId, Ty)>> = Vec::new();
    let mut temps: Vec<Vec<(VarId, Ty)>> = Vec::new();
    for (k, &fi) in fis.iter().enumerate() {
        let mut srow = Vec::new();
        let mut trow = Vec::new();
        for p in &functions[fi].params {
            let pname = var_table.get(p.var).name;
            let s = var_table.alloc(
                almide_base::intern::sym(&format!("__mt{scc_no}_{k}_{}", pname.as_str())),
                p.ty.clone(),
                Mutability::Var,
                None,
            );
            let t = var_table.alloc(
                almide_base::intern::sym(&format!("__mt{scc_no}_tmp_{k}_{}", pname.as_str())),
                p.ty.clone(),
                Mutability::Let,
                None,
            );
            srow.push((s, p.ty.clone()));
            trow.push((t, p.ty.clone()));
        }
        slots.push(srow);
        temps.push(trow);
    }

    // Each member's body: params → its slots, then tail rewrites.
    let mut arm_bodies: Vec<IrExpr> = Vec::new();
    for (k, &fi) in fis.iter().enumerate() {
        let mut body = functions[fi].body.clone();
        for (pi, p) in functions[fi].params.iter().enumerate() {
            let repl = mt_var(slots[k][pi].0, p.ty.clone());
            body = substitute::substitute_var_in_expr(&body, p.var, &repl);
        }
        arm_bodies.push(rewrite_mutual_tail(
            body,
            &member_idx,
            &slots,
            &temps,
            tag_var,
            result_var,
        ));
    }

    // `if tag == 0 { arm0 } else if tag == 1 { arm1 } else { armN }` — the last
    // member rides the final else, so the chain is total without a trap arm.
    let mut chain = arm_bodies.pop().expect("scc has >= 2 members");
    for (k, arm) in arm_bodies.into_iter().enumerate().rev() {
        let cond = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::Eq,
                left: Box::new(mt_var(tag_var, Ty::Int)),
                right: Box::new(IrExpr {
                    kind: IrExprKind::LitInt { value: k as i64 },
                    ty: Ty::Int,
                    span: None,
                    def_id: None,
                }),
            },
            ty: Ty::Bool,
            span: None,
            def_id: None,
        };
        chain = IrExpr {
            kind: IrExprKind::If { cond: Box::new(cond), then: Box::new(arm), else_: Box::new(chain) },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        };
    }

    let while_expr = IrExpr {
        kind: IrExprKind::While {
            cond: Box::new(IrExpr {
                kind: IrExprKind::LitBool { value: true },
                ty: Ty::Bool,
                span: None,
                def_id: None,
            }),
            body: vec![IrStmt { kind: IrStmtKind::Expr { expr: chain }, span: None }],
        },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };

    let disp_body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![
                IrStmt {
                    kind: IrStmtKind::Bind {
                        var: result_var,
                        mutability: Mutability::Var,
                        ty: ret_ty.clone(),
                        value: default_for_type(&ret_ty),
                    },
                    span: None,
                },
                IrStmt { kind: IrStmtKind::Expr { expr: while_expr }, span: None },
            ],
            expr: Some(Box::new(mt_var(result_var, ret_ty.clone()))),
        },
        ty: ret_ty.clone(),
        span: None,
        def_id: None,
    };

    let disp_name = almide_base::intern::sym(&format!(
        "__mutual_tco_{scc_no}_{}",
        functions[fis[0]].name.as_str()
    ));
    let mut disp_params: Vec<IrParam> = vec![IrParam {
        var: tag_var,
        ty: Ty::Int,
        name: var_table.get(tag_var).name,
        borrow: ParamBorrow::Own,
        is_mut: false,
        open_record: None,
        default: None,
        attrs: vec![],
    }];
    for row in &slots {
        for (sv, sty) in row {
            disp_params.push(IrParam {
                var: *sv,
                ty: sty.clone(),
                name: var_table.get(*sv).name,
                borrow: ParamBorrow::Own,
                is_mut: false,
                open_record: None,
                default: None,
                attrs: vec![],
            });
        }
    }
    let dispatcher = IrFunction {
        name: disp_name,
        params: disp_params,
        ret_ty: ret_ty.clone(),
        body: disp_body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    };

    // Members become thin wrappers: own args in own slots, zero-literals in
    // the others (never read before the entry arm assigns or jumps away).
    for (k, &fi) in fis.iter().enumerate() {
        let mut args: Vec<IrExpr> = vec![IrExpr {
            kind: IrExprKind::LitInt { value: k as i64 },
            ty: Ty::Int,
            span: None,
            def_id: None,
        }];
        for (m, row) in slots.iter().enumerate() {
            for (i, (_sv, sty)) in row.iter().enumerate() {
                if m == k {
                    let p = &functions[fi].params[i];
                    args.push(mt_var(p.var, p.ty.clone()));
                } else {
                    args.push(mt_scalar_zero(sty));
                }
            }
        }
        functions[fi].body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: disp_name },
                args,
                type_args: vec![],
            },
            ty: ret_ty.clone(),
            span: None,
            def_id: None,
        };
    }

    functions.push(dispatcher);
}

/// The mutual twin of `rewrite_tail_expr`, with the ownership machinery
/// removed (all-scalar by the candidate gate). Any shape not named here is a
/// base case, and a base case containing a member call still calls that
/// member's WRAPPER — correct by construction, merely un-optimized — so the
/// catch-all is safe against future IrExprKind growth.
fn rewrite_mutual_tail(
    expr: IrExpr,
    member_idx: &HashMap<almide_base::intern::Sym, usize>,
    slots: &[Vec<(VarId, Ty)>],
    temps: &[Vec<(VarId, Ty)>],
    tag_var: VarId,
    result_var: VarId,
) -> IrExpr {
    match expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
            if member_idx.contains_key(&name) =>
        {
            emit_mutual_jump(member_idx[&name], args, slots, temps, tag_var)
        }
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond,
                then: Box::new(rewrite_mutual_tail(*then, member_idx, slots, temps, tag_var, result_var)),
                else_: Box::new(rewrite_mutual_tail(*else_, member_idx, slots, temps, tag_var, result_var)),
            },
            ty: Ty::Unit,
            span: expr.span,
            def_id: None,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject,
                arms: arms
                    .into_iter()
                    .map(|arm| IrMatchArm {
                        pattern: arm.pattern,
                        guard: arm.guard,
                        body: rewrite_mutual_tail(arm.body, member_idx, slots, temps, tag_var, result_var),
                    })
                    .collect(),
            },
            ty: Ty::Unit,
            span: expr.span,
            def_id: None,
        },
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block {
                stmts,
                expr: Some(Box::new(rewrite_mutual_tail(*tail, member_idx, slots, temps, tag_var, result_var))),
            },
            ty: Ty::Unit,
            span: expr.span,
            def_id: None,
        },
        kind => {
            let base = IrExpr { kind, ty: expr.ty, span: expr.span, def_id: expr.def_id };
            IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![IrStmt {
                        kind: IrStmtKind::Assign { var: result_var, value: base },
                        span: None,
                    }],
                    expr: Some(Box::new(IrExpr {
                        kind: IrExprKind::Break,
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    })),
                },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            }
        }
    }
}

/// `f → g(args)` in tail position becomes: bind g's temps from the args (all
/// reads see pre-jump slot values), move the temps into g's slots, set the
/// tag, continue.
fn emit_mutual_jump(
    target: usize,
    args: Vec<IrExpr>,
    slots: &[Vec<(VarId, Ty)>],
    temps: &[Vec<(VarId, Ty)>],
    tag_var: VarId,
) -> IrExpr {
    let mut stmts: Vec<IrStmt> = Vec::new();
    for (i, arg) in args.into_iter().enumerate() {
        let (tv, tty) = &temps[target][i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: *tv,
                mutability: Mutability::Let,
                ty: tty.clone(),
                value: arg,
            },
            span: None,
        });
    }
    for (i, (sv, sty)) in slots[target].iter().enumerate() {
        let (tv, _) = &temps[target][i];
        stmts.push(IrStmt {
            kind: IrStmtKind::Assign { var: *sv, value: mt_var(*tv, sty.clone()) },
            span: None,
        });
    }
    stmts.push(IrStmt {
        kind: IrStmtKind::Assign {
            var: tag_var,
            value: IrExpr {
                kind: IrExprKind::LitInt { value: target as i64 },
                ty: Ty::Int,
                span: None,
                def_id: None,
            },
        },
        span: None,
    });
    IrExpr {
        kind: IrExprKind::Block {
            stmts,
            expr: Some(Box::new(IrExpr {
                kind: IrExprKind::Continue,
                ty: Ty::Unit,
                span: None,
                def_id: None,
            })),
        },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    }
}
