/// CONTINUATION LIFT, poison-oracle-driven (#1147, a pre-lowering program pass).
///
/// A statement-position effect `!` has no mid-function early return on the v1
/// MIR, so the shared unwrap desugar pushes the WHOLE remaining block — loops
/// included — into the ok arm of its err-check region. When such a loop
/// carries a heap slot whose release spans regions, the flat v4 ownership
/// certificate cannot represent the arm: `flush_branch` poisons the object
/// (`{i|}`) and the fn drops out of the kernel-checked witness
/// (`cert_poisoned_excluded`, #1146).
///
/// The lift outlines the continuation into a synthesized effect fn
/// (`effect_cont_synth_*`) whose tail the parent calls with an explicit `!` —
/// the err-check arm then holds a flat call, and the loop certifies at the
/// TOP level of the synthesized fn (the proven `i(…)m` shape). The
/// branch_lift discipline: no new cert / Coq machinery, just moving the
/// construct into a position the proven lowering already handles.
///
/// WHY ORACLE-DRIVEN (round 1's negative result, recorded on #1147): whether
/// a loop-in-arm actually poisons depends on cert-level facts — a loop whose
/// slot events balance inside the arm flat-flushes fine. A syntactic trigger
/// re-derives none of that: it fired on working wasm_cross fixtures and
/// REGRESSED them, and exposed never-lowered class-b fns as new walled-real
/// entries. So the lift fires ONLY when the UN-lifted fn's own certificate
/// says poisoned: pre-filter syntactically, LOWER the candidate, ask
/// `certificate::ownership_certificate_with_poison`, and split on a genuine
/// poison verdict only. Self-limiting by construction: a fn that lowers and
/// certifies clean — or that walls before certifying — is never touched.
///
/// Runs on the WHOLE linked program (pipeline + classify — desugar-before-
/// both: the caps `mir == ir` count sees the lifted tree on BOTH sides).
///
/// Conservative guards (skip rather than mis-lift): only the fn-body TOP
/// block is split (no enclosing loop to `break` out of); the continuation
/// must not assign to a variable bound before it (params are copies);
/// generic fns are skipped; `!` and loops inside lambda literals neither
/// trigger nor count.
pub fn lift_poisoning_continuations(program: &mut almide_ir::IrProgram) {
    let mut counter: u32 = 0;
    {
        let almide_ir::IrProgram { functions, top_lets, var_table, .. } = &mut *program;
        let globals_ty: std::collections::HashMap<almide_ir::VarId, Ty> =
            top_lets.iter().map(|tl| (tl.var, tl.ty.clone())).collect();
        let global_vars: std::collections::HashSet<almide_ir::VarId> =
            top_lets.iter().map(|tl| tl.var).collect();
        cl_lift_in_fns(functions, var_table, &globals_ty, &global_vars, &mut counter);
    }
    for module in program.modules.iter_mut() {
        let almide_ir::IrModule { functions, top_lets, var_table, .. } = &mut *module;
        let globals_ty: std::collections::HashMap<almide_ir::VarId, Ty> =
            top_lets.iter().map(|tl| (tl.var, tl.ty.clone())).collect();
        let global_vars: std::collections::HashSet<almide_ir::VarId> =
            top_lets.iter().map(|tl| tl.var).collect();
        cl_lift_in_fns(functions, var_table, &globals_ty, &global_vars, &mut counter);
    }
}

fn cl_lift_in_fns(
    functions: &mut Vec<almide_ir::IrFunction>,
    vt: &mut almide_ir::VarTable,
    globals_ty: &std::collections::HashMap<almide_ir::VarId, Ty>,
    global_vars: &std::collections::HashSet<almide_ir::VarId>,
    counter: &mut u32,
) {
    // TRANSACTIONAL per fn: the lift may only IMPROVE the fn's standing. The
    // original poisons but lowers (the executable verifier covers it); if any
    // piece of the split chain walled or still poisoned, committing it would
    // trade a covered fn for a walled-real one. So: split the whole chain on
    // a clone-backed fn, validate EVERY piece (parent + synths) lowers with a
    // clean cert, and roll back to the untouched original otherwise.
    let n = functions.len();
    for i in 0..n {
        if !cl_prefilter(&functions[i]) || !cl_oracle_says_poisoned(&functions[i], globals_ty) {
            continue;
        }
        let original = functions[i].clone();
        // Split the fn, then keep splitting each synthesized continuation
        // that still has the `!`-then-loop shape (round 1 showed the fully
        // split chain is what lowers; a half-split middle piece can wall).
        // Each split strictly shrinks a continuation, so this terminates.
        let mut chain: Vec<almide_ir::IrFunction> = Vec::new();
        {
            let ret_ty = functions[i].ret_ty.clone();
            let mut body = std::mem::take(&mut functions[i].body);
            chain.extend(cl_lift_top_block(&mut body, &ret_ty, vt, global_vars, counter));
            functions[i].body = body;
        }
        let mut j = 0;
        while j < chain.len() {
            if cl_prefilter(&chain[j]) {
                let ret_ty = chain[j].ret_ty.clone();
                let mut body = std::mem::take(&mut chain[j].body);
                let more = cl_lift_top_block(&mut body, &ret_ty, vt, global_vars, counter);
                chain[j].body = body;
                if !more.is_empty() {
                    chain.extend(more);
                    continue; // re-check the same piece
                }
            }
            j += 1;
        }
        // Validate: every piece must lower AND certify without poison.
        let dbg = std::env::var_os("ALMIDE_DBG_CONTLIFT").is_some();
        let all_clean = std::iter::once(&functions[i]).chain(chain.iter()).all(|f| {
            match lower_function(f, globals_ty) {
                Ok(mir) => {
                    let poisoned = crate::certificate::ownership_certificate_with_poison(&mir).1;
                    if dbg && poisoned {
                        eprintln!("[contlift] piece {} still POISONS", f.name.as_str());
                    }
                    !poisoned
                }
                Err(e) => {
                    if dbg {
                        eprintln!("[contlift] piece {} WALLS: {:?}", f.name.as_str(), e);
                    }
                    false
                }
            }
        });
        if all_clean && !chain.is_empty() {
            functions.extend(chain);
        } else {
            // Roll back: the original poisons but stays lowered and
            // verifier-covered — strictly better than a walled chain.
            functions[i] = original;
        }
    }
}

/// Cheap syntactic gate before the (lowering-cost) oracle: an effect fn whose
/// top block has a `!`-statement followed by a loop-bearing continuation.
fn cl_prefilter(func: &almide_ir::IrFunction) -> bool {
    if !func.is_effect || func.generics.is_some() {
        return false;
    }
    let almide_ir::IrExprKind::Block { stmts, expr: tail } = &func.body.kind else {
        return false;
    };
    cl_split_index(stmts, tail).is_some()
}

/// The oracle: lower the UN-lifted fn and ask its certificate. A fn that
/// walls (`Err`) never reaches a cert — not poisoned, never lifted.
fn cl_oracle_says_poisoned(
    func: &almide_ir::IrFunction,
    globals_ty: &std::collections::HashMap<almide_ir::VarId, Ty>,
) -> bool {
    match lower_function(func, globals_ty) {
        Ok(mir) => crate::certificate::ownership_certificate_with_poison(&mir).1,
        Err(_) => false,
    }
}

/// Split the fn-body block at the FIRST qualifying `!`-statement, returning
/// the synthesized continuation fn (empty when a guard declines). The body's
/// tail becomes `effect_cont_synth_N(free…)!`.
fn cl_lift_top_block(
    body: &mut almide_ir::IrExpr,
    ret_ty: &Ty,
    vt: &mut almide_ir::VarTable,
    global_vars: &std::collections::HashSet<almide_ir::VarId>,
    counter: &mut u32,
) -> Vec<almide_ir::IrFunction> {
    use almide_ir::{
        CallTarget, IrExpr, IrExprKind, IrFunction, IrParam, IrStmt, IrVisibility, ParamBorrow,
    };
    let IrExprKind::Block { stmts, expr: tail } = &mut body.kind else {
        return Vec::new();
    };
    let Some(k) = cl_split_index(stmts, tail) else {
        return Vec::new();
    };

    let cont_stmts: Vec<IrStmt> = stmts.split_off(k + 1);
    let cont_tail = tail.take();
    let span = cont_stmts.first().and_then(|s| s.span).or(body.span);
    let cont_body = IrExpr {
        kind: IrExprKind::Block { stmts: cont_stmts, expr: cont_tail },
        ty: ret_ty.clone(),
        span,
        def_id: None,
    };

    let bound: std::collections::HashSet<almide_ir::VarId> = std::collections::HashSet::new();
    let params: Vec<almide_ir::VarId> = almide_ir::free_vars::free_vars(&cont_body, &bound)
        .into_iter()
        .filter(|v| !global_vars.contains(v))
        .collect();

    let id = *counter;
    *counter = id + 1;
    // Plain (non-`__`) name for the same reason as `branch_lift_synth_*`: a
    // `__` prefix would be rewritten to a runtime-intrinsic call by v0
    // codegen's builtin lowering.
    let func_name = sym(&format!("effect_cont_synth_{}", id));

    let func_params: Vec<IrParam> = params
        .iter()
        .map(|&vid| {
            let info = vt.get(vid);
            IrParam {
                var: vid,
                ty: info.ty.clone(),
                name: info.name,
                borrow: ParamBorrow::Own,
                is_mut: false,
                open_record: None,
                default: None,
                attrs: vec![],
            }
        })
        .collect();

    let args: Vec<IrExpr> = params
        .iter()
        .map(|&vid| IrExpr {
            kind: IrExprKind::Var { id: vid },
            ty: vt.get(vid).ty.clone(),
            span,
            def_id: None,
        })
        .collect();

    // The call-site ty is the effect CARRIER (`Result[T, String]`) — the
    // frontend's convention for a named effect call under `!`; the Unwrap
    // above it carries the unwrapped T. A plain-T call ty made the unwrap
    // desugar read the subject as a non-Result ("empty deferred heap value").
    let call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Named { name: func_name },
            args,
            type_args: vec![],
        },
        ty: Ty::result(ret_ty.clone(), Ty::String),
        span,
        def_id: None,
    };
    // The explicit `!` in the PROVEN positions: for a Unit continuation
    // (every test fn) the call is a `!`-STATEMENT with no tail — exactly the
    // stmt-control shape the unwrap desugar already lowers; for a value
    // continuation, the bind-position `!` (`let __cl_ret = cont(…)!`) with
    // the var as tail. A bare tail call left the callee's carrier block on
    // the wasm stack (round 1's invalid module), a tail-position
    // `Unwrap(call)` walled inside a preceding err-check arm, and a
    // Unit-typed BIND walled as an empty deferred heap value.
    let unwrapped = IrExpr {
        kind: IrExprKind::Unwrap { expr: Box::new(call) },
        ty: ret_ty.clone(),
        span,
        def_id: None,
    };
    if matches!(ret_ty, Ty::Unit) {
        stmts.push(IrStmt { kind: almide_ir::IrStmtKind::Expr { expr: unwrapped }, span });
        *tail = None;
    } else {
        let ret_var = vt.alloc(sym("__cl_ret"), ret_ty.clone(), almide_ir::Mutability::Let, span);
        stmts.push(IrStmt {
            kind: almide_ir::IrStmtKind::Bind {
                var: ret_var,
                mutability: almide_ir::Mutability::Let,
                ty: ret_ty.clone(),
                value: unwrapped,
            },
            span,
        });
        *tail = Some(Box::new(IrExpr {
            kind: IrExprKind::Var { id: ret_var },
            ty: ret_ty.clone(),
            span,
            def_id: None,
        }));
    }

    vec![IrFunction {
        name: func_name,
        params: func_params,
        ret_ty: ret_ty.clone(),
        body: cont_body,
        is_effect: true,
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
    }]
}

/// The first index k where `stmts[k]` carries a statement-level effect `!`
/// AND the continuation (`stmts[k+1..]` + tail) is non-empty, carries a
/// loop, and passes the lost-write guard.
fn cl_split_index(
    stmts: &[almide_ir::IrStmt],
    tail: &Option<Box<almide_ir::IrExpr>>,
) -> Option<usize> {
    for k in 0..stmts.len() {
        if !cl_stmt_has_unwrap(&stmts[k]) {
            continue;
        }
        let after = &stmts[k + 1..];
        if after.is_empty() && tail.is_none() {
            continue;
        }
        if !cl_continuation_has_loop(after, tail) {
            continue;
        }
        if cl_continuation_writes_outer(after) {
            return None; // conservative: a later split point would drop the writes too
        }
        return Some(k);
    }
    None
}

fn cl_stmt_has_unwrap(stmt: &almide_ir::IrStmt) -> bool {
    struct Find {
        found: bool,
    }
    impl almide_ir::IrVisitor for Find {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if self.found || matches!(e.kind, almide_ir::IrExprKind::Lambda { .. }) {
                return;
            }
            if matches!(e.kind, almide_ir::IrExprKind::Unwrap { .. }) {
                self.found = true;
                return;
            }
            almide_ir::walk_expr(self, e);
        }
    }
    let mut f = Find { found: false };
    almide_ir::IrVisitor::visit_stmt(&mut f, stmt);
    f.found
}

fn cl_continuation_has_loop(
    stmts: &[almide_ir::IrStmt],
    tail: &Option<Box<almide_ir::IrExpr>>,
) -> bool {
    struct Find {
        found: bool,
    }
    impl almide_ir::IrVisitor for Find {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if self.found || matches!(e.kind, almide_ir::IrExprKind::Lambda { .. }) {
                return;
            }
            if matches!(
                e.kind,
                almide_ir::IrExprKind::ForIn { .. } | almide_ir::IrExprKind::While { .. }
            ) {
                self.found = true;
                return;
            }
            almide_ir::walk_expr(self, e);
        }
    }
    let mut f = Find { found: false };
    for s in stmts {
        almide_ir::IrVisitor::visit_stmt(&mut f, s);
    }
    if let Some(t) = tail {
        almide_ir::IrVisitor::visit_expr(&mut f, t);
    }
    f.found
}

/// Does the continuation write (assign / in-place mutate) a var it does not
/// itself bind? Such a var would become a by-value param and the write lost.
fn cl_continuation_writes_outer(stmts: &[almide_ir::IrStmt]) -> bool {
    struct Scan {
        bound: std::collections::HashSet<almide_ir::VarId>,
        writes: Vec<almide_ir::VarId>,
    }
    impl almide_ir::IrVisitor for Scan {
        fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
            match &s.kind {
                almide_ir::IrStmtKind::Bind { var, .. } => {
                    self.bound.insert(*var);
                }
                almide_ir::IrStmtKind::Assign { var, .. } => self.writes.push(*var),
                almide_ir::IrStmtKind::IndexAssign { target, .. }
                | almide_ir::IrStmtKind::MapInsert { target, .. }
                | almide_ir::IrStmtKind::FieldAssign { target, .. } => self.writes.push(*target),
                _ => {}
            }
            almide_ir::walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if let almide_ir::IrExprKind::ForIn { var, var_tuple, .. } = &e.kind {
                self.bound.insert(*var);
                if let Some(vs) = var_tuple {
                    for v in vs {
                        self.bound.insert(*v);
                    }
                }
            }
            almide_ir::walk_expr(self, e);
        }
    }
    let mut s = Scan { bound: std::collections::HashSet::new(), writes: Vec::new() };
    for st in stmts {
        almide_ir::IrVisitor::visit_stmt(&mut s, st);
    }
    s.writes.iter().any(|w| !s.bound.contains(w))
}
