//! One representative path per (function, category) — ADR-0026 D4.
//!
//! After the solve, a breadth-first search per category runs forward from
//! every operation that adds it, along the edges whose contribution carries
//! it, so each node it reaches learns its shortest derivation. A path then
//! reads off from a function's node to the operation: the functions it goes
//! through, the parameter a callback came in by (`apply → f (arg 2 of
//! apply)`), the concrete closure when the value is known, and the operation
//! with its location. Where several values may flow (a parameter receives
//! from many call sites, an untracked value), the next step is marked
//! `e.g.`: it is one of them, never asserted to be the callee.

use std::collections::{HashMap, VecDeque};
use almide_ir::effect::Effect;
use super::graph::{bit, Edge, Graph, NodeId};

struct Hop {
    /// The node one step closer to the operation; `None` at the operation.
    next: Option<NodeId>,
    label: Option<String>,
}

#[derive(Clone, Copy)]
enum Role {
    Plain,
    /// The value passed as arg i of the callee.
    InstArg(usize),
    /// What arg i of the scope function receives.
    ConcParam(usize),
}

pub(super) struct Paths {
    per_cat: Vec<HashMap<NodeId, Hop>>,
}

/// Longest path rendered; a longer one is cut with `…`.
const MAX_STEPS: usize = 48;

impl Paths {
    pub fn compute(g: &Graph) -> Paths {
        let mut out: Vec<Vec<(usize, Role)>> = vec![Vec::new(); g.nodes.len()];
        for (ei, e) in g.edges.iter().enumerate() {
            match e {
                Edge::Const { .. } => {}
                Edge::Copy { from, .. } => out[*from].push((ei, Role::Plain)),
                Edge::Inst { src, args, .. } => {
                    out[*src].push((ei, Role::Plain));
                    for (i, a) in args.iter().enumerate() {
                        if let Some(a) = a {
                            out[*a].push((ei, Role::InstArg(i)));
                        }
                    }
                }
                Edge::Concretize { from, scope, .. } => {
                    out[*from].push((ei, Role::Plain));
                    for &i in &g.nodes[*from].value.syms {
                        if let Some(&p) = g.param_in[*scope].get(i) {
                            out[p].push((ei, Role::ConcParam(i)));
                        }
                    }
                }
            }
        }
        let per_cat = Effect::ALL.iter().map(|&c| search(g, &out, bit(c))).collect();
        Paths { per_cat }
    }

    /// `main → use_it → f (arg 1 of use_it) → closure (line 4:42 in make) → fs.read_text (line 4:51)`.
    pub fn render(&self, g: &Graph, start: NodeId, c: Effect) -> Option<String> {
        let parents = &self.per_cat[Effect::ALL.iter().position(|&e| e == c)?];
        let mut steps = Steps::default();
        steps.push(g.nodes[start].label.as_deref());
        let mut n = start;
        for _ in 0..MAX_STEPS {
            let hop = parents.get(&n)?;
            if g.nodes[n].may {
                steps.example = true;
            }
            steps.push(hop.label.as_deref());
            match hop.next {
                None => return Some(steps.parts.join(" → ")),
                Some(m) => {
                    steps.push(g.nodes[m].label.as_deref());
                    n = m;
                }
            }
        }
        steps.parts.push("…".to_string());
        Some(steps.parts.join(" → "))
    }
}

#[derive(Default)]
struct Steps {
    parts: Vec<String>,
    /// The next printed step is one of several values that may flow here.
    example: bool,
}

impl Steps {
    fn push(&mut self, label: Option<&str>) {
        let Some(label) = label else { return };
        if self.parts.last().is_some_and(|l| l == label || l.strip_prefix("e.g. ") == Some(label)) {
            return;
        }
        let text = if std::mem::take(&mut self.example) { format!("e.g. {label}") } else { label.to_string() };
        self.parts.push(text);
    }
}

fn search(g: &Graph, out: &[Vec<(usize, Role)>], c: u32) -> HashMap<NodeId, Hop> {
    let mut parent: HashMap<NodeId, Hop> = HashMap::new();
    let mut queue = VecDeque::new();
    for e in &g.edges {
        if let Edge::Const { to, cats, label, .. } = e
            && cats & c != 0
            && !parent.contains_key(to)
        {
            parent.insert(*to, Hop { next: None, label: label.clone() });
            queue.push_back(*to);
        }
    }
    while let Some(n) = queue.pop_front() {
        for &(ei, role) in &out[n] {
            let to = g.edges[ei].to();
            if parent.contains_key(&to) || g.nodes[to].value.cats & c == 0 {
                continue;
            }
            let Some(label) = hop_label(g, &g.edges[ei], role) else { continue };
            parent.insert(to, Hop { next: Some(n), label });
            queue.push_back(to);
        }
    }
    parent
}

/// The label of stepping along `edge` in `role`; `None` when the edge does
/// not carry the source's set in that role (a sym the callee never calls).
fn hop_label(g: &Graph, edge: &Edge, role: Role) -> Option<Option<String>> {
    match (edge, role) {
        (_, Role::Plain) => Some(None),
        (Edge::Inst { src, callee, .. }, Role::InstArg(i)) => g.nodes[*src].value.syms.contains(&i).then(|| {
            Some(format!("{} → {}", g.fn_names[*callee], g.param_names[*callee].get(i).map_or("?", String::as_str)))
        }),
        (Edge::Concretize { scope, .. }, Role::ConcParam(i)) => {
            Some(g.param_names[*scope].get(i).cloned())
        }
        _ => None,
    }
}
