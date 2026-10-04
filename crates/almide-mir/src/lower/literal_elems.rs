//! A list or map literal with a heap `??` element binds the element first (#2739 B2).
//!
//! The literal builders materialize an element that is a variable or a
//! literal; a heap `??` (`o ?? "none"` over an `Option[String]`) in element
//! position is neither, so the literal walls wherever it stands — a call
//! argument, a `let`, a tail, a match arm. The same element bound by a `let`
//! first lowers. [`bind_heap_unwrap_or_literal_elems`] makes that rewrite:
//!
//! ```text
//! ["X-Echo": o ?? "none", "K": "v"]  ≡  { let e = o ?? "none"; ["X-Echo": e, "K": "v"] }
//! ```
//!
//! Only when every other element (and every map key) is a variable or a
//! literal, so no element with effects is reordered past another; the `??`
//! elements keep their order. No call is added or removed (the caps
//! `mir == ir` count is unchanged). The block it leaves in a call argument is
//! absorbed by `hoist_block_call_args`, which runs after it.

use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarTable};
use almide_lang::intern::sym;

/// A variable or a literal: evaluating it has no effect and reads no state a
/// hoisted element could change.
fn is_plain(e: &IrExpr) -> bool {
    matches!(
        e.kind,
        IrExprKind::Var { .. }
            | IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitStr { .. }
    )
}

/// A `??` producing a heap value.
fn is_heap_unwrap_or(e: &IrExpr) -> bool {
    matches!(e.kind, IrExprKind::UnwrapOr { .. }) && crate::lower::is_heap_ty(&e.ty)
}

/// The element slots of a list or map literal, in evaluation order.
fn elements_mut(e: &mut IrExpr) -> Option<Vec<&mut IrExpr>> {
    match &mut e.kind {
        IrExprKind::List { elements } => Some(elements.iter_mut().collect()),
        IrExprKind::MapLiteral { entries } => Some(entries.iter_mut().flat_map(|(k, v)| [k, v]).collect()),
        _ => None,
    }
}

/// Bind each heap `??` element of the literal `e` to a fresh `let`, returning
/// the binds in order; empty when the literal does not qualify.
fn bind_elements(e: &mut IrExpr, vt: &mut VarTable) -> Vec<IrStmt> {
    let Some(elems) = elements_mut(e) else { return Vec::new() };
    if !elems.iter().any(|x| is_heap_unwrap_or(x)) || !elems.iter().all(|x| is_plain(x) || is_heap_unwrap_or(x)) {
        return Vec::new();
    }
    let mut binds = Vec::new();
    for el in elems.into_iter().filter(|x| is_heap_unwrap_or(x)) {
        let ty = el.ty.clone();
        let var = vt.alloc(sym("__elem"), ty.clone(), Mutability::Let, None);
        let read = IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: el.span, def_id: None };
        let value = std::mem::replace(el, read);
        let span = value.span;
        binds.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
    }
    binds
}

struct Walk<'a> {
    vt: &'a mut VarTable,
}

impl IrMutVisitor for Walk<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let stmts = bind_elements(e, self.vt);
        if stmts.is_empty() {
            return;
        }
        let ty = e.ty.clone();
        let span = e.span;
        let literal = std::mem::replace(e, IrExpr { kind: IrExprKind::Unit, ty: ty.clone(), span, def_id: None });
        *e = IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(literal)) }, ty, span, def_id: None };
    }
}

/// The program pass (shared chain: the pipeline and the corpus classifier run
/// it at the same point, before `hoist_block_call_args`).
pub fn bind_heap_unwrap_or_literal_elems(program: &mut almide_ir::IrProgram) {
    let almide_ir::IrProgram { functions, modules, var_table, .. } = program;
    for func in functions.iter_mut().chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut())) {
        Walk { vt: &mut *var_table }.visit_expr_mut(&mut func.body);
    }
}
