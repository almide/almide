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
    // Collect functions with mutated params: name → (param_indices, param_types)
    // name → (mut param index, its type, callee returned Unit before the
    // rewrite). Non-Unit EFFECT fns are excluded (Result-wrap interplay).
    // Call sites are keyed by BARE name, so a name that resolves to more
    // than one function (same-name fns across modules, the #692 class)
    // must be excluded wholesale: rewriting the callee but not a caller —
    // or a caller of the OTHER same-name fn — leaves an invalid module
    // (the pass previously indexed mutated_params[0] on the same-name
    // NON-mut sibling and panicked).
    let mut name_count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for func in program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()))
    {
        *name_count.entry(func.name.as_str()).or_insert(0) += 1;
    }
    let mut mut_fns: MutFns = std::collections::HashMap::new();
    let collect = |func: &IrFunction, mut_fns: &mut MutFns| {
        if func.mutated_params.len() != 1 {
            return;
        }
        if name_count.get(func.name.as_str()).copied().unwrap_or(0) != 1 {
            return;
        }
        let idx = func.mutated_params[0];
        let Some(p) = func.params.get(idx) else { return };
        let was_unit = matches!(func.ret_ty, Ty::Unit);
        if !was_unit && func.is_effect {
            return;
        }
        mut_fns.insert(func.name.to_string(), (idx, p.ty.clone(), was_unit));
    };
    for func in &program.functions {
        collect(func, &mut mut_fns);
    }
    if std::env::var("ALMIDE_MP_PROBE").is_ok() {
        for (k, v) in &mut_fns {
            eprintln!("[mp] fn {} → {:?}", k, v);
        }
    }
    for module in &program.modules {
        for func in &module.functions {
            collect(func, &mut mut_fns);
        }
    }

    if mut_fns.is_empty() {
        return false;
    }

    // Phase 1: Rewrite function bodies. Unit-returning fns return the
    // mutated param; value-returning fns return (orig, mutated) as a tuple
    // (#705 — previously the non-Unit case was silently skipped, so the
    // caller's List never saw a reallocating push: `len=1` on wasm vs
    // `len=3` native, and mlp's loss printed 0.0).
    let vt = &mut program.var_table;
    for func in program
        .functions
        .iter_mut()
        .chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        let Some(&(entry_idx, _, was_unit)) = mut_fns.get(func.name.as_str()) else { continue };
        // Name-keyed entry — confirm THIS func is the one that was
        // collected (unique-name invariant above makes this a plain
        // assertion, but stay defensive).
        let Some(&mut_idx) = func.mutated_params.first() else { continue };
        if mut_idx != entry_idx {
            continue;
        }
        let mut_var = func.params[mut_idx].var;
        let mut_ty = func.params[mut_idx].ty.clone();
        let var_expr = |ty: Ty| IrExpr {
            kind: IrExprKind::Var { id: mut_var },
            ty,
            span: None,
            def_id: None,
        };
        if was_unit {
            func.ret_ty = mut_ty.clone();
            // Wrap existing body in a block with the param as tail.
            let old_body = std::mem::replace(
                &mut func.body,
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            func.body = IrExpr {
                kind: IrExprKind::Block {
                    stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: old_body }, span: None }],
                    expr: Some(Box::new(var_expr(mut_ty))),
                },
                ty: func.ret_ty.clone(),
                span: None,
                def_id: None,
            };
        } else {
            // { let __mp_ret: T = <old body>; (__mp_ret, mut_param) } — the
            // body runs first (its mutations land in the param local), then
            // the tuple pairs the original result with the final buffer.
            let orig_ty = func.ret_ty.clone();
            let tuple_ty = Ty::Tuple(vec![orig_ty.clone(), mut_ty.clone()]);
            func.ret_ty = tuple_ty.clone();
            let ret_var = vt.alloc(sym("__mp_ret"), orig_ty.clone(), Mutability::Let, None);
            let old_body = std::mem::replace(
                &mut func.body,
                IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
            );
            let ret_read = IrExpr {
                kind: IrExprKind::Var { id: ret_var },
                ty: orig_ty.clone(),
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
                    expr: Some(Box::new(IrExpr {
                        kind: IrExprKind::Tuple { elements: vec![ret_read, var_expr(mut_ty)] },
                        ty: tuple_ty.clone(),
                        span: None,
                        def_id: None,
                    })),
                },
                ty: tuple_ty,
                span: None,
                def_id: None,
            };
        }
        // The convention is now explicit in the tree; the field would
        // otherwise keep tripping mut-param gates (the v1 C-132 wall).
        func.mutated_params.clear();
    }

    // Phase 2: Rewrite call sites — write the mutated buffer back. A
    // bottom-up IrMutVisitor rewrites EVERY position uniformly (statement,
    // Bind/Assign RHS, nested expression, loop bodies): the callee's
    // signature changed globally, so an unrewritten site is not merely
    // un-written-back — it is an invalid module (i32 tuple vs the old
    // scalar). The call becomes a Block expression:
    //
    //   { let (__mp_res, __mp_buf) = <call>; <writeback>; __mp_res }
    //
    // and the writeback targets the argument PLACE: a bare var assigns it,
    // a `b.items` field FieldAssigns it, and a temp (no named place) skips
    // the writeback — native mutates an invisible temp there too.
    let vt = &mut program.var_table;
    let mut rw = CallSiteRewriter { mut_fns: &mut_fns, vt };
    for func in program
        .functions
        .iter_mut()
        .chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()))
    {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut program.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for m in &mut program.modules {
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
    }

    true
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
}

impl IrMutVisitor for CallSiteRewriter<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Bottom-up: children first, so a mut-call argument nested inside
        // another mut-call is already rewritten when the outer one wraps.
        walk_expr_mut(self, expr);

        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind else {
            return;
        };
        let Some((idx, mut_ty, was_unit)) = self.mut_fns.get(name.as_str()).cloned() else {
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
