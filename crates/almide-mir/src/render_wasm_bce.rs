// #806 step 4: bounds-check elision for hot loops — render-level LOOP
// VERSIONING. Every `v[i]` in a hot loop pays an inline range check (~5 ALU
// ops + a conditional trap); LLVM removes the equivalent checks from
// Rust-native loops, and the check alone measured ~28% of spectralnorm's and
// ~39% of fannkuchredux's wasm inner loops. Cranelift cannot remove them
// (it must preserve the trap), so the renderer does, with a shape that keeps
// correctness UNCONDITIONAL:
//
//   (if <guard: every elidable index provably in [0, len) for the whole run>
//     (then <the loop, elided accesses rendered UNCHECKED>)
//     (else <the loop, rendered exactly as today — every check intact>))
//
// The guard is evaluated ONCE at loop entry. A TRUE guard proves no elided
// check could ever fire, so removing them is observation-equivalent; a FALSE
// guard (including any case the analysis cannot see through — aliasing,
// overflow near i64::MAX, a negative entry index) falls into the byte-exact
// original loop. Guard imprecision can therefore only COST SPEED, never
// correctness — there is no early-trap, no reordering, and the MIR (and its
// Perceus certificate) is untouched: this is a render-level transform like
// the #806 br_if fusion and f64-local classification.
//
// What makes the guard sound for the WHOLE loop from one entry evaluation:
// - The region admits NO op that can change any list's length or free a
//   block: no calls, no allocs, no drops/dups, no raw-memory prims
//   (`ListSetScalar` writes an element slot, never the header — in the fast
//   copy its index is exactly what the guard proved in-bounds). So `len(L)`
//   read at entry holds for every iteration.
// - Indices are bounded by the loop's own top-tested break compare over
//   monotone induction variables (see `Ind`), or are compile-time constants.
//   Monotonicity comes from the variable having exactly ONE write in the
//   region: `SetLocal v ← v ± <positive const>`.
// - An induction-relative access must sit AFTER the break check and BEFORE
//   the increment, so the compare's bound holds at the access point.
// - Guard arithmetic avoids wrap: runtime entry values are only ever
//   COMPARED against constants or against `len - c` where `len < 2^31` (an
//   i32 header field, u-extended) and `|c| ≤ MAX_BCE_OFFSET` — never added
//   to anything unbounded.

/// One versionable loop: the region `[LoopStart .. end_idx]`, the entry
/// guard (an i32 WAT expression over already-materialized locals), and the
/// op indices whose bounds check the FAST copy may omit.
pub(crate) struct BcePlan {
    pub(crate) end_idx: usize,
    pub(crate) guard: String,
    pub(crate) elide: BTreeSet<usize>,
}

/// Offsets beyond this are rejected (they'd be degenerate code anyway); the
/// cap keeps every `len - (c+1)` / `-cmin` guard constant trivially wrap-free.
const MAX_BCE_OFFSET: i64 = 4096;
/// A versioned loop is emitted twice; cap the duplication.
const MAX_BCE_REGION_OPS: usize = 400;

/// The loop's induction shape, extracted from the FIRST `LoopBreakUnless`'s
/// compare (canonicalized to `x < y` / `x ≤ y` = "continue while"):
/// - `Up`: `j < n` (or `≤`), `j` stepped only by `j += c` (c > 0), `n` invariant.
/// - `Down`: `m < j` (or `≤`), `j` stepped only by `j -= c` (c > 0), `m` invariant.
/// - `Two`: `lo < hi` (or `≤`), `lo` stepped up, `hi` stepped down — both stay
///   inside `[lo₀, hi₀]` at every access point (fannkuch's flip/swap shape).
enum Ind {
    Up { j: ValueId, n: ValueId, le: bool, incr: usize },
    Down { j: ValueId, m: ValueId, le: bool, incr: usize },
    Two { lo: ValueId, hi: ValueId, incr_lo: usize, incr_hi: usize },
}

/// Per-list guard requirements accumulated over its elidable accesses.
#[derive(Default)]
struct ListReq {
    /// Largest compile-time-constant index (needs `len > max_const`).
    max_const: Option<i64>,
    /// Induction-relative offset range (needs the `Ind`-shaped bounds).
    offsets: Option<(i64, i64)>,
}

impl ListReq {
    fn add_const(&mut self, c: i64) {
        self.max_const = Some(self.max_const.map_or(c, |m: i64| m.max(c)));
    }
    fn add_offset(&mut self, c: i64) {
        self.offsets = Some(match self.offsets {
            None => (c, c),
            Some((lo, hi)) => (lo.min(c), hi.max(c)),
        });
    }
}

/// Scan `func` for innermost loops that can be versioned; returns the plans
/// keyed by their `LoopStart` op index. Pure analysis — the caller
/// (`render_op_range`) does the two-copy emission.
pub(crate) fn analyze_bce(func: &MirFunction) -> BTreeMap<usize, BcePlan> {
    // SSA-consts: `ConstInt` dsts never reassigned (same rule as Fuser::scan_consts).
    let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
    for op in &func.ops {
        if let Op::ConstInt { dst, value } = op {
            consts.insert(*dst, *value);
        }
    }
    let mut setlocal_targets: BTreeSet<ValueId> = BTreeSet::new();
    for op in &func.ops {
        if let Op::SetLocal { local, .. } = op {
            consts.remove(local);
            setlocal_targets.insert(*local);
        }
    }
    let mut def_count: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut def_site: BTreeMap<ValueId, usize> = BTreeMap::new();
    for (i, op) in func.ops.iter().enumerate() {
        if let Some(d) = defined_value(op) {
            *def_count.entry(d).or_insert(0) += 1;
            def_site.insert(d, i);
        }
    }
    let tables = BceTables { consts, setlocal_targets, def_count, def_site };

    // EVERY loop level may get a plan — a nested planned loop re-applies its
    // own guard inside the enclosing copy (render_op_range composes the elide
    // sets), so e.g. fannkuch's flip loop elides its `perm[0]` while the swap
    // loop inside it keeps its own two-var plan.
    let mut plans = BTreeMap::new();
    let mut stack: Vec<usize> = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => stack.push(i),
            Op::LoopEnd => {
                let s = stack.pop().expect("LoopEnd without LoopStart");
                if let Some(p) = analyze_bce_region(func, s, i, &tables) {
                    plans.insert(s, p);
                }
            }
            _ => {}
        }
    }
    plans
}

struct BceTables {
    consts: BTreeMap<ValueId, i64>,
    setlocal_targets: BTreeSet<ValueId>,
    def_count: BTreeMap<ValueId, usize>,
    def_site: BTreeMap<ValueId, usize>,
}

/// Region-local write map: `SetLocal` sites and op-def sites per value, plus
/// each op's LOOP DEPTH relative to the region root (0 = the root's own
/// body, 1+ = inside a nested loop).
struct BceRegion {
    set: BTreeMap<ValueId, Vec<(usize, ValueId)>>,
    def: BTreeMap<ValueId, Vec<usize>>,
    first_break: Option<usize>,
    loop_depth: BTreeMap<usize, u32>,
}

impl BceRegion {
    fn invariant(&self, v: ValueId) -> bool {
        !self.set.contains_key(&v) && !self.def.contains_key(&v)
    }
    fn at_root_depth(&self, i: usize) -> bool {
        self.loop_depth.get(&i).copied() == Some(0)
    }
}

/// Whitelist + collect the region's write sites. `None` = an op that could
/// change a list length, free a block, or observe anything (call/alloc/drop/
/// raw-memory prim) — the loop cannot be versioned. Nested loops ARE allowed
/// (the whitelist applies to their bodies too, flat, so length invariance
/// still holds region-wide); the depth map lets the induction analysis
/// restrict itself to root-depth positions.
fn scan_bce_region(func: &MirFunction, s: usize, e: usize) -> Option<BceRegion> {
    let mut r = BceRegion {
        set: BTreeMap::new(),
        def: BTreeMap::new(),
        first_break: None,
        loop_depth: BTreeMap::new(),
    };
    let mut ldepth: u32 = 0;
    let mut idepth: u32 = 0;
    for i in s + 1..e {
        let op = &func.ops[i];
        match op {
            Op::ConstInt { .. }
            | Op::Const { .. }
            | Op::IntBinOp { .. }
            | Op::ListGetScalar { .. }
            | Op::ListSetScalar { .. }
            | Op::SetLocal { .. } => {}
            Op::IfThen { .. } => idepth += 1,
            Op::EndIf { .. } => idepth = idepth.saturating_sub(1),
            Op::Else { .. } => {}
            Op::LoopStart => ldepth += 1,
            Op::LoopEnd => ldepth = ldepth.saturating_sub(1),
            Op::LoopBreakUnless { .. } => {
                // The bound the induction argument leans on must be checked on
                // EVERY iteration of the ROOT loop: only a break at loop depth
                // 0 outside any `if` arm qualifies. Extra/nested/conditional
                // breaks only SHRINK the iteration space — always safe to keep.
                if r.first_break.is_none() && ldepth == 0 && idepth == 0 {
                    r.first_break = Some(i);
                }
            }
            // Scalar float prims: pure register arithmetic, no memory, no trap.
            Op::Prim { kind, .. } => match kind {
                PrimKind::FloatUn(_)
                | PrimKind::FloatBin(_)
                | PrimKind::FloatCmp(_)
                | PrimKind::F64FromInt
                | PrimKind::IntToFloat
                | PrimKind::FloatToInt
                | PrimKind::FloatBits
                | PrimKind::F32Demote
                | PrimKind::F32Promote
                | PrimKind::IntToF32
                | PrimKind::F32Bits
                | PrimKind::F32Bin(_)
                | PrimKind::F32Cmp(_)
                | PrimKind::F32Un(_) => {}
                _ => return None,
            },
            _ => return None,
        }
        if let Some(d) = defined_value(op) {
            r.def.entry(d).or_default().push(i);
        }
        if let Op::SetLocal { local, src } = op {
            r.set.entry(*local).or_default().push((i, *src));
        }
        r.loop_depth.insert(i, ldepth);
    }
    Some(r)
}

/// `v` is a clean induction variable in the region: written EXACTLY once,
/// by `SetLocal v ← v ± <positive const>` whose step expression is a unique,
/// in-region, never-reassigned def. Returns `(is_up, setlocal_idx)`.
fn bce_step_of(
    func: &MirFunction,
    region: &BceRegion,
    t: &BceTables,
    s: usize,
    e: usize,
    v: ValueId,
) -> Option<(bool, usize)> {
    let sets = region.set.get(&v)?;
    if sets.len() != 1 || region.def.contains_key(&v) {
        return None;
    }
    let (sl_idx, src) = sets[0];
    // The single step must sit in the ROOT body: inside a nested loop it
    // would advance the variable repeatedly between two root break checks,
    // voiding the "bounded at every root-depth access" argument.
    if !region.at_root_depth(sl_idx) {
        return None;
    }
    if t.setlocal_targets.contains(&src) || t.def_count.get(&src) != Some(&1) {
        return None;
    }
    let d = *t.def_site.get(&src)?;
    if d <= s || d >= e || d >= sl_idx {
        return None;
    }
    match &func.ops[d] {
        Op::IntBinOp { op: IntOp::Add, a, b, .. } => {
            let c = if *a == v {
                t.consts.get(b)
            } else if *b == v {
                t.consts.get(a)
            } else {
                None
            };
            (c.copied().unwrap_or(0) > 0).then_some((true, sl_idx))
        }
        Op::IntBinOp { op: IntOp::Sub, a, b, .. } if *a == v => {
            (t.consts.get(b).copied().unwrap_or(0) > 0).then_some((false, sl_idx))
        }
        _ => None,
    }
}

/// Extract the induction shape from the first break's compare (which must be
/// the op immediately before it — the same shape the br_if fusion targets).
fn bce_induction(
    func: &MirFunction,
    region: &BceRegion,
    t: &BceTables,
    s: usize,
    e: usize,
) -> Option<Ind> {
    let k = region.first_break?;
    let Op::LoopBreakUnless { cond } = &func.ops[k] else { return None };
    if k == s + 1 {
        return None;
    }
    let Op::IntBinOp { dst, op, a, b } = &func.ops[k - 1] else { return None };
    if dst != cond {
        return None;
    }
    // Canonicalize to "continue while x < y" (`le` = the ≤ variants).
    let (le, x, y) = match op {
        IntOp::Lt => (false, *a, *b),
        IntOp::Le => (true, *a, *b),
        IntOp::Gt => (false, *b, *a),
        IntOp::Ge => (true, *b, *a),
        _ => return None,
    };
    match (bce_step_of(func, region, t, s, e, x), bce_step_of(func, region, t, s, e, y)) {
        (Some((true, ix)), None) if region.invariant(y) => {
            Some(Ind::Up { j: x, n: y, le, incr: ix })
        }
        (None, Some((false, iy))) if region.invariant(x) => {
            Some(Ind::Down { j: y, m: x, le, incr: iy })
        }
        (Some((true, ix)), Some((false, iy))) => {
            Some(Ind::Two { lo: x, hi: y, incr_lo: ix, incr_hi: iy })
        }
        _ => None,
    }
}

/// The last op index at which an induction-relative access may sit: before
/// every increment of the variables the bound argument leans on.
fn bce_incr_floor(ind: &Ind) -> usize {
    match ind {
        Ind::Up { incr, .. } | Ind::Down { incr, .. } => *incr,
        Ind::Two { incr_lo, incr_hi, .. } => (*incr_lo).min(*incr_hi),
    }
}

/// The induction variable(s) an index local may equal directly.
fn bce_is_ind_var(ind: &Ind, v: ValueId) -> bool {
    match ind {
        Ind::Up { j, .. } | Ind::Down { j, .. } => *j == v,
        Ind::Two { lo, hi, .. } => *lo == v || *hi == v,
    }
}

/// Resolve an access index against the induction shape: the variable itself
/// (offset 0) or a unique in-region `var ± const` def computed before use.
fn bce_offset_of(
    func: &MirFunction,
    t: &BceTables,
    ind: &Ind,
    s: usize,
    p: usize,
    ix: ValueId,
) -> Option<i64> {
    if bce_is_ind_var(ind, ix) {
        return Some(0);
    }
    if t.setlocal_targets.contains(&ix) || t.def_count.get(&ix) != Some(&1) {
        return None;
    }
    let d = *t.def_site.get(&ix)?;
    if d <= s || d >= p {
        return None;
    }
    let off = match &func.ops[d] {
        Op::IntBinOp { op: IntOp::Add, a, b, .. } => {
            if bce_is_ind_var(ind, *a) {
                *t.consts.get(b)?
            } else if bce_is_ind_var(ind, *b) {
                *t.consts.get(a)?
            } else {
                return None;
            }
        }
        Op::IntBinOp { op: IntOp::Sub, a, b, .. } if bce_is_ind_var(ind, *a) => {
            -*t.consts.get(b)?
        }
        _ => return None,
    };
    (off.abs() <= MAX_BCE_OFFSET).then_some(off)
}

/// Analyze one innermost loop region `[s..e]`; `Some` = a versionable plan.
fn analyze_bce_region(
    func: &MirFunction,
    s: usize,
    e: usize,
    t: &BceTables,
) -> Option<BcePlan> {
    if e - s > MAX_BCE_REGION_OPS {
        return None;
    }
    let region = scan_bce_region(func, s, e)?;
    let ind = bce_induction(func, &region, t, s, e);

    let mut reqs: BTreeMap<ValueId, ListReq> = BTreeMap::new();
    let mut elide: BTreeSet<usize> = BTreeSet::new();
    for p in s + 1..e {
        let (l, ix) = match &func.ops[p] {
            Op::ListGetScalar { list, idx, .. } => (*list, *idx),
            Op::ListSetScalar { list, idx, .. } => (*list, *idx),
            _ => continue,
        };
        if !region.invariant(l) {
            continue;
        }
        if let Some(c) = t.consts.get(&ix).copied() {
            // A constant index needs only `len > c` — any loop shape.
            if (0..=MAX_BCE_OFFSET).contains(&c) {
                reqs.entry(l).or_default().add_const(c);
                elide.insert(p);
            }
            continue;
        }
        let Some(ind) = &ind else { continue };
        // The break's bound holds only between the check and the increment,
        // and only at ROOT depth (inside a nested loop the induction variable
        // is frozen, but the position-order argument needs straight-line
        // execution from the break to the access within one root iteration —
        // depth 0 gives exactly that).
        if p <= region.first_break.unwrap_or(usize::MAX)
            || p >= bce_incr_floor(ind)
            || !region.at_root_depth(p)
        {
            continue;
        }
        if let Some(c) = bce_offset_of(func, t, ind, s, p, ix) {
            reqs.entry(l).or_default().add_offset(c);
            elide.insert(p);
        }
    }
    if elide.is_empty() {
        return None;
    }

    let mut conds: Vec<String> = Vec::new();
    for (l, rq) in &reqs {
        let len64 = format!(
            "(i64.extend_i32_u (i32.load (i32.add (local.get {}) (i32.const {LIST_LEN_OFFSET}))))",
            local(*l)
        );
        if let Some(c) = rq.max_const {
            conds.push(format!("(i64.lt_s (i64.const {c}) {len64})"));
        }
        if let Some((cmin, cmax)) = rq.offsets {
            match ind.as_ref().expect("offset access implies induction shape") {
                Ind::Up { j, n, le, .. } => {
                    conds.push(format!(
                        "(i64.ge_s (local.get {}) (i64.const {}))",
                        local(*j),
                        -cmin
                    ));
                    conds.push(format!(
                        "(i64.le_s (local.get {}) (i64.sub {len64} (i64.const {})))",
                        local(*n),
                        cmax + i64::from(*le)
                    ));
                }
                Ind::Down { j, m, le, .. } => {
                    conds.push(format!(
                        "(i64.ge_s (local.get {}) (i64.const {}))",
                        local(*m),
                        -(cmin + i64::from(!*le))
                    ));
                    conds.push(format!(
                        "(i64.le_s (local.get {}) (i64.sub {len64} (i64.const {})))",
                        local(*j),
                        cmax + 1
                    ));
                }
                Ind::Two { lo, hi, .. } => {
                    conds.push(format!(
                        "(i64.ge_s (local.get {}) (i64.const {}))",
                        local(*lo),
                        -cmin
                    ));
                    conds.push(format!(
                        "(i64.le_s (local.get {}) (i64.sub {len64} (i64.const {})))",
                        local(*hi),
                        cmax + 1
                    ));
                }
            }
        }
    }
    let guard = conds
        .into_iter()
        .reduce(|a, b| format!("(i32.and {a} {b})"))
        .expect("non-empty elide implies at least one condition");
    Some(BcePlan { end_idx: e, guard, elide })
}

/// The UNCHECKED render of a scalar element access — byte-identical to the
/// checked form (`render_op_alloc_lit`) minus the range check the loop guard
/// already discharged.
pub(crate) fn render_list_access_unchecked(op: &Op, floats: &BTreeSet<ValueId>) -> String {
    match op {
        Op::ListGetScalar { dst, list, idx } => {
            let load = if floats.contains(dst) { "f64.load" } else { "i64.load" };
            format!(
                "    (local.set {d} ({load} (i32.add (i32.add (local.get {l}) (i32.const {LIST_HEADER}))\n\
                 \x20                                     (i32.mul (i32.wrap_i64 (local.get {i})) (i32.const {ELEM_SIZE})))))\n",
                d = local(*dst),
                l = local(*list),
                i = local(*idx),
            )
        }
        Op::ListSetScalar { list, idx, val } => {
            let store = if floats.contains(val) { "f64.store" } else { "i64.store" };
            format!(
                "    ({store} (i32.add (i32.add (local.get {l}) (i32.const {LIST_HEADER}))\n\
                 \x20                       (i32.mul (i32.wrap_i64 (local.get {i})) (i32.const {ELEM_SIZE})))\n\
                 \x20              (local.get {v}))\n",
                l = local(*list),
                i = local(*idx),
                v = local(*val),
            )
        }
        _ => unreachable!("render_list_access_unchecked: {op:?} is not a list access"),
    }
}
