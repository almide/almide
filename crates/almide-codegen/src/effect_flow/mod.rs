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
use almide_lang::types::Ty;
use graph::{cats_of, Edge, FnIx, Graph};
use index::{Index, Pools};
use scan::{may_hold_fn, Scan, Shared};

pub(crate) use crate::pass_effect_inference::{declared_direct_effects, module_to_effect, runtime_name_to_effect};

/// The solved graph of a program.
struct Solved<'a> {
    g: Graph,
    ix: Index<'a>,
    paths: paths::Paths,
}

fn solve_program(program: &IrProgram) -> Solved<'_> {
    let _aliases = scan::aliases::Scope::new(program);
    let mut g = Graph::default();
    let ix = Index::build(program, &mut g);
    let pools = Pools::new(&mut g);
    let mut sh = Shared::default();
    for f in 0..ix.fns.len() {
        scan_function(program, &ix, &pools, &mut sh, &mut g, f);
    }
    wire_taken(&ix, &pools, &sh.taken, &mut g);
    pools.finish(&mut g);
    g.solve();
    let paths = paths::Paths::compute(&g);
    Solved { g, ix, paths }
}

/// A pure function (or a top-level `let`) that runs an `effect fn` reached
/// through a function VALUE (#4151): a callback it was handed, a record
/// field, an element, or a parameter's argument at one of its call sites
/// that carries the bit through. E006 judges direct calls and the names
/// that hold an effect fn; this judges every value the flow can follow.
#[derive(Debug, Clone)]
pub struct EffectValueViolation {
    /// `name`, or `module.name` in a module; `<top-level>` for a `let`.
    pub function: String,
    /// The module the function is declared in (`None`: the entry program).
    pub module: Option<String>,
    /// `f → field `read` → fs.read_text (line 5:26)` — ADR-0026 D4's shape.
    pub path: String,
    /// Where in the function the effect enters (the first edge of the path).
    pub site: Option<Span>,
}

/// Every pure function of `program` whose solved set carries the effect bit
/// concretely — not through a parameter of its own (that is a transparent
/// callback, charged at its call sites: ADR-0009 D2), but from a value it
/// reads, so no call site can be blamed instead.
pub fn effect_value_violations(program: &IrProgram) -> Vec<EffectValueViolation> {
    let Solved { g, ix, paths } = solve_program(program);
    let mut out = Vec::new();
    for info in &ix.fns {
        let pure = info.func.is_none_or(|f| !f.is_effect);
        if !pure || g.nodes[info.own].value.cats & graph::EFFECT_FN == 0 {
            continue;
        }
        let path = paths.render_bit(&g, info.own, graph::EFFECT_FN).unwrap_or_else(|| info.name.clone());
        let module = info.table.checked_sub(1).map(|m| program.modules[m].name.to_string());
        out.push(EffectValueViolation {
            function: info.name.clone(),
            module,
            path,
            site: paths.first_site(&g, info.own, graph::EFFECT_FN),
        });
    }
    out
}

/// Solves the category sets of every function of `program`.
pub fn infer(program: &IrProgram) -> EffectMap {
    let Solved { g, ix, paths } = solve_program(program);
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
            // An `@extern` fn is ⊤ (#3245): its foreign body performs every
            // category, seeded as a constant on its own node.
            let declared = declared_direct_effects(func).into_iter().fold(0, |m, c| m | graph::bit(c));
            if declared != 0 {
                let label = format!("@extern {}", func.name);
                // Categories only: a plain-`fn` `@extern` is the JS / native
                // host's synchronous import, callable from a pure fn (#4159
                // leaves its effect bit to a ruling; see ADR-0026's note).
                scan.g.push(Edge::Const { to: info.own, cats: declared, sym: None, label: Some(label) });
            }
            // The effect bit (#4151): an `effect fn` runs effects by its
            // declaration, so calling it — or calling a value that holds it —
            // carries the bit.
            // An `effect fn` whose foreign body only ends the program
            // (`process.exit: -> Never`) is ADR-0022's abort, which a plain
            // fn may do — the lowered `assert` arm calls it.
            let only_aborts = is_foreign_bodied(func) && matches!(func.ret_ty, Ty::Never);
            if func.is_effect && !only_aborts {
                scan.g.push(Edge::Const { to: info.own, cats: graph::EFFECT_FN, sym: None, label: None });
            }
            scan.params = func.params.iter().enumerate().filter(|(_, p)| may_hold_fn(&p.ty)).map(|(i, p)| (p.var, i)).collect();
            if is_foreign_bodied(func) {
                foreign_summary(&mut scan, func);
            }
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

/// A function whose body is not Almide the scan can read: an `@intrinsic`
/// (`= _`, the runtime implements it) or any other hole body. Before #4152
/// its empty body was scanned as "performs nothing", so `list.map(xs,
/// fs.read_text)` charged no category and no effect bit to anyone.
pub(super) fn is_foreign_bodied(func: &IrFunction) -> bool {
    matches!(func.body.kind, IrExprKind::Hole) || func.attrs.iter().any(|a| a.name.as_str() == "intrinsic")
}

/// What a foreign body is assumed to do with its parameters (#4152). A
/// parameter of fn type is a callback: the body runs it (the call site is
/// charged with the argument's set — transparent, ADR-0009 D2 / ADR-0026
/// D1) and does not keep it — no stdlib `@intrinsic` stores a callback in
/// what it returns (`http.serve` runs its handler). Any other parameter that
/// may hold a fn (a type variable: `list.push(xs, x)`, `fold`'s `init`) may
/// be stored or handed to a callback: it escapes into the pools at each call
/// site, at the argument's own type, and the result is read back from the
/// pools there at the call's type (`Scan::user_call`). Its declared
/// category is charged where it is used — a call (`Scan::call`) or a
/// reference to it as a value (`Scan::fn_ref`) — not on the declaration,
/// which `[permissions]` would otherwise judge whether or not anything uses
/// it.
fn foreign_summary(scan: &mut Scan<'_, '_>, func: &IrFunction) {
    for (i, p) in func.params.iter().enumerate() {
        if matches!(p.ty, Ty::Fn { .. }) {
            let sym = scan.sym_node(i);
            scan.g.copy(scan.performer, sym);
        }
    }
}

/// A fn taken as a value can be called from anywhere: it is charged with
/// everything it performs once its parameters receive untracked values, and
/// what it returns is untracked.
fn wire_taken(ix: &Index<'_>, pools: &Pools, taken: &BTreeSet<FnIx>, g: &mut Graph) {
    for &f in taken {
        let info = &ix.fns[f];
        g.push(Edge::Concretize { to: info.fn_val, from: info.own, scope: f });
        let Some(func) = info.func else { continue };
        for (i, p) in func.params.iter().enumerate() {
            if may_hold_fn(&p.ty) {
                let r = pools.read(g, &p.ty);
                let to = g.param_in[f][i];
                g.copy(to, r);
            }
        }
        if !index::is_bare_typevar(&func.ret_ty) {
            let to = pools.write(g, &func.ret_ty);
            g.push(Edge::Concretize { to, from: info.ret, scope: f });
        }
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
