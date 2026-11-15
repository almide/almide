//! Per-node ownership marks, and the guarantee that makes them sound
//! (#3143).
//!
//! A mark says "this node's lowering handed its consumer one credit" (a
//! module call's owned result, an `if` / `??` whose arms were normalized,
//! an extraction that released its carrier, …). The reader holds only the
//! node, so the mark is keyed by the node's ADDRESS — and an address is an
//! identity only while its node is alive. The emitter lowers nodes it
//! builds itself (a map literal's pairs list, a `m[k]`'s `map.get`
//! arguments) and used to drop them when done: the next node allocated at
//! a freed address inherited the dead one's mark. Under a LIFO free list
//! the literal rebuilt right after its twin was dropped lands there every
//! time, so a borrowed `if` read as an owned join and freed a list a
//! binding still held (#3139); whether it fired depended on the allocator.
//!
//! The class is closed by scope, not by forgetting: a mark is accepted
//! only for a node that outlives this table. [`NodeMarks::register`] takes
//! a tree the emitter borrows for its whole life (the fn body, a top-let
//! initializer), and [`NodeMarks::pin`] takes ownership of a tree the
//! emitter built, keeping it alive until the table drops. Every registered
//! address therefore belongs to one live node for the table's whole life —
//! no other node can be allocated there — and a node that is neither
//! (a temporary lowered from a stack local) can never carry a mark, so no
//! read can see a stale one. A mark offered for such a node is refused —
//! the node reads as borrowed, the conservative side (a missed credit is a
//! leak, never a free) — and a debug build stops on it, since the site
//! should have pinned its tree.

use std::collections::HashSet;
use std::rc::Rc;

use almide_ir::visit::{IrVisitor, walk_expr};
use almide_ir::{CallTarget, IrExpr, IrExprKind};

fn expr_key(e: &IrExpr) -> usize {
    e as *const IrExpr as usize
}

fn target_key(t: &CallTarget) -> usize {
    t as *const CallTarget as usize
}

#[derive(Default)]
pub(crate) struct NodeMarks {
    /// Addresses of every node (and every call's `CallTarget`) that lives
    /// as long as this table: registered trees and pinned ones.
    live: HashSet<usize>,
    /// The marked subset of `live`.
    marks: HashSet<usize>,
    /// Trees the emitter built and lowered: owned here so their addresses
    /// stay theirs.
    pinned: Vec<Rc<IrExpr>>,
    /// Argument lists the emitter built (cloned operands) and lowered.
    pinned_args: Vec<Rc<[IrExpr]>>,
    /// #3406: Var reads at which their var DIES (dying_move.rs) — the
    /// receiver of a consuming op that is the rhs of the var's own
    /// reassignment, or the frame's tail. Keyed like a mark; consumed by
    /// the route that hands the var's credit over.
    dying: HashSet<usize>,
    /// #3406: the dying Var read being lowered right now as a MOVE (its
    /// lowering's `forget` does not clear it; the route that set it does).
    moving: Option<usize>,
    /// #3406: `b.f` reads that MOVE the slot's credit out of a record the
    /// frame holds uniquely (a dying spread base), with the slot's offset.
    takes: std::collections::HashMap<usize, u32>,
}

struct Collect<'a>(&'a mut HashSet<usize>);

impl IrVisitor for Collect<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        self.0.insert(expr_key(e));
        if let IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } = &e.kind {
            self.0.insert(target_key(target));
        }
        walk_expr(self, e);
    }
}

impl NodeMarks {
    /// A tree borrowed for at least this table's life.
    pub(crate) fn register(&mut self, root: &IrExpr) {
        Collect(&mut self.live).visit_expr(root);
    }

    /// A function frame's borrowed trees: its body and the top-let
    /// initializers its prelude lowers — with pinned temporaries, the only
    /// nodes a mark may name.
    pub(crate) fn register_frame<'t>(&mut self, body: &IrExpr, top_lets: impl Iterator<Item = &'t IrExpr>) {
        self.register(body);
        top_lets.for_each(|e| self.register(e));
    }

    /// Take a tree the emitter built, keep it alive with this table, and
    /// hand back a handle to lower it through.
    pub(crate) fn pin(&mut self, tree: IrExpr) -> Rc<IrExpr> {
        let tree = Rc::new(tree);
        self.register(&tree);
        self.pinned.push(Rc::clone(&tree));
        tree
    }

    /// [`Self::pin`] for an argument list the emitter assembled.
    pub(crate) fn pin_args(&mut self, args: Vec<IrExpr>) -> Rc<[IrExpr]> {
        let args: Rc<[IrExpr]> = args.into();
        for a in args.iter() {
            self.register(a);
        }
        self.pinned_args.push(Rc::clone(&args));
        args
    }

    fn accept(&self, key: usize, what: &str) -> bool {
        let live = self.live.contains(&key);
        debug_assert!(
            live,
            "#3143: an ownership mark for a {what} the emitter neither borrows nor pinned — \
             a temporary's address can be reused once it drops; build it with `NodeMarks::pin`"
        );
        live
    }

    /// This node's lowering handed its consumer one credit.
    pub(crate) fn mark(&mut self, e: &IrExpr) {
        if self.accept(expr_key(e), "node") {
            self.marks.insert(expr_key(e));
        }
    }

    /// This call's result is owned by the caller (#1990 / #2004).
    pub(crate) fn mark_target(&mut self, t: &CallTarget) {
        if self.accept(target_key(t), "call target") {
            self.marks.insert(target_key(t));
        }
    }

    /// The node is being lowered (again): only this lowering may mark it.
    pub(crate) fn forget(&mut self, e: &IrExpr) {
        self.marks.remove(&expr_key(e));
        if let IrExprKind::Call { target, .. } = &e.kind {
            self.marks.remove(&target_key(target));
        }
    }

    /// #3406: the var read at `e` dies there (see the field).
    pub(crate) fn set_dying(&mut self, e: &IrExpr, on: bool) {
        if !on {
            self.dying.remove(&expr_key(e));
        } else if self.accept(expr_key(e), "dying read") {
            self.dying.insert(expr_key(e));
        }
    }

    /// Consume the dying note on `e`, if it carries one.
    pub(crate) fn take_dying(&mut self, e: &IrExpr) -> bool {
        self.dying.remove(&expr_key(e))
    }

    /// Is the var read at `e` noted dying (without consuming the note)?
    pub(crate) fn is_dying(&self, e: &IrExpr) -> bool {
        self.dying.contains(&expr_key(e))
    }

    /// #3406: the field read at `e` moves the slot at `off` out (`None`
    /// withdraws the note).
    /// The node is named by its address (a node of a live tree).
    pub(crate) fn set_take(&mut self, key: usize, off: Option<u32>) {
        match off {
            Some(off) if self.accept(key, "slot take") => {
                self.takes.insert(key, off);
            }
            _ => {
                self.takes.remove(&key);
            }
        }
    }

    /// #3406: the Var read `e` is lowered as a move (`on`), or no longer.
    pub(crate) fn set_moving(&mut self, e: &IrExpr, on: bool) {
        self.moving = on.then(|| expr_key(e));
    }

    /// Is the Var read `e` being lowered as a move?
    pub(crate) fn is_moving(&self, e: &IrExpr) -> bool {
        self.moving == Some(expr_key(e))
    }

    /// Does the field read `e` carry a pending slot-take note?
    pub(crate) fn has_take(&self, e: &IrExpr) -> bool {
        self.takes.contains_key(&expr_key(e))
    }

    /// Consume the slot-take note on `e`: the slot offset to move out.
    pub(crate) fn take_slot(&mut self, e: &IrExpr) -> Option<u32> {
        self.takes.remove(&expr_key(e))
    }

    pub(crate) fn is_marked(&self, e: &IrExpr) -> bool {
        self.marks.contains(&expr_key(e))
    }

    pub(crate) fn is_target_marked(&self, t: &CallTarget) -> bool {
        self.marks.contains(&target_key(t))
    }
}
