//! #2004 — the argument-temporary class: a droppable value PRODUCED BY A
//! CALL and consumed directly as an argument of a module op
//! (`string.len(int.to_string(i))`) had no owner. The table path is
//! covered by the callee-owned convention (an owned argument moves into
//! the callee, `rc_arg_guard`); the NATIVE arms — `string.len`,
//! `list.len`, the contains / index / sum family, the in-place map and
//! list mutators — read or store the block and never spend its credit,
//! so every such temporary stayed at rc 1 forever (16 B per call in the
//! credit probe; 80 B for a two-concat line).
//!
//! The fix is at the IR, before lowering: every such argument is BOUND
//! first — `op(f(x))` becomes `{ let t = f(x); op(t) }` — so the Bind
//! route owns the temporary (its one credit, #1986) and the frame's exit
//! plan releases it exactly as it releases any other local. The native
//! arm then sees a plain Var, the argument shape every arm already
//! handles (a retaining arm shares it; a reading arm borrows it).
//!
//! Hoisting keeps the arguments' relative order (binds in argument order,
//! then the call). A hoisted argument is a non-effect call — an effect
//! call arrives wrapped in `Try`/`Unwrap` and is left alone — so the
//! evaluation order change against the non-hoisted arguments (Vars,
//! literals, reads) is unobservable.

use almide_base::intern::sym;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, Mutability, VarTable};
use almide_types::types::constructor::TypeConstructorId;
use almide_types::types::Ty;

/// Rewrite the program's function bodies and initializers; `None` when
/// nothing needed binding (the emitter then reads the original tree).
pub(crate) fn bind_native_temporaries(ir: &IrProgram) -> Option<IrProgram> {
    let mut out = ir.clone();
    let mut changed = false;
    {
        let mut v = Binder { vars: &mut out.var_table, changed: &mut changed };
        for f in out.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in out.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for m in out.modules.iter_mut() {
        let mut v = Binder { vars: &mut m.var_table, changed: &mut changed };
        for f in m.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    changed.then_some(out)
}

/// The RC-droppable shapes (rc_ownership.rs `rc_droppable`, by Ty): Str,
/// Bytes, and a List whose elements are non-Str scalars.
fn droppable_ty(t: &Ty) -> bool {
    match t {
        Ty::String | Ty::Bytes => true,
        Ty::Applied(TypeConstructorId::List, args) => matches!(
            args.first(),
            Some(
                Ty::Int
                    | Ty::Float
                    | Ty::Bool
                    | Ty::Int8
                    | Ty::Int16
                    | Ty::Int32
                    | Ty::Int64
                    | Ty::UInt8
                    | Ty::UInt16
                    | Ty::UInt32
                    | Ty::UInt64
                    | Ty::Float32
                    | Ty::Float64
            )
        ),
        _ => false,
    }
}

/// A call that produces its value: a Named user fn, or a module op
/// (linked or native — either way a block nobody else owns).
fn is_produced_by_call(e: &IrExpr) -> bool {
    matches!(
        &e.kind,
        IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Module { .. }, .. }
    )
}

struct Binder<'a> {
    vars: &'a mut VarTable,
    changed: &'a mut bool,
}

impl IrMutVisitor for Binder<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::Call { target: CallTarget::Module { module, .. }, args, .. } = &mut e.kind
        else {
            return;
        };
        if module.as_str() == "prim" {
            return;
        }
        let mut binds: Vec<IrStmt> = Vec::new();
        for a in args.iter_mut() {
            if !(droppable_ty(&a.ty) && is_produced_by_call(a)) {
                continue;
            }
            let ty = a.ty.clone();
            let span = a.span;
            let id = self.vars.alloc(sym("__arg_tmp"), ty.clone(), Mutability::Let, span);
            let value = std::mem::replace(
                a,
                IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None },
            );
            binds.push(IrStmt {
                kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value },
                span,
            });
        }
        if binds.is_empty() {
            return;
        }
        *self.changed = true;
        let ty = e.ty.clone();
        let span = e.span;
        let call = std::mem::take(e);
        *e = IrExpr {
            kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(call)) },
            ty,
            span,
            def_id: None,
        };
    }
}
