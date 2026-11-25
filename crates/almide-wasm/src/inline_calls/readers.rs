//! Checked readers at the sites that take their Option apart (#4944), split
//! from inline_calls.rs for the file budget: the admission (`reader_body`),
//! the two site shapes (`Rewriter::reader_site`) and the test hook that
//! records them (`reader_sites`).
//!
//! A CHECKED READER is a module fn with read-only byte-buffer and scalar
//! params whose body is `if guard then some(read) else none`, the guard and
//! the read admitted scalar expressions over the params (a byte view read
//! included). This is the shape of every typed `get_*` of the byte IO matrix
//! (`if fits(b, pos, w) then some(bytes.__read_*(b, pos)) else none`),
//! decided from the declaration's body, not its name. As a call, each read
//! answers a fresh `some` cell — an allocation and a free per read, ~16x a
//! `bytes.get ?? d` in a hot loop (#4944). Where the caller takes the Option
//! apart on the spot the cell is never needed:
//!
//!   m.f(args) ?? d                      =>  { binds; if guard then read else d }
//!   match m.f(args) { some(v) => A,     =>  { binds; if guard then { let v = read; A }
//!                     none => B }                            else B }
//!
//! The arguments bind (or substitute) exactly as any inlined call's do
//! (`inline_parts`), before the guard; `d` / `B` evaluate only on the none
//! path and `A` only on the some path, after the arguments — the order the
//! call and the `??` / `match` had.

use std::collections::HashSet;

use almide_ir::visit_mut::IrMutVisitor as _;
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrMatchArm, IrPattern, IrProgram, IrStmt, IrStmtKind, Mutability};
use almide_types::types::constructor::TypeConstructorId;
use almide_types::types::Ty;

use super::writers::{Respell, WRITER_MAX};
use super::{admissible_size, inline_parts, is_byte_view, is_scalar, Callee, Respan, Rewriter};

/// The scalar payload type of an `Option[T]`.
fn option_payload(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(TypeConstructorId::Option, args) => match args.as_slice() {
            [t] if is_scalar(t) => Some(t),
            _ => None,
        },
        _ => None,
    }
}

/// The body of a checked reader (see the module doc) as the scalar `if guard
/// then read else none`, the `none` a placeholder each site replaces, the
/// module's own `Named` calls respelled `module.fn` (they resolve to the same
/// declarations from any space); or None.
pub(super) fn reader_body(f: &IrFunction, fns: &HashSet<String>, module: &str) -> Option<IrExpr> {
    let payload_ty = option_payload(&f.ret_ty)?;
    if !f.params.iter().any(|p| is_byte_view(&p.ty)) || !f.params.iter().all(|p| !p.is_mut && (is_scalar(&p.ty) || is_byte_view(&p.ty))) {
        return None;
    }
    let body = match &f.body.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } if stmts.is_empty() => tail,
        _ => &f.body,
    };
    let IrExprKind::If { cond, then, else_ } = &body.kind else { return None };
    let (IrExprKind::OptionSome { expr: read }, IrExprKind::OptionNone) = (&then.kind, &else_.kind) else { return None };
    if read.ty != *payload_ty || admissible_size(cond, fns)? + admissible_size(read, fns)? > WRITER_MAX {
        return None;
    }
    let mut body = IrExpr {
        kind: IrExprKind::If { cond: cond.clone(), then: read.clone(), else_: else_.clone() },
        ty: payload_ty.clone(),
        span: body.span,
        def_id: None,
    };
    Respell(module).visit_expr_mut(&mut body);
    Some(body)
}

/// How a site takes the reader's Option apart.
enum Use {
    /// `?? d`: the fallback.
    Or(IrExpr),
    /// `match`: the some arm (its binder, when it binds) and the none arm.
    Match(Option<(almide_ir::VarId, Ty)>, IrExpr, IrExpr),
}

/// A two-arm, guard-free `match` over an Option: (the some arm's binder, its
/// body, the other arm's body), the other arm `none` or `_`.
fn option_arms(arms: &[IrMatchArm]) -> Option<(Option<(almide_ir::VarId, Ty)>, IrExpr, IrExpr)> {
    let [a, b] = arms else { return None };
    if a.guard.is_some() || b.guard.is_some() {
        return None;
    }
    let some_of = |p: &IrPattern| match p {
        IrPattern::Some { inner } => match inner.as_ref() {
            IrPattern::Bind { var, ty } => Some(Some((*var, ty.clone()))),
            IrPattern::Wildcard => Some(None),
            _ => None,
        },
        _ => None,
    };
    let other = |p: &IrPattern| matches!(p, IrPattern::None | IrPattern::Wildcard);
    match (some_of(&a.pattern), some_of(&b.pattern)) {
        (Some(bind), _) if other(&b.pattern) => Some((bind, a.body.clone(), b.body.clone())),
        (_, Some(bind)) if matches!(a.pattern, IrPattern::None) => Some((bind, b.body.clone(), a.body.clone())),
        _ => None,
    }
}

impl Rewriter<'_> {
    /// `m.f(args) ?? d` or a `match m.f(args) { .. }` whose callee is a
    /// checked reader becomes the reader's binds and its `if`, the none arm
    /// the site's own (see the module doc). Returns whether `e` changed.
    pub(super) fn reader_site(&mut self, e: &mut IrExpr) -> bool {
        let (call, usage) = match &e.kind {
            IrExprKind::UnwrapOr { expr, fallback } => (expr.as_ref(), Use::Or(fallback.as_ref().clone())),
            IrExprKind::Match { subject, arms } => match option_arms(arms) {
                Some((bind, some, none)) => (subject.as_ref(), Use::Match(bind, some, none)),
                None => return false,
            },
            _ => return false,
        };
        // A `module.fn(..)` site of any space, or a `fn(..)` site of the
        // reader's own module (`next_u32_le`'s `match get_u32_le(b, pos)`).
        let (key, c, args) = match &call.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } => {
                let key = format!("{}.{}", module.as_str(), func.as_str());
                let Some(c) = self.cross.get(&key) else { return false };
                (key, c, args)
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name.as_str() != self.this => {
                let Some(c) = self.callees.get(name.as_str()) else { return false };
                (name.as_str().to_string(), c, args)
            }
            _ => return false,
        };
        if !c.reader || !self.args_fit(c, args) {
            return false;
        }
        if let Use::Or(d) = &usage
            && Some(&d.ty) != option_payload(&call.ty)
        {
            return false;
        }
        let mut c: Callee = c.clone();
        Respan(call.span).visit_expr_mut(&mut c.body);
        let (stmts, tail) = inline_parts(&c, &args.clone(), call.span, self.vt);
        let IrExprKind::If { cond, then: read, .. } = tail.kind else { return false };
        let (then, else_) = match usage {
            Use::Or(d) => (*read, d),
            Use::Match(bind, some, none) => {
                let then = match bind {
                    Some((var, ty)) => IrExpr {
                        ty: some.ty.clone(),
                        span: some.span,
                        def_id: None,
                        kind: IrExprKind::Block {
                            stmts: vec![IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: *read }, span: call.span }],
                            expr: Some(Box::new(some)),
                        },
                    },
                    // `some(_)`: the read has no effect to keep.
                    None => some,
                };
                (then, none)
            }
        };
        let choice = IrExpr {
            kind: IrExprKind::If { cond, then: Box::new(then), else_: Box::new(else_) },
            ty: e.ty.clone(),
            span: e.span,
            def_id: None,
        };
        READER_SITES.with(|w| w.borrow_mut().as_mut().map(|v| v.push(key)));
        *e = if stmts.is_empty() {
            choice
        } else {
            IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(choice)) }, ty: e.ty.clone(), span: e.span, def_id: None }
        };
        true
    }
}

thread_local! {
    /// The `module.fn` of each checked-reader site rewritten, while a
    /// [`reader_sites`] call records (this thread only).
    static READER_SITES: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Test hook: emit `ir` and return the `module.fn` of every checked-reader
/// site the inliner rewrote (`Rewriter::reader_site`), in rewrite order.
#[doc(hidden)]
pub fn reader_sites(ir: &IrProgram) -> Vec<String> {
    READER_SITES.with(|w| *w.borrow_mut() = Some(Vec::new()));
    let _ = crate::emit_program(ir);
    READER_SITES.with(|w| w.borrow_mut().take()).unwrap_or_default()
}
