# v1 heap-loop-carried ownership — option C (cert-spine extension), the COMPLETENESS fix

**CEO chose C ("C一択", 2026-06-20): close the proof spine's completeness gap at the ROOT — teach the
proven checker to reason about a loop-carried heap accumulator — rather than route around it (A: in-place
push) or hack the rep (B). C lets the user write the NATURAL recursive `acc + [x]` and have it PROVEN.**

## The gap (why the 11 remaining yaml walls need this)

The base ownership cert (OwnershipChecker.v) is a FLAT per-object event stream — no loop notion. A
loop-carried heap accumulator (`acc = acc + [x]` per iteration: drop old object, alloc new, rebind the
slot) is unrepresentable: an object's `i` is in iteration K, its `d` in K+1 — different objects sharing
one SLOT. `verify_ownership` (flat, one pass) sees an unbalanced `d`/`i` and FALSE-REJECTS safe code.
This is a **completeness** hole (soundness was never at risk). The 11 walls (collect_*/parse_*/seq_item/
map_entry, flow_rec↔flow_step, block_*) all hinge on it (append accumulator + mutual recursion).

## ✅ LANDED 2026-06-20 (commit 7f673b4c): the SOUNDNESS PROOF — the ②-critical core

`proofs/OwnershipLoop.v` (in the proof gate: `_CoqProject` + `check.sh` coqc **and** coqchk + claim-drift;
"PROOF SPINE OK", axiom-clean "Closed under the global context"). It adds a `Loop : list FlatOp -> Op`
construct and PROVES:
- `exec_list` (the checker fold) Loop rule: accept a loop iff its body PRESERVES rc (and doesn't fault)
  from the entry count.
- `Unrolls` : the abstract cert unrolls to a concrete flat run (each `Loop body` → n copies of body).
- **`check_unroll_sound`**: `check ops = true → ∀ unrolling, no_double_free ∧ no_leak`. I.e., a rc-preserving
  loop body is leak/double-free-free for ANY iteration count (induction via `exec_flat_repeat_preserve`).
The accumulator slot cert is `[Inc; Loop [FDec; FInc]; MoveOut]` (acquire once; each iter release-old +
acquire-new = net 0; move out the final). Loop bodies are FLAT (no nested loop) — sufficient for the v1
parser walls (one drop+alloc per iteration); nested loops are a future compose-able extension.
The hard, irreducible part of C (the Coq re-proof — "C needs Coq, not corpus-wall-verifiable") is DONE
and kernel-verified. The rest is gate-verifiable engineering.

## Remaining C integration (each gate-verifiable — corpus-wall + byte-match + the proof gate)

1. **Production checker + cert format**: port the `Loop` construct into `OwnershipChecker.v` (the EXTRACTED
   runnable checker) + a cert-format loop delimiter (e.g. `(`/`)` nesting in `parse_go`), re-prove
   `check_sound`/`check_cert_sound` for the Loop arm (reusing OwnershipLoop's `check_unroll_sound`), and
   re-Extract (Extract.v → checker.ml). `proofs/build-checker.sh` round-trips it on a loop cert.
2. **Rust cert emission** (`certificate.rs` / `lib.rs verify_ownership`): track `Op::SetLocal` REBIND of a
   heap loop-carried local (object_of[acc] ← object_of[new]); emit the slot's cert as `Inc … Loop[body] …
   MoveOut` (the body = the per-iteration drop-old + alloc-new + any balanced temps), the OTHER objects
   per-iteration-balanced inside the loop. This is where the flat one-pass becomes loop-aware.
3. **Lowering** (`lower/mod.rs`): emit the heap-loop-carried accumulator MIR — the append-accumulator TCO
   (the functional `acc + [x]` loop with `LoopStart/SetLocal/LoopEnd` markers), now CERT-BACKED so
   `verify_ownership` accepts it. Plus mutual-recursion inlining (flow_step→flow_rec) to expose the loop.
4. **Verify**: corpus-wall ACCEPT (the re-extracted checker accepts the loop certs) + the yaml walls clear
   (flow_rec/collect_*/block_*) + byte-match v0 + leak-loop.

After C lands end-to-end: the 11 walls fall (with value.object/stringify + tuple-heap for the Value-parser
subset), driving yaml → 0 — on a PROVEN spine, the v1 completeness ideal.
