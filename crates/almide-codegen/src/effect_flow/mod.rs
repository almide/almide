//! Effect categories ride fn values — ADR-0026 D1, stage 1.
//!
//! The category set of a function value is solved from the program, never
//! written: a lambda's set is what its body performs, a named fn's is what
//! its body performs, and the set *moves with the value*. A function that
//! calls a value it is handed performs that value's set, at the call site
//! that hands it over (a bare fn-type parameter is transparent, as the effect
//! bit already is under ADR-0009 D2). A value stored or returned keeps the
//! set of the value stored: a `let`/`var` local, a record field, a return
//! value, a top-level `let`. Creating a closure performs nothing.
//!
//! Where the sets live: in this analysis, keyed by those positions, not in
//! `Ty::Fn`. They are computed after type checking and dropped with the
//! `EffectMap`, so they are erased by construction and monomorphisation never
//! sees them. Internally a set may name "whatever arg i of f does" (an
//! effect variable, one per fn-typed parameter); it is solved away at every
//! call site and is only ever printed as the parameter, by name and
//! position (D1, D4).
//!
//! Soundness of the report: a fn value that leaves the tracked positions (a
//! list, an option payload, a variant, an argument of a call whose callee is
//! not a scanned fn) joins a pool per arity, and every call of a value read
//! back from such a position is charged with that pool. A stdlib or runtime
//! call is assumed to run every fn argument it is given.

mod graph;
mod index;
mod paths;
mod scan;

use std::collections::{BTreeSet, HashMap, HashSet};
use almide_ir::effect::{Effect, EffectMap, FunctionEffects};
use almide_ir::*;
use graph::{cats_of, Edge, FnIx, Graph};
use index::{Index, Pools};
use scan::{may_hold_fn, Scan, Shared};

pub(crate) use crate::pass_effect_inference::{module_to_effect, runtime_name_to_effect};

/// Solves the category sets of every function of `program`.
pub fn infer(program: &IrProgram) -> EffectMap {
    let mut g = Graph::default();
    let ix = Index::build(program, &mut g);
    let pools = Pools::new(&mut g);
    let mut sh = Shared::default();
    for f in 0..ix.fns.len() {
        scan_function(program, &ix, &pools, &mut sh, &mut g, f);
    }
    wire_taken(&ix, &pools, &sh.taken, &mut g);
    g.solve();
    let paths = paths::Paths::compute(&g);
    let mut direct: HashMap<usize, u32> = HashMap::new();
    for e in &g.edges {
        if let Edge::Const { to, cats, .. } = e {
            *direct.entry(*to).or_default() |= cats;
        }
    }
    let mut map = EffectMap::default();
    for info in &ix.fns {
        if let Some(func) = info.func {
            map.functions.insert(info.name.clone(), summarize(&g, &paths, &direct, info, func));
        }
    }
    map
}

fn scan_function(program: &IrProgram, ix: &Index<'_>, pools: &Pools, sh: &mut Shared, g: &mut Graph, f: FnIx) {
    let info = &ix.fns[f];
    let mut scan = Scan {
        g,
        ix,
        pools,
        sh,
        table: info.table,
        scope: f,
        params: HashMap::new(),
        locals: HashMap::new(),
        syms: HashMap::new(),
        performer: info.own,
        ret_target: Some(info.ret),
    };
    match info.func {
        Some(func) => {
            scan.params = func.params.iter().enumerate().filter(|(_, p)| may_hold_fn(&p.ty)).map(|(i, p)| (p.var, i)).collect();
            scan.bind_locals(&func.body);
            let tail = scan.flow(&func.body);
            if let Some(t) = tail {
                scan.g.copy(info.ret, t);
            }
        }
        None => {
            // A variable table's top-level `let`s.
            let lets = match info.table {
                0 => &program.top_lets,
                t => &program.modules[t - 1].top_lets,
            };
            for l in lets {
                let v = scan.flow(&l.value);
                match (v, ix.toplets[info.table].get(&l.var)) {
                    (Some(v), Some(&n)) => scan.g.copy(n, v),
                    (v, _) => scan.escape(v, &l.ty),
                }
            }
        }
    }
}

/// A fn taken as a value can be called from anywhere: it is charged with
/// everything it performs once its parameters receive untracked values, and
/// what it returns is untracked.
fn wire_taken(ix: &Index<'_>, pools: &Pools, taken: &BTreeSet<FnIx>, g: &mut Graph) {
    for &f in taken {
        let info = &ix.fns[f];
        g.edges.push(Edge::Concretize { to: info.fn_val, from: info.own, scope: f });
        let Some(func) = info.func else { continue };
        for (i, p) in func.params.iter().enumerate() {
            if may_hold_fn(&p.ty) {
                g.copy(g.param_in[f][i], pools.read(&p.ty));
            }
        }
        g.edges.push(Edge::Concretize { to: pools.write(&func.ret_ty), from: info.ret, scope: f });
    }
}

fn set_of(mask: u32) -> HashSet<Effect> {
    cats_of(mask).collect()
}

fn summarize(g: &Graph, paths: &paths::Paths, direct: &HashMap<usize, u32>, info: &index::FnInfo<'_>, func: &IrFunction) -> FunctionEffects {
    let own = &g.nodes[info.own].value;
    let ret = &g.nodes[info.ret].value;
    let direct = direct.get(&info.own).copied().unwrap_or(0);
    let arg = |i: &usize| func.params.get(*i).map_or_else(|| format!("arg {}", i + 1), |p| format!("{} (arg {})", p.name, i + 1));
    FunctionEffects {
        direct: set_of(direct),
        transitive: set_of(own.cats),
        is_effect: func.is_effect,
        indirect: own.syms.iter().map(arg).collect(),
        returns: set_of(ret.cats),
        returns_indirect: ret.syms.iter().map(arg).collect(),
        paths: cats_of(own.cats).filter_map(|c| paths.render(g, info.own, c).map(|p| (c, p))).collect(),
    }
}
