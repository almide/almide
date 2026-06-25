# 柱C extension: bring Value rc into the certified region

**Status: Brick 1+2 LANDED — carrier rc is CERTIFIED (verify_ownership + ownership_certificate, atomic, agree). 2026-06-24.**

The prim.handle-fed Value-rc class is now IN the certified region: an unbalanced `rc_inc(prim.handle(v))` is a Leak that BOTH the executable verifier and the proven cert catch (`i a d` → rc>0). corpus-wall ACCEPT 18165 (byte-identical), mir 502/0, the verify_ownership↔cert agreement holds, unit test `value_rc_carrier_balance_is_certified`. CORRECTION to the earlier scope: the corpus DOES contain a prim.handle-fed rc_dec (`__drop_value`'s `prim.rc_dec(prim.handle(v))`), but it ACCEPTs — the conservative RcDec-at-rc-0 no-op (verifier) + the carrier projection (cert) keep the borrowed-param self-consume sound, verified by the proven Coq checker. The REMAINING brick is #3 (load64→object slot provenance) which actually verifies reduce_value's `__copy_value` (load64-fed children); until then load64-fed Value rc stays on the differential-test floor.

## Why
The Value reference-count ops (`prim.rc_inc`/`rc_dec`, `Op::DropValue`) live in the PRIM region the ownership checker treats as a NO-OP. So Value-rc balance is INVISIBLE to the cert — only the coarse leak-test (OOM/trap over 5000×) catches an imbalance. This is the structural blocker that stopped `list.reduce_value`: byte-correct + 5/7 leak-safe, but the heap-payload accumulator-return (`(a,b)=>a` over Str/Array) leaked, and getting the rc exactly right was trial-and-error against the coarse leak-test with no verification feedback. corpus-wall ACCEPTs such code (prim-rc blind) — an accept-but-unsafe only the leak-test guards.

## Feasibility (4-agent scope, validated): tractable, RUST-FIRST
- **The Coq proof is NOT the blocker.** OwnershipChecker.v's Op alphabet already carries Inc/Alias (+1) and Dec/MoveOut (−1); `exec` already faults a −1 at rc≤0 (double-free) and leaks any object with rc>0 at end; RuntimeModel.v/WasmRcDec.v already prove the emitted `$rc_dec` realizes the abstract rc cell-write. Modeling rc needs NO new lemma and no re-proof of `check_line_unroll_sound`.
- **The blocker is purely the untrusted Rust side**: `verify_ownership` (lib.rs:844) and `ownership_certificate` (certificate.rs) treat `Op::Prim{RcInc/RcDec}` as a flat no-op, so rc balance is invisible.
- **Provenance is recoverable for the NAMEABLE case**: `prim.handle(v)` lowers with the source object in `args[0]` (calls_p4.rs:790), so the checker can reconstruct `handle→object` (an address-carrier) with NO MIR redesign. The honest boundary: `prim.load64(raw_addr)`-fed rc (the loop-bodied element rc in `__varr_copy`/`__drop_value`, and `__copy_value`'s children) stays opaque and remains on the differential-test floor until a slot-provenance brick (typed slot layouts / `Repr::HandleAddr`).

## The bricks (land atomically per the certificate_p2.rs invariant "cert ⟺ verify_ownership agree on every case")
1. **Carrier rc in BOTH verify_ownership AND ownership_certificate, together.** verify_ownership (PROTOTYPED + gated: corpus-wall ACCEPT 18165, mir 501/0): `Prim{Handle,dst,[src]}` → `object_of[dst]=object_of[src]` (carrier, no rc/dead); `RcInc(carrier)` → `rc[o]+=1`; `RcDec(carrier)` → release only if `rc≥1` (rc_dec-on-borrowed-at-rc-0 stays a no-op — the conservative escape that cannot newly-reject the existing corpus). ownership_certificate must MIRROR this (emit `a` for RcInc(carrier), `d` for RcDec(carrier,owned)) so the two AGREE and the Coq checker (already rc-aware) verifies. Corpus impact: NONE — every corpus rc op is load64-fed (no carrier), so no new cert bytes, `plus_one_events_backed` unbroken, corpus-wall byte-identical ACCEPT.
2. **plus_one_events_backed extension** (classify_corpus.rs:331) — count RcInc prim ops as backing the cert `a`, atomically with brick 1's cert emission (else the `i==allocs+heap_results && a==dups` assert breaks).
3. **load64→object slot provenance** — the REAL reduce_value fix. Track a loaded element handle (`prim.load_handle(block+12+i*8)`) back to the block's element-object via a typed slot layout, so the loop-bodied element rc in `__copy_value`/`__varr_copy` becomes verifiable. This is the larger brick (needs Repr::HandleAddr or slot-typed loads). Until then reduce_value (and any load64-fed Value-rc combinator) stays on the leak-test floor / walled.

## Corpus risk (mapped)
- `plus_one_events_backed` hard-asserts `i==allocs+heap_results && a==dups` — the instant rc emits a cert byte for a CORPUS fn it breaks. AVOIDED because corpus rc is all load64-fed (no carrier → no cert byte). Brick 1+2 only emit for the prim.handle-fed case, which does not occur in the corpus yet.
- `__drop_value:740` does `prim.rc_dec(prim.handle(v))` on a BORROWED param (rc 0). The conservative RcDec rule (model only rc≥1) keeps this a no-op → cannot newly-reject. (A future precise rule "RcDec on a borrowed carrier = the authorized self-consume" would model it, but is NOT needed for brick 1.)

## Prototyped diff (reverted, lib.rs verify_ownership Prim arm)
Split the `| Op::Prim { .. }` no-op into `Op::Prim { kind, dst, args } => match kind { Handle => carrier; RcInc => rc+=1 if carrier; RcDec => rc-=1 if carrier && rc≥1; _ => {} }`. Gated: cargo test -p almide-mir 501/0, corpus-wall ACCEPT 18165 (byte-identical). It is a STRICT REFINEMENT for the corpus but introduces a verify_ownership↔cert DIVERGENCE for prim.handle-fed rc — so it must land WITH the matching ownership_certificate emission (brick 1+2 atomic), never alone.

## Brick 3 (load64-fed nested-element rc) — FOUNDATION LANDED: the core composition lemma is PROVEN (proofs/CoownLoop.v, 2026-06-25)

The conceptual hardest piece is done: `proofs/CoownLoop.v` proves on the Coq kernel (axiom-clean, coqchk-reverified, in check.sh's proof gate) the CO-OWN COPY / RECURSIVE-DROP balance OwnershipLoop.v explicitly excluded — model the container's immediate elements as a refcount VECTOR; the producer co-own FILL (+1 each, the rc_inc) followed by the container's recursive DROP (-1 each, faulting at <=0) returns EVERY source element to its original rc (`coown_fill_drop_neutral`), giving `coown_copy_no_double_free` + `coown_copy_no_leak` for ANY element count. This is the net-+1-balanced-by-a-separate-recursive-drop case keyed by element count, not per-iteration. REMAINING (engineering): the MIR/cert integration that pairs a concrete producer's fill events with the matching recursive-drop by container identity (a typed nested-element model + a cert section) so __copy_value/__varr_copy/value.merge inherit the proven balance instead of riding the leak-floor. The original scope (kept below for the integration design):

### Original scope — the integration that consumes CoownLoop.v (still fresh engineering)

There is NO small SOUND cert-proving slice. The "structural reduces to the proven map-fill" idea is WRONG: map-fill's per-iteration body is net-0 (rebind: drop-old + inc-new), which is exactly why OwnershipLoop.v's rc-preserving Loop rule accepts it; a CO-OWN copy loop is net-+1 per element, and OwnershipLoop.v (header line 19-21) explicitly defers/excludes nested co-own. The balancing rc_dec lives in a SEPARATE raw-handle recursive routine (__vdrop_arr/__drop_value) whose cert is EMPTY, so no single-function fold can witness the balance.

The real Brick 3 needs (all NEW, multi-week):
1. A typed nested-element MIR model — promote load64-fed element handles from untracked raw i64 to a tracked-but-NESTED class (an Op carrying an `ElementOf(container)` relation), so rc_inc on element e is recorded against the container's child-account, not a ghost rc[o]=0.
2. A cert section pairing a producer's accumulated child-incs with the recursive-drop's child-decs by CONTAINER IDENTITY + element count (a GLOBAL property across the producer loop and the separate drop loop, not per-iteration).
3. A Coq development `CoownLoop.v` (well-founded induction on container shape — List[List[String]] is 2-level, leaf count ≠ intermediate count) + a RuntimeModel block model (container length fields + rt_recursive_drop emitting exactly N child decs) + the composition lemma SUM(co-own incs) == SUM(recursive-drop decs) + thread a new cert section through Extract.v + driver.ml.

Indivisible: cannot cert-prove ONE producer (reduce_value's __copy_value) without the shared nested-element model + composition lemma. Schedule as a dedicated fresh formal brick.

## Containment-hardening (the tractable near-term, 2026-06-24)
The trust floor is INTACT: load64-fed rc is an unmodeled no-op GATED by the rc_inc/rc_dec whitelist (calls_p4.rs — only named trusted producers/consumers may name rc_inc/rc_dec; any other .almd fn calling them REJECTS at lowering) + leak-loop fixtures (value_array, value_as_array, list_set_value, list_sort_str, value_merge, + value_object added here). Optional further hardening (deferred, low value): registry-ify the manual whitelist (GOTCHA 3 — it grows by hand-edited match arm).

## Brick 3 integration — the precise fresh-engineering spec (consumes CoownLoop.v, 2026-06-25)

The balance is CROSS-FUNCTION (producer `__varr_copy` fills N child-incs; the balancing N child-decs live in the SEPARATE `__drop_value`), so NO per-function checker change works — modeling child rc inside `__varr_copy` alone would FALSE-REJECT it (the decs are elsewhere). CoownLoop.v proves the PRINCIPLE; the integration must pair the two sides by container. Design:

1. **Typed nested-element model (MIR/lib.rs)** — a load `prim.load64(container + 12 + i*8)` from a TRACKED container at an element offset yields an `ElementOf(container)` handle (not a ghost rc[o]=0 on an arbitrary address). This is the provenance the carrier-rc brick lacked for load64.
2. **Co-own-producer recognition** — a fn that, in a `0..len(container)` loop, rc_inc's each `ElementOf(dst)` and stores into `dst` emits ONE abstract `ContainerFill dst` event (N child-incs, N = len). The whitelist (calls_p4.rs) becomes the recognized set.
3. **Recursive-drop recognition** — `__drop_value`/`__vdrop_arr`/`__vdrop_obj` that rc_dec each `ElementOf(v)` in a `0..len(v)` loop emits `ContainerDrop v` (N child-decs).
4. **Cert section + pairing** — at the CALLER, a container (value.array/object/copy result) is alloc'd (`i`, block) + filled (`ContainerFill`) and later dropped (`DropValue` = block `d` + `ContainerDrop`). The proven checker pairs `ContainerFill c` with `ContainerDrop c` by container identity; CoownLoop.v gives the per-element balance for any N.
5. **Composition theorem (Coq)** — a container that is BLOCK-balanced (OwnershipChecker.v i/d) AND CHILD-balanced (CoownLoop.v fill/drop) is fully leak/double-free-free. This is the one new lemma combining the two models; everything else is Rust (the ElementOf provenance + the two recognizers + the cert section). With it, __copy_value/__varr_copy/value.merge are cert-PROVEN and the rc whitelist (calls_p4.rs) is retired.

Until then: the whitelist + leak-floor fixtures are the trusted gate, now GROUNDED in CoownLoop.v (calls_p4.rs comment) — a name belongs there iff it follows the proven co-own/recursive-drop pattern.

## Brick 3 integration — Coq half DONE; Rust half confirmed MULTI-WEEK (2026-06-25)

The COMPOSITION half is landed + kernel-checked: proofs/CoownCompose.v `lifecycle_safe` (block-balanced ∧ children-source-owned ⇒ fully safe), composing OwnershipChecker.v (block) with CoownLoop.v (child). In the proof gate.

The ELEMENTOF (Rust cert-section) half was scoped with adversarial verification, which REFUTED the hoped-for small slice:
- The "strict refinement rides the existing block i/d pairing (no corpus regression)" claim is FALSE: a co-own producer's fill (`__varr_copy`) and its balancing recursive drop (`__vdrop_arr`/`__drop_value`) are in DIFFERENT functions, so the child account is genuinely cross-function — no per-function cert refinement catches it.
- It does NOT catch reduce_value's str-a/arr-a leak: that leak is an UNDER-RELEASED load64-fed CHILD (the recursive drop IS present and runs; the imbalance is a cross-function, element-count-keyed count that is load64-fed = invisible to both the block account and the carrier-rc brick). Catching it needs the full load64→ElementOf(container,index) provenance + ContainerFill↔ContainerDrop pairing across the two functions — confirmed MULTI-WEEK fresh engineering, tracked distinctly.

TRACTABLE TODAY (landed): the rc whitelist — the actual trust anchor — is promoted to ONE shared source (crates/almide-mir/src/coown_names.rs: COOWN_PRODUCERS / COOWN_DROPS / COOWN_SET_REPLACE), grounded in CoownLoop.v + CoownCompose.v, consumed by calls_p4.rs, and pinned by the `coown_names_documented` test so the producer/consumer halves cannot silently drift. corpus-wall ACCEPT (behavior-preserving), mir 503/0.
