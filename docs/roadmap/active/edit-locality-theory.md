# edit-locality theory — the theorem Almide exists to prove

**Status**: active (constitution written 2026-08-15; Stage 1 landed as
`docs/specs/edit-locality.md`).
**Mission link**: MSR (modification survival rate) is the empirical shadow of
one semantic property — that the observable blast radius of a source edit is
bounded by the executions that pass through the edited code. This document is
the plan to turn that property from a design instinct into a proven theorem
with a live experimental loop.

## The claim

> **Edit Frame (L1).** A signature-preserving edit to a definition cannot
> change the observables (stdout, stderr, exit code — the set fixed by
> `docs/contracts/contracts.toml`) of any execution that does not pass
> through the edited definition.

Almost no implemented language satisfies this. Overload resolution, implicit
instances, macros, whole-program inference, and dynamically scoped effect
handlers each break it: they let a distant edit change the meaning of
unedited code. Almide's existing discipline — `effect` in the signature,
explicit propagation only (ADR-0008, E041/E042), qualified cross-module
access — is precisely the set of preconditions L1 needs. The rules were
adopted one-by-one for MSR reasons; L1 is the single statement they were all
serving.

## Why this is the anti-Koka position, not the diet-Koka position

Koka's crown jewel — dynamically scoped algebraic effect handlers — is a
frame-violation machine by construction: the meaning of `perform op` depends
on which handler is installed somewhere in the calling context, possibly in
another file. Installing or reordering a handler is a distant edit that
changes the behavior of unedited code. **Koka cannot prove L1 without
amputating handlers.** So the two languages sit on opposite poles of one
axis: Koka maximizes effect expressiveness, Almide maximizes edit locality.
Any future feature proposal gets one gate question: *does it preserve L1?*
(Handlers, implicit instances, and glob re-exports all fail the gate; that is
the gate working.)

## Stages

1. **State + hunt** (DONE 2026-08-15): `docs/specs/edit-locality.md` states
   L1–L3 over the current language, maps each existing rule to the locality
   role it plays, and records the counterexample hunt over the implemented
   compiler. Every found violation becomes either a language fix or an
   explicit side condition — never an unwritten exception. Day-one yield:
   the hunt reproduced a **live cross-target divergence** on almide 0.57.0
   (V1 — LICM speculatively hoists body-inferred-"pure" partial ops out of
   zero-trip loops; native traps, wasm prints) that 421 `wasm_cross`
   fixtures and the differential fuzz had not caught. The theorem found a
   real contract violation before a single line of proof was written.
2. **Kernel calculus, mechanized** (DONE 2026-08-15): λ_almd lives in
   `crates/almide-edit-belt` — the third 0-sorry Lean belt, kernel-checked
   by the CI `lean-proofs` job alongside perceus and race. The calculus:
   top-level definitions with DECLARED signatures (`effect` flag included),
   `let` / calls / `Result` construction and `match` / explicit `!`
   (ADR-0008, as a `norm`/`abrupt` outcome split) / `??` / `print` (the
   observable), big-step semantics instrumented with the trace and a ledger
   of every definition entered. Theorems, all sorry-free:
   - `ev_agree` — evaluation depends only on the definitions it actually
     entered. The whole of L1 in one lemma; provable in a page because the
     call rule reads the table at the called name and nowhere else.
   - `edit_frame` / `edit_frame_observables` — L1 transport + (with
     `ev_det`, determinism) the observables-identical form. A finding worth
     recording: the UNTYPED frame does not need signature preservation at
     all — replacing `f` by anything preserves executions that avoid `f`.
     Signature preservation is purely the TYPED half's hypothesis:
   - `typing_modular` — same-signature body swap keeps the program
     well-typed, with every other definition's derivation literally the
     same proof object (typing consults signatures only).
   - `pure_silent` — code typed at `eff = false` evaluates with an empty
     trace: the semantic content of "a pure `fn` cannot produce
     observables" (E006's fence, machine-checked).
   Honest scope: unary calls, `Result`-only data, one effect; must-use
   (E041/E042) stays an implementation-level diagnostic — the kernel models
   ADR-0008's runtime meaning, not its lint surface.
3. **Backends as refinements** (DONE at fragment scope, 2026-08-15): the
   kernel is an EXECUTABLE specification — `Evaluator.lean` adds a
   fuel-indexed `evalE` with `eval_sound` (whatever it returns, the
   relation derives; with `ev_det`, uniquely — completeness deliberately
   unproven, only `some`-outputs are consumed). Enforcement is
   generator-total, not hand-picked: `Corpus.lean` deterministically
   generates a 48-program typed corpus over the λ_almd-expressible
   fragment, proves every program's kernel observables at Lean compile
   time (`#guard corpusOK`), and `conformancegen` emits/pins the committed
   corpus (`proofs/kernel-conformance/`, CI drift-gated);
   `tests/kernel_conformance_test.rs` runs all of it on native AND wasm
   against those expected traces — 48/48 on both targets at landing, plus
   the hand-written seven-program family under contract C-280. The trust
   seam is the ~80-line `erase`/`render` pair, reviewed once; the
   three-layer standing lives in `docs/contracts/proven-vs-trusted.md`.
   Day-one yield: the corpus caught two real compiler bugs on its first
   run — almide#1428 (checker accepts a bare `err(..)` match subject,
   codegen emits invalid Rust) and almide#1429 (the v1 renderer splits an
   effect fn's signature from its body on a bare-parameter tail). What
   remains is research-grade, out of reach without a prover-hosted
   compiler: a verified surface-core→λ_almd translation and per-pass
   simulation proofs over the Rust implementation. The fragment gate is
   the ratchet until then; the R-group obligations in
   `docs/specs/edit-locality.md` §3 stay on their own ledger.
4. **Prediction loop** (MACHINERY LIVE 2026-08-15; closes as post-landing
   runs accumulate): `proofs/l1-verdicts.toml` is the lab book — every
   semantics-touching change records its L1 verdict AND a committed MSR
   prediction (up/neutral/down with the causal mechanism) BEFORE any
   measurement; `scripts/check-l1-verdicts.sh` gates the schema in CI.
   Dojo's side is `src/l1_loop.almd` (written in Almide, per the dogfood
   rule — and dogfooding it immediately caught the `regex.captures`
   doc/implementation drift, almide#1432): it joins the ledger against
   `runs/*/summary.md` headline MSR numbers and reports each prediction's
   standing — confirmed / refuted / inconclusive / awaiting — into
   `dashboards/l1-loop.md`. A refuted prediction stays in the ledger as a
   finding about the theory. Seeded with LV-001..005 (the arc's five
   landings, all predicted `up`); all await post-landing runs — the last
   scored run predates the arc, so the loop's first real datapoint arrives
   with the next Dojo round. Theory that predicts measurements is the layer
   no other language team has — Koka has proofs without an experiment,
   Dojo-less languages have experiments without a theorem.

## Non-goals

- Competing with Koka on effect expressiveness. That race is 14 years lost
  and winning it would cost L1.
- Proving anything about memory or timing. The observable set is stdout /
  stderr / exit code, exactly as the contract ledger already fixes it.
