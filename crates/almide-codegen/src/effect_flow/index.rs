//! Which function a name calls, and the per-type pools of untracked values.

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

/// One pool of untracked fn values: every value escaped at one fn type.
struct Pool {
    /// The type, aliases expanded and `effect` slots in carrier form; it may
    /// hold type variables (an escape or a read inside a generic body).
    ty: Ty,
    /// No type variable or unknown in `ty`.
    exact: bool,
    w: NodeId,
    r: NodeId,
}

/// The pools of untracked fn values, one per fn type. Before #4151 a pool
/// was one per arity, so every escaped closure of one argument was read
/// back by every call of an untracked one-argument value — an `effect`
/// handler stored in a list was charged to a pure middleware chain over
/// `(Handler) -> Handler`. A pool became one per signature, but a read whose
/// type held a type variable still read every signature of its arity: the
/// heap's `(h.cmp)(a, b)` at `(A, A) -> Int` was charged with an
/// `effect (Float, Float) -> Unit` handler the program stored in a record
/// (#4522). A read now sees the pools whose type it could be an instance
/// of ([`may_match`]): a value read at `(A, A) -> Int` was escaped at some
/// `(X, X) -> Int`, never at a type that returns something else. Two exact
/// types see each other only when equal, as before. Nodes are made on first
/// use; [`Pools::finish`] wires the reads that span pools.
pub(super) struct Pools {
    /// Values escaped at a type that is not a fn type (a type variable, an
    /// unknown): any read may see them.
    wild_w: NodeId,
    /// A read at a type that is not a fn type: sees every pool.
    read_all: NodeId,
    pools: RefCell<HashMap<String, Pool>>,
}

impl Pools {
    pub fn new(g: &mut Graph) -> Pools {
        let label = || Some("an untracked closure value".to_string());
        let wild_w = g.may_node(label());
        let read_all = g.may_node(label());
        g.copy(read_all, wild_w);
        Pools { wild_w, read_all, pools: RefCell::new(HashMap::new()) }
    }

    /// The pool of fn type `ty` (made on first use); `None` when `ty` is not
    /// a fn type.
    fn pool(&self, g: &mut Graph, ty: &Ty) -> Option<(NodeId, NodeId)> {
        let Ty::Fn { params, ret, is_effect } = peel(ty) else { return None };
        let arity = params.len();
        let sig = Ty::Fn { params, ret, is_effect };
        // An expansion too deep to follow is any fn of the arity.
        let ty = expand(&sig, 0).unwrap_or_else(|| Ty::Fn { params: vec![Ty::Unknown; arity], ret: Box::new(Ty::Unknown), is_effect: false });
        let key = ty.display();
        if let Some(p) = self.pools.borrow().get(&key) {
            return Some((p.w, p.r));
        }
        let label = || Some("an untracked closure value".to_string());
        let w = g.may_node(label());
        let r = g.may_node(label());
        g.copy(r, w);
        g.copy(r, self.wild_w);
        g.copy(self.read_all, w);
        let exact = !ty.has_unresolved_deep();
        self.pools.borrow_mut().insert(key, Pool { ty, exact, w, r });
        Some((w, r))
    }

    pub fn write(&self, g: &mut Graph, ty: &Ty) -> NodeId {
        self.pool(g, ty).map_or(self.wild_w, |(w, _)| w)
    }

    pub fn read(&self, g: &mut Graph, ty: &Ty) -> NodeId {
        self.pool(g, ty).map_or(self.read_all, |(_, r)| r)
    }

    /// Wires the reads that span pools: a read sees every other pool whose
    /// type it may share once its type variables are bound. Two exact types
    /// share nothing (each pool already reads its own).
    pub fn finish(&self, g: &mut Graph) {
        let pools = self.pools.borrow();
        let mut all: Vec<&Pool> = pools.values().collect();
        // Edge order follows the pools' types, not the map's iteration.
        all.sort_by_key(|p| p.r);
        for read in &all {
            for write in &all {
                if read.r != write.r && !(read.exact && write.exact) && may_match(&read.ty, &write.ty) {
                    g.copy(read.r, write.w);
                }
            }
        }
    }
}

/// May a value of type `a` also have type `b`: the two unify once every type
/// variable and unknown is free to be anything (each occurrence on its own,
/// which only adds pairs). An `effect` slot's carrier `Result[B, String]`
/// meets a plain `B` result too, as the checker's unify ignores the effect
/// marker. A nominal type meets its structural form.
fn may_match(a: &Ty, b: &Ty) -> bool {
    fn open(t: &Ty) -> bool {
        matches!(t, Ty::TypeVar(_) | Ty::Unknown | Ty::Never | Ty::OpenRecord { .. } | Ty::Union(_) | Ty::ConstParam { .. } | Ty::ConstValue { .. })
    }
    fn all(xs: &[Ty], ys: &[Ty]) -> bool {
        xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| may_match(x, y))
    }
    fn last(s: &str) -> &str {
        s.rsplit_once('.').map_or(s, |(_, n)| n)
    }
    fn carried(t: &Ty) -> &Ty {
        match t {
            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 && args[1] == Ty::String => &args[0],
            _ => t,
        }
    }
    match (a, b) {
        _ if open(a) || open(b) => true,
        (Ty::Named(x, xs), Ty::Named(y, ys)) => last(x.as_str()) == last(y.as_str()) && all(xs, ys),
        (Ty::Named(..), _) | (_, Ty::Named(..)) => true,
        (Ty::Fn { params: p, ret: r, .. }, Ty::Fn { params: q, ret: s, .. }) => {
            all(p, q) && (may_match(r, s) || may_match(carried(r), s) || may_match(r, carried(s)))
        }
        (Ty::Applied(c, xs), Ty::Applied(d, ys)) => c == d && all(xs, ys),
        (Ty::Tuple(xs), Ty::Tuple(ys)) => all(xs, ys),
        (Ty::Record { fields: f }, Ty::Record { fields: g }) => {
            f.len() == g.len() && f.iter().all(|(n, t)| g.iter().any(|(m, u)| n == m && may_match(t, u)))
        }
        (Ty::Variant { name: x, .. }, Ty::Variant { name: y, .. }) => last(x.as_str()) == last(y.as_str()),
        (Ty::Int | Ty::Int64, Ty::Int | Ty::Int64) | (Ty::Float | Ty::Float64, Ty::Float | Ty::Float64) => true,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b) && a.children().is_empty() && b.children().is_empty(),
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

/// A fn type with every alias expanded and every `effect (A) -> B` slot in
/// its carrier form `(A) -> Result[B, String]` (the form a named `effect fn`
/// value has); `None` when the type nests too deep to follow.
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
