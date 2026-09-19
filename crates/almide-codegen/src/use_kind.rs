//! Use-kind analysis: every occurrence of every local in a body, tagged with
//! the syntactic position it sits in — the ONE walk the native ownership
//! passes read their facts from (#2186).
//!
//! `BorrowInsertion` asks "does any occurrence of this param consume it, or
//! hand it to a `&mut` slot?"; `CaptureClone` asks "which captures does this
//! closure write?" and "does this fold body only borrow its captures?";
//! `CloneInsertion` asks "how many times is this var mentioned?" and "does
//! this loop body only borrow its binder?". Each used to carry its own
//! recogniser over the tree, and the recognisers disagreed on which nodes
//! they descended into. Now there is one exhaustive walk — the shape of
//! `almide_ir::visit::walk_expr`, with every child tagged by the position its
//! parent puts it in — that records a [`Use`] per occurrence, and each pass
//! states its policy as a predicate over [`Site`]s. A new `IrExprKind` is a
//! compile error here, not a silently unclassified position.
//!
//! The walk classifies POSITIONS, not meanings: whether a `Result` position
//! "needs ownership" is the borrow pass's policy, whether an `Operand` counts
//! as a by-value read is the interpolation guard's — see the consumers. The
//! one classification every consumer shares is the call-slot mode, which the
//! caller supplies through a [`SlotOracle`]: before `BorrowInsertion` only the
//! signature table knows it; afterwards the IR spells every borrow, so a bare
//! argument is consumed ([`ExplicitBorrows`]).

use std::collections::{HashMap, HashSet};
use almide_base::intern::Sym;
use almide_ir::*;
use almide_lang::types::Ty;

#[path = "use_kind_types.rs"]
mod types;
pub use types::*;

/// The call-slot modes the walk cannot read off the tree.
pub trait SlotOracle {
    /// The mode of argument `index` of a `Call` / `TailCall` to `target`.
    fn call_slot(&self, target: &CallTarget, index: usize, arg: &IrExpr) -> SlotMode;
    /// The mode of argument `index` of a `RuntimeCall` to `symbol`.
    fn runtime_slot(&self, symbol: Sym, index: usize, arg: &IrExpr) -> SlotMode;
}

/// After `BorrowInsertion` the IR spells every borrow as a `Borrow` node
/// around the argument, so an argument that reaches the callee bare is
/// consumed.
pub struct ExplicitBorrows;

impl SlotOracle for ExplicitBorrows {
    fn call_slot(&self, _: &CallTarget, _: usize, _: &IrExpr) -> SlotMode { SlotMode::Consume }
    fn runtime_slot(&self, _: Sym, _: usize, _: &IrExpr) -> SlotMode { SlotMode::Consume }
}

/// Every occurrence of every local in one body, in evaluation order.
#[derive(Debug, Default)]
pub struct UseSites {
    uses: Vec<Use>,
    /// Parent arm of each arm id (`0` is the root and has no entry).
    arm_parent: HashMap<u32, u32>,
}

impl UseSites {
    /// The occurrences in `expr`, which sits in `root` position (a fn body
    /// is a [`Site::Result`]).
    pub fn of_expr(expr: &IrExpr, root: Site, oracle: &dyn SlotOracle) -> Self {
        let mut w = Walk::new(oracle);
        w.expr(expr, root);
        UseSites { uses: w.uses, arm_parent: w.arm_parent }
    }

    /// Can `later` still run once `earlier` has — is it in the same arm, an
    /// enclosing one, or a NESTED one (and after it in evaluation order)?
    /// Only an occurrence in a sibling branch is excluded: that is exactly
    /// the set the clone pass keeps a variable live for (`remaining` counts
    /// every later occurrence except those `deduct_sibling_uses` removes), so
    /// a consuming `earlier` with such a `later` is cloned, never moved. A
    /// guarded match arm is the nested case: the guard's fall-through
    /// re-tests the subject inside the arm's own block.
    pub fn keeps_live(&self, earlier: &Use, later: &Use) -> bool {
        later.arm == earlier.arm || self.encloses(later.arm, earlier.arm) || self.encloses(earlier.arm, later.arm)
    }

    /// Is `outer` a proper ancestor of `inner` in the arm tree?
    fn encloses(&self, outer: u32, inner: u32) -> bool {
        let mut a = inner;
        while let Some(&p) = self.arm_parent.get(&a) {
            if p == outer { return true; }
            a = p;
        }
        false
    }

    /// The occurrences in a statement list (a loop body).
    pub fn of_stmts(stmts: &[IrStmt], oracle: &dyn SlotOracle) -> Self {
        let mut w = Walk::new(oracle);
        for s in stmts { w.stmt(s); }
        UseSites { uses: w.uses, arm_parent: w.arm_parent }
    }

    /// The occurrences in a function body.
    pub fn of_fn(func: &IrFunction, oracle: &dyn SlotOracle) -> Self {
        Self::of_expr(&func.body, Site::Result, oracle)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Use> {
        self.uses.iter()
    }

    /// The occurrences of one variable.
    pub fn of(&self, var: VarId) -> impl Iterator<Item = &Use> {
        self.uses.iter().filter(move |u| u.var == var)
    }

    /// Does `var` occur as a `Var` node anywhere — closure bodies included?
    pub fn occurs(&self, var: VarId) -> bool {
        self.of(var).any(Use::is_node)
    }

    /// The syntactic count per variable: every `Var` node plus every
    /// in-place write target (a write reads-and-writes its container), the
    /// number `CloneInsertion`'s last-use countdown starts from. A plain
    /// reassignment names its target without reading it and is not counted.
    pub fn counts(&self) -> HashMap<VarId, u32> {
        let mut counts = HashMap::new();
        for u in &self.uses {
            if u.site != Site::Reassign {
                *counts.entry(u.var).or_insert(0) += 1;
            }
        }
        counts
    }

    /// The variables written directly: reassigned, mutated in place, or
    /// `&mut`-borrowed as a bare `Var`.
    pub fn written(&self) -> HashSet<VarId> {
        self.uses.iter().filter(|u| u.is_write(false)).map(|u| u.var).collect()
    }
}

struct Walk<'a> {
    oracle: &'a dyn SlotOracle,
    uses: Vec<Use>,
    depth: u32,
    in_chain: bool,
    mut_depth: u32,
    loop_depth: u32,
    outer_lambda: Option<u32>,
    stmt: u32,
    /// See [`Use::top_stmt`]: set by [`Walk::stmt`] while `nest <= 1` — the
    /// root block's own statements (or a bare statement list) — and
    /// inherited by every statement below them.
    top_stmt: u32,
    /// How many statement lists enclose the current position.
    nest: u32,
    arm: u32,
    next_arm: u32,
    arm_parent: HashMap<u32, u32>,
    /// The vars a direct `&v` argument of an enclosing call borrows (one
    /// set per enclosing call): the clone pass forces every other
    /// occurrence of those vars among that call's arguments to clone.
    guarded: Vec<HashSet<VarId>>,
    /// The vars an enclosing call, `match` or loop holds a borrow of across
    /// its operands (one set per such node) — see [`Use::held_across`].
    held: Vec<HashSet<VarId>>,
    /// The outermost fan arm being walked (see [`Use::fan_arm`]) and how many
    /// `held` sets were pushed outside its fan node.
    fan_arm: Option<(almide_base::span::Span, u32)>,
    held_fan_outside: usize,
    /// How many `held` sets were pushed OUTSIDE the outermost lambda being
    /// walked: only those hold across the closure's construction; a call
    /// inside the lambda body borrows when the closure runs, not when it is
    /// built.
    held_outside: usize,
}

impl<'a> Walk<'a> {
    fn new(oracle: &'a dyn SlotOracle) -> Self {
        Walk {
            oracle, uses: Vec::new(), depth: 0, in_chain: false, mut_depth: 0, loop_depth: 0,
            outer_lambda: None, stmt: 0, top_stmt: 0, nest: 0, arm: 0, next_arm: 0, arm_parent: HashMap::new(), guarded: Vec::new(),
            held: Vec::new(), held_outside: 0, fan_arm: None, held_fan_outside: 0,
        }
    }

    fn record(&mut self, var: VarId, site: Site, chain: Option<Chain>) {
        let guard_forced = self.guarded.iter().any(|g| g.contains(&var));
        let held_across = if self.depth > 0 {
            self.held[..self.held_outside].iter().any(|h| h.contains(&var))
        } else if self.fan_arm.is_some() {
            self.held[..self.held_fan_outside].iter().any(|h| h.contains(&var))
        } else {
            false
        };
        self.uses.push(Use {
            var, site, chain, depth: self.depth, in_chain: self.in_chain, in_mut: self.mut_depth > 0,
            in_loop: self.loop_depth > 0, outer_lambda: self.outer_lambda, stmt: self.stmt, top_stmt: self.top_stmt,
            arm: self.arm, guard_forced, held_across, fan_arm: self.fan_arm,
        });
    }

    /// Walk `e` in a fresh arm under the current one.
    fn arm_expr(&mut self, e: &IrExpr, site: Site) {
        let parent = self.arm;
        self.next_arm += 1;
        self.arm = self.next_arm;
        self.arm_parent.insert(self.arm, parent);
        self.expr(e, site);
        self.arm = parent;
    }

    /// Walk a statement list in a fresh arm under the current one.
    fn arm_stmts(&mut self, stmts: &[IrStmt]) {
        let parent = self.arm;
        self.next_arm += 1;
        self.arm = self.next_arm;
        self.arm_parent.insert(self.arm, parent);
        self.stmts(stmts);
        self.arm = parent;
    }

    /// Walk a statement list one nesting level down.
    fn stmts(&mut self, stmts: &[IrStmt]) {
        self.nest += 1;
        for s in stmts { self.stmt(s); }
        self.nest -= 1;
    }

    /// Advance the statement ordinal; a root-level statement also opens a
    /// new outermost statement (see [`Use::top_stmt`]).
    fn next_stmt(&mut self) {
        self.stmt += 1;
        if self.nest <= 1 {
            self.top_stmt = self.stmt;
        }
    }

    /// Walk a call's arguments in the order they EVALUATE once
    /// `hoist_conflicting_reads` has run: an argument that reads the variable
    /// a sibling `&mut` argument borrows is hoisted into a `let` before the
    /// call, so it runs before every argument that stays. The table must say
    /// so, or a consuming read in a staying argument looks earlier than a
    /// hoisted read that in fact precedes it — `map.insert(m, k,
    /// map.get_or(m, k, 0) + 1)` on `mut m` hoists the `get_or`, which reads
    /// `k` first; the `insert` then moves `k` as its LAST use.
    fn call_args(&mut self, args: &[IrExpr], modes: &[SlotMode]) {
        let mut_id = args.iter().zip(modes).find_map(|(a, m)| Self::mut_borrowed_var(a, *m));
        let hoisted = |i: usize| {
            mut_id.is_some_and(|v| Self::mut_borrowed_var(&args[i], modes[i]).is_none() && Self::reads(&args[i], v))
        };
        for (i, (a, mode)) in args.iter().zip(modes).enumerate() {
            if hoisted(i) { self.guarded_operand(a, *mode, Site::Arg(*mode)); }
        }
        for (i, (a, mode)) in args.iter().zip(modes).enumerate() {
            if !hoisted(i) { self.guarded_operand(a, *mode, Site::Arg(*mode)); }
        }
    }

    /// Walk one call operand under the call's guard set. The direct `&v`
    /// itself is not forced by its own guard — the clone pass leaves that
    /// borrow bare and clones the OTHER occurrences of `v` in the argument
    /// list — so its variable leaves the top set for the duration.
    fn guarded_operand(&mut self, e: &IrExpr, mode: SlotMode, site: Site) {
        // A lambda literal handed to a callee's NON-ESCAPING fn slot (#2288)
        // is a scope like a chain step: the callee only calls it, so what it
        // reads is borrowed for the call, not captured. Before
        // `BorrowInsertion` the slot mode says so; afterwards the `Borrow`
        // node around the lambda does (see the `Borrow` arm of `expr`).
        if mode == SlotMode::Borrow && matches!(e.kind, IrExprKind::Lambda { .. }) {
            return self.scope_lambda(e);
        }
        let own = Self::direct_borrow_of(e, mode).filter(|v| self.guarded.last().is_some_and(|g| g.contains(v)));
        if let Some(v) = own { self.guarded.last_mut().map(|g| g.remove(&v)); }
        self.expr(e, site);
        if let Some(v) = own { self.guarded.last_mut().map(|g| g.insert(v)); }
    }

    /// A lambda literal that is a SCOPE, not a closure: its body runs inside
    /// the call it is handed to (once per element for a chain step, as often
    /// as the callee calls it for a borrowed fn slot) and never outlives it,
    /// so `depth` stays 0 and `in_loop` holds.
    fn scope_lambda(&mut self, lambda: &IrExpr) {
        let IrExprKind::Lambda { body, .. } = &lambda.kind else { return self.expr(lambda, Site::Callback) };
        self.loop_depth += 1;
        self.arm_expr(body, Site::Result);
        self.loop_depth -= 1;
    }

    /// The variable `e` borrows directly as a call operand: a `Borrow { Var }`,
    /// or a bare `Var` at a `Borrow` / `Mut` slot.
    fn direct_borrow_of(e: &IrExpr, mode: SlotMode) -> Option<VarId> {
        match &e.kind {
            IrExprKind::Borrow { expr, .. } => match &expr.kind { IrExprKind::Var { id } => Some(*id), _ => None },
            IrExprKind::Var { id } if matches!(mode, SlotMode::Borrow | SlotMode::Mut) => Some(*id),
            _ => None,
        }
    }

    /// The variable a `&mut` argument borrows: a bare `Var` at a `Mut` slot
    /// before `BorrowInsertion`, an explicit mutable `Borrow` of one after.
    fn mut_borrowed_var(a: &IrExpr, mode: SlotMode) -> Option<VarId> {
        match &a.kind {
            IrExprKind::Var { id } if mode == SlotMode::Mut => Some(*id),
            IrExprKind::Borrow { expr, mutable: true, .. } => match &expr.kind {
                IrExprKind::Var { id } => Some(*id),
                _ => None,
            },
            _ => None,
        }
    }

    /// Does `arg` mention `var` anywhere (closure bodies included)?
    fn reads(arg: &IrExpr, var: VarId) -> bool {
        UseSites::of_expr(arg, Site::Operand, &ExplicitBorrows).occurs(var)
    }

    /// The variables a call holds a borrow of across its argument list — a
    /// `&v` or `&v.f` argument, a bare place at a `Borrow` / `Mut` slot, a
    /// method receiver — for [`Use::held_across`]. Broader than
    /// [`Self::direct_borrows`] (which mirrors the clone pass's E0505 guard
    /// exactly): a borrow through a field or index holds the root too.
    fn held_operands(args: &[IrExpr], modes: &[SlotMode], target: Option<&CallTarget>) -> HashSet<VarId> {
        let mut out = HashSet::new();
        for (a, mode) in args.iter().zip(modes) {
            let root = match &a.kind {
                IrExprKind::Borrow { .. } => borrow_root(a),
                _ if matches!(mode, SlotMode::Borrow | SlotMode::Mut) => place_root(a),
                _ => None,
            };
            out.extend(root);
        }
        if let Some(CallTarget::Method { object, .. }) = target { out.extend(place_root(object)); }
        out
    }

    /// The variables a call keeps borrowed through its argument list: a
    /// direct `&v` argument (the clone pass's guard set after
    /// `BorrowInsertion` spelled it), or a bare `v` handed to a slot the
    /// oracle says borrows (the same borrow before it is spelled).
    fn direct_borrows(args: &[IrExpr], modes: &[SlotMode], target: Option<&CallTarget>) -> HashSet<VarId> {
        let mut out = HashSet::new();
        let mut take = |e: &IrExpr, mode: SlotMode| {
            if let Some(id) = Self::direct_borrow_of(e, mode) { out.insert(id); }
        };
        for (a, mode) in args.iter().zip(modes) { take(a, *mode); }
        if let Some(CallTarget::Method { object, .. }) = target { take(object, SlotMode::Borrow); }
        out
    }

    /// A chain step / collector lambda: a scope that runs once per element
    /// (see `Use::depth`), its body an arm below the current one. Anything
    /// that is not a lambda literal (a stored closure value) is a callback.
    fn chain_lambda(&mut self, lambda: &IrExpr) {
        match &lambda.kind {
            IrExprKind::Lambda { body, .. } => {
                self.loop_depth += 1;
                self.arm_expr(body, Site::Result);
                self.loop_depth -= 1;
            }
            _ => self.expr(lambda, Site::Callback),
        }
    }

    /// Visit `e`, which sits in `site` position.
    fn expr(&mut self, e: &IrExpr, site: Site) {
        match &e.kind {
            IrExprKind::Var { id } => self.record(*id, site, None),

            // ── Leaves ──
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
            | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::FnRef { .. }
            | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::Break
            | IrExprKind::Continue | IrExprKind::Hole | IrExprKind::Todo { .. }
            | IrExprKind::RenderedCall { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::ClosureCreate { .. } => {}

            // ── Place projections root a chain ──
            IrExprKind::Member { object, .. } => {
                self.place(object, Site::Member, Chain { top: site, len: 1, heap: heap(&e.ty) })
            }
            IrExprKind::TupleIndex { object, .. } => {
                self.place(object, Site::TupleIndex, Chain { top: site, len: 1, heap: heap(&e.ty) })
            }
            IrExprKind::Deref { expr } => {
                self.place(expr, Site::Deref, Chain { top: site, len: 1, heap: heap(&e.ty) })
            }

            // ── One child, fixed position ──
            // `&(lambda)`: a lambda literal at a borrowed fn slot, spelled by
            // `BorrowInsertion` — a scope, not a closure (#2288).
            IrExprKind::Borrow { expr, mutable: false, .. } if matches!(expr.kind, IrExprKind::Lambda { .. }) => {
                self.scope_lambda(expr)
            }
            IrExprKind::Borrow { expr, mutable, .. } => {
                self.mut_depth += u32::from(*mutable);
                self.expr(expr, Site::Borrow { mutable: *mutable });
                self.mut_depth -= u32::from(*mutable);
            }
            IrExprKind::Clone { expr } => self.expr(expr, Site::Clone),
            IrExprKind::ResultOk { expr } => self.expr(expr, Site::Construct(Ctor::Ok)),
            IrExprKind::ResultErr { expr } => self.expr(expr, Site::Construct(Ctor::Err)),
            IrExprKind::OptionSome { expr } => self.expr(expr, Site::Construct(Ctor::Some)),
            IrExprKind::UnOp { operand: x, .. } | IrExprKind::OptionalChain { expr: x, .. }
            | IrExprKind::Try { expr: x } | IrExprKind::Unwrap { expr: x }
            | IrExprKind::ToOption { expr: x } | IrExprKind::BoxNew { expr: x }
            | IrExprKind::ToVec { expr: x } | IrExprKind::RcWrap { expr: x, .. } => {
                self.expr(x, Site::Operand)
            }
            IrExprKind::Lambda { body, lambda_id, .. } => {
                if self.depth == 0 { self.outer_lambda = *lambda_id; self.held_outside = self.held.len(); }
                self.depth += 1;
                self.arm_expr(body, Site::Result);
                self.depth -= 1;
                if self.depth == 0 { self.outer_lambda = None; }
            }

            // ── Two children ──
            IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } => {
                self.expr(left, Site::Concat);
                self.expr(right, Site::Concat);
            }
            // A comparison of two `String` places / literals is compared as
            // `&str` views by the clone pass (`pass_clone_compare::can_borrow`
            // — the same shape test): each operand is a read through a
            // reference. Every other comparison operand is an `Operand`.
            IrExprKind::BinOp { op, left: a, right: b }
                if matches!(op, BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte)
                    && a.ty == Ty::String && b.ty == Ty::String
                    && stable_string_operand(a) && stable_string_operand(b) =>
            {
                self.expr(a, Site::Compare);
                self.expr(b, Site::Compare);
            }
            IrExprKind::BinOp { left: a, right: b, .. } | IrExprKind::Range { start: a, end: b, .. } => {
                self.expr(a, Site::Operand);
                self.expr(b, Site::Operand);
            }
            IrExprKind::IndexAccess { object, index } => {
                self.expr(object, Site::Index);
                self.expr(index, Site::Operand);
            }
            IrExprKind::MapAccess { object, key } => {
                self.expr(object, Site::MapKeyed);
                self.expr(key, Site::Operand);
            }
            IrExprKind::UnwrapOr { expr, fallback } => {
                self.expr(expr, Site::Result);
                self.expr(fallback, Site::Result);
            }

            // ── Control flow ──
            IrExprKind::If { cond, then, else_ } => {
                self.expr(cond, Site::Operand);
                self.arm_expr(then, Site::Result);
                self.arm_expr(else_, Site::Result);
            }
            IrExprKind::Match { subject, arms } => {
                self.held.push(place_root(subject).into_iter().collect());
                self.expr(subject, Site::Scrutinee);
                for arm in arms {
                    let parent = self.arm;
                    self.next_arm += 1;
                    self.arm = self.next_arm;
                    self.arm_parent.insert(self.arm, parent);
                    self.pattern(&arm.pattern);
                    if let Some(g) = &arm.guard { self.expr(g, Site::Operand); }
                    self.expr(&arm.body, Site::Result);
                    self.arm = parent;
                }
                self.held.pop();
            }
            IrExprKind::Block { stmts, expr } => {
                self.stmts(stmts);
                if let Some(tail) = expr {
                    self.nest += 1;
                    self.next_stmt();
                    self.expr(tail, Site::Result);
                    self.nest -= 1;
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.held.push(borrow_root(iterable).into_iter().collect());
                let consumed = loop_elements_consumed(*var, var_tuple.as_deref(), &iterable.ty, body, self.oracle);
                self.expr(iterable, Site::Iterable { consumed });
                self.loop_depth += 1;
                self.arm_stmts(body);
                self.loop_depth -= 1;
                self.held.pop();
            }
            IrExprKind::While { cond, body } => {
                self.loop_depth += 1;
                let parent = self.arm;
                self.next_arm += 1;
                self.arm = self.next_arm;
                self.arm_parent.insert(self.arm, parent);
                self.expr(cond, Site::Operand);
                self.stmts(body);
                self.arm = parent;
                self.loop_depth -= 1;
            }

            // ── Calls ──
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                let modes: Vec<SlotMode> = args.iter().enumerate().map(|(i, a)| self.oracle.call_slot(target, i, a)).collect();
                self.guarded.push(Self::direct_borrows(args, &modes, Some(target)));
                self.held.push(Self::held_operands(args, &modes, Some(target)));
                match target {
                    CallTarget::Method { object, .. } => self.guarded_operand(object, SlotMode::Borrow, Site::Receiver),
                    CallTarget::Computed { callee } => self.expr(callee, Site::Callee),
                    CallTarget::Named { .. } | CallTarget::Module { .. } => {}
                }
                self.call_args(args, &modes);
                self.held.pop();
                self.guarded.pop();
            }
            IrExprKind::RuntimeCall { symbol, args } => {
                let modes: Vec<SlotMode> = args.iter().enumerate().map(|(i, a)| self.oracle.runtime_slot(*symbol, i, a)).collect();
                self.guarded.push(Self::direct_borrows(args, &modes, None));
                self.held.push(Self::held_operands(args, &modes, None));
                self.call_args(args, &modes);
                self.held.pop();
                self.guarded.pop();
            }

            // ── Constructors ──
            IrExprKind::List { elements } => self.each(elements, Site::Construct(Ctor::List)),
            IrExprKind::Tuple { elements } => self.each(elements, Site::Construct(Ctor::Tuple)),
            // A fan's arms are implicit move closures (#2239): at lambda depth
            // 0 the OUTERMOST fan gives each arm an identity — the fan node's
            // span and the arm's index — so the capture-move rule can see which
            // arm holds a variable's last occurrence. Arms are NOT arms of the
            // arm tree (every one of them runs), so `keeps_live` is untouched.
            IrExprKind::Fan { exprs } => self.fan(e.span, exprs),
            IrExprKind::RustMacro { args, .. } => self.each(args, Site::Operand),
            IrExprKind::Record { fields, .. } => self.fields(fields, Site::Construct(Ctor::Record)),
            IrExprKind::InlineRust { args, .. } => self.fields(args, Site::Operand),
            // FIELDS BEFORE THE BASE — the order the emitted Rust runs them
            // (`T { f: e, ..base }` evaluates each written field, then the
            // base), and this walk's order is what `cloned_anyway` reads as
            // "a later occurrence still follows this one". Walking the base
            // first made `Token { ...t, start: t.start - lead }` look like a
            // move that a later read has to clone around, so the chain's
            // source was left a borrow and the base rendered `..t` on a `&T`
            // that rustc rejects (#2315). The base is the LAST thing the
            // record literal touches; nothing follows it, and the element is
            // simply consumed — which is also the cheaper answer, since an
            // owned source moves the remaining fields out with no clone at all.
            IrExprKind::SpreadRecord { base, fields } => {
                self.fields(fields, Site::Construct(Ctor::SpreadField));
                self.expr(base, Site::Construct(Ctor::SpreadBase));
            }
            IrExprKind::MapLiteral { entries } => {
                for (k, v) in entries {
                    self.expr(k, Site::Construct(Ctor::MapKey));
                    self.expr(v, Site::Construct(Ctor::MapValue));
                }
            }
            IrExprKind::StringInterp { parts } => {
                for p in parts {
                    if let IrStringPart::Expr { expr } = p { self.expr(expr, Site::Construct(Ctor::Interp)); }
                }
            }
            IrExprKind::IterChain { source, consume, steps, collector } => {
                let outer = std::mem::replace(&mut self.in_chain, true);
                let consumed = *consume && chain_elements_consumed(&source.ty, steps, collector, self.oracle);
                self.expr(source, Site::Iterable { consumed });
                for step in steps {
                    match step {
                        IterStep::Map { lambda } | IterStep::Filter { lambda }
                        | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                            self.chain_lambda(lambda)
                        }
                        IterStep::Take { n } => self.expr(n, Site::Operand),
                        IterStep::Enumerate => {}
                    }
                }
                match collector {
                    IterCollector::Collect | IterCollector::Sum { .. } | IterCollector::Len => {}
                    IterCollector::Fold { init, lambda } => {
                        self.expr(init, Site::FoldInit);
                        self.chain_lambda(lambda);
                    }
                    IterCollector::Any { lambda } | IterCollector::All { lambda }
                    | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
                        self.chain_lambda(lambda)
                    }
                }
                self.in_chain = outer;
            }
        }
    }

    /// The object of a projection: extend the chain through nested
    /// projections, record the root when it is a `Var`, or classify any other
    /// object as an ordinary child in `site` position.
    fn place(&mut self, object: &IrExpr, site: Site, chain: Chain) {
        let deeper = Chain { len: chain.len + 1, ..chain };
        match &object.kind {
            IrExprKind::Var { id } => self.record(*id, site, Some(chain)),
            IrExprKind::Member { object: inner, .. } => self.place(inner, Site::Member, deeper),
            IrExprKind::TupleIndex { object: inner, .. } => self.place(inner, Site::TupleIndex, deeper),
            IrExprKind::Deref { expr: inner } => self.place(inner, Site::Deref, deeper),
            _ => self.expr(object, site),
        }
    }

    /// The arms of a `fan` (see the `Fan` case of [`Self::expr`]).
    fn fan(&mut self, span: Option<almide_base::span::Span>, exprs: &[IrExpr]) {
        let (Some(span), true) = (span, self.depth == 0 && self.fan_arm.is_none()) else {
            return self.each(exprs, Site::Construct(Ctor::Fan));
        };
        self.held_fan_outside = self.held.len();
        for (i, arm) in exprs.iter().enumerate() {
            self.fan_arm = Some((span, i as u32));
            self.expr(arm, Site::Construct(Ctor::Fan));
        }
        self.fan_arm = None;
    }

    fn each(&mut self, exprs: &[IrExpr], site: Site) {
        for e in exprs { self.expr(e, site); }
    }

    fn fields(&mut self, fields: &[(Sym, IrExpr)], site: Site) {
        for (_, e) in fields { self.expr(e, site); }
    }

    fn pattern(&mut self, p: &IrPattern) {
        match p {
            IrPattern::Wildcard | IrPattern::Bind { .. } | IrPattern::None => {}
            IrPattern::Literal { expr } => self.expr(expr, Site::Operand),
            IrPattern::Constructor { args, .. } => { for a in args { self.pattern(a); } }
            IrPattern::RecordPattern { fields, .. } => {
                for f in fields {
                    if let Some(p) = &f.pattern { self.pattern(p); }
                }
            }
            IrPattern::Tuple { elements } => { for e in elements { self.pattern(e); } }
            IrPattern::List { elements, rest } => {
                for e in elements { self.pattern(e); }
                if let Some(r) = rest { self.pattern(r); }
            }
            IrPattern::As { inner, .. } | IrPattern::Some { inner } | IrPattern::Ok { inner }
            | IrPattern::Err { inner } => self.pattern(inner),
        }
    }

    fn stmt(&mut self, s: &IrStmt) {
        self.next_stmt();
        match &s.kind {
            IrStmtKind::Bind { value, .. } => self.expr(value, Site::Assigned),
            IrStmtKind::BindDestructure { pattern, value } => {
                self.pattern(pattern);
                self.expr(value, Site::Assigned);
            }
            IrStmtKind::Assign { var, value } => {
                self.record(*var, Site::Reassign, None);
                self.expr(value, Site::Assigned);
            }
            IrStmtKind::FieldAssign { target, value, .. } => {
                self.record(*target, Site::InPlace, None);
                self.expr(value, Site::Assigned);
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                self.record(*target, Site::InPlace, None);
                self.expr(index, Site::Operand);
                self.expr(value, Site::Assigned);
            }
            IrStmtKind::MapInsert { target, key, value } => {
                self.record(*target, Site::InPlace, None);
                self.expr(key, Site::Operand);
                self.expr(value, Site::Assigned);
            }
            IrStmtKind::ListSwap { target, a, b } => {
                self.record(*target, Site::InPlace, None);
                self.expr(a, Site::Operand);
                self.expr(b, Site::Operand);
            }
            IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => {
                self.record(*target, Site::InPlace, None);
                self.expr(end, Site::Operand);
            }
            IrStmtKind::ListCopySlice { dst, src, len } => {
                self.record(*dst, Site::InPlace, None);
                self.record(*src, Site::Operand, None);
                self.expr(len, Site::Operand);
            }
            IrStmtKind::Guard { cond, else_ } => {
                self.expr(cond, Site::Operand);
                self.expr(else_, Site::Operand);
            }
            IrStmtKind::Expr { expr } => self.expr(expr, Site::Operand),
            IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => self.record(*var, Site::Operand, None),
            IrStmtKind::Comment { .. } => {}
        }
    }
}

/// The variables `expr` writes directly: reassigned, mutated in place, or
/// `&mut`-borrowed as a bare `Var` (the form `list.push(v, …)` takes after
/// `BorrowInsertion`). Shared by `CaptureClone` (a capture the closure writes
/// keeps its bare bind so the shared-cell wiring sees it) and `StreamFusion`
/// (a chain whose callback writes its source cannot borrow it, #2098).
pub fn written_vars(expr: &IrExpr) -> HashSet<VarId> {
    UseSites::of_expr(expr, Site::Operand, &ExplicitBorrows).written()
}

/// Is a value of this type cloned rather than copied — the copy-ness
/// classifier every clone decision derives from (#531).
fn heap(ty: &Ty) -> bool {
    !almide_ir::top_let_storage::clone_free(ty)
}

/// A `String` place or literal a comparison can read as a `&str` view —
/// `pass_clone_compare::stable`, restated here so the walk classifies the
/// operand the way the clone pass will spell it.
fn stable_string_operand(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Var { .. } | IrExprKind::LitStr { .. } => true,
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => stable_string_operand(object),
        _ => false,
    }
}

/// The element type of a `List[T]` iterable, `None` for anything else.
fn list_element_ty(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(almide_lang::types::TypeConstructorId::List, args) => args.first(),
        _ => None,
    }
}

/// Does the body of an iteration only READ the element it binds — so the
/// iteration can walk a borrow of its source (`.iter()`) and the source is
/// never needed owned? A `Copy` element is read whatever the body does with
/// it (the iteration copies it out). A heap element is read when every
/// occurrence, at closure depth 0 and outside any `&mut`, is a borrowed call
/// slot, a shared borrow, a clone, a field read whose value is not itself
/// moved on, a nested borrowed iteration, or — for a `String` — an
/// interpolation part or a `&str` comparison. Anything else (returned,
/// concatenated, built into a value, handed to an owned slot, bound to a
/// local, matched, captured, mutated) demands the element owned, and the
/// borrow verdict owns the SOURCE for it: an owned source moves the element
/// out for free where a borrowed one would clone it per element.
///
/// This is the rule the reference-counted compilers apply per element (a
/// list read borrows into the list and the element is retained only when an
/// occurrence demands it); here the demand is lifted to the source's mode,
/// because on the native leg a borrowed source can only clone. Shared by the
/// borrow verdict (pre-`BorrowInsertion`, through the signature oracle), the
/// clone pass (post, `ExplicitBorrows`) and the ownership certifier, so the
/// three read one rule.
///
/// `fields_move` says whether a heap field of the binder CAN be moved out
/// when the source is owned: a chain lambda's param is fresh per element
/// and its fields move (so a field moved on demands the element); a `for`
/// binder's fields are never moved out (`LoopMarks::binders` — the binder
/// may be `&T`), so a field read is a read whatever position it feeds.
pub fn element_reads_only(uses: &UseSites, var: VarId, ty: &Ty, fields_move: bool) -> bool {
    if !heap(ty) {
        return true;
    }
    let is_string = matches!(ty, Ty::String);
    let all: Vec<&Use> = uses.of(var).collect();
    // A consuming occurrence that a later occurrence of the binder can still
    // follow in the same iteration is CLONED there by the clone pass (the
    // binder stays live), as is one the E0505 call guard forces: ownership
    // of the element buys it nothing, and a `&T` binder clones the same once
    // (`param_borrow`'s `cloned_anyway`, applied per element).
    let cloned_anyway = |u: &Use| u.guard_forced
        || all.iter().any(|w| !std::ptr::eq(*w, u) && (*w as *const Use) > (u as *const Use) && uses.keeps_live(u, w));
    all.iter().all(|u| {
        if u.depth > 0 || u.in_mut || u.is_write(true) {
            return false;
        }
        let reads = match u.site {
            Site::Arg(SlotMode::Borrow) | Site::Borrow { mutable: false } | Site::Clone
            | Site::Iterable { consumed: false } => true,
            // A field read: the field's own position decides. A heap field
            // moved on (returned, concatenated, built into a value) would
            // move out of a `&T` binder — E0507 — so it demands the element.
            Site::Member => match u.chain {
                Some(c) => !(fields_move && c.heap && element_top_consumes(c.top)),
                None => true,
            },
            Site::Construct(Ctor::Interp) | Site::Compare => is_string,
            _ => false,
        };
        reads || (u.site != Site::Arg(SlotMode::Mut) && cloned_anyway(u))
    })
}

/// Does a projection chain's top position move the projected value?
fn element_top_consumes(top: Site) -> bool {
    matches!(
        top,
        Site::Result | Site::Scrutinee | Site::Concat | Site::Construct(_) | Site::Receiver
            | Site::Callback | Site::FoldInit | Site::Arg(SlotMode::Consume | SlotMode::Mut)
            | Site::Iterable { consumed: true } | Site::Assigned | Site::Borrow { mutable: true }
    )
}

/// Does a fused chain need its source's elements OWNED — is some lambda the
/// source element reaches not a pure read of it, or does the element leave
/// the chain as a value (`source_element_receivers`)?
pub fn chain_elements_consumed(source_ty: &Ty, steps: &[IterStep], collector: &IterCollector, oracle: &dyn SlotOracle) -> bool {
    let Some(elem) = list_element_ty(source_ty) else { return true };
    let Some(receivers) = almide_ir::source_element_receivers(steps, collector) else { return true };
    receivers.iter().any(|(binder, lambda)| {
        let IrExprKind::Lambda { body, .. } = &lambda.kind else { return true };
        !element_reads_only(&UseSites::of_expr(body, Site::Result, oracle), *binder, elem, true)
    })
}

/// Does a `for` loop need its iterable's elements OWNED? A destructuring
/// binder (a `Map` loop's pairs, a tuple list) and a non-`List` iterable
/// always do; a plain `List` binder only when its body is not a pure read.
pub fn loop_elements_consumed(var: VarId, var_tuple: Option<&[VarId]>, iterable_ty: &Ty, body: &[IrStmt], oracle: &dyn SlotOracle) -> bool {
    let Some(elem) = list_element_ty(iterable_ty) else { return true };
    if var_tuple.is_some() {
        return true;
    }
    !element_reads_only(&UseSites::of_stmts(body, oracle), var, elem, false)
}

/// The variable a place expression reads — through fields, tuple indices,
/// indexes, keys, derefs and shared borrows.
fn place_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::IndexAccess { object, .. } | IrExprKind::MapAccess { object, .. }
        | IrExprKind::Deref { expr: object } | IrExprKind::Borrow { expr: object, .. } => place_root(object),
        _ => None,
    }
}

/// The variable a `Borrow` node holds; `None` for a by-value expression.
fn borrow_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Borrow { expr, .. } => place_root(expr),
        _ => None,
    }
}
