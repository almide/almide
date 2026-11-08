//! Static closure dispatch where the caller knows the callee lambda (#4006,
//! no-perf-loss rule).
//!
//! A program function that calls a closure param dispatches on the lambda
//! index the closure's first word holds (`__almd_ci_*`), so every call of a
//! callback inside a combinator or a recursion paid the table switch and kept
//! the callee out of LLVM's sight; the standard codegen monomorphizes the
//! combinator over the closure's type and calls it directly. Here the
//! function renders once as `NAME__k<const K0: i64, ..>` — one const per
//! KEYED param (below) — whose dispatch reads `K` when it is known (`>= 0`)
//! and the closure's word otherwise, plus `NAME` itself, a wrapper passing
//! `-1` for every key, which every other call site keeps calling. A call
//! whose closure argument is a block the caller built from a lifted lambda
//! (`native_hof::lambda_of`) names the specialization with that lambda's
//! table index, and the dispatch folds to the direct call.
//!
//! Sound because the key equals the word it replaces: a lambda-built block is
//! defined once, from a `FuncRef` defined once, and the keyed param is never
//! rebound, so the word the dispatch would read is the index the caller
//! named. The MIR, its ownership events and the witnesses are unchanged —
//! this is a spelling of the same dispatch.
//!
//! A param is keyed when it is never rebound and a `CallIndirect` of the
//! function dispatches on its first word (`ListGetScalar` at the constant
//! index 0) with it as the env. Lifted lambdas
//! (dispatch-table arms, which call by name) and narrow-ABI functions keep
//! their one shape.

use crate::{CallArg, MirFunction, Op, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Program {
    /// Each keyed function's keyed params, in param order (key `K<i>` is
    /// the i-th).
    keyed: BTreeMap<String, Vec<ValueId>>,
    /// A lifted lambda's dispatch-table index.
    lambdas: BTreeMap<String, usize>,
    /// Every program function.
    fns: BTreeSet<String>,
}

#[derive(Default)]
struct Current {
    /// The keyed params of the function being rendered → their key.
    params: BTreeMap<ValueId, usize>,
    /// Its dispatch-index reads of a keyed param → that key.
    reads: BTreeMap<ValueId, usize>,
    /// The function itself, for the shared-cell windows
    /// (`native_cell_window`), and the takes whose cell a read emptied.
    func: Option<MirFunction>,
    emptied: BTreeSet<ValueId>,
    /// Its borrow-join arms (`borrow_join`), for the liveness reads.
    arms: BTreeSet<ValueId>,
}

thread_local! {
    static PROGRAM: RefCell<Program> = RefCell::new(Program::default());
    static CURRENT: RefCell<Current> = RefCell::new(Current::default());
}

fn rebound(f: &MirFunction) -> BTreeSet<ValueId> {
    f.ops
        .iter()
        .filter_map(|op| match op {
            Op::SetLocal { local, .. } => Some(*local),
            _ => None,
        })
        .collect()
}

/// The params of `f` nothing rebinds.
fn fixed_params(f: &MirFunction) -> BTreeSet<ValueId> {
    let rebound = rebound(f);
    f.params.iter().map(|p| p.value).filter(|v| !rebound.contains(v)).collect()
}

/// `f`'s reads of a fixed param's first word that some `CallIndirect` with
/// that param as its env dispatches on: read → param.
fn dispatch_reads(f: &MirFunction) -> BTreeMap<ValueId, ValueId> {
    let fixed = fixed_params(f);
    let mut defs: BTreeMap<ValueId, usize> = BTreeMap::new();
    for op in &f.ops {
        let set = match op {
            Op::SetLocal { local, .. } => Some(*local),
            op => crate::mir_ops::defined_value(op),
        };
        if let Some(v) = set {
            *defs.entry(v).or_default() += 1;
        }
    }
    let once = |v: &ValueId| defs.get(v) == Some(&1);
    let zero: BTreeSet<ValueId> = f
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, value: 0 } if once(dst) => Some(*dst),
            _ => None,
        })
        .collect();
    let reads: BTreeMap<ValueId, ValueId> = f
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ListGetScalar { dst, list, idx } if fixed.contains(list) && zero.contains(idx) && once(dst) => Some((*dst, *list)),
            _ => None,
        })
        .collect();
    reads
        .into_iter()
        .filter(|(dst, env)| {
            f.ops.iter().any(|op| {
                matches!(op, Op::CallIndirect { table_idx, args, .. }
                    if table_idx == dst && matches!(args.first(), Some(CallArg::Handle(e)) if e == env))
            })
        })
        .collect()
}

/// A function that may take keys: not a lifted lambda, not `main`, and of
/// no narrow-ABI signature (that wrapper owns the name already).
fn may_key(f: &MirFunction, sigs: &crate::render_native::NativeSigs) -> bool {
    let narrow = sigs
        .get(f.name.as_str())
        .is_some_and(|(ps, r)| ps.iter().chain(r.iter()).any(|k| k.narrow_rust().is_some()));
    !f.name.starts_with("__lambda_") && f.name != "main" && !narrow
}

/// Record the keyed params of the program about to render.
pub(crate) fn set_program(user_fns: &BTreeMap<&str, &MirFunction>, sigs: &crate::render_native::NativeSigs) {
    let lambdas: BTreeMap<String, usize> = user_fns
        .keys()
        .filter(|n| n.starts_with("__lambda_"))
        .enumerate()
        .map(|(i, n)| (n.to_string(), i))
        .collect();
    let candidates: Vec<&MirFunction> = user_fns.values().copied().filter(|f| may_key(f, sigs)).collect();
    let keyed = candidates
        .iter()
        .map(|f| (f.name.clone(), dispatch_reads(f).into_values().collect::<BTreeSet<ValueId>>()))
        .filter(|(_, set)| !set.is_empty());
    let keyed = keyed
        .map(|(n, set)| {
            let order = user_fns[n.as_str()].params.iter().map(|p| p.value).filter(|v| set.contains(v)).collect();
            (n, order)
        })
        .collect();
    let fns = user_fns.keys().map(|n| n.to_string()).collect();
    PROGRAM.with(|p| *p.borrow_mut() = Program { keyed, lambdas, fns });
    CURRENT.with(|c| *c.borrow_mut() = Current::default());
}

/// Enter `func`'s render.
pub(crate) fn begin_fn(func: &MirFunction, arms: &BTreeSet<ValueId>) {
    let params: BTreeMap<ValueId, usize> = PROGRAM.with(|p| {
        p.borrow().keyed.get(&func.name).map(|ks| ks.iter().enumerate().map(|(i, v)| (*v, i)).collect()).unwrap_or_default()
    });
    let reads = if params.is_empty() {
        BTreeMap::new()
    } else {
        dispatch_reads(func).into_iter().filter_map(|(r, p)| params.get(&p).map(|k| (r, *k))).collect()
    };
    CURRENT.with(|c| *c.borrow_mut() = Current { params, reads, func: Some(func.clone()), emptied: BTreeSet::new(), arms: arms.clone() });
}

/// The shared-cell read `d` of `cell` takes the payload out
/// (`native_cell_window`); the take ending its window is then noted as
/// finding the cell emptied.
pub(crate) fn cell_read_takes(d: ValueId, cell: ValueId, tys: &BTreeMap<ValueId, crate::render_native::NTy>) -> bool {
    let program_fn = |n: &str| PROGRAM.with(|p| p.borrow().fns.contains(n));
    CURRENT.with(|c| {
        let mut c = c.borrow_mut();
        let Some(func) = c.func.as_ref() else { return false };
        let at = func.ops.iter().position(|op| matches!(op, Op::Prim { kind: crate::PrimKind::LoadHandle, dst: Some(x), .. } if *x == d));
        let end = at.and_then(|at| crate::native_cell_window::take_window(func, at, (d, cell), tys, &program_fn));
        end.is_some_and(|x| c.emptied.insert(x))
    })
}

/// The payload read `d` of the owned carrier `base` may move the payload out:
/// nothing reads the carrier, or an address into it, after the read, and
/// its binding is its own (no param, never rebound) — Rust's drop flag then
/// skips the carrier's release, where a view left it to drop whole.
pub(crate) fn payload_moves(d: ValueId, base: ValueId) -> bool {
    CURRENT.with(|c| {
        let c = c.borrow();
        let Some(func) = c.func.as_ref() else { return false };
        let Some(at) = func.ops.iter().position(|op| matches!(op, Op::Prim { kind: crate::PrimKind::LoadHandle, dst: Some(x), .. } if *x == d)) else { return false };
        let root = crate::render_native_borrow_read::roots(func);
        let own = !rebound(func).contains(&base) && !func.params.iter().any(|p| p.value == base);
        let mut reads = Vec::new();
        own && crate::render_native_move::dying_after(func, at, base, &c.arms)
            && func.ops[at + 1..].iter().all(|op| {
                reads.clear();
                crate::mir_ops::op_reads(op, &mut reads);
                crate::render_native_move::bookkeeping(op) || !reads.iter().any(|r| *r == base || root.get(r) == Some(&base))
            })
    })
}

/// The take `x` finds its cell emptied by the read before it.
pub(crate) fn cell_emptied(x: ValueId) -> bool {
    CURRENT.with(|c| c.borrow().emptied.contains(&x))
}

/// The key a dispatch-index read `d` of the function being rendered takes.
pub(crate) fn read_key(d: ValueId) -> Option<usize> {
    CURRENT.with(|c| c.borrow().reads.get(&d).copied())
}

/// The callee head of a call to `name` with `args`: its specialization when
/// some key is known here, else `None` (the plain name, the `-1` wrapper).
pub(crate) fn call_head(name: &str, args: &[CallArg], callee: &MirFunction) -> Option<String> {
    let ks = PROGRAM.with(|p| p.borrow().keyed.get(name).cloned())?;
    let mut known = false;
    let keys: Vec<String> = ks
        .iter()
        .map(|kp| {
            let pos = callee.params.iter().position(|p| p.value == *kp);
            let arg = pos.and_then(|i| match args.get(i) {
                Some(CallArg::Handle(v)) => Some(*v),
                _ => None,
            });
            let lambda = arg.and_then(crate::native_hof::lambda_of);
            let key = lambda.and_then(|l| PROGRAM.with(|p| p.borrow().lambdas.get(&l).map(|i| i.to_string())));
            known |= key.is_some();
            format!("{{ {} }}", key.unwrap_or_else(|| "-1".to_string()))
        })
        .collect();
    known.then(|| format!("{}__k::<{}>", crate::render_native::mangle(name), keys.join(", ")))
}

/// `rendered` (the text of `func`, its signature first) as the keyed body
/// `NAME__k<const K..: i64>` plus the `NAME` wrapper that passes `-1` for
/// every key. Unchanged for a function with no keyed param.
pub(crate) fn specialize(func: &MirFunction, rendered: String) -> Result<String, crate::lower::LowerError> {
    let Some(ks) = PROGRAM.with(|p| p.borrow().keyed.get(&func.name).cloned()) else { return Ok(rendered) };
    let m = crate::render_native::mangle(&func.name);
    let head = format!("fn {m}(");
    let (true, Some(sig_end)) = (rendered.starts_with(&head), rendered.find(" {\n")) else {
        return Err(crate::render_native::wall(format!("native: keyed dispatch of `{}`: the rendered signature is not found", func.name)));
    };
    let consts: Vec<String> = (0..ks.len()).map(|i| format!("const K{i}: i64")).collect();
    let body = rendered.replacen(&head, &format!("fn {m}__k<{}>(", consts.join(", ")), 1);
    // The wrapper: the same signature (no param is rebound in it), every key
    // unknown.
    let sig = rendered[..sig_end].replace("(mut ", "(").replace(", mut ", ", ");
    let args: Vec<String> = func.params.iter().map(|p| crate::render_native::var(p.value)).collect();
    let unknown: Vec<&str> = ks.iter().map(|_| "{ -1 }").collect();
    Ok(format!("{body}\n#[inline(always)]\n{sig} {{\n    {m}__k::<{}>({})\n}}\n", unknown.join(", "), args.join(", ")))
}
