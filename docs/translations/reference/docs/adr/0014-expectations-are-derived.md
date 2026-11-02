# ADR-0014: Pinned expectations are machine-derived or absent — never hand-maintained

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](0014-expectations-are-derived.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0014-expectations-are-derived.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0014-expectations-are-derived.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0014-expectations-are-derived.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


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
