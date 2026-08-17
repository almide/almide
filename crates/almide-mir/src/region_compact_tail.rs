// ── tail of region_compact.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

/// Remove pure value ops (const / add-chain / Handle bridge) the rewrite left
/// dead, to fixpoint — the address arithmetic dies backwards.
fn sweep_dead(f: &mut MirFunction) {
    loop {
        let mut occ: BTreeMap<ValueId, usize> = BTreeMap::new();
        let mut vals: Vec<ValueId> = Vec::new();
        for op in &f.ops {
            vals.clear();
            op_values(op, &mut vals);
            for v in &vals {
                *occ.entry(*v).or_insert(0) += 1;
            }
        }
        if let Some(r) = f.ret {
            *occ.entry(r).or_insert(0) += 1;
        }
        let before = f.ops.len();
        f.ops.retain(|op| match op {
            Op::ConstInt { dst, .. } | Op::IntBinOp { dst, .. } => occ.get(dst) != Some(&1),
            Op::Prim {
                kind: PrimKind::Handle,
                dst: Some(d),
                ..
            } => occ.get(d) != Some(&1),
            _ => true,
        });
        if f.ops.len() == before {
            break;
        }
    }
}

/// Replace a family's WINDOW-side singleton `ListLit`s with their zero-filled
/// compact twins. The singleton dsts are exactly the trailing `m` Handle args
/// of every `__rgn_` call into the family (pass B1 built them for those calls
/// alone), so the scan is positional, not nominal.
///
/// Router only (codopsy round-3 sweep, #852): the call scan and the in-place
/// `ListLit` rewrite moved verbatim into the two named helpers below. The
/// per-host order is unchanged — collect the dsts over the WHOLE body first,
/// then rewrite, then sweep, so a `ListLit` that precedes its `__rgn_` call is
/// still caught.
fn compact_host_singletons(
    prog: &mut MirProgram,
    hosts: &BTreeSet<usize>,
    names: &BTreeSet<String>,
    m: usize,
    bytes: u32,
) {
    if m == 0 {
        return;
    }
    for &hf in hosts {
        let f = &mut prog.functions[hf];
        let sdsts = host_singleton_dsts(f, names, m);
        replace_singleton_list_lits(f, &sdsts, bytes);
        sweep_dead(f);
    }
}

/// Extracted verbatim from `compact_host_singletons` (codopsy round-3 sweep,
/// #852): the host's singleton dsts — the trailing `m` `Handle` args of every
/// `__rgn_` call whose ORIGINAL name belongs to the family. A call with fewer
/// than `m` args, or a non-`Handle` arg in the trailing window, contributes
/// nothing; the scan stays positional, not nominal.
fn host_singleton_dsts(f: &MirFunction, names: &BTreeSet<String>, m: usize) -> BTreeSet<ValueId> {
    let mut sdsts: BTreeSet<ValueId> = BTreeSet::new();
    for op in &f.ops {
        if let Op::CallFn { name, args, .. } = op {
            let Some(orig) = name.strip_prefix("__rgn_") else {
                continue;
            };
            if names.contains(orig) && args.len() >= m {
                for a in &args[args.len() - m..] {
                    if let crate::CallArg::Handle(v) = a {
                        sdsts.insert(*v);
                    }
                }
            }
        }
    }
    sdsts
}

/// Extracted verbatim from `compact_host_singletons` (codopsy round-3 sweep,
/// #852): rewrites every `ListLit` whose dst is a singleton into its
/// zero-filled compact twin, in place, leaving every other op untouched.
fn replace_singleton_list_lits(f: &mut MirFunction, sdsts: &BTreeSet<ValueId>, bytes: u32) {
    for op in f.ops.iter_mut() {
        if let Op::ListLit { dst, .. } = op {
            if sdsts.contains(dst) {
                *op = Op::Prim {
                    kind: PrimKind::RegionAllocC { bytes, zero: true },
                    dst: Some(*dst),
                    args: vec![],
                };
            }
        }
    }
}

/// Entry: one joint decision per clone family. `comps` pairs each family's
/// ORIGINAL member names with its singleton shape vector (region_alloc's
/// consolidation already made shapes agree across shared members) and the
/// window HOST function indices (where the singletons are built).
pub(crate) fn compact_clone_families(
    prog: &mut MirProgram,
    comps: &[(BTreeSet<String>, Vec<Vec<i64>>, BTreeSet<usize>)],
) {
    for (names, shapes, hosts) in comps {
        let idxs: Vec<usize> = names
            .iter()
            .filter_map(|n| {
                let cn = rgn_name(n);
                prog.functions.iter().position(|f| f.name == cn)
            })
            .collect();
        if idxs.len() != names.len() {
            continue;
        }
        let Some(plan) = analyze(prog, &idxs, shapes) else {
            continue;
        };
        let (_, bytes) = field_offsets(&plan.slots);
        apply(prog, &idxs, &plan, shapes);
        compact_host_singletons(prog, hosts, names, shapes.len(), bytes);
    }
}
