
/// Per-object refcount-event accumulator, preserving object creation order.
struct Streams {
    of: BTreeMap<ValueId, ValueId>, // handle → object representative
    order: Vec<ValueId>,            // objects in first-seen order
    stream: BTreeMap<ValueId, String>,
    frames: Vec<BranchFrame>, // open IfThen regions, innermost last
    /// A region flush emitted the always-rejecting POISON `{i|}` (an arm
    /// containing a nested region delimiter cannot be represented flat).
    /// The poison REPLACES the arm's real events, so any COUNT read off a
    /// poisoned certificate is meaningless — `plus_one_events_backed` must
    /// skip its equality (#1146; the kernel-proven checker still rejects the
    /// poisoned cert itself, which is the poison's whole job).
    poisoned: bool,
}

fn seg_net(seg: &str) -> i64 {
    seg.chars()
        .map(|c| match c {
            'i' | 'a' => 1,
            'd' | 'm' => -1,
            _ => 0, // b (+0), loop/branch delimiters
        })
        .sum()
}

impl Streams {
    fn new() -> Self {
        Streams {
            of: BTreeMap::new(),
            order: Vec::new(),
            stream: BTreeMap::new(),
            frames: Vec::new(),
            poisoned: false,
        }
    }
    /// Append an event segment to `o` — into the innermost open branch arm when
    /// one exists (buffered until the region's flush), else onto the stream.
    fn append_seg(&mut self, o: ValueId, seg: &str) {
        if let Some(fr) = self.frames.last_mut() {
            if !fr.then_ev.contains_key(&o) && !fr.else_ev.contains_key(&o) {
                fr.order.push(o);
            }
            let map = if fr.in_else { &mut fr.else_ev } else { &mut fr.then_ev };
            map.entry(o).or_default().push_str(seg);
            return;
        }
        if !self.stream.contains_key(&o) {
            self.stream.insert(o, String::new());
            self.order.push(o);
        }
        self.stream.get_mut(&o).expect("o was just inserted above if it was not already present").push_str(seg);
    }
    /// Record a +1/−1/+0 event (`'i'`/`'d'`/`'b'`…) on object `o`.
    fn event(&mut self, o: ValueId, c: char) {
        let mut buf = [0u8; 4];
        self.append_seg(o, c.encode_utf8(&mut buf));
    }
    /// Open an `IfThen` region: subsequent events buffer into its then arm.
    fn open_branch(&mut self) {
        self.frames.push(BranchFrame::default());
    }
    /// `Else` marker: subsequent events buffer into the else arm.
    fn else_branch(&mut self) {
        if let Some(fr) = self.frames.last_mut() {
            fr.in_else = true;
        }
    }
    /// Close the innermost region (`EndIf`): per object, flush FLAT when both
    /// arms self-balance (net 0 — byte-identical to the ungrouped emission),
    /// else grouped `{then|else}` (the proven CBranch agreement rule). An arm
    /// that itself contains a region delimiter (a nested grouped branch or a
    /// loop) cannot be represented in a FLAT v4 arm body — emit the always-
    /// rejecting poison `{i|}` instead (conservative: never a silent accept).
    fn flush_branch(&mut self) {
        let fr = match self.frames.pop() {
            Some(fr) => fr,
            None => return, // EndIf without IfThen — malformed MIR, nothing buffered
        };
        for o in fr.order {
            let t = fr.then_ev.get(&o).cloned().unwrap_or_default();
            let e = fr.else_ev.get(&o).cloned().unwrap_or_default();
            let seg = if seg_net(&t) == 0 && seg_net(&e) == 0 {
                format!("{t}{e}")
            } else if t.contains(['(', ')', '{', '}', '[', ']'])
                || e.contains(['(', ')', '{', '}', '[', ']'])
            {
                self.poisoned = true;
                "{i|}".to_string()
            } else {
                format!("{{{t}|{e}}}")
            };
            self.append_seg(o, &seg);
        }
    }
    /// The current rc balance of `o`'s line (i/a = +1, d/m = −1), INCLUDING the
    /// events buffered in open branch arms — used to decide whether a
    /// branch-merge val still HOLDS its reference (an un-consumed arm value
    /// flowing through `EndIf {{ val }}` is a real move the stream must see).
    fn balance(&self, o: ValueId) -> i64 {
        let mut b = self.stream.get(&o).map(|line| seg_net(line)).unwrap_or(0);
        for fr in &self.frames {
            if let Some(t) = fr.then_ev.get(&o) {
                b += seg_net(t);
            }
            if let Some(e) = fr.else_ev.get(&o) {
                b += seg_net(e);
            }
        }
        b
    }
    fn object_of(&self, handle: ValueId) -> ValueId {
        // Well-formed MIR always has the handle mapped; fall back to identity so a
        // malformed input yields an unbalanced (rejected) certificate rather than
        // a panic.
        self.of.get(&handle).copied().unwrap_or(handle)
    }
}

/// Pre-scan for HEAP loop-carried SLOTS (option C). A `SetLocal { local, src }`
/// inside a `LoopStart`…`LoopEnd` region, whose `src` is a heap object (an
/// `Alloc`/heap-call-result allocated in the loop body — the `acc + [x]` feeder),
/// makes `local` a loop-carried accumulator slot: across iterations the slot drops
/// its old object and acquires `src` as the new one. The certificate folds the
/// slot's per-iteration drop-old + acquire-new into ONE stream wrapped in loop
/// delimiters `(`…`)`, so it reads `i(id)m` (acquire once; loop body acquire-new +
/// drop-old = rc-preserving; move out the final) — accepted by the proven
/// `check_cert_lc`. Returns `feeder -> slot` (route the feeder's `i` to the slot
/// stream) and the set of slot locals (open/close `(`/`)` around the loop body).
fn loop_carried_slots(
    func: &MirFunction,
) -> (BTreeMap<ValueId, ValueId>, BTreeSet<ValueId>, BTreeSet<ValueId>) {
    // Sequential-phase split (codopsy8 complexity sweep): phase 1 computes the heap-object
    // set (its own internal if/else/endif branch-stack state-threading, UNCHANGED — only the
    // phase BOUNDARY is named, the risky stack algorithm inside is untouched); phase 2 reads
    // that finished set (read-only) to find the loop-carried feeder slots. Pure text-move, no
    // logic change.
    let heap_objs = loop_carried_slots_heap_objs(func);
    loop_carried_slots_feeder_slots(func, &heap_objs)
}

/// Extracted from `loop_carried_slots` (codopsy8 complexity sweep, phase 1 of 2): heap
/// object dsts — Alloc/ListLit, calls with a heap result, Dup (always a heap handle), and a
/// branch-MERGE dst whose arm value is heap. Verbatim (the `if_stack` branch-stack algorithm
/// is UNCHANGED — this only names the phase boundary, not the algorithm itself).
fn loop_carried_slots_heap_objs(func: &MirFunction) -> BTreeSet<ValueId> {
    let mut heap_objs: BTreeSet<ValueId> = BTreeSet::new();
    for p in &func.params {
        if p.repr.is_heap() {
            heap_objs.insert(p.value);
        }
    }
    // Open-branch stack for the merge-dst scan below: (IfThen dst, then-arm val was heap).
    let mut if_stack: Vec<(Option<ValueId>, bool)> = Vec::new();
    for op in &func.ops {
        heap_objs_step(op, &mut heap_objs, &mut if_stack);
    }
    heap_objs
}

/// One op of the heap-object scan (the `if_stack` branch-stack algorithm is
/// UNCHANGED — this names the per-op step, not the algorithm).
fn heap_objs_step(
    op: &Op,
    heap_objs: &mut BTreeSet<ValueId>,
    if_stack: &mut Vec<(Option<ValueId>, bool)>,
) {
    // A call with a heap result introduces a heap object, like Alloc.
    if let Some(d) = heap_call_dst(op) {
        heap_objs.insert(d);
        return;
    }
    match op {
        // ListLit joins Alloc as an alloc-class introducer (rung 4/5: scalar
        // list AND record literals) — without it a record reassign's SetLocal
        // feeder goes unrecognized and the slot reads flat `idd` + `i` (the
        // exact false double-free/leak the kernel checker rejected when the
        // records slab first landed).
        Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => {
            heap_objs.insert(*dst);
        }
        // A Dup is ALWAYS a heap handle (an alias acquire on an existing heap
        // object — scalars are never Dup'd): the SWAP-CARRY rebind (`cur =
        // merged` lowered as `Dup tmp = merged; Drop cur; SetLocal cur = tmp`
        // since the whole-var alias-edge elision) feeds its slot through the
        // Dup's dst. Without this the slot goes unrecognized and the in-loop
        // drop-old + scope-end drop read flat (`idd`) — the loop_buffer_churn
        // false double-free the Trust Spine gate caught. NO src gate: the
        // C-132 write-back rebind (`t = __mp_buf` in a loop) Dups a BORROWED
        // tuple-slot LoadHandle — not itself in `heap_objs` — and the src-gated
        // form left that slot unrecognized (`idm` + a flat `a`, both rejected).
        Op::Dup { dst, .. } => {
            heap_objs.insert(*dst);
        }
        // A branch-MERGE dst whose arm value is heap (`acc = if c then acc + [x]
        // else acc` — the arm's Else/EndIf val moves the arm's heap object into
        // the merge) IS a heap object: the following `SetLocal { local, src: dst }`
        // is the loop-carried rebind, and the slot goes unrecognized without this
        // (the accumulator then reads flat `iamdm` — a false imbalance the kernel
        // checker rejects while the strict render runs correctly). The stack pairs
        // each EndIf with its IfThen so nesting resolves inner-first; `Else { val }`
        // carries the then-arm value, `EndIf { val }` the else-arm value.
        Op::IfThen { dst, .. } => {
            if_stack.push((*dst, false));
        }
        Op::Else { val } => {
            if let (Some(frame), Some(v)) = (if_stack.last_mut(), val) {
                if heap_objs.contains(v) {
                    frame.1 = true;
                }
            }
        }
        Op::EndIf { val } => endif_merge_heap(*val, heap_objs, if_stack),
        _ => {}
    }
}

/// Close one branch frame: the merge dst is a heap object iff either arm's
/// val was (then recorded at `Else`, else read here).
fn endif_merge_heap(
    val: Option<ValueId>,
    heap_objs: &mut BTreeSet<ValueId>,
    if_stack: &mut Vec<(Option<ValueId>, bool)>,
) {
    if let Some((dst, then_heap)) = if_stack.pop() {
        let heap = then_heap || val.map_or(false, |v| heap_objs.contains(&v));
        if heap {
            if let Some(d) = dst {
                heap_objs.insert(d);
            }
        }
    }
}

/// Extracted from `loop_carried_slots` (codopsy8 complexity sweep, phase 2 of 2): the
/// feeder-to-slot map, keyed off the (already-finished, read-only) `heap_objs` set from
/// phase 1. STRAIGHT-LINE (non-loop) heap slots: a `SetLocal { local, src }` with a heap
/// `src` OUTSIDE any loop region (the unrolled identity-else shadow-rebind
/// append-accumulator — porta serialize_opts). Each such reassign is folded into its OWN
/// `(id)` CLoop body (`(` at the feeder's `i`, `)` at the SetLocal), so a body with k
/// reassigns reads `i(id)…(id)m` — the SAME rc-preserving unit the loop slot proves,
/// accepted by check_cert_lc. A SCALAR `src` (a loop counter `i+1`) is not a heap_obj, so it
/// is never a slot here (no spurious fold). Verbatim.
fn loop_carried_slots_feeder_slots(
    func: &MirFunction,
    heap_objs: &BTreeSet<ValueId>,
) -> (BTreeMap<ValueId, ValueId>, BTreeSet<ValueId>, BTreeSet<ValueId>) {
    let mut feeder_to_slot: BTreeMap<ValueId, ValueId> = BTreeMap::new();
    let mut slots: BTreeSet<ValueId> = BTreeSet::new();
    let mut line_slots: BTreeSet<ValueId> = BTreeSet::new();
    let mut depth: u32 = 0;
    for op in &func.ops {
        match op {
            Op::LoopStart => depth += 1,
            Op::LoopEnd => depth = depth.saturating_sub(1),
            Op::SetLocal { local, src } if heap_objs.contains(src) => {
                feeder_to_slot.insert(*src, *local);
                if depth > 0 {
                    slots.insert(*local);
                } else {
                    line_slots.insert(*local);
                }
            }
            _ => {}
        }
    }
    (feeder_to_slot, slots, line_slots)
}

/// Emit the per-object ownership certificate (format v2) for a function. Heap
/// loop-carried accumulator slots are folded into a single `i(id)m` stream with
/// loop delimiters (option C); everything else is the flat per-object format.
/// The mutable emission state of [`ownership_certificate`] — one step per op
/// (#781: the cog-123 loop body became [`CertScan::step`]).
struct CertScan {
    depth: u32,
    s: Streams,
    released_merge_dsts: std::collections::HashSet<crate::ValueId>,
    consumed_values: std::collections::HashSet<crate::ValueId>,
    feeder_to_slot: BTreeMap<ValueId, ValueId>,
    slots: BTreeSet<ValueId>,
    line_slots: BTreeSet<ValueId>,
}

impl CertScan {
    /// One op's certificate emission. Verbatim text move of the emission loop
    /// body (locals renamed to fields); the op families are classified by
    /// [`drop_family_value`] / [`alloc_class_prim_dst`] / [`heap_call_dst`] and
    /// the loop-slot feeder routing is [`Self::feed_or_own`].
    fn step(&mut self, op: &Op) {
        // Plain release (−1). A `DropListStr`/`DropListValue` is the SAME single `d` on the LIST
        // object — its elements were already accounted as `m` (consumed) when stored into it, so
        // the recursive runtime free (per-String, or per-Value via `$__drop_value`) adds no extra
        // cert event.
        if let Some(v) = drop_family_value(op) {
            let o = self.s.object_of(v);
            self.s.event(o, 'd');
            return;
        }
        // An allocator-class prim result is a +1, like `Alloc` — a fresh owned value on its
        // OWN stream (`i`), balanced by the caller's scope-end drop or a heap-return move-out.
        if let Some(d) = alloc_class_prim_dst(op) {
            self.s.of.insert(d, d);
            self.s.event(d, 'i');
            return;
        }
        // A call that returns a FRESH OWNED heap value (the callee allocated
        // it and moved it out to us — the return-mode signature read at the
        // call site, callee not opened) is a +1, like Alloc. A `CallIndirect`
        // (a closure invocation) returning heap is the SAME: a closure moves its
        // result out, so a heap-returning closure call (`let o = f(x)` where
        // `f: (Int) -> Option[Int]`) owns a fresh value, dropped at scope end —
        // the foundation for `list.filter_map` / `flat_map`. A non-capturing
        // lifted lambda materializes its result (`Some(x)` allocs), and a closure
        // param points to one — so the result is always owned, never borrowed.
        //
        // A heap loop-carried FEEDER (`new = acc + [x]`): its `i` belongs to
        // the SLOT stream (the slot absorbs `new` via the following SetLocal),
        // folded inside the loop delimiters → `i(id)m`. Otherwise it is a
        // fresh owned object with its own stream (`i`).
        if let Some(d) = heap_call_dst(op) {
            self.feed_or_own(d, 'i');
            return;
        }
        match op {
            // A rung-4 scalar-list LITERAL is alloc-class — the IDENTICAL `i` (and
            // loop-slot feeder routing) the `Alloc{DynList}` it replaced emitted.
            // The element load/store ops are ownership-NEUTRAL (a borrowed handle
            // read/write), so they need no event arm — the catch-all below skips them.
            Op::Alloc { dst, .. } | Op::ListLit { dst, .. } => self.feed_or_own(*dst, 'i'),
            Op::Dup { dst, src } => self.dup_step(*dst, *src),
            // MOVE-OUT (−1): the reference is transferred out (into a container /
            // a consuming callee). `m` distinguishes move from a plain drop.
            Op::Consume { v } => {
                let o = self.s.object_of(*v);
                self.s.event(o, 'm');
            }
            Op::SetLocal { local, .. } => self.line_slot_close(*local),
            // Open the branch region (format v4, brick 5a): arm events buffer per
            // arm so the flush can group non-self-balancing arms as `{then|else}`.
            // The released merge dst's `i` (the arm's moved-in reference, +1) is a
            // PRE-REGION event — the merge object is acquired at the merge point,
            // outside either arm — so it is emitted before the region opens.
            //
            // A merge dst that FEEDS a heap slot (`acc = if c then acc + [x]
            // else acc`): the arms move their value into the merge (+1
            // received), and the following SetLocal absorbs it into the
            // slot — route the merge's `i` into the SLOT stream exactly as
            // the Alloc/heap-call feeders route theirs, so the per-iteration
            // body folds rc-preserving (`i(iamd)m`) instead of the flat
            // `iamdm` false imbalance the kernel checker rejects.
            Op::IfThen { dst, .. } => self.open_merge(*dst),
            Op::Else { val } => self.close_arm(val, false),
            Op::EndIf { val } => self.close_arm(val, true),
            // A LIVE USE — a read-only borrow or an in-place unique use (`xs[i] = v`
            // via MakeUnique) — on an object whose stream HOLDS ownership (it has a
            // +1 event) is witnessed as `b` (+0, liveness-guarded, brick 5b): a use
            // after the last release makes the proven checker FAULT — owned-object
            // use-after-free is now witnessable, not invisible. An object with no
            // +1 on its stream (a borrowed param used directly) stays event-free:
            // its liveness is the CALLER's obligation, discharged by the call-mode
            // agreement (CallModes.v), not by this stream's count.
            Op::Borrow { v } | Op::MakeUnique { v } => self.live_use_step(*v),
            // Loop delimiters for a heap loop-carried slot: open `(` on each slot
            // stream when entering a top-level loop, close `)` on leaving — so the
            // slot's per-iteration acquire-new + drop-old reads `(id)`, certifying a
            // rc-preserving body (option C, proved in check_line_unroll_sound).
            Op::LoopStart => self.enter_loop(),
            Op::LoopEnd => self.exit_loop(),
            // No refcount change: Const/Pure/IntBinOp/scalar SetLocal, and a call
            // with a void/scalar result (its heap-handle args are borrowed).
            _ => self.rc_prim_step(op),
        }
    }

    /// VALUE-RC (柱C extension) — MIRROR verify_ownership's carrier model so the cert and the
    /// executable verifier AGREE on the prim.handle-fed rc case. prim.handle(v) registers the
    /// handle as a CARRIER of v's object (no event); rc_inc/rc_dec on a carrier emit `a`/`d`
    /// (the proven checker, already rc-aware, verifies the balance). A load64-fed handle has no
    /// `of` entry → no event, exactly as before (the differential-test floor). Any other op
    /// is refcount-neutral (the former catch-all).
    fn rc_prim_step(&mut self, op: &Op) {
        match op {
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } => {
                if let Some(&o) = args.first().and_then(|a| self.s.of.get(a)) {
                    self.s.of.insert(*d, o);
                }
            }
            Op::Prim { kind: PrimKind::RcInc, args, .. } => {
                if let Some(&o) = args.first().and_then(|a| self.s.of.get(a)) {
                    self.s.event(o, 'a');
                }
            }
            Op::Prim { kind: PrimKind::RcDec, args, .. } => {
                if let Some(&o) = args.first().and_then(|a| self.s.of.get(a)) {
                    self.s.event(o, 'd');
                }
            }
            _ => {}
        }
    }

    /// An arm value that still HOLDS its reference when it flows into the merge
    /// (`Else/EndIf { val }` with no prior `Consume` — the declared-Result tail-if
    /// style, effect_tco::checked) MOVES it there: emit the `m` the explicit-Consume
    /// style already has. A val already consumed (balance 0) or never tracked
    /// (a scalar) is untouched. The `m` lands in the CLOSING arm's buffer (then
    /// at `Else`, else at `EndIf`); then the region switches arm / flushes.
    fn close_arm(&mut self, val: &Option<ValueId>, is_end: bool) {
        if let Some(v) = val {
            if self.arm_val_moves(*v) {
                let o = self.s.object_of(*v);
                self.s.event(o, 'm');
            }
        }
        if is_end {
            self.s.flush_branch();
        } else {
            self.s.else_branch();
        }
    }

    /// Route `dst`'s event into its loop-carried SLOT stream when it FEEDS one.
    /// Resolve the slot through `of`: a Dup-INITIALIZED slot (`var iv =
    /// state.iv`) aliases the Dup's source object — its 'a'/'d'/'m' land
    /// there, so the loop `(i…)`/feeder events must land on the SAME
    /// stream (they split across two unbalanced lines otherwise — the
    /// bytes_set_value_semantics::rotate REJECT, F8 residue). A STRAIGHT-LINE
    /// slot opens its `(id)` CLoop body before the feeder's event.
    /// `false` = not a feeder (nothing emitted).
    fn try_feed(&mut self, dst: ValueId, ev: char) -> bool {
        let Some(&slot) = self.feeder_to_slot.get(&dst) else { return false };
        let so = self.s.object_of(slot);
        self.s.of.insert(dst, so);
        if self.line_slots.contains(&slot) && self.s.frames.is_empty() {
            self.s.event(so, '(');
        }
        self.s.event(so, ev);
        true
    }

    /// A feeder's event routes to its slot stream; anything else opens its OWN
    /// stream with the same event.
    fn feed_or_own(&mut self, dst: ValueId, ev: char) {
        if !self.try_feed(dst, ev) {
            self.s.of.insert(dst, dst);
            self.s.event(dst, ev);
        }
    }

    /// Does this arm value MOVE into the merge? An EXPLICITLY-Consumed arm value
    /// already emitted its move `m` — the val-move would double-count it (the
    /// `else base` Var-arm `iammd` REJECT: the Dup'd value's Consume + this rule
    /// both fired on the shared base object); only the never-Consumed style
    /// (effect-TCO tail-if) qualifies. Loop-carried machinery keeps its own
    /// `(id)` accounting — a slot or feeder flowing through a branch inside the
    /// loop is NOT a move-out (heap_result_if_append's accumulator would
    /// double-`m`).
    fn arm_val_moves(&self, v: crate::ValueId) -> bool {
        self.s.of.contains_key(&v)
            && self.s.balance(self.s.object_of(v)) > 0
            && !self.consumed_values.contains(&v)
            && !self.slots.contains(&self.s.object_of(v))
            && !self.feeder_to_slot.contains_key(&v)
            && !self.line_slots.contains(&self.s.object_of(v))
    }

    /// Does object `o`'s stream (or an open arm buffer) hold a +1 event?
    fn stream_owns(&self, o: ValueId) -> bool {
        self.s.stream.get(&o).map_or(false, |l| l.contains(['i', 'a']))
            || self.s.frames.iter().any(|fr| {
                fr.then_ev.get(&o).map_or(false, |l| l.contains(['i', 'a']))
                    || fr.else_ev.get(&o).map_or(false, |l| l.contains(['i', 'a']))
            })
    }

    /// Close a STRAIGHT-LINE slot's `(id)` CLoop body: the feeder's `i` + the drop-old's `d`
    /// were already emitted; `)` here makes the per-reassign stream read `(id)` (rc-preserving).
    /// A loop slot's SetLocal carries no cert event (its parens are the LoopStart/LoopEnd
    /// delimiters); a scalar SetLocal is cert-neutral. So this fires ONLY for a line slot.
    /// (Inside a BRANCH frame the line-slot rebind emits NO parens — the arm's
    /// flat `i…d` nets 0 and a delimiter in an arm buffer poisons the flush
    /// (`{i|}`); the fold is only needed for straight-line REPEATED reassigns.)
    fn line_slot_close(&mut self, local: ValueId) {
        if self.line_slots.contains(&local) && self.s.frames.is_empty() {
            let so = self.s.object_of(local);
            self.s.event(so, ')');
        }
    }

    /// Emit one loop delimiter on every slot stream.
    fn slot_delimiters(&mut self, ev: char) {
        for slot in &self.slots {
            let so = self.s.object_of(*slot);
            self.s.event(so, ev);
        }
    }

    /// ALIAS acquire (+1): a new handle on an existing shared object.
    /// `a` (not `i`) records the share-vs-move ground fact (format v1).
    /// A Dup that FEEDS a loop-carried slot (`cur = merged` swap-carry:
    /// `Dup tmp = merged; Drop cur; SetLocal cur = tmp`) routes its `a`
    /// into the SLOT stream, exactly as the Alloc/heap-call feeders route
    /// their `i`: the slot's per-iteration acquire-new + drop-old then
    /// reads `(ad)` (rc-preserving), instead of the drop-old landing flat
    /// next to the scope-end drop (`idd` — a false double-free).
    fn dup_step(&mut self, dst: ValueId, src: ValueId) {
        if !self.try_feed(dst, 'a') {
            let o = self.s.object_of(src);
            self.s.of.insert(dst, o);
            self.s.event(o, 'a');
        }
    }

    /// Open the branch region (format v4, brick 5a) after crediting the merge
    /// dst: a slot FEEDER routes its `i` into the slot stream; a RELEASED merge
    /// opens its own stream. Both are PRE-REGION events — the merge object is
    /// acquired at the merge point, outside either arm.
    fn open_merge(&mut self, dst: Option<ValueId>) {
        if let Some(d) = dst {
            if !self.try_feed(d, 'i') && self.released_merge_dsts.contains(&d) {
                self.s.of.insert(d, d);
                self.s.event(d, 'i');
            }
        }
        self.s.open_branch();
    }

    /// A LIVE USE (Borrow / MakeUnique) on an object whose stream HOLDS
    /// ownership is witnessed as `b` (+0, liveness-guarded, brick 5b); an
    /// object with no +1 on its stream (a borrowed param used directly) stays
    /// event-free — its liveness is the CALLER's obligation (CallModes.v).
    fn live_use_step(&mut self, v: ValueId) {
        if self.s.of.contains_key(&v) {
            let o = self.s.object_of(v);
            if self.stream_owns(o) {
                self.s.event(o, 'b');
            }
        }
    }

    /// Top-level loop entry: open `(` on every slot stream (option C).
    fn enter_loop(&mut self) {
        if self.depth == 0 {
            self.slot_delimiters('(');
        }
        self.depth += 1;
    }

    /// Top-level loop exit: close `)` on every slot stream.
    fn exit_loop(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 {
            self.slot_delimiters(')');
        }
    }
}

/// The value a DROP-family op releases (every recursive drop variant is the
/// same single `d` on its object), or `None` for any other op.
fn drop_family_value(op: &Op) -> Option<ValueId> {
    match op {
        Op::Drop { v }
        | Op::DropListStr { v }
        | Op::DropValue { v }
        | Op::DropListValue { v }
        | Op::DropListStrValue { v }
        | Op::DropListStrStr { v }
        | Op::DropListIntStr { v }
        | Op::DropListStrInt { v }
        | Op::DropResultListValue { v }
        | Op::DropResultValue { v }
        | Op::DropResultStrInt { v }
        | Op::DropResultValueInt { v }
        | Op::DropResultListValueInt { v }
        | Op::DropResultListStrInt { v }
        | Op::DropResultListStr { v }
        | Op::DropListListStr { v }
        | Op::DropVariant { v, .. }
        | Op::DropWrapperRec { v, .. } => Some(*v),
        _ => None,
    }
}

/// The dst of an ALLOCATOR-CLASS prim — one that returns a fresh OWNED heap
/// value the caller must balance, exactly like `Alloc`. Without the `i` these
/// emit, the heap result would be an unbacked object the cert never opens and
/// the verify_ownership/cert agreement breaks for the owning stdlib body.
/// Every arg of every prim here is BORROWED (no cert event). The roster and
/// what each allocates:
/// `args_get_list` → `List[String]` (argv[1..]); `env_get` → `Option[String]`
/// (a 0/1-slot block owning the value String when some); `read_text_file` →
/// `Result[String, String]` (cap-as-tag, owns one payload String); `read_dir`
/// → `Result[List[String], String]` (balanced by the recursive
/// `DropResultListStr`); `write_text_file` / `make_dir` / `remove_all` →
/// `Result[Unit, String]` (Ok carries NO payload, Err owns one message
/// String); `read_line` / `read_n_bytes` → a fresh canonical `String` (flat
/// `Drop` — a String owns no nested handles).
fn alloc_class_prim_dst(op: &Op) -> Option<ValueId> {
    match op {
        Op::Prim {
            kind:
                PrimKind::ArgsGetList
                | PrimKind::EnvGet
                | PrimKind::ReadTextFile
                | PrimKind::ReadDir
                | PrimKind::WriteTextFile
                | PrimKind::MakeDir
                | PrimKind::RemoveAll
                | PrimKind::ReadLine
                | PrimKind::ReadNBytes,
            dst: Some(d),
            ..
        } => Some(*d),
        _ => None,
    }
}

/// The dst of a call op returning a HEAP result (any call form), or `None`.
fn heap_call_dst(op: &Op) -> Option<ValueId> {
    match op {
        Op::Call { dst: Some(d), result: Some(r), .. }
        | Op::CallFn { dst: Some(d), result: Some(r), .. }
        | Op::CallImport { dst: Some(d), result: Some(r), .. }
        | Op::CallIndirect { dst: Some(d), result: Some(r), .. }
            if r.is_heap() =>
        {
            Some(*d)
        }
        _ => None,
    }
}

/// The number of `i` events [`ownership_certificate`] credits to branch-MERGE
/// dsts: the RELEASED merges (the arm's moved-in reference, later Consumed/
/// Dropped/val-flowed/returned) plus the slot-FEEDER merges (`acc = if c then
/// acc + [x] else acc`, whose `i` routes into the loop-carried slot stream).
/// Both are backed by the arm value's real producer — the merge is a reference
/// changing hands (the wasm merge local.set), not a synthetic `+1`. classify's
/// borrow-by-default backing gate uses THIS count so the gate and the emission
/// stay in lockstep by construction (one credit per IfThen op occurrence,
/// mirroring `CertScan::step`'s pre-region emission exactly).
pub fn merge_dst_i_credits(func: &MirFunction) -> usize {
    let (feeder_to_slot, _, _) = loop_carried_slots(func);
    let mut merge_dsts: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    let mut released: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    for op in &func.ops {
        match op {
            Op::IfThen { dst: Some(d), .. } => {
                merge_dsts.insert(*d);
            }
            Op::Consume { v } | Op::Drop { v } | Op::DropListStr { v } => {
                if merge_dsts.contains(v) {
                    released.insert(*v);
                }
            }
            Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) } => {
                if merge_dsts.contains(v) {
                    released.insert(*v);
                }
            }
            _ => {}
        }
    }
    if let Some(r) = func.ret {
        if merge_dsts.contains(&r) {
            released.insert(r);
        }
    }
    // HEAP merges only — the exact filter `ownership_certificate_released_merge_dsts`
    // applies (a scalar merge carries no reference; crediting it would desync
    // this count from the certificate's `i` events).
    let heap_objs = loop_carried_slots_heap_objs(func);
    released.retain(|d| heap_objs.contains(d));
    func.ops
        .iter()
        .filter(|op| match op {
            Op::IfThen { dst: Some(d), .. } => {
                feeder_to_slot.contains_key(d) || released.contains(d)
            }
            _ => false,
        })
        .count()
}

/// Extracted from `ownership_certificate` (codopsy8 complexity sweep, pre-scan phase 1 of
/// 2, verbatim — the original code already scoped this as its own `{ .. }` block): a
/// branch-MERGE dst (`Op::IfThen { dst }`) that is later RELEASED — consumed by an OUTER
/// frame (the nested monadic-`!` chain: the inner match's merged Result moves into the
/// outer merge) or returned — RECEIVES the arm value each arm moved in (the arm's `m`).
/// Record that move-in as the merge object's `i` so its later `m`/`d` balances ("im", the
/// physical rc: the arm's −1 and the merge's +1 are the same reference changing hands). An
/// UNUSED merge dst stays event-free exactly as before. Without this the chained-`!`
/// witness read as a bare `m` and the proven checker REJECTED it (flight-evidence-gaps F8).
fn ownership_certificate_released_merge_dsts(
    func: &MirFunction,
) -> std::collections::HashSet<crate::ValueId> {
    let mut released_merge_dsts: std::collections::HashSet<crate::ValueId> =
        std::collections::HashSet::new();
    let mut merge_dsts: std::collections::HashSet<crate::ValueId> = std::collections::HashSet::new();
    for op in &func.ops {
        match op {
            Op::IfThen { dst: Some(d), .. } => {
                merge_dsts.insert(*d);
            }
            Op::Consume { v } | Op::Drop { v } | Op::DropListStr { v } => {
                if merge_dsts.contains(v) {
                    released_merge_dsts.insert(*v);
                }
            }
            // An INNER merge flowing out as an OUTER arm value (`Else/EndIf { val }`
            // — the effect-TCO nested-if chain) is released the same way: the val-move
            // rule below emits its `m`.
            Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) } => {
                if merge_dsts.contains(v) {
                    released_merge_dsts.insert(*v);
                }
            }
            _ => {}
        }
    }
    if let Some(r) = func.ret {
        if merge_dsts.contains(&r) {
            released_merge_dsts.insert(r);
        }
    }
    // Ownership is a HEAP property: only a merge whose arm value is heap
    // carries a reference into the dst. A SCALAR merge dst flowing out as an
    // outer arm value (the lex-min fold's flag selects) must NOT become an
    // object — its synthetic `i` + one-sided arm `m` certs as `i{m|}` /
    // `i{|m}`, which the kernel-proven checker rejects while the executable
    // verifier (scalar-blind by design) accepts — the PCC corpus-wall
    // divergence, 2026-08-03. Same filter mirrored in `merge_dst_i_credits`.
    let heap_objs = loop_carried_slots_heap_objs(func);
    released_merge_dsts.retain(|d| heap_objs.contains(d));
    released_merge_dsts
}

/// Extracted from `ownership_certificate` (codopsy8 complexity sweep, pre-scan phase 2 of
/// 2, verbatim): the set of values EXPLICITLY moved out by an `Op::Consume` — the arm-value
/// move for the LitStr/Var/concat arms (`lower_heap_result_arm`). Such a value's `m` is
/// ALREADY on its object's stream, so the `Else/EndIf {val}` val-move rule in `CertScan::step`
/// must NOT emit a SECOND `m` for it. The per-object `balance > 0` guard alone cannot catch
/// this when the value ALIASES a still-live scope local (`else base` — the Var arm Dups
/// base, so the shared object keeps balance 1 after the Consume, and the val-move
/// double-`m`'d it → the `iammd` REJECT). Only the val-move-ONLY style (the effect-TCO
/// declared-Result tail-if, whose arms never Consume) should reach the rule.
fn ownership_certificate_consumed_values(func: &MirFunction) -> std::collections::HashSet<crate::ValueId> {
    func.ops
        .iter()
        .filter_map(|op| match op {
            Op::Consume { v } => Some(*v),
            _ => None,
        })
        .collect()
}

pub fn ownership_certificate(func: &MirFunction) -> String {
    ownership_certificate_with_poison(func).0
}

/// [`ownership_certificate`] plus whether ANY region flush emitted the
/// always-rejecting POISON `{i|}` (a nested-region arm that cannot be
/// represented flat). The poison REPLACES real arm events, so event COUNTS
/// read off a poisoned certificate are meaningless — the backing gate skips
/// them (#1146); the kernel-proven checker still rejects the poisoned cert,
/// which is the poison's whole job.
pub fn ownership_certificate_with_poison(func: &MirFunction) -> (String, bool) {
    // Sequential-phase split (codopsy8 complexity sweep): the two pre-scan sets below are
    // each an independent, self-contained computation over `func.ops` (the original code
    // already delineated the first as its own `{ .. }` scope) — extracted verbatim as their
    // own named functions. `CertScan::step` (protected, unchanged) and the rest of the
    // emission pipeline below are untouched.
    let (feeder_to_slot, slots, line_slots) = loop_carried_slots(func);
    let mut depth: u32 = 0;
    let mut s = Streams::new();

    let released_merge_dsts = ownership_certificate_released_merge_dsts(func);
    let consumed_values = ownership_certificate_consumed_values(func);

    // Heap params are BORROWED (the v1 calling convention): the CALLER owns the
    // reference and releases it, so a param contributes NO `i` event — that `+1`
    // would be SYNTHETIC, unbacked by any runtime `Alloc`/`rc_inc` (the gate-blind
    // use-after-free class). We still register the object identity (`of`) so that
    // a body which releases (`Drop`/`Consume`) or returns a borrowed param WITHOUT
    // first acquiring its own reference (a `Dup`) emits a `d`/`m` at rc 0 — which
    // the proven checker FAULTS (REJECT), exactly the double-free that owning the
    // caller's reference would cause. A `Dup` of the param emits the real `a`.
    for p in &func.params {
        if p.repr.is_heap() {
            s.of.insert(p.value, p.value);
        }
    }

    // Decomposed (#781, cog 123): the per-op emission lives in `CertScan::step`;
    // the pre-scan state moved into the scan struct verbatim.
    let mut scan = CertScan {
        depth,
        s,
        released_merge_dsts,
        consumed_values,
        feeder_to_slot,
        slots,
        line_slots,
    };
    for op in &func.ops {
        scan.step(op);
    }

    // Defensive: a dangling IfThen (no EndIf — malformed MIR) still flushes, so
    // its buffered arm events land on the stream (and unbalance ⟹ reject) rather
    // than vanish.
    while !scan.s.frames.is_empty() {
        scan.s.flush_branch();
    }

    // A heap return is MOVED OUT to the caller (a −1) — a move, hence `m`.
    if let Some(r) = func.ret {
        if scan.s.of.contains_key(&r) {
            let o = scan.s.object_of(r);
            scan.s.event(o, 'm');
        }
    }

    let mut out = String::new();
    for o in &scan.s.order {
        out.push_str(&scan.s.stream[o]);
        out.push('\n');
    }
    (out, scan.s.poisoned)
}

/// The NON-RECURRING soundness gate for the borrow-by-default calling
/// convention, shared by the corpus classifier AND the lowering exit (#1146):
/// EVERY `+1` event in the ownership certificate must be BACKED by a real
/// runtime op — an `i` by an `Alloc`/`ListLit`, a heap-result call, or a
/// credited branch merge; an `a` by a `Dup` — and every such op must have its
/// cert line. A strict EQUALITY, so an unbacked synthetic `+1` (the
/// gate-blind use-after-free class) AND a backed-but-uncertified op (the
/// fs.fold_lines_chunked loop shape: one more real op than cert lines) both
/// refuse.
pub fn plus_one_events_backed(func: &MirFunction) -> bool {
    let (cert, poisoned) = ownership_certificate_with_poison(func);
    // A POISONED certificate deliberately replaced a nested-region arm's real
    // events with the always-rejecting `{i|}` — its counts cannot be compared
    // against the op list (the fs.fold_lines_chunked class, #1146). The
    // poison's soundness story is the kernel checker's REJECTION of the cert
    // itself; this equality only claims the flat-representable population.
    if poisoned {
        return true;
    }
    let i = cert.chars().filter(|c| *c == 'i').count();
    let a = cert.chars().filter(|c| *c == 'a').count();
    let allocs = func
        .ops
        .iter()
        .filter(|o| matches!(o, crate::Op::Alloc { .. } | crate::Op::ListLit { .. }))
        .count();
    let heap_results = func
        .ops
        .iter()
        .filter(|o| match o {
            crate::Op::Call { dst: Some(_), result: Some(r), .. }
            | crate::Op::CallFn { dst: Some(_), result: Some(r), .. }
            | crate::Op::CallIndirect { dst: Some(_), result: Some(r), .. } => r.is_heap(),
            _ => false,
        })
        .count();
    let dups = func.ops.iter().filter(|o| matches!(o, crate::Op::Dup { .. })).count();
    let merge_credits = merge_dst_i_credits(func);
    i == allocs + heap_results + merge_credits && a == dups
}
