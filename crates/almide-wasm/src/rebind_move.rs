//! #3661: a var bound from a source that DIES at the bind takes the
//! source's credit instead of sharing it.
//!
//! `var a = acc` in a fold callback, or `var a = s` before a self tail call,
//! shared the block (+1) although nothing reads the source again. The first
//! in-place write on `a` — `list.push`, `bytes.set_u8`, a string append, any
//! mutator that judges `rc == 1` — then met the block at rc 2 and copied it
//! whole, so a loop of such writes stayed quadratic. Where the bind is the
//! source's last read, the bind now MOVES the credit: no share, the source's
//! local is emptied (a later release of it is a release of NULL), and the
//! write finds the block unique. One rule for every mutator, decided from
//! liveness alone — no list of operations.
//!
//! **Liveness.** [`dying_rebinds`] walks a frame body backwards in
//! structure: a block statement's continuation is the statements after it,
//! the block's tail and the block's own continuation; a branch's is its
//! `if` / `match`'s; a child of any other node also has its siblings (in
//! either order, so no evaluation order is assumed). A loop body's
//! continuation adds every var the loop mentions (the next iteration), and
//! a lambda body's every var the body mentions (the next call) — minus the
//! vars declared inside, which are rebound before they are read again. A
//! bind whose source is in none of that dies there.
//!
//! **Views.** A match binding, a destructured `let` and a loop variable are
//! views into their subject's block, which an in-place write on the moved
//! block would change under them. A source any match subject, destructure
//! or loop iterable of the frame mentions is never moved.
//!
//! **Ownership.** Only a source whose credit the frame holds moves
//! (`dying_local`): an owned local, an owned param, a fold accumulator. A
//! param the frame only borrows (param_borrow.rs) is made owned when its
//! last read is a `var` rebind, so the caller hands its credit over instead
//! of lending the block the rebind would copy. The target must be a plain
//! local, not a C-319 cell — which a var a lambda binds and writes itself no
//! longer is (cells.rs): only a var a lambda captures from outside is.
//!
//! Value semantics hold by count: a source read after the bind does not die
//! there and shares as before, and a block another holder still has is at
//! rc > 1 when the write judges it, so it is copied.

use std::collections::{HashMap, HashSet};

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrPattern, IrStmt, IrStmtKind, Mutability, VarId};

use crate::emitter::Emitter;
use crate::SliceTy;

type Counts = HashMap<VarId, u32>;

/// How often each tracked var is mentioned (read, or written through) in a
/// node: `Var` reads and the targets of the statements that write a var.
struct Mentions<'t> {
    tracked: &'t HashSet<VarId>,
    counts: Counts,
}

impl Mentions<'_> {
    fn note(&mut self, v: VarId) {
        if self.tracked.contains(&v) {
            *self.counts.entry(v).or_default() += 1;
        }
    }
}

impl IrVisitor for Mentions<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let IrExprKind::Var { id } = &e.kind {
            self.note(*id);
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Assign { var, .. }
            | IrStmtKind::IndexAssign { target: var, .. }
            | IrStmtKind::MapInsert { target: var, .. }
            | IrStmtKind::FieldAssign { target: var, .. }
            | IrStmtKind::ListSwap { target: var, .. }
            | IrStmtKind::ListReverse { target: var, .. }
            | IrStmtKind::ListRotateLeft { target: var, .. }
            | IrStmtKind::RcInc { var }
            | IrStmtKind::RcDec { var } => self.note(*var),
            IrStmtKind::ListCopySlice { dst, src, .. } => {
                self.note(*dst);
                self.note(*src);
            }
            _ => {}
        }
        walk_stmt(self, s);
    }
}

fn counts_expr(e: &IrExpr, tracked: &HashSet<VarId>) -> Counts {
    let mut m = Mentions { tracked, counts: Counts::new() };
    m.visit_expr(e);
    m.counts
}

fn counts_stmts(stmts: &[IrStmt], tracked: &HashSet<VarId>) -> Counts {
    let mut m = Mentions { tracked, counts: Counts::new() };
    stmts.iter().for_each(|s| m.visit_stmt(s));
    m.counts
}

fn keys(c: &Counts) -> HashSet<VarId> {
    c.keys().copied().collect()
}

/// The candidates, and the vars a view is taken of: every bind of a whole
/// var (`let a = v` / `var a = v`), and every var any match subject,
/// destructure or loop iterable mentions.
#[derive(Default)]
struct Survey {
    sources: HashSet<VarId>,
    viewed: HashSet<VarId>,
}

impl Survey {
    fn view(&mut self, root: &IrExpr) {
        struct Vars<'s>(&'s mut HashSet<VarId>);
        impl IrVisitor for Vars<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Var { id } = &e.kind {
                    self.0.insert(*id);
                }
                walk_expr(self, e);
            }
        }
        Vars(&mut self.viewed).visit_expr(root);
    }
}

impl IrVisitor for Survey {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Match { subject, .. } => self.view(subject),
            IrExprKind::ForIn { iterable, .. } => self.view(iterable),
            _ => {}
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Bind { value, .. } if let IrExprKind::Var { id } = &value.kind => {
                self.sources.insert(*id);
            }
            IrStmtKind::BindDestructure { value, .. } => self.view(value),
            _ => {}
        }
        walk_stmt(self, s);
    }
}

/// Every var a loop / lambda body declares (each is rebound before it is
/// read again on the next pass).
struct Declared<'t> {
    tracked: &'t HashSet<VarId>,
    out: HashSet<VarId>,
}

impl Declared<'_> {
    fn note(&mut self, v: VarId) {
        if self.tracked.contains(&v) {
            self.out.insert(v);
        }
    }
}

impl IrVisitor for Declared<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.note(*var);
                var_tuple.iter().flatten().for_each(|v| self.note(*v));
            }
            IrExprKind::Lambda { params, .. } => params.iter().for_each(|(v, _)| self.note(*v)),
            _ => {}
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Bind { var, .. } = &s.kind {
            self.note(*var);
        }
        walk_stmt(self, s);
    }
    fn visit_pattern(&mut self, p: &IrPattern) {
        match p {
            IrPattern::Bind { var, .. } | IrPattern::As { var, .. } => self.note(*var),
            _ => {}
        }
        almide_ir::visit::walk_pattern(self, p);
    }
}

/// How the node being visited learns what is live after it.
enum Ctx {
    /// Set by the parent for exactly this child.
    Given(HashSet<VarId>),
    /// A child of a node with no special order: the parent's continuation
    /// plus whatever its OTHER children mention (`all` minus this child's).
    Siblings { live: HashSet<VarId>, all: Counts },
}

struct Liveness<'t, F: FnMut(&IrExpr, Mutability)> {
    tracked: &'t HashSet<VarId>,
    /// The frame's C-319 cells: a bind into one shares (its cell holds the
    /// occupant), so it is never a move.
    cells: &'t HashSet<VarId>,
    ctx: Vec<Ctx>,
    found: F,
}

impl<F: FnMut(&IrExpr, Mutability)> Liveness<'_, F> {
    fn given(&mut self, live: HashSet<VarId>, e: &IrExpr) {
        self.ctx.push(Ctx::Given(live));
        self.visit_expr(e);
        self.ctx.pop();
    }

    /// A statement list run in order, `after` live once it is done.
    fn stmts(&mut self, stmts: &[IrStmt], after: &HashSet<VarId>) {
        let mut live = after.clone();
        let mut lives = Vec::with_capacity(stmts.len());
        for s in stmts.iter().rev() {
            lives.push(live.clone());
            live.extend(keys(&counts_stmts(std::slice::from_ref(s), self.tracked)));
        }
        for (s, live) in stmts.iter().zip(lives.into_iter().rev()) {
            self.stmt(s, live);
        }
    }

    fn stmt(&mut self, s: &IrStmt, live: HashSet<VarId>) {
        if let IrStmtKind::Bind { var, value, mutability, .. } = &s.kind
            && let IrExprKind::Var { id } = &value.kind
            && self.tracked.contains(id)
            && !self.cells.contains(var)
            && !live.contains(id)
        {
            (self.found)(value, *mutability);
        }
        let all = counts_stmts(std::slice::from_ref(s), self.tracked);
        self.ctx.push(Ctx::Siblings { live, all });
        walk_stmt(self, s);
        self.ctx.pop();
    }

    /// The continuation of a loop / lambda body: its own mentions (the
    /// next pass) and `outer`, minus what the body declares.
    fn repeated(&self, outer: &HashSet<VarId>, whole: &IrExpr, body: &IrExpr) -> HashSet<VarId> {
        let mut decl = Declared { tracked: self.tracked, out: HashSet::new() };
        decl.visit_expr(body);
        let mut live = outer.clone();
        live.extend(keys(&counts_expr(whole, self.tracked)));
        live.retain(|v| !decl.out.contains(v));
        live
    }

    fn expr(&mut self, e: &IrExpr, live: HashSet<VarId>) {
        let union = |a: &HashSet<VarId>, b: Counts| a.iter().copied().chain(b.into_keys()).collect::<HashSet<_>>();
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                let after = match expr {
                    Some(t) => union(&live, counts_expr(t, self.tracked)),
                    None => live.clone(),
                };
                self.stmts(stmts, &after);
                if let Some(t) = expr {
                    self.given(live, t);
                }
            }
            IrExprKind::If { cond, then, else_ } => {
                let mut c = union(&live, counts_expr(then, self.tracked));
                c.extend(keys(&counts_expr(else_, self.tracked)));
                self.given(c, cond);
                self.given(live.clone(), then);
                self.given(live, else_);
            }
            IrExprKind::Match { subject, arms } => {
                let mut rest = live.clone();
                for a in arms {
                    rest.extend(keys(&counts_expr(&a.body, self.tracked)));
                    if let Some(g) = &a.guard {
                        rest.extend(keys(&counts_expr(g, self.tracked)));
                    }
                }
                self.given(rest.clone(), subject);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.given(rest.clone(), g);
                    }
                    self.given(live.clone(), &a.body);
                }
            }
            IrExprKind::ForIn { iterable, body, .. } | IrExprKind::While { cond: iterable, body } => {
                let next = self.repeated(&live, e, e);
                self.given(union(&live, counts_expr(e, self.tracked)), iterable);
                self.stmts(body, &next);
            }
            IrExprKind::Lambda { params, body, .. } => {
                let mut next = self.repeated(&live, body, body);
                params.iter().for_each(|(v, _)| {
                    next.remove(v);
                });
                self.given(next, body);
            }
            _ => {
                let all = counts_expr(e, self.tracked);
                self.ctx.push(Ctx::Siblings { live, all });
                walk_expr(self, e);
                self.ctx.pop();
            }
        }
    }
}

impl<F: FnMut(&IrExpr, Mutability)> IrVisitor for Liveness<'_, F> {
    fn visit_expr(&mut self, e: &IrExpr) {
        let here = counts_expr(e, self.tracked);
        if here.is_empty() {
            return;
        }
        let live = match self.ctx.last() {
            Some(Ctx::Given(l)) => l.clone(),
            Some(Ctx::Siblings { live, all }) => {
                let mut l = live.clone();
                l.extend(all.iter().filter(|(v, n)| here.get(v).is_none_or(|m| *n > m)).map(|(v, _)| *v));
                l
            }
            None => HashSet::new(),
        };
        self.expr(e, live);
    }
    fn visit_pattern(&mut self, _: &IrPattern) {}
}

/// Every `let` / `var` bind of a whole var in `body` at which that var
/// dies, with the bind's mutability: the read node (the bind's value).
pub(crate) fn dying_rebinds(body: &IrExpr, mut found: impl FnMut(&IrExpr, Mutability)) {
    let mut survey = Survey::default();
    survey.visit_expr(body);
    let tracked: HashSet<VarId> = survey.sources.difference(&survey.viewed).copied().collect();
    if tracked.is_empty() {
        return;
    }
    let cells = crate::cells::cell_vars_of(body);
    let mut l = Liveness { tracked: &tracked, cells: &cells, ctx: Vec::new(), found: &mut found };
    l.visit_expr(body);
}

impl Emitter<'_> {
    /// Note the frame body's dying whole-var binds (`dying_local` takes the
    /// note at the bind).
    pub(crate) fn note_dying_rebinds(&mut self, body: &IrExpr) {
        if self.metered {
            return;
        }
        let marks = &mut self.owned_call_marks;
        dying_rebinds(body, |read, _| marks.set_dying(read, true));
    }

    /// The source the bind of `value` into a plain `declared` local moves
    /// out of — its var and local — when the bind is its noted last read
    /// and the frame holds its credit. `None`: the bind shares as before.
    pub(crate) fn take_rebind_move(&mut self, value: &IrExpr, declared: SliceTy) -> Option<(VarId, u32)> {
        if !matches!(value.kind, IrExprKind::Var { .. }) || !self.owned_call_marks.is_dying(value) {
            return None;
        }
        let (id, idx, ty) = self.dying_local(value)?;
        (ty == declared).then_some((id, idx))
    }

    /// The moved source no longer holds the block: the new local does.
    pub(crate) fn empty_rebound(&mut self, dst: u32, src: u32) {
        self.f.instructions().i32_const(0).local_set(src);
        if let Some(w) = self.witness.as_mut() {
            if !w.bind_moved(dst, src) {
                w.poison();
            }
            w.empty_local(src);
        }
    }
}
