# Translation phase 3 — standard-library and ADR reader snapshots

> Historical batch record: counts and placement descriptions below describe this phase at creation, not the current layout or publication state. The subsequent sibling migration uses `reference/<full-source-path-stem>.md` (English), `.ja.md` (Japanese), and `.zh-CN.md` (Simplified Chinese); `.en.md` and language-directory reader routing are retired. Canonical originals remain unchanged. Table destinations and labels below use the migrated paths; source-language snapshot copies added by migration are not additional translated originals. Current cumulative coverage is tracked in [TRANSLATION-INVENTORY](../project/TRANSLATION-INVENTORY.md).

Pinned originals: `35297a612fb29e3f78f6587aba253aa91d51f466`. This phase retains all phase 2 files unchanged and adds **14 originals / 28 reader translations**. The repository's original documents, generators, compiler code and CI are unchanged. Publication is separate and is not claimed here.

- Standard-library scope: `int`, `float`, `option`, `result`, `list`, `map`, `set`, `string`, `secret`, `base64`, `hex`.
- ADR scope: `0013-corpus-stays-id-keyed`, `0014-expectations-are-derived`, `0030-compile-cost-is-a-contract`. Duplicate ADR number 0013 is distinguished by its full filename; the String record is not covered by this phase.
- All source files in this batch are English. Japanese and Simplified Chinese variants are reader-only snapshots outside the canonical source directories.
- Original fenced code, comments, signatures and since metadata are preserved verbatim; translated API explanations are outside the retained fences. Active generated-index markers are not copied.
- ADR acceptance, measurements and strong safety/competitor claims remain historical source statements, not newly tested results or independent guarantees. No normative decision or deployment recommendation is made by translating them.
- `float` comparison/NaN wording and `int.abs` minimum-value behavior retain the source's scope qualifications. Secret display hiding is not expanded into encryption, memory wiping or universal leak prevention.
- Examples and historical measurements were not independently run. Mechanical checks and selected independent meaning reviews support translation fidelity, not a new compiler/security audit.

The frozen inventory still has 556 original Markdown rows. Phase 1 covered 126 rows, phase 2 adds 24, and phase 3 adds 14; **392 rows remain pending** after these local drafts are accounted for. New translated/metadata files do not increase that frozen denominator. The old central inventory is preserved; this status is its additive phase supplement.

## Added files

| Original | Language | Translation |
|---|---|---|
| [`docs/adr/0013-corpus-stays-id-keyed.md`](../adr/0013-corpus-stays-id-keyed.md) | ja | [docs/translations/reference/docs/adr/0013-corpus-stays-id-keyed.ja.md](reference/docs/adr/0013-corpus-stays-id-keyed.ja.md) |
| [`docs/adr/0013-corpus-stays-id-keyed.md`](../adr/0013-corpus-stays-id-keyed.md) | zh-CN | [docs/translations/reference/docs/adr/0013-corpus-stays-id-keyed.zh-CN.md](reference/docs/adr/0013-corpus-stays-id-keyed.zh-CN.md) |
| [`docs/adr/0014-expectations-are-derived.md`](../adr/0014-expectations-are-derived.md) | ja | [docs/translations/reference/docs/adr/0014-expectations-are-derived.ja.md](reference/docs/adr/0014-expectations-are-derived.ja.md) |
| [`docs/adr/0014-expectations-are-derived.md`](../adr/0014-expectations-are-derived.md) | zh-CN | [docs/translations/reference/docs/adr/0014-expectations-are-derived.zh-CN.md](reference/docs/adr/0014-expectations-are-derived.zh-CN.md) |
| [`docs/adr/0030-compile-cost-is-a-contract.md`](../adr/0030-compile-cost-is-a-contract.md) | ja | [docs/translations/reference/docs/adr/0030-compile-cost-is-a-contract.ja.md](reference/docs/adr/0030-compile-cost-is-a-contract.ja.md) |
| [`docs/adr/0030-compile-cost-is-a-contract.md`](../adr/0030-compile-cost-is-a-contract.md) | zh-CN | [docs/translations/reference/docs/adr/0030-compile-cost-is-a-contract.zh-CN.md](reference/docs/adr/0030-compile-cost-is-a-contract.zh-CN.md) |
| [`docs/stdlib/base64.md`](../stdlib/base64.md) | ja | [docs/translations/reference/docs/stdlib/base64.ja.md](reference/docs/stdlib/base64.ja.md) |
| [`docs/stdlib/base64.md`](../stdlib/base64.md) | zh-CN | [docs/translations/reference/docs/stdlib/base64.zh-CN.md](reference/docs/stdlib/base64.zh-CN.md) |
| [`docs/stdlib/float.md`](../stdlib/float.md) | ja | [docs/translations/reference/docs/stdlib/float.ja.md](reference/docs/stdlib/float.ja.md) |
| [`docs/stdlib/float.md`](../stdlib/float.md) | zh-CN | [docs/translations/reference/docs/stdlib/float.zh-CN.md](reference/docs/stdlib/float.zh-CN.md) |
| [`docs/stdlib/hex.md`](../stdlib/hex.md) | ja | [docs/translations/reference/docs/stdlib/hex.ja.md](reference/docs/stdlib/hex.ja.md) |
| [`docs/stdlib/hex.md`](../stdlib/hex.md) | zh-CN | [docs/translations/reference/docs/stdlib/hex.zh-CN.md](reference/docs/stdlib/hex.zh-CN.md) |
| [`docs/stdlib/int.md`](../stdlib/int.md) | ja | [docs/translations/reference/docs/stdlib/int.ja.md](reference/docs/stdlib/int.ja.md) |
| [`docs/stdlib/int.md`](../stdlib/int.md) | zh-CN | [docs/translations/reference/docs/stdlib/int.zh-CN.md](reference/docs/stdlib/int.zh-CN.md) |
| [`docs/stdlib/list.md`](../stdlib/list.md) | ja | [docs/translations/reference/docs/stdlib/list.ja.md](reference/docs/stdlib/list.ja.md) |
| [`docs/stdlib/list.md`](../stdlib/list.md) | zh-CN | [docs/translations/reference/docs/stdlib/list.zh-CN.md](reference/docs/stdlib/list.zh-CN.md) |
| [`docs/stdlib/map.md`](../stdlib/map.md) | ja | [docs/translations/reference/docs/stdlib/map.ja.md](reference/docs/stdlib/map.ja.md) |
| [`docs/stdlib/map.md`](../stdlib/map.md) | zh-CN | [docs/translations/reference/docs/stdlib/map.zh-CN.md](reference/docs/stdlib/map.zh-CN.md) |
| [`docs/stdlib/option.md`](../stdlib/option.md) | ja | [docs/translations/reference/docs/stdlib/option.ja.md](reference/docs/stdlib/option.ja.md) |
| [`docs/stdlib/option.md`](../stdlib/option.md) | zh-CN | [docs/translations/reference/docs/stdlib/option.zh-CN.md](reference/docs/stdlib/option.zh-CN.md) |
| [`docs/stdlib/result.md`](../stdlib/result.md) | ja | [docs/translations/reference/docs/stdlib/result.ja.md](reference/docs/stdlib/result.ja.md) |
| [`docs/stdlib/result.md`](../stdlib/result.md) | zh-CN | [docs/translations/reference/docs/stdlib/result.zh-CN.md](reference/docs/stdlib/result.zh-CN.md) |
| [`docs/stdlib/secret.md`](../stdlib/secret.md) | ja | [docs/translations/reference/docs/stdlib/secret.ja.md](reference/docs/stdlib/secret.ja.md) |
| [`docs/stdlib/secret.md`](../stdlib/secret.md) | zh-CN | [docs/translations/reference/docs/stdlib/secret.zh-CN.md](reference/docs/stdlib/secret.zh-CN.md) |
| [`docs/stdlib/set.md`](../stdlib/set.md) | ja | [docs/translations/reference/docs/stdlib/set.ja.md](reference/docs/stdlib/set.ja.md) |
| [`docs/stdlib/set.md`](../stdlib/set.md) | zh-CN | [docs/translations/reference/docs/stdlib/set.zh-CN.md](reference/docs/stdlib/set.zh-CN.md) |
| [`docs/stdlib/string.md`](../stdlib/string.md) | ja | [docs/translations/reference/docs/stdlib/string.ja.md](reference/docs/stdlib/string.ja.md) |
| [`docs/stdlib/string.md`](../stdlib/string.md) | zh-CN | [docs/translations/reference/docs/stdlib/string.zh-CN.md](reference/docs/stdlib/string.zh-CN.md) |

## Still pending original files

The following rows are not claimed translated by phases 1–3. The class is a routing label, not a permanent exclusion.

| Original | Routing class | Status |
|---|---|---|
| [`.claude/commands/almide-lang-bench.md`](../../.claude/commands/almide-lang-bench.md) | Agent instructions / embedded template | Pending |
| [`.claude/commands/almide-msr.md`](../../.claude/commands/almide-msr.md) | Agent instructions / embedded template | Pending |
| [`.claude/commands/almide-native-perf.md`](../../.claude/commands/almide-native-perf.md) | Agent instructions / embedded template | Pending |
| [`.claude/commands/almide-release.md`](../../.claude/commands/almide-release.md) | Agent instructions / embedded template | Pending |
| [`.claude/commands/almide-stats.md`](../../.claude/commands/almide-stats.md) | Agent instructions / embedded template | Pending |
| [`.claude/commands/almide-test.md`](../../.claude/commands/almide-test.md) | Agent instructions / embedded template | Pending |
| [`.claude/commands/almide-wasm-size.md`](../../.claude/commands/almide-wasm-size.md) | Agent instructions / embedded template | Pending |
| [`CHANGELOG.md`](../../CHANGELOG.md) | Release history | Pending |
| [`CLAUDE.md`](../../CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/CLAUDE.md`](../../crates/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-base/CLAUDE.md`](../../crates/almide-base/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-codegen/CLAUDE.md`](../../crates/almide-codegen/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-frontend/CLAUDE.md`](../../crates/almide-frontend/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-interp/CLAUDE.md`](../../crates/almide-interp/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-ir/CLAUDE.md`](../../crates/almide-ir/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-lang/CLAUDE.md`](../../crates/almide-lang/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-optimize/CLAUDE.md`](../../crates/almide-optimize/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-perceus-belt/README.md`](../../crates/almide-perceus-belt/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`crates/almide-syntax/CLAUDE.md`](../../crates/almide-syntax/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-tools/CLAUDE.md`](../../crates/almide-tools/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-types/CLAUDE.md`](../../crates/almide-types/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-wasm-vm/REQUIREMENTS.md`](../../crates/almide-wasm-vm/REQUIREMENTS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/CHEATSHEET.md`](../CHEATSHEET.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/GRAMMAR.md`](../GRAMMAR.md) | Normative specification / grammar | Pending |
| [`docs/SPEC.md`](../SPEC.md) | Normative specification / grammar | Pending |
| [`docs/STABILITY.md`](../STABILITY.md) | Normative specification / grammar | Pending |
| [`docs/TRUST-SPINE.md`](../TRUST-SPINE.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/adr/0001-deterministic-time-units.md`](../adr/0001-deterministic-time-units.md) | Decision history — immutable original | Pending |
| [`docs/adr/0002-fallibility-effect-orthogonal.md`](../adr/0002-fallibility-effect-orthogonal.md) | Decision history — immutable original | Pending |
| [`docs/adr/0003-error-type-conversion-at-propagation.md`](../adr/0003-error-type-conversion-at-propagation.md) | Decision history — immutable original | Pending |
| [`docs/adr/0004-error-branchability-doctrine.md`](../adr/0004-error-branchability-doctrine.md) | Decision history — immutable original | Pending |
| [`docs/adr/0005-operators-desugar-to-stdlib.md`](../adr/0005-operators-desugar-to-stdlib.md) | Decision history — immutable original | Pending |
| [`docs/adr/0006-fallibility-polymorphic-hofs.md`](../adr/0006-fallibility-polymorphic-hofs.md) | Decision history — immutable original | Pending |
| [`docs/adr/0007-partition-replaces-collect.md`](../adr/0007-partition-replaces-collect.md) | Decision history — immutable original | Pending |
| [`docs/adr/0008-explicit-propagation-only.md`](../adr/0008-explicit-propagation-only.md) | Decision history — immutable original | Pending |
| [`docs/adr/0009-fn-type-quadrant-transparency.md`](../adr/0009-fn-type-quadrant-transparency.md) | Decision history — immutable original | Pending |
| [`docs/adr/0010-option-type-shorthand.md`](../adr/0010-option-type-shorthand.md) | Decision history — immutable original | Pending |
| [`docs/adr/0011-execution-substrate-is-a-free-variable.md`](../adr/0011-execution-substrate-is-a-free-variable.md) | Decision history — immutable original | Pending |
| [`docs/adr/0012-typed-error-refinement-in-the-marker.md`](../adr/0012-typed-error-refinement-in-the-marker.md) | Decision history — immutable original | Pending |
| [`docs/adr/0013-string-shared-immutable-buffer.md`](../adr/0013-string-shared-immutable-buffer.md) | Decision history — immutable original | Pending |
| [`docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md`](../adr/0015-reference-evaluator-is-fresh-source-level-python.md) | Decision history — immutable original | Pending |
| [`docs/adr/0016-numeric-determinism-is-normative.md`](../adr/0016-numeric-determinism-is-normative.md) | Decision history — immutable original | Pending |
| [`docs/adr/0017-flight-profile-is-a-normative-subset.md`](../adr/0017-flight-profile-is-a-normative-subset.md) | Decision history — immutable original | Pending |
| [`docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md`](../adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md) | Decision history — immutable original | Pending |
| [`docs/adr/0020-http-serving-and-handler-state.md`](../adr/0020-http-serving-and-handler-state.md) | Decision history — immutable original | Pending |
| [`docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md`](../adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) | Decision history — immutable original | Pending |
| [`docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md`](../adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) | Decision history — immutable original | Pending |
| [`docs/adr/0023-wasm-http-over-wasi-http.md`](../adr/0023-wasm-http-over-wasi-http.md) | Decision history — immutable original | Pending |
| [`docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md`](../adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) | Decision history — immutable original | Pending |
| [`docs/adr/0025-wasm-process-is-a-private-host-capability.md`](../adr/0025-wasm-process-is-a-private-host-capability.md) | Decision history — immutable original | Pending |
| [`docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md`](../adr/0026-effect-categories-ride-the-transparent-fn-type.md) | Decision history — immutable original | Pending |
| [`docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md`](../adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) | Decision history — immutable original | Pending |
| [`docs/adr/0028-fan-selection-names-and-cost-contract.md`](../adr/0028-fan-selection-names-and-cost-contract.md) | Decision history — immutable original | Pending |
| [`docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md`](../adr/0029-assert-aborts-runs-its-closure-in-a-child.md) | Decision history — immutable original | Pending |
| [`docs/adr/0031-raw-pointers-carry-provenance.md`](../adr/0031-raw-pointers-carry-provenance.md) | Decision history — immutable original | Pending |
| [`docs/adr/0032-a-response-content-type-is-its-body-type.md`](../adr/0032-a-response-content-type-is-its-body-type.md) | Decision history — immutable original | Pending |
| [`docs/adr/README.md`](../adr/README.md) | Decision history — immutable original | Pending |
| [`docs/benchmarks/README.md`](../benchmarks/README.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/benchmarks/fan-floor.md`](../benchmarks/fan-floor.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/benchmarks/float-printer.md`](../benchmarks/float-printer.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/bugfix-research-2026-09-09.md`](../bugfix-research-2026-09-09.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/bugfix-research-2026-09-12.md`](../bugfix-research-2026-09-12.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/contracts/.conformance-header.md`](../contracts/.conformance-header.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/.readme-header.md`](../contracts/.readme-header.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/C-006-fan-timeout-removed.md`](../contracts/C-006-fan-timeout-removed.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/C-008-009-010-repr.md`](../contracts/C-008-009-010-repr.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/C-033-cow-truth-table.md`](../contracts/C-033-cow-truth-table.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/README.md`](../contracts/README.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/conformance.md`](../contracts/conformance.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/proven-vs-trusted.md`](../contracts/proven-vs-trusted.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/design/MEMORY-SAFETY.md`](../design/MEMORY-SAFETY.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/fan-cross-language.md`](../design/fan-cross-language.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/fan-semantics-audit.md`](../design/fan-semantics-audit.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/fan-usability-eval.md`](../design/fan-usability-eval.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/fan-capture-ownership-review.md`](../fan-capture-ownership-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/fan-parallel-safety-review.md`](../fan-parallel-safety-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/native-projection-ownership-review.md`](../native-projection-ownership-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/native-read-borrow-review.md`](../native-read-borrow-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/BENCHMARKS.md`](../project/BENCHMARKS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/CLAUDE_TEMPLATE.md`](../project/CLAUDE_TEMPLATE.md) | Agent instructions / embedded template | Pending |
| [`docs/project/org-trust-status.md`](../project/org-trust-status.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/record-option-cells-verification.md`](../record-option-cells-verification.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/releases/README.md`](../releases/README.md) | Release history | Pending |
| [`docs/releases/v0.68.0-rc1.md`](../releases/v0.68.0-rc1.md) | Release history | Pending |
| [`docs/releases/v0.68.0-rc2.md`](../releases/v0.68.0-rc2.md) | Release history | Pending |
| [`docs/releases/v0.68.0.md`](../releases/v0.68.0.md) | Release history | Pending |
| [`docs/releases/v0.69.0-rc1.md`](../releases/v0.69.0-rc1.md) | Release history | Pending |
| [`docs/releases/v0.69.0.md`](../releases/v0.69.0.md) | Release history | Pending |
| [`docs/roadmap/ALL_AXES.md`](../roadmap/ALL_AXES.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/GRAND_PLAN.md`](../roadmap/GRAND_PLAN.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/PRODUCTION_READY.md`](../roadmap/PRODUCTION_READY.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/README.md`](../roadmap/README.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/ROAD_TO_1_0.md`](../roadmap/ROAD_TO_1_0.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/adr-0013-string-sharing.md`](../roadmap/active/adr-0013-string-sharing.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/almide-perceus-belt.md`](../roadmap/active/almide-perceus-belt.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/arena-breakthroughs.md`](../roadmap/active/arena-breakthroughs.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/async-inception.md`](../roadmap/active/async-inception.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/async-world-claim.md`](../roadmap/active/async-world-claim.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/behavioral-contract.md`](../roadmap/active/behavioral-contract.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/build-speed-runtime-rlib.md`](../roadmap/active/build-speed-runtime-rlib.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/camp4-general-heap-match.md`](../roadmap/active/camp4-general-heap-match.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/certificate-format-v1.md`](../roadmap/active/certificate-format-v1.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/certification-grade.md`](../roadmap/active/certification-grade.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/ci-critical-path.md`](../roadmap/active/ci-critical-path.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/ci-warnings-cleanup.md`](../roadmap/active/ci-warnings-cleanup.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/closure-cross-target-completeness.md`](../roadmap/active/closure-cross-target-completeness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/codegen-traversal-totality.md`](../roadmap/active/codegen-traversal-totality.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/completeness-by-construction.md`](../roadmap/active/completeness-by-construction.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/concurrency-stance.md`](../roadmap/active/concurrency-stance.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/continuous-improvement.md`](../roadmap/active/continuous-improvement.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/correctness-guarantee-gaps.md`](../roadmap/active/correctness-guarantee-gaps.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/cross-target-completeness.md`](../roadmap/active/cross-target-completeness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/determinism-belt.md`](../roadmap/active/determinism-belt.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/deterministic-bounds.md`](../roadmap/active/deterministic-bounds.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/edit-locality-theory.md`](../roadmap/active/edit-locality-theory.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/effect-inspection-harness.md`](../roadmap/active/effect-inspection-harness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/effect-system-capability.md`](../roadmap/active/effect-system-capability.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/effectful-27-blueprint.md`](../roadmap/active/effectful-27-blueprint.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/execution-inception.md`](../roadmap/active/execution-inception.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/fan-v2-examples.md`](../roadmap/active/fan-v2-examples.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/fan-v2.md`](../roadmap/active/fan-v2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-evidence-gaps.md`](../roadmap/active/flight-evidence-gaps.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-organization.md`](../roadmap/active/flight-organization.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-profile.md`](../roadmap/active/flight-profile.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-qualification.md`](../roadmap/active/flight-qualification.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-reference-app.md`](../roadmap/active/flight-reference-app.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-rust-ferrocene.md`](../roadmap/active/flight-rust-ferrocene.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-subset-spec.md`](../roadmap/active/flight-subset-spec.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-wcet-loops.md`](../roadmap/active/flight-wcet-loops.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/fuzz-findings-triage.md`](../roadmap/active/fuzz-findings-triage.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/integer-literal-domain.md`](../roadmap/active/integer-literal-domain.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/interp-is-desugar-to-tostring.md`](../roadmap/active/interp-is-desugar-to-tostring.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/issue-inventory-2026-09-21.md`](../roadmap/active/issue-inventory-2026-09-21.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/issue-inventory-2026-09-21/issue-header-spec.md`](../roadmap/active/issue-inventory-2026-09-21/issue-header-spec.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/issue-ledger-burndown.md`](../roadmap/active/issue-ledger-burndown.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/lifted-carrier-abi.md`](../roadmap/active/lifted-carrier-abi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/llm-first-language.md`](../roadmap/active/llm-first-language.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/logical-time-async.md`](../roadmap/active/logical-time-async.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/logical-time-implementation.md`](../roadmap/active/logical-time-implementation.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/logical-time-proofs.md`](../roadmap/active/logical-time-proofs.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/map-data-structure-roadmap.md`](../roadmap/active/map-data-structure-roadmap.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/mir-caps-call-count-breach.md`](../roadmap/active/mir-caps-call-count-breach.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/mission-critical-attack-list.md`](../roadmap/active/mission-critical-attack-list.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/native-boxed-pattern-lowering.md`](../roadmap/active/native-boxed-pattern-lowering.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/native-trust-spine.md`](../roadmap/active/native-trust-spine.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/net-unix-shm-embedded.md`](../roadmap/active/net-unix-shm-embedded.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/outside-review-audit-2026-07.md`](../roadmap/active/outside-review-audit-2026-07.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/playground-revamp.md`](../roadmap/active/playground-revamp.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/post-0.29-improvement-sweep.md`](../roadmap/active/post-0.29-improvement-sweep.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/protocol-any-existentials.md`](../roadmap/active/protocol-any-existentials.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/receipt-logic.md`](../roadmap/active/receipt-logic.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/reconciliation-followup.md`](../roadmap/active/reconciliation-followup.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/result-family-from-type.md`](../roadmap/active/result-family-from-type.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/return-op-eradication.md`](../roadmap/active/return-op-eradication.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/rot-eradication.md`](../roadmap/active/rot-eradication.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/security-by-construction.md`](../roadmap/active/security-by-construction.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/selfhost-link-v2.md`](../roadmap/active/selfhost-link-v2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/shell-scripting-surface.md`](../roadmap/active/shell-scripting-surface.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/stdlib-redesign.md`](../roadmap/active/stdlib-redesign.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/test-surface-25x.md`](../roadmap/active/test-surface-25x.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/ticks-interface-audit.md`](../roadmap/active/ticks-interface-audit.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/trust-layer.md`](../roadmap/active/trust-layer.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/type-where-constraints.md`](../roadmap/active/type-where-constraints.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v0-unwrap-early-return-leak.md`](../roadmap/active/v0-unwrap-early-return-leak.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-adt-brick5-goal.md`](../roadmap/active/v1-adt-brick5-goal.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-adt-value-model.md`](../roadmap/active/v1-adt-value-model.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-backlog.md`](../roadmap/active/v1-backlog.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-bolt-backlog.md`](../roadmap/active/v1-bolt-backlog.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-develop-reflow.md`](../roadmap/active/v1-develop-reflow.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-heap-result-control-flow.md`](../roadmap/active/v1-heap-result-control-flow.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-join-completeness.md`](../roadmap/active/v1-join-completeness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-kgi-kpi.md`](../roadmap/active/v1-kgi-kpi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-loop-ownership-cert.md`](../roadmap/active/v1-loop-ownership-cert.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-mir-architecture.md`](../roadmap/active/v1-mir-architecture.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-org-byte-verification.md`](../roadmap/active/v1-org-byte-verification.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-parser-tco-lever.md`](../roadmap/active/v1-parser-tco-lever.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-phase1-mir-core.md`](../roadmap/active/v1-phase1-mir-core.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-proof-architecture.md`](../roadmap/active/v1-proof-architecture.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-proof-spine-progress.md`](../roadmap/active/v1-proof-spine-progress.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-records-svg.md`](../roadmap/active/v1-records-svg.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-release-path.md`](../roadmap/active/v1-release-path.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-selfhost-machinery.md`](../roadmap/active/v1-selfhost-machinery.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-selfhost-print-floor.md`](../roadmap/active/v1-selfhost-print-floor.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-spine-correctness-holes.md`](../roadmap/active/v1-spine-correctness-holes.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-system-map.md`](../roadmap/active/v1-system-map.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-tco-self-recursion.md`](../roadmap/active/v1-tco-self-recursion.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-v0-parity.md`](../roadmap/active/v1-v0-parity.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-value-model.md`](../roadmap/active/v1-value-model.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/value-rc-cert.md`](../roadmap/active/value-rc-cert.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/wasm-frees-ownership-discipline.md`](../roadmap/active/wasm-frees-ownership-discipline.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/wasm-ownership-emit-mechanization.md`](../roadmap/active/wasm-ownership-emit-mechanization.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/wasm-platform-frontier.md`](../roadmap/active/wasm-platform-frontier.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/zero-committed-shell.md`](../roadmap/active/zero-committed-shell.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/bundled-almide-dispatch.md`](../roadmap/done/bundled-almide-dispatch.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/bundled-almide-ideal-form.md`](../roadmap/done/bundled-almide-ideal-form.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/claim-wording-perceus-byte-identity.md`](../roadmap/done/claim-wording-perceus-byte-identity.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/closure-architecture-v2.md`](../roadmap/done/closure-architecture-v2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/closure-codegen-cross-target-gaps.md`](../roadmap/done/closure-codegen-cross-target-gaps.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/code-health-codopsy.md`](../roadmap/done/code-health-codopsy.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/codegen-ideal-form.md`](../roadmap/done/codegen-ideal-form.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/effect-system-phase1-2.md`](../roadmap/done/effect-system-phase1-2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/hkt-foundation-phase1.md`](../roadmap/done/hkt-foundation-phase1.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/playground-repair.md`](../roadmap/done/playground-repair.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/reimpl-lint.md`](../roadmap/done/reimpl-lint.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/stdlib-declarative-unification.md`](../roadmap/done/stdlib-declarative-unification.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/variant-exhaustiveness-refinement.md`](../roadmap/done/variant-exhaustiveness-refinement.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/wasm-engine-redesign.md`](../roadmap/done/wasm-engine-redesign.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/wasm-optimization-roadmap.md`](../roadmap/done/wasm-optimization-roadmap.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/almide-to-almide-ffi.md`](../roadmap/on-hold/almide-to-almide-ffi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/almide-ui.md`](../roadmap/on-hold/almide-ui.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/api-diff-auto-versioning.md`](../roadmap/on-hold/api-diff-auto-versioning.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/benchmark-next-phase.md`](../roadmap/on-hold/benchmark-next-phase.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/compile-time-contracts.md`](../roadmap/on-hold/compile-time-contracts.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/diagnostic-snippet-externalization.md`](../roadmap/on-hold/diagnostic-snippet-externalization.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/flow-design.md`](../roadmap/on-hold/flow-design.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/flow-spec-draft.md`](../roadmap/on-hold/flow-spec-draft.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/gpu-compute.md`](../roadmap/on-hold/gpu-compute.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/ir-optimization-tier2.md`](../roadmap/on-hold/ir-optimization-tier2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/law-blocks-experiment.md`](../roadmap/on-hold/law-blocks-experiment.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/law-proofs-design.md`](../roadmap/on-hold/law-proofs-design.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/llm-integration.md`](../roadmap/on-hold/llm-integration.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/lsp-code-actions.md`](../roadmap/on-hold/lsp-code-actions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/ml-inference.md`](../roadmap/on-hold/ml-inference.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/mlir-backend-adoption.md`](../roadmap/on-hold/mlir-backend-adoption.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/package-registry.md`](../roadmap/on-hold/package-registry.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/package-version-resolution.md`](../roadmap/on-hold/package-version-resolution.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/performance-research.md`](../roadmap/on-hold/performance-research.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/porta-embedded.md`](../roadmap/on-hold/porta-embedded.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/process-composition.md`](../roadmap/on-hold/process-composition.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/rainbow-bridge.md`](../roadmap/on-hold/rainbow-bridge.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/region-inference.md`](../roadmap/on-hold/region-inference.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/research-modification-survival-rate-paper.md`](../roadmap/on-hold/research-modification-survival-rate-paper.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/rumbling.md`](../roadmap/on-hold/rumbling.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/secure-by-design.md`](../roadmap/on-hold/secure-by-design.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/shell-completions.md`](../roadmap/on-hold/shell-completions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/snapshot-testing.md`](../roadmap/on-hold/snapshot-testing.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/supervision-and-actors.md`](../roadmap/on-hold/supervision-and-actors.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/wasm-exception-handling.md`](../roadmap/on-hold/wasm-exception-handling.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/wasm-http-client.md`](../roadmap/on-hold/wasm-http-client.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/web-framework.md`](../roadmap/on-hold/web-framework.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L0-conventions.md`](../roadmap/stdlib-redesign/L0-conventions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L1-categories.md`](../roadmap/stdlib-redesign/L1-categories.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-args.md`](../roadmap/stdlib-redesign/L2-args.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-base64.md`](../roadmap/stdlib-redesign/L2-base64.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-bigint.md`](../roadmap/stdlib-redesign/L2-bigint.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-bool.md`](../roadmap/stdlib-redesign/L2-bool.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-bytes.md`](../roadmap/stdlib-redesign/L2-bytes.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-compute.md`](../roadmap/stdlib-redesign/L2-compute.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-datetime.md`](../roadmap/stdlib-redesign/L2-datetime.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-deque.md`](../roadmap/stdlib-redesign/L2-deque.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-duration.md`](../roadmap/stdlib-redesign/L2-duration.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-env.md`](../roadmap/stdlib-redesign/L2-env.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-error.md`](../roadmap/stdlib-redesign/L2-error.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-ffi.md`](../roadmap/stdlib-redesign/L2-ffi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-float.md`](../roadmap/stdlib-redesign/L2-float.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-fs.md`](../roadmap/stdlib-redesign/L2-fs.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-hash.md`](../roadmap/stdlib-redesign/L2-hash.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-heap.md`](../roadmap/stdlib-redesign/L2-heap.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-hex.md`](../roadmap/stdlib-redesign/L2-hex.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-html.md`](../roadmap/stdlib-redesign/L2-html.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-http.md`](../roadmap/stdlib-redesign/L2-http.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-int.md`](../roadmap/stdlib-redesign/L2-int.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-io.md`](../roadmap/stdlib-redesign/L2-io.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-json.md`](../roadmap/stdlib-redesign/L2-json.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-list.md`](../roadmap/stdlib-redesign/L2-list.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-map.md`](../roadmap/stdlib-redesign/L2-map.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-math.md`](../roadmap/stdlib-redesign/L2-math.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-matrix.md`](../roadmap/stdlib-redesign/L2-matrix.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-net.md`](../roadmap/stdlib-redesign/L2-net.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-option.md`](../roadmap/stdlib-redesign/L2-option.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-path.md`](../roadmap/stdlib-redesign/L2-path.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-process.md`](../roadmap/stdlib-redesign/L2-process.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-random.md`](../roadmap/stdlib-redesign/L2-random.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-regex.md`](../roadmap/stdlib-redesign/L2-regex.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-result.md`](../roadmap/stdlib-redesign/L2-result.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-secret.md`](../roadmap/stdlib-redesign/L2-secret.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-set.md`](../roadmap/stdlib-redesign/L2-set.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-sized-numbers.md`](../roadmap/stdlib-redesign/L2-sized-numbers.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-string.md`](../roadmap/stdlib-redesign/L2-string.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-testing.md`](../roadmap/stdlib-redesign/L2-testing.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-unix.md`](../roadmap/stdlib-redesign/L2-unix.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-url.md`](../roadmap/stdlib-redesign/L2-url.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-uuid.md`](../roadmap/stdlib-redesign/L2-uuid.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-value.md`](../roadmap/stdlib-redesign/L2-value.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-zlib.md`](../roadmap/stdlib-redesign/L2-zlib.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/README.md`](../roadmap/stdlib-redesign/README.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/facts/categories.md`](../roadmap/stdlib-redesign/facts/categories.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/facts/naming-conventions.md`](../roadmap/stdlib-redesign/facts/naming-conventions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/protocol-view.md`](../roadmap/stdlib-redesign/protocol-view.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/target-surface.md`](../roadmap/stdlib-redesign/target-surface.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/specs/CLAUDE.md`](../specs/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`docs/specs/als/README.md`](../specs/als/README.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/STANDARD.md`](../specs/als/STANDARD.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/bounded.md`](../specs/als/bounded.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/collections.md`](../specs/als/collections.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/data-formats.md`](../specs/als/data-formats.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/deterministic-time.md`](../specs/als/deterministic-time.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/expressions.md`](../specs/als/expressions.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/implementation.md`](../specs/als/implementation.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/runtime.md`](../specs/als/runtime.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/semantics.md`](../specs/als/semantics.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/strings.md`](../specs/als/strings.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/text-and-numbers.md`](../specs/als/text-and-numbers.md) | Normative specification / grammar | Pending |
| [`docs/specs/cli.md`](../specs/cli.md) | Normative specification / grammar | Pending |
| [`docs/specs/codegen.md`](../specs/codegen.md) | Normative specification / grammar | Pending |
| [`docs/specs/edit-locality.md`](../specs/edit-locality.md) | Normative specification / grammar | Pending |
| [`docs/specs/effect-fn-call-semantics.md`](../specs/effect-fn-call-semantics.md) | Normative specification / grammar | Pending |
| [`docs/specs/effect-system.md`](../specs/effect-system.md) | Normative specification / grammar | Pending |
| [`docs/specs/language.md`](../specs/language.md) | Normative specification / grammar | Pending |
| [`docs/specs/module-system.md`](../specs/module-system.md) | Normative specification / grammar | Pending |
| [`docs/specs/package-system.md`](../specs/package-system.md) | Normative specification / grammar | Pending |
| [`docs/specs/perceus.md`](../specs/perceus.md) | Normative specification / grammar | Pending |
| [`docs/specs/result-option-effect.md`](../specs/result-option-effect.md) | Normative specification / grammar | Pending |
| [`docs/specs/scoped.md`](../specs/scoped.md) | Normative specification / grammar | Pending |
| [`docs/specs/type-system.md`](../specs/type-system.md) | Normative specification / grammar | Pending |
| [`docs/stdlib/README.md`](../stdlib/README.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/bigint.md`](../stdlib/bigint.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/bytes.md`](../stdlib/bytes.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/compute.md`](../stdlib/compute.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/datetime.md`](../stdlib/datetime.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/deque.md`](../stdlib/deque.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/ffi.md`](../stdlib/ffi.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/float32.md`](../stdlib/float32.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/float64.md`](../stdlib/float64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/fs.md`](../stdlib/fs.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/hash.md`](../stdlib/hash.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/heap.md`](../stdlib/heap.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/html.md`](../stdlib/html.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/http.md`](../stdlib/http.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int16.md`](../stdlib/int16.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int32.md`](../stdlib/int32.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int64.md`](../stdlib/int64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int8.md`](../stdlib/int8.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/math.md`](../stdlib/math.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/matrix.md`](../stdlib/matrix.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/net.md`](../stdlib/net.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/process.md`](../stdlib/process.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/random.md`](../stdlib/random.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/regex.md`](../stdlib/regex.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/testing.md`](../stdlib/testing.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint16.md`](../stdlib/uint16.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint32.md`](../stdlib/uint32.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint64.md`](../stdlib/uint64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint8.md`](../stdlib/uint8.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/unix.md`](../stdlib/unix.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uuid.md`](../stdlib/uuid.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/value.md`](../stdlib/value.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/zlib.md`](../stdlib/zlib.md) | Standard library — generated sections / gates | Pending |
| [`docs/stream-fusion-safety-review.md`](../stream-fusion-safety-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm-decoder-reclaim-review.md`](../wasm-decoder-reclaim-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/JS-HOST-ASYNC-IMPORTS.md`](../wasm/JS-HOST-ASYNC-IMPORTS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/README.md`](../wasm/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/WASM-OUTPUT.md`](../wasm/WASM-OUTPUT.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/capability-system.md`](../wasm/capability-system.md) | Handwritten documentation — consumer audit pending | Pending |
| [`proofs/COVERAGE.md`](../../proofs/COVERAGE.md) | Proof records — machine evidence | Pending |
| [`proofs/DO330-GAP.md`](../../proofs/DO330-GAP.md) | Proof records — machine evidence | Pending |
| [`proofs/FORMAL-CREDIT.md`](../../proofs/FORMAL-CREDIT.md) | Proof records — machine evidence | Pending |
| [`proofs/README.md`](../../proofs/README.md) | Proof records — machine evidence | Pending |
| [`proofs/STAGE-STATUS.md`](../../proofs/STAGE-STATUS.md) | Proof records — machine evidence | Pending |
| [`proofs/TOOL-QUALIFICATION.md`](../../proofs/TOOL-QUALIFICATION.md) | Proof records — machine evidence | Pending |
| [`proofs/TOR.md`](../../proofs/TOR.md) | Proof records — machine evidence | Pending |
| [`proofs/TRUSTED_BASE.md`](../../proofs/TRUSTED_BASE.md) | Proof records — machine evidence | Pending |
| [`proofs/regex/README.md`](../../proofs/regex/README.md) | Proof records — machine evidence | Pending |
| [`research/CLAUDE.md`](../../research/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`research/benchmark/exercises/BENCHMARK.md`](../../research/benchmark/exercises/BENCHMARK.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/fuzz-green/README.md`](../../research/benchmark/fuzz-green/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/lang-bench/README.md`](../../research/benchmark/lang-bench/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/lang-bench/tasks-example/xlang/README.md`](../../research/benchmark/lang-bench/tasks-example/xlang/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/01-add-sub/prompt.md`](../../research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/01-add-sub/prompt.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/02-rename-show-to-print/prompt.md`](../../research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/02-rename-show-to-print/prompt.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/hot-issues-2026-09-10/README.md`](../../research/benchmark/perf/hot-issues-2026-09-10/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md`](../../research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/spectralnorm/README.md`](../../research/benchmark/perf/spectralnorm/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/string-gap-1004.md`](../../research/benchmark/perf/string-gap-1004.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/stdlib/HANDOFF.md`](../../research/benchmark/stdlib/HANDOFF.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/three-way/README.md`](../../research/benchmark/three-way/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/beyond-rust/README.md`](../../research/beyond-rust/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/LESSONS.md`](../../research/grammar-lab/LESSONS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/REPORT.md`](../../research/grammar-lab/REPORT.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/experiments/optional-handling/REPORT.md`](../../research/grammar-lab/experiments/optional-handling/REPORT.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/prompts/layer1_rules.md`](../../research/grammar-lab/prompts/layer1_rules.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/prompts/layer2_combinator.md`](../../research/grammar-lab/prompts/layer2_combinator.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/prompts/layer2_laws.md`](../../research/grammar-lab/prompts/layer2_laws.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/prompts/layer2_match.md`](../../research/grammar-lab/prompts/layer2_match.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/prompts/layer2_variant_fn.md`](../../research/grammar-lab/prompts/layer2_variant_fn.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/prompts/layer2_variant_paren.md`](../../research/grammar-lab/prompts/layer2_variant_paren.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/spike/charge-probe/BURNDOWN.md`](../../research/spike/charge-probe/BURNDOWN.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/spike/charge-probe/REPORT.md`](../../research/spike/charge-probe/REPORT.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/spike/logical-time-race/GATE.md`](../../research/spike/logical-time-race/GATE.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/spike/substrate-observability/README.md`](../../research/spike/substrate-observability/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/spike/v1-mir/GATE.md`](../../research/spike/v1-mir/GATE.md) | Handwritten documentation — consumer audit pending | Pending |
