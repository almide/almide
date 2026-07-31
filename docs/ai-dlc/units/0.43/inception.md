# Unit 0.43 — Plan: the concurrency stance, and the fan family it settles

- **Aim**: 0.4x arc — the ladder's own ordering rule says decision documents are the most
  upstream work ("concurrency の立場は cross-target 契約・cranelift 実行モデル・critical
  profile・仕様凍結すべての上流"). Everything downstream of it was being built on an
  implicit assumption, and four filed reports showed the assumption was not even
  self-consistent.
- **Issues**: [#1000](https://github.com/almide/almide/issues/1000),
  [#1023](https://github.com/almide/almide/issues/1023),
  [#1024](https://github.com/almide/almide/issues/1024),
  [#1025](https://github.com/almide/almide/issues/1025),
  [#1026](https://github.com/almide/almide/issues/1026)

## In three lines

Almide had no stated concurrency stance, and SPEC.md contained two promises about `fan`
that cannot both hold — "results are identical either way" and "first to complete wins".
Four reports independently found `fan` not doing what the prose said.
Done means the stance is written, and each of the four is resolved FROM it rather than
patched one at a time.

## Background

Verified against the shipped 0.41.0 compiler, not taken from the issue text:

- `docs/SPEC.md` §13.1 promises timing-independence; §13.2 promises wall-clock racing.
- `docs/contracts/contracts.toml` C-004 already documented list-order determinism, so the
  contract ledger and the prose contradicted each other and the implementation followed
  the ledger.
- wasm32 has no threads, so a timing-dependent construct is not implementable there. This
  is the same reason `fan.timeout` was removed in 0.29.0.
- The mission metric is modification survival rate. A program whose output changes between
  runs breaks the measurement, so determinism here is part of the definition of correct.

## Scope

- S1 Decide the stance and write it as a roadmap page (#1000's stated deliverable).
- S2 Derive each of the four reports' correct behaviour from the stance, rather than
  choosing per-issue.
- S3 Land SPEC.md, the contract ledger, and the implementation together for each.

## Out of scope

- Per-arm output buffering to retire C-004's interleaving EXCEPTION clause. Related, but a
  separate question from any of the four reports — and the trap measurement showed it is
  not needed for #1026.
- Structured concurrency itself. The stance records the condition for revisiting it.

## Done-criteria

- The stance exists as a roadmap page and the ladder row points at it.
- All four issues closed, each with the reasoning that closed it, and each traceable to the
  stance rather than to a local judgement call.
- Every behaviour change carries a contract entry and a fixture in the same commit.
- Contract gate green with flagged = 0.

## Risks

- **R1 — deciding by taste instead of by constraint.** Absorption: the write-up must name
  which EXISTING promise is being kept and which is being dropped, so the decision is
  auditable rather than preferential.
- **R2 — a removal cascading into surfaces nobody checked.** Absorption: after any removal,
  re-run the full spec suite AND the contract gate, and treat a broken migration target as
  a finding in its own right.
- **R3 — implementing from a design note instead of a measurement.** Absorption: measure
  the current behaviour of each reported case BEFORE writing the fix.

## Proposed Bolts

- **B1** — Decide the stance; write the roadmap page; correct SPEC.md's false claims.
- **B2** — #1025: the aliasing soundness hole (stance-independent, so it goes first).
- **B3** — #1024: `fan.race`.
- **B4** — #1023: cancellation.
- **B5** — #1026: the trap exit.
