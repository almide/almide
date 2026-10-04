//! Which function a name calls, and the per-arity pools of untracked values.

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

/// Arity buckets of escaped fn values: `0..=8`, `9` for more, and a wildcard
/// for a value whose type does not say (a type variable, unknown).
const BUCKETS: usize = 10;
const WILD: usize = BUCKETS;

pub(super) struct Pools {
    write: Vec<NodeId>,
    read: Vec<NodeId>,
    read_all: NodeId,
}

impl Pools {
    pub fn new(g: &mut Graph) -> Pools {
        let label = || Some("an untracked closure value".to_string());
        let write: Vec<NodeId> = (0..=WILD).map(|_| g.may_node(label())).collect();
        let read: Vec<NodeId> = (0..BUCKETS).map(|k| {
            let r = g.may_node(label());
            g.copy(r, write[k]);
            g.copy(r, write[WILD]);
            r
        }).collect();
        let read_all = g.may_node(label());
        write.iter().for_each(|&w| g.copy(read_all, w));
        Pools { write, read, read_all }
    }

    fn key(ty: &Ty) -> usize {
        match peel(ty) {
            Ty::Fn { params, .. } => params.len().min(BUCKETS - 1),
            _ => WILD,
        }
    }

    pub fn write(&self, ty: &Ty) -> NodeId {
        self.write[Self::key(ty)]
    }

    pub fn read(&self, ty: &Ty) -> NodeId {
        match Self::key(ty) {
            WILD => self.read_all,
            k => self.read[k],
        }
    }
}

/// `Option[F]` / `Result[F, E]` → `F`.
fn peel(ty: &Ty) -> &Ty {
    match ty {
        Ty::Applied(TypeConstructorId::Option | TypeConstructorId::Result, args) if !args.is_empty() => peel(&args[0]),
        _ => ty,
    }
}
