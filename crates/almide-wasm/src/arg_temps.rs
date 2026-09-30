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
pub(crate) fn droppable_ty(t: &Ty) -> bool {
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

/// The operand types the Binder names: the droppable shapes, and a user
/// type by name (a record or a variant, `Ty::Named`) — whether ITS block is
/// released is the Bind route's call (`rc_droppable` over the type table);
/// a name whose shape carries no block binds a plain local (#2972: `T("x") ==
/// T("y")` over a variant with a String payload left both operands live).
pub(crate) fn bindable_ty(t: &Ty) -> bool {
    droppable_ty(t) || matches!(t, Ty::Named(..) | Ty::Record { .. } | Ty::Variant { .. })
}

/// A call that produces its value: a Named user fn, or a module op
/// (linked or native — either way a block nobody else owns). A closure call
/// (`Computed`) is owned the same way (rc_ownership.rs: the lifted body's
/// epilogue hands the caller one credit) and counts for the plain readers
/// (#2970: `match f(x) { … }` inside the fallible-HOF carriers left every
/// element's Result block unowned); an EXTRACTION already releases an owned
/// closure-call carrier at the `!` itself (data.rs `release_ok_carrier`), so
/// `extraction` keeps that immediate release instead of parking it. A `??`
/// releases nothing (#2969: `g("41") ?? "err"` over an effect fn value left
/// the carrier live), so only `!` / `?` pass `true`.
fn is_produced_by_call(e: &IrExpr, extraction: bool) -> bool {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Named { .. } | CallTarget::Module { .. }, .. } => true,
        IrExprKind::Call { target: CallTarget::Computed { .. }, .. } => !extraction,
        // `m[k]` lowers as the `map.get` call (emitter.rs `lower_data`).
        IrExprKind::MapAccess { .. } => true,
        _ => false,
    }
}

/// A literal operand: evaluating it has no effect, so it may stay in place
/// while the operands around it are named.
fn is_literal(e: &IrExpr) -> bool {
    matches!(
        e.kind,
        IrExprKind::LitInt { .. }
            | IrExprKind::LitFloat { .. }
            | IrExprKind::LitBool { .. }
            | IrExprKind::LitStr { .. }
            | IrExprKind::Unit
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

    /// #2515 — `let (a, b) = <subject>` reads each bound position out of the
    /// subject as a VIEW (no `+1`, patterns.rs) and never releases the
    /// subject: correct for a subject someone else holds, a leak of the
    /// whole block and everything in it for one this frame owns (160 B per
    /// `let (b, k) = mk(i)` with a 64 B buffer inside). The subject is named
    /// first — `let t = <subject>; let (a, b) = t` — so the answer to "does
    /// this frame own it" is the Bind route's (`rc_owned_result`: an owned
    /// result is taken as is, a borrowed one takes `+1`) and the frame's
    /// exit plan releases `t` like any other local. The binds stay views of
    /// `t`'s fields, which live until `t` goes: the rewritten program is the
    /// one a user writes by hand, and that shape already balances.
    ///
    /// The binds are NOT made owners (the other way to close the leak): a
    /// pattern bind that owns its slot freed a returned value under its
    /// reader once already (patterns.rs, the list-rest note); naming the
    /// subject changes no pattern-bind rule. A subject that is already a
    /// variable is left alone — it has its own owner.
    fn name_destructure_subjects(&mut self, stmts: &mut Vec<IrStmt>) {
        let named = |s: &IrStmt| {
            matches!(&s.kind, IrStmtKind::BindDestructure { value, .. }
                if droppable_ty(&value.ty) && !matches!(value.kind, IrExprKind::Var { .. }))
        };
        if !stmts.iter().any(named) {
            return;
        }
        *self.changed = true;
        let mut out = Vec::with_capacity(stmts.len() + 1);
        for mut s in std::mem::take(stmts) {
            if named(&s)
                && let IrStmtKind::BindDestructure { value, .. } = &mut s.kind
            {
                let ty = value.ty.clone();
                let span = value.span;
                let id = self.vars.alloc(sym("__destructure_subject"), ty.clone(), Mutability::Let, span);
                let subject = std::mem::replace(
                    value,
                    IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None },
                );
                out.push(IrStmt {
                    kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value: subject },
                    span: s.span,
                });
            }
            out.push(s);
        }
        *stmts = out;
    }

    /// #2988 — a list / tuple / record literal whose operand can PROPAGATE
    /// (`[1, chk(s)!]`, `(1, chk(s)!)`, `P { a: 1, b: chk(s)! }`). The
    /// emitter allocates the container first and lowers the operands into
    /// it, so a `!` that returns from the middle of the build leaves the
    /// half-built block held only by the operand stack: never released,
    /// nor the credits of the operands already stored. Every operand is
    /// bound first, in order — `{ let a = 1; let b = chk(s)!; [a, b] }` —
    /// so the `!` exits before anything is allocated and the operands
    /// already evaluated are frame locals its exit plan releases. (The
    /// one-slot boxes do the same in the emitter, data.rs
    /// `lower_payload_then_box`.) A literal no operand of which propagates
    /// keeps its tree, and its bytes.
    fn hoist_propagating_elements(&mut self, e: &mut IrExpr) -> bool {
        let operands: Vec<&mut IrExpr> = match &mut e.kind {
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter_mut().collect(),
            IrExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, v)| v).collect(),
            // The base is evaluated first (copy, then overwrite).
            IrExprKind::SpreadRecord { base, fields } => {
                std::iter::once(base.as_mut()).chain(fields.iter_mut().map(|(_, v)| v)).collect()
            }
            _ => return false,
        };
        if !operands.iter().any(|a| crate::fs_meta::expr_propagates(a)) {
            return false;
        }
        let mut binds = Vec::new();
        for a in operands {
            if is_literal(a) {
                continue;
            }
            let (ty, span) = (a.ty.clone(), a.span);
            let id = self.vars.alloc(sym("__elem_tmp"), ty.clone(), Mutability::Let, span);
            let value =
                std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None });
            binds.push(IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value }, span });
        }
        *self.changed = true;
        let (ty, span) = (e.ty.clone(), e.span);
        let lit = std::mem::take(e);
        *e = IrExpr { kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(lit)) }, ty, span, def_id: None };
        true
    }

    fn walk_with_tail(&mut self, e: &mut IrExpr, tail: bool) {
        match &mut e.kind {
            IrExprKind::Lambda { body, .. } => self.visit_with_tail(body, true),
            // Fallible callbacks use ok(unwrap(call)) as the identity
            // carrier wrapper. Keep it visible to fan's forwarding route.
            IrExprKind::ResultOk { expr } => self.visit_with_tail(expr, tail),
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
    ) || is_constructed_here(e)
        || unwrap_or_joins_owned(e)
        // `r?` (Result → Option) builds a fresh some-cell (data.rs, #2969).
        || matches!(&e.kind, IrExprKind::ToOption { .. })
        // A value `match` hands its join one credit on every arm
        // (patterns.rs `lower_arm_body`): `a + match r { … }` read it and
        // released nothing (#2972). A value `if` is owned exactly when its
        // lowering normalized it; named either way, the Bind route decides.
        || matches!(&e.kind, IrExprKind::Match { .. } | IrExprKind::If { .. })
}

/// #2971 / #2972 / #3018 — a block the expression CONSTRUCTS: a tuple, a
/// record, a `some` / `ok` / `err` box. A reader (`match (1, (4, 5), 3)`,
/// `(1, 2) == (1, 2)`, `(2, 7).0`) consumes nothing, so like a call result
/// the fresh block has no owner unless it is named first.
fn is_constructed_here(e: &IrExpr) -> bool {
    matches!(
        &e.kind,
        IrExprKind::Tuple { .. }
            | IrExprKind::Record { .. }
            | IrExprKind::SpreadRecord { .. }
            | IrExprKind::OptionSome { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            | IrExprKind::MapLiteral { .. }
            | IrExprKind::EmptyMap
    )
}

/// `r ?? fb` over a heap payload hands its consumer a credit when the
/// fallback may be a fresh block (#2970, data.rs `own_unwrap_or_join`), so a
/// reader that consumes nothing must see it through a bound local. A var or
/// a string literal fallback never makes the join owned (a borrow, a pool
/// static): that `??` stays a view and is not named.
pub(crate) fn unwrap_or_joins_owned(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::UnwrapOr { fallback, .. } => {
            !matches!(fallback.kind, IrExprKind::Var { .. } | IrExprKind::LitStr { .. })
        }
        _ => false,
    }
}

/// #2312: `"${int.to_string(x)}"` displays exactly as `"${x}"` (`x: Int`
/// — the decimal digits either way), so the part becomes `x` BEFORE the
/// operand scan: no String temporary is produced, bound or released, and
/// the build appends the digits from the itoa scratch. Stdout is the same
/// bytes; one allocation per such part is gone.
fn fold_int_display_parts(parts: &mut [almide_ir::IrStringPart], changed: &mut bool) {
    for p in parts.iter_mut() {
        let almide_ir::IrStringPart::Expr { expr } = p else { continue };
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &mut expr.kind else {
            continue;
        };
        if module.as_str() == "int" && func.as_str() == "to_string" && args.len() == 1 && args[0].ty == Ty::Int {
            let x = args.pop().expect("one arg");
            *expr = x;
            *changed = true;
        }
    }
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
        if self.hoist_propagating_elements(e) {
            return;
        }
        match &mut e.kind {
            IrExprKind::Block { stmts, .. } | IrExprKind::While { body: stmts, .. } => {
                self.name_destructure_subjects(stmts);
                return;
            }
            IrExprKind::ForIn { body, .. } => self.name_destructure_subjects(body),
            _ => {}
        }
        if let IrExprKind::StringInterp { parts } = &mut e.kind {
            fold_int_display_parts(parts, self.changed);
        }
        // A field / position read consumes nothing either: its object is
        // named when it is an owned `r ?? fb` (data.rs `own_unwrap_or_join`),
        // a call result or a constructed literal (#3018).
        let projection = matches!(
            e.kind,
            IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. } | IrExprKind::OptionalChain { .. }
        );
        let unboxes = matches!(e.kind, IrExprKind::Try { .. } | IrExprKind::Unwrap { .. });
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
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => vec![object.as_mut()],
            IrExprKind::OptionalChain { expr, .. } => vec![expr.as_mut()],
            IrExprKind::StringInterp { parts } => parts
                .iter_mut()
                .filter_map(|p| match p {
                    almide_ir::IrStringPart::Expr { expr } => Some(expr),
                    almide_ir::IrStringPart::Lit { .. } => None,
                })
                .collect(),
            _ => return,
        };
        // Children were rewritten first: an operand may already be a
        // `{ let …; value }` block — its value is the block's tail.
        let mut named: Vec<bool> = operands
            .iter()
            .map(|a| {
                let core = tail_of(a);
                let produced = if projection {
                    // #3018: a field / position read of a call result or a
                    // constructed literal (`f().0`, `(2, 7).0`, `f().a`) names
                    // the object so the frame releases it after the read.
                    unwrap_or_joins_owned(core) || is_produced_by_call(core, false) || is_constructed_here(core)
                } else {
                    // A `!` / `?` of a constructed box (`err(e)!`, `ok(x)!`)
                    // has its own raise / unbox route and stays unnamed; a `??`
                    // over one (`some(2.5) ?? 0.1`) only reads it.
                    is_produced_by_call(core, unboxes)
                        || (is_born_here(core) && !(unboxes && is_constructed_here(core)))
                };
                bindable_ty(&a.ty) && produced
            })
            .collect();
        // #2969: an extraction a reader consumes (`"[" + next(s)!`) hands it
        // the payload's credit when its carrier was an owned temporary
        // (data_unwrap.rs `release_ok_carrier`), and the reader releases
        // nothing — it is named like any produced value. It is an EFFECT
        // call, so every operand before it is named too, in order: a scalar
        // effect operand left in place (`"${count()!} ${name()!}"`) would
        // otherwise run after it.
        if !projection
            && let Some(last) = operands.iter().rposition(|a| {
                bindable_ty(&a.ty) && matches!(tail_of(a).kind, IrExprKind::Try { .. } | IrExprKind::Unwrap { .. })
            })
        {
            named[last] = true;
            for (i, a) in operands.iter().enumerate().take(last) {
                if !is_literal(a) {
                    named[i] = true;
                }
            }
        }
        let mut binds: Vec<IrStmt> = Vec::new();
        for (a, name) in operands.into_iter().zip(named) {
            if !name {
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
