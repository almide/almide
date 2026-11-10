//! #4614: which nodes of a sequence can change one object's paths.
//!
//! [`super::render`] walks the whole frame once per object. A function that
//! builds N objects in one straight line (a module-level `List` of N records)
//! then cost N walks of N nodes. Almost every node names other objects: on a
//! path of object `o`, `step` changes nothing at an event that names neither
//! `o` nor a local some live path of `o` holds, and a branch or loop whose
//! whole subtree is such events leaves every path as it found it (each arm
//! yields the same path, and the dedup after the node keeps one).
//!
//! The index lists, per sequence, the nodes that can matter: those that can
//! end a path (a `Jump` / `Exit` / `Abort` anywhere in the subtree, for every
//! object), and the rest by the objects and locals their subtree names. The
//! walk visits only those. A skipped node still queues the loops the full walk
//! would have queued — once per live path, in walk order — as activations
//! that touch nothing ([`super::render_object`]): the queue order (the order
//! the loop lines are written in), the loop cap and each loop line's path set
//! are exactly what the full walk produces.

use std::collections::{BTreeSet, HashMap};

use super::{Ev, Node};

/// A loop the walk queues: its body and the locals it carries.
pub(super) type LoopRef<'t> = (&'t [Node], &'t [u32]);

/// What a subtree names.
#[derive(Default)]
struct Touch {
    objs: Vec<u32>,
    locals: Vec<u32>,
    ends: bool,
}

impl Touch {
    fn merge(&mut self, t: Touch) {
        self.objs.extend(t.objs);
        self.locals.extend(t.locals);
        self.ends |= t.ends;
    }

    fn settle(mut self) -> Touch {
        self.objs.sort_unstable();
        self.objs.dedup();
        self.locals.sort_unstable();
        self.locals.dedup();
        self
    }
}

fn ev_touch(ev: &Ev) -> Touch {
    let (objs, locals, ends) = match ev {
        Ev::Birth(b) | Ev::Op(b, _) => (vec![*b], vec![], false),
        Ev::Bind { local, obj, .. } => (vec![*obj], vec![*local], false),
        Ev::Alias { local, src, .. } => (vec![], vec![*local, *src], false),
        Ev::LOp(l, _) | Ev::DecOld(l) => (vec![], vec![*l], false),
        Ev::Jump | Ev::Exit | Ev::Abort => (vec![], vec![], true),
        Ev::Open | Ev::Arm | Ev::Close | Ev::LoopOpen | Ev::LoopClose | Ev::Carry { .. } => (vec![], vec![], false),
    };
    Touch { objs, locals, ends }
}

/// The loops a walk queues when it passes `node`: a loop itself, and the
/// loops of a branch's arms in arm order (not those inside a loop body — the
/// body's own walk queues them).
fn direct_loops<'t>(node: &'t Node, out: &mut Vec<LoopRef<'t>>) {
    match node {
        Node::Loop(body, carried) => out.push((body.as_slice(), carried.as_slice())),
        Node::Branch(arms) => arms.iter().flatten().for_each(|n| direct_loops(n, out)),
        Node::Ev(_) => {}
    }
}

/// One sequence's index.
#[derive(Default)]
pub(super) struct SeqIndex<'t> {
    /// Nodes whose subtree can end a path.
    always: Vec<usize>,
    by_obj: HashMap<u32, Vec<usize>>,
    by_local: HashMap<u32, Vec<usize>>,
    /// (node, the loops it queues), by node.
    loops: Vec<(usize, Vec<LoopRef<'t>>)>,
}

fn first_from(v: &[usize], from: usize) -> Option<usize> {
    v.get(v.partition_point(|&i| i < from)).copied()
}

impl<'t> SeqIndex<'t> {
    /// The first node at or after `from` that can change a path of object
    /// `o` whose live paths hold `held`.
    pub(super) fn next(&self, from: usize, o: u32, held: &BTreeSet<u32>) -> Option<usize> {
        let mut best = first_from(&self.always, from);
        let mut take = |c: Option<usize>| {
            if let Some(c) = c {
                best = Some(best.map_or(c, |b| b.min(c)));
            }
        };
        take(self.by_obj.get(&o).and_then(|v| first_from(v, from)));
        for l in held {
            take(self.by_local.get(l).and_then(|v| first_from(v, from)));
        }
        best
    }

    /// The loops the nodes `from..to` queue, in node order.
    pub(super) fn loops_between(&self, from: usize, to: usize) -> impl Iterator<Item = &LoopRef<'t>> {
        let lo = self.loops.partition_point(|(i, _)| *i < from);
        let hi = self.loops.partition_point(|(i, _)| *i < to);
        self.loops[lo..hi].iter().flat_map(|(_, refs)| refs.iter())
    }
}

/// Every sequence of a tree (the frame, each arm, each loop body), by address.
pub(super) struct Index<'t> {
    seqs: HashMap<*const Node, SeqIndex<'t>>,
}

impl<'t> Index<'t> {
    pub(super) fn new(tree: &'t [Node]) -> Index<'t> {
        let mut ix = Index { seqs: HashMap::new() };
        ix.add(tree);
        ix
    }

    /// The index of `seq` (`None` for an empty one: nothing to walk).
    pub(super) fn get(&self, seq: &[Node]) -> Option<&SeqIndex<'t>> {
        if seq.is_empty() { None } else { self.seqs.get(&seq.as_ptr()) }
    }

    fn add(&mut self, seq: &'t [Node]) -> Touch {
        let mut si = SeqIndex::default();
        let mut all = Touch::default();
        for (i, node) in seq.iter().enumerate() {
            let t = match node {
                Node::Ev(ev) => ev_touch(ev),
                Node::Branch(arms) => {
                    let mut t = Touch::default();
                    for arm in arms {
                        t.merge(self.add(arm));
                    }
                    t.settle()
                }
                Node::Loop(body, _) => self.add(body),
            };
            let mut refs = Vec::new();
            direct_loops(node, &mut refs);
            if !refs.is_empty() {
                si.loops.push((i, refs));
            }
            if t.ends {
                si.always.push(i);
            } else {
                for &o in &t.objs {
                    si.by_obj.entry(o).or_default().push(i);
                }
                for &l in &t.locals {
                    si.by_local.entry(l).or_default().push(i);
                }
            }
            all.merge(t);
        }
        if !seq.is_empty() {
            self.seqs.insert(seq.as_ptr(), si);
        }
        all.settle()
    }
}
