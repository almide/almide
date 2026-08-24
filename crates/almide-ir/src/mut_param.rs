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
//! Effect fns with a non-Unit return are SKIPPED (their return is later
//! Result-wrapped; tuple-inside-Result plumbing is a separate brick) — they
//! keep the semantics they had. A rewritten fn's `mutated_params` is CLEARED:
//! the convention is now explicit in the tree (the v1 C-132 wall keys on it,
//! and LICM's conservatism is subsumed by the call-site Assign).
//!
//! Callers: the v0 wasm nanopass (`MutParamLoweringPass`) and the v1 MIR
//! pipeline's pre-lowering (both `source_to_ir` twins — desugar-before-both).

use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
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
    rewrite_signatures(program, &mut_fns);
    rewrite_call_sites(program, &mut_fns);
    fold_tail_writebacks(program, &mut_fns);
    true
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
    let Some(&(idx, ref mut_ty, was_unit)) = mut_fns.get(&scope_key(scope, func.name.as_str())) else { return };
    if !was_unit {
        return;
    }
    let Some(p) = func.params.get(idx).map(|prm| prm.var) else { return };
    let IrExprKind::Block { stmts, expr: tail } = &mut func.body.kind else { return };
    // The phase-1 tail must be exactly the mut-param read.
    if !matches!(tail.as_deref().map(|t| &t.kind), Some(IrExprKind::Var { id }) if *id == p) {
        return;
    }
    // The last statement must be the terminal Unit `if`.
    let Some(last) = stmts.last_mut() else { return };
    let IrStmtKind::Expr { expr: if_expr } = &mut last.kind else { return };
    if !matches!(if_expr.kind, IrExprKind::If { .. }) {
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
        if let Some(t) = expr.as_deref_mut() {
            if matches!(t.kind, IrExprKind::Block { .. } | IrExprKind::If { .. }) {
                if !fold_if_arms(t, p, mut_ty, scope, mut_fns) {
                    return false;
                }
                e.ty = mut_ty.clone();
                return true;
            }
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
                            IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                                if (mut_fns.contains_key(name.as_str()) || mut_fns.contains_key(&scope_key(scope, name.as_str())))
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
    false
}

/// The SCOPED bare-name key: a bare `CallTarget::Named` call resolves inside
/// its own scope only (the main file, or one module's own body — every
/// cross-module spelling is mangled by `resolve_user_module_calls` before this
/// pass runs), so bare keys are namespaced per scope. `""` is the main scope.
/// This is what makes a user `fn replace` immune to stdlib/string.almd's
/// unrelated `replace` (#1558: the old GLOBAL bare count silently excluded the
/// user fn from the rewrite and the wall message never said why).
fn scope_key(scope: &str, name: &str) -> String {
    format!("{scope}\u{1}{name}")
}

/// Functions eligible for the move-mode rewrite: name → (mut param index, its
/// type, whether the callee returned Unit before the rewrite).
///
/// Non-Unit EFFECT fns are excluded (Result-wrap interplay). Call sites are
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
    for func in program.functions.iter() {
        *name_count.entry(scope_key("", func.name.as_str())).or_insert(0) += 1;
    }
    for m in &program.modules {
        for func in &m.functions {
            *name_count.entry(scope_key(m.name.as_str(), func.name.as_str())).or_insert(0) += 1;
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
                mut_fns.insert(scope_key(m.name.as_str(), func.name.as_str()), entry);
            }
        }
    }
    if std::env::var("ALMIDE_MP_PROBE").is_ok() {
        for (k, v) in &mut_fns {
            eprintln!("[mp] fn {} → {:?}", k, v);
        }
    }
    mut_fns
}

/// One function's [`MutFns`] entry, or `None` when it is not eligible.
fn mut_fn_entry(
    func: &IrFunction,
    same_scope_count: usize,
) -> Option<(usize, Ty, bool)> {
    if func.mutated_params.len() != 1 {
        return None;
    }
    if same_scope_count != 1 {
        return None;
    }
    let idx = func.mutated_params[0];
    let p = func.params.get(idx)?;
    let was_unit = matches!(func.ret_ty, Ty::Unit);
    if !was_unit && func.is_effect {
        return None;
    }
    Some((idx, p.ty.clone(), was_unit))
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
    for m in program.modules.iter_mut() {
        let scope = m.name.to_string();
        for func in m.functions.iter_mut() {
            rewrite_one_signature(func, &scope, vt, mut_fns);
        }
    }
}

/// Give one eligible function the move-mode signature and body.
fn rewrite_one_signature(func: &mut IrFunction, scope: &str, vt: &mut VarTable, mut_fns: &MutFns) {
    let Some(&(entry_idx, _, was_unit)) = mut_fns.get(&scope_key(scope, func.name.as_str())) else { return };
    // Name-keyed entry — confirm THIS func is the one that was
    // collected (unique-name invariant above makes this a plain
    // assertion, but stay defensive).
    let Some(&mut_idx) = func.mutated_params.first() else { return };
    if mut_idx != entry_idx {
        return;
    }
    let mut_var = func.params[mut_idx].var;
    let mut_ty = func.params[mut_idx].ty.clone();
    if was_unit {
        rewrite_unit_body(func, mut_var, mut_ty);
    } else {
        rewrite_value_body(func, vt, mut_var, mut_ty);
    }
    // The convention is now explicit in the tree; the field would
    // otherwise keep tripping mut-param gates (the v1 C-132 wall).
    func.mutated_params.clear();
}

/// Unit-returning callee: `{ <old body>; mut_param }` — wrap the existing body
/// in a block whose tail reads the mutated param.
fn rewrite_unit_body(func: &mut IrFunction, mut_var: VarId, mut_ty: Ty) {
    func.ret_ty = mut_ty.clone();
    let old_body = std::mem::replace(&mut func.body, unit_placeholder());
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
    let orig_ty = func.ret_ty.clone();
    let tuple_ty = Ty::Tuple(vec![orig_ty.clone(), mut_ty.clone()]);
    func.ret_ty = tuple_ty.clone();
    let ret_var = vt.alloc(sym("__mp_ret"), orig_ty.clone(), Mutability::Let, None);
    let old_body = std::mem::replace(&mut func.body, unit_placeholder());
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
fn rewrite_call_sites(program: &mut IrProgram, mut_fns: &MutFns) {
    let vt = &mut program.var_table;
    let mut rw = CallSiteRewriter { mut_fns, vt, scope: String::new() };
    for func in program.functions.iter_mut() {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut program.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for m in &mut program.modules {
        rw.scope = m.name.to_string();
        for func in m.functions.iter_mut() {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
    }
}

type MutFns = std::collections::HashMap<String, (usize, Ty, bool)>;

/// The caller-side slot the mutated buffer writes back into.
enum ArgPlace {
    Var(VarId),
    Field(VarId, almide_base::intern::Sym),
    /// No named place (a temp expression) — native mutates an unobservable
    /// temporary there as well, so skipping the writeback is equivalent.
    None,
}

fn mut_arg_place(arg: &IrExpr) -> ArgPlace {
    match &arg.kind {
        IrExprKind::Var { id } => ArgPlace::Var(*id),
        IrExprKind::Member { object, field } => match &object.kind {
            IrExprKind::Var { id } => ArgPlace::Field(*id, *field),
            _ => ArgPlace::None,
        },
        _ => ArgPlace::None,
    }
}

struct CallSiteRewriter<'a> {
    mut_fns: &'a MutFns,
    vt: &'a mut VarTable,
    /// The scope whose bodies are currently being rewritten ("" = main): a
    /// BARE callee name resolves against this scope's key; the mangled and
    /// dotted spellings are global keys tried first.
    scope: String,
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
        if self.wrapper_rotation_applies(expr) {
            Self::rotate_wrapper_into_block(expr);
            return;
        }

        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind else {
            return;
        };
        let Some((idx, mut_ty, was_unit)) = self
            .mut_fns
            .get(name.as_str())
            .or_else(|| self.mut_fns.get(&scope_key(&self.scope, name.as_str())))
            .cloned()
        else {
            return;
        };
        let Some(arg) = args.get(idx) else { return };
        let place = mut_arg_place(arg);
        let span = expr.span;

        let orig_ty = expr.ty.clone();
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
        let writeback = match place {
            ArgPlace::Var(v) => Some(IrStmt {
                kind: IrStmtKind::Assign { var: v, value: buf_read(mut_ty.clone()) },
                span,
            }),
            ArgPlace::Field(obj, field) => Some(IrStmt {
                kind: IrStmtKind::FieldAssign { target: obj, field, value: buf_read(mut_ty.clone()) },
                span,
            }),
            ArgPlace::None => None,
        };

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
        if let Some(wb) = writeback {
            stmts.push(wb);
        }
        *expr = IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
            ty: if was_unit { Ty::Unit } else { orig_ty },
            span,
            def_id: None,
        };
    }
}

impl CallSiteRewriter<'_> {
    /// Is `expr` Unwrap/Try over a JUST-REWRITTEN move-mode Block whose first stmt
    /// binds a was-Unit mut-fn call (the `h(a)!` shape, #1207)? Structurally
    /// unambiguous: after the bottom-up walk, a USER-written `Bind` of a mut-fn
    /// call has its Call already replaced by a Block, so a bare `Bind{value: Call}`
    /// to a collected name can only be the rewriter's own bind.
    fn wrapper_rotation_applies(&self, expr: &IrExpr) -> bool {
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) = &expr.kind
        else {
            return false;
        };
        let IrExprKind::Block { stmts, expr: tail } = &inner.kind else { return false };
        let Some(IrStmt { kind: IrStmtKind::Bind { value, .. }, .. }) = stmts.first() else {
            return false;
        };
        let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &value.kind else {
            return false;
        };
        matches!(
            self.mut_fns
                .get(name.as_str())
                .or_else(|| self.mut_fns.get(&scope_key(&self.scope, name.as_str()))),
            Some(&(_, _, true))
        ) && matches!(tail.as_deref().map(|t| &t.kind), Some(IrExprKind::Unit))
    }

    /// Perform the rotation [`Self::wrapper_rotation_applies`] admitted. The
    /// wrapper's err-propagation moves INTO the bind (`let __mp_buf = call!`),
    /// so on the err path the writeback never runs — matching the native
    /// by-reference semantics (a failed callee's caller-visible buffer is
    /// whatever the callee left in it; here the callee never returns a buffer
    /// on err, and the caller's binding keeps its pre-call value).
    fn rotate_wrapper_into_block(expr: &mut IrExpr) {
        let span = expr.span;
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
            let IrStmtKind::Bind { value, ty, .. } = &mut stmts[0].kind else {
                unreachable!("wrapper_rotation_applies checked the bind");
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
            *value = IrExpr { kind: wrapped_kind, ty: ty.clone(), span, def_id: None };
        }
        expr.kind = IrExprKind::Block { stmts, expr: tail };
        expr.ty = Ty::Unit;
    }
}
