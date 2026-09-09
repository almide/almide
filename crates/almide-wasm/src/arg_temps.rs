//! #2004 — the argument-temporary class, LANGUAGE-CONSTRUCT half: a
//! droppable value produced by a call or born in the expression (a
//! literal list, an interpolation, an inner concat) and consumed directly
//! by a binary op, a `for … in`, a `match`, an index, or an interpolation
//! (`xs + [4]`, `a + b + c`, `for c in string.chars(s)`) had no owner.
//! The consumer reads the block and never spends its credit, so every
//! such temporary stayed at rc 1 forever.
//!
//! The fix is at the IR, before lowering: every such operand is BOUND
//! first — `op(f(x))` becomes `{ let t = f(x); op(t) }` — so the Bind
//! route owns the temporary (its one credit, #1986) and the frame's exit
//! plan releases it exactly as it releases any other local.
//!
//! The MODULE-OP half is not here: an argument of a native arm is lowered
//! under the mode the arm DECLARES at the site (`lower_arg`, arm.rs
//! `ArgMode::Borrow | Retain`), and the wrapper releases what a Borrow
//! left behind. There is no list of "reader ops" anywhere — the gate
//! scripts/check-arm-args.sh refuses an undeclared argument.
//!
//! Hoisting keeps the operands' relative order (binds in operand order,
//! then the consumer). A hoisted operand is a non-effect call — an effect
//! call arrives wrapped in `Try`/`Unwrap` and is left alone — so the
//! evaluation order change against the non-hoisted operands (Vars,
//! literals, reads) is unobservable.
//! Extraction (`Try`, `Unwrap`, `UnwrapOr`) binds its single input at the
//! extraction site as well. The wrapper then survives payload reads and
//! joins the normal frame releases, including the propagation edge.

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
        let mut v = Binder { vars: &mut out.var_table, changed: &mut changed, tail: false };
        for f in out.functions.iter_mut() {
            v.visit_with_tail(&mut f.body, true);
        }
        for tl in out.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for m in out.modules.iter_mut() {
        let mut v = Binder { vars: &mut m.var_table, changed: &mut changed, tail: false };
        for f in m.functions.iter_mut() {
            v.visit_with_tail(&mut f.body, true);
        }
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    changed.then_some(out)
}

/// The RC-droppable shapes (rc_ownership.rs `rc_droppable`, by Ty): Str,
/// Bytes, and the flat-payload blocks — a List / tuple / Option / Result
/// whose slots are all non-Str scalars (#2010 stage 1). Records and
/// variants need the type table and are bound by the emitter's own
/// routes.
fn droppable_ty(t: &Ty) -> bool {
    match t {
        Ty::String | Ty::Bytes => true,
        // Any List (stage 2a: the spine is released; elements are 2b).
        Ty::Applied(TypeConstructorId::List, _) => true,
        // Stage 2c: any Option / Result / tuple block (rc_droppable).
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, _) => true,
        Ty::Tuple(_) => true,
        // Stage 2c-ii: records and variants (an Excluded name binds a plain local).
        Ty::Applied(TypeConstructorId::UserDefined(_), _) => true,
        // Map stage a: the entries array is a credit.
        Ty::Applied(TypeConstructorId::Map | TypeConstructorId::Set, _) => true,
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
    tail: bool,
}

impl Binder<'_> {
    fn visit_with_tail(&mut self, e: &mut IrExpr, tail: bool) {
        let saved = self.tail;
        self.tail = tail;
        self.visit_expr_mut(e);
        self.tail = saved;
    }

    fn walk_with_tail(&mut self, e: &mut IrExpr, tail: bool) {
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => {
                for stmt in stmts { self.visit_stmt_mut(stmt); }
                if let Some(expr) = expr { self.visit_with_tail(expr, tail); }
            }
            IrExprKind::If { cond, then, else_ } => {
                self.visit_expr_mut(cond);
                self.visit_with_tail(then, tail);
                self.visit_with_tail(else_, tail);
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr_mut(subject);
                for arm in arms {
                    self.visit_pattern_mut(&mut arm.pattern);
                    if let Some(guard) = &mut arm.guard { self.visit_expr_mut(guard); }
                    self.visit_with_tail(&mut arm.body, tail);
                }
            }
            _ => walk_expr_mut(self, e),
        }
    }
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
        let tail = self.tail;
        self.tail = false;
        self.walk_with_tail(e, tail);
        self.tail = tail;
        // Tail extraction has a dedicated carrier-transfer route. Naming
        // its operand would hide the call and disable constant-stack TCO.
        if tail && matches!(e.kind, IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }) {
            return;
        }
        let operands: Vec<&mut IrExpr> = match &mut e.kind {
            // A binary op over droppable operands — concatenation, or an
            // equality / ordering test on strings and lists — reads both
            // and consumes neither.
            IrExprKind::BinOp { left, right, .. } => vec![left.as_mut(), right.as_mut()],
            // The other readers of a droppable value: a loop over it, a
            // match on it, an index into it, an interpolation of it.
            IrExprKind::ForIn { iterable, .. } => vec![iterable.as_mut()],
            IrExprKind::Match { subject, .. } => vec![subject.as_mut()],
            // Extraction borrows the payload; keep the temporary wrapper
            // owned so both success and propagation release its credit.
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr }
            | IrExprKind::UnwrapOr { expr, .. }
                if matches!(expr.ty, Ty::Applied(TypeConstructorId::Result | TypeConstructorId::Option, _)) =>
            {
                // Move-mode effect calls can carry a raw payload type here;
                // their carrier is supplied by the ABI, not this annotation.
                vec![expr.as_mut()]
            }
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
