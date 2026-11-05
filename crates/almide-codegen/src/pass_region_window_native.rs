//! The region window on the CERTIFIED native render (#4006 C2).
//!
//! The standard codegen runs a discovered `consume(produce(scalars))` window
//! in its arena (`pass_region_window.rs`): the pair's callee closure is cloned
//! as `__rgn_` twins over `Copy` twin enums whose recursive fields are arena
//! handles, and the site becomes `almide_region_window(|| …)`. The v1 render
//! lowers IR to MIR instead of rendering it, so it takes the same recogniser
//! and the same twin synthesis, and spells the site as a call to a fn of its
//! own — `__rgn_window_<k>(scalars) = __rgn_consume(.., __rgn_produce(..), ..)`,
//! scalar params, scalar result, marked [`REGION_WINDOW_ATTR`]. The MIR of a
//! marked fn is bracketed by the arena mark's save and restore
//! (`almide_mir::pipeline_native_render`), so the window is an object of the
//! ownership certificate: acquired once at entry, released once at the end.
//!
//! The call site evaluates the window's scalar arguments OUTSIDE the window,
//! in source order, and passes them in: nothing the caller evaluates is
//! allocated in the arena.

use std::collections::HashSet;

use almide_base::intern::{sym, Sym};
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::Ty;

use super::pass::{NanoPass, Target};
use super::pass_region_window::{build_cx, enum_name, find_sites, is_scalar_ty, plan_twins, twin_name, TwinPlan};
use super::pass_region_window_clone::synthesize_twins;

/// Rewrite every discovered window of `program` onto `__rgn_` twins and a
/// marked window fn, for the certified native render. Returns whether any
/// window was rewritten. `ALMIDE_REGION_OFF=1` disables it, as on the
/// standard codegen.
///
/// The recogniser reads the post-`ResolveCalls` call spelling, so it runs on
/// a resolved copy; the plan (the admitted pairs, the twin enums and the
/// callee closure, all by name) is then applied to `program` itself.
pub fn native_region_windows(program: &mut IrProgram) -> bool {
    if almide_base::env::flag("ALMIDE_REGION_OFF") {
        return false;
    }
    let resolved = super::pass_resolve_calls::ResolveCallsPass.run(program.clone(), Target::Rust).program;
    let cx = build_cx(&resolved);
    if cx.pure.is_empty() {
        return false;
    }
    let sites = find_sites(&resolved, &cx);
    if sites.is_empty() {
        return false;
    }
    let Some(plan) = plan_twins(&resolved, &cx, sites) else { return false };
    let admitted: HashSet<(Sym, Sym)> = plan.sites.iter().map(|s| (s.consume, s.produce)).collect();
    synthesize_twins(program, &TwinPlan { sites: Vec::new(), enums: plan.enums, fns: plan.fns });
    let mut rw = WindowRewriter { admitted: &admitted, vt: &mut program.var_table, windows: Vec::new() };
    for f in &mut program.functions {
        rw.visit_expr_mut(&mut f.body);
    }
    let windows = std::mem::take(&mut rw.windows);
    let changed = !windows.is_empty();
    program.functions.extend(windows);
    changed
}

/// Replace each admitted site by a call to a fresh window fn.
struct WindowRewriter<'a> {
    admitted: &'a HashSet<(Sym, Sym)>,
    vt: &'a mut VarTable,
    windows: Vec<IrFunction>,
}

impl IrMutVisitor for WindowRewriter<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if let Some(call) = self.window_call(e) {
            *e = call;
        }
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
    }
}

impl WindowRewriter<'_> {
    /// The call to a new window fn standing for the admitted site `e`, or
    /// `None` when `e` is not one.
    fn window_call(&mut self, e: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Call { target: CallTarget::Named { name: consume }, args, type_args } = &e.kind else {
            return None;
        };
        let heap = args.iter().position(|a| !is_scalar_ty(&a.ty))?;
        let IrExprKind::Call { target: CallTarget::Named { name: produce }, args: fargs, type_args: ftargs } = &args[heap].kind
        else {
            return None;
        };
        if !self.admitted.contains(&(*consume, *produce)) || !is_scalar_ty(&e.ty) {
            return None;
        }
        // The caller's scalar arguments, in source order: consume's before the
        // heap one, produce's own, then consume's after it.
        let mut outer: Vec<IrExpr> = Vec::new();
        let mut params: Vec<IrParam> = Vec::new();
        let mut param = |a: &IrExpr, vt: &mut VarTable| -> IrExpr {
            let name = sym(&format!("__rgn_a{}", params.len()));
            let var = vt.alloc(name, a.ty.clone(), Mutability::Let, a.span);
            params.push(IrParam {
                var,
                ty: a.ty.clone(),
                name,
                borrow: ParamBorrow::Own,
                is_mut: false,
                open_record: None,
                default: None,
                attrs: Vec::new(),
            });
            outer.push(a.clone());
            IrExpr { kind: IrExprKind::Var { id: var }, ty: a.ty.clone(), span: a.span, def_id: None }
        };
        let mut inner: Vec<IrExpr> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            if i != heap {
                inner.push(param(a, self.vt));
                continue;
            }
            let fa: Vec<IrExpr> = fargs.iter().map(|x| param(x, self.vt)).collect();
            let twin_ty = Ty::Named(twin_name(enum_name(&a.ty)?), Default::default());
            inner.push(IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: twin_name(*produce) },
                    args: fa,
                    type_args: ftargs.clone(),
                },
                ty: twin_ty,
                span: a.span,
                def_id: None,
            });
        }
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: twin_name(*consume) },
                args: inner,
                type_args: type_args.clone(),
            },
            ty: e.ty.clone(),
            span: e.span,
            def_id: None,
        };
        let name = sym(&format!("__rgn_window_{}", self.windows.len()));
        self.windows.push(IrFunction {
            name,
            params,
            ret_ty: e.ty.clone(),
            body,
            is_effect: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![IrFunction::scoped_marker(REGION_WINDOW_ATTR)],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 1,
            def_id: None,
            mutated_params: Vec::new(),
            module_origin: None,
        });
        Some(IrExpr {
            kind: IrExprKind::Call { target: CallTarget::Named { name }, args: outer, type_args: Vec::new() },
            ty: e.ty.clone(),
            span: e.span,
            def_id: None,
        })
    }
}
