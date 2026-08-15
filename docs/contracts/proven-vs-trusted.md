# Proven vs trusted — the boundary map

The first question an auditor asks about a compiler that ships proofs is *which
part is proven*. This page answers it, and points at the evidence for each
answer.

The full ledger — toolchain pins, the irreducible trust roots, the tamper drill,
the per-bug-class regression pins — is [`proofs/TRUSTED_BASE.md`](../../proofs/TRUSTED_BASE.md).
This page is the map; that page is the territory.

## The one-sentence version

An **accepted certificate proves the function is memory-safe** — no double-free,
no leak, no dangling name, no undeclared capability. It proves **nothing about
whether the lowering picked the right semantics**: a certified-sound function
can still print the wrong string.

## Where the boundary runs

| Stage | Status | What backs it |
|---|---|---|
| Lexer, parser | trusted | differential fuzz, the spec corpus |
| Type checker kernel | **proven** | `proofs/OwnershipChecker.v`, `TypeConcretization.v`, `NameTotality.v` |
| Coown patterns | **proven** | `proofs/CoownCompose.v`, `CoownLoop.v`, `CowSafety.v` |
| AST → IR lowering | trusted | the checker's `TypeMap` is the source of truth; 974-file emit baselines |
| IR → MIR lowering | **trusted** | ← *this is the gap F3 (#777) is about* |
| MIR ownership witness | **proven to be re-checkable** | `proofs/gate.sh`: the untrusted producer emits a witness, the kernel-proven checker re-verifies it |
| MIR → wasm bytes | trusted | `proofs/check-wasm-bytes.sh`, `WasmEncode.v` for the `rc_inc`/`rc_dec` byte trees |
| wasmtime | unqualified tool | out of scope by construction |

## Why the IR → MIR row is the one that matters

Every one of the five output-breaking bugs found in the 2026-07-03 audit lived
in that row, and all five were *accepted by the certificate* — the MIR was
RC-balanced and name-resolvable, it simply computed something else.

They share a shape, quoted from the trusted-base ledger:

> a REGISTRY or CONVENTION (tracking sets, layout tables, calling convention,
> the scalar deferred-Const fallback) drifted between producer and consumer
> inside the trusted zone.

A certificate cannot catch that, because drift produces *valid* MIR. What
catches it is a post-pass over the lowering's own output, checking invariants
the lowering is supposed to maintain — the same shape as
`assert_names_resolvable` and the `ConcretizeTypes` gate. That gate exists now
(`crates/almide-mir/src/mir_wellformed.rs`, #777 item 2): every function the
lowering emits is checked for def-before-use over its op stream, the
defines/reads split is asserted to partition `op_values` on every real op (so
the three occurrence functions cannot drift), and a violation surfaces as a
named wall rather than rendering. It runs inside `lower_function_all`, so every
CI leg that lowers — Test WASM, Cross-Target, Trust Spine — exercises it on
every build.

The per-class regression pins for all five are in
[`proofs/TRUSTED_BASE.md`](../../proofs/TRUSTED_BASE.md#the-five-2026-07-03-trusted-zone-bug-classes-and-their-regression-pins).

## The edit-locality kernel seam (Stage 3)

The λ_almd kernel (`crates/almide-edit-belt`, the third 0-sorry Lean belt)
gives the language a machine-checked reference semantics for its core:
`ev_agree`/`edit_frame`/`ev_det` (L1 with determinism), `typing_modular`,
`pure_silent`, and `eval_sound` (the executable evaluator agrees with the
relation). The bridge to the shipping compiler runs through three layers
with different standings:

- **kernel-checked (proven)** — the hand-written family's observables:
  `k1_obs`…`kAll_obs` and `corpus_total` are theorems proved `:= by rfl`,
  i.e. by Lean KERNEL reduction, and their axiom sets are pinned with
  `#guard_msgs in #print axioms` (`propext` only). `eval_sound` +
  `ev_det` lift each pinned `evalE` output to THE derivation of `Ev`
  (`kAll_ev`). For the generated corpus, what is kernel-checked is
  TOTALITY — every one of the 48 programs evaluates to `some`
  observables (`corpus_total`).
- **evaluator-pinned (trusted)** — the VALUES in
  `proofs/kernel-conformance/*.expected`: they are emitted by the
  COMPILED Lean evaluator (`lake exe conformancegen --write`), so
  trusting them means trusting the Lean compiler — the exact seam lean4
  itself names with its `trustCompiler` axiom. The unproven completeness
  direction of `evalE` is likewise an enumerable object, not a doc
  comment: `EvalCompleteness` states it, the marker axiom
  `trustEvalCompleteness` (kept at `True`) tags any argument that needs
  it, and the CI axiom ratchet enumerates every `axiom` in the belts.
- **trusted (reviewed seam)** — the `eraseE`/`eraseChain` + `render*` pair
  in `Corpus.lean`: two ~40-line functions walking the same surface
  grammar, one producing the λ_almd term the evaluator scores, one the
  almide text the compiler eats. Reviewing that they agree
  constructor-by-constructor IS the trust obligation — reviewed once, not
  per program. (The hand-written family adds the same seam by hand:
  `spec/wasm_cross/kernel_conformance.almd` ↔ `kAll`, Rust literal ↔ Lean
  literal.)
- **gated** — that both backends produce those observables:
  `tests/kernel_conformance_test.rs` runs the whole corpus on native and
  (with a wasm runtime present) on wasm; the `wasm_cross` harness carries
  the family; the `conformancegen --check` CI step pins the committed
  corpus to `Corpus.lean` byte-for-byte. All under contract C-280.

Lean's own three-tier vocabulary, which the classification above adopts
(the correction came from reading lean4's source — Survey 4):

| Lean idiom | Trust level | Why |
|---|---|---|
| `decide` / `:= by rfl` | kernel proof | the kernel re-reduces the term itself |
| `#guard` | untrusted pin, **not a proof** | runs the untrusted elaborator evaluator (lean4 `src/Init/Guard.lean`) |
| `native_decide` | compiled-evaluator proof **modulo a named axiom** | materializes `Lean.ofReduceBool`/`trustCompiler` so `#print axioms` shows the seam |

The belts use tier 1 for every conformance claim; the `.expected` files
sit at tier 3's trust level (compiled evaluation) with the seam recorded
here instead of in an axiom, because the consumer is a shell gate, not a
Lean proof.

"Backends are refinements of the kernel" is therefore enforced over the
GENERATED λ_almd-expressible fragment — machine-computed programs,
machine-proven expected values, machine-diffed backends — with one
reviewed ~80-line seam. What remains out of reach without rewriting the
compiler in a prover: a verified surface-core→λ_almd translation and
per-pass simulation proofs over the Rust implementation itself. Those are
recorded as the research-grade residual in
`docs/roadmap/active/edit-locality-theory.md`, and the fragment gate is
the ratchet that holds until then. Day one of running it, the corpus
caught two real compiler bugs (almide#1428: checker-accepted program dies
in codegen; almide#1429: the v1 renderer splits an effect fn's signature
from its body on a bare-parameter tail) — the gate bites.

## What each gate actually claims

| Gate | Claim | NOT a claim |
|---|---|---|
| `proofs/gate.sh` | the witnessed MIR is RC-safe, name-total, capability-bounded | that the wasm bytes match the witness |
| `proofs/corpus-wall.sh` | `lower_function` is total over the corpus: every function is `Ok` or an explicit `Unsupported` | that an `Ok` function has correct output |
| `proofs/output-parity.sh` | native and wasm agree, for the baseline set | anything outside that set |
| `scripts/check-contracts.sh` | every observable cross-target promise has executable evidence | that the promise is the right one |
| the cross-target fuzz | no divergence found in N programs | no divergence exists |
| the kernel-conformance pin (C-280) | the compiled family's observables equal the kernel-checked λ_almd trace, on both targets; the generated corpus matches the compiled evaluator's traces | that every almide program refines the kernel — only the fragment's image is pinned; corpus `.expected` values are evaluator-pinned (compiled Lean), not kernel-checked |

Reproduce all of it: `make verify-trust`.
