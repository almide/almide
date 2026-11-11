// ── part of native_move_params.rs, include!-spliced at module level ──
// What the move-param solve reads off the whole program before it starts:
// the call-site facts per (function, param index), and the per-param rules
// the takeovers consult.

thread_local! {
    /// The (function, param index) pairs every call site of the program fills
    /// with a value released right after the call ([`rewrite`] sets them).
    static DYING_SITES: std::cell::RefCell<BTreeSet<(String, usize)>> = const { std::cell::RefCell::new(BTreeSet::new()) };
}

thread_local! {
    /// The (function, param index) pairs whose native signature is a native
    /// `Result` ([`rewrite`] sets them).
    static NATIVE_RESULTS: std::cell::RefCell<BTreeSet<(String, usize)>> = const { std::cell::RefCell::new(BTreeSet::new()) };
}

thread_local! {
    /// The functions of the program ([`rewrite`] sets them): a call to any
    /// other name has no body here, only its declared runtime route.
    static BODIES: std::cell::RefCell<BTreeSet<String>> = const { std::cell::RefCell::new(BTreeSet::new()) };
}

fn has_body(name: &str) -> bool {
    BODIES.with(|d| d.borrow().contains(name))
}

fn native_result(name: &str, k: usize) -> bool {
    NATIVE_RESULTS.with(|d| d.borrow().contains(&(name.to_string(), k)))
}

fn dying_at_every_site(name: &str, k: usize) -> bool {
    DYING_SITES.with(|d| d.borrow().contains(&(name.to_string(), k)))
}

/// Every call site of each function, by position: whether its argument there
/// is a value the caller releases right after the call (`release_after`).
fn dying_sites(functions: &[MirFunction]) -> BTreeSet<(String, usize)> {
    let mut seen: BTreeMap<(String, usize), bool> = BTreeMap::new();
    for f in functions {
        let depth = depths(f);
        for (i, op) in f.ops.iter().enumerate() {
            let Op::CallFn { name, args, .. } = op else { continue };
            for (k, a) in args.iter().enumerate() {
                let dies = matches!(a, crate::CallArg::Handle(v) if release_after(f, i, *v, &depth).is_some());
                *seen.entry((name.clone(), k)).or_insert(true) &= dies;
            }
        }
    }
    seen.into_iter().filter(|(_, d)| *d).map(|(s, _)| s).collect()
}

/// Every function some `FuncRef` takes as a value.
fn taken_functions(functions: &[MirFunction]) -> BTreeSet<String> {
    functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            Op::FuncRef { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// The (function, param index) pairs whose native signature is a native
/// `Result`.
fn native_result_params(sigs: &crate::render_native::NativeSigs) -> BTreeSet<(String, usize)> {
    sigs.iter()
        .flat_map(|(name, (params, _))| {
            params
                .iter()
                .enumerate()
                .filter(|(_, p)| matches!(p, crate::render_native::NativeSigKind::Res | crate::render_native::NativeSigKind::ResStr))
                .map(move |(k, _)| (name.clone(), k))
        })
        .collect()
}

/// The functions some op reaches other than by a direct named call: a
/// generated drop (`DropVariant` / `DropWrapperRec` name their target) and a
/// fan arm (the render spawns it on a thread of its own).
fn reached_otherwise(functions: &[MirFunction]) -> BTreeSet<String> {
    functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            Op::DropVariant { ty, .. } => Some(format!("__drop_{}", crate::lower::drop_fn_ident(ty))),
            Op::DropWrapperRec { drop_fn, .. } => Some(format!("__drop_{}", crate::lower::drop_fn_ident(drop_fn))),
            Op::CallFn { name, .. } if crate::fan_native::arm(name).is_some() => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// Every taken function in the program, by param shape (heap-ness per slot).
fn closure_table(functions: &[MirFunction], taken: &BTreeSet<String>) -> Vec<(String, Vec<bool>)> {
    functions
        .iter()
        .filter(|f| taken.contains(&f.name))
        .map(|f| (f.name.clone(), f.params.iter().map(|p| p.repr.is_heap()).collect()))
        .collect()
}

/// The values of `f` a site may only lend: one the ownership mirror does not
/// track (a raw slot load, `prim.load_str(h + 12 + i * 8)`, a violation of
/// the body as lowered), and a list literal of scalars that stays a literal
/// (`decode_default(v, "ns", [1, 2])`, which the render keeps on the stack
/// as an array; handed over it would be a heap list built on every call). A
/// `var` the literal starts (`var xs = []`) is rebound, a heap list, and a
/// list of heap elements is a heap list whether lent or handed over.
fn lent_only_values(f: &MirFunction) -> BTreeSet<ValueId> {
    let rebound: BTreeSet<ValueId> = f
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::SetLocal { local, .. } => Some(*local),
            _ => None,
        })
        .collect();
    let scalars: BTreeSet<ValueId> = f
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, .. } | Op::IntBinOp { dst, .. } => Some(*dst),
            _ => None,
        })
        .collect();
    let literals = f.ops.iter().filter_map(|op| match op {
        Op::ListLit { dst, elems } if !elems.is_empty() && !rebound.contains(dst) && elems.iter().all(|e| scalars.contains(e)) => Some(*dst),
        _ => None,
    });
    let untracked: BTreeSet<ValueId> = match crate::verify_ownership(f) {
        Ok(()) => BTreeSet::new(),
        Err(found) => found.iter().map(|v| v.value).collect(),
    };
    untracked.into_iter().chain(literals).collect()
}

/// The possible callees of a call op and its args: the named one, or the
/// closure-table entries whose param shape matches a closure call's args.
fn site_callees<'o>(op: &'o Op, table: &[(String, Vec<bool>)]) -> Option<(Vec<String>, &'o [crate::CallArg])> {
    match op {
        Op::CallFn { name, args, .. } => Some((vec![name.clone()], args)),
        Op::CallIndirect { args, .. } => Some((
            table
                .iter()
                .filter(|(_, shape)| {
                    shape.len() == args.len() && shape.iter().zip(args.iter()).all(|(h, a)| *h == matches!(a, crate::CallArg::Handle(_)))
                })
                .map(|(n, _)| n.clone())
                .collect(),
            args,
        )),
        _ => None,
    }
}

/// (callee, position) pairs that cannot take a handed-over reference: some
/// call site fills them with something other than a heap handle, or with a
/// value the caller may only lend ([`lent_only_values`]). That param of every
/// possible callee keeps borrowing while its other params may still be taken
/// over.
fn not_handle_positions(functions: &[MirFunction], table: &[(String, Vec<bool>)]) -> BTreeSet<(String, usize)> {
    let mut out: BTreeSet<(String, usize)> = BTreeSet::new();
    for op in functions.iter().flat_map(|f| f.ops.iter()) {
        if let Op::CallFn { name, args, .. } = op {
            out.extend(args.iter().enumerate().filter(|(_, a)| !matches!(a, crate::CallArg::Handle(_))).map(|(k, _)| (name.clone(), k)));
        }
    }
    for f in functions {
        let lent = lent_only_values(f);
        for (callees, args) in f.ops.iter().filter_map(|op| site_callees(op, table)) {
            for (k, _) in args.iter().enumerate().filter(|(_, a)| matches!(a, crate::CallArg::Handle(v) if lent.contains(v))) {
                out.extend(callees.iter().map(|c| (c.clone(), k)));
            }
        }
    }
    out
}

/// (callee, position) pairs some call site fills with a value it does not
/// release right after the call (it reads it again): that site would copy.
fn kept_positions(functions: &[MirFunction]) -> BTreeSet<(String, usize)> {
    let mut kept: BTreeSet<(String, usize)> = BTreeSet::new();
    for f in functions {
        let depth = depths(f);
        for (i, op) in f.ops.iter().enumerate() {
            let Op::CallFn { name, args, .. } = op else { continue };
            for (k, a) in args.iter().enumerate() {
                if matches!(a, crate::CallArg::Handle(v) if release_after(f, i, *v, &depth).is_none()) {
                    kept.insert((name.clone(), k));
                }
            }
        }
    }
    kept
}

/// The releases of a param given away on some arms (`given`) and never read
/// again on the others (`some(..) => runs + [..], none => [(c, 1)]`): each
/// give, and the start of each arm it dies unread in (the release lands
/// after the op before that start).
fn dead_arm_releases(func: &MirFunction, p: ValueId, given: &[usize]) -> Option<Vec<usize>> {
    if given.is_empty() || func.ret == Some(p) {
        return None;
    }
    let dead = crate::mir_liveness::dead_arm_starts(func, p);
    let mut ends = given.to_vec();
    ends.extend(dead.iter().copied());
    let covered = !dead.is_empty() && crate::mir_liveness::last_use_on_every_path(func, p, &ends);
    covered.then(|| given.iter().copied().chain(dead.iter().map(|&s| s - 1)).collect())
}

/// What [`field_given`] saw of a param's heap fields: the ops that load one
/// (`loads`), the ops that give one away at its last use (`at`), and those
/// of them that keep the field whatever the modes turn out to be (`stored`:
/// a copy into a new block, a by-value runtime position, a concatenation's
/// left side). A give to a program function's move param is not one: that
/// callee may only read the field, its mode resting on this very takeover.
#[derive(Default)]
struct FieldGives {
    loads: Vec<usize>,
    at: Vec<usize>,
    stored: Vec<usize>,
}

/// Whether every path through `lo..hi`, a run of ops at depth `d`, passes a
/// give: one sits in the run itself, or both arms of a branch in it do. A
/// loop body may run no times and an exit leaves before the rest, so neither
/// counts and an exit refuses the run.
fn every_path_gives(func: &MirFunction, depth: &[i32], lo: usize, hi: usize, d: i32, gives: &[usize]) -> bool {
    let close = |from: usize| (from + 1..hi).find(|&j| depth[j] == d && matches!(func.ops[j], Op::EndIf { .. } | Op::LoopEnd));
    let mut i = lo;
    while i < hi {
        match &func.ops[i] {
            Op::Return { .. } | Op::LoopBreakUnless { .. } => return false,
            _ if gives.contains(&i) => return true,
            Op::IfThen { .. } => {
                let Some(end) = close(i) else { return false };
                let els = (i + 1..end).find(|&j| depth[j] == d && matches!(func.ops[j], Op::Else { .. }));
                if els.is_some_and(|els| {
                    every_path_gives(func, depth, i + 1, els, d + 1, gives)
                        && every_path_gives(func, depth, els + 1, end, d + 1, gives)
                }) {
                    return true;
                }
                i = end;
            }
            Op::LoopStart => {
                let Some(end) = close(i) else { return false };
                i = end;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// A param's heap fields are loaded in one arm, and every path through the
/// rest of that arm stores one (`Node(l, v, r) => if c then Node(f(l), v,
/// r) else Node(l, v, f(r))`). A caller that keeps its argument then copies
/// the record once for the call, where the borrowing body copied the field
/// it stores on each such path: the takeover adds no copy on a path that
/// reads the fields, and moves the field wherever the caller's value dies.
fn gives_wherever_loaded(func: &MirFunction, depth: &[i32], gives: &FieldGives) -> bool {
    let Some(&first) = gives.loads.first() else { return false };
    let d = depth[first];
    let hi = (first + 1..func.ops.len()).find(|&j| depth[j] < d).unwrap_or(func.ops.len());
    gives.loads.iter().all(|&l| l < hi) && every_path_gives(func, depth, first, hi, d, &gives.stored)
}

/// Where a record param whose field is given at `end` is released: right
/// there at the top level, or after the close of the top-level branch the
/// give sits in (`match r { ok(xs) => some(xs), err(_) => none }`), nothing
/// reading the record after it. Inside a branch the field moves on one path
/// only, so the takeover must cost no caller a copy: every site of param `k`
/// hands a value that dies at the call, or the body gives a field on every
/// path that loads one ([`gives_wherever_loaded`]). A param read inside a
/// branch (`at_top` false: a variant's payload read in its arm) is held to
/// the same.
fn branch_close(func: &MirFunction, depth: &[i32], end: usize, k: usize, at_top: bool, gives: &FieldGives) -> Option<usize> {
    if depth[end] == 0 && at_top {
        return Some(end);
    }
    if !dying_at_every_site(&func.name, k) && !gives_wherever_loaded(func, depth, gives) {
        return None;
    }
    if depth[end] == 0 {
        return Some(end);
    }
    (end + 1..func.ops.len())
        .find(|&j| depth[j] == 0 && !matches!(func.ops[j], Op::Else { .. }))
        .filter(|&j| matches!(func.ops[j], Op::EndIf { .. }))
}

/// The takeover of `func`'s params over its body with the nested tail
/// branches flattened (`native_unnest_tail`), where the body as lowered would
/// carry a consuming event in an arm of an arm. The flattened body is judged
/// before any param moves: it is kept only if it adds no violation and no
/// poison of its own.
fn own_unnested(
    func: &MirFunction,
    takes: &dyn Fn(usize) -> bool,
    hands: &dyn Fn(usize) -> bool,
    owned: &Modes<'_>,
) -> Option<MirFunction> {
    let flat = crate::native_unnest_tail::unnest(func).filter(|g| no_new_violations(func, g))?;
    own_params(&flat, takes, hands, owned)
}
