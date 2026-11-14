//! Checked in-place writers at their write-back sites (#4550), split from
//! inline_calls.rs for the file budget: the admission (`writer_body`), the
//! two site shapes (`Rewriter::splice_writers`, `splice_tail_writers`) and
//! the test hook that records them (`writer_sites`).

use std::collections::HashSet;

use almide_ir::visit_mut::IrMutVisitor as _;
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrProgram, IrStmt, IrStmtKind, Mutability, VarId};
use almide_types::types::Ty;

use super::{admissible_size, inline_parts, is_byte_view, is_scalar, Callee, Respan, Rewriter};

/// Counts reads of one variable under any node kind.
struct Reads(VarId, usize);

impl almide_ir::visit::IrVisitor for Reads {
    fn visit_expr(&mut self, e: &IrExpr) {
        if matches!(e.kind, IrExprKind::Var { id } if id == self.0) {
            self.1 += 1;
        }
        almide_ir::visit::walk_expr(self, e);
    }
}

/// Reads of `v` anywhere under `e` (any expression kind).
fn reads_anywhere(e: &IrExpr, v: VarId) -> usize {
    let mut r = Reads(v, 0);
    almide_ir::visit::IrVisitor::visit_expr(&mut r, e);
    r.1
}

/// Reads of `v` anywhere under one statement.
fn stmt_reads(s: &IrStmt, v: VarId) -> usize {
    let mut r = Reads(v, 0);
    almide_ir::visit::IrVisitor::visit_stmt(&mut r, s);
    r.1
}

/// The node cap of a checked writer's body: its guard, the in-place write
/// and the cold out-of-range call, each a call over params and literals.
const WRITER_MAX: usize = 20;

/// A call of a checked writer's arm: a Unit call (a `Named` fn of the
/// module, or a non-`prim` module call) whose arguments are params, string
/// literals or admitted scalar expressions. Returns its node count.
fn writer_arm_size(e: &IrExpr, fns: &HashSet<String>, params: &[VarId]) -> Option<usize> {
    let (IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args }) = &e.kind else {
        return None;
    };
    let ok = match target {
        CallTarget::Named { name } => fns.contains(name.as_str()),
        CallTarget::Module { module, .. } => module.as_str() != "prim",
        _ => false,
    };
    if !ok || e.ty != Ty::Unit {
        return None;
    }
    let mut n = 1;
    for a in args {
        n += match &a.kind {
            IrExprKind::Var { id } if params.contains(id) => 1,
            IrExprKind::LitStr { .. } => 1,
            _ if is_scalar(&a.ty) => admissible_size(a, fns)?,
            _ => return None,
        };
    }
    Some(n)
}

/// The one statement of a write-back arm `{ call; buf }`: the call, when the
/// block's tail hands back exactly the buffer param `buf`.
fn write_back_arm(e: &IrExpr, buf: VarId) -> Option<&IrExpr> {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &e.kind else { return None };
    let [IrStmt { kind: IrStmtKind::Expr { expr: call }, .. }] = stmts.as_slice() else { return None };
    matches!(tail.kind, IrExprKind::Var { id } if id == buf).then_some(call)
}

/// A CHECKED IN-PLACE WRITER (#4550's other half): a fn with ONE `mut`
/// byte-buffer param, the rest scalars, whose body is `if guard then write
/// else out_of_range` — the guard an admitted scalar expression over the
/// params (a byte view read included), each arm one Unit call over the
/// params and literals. This is the shape of every typed `put_*` of the byte
/// IO matrix (`if fits(b, pos, w) then bytes.__set_*(b, pos, x) else
/// put_out_of_range(..)`), decided from the declaration's body, not its
/// name. The IR holds it in its write-back form (C-132): the fn returns the
/// buffer, each arm `{ call; b }`, and a site is `let t = m.f(b, ..); b = t`.
/// Inlined at such a site (`Rewriter::splice_writers`) the buffer is
/// substituted, never bound: the write goes through the caller's own
/// variable — the place the write-back would have rebound — and the guard
/// reads that variable's block. Returns the mut param's index and the Unit
/// body `if guard then write else out_of_range`, the module's own `Named`
/// calls respelled `module.fn` (they resolve to the same declarations from
/// any space), or None.
pub(super) fn writer_body(f: &IrFunction, fns: &HashSet<String>, module: &str) -> Option<(usize, IrExpr)> {
    let muts: Vec<usize> = f.params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(k, _)| k).collect();
    let [k] = muts.as_slice() else { return None };
    let buf = &f.params[*k];
    if !is_byte_view(&buf.ty) || f.ret_ty != buf.ty || !f.params.iter().all(|p| p.is_mut || is_scalar(&p.ty)) {
        return None;
    }
    let body = match &f.body.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } if stmts.is_empty() => tail,
        _ => &f.body,
    };
    let IrExprKind::If { cond, then, else_ } = &body.kind else { return None };
    let (then, else_) = (write_back_arm(then, buf.var)?, write_back_arm(else_, buf.var)?);
    let params: Vec<VarId> = f.params.iter().map(|p| p.var).collect();
    let size = admissible_size(cond, fns)? + writer_arm_size(then, fns, &params)? + writer_arm_size(else_, fns, &params)?;
    if size > WRITER_MAX {
        return None;
    }
    let mut body = IrExpr {
        kind: IrExprKind::If { cond: cond.clone(), then: Box::new(then.clone()), else_: Box::new(else_.clone()) },
        ty: Ty::Unit,
        span: body.span,
        def_id: None,
    };
    Respell(module).visit_expr_mut(&mut body);
    Some((*k, body))
}

/// Respells a module's own `Named` calls as `module.fn` calls.
struct Respell<'a>(&'a str);

impl almide_ir::visit_mut::IrMutVisitor for Respell<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        almide_ir::visit_mut::walk_expr_mut(self, e);
        let (IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. }) = &mut e.kind else { return };
        if let CallTarget::Named { name } = target {
            *target = CallTarget::Module { module: almide_base::intern::sym(self.0), func: *name, def_id: None };
        }
    }
}

impl Rewriter<'_> {
    /// A write-back site of a checked writer — `let t = m.f(.., b, ..)`
    /// then `b = t`, the buffer argument the bare variable the next
    /// statement rebinds, and `t` read nowhere else — becomes the writer's
    /// argument binds and its Unit body as a statement (`writer_body`). The
    /// rebinding goes: the inlined write updates `b` in place (its COW gate
    /// repoints `b` at a unique copy), which is what `b = t` stored.
    pub(super) fn splice_writers(&mut self, stmts: &mut Vec<IrStmt>, tail: Option<&IrExpr>) {
        let mut k = 0;
        while k + 1 < stmts.len() {
            let site = match (&stmts[k].kind, &stmts[k + 1].kind) {
                (
                    IrStmtKind::Bind { var: t, mutability: Mutability::Let, value, .. },
                    IrStmtKind::Assign { var: b, value: IrExpr { kind: IrExprKind::Var { id }, .. } },
                ) if id == t => self.writer_site(value, *b).filter(|_| {
                    let later: usize = stmts[k + 2..].iter().map(|s| stmt_reads(s, *t)).sum();
                    later == 0 && tail.is_none_or(|e| reads_anywhere(e, *t) == 0)
                }),
                _ => None,
            };
            let Some((c, args, span)) = site else {
                k += 1;
                continue;
            };
            if let IrStmtKind::Bind { value: IrExpr { kind: IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }, .. }, .. } =
                &stmts[k].kind
            {
                let key = format!("{}.{}", module.as_str(), func.as_str());
                WRITER_SITES.with(|w| w.borrow_mut().as_mut().map(|v| v.push(key)));
            }
            let (mut pre, body) = inline_parts(&c, &args, span, self.vt);
            pre.push(IrStmt { kind: IrStmtKind::Expr { expr: body }, span });
            let n = pre.len();
            stmts.splice(k..k + 2, pre);
            self.changed = true;
            k += n;
        }
    }

    /// A checked writer in TAIL position of a fn whose `mut` param `b` is its
    /// buffer: in the write-back form (C-132) the fn returns `b`, and the
    /// call's value is the written buffer it returns. Spliced as the
    /// writer's binds and Unit body, then `b` — the same buffer, written in
    /// place through `b` (its COW gate repoints `b` at a unique copy) —
    /// which is the writer's own body's shape. Tail positions are a block's
    /// tail and an `if`'s arms, recursively.
    pub(super) fn splice_tail_writers(&mut self, e: &mut IrExpr, muts: &[VarId]) {
        match &mut e.kind {
            IrExprKind::Block { expr: Some(t), .. } => return self.splice_tail_writers(t, muts),
            IrExprKind::If { then, else_, .. } => {
                self.splice_tail_writers(then, muts);
                return self.splice_tail_writers(else_, muts);
            }
            _ => {}
        }
        let Some(&b) = muts.iter().find(|b| self.writer_site(e, **b).is_some()) else { return };
        let Some((c, args, span)) = self.writer_site(e, b) else { return };
        let key = match &e.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
            | IrExprKind::TailCall { target: CallTarget::Module { module, func, .. }, .. } => {
                format!("{}.{}", module.as_str(), func.as_str())
            }
            _ => return,
        };
        WRITER_SITES.with(|w| w.borrow_mut().as_mut().map(|v| v.push(key)));
        let (mut stmts, body) = inline_parts(&c, &args, span, self.vt);
        stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: body }, span });
        let tail = IrExpr { kind: IrExprKind::Var { id: b }, ty: e.ty.clone(), span, def_id: None };
        *e = IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, ty: e.ty.clone(), span, def_id: None };
        self.changed = true;
    }

    /// The writer a write-back site calls, when its buffer argument is the
    /// variable `b` the site rebinds and the other arguments fit.
    fn writer_site(&self, value: &IrExpr, b: VarId) -> Option<(Callee, Vec<IrExpr>, Option<almide_ir::Span>)> {
        let (IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
        | IrExprKind::TailCall { target: CallTarget::Module { module, func, .. }, args }) = &value.kind
        else {
            return None;
        };
        let c = self.cross.get(&format!("{}.{}", module.as_str(), func.as_str()))?;
        let k = c.writer?;
        if !matches!(args.get(k).map(|a| &a.kind), Some(IrExprKind::Var { id }) if *id == b) || !self.args_fit(c, args) {
            return None;
        }
        let mut c = c.clone();
        Respan(value.span).visit_expr_mut(&mut c.body);
        Some((c, args.clone(), value.span))
    }
}

thread_local! {
    /// The `module.fn` of each checked-writer site spliced, while a
    /// [`writer_sites`] call records (this thread only).
    static WRITER_SITES: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Test hook: emit `ir` and return the `module.fn` of every checked-writer
/// site the inliner spliced (`Rewriter::splice_writers`), in splice order.
#[doc(hidden)]
pub fn writer_sites(ir: &IrProgram) -> Vec<String> {
    WRITER_SITES.with(|w| *w.borrow_mut() = Some(Vec::new()));
    let _ = crate::emit_program(ir);
    WRITER_SITES.with(|w| w.borrow_mut().take()).unwrap_or_default()
}
