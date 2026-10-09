//! Which vars a `mut` call site may MOVE into the call (#3337, #3342) — the
//! block walk's statement analysis, split from writeback_move.rs.
//!
//! A var moves when nothing reads it between the call and the statement
//! that rebinds it, so the emptied local is never observed: the C-132
//! write-back (`let b = f(xs, …); xs = b`, with any statements between
//! that do not read `xs` — arg_temps.rs names a `match` subject first, so
//! the carried form is `let t = f(xs, …); let v = match t { … }; xs = v`),
//! the direct form `xs = <value>`, and a `match f(xs, …) { ok(..) => { xs =
//! b; … }, err(..) => { xs = b; … } }` whose every arm rebinds it
//! (mut_param_err_carry.rs). The call is one the statement's value ENDS in
//! — the value itself, the tail of a block, every arm of an `if` / `match`
//! — or a `match` subject none of whose arms (guards included) reads the
//! var. A statement between that may leave the block (`break`,
//! `continue`, a `guard`) disqualifies the later rebind.
//!
//! An err leaving the frame between the call and the rebind (`!`, a raise)
//! takes the moved var with it: a local dies with the frame, and a CARRYING
//! fn pairs its own buffers with every raise explicitly (`err((e, b))`), a
//! READ of the var this analysis sees.

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_base::intern::Sym;
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, IrStmtKind, VarId};

use super::reads_in_block;

/// A place a `mut` argument names: a var, or one field of a record var
/// (#3343 — `bump(h.xs, k)`, rebound by the write-back's `h.xs = b`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Place {
    Var(VarId),
    Field(VarId, Sym),
}

impl Place {
    /// The var the place lives in — a read of it may read the place.
    fn root(self) -> VarId {
        match self {
            Place::Var(v) | Place::Field(v, _) => v,
        }
    }

    /// The place a `mut` argument names, if it is one.
    pub(crate) fn of(a: &IrExpr) -> Option<Place> {
        match &a.kind {
            IrExprKind::Var { id } => Some(Place::Var(*id)),
            IrExprKind::Member { object, field } => match &object.kind {
                IrExprKind::Var { id } => Some(Place::Field(*id, *field)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The value statement `s` rebinds this place to, if it does.
    fn rebound_by(self, s: &IrStmt) -> Option<&IrExpr> {
        match (&s.kind, self) {
            (IrStmtKind::Assign { var, value }, Place::Var(v)) if *var == v => Some(value),
            (IrStmtKind::FieldAssign { target, field, value }, Place::Field(h, f)) if *target == h && *field == f => Some(value),
            _ => None,
        }
    }
}

/// One statement's move-in sites.
#[derive(Clone)]
pub(super) struct MoveIn {
    /// Each candidate call's argument slice (its identity) and the vars it
    /// may take.
    at: Vec<(usize, Vec<Place>)>,
    /// The tuple the call answered (`let t = f(…); let (r, b) = t`,
    /// arg_temps.rs names it) and its binds that are NOT written back: its
    /// slots keep a credit on each buffer until the frame releases it —
    /// released right after the last write-back instead
    /// (`settle_subject`), when nothing else reads it.
    pub(super) subject: Option<(VarId, Vec<VarId>)>,
    /// The index of the last statement of the site (the last write-back).
    pub(super) last: usize,
}

impl MoveIn {
    /// This statement's sites joined to the enclosing statement's (a value
    /// block lowered inside it, whose own walk would otherwise drop them).
    pub(super) fn joined(site: Option<MoveIn>, outer: &Option<MoveIn>) -> Option<MoveIn> {
        let Some(o) = outer else { return site };
        let mut m = site.unwrap_or(MoveIn { at: Vec::new(), subject: None, last: usize::MAX });
        m.at.extend(o.at.iter().cloned());
        Some(m)
    }

    /// The places the call with this argument slice may take, if it is one.
    pub(super) fn vars_at(&self, args: &[IrExpr]) -> Option<Vec<Place>> {
        let at = args.as_ptr() as usize;
        self.at.iter().find(|(p, _)| *p == at).map(|(_, v)| v.clone())
    }
}

/// The move-in sites of statement `i` of a block, if any.
pub(super) fn move_in_site(stmts: &[IrStmt], tail: Option<&IrExpr>, i: usize) -> Option<MoveIn> {
    let (value, mut vars, subject, mut last) = match &stmts.get(i)?.kind {
        IrStmtKind::Assign { var, value } => (value, vec![Place::Var(*var)], None, i),
        IrStmtKind::FieldAssign { target, field, value } => (value, vec![Place::Field(*target, *field)], None, i),
        IrStmtKind::Bind { var, value, .. } => match write_backs(stmts, tail, i, *var) {
            Some((vars, subject, last)) => (value, vars.into_iter().map(Place::Var).collect(), subject, last),
            None => (value, arm_rebinds(value), None, i),
        },
        IrStmtKind::BindDestructure { value, .. } => (value, arm_rebinds(value), None, i),
        IrStmtKind::Expr { expr } => (expr, arm_rebinds(expr), None, i),
        _ => return None,
    };
    // Any var rebound further down the block before anything reads it.
    let mut at = Vec::new();
    exit_calls(value, &[], &mut at);
    if at.is_empty() {
        return None;
    }
    let args = at.iter().flat_map(|(_, args)| args.iter().copied()).collect::<Vec<Place>>();
    for v in args {
        if !vars.contains(&v)
            && let Some(k) = rebound_before_read(stmts, tail, i, v)
        {
            vars.push(v);
            last = last.max(k);
        }
    }
    if vars.is_empty() {
        return None;
    }
    let mut at = Vec::new();
    exit_calls(value, &vars, &mut at);
    (!at.is_empty()).then_some(MoveIn { at, subject, last })
}

/// The move-in sites of a fold body: the fold rebinds the accumulator
/// `acc` to the body's value right after it, so the body is the value of
/// an `acc = body` statement.
pub(super) fn fold_site(body: &IrExpr, acc: VarId) -> Option<MoveIn> {
    let mut at = Vec::new();
    exit_calls(body, &[Place::Var(acc)], &mut at);
    (!at.is_empty()).then_some(MoveIn { at, subject: None, last: usize::MAX })
}

/// The index of the statement after `i` that rebinds `var` (`var = …` not
/// reading it) with nothing between that reads it or may leave the block —
/// or, past the last statement, a tail that rebinds it first on every path
/// (the carried `match t { ok(..) => { xs = b; r }, err(..) => { xs = b; … } }`).
fn rebound_before_read(stmts: &[IrStmt], tail: Option<&IrExpr>, i: usize, place: Place) -> Option<usize> {
    for (k, s) in stmts.iter().enumerate().skip(i + 1) {
        if let Some(value) = place.rebound_by(s) {
            return (!reads(value, place.root())).then_some(k);
        }
        if stmt_reads(s, place.root()) || leaves(s) {
            return None;
        }
    }
    tail.is_some_and(|t| rebinds_first(t, place)).then_some(i)
}

/// Does every path through `e` rebind `var` before anything reads it?
fn rebinds_first(e: &IrExpr, place: Place) -> bool {
    let var = place.root();
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                if let Some(value) = place.rebound_by(s) {
                    return !reads(value, var);
                }
                if stmt_reads(s, var) || leaves(s) {
                    return false;
                }
            }
            expr.as_deref().is_some_and(|t| rebinds_first(t, place))
        }
        IrExprKind::Match { subject, arms } => {
            !reads(subject, var)
                && !arms.is_empty()
                && arms.iter().all(|a| !a.guard.as_ref().is_some_and(|g| reads(g, var)) && rebinds_first(&a.body, place))
        }
        IrExprKind::If { cond, then, else_ } => !reads(cond, var) && rebinds_first(then, place) && rebinds_first(else_, place),
        _ => false,
    }
}

/// `let b = …; xs = b` (or `let t = …; let (r, b1, …) = t; xs = b1; …`):
/// the written-back vars, the destructured subject that may settle early,
/// and the index of the last write-back.
fn write_backs(
    stmts: &[IrStmt],
    tail: Option<&IrExpr>,
    i: usize,
    bound: VarId,
) -> Option<(Vec<VarId>, Option<(VarId, Vec<VarId>)>, usize)> {
    let (bufs, subject, first) = match stmts.get(i + 1).map(|s| &s.kind) {
        Some(IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value })
            if matches!(&value.kind, IrExprKind::Var { id } if *id == bound) =>
        {
            let binds: Vec<VarId> = elements
                .iter()
                .map(|p| match p {
                    IrPattern::Bind { var, .. } => Some(*var),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            (binds.clone(), Some((bound, binds)), i + 2)
        }
        _ => (vec![bound], None, i + 1),
    };
    let backs: Vec<(VarId, VarId)> = stmts[first..]
        .iter()
        .map_while(|s| match &s.kind {
            IrStmtKind::Assign { var, value } => match &value.kind {
                IrExprKind::Var { id } if bufs.contains(id) => Some((*var, *id)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if backs.is_empty() {
        return None;
    }
    // The subject settles early only when the destructure is its one read
    // and each written-back view is read once (by its write-back); the
    // binds it hands on are the ones NOT written back.
    let subject = subject.and_then(|(t, binds)| {
        let once = reads_in_block(stmts, tail, t) == 1
            && backs.iter().all(|&(_, b)| reads_in_block(stmts, tail, b) == 1);
        once.then(|| (t, binds.into_iter().filter(|b| !backs.iter().any(|&(_, w)| w == *b)).collect()))
    });
    Some((backs.iter().map(|&(v, _)| v).collect(), subject, first + backs.len() - 1))
}

/// The vars EVERY arm of a `match` value rebinds at its top level (the
/// carried err arm's `xs = b; err(e)!`).
fn arm_rebinds(e: &IrExpr) -> Vec<Place> {
    let e = unwrapped(e);
    let IrExprKind::Match { arms, .. } = &e.kind else { return Vec::new() };
    let assigned = |a: &IrMatchArm| -> Vec<Place> {
        match &a.body.kind {
            IrExprKind::Block { stmts, .. } => stmts
                .iter()
                .filter_map(|s| match &s.kind {
                    IrStmtKind::Assign { var, .. } => Some(Place::Var(*var)),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let mut arms = arms.iter();
    let Some(first) = arms.next() else { return Vec::new() };
    let mut vars = assigned(first);
    for a in arms {
        let these = assigned(a);
        vars.retain(|v| these.contains(v));
    }
    vars
}

fn unwrapped(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => unwrapped(expr),
        _ => e,
    }
}

/// The candidate calls of `e` with the vars each may take: the calls `e`
/// ends in, and a `match` subject call for the vars none of its arms reads.
/// With no `vars`, each candidate lists its own direct Var arguments.
fn exit_calls(e: &IrExpr, vars: &[Place], out: &mut Vec<(usize, Vec<Place>)>) {
    let take = |args: &[IrExpr]| -> Vec<Place> {
        if !vars.is_empty() {
            return vars.to_vec();
        }
        args.iter().filter_map(Place::of).collect()
    };
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Named { .. }, args, .. } => out.push((args.as_ptr() as usize, take(args))),
        IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => exit_calls(expr, vars, out),
        IrExprKind::Block { stmts, expr } => {
            if let Some(t) = expr {
                exit_calls(t, vars, out);
            }
            // A statement's call inside the value block, for the vars nothing
            // after it in the block (the tail included) reads (#3342: the
            // `let t = f(xs, …); match t { … }` a subject's naming builds).
            for (j, st) in stmts.iter().enumerate() {
                let Some(v) = stmt_value(st) else { continue };
                let mut inner = Vec::new();
                exit_calls(v, vars, &mut inner);
                let rest = &stmts[j + 1..];
                for (at, cand) in inner {
                    let free: Vec<Place> = cand
                        .into_iter()
                        .filter(|p| {
                            !rest.iter().any(|s| stmt_reads(s, p.root()) || leaves(s))
                                && !expr.as_deref().is_some_and(|t| reads(t, p.root()))
                        })
                        .collect();
                    if !free.is_empty() {
                        out.push((at, free));
                    }
                }
            }
        }
        IrExprKind::If { then, else_, .. } => {
            exit_calls(then, vars, out);
            exit_calls(else_, vars, out);
        }
        IrExprKind::Match { subject, arms } => {
            arms.iter().for_each(|a| exit_calls(&a.body, vars, out));
            if let IrExprKind::Call { target: CallTarget::Named { .. }, args, .. } = &unwrapped(subject).kind {
                let free: Vec<Place> = take(args)
                    .into_iter()
                    .filter(|p| !arms.iter().any(|a| a.guard.as_ref().is_some_and(|g| reads(g, p.root())) || reads_before_rebind(&a.body, *p)))
                    .collect();
                if !free.is_empty() {
                    out.push((args.as_ptr() as usize, free));
                }
            }
        }
        _ => {}
    }
}

/// Does `e` read `var` before rebinding it — an arm `{ xs = b; err((e, xs))! }`
/// rebinds first, so its later read sees the rebound block.
fn reads_before_rebind(e: &IrExpr, place: Place) -> bool {
    let var = place.root();
    let IrExprKind::Block { stmts, expr } = &e.kind else { return reads(e, var) };
    for s in stmts {
        if let Some(value) = place.rebound_by(s) {
            return reads(value, var);
        }
        if stmt_reads(s, var) || leaves(s) {
            return true;
        }
    }
    expr.as_deref().is_some_and(|t| reads(t, var))
}

/// The value a statement evaluates (its candidate calls' home).
fn stmt_value(s: &IrStmt) -> Option<&IrExpr> {
    match &s.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } | IrStmtKind::Assign { value, .. } => Some(value),
        IrStmtKind::Expr { expr } => Some(expr),
        _ => None,
    }
}

/// Does statement `s` read `var`?
fn stmt_reads(s: &IrStmt, var: VarId) -> bool {
    let mut f = Finder(var, false);
    f.visit_stmt(s);
    f.1
}

/// May statement `s` leave the block early — a `guard`, or a `break` /
/// `continue` not inside a loop or a lambda of its own?
fn leaves(s: &IrStmt) -> bool {
    struct Jump(bool);
    impl IrVisitor for Jump {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Break | IrExprKind::Continue => self.0 = true,
                IrExprKind::While { .. } | IrExprKind::ForIn { .. } | IrExprKind::Lambda { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    if matches!(s.kind, IrStmtKind::Guard { .. }) {
        return true;
    }
    let mut j = Jump(false);
    j.visit_stmt(s);
    j.0
}

/// Does `e` read `var`'s block — as a value, or as the target of an
/// in-place statement (`var[i] = …`, `var.f = …`, …)? Rebinding it
/// (`var = …`) is not a read.
fn reads(e: &IrExpr, var: VarId) -> bool {
    let mut f = Finder(var, false);
    f.visit_expr(e);
    f.1
}

struct Finder(VarId, bool);

impl IrVisitor for Finder {
    fn visit_expr(&mut self, e: &IrExpr) {
        if matches!(&e.kind, IrExprKind::Var { id } if *id == self.0) {
            self.1 = true;
        }
        if !self.1 {
            walk_expr(self, e);
        }
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        let hit = match &s.kind {
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. }
            | IrStmtKind::ListSwap { target, .. }
            | IrStmtKind::ListReverse { target, .. }
            | IrStmtKind::ListRotateLeft { target, .. }
            | IrStmtKind::RcInc { var: target }
            | IrStmtKind::RcDec { var: target } => *target == self.0,
            IrStmtKind::ListCopySlice { dst, src, .. } => *dst == self.0 || *src == self.0,
            _ => false,
        };
        self.1 |= hit;
        if !self.1 {
            walk_stmt(self, s);
        }
    }
}

