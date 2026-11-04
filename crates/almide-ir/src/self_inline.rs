//! Self-recursive inlining, one level (#4456) — the native legs' answer to
//! "LLVM never inlines a recursive call".
//!
//! ```text
//! fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)
//! ```
//! becomes (conceptually)
//! ```text
//! fn fib(n: Int) -> Int =
//!   if n < 2 then n
//!   else { let a = n - 1; if a < 2 then a else fib(a - 1) + fib(a - 2) }
//!      + { let b = n - 2; if b < 2 then b else fib(b - 1) + fib(b - 2) }
//! ```
//!
//! Each self-call in the body is replaced, ONCE, by a copy of the body with
//! the call's arguments bound to fresh `let`s; the calls inside the copies
//! stay calls. LLVM's inliner refuses recursive edges, so without this the
//! rendered Rust (or any C a clang build sees) keeps one call per node. With
//! it, LLVM sees all the grandchild calls of a node in one function, and
//! because the function is pure (it infers `memory(none)` from the scalar
//! body) it merges the identical ones — `fib(a - 2)` and `fib(b - 1)` are
//! both `fib(n - 3)`. fib(35) on the native leg: 293M → 11M instructions,
//! 27.0 → 2.2 ms (GCE n2-standard-8, rustc 1.99.1, x86-64-v3).
//!
//! ## Why this preserves every observable
//!
//! Inlining is beta-reduction with the arguments bound first: the arguments
//! are evaluated once, in their original order, before the copied body runs —
//! exactly the evaluation a call performs. No operator is reassociated (so
//! Int stays wrapping in the same places, contract C-170, and Float rounds
//! identically). The admitted body language (below) has no allocation, no
//! effect, no trap and no call other than the self-call, so nothing an
//! alloc ledger, a meter or stdout could see moves. The one difference is
//! recursion DEPTH, which only shrinks: a stack overflow can be removed,
//! never added (call-stack exhaustion is outside the promise, contract C-196).
//!
//! ## Preconditions (every one checked by [`eligible`])
//!
//! * A plain user fn: not effect, test or generic; no extern/export/other
//!   attribute. One to four parameters, every one `Int`/`Float`/`Bool`,
//!   neither `mut` nor defaulted; a scalar return type.
//! * The body is built only from the parameters, immutable scalar `let`s,
//!   scalar literals, `+ - *` (Int and Float), Float `/` (IEEE, no trap),
//!   comparisons, `and`/`or`/`not`, negation, `if`, and self-calls whose
//!   arguments are themselves in this language. Int `/`, `%` and `^` are
//!   out (a trap, or a runtime helper call).
//! * At least one self-call, at most [`MAX_SELF_CALLS`], and none in tail
//!   position: a tail self-call is the TCO loop's to rewrite, and a copy in
//!   tail position would hide it.
//! * The body has at most [`MAX_BODY_NODES`] nodes, so the rewritten body is
//!   bounded by `(1 + MAX_SELF_CALLS) * MAX_BODY_NODES`.
//!
//! Run BEFORE the accumulator rewrite (`accum_tre`): a function this pass
//! rewrites no longer has `accum_tre`'s shape and keeps its calls for LLVM
//! to merge, which wins (fib: 2.2 ms against 27.0 for the accumulator loop
//! alone); a function it declines still gets the accumulator loop.
//!
//! Native legs only (the v1 native rungs and the standard Rust codegen). The
//! wasm legs keep the accumulator loop: wasmtime does no call merging, and
//! the copies would cost module bytes. `ALMIDE_SELF_INLINE_OFF` is the
//! ablation the perf gate drives.

use std::collections::HashMap;

use almide_base::intern::Sym;
use almide_lang::types::Ty;

use crate::*;

/// The most self-calls a body may hold (the copies multiply the body).
pub const MAX_SELF_CALLS: usize = 4;
/// The most IR nodes the original body may hold.
pub const MAX_BODY_NODES: usize = 48;

fn is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool)
}

/// True iff [`rewrite`] would transform `func`.
pub fn is_candidate(func: &IrFunction) -> bool {
    eligible(func)
}

fn eligible(func: &IrFunction) -> bool {
    let plain_fn = !func.is_effect
        && !func.is_test
        && func.generics.as_ref().is_none_or(|g| g.is_empty())
        && func.extern_attrs.is_empty()
        && func.export_attrs.is_empty()
        && !func.has_attrs_besides_effect_origin();
    let plain_params = (1..=4).contains(&func.params.len())
        && func.params.iter().all(|p| is_scalar(&p.ty) && !p.is_mut && p.default.is_none());
    if !(plain_fn && plain_params && is_scalar(&func.ret_ty)) {
        return false;
    }
    let mut scan = Scan { name: func.name, arity: func.params.len(), calls: 0, nodes: 0 };
    let mut scope: Vec<VarId> = func.params.iter().map(|p| p.var).collect();
    scan.pure(&func.body, &mut scope)
        && (1..=MAX_SELF_CALLS).contains(&scan.calls)
        && scan.nodes <= MAX_BODY_NODES
        && !self_call_in_tail(&func.body, func.name)
}

struct Scan {
    name: Sym,
    arity: usize,
    calls: usize,
    nodes: usize,
}

impl Scan {
    /// Is `e` in the admitted language, reading only vars in `scope`?
    fn pure(&mut self, e: &IrExpr, scope: &mut Vec<VarId>) -> bool {
        self.nodes += 1;
        if !is_scalar(&e.ty) {
            return false;
        }
        match &e.kind {
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } => true,
            IrExprKind::Var { id } => scope.contains(id),
            IrExprKind::UnOp { op: UnOp::NegInt | UnOp::NegFloat | UnOp::Not, operand } => self.pure(operand, scope),
            IrExprKind::BinOp { op, left, right } => {
                matches!(
                    op,
                    BinOp::AddInt | BinOp::SubInt | BinOp::MulInt
                        | BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
                        | BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte
                        | BinOp::Eq | BinOp::Neq | BinOp::And | BinOp::Or
                ) && is_scalar(&left.ty)
                    && is_scalar(&right.ty)
                    && self.pure(left, scope)
                    && self.pure(right, scope)
            }
            IrExprKind::If { cond, then, else_ } => {
                self.pure(cond, scope) && self.pure(then, scope) && self.pure(else_, scope)
            }
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let depth = scope.len();
                let ok = stmts.iter().all(|s| match &s.kind {
                    IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value } if is_scalar(ty) => {
                        let ok = self.pure(value, scope);
                        scope.push(*var);
                        ok
                    }
                    _ => false,
                }) && self.pure(tail, scope);
                scope.truncate(depth);
                ok
            }
            IrExprKind::Call { target: CallTarget::Named { name }, args, type_args }
                if *name == self.name && type_args.is_empty() && args.len() == self.arity =>
            {
                self.calls += 1;
                args.iter().all(|a| self.pure(a, scope))
            }
            _ => false,
        }
    }
}

/// Does a self-call sit in tail position of `e`?
fn self_call_in_tail(e: &IrExpr, name: Sym) -> bool {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Named { name: n }, .. } => *n == name,
        IrExprKind::If { then, else_, .. } => self_call_in_tail(then, name) || self_call_in_tail(else_, name),
        IrExprKind::Block { expr: Some(tail), .. } => self_call_in_tail(tail, name),
        _ => false,
    }
}

/// Inline every self-call of `func` once, when [`eligible`]. Returns whether
/// it rewrote. New locals are allocated in `vars` (the table `func` belongs to).
pub fn rewrite(func: &mut IrFunction, vars: &mut VarTable) -> bool {
    if !eligible(func) {
        return false;
    }
    let original = func.body.clone();
    let params: Vec<VarId> = func.params.iter().map(|p| p.var).collect();
    let mut cx = Inline { name: func.name, ret_ty: func.ret_ty.clone(), params, original: &original, vars };
    // The outer body keeps its own vars: only the self-calls change.
    func.body = cx.outer(&original);
    true
}

struct Inline<'a> {
    name: Sym,
    ret_ty: Ty,
    params: Vec<VarId>,
    original: &'a IrExpr,
    vars: &'a mut VarTable,
}

impl Inline<'_> {
    /// The outer body: identical, except every self-call becomes a copy.
    fn outer(&mut self, e: &IrExpr) -> IrExpr {
        let mut out = e.clone();
        out.kind = match &e.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if *name == self.name => {
                let args: Vec<IrExpr> = args.iter().map(|a| self.outer(a)).collect();
                return self.copy_at(args, e.span);
            }
            IrExprKind::UnOp { op, operand } => IrExprKind::UnOp { op: *op, operand: Box::new(self.outer(operand)) },
            IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
                op: *op,
                left: Box::new(self.outer(left)),
                right: Box::new(self.outer(right)),
            },
            IrExprKind::If { cond, then, else_ } => IrExprKind::If {
                cond: Box::new(self.outer(cond)),
                then: Box::new(self.outer(then)),
                else_: Box::new(self.outer(else_)),
            },
            IrExprKind::Block { stmts, expr: Some(tail) } => IrExprKind::Block {
                stmts: stmts
                    .iter()
                    .map(|s| match &s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => IrStmt {
                            kind: IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: self.outer(value) },
                            span: s.span,
                        },
                        _ => unreachable!("eligible admits only scalar `let`s"),
                    })
                    .collect(),
                expr: Some(Box::new(self.outer(tail))),
            },
            other => other.clone(),
        };
        out
    }

    /// `{ let p1' = a1; …; body[p := p'] }` — the arguments bound first, in
    /// order, then a copy of the original body over fresh vars.
    fn copy_at(&mut self, args: Vec<IrExpr>, span: Option<Span>) -> IrExpr {
        let mut map: HashMap<VarId, VarId> = HashMap::new();
        let mut stmts = Vec::with_capacity(args.len());
        for (param, arg) in self.params.clone().into_iter().zip(args) {
            let ty = self.vars.get(param).ty.clone();
            let fresh = self.vars.alloc_like(param, ty.clone(), Mutability::Let);
            map.insert(param, fresh);
            stmts.push(IrStmt { kind: IrStmtKind::Bind { var: fresh, mutability: Mutability::Let, ty, value: arg }, span });
        }
        let body = self.renamed(self.original, &mut map);
        IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(body)) },
            ty: self.ret_ty.clone(),
            span,
            def_id: None,
        }
    }

    /// A copy of `e` (in the admitted language) with every var renamed
    /// through `map`; each `let` it binds gets a fresh var. Self-calls stay.
    fn renamed(&mut self, e: &IrExpr, map: &mut HashMap<VarId, VarId>) -> IrExpr {
        let mut out = e.clone();
        out.kind = match &e.kind {
            IrExprKind::Var { id } => {
                let id = *map.get(id).expect("eligible: every var is a param or a `let` in scope");
                self.vars.increment_use(id);
                IrExprKind::Var { id }
            }
            IrExprKind::UnOp { op, operand } => IrExprKind::UnOp { op: *op, operand: Box::new(self.renamed(operand, map)) },
            IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
                op: *op,
                left: Box::new(self.renamed(left, map)),
                right: Box::new(self.renamed(right, map)),
            },
            IrExprKind::If { cond, then, else_ } => IrExprKind::If {
                cond: Box::new(self.renamed(cond, map)),
                then: Box::new(self.renamed(then, map)),
                else_: Box::new(self.renamed(else_, map)),
            },
            IrExprKind::Block { stmts, expr: Some(tail) } => {
                let stmts = stmts
                    .iter()
                    .map(|s| match &s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => {
                            let value = self.renamed(value, map);
                            let fresh = self.vars.alloc_like(*var, ty.clone(), *mutability);
                            map.insert(*var, fresh);
                            IrStmt { kind: IrStmtKind::Bind { var: fresh, mutability: *mutability, ty: ty.clone(), value }, span: s.span }
                        }
                        _ => unreachable!("eligible admits only scalar `let`s"),
                    })
                    .collect();
                IrExprKind::Block { stmts, expr: Some(Box::new(self.renamed(tail, map))) }
            }
            IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
                target: target.clone(),
                args: args.iter().map(|a| self.renamed(a, map)).collect(),
                type_args: type_args.clone(),
            },
            other => other.clone(),
        };
        out
    }
}
