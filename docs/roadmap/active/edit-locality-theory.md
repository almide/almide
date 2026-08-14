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
3. **Backends as refinements**: position native and wasm codegen as
   refinements of the kernel semantics. The 280-contract ledger is then no
   longer the *definition* of cross-target agreement but its *test shadow* —
   contracts become corollaries, the ledger stays as the empirical ratchet.
4. **Prediction loop**: for each proposed language change, derive the L1
   verdict (preserved / needs side condition / violated) and a predicted MSR
   direction; Dojo measures the actual delta. Theory that predicts
   measurements is the layer no other language team has — Koka has proofs
   without an experiment, Dojo-less languages have experiments without a
   theorem.

## Non-goals

- Competing with Koka on effect expressiveness. That race is 14 years lost
  and winning it would cost L1.
- Proving anything about memory or timing. The observable set is stdout /
  stderr / exit code, exactly as the contract ledger already fixes it.
