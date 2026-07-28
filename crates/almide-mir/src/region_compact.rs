//! Compact headerless block layout inside region clones (issue #838 stage 2).
//!
//! Stage 1 (region_alloc.rs) made allocation a pure bump and teardown a
//! frontier reset, but every variant node still carried the full canonical
//! block: `[rc][len][cap][tag:i64][field:i64…]` — 36 bytes for binarytrees'
//! `Node(Tree, Tree)` vs the 8 bytes an arena-style Rust program uses. Deep
//! trees are cache-miss-bound on TRAVERSAL, so the footprint — not the
//! instruction count — is the remaining gap (measured: hand-stripping the
//! header stores + tag bounds check from the clones moved binarytrees by ~1%;
//! the 4.5× footprint is everything).
//!
//! This pass rewrites qualified clone FAMILIES to a compact layout:
//!
//! - `Alloc [tag, f1, …]`   → `RegionAllocC { bytes }` — no header, no tag slot;
//!   handle fields pack as raw 4-byte pointers, scalars as 8-byte i64s.
//! - field store/load        → `RegionStore*/RegionLoad* { off }` at the packed
//!   offset (the `Handle`/`Add` address bridges die with the rewrite).
//! - tag read (elem 0)       → `RegionTagSel`: inside a region every variant
//!   value is either THE per-region nullary-ctor singleton (region_alloc pass
//!   B2 aliases every all-const `ListLit` to one shared block) or a compact
//!   dynamic block, and every block is a distinct bump address — pointer
//!   identity against the singleton IS the tag.
//!
//! SOUNDNESS — all of the following are mechanically verified per family (the
//! connected component of window closures; a shared clone body gets one joint
//! decision), on top of the stage-1 window guarantees (closed call graph, no
//! escapes, drops removed, frontier-reset teardown):
//!
//! 1. ONE dynamic shape: every `Alloc` in the family is `Init::DynList` with
//!    the same const elem count and the same const tag, and every alloc site
//!    stores every slot exactly once (straight-line construction). With the
//!    singletons this is the COMPLETE set of blocks a family value can be.
//! 2. Every raw memory op resolves: each `Load`/`LoadHandle`/`Store` address
//!    chases (through `IntBinOp::Add`-with-const and the `Handle` bridge) to
//!    `base + LIST_HEADER + 8k` with `k < n`, and each `ListGetScalar` index
//!    is const. Slot kinds (handle vs scalar) unify across every store and
//!    load. `Store` roots must be the family's own `Alloc` dsts (construction
//!    only — no field mutation).
//! 3. No other block source or address sink: any surviving `ListLit` (a
//!    non-const list), any other `Alloc` init, `ListSetScalar`, `ElemAddr`, or
//!    a chased address value used by ANY op outside its own chain (including
//!    the function result) rejects the family.
//! 4. Singletons are nullary: every singleton shape has exactly one element
//!    (its tag). A `RegionTagSel` hit then returns `shape[0]` — correct under
//!    BOTH readings (variant tag, or elem-0 of a const one-element list),
//!    so no path-sensitivity is needed. Field reads (k ≥ 1) can only execute
//!    on dynamic blocks: the ORIGINAL program's tag dispatch guards them, and
//!    the rewrite changes no control flow.
//!
//! Failing any check leaves the family on the stage-1 layout — correctness
//! never depends on qualification, only the speedup does. Like every render
//! pass, fidelity is covered by the differential gates (spec corpus,
//! diff-fuzz, output-parity).

use crate::region_alloc::{fn_tables, rgn_name};
use crate::render_wasm::{defined_value, op_values, ELEM_SIZE, LIST_HEADER};
use crate::{Init, IntOp, MirFunction, MirProgram, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// One planned op rewrite. `k` is the ORIGINAL element index (tag = 0).
#[derive(Clone)]
enum Rw {
    /// The dynamic-shape `Alloc` → `RegionAllocC`.
    AllocC { dst: ValueId },
    /// A construction store of the (elided) tag slot.
    Delete,
    StoreH { base: ValueId, val: ValueId, k: usize },
    StoreS { base: ValueId, val: ValueId, k: usize },
    LoadH { dst: ValueId, base: ValueId, k: usize },
    LoadS { dst: ValueId, base: ValueId, k: usize },
    /// An elem-0 read → the `RegionTagSel` chain over the singletons.
    TagRead { dst: ValueId, base: ValueId },
}

struct Plan {
    tag: i64,
    /// Per field slot (elem 1..n): true = handle (4 bytes), false = scalar (8).
    slots: Vec<bool>,
    rewrites: BTreeMap<(usize, usize), Rw>,
}

/// Packed field offsets + block size for a slot-kind vector. Handles are raw
/// 4-byte pointers; scalars are 8-byte i64s kept 8-aligned. `$alloc` is an
/// exact-byte bump, so the size needs no rounding beyond the 4-byte floor.
fn field_offsets(slots: &[bool]) -> (Vec<u32>, u32) {
    let mut offs = Vec::with_capacity(slots.len());
    let mut off: u32 = 0;
    for &is_handle in slots {
        if is_handle {
            offs.push(off);
            off += 4;
        } else {
            off = (off + 7) & !7;
            offs.push(off);
            off += 8;
        }
    }
    (offs, off.max(4))
}

/// Chase an address value through `Add`-with-const and the `Handle` bridge to
/// `(base_handle, elem_index)`, recording every traversed op/value for the
/// escape audit. `None` = unresolvable (rejects the family).
fn chase_elem(
    f: &MirFunction,
    def: &BTreeMap<ValueId, usize>,
    consts: &BTreeMap<ValueId, i64>,
    addr: ValueId,
    chain_ops: &mut BTreeSet<usize>,
    chain_vals: &mut BTreeSet<ValueId>,
) -> Option<(ValueId, usize)> {
    let header = LIST_HEADER as i64;
    let elem = ELEM_SIZE as i64;
    let mut off: i64 = 0;
    let mut v = addr;
    for _ in 0..12 {
        let &d = def.get(&v)?;
        match &f.ops[d] {
            Op::IntBinOp { op: IntOp::Add, a, b, .. } => {
                chain_ops.insert(d);
                chain_vals.insert(v);
                if let Some(&c) = consts.get(b) {
                    off += c;
                    v = *a;
                } else if let Some(&c) = consts.get(a) {
                    off += c;
                    v = *b;
                } else {
                    return None;
                }
            }
            Op::Prim { kind: PrimKind::Handle, args, .. } => {
                chain_ops.insert(d);
                chain_vals.insert(v);
                if off < header || (off - header) % elem != 0 {
                    return None;
                }
                return Some((args[0], ((off - header) / elem) as usize));
            }
            _ => return None,
        }
    }
    None
}

fn unify_slot(slots: &mut BTreeMap<usize, bool>, k: usize, is_handle: bool) -> Option<()> {
    match slots.get(&k) {
        Some(&prev) if prev != is_handle => None,
        _ => {
            slots.insert(k, is_handle);
            Some(())
        }
    }
}

fn analyze(prog: &MirProgram, idxs: &[usize], shapes: &[Vec<i64>]) -> Option<Plan> {
    let mut n_elems: Option<usize> = None;
    let mut tag: Option<i64> = None;
    let mut slot_map: BTreeMap<usize, bool> = BTreeMap::new();
    let mut rewrites: BTreeMap<(usize, usize), Rw> = BTreeMap::new();
    let mut max_k: usize = 0;
    // Deferred per-site construction coverage — validated against the FINAL
    // elem count (a consumer-only member may run before any alloc is seen).
    let mut all_covs: Vec<BTreeMap<usize, usize>> = Vec::new();

    for &fi in idxs {
        scan_region_function(
            prog, fi,
            &mut ScanState {
                n_elems: &mut n_elems, tag: &mut tag, slot_map: &mut slot_map,
                rewrites: &mut rewrites, max_k: &mut max_k, all_covs: &mut all_covs,
            },
        )?;
    }

    let n = n_elems?;
    if max_k >= n {
        return None;
    }
    // Straight-line construction: every slot of every alloc site stored
    // exactly once.
    for cov in &all_covs {
        if cov.len() != n || cov.values().any(|&c| c != 1) {
            return None;
        }
    }
    // Soundness §4: every singleton must be the max-arity-PADDED image of a
    // nullary ctor — same elem count as the dynamic shape, all-zero past the
    // tag. Its compact twin is a zero-filled block, so an in-family k ≥ 1 read
    // that does hit a singleton at runtime reads 0 under BOTH layouts, and a
    // tag read returns shape[0] via `RegionTagSel` — no path-sensitivity
    // needed. A const block with nonzero payload could be a REAL list whose
    // element reads the twin cannot reproduce — reject.
    if shapes.iter().any(|s| s.len() != n || s[1..].iter().any(|&v| v != 0)) {
        return None;
    }
    let slots: Vec<bool> = (1..n).map(|k| slot_map.get(&k).copied()).collect::<Option<_>>()?;
    Some(Plan { tag: tag?, slots, rewrites })
}

/// The analysis state one function's scan reads and extends.
///
/// Every field accumulates ACROSS the functions of a clone family — a family
/// shares one compact layout, so the element count, tag and slot map must agree
/// on all of them. Threading them as one value is what keeps `scan_region_function`
/// from being handed a partial set.
struct ScanState<'a> {
    /// The family's element count; `None` until the first alloc site fixes it.
    n_elems: &'a mut Option<usize>,
    /// The family's tag value.
    tag: &'a mut Option<i64>,
    /// Per-slot: whether the slot holds a handle.
    slot_map: &'a mut BTreeMap<usize, bool>,
    /// The rewrites the apply pass will perform, keyed by (function, op).
    rewrites: &'a mut BTreeMap<(usize, usize), Rw>,
    /// The highest slot index any in-family read touches.
    max_k: &'a mut usize,
    /// Per-site construction coverage, validated against the FINAL element
    /// count — a consumer-only member can run before any alloc is seen.
    all_covs: &'a mut Vec<BTreeMap<usize, usize>>,
}

/// The per-member scan workspace: the value tables `chase_elem` resolves
/// against, plus everything one function's scan accumulates for itself (the
/// chase audit trail and the construction coverage) — as opposed to the
/// family-wide `ScanState` it also extends.
///
/// Extracted from `scan_region_function` (codopsy r2, #852): these six values
/// were the one-body scan's plain locals; the per-arm deciders below share
/// them through this struct instead. (The old body also re-borrowed the
/// `ScanState` fields into plain locals to keep its `*n_elems = Some(n)` /
/// `*max_k` spellings; the deciders now read `st.` paths directly, so that
/// re-borrow is retired.)
struct MemberScan<'f> {
    /// The family member under scan.
    f: &'f MirFunction,
    /// value → defining-op index (from `fn_tables`).
    def: BTreeMap<ValueId, usize>,
    /// value → const-int (from `fn_tables`).
    consts: BTreeMap<ValueId, i64>,
    /// Every op a chase traversed — exempt from the escape audit.
    chain_ops: BTreeSet<usize>,
    /// Every value a chase traversed — nothing outside the chains may use one.
    chain_vals: BTreeSet<ValueId>,
    /// dst → per-slot construction-store count (must end exactly-once each).
    site_cov: BTreeMap<ValueId, BTreeMap<usize, usize>>,
}

/// Scan one function of a clone family.
///
/// `None` rejects the whole family: the layouts must agree across it, so a
/// single member that cannot be compacted disqualifies the rest.
///
/// Router only (codopsy r2, #852): the block-source pass, the four memory-op
/// arms, and the escape audit each moved verbatim into the named deciders
/// below. The op-kind dispatch keeps the ORIGINAL arm order — the two
/// wrong-width `Load`/`Store` rejections must stay behind their 8-byte
/// siblings.
fn scan_region_function(
    prog: &MirProgram,
    fi: usize,
    st: &mut ScanState<'_>,
) -> Option<()> {
    let f = &prog.functions[fi];
    let (def, consts) = fn_tables(f);
    let mut ms = MemberScan {
        f,
        def,
        consts,
        chain_ops: BTreeSet::new(),
        chain_vals: BTreeSet::new(),
        site_cov: BTreeMap::new(),
    };

    scan_block_sources(&mut ms, st, fi)?;

    for (i, op) in f.ops.iter().enumerate() {
        match op {
            Op::ListGetScalar { dst, list, idx } => {
                scan_list_get_scalar(&ms, st, (fi, i), *dst, *list, *idx)?;
            }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } => {
                scan_handle_load(&mut ms, st, (fi, i), *d, args)?;
            }
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(d), args } => {
                scan_scalar_load(&mut ms, st, (fi, i), *d, args)?;
            }
            Op::Prim { kind: PrimKind::Load { .. }, .. } => return None,
            Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } => {
                scan_construction_store(&mut ms, st, (fi, i), args)?;
            }
            Op::Prim { kind: PrimKind::Store { .. }, .. } => return None,
            _ => {}
        }
    }

    audit_chain_escapes(&ms, st, fi)?;
    st.all_covs.extend(ms.site_cov.into_values());
    Some(())
}

/// Block sources first: exactly the DynList-const shape, nothing else.
///
/// Extracted from `scan_region_function` (codopsy r2, #852): decides the
/// family's element count from each dynamic `Alloc` site (all sites must
/// agree, 1..=16), opens the site's construction-coverage map, and plans the
/// `RegionAllocC` rewrite; any other block source — another `Alloc` init, a
/// surviving `ListLit`, `ListSetScalar`, or `ElemAddr` — rejects the family
/// (soundness §1/§3). Verbatim.
fn scan_block_sources(ms: &mut MemberScan<'_>, st: &mut ScanState<'_>, fi: usize) -> Option<()> {
    let f = ms.f;
    for (i, op) in f.ops.iter().enumerate() {
        match op {
            Op::Alloc { dst, init: Init::DynList { len }, .. } => {
                let &n = ms.consts.get(len)?;
                let n = usize::try_from(n).ok().filter(|n| (1..=16).contains(n))?;
                match st.n_elems {
                    Some(prev) if *prev != n => return None,
                    _ => *st.n_elems = Some(n),
                }
                ms.site_cov.insert(*dst, BTreeMap::new());
                st.rewrites.insert((fi, i), Rw::AllocC { dst: *dst });
            }
            Op::Alloc { .. } | Op::ListLit { .. } | Op::ListSetScalar { .. } => return None,
            Op::Prim { kind: PrimKind::ElemAddr, .. } => return None,
            _ => {}
        }
    }
    Some(())
}

/// Extracted from `scan_region_function` (codopsy r2, #852): the
/// `ListGetScalar` arm — a const-index read is either the tag (`k == 0`,
/// planned as the `RegionTagSel` chain) or a field load (`k ≥ 1`, unified as
/// a scalar slot). A non-const index rejects the family. Verbatim.
fn scan_list_get_scalar(
    ms: &MemberScan<'_>,
    st: &mut ScanState<'_>,
    key: (usize, usize),
    dst: ValueId,
    list: ValueId,
    idx: ValueId,
) -> Option<()> {
    let &k = ms.consts.get(&idx)?;
    let k = usize::try_from(k).ok()?;
    if k == 0 {
        st.rewrites.insert(key, Rw::TagRead { dst, base: list });
    } else {
        *st.max_k = (*st.max_k).max(k);
        unify_slot(st.slot_map, k, false)?;
        st.rewrites.insert(key, Rw::LoadS { dst, base: list, k });
    }
    Some(())
}

/// Extracted from `scan_region_function` (codopsy r2, #852): the `LoadHandle`
/// arm — chase the address to `(base, k)`, reject a tag-slot hit, unify slot
/// `k` as a handle, and plan the `RegionLoadH` rewrite. Verbatim.
fn scan_handle_load(
    ms: &mut MemberScan<'_>,
    st: &mut ScanState<'_>,
    key: (usize, usize),
    d: ValueId,
    args: &[ValueId],
) -> Option<()> {
    let (base, k) =
        chase_elem(ms.f, &ms.def, &ms.consts, args[0], &mut ms.chain_ops, &mut ms.chain_vals)?;
    if k == 0 {
        return None; // a tag is never a handle
    }
    *st.max_k = (*st.max_k).max(k);
    unify_slot(st.slot_map, k, true)?;
    st.rewrites.insert(key, Rw::LoadH { dst: d, base, k });
    Some(())
}

/// Extracted from `scan_region_function` (codopsy r2, #852): the 8-byte `Load`
/// arm — chase the address to `(base, k)`; elem 0 is a tag read, any other
/// slot unifies as a scalar and plans the `RegionLoadS` rewrite. Verbatim.
fn scan_scalar_load(
    ms: &mut MemberScan<'_>,
    st: &mut ScanState<'_>,
    key: (usize, usize),
    d: ValueId,
    args: &[ValueId],
) -> Option<()> {
    let (base, k) =
        chase_elem(ms.f, &ms.def, &ms.consts, args[0], &mut ms.chain_ops, &mut ms.chain_vals)?;
    if k == 0 {
        st.rewrites.insert(key, Rw::TagRead { dst: d, base });
    } else {
        *st.max_k = (*st.max_k).max(k);
        unify_slot(st.slot_map, k, false)?;
        st.rewrites.insert(key, Rw::LoadS { dst: d, base, k });
    }
    Some(())
}

/// Extracted from `scan_region_function` (codopsy r2, #852): the 8-byte
/// `Store` arm — a construction store into the family's own fresh block. A
/// tag-slot store (`k == 0`) must carry the family's one const tag and is
/// planned away (`Rw::Delete`); a field store unifies the slot as handle or
/// scalar depending on whether the stored value is a `Handle` bridge.
/// Verbatim.
fn scan_construction_store(
    ms: &mut MemberScan<'_>,
    st: &mut ScanState<'_>,
    key: (usize, usize),
    args: &[ValueId],
) -> Option<()> {
    let (base, k) =
        chase_elem(ms.f, &ms.def, &ms.consts, args[0], &mut ms.chain_ops, &mut ms.chain_vals)?;
    // Construction only: the root must be this family's own
    // fresh Alloc (no writes into loaded/param blocks).
    let cov = ms.site_cov.get_mut(&base)?;
    *cov.entry(k).or_insert(0) += 1;
    if k == 0 {
        let &t = ms.consts.get(&args[1])?;
        match st.tag {
            Some(prev) if *prev != t => return None,
            _ => *st.tag = Some(t),
        }
        st.rewrites.insert(key, Rw::Delete);
    } else if let Some(&hd) = ms.def.get(&args[1]) {
        *st.max_k = (*st.max_k).max(k);
        if let Op::Prim { kind: PrimKind::Handle, args: hargs, .. } = &ms.f.ops[hd] {
            // A handle bridged to i64 for the old 8-byte slot:
            // store the raw i32 pointer instead.
            ms.chain_ops.insert(hd);
            ms.chain_vals.insert(args[1]);
            unify_slot(st.slot_map, k, true)?;
            st.rewrites
                .insert(key, Rw::StoreH { base, val: hargs[0], k });
        } else {
            unify_slot(st.slot_map, k, false)?;
            st.rewrites.insert(key, Rw::StoreS { base, val: args[1], k });
        }
    } else {
        *st.max_k = (*st.max_k).max(k);
        unify_slot(st.slot_map, k, false)?;
        st.rewrites.insert(key, Rw::StoreS { base, val: args[1], k });
    }
    Some(())
}

/// Escape audit: a chased address (or bridged handle-int) may only be
/// consumed by its own chain or by an op this pass rewrites away.
///
/// Extracted from `scan_region_function` (codopsy r2, #852): rejects the
/// family (soundness §3) when any op outside the chains — or the function
/// result — consumes a chained value. Verbatim.
fn audit_chain_escapes(ms: &MemberScan<'_>, st: &ScanState<'_>, fi: usize) -> Option<()> {
    let mut vals: Vec<ValueId> = Vec::new();
    for (i, op) in ms.f.ops.iter().enumerate() {
        if ms.chain_ops.contains(&i) || st.rewrites.contains_key(&(fi, i)) {
            continue;
        }
        vals.clear();
        op_values(op, &mut vals);
        let dst = defined_value(op);
        if vals.iter().any(|v| Some(*v) != dst && ms.chain_vals.contains(v)) {
            return None;
        }
    }
    if let Some(r) = ms.f.ret {
        if ms.chain_vals.contains(&r) {
            return None;
        }
    }
    Some(())
}

/// Everything one planned rewrite needs in order to become ops.
///
/// The first three fields are the family's compact layout (the packed field
/// offsets, the block size, the one const tag); `shapes` are the family's
/// singleton shapes and `sing` the singleton params of the member currently
/// being rewritten. Bundling them is what keeps `compact_replacement` and
/// `tag_select_chain` off a seven-value argument list (the `MemberScan`
/// precedent above).
struct CompactSite<'a> {
    /// Packed byte offset of each FIELD slot (elem 1..n), from `field_offsets`.
    offs: &'a [u32],
    /// The compact block size in bytes, from `field_offsets`.
    bytes: u32,
    /// The family's one const tag — the dynamic-block answer of a tag read.
    tag: i64,
    /// The family's singleton shapes, in the order pass B2 appended them.
    shapes: &'a [Vec<i64>],
    /// This member's singleton params, positionally aligned with `shapes`.
    sing: &'a [ValueId],
}

/// Rewrite one clone family's planned ops to the compact layout.
///
/// Router only (codopsy round-3 sweep, #852): the fresh-id watermark scan, the
/// singleton param slice, the per-`Rw` op construction and the tag-select chain
/// each moved verbatim into the named helpers below. The planned rewrites are
/// still walked in REVERSE op order, so a splice never shifts an index that is
/// still pending, and `max_id` still threads across the whole member so every
/// tag chain in it mints distinct ids.
fn apply(prog: &mut MirProgram, idxs: &[usize], plan: &Plan, shapes: &[Vec<i64>]) {
    let (offs, bytes) = field_offsets(&plan.slots);
    for &fi in idxs {
        let f = &mut prog.functions[fi];
        let m = shapes.len();
        let sing = trailing_singleton_params(f, m);
        let mut max_id: u32 = max_value_id(f);

        let items: Vec<(usize, Rw)> = plan
            .rewrites
            .range((fi, 0)..=(fi, usize::MAX))
            .map(|((_, i), rw)| (*i, rw.clone()))
            .collect();
        let site = CompactSite { offs: &offs, bytes, tag: plan.tag, shapes, sing: &sing };
        for (i, rw) in items.into_iter().rev() {
            let repl: Vec<Op> = compact_replacement(&site, rw, &mut max_id);
            f.ops.splice(i..=i, repl);
        }
        sweep_dead(f);
    }
}

/// Extracted verbatim from `apply` (codopsy round-3 sweep, #852): the member's
/// own singleton parameter values, positionally aligned with the family's
/// shape vector.
fn trailing_singleton_params(f: &MirFunction, m: usize) -> Vec<ValueId> {
    // Pass B2 appended the singleton params last, in shape order.
    f.params[f.params.len() - m..].iter().map(|p| p.value).collect()
}

/// Extracted verbatim from `apply` (codopsy round-3 sweep, #852): the highest
/// `ValueId` the function mentions, across its ops and its params — the
/// watermark the tag-select chain mints fresh ids above.
fn max_value_id(f: &MirFunction) -> u32 {
    let mut max_id: u32 = 0;
    let mut vals: Vec<ValueId> = Vec::new();
    for op in &f.ops {
        vals.clear();
        op_values(op, &mut vals);
        for v in &vals {
            max_id = max_id.max(v.0);
        }
    }
    for p in &f.params {
        max_id = max_id.max(p.value.0);
    }
    max_id
}

/// Extracted verbatim from `apply` (codopsy round-3 sweep, #852): the ops one
/// planned rewrite expands to. Arm order and every emitted op are unchanged;
/// `max_id` is threaded through so the tag chain keeps handing out fresh ids in
/// the same sequence as when this match sat inline.
fn compact_replacement(site: &CompactSite<'_>, rw: Rw, max_id: &mut u32) -> Vec<Op> {
    match rw {
        Rw::Delete => vec![],
        Rw::AllocC { dst } => vec![Op::Prim {
            kind: PrimKind::RegionAllocC { bytes: site.bytes, zero: false },
            dst: Some(dst),
            args: vec![],
        }],
        Rw::StoreH { base, val, k } => vec![Op::Prim {
            kind: PrimKind::RegionStoreH { off: site.offs[k - 1] },
            dst: None,
            args: vec![base, val],
        }],
        Rw::StoreS { base, val, k } => vec![Op::Prim {
            kind: PrimKind::RegionStoreS { off: site.offs[k - 1] },
            dst: None,
            args: vec![base, val],
        }],
        Rw::LoadH { dst, base, k } => vec![Op::Prim {
            kind: PrimKind::RegionLoadH { off: site.offs[k - 1] },
            dst: Some(dst),
            args: vec![base],
        }],
        Rw::LoadS { dst, base, k } => vec![Op::Prim {
            kind: PrimKind::RegionLoadS { off: site.offs[k - 1] },
            dst: Some(dst),
            args: vec![base],
        }],
        Rw::TagRead { dst, base } => tag_select_chain(site, dst, base, max_id),
    }
}

/// Extracted verbatim from `apply` (codopsy round-3 sweep, #852): an elem-0
/// read becomes the `RegionTagSel` chain over the family's singletons. With no
/// singletons the family's one const tag is the whole answer. Otherwise the
/// chain is built innermost-first — the dynamic-block tag is the default and
/// each shape wraps it in reverse shape order, so the LAST op emitted is the
/// shape-0 test and it is the one that defines `dst`.
fn tag_select_chain(
    site: &CompactSite<'_>,
    dst: ValueId,
    base: ValueId,
    max_id: &mut u32,
) -> Vec<Op> {
    let m = site.shapes.len();
    if m == 0 {
        return vec![Op::ConstInt { dst, value: site.tag }];
    }
    // dst = (base==s0) ? tag0 : ((base==s1) ? tag1 : dyn)
    let mut seq = Vec::with_capacity(m + 1);
    *max_id += 1;
    let mut cur = ValueId(*max_id);
    seq.push(Op::ConstInt { dst: cur, value: site.tag });
    for (j, s) in site.shapes.iter().enumerate().rev() {
        let d = if j == 0 {
            dst
        } else {
            *max_id += 1;
            ValueId(*max_id)
        };
        seq.push(Op::Prim {
            kind: PrimKind::RegionTagSel { tag: s[0] },
            dst: Some(d),
            args: vec![base, site.sing[j], cur],
        });
        cur = d;
    }
    seq
}

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
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), .. } => occ.get(d) != Some(&1),
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
            let Some(orig) = name.strip_prefix("__rgn_") else { continue };
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
        let Some(plan) = analyze(prog, &idxs, shapes) else { continue };
        let (_, bytes) = field_offsets(&plan.slots);
        apply(prog, &idxs, &plan, shapes);
        compact_host_singletons(prog, hosts, names, shapes.len(), bytes);
    }
}
