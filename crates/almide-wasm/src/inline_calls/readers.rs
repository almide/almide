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
//! apart on the spot the cell is never needed. When the guard is a room test
//! (`fits(b, pos, w)`, recognised by its body) it becomes the branch-free
//! room op `bytes.room#(b, pos, w)`, and:
//!
//!   m.f(args) ?? d       (d a literal or a variable)
//!     =>  { binds; bytes.__read_*#or(b, pos, w, d) }     one room test, the read proven
//!   m.f(args) ?? d       (any other d)
//!     =>  { binds; let r = read; if bytes.room#(b, pos, w) then r else d }
//!   match m.f(args) { some(v) => A, none => B }
//!     =>  { binds; let v = read; if bytes.room#(b, pos, w) then A else B }
//!
//! The read is TOTAL (a window out of room reads the type's default, C-229),
//! so it may run before the test; it goes there because an `if` whose
//! condition and an arm both borrow one buffer doubles that buffer's witness
//! paths at every site (a frame of many reads then declines past the path
//! cap). A reader whose guard is not a room test, or whose read is not a
//! position read of the matrix (`get_f16_be` reads through a helper), keeps
//! its call.
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

/// A module fn whose body is a byte window's room test over its params —
/// `p >= 0 and p <= bytes.len(b) - w` (the bytes module's `fits`) — as the
/// indices of `(b, p, w)`. Read from the declaration's body, not its name.
fn room_test(f: &IrFunction) -> Option<(usize, usize, usize)> {
    use almide_ir::BinOp;
    let body = match &f.body.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } if stmts.is_empty() => tail,
        _ => &f.body,
    };
    let IrExprKind::BinOp { op: BinOp::And, left, right } = &body.kind else { return None };
    let IrExprKind::BinOp { op: BinOp::Gte, left: p0, right: zero } = &left.kind else { return None };
    let IrExprKind::BinOp { op: BinOp::Lte, left: p1, right: room } = &right.kind else { return None };
    let IrExprKind::BinOp { op: BinOp::SubInt, left: len, right: w } = &room.kind else { return None };
    // The same reading of `bytes.len(b)` as `Emitter::proven_room`'s.
    let IrExprKind::Call { target: CallTarget::Module { module: lm, func: lf, .. }, args: largs, .. } = &len.kind else { return None };
    let [b] = largs.as_slice() else { return None };
    let reads_len = lm.as_str() == "bytes" && lf.as_str() == "len" && matches!(zero.kind, IrExprKind::LitInt { value: 0 });
    let param = |e: &IrExpr| match e.kind {
        IrExprKind::Var { id } => f.params.iter().position(|p| p.var == id),
        _ => None,
    };
    let (pi, bi, wi) = (param(p0)?, param(b)?, param(w)?);
    (reads_len && param(p1)? == pi).then_some((bi, pi, wi))
}

/// The body of a checked reader (see the module doc) as the scalar `if guard
/// then read else none`, the `none` a placeholder each site replaces, the
/// module's own `Named` calls respelled `module.fn` (they resolve to the same
/// declarations from any space); or None. A guard that calls a room test
/// with a literal width (`fits(b, pos, 4)`) becomes the branch-free room op
/// (`front_desugar::BYTES_ROOM`): the short-circuit `and` it would inline to
/// is a branch of its own around the buffer's read, which multiplies the
/// paths the frame's witness enumerates at every site.
pub(super) fn reader_body(f: &IrFunction, fns: &HashSet<String>, module: &str, decls: &[IrFunction]) -> Option<IrExpr> {
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
    let mut cond: IrExpr = (**cond).clone();
    if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &cond.kind
        && let Some((bi, pi, wi)) = decls.iter().find(|d| d.name == *name).and_then(room_test)
        && matches!(args.get(wi).map(|a| &a.kind), Some(IrExprKind::LitInt { .. }))
    {
        let args = vec![args[bi].clone(), args[pi].clone(), args[wi].clone()];
        cond.kind = IrExprKind::Call {
            target: CallTarget::Module {
                module: almide_base::intern::sym("bytes"),
                func: almide_base::intern::sym(crate::emit::front_desugar::BYTES_ROOM),
                def_id: None,
            },
            args,
            type_args: Vec::new(),
        };
    }
    let mut body = IrExpr {
        kind: IrExprKind::If { cond: Box::new(cond), then: read.clone(), else_: else_.clone() },
        ty: payload_ty.clone(),
        span: body.span,
        def_id: None,
    };
    Respell(module).visit_expr_mut(&mut body);
    Some(body)
}

/// A `some(..)` arm's binder: the variable and its type, or none for `some(_)`.
type SomeBind = Option<(almide_ir::VarId, Ty)>;

/// How a site takes the reader's Option apart.
enum Use {
    /// `?? d`: the fallback.
    Or(IrExpr),
    /// `match`: the some arm (its binder, when it binds) and the none arm.
    Match(SomeBind, IrExpr, IrExpr),
}

/// A two-arm, guard-free `match` over an Option: (the some arm's binder, its
/// body, the other arm's body), the other arm `none` or `_`.
fn option_arms(arms: &[IrMatchArm]) -> Option<(SomeBind, IrExpr, IrExpr)> {
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

/// The `(b, pos, w)` of a room op call (`front_desugar::BYTES_ROOM`).
fn room_op_args(cond: &IrExpr) -> Option<(IrExpr, IrExpr, IrExpr)> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &cond.kind else { return None };
    let [b, p, w] = args.as_slice() else { return None };
    (module.as_str() == "bytes" && func.as_str() == crate::emit::front_desugar::BYTES_ROOM).then(|| (b.clone(), p.clone(), w.clone()))
}

/// A position read of the byte IO matrix, `bytes.__read_*(b, pos)`: its
/// function name and arguments.
fn read_call(e: &IrExpr) -> Option<(&str, &[IrExpr])> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind else { return None };
    let f = func.as_str();
    (module.as_str() == "bytes" && f.starts_with("__read_") && !f.ends_with("_at") && !f.ends_with("_array") && args.len() == 2)
        .then_some((f, args.as_slice()))
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
        // Only a room-test guard over a position read of the matrix: that read
        // is total, so it may run before the test (see the module doc). Any
        // other reader keeps its call.
        match &c.body.kind {
            IrExprKind::If { cond, then, .. } if room_op_args(cond).is_some() && read_call(then).is_some() => {}
            _ => return false,
        }
        let mut c: Callee = c.clone();
        Respan(call.span).visit_expr_mut(&mut c.body);
        let (mut stmts, tail) = inline_parts(&c, &args.clone(), call.span, self.vt);
        let IrExprKind::If { cond, then: read, .. } = tail.kind else { return false };
        let trivial = |d: &IrExpr| matches!(d.kind, IrExprKind::Var { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. });
        let choice = match usage {
            // One room test and the read proven in place, no branch in the IR:
            // a literal or variable fallback is unobservable to evaluate on
            // the none path only, which is where the op lowers it.
            Use::Or(d) if trivial(&d) => {
                let Some((func, _)) = read_call(&read) else { return false };
                let Some((rb, rp, rw)) = room_op_args(&cond) else { return false };
                IrExpr {
                    kind: IrExprKind::Call {
                        target: CallTarget::Module {
                            module: almide_base::intern::sym("bytes"),
                            func: almide_base::intern::sym(&format!("{func}{}", crate::bytes::READ_OR_SUFFIX)),
                            def_id: None,
                        },
                        args: vec![rb, rp, rw, d],
                        type_args: Vec::new(),
                    },
                    ty: e.ty.clone(),
                    span: e.span,
                    def_id: None,
                }
            }
            // The read before the `if`, which then reads no buffer in an arm.
            Use::Or(d) => {
                let ty = read.ty.clone();
                let r = self.vt.alloc(almide_base::intern::sym("__read"), ty.clone(), Mutability::Let, call.span);
                stmts.push(IrStmt { kind: IrStmtKind::Bind { var: r, mutability: Mutability::Let, ty: ty.clone(), value: *read }, span: call.span });
                let rv = IrExpr { kind: IrExprKind::Var { id: r }, ty, span: call.span, def_id: None };
                IrExpr { kind: IrExprKind::If { cond, then: Box::new(rv), else_: Box::new(d) }, ty: e.ty.clone(), span: e.span, def_id: None }
            }
            Use::Match(bind, some, none) => {
                // `some(_)`: the read has nothing to bind and no effect to keep.
                if let Some((var, ty)) = bind {
                    stmts.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: *read }, span: call.span });
                }
                IrExpr { kind: IrExprKind::If { cond, then: Box::new(some), else_: Box::new(none) }, ty: e.ty.clone(), span: e.span, def_id: None }
            }
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
