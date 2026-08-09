//! Continuation-lift for effect-`!` statements whose continuation carries a
//! loop (#1147).
//!
//! A statement-position effect `!` has no mid-function early return on the v1
//! MIR, so the shared desugar (`desugar_stmt_control_unwrap` /
//! `desugar_unwrap_b.rs`) pushes the WHOLE remaining block — loops included —
//! into the ok arm of the err-check region. A loop inside a branch arm is
//! unrepresentable in the flat v4 ownership certificate: `flush_branch`
//! poisons the object (`{i|}`), and the fn drops out of the kernel-checked
//! witness (`cert_poisoned_excluded`, #1146).
//!
//! The lift removes the shape at its source, in the branch_lift discipline
//! ("no new ownership-cert / Coq machinery — move the construct into a
//! position the proven lowering already handles"): when a top-level statement
//! of an effect fn's body contains an effect `!` and the statements AFTER it
//! contain a loop, the continuation is outlined into a synthesized effect fn
//! and the body's tail becomes a plain call to it. The err-check arm then
//! holds a flat `CallFn` (representable), and the loop certifies at the TOP
//! level of the synthesized fn — the proven `i(…)m` shape.
//!
//! Runs in the shared optimize phase, so every consumer (v0 codegen, the v1
//! renders, the interpreter, the caps gates) sees the same outlined tree —
//! the mir == ir call-count parity is preserved by construction.
//!
//! Conservative guards (skip the fn rather than mis-lift):
//! - only the fn-body TOP block is split (no enclosing loop, so the
//!   continuation cannot carry `break`/`continue` out of scope);
//! - the continuation must not ASSIGN to a variable bound before it (params
//!   are copies — a write would be lost);
//! - generic fns are skipped (the helper would need the type params);
//! - `!` inside lambda literals does not trigger (a lambda's `!` propagates
//!   within the lambda, creating no statement-level region here), and loops
//!   inside lambda literals do not count (they lower in the lambda's fn).

use std::collections::HashSet;
use almide_ir::free_vars::free_vars;
use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::*;
use almide_base::intern::sym;

/// Outline loop-bearing `!`-continuations in every effect fn. Newly
/// synthesized fns are processed too (a continuation may itself contain
/// another `!` statement followed by another loop).
pub fn lift_loop_continuations(program: &mut IrProgram) {
    let mut counter: u32 = 0;
    {
        let IrProgram { functions, top_lets, var_table, .. } = &mut *program;
        let globals: HashSet<VarId> = top_lets.iter().map(|tl| tl.var).collect();
        lift_in_fns(functions, var_table, &globals, &mut counter);
    }
    for module in program.modules.iter_mut() {
        let IrModule { functions, top_lets, var_table, .. } = &mut *module;
        let globals: HashSet<VarId> = top_lets.iter().map(|tl| tl.var).collect();
        lift_in_fns(functions, var_table, &globals, &mut counter);
    }
}

fn lift_in_fns(
    functions: &mut Vec<IrFunction>,
    vt: &mut VarTable,
    globals: &HashSet<VarId>,
    counter: &mut u32,
) {
    // Index-walk so fns synthesized during the loop are processed as well.
    let mut i = 0;
    while i < functions.len() {
        if functions[i].is_effect && functions[i].generics.is_none() {
            let (is_test, ret_ty) = (functions[i].is_test, functions[i].ret_ty.clone());
            let mut body = std::mem::take(&mut functions[i].body);
            let synthesized =
                lift_top_block(&mut body, &ret_ty, is_test, vt, globals, counter);
            functions[i].body = body;
            functions.extend(synthesized);
        }
        i += 1;
    }
}

/// Split the fn-body block at the FIRST `!`-statement whose continuation
/// carries a loop, returning the synthesized continuation fn (empty when the
/// shape is absent or a guard declines). The tail becomes the call.
fn lift_top_block(
    body: &mut IrExpr,
    ret_ty: &almide_lang::types::Ty,
    is_test: bool,
    vt: &mut VarTable,
    globals: &HashSet<VarId>,
    counter: &mut u32,
) -> Vec<IrFunction> {
    let IrExprKind::Block { stmts, expr: tail } = &mut body.kind else {
        return Vec::new();
    };
    let Some(k) = split_index(stmts, tail) else {
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

    let bound: HashSet<VarId> = HashSet::new();
    let params: Vec<VarId> = free_vars(&cont_body, &bound)
        .into_iter()
        .filter(|v| !globals.contains(v))
        .collect();

    let id = *counter;
    *counter = id + 1;
    // Plain (non-`__`) name for the same reason as `branch_lift_synth_*`: a
    // `__` prefix would be rewritten to a runtime-intrinsic call by codegen's
    // builtin lowering.
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

    let call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Named { name: func_name },
            args,
            type_args: vec![],
        },
        ty: ret_ty.clone(),
        span,
        def_id: None,
    };
    // Spell the propagation with an explicit `!` in EVERY enclosing kind —
    // the ADR-0008 canonical tail shape the proven machinery handles. A TEST
    // fn's emitted tail expects Unit (the harness unwraps inside the body);
    // a plain effect fn re-wraps the unwrapped value into its own carrier
    // (`Ok(call?)` ≡ `call` semantically). A BARE tail call was NOT
    // equivalent on the v1 wasm render: the callee's carrier block stayed on
    // the stack ("values remaining on stack at end of block" — invalid
    // module) because the Unit-typed call node hid the carrier from the
    // return plumbing.
    let _ = is_test;
    let tail_expr = IrExpr {
        kind: IrExprKind::Unwrap { expr: Box::new(call) },
        ty: ret_ty.clone(),
        span,
        def_id: None,
    };
    *tail = Some(Box::new(tail_expr));

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
/// AND the continuation (`stmts[k+1..]` + tail) is non-empty, carries a loop,
/// and passes the lost-write guard.
fn split_index(stmts: &[IrStmt], tail: &Option<Box<IrExpr>>) -> Option<usize> {
    for k in 0..stmts.len() {
        if !stmt_has_unwrap(&stmts[k]) {
            continue;
        }
        let after = &stmts[k + 1..];
        if after.is_empty() && tail.is_none() {
            continue;
        }
        if !continuation_has_loop(after, tail) {
            continue;
        }
        if continuation_writes_outer(after) {
            return None; // conservative: a later split point would drop the writes too
        }
        return Some(k);
    }
    None
}

fn stmt_has_unwrap(stmt: &IrStmt) -> bool {
    struct Find {
        found: bool,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found || matches!(e.kind, IrExprKind::Lambda { .. }) {
                return;
            }
            if matches!(e.kind, IrExprKind::Unwrap { .. }) {
                self.found = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { found: false };
    f.visit_stmt(stmt);
    f.found
}

fn continuation_has_loop(stmts: &[IrStmt], tail: &Option<Box<IrExpr>>) -> bool {
    struct Find {
        found: bool,
    }
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found || matches!(e.kind, IrExprKind::Lambda { .. }) {
                return;
            }
            if matches!(e.kind, IrExprKind::ForIn { .. } | IrExprKind::While { .. }) {
                self.found = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    let mut f = Find { found: false };
    for s in stmts {
        f.visit_stmt(s);
    }
    if let Some(t) = tail {
        f.visit_expr(t);
    }
    f.found
}

/// Does the continuation write (assign / in-place mutate) a var it does not
/// itself bind? Such a var would become a by-value param and the write lost.
fn continuation_writes_outer(stmts: &[IrStmt]) -> bool {
    struct Scan {
        bound: HashSet<VarId>,
        writes: Vec<VarId>,
    }
    impl IrVisitor for Scan {
        fn visit_stmt(&mut self, s: &IrStmt) {
            match &s.kind {
                IrStmtKind::Bind { var, .. } => {
                    self.bound.insert(*var);
                }
                IrStmtKind::Assign { var, .. } => self.writes.push(*var),
                IrStmtKind::IndexAssign { target, .. }
                | IrStmtKind::MapInsert { target, .. }
                | IrStmtKind::FieldAssign { target, .. } => self.writes.push(*target),
                _ => {}
            }
            almide_ir::visit::walk_stmt(self, s);
        }
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::ForIn { var, var_tuple, .. } = &e.kind {
                self.bound.insert(*var);
                if let Some(vs) = var_tuple {
                    for v in vs {
                        self.bound.insert(*v);
                    }
                }
            }
            walk_expr(self, e);
        }
    }
    let mut s = Scan { bound: HashSet::new(), writes: Vec::new() };
    for st in stmts {
        s.visit_stmt(st);
    }
    s.writes.iter().any(|w| !s.bound.contains(w))
}
