//! Ownership certificate emission — the seam between the untrusted compiler and
//! the KERNEL-PROVEN checker (proofs/, the v1 flight-grade spine).
//!
//! `ownership_certificate` projects a function's MIR ownership ops to the
//! per-object refcount-event stream (certificate format v0): one line per
//! reference-counted OBJECT, `i` = an ownership +1 (Alloc/Dup), `d` = a −1
//! (Drop/Consume, and the move-out of a heap return). This is the SAME
//! per-object accounting [`crate::verify_ownership`] enforces — but emitted as a
//! portable certificate the proven Coq checker `check_all` re-verifies. So each
//! build's memory-safety is re-checkable by a proven artifact, not just by the
//! (untrusted) compiler's own pass.
//!
//! By construction the proven checker accepts `ownership_certificate(f)` iff
//! `verify_ownership(f)` accepts (same invariant); the unit tests pin that
//! correspondence, and `proofs/gate.sh` runs the actual proven binary on it.

use crate::{CallArg, Capability, MirFunction, Op, ValueId};
use std::collections::BTreeMap;

/// The name-totality witness (proofs/NameTotality.v, the 2nd flight-grade
/// property): the DEFINED value ids (params + op results) and the USED value ids
/// (operands/args). The kernel-proven `check_names` accepts iff `used ⊆ defined`
/// — i.e. no dangling MIR reference (a use of an undefined value = undefined
/// behavior). Emitted like the ownership certificate, for the proven checker.
pub struct NameWitness {
    pub defined: Vec<ValueId>,
    pub used: Vec<ValueId>,
}

/// Serialize the name-totality witness in the format `proofs/NameTotality.v`'s
/// `check_names_cert` parses: `<defined ids>|<used ids>` (space-separated nats).
/// The proven checker accepts iff `used ⊆ defined` (no dangling reference).
pub fn name_witness_string(func: &MirFunction) -> String {
    let w = name_witness(func);
    let ids = |v: &[ValueId]| v.iter().map(|x| x.0.to_string()).collect::<Vec<_>>().join(" ");
    format!("{}|{}", ids(&w.defined), ids(&w.used))
}

/// Collect the (defined, used) value ids of a function for name-totality.
/// Duplicates are harmless — the proven checker is set-membership.
pub fn name_witness(func: &MirFunction) -> NameWitness {
    let mut defined: Vec<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut used: Vec<ValueId> = Vec::new();
    let record_args = |args: &[CallArg], used: &mut Vec<ValueId>| {
        for a in args {
            if let CallArg::Handle(v) | CallArg::Scalar(v) = a {
                used.push(*v);
            }
        }
    };
    for op in &func.ops {
        match op {
            Op::Alloc { dst, .. } | Op::Const { dst } | Op::ConstInt { dst, .. } => {
                defined.push(*dst)
            }
            Op::Dup { dst, src } => {
                defined.push(*dst);
                used.push(*src);
            }
            Op::Drop { v }
            | Op::DropListStr { v }
            | Op::Consume { v }
            | Op::Borrow { v }
            | Op::MakeUnique { v } => {
                used.push(*v)
            }
            Op::Pure { dst, uses } => {
                defined.push(*dst);
                used.extend(uses.iter().copied());
            }
            Op::IntBinOp { dst, a, b, .. } => {
                defined.push(*dst);
                used.push(*a);
                used.push(*b);
            }
            Op::Prim { dst, args, .. } => {
                if let Some(d) = dst {
                    defined.push(*d);
                }
                used.extend(args.iter().copied());
            }
            Op::Call { dst, args, .. } | Op::CallFn { dst, args, .. } => {
                if let Some(d) = dst {
                    defined.push(*d);
                }
                record_args(args, &mut used);
            }
            // A closure call USES the table-index value (the closure) plus its args.
            Op::CallIndirect { dst, table_idx, args, .. } => {
                if let Some(d) = dst {
                    defined.push(*d);
                }
                used.push(*table_idx);
                record_args(args, &mut used);
            }
            // The if-condition is USED; the result `dst` is DEFINED; the arm values are
            // USED. (The arm OPS, flat between the markers, define/use as usual.)
            Op::IfThen { cond, dst } => {
                used.push(*cond);
                if let Some(d) = dst {
                    defined.push(*d);
                }
            }
            Op::Else { val } | Op::EndIf { val } => {
                used.extend(val.iter().copied());
            }
            // Loop markers: the break cond is USED. `LoopStart`/`LoopEnd` bind nothing.
            Op::LoopBreakUnless { cond } => used.push(*cond),
            Op::LoopStart | Op::LoopEnd => {}
            // A scalar reassignment USES the source value and the target local (already
            // defined by its `var` bind — re-written, not newly defined).
            Op::SetLocal { local, src } => {
                used.push(*local);
                used.push(*src);
            }
            // A function reference DEFINES its scalar slot value; it uses no MIR value
            // (the referenced function name is resolved structurally by the render).
            Op::FuncRef { dst, .. } => defined.push(*dst),
        }
    }
    if let Some(r) = func.ret {
        used.push(r);
    }
    NameWitness { defined, used }
}

/// The capability-bound witness (proofs/CapabilityBound.v, the 4th flight-grade
/// property): the DECLARED capability allowlist (the function's effect
/// signature) and the USED capabilities (those its body's runtime calls reach).
/// The kernel-proven `check_caps` accepts iff `used ⊆ allowed` — i.e. the
/// function reaches no host effect it did not declare (the sandbox promise).
pub struct CapWitness {
    pub allowed: Vec<Capability>,
    pub used: Vec<Capability>,
}

/// Collect the (declared, used) capabilities of a function. Used capabilities
/// are derived from the runtime calls in the body via [`crate::RtFn::capability`]
/// (the single, exhaustive mapping). NOTE: capabilities reached transitively
/// through [`Op::CallFn`] (user/runtime callees) are a later brick — this
/// witness covers a function's DIRECT host effects.
/// The lifted-function NAME a value denotes, if it was bound by an `Op::FuncRef` in this
/// function — the closures caps fold reads this to follow a `CallIndirect` through a known
/// lambda (MIR values are single-assignment, so the lookup is unambiguous).
fn funcref_name(func: &MirFunction, v: ValueId) -> Option<&str> {
    func.ops.iter().find_map(|op| match op {
        Op::FuncRef { dst, name } if *dst == v => Some(name.as_str()),
        _ => None,
    })
}

pub fn cap_witness(func: &MirFunction) -> CapWitness {
    let mut used: Vec<Capability> = Vec::new();
    for op in &func.ops {
        if let Op::Call { func: rt, .. } = op {
            if let Some(cap) = rt.capability() {
                used.push(cap);
            }
        }
        // The `fd_write` primitive is the host-effect floor op — it reaches Stdout, so
        // a self-hosted runtime fn using it (print_str) must declare Stdout, exactly
        // like a `PrintStr` runtime call (this keeps the sandbox accounting complete).
        if let Op::Prim { kind: crate::PrimKind::FdWrite, .. } = op {
            used.push(Capability::Stdout);
        }
        // SOUNDNESS CRUX: a CallIndirect invokes a closure that may reach ANY capability.
        // When the table index resolves to a KNOWN lifted lambda (a `FuncRef` in THIS
        // function), its REAL caps are folded transitively by `reachable_caps` (which
        // follows the same `FuncRef` edge) — no conservative taint needed, so a non-printing
        // closure stays caps-verified. Only a DYNAMIC closure (table_idx not a local
        // `FuncRef` — e.g. a closure PARAMETER) is unanalyzable here, so it conservatively
        // marks Stdout used: such a fn is caps-verified ONLY if it DECLARES it (a closure
        // that secretly writes Stdout can never pass un-witnessed — accept-but-unsafe).
        if let Op::CallIndirect { table_idx, .. } = op {
            if funcref_name(func, *table_idx).is_none() {
                used.push(Capability::Stdout);
            }
        }
    }
    CapWitness { allowed: func.declared_caps.clone(), used }
}

/// Serialize the capability witness in the format `proofs/CapabilityBound.v`'s
/// `check_caps_cert` parses: `<allowed ids>|<used ids>` (space-separated
/// registry ids, via [`Capability::id`]). The proven checker accepts iff
/// `used ⊆ allowed` (no undeclared host effect).
pub fn cap_witness_string(func: &MirFunction) -> String {
    let w = cap_witness(func);
    let ids = |v: &[Capability]| {
        v.iter().map(|c| c.id().to_string()).collect::<Vec<_>>().join(" ")
    };
    format!("{}|{}", ids(&w.allowed), ids(&w.used))
}

/// The capabilities a function reaches TRANSITIVELY: its direct caps (its own
/// runtime calls) plus those of every function it calls via [`Op::CallFn`], to a
/// fixpoint. `program` maps a function name to its MIR; `visited` breaks cycles.
/// This is the COMPILER-side reachability fold — the proven checker re-verifies
/// the result by the per-call-site subset rule (`check_caps`), so a program is
/// rejected for a capability a CALLEE reaches even with no direct effect.
///
/// NOTE (honest scope): a callee NOT in `program` (out of the lowering subset)
/// contributes no caps here — sound only when every reachable function lowers;
/// treating an unknown callee as reaching ANY capability (conservative reject)
/// is the hardening that makes it sound in general.
pub fn reachable_caps(
    name: &str,
    program: &BTreeMap<String, MirFunction>,
    visited: &mut std::collections::BTreeSet<String>,
) -> Vec<Capability> {
    let mut caps: Vec<Capability> = Vec::new();
    if !visited.insert(name.to_string()) {
        return caps; // already folded in (cycle / diamond)
    }
    let func = match program.get(name) {
        Some(f) => f,
        None => return caps,
    };
    caps.extend(cap_witness(func).used); // direct caps
    for op in &func.ops {
        if let Op::CallFn { name: callee, .. } = op {
            caps.extend(reachable_caps(callee, program, visited));
        }
        // A FuncRef CREATES a closure to a known lifted lambda; fold that lambda's caps at
        // CREATION. This accounts the closure's effects in this function regardless of HOW
        // or WHETHER it is later invoked (a CallIndirect, a deferred call, an operand call,
        // or never) — so there is NO call-site coverage requirement, which is what makes
        // incremental lambda-lifting sound. Precise: a pure lambda folds ∅, a printing one
        // folds Stdout. The same edge cap_witness trusts to drop the CallIndirect taint.
        if let Op::FuncRef { name: callee, .. } = op {
            caps.extend(reachable_caps(callee, program, visited));
        }
    }
    caps
}

/// The TRANSITIVE capability witness for a caller: `<declared ids>|<reachable
/// ids>` (reachable = direct ∪ all callees' caps, transitively). The proven
/// `check_caps_cert` accepts iff `reachable ⊆ declared` — the per-call-site
/// subset rule applied across the call graph, with the checker doing only the
/// subset (the compiler did the reachability fold).
pub fn transitive_cap_witness_string(
    func: &MirFunction,
    program: &BTreeMap<String, MirFunction>,
) -> String {
    let mut visited = std::collections::BTreeSet::new();
    let reachable = reachable_caps(func.name.as_str(), program, &mut visited);
    let ids = |v: &[Capability]| {
        v.iter().map(|c| c.id().to_string()).collect::<Vec<_>>().join(" ")
    };
    format!("{}|{}", ids(&func.declared_caps), ids(&reachable))
}

/// Conservative transitive capability-reachability — the SOUND basis for a
/// corpus capability gate across `Op::CallFn` edges. A function's empty (direct)
/// capability witness is a sound claim of effect-freedom ONLY if this returns
/// `false`: the direct witness alone misses what a CALLEE reaches, and
/// [`reachable_caps`] treats an unknown callee as contributing ∅ — unsound for an
/// effectful one (its honest-scope caveat). This closes that hole conservatively.
///
/// Returns `true` if `name` reaches a host capability DIRECTLY (it has an
/// `Op::Call` whose `RtFn` bears one) or through ANY `Op::CallFn` callee that is
/// not provably effect-free. A callee NOT in `program` is provably free only when
/// `is_known_free(callee)` — the CALLER supplies that policy (e.g. variant
/// constructors, known effect-free builtins, purity-gated stdlib `Module` calls).
/// Any other unknown callee (a walled or cross-file user function whose effects
/// are unseen) is treated as reaching a capability — the conservative direction,
/// so a gate built on this NEVER over-accepts. `visited` breaks cycles.
///
/// `is_elided(name)` reports a function whose source had MORE call nodes than its
/// MIR has call-ops — i.e. a call ELIDED by Opaque lowering (a list element, a
/// ctor payload, a BinOp operand). An elided call's effects are absent from
/// `func.ops`, so this fold cannot see them; such a function (and so any caller)
/// is conservatively TAINTED — its capability witness is incompletely captured
/// and must not be claimed safe.
pub fn reaches_capability_or_unknown(
    name: &str,
    program: &BTreeMap<String, MirFunction>,
    is_known_free: &dyn Fn(&str) -> bool,
    is_elided: &dyn Fn(&str) -> bool,
    visited: &mut std::collections::BTreeSet<String>,
) -> bool {
    if !visited.insert(name.to_string()) {
        return false; // cycle / diamond: already accounted on the stack
    }
    let func = match program.get(name) {
        Some(f) => f,
        None => return !is_known_free(name),
    };
    if is_elided(name) {
        return true; // an elided call hides effects from this fold — conservatively tainted
    }
    if !cap_witness(func).used.is_empty() {
        return true; // a direct host effect (today: Stdout via an RtFn `Op::Call`)
    }
    func.ops.iter().any(|op| match op {
        Op::CallFn { name: callee, .. } => {
            reaches_capability_or_unknown(callee, program, is_known_free, is_elided, visited)
        }
        // A FuncRef closure's effects reach this function at creation — fold like a callee
        // (the boolean counterpart of the FuncRef edge in `reachable_caps_or_tainted`).
        Op::FuncRef { name: callee, .. } => {
            reaches_capability_or_unknown(callee, program, is_known_free, is_elided, visited)
        }
        _ => false,
    })
}

/// The transitive reachable capabilities of `name`, or `None` if its `Op::CallFn`
/// closure hits an UNANALYZABLE callee — an unknown/cross-file callee (not in
/// `program` and not `is_known_free`) or one with an ELIDED call (`is_elided`)
/// whose effects are absent from its MIR. A `None` function cannot be capability-
/// verified (its reachable set is incomplete, so a hidden effect could exceed any
/// declared bound). A `Some(set)` function's effects are FULLY known: the gate
/// then emits `<declared>|<set>` and the proven `check_caps_cert` verifies
/// `set ⊆ declared` — so an EFFECTFUL function is verified against its OWN declared
/// capability bound, not merely excluded for touching a capability. This is the
/// set-valued counterpart of [`reaches_capability_or_unknown`].
pub fn reachable_caps_or_tainted(
    name: &str,
    program: &BTreeMap<String, MirFunction>,
    is_known_free: &dyn Fn(&str) -> bool,
    is_elided: &dyn Fn(&str) -> bool,
    visited: &mut std::collections::BTreeSet<String>,
) -> Option<Vec<Capability>> {
    if !visited.insert(name.to_string()) {
        return Some(Vec::new()); // cycle / diamond: already folded on the stack
    }
    let func = match program.get(name) {
        Some(f) => f,
        None => return if is_known_free(name) { Some(Vec::new()) } else { None },
    };
    if is_elided(name) {
        return None; // an elided call hides effects from this fold — unanalyzable
    }
    let mut caps = cap_witness(func).used;
    for op in &func.ops {
        if let Op::CallFn { name: callee, .. } = op {
            match reachable_caps_or_tainted(callee, program, is_known_free, is_elided, visited) {
                Some(c) => caps.extend(c),
                None => return None,
            }
        }
        // A FuncRef CREATES a closure to a lifted lambda — fold its caps at CREATION,
        // exactly as [`reachable_caps`] does. Coverage-free: the closure's effects reach
        // this function however or whether it is later invoked (a CallIndirect, a deferred
        // call, or never). WITHOUT this, a function holding a printing lifted lambda would
        // be falsely caps-VERIFIED here (the lambda's Stdout unseen by the CallFn-only fold)
        // the moment lambda-lifting emits FuncRef into the corpus — an accept-but-unsafe
        // hole. The lambda is in `program` (the harness puts every lifted aux in the
        // in-profile map); an unanalyzable/elided lambda taints (`None`) like any callee.
        if let Op::FuncRef { name: callee, .. } = op {
            match reachable_caps_or_tainted(callee, program, is_known_free, is_elided, visited) {
                Some(c) => caps.extend(c),
                None => return None,
            }
        }
    }
    Some(caps)
}

/// Per-object refcount-event accumulator, preserving object creation order.
struct Streams {
    of: BTreeMap<ValueId, ValueId>, // handle → object representative
    order: Vec<ValueId>,            // objects in first-seen order
    stream: BTreeMap<ValueId, String>,
}

impl Streams {
    fn new() -> Self {
        Streams { of: BTreeMap::new(), order: Vec::new(), stream: BTreeMap::new() }
    }
    /// Record a +1/−1 event (`'i'`/`'d'`) on object `o`.
    fn event(&mut self, o: ValueId, c: char) {
        if !self.stream.contains_key(&o) {
            self.stream.insert(o, String::new());
            self.order.push(o);
        }
        self.stream.get_mut(&o).unwrap().push(c);
    }
    fn object_of(&self, handle: ValueId) -> ValueId {
        // Well-formed MIR always has the handle mapped; fall back to identity so a
        // malformed input yields an unbalanced (rejected) certificate rather than
        // a panic.
        self.of.get(&handle).copied().unwrap_or(handle)
    }
}

/// Emit the per-object ownership certificate (format v0) for a function.
pub fn ownership_certificate(func: &MirFunction) -> String {
    let mut s = Streams::new();

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

    for op in &func.ops {
        match op {
            Op::Alloc { dst, .. } => {
                s.of.insert(*dst, *dst);
                s.event(*dst, 'i');
            }
            Op::Dup { dst, src } => {
                // ALIAS acquire (+1): a new handle on an existing shared object.
                // `a` (not `i`) records the share-vs-move ground fact (format v1).
                let o = s.object_of(*src);
                s.of.insert(*dst, o);
                s.event(o, 'a');
            }
            // Plain release (−1). A `DropListStr` is the SAME single `d` on the LIST object —
            // its element Strings were already accounted as `m` (consumed) when stored into
            // it, so the recursive runtime free adds no extra cert event.
            Op::Drop { v } | Op::DropListStr { v } => {
                let o = s.object_of(*v);
                s.event(o, 'd');
            }
            // MOVE-OUT (−1): the reference is transferred out (into a container /
            // a consuming callee). `m` distinguishes move from a plain drop.
            Op::Consume { v } => {
                let o = s.object_of(*v);
                s.event(o, 'm');
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
            Op::Call { dst: Some(d), result: Some(r), .. }
            | Op::CallFn { dst: Some(d), result: Some(r), .. }
            | Op::CallIndirect { dst: Some(d), result: Some(r), .. }
                if r.is_heap() =>
            {
                s.of.insert(*d, *d);
                s.event(*d, 'i');
            }
            // No refcount change: Borrow/MakeUnique/Const/Pure/IntBinOp, and a
            // call with a void/scalar result (its heap-handle args are borrowed).
            _ => {}
        }
    }

    // A heap return is MOVED OUT to the caller (a −1) — a move, hence `m`.
    if let Some(r) = func.ret {
        if s.of.contains_key(&r) {
            let o = s.object_of(r);
            s.event(o, 'm');
        }
    }

    let mut out = String::new();
    for o in &s.order {
        out.push_str(&s.stream[o]);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        verify_ownership, CallArg, Capability, Init, MirFunction, MirParam, Op, Repr, RtFn,
        ValueId, PLACEHOLDER_LAYOUT,
    };

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }
    fn func(ops: Vec<Op>) -> MirFunction {
        MirFunction { name: "f".into(), ops, ..Default::default() }
    }

    #[test]
    fn alias_then_drops_is_one_balanced_object() {
        // a = Alloc; b = Dup a; Drop a; Drop b  → ONE object (a), stream "iidd".
        let (a, b) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Dup { dst: b, src: a },
            Op::Drop { v: a },
            Op::Drop { v: b },
        ]);
        // Alloc(i), Dup→alias(a), Drop(d), Drop(d): the alias acquire is `a`.
        assert_eq!(ownership_certificate(&f), "iadd\n");
        assert_eq!(verify_ownership(&f), Ok(())); // checker would accept ⟺ this
    }

    #[test]
    fn two_objects_each_balanced() {
        let (a, b) = (ValueId(0), ValueId(1));
        let f = func(vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::Alloc { dst: b, repr: heap(), init: Init::Opaque },
            Op::Drop { v: a },
            Op::Drop { v: b },
        ]);
        // object a: "id", object b: "id" — two balanced lines.
        assert_eq!(ownership_certificate(&f), "id\nid\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn leak_shows_as_unbalanced_object() {
        // a allocated, never dropped → stream "i" (rc ends 1 = leak).
        let a = ValueId(0);
        let f = func(vec![Op::Alloc { dst: a, repr: heap(), init: Init::Opaque }]);
        assert_eq!(ownership_certificate(&f), "i\n");
        // verify_ownership flags it too — the certificate faithfully carries it.
        assert!(verify_ownership(&f).is_err());
    }

    // ── faithfulness mechanism ──
    // The certificate must honestly represent the ownership pass: the proven
    // checker's verdict on `ownership_certificate(f)` must equal `verify_ownership(f)`'s.
    // Otherwise the PCC chain certifies the wrong thing. We pin it over many
    // random WELL-FORMED ownership sequences.

    /// Re-run the proven checker's decision in Rust (mirrors the Coq `check_all`):
    /// every line's i/d stream must never dec-below-zero and must end at 0.
    fn cert_all_balanced(cert: &str) -> bool {
        cert.lines().all(|line| {
            let mut rc: i64 = 0;
            for c in line.chars() {
                match c {
                    // format v1: i/a = +1 (fresh/alias), d/m = −1 (release/move-out).
                    'i' | 'a' => rc += 1,
                    'd' | 'm' => {
                        if rc == 0 {
                            return false; // double-free / use-after-move
                        }
                        rc -= 1;
                    }
                    _ => {}
                }
            }
            rc == 0 // leak iff != 0
        })
    }

    /// A tiny seeded PRNG (no dep), so the random test is deterministic.
    fn next_rand(state: &mut u64) -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *state
    }

    /// Build a random WELL-FORMED ownership op sequence (only dup/drop LIVE
    /// handles). Leftover-undropped handles make it a leak; dropping all makes it
    /// balanced — so the corpus spans accept and reject.
    fn gen_wellformed(seed: u64) -> MirFunction {
        let mut st = seed.wrapping_add(1);
        let mut live: Vec<ValueId> = Vec::new();
        let mut next: u32 = 0;
        let mut ops: Vec<Op> = Vec::new();
        let steps = 3 + (next_rand(&mut st) % 9) as usize;
        for _ in 0..steps {
            let choice = next_rand(&mut st) % 4;
            match choice {
                0 => {
                    // Alloc a fresh object.
                    let v = ValueId(next);
                    next += 1;
                    ops.push(Op::Alloc { dst: v, repr: heap(), init: Init::Opaque });
                    live.push(v);
                }
                1 if !live.is_empty() => {
                    // Dup a live handle → a new handle on the same object.
                    let src = live[(next_rand(&mut st) as usize) % live.len()];
                    let v = ValueId(next);
                    next += 1;
                    ops.push(Op::Dup { dst: v, src });
                    live.push(v);
                }
                2 if !live.is_empty() => {
                    // Drop a live handle.
                    let i = (next_rand(&mut st) as usize) % live.len();
                    let v = live.remove(i);
                    ops.push(Op::Drop { v });
                }
                _ if !live.is_empty() => {
                    // Borrow (no refcount change — must be skipped by the cert).
                    let v = live[(next_rand(&mut st) as usize) % live.len()];
                    ops.push(Op::Borrow { v });
                }
                _ => {}
            }
        }
        func(ops)
    }

    #[test]
    fn certificate_verdict_matches_verify_ownership() {
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            let cert_ok = cert_all_balanced(&ownership_certificate(&f));
            let verify_ok = verify_ownership(&f).is_ok();
            assert_eq!(
                cert_ok, verify_ok,
                "seed {seed}: certificate says {cert_ok}, verify_ownership says {verify_ok}\nops: {:?}",
                f.ops
            );
        }
    }

    #[test]
    fn name_witness_total_for_wellformed_mirs() {
        // The 2nd property: every used value id is defined (no dangling MIR
        // reference). For well-formed MIRs the witness satisfies the proven
        // `check_names` (used ⊆ defined). Pinned over the random corpus.
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            let w = name_witness(&f);
            for u in &w.used {
                assert!(
                    w.defined.contains(u),
                    "seed {seed}: used {u:?} is not defined (dangling)\nops: {:?}",
                    f.ops
                );
            }
        }
    }

    #[test]
    fn cap_witness_derives_used_from_runtime_calls() {
        // The 4th property: used capabilities come from the body's runtime calls
        // (PrintInt reaches Stdout); pure heap ops reach none. The witness checks
        // them against the declared bound.
        let print = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
        ]);
        assert_eq!(cap_witness(&print).used, vec![Capability::Stdout]);

        // A pure heap op (no host effect) leaves the used set empty.
        let pure = func(vec![
            Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Opaque },
            Op::MakeUnique { v: ValueId(0) },
            Op::Call {
                dst: Some(ValueId(0)),
                func: RtFn::ListPush,
                args: vec![CallArg::Handle(ValueId(0)), CallArg::Imm(1)],
            result: None },
            Op::Drop { v: ValueId(0) },
        ]);
        assert!(cap_witness(&pure).used.is_empty());
    }

    #[test]
    fn call_indirect_conservatively_taints_every_capability() {
        // THE CLOSURES SOUNDNESS CRUX: a CallIndirect invokes an unanalyzable closure that
        // may reach ANY capability, so the witness must conservatively mark every modeled
        // cap (Stdout) USED — a fn with a closure call is caps-verified ONLY if it DECLARES
        // it. A pure-looking fn that calls a secretly-Stdout closure can never pass
        // un-witnessed (accept-but-unsafe).
        let closure_caller = func(vec![
            Op::ConstInt { dst: ValueId(0), value: 0 }, // the closure value (a table index)
            Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None },
        ]);
        let w = cap_witness(&closure_caller);
        assert_eq!(w.used, vec![Capability::Stdout], "a CallIndirect must witness Stdout used");
        // With no declared caps (the default), used ⊄ allowed → the proven `used ⊆ allowed`
        // checker REJECTS it as caps-verified — it stays honestly caps-unverified.
        assert!(
            !w.used.iter().all(|c| w.allowed.contains(c)),
            "a closure caller with no declared caps must NOT be silently caps-verified"
        );
    }

    #[test]
    fn call_indirect_through_a_pure_funcref_folds_no_caps() {
        use std::collections::{BTreeMap, BTreeSet};
        // PRECISE FOLD: main = `f = FuncRef("pure_lambda"); (f)()` where the lambda is PURE.
        // The table index resolves to a KNOWN lifted lambda, so cap_witness does NOT
        // conservatively taint Stdout — the lambda's real (empty) caps are folded instead,
        // keeping a non-printing closure caps-VERIFIED (no spurious taint, no corpus drop).
        let mut pure_lambda = func(vec![Op::ConstInt { dst: ValueId(0), value: 1 }]);
        pure_lambda.name = "pure_lambda".into();
        let mut main = func(vec![
            Op::FuncRef { dst: ValueId(0), name: "pure_lambda".into() },
            Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None },
        ]);
        main.name = "main".into();
        assert!(
            cap_witness(&main).used.is_empty(),
            "a CallIndirect to a known pure lambda must not conservatively taint"
        );
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [pure_lambda, main.clone()] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        assert!(
            reachable_caps("main", &program, &mut seen).is_empty(),
            "a pure-lambda closure reaches no capability"
        );
    }

    #[test]
    fn call_indirect_through_a_printing_funcref_still_reaches_stdout() {
        use std::collections::{BTreeMap, BTreeSet};
        // ADVERSARIAL: the lambda SECRETLY prints. The known-FuncRef fold must STILL surface
        // its Stdout (no accept-but-unsafe) — the fold follows the same edge cap_witness
        // dropped the taint for, so a printing lambda's effect always reaches the caller.
        let mut printing_lambda = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call {
                dst: None,
                func: RtFn::PrintInt,
                args: vec![CallArg::Scalar(ValueId(0))],
                result: None,
            },
        ]);
        printing_lambda.name = "printing_lambda".into();
        let mut main = func(vec![
            Op::FuncRef { dst: ValueId(0), name: "printing_lambda".into() },
            Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None },
        ]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [printing_lambda, main.clone()] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        assert!(
            reachable_caps("main", &program, &mut seen).contains(&Capability::Stdout),
            "a printing closure must still surface Stdout transitively (no accept-but-unsafe)"
        );
        assert_eq!(
            transitive_cap_witness_string(&main, &program),
            "|0",
            "rejected: declares no caps but reaches Stdout through the lambda"
        );
    }

    #[test]
    fn func_ref_alone_accounts_the_lambda_caps_without_a_call_indirect() {
        use std::collections::{BTreeMap, BTreeSet};
        // COVERAGE-FREE SOUNDNESS: a FuncRef to a printing lambda accounts its Stdout in the
        // creating function EVEN WITH NO CallIndirect — the closure might be invoked via a
        // deferred/operand call path or passed elsewhere, so accounting at CREATION means
        // incremental lambda-lifting cannot lose a closure's effect however the call lowers.
        let mut printing = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call {
                dst: None,
                func: RtFn::PrintInt,
                args: vec![CallArg::Scalar(ValueId(0))],
                result: None,
            },
        ]);
        printing.name = "printing".into();
        // main only CREATES the closure (FuncRef) — it does NOT CallIndirect it.
        let mut main = func(vec![Op::FuncRef { dst: ValueId(0), name: "printing".into() }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [printing, main.clone()] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        assert!(
            reachable_caps("main", &program, &mut seen).contains(&Capability::Stdout),
            "creating a closure to a printing lambda must account its Stdout even without a call"
        );
    }

    #[test]
    fn cap_witness_string_matches_the_coq_parser_format() {
        // declares Stdout, prints → `0|0`  (allowed ⊇ used → checker accepts).
        let mut declared = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
        ]);
        declared.declared_caps = vec![Capability::Stdout];
        assert_eq!(cap_witness_string(&declared), "0|0");

        // declares nothing, prints → `|0`  (used ⊄ allowed → checker rejects).
        let mut undeclared = declared.clone();
        undeclared.declared_caps = vec![];
        assert_eq!(cap_witness_string(&undeclared), "|0");
    }

    #[test]
    fn reachable_caps_folds_transitively_and_survives_cycles() {
        use std::collections::{BTreeMap, BTreeSet};
        // main → beep (prints, reaches Stdout). main has NO direct effect.
        let mut beep = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
        ]);
        beep.name = "beep".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "beep".into(), args: vec![] , result: None }]);
        main.name = "main".into();
        // A cycle main→loop→main must not diverge.
        let mut looper = func(vec![Op::CallFn { dst: None, name: "main".into(), args: vec![] , result: None }]);
        looper.name = "loop".into();
        main.ops.push(Op::CallFn { dst: None, name: "loop".into(), args: vec![] , result: None });

        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [beep, main.clone(), looper] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        let reach = reachable_caps("main", &program, &mut seen);
        // main reaches Stdout ONLY transitively (via beep) — the per-call-site fold.
        assert!(reach.contains(&Capability::Stdout));
        // And the transitive witness rejects (declared empty, reachable Stdout).
        assert_eq!(transitive_cap_witness_string(&main, &program), "|0");
    }

    // ── borrow-by-default calling convention (heap params) ──
    // A heap param is BORROWED: the caller owns the reference. So a param emits
    // NO `i` (no synthetic +1), and a body that releases/returns it WITHOUT first
    // acquiring its own reference (a `Dup`) is correctly REJECTED. The cert and
    // `verify_ownership` must agree on every case below.

    fn param_fn(name: &str, ops: Vec<Op>, ret: Option<ValueId>) -> MirFunction {
        MirFunction {
            name: name.into(),
            params: vec![MirParam { value: ValueId(0), repr: heap() }],
            ops,
            ret,
            ..Default::default()
        }
    }

    #[test]
    fn borrow_only_param_has_no_ownership_event() {
        // fn(p) { borrow p } — the param is read, never owned: empty cert, accepts.
        let f = param_fn("borrow_only", vec![Op::Borrow { v: ValueId(0) }], None);
        assert_eq!(ownership_certificate(&f), "");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn returning_a_borrowed_param_directly_is_rejected() {
        // fn(p) -> p : returning the borrowed reference without acquiring our own
        // hands the caller a SECOND owner of its own reference = a double-free.
        // Cert is `m` at rc 0 (no preceding `i`) → the proven checker faults.
        let f = param_fn("return_param", vec![], Some(ValueId(0)));
        assert_eq!(ownership_certificate(&f), "m\n");
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn acquiring_then_returning_a_param_balances() {
        // fn(p) { let q = dup p; q } — the CORRECT way to return a param: acquire
        // our own reference (`a`) then move it out (`m`). Cert `am` balances.
        let f = param_fn(
            "acquire_return",
            vec![Op::Dup { dst: ValueId(1), src: ValueId(0) }],
            Some(ValueId(1)),
        );
        assert_eq!(ownership_certificate(&f), "am\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn releasing_a_borrowed_param_is_rejected() {
        // fn(p) { drop p } — releasing a reference we do not own. Cert `d` at
        // rc 0 → faulted; verify reports the double-free.
        let f = param_fn("drop_borrow", vec![Op::Drop { v: ValueId(0) }], None);
        assert_eq!(ownership_certificate(&f), "d\n");
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn passing_a_borrowed_param_to_a_call_is_accepted() {
        // fn(p) { g(p) } — borrowing the param into a call (no refcount change):
        // no cert event for the param, accepts.
        let f = param_fn(
            "forward",
            vec![Op::CallFn {
                dst: None,
                name: "g".into(),
                args: vec![CallArg::Handle(ValueId(0))],
                result: None,
            }],
            None,
        );
        assert_eq!(ownership_certificate(&f), "");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    // ── conservative transitive capability reachability (brick #49) ──

    #[test]
    fn transitive_capability_through_callfn_is_caught() {
        use std::collections::{BTreeMap, BTreeSet};
        // main → beep; beep prints (PrintInt → Stdout). main has NO direct cap but
        // reaches one transitively — the fold MUST flag it (the direct witness wouldn't).
        let mut beep = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))], result: None },
        ]);
        beep.name = "beep".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "beep".into(), args: vec![], result: None }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [beep, main] {
            program.insert(f.name.clone(), f);
        }
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        let mut v = BTreeSet::new();
        assert!(reaches_capability_or_unknown("main", &program, &none_free, &not_elided, &mut v));
    }

    #[test]
    fn an_elided_call_taints_the_function_and_its_callers() {
        use std::collections::{BTreeMap, BTreeSet};
        // main → helper; helper has NO direct cap and NO CallFn, but it ELIDED a
        // call (its source had a call the MIR dropped to Opaque) — so its caps are
        // incompletely captured. main must be tainted transitively.
        let mut helper = func(vec![Op::Const { dst: ValueId(0) }]); // looks pure, but elided
        helper.name = "helper".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "helper".into(), args: vec![], result: None }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [helper, main] {
            program.insert(f.name.clone(), f);
        }
        let none_free = |_: &str| false;
        let elided_helper = |n: &str| n == "helper";
        let mut v = BTreeSet::new();
        assert!(reaches_capability_or_unknown("main", &program, &none_free, &elided_helper, &mut v));
        // Without the elision, the same pure chain is effect-free.
        let not_elided = |_: &str| false;
        let mut v2 = BTreeSet::new();
        assert!(!reaches_capability_or_unknown("main", &program, &none_free, &not_elided, &mut v2));
    }

    #[test]
    fn unknown_callee_is_conservatively_tainted_unless_freed_by_policy() {
        use std::collections::{BTreeMap, BTreeSet};
        // f → helper, helper NOT in the program: tainted by default, free iff the policy says so.
        let mut f = func(vec![Op::CallFn { dst: None, name: "helper".into(), args: vec![], result: None }]);
        f.name = "f".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        program.insert("f".to_string(), f);
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        let mut v1 = BTreeSet::new();
        assert!(reaches_capability_or_unknown("f", &program, &none_free, &not_elided, &mut v1));
        let helper_free = |n: &str| n == "helper";
        let mut v2 = BTreeSet::new();
        assert!(!reaches_capability_or_unknown("f", &program, &helper_free, &not_elided, &mut v2));
    }

    #[test]
    fn pure_chain_and_cycle_are_effect_free() {
        use std::collections::{BTreeMap, BTreeSet};
        // a → b → a, both pure (no caps, no unknown callees): effect-free, terminates.
        let mut a = func(vec![Op::CallFn { dst: None, name: "b".into(), args: vec![], result: None }]);
        a.name = "a".into();
        let mut b = func(vec![Op::CallFn { dst: None, name: "a".into(), args: vec![], result: None }]);
        b.name = "b".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for x in [a, b] {
            program.insert(x.name.clone(), x);
        }
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        let mut v = BTreeSet::new();
        assert!(!reaches_capability_or_unknown("a", &program, &none_free, &not_elided, &mut v));
    }

    #[test]
    fn reachable_caps_returns_the_set_or_taints() {
        use std::collections::{BTreeMap, BTreeSet};
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        // main → beep (prints Stdout): reachable = {Stdout}, FULLY known.
        let mut beep = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))], result: None },
        ]);
        beep.name = "beep".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "beep".into(), args: vec![], result: None }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [beep, main] {
            program.insert(f.name.clone(), f);
        }
        let mut v = BTreeSet::new();
        assert_eq!(
            reachable_caps_or_tainted("main", &program, &none_free, &not_elided, &mut v),
            Some(vec![Capability::Stdout])
        );
        // An unknown callee → None (incomplete reachable set, cannot verify).
        let mut f = func(vec![Op::CallFn { dst: None, name: "x".into(), args: vec![], result: None }]);
        f.name = "f".into();
        let mut p2: BTreeMap<String, MirFunction> = BTreeMap::new();
        p2.insert("f".to_string(), f);
        let mut v2 = BTreeSet::new();
        assert_eq!(reachable_caps_or_tainted("f", &p2, &none_free, &not_elided, &mut v2), None);
        // An elided callee → None.
        let elided = |n: &str| n == "f";
        let mut v3 = BTreeSet::new();
        assert_eq!(reachable_caps_or_tainted("f", &p2, &none_free, &elided, &mut v3), None);
    }

    #[test]
    fn tainted_fold_follows_funcref_edges_to_a_lifted_lambda() {
        use std::collections::{BTreeMap, BTreeSet};
        // THE harness path (classify_corpus uses `reachable_caps_or_tainted`, NOT
        // `reachable_caps`). A main that holds a lifted lambda via `Op::FuncRef` must fold
        // that lambda's caps here too, or the corpus gate would falsely caps-VERIFY a main
        // whose lifted lambda secretly prints (accept-but-unsafe) the moment lambda-lifting
        // emits FuncRef into the corpus.
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        // ADVERSARIAL: the lifted lambda prints. main = `FuncRef(printing_lambda)` only —
        // no CallIndirect — so coverage-free folding (at creation) must still surface Stdout.
        let mut printing_lambda = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call {
                dst: None,
                func: RtFn::PrintInt,
                args: vec![CallArg::Scalar(ValueId(0))],
                result: None,
            },
        ]);
        printing_lambda.name = "printing_lambda".into();
        let mut main = func(vec![Op::FuncRef { dst: ValueId(0), name: "printing_lambda".into() }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [printing_lambda, main] {
            program.insert(f.name.clone(), f);
        }
        let mut v = BTreeSet::new();
        assert_eq!(
            reachable_caps_or_tainted("main", &program, &none_free, &not_elided, &mut v),
            Some(vec![Capability::Stdout]),
            "a main holding a printing lifted lambda must reach Stdout in the harness fold"
        );
        // A PURE lifted lambda folds ∅ — no spurious taint (keeps a non-printing closure
        // caps-verified, so the corpus caps count does not drop).
        let mut pure_lambda = func(vec![Op::ConstInt { dst: ValueId(0), value: 1 }]);
        pure_lambda.name = "pure_lambda".into();
        let mut main2 = func(vec![Op::FuncRef { dst: ValueId(0), name: "pure_lambda".into() }]);
        main2.name = "main".into();
        let mut p2: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [pure_lambda, main2] {
            p2.insert(f.name.clone(), f);
        }
        let mut v2 = BTreeSet::new();
        assert_eq!(
            reachable_caps_or_tainted("main", &p2, &none_free, &not_elided, &mut v2),
            Some(Vec::new()),
            "a main holding a pure lifted lambda reaches no capability"
        );
    }

    #[test]
    fn every_plus_one_event_is_backed_by_a_real_op() {
        // The NON-RECURRING soundness gate. Borrow-by-default holds iff EVERY `+1`
        // in the certificate is backed by a real runtime op: an `i` by an `Alloc`
        // or a heap-result call, an `a` by a `Dup`. A param can NEVER inject an
        // unbacked `+1` (the gate-blind use-after-free class). If a future brick
        // ever emits a param `i` again, this equality breaks and the gate fails.
        fn backed(f: &MirFunction) -> bool {
            let cert = ownership_certificate(f);
            let i = cert.chars().filter(|c| *c == 'i').count();
            let a = cert.chars().filter(|c| *c == 'a').count();
            let allocs = f.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
            let heap_results = f
                .ops
                .iter()
                .filter(|o| match o {
                    Op::Call { dst: Some(_), result: Some(r), .. }
                    | Op::CallFn { dst: Some(_), result: Some(r), .. } => r.is_heap(),
                    _ => false,
                })
                .count();
            let dups = f.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count();
            i == allocs + heap_results && a == dups
        }
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            assert!(backed(&f), "seed {seed} has an unbacked +1\nops: {:?}", f.ops);
        }
        // Param-bearing functions: the borrowed param injects no `i`/`a`.
        assert!(backed(&param_fn("b", vec![Op::Borrow { v: ValueId(0) }], None)));
        assert!(backed(&param_fn(
            "ar",
            vec![Op::Dup { dst: ValueId(1), src: ValueId(0) }],
            Some(ValueId(1)),
        )));
    }
}
