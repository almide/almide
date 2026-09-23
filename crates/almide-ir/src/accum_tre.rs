//! Accumulator tail-recursion elimination (#2577) — the LLVM TRE
//! accumulator transform, done once on the IR so every leg that runs it
//! makes the SAME decision from the SAME preconditions.
//!
//! ```text
//! fn fib(n: Int) -> Int = if n < 2 then n else fib(n - 1) + fib(n - 2)
//! ```
//! becomes (conceptually)
//! ```text
//! fn fib(n: Int) -> Int = {
//!   var m = n; var acc = 0
//!   while m >= 2 { acc = acc + fib(m - 1); m = m - 2 }
//!   acc + m
//! }
//! ```
//! One recursive call per non-leaf instead of two (fib(35): 14.9M calls,
//! not 29.9M).
//!
//! ## Preconditions (every one checked by [`match_shape`])
//!
//! * **Shape.** The body is `if C then B else f(A) OP f(p - k)` (either
//!   branch may hold the recursion; either operand of `OP` may be the
//!   `f(p - k)` one) for a fn `f` with exactly ONE parameter `p`, which is
//!   `Int`, and an `Int` return type. Not generic, not `effect`, no
//!   extern/export/scoped attribute.
//! * **The operator is associative and commutative on the type, with an
//!   identity.** Only `AddInt` (identity 0) and `MulInt` (identity 1).
//!   Almide `Int` `+`/`*` are two's-complement WRAPPING on every leg
//!   (contract C-170; native renders `wrapping_add`/`wrapping_mul`, wasm
//!   `i64.add`/`i64.mul`), so Int is the commutative ring Z/2^64:
//!   reassociating a sum or product cannot change its value, and there is
//!   no overflow trap whose firing reassociation could move. `Float` is NOT
//!   eligible (IEEE `+` is not associative — the rewrite would change the
//!   rounded result, and the sign of a zero sum), nor is `String`/`List`
//!   concatenation (not commutative) — those are never matched.
//! * **Purity, checked structurally.** `C`, `B` and `A` may only be built
//!   from the parameter, integer/bool literals, the wrapping Int ops
//!   (`+ - *`), comparisons, `and`/`or`/`not` and Int negation. No call of
//!   any kind other than the two self-calls, no division (a trap), no
//!   allocation, no effect: the function computes a pure Int from an Int,
//!   so evaluation order is unobservable — no output, no allocation the
//!   alloc ledgers could see move, no trap that could fire in a different
//!   place.
//! * **Termination is preserved, not introduced.** `C` must be a bound
//!   test of `p` against an integer literal that sends every `p >= L` to
//!   the recursive branch (`p < L`, `p <= L - 1`, `p >= L`, `p > L - 1`, or
//!   the mirrored literal-first spellings), and `k >= 1` with `L - k`
//!   representable. The loop `while m >= L { …; m = m - k }` then provably
//!   terminates and `m - k` never wraps. The original's right spine had
//!   the same length, so the only observable difference is recursion
//!   DEPTH: the right spine no longer consumes stack. A stack overflow can
//!   be removed, never added, and no value changes.
//!
//! Metering: the new loop adds a loop head, which the deterministic meter
//! (ALS-DT2) would charge. The structural wasm leg therefore runs this only
//! when the program brackets no budget/timeout region (the meter is elided
//! whole there, so charges are unobservable); `ALMIDE_FUEL_PROBE` builds
//! route to the incumbent leg, which does not run this rewrite.

use almide_base::intern::sym;
use almide_lang::types::Ty;

use crate::*;

/// The matched shape of an eligible function.
struct Shape {
    param: VarId,
    /// The base-case value (`B`), read at loop exit with `p := m`.
    base: IrExpr,
    /// The non-tail recursive call's argument (`A`).
    left_arg: IrExpr,
    op: BinOp,
    /// The recursive branch runs for every `p >= bound`.
    bound: i64,
    /// The tail spine's step: `f(p - step)`.
    step: i64,
}

/// True iff [`rewrite`] would transform `func` — a cheap check so callers
/// can avoid cloning a program that has nothing to rewrite.
pub fn is_candidate(func: &IrFunction) -> bool {
    match_shape(func).is_some()
}

/// Rewrite `func` in place into the accumulator loop form when every
/// precondition in the module docs holds. Returns whether it rewrote.
/// New locals are allocated in `vars` (the table `func` belongs to).
pub fn rewrite(func: &mut IrFunction, vars: &mut VarTable) -> bool {
    let Some(shape) = match_shape(func) else { return false };
    let span = func.body.span;
    let m = vars.alloc(sym("__acc_n"), Ty::Int, Mutability::Var, None);
    let acc = vars.alloc(sym("__acc"), Ty::Int, Mutability::Var, None);
    let mut var = |id: VarId| {
        vars.increment_use(id);
        IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span, def_id: None }
    };
    let lit = |value: i64| IrExpr { kind: IrExprKind::LitInt { value }, ty: Ty::Int, span, def_id: None };
    let bin = |op: BinOp, ty: Ty, l: IrExpr, r: IrExpr| IrExpr {
        kind: IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) },
        ty,
        span,
        def_id: None,
    };
    let at_m = |e: &IrExpr, v: IrExpr| substitute_var_in_expr(e, shape.param, &v);
    let identity = if shape.op == BinOp::MulInt { 1 } else { 0 };
    let call = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Named { name: func.name },
            args: vec![at_m(&shape.left_arg, var(m))],
            type_args: Vec::new(),
        },
        ty: Ty::Int,
        span,
        def_id: None,
    };
    let stmt = |kind: IrStmtKind| IrStmt { kind, span };
    let body = vec![
        stmt(IrStmtKind::Assign { var: acc, value: bin(shape.op, Ty::Int, var(acc), call) }),
        stmt(IrStmtKind::Assign { var: m, value: bin(BinOp::SubInt, Ty::Int, var(m), lit(shape.step)) }),
    ];
    let cond = bin(BinOp::Gte, Ty::Bool, var(m), lit(shape.bound));
    let while_ = IrExpr {
        kind: IrExprKind::While { cond: Box::new(cond), body },
        ty: Ty::Unit,
        span,
        def_id: None,
    };
    let result = bin(shape.op, Ty::Int, var(acc), at_m(&shape.base, var(m)));
    let param = var(shape.param);
    func.body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![
                stmt(IrStmtKind::Bind { var: m, mutability: Mutability::Var, ty: Ty::Int, value: param }),
                stmt(IrStmtKind::Bind { var: acc, mutability: Mutability::Var, ty: Ty::Int, value: lit(identity) }),
                stmt(IrStmtKind::Expr { expr: while_ }),
            ],
            expr: Some(Box::new(result)),
        },
        ty: Ty::Int,
        span,
        def_id: None,
    };
    true
}

fn match_shape(func: &IrFunction) -> Option<Shape> {
    let [p] = func.params.as_slice() else { return None };
    if p.ty != Ty::Int || p.is_mut || p.default.is_some() || func.ret_ty != Ty::Int {
        return None;
    }
    if func.is_effect
        || func.is_test
        || func.generics.as_ref().is_some_and(|g| !g.is_empty())
        || !func.extern_attrs.is_empty()
        || !func.export_attrs.is_empty()
        || !func.attrs.is_empty()
    {
        return None;
    }
    let IrExprKind::If { cond, then, else_ } = &peel(&func.body).kind else { return None };
    let (rec_when_true, base, rec) = if is_rec_op(then) { (true, else_, then) } else { (false, then, else_) };
    let bound = rec_bound(cond, p.var, rec_when_true)?;
    let IrExprKind::BinOp { op, left, right } = &peel(rec).kind else { return None };
    if !matches!(op, BinOp::AddInt | BinOp::MulInt) {
        return None;
    }
    let name = func.name.as_str();
    let (l, r) = (self_call_arg(left, name)?, self_call_arg(right, name)?);
    // Prefer the right operand as the loop spine; either works (the fn is
    // pure, so which call runs first is unobservable).
    let (left_arg, step) = match (step_of(r, p.var), step_of(l, p.var)) {
        (Some(k), _) => (l, k),
        (None, Some(k)) => (r, k),
        _ => return None,
    };
    let base = peel(base);
    if !(scalar(cond, p.var) && scalar(base, p.var) && scalar(left_arg, p.var)) {
        return None;
    }
    if step < 1 || bound.checked_sub(step).is_none() {
        return None;
    }
    Some(Shape { param: p.var, base: base.clone(), left_arg: left_arg.clone(), op: *op, bound, step })
}

/// Look through `{ e }` blocks with no statements.
fn peel(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Block { stmts, expr: Some(inner) } if stmts.is_empty() => peel(inner),
        _ => e,
    }
}

fn is_rec_op(e: &IrExpr) -> bool {
    matches!(&peel(e).kind, IrExprKind::BinOp { op: BinOp::AddInt | BinOp::MulInt, left, right }
        if matches!(left.kind, IrExprKind::Call { .. }) && matches!(right.kind, IrExprKind::Call { .. }))
}

/// `L` such that the recursive branch runs exactly when `p >= L`.
fn rec_bound(cond: &IrExpr, p: VarId, rec_when_true: bool) -> Option<i64> {
    let IrExprKind::BinOp { op, left, right } = &peel(cond).kind else { return None };
    let is_p = |e: &IrExpr| matches!(peel(e).kind, IrExprKind::Var { id } if id == p);
    let lit = |e: &IrExpr| match peel(e).kind {
        IrExprKind::LitInt { value } => Some(value),
        _ => None,
    };
    // Normalize to `p OP c`.
    let (op, c) = if is_p(left) {
        (*op, lit(right)?)
    } else if is_p(right) {
        let flipped = match op {
            BinOp::Lt => BinOp::Gt,
            BinOp::Lte => BinOp::Gte,
            BinOp::Gt => BinOp::Lt,
            BinOp::Gte => BinOp::Lte,
            _ => return None,
        };
        (flipped, lit(left)?)
    } else {
        return None;
    };
    match (op, rec_when_true) {
        // `if p < c then base else rec` / `if p >= c then rec else base`
        (BinOp::Lt, false) | (BinOp::Gte, true) => Some(c),
        // `if p <= c then base else rec` / `if p > c then rec else base`
        (BinOp::Lte, false) | (BinOp::Gt, true) => c.checked_add(1),
        _ => None,
    }
}

fn self_call_arg<'a>(e: &'a IrExpr, name: &str) -> Option<&'a IrExpr> {
    match &peel(e).kind {
        IrExprKind::Call { target: CallTarget::Named { name: n }, args, type_args }
            if n.as_str() == name && type_args.is_empty() =>
        {
            match args.as_slice() {
                [a] => Some(a),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `k` of an argument spelled `p - k` (literal `k`).
fn step_of(arg: &IrExpr, p: VarId) -> Option<i64> {
    let IrExprKind::BinOp { op: BinOp::SubInt, left, right } = &peel(arg).kind else { return None };
    match (&peel(left).kind, &peel(right).kind) {
        (IrExprKind::Var { id }, IrExprKind::LitInt { value }) if *id == p => Some(*value),
        _ => None,
    }
}

/// A pure, allocation-free, trap-free scalar expression over `p`.
fn scalar(e: &IrExpr, p: VarId) -> bool {
    match &e.kind {
        IrExprKind::Var { id } => *id == p,
        IrExprKind::LitInt { .. } | IrExprKind::LitBool { .. } => true,
        IrExprKind::UnOp { op: UnOp::NegInt | UnOp::Not, operand } => scalar(operand, p),
        IrExprKind::BinOp { op, left, right } => {
            matches!(
                op,
                BinOp::AddInt | BinOp::SubInt | BinOp::MulInt
                    | BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte
                    | BinOp::Eq | BinOp::Neq | BinOp::And | BinOp::Or
            ) && matches!(left.ty, Ty::Int | Ty::Bool)
                && scalar(left, p)
                && scalar(right, p)
        }
        IrExprKind::If { cond, then, else_ } => scalar(cond, p) && scalar(then, p) && scalar(else_, p),
        IrExprKind::Block { stmts, expr: Some(inner) } if stmts.is_empty() => scalar(inner, p),
        _ => false,
    }
}
