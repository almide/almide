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

/// A module op that only READS its droppable arguments — binding an
/// argument to a reader is always sound (the frame owns the temporary,
/// the arm borrows a Var). A RETAINER — a container insert, a push, a
/// constructor that keeps the block — adopts a fresh temporary as-is
/// today, and would need a share for a Var it did not get: `set.insert
/// (h, "s" + …)` with the key bound freed the key under the set
/// (map_set_index_threshold printed stale keys). Retainers stay on the
/// old path: their arguments are not bound here. Koka / Lean carry this
/// as a per-primitive borrow summary; this is that summary, by module
/// for the copy-semantics modules and by name for `list`.
fn reader_op(module: &str, func: &str) -> bool {
    match module {
        // Strings, bytes and scalars copy what they read; nothing they
        // return holds an argument block.
        "string" | "bytes" | "int" | "float" | "math" | "base64" | "url" | "datetime" | "int8"
        | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32" | "uint64" | "float32"
        | "float64" => true,
        // List readers over scalar elements (a droppable list argument
        // is a List of scalars — no element block can be retained).
        "list" => matches!(
            func,
            "len" | "length" | "sum" | "product" | "contains" | "join" | "min" | "max"
                | "is_empty" | "index_of" | "count" | "all" | "any" | "reverse" | "take"
                | "drop" | "slice" | "sort" | "map" | "filter" | "first" | "last" | "get"
                | "get_or" | "head" | "tail" | "zip" | "enumerate" | "fold" | "reduce"
        ),
        _ => false,
    }
}

/// The value an expression evaluates to, through block wrappers.
fn tail_of(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Block { expr: Some(t), .. } => tail_of(t),
        _ => e,
    }
}

struct Binder<'a> {
    vars: &'a mut VarTable,
    changed: &'a mut bool,
}

/// A droppable operand of a concatenation that is born in the
/// expression itself — a literal list, an interpolation, an inner
/// concat — and, like a call result, has no owner after the op reads it
/// (`xs + [4]` leaked the `[4]`; `a + b + c` leaked the inner `a + b`).
fn is_born_here(e: &IrExpr) -> bool {
    matches!(
        &e.kind,
        IrExprKind::List { .. } | IrExprKind::StringInterp { .. } | IrExprKind::BinOp { .. }
    )
}

impl IrMutVisitor for Binder<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let operands: Vec<&mut IrExpr> = match &mut e.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                if !reader_op(module.as_str(), func.as_str()) {
                    return;
                }
                args.iter_mut().collect()
            }
            // A binary op over droppable operands — concatenation, or an
            // equality / ordering test on strings and lists — reads both
            // and consumes neither.
            IrExprKind::BinOp { left, right, .. } => vec![left.as_mut(), right.as_mut()],
            // The other readers of a droppable value: a loop over it, a
            // match on it, an index into it, an interpolation of it.
            IrExprKind::ForIn { iterable, .. } => vec![iterable.as_mut()],
            IrExprKind::Match { subject, .. } => vec![subject.as_mut()],
            IrExprKind::IndexAccess { object, .. } => vec![object.as_mut()],
            IrExprKind::StringInterp { parts } => parts
                .iter_mut()
                .filter_map(|p| match p {
                    almide_ir::IrStringPart::Expr { expr } => Some(expr),
                    almide_ir::IrStringPart::Lit { .. } => None,
                })
                .collect(),
            _ => return,
        };
        let mut binds: Vec<IrStmt> = Vec::new();
        for a in operands {
            // Children were rewritten first: an operand may already be a
            // `{ let …; value }` block — its value is the block's tail.
            let core = tail_of(a);
            if !(droppable_ty(&a.ty) && (is_produced_by_call(core) || is_born_here(core))) {
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
