# ADR-0014: Pinned expectations are machine-derived or absent — never hand-maintained

- **Status**: Accepted
- **Date**: 2026-08-20
- **Context**: `spec/wasm_cross` asserts cross-target AGREEMENT and commits no
  expected outputs; the diagnostics corpus pins expectations in `meta.toml`.
  The reference survey ranks Roc's two-part fixture form (human-authored
  head, machine-derived expectation tail, mutually-exclusive check/bless)
  among the strongest corpus designs, and QUALIFICATION.md declares this
  repository's co-drift blind spot: agreement cannot catch both targets
  being wrong identically. The 2026-08-20 greenfield cutover also caught a
  neighbouring session hand-writing golden rows with placeholder hashes —
  the exact failure this decision forbids.
- **Decision**: When an expectation is pinned in this repository, it is
  DERIVED by an instrument from structured data (today: the diagnostics
  `meta.toml` fields asserted by the runner; the λ_almd kernel-conformance
  traces emitted by the checked evaluator) — a hand-written expected-output
  file is forbidden. The Roc-form migration of `spec/wasm_cross` (committed
  expected traces per fixture) happens ONLY as the output format of the
  reference evaluator when it lands (the declared instrument for closing
  limitation #2, grown from the λ_almd belt), and not before.
- **Rationale**: An expectation file's value is exactly its provenance. Roc's
  own load-bearing detail is that EXPECTED is generated from report data,
  never re-parsed or hand-edited; adopting the file format without the
  deriving instrument would add 591 hand-maintained files — the drift
  surface the survey's negative controls (Koka, MoonBit) document.
- **Alternatives**: (a) migrate now with hand-seeded expectations — rejected
  as above. (b) never pin traces, agreement forever — rejected: leaves the
  co-drift blind spot open permanently.
- **Consequences**: Item "two-part fixture form" is folded into the
  reference-evaluator work, not tracked separately. Until then, agreement +
  the implementations' third leg carry cross-target truth, and that
  limitation stays DECLARED in QUALIFICATION.md.
- **Falsifier**: A co-drift bug reaching a release would prove agreement +
  declared limitation insufficient and pull the evaluator forward.
- **References**: ../almide-references survey (B2, item 2), QUALIFICATION.md
  limitation 2, C-280 (λ_almd kernel conformance).
