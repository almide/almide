//! ClonePass: insert Clone IR nodes for heap-type variables in Rust.
//!
//! **Last-use optimization**: tracks remaining uses per variable.
//! At the final use of a variable, ownership is transferred (move) instead of cloning.
//! Inside loops, clones are always inserted (the body executes multiple times).
//! At branches (if/match), remaining counts are merged conservatively (min).

use std::collections::{HashSet, HashMap};
use std::rc::Rc;
use almide_ir::*;
use almide_base::{Span, Sym};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};
use super::pass_clone_places::{insert_clones_index_access, insert_clones_map_access, insert_clones_member};
use super::pass_clone_loops::{insert_clones_for_in, insert_clones_while, take_borrowed_loop_vars};

#[path = "pass_clone_calls.rs"]
mod calls;
use calls::{insert_clones_call, insert_clones_runtime_call};

#[derive(Debug)]
pub struct CloneInsertionPass;

impl NanoPass for CloneInsertionPass {
    fn name(&self) -> &str { "CloneInsertion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowInsertion"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        for f in &mut program.functions { super::pass_clone_projection::fold_bindings(&mut f.body); }
        for m in &mut program.modules {
            for f in &mut m.functions { super::pass_clone_projection::fold_bindings(&mut f.body); }
        }
        compute_use_counts(&mut program);
        let top_let_vars: HashSet<VarId> = program.top_lets.iter().map(|tl| tl.var).collect();

        // Compute syntactic counts (no loop/lambda bumps) for remaining tracking
        let syntactic = SyntacticCounts::of(&program.functions, &program.top_lets);

        let marks = CloneMarks {
            always: program.codegen_annotations.always_clone_vars.clone(),
            tco_owned: program.codegen_annotations.tco_owned_params.clone(),
            tco_fns: program.codegen_annotations.tco_rewritten_fns.clone(),
        };
        let sets = ClassSets::split(&program.var_table, &top_let_vars, &syntactic.total, &marks);
        rewrite_bodies(&mut program.functions, &mut program.top_lets, &syntactic, &sets, &marks);

        let IrProgram { modules, var_table, .. } = &mut program;
        for module in modules.iter_mut() {
            let module_top_lets: HashSet<VarId> = module.top_lets.iter().map(|tl| tl.var).collect();
            let module_syntactic = SyntacticCounts::of(&module.functions, &module.top_lets);
            let m_sets = ClassSets::split(var_table, &module_top_lets, &module_syntactic.total, &marks);
            rewrite_bodies(&mut module.functions, &mut module.top_lets, &module_syntactic, &m_sets, &marks);
        }
        // Loop binders the bodies only borrowed (#1673): the walker binds them
        // `&T` off `xs.iter()`.
        program.codegen_annotations.borrowed_loop_vars = take_borrowed_loop_vars();
        program.codegen_annotations.consumed_loop_vars = super::pass_clone_loops::take_consumed_loop_vars();
        PassResult { program, changed: true }
    }
}

// ── Syntactic use-count (no loop/lambda bumps) ─────────────────────

/// The syntactic counts of one function group (the program's own fns +
/// top-lets, or one module's), kept BOTH as the group-wide total and per
/// body (#1232).
///
/// `total` is what classifies a var (`split_clone_ids`) and seeds its
/// `remaining` count — deliberately group-wide, not per body: a VarId can
/// live in two bodies (`branch_lift` helpers keep the enclosing fn's ids as
/// their params), and such a var's last-use count must span both so neither
/// body moves it. The per-body maps only say WHICH ids a body mentions, so
/// the rewrite walk can narrow its tracking to those — see [`BodyScope`].
struct SyntacticCounts {
    total: HashMap<VarId, u32>,
    /// One map per `functions[i]`, in order.
    fn_bodies: Vec<HashMap<VarId, u32>>,
    /// One map per `top_lets[i]`, in order.
    top_let_bodies: Vec<HashMap<VarId, u32>>,
}

impl SyntacticCounts {
    fn of(functions: &[IrFunction], top_lets: &[IrTopLet]) -> SyntacticCounts {
        let mut total = HashMap::new();
        let count = |body: &IrExpr, total: &mut HashMap<VarId, u32>| {
            let mut counts = HashMap::new();
            count_syntactic(body, &mut counts);
            for (id, n) in &counts {
                *total.entry(*id).or_insert(0) += n;
            }
            counts
        };
        let fn_bodies = functions.iter().map(|f| count(&f.body, &mut total)).collect();
        let top_let_bodies = top_lets.iter().map(|tl| count(&tl.value, &mut total)).collect();
        SyntacticCounts { total, fn_bodies, top_let_bodies }
    }
}

/// The annotation sets earlier passes left for this one.
struct CloneMarks {
    always: HashSet<VarId>,
    tco_owned: HashSet<VarId>,
    tco_fns: HashSet<Sym>,
}

/// The `always` / `eligible` classification of one function group, in both
/// flavours: with the TCO-owned exemption (for the bodies TailCallOpt
/// rewrote, and every top-let) and without it.
struct ClassSets {
    always: HashSet<VarId>,
    eligible: HashSet<VarId>,
    always_plain: HashSet<VarId>,
    eligible_plain: HashSet<VarId>,
}

impl ClassSets {
    fn split(vt: &VarTable, top_let_vars: &HashSet<VarId>, syntactic: &HashMap<VarId, u32>, marks: &CloneMarks) -> ClassSets {
        let (always, eligible) = split_clone_ids(vt, top_let_vars, syntactic, &marks.always, &marks.tco_owned);
        // #1130: the TCO exemption holds ONLY inside the body TailCallOpt
        // rewrote. A VarId can live in another function too — `branch_lift`
        // lifts an in-loop branch into a helper whose params ARE the
        // enclosing fn's vars — and there the compensating clone plan does
        // not exist, so its bare moves were a rustc E0382. Everything else
        // gets the ordinary last-use analysis.
        let no_exempt: HashSet<VarId> = HashSet::new();
        let (always_plain, eligible_plain) = split_clone_ids(vt, top_let_vars, syntactic, &marks.always, &no_exempt);
        ClassSets { always, eligible, always_plain, eligible_plain }
    }

    /// The pair a function body walks under: the exempting flavour inside a
    /// TCO-rewritten body, the plain one everywhere else.
    fn for_fn(&self, tco_here: bool) -> (&HashSet<VarId>, &HashSet<VarId>) {
        if tco_here { (&self.always, &self.eligible) } else { (&self.always_plain, &self.eligible_plain) }
    }
}

/// The classification narrowed to ONE body (#1232): the walk over a body
/// only ever tests, decrements, deducts or min-merges ids that body mentions
/// (a `Var` node or an in-place-mutation target — exactly what
/// `SyntacticCounter` records), so dropping every other id from the three
/// maps changes nothing it emits. It changes what the maps cost: `remaining`
/// used to hold every eligible id of the whole program, and every If/Match
/// node cloned it once per branch and min-merged it over the full eligible
/// set — proportional to the program, not to the function.
struct BodyScope {
    always: HashSet<VarId>,
    eligible: HashSet<VarId>,
    /// Seeded from the GROUP-wide count (see [`SyntacticCounts`]), so an id
    /// shared with another body still counts that body's uses.
    remaining: HashMap<VarId, u32>,
}

impl BodyScope {
    fn narrow(mentioned: &HashMap<VarId, u32>, always: &HashSet<VarId>, eligible: &HashSet<VarId>, total: &HashMap<VarId, u32>) -> BodyScope {
        let mut scope = BodyScope { always: HashSet::new(), eligible: HashSet::new(), remaining: HashMap::new() };
        for &id in mentioned.keys() {
            if always.contains(&id) {
                scope.always.insert(id);
            }
            if eligible.contains(&id) {
                scope.eligible.insert(id);
                scope.remaining.insert(id, total.get(&id).copied().unwrap_or(0));
            }
        }
        scope
    }
}

/// Rewrite every body of one function group under its own [`BodyScope`].
fn rewrite_bodies(functions: &mut [IrFunction], top_lets: &mut [IrTopLet], syntactic: &SyntacticCounts, sets: &ClassSets, marks: &CloneMarks) {
    for (func, mentioned) in functions.iter_mut().zip(&syntactic.fn_bodies) {
        let (always, eligible) = sets.for_fn(marks.tco_fns.contains(&func.name));
        let owned = func.params.iter().filter(|p| p.borrow == ParamBorrow::Own).map(|p| p.var).collect();
        func.body = rewrite_body(std::mem::take(&mut func.body), mentioned, always, eligible, &syntactic.total, owned, &marks.tco_owned);
    }
    for (tl, mentioned) in top_lets.iter_mut().zip(&syntactic.top_let_bodies) {
        tl.value = rewrite_body(std::mem::take(&mut tl.value), mentioned, &sets.always, &sets.eligible, &syntactic.total, HashSet::new(), &marks.tco_owned);
    }
}

fn rewrite_body(body: IrExpr, mentioned: &HashMap<VarId, u32>, always: &HashSet<VarId>, eligible: &HashSet<VarId>, total: &HashMap<VarId, u32>, mut owned: HashSet<VarId>, tco_owned: &HashSet<VarId>) -> IrExpr {
    owned.extend(almide_ir::free_vars::bound_vars(&body));
    // TCO manages these moves itself and exempts them from our use counts.
    owned.retain(|v| !tco_owned.contains(v));
    let mut scope = BodyScope::narrow(mentioned, always, eligible, total);
    // #1230: any id the branch walk can deduct lives in `remaining`, whose
    // key set is exactly `scope.eligible` — so the branch-count memo only
    // needs to track that set. Counts for other ids are deduct no-ops.
    let memo = BranchCounts::compute(&body, &scope.eligible);
    // Nothing is fresh at function top level — see `CloneCtx::fresh`.
    let no_fresh: HashSet<VarId> = HashSet::new();
    insert_clones_live(body, &mut CloneCtx {
        always: &scope.always,
        eligible: &scope.eligible,
        remaining: &mut scope.remaining,
        in_loop: false,
        memo: &memo,
        fresh: &no_fresh,
        owned: &owned,
    })
}

/// Counts every syntactic `Var` use by riding the exhaustive `IrVisitor` walk —
/// so no node kind (incl. `IterChain`/`RcWrap`/`TailCall`, present here because
/// StreamFusion/TCO run before this pass) can silently drop a subtree and
/// under-count a var, which would desync the `remaining` last-use tracking.
struct SyntacticCounter<'a> {
    counts: &'a mut HashMap<VarId, u32>,
}

impl IrVisitor for SyntacticCounter<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Var { id } = &expr.kind {
            *self.counts.entry(*id).or_insert(0) += 1;
        }
        walk_expr(self, expr); // exhaustive recursion into all children
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        // An in-place mutation `a[i]=v` / `a.f=v` / `a[k]=v` reads-and-writes `a`,
        // but the target is a bare `VarId` field — NOT a `Var` expr node — so the
        // expr walk above never sees it. Count it explicitly: this makes the
        // mutation a *use* of `a`, so when an alias `var b = a` precedes it, the
        // bind is no longer `a`'s last use → the eligible-move path clones at the
        // bind instead of moving, and the later in-place write operates on owned
        // `a` (not a moved value → no E0382). Without this, shapes B/C/I above
        // emit `let mut b = a;`/`a.clone(); f(a);` then mutate the moved `a`.
        match &stmt.kind {
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => {
                *self.counts.entry(*target).or_insert(0) += 1;
            }
            _ => {}
        }
        walk_stmt(self, stmt); // exhaustive recursion into the stmt's expr children
    }
}

fn count_syntactic(expr: &IrExpr, counts: &mut HashMap<VarId, u32>) {
    SyntacticCounter { counts }.visit_expr(expr);
}

// ── Branch-count memo (#1230) ──────────────────────────────────────
//
// `insert_clones_if` / `insert_clones_match` need the syntactic use-counts of
// every sibling branch BEFORE walking a branch (see `deduct_sibling_uses`).
// Counting them with a fresh `count_syntactic` at every If/Match node re-walks
// the full remaining subtree per level — O(k²) on a k-arm else-if ladder. This
// prepass computes all of those maps in ONE bottom-up walk of the body and the
// walk looks them up instead.
//
// Keying: branch subtrees are memoized by their node's heap address — an If's
// `then`/`else_` are `Box<IrExpr>` targets, a match arm's `body` is an element
// of the `Vec<IrMatchArm>` buffer. Both stay at fixed addresses while their
// owner structs move by value (`std::mem::take` of the body, `*then` box
// derefs, the `arms` Vec moving out of the parent kind), and the rewrite walk
// looks a node up top-down at the moment it reaches it — always a still-alive
// original subtree, never a rebuilt output node — so a key can neither dangle
// nor collide with a reused allocation. A miss is still CORRECT, not just
// slow: `BranchCounts::branch`/`arm` fall back to the fresh count the code
// always did, so behavior never depends on a hit.
//
// Counts are restricted to `tracked` (union of the eligible sets): deduction
// only ever touches ids present in `remaining`, whose key set is an eligible
// set, so dropping untracked ids is behavior-neutral and keeps the per-branch
// maps (and the absorb cost of merging them upward) small.
pub(crate) struct BranchCounts {
    map: HashMap<*const IrExpr, Rc<HashMap<VarId, u32>>>,
}

impl BranchCounts {
    fn compute(body: &IrExpr, tracked: &HashSet<VarId>) -> BranchCounts {
        let mut pc = BranchPrecounter { tracked, memo: HashMap::new(), acc: HashMap::new() };
        pc.visit_expr(body);
        BranchCounts { map: pc.memo }
    }

    /// Counts for one If branch subtree (`then` or `else_`).
    fn branch(&self, node: &IrExpr) -> Rc<HashMap<VarId, u32>> {
        if let Some(c) = self.map.get(&std::ptr::from_ref(node)) {
            return Rc::clone(c);
        }
        let mut c = HashMap::new();
        count_syntactic(node, &mut c);
        Rc::new(c)
    }

    /// Combined guard+body counts for one match arm (keyed by the body node),
    /// mirroring what `insert_clones_match` used to count per arm.
    fn arm(&self, arm: &IrMatchArm) -> Rc<HashMap<VarId, u32>> {
        if let Some(c) = self.map.get(&std::ptr::from_ref(&arm.body)) {
            return Rc::clone(c);
        }
        let mut c = HashMap::new();
        if let Some(g) = &arm.guard {
            count_syntactic(g, &mut c);
        }
        count_syntactic(&arm.body, &mut c);
        Rc::new(c)
    }
}

/// The bottom-up counting visitor behind [`BranchCounts::compute`]. Rides the
/// same exhaustive `IrVisitor` walk as `SyntacticCounter` (same +1 per `Var`
/// node, same in-place-mutation target bump), except at If/Match: their branch
/// children are counted into a fresh accumulator via [`Self::count_branch`],
/// memoized, then absorbed into the enclosing accumulator — each node is
/// visited exactly once for the whole body.
struct BranchPrecounter<'a> {
    tracked: &'a HashSet<VarId>,
    memo: HashMap<*const IrExpr, Rc<HashMap<VarId, u32>>>,
    acc: HashMap<VarId, u32>,
}

impl BranchPrecounter<'_> {
    fn bump(&mut self, id: VarId) {
        if self.tracked.contains(&id) {
            *self.acc.entry(id).or_insert(0) += 1;
        }
    }

    /// Run `count` against a fresh accumulator, memoize the result under
    /// `key`, and add it back into the surrounding accumulator so the parent
    /// subtree's total still includes this branch.
    fn count_branch(&mut self, key: *const IrExpr, count: impl FnOnce(&mut Self)) {
        let outer = std::mem::take(&mut self.acc);
        count(self);
        let counts = std::mem::replace(&mut self.acc, outer);
        for (id, n) in &counts {
            *self.acc.entry(*id).or_insert(0) += n;
        }
        self.memo.insert(key, Rc::new(counts));
    }
}

impl IrVisitor for BranchPrecounter<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Var { id } => self.bump(*id),
            IrExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                self.count_branch(std::ptr::from_ref(&**then), |s| s.visit_expr(then));
                self.count_branch(std::ptr::from_ref(&**else_), |s| s.visit_expr(else_));
                return; // children fully visited above — walk_expr would double-count
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                for arm in arms {
                    // Pattern counts belong to the ENCLOSING accumulator, not
                    // the arm map: `insert_clones_match` counts guard+body only.
                    self.visit_pattern(&arm.pattern);
                    self.count_branch(std::ptr::from_ref(&arm.body), |s| {
                        if let Some(g) = &arm.guard {
                            s.visit_expr(g);
                        }
                        s.visit_expr(&arm.body);
                    });
                }
                return; // children fully visited above
            }
            // Every other node kind delegates to the exhaustive primitive —
            // the traversal-totality lint forbids a silent `_ => {}` (DIV2:
            // a dropped-children catch-all is the native↔wasm divergence class).
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        // Same in-place-mutation target accounting as `SyntacticCounter`.
        match &stmt.kind {
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => self.bump(*target),
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

// ── Clone ID classification ────────────────────────────────────────

pub(super) fn needs_clone(ty: &Ty) -> bool {
    // §4 stage 2c (#531): derived from THE copy-ness classifier — see the
    // projection table in almide_ir::top_let_storage.
    !almide_ir::top_let_storage::clone_free(ty)
}

/// Split clone candidates into "always clone" and "eligible for last-use move".
fn split_clone_ids(
    vt: &VarTable,
    top_let_vars: &HashSet<VarId>,
    syntactic: &HashMap<VarId, u32>,
    always_clone_marks: &HashSet<VarId>,
    tco_owned_params: &HashSet<VarId>,
) -> (HashSet<VarId>, HashSet<VarId>) {
    let mut always = HashSet::new();
    let mut eligible = HashSet::new();

    for i in 0..vt.len() {
        let id = VarId(i as u32);
        let info = vt.get(id);
        if !needs_clone(&info.ty) { continue; }
        // A TCO loop param whose clone/move decisions the TailCallOpt pass
        // made itself: every consuming read is already explicitly Clone-
        // wrapped or a deliberate bare move at a provably-final read. In
        // NEITHER set, its remaining bare reads fall through as moves —
        // the in-loop always-clone rule here is what made tail-recursive
        // list/string accumulators O(n²) on native (wasm ran O(n)).
        if tco_owned_params.contains(&id) { continue; }

        let _name = almide_base::intern::resolve(info.name);
        if top_let_vars.contains(&id) || matches!(&info.ty, Ty::Fn { .. } | Ty::TypeVar(_))
            || always_clone_marks.contains(&id)
            || info.module_origin.is_some()
        {
            // `module_origin` marks a module top-let Var (decl side set in
            // lower/mod.rs, use side in lower/expressions.rs) whose Rust
            // storage is a `static LazyLock<T>` rendered by the walker as
            // `(*ALMIDE_RT_<MOD>_<NAME>)`. A static can never be moved
            // from, so every consuming use must clone — the same `always`
            // class as same-file `top_let_vars`. The use site allocates a
            // fresh VarId with a clean name, so neither the set lookup nor
            // any name prefix can catch it.
            always.insert(id);
        } else {
            let syn = syntactic.get(&id).copied().unwrap_or(0);
            if syn > 1 {
                // Multiple syntactic uses → eligible for last-use optimization
                eligible.insert(id);
            } else if info.use_count > 1 {
                // Single syntactic use but bumped (loop/lambda) → always clone
                always.insert(id);
            }
            // syn <= 1 && use_count <= 1: single use, no loop → move by default
        }
    }
    (always, eligible)
}

// ── Clone insertion with last-use tracking ─────────────────────────

/// Bundles the four values threaded unchanged through every recursive call
/// in `insert_clones_live` / `insert_clone_stmts_live` (and their arm
/// helpers), so each fn stays at or under the `max-params` limit. `in_loop`
/// flips to `true` for a nested loop body/cond — built as a fresh `CloneCtx`
/// reborrowing `remaining` (same shape as `HoistCtx` in pass_licm_hoist.rs).
pub(crate) struct CloneCtx<'a> {
    /// Value bindings and explicitly owned parameters; excludes borrowed parameters.
    pub(crate) owned: &'a HashSet<VarId>,
    pub(crate) always: &'a HashSet<VarId>,
    pub(crate) eligible: &'a HashSet<VarId>,
    pub(crate) remaining: &'a mut HashMap<VarId, u32>,
    pub(crate) in_loop: bool,
    /// Per-body branch-count memo (#1230) — see [`BranchCounts`].
    pub(crate) memo: &'a BranchCounts,
    /// Vars rebound on EVERY iteration of the innermost enclosing loop — the
    /// loop's own binders and the `let`s at its body's top level (#1673).
    /// Their last use in that body is a move even though `in_loop` holds:
    /// the next iteration binds a fresh value. Empty outside loops and
    /// inside lambda bodies (a closure may run many times per iteration).
    pub(crate) fresh: &'a HashSet<VarId>,
}

fn make_clone(id: VarId, ty: Ty, span: Option<Span>) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Clone {
            expr: Box::new(IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None }),
        },
        ty, span, def_id: None,
    }
}

/// `Var { id }` arm of [`insert_clones_live`] — the core decision point for
/// clone-vs-move on a variable reference. Merges the two former match-guard
/// arms (`always`/`eligible`); an id tracked by neither falls through
/// unchanged, same as the exhaustive `other` catch-all's no-op on a
/// childless node.
fn insert_clones_var(id: VarId, ty: Ty, span: Option<Span>, ctx: &mut CloneCtx) -> IrExpr {
    if ctx.always.contains(&id) {
        // Still a syntactic use: an id here may be an ELIGIBLE var only
        // temporarily forced into `always` by the E0505 call guard (its
        // borrow/move occurrences inside one call). Without the decrement,
        // the occurrence is invisible to the last-use count and every LATER
        // use of the var stops qualifying as a move — a spurious clone on
        // the next statement's genuinely-final use.
        if let Some(r) = ctx.remaining.get_mut(&id) {
            *r = r.saturating_sub(1);
        }
        return make_clone(id, ty, span);
    }
    if ctx.eligible.contains(&id) {
        if let Some(r) = ctx.remaining.get_mut(&id) {
            *r = r.saturating_sub(1);
            if *r == 0 && (!ctx.in_loop || ctx.fresh.contains(&id)) {
                // Last use outside a loop — or of a var the loop rebinds
                // every iteration (#1673) — → move (no clone)
                return IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None };
            }
        }
        return make_clone(id, ty, span);
    }
    IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None }
}

/// Deduct a sibling branch's syntactic uses from `remaining` on entry to a
/// branch (#1143): `remaining` is a whole-fn count, so without this a use in
/// one branch still counts the mutually-exclusive sibling's uses — the true
/// last use on a path can never reach 0 and every branch tail pays a spurious
/// clone (`step`'s fold-accumulator `map.set(acc, …)` cloned the Map per
/// line). Counting is the same `SyntacticCounter` that built `remaining`, so
/// deductions stay 1:1 with the decrements the branch walk will perform.
fn deduct_sibling_uses(remaining: &mut HashMap<VarId, u32>, sibling: &HashMap<VarId, u32>) {
    for (id, n) in sibling {
        if let Some(r) = remaining.get_mut(id) {
            *r = r.saturating_sub(*n);
        }
    }
}

/// `If { cond, then, else_ }` arm of [`insert_clones_live`]: save/restore/min
/// for branches (the branch that consumed more `remaining` wins — conservative),
/// with the sibling branch's uses deducted on entry so a path's genuine last
/// use can move.
fn insert_clones_if(cond: Box<IrExpr>, then: Box<IrExpr>, else_: Box<IrExpr>, ctx: &mut CloneCtx) -> IrExprKind {
    // Memo lookups must happen while `then`/`else_` are still behind their
    // original Boxes — the box target address is the memo key (#1230).
    let then_counts = ctx.memo.branch(&then);
    let else_counts = ctx.memo.branch(&else_);
    let new_cond = insert_clones_live(*cond, ctx);
    let saved = ctx.remaining.clone();
    deduct_sibling_uses(ctx.remaining, &else_counts);
    let new_then = insert_clones_live(*then, ctx);
    let then_remaining = std::mem::replace(ctx.remaining, saved);
    deduct_sibling_uses(ctx.remaining, &then_counts);
    let new_else = insert_clones_live(*else_, ctx);
    for &id in ctx.eligible.iter() {
        let t = then_remaining.get(&id).copied().unwrap_or(0);
        let e = ctx.remaining.get(&id).copied().unwrap_or(0);
        ctx.remaining.insert(id, t.min(e));
    }
    IrExprKind::If {
        cond: Box::new(new_cond),
        then: Box::new(new_then),
        else_: Box::new(new_else),
    }
}

/// `Match { subject, arms }` arm of [`insert_clones_live`]: same save/min
/// strategy as [`insert_clones_if`], generalized to N arms, with every
/// sibling arm's uses deducted on entry (see [`deduct_sibling_uses`]).
fn insert_clones_match(subject: IrExpr, arms: Vec<IrMatchArm>, ctx: &mut CloneCtx) -> IrExprKind {
    let owned_final = super::pass_clone_projection::root(&subject).is_some_and(|id|
        ctx.owned.contains(&id) && !ctx.always.contains(&id) && !ctx.in_loop
            && ctx.remaining.get(&id).copied().unwrap_or(1) <= 1);
    let borrowed = if owned_final { None } else {
        super::pass_clone_projection::match_binders(&subject, &arms)
    };
    let subject = if borrowed.is_some() {
        let mut subject = subject;
        if let IrExprKind::RuntimeCall { symbol, .. } = &mut subject.kind {
            *symbol = almide_base::intern::sym("almide_list_get_ref!");
            subject
        } else {
            IrExpr { ty: subject.ty.clone(), span: subject.span, def_id: None,
                kind: IrExprKind::Borrow { expr: Box::new(subject), as_str: false, mutable: false } }
        }
    } else { subject };
    let new_subject = insert_clones_live(subject, ctx);
    // Memo lookups must happen before `arms.into_iter()` moves the arms out of
    // the Vec buffer — the body's buffer address is the memo key (#1230).
    let arm_counts: Vec<Rc<HashMap<VarId, u32>>> =
        arms.iter().map(|arm| ctx.memo.arm(arm)).collect();
    let mut total_counts: HashMap<VarId, u32> = HashMap::new();
    for c in &arm_counts {
        for (id, n) in c.iter() {
            *total_counts.entry(*id).or_insert(0) += n;
        }
    }
    let saved = ctx.remaining.clone();
    let mut min_remaining = HashMap::new();
    let mut new_arms = Vec::with_capacity(arms.len());

    for (i, arm) in arms.into_iter().enumerate() {
        *ctx.remaining = saved.clone();
        // siblings = total - own, computed elementwise BEFORE the saturating
        // deduction so saturation can't distort the difference. Read straight
        // off the two maps — no per-arm copy of `total_counts` (#1232).
        for (id, total) in &total_counts {
            let own = arm_counts[i].get(id).copied().unwrap_or(0);
            if let Some(r) = ctx.remaining.get_mut(id) {
                *r = r.saturating_sub(total - own);
            }
        }
        let owned: HashSet<_> = ctx.owned.iter().copied()
            .filter(|v| !borrowed.as_ref().is_some_and(|vars| vars.contains(v))).collect();
        let mut arm_ctx = CloneCtx { owned: &owned, always: ctx.always, eligible: ctx.eligible,
            remaining: ctx.remaining, in_loop: ctx.in_loop, memo: ctx.memo, fresh: ctx.fresh };
        let new_guard = arm.guard.map(|g| insert_clones_live(g, &mut arm_ctx));
        let new_body = insert_clones_live(arm.body, &mut arm_ctx);
        new_arms.push(IrMatchArm { pattern: arm.pattern, guard: new_guard, body: new_body });

        if i == 0 {
            min_remaining = ctx.remaining.clone();
        } else {
            for &id in ctx.eligible.iter() {
                let cur = ctx.remaining.get(&id).copied().unwrap_or(0);
                let prev = min_remaining.get(&id).copied().unwrap_or(0);
                min_remaining.insert(id, cur.min(prev));
            }
        }
    }
    *ctx.remaining = min_remaining;
    IrExprKind::Match { subject: Box::new(new_subject), arms: new_arms }
}

pub(crate) fn insert_clones_live(mut expr: IrExpr, ctx: &mut CloneCtx) -> IrExpr {
    if super::pass_clone_compare::rewrite(&mut expr, ctx) { return expr; }
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Var { id } => return insert_clones_var(id, ty, span, ctx),

        // ── Block: sequential statements ───────────────────────────
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: insert_clone_stmts_live(stmts, ctx),
            expr: expr.map(|e| Box::new(insert_clones_live(*e, ctx))),
        },

        IrExprKind::If { cond, then, else_ } => insert_clones_if(cond, then, else_, ctx),
        IrExprKind::Match { subject, arms } => insert_clones_match(*subject, arms, ctx),
        IrExprKind::ForIn { var, var_tuple, iterable, body } => insert_clones_for_in(var, var_tuple, *iterable, body, ctx),
        IrExprKind::While { cond, body } => insert_clones_while(*cond, body, ctx),

        IrExprKind::Call { target, args, type_args } => insert_clones_call(target, args, type_args, ctx),
        IrExprKind::RuntimeCall { symbol, args } => {
            let args = insert_clones_runtime_call(args, ctx);
            IrExprKind::RuntimeCall { symbol, args }
        }

        IrExprKind::StringInterp { parts } => super::pass_clone_interp::insert_clones_string_interp(parts, ctx),
        IrExprKind::IndexAccess { object, index } => return insert_clones_index_access(*object, *index, ty, span, ctx),
        IrExprKind::MapAccess { object, key } => return insert_clones_map_access(*object, *key, ty, span, ctx),

        IrExprKind::Member { object, field } => return insert_clones_member(*object, field, ty, span, ctx),
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: super::pass_clone_record_fields::rewrite(fields, ctx),
        },
        IrExprKind::SpreadRecord { base, fields } => {
            // Fields are evaluated before the spread base in Rust struct literals
            let new_fields: Vec<_> = fields.into_iter().map(|(k, v)| (k, insert_clones_live(v, ctx))).collect();
            let new_base = insert_clones_live(*base, ctx);
            IrExprKind::SpreadRecord { base: Box::new(new_base), fields: new_fields }
        },
        IrExprKind::Clone { expr } => {
            let mut inner = insert_clones_live(*expr, ctx);
            // An existing clone already preserves ownership. The liveness
            // walk still consumes the use, but must not add a second clone.
            if let IrExprKind::Clone { expr: unwrapped } = inner.kind {
                inner = *unwrapped;
            }
            IrExprKind::Clone { expr: Box::new(inner) }
        },
        IrExprKind::Borrow { expr, as_str, mutable } => {
            let mut inner = insert_clones_live(*expr, ctx);
            // Strip clone inside borrow: &x.clone() → &x (borrow doesn't consume ownership)
            if let IrExprKind::Clone { expr: unwrapped } = inner.kind {
                inner = *unwrapped;
            }
            IrExprKind::Borrow { expr: Box::new(inner), as_str, mutable }
        },
        // A closure body may run any number of times per loop iteration, so
        // nothing the enclosing loop rebinds is fresh inside it (#1673): walk
        // the lambda with an empty `fresh` set, otherwise unchanged.
        kind @ IrExprKind::Lambda { .. } => {
            let no_fresh: HashSet<VarId> = HashSet::new();
            let e = IrExpr { kind, ty: ty.clone(), span, def_id: None };
            // Captured values belong to the reusable closure. Only values
            // introduced inside this invocation may have fields moved out.
            let owned = almide_ir::free_vars::bound_vars(&e);
            let mut lam_ctx = CloneCtx { always: ctx.always, eligible: ctx.eligible, remaining: ctx.remaining, in_loop: ctx.in_loop, memo: ctx.memo, fresh: &no_fresh, owned: &owned };
            return e.map_children(&mut |child| insert_clones_live(child, &mut lam_ctx));
        }
        // Default: recurse into every child through the exhaustive `map_children`
        // chokepoint. Every node whose clone insertion is just "recurse into the
        // children, left to right" lands here — BinOp/UnOp/Lambda/
        // UnwrapOr/Range/MapLiteral/BoxNew included: their former hand-written
        // arms were byte-for-byte what `map_children` does for the same kind — `map_children` visits them in
        // exactly that order, which is what the liveness countdown needs — so no
        // un-listed node kind (`IterChain`/`RcWrap`/`TailCall`/future variants)
        // silently drops its subtree — that was the DIV2-sibling
        // (clone insertion blind to closures fused inside a chain). Leaf kinds have
        // no children and pass through unchanged.
        other => {
            let e = IrExpr { kind: other, ty: ty.clone(), span, def_id: None };
            return e.map_children(&mut |child| insert_clones_live(child, ctx));
        }
    };

    IrExpr { kind, ty, span, def_id: None }
}

/// Account an in-place-mutation `target` as a use, mirroring the +1 that
/// `SyntacticCounter::visit_stmt` recorded. Only `eligible` (last-use-move) vars
/// track `remaining`; `always`/move-by-default vars don't appear there.
fn count_target_use(target: VarId, eligible: &HashSet<VarId>, remaining: &mut HashMap<VarId, u32>) {
    if eligible.contains(&target) {
        if let Some(r) = remaining.get_mut(&target) {
            *r = r.saturating_sub(1);
        }
    }
}

pub(crate) fn insert_clone_stmts_live(stmts: Vec<IrStmt>, ctx: &mut CloneCtx) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty, value: insert_clones_live(value, ctx),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: insert_clones_live(value, ctx) },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: insert_clones_live(expr, ctx) },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: insert_clones_live(cond, ctx), else_: insert_clones_live(else_, ctx),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern, value: insert_clones_live(value, ctx),
            },
            // In-place mutations: process the sub-exprs first (they may consume
            // vars), THEN account the target as a use of `target` itself —
            // `count_target_use` decrements `remaining[target]` to match the +1
            // that `SyntacticCounter::visit_stmt` added, keeping last-use tracking
            // consistent for any later use of `target`. The target binding is NOT
            // cloned/moved (the statement writes through it in place); this is a
            // pure counter decrement.
            IrStmtKind::IndexAssign { target, index, value } => {
                let index = insert_clones_live(index, ctx);
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::IndexAssign { target, index, value }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::FieldAssign { target, field, value }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                let key = insert_clones_live(key, ctx);
                let value = insert_clones_live(value, ctx);
                count_target_use(target, ctx.eligible, ctx.remaining);
                IrStmtKind::MapInsert { target, key, value }
            }
            // Default: recurse every expr child via the exhaustive `map_exprs`
            // chokepoint so no un-listed stmt kind (`ListSwap`/`ListReverse`/… —
            // which `count_syntactic` already counts) drops its expr subtree.
            other => IrStmt { kind: other, span: s.span }
                .map_exprs(&mut |e| insert_clones_live(e, ctx))
                .kind,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
