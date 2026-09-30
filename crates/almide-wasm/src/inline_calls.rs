//! Small scalar-fn inlining on the structural leg (#2980): a call to a small
//! function whose params, result and every intermediate value are scalars is
//! replaced by its body — a block that binds the arguments, then the callee's
//! own `let`s and tail, all under fresh VarIds of the caller's space.
//!
//! Why here and not only in `almide_mir::lower::inline_small_scalar_fns`: that
//! pass reduces a callee to ONE expression by substituting its `let`s away,
//! and admits only main-program callees. Both limits leave the hot stdlib
//! kernels as calls — the self-hosted trig kernels (`__k_sin`, `__k_cos`, the
//! reduction's medium path) bind a value and read it five times, which
//! substitution would duplicate past its node cap — and a call is what the
//! wasm leg pays for them: wasmtime inlines nothing unless `-C inlining` is
//! set, and a cold call site in a hot body also costs registers across it.
//! Binding instead of substituting keeps each value computed once.
//!
//! WHAT IS ADMITTED (conservative; a decline keeps the call):
//!   - the callee is a function of the SAME var space as the caller (the main
//!     program, or one module), reached by a `Named` call whose name no entry
//!     function shadows (an intra-module call resolves to the entry fn first);
//!   - not effect, not a test, no generics, not `main`, not recursive into
//!     itself, arity matches; neither side is a `scoped` fn or an outlined
//!     `scoped { … }` block (#1997: the region window around a call there is
//!     an obligation, and inlining would take the call away);
//!   - params and result are Int / Float / Bool, and the body is built only
//!     from literals, variables, unary/binary operators, `if`, blocks of
//!     immutable scalar `let`s, and calls (a `Named` call of the same space,
//!     or a scalar `prim.*` op) whose arguments and result are scalars —
//!     nothing that can leave the body early (`!`, `guard`, `break`), no loop,
//!     no heap value, so there is no ownership event to move;
//!   - the body is tiny (the node cap below, a trapping operator weighing
//!     what its guard costs): the module must not grow for it.
//!
//! ARGUMENTS keep call semantics: each is evaluated once, in order, into a
//! fresh `let` — except where the shared speculation rule
//! ([`almide_ir::speculation::is_speculation_safe`], #2958) says moving it is
//! unobservable: a speculation-safe argument whose param is read at most once
//! (or that is a bare variable or literal) is substituted instead of bound.
//! A `tail` call inside the callee becomes a plain call: it is no longer the
//! frame's last act (the emitter still returns through a call in the
//! caller's own tail position).

use std::collections::{HashMap, HashSet};

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrFunction, IrProgram, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_types::types::Ty;

/// Inline a callee this small (in weighted body nodes) at every site: its
/// body costs about what the call and its argument shuffling do, so the
/// module does not grow for it (the size ledgers are an axis too, #2980).
/// Measured before settling here: letting ~80-node callees inline at up to
/// a few sites won 7% on a trig-bound loop but grew the corpus 3.1% (+20%
/// on programs linking trig); a single-site tier for them still grew the
/// size ladder's wordfreq rung (the arguments' own locals), for <4%.
const MAX_SIZE: usize = 12;
/// The node weight of an operator that can trap (`/` `%` `**` by a
/// non-literal): its inline guard and abort frame are that much code.
const TRAP_WEIGHT: usize = 16;
/// Rounds: the second inlines what the first exposed, still under the cap.
const ROUNDS: usize = 2;

/// The pure scalar `prim.*` ops an inlined body may call (the Float/bit
/// floor the self-hosted math is written in). Anything touching memory,
/// allocation or the host is outside.
const PURE_PRIMS: &[&str] = &[
    "ffrombits", "fbits", "band", "bor", "bxor", "bshl", "bshr", "bshr_u", "f2i", "i2f", "feq", "fne",
    "flt", "fle", "fgt", "fge", "fadd", "fsub", "fmul", "fdiv", "fabs", "fneg", "fsqrt", "ffloor", "fceil",
    "fcopysign", "fnearest", "fmin", "fmax",
];

fn is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool)
}

/// The body-node count of an admissible body, or None when the body leaves
/// the admitted subset. `same_space` names the calls that stay calls.
fn admissible_size(e: &IrExpr, fns: &HashSet<String>) -> Option<usize> {
    let sub = |x: &IrExpr| admissible_size(x, fns);
    let n = match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } | IrExprKind::Var { .. } => {
            if !is_scalar(&e.ty) {
                return None;
            }
            1
        }
        // A trapping operator (the shared rule, #2958) emits its guard and
        // abort frame at every copy: weigh it like the code it becomes.
        IrExprKind::BinOp { op, left, right } => {
            let w = if almide_ir::speculation::binop_may_trap(*op, right) { TRAP_WEIGHT } else { 1 };
            w + sub(left)? + sub(right)?
        }
        IrExprKind::UnOp { operand, .. } => 1 + sub(operand)?,
        IrExprKind::If { cond, then, else_ } => 1 + sub(cond)? + sub(then)? + sub(else_)?,
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let mut n = 1 + sub(tail)?;
            for s in stmts {
                let IrStmtKind::Bind { mutability: Mutability::Let, ty, value, .. } = &s.kind else {
                    return None;
                };
                if !is_scalar(ty) {
                    return None;
                }
                n += 1 + sub(value)?;
            }
            n
        }
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => call_size(e, target, args, fns)?,
        _ => return None,
    };
    if !is_scalar(&e.ty) {
        return None;
    }
    Some(n)
}

/// A call's admissible size: a `Named` call of the same space or a pure
/// scalar prim, scalar arguments and result (split from `admissible_size`).
fn call_size(e: &IrExpr, target: &CallTarget, args: &[IrExpr], fns: &HashSet<String>) -> Option<usize> {
    let ok = match target {
        CallTarget::Named { name } => fns.contains(name.as_str()),
        CallTarget::Module { module, func, .. } => module.as_str() == "prim" && PURE_PRIMS.contains(&func.as_str()),
        _ => false,
    };
    if !ok || !is_scalar(&e.ty) {
        return None;
    }
    let mut n = 1;
    for a in args {
        if !is_scalar(&a.ty) {
            return None;
        }
        n += admissible_size(a, fns)?;
    }
    Some(n)
}

fn calls_itself(e: &IrExpr, name: &str) -> bool {
    let mut found = false;
    visit(e, &mut |x| {
        if let IrExprKind::Call { target: CallTarget::Named { name: n }, .. }
        | IrExprKind::TailCall { target: CallTarget::Named { name: n }, .. } = &x.kind
            && n.as_str() == name
        {
            found = true;
        }
    });
    found
}

/// Pre-order walk over the admitted subset (the only shapes a candidate holds).
fn visit(e: &IrExpr, f: &mut dyn FnMut(&IrExpr)) {
    f(e);
    match &e.kind {
        IrExprKind::BinOp { left, right, .. } => {
            visit(left, f);
            visit(right, f);
        }
        IrExprKind::UnOp { operand, .. } => visit(operand, f),
        IrExprKind::If { cond, then, else_ } => {
            visit(cond, f);
            visit(then, f);
            visit(else_, f);
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                if let IrStmtKind::Bind { value, .. } = &s.kind {
                    visit(value, f);
                }
            }
            if let Some(t) = expr {
                visit(t, f);
            }
        }
        IrExprKind::Call { args, .. } | IrExprKind::TailCall { args, .. } => {
            for a in args {
                visit(a, f);
            }
        }
        _ => {}
    }
}

fn reads_of(body: &IrExpr, v: VarId) -> usize {
    let mut n = 0;
    visit(body, &mut |x| {
        if matches!(x.kind, IrExprKind::Var { id } if id == v) {
            n += 1;
        }
    });
    n
}

#[derive(Clone)]
struct Callee {
    params: Vec<(VarId, Ty)>,
    body: IrExpr,
}

/// Copy `e` with every bound var renamed to a fresh one of `vt` (`env` maps
/// the callee's vars to their replacement expressions); tail calls become
/// plain calls.
fn instantiate(e: &IrExpr, env: &mut HashMap<VarId, IrExpr>, vt: &mut VarTable) -> IrExpr {
    let kind = match &e.kind {
        IrExprKind::Var { id } => return env.get(id).cloned().unwrap_or_else(|| e.clone()),
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op: *op,
            left: Box::new(instantiate(left, env, vt)),
            right: Box::new(instantiate(right, env, vt)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp { op: *op, operand: Box::new(instantiate(operand, env, vt)) },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(instantiate(cond, env, vt)),
            then: Box::new(instantiate(then, env, vt)),
            else_: Box::new(instantiate(else_, env, vt)),
        },
        IrExprKind::Block { stmts, expr } => {
            let mut out = Vec::with_capacity(stmts.len());
            for s in stmts {
                if let IrStmtKind::Bind { var, mutability, ty, value } = &s.kind {
                    let value = instantiate(value, env, vt);
                    let info = vt.get(*var).clone();
                    let fresh = vt.alloc(info.name, ty.clone(), *mutability, info.span);
                    env.insert(*var, IrExpr { kind: IrExprKind::Var { id: fresh }, ty: ty.clone(), span: None, def_id: None });
                    out.push(IrStmt { kind: IrStmtKind::Bind { var: fresh, mutability: *mutability, ty: ty.clone(), value }, span: s.span });
                }
            }
            IrExprKind::Block { stmts: out, expr: expr.as_ref().map(|t| Box::new(instantiate(t, env, vt))) }
        }
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target: target.clone(),
            args: args.iter().map(|a| instantiate(a, env, vt)).collect(),
            type_args: type_args.clone(),
        },
        IrExprKind::TailCall { target, args } => IrExprKind::Call {
            target: target.clone(),
            args: args.iter().map(|a| instantiate(a, env, vt)).collect(),
            type_args: Vec::new(),
        },
        other => other.clone(),
    };
    IrExpr { kind, ..e.clone() }
}

/// The inlined form of one call as statements plus a tail: the argument
/// binds (or substitutions), the callee's own top-level `let`s renamed, then
/// its renamed tail.
fn inline_parts(c: &Callee, args: &[IrExpr], span: Option<almide_ir::Span>, vt: &mut VarTable) -> (Vec<IrStmt>, IrExpr) {
    let mut env = HashMap::new();
    let mut stmts = Vec::new();
    for ((pv, pty), a) in c.params.iter().zip(args) {
        let trivial = matches!(
            a.kind,
            IrExprKind::Var { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. }
        );
        if trivial || (almide_ir::speculation::is_speculation_safe(a) && reads_of(&c.body, *pv) <= 1) {
            env.insert(*pv, a.clone());
        } else {
            let info = vt.get(*pv).clone();
            let fresh = vt.alloc(info.name, pty.clone(), Mutability::Let, info.span);
            env.insert(*pv, IrExpr { kind: IrExprKind::Var { id: fresh }, ty: pty.clone(), span: None, def_id: None });
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind { var: fresh, mutability: Mutability::Let, ty: pty.clone(), value: a.clone() },
                span,
            });
        }
    }
    match instantiate(&c.body, &mut env, vt) {
        IrExpr { kind: IrExprKind::Block { stmts: inner, expr: Some(tail) }, .. } => {
            stmts.extend(inner);
            (stmts, *tail)
        }
        body => (stmts, body),
    }
}

/// The inlined form of one call in expression position: a block, or the bare
/// body when nothing needed binding.
fn inline_site(c: &Callee, args: &[IrExpr], site: &IrExpr, vt: &mut VarTable) -> IrExpr {
    let (stmts, tail) = inline_parts(c, args, site.span, vt);
    if stmts.is_empty() {
        return IrExpr { span: site.span, ..tail };
    }
    IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) },
        ty: site.ty.clone(),
        span: site.span,
        def_id: None,
    }
}

struct Rewriter<'a> {
    callees: &'a HashMap<String, Callee>,
    this: &'a str,
    vt: &'a mut VarTable,
    changed: bool,
}

impl Rewriter<'_> {
    /// The callee a call inlines to here, when this site admits it.
    fn admit(&self, e: &IrExpr) -> Option<(Callee, Vec<IrExpr>)> {
        let (IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
        | IrExprKind::TailCall { target: CallTarget::Named { name }, args }) = &e.kind
        else {
            return None;
        };
        if name.as_str() == self.this {
            return None;
        }
        let c = self.callees.get(name.as_str())?;
        if args.len() != c.params.len() || !args.iter().all(|a| is_scalar(&a.ty)) {
            return None;
        }
        Some((c.clone(), args.clone()))
    }

    /// A `let x = f(..)` statement inlines into the enclosing block's own
    /// statement list — `let`s, then `let x = tail` — instead of a block as
    /// the binding's value: the same order of evaluation, one frame shape
    /// the emitter (and its witness) already reads.
    fn splice_binds(&mut self, stmts: &mut Vec<IrStmt>) {
        let mut out = Vec::with_capacity(stmts.len());
        for st in std::mem::take(stmts) {
            if let IrStmtKind::Bind { var, mutability, ty, value } = &st.kind
                && let Some((c, args)) = self.admit(value)
            {
                let (pre, tail) = inline_parts(&c, &args, value.span, self.vt);
                out.extend(pre);
                out.push(IrStmt {
                    kind: IrStmtKind::Bind { var: *var, mutability: *mutability, ty: ty.clone(), value: IrExpr { span: value.span, ..tail } },
                    span: st.span,
                });
                self.changed = true;
                continue;
            }
            out.push(st);
        }
        *stmts = out;
    }

    fn expr(&mut self, e: &mut IrExpr) {
        if let IrExprKind::Block { stmts, .. } = &mut e.kind {
            self.splice_binds(stmts);
        }
        almide_ir::visit_mut::walk_expr_mut(self, e);
        let Some((c, args)) = self.admit(e) else { return };
        *e = inline_site(&c, &args, e, self.vt);
        self.changed = true;
    }
}

impl almide_ir::visit_mut::IrMutVisitor for Rewriter<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        self.expr(e);
    }
}

/// The admitted callees of one var space.
fn callees_of(fns: &[IrFunction], shadowed: &HashSet<String>) -> HashMap<String, Callee> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for f in fns {
        *count.entry(f.name.as_str()).or_default() += 1;
    }
    let names: HashSet<String> =
        fns.iter().map(|f| f.name.as_str().to_string()).filter(|n| count[n.as_str()] == 1).collect();
    let mut out = HashMap::new();
    for f in fns {
        let name = f.name.as_str();
        if f.is_effect
            || f.is_test
            || f.generics.as_ref().is_some_and(|g| !g.is_empty())
            || name == "main"
            || f.is_scoped_fn()
            || f.is_scoped_block_entry()
            || count[name] != 1
            || shadowed.contains(name)
            || !is_scalar(&f.ret_ty)
            || !f.params.iter().all(|p| is_scalar(&p.ty) && !p.is_mut)
            || calls_itself(&f.body, name)
        {
            continue;
        }
        let Some(size) = admissible_size(&f.body, &names) else { continue };
        if size > MAX_SIZE {
            continue;
        }
        let params = f.params.iter().map(|p| (p.var, p.ty.clone())).collect();
        out.insert(name.to_string(), Callee { params, body: f.body.clone() });
    }
    out
}

/// Inline every admitted call in every var space; None when nothing changed
/// (the common program is not cloned twice).
pub(crate) fn inline_small_scalar_calls(ir: &IrProgram) -> Option<IrProgram> {
    // A budget/timeout region meters calls deterministically (ALS-DT2):
    // removing one would move an observable charge, as for accum_tre.
    if crate::fuel::program_has_regions(ir) {
        return None;
    }
    // The A/B switch: the same program with every call kept.
    if almide_base::env::flag("ALMIDE_INLINE_OFF") {
        return None;
    }
    let entry_names: HashSet<String> = ir.functions.iter().map(|f| f.name.as_str().to_string()).collect();
    let mut out = ir.clone();
    let mut any = false;
    for _ in 0..ROUNDS {
        let mut changed = false;
        let callees = callees_of(&out.functions, &HashSet::new());
        changed |= rewrite_space(&mut out.functions, &callees, &mut out.var_table);
        for m in out.modules.iter_mut() {
            let callees = callees_of(&m.functions, &entry_names);
            changed |= rewrite_space(&mut m.functions, &callees, &mut m.var_table);
        }
        any |= changed;
        if !changed {
            break;
        }
    }
    any.then_some(out)
}

fn rewrite_space(fns: &mut [IrFunction], callees: &HashMap<String, Callee>, vt: &mut VarTable) -> bool {
    if callees.is_empty() {
        return false;
    }
    let mut changed = false;
    for f in fns.iter_mut() {
        // A region (#1997) is an obligation around the calls it holds: its
        // frames keep their calls.
        if f.is_scoped_fn() || f.is_scoped_block_entry() {
            continue;
        }
        let this = f.name.as_str().to_string();
        let mut r = Rewriter { callees, this: &this, vt, changed: false };
        almide_ir::visit_mut::IrMutVisitor::visit_expr_mut(&mut r, &mut f.body);
        changed |= r.changed;
    }
    changed
}

