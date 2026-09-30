//! The C-132 move-mode write-back convention, as a target-independent IR
//! rewrite: `mut` parameter functions return their mutated buffer, and every
//! call site assigns it back into the argument's place.
//!
//! Almide's `mut` parameter modifier means "this function may modify the
//! argument". Backends without pass-by-reference (wasm linear memory, the v1
//! MIR spine on BOTH its legs) lower it by value flow:
//!
//!   fn add_item(mut xs: List[Int], x: Int) -> Unit = list.push(xs, x)
//!   add_item(data, 1)
//!
//! becomes
//!
//!   fn add_item(xs: List[Int], x: Int) -> List[Int] = { list.push(xs, x); xs }
//!   data = add_item(data, 1)
//!
//! A fn that already RETURNS a value gets the tuple form (#705):
//!
//!   fn push9(mut v: List[Int], x: Int) -> Int = { list.push(v, x); list.len(v) - 1 }
//!   let i = push9(data, 7)
//!
//! becomes
//!
//!   fn push9(v, x) -> (Int, List[Int]) = { let __mp_ret = <body>; (__mp_ret, v) }
//!   let __mp_tmp = push9(data, 7); data = __mp_tmp.1; let i = __mp_tmp.0
//!
//! An EFFECT fn with a non-Unit return takes the same tuple rewrite (#1575:
//! the effect marker changes the return channel, not the parameter
//! convention), whether or not it can err (#1576): `(T, Buf)` rides the OK
//! payload — the lifted carrier is `Result[(T, Buf), E]`. A fn that CAN err
//! carries the buffer on its err arm too (#2917, #1871 ruling (B): a write
//! made before the err is visible to the caller, as native's `&mut`), and
//! every call site writes back on both arms — `mut_param_err_carry`. A declared-
//! Result effect fn (`-> T!`, the single-layer effect ABI: the body's
//! `ok`/`err` ARE the effect channel) is first stripped to its raw payload
//! (`ok(x)` tail -> `x`, an `err(e)` tail stays as the raise leaf, any other
//! Result-typed tail is `!`-unwrapped) so the tuple pairs `T` — not the
//! Result — with the buffer; the previous `(Result, Buf)` pairing double-
//! layered the carrier (structural `ok(22)` where native printed `22`, an
//! invalid module on the incumbent). A declared-Result fn whose err type is
//! not the default `String` carrier stays excluded (honest wall): the lifted
//! carrier the wasm ABI builds for a raw tuple is `Result[_, String]`.
//! A rewritten fn's `mutated_params` is CLEARED:
//! the convention is now explicit in the tree (the v1 C-132 wall keys on it,
//! and LICM's conservatism is subsumed by the call-site Assign).
//!
//! Callers: the v0 wasm nanopass (`MutParamLoweringPass`) and the v1 MIR
//! pipeline's pre-lowering (both `source_to_ir` twins — desugar-before-both).

use crate::visit::IrVisitor;
use crate::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use crate::*;
use almide_base::intern::sym;
use almide_lang::types::Ty;

/// Apply the move-mode rewrite program-wide. Returns `true` when anything
/// changed. See the module doc for the exact convention and exclusions.
pub fn lower_mut_params_move_mode(program: &mut IrProgram) -> bool {
    let mut_fns = collect_mut_fns(program);
    if mut_fns.is_empty() {
        return false;
    }
    // Read before `rewrite_signatures` strips a declared-Result body's ok layer.
    let never_err_fns = crate::mut_param_unpropagated::never_err_effect_fn_keys(program);
    let never_err: std::collections::HashSet<String> =
        never_err_fns.iter().filter(|k| mut_fns.contains_key(k.as_str())).cloned().collect();
    // The can-err ones carry their buffer on the err arm too (#2917).
    let carry = crate::mut_param_err_carry::carry_keys(program, |k| mut_fns.contains_key(k), &never_err_fns);
    rewrite_signatures(program, &mut_fns);
    rewrite_call_sites(program, &mut_fns, &carry);
    crate::mut_param_err_carry::carry_bodies(program, &mut_fns, &carry);
    crate::mut_param_unpropagated::settle(program, &never_err);
    fold_tail_writebacks(program, &mut_fns);
    hoist_branch_writebacks(program);
    true
}

/// Phase 4: hoist a write-back OUT of an if/match arm into straight-line
/// position (#1688's gunzip shape — the terminal fold cannot reach a branch
/// that is not the fn's last statement, and a write-back Assign left inside
/// an arm is refused by the structural emitter's branch wall). Two forms:
///
/// was-Unit callee (arm = `{ let b = call; p = b; () }`):
///   if c then WB(call1) else <arm2>            (Unit position)
///   → { p = if c then call1 else { <arm2>; p } }
///
/// value callee (arm = `{ let (r, b) = call; p = b; r }` : T):
///   match k { a1 => WB(call1), a2 => <arm2>, … }   : T
///   → { let (__mp_hres, __mp_hbuf) = match k { a1 => call1, a2 => (<arm2>, p), … };
///       p = __mp_hbuf; __mp_hres }               : T
///
/// Either way the Assign lands OUTSIDE the branch: every path yields the
/// buffer (the callee's successor handle, or the param read back), and one
/// straight-line write-back applies whichever ran — the exact discipline the
/// proven straight-line form already uses. Arms that do not write `p` are
/// admitted by wrapping (`{ arm; p }` reads the post-arm local, so a direct
/// stdlib mut call in an arm — `bytes.push(out, b)` — flows through
/// correctly); an arm that writes `p` some OTHER way (a nested branch
/// write-back, a second mut param) declines the hoist and stays walled.
fn hoist_branch_writebacks(program: &mut IrProgram) {
    {
        let mut h = WritebackHoister { vt: &mut program.var_table };
        for func in program.functions.iter_mut() {
            h.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            h.visit_expr_mut(&mut tl.value);
        }
    }
    // Module bodies hoist against the MODULE's table — the program table's
    // ids are a different namespace (the spliced-mut-param ICE, #1700).
    for m in &mut program.modules {
        let mut h = WritebackHoister { vt: &mut m.var_table };
        for func in m.functions.iter_mut() {
            h.visit_expr_mut(&mut func.body);
        }
        for tl in &mut m.top_lets {
            h.visit_expr_mut(&mut tl.value);
        }
    }
}

struct WritebackHoister<'a> {
    vt: &'a mut VarTable,
}

/// The phase-2 write-back block, either form. Returns (p, call, orig_ty:
/// None = was-Unit form).
fn as_writeback_block(e: &IrExpr) -> Option<(VarId, &IrExpr, Option<Ty>)> {
    let IrExprKind::Block { stmts, expr } = &e.kind else { return None };
    if stmts.len() != 2 {
        return None;
    }
    let IrStmtKind::Assign { var: p, value: av } = &stmts[1].kind else { return None };
    match (&stmts[0].kind, expr.as_deref().map(|t| &t.kind)) {
        // was-Unit: Bind{buf = call}; p = buf; ()
        (IrStmtKind::Bind { var: buf, value: call, .. }, None | Some(IrExprKind::Unit))
            if matches!(&av.kind, IrExprKind::Var { id } if id == buf)
                && matches!(call.kind, IrExprKind::Call { .. }) =>
        {
            Some((*p, call, None))
        }
        // value: BindDestructure{(res, buf) = call}; p = buf; res
        (
            IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value: call },
            Some(IrExprKind::Var { id: tail_id }),
        ) if matches!(call.kind, IrExprKind::Call { .. }) => {
            let [IrPattern::Bind { var: res, ty: res_ty }, IrPattern::Bind { var: buf, .. }] =
                elements.as_slice()
            else {
                return None;
            };
            if tail_id == res && matches!(&av.kind, IrExprKind::Var { id } if id == buf) {
                Some((*p, call, Some(res_ty.clone())))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Does the expression contain an `Assign` to `var` at any depth? (The
/// decline scan: a p-writing arm that is not the recognized write-back
/// block cannot be wrapped.)
fn assigns_var(e: &IrExpr, var: VarId) -> bool {
    struct Scan {
        var: VarId,
        hit: bool,
    }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Assign { var, .. } = &s.kind
                && *var == self.var
            {
                self.hit = true;
            }
            if !self.hit {
                crate::visit::walk_stmt(self, s);
            }
        }
    }
    let mut s = Scan { var, hit: false };
    crate::visit::IrVisitor::visit_expr(&mut s, e);
    s.hit
}

impl WritebackHoister<'_> {
    /// Try the hoist on a value- or unit-position branch node whose arms
    /// include at least one write-back block. Returns false = shape not
    /// covered (left for the wall).
    fn try_hoist(&mut self, e: &mut IrExpr) -> bool {
        // Collect arm expressions (then/else, or every match arm body).
        let arms: Vec<&IrExpr> = match &e.kind {
            IrExprKind::If { then, else_, .. } => vec![then, else_],
            IrExprKind::Match { arms, .. } => arms.iter().map(|a| &a.body).collect(),
            _ => return false,
        };
        // One agreed (p, form) across every write-back arm.
        let mut agreed: Option<(VarId, Option<Ty>)> = None;
        for a in &arms {
            if let Some((p, _, ref orig)) = as_writeback_block(a) {
                match &agreed {
                    None => agreed = Some((p, orig.clone())),
                    Some((q, prev)) if *q == p && prev == orig => {}
                    _ => return false,
                }
            }
        }
        let Some((p, orig_ty)) = agreed else { return false };
        // Non-write-back arms must not write p any other way; neither may
        // the subject or a guard.
        for a in &arms {
            if as_writeback_block(a).is_none() && assigns_var(a, p) {
                return false;
            }
        }
        if let IrExprKind::Match { subject, arms, .. } = &e.kind {
            if assigns_var(subject, p) {
                return false;
            }
            for a in arms.iter() {
                if let Some(g) = &a.guard
                    && assigns_var(g, p)
                {
                    return false;
                }
            }
        }
        if let IrExprKind::If { cond, .. } = &e.kind
            && assigns_var(cond, p)
        {
            return false;
        }
        let mut_ty = self.vt.get(p).ty.clone();
        let p_read = |ty: Ty| IrExpr {
            kind: IrExprKind::Var { id: p },
            ty,
            span: None,
            def_id: None,
        };
        let span = e.span;
        match orig_ty {
            // was-Unit form: arms yield the buffer; one Assign outside.
            None => {
                let rewrite = |arm: &mut IrExpr, mut_ty: &Ty| {
                    if let Some((_, _, _)) = as_writeback_block(arm) {
                        let IrExprKind::Block { stmts, .. } = &mut arm.kind else { unreachable!() };
                        let IrStmtKind::Bind { value, .. } = stmts.remove(0).kind else {
                            unreachable!()
                        };
                        *arm = value;
                    } else {
                        let inner = std::mem::replace(
                            arm,
                            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
                        );
                        arm.kind = IrExprKind::Block {
                            stmts: vec![IrStmt {
                                kind: IrStmtKind::Expr { expr: inner },
                                span: None,
                            }],
                            expr: Some(Box::new(IrExpr {
                                kind: IrExprKind::Var { id: p },
                                ty: mut_ty.clone(),
                                span: None,
                                def_id: None,
                            })),
                        };
                    }
                    arm.ty = mut_ty.clone();
                };
                match &mut e.kind {
                    IrExprKind::If { then, else_, .. } => {
                        rewrite(then, &mut_ty);
                        rewrite(else_, &mut_ty);
                    }
                    IrExprKind::Match { arms, .. } => {
                        for a in arms.iter_mut() {
                            rewrite(&mut a.body, &mut_ty);
                        }
                    }
                    _ => unreachable!(),
                }
                let branch = std::mem::replace(
                    e,
                    IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span, def_id: None },
                );
                let mut branch = branch;
                branch.ty = mut_ty.clone();
                e.kind = IrExprKind::Block {
                    stmts: vec![IrStmt {
                        kind: IrStmtKind::Assign { var: p, value: branch },
                        span,
                    }],
                    expr: None,
                };
                e.ty = Ty::Unit;
            }
            // value form: arms yield (orig, buffer); destructure + one
            // Assign outside, the branch's own value restored as the tail.
            Some(orig) => {
                let tuple_ty = Ty::Tuple(vec![orig.clone(), mut_ty.clone()]);
                let rewrite = |arm: &mut IrExpr, tuple_ty: &Ty, orig: &Ty, mut_ty: &Ty| {
                    if as_writeback_block(arm).is_some() {
                        let IrExprKind::Block { stmts, .. } = &mut arm.kind else { unreachable!() };
                        let IrStmtKind::BindDestructure { value, .. } = stmts.remove(0).kind
                        else {
                            unreachable!()
                        };
                        *arm = value;
                    } else {
                        let inner = std::mem::replace(
                            arm,
                            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
                        );
                        arm.kind = IrExprKind::Tuple {
                            elements: vec![inner, IrExpr {
                                kind: IrExprKind::Var { id: p },
                                ty: mut_ty.clone(),
                                span: None,
                                def_id: None,
                            }],
                        };
                        let _ = orig;
                    }
                    arm.ty = tuple_ty.clone();
                };
                match &mut e.kind {
                    IrExprKind::If { then, else_, .. } => {
                        rewrite(then, &tuple_ty, &orig, &mut_ty);
                        rewrite(else_, &tuple_ty, &orig, &mut_ty);
                    }
                    IrExprKind::Match { arms, .. } => {
                        for a in arms.iter_mut() {
                            rewrite(&mut a.body, &tuple_ty, &orig, &mut_ty);
                        }
                    }
                    _ => unreachable!(),
                }
                let res = self.vt.alloc(sym("__mp_hres"), orig.clone(), Mutability::Let, None);
                let buf = self.vt.alloc(sym("__mp_hbuf"), mut_ty.clone(), Mutability::Let, None);
                let mut branch = std::mem::replace(
                    e,
                    IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span, def_id: None },
                );
                branch.ty = tuple_ty.clone();
                e.kind = IrExprKind::Block {
                    stmts: vec![
                        IrStmt {
                            kind: IrStmtKind::BindDestructure {
                                pattern: IrPattern::Tuple {
                                    elements: vec![
                                        IrPattern::Bind { var: res, ty: orig.clone() },
                                        IrPattern::Bind { var: buf, ty: mut_ty.clone() },
                                    ],
                                },
                                value: branch,
                            },
                            span,
                        },
                        IrStmt {
                            kind: IrStmtKind::Assign {
                                var: p,
                                value: IrExpr {
                                    kind: IrExprKind::Var { id: buf },
                                    ty: mut_ty.clone(),
                                    span: None,
                                    def_id: None,
                                },
                            },
                            span,
                        },
                    ],
                    expr: Some(Box::new(IrExpr {
                        kind: IrExprKind::Var { id: res },
                        ty: orig.clone(),
                        span: None,
                        def_id: None,
                    })),
                };
                e.ty = orig;
            }
        }
        let _ = p_read;
        true
    }
}

impl IrMutVisitor for WritebackHoister<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Bottom-up, so a nested branch's own write-backs hoist first and
        // the outer branch sees plain expressions.
        walk_expr_mut(self, expr);
        if matches!(expr.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) {
            self.try_hoist(expr);
        }
    }
}

/// Phase 3: fold a write-back that immediately flows into the fn's own
/// move-mode return INTO a direct tail call — the recursion/terminal-`if`
/// shape (#1207 repro A):
///
///   fn walk(buf, n) -> Bytes = { if n == 0 then () else { …; let t = walk(buf, n-1)!; buf = t }; buf }
///   →                          { if n == 0 then buf else { …; walk(buf, n-1)! } }
///
/// `{ let t = call; p = t }; return p` at the very end of the fn is the
/// identity on `return call` (no read of `p` intervenes), so the fold is
/// semantics-preserving — and it removes the one write-back the lowering has
/// no sound ownership story for: a borrowed-param slot reassigned INSIDE a
/// branch arm (drop-old frees the caller's reference; skipping drop-old
/// either leaks a reference per call — the kernel-proven ownership checker
/// rejects the witness — or, without the acquire, frees the buffer the slot
/// still aliases). After the fold every path is a PROVEN shape: the untaken
/// arm returns the borrowed param (the plain pass-through), the call arm is
/// an ordinary effect/value tail call whose result IS the fn's result.
///
/// NARROW by construction: only a was-Unit mut fn whose phase-1 body is
/// `Block{ …, Expr(If), tail: Var(mut_param) }` (the exact shape
/// `rewrite_unit_body` built), and only arm LEAVES that are the phase-2
/// rewriter's own write-back block targeting that same param. Anything else
/// is left untouched (the lowering's honest wall keeps guarding it).
fn fold_tail_writebacks(program: &mut IrProgram, mut_fns: &MutFns) {
    for func in program.functions.iter_mut() {
        fold_one_fn_tail_writebacks(func, "", mut_fns);
    }
    for m in program.modules.iter_mut() {
        let scope = m.name.to_string();
        for func in m.functions.iter_mut() {
            fold_one_fn_tail_writebacks(func, &scope, mut_fns);
        }
    }
}

fn fold_one_fn_tail_writebacks(func: &mut IrFunction, scope: &str, mut_fns: &MutFns) {
    // Only a rewritten was-Unit mut fn (phase 1 cleared `mutated_params`; the
    // scope-keyed entry survives).
    let Some(&(idx, ref mut_ty, was_unit, _, ref extra)) = mut_fns.get(&scope_key(scope, func.name.as_str())) else { return };
    if !was_unit || !extra.is_empty() {
        return;
    }
    let Some(p) = func.params.get(idx).map(|prm| prm.var) else { return };
    let IrExprKind::Block { stmts, expr: tail } = &mut func.body.kind else { return };
    // The phase-1 tail must be exactly the mut-param read.
    if !matches!(tail.as_deref().map(|t| &t.kind), Some(IrExprKind::Var { id }) if *id == p) {
        return;
    }
    // The last statement must be the terminal Unit `if` (or `match` —
    // the same branch-leaf write-back shape, #1688's second column).
    let Some(last) = stmts.last_mut() else { return };
    let IrStmtKind::Expr { expr: if_expr } = &mut last.kind else { return };
    if !matches!(if_expr.kind, IrExprKind::If { .. } | IrExprKind::Match { .. }) {
        return;
    }
    let mut folded = if_expr.clone();
    if !fold_if_arms(&mut folded, p, mut_ty, scope, mut_fns) {
        return;
    }
    folded.ty = mut_ty.clone();
    stmts.pop();
    *tail = Some(Box::new(folded));
}

/// Rewrite every leaf arm of the terminal `if` tree: a write-back leaf becomes
/// the direct tail call, any other Unit leaf becomes (or is followed by) the
/// param read. Returns false (fold declined, tree possibly half-mutated — the
/// caller drops the clone) if a leaf is outside the recognized set.
fn fold_if_arms(e: &mut IrExpr, p: VarId, mut_ty: &Ty, scope: &str, mut_fns: &MutFns) -> bool {
    if let IrExprKind::If { then, else_, .. } = &mut e.kind {
        e.ty = mut_ty.clone();
        return fold_if_arms(then, p, mut_ty, scope, mut_fns) && fold_if_arms(else_, p, mut_ty, scope, mut_fns);
    }
    // A terminal `match` folds arm-wise like `if` (#1688): every arm body
    // is a leaf of the same vocabulary. Guards are Bool expressions — a
    // Unit write-back cannot appear there, so only bodies are rewritten.
    if let IrExprKind::Match { arms, .. } = &mut e.kind {
        e.ty = mut_ty.clone();
        return arms
            .iter_mut()
            .all(|arm| fold_if_arms(&mut arm.body, p, mut_ty, scope, mut_fns));
    }
    let param_read = |span| IrExpr {
        kind: IrExprKind::Var { id: p },
        ty: mut_ty.clone(),
        span,
        def_id: None,
    };
    // A write-back leaf: `Block{ …, let t = <call maybe under !>, p = t; () }`
    // — the phase-2 rewriter's own output. Fold to `Block{ …, tail: <call> }`.
    if let IrExprKind::Block { stmts: _, expr } = &mut e.kind {
        // The rotation leaves the write-back block as the arm block's TAIL
        // (`{ …; { let t = call!; p = t; () } }`) — recurse into a structured
        // tail so the fold reaches the leaf.
        if let Some(t) = expr.as_deref_mut()
            && matches!(
                t.kind,
                IrExprKind::Block { .. } | IrExprKind::If { .. } | IrExprKind::Match { .. }
            )
        {
            if !fold_if_arms(t, p, mut_ty, scope, mut_fns) {
                return false;
            }
            e.ty = mut_ty.clone();
            return true;
        }
    }
    if let IrExprKind::Block { stmts, expr } = &mut e.kind {
        if matches!(expr.as_deref().map(|t| &t.kind), None | Some(IrExprKind::Unit))
            && stmts.len() >= 2
        {
            let is_wb = {
                let assign_ok = matches!(
                    &stmts[stmts.len() - 1].kind,
                    IrStmtKind::Assign { var, value } if *var == p
                        && matches!(&value.kind, IrExprKind::Var { .. })
                );
                let bind_call = match &stmts[stmts.len() - 2].kind {
                    IrStmtKind::Bind { value, .. } => {
                        let inner = match &value.kind {
                            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => expr,
                            _ => value,
                        };
                        matches!(&inner.kind,
                            IrExprKind::Call { target, args, .. }
                                if call_spelling(target).is_some_and(|name| mut_fns.contains_key(&name) || mut_fns.contains_key(&scope_key(scope, &name)))
                                    && args.iter().any(|a| matches!(&a.kind, IrExprKind::Var { id } if *id == p)))
                    }
                    _ => false,
                };
                assign_ok && bind_call
            };
            if is_wb {
                stmts.pop(); // the write-back Assign
                let Some(IrStmt { kind: IrStmtKind::Bind { value, .. }, .. }) = stmts.pop() else {
                    unreachable!("is_wb checked the bind");
                };
                e.ty = mut_ty.clone();
                let IrExprKind::Block { expr, .. } = &mut e.kind else { unreachable!() };
                *expr = Some(Box::new(value));
                return true;
            }
        }
        // A Unit block leaf with no write-back: keep its statements, return the param.
        if matches!(expr.as_deref().map(|t| &t.kind), None | Some(IrExprKind::Unit)) {
            let span = e.span;
            e.ty = mut_ty.clone();
            let IrExprKind::Block { expr, .. } = &mut e.kind else { unreachable!() };
            *expr = Some(Box::new(param_read(span)));
            return true;
        }
        return false;
    }
    // A bare Unit leaf (`()` arm): the param read.
    if matches!(e.kind, IrExprKind::Unit) {
        *e = param_read(e.span);
        return true;
    }
    // Any other Unit-typed leaf expression (`bytes.push(out, b)` as a bare
    // arm, a println, a loop): keep it as a statement and return the param
    // — the same treatment the Unit BLOCK leaf already gets. Post-phase-2
    // no bare user mut-call can sit here (every one became a write-back
    // block), so the leaf mutates `p` only through conventions the
    // lowering already owns in straight-line position (#1688).
    if matches!(e.ty, Ty::Unit) {
        let span = e.span;
        let leaf = std::mem::replace(e, param_read(span));
        e.kind = IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: leaf }, span }],
            expr: Some(Box::new(param_read(span))),
        };
        return true;
    }
    false
}

/// The SCOPED bare-name key: a bare `CallTarget::Named` call resolves inside
/// its own scope only (the main file, or one module's own body — every
/// cross-module spelling is mangled by `resolve_user_module_calls` before this
/// pass runs), so bare keys are namespaced per scope. `""` is the main scope.
/// This is what makes a user `fn replace` immune to stdlib/string.almd's
/// unrelated `replace` (#1558: the old GLOBAL bare count silently excluded the
/// user fn from the rewrite and the wall message never said why).
pub(crate) fn scope_key(scope: &str, name: &str) -> String {
    format!("{scope}\u{1}{name}")
}

/// The spelling a call site uses for a collected fn: a bare/dotted/mangled
/// `Named` name as written, or the `module.func` key of a RESOLVED
/// `CallTarget::Module` call — the shape a cross-module call to a user fn
/// (`place_order.place_order(repo, ..)` from main, its mono instance
/// included) arrives in. Before the Module arm the DDD tree's can-err
/// entry points were collected (the dotted key existed) but their call
/// sites never rewrote, and the callee's tuple met the caller's record
/// (structural `ty-mismatch:Tuple-vs-Named`).
pub(crate) fn call_spelling(target: &CallTarget) -> Option<String> {
    match target {
        CallTarget::Named { name } => Some(name.to_string()),
        // A STDLIB module call (`list.clear(xs)`, `string.push(s, c)`) is
        // lowered in place by the emitters' own mutator paths and must not
        // be rewritten (rewriting it walled eleven fixtures with
        // `call-unit-in-value`); only a USER module's resolved call takes
        // the move-mode form.
        CallTarget::Module { module, .. }
            if almide_lang::stdlib_info::is_any_stdlib(module.as_str()) =>
        {
            None
        }
        CallTarget::Module { module, func, .. } => Some(format!("{module}.{func}")),
        CallTarget::Method { .. } | CallTarget::Computed { .. } => None,
    }
}

/// Functions eligible for the move-mode rewrite: name → (mut param index, its
/// type, whether the callee returned Unit before the rewrite).
///
/// Non-Unit effect fns take the tuple form too (#1576). Call sites are
/// keyed by BARE name, so a name that resolves to more than one function
/// (same-name fns across modules, the #692 class) must be excluded wholesale:
/// rewriting the callee but not a caller — or a caller of the OTHER same-name
/// fn — leaves an invalid module (the pass previously indexed
/// `mutated_params[0]` on the same-name NON-mut sibling and panicked).
fn collect_mut_fns(program: &IrProgram) -> MutFns {
    // Bare-name ambiguity is PER SCOPE: a bare call site can only refer to a
    // fn in its own scope, so only a same-scope duplicate makes the rewrite
    // unsafe (#692). The old global count also swept every linked stdlib
    // module, so a user fn merely SHARING a verb with string.replace et al.
    // was silently excluded and its wasm leg walled (#1558).
    let mut name_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // A `Type.method` spelling is resolved by TYPE, not by scope: a
    // monomorphized generic caller calls the bound's method as
    // `Named{MemoryOrderRepo.save}` from ITS module (the DDD use case in
    // `place_order`), while the method lives in `memory_repo`. Such a
    // name gets a GLOBAL key when exactly one definition in the whole
    // program spells it; before that the cross-module method call was
    // never rewritten, the callee's returned buffer was dropped, and the
    // repository silently lost every save (native `rev=$37.50`, wasm
    // `rev=$0.00` then `no such order`).
    let mut method_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for func in program.functions.iter() {
        *name_count.entry(scope_key("", func.name.as_str())).or_insert(0) += 1;
        if func.name.as_str().contains('.') {
            *method_count.entry(func.name.to_string()).or_insert(0) += 1;
        }
    }
    for m in &program.modules {
        for func in &m.functions {
            *name_count.entry(scope_key(m.name.as_str(), func.name.as_str())).or_insert(0) += 1;
            if func.name.as_str().contains('.') {
                *method_count.entry(func.name.to_string()).or_insert(0) += 1;
            }
        }
    }
    let mut mut_fns: MutFns = std::collections::HashMap::new();
    // Program-level functions first, then module ones — the original
    // collection order, which a duplicate name would otherwise decide.
    for func in program.functions.iter() {
        if let Some(entry) = mut_fn_entry(func, name_count.get(&scope_key("", func.name.as_str())).copied().unwrap_or(0)) {
            mut_fns.insert(scope_key("", func.name.as_str()), entry);
        }
    }
    for m in &program.modules {
        for func in &m.functions {
            if let Some(entry) = mut_fn_entry(func, name_count.get(&scope_key(m.name.as_str(), func.name.as_str())).copied().unwrap_or(0)) {
                // A MODULE fn's call sites spell THREE names: bare from inside
                // the module, MODULE-QUALIFIED (`convmut.Box.bump`) pre-resolution,
                // and the MANGLED runtime symbol (`almide_rt_convmut_Box_bump`)
                // after `resolve_user_module_calls`. The body rewrite
                // keys the bare name; the CALL-SITE rewriter must hit either
                // spelling or the rewritten callee's returned buffer is left
                // unconsumed on the caller's stack (invalid wasm — the #1549
                // cross-module mut-receiver leg). The bare name passed the
                // uniqueness guard above, so the qualified alias is likewise
                // unambiguous.
                mut_fns.insert(
                    format!(
                        "almide_rt_{}_{}",
                        m.name.as_str().replace('.', "_"),
                        func.name.as_str().replace('.', "_")
                    ),
                    entry.clone(),
                );
                mut_fns.insert(format!("{}.{}", m.name.as_str(), func.name.as_str()), entry.clone());
                if method_count.get(func.name.as_str()) == Some(&1) {
                    mut_fns.insert(func.name.to_string(), entry.clone());
                }
                mut_fns.insert(scope_key(m.name.as_str(), func.name.as_str()), entry);
            }
        }
    }
    if almide_base::env::flag("ALMIDE_MP_PROBE") {
        for (k, v) in &mut_fns {
            eprintln!("[mp] fn {} → {:?}", k, v);
        }
    }
    mut_fns
}

/// One function's [`MutFns`] entry, or `None` when it is not eligible.
fn mut_fn_entry(func: &IrFunction, same_scope_count: usize) -> Option<MutEntry> {
    let probe = almide_base::env::flag("ALMIDE_MP_PROBE");
    if func.mutated_params.is_empty() {
        return None;
    }
    if same_scope_count != 1 {
        if probe {
            eprintln!("[mp-reject] {} same_scope_count={}", func.name, same_scope_count);
        }
        return None;
    }
    let mut idxs = func.mutated_params.clone();
    idxs.sort_unstable();
    idxs.dedup();
    let idx = idxs[0];
    let p = func.params.get(idx)?;
    let mut extra = Vec::new();
    for &i in &idxs[1..] {
        extra.push((i, func.params.get(i)?.ty.clone()));
    }
    let was_unit = matches!(func.ret_ty, Ty::Unit);
    let Some(payload) = value_payload_ty(func) else {
        if probe {
            eprintln!("[mp-reject] {} declared-Result effect fn with a non-String err carrier", func.name);
        }
        return None;
    };
    Some((idx, p.ty.clone(), was_unit, payload, extra))
}

/// The type the tuple rewrite pairs with the buffer — the callee's ORIGINAL
/// return, except for a declared-Result EFFECT fn (`-> T!`), whose Result is
/// the single-layer effect channel rather than a value: there the payload is
/// the ok type `T`, and the body is stripped to it by [`strip_ok_layer`].
/// `None` for a declared-Result effect fn whose err type is not the default
/// `String` carrier — the lifted `Result[(T, Buf), String]` the wasm ABI
/// builds for a raw tuple could not carry it, so the fn stays excluded.
fn value_payload_ty(func: &IrFunction) -> Option<Ty> {
    match single_layer_ok_ty(func) {
        Some((ok_ty, err_ty)) => matches!(err_ty, Ty::String).then_some(ok_ty),
        None => Some(func.ret_ty.clone()),
    }
}

/// `Some((T, E))` when `func` is an effect fn declared `-> Result[T, E]`.
fn single_layer_ok_ty(func: &IrFunction) -> Option<(Ty, Ty)> {
    if !func.is_effect {
        return None;
    }
    Some((func.ret_ty.result_ok_ty()?, func.ret_ty.result_err_ty()?))
}

/// Phase 1: rewrite function bodies. Unit-returning fns return the
/// mutated param; value-returning fns return (orig, mutated) as a tuple
/// (#705 — previously the non-Unit case was silently skipped, so the
/// caller's List never saw a reallocating push: `len=1` on wasm vs
/// `len=3` native, and mlp's loss printed 0.0).
fn rewrite_signatures(program: &mut IrProgram, mut_fns: &MutFns) {
    let vt = &mut program.var_table;
    for func in program.functions.iter_mut() {
        rewrite_one_signature(func, "", vt, mut_fns);
    }
    // Module fns index their MODULE's table — allocating their writeback
    // locals in the program table hands them VarIds past their own table's
    // len (the spliced-mut-param ICE, #1700: zlib was the first registry
    // module with `mut` params).
    for m in program.modules.iter_mut() {
        let scope = m.name.to_string();
        let mvt = &mut m.var_table;
        for func in m.functions.iter_mut() {
            rewrite_one_signature(func, &scope, mvt, mut_fns);
        }
    }
}

/// Give one eligible function the move-mode signature and body.
fn rewrite_one_signature(func: &mut IrFunction, scope: &str, vt: &mut VarTable, mut_fns: &MutFns) {
    let Some((entry_idx, _, was_unit, _, extra)) = mut_fns.get(&scope_key(scope, func.name.as_str())).cloned() else { return };
    // Name-keyed entry — confirm THIS func is the one that was
    // collected (unique-name invariant above makes this a plain
    // assertion, but stay defensive).
    let Some(&mut_idx) = func.mutated_params.iter().min() else { return };
    if mut_idx != entry_idx {
        return;
    }
    let mut_var = func.params[mut_idx].var;
    let mut_ty = func.params[mut_idx].ty.clone();
    if !extra.is_empty() {
        let mut bufs = vec![(mut_var, mut_ty)];
        bufs.extend(extra.iter().map(|(i, t)| (func.params[*i].var, t.clone())));
        rewrite_multi_body(func, vt, &bufs, was_unit);
    } else if was_unit {
        rewrite_unit_body(func, mut_var, mut_ty);
    } else {
        rewrite_value_body(func, vt, mut_var, mut_ty);
    }
    // The convention is now explicit in the tree; the field would
    // otherwise keep tripping mut-param gates (the v1 C-132 wall).
    func.mutated_params.clear();
}

/// A fn with SEVERAL `mut` params (#2907): `{ <old body>; (p1, …, pN) }` when
/// it returned Unit, `{ let __mp_ret = <old body>; (__mp_ret, p1, …, pN) }`
/// otherwise — the single-param forms below with every buffer in the tuple.
/// Its guard exits are paired with the same buffers.
fn rewrite_multi_body(func: &mut IrFunction, vt: &mut VarTable, bufs: &[(VarId, Ty)], was_unit: bool) {
    let single_layer = single_layer_ok_ty(func).map(|(ok_ty, _)| ok_ty);
    let orig_ty = single_layer.clone().unwrap_or_else(|| func.ret_ty.clone());
    let mut elems: Vec<Ty> = if was_unit { vec![] } else { vec![orig_ty.clone()] };
    elems.extend(bufs.iter().map(|(_, t)| t.clone()));
    let tuple_ty = Ty::Tuple(elems);
    func.ret_ty = tuple_ty.clone();
    let reads = || bufs.iter().map(|(v, t)| var_read(*v, t.clone())).collect::<Vec<_>>();
    let mut old_body = std::mem::replace(&mut func.body, unit_placeholder());
    if let Some(ok_ty) = &single_layer {
        strip_ok_layer(&mut old_body, ok_ty);
    }
    let ty = tuple_ty.clone();
    pair_guard_exits(&mut old_body, func.is_effect, &tuple_ty, &mut |e| {
        let span = e.span;
        let mut elements = if was_unit { vec![] } else { vec![e.clone()] };
        elements.extend(reads());
        let tuple = IrExpr { kind: IrExprKind::Tuple { elements }, ty: ty.clone(), span, def_id: None };
        if was_unit {
            IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: e }, span: None }],
                    expr: Some(Box::new(tuple)),
                },
                ty: ty.clone(),
                span,
                def_id: None,
            }
        } else {
            tuple
        }
    });
    let (stmt, mut elements) = if was_unit {
        (IrStmt { kind: IrStmtKind::Expr { expr: old_body }, span: None }, vec![])
    } else {
        let ret_var = vt.alloc(sym("__mp_ret"), orig_ty.clone(), Mutability::Let, None);
        let bind = IrStmt {
            kind: IrStmtKind::Bind { var: ret_var, mutability: Mutability::Let, ty: orig_ty.clone(), value: old_body },
            span: None,
        };
        (bind, vec![var_read(ret_var, orig_ty)])
    };
    elements.extend(reads());
    let tuple = IrExpr { kind: IrExprKind::Tuple { elements }, ty: tuple_ty.clone(), span: None, def_id: None };
    func.body = IrExpr {
        kind: IrExprKind::Block { stmts: vec![stmt], expr: Some(Box::new(tuple)) },
        ty: tuple_ty,
        span: None,
        def_id: None,
    };
}

/// Unit-returning callee: `{ <old body>; mut_param }` — wrap the existing body
/// in a block whose tail reads the mutated param.
fn rewrite_unit_body(func: &mut IrFunction, mut_var: VarId, mut_ty: Ty) {
    func.ret_ty = mut_ty.clone();
    let mut old_body = std::mem::replace(&mut func.body, unit_placeholder());
    let new_ret = func.ret_ty.clone();
    pair_guard_exits(&mut old_body, func.is_effect, &new_ret, &mut |e| IrExpr {
        ty: mut_ty.clone(),
        span: e.span,
        def_id: None,
        kind: IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: e }, span: None }],
            expr: Some(Box::new(var_read(mut_var, mut_ty.clone()))),
        },
    });
    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: old_body }, span: None }],
            expr: Some(Box::new(var_read(mut_var, mut_ty))),
        },
        ty: func.ret_ty.clone(),
        span: None,
        def_id: None,
    };
}

/// Value-returning callee: `{ let __mp_ret: T = <old body>; (__mp_ret, mut_param) }`
/// — the body runs first (its mutations land in the param local), then the
/// tuple pairs the original result with the final buffer.
fn rewrite_value_body(func: &mut IrFunction, vt: &mut VarTable, mut_var: VarId, mut_ty: Ty) {
    let single_layer = single_layer_ok_ty(func).map(|(ok_ty, _)| ok_ty);
    let orig_ty = single_layer.clone().unwrap_or_else(|| func.ret_ty.clone());
    let tuple_ty = Ty::Tuple(vec![orig_ty.clone(), mut_ty.clone()]);
    func.ret_ty = tuple_ty.clone();
    let ret_var = vt.alloc(sym("__mp_ret"), orig_ty.clone(), Mutability::Let, None);
    let mut old_body = std::mem::replace(&mut func.body, unit_placeholder());
    if let Some(ok_ty) = &single_layer {
        strip_ok_layer(&mut old_body, ok_ty);
    }
    pair_guard_exits(&mut old_body, func.is_effect, &tuple_ty.clone(), &mut |e| IrExpr {
        ty: tuple_ty.clone(),
        span: e.span,
        def_id: None,
        kind: IrExprKind::Tuple { elements: vec![e, var_read(mut_var, mut_ty.clone())] },
    });
    let tuple = IrExpr {
        kind: IrExprKind::Tuple {
            elements: vec![var_read(ret_var, orig_ty.clone()), var_read(mut_var, mut_ty)],
        },
        ty: tuple_ty.clone(),
        span: None,
        def_id: None,
    };
    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![IrStmt {
                kind: IrStmtKind::Bind {
                    var: ret_var,
                    mutability: Mutability::Let,
                    ty: orig_ty,
                    value: old_body,
                },
                span: None,
            }],
            expr: Some(Box::new(tuple)),
        },
        ty: tuple_ty,
        span: None,
        def_id: None,
    };
}

/// A `guard c else v` in a move-mode fn's body RETURNS `v` from the fn, past
/// the `(ret, buf)` / `buf` tail the rewrite built (#2907): the guard's value
/// exit is paired with the buffer the same way (`pair`). A lambda's guards are
/// the lambda's own exits and are left alone. In an EFFECT fn a raising else
/// (`err(m)`, `err(m)!`, any Result-typed exit) is the err channel — it carries
/// its buffer by other means (`mut_param_err_carry` pairs it, #2917) — so
/// only a non-raising exit is paired here.
fn pair_guard_exits(body: &mut IrExpr, is_effect: bool, new_ret: &Ty, pair: &mut dyn FnMut(IrExpr) -> IrExpr) {
    struct Pairer<'a> {
        is_effect: bool,
        new_ret: &'a Ty,
        pair: &'a mut dyn FnMut(IrExpr) -> IrExpr,
    }
    /// A raising `err(m)` / `err(m)!` exit of an effect fn is typed with the
    /// fn's ORIGINAL ok type; the fn now returns `new_ret` through its lifted
    /// carrier, so the raise is retyped to that carrier — the propagation
    /// shape the guard's early return recognizes (`err(m)!` over the fn's own
    /// Result). The err payload is untouched.
    fn retype_raise(e: &mut IrExpr, new_ret: &Ty) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => {
                retype_raise(t, new_ret);
                e.ty = t.ty.clone();
            }
            IrExprKind::Unwrap { expr } | IrExprKind::Try { expr }
                if matches!(expr.kind, IrExprKind::ResultErr { .. }) =>
            {
                retype_raise(expr, new_ret);
                e.ty = new_ret.clone();
            }
            IrExprKind::ResultErr { .. } => {
                if let Some(err_ty) = e.ty.result_err_ty() {
                    e.ty = Ty::result(new_ret.clone(), err_ty);
                }
            }
            _ => {}
        }
    }
    fn raises(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => raises(t),
            IrExprKind::ResultErr { .. } | IrExprKind::Unwrap { .. } | IrExprKind::Try { .. } => true,
            _ => e.ty.result_ok_ty().is_some(),
        }
    }
    impl IrMutVisitor for Pairer<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            if !matches!(e.kind, IrExprKind::Lambda { .. }) {
                walk_expr_mut(self, e);
            }
        }
        fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
            walk_stmt_mut(self, stmt);
            if let IrStmtKind::Guard { else_, .. } = &mut stmt.kind {
                if self.is_effect && raises(else_) {
                    retype_raise(else_, self.new_ret);
                } else {
                    let exit = std::mem::replace(else_, unit_placeholder());
                    *else_ = (self.pair)(exit);
                }
            }
        }
    }
    Pairer { is_effect, new_ret, pair }.visit_expr_mut(body);
}

/// Strip the single-layer effect Result off a declared-Result effect fn's
/// body so it yields the raw ok payload: every TAIL position (block tail,
/// if/match arm) that constructs `ok(x)` becomes `x`; an `err(e)` tail keeps
/// its node and takes the raw type (the emitters' raise leaf — the fn's
/// lifted carrier is what it returns through); any other Result-typed tail
/// (a call returning the Result, a variable) is `!`-unwrapped, which raises
/// the same way. Statement-position `err`s (a guard's else arm) are already
/// raise leaves and are not touched.
pub(crate) fn strip_ok_layer(e: &mut IrExpr, ok_ty: &Ty) {
    match &mut e.kind {
        IrExprKind::ResultOk { expr } => {
            let inner = std::mem::replace(&mut **expr, unit_placeholder());
            *e = inner;
            e.ty = ok_ty.clone();
        }
        IrExprKind::ResultErr { .. } => e.ty = ok_ty.clone(),
        IrExprKind::Block { expr: Some(tail), .. } => {
            strip_ok_layer(tail, ok_ty);
            e.ty = ok_ty.clone();
        }
        IrExprKind::If { then, else_, .. } => {
            strip_ok_layer(then, ok_ty);
            strip_ok_layer(else_, ok_ty);
            e.ty = ok_ty.clone();
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms.iter_mut() {
                strip_ok_layer(&mut arm.body, ok_ty);
            }
            e.ty = ok_ty.clone();
        }
        _ if e.ty.result_ok_ty().is_some() => {
            let span = e.span;
            let inner = std::mem::replace(e, unit_placeholder());
            *e = IrExpr {
                kind: IrExprKind::Unwrap { expr: Box::new(inner) },
                ty: ok_ty.clone(),
                span,
                def_id: None,
            };
        }
        // Already raw (a `!`-unwrapped tail the checker lifted): nothing to strip.
        _ => {}
    }
}

/// A typed read of `id`. Span-less: these nodes are synthesized, not parsed.
fn var_read(id: VarId, ty: Ty) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None }
}

/// The throwaway node `std::mem::replace` swaps in while a body is taken.
fn unit_placeholder() -> IrExpr {
    IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None }
}

/// Phase 2: Rewrite call sites — write the mutated buffer back. A
/// bottom-up IrMutVisitor rewrites EVERY position uniformly (statement,
/// Bind/Assign RHS, nested expression, loop bodies): the callee's
/// signature changed globally, so an unrewritten site is not merely
/// un-written-back — it is an invalid module (i32 tuple vs the old
/// scalar). The call becomes a Block expression:
///
///   { let (__mp_res, __mp_buf) = <call>; <writeback>; __mp_res }
///
/// and the writeback targets the argument PLACE: a bare var assigns it,
/// a `b.items` field FieldAssigns it, and a temp (no named place) skips
/// the writeback — native mutates an invisible temp there too.
fn rewrite_call_sites(program: &mut IrProgram, mut_fns: &MutFns, carry: &std::collections::HashSet<String>) {
    use crate::mut_param_err_carry::{own_buffers, settle_sites};
    {
        let vt = &mut program.var_table;
        let mut rw = CallSiteRewriter::new(mut_fns, vt, String::new(), carry);
        for func in program.functions.iter_mut() {
            rw.own_bufs = own_buffers(func, "", mut_fns, carry);
            rw.visit_expr_mut(&mut func.body);
        }
        rw.own_bufs.clear();
        for tl in &mut program.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
        let pending = std::mem::take(&mut rw.carry_blocks);
        let bodies = program.functions.iter_mut().map(|f| &mut f.body).chain(program.top_lets.iter_mut().map(|t| &mut t.value));
        settle_sites(bodies, &pending, &mut program.var_table);
    }
    // Same table discipline as rewrite_signatures: module bodies allocate
    // their call-site temporaries in the MODULE's table.
    for m in &mut program.modules {
        let scope = m.name.to_string();
        let mut rw = CallSiteRewriter::new(mut_fns, &mut m.var_table, scope.clone(), carry);
        for func in m.functions.iter_mut() {
            rw.own_bufs = own_buffers(func, &scope, mut_fns, carry);
            rw.visit_expr_mut(&mut func.body);
        }
        rw.own_bufs.clear();
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
        let pending = std::mem::take(&mut rw.carry_blocks);
        let bodies = m.functions.iter_mut().map(|f| &mut f.body).chain(m.top_lets.iter_mut().map(|t| &mut t.value));
        settle_sites(bodies, &pending, &mut m.var_table);
    }
}

/// name → (mut param index, its type, was-Unit, the callee's ORIGINAL raw
/// return type). The ret rides along because a `!`-wrapped effect call's Call
/// NODE carries the lifted `Result[T, String]` carrier, not T — the caller's
/// destructure element must be typed by the callee's declaration, never by
/// the call expression (#1575; the same lifted-carrier trap as #1573).
pub(crate) type MutFns = std::collections::HashMap<String, MutEntry>;

/// A fn's move-mode entry: its FIRST `mut` param (index, type), was-Unit, the
/// raw payload type, and every further `mut` param (#2907). With extras the
/// fn returns ALL its buffers — `(buf1, …, bufN)` for a was-Unit fn,
/// `(ret, buf1, …, bufN)` otherwise — and each call site writes each back.
pub(crate) type MutEntry = (usize, Ty, bool, Ty, Vec<(usize, Ty)>);

use crate::mut_param_place::{mut_arg_place, writeback_stmts, ArgPlace};

struct CallSiteRewriter<'a> {
    mut_fns: &'a MutFns,
    vt: &'a mut VarTable,
    /// The scope whose bodies are currently being rewritten ("" = main): a
    /// BARE callee name resolves against this scope's key; the mangled and
    /// dotted spellings are global keys tried first.
    scope: String,
    /// The can-err callees that carry their buffer on the err arm (#2917,
    /// `mut_param_err_carry`).
    carry: &'a std::collections::HashSet<String>,
    /// The site blocks of those callees, by their first bound var: a `!`
    /// over one is lowered where it is met, the rest take the Result-valued
    /// form once the walk is done.
    carry_blocks: std::collections::HashSet<VarId>,
    /// The buffers the fn being walked carries on its own err arm.
    own_bufs: std::collections::HashSet<VarId>,
}

impl IrMutVisitor for CallSiteRewriter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Bottom-up: children first, so a mut-call argument nested inside
        // another mut-call is already rewritten when the outer one wraps.
        walk_expr_mut(self, expr);

        // The effect-call wrapper interaction (#1207): `h(a)!` arrives here as
        // `Unwrap{Call}` (`Try{Call}` for the frontend's propagation wrap), and the
        // bottom-up walk above has ALREADY turned the inner Call into the move-mode
        // Block — leaving `Unwrap{Block{…}}`, a shape no downstream lowering accepts
        // (the effect-statement lowerer wants a call, the match machinery an
        // untracked-subject wall). Rotate the wrapper back onto the bound call:
        //   Unwrap{ Block{ [let __mp_buf = call, <writeback>], () } }
        //   → Block{ [let __mp_buf = Unwrap{call}, <writeback>], () }
        // — semantically identical (the unwrap yields the buffer the ok arm
        // carries; err propagates before any writeback, exactly the by-reference
        // order native observes), and the tree now carries only proven shapes
        // (the C-222 bind-position unwrap + a statement Block).
        if crate::mut_param_err_carry::propagate_site(expr, &mut self.carry_blocks, self.vt, &self.own_bufs) {
            return;
        }
        if self.wrapper_rotation_applies(expr) {
            Self::rotate_wrapper_into_block(expr);
            return;
        }
        if self.result_payload_rotation_applies(expr) {
            Self::rotate_wrapper_onto_tail(expr);
            return;
        }

        if almide_base::env::flag("ALMIDE_MP_PROBE")
            && let IrExprKind::Call { target, .. } = &expr.kind
        {
            eprintln!("[mp-call] scope={:?} target={:?}", self.scope, target);
        }
        let IrExprKind::Call { target, args, .. } = &expr.kind else {
            return;
        };
        let Some(name) = call_spelling(target) else { return };
        let Some((idx, mut_ty, was_unit, callee_ret, extra)) = self.lookup_mut_fn(&name).cloned() else {
            return;
        };
        let carries = self.carry.contains(&name) || self.carry.contains(&scope_key(&self.scope, &name));
        if !extra.is_empty() {
            self.rewrite_multi_call(expr, idx, mut_ty, was_unit, callee_ret, &extra);
            self.carry_site(&*expr, carries);
            return;
        }
        let Some(arg) = args.get(idx) else { return };
        let place = mut_arg_place(arg);
        let span = expr.span;

        // The callee's declared raw return, NOT expr.ty: a `!`-wrapped effect
        // call's Call node is typed with the lifted carrier, and typing the
        // destructure element with it makes the tuple's scalar half read as a
        // heap handle downstream (#1575).
        let orig_ty = callee_ret;
        let mut call = std::mem::replace(
            expr,
            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
        );

        let buf = self.vt.alloc(sym("__mp_buf"), mut_ty.clone(), Mutability::Let, None);
        let buf_read = |ty: Ty| IrExpr {
            kind: IrExprKind::Var { id: buf },
            ty,
            span: None,
            def_id: None,
        };
        let writeback = writeback_stmts(&place, buf_read(mut_ty.clone()), self.vt, span);

        let (bind_stmt, tail) = if was_unit {
            // Callee now returns the buffer directly.
            call.ty = mut_ty.clone();
            let bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var: buf,
                    mutability: Mutability::Let,
                    ty: mut_ty.clone(),
                    value: call,
                },
                span,
            };
            let unit_tail =
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
            (bind, unit_tail)
        } else {
            // Callee returns (orig, buffer): destructure both — the proven
            // `let (a, b) = f(..)` ownership path (a hand-built TupleIndex
            // read left the extracted buffer aliased to a slot the tuple
            // temp's drop then freed).
            let tuple_ty = Ty::Tuple(vec![orig_ty.clone(), mut_ty.clone()]);
            call.ty = tuple_ty;
            let res = self.vt.alloc(sym("__mp_res"), orig_ty.clone(), Mutability::Let, None);
            let bind = IrStmt {
                kind: IrStmtKind::BindDestructure {
                    pattern: IrPattern::Tuple {
                        elements: vec![
                            IrPattern::Bind { var: res, ty: orig_ty.clone() },
                            IrPattern::Bind { var: buf, ty: mut_ty.clone() },
                        ],
                    },
                    value: call,
                },
                span,
            };
            let res_tail = IrExpr {
                kind: IrExprKind::Var { id: res },
                ty: orig_ty.clone(),
                span: None,
                def_id: None,
            };
            (bind, res_tail)
        };

        let mut stmts = vec![bind_stmt];
        stmts.extend(writeback);
        *expr = IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
            ty: if was_unit { Ty::Unit } else { orig_ty },
            span,
            def_id: None,
        };
        self.carry_site(&*expr, carries);
    }
}

impl<'a> CallSiteRewriter<'a> {
    fn new(
        mut_fns: &'a MutFns,
        vt: &'a mut VarTable,
        scope: String,
        carry: &'a std::collections::HashSet<String>,
    ) -> Self {
        CallSiteRewriter { mut_fns, vt, scope, carry, carry_blocks: Default::default(), own_bufs: Default::default() }
    }

    /// Record a can-err callee's site block (#2917).
    fn carry_site(&mut self, expr: &IrExpr, carries: bool) {
        if carries && let Some(v) = crate::mut_param_err_carry::site_key(expr) {
            self.carry_blocks.insert(v);
        }
    }

    /// A call to a fn with several `mut` params (#2907):
    /// `{ let (__mp_res?, b1, …, bN) = <call>; <writeback each>; __mp_res | () }`.
    fn rewrite_multi_call(
        &mut self,
        expr: &mut IrExpr,
        idx: usize,
        mut_ty: Ty,
        was_unit: bool,
        orig_ty: Ty,
        extra: &[(usize, Ty)],
    ) {
        let span = expr.span;
        let mut call = std::mem::replace(expr, unit_placeholder());
        let IrExprKind::Call { args, .. } = &call.kind else { unreachable!("the caller matched a Call") };
        let slots: Vec<(usize, Ty)> = std::iter::once((idx, mut_ty)).chain(extra.iter().cloned()).collect();
        let places: Vec<ArgPlace> = slots.iter().map(|(i, _)| args.get(*i).map_or(ArgPlace::None, mut_arg_place)).collect();
        let mut pats = Vec::new();
        let mut elem_tys = Vec::new();
        let res = (!was_unit).then(|| self.vt.alloc(sym("__mp_res"), orig_ty.clone(), Mutability::Let, None));
        if let Some(r) = res {
            pats.push(IrPattern::Bind { var: r, ty: orig_ty.clone() });
            elem_tys.push(orig_ty.clone());
        }
        let mut writebacks = Vec::new();
        for ((_, ty), place) in slots.iter().zip(places) {
            let buf = self.vt.alloc(sym("__mp_buf"), ty.clone(), Mutability::Let, None);
            pats.push(IrPattern::Bind { var: buf, ty: ty.clone() });
            elem_tys.push(ty.clone());
            let read = var_read(buf, ty.clone());
            writebacks.extend(writeback_stmts(&place, read, self.vt, span));
        }
        call.ty = Ty::Tuple(elem_tys);
        let mut stmts = vec![IrStmt {
            kind: IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements: pats }, value: call },
            span,
        }];
        stmts.extend(writebacks);
        let (tail, ty) = match res {
            Some(r) => (var_read(r, orig_ty.clone()), orig_ty),
            None => (unit_placeholder(), Ty::Unit),
        };
        *expr = IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, ty, span, def_id: None };
    }

    /// Is `expr` Unwrap/Try over a JUST-REWRITTEN move-mode Block whose first stmt
    /// binds a mut-fn call (the `h(a)!` shape, #1207)? Structurally
    /// unambiguous: after the bottom-up walk, a USER-written `Bind` of a mut-fn
    /// call has its Call already replaced by a Block, so a bare `Bind{value: Call}`
    /// to a collected name can only be the rewriter's own bind. Two shapes:
    /// the was-Unit `Bind` + Unit tail (#1207), and the value-returning
    /// `BindDestructure((res, buf))` + `res` tail — a never-err effect fn's
    /// tuple form (#1575) arrives under the same wrapper.
    fn wrapper_rotation_applies(&self, expr: &IrExpr) -> bool {
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) = &expr.kind
        else {
            return false;
        };
        let IrExprKind::Block { stmts, expr: tail } = &inner.kind else { return false };
        match stmts.first() {
            Some(IrStmt { kind: IrStmtKind::Bind { value, .. }, .. }) => {
                let IrExprKind::Call { target, .. } = &value.kind else { return false };
                let Some(name) = call_spelling(target) else { return false };
                matches!(self.lookup_mut_fn(&name), Some((_, _, true, _, extra)) if extra.is_empty())
                    && matches!(tail.as_deref().map(|t| &t.kind), Some(IrExprKind::Unit))
            }
            Some(IrStmt { kind: IrStmtKind::BindDestructure { value, .. }, .. }) => {
                let IrExprKind::Call { target, .. } = &value.kind else { return false };
                let Some(name) = call_spelling(target) else { return false };
                let tail = tail.as_deref().map(|t| &t.kind);
                match self.lookup_mut_fn(&name) {
                    Some((_, _, false, payload, _)) => {
                        payload.result_ok_ty().is_none() && matches!(tail, Some(IrExprKind::Var { .. }))
                    }
                    // A was-Unit fn with several `mut` params destructures its
                    // buffer tuple under a Unit tail (#2907).
                    Some((_, _, true, _, extra)) => !extra.is_empty() && matches!(tail, Some(IrExprKind::Unit)),
                    None => false,
                }
            }
            _ => false,
        }
    }

    /// `h(a)!` where `h` is a NON-effect fn whose declared return is itself a
    /// `Result` (#2907): its move-mode tuple is `(Result, Buf)`, a plain value,
    /// so the `!` belongs to the destructured `__mp_res`, not to the call —
    /// rotating it onto the call unwrapped the TUPLE (`unwrap-of:Tuple`). And
    /// native mutates through `&mut` before the Result is inspected, so the
    /// write-back runs on the err path too: the wrapper moves onto the tail,
    /// after the write-back. (A declared-Result EFFECT fn pairs its stripped
    /// ok payload, never a Result, and keeps the call rotation above.)
    fn result_payload_rotation_applies(&self, expr: &IrExpr) -> bool {
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) = &expr.kind
        else {
            return false;
        };
        let IrExprKind::Block { stmts, expr: tail } = &inner.kind else { return false };
        let Some(IrStmt { kind: IrStmtKind::BindDestructure { value, .. }, .. }) = stmts.first() else {
            return false;
        };
        let IrExprKind::Call { target, .. } = &value.kind else { return false };
        let Some(name) = call_spelling(target) else { return false };
        matches!(self.lookup_mut_fn(&name), Some((_, _, false, payload, _)) if payload.result_ok_ty().is_some())
            && matches!(tail.as_deref().map(|t| &t.kind), Some(IrExprKind::Var { .. }))
    }

    /// Perform [`Self::result_payload_rotation_applies`]'s rotation:
    /// `W{ Block{ stmts, res } }` → `Block{ stmts, W{ res } }`.
    fn rotate_wrapper_onto_tail(expr: &mut IrExpr) {
        let ty = expr.ty.clone();
        let is_unwrap = matches!(expr.kind, IrExprKind::Unwrap { .. });
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) =
            std::mem::replace(&mut expr.kind, IrExprKind::Unit)
        else {
            unreachable!("result_payload_rotation_applies checked the wrapper kind");
        };
        let IrExprKind::Block { stmts, expr: Some(tail) } = inner.kind else {
            unreachable!("result_payload_rotation_applies checked the block");
        };
        let span = tail.span;
        let wrapped = if is_unwrap {
            IrExprKind::Unwrap { expr: tail }
        } else {
            IrExprKind::Try { expr: tail }
        };
        let tail = IrExpr { kind: wrapped, ty: ty.clone(), span, def_id: None };
        expr.kind = IrExprKind::Block { stmts, expr: Some(Box::new(tail)) };
        expr.ty = ty;
    }

    fn lookup_mut_fn(&self, name: &str) -> Option<&MutEntry> {
        self.mut_fns.get(name).or_else(|| self.mut_fns.get(&scope_key(&self.scope, name)))
    }

    /// Perform the rotation [`Self::wrapper_rotation_applies`] admitted. The
    /// wrapper's err-propagation moves INTO the bind (`let __mp_buf = call!` /
    /// `let (__mp_res, __mp_buf) = call!`), so on the err path the writeback
    /// never runs. That is exact for a never-err callee (the err path cannot
    /// fire); a can-err callee carries its buffer on the err arm and takes
    /// `mut_param_err_carry::propagate_site` instead. Only a can-err callee
    /// whose raises cannot be paired (see `mut_param_err_carry`) still takes
    /// this order, which keeps the caller's pre-call binding on err.
    fn rotate_wrapper_into_block(expr: &mut IrExpr) {
        let span = expr.span;
        let block_ty = {
            let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) =
                &expr.kind
            else {
                unreachable!("wrapper_rotation_applies checked the wrapper kind");
            };
            inner.ty.clone()
        };
        let is_unwrap = matches!(expr.kind, IrExprKind::Unwrap { .. });
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) =
            std::mem::replace(&mut expr.kind, IrExprKind::Unit)
        else {
            unreachable!("wrapper_rotation_applies checked the wrapper kind");
        };
        let block = *inner;
        let IrExprKind::Block { mut stmts, expr: tail } = block.kind else {
            unreachable!("wrapper_rotation_applies checked the block");
        };
        {
            let (value, wrapped_ty) = match &mut stmts[0].kind {
                IrStmtKind::Bind { value, ty, .. } => (value, ty.clone()),
                IrStmtKind::BindDestructure { value, .. } => {
                    let ty = value.ty.clone();
                    (value, ty)
                }
                _ => unreachable!("wrapper_rotation_applies checked the bind"),
            };
            let call = std::mem::replace(
                value,
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            let wrapped_kind = if is_unwrap {
                IrExprKind::Unwrap { expr: Box::new(call) }
            } else {
                IrExprKind::Try { expr: Box::new(call) }
            };
            *value = IrExpr { kind: wrapped_kind, ty: wrapped_ty, span, def_id: None };
        }
        expr.kind = IrExprKind::Block { stmts, expr: tail };
        expr.ty = block_ty;
    }
}
