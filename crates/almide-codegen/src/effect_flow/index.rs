//! Which function a name calls, and the per-arity pools of untracked values.

use std::cell::RefCell;
use std::collections::HashMap;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use super::graph::{FnIx, Graph, NodeId};
use super::scan::may_hold_fn;

/// One function scope: a scanned fn, or the pseudo-scope a variable table's
/// top-level `let`s are evaluated in.
pub(super) struct FnInfo<'a> {
    pub func: Option<&'a IrFunction>,
    /// Key in the effect map: the bare name at the root, `module.name` in a module.
    pub name: String,
    /// Variable table: 0 for the root program, `1 + m` for module `m`.
    pub table: usize,
    /// What a call performs (scoped: may name this fn's parameters).
    pub own: NodeId,
    /// The fn value a call returns (scoped likewise).
    pub ret: NodeId,
    /// This fn taken as a value: concrete.
    pub fn_val: NodeId,
}

pub(super) struct Index<'a> {
    pub fns: Vec<FnInfo<'a>>,
    root: HashMap<String, FnIx>,
    in_module: HashMap<(usize, String), FnIx>,
    /// Top-level `let` nodes per variable table.
    pub toplets: Vec<HashMap<VarId, NodeId>>,
    /// The pseudo-scope of each variable table.
    pub top_scope: Vec<FnIx>,
}

impl<'a> Index<'a> {
    pub fn build(program: &'a IrProgram, g: &mut Graph) -> Index<'a> {
        let mut ix = Index { fns: Vec::new(), root: HashMap::new(), in_module: HashMap::new(), toplets: Vec::new(), top_scope: Vec::new() };
        for func in &program.functions {
            let f = ix.add(g, Some(func), func.name.to_string(), 0);
            ix.root.insert(func.name.to_string(), f);
        }
        for (m, module) in program.modules.iter().enumerate() {
            for func in &module.functions {
                let qualified = format!("{}.{}", module.name, func.name);
                let f = ix.add(g, Some(func), qualified.clone(), m + 1);
                ix.root.insert(qualified, f);
                ix.in_module.insert((m, func.name.to_string()), f);
            }
        }
        let tables = std::iter::once(("<top-level>".to_string(), &program.top_lets, &program.var_table))
            .chain(program.modules.iter().map(|m| (format!("{}.<top-level>", m.name), &m.top_lets, &m.var_table)));
        for (t, (name, lets, vt)) in tables.enumerate() {
            let scope = ix.add(g, None, name, t);
            ix.top_scope.push(scope);
            let nodes = lets.iter()
                .filter(|l| may_hold_fn(&l.ty))
                .map(|l| (l.var, g.node(Some(format!("top-level `{}`", var_name(vt, l.var))))))
                .collect();
            ix.toplets.push(nodes);
        }
        ix
    }

    fn add(&mut self, g: &mut Graph, func: Option<&'a IrFunction>, name: String, table: usize) -> FnIx {
        let f = self.fns.len();
        let own = g.node(Some(name.clone()));
        let ret = g.node(None);
        let fn_val = g.node(None);
        let params = func.map_or(&[][..], |fun| fun.params.as_slice());
        let received: Vec<NodeId> = params.iter().map(|_| g.may_node(None)).collect();
        g.param_in.push(received);
        g.param_names.push(params.iter().enumerate().map(|(i, p)| format!("{} (arg {} of {})", p.name, i + 1, name)).collect());
        g.fn_names.push(name.clone());
        self.fns.push(FnInfo { func, name, table, own, ret, fn_val });
        f
    }

    /// The function `name` calls from variable table `table`: a sibling in
    /// the same module first, then a root fn or a qualified `module.fn`.
    pub fn resolve(&self, table: usize, name: &str) -> Option<FnIx> {
        let local = table.checked_sub(1).and_then(|m| self.in_module.get(&(m, name.to_string())));
        local.or_else(|| self.root.get(name)).copied()
    }
}

pub(super) fn var_name(vt: &VarTable, id: VarId) -> String {
    vt.entries.get(id.0 as usize).map_or_else(|| format!("v{}", id.0), |v| v.name.to_string())
}

/// Arity buckets of escaped fn values: `0..=8`, `9` for more.
const BUCKETS: usize = 10;

/// Where an escaped fn value is pooled, by its static type.
enum Key {
    /// A fully known fn signature (aliases expanded, `effect` slots in their
    /// carrier form): only values of that signature are read back at it.
    Exact(String, usize),
    /// A fn type with a type variable or an unknown in it: any signature of
    /// that arity.
    Arity(usize),
    /// Not a fn type at all (a type variable, unknown): any fn.
    Wild,
}

/// The pools of untracked fn values. Before #4151 a pool was one per arity,
/// so every escaped closure of one argument was read back by every call of
/// an untracked one-argument value — an `effect` handler stored in a list
/// was charged to a pure middleware chain over `(Handler) -> Handler`. A
/// pool is now one per signature, and a read whose type does not pin the
/// signature still reads every pool it could alias. Nodes are made on
/// first use; [`Pools::finish`] wires the reads that span pools.
pub(super) struct Pools {
    wild_w: NodeId,
    arity_w: Vec<NodeId>,
    arity_r: Vec<NodeId>,
    read_all: NodeId,
    exact: RefCell<HashMap<String, (usize, NodeId, NodeId)>>,
}

impl Pools {
    pub fn new(g: &mut Graph) -> Pools {
        let label = || Some("an untracked closure value".to_string());
        let wild_w = g.may_node(label());
        let arity_w: Vec<NodeId> = (0..BUCKETS).map(|_| g.may_node(label())).collect();
        let arity_r: Vec<NodeId> = (0..BUCKETS).map(|a| {
            let r = g.may_node(label());
            g.copy(r, arity_w[a]);
            g.copy(r, wild_w);
            r
        }).collect();
        let read_all = g.may_node(label());
        g.copy(read_all, wild_w);
        arity_w.iter().for_each(|&w| g.copy(read_all, w));
        Pools { wild_w, arity_w, arity_r, read_all, exact: RefCell::new(HashMap::new()) }
    }

    fn key(ty: &Ty) -> Key {
        match peel(ty) {
            Ty::Fn { params, ret, is_effect } => {
                let a = params.len().min(BUCKETS - 1);
                let sig = Ty::Fn { params, ret, is_effect };
                match signature(&sig) {
                    Some(s) => Key::Exact(s, a),
                    None => Key::Arity(a),
                }
            }
            _ => Key::Wild,
        }
    }

    fn exact(&self, g: &mut Graph, sig: String, a: usize) -> (NodeId, NodeId) {
        if let Some(&(_, w, r)) = self.exact.borrow().get(&sig) {
            return (w, r);
        }
        let label = || Some("an untracked closure value".to_string());
        let w = g.may_node(label());
        let r = g.may_node(label());
        g.copy(r, w);
        g.copy(r, self.arity_w[a]);
        g.copy(r, self.wild_w);
        self.exact.borrow_mut().insert(sig, (a, w, r));
        (w, r)
    }

    pub fn write(&self, g: &mut Graph, ty: &Ty) -> NodeId {
        match Self::key(ty) {
            Key::Exact(s, a) => self.exact(g, s, a).0,
            Key::Arity(a) => self.arity_w[a],
            Key::Wild => self.wild_w,
        }
    }

    pub fn read(&self, g: &mut Graph, ty: &Ty) -> NodeId {
        match Self::key(ty) {
            Key::Exact(s, a) => self.exact(g, s, a).1,
            Key::Arity(a) => self.arity_r[a],
            Key::Wild => self.read_all,
        }
    }

    /// Wires the reads that see every exact pool: an arity read sees each
    /// signature of its arity, the wildcard read sees them all.
    pub fn finish(&self, g: &mut Graph) {
        for &(a, w, _) in self.exact.borrow().values() {
            g.copy(self.arity_r[a], w);
            g.copy(self.read_all, w);
        }
    }
}

/// A type that is (an `Option`/`Result` of) a bare type variable.
pub(super) fn is_bare_typevar(ty: &Ty) -> bool {
    matches!(peel(ty), Ty::TypeVar(_))
}

/// `Option[F]` / `Result[F, E]` → `F`.
fn peel(ty: &Ty) -> Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, args) if !args.is_empty() => peel(&args[0]),
        Ty::Named(..) => super::scan::aliases::target(ty).map_or_else(|| ty.clone(), |t| peel(&t)),
        _ => ty.clone(),
    }
}

/// The signature text of a fn type with every alias expanded and every
/// `effect (A) -> B` slot in its carrier form `(A) -> Result[B, String]`
/// (the form a named `effect fn` value has); `None` when a type variable or
/// an unknown leaves it open.
fn signature(ty: &Ty) -> Option<String> {
    let t = expand(ty, 0)?;
    (!t.has_unresolved_deep()).then(|| t.display())
}

fn expand(ty: &Ty, depth: usize) -> Option<Ty> {
    if depth > 32 {
        return None;
    }
    let ty = match ty {
        Ty::Named(..) => match super::scan::aliases::target(ty) {
            Some(t) => return expand(&t, depth + 1),
            None => ty.clone(),
        },
        Ty::Fn { params, ret, is_effect: true } => {
            let ret = if ret.is_result() { (**ret).clone() } else { Ty::result((**ret).clone(), Ty::String) };
            Ty::Fn { params: params.clone(), ret: Box::new(ret), is_effect: false }
        }
        _ => ty.clone(),
    };
    let failed = std::cell::Cell::new(false);
    let out = ty.map_children(&|c| expand(c, depth + 1).unwrap_or_else(|| {
        failed.set(true);
        Ty::Unknown
    }));
    (!failed.get()).then_some(out)
}
