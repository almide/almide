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

    pub(crate) fn is_marked(&self, e: &IrExpr) -> bool {
        self.marks.contains(&expr_key(e))
    }

    pub(crate) fn is_target_marked(&self, t: &CallTarget) -> bool {
        self.marks.contains(&target_key(t))
    }
}
