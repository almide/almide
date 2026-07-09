<!-- description: GOAL PROMPT — cert format brick 6: retire extraction trust (kernel-as-oracle gate + verified extraction) -->
# GOAL PROMPT — cert format brick 6: retire the extraction trust (kernel oracle + verified extraction)

> **Read first**: [certificate-format-v1](certificate-format-v1.md) (the ladder —
> bricks 1–5 and 3c ALL shipped; this is the LAST brick),
> `proofs/TRUSTED_BASE.md` §"trusted base" item 2 (the exact claim this brick
> retires), `proofs/Extract.v` + `proofs/driver.ml` + `proofs/build-checker.sh`
> (the pipeline being de-trusted), `proofs/check.sh` (where the new gate wires
> in), `proofs/WasmIsa.v`'s header (the PRECEDENT: when an ecosystem library
> targets old Coq, bring the ARCHITECTURE in-tree instead of the import).

## Context — where the ladder stands (2026-07-09, commit `9ac1d4d2`)

- Bricks SHIPPED: 1 (alphabet), 2a/2b/2c (calls + modes + manifest caps),
  3a/3b/3c (op→wasm table, runtime memory model, raw-byte ⟶ ISA decoder
  `WasmDecode.v`), 4a/4b (perceus + reuse + FreeList + CowSafety + real-RC
  renderer), 5a/5b/5c (branch agreement, `b`, closure signatures + capturing
  closures). Spine: 22+ proof files, all axiom-clean, coqchk'd, claim-drift
  gated; corpus 4,745 in-profile fns / 27k+ heap objects re-verified per build.
- **The one remaining trust hole** (TRUSTED_BASE.md item 2, verbatim): "OCaml
  extraction + the OCaml compiler. The proven `check_cert` is extracted to
  OCaml and compiled by `ocamlopt`. This is the Thompson hole; CertiCoq +
  CompCert close it — brick 6, not yet done." Today `accept ⟹ P` is proven of
  the GALLINA checker; the binary that actually runs in `gate.sh`/
  `corpus-wall.sh` is produced by UNVERIFIED extraction + ocamlopt.

## The goal (one line)

> **Make the per-build PCC gate's verdict rest on the KERNEL, not on the
> extraction pipeline — and, where the Rocq-9 ecosystem allows, replace the
> unverified extraction itself with a PROVEN one — so TRUSTED_BASE.md item 2
> shrinks from "extraction + ocamlopt are trusted" to a recorded residual no
> bigger than the kernel we already trust.**

## Non-negotiable invariants (breaking any = the brick failed, revert)

1. **No weakening of the fast path**: the extracted `./checker` binary stays —
   it is the ergonomic per-witness tool (`gate.sh` rows, corpus one-pass). The
   brick ADDS a stronger verdict source; it never replaces green rows with
   slower-but-equal ones silently. Gate output must say which oracle ran.
2. **Axiom purity + claim-drift**: any new Coq surface is axiom-clean,
   coqchk'd, ledgered; the claim-drift gate keeps rows ⊆ kernel-checked.
3. **No new UNPROVEN trust**: adopting a plugin (verified extraction /
   CertiCoq) must not smuggle in axioms — run `Print Assumptions` on the
   extracted artifact's correctness theorem and record what the plugin itself
   assumes (its own trusted base) in TRUSTED_BASE.md. If a route adds MORE
   trust than it removes, reject it.
4. **Determinism / CI parity**: everything must run on the local Rocq 9.1.1
   source build AND the CI opam Rocq 9.x (trust-spine.yml). A route that only
   works locally is not shipped.
5. **Non-vacuous demos**: the new oracle must be shown to CATCH a wrong
   verdict — a tamper drill (see 6b), at the byte level, in the gate.
6. Commit style: English, one line, no prefix. Push only at all-gates-green.

## Sub-brick 6a — SCOUT the ecosystem routes (do this first, timebox it)

Record findings in this file before implementing anything:

- **Route A — verified extraction to OCaml** (`rocq-verified-extraction` /
  MetaRocq erasure, the PLDI'24 "Verified Extraction from Coq to OCaml" line):
  `opam search verified-extraction metarocq metacoq` on the CI switch; try
  installing against Rocq 9.x. If it installs: extract `check_bc` /
  `check_names_cert` / `check_caps_cert` / `check_prog_cert` /
  `check_modes_cert` through it, diff the produced OCaml's BEHAVIOR against
  the current extraction on every gate fixture. What it buys: the erasure step
  becomes a THEOREM; ocamlopt + the OCaml runtime remain (record that).
- **Route B — CertiCoq (→ C light / CompCert)**: `opam search certicoq`; it
  historically targets Coq 8.x — if it does not install on Rocq 9 (expected),
  record the exact incompatibility and CLOSE the route with the WasmCert
  precedent ("the import is infeasible; the architecture is what we take").
  Do NOT sink days into forcing it.
- Whatever the outcome, **Route C ships regardless** — it is in-tree, zero
  dependencies, and kernel-grade.

## Sub-brick 6b — the KERNEL-AS-ORACLE gate (Route C, the core of the brick)

**The idea**: the strongest possible verdict on a per-build witness is the
KERNEL ITSELF evaluating the proven checker on the exact witness bytes. We
already compile the proofs per build (`check.sh`); generating and `coqc`-ing a
tiny assertion file makes the extraction pipeline irrelevant to the GATE's
trust story:

```
(* generated per build by gate.sh — witness bytes inlined verbatim *)
From AlmideTrust Require Import OwnershipChecker CallModes ...
Definition w_ownership : string := "<witness bytes>"%string.
Goal check_bc w_ownership = true.  Proof. vm_compute. reflexivity. Qed.
Goal check_bc "<a REJECT witness>" = false.  Proof. vm_compute. reflexivity. Qed.
```

- **Generator**: a `gate.sh` helper `kernel_verify <mode> <witness-file>
  <expected>` that escapes the bytes into a Coq string literal (the witness
  alphabet is ASCII + newline; double any `"`), instantiates the right checker
  (`check_bc` / `check_names_cert` / `check_caps_cert` / `check_prog_cert` /
  `check_modes_cert` — same dispatch as driver.ml), writes
  `KernelGate_<n>.v`, and `coqc`s it. A failing assertion = a failing build.
- **Coverage**: EVERY gate row (hand-built + real-source + manifest rows) gets
  a kernel-oracle twin — the row's verdict is then certified by the kernel,
  with the extracted binary as the cross-check (two independent executions
  must agree; disagreement fails loudly = the extraction bug detector).
- **Corpus scale** (27k-object ownership cert, ~4.7k per-function witnesses):
  measure `vm_compute` on the full `ownership.cert` string first — if it
  completes in CI-acceptable time, wire `corpus-wall.sh` the same way (one
  assertion over the whole file; the names/caps per-function loops can batch
  N-per-file). If it does NOT scale, the honest scope is: kernel oracle on
  the FULL gate fixture set + a documented corpus sample, extracted binary on
  the full corpus — RECORD the boundary in TRUSTED_BASE.md (do not silently
  sample).
- **Tamper drill (the non-vacuity demo)**: a gate row that (i) corrupts one
  byte of a witness and shows the kernel oracle REJECTS what the binary also
  rejects (agreement on reject), and (ii) feeds the binary and the kernel
  DIFFERENT bytes to prove the agreement check actually fires (a simulated
  divergent-extraction, caught). The drill runs in the gate, every build.
- **TRUSTED_BASE.md rewrite** (same PR): item 2 becomes "the extracted binary
  is a FAST PATH cross-checked against the kernel oracle on <scope>; the
  extraction pipeline is no longer a trust root for the gate verdict; residual:
  ocamlopt for the *convenience* binary, the kernel (item 1) for the verdict."
  The known-limitations line "Extraction is trusted (item 2 above) until
  CertiCoq/CompCert" is updated to the shipped state.

## Sub-brick 6c — adopt Route A if (and only if) the scout says yes

If verified extraction installs cleanly on BOTH toolchains: switch
`build-checker.sh` to it, keep the byte-demos identical (all 25+ must be
verdict-identical), record the plugin's own trusted base honestly, and ledger
the correctness theorem's `Print Assumptions`. The kernel oracle from 6b STAYS
— defense in depth, and it covers driver.ml's I/O glue which no extraction
route proves. If the scout says no: record why (versions, exact error) and
ship 6b alone — that already retires the trust claim.

## Verification ladder (run in this order, stop on first red)

```
cd proofs && ./check.sh          # spine + axiom audit + claim-drift (+ new gens compile)
./build-checker.sh               # extraction + byte demos (verdict-identical)
./gate.sh                        # every row + its kernel-oracle twin + tamper drill
cargo test -q -p almide-mir
./corpus-wall.sh                 # + kernel oracle at whatever scope 6b measured
cargo test -q                    # workspace zero-fail
```

## Exit criteria (all must hold)

- [ ] Every gate.sh row's verdict is certified by the KERNEL (generated
      assertion, coqc green) with binary/kernel agreement enforced.
- [ ] Tamper drill in the gate: a corrupted witness and a simulated divergence
      are both CAUGHT, every build.
- [ ] Corpus-scale decision made by MEASUREMENT and recorded (full kernel
      verification, or the honest documented boundary).
- [ ] Route A / B scout results recorded here; Route A adopted iff it installs
      on both toolchains with no new unproven axioms (ledgered if adopted).
- [ ] TRUSTED_BASE.md item 2 + known-limitations rewritten to the shipped
      state; certificate-format-v1.md brick 6 marked SHIPPED with honest scope.
- [ ] Trust Spine workflow green on the push (the CI opam Rocq must run the
      kernel-oracle gate — watch its runtime; cache stays warm).

## What NOT to do

- Do not force CertiCoq/CompCert onto Rocq 9 — the WasmCert precedent applies:
  take the architecture, not the import.
- Do not let the kernel oracle SLOW the developer loop unbounded — measure,
  then decide scope; `native_compute` is an allowed fallback for speed but
  RECORD that it reintroduces ocamlopt into that one path (vm_compute does
  not).
- Do not delete or bypass the extracted binary path; do not rewrite driver.ml
  in the same PR (scope creep).
- Do not start heap-capture closures / A2-beyond-rc here — separate ratchets.
