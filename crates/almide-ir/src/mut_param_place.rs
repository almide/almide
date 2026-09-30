//! The caller-side place a C-132 write-back stores the mutated buffer into
//! (`mut_param`'s phase 2), at any depth (#3092).
//!
//! A `mut` argument is a binding followed by any chain of record fields and
//! tuple slots: `xs`, `b.items`, `o.inner.xs`, `t.0`, `r.t.0`. The one-level
//! shapes write back with the statement the backends already carry
//! (`xs = buf`, `b.items = buf`). A deeper place is read level by level into
//! fresh `var` temps, the innermost is written, and each level is stored back
//! into its holder, innermost first — the shape a nested assignment target
//! lowers to (#3066):
//!
//! ```text
//! o.inner.xs  →  var t1 = o.inner; t1.xs = buf; o.inner = t1
//! t.0         →  t = (buf, t.1)
//! ```
//!
//! Every level is read AFTER the call, so a write the callee made to another
//! part of the same value (a global's sibling field) is kept, as it is for a
//! one-level place. Before this, a deeper place wrote nothing back: the wasm
//! leg printed the pre-call value where native printed the mutation.

use crate::*;
use almide_base::intern::{sym, Sym};
use almide_lang::types::Ty;

/// One step below the root binding, with the type of the value it reaches.
#[derive(Clone)]
pub(crate) enum Step {
    Field(Sym, Ty),
    Slot(usize, Ty),
}

/// The caller-side slot the mutated buffer writes back into.
pub(crate) enum ArgPlace {
    /// A binding (with its type) and the steps below it, root-first.
    Path(VarId, Ty, Vec<Step>),
    /// No named place (a temp expression) — native mutates an unobservable
    /// temporary there as well, so skipping the writeback is equivalent.
    None,
}

/// The place `arg` names, or [`ArgPlace::None`] for a temporary.
pub(crate) fn mut_arg_place(arg: &IrExpr) -> ArgPlace {
    let mut steps = Vec::new();
    let mut cur = arg;
    loop {
        match &cur.kind {
            IrExprKind::Var { id } => {
                steps.reverse();
                return ArgPlace::Path(*id, cur.ty.clone(), steps);
            }
            IrExprKind::Member { object, field } => {
                steps.push(Step::Field(*field, cur.ty.clone()));
                cur = object;
            }
            IrExprKind::TupleIndex { object, index } => {
                steps.push(Step::Slot(*index, cur.ty.clone()));
                cur = object;
            }
            _ => return ArgPlace::None,
        }
    }
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

/// The statements that store `value` into `place`, or none for a temporary
/// (or a tuple step whose holder type is not a tuple — left unwritten, as
/// before, rather than guessed).
pub(crate) fn writeback_stmts(place: &ArgPlace, value: IrExpr, vt: &mut VarTable, span: Option<Span>) -> Vec<IrStmt> {
    let ArgPlace::Path(root, root_ty, steps) = place else { return Vec::new() };
    let Some((last, inner)) = steps.split_last() else {
        return vec![IrStmt { kind: IrStmtKind::Assign { var: *root, value }, span }];
    };
    // Read each holder below the root into a fresh `var`.
    let mut stmts = Vec::new();
    let mut holders: Vec<(VarId, Ty)> = vec![(*root, root_ty.clone())];
    for step in inner {
        let (h, h_ty) = holders.last().cloned().expect("root holder");
        let read = step_read(h, &h_ty, step);
        let ty = read.ty.clone();
        let tmp = vt.alloc(sym("__mp_place"), ty.clone(), Mutability::Var, None);
        stmts.push(IrStmt { kind: IrStmtKind::Bind { var: tmp, mutability: Mutability::Var, ty: ty.clone(), value: read }, span });
        holders.push((tmp, ty));
    }
    // Write the innermost, then store every level back, innermost first.
    let mut writes = vec![(holders.last().cloned().expect("holder"), last.clone(), value)];
    for (k, step) in inner.iter().enumerate().rev() {
        let (tmp, tmp_ty) = holders[k + 1].clone();
        writes.push((holders[k].clone(), step.clone(), node(IrExprKind::Var { id: tmp }, tmp_ty)));
    }
    for ((h, h_ty), step, v) in writes {
        match step_write(h, &h_ty, &step, v) {
            Some(kind) => stmts.push(IrStmt { kind, span }),
            None => return Vec::new(),
        }
    }
    stmts
}

/// `h.f` / `h.k`.
fn step_read(h: VarId, h_ty: &Ty, step: &Step) -> IrExpr {
    let object = Box::new(node(IrExprKind::Var { id: h }, h_ty.clone()));
    match step {
        Step::Field(field, ty) => node(IrExprKind::Member { object, field: *field }, ty.clone()),
        Step::Slot(index, ty) => node(IrExprKind::TupleIndex { object, index: *index }, ty.clone()),
    }
}

/// `h.f = v`, or `h = (h.0, .., v, ..)` for a tuple slot.
fn step_write(h: VarId, h_ty: &Ty, step: &Step, v: IrExpr) -> Option<IrStmtKind> {
    match step {
        Step::Field(field, _) => Some(IrStmtKind::FieldAssign { target: h, field: *field, value: v }),
        Step::Slot(index, _) => {
            let Ty::Tuple(elems) = h_ty else { return None };
            let mut v = Some(v);
            let elements = elems
                .iter()
                .enumerate()
                .map(|(i, t)| match i == *index {
                    true => v.take().expect("one slot"),
                    false => step_read(h, h_ty, &Step::Slot(i, t.clone())),
                })
                .collect();
            Some(IrStmtKind::Assign { var: h, value: node(IrExprKind::Tuple { elements }, h_ty.clone()) })
        }
    }
}
