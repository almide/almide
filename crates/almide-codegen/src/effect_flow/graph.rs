//! The constraint graph the category sets are solved on (ADR-0026 D1).
//!
//! A node holds an [`ESet`]: concrete categories plus the parameters of ONE
//! function (its scope) whose callers decide the rest. A node scoped to a
//! function may carry that function's parameters; a global node (a record
//! field, the untracked pool, what a parameter receives, a fn taken as a
//! value) is always concrete. Every edge only adds, the lattice is finite, so
//! the round-robin solve reaches the least fixpoint with no iteration cap.

use std::collections::BTreeSet;
use almide_ir::effect::Effect;

pub(super) type NodeId = usize;
pub(super) type FnIx = usize;

/// A category set: concrete categories (a bit per [`Effect::ALL`] entry) and
/// the parameter positions of the node's scope function — "whatever arg i
/// does". Never printed as a variable: a reader sees the concrete set, or the
/// parameter by name and position (ADR-0026 D1, D4).
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(super) struct ESet {
    pub cats: u32,
    pub syms: BTreeSet<usize>,
}

impl ESet {
    /// Adds `other`; true when something new arrived.
    fn absorb(&mut self, other: &ESet) -> bool {
        let before = (self.cats, self.syms.len());
        self.cats |= other.cats;
        self.syms.extend(other.syms.iter().copied());
        before != (self.cats, self.syms.len())
    }
}

pub(super) fn bit(e: Effect) -> u32 {
    1 << (e as u32)
}

/// The categories of a mask, in declaration order.
pub(super) fn cats_of(mask: u32) -> impl Iterator<Item = Effect> {
    Effect::ALL.into_iter().filter(move |e| mask & bit(*e) != 0)
}

pub(super) struct Node {
    /// What the node is, printed when a path passes through it.
    pub label: Option<String>,
    /// A path that leaves this node continues to one of several values that
    /// may flow here; the next step is an example, not the callee (D4).
    pub may: bool,
    pub value: ESet,
}

pub(super) enum Edge {
    /// `to ⊇ cats`, an operation named by `label`; or `to ⊇ {arg sym}`.
    Const { to: NodeId, cats: u32, sym: Option<usize>, label: Option<String> },
    /// `to ⊇ from`. `from` is global or shares `to`'s scope.
    Copy { to: NodeId, from: NodeId },
    /// A call of `callee`: `to ⊇ src` with each of `src`'s parameter syms
    /// replaced by the set of the value passed in that position.
    Inst { to: NodeId, src: NodeId, callee: FnIx, args: Vec<Option<NodeId>> },
    /// `to ⊇ from` made concrete: each sym of `scope` is replaced by
    /// everything any call site passes in that position. Used where a value
    /// leaves its function's frame (a field, the pool, a fn taken as a value).
    Concretize { to: NodeId, from: NodeId, scope: FnIx },
}

impl Edge {
    pub fn to(&self) -> NodeId {
        match self {
            Edge::Const { to, .. } | Edge::Copy { to, .. } | Edge::Inst { to, .. } | Edge::Concretize { to, .. } => *to,
        }
    }
}

#[derive(Default)]
pub(super) struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// `param_in[f][i]`: every value any call site passes as arg i of f.
    pub param_in: Vec<Vec<NodeId>>,
    /// `param_names[f][i]`: `name (arg i+1 of f)`.
    pub param_names: Vec<Vec<String>>,
    /// Display name of every function scope.
    pub fn_names: Vec<String>,
}

impl Graph {
    pub fn node(&mut self, label: Option<String>) -> NodeId {
        self.nodes.push(Node { label, may: false, value: ESet::default() });
        self.nodes.len() - 1
    }

    pub fn may_node(&mut self, label: Option<String>) -> NodeId {
        let id = self.node(label);
        self.nodes[id].may = true;
        id
    }

    pub fn copy(&mut self, to: NodeId, from: NodeId) {
        if to != from {
            self.edges.push(Edge::Copy { to, from });
        }
    }

    /// Solves every edge to the least fixpoint.
    pub fn solve(&mut self) {
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..self.edges.len() {
                changed |= self.apply(i);
            }
        }
    }

    fn apply(&mut self, i: usize) -> bool {
        let add = match &self.edges[i] {
            Edge::Const { cats, sym, .. } => ESet { cats: *cats, syms: sym.iter().copied().collect() },
            Edge::Copy { from, .. } => self.nodes[*from].value.clone(),
            Edge::Inst { src, args, .. } => self.instantiate(*src, args),
            Edge::Concretize { from, scope, .. } => ESet { cats: self.concrete(*from, *scope), syms: BTreeSet::new() },
        };
        let to = self.edges[i].to();
        self.nodes[to].value.absorb(&add)
    }

    fn instantiate(&self, src: NodeId, args: &[Option<NodeId>]) -> ESet {
        let s = &self.nodes[src].value;
        let mut out = ESet { cats: s.cats, syms: BTreeSet::new() };
        for &i in &s.syms {
            if let Some(Some(a)) = args.get(i) {
                out.absorb(&self.nodes[*a].value);
            }
        }
        out
    }

    fn concrete(&self, from: NodeId, scope: FnIx) -> u32 {
        let f = &self.nodes[from].value;
        f.syms.iter()
            .filter_map(|&i| self.param_in.get(scope).and_then(|p| p.get(i)))
            .fold(f.cats, |acc, &p| acc | self.nodes[p].value.cats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chain far deeper than any round budget still carries the leaf's
    /// category to the root (the #3238 property, on the new solver).
    #[test]
    fn a_long_call_chain_reaches_the_root() {
        let mut g = Graph::default();
        let n = 200;
        let own: Vec<NodeId> = (0..n).map(|_| g.node(None)).collect();
        g.param_in = vec![Vec::new(); n];
        for i in 0..n - 1 {
            g.edges.push(Edge::Inst { to: own[i], src: own[i + 1], callee: i + 1, args: Vec::new() });
        }
        g.edges.push(Edge::Const { to: own[n - 1], cats: bit(Effect::IO), sym: None, label: None });
        g.solve();
        assert!(own.iter().all(|&o| g.nodes[o].value.cats == bit(Effect::IO)));
    }

    /// A parameter sym is replaced by the argument at each call site: the
    /// pure argument adds nothing, the effectful one adds its category.
    #[test]
    fn a_parameter_sym_takes_the_argument_of_each_call_site() {
        let mut g = Graph::default();
        let apply_own = g.node(None);
        g.edges.push(Edge::Const { to: apply_own, cats: 0, sym: Some(0), label: None });
        let pure_arg = g.node(None);
        let io_arg = g.node(None);
        g.edges.push(Edge::Const { to: io_arg, cats: bit(Effect::IO), sym: None, label: None });
        let caller_a = g.node(None);
        let caller_b = g.node(None);
        g.edges.push(Edge::Inst { to: caller_a, src: apply_own, callee: 0, args: vec![Some(pure_arg)] });
        g.edges.push(Edge::Inst { to: caller_b, src: apply_own, callee: 0, args: vec![Some(io_arg)] });
        g.param_in = vec![Vec::new()];
        g.solve();
        assert_eq!(g.nodes[caller_a].value.cats, 0);
        assert_eq!(g.nodes[caller_b].value.cats, bit(Effect::IO));
        assert_eq!(g.nodes[apply_own].value.syms, [0].into());
    }
}
