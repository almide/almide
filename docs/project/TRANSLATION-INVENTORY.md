# Documentation translation inventory

Snapshot: `9a95b28519b90ae535dc9918bfed04805dc4675c` (2026-11-02).
This inventory covers **556 tracked Markdown files** at that snapshot, not 556 independent English documents. The existing README translations are included. New translations and this inventory itself are outside that frozen denominator.

## File convention and authority

- Keep existing source paths and meanings. For English reader guides, `name.md`, `name.ja.md`, and `name.zh-CN.md` are siblings with language navigation.
- Preserve code, commands, identifiers, dates, measurements, source links, and qualification of guarantees. Link to translated siblings only when they exist; untranslated destinations keep their original links.
- A translation is a reader aid, not a new normative source. The original specification and its upstream ALS authority remain controlling; translation disagreements must be fixed, not used to change language semantics.
- Existing originals can be Japanese or mixed-language. Do not label every existing `.md` file English or silently replace it to enforce a suffix convention. Inspect each before deciding whether `.en.md` is required to preserve its identity.
- Current batches: eighteen reader guides with thirty-six translated files, plus 104 diagnostic-code pages and their index in Japanese and Simplified Chinese (210 files), for 246 new language files. Japanese originals of BREAKING_CHANGE_POLICY and HIDDEN_OPERATIONS retain their existing `.md` paths, with `.en.md` and `.zh-CN.md` siblings. Existing README translations are preserved with only links to these guides updated. All diagnostic originals, including their README, remain byte-identical; the general docs index links to the separate reader tree. The CLI remains English. No compiler, generator, workflow, helper, or CI policy changes are included.

## Source caveats retained, not silently resolved

Translation preserves source statements; it is not independent verification of every claim. `SECURITY.md` says “latest minor release line”, whereas `SUPPORT.md` says “latest release”. The intended maintenance scope needs owner reconciliation. `docs/mcp.md` describes this repository as scored on LLM writing accuracy, while `CLAUDE.md` assigns LLM writability to Dojo and compiler correctness here. These original statements have not been rewritten by this localization batch. CI behavior described in support prose is likewise not a newly established guarantee.

## Remaining work and safe placement

The classifications below are routing decisions, not completion claims or permanent exclusions.

- **Handwritten documentation:** audit consumers and source language, then translate in coherent groups. Pending rows are not yet certified safe to duplicate.
- **Diagnostics:** `build.rs` and `tests/explain_docs_test.rs` select numeric `E…` stems; sibling suffixes are excluded by those two readers. Other consumers still need review. Website translations would not add translated `almide explain` output; CLI localization is a separate implementation scope.
- **Standard library:** generated signature blocks and executable fences need a translation arrangement that retains a single generated API source. Do not hand-copy generated indexes and claim they stay synchronized. Inspect generators before choosing adjacent prose files or a separate translated guide tree.
- **Roadmap:** `generate-readme.sh` counts `active/*.md` and similar globs. Sibling translations there would create false roadmap items. Use a separate translated tree outside those globs, retaining dated original pointers; do not change the generator or counts in this task.
- **Specifications / ALS:** translations are possible as explicitly non-normative reading aids linked to a pinned source. Preserve section IDs and original normative authority; audit contract section scanners before placement, preferring a separate tree where necessary.
- **ADR:** preserve numbered original bodies and history. Non-normative, source-pinned translations can live outside the numbered original directory, with navigation in a mutable index rather than editing immutable records.
- **Evidence / proof / releases:** preserve recorded facts, dates and source identity. Separate explanatory translations from generated or sealed evidence. Do not duplicate markers that generators or checks interpret as authoritative inputs.
- **Agent instructions / templates:** do not create competing automatically loaded instructions or replace the embedded template; consider explicitly labeled human-readable translations outside instruction discovery paths.

## Handwritten-file consumer audit

The [consumer audit](./TRANSLATION-CONSUMERS.md) inspects the 80 files originally routed to handwritten-document review: five translation candidates, sixteen requiring special handling, and fifty-nine still awaiting deeper body/indirect-consumer review. It is a frozen audit, not a claim that the original routing class was correct. In particular, `STABILITY.md` is normative, `org-trust-status.md` is legacy generated evidence, `fuzz-green/README.md` is parsed evidence, and experiment prompt files are machine inputs. The first of these is already reclassified below; the per-file audit supplies the finer routing for the rest.

The existing `check-docs-only.sh` and `ci_prose.py` have different path scopes. A documentation-only classification by the former does not guarantee that all workflows skip their compiler jobs. No classifier is changed here.

## Current coverage

Of the frozen 556 Markdown rows, 126 have existing translations or newly supplied reading variants; **430 remain pending**. This is a file inventory, not 556 unique English documents: the three existing root README language files are counted separately. No universal translation-completeness claim is made.

The diagnostic placement audit is included in [the consumer audit](./TRANSLATION-CONSUMERS.md). `docs/translations/{ja,zh-CN}/diagnostics/` avoids code scanners and preserves the original CLI output. [Japanese source caveats](../translations/ja/diagnostics/README.md#source-caveats) and [Chinese source caveats](../translations/zh-CN/diagnostics/README.md#source-caveats) identify original wording conflicts instead of guessing implementation behavior. All 104 numeric E-code pages have source-pinned reading variants; the remaining inventory still requires staged work.

## Counts by class

| Class | Files |
|---|---:|
| Agent instructions / embedded template | 22 |
| Decision history — immutable original | 33 |
| Diagnostic docs — embedded / scanned | 105 |
| Evidence / generated blocks / gate inputs | 12 |
| Existing translations | 3 |
| Handwritten documentation — consumer audit pending | 66 |
| Normative specification / grammar | 27 |
| Proof records — machine evidence | 9 |
| Reader guides — translated batches | 18 |
| Release history | 7 |
| Roadmap — scanned tree / generated index | 202 |
| Standard library — generated sections / gates | 52 |

## File-by-file status

`Drafted in this PR` means the two translated files are supplied, not that the PR has merged. `Existing — preserved` means existing translations were not rewritten. `Pending` includes consumer/source-language review and translation work.

| Source path | Class | Translation status |
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
| [`README.ja.md`](../../README.ja.md) | Existing translations | Existing — preserved |
| [`README.md`](../../README.md) | Existing translations | Existing — preserved |
| [`README.zh-CN.md`](../../README.zh-CN.md) | Existing translations | Existing — preserved |
| [`SECURITY.md`](../../SECURITY.md) | Reader guides — translated batches | Drafted in this PR |
| [`SUPPORT.md`](../../SUPPORT.md) | Reader guides — translated batches | Drafted in this PR |
| [`crates/CLAUDE.md`](../../crates/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/README.md`](../../crates/README.md) | Reader guides — translated batches | Drafted in this PR |
| [`crates/almide-base/CLAUDE.md`](../../crates/almide-base/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-codegen/CLAUDE.md`](../../crates/almide-codegen/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-egg-lab/README.md`](../../crates/almide-egg-lab/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`crates/almide-frontend/CLAUDE.md`](../../crates/almide-frontend/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-interp/CLAUDE.md`](../../crates/almide-interp/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-ir/CLAUDE.md`](../../crates/almide-ir/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-kernel/README.md`](../../crates/almide-kernel/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`crates/almide-lang/CLAUDE.md`](../../crates/almide-lang/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-optimize/CLAUDE.md`](../../crates/almide-optimize/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-perceus-belt/README.md`](../../crates/almide-perceus-belt/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`crates/almide-syntax/CLAUDE.md`](../../crates/almide-syntax/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-tools/CLAUDE.md`](../../crates/almide-tools/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-types/CLAUDE.md`](../../crates/almide-types/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`crates/almide-wasm-vm/REQUIREMENTS.md`](../../crates/almide-wasm-vm/REQUIREMENTS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`demo/make-verify/README.md`](../../demo/make-verify/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/CHEATSHEET.md`](../../docs/CHEATSHEET.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/GRAMMAR.md`](../../docs/GRAMMAR.md) | Normative specification / grammar | Pending |
| [`docs/README.md`](../../docs/README.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/SPEC.md`](../../docs/SPEC.md) | Normative specification / grammar | Pending |
| [`docs/STABILITY.md`](../../docs/STABILITY.md) | Normative specification / grammar | Pending |
| [`docs/TRUST-SPINE.md`](../../docs/TRUST-SPINE.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/adr/0001-deterministic-time-units.md`](../../docs/adr/0001-deterministic-time-units.md) | Decision history — immutable original | Pending |
| [`docs/adr/0002-fallibility-effect-orthogonal.md`](../../docs/adr/0002-fallibility-effect-orthogonal.md) | Decision history — immutable original | Pending |
| [`docs/adr/0003-error-type-conversion-at-propagation.md`](../../docs/adr/0003-error-type-conversion-at-propagation.md) | Decision history — immutable original | Pending |
| [`docs/adr/0004-error-branchability-doctrine.md`](../../docs/adr/0004-error-branchability-doctrine.md) | Decision history — immutable original | Pending |
| [`docs/adr/0005-operators-desugar-to-stdlib.md`](../../docs/adr/0005-operators-desugar-to-stdlib.md) | Decision history — immutable original | Pending |
| [`docs/adr/0006-fallibility-polymorphic-hofs.md`](../../docs/adr/0006-fallibility-polymorphic-hofs.md) | Decision history — immutable original | Pending |
| [`docs/adr/0007-partition-replaces-collect.md`](../../docs/adr/0007-partition-replaces-collect.md) | Decision history — immutable original | Pending |
| [`docs/adr/0008-explicit-propagation-only.md`](../../docs/adr/0008-explicit-propagation-only.md) | Decision history — immutable original | Pending |
| [`docs/adr/0009-fn-type-quadrant-transparency.md`](../../docs/adr/0009-fn-type-quadrant-transparency.md) | Decision history — immutable original | Pending |
| [`docs/adr/0010-option-type-shorthand.md`](../../docs/adr/0010-option-type-shorthand.md) | Decision history — immutable original | Pending |
| [`docs/adr/0011-execution-substrate-is-a-free-variable.md`](../../docs/adr/0011-execution-substrate-is-a-free-variable.md) | Decision history — immutable original | Pending |
| [`docs/adr/0012-typed-error-refinement-in-the-marker.md`](../../docs/adr/0012-typed-error-refinement-in-the-marker.md) | Decision history — immutable original | Pending |
| [`docs/adr/0013-corpus-stays-id-keyed.md`](../../docs/adr/0013-corpus-stays-id-keyed.md) | Decision history — immutable original | Pending |
| [`docs/adr/0013-string-shared-immutable-buffer.md`](../../docs/adr/0013-string-shared-immutable-buffer.md) | Decision history — immutable original | Pending |
| [`docs/adr/0014-expectations-are-derived.md`](../../docs/adr/0014-expectations-are-derived.md) | Decision history — immutable original | Pending |
| [`docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md`](../../docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md) | Decision history — immutable original | Pending |
| [`docs/adr/0016-numeric-determinism-is-normative.md`](../../docs/adr/0016-numeric-determinism-is-normative.md) | Decision history — immutable original | Pending |
| [`docs/adr/0017-flight-profile-is-a-normative-subset.md`](../../docs/adr/0017-flight-profile-is-a-normative-subset.md) | Decision history — immutable original | Pending |
| [`docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md`](../../docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md) | Decision history — immutable original | Pending |
| [`docs/adr/0020-http-serving-and-handler-state.md`](../../docs/adr/0020-http-serving-and-handler-state.md) | Decision history — immutable original | Pending |
| [`docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md`](../../docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) | Decision history — immutable original | Pending |
| [`docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md`](../../docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) | Decision history — immutable original | Pending |
| [`docs/adr/0023-wasm-http-over-wasi-http.md`](../../docs/adr/0023-wasm-http-over-wasi-http.md) | Decision history — immutable original | Pending |
| [`docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md`](../../docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) | Decision history — immutable original | Pending |
| [`docs/adr/0025-wasm-process-is-a-private-host-capability.md`](../../docs/adr/0025-wasm-process-is-a-private-host-capability.md) | Decision history — immutable original | Pending |
| [`docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md`](../../docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md) | Decision history — immutable original | Pending |
| [`docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md`](../../docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) | Decision history — immutable original | Pending |
| [`docs/adr/0028-fan-selection-names-and-cost-contract.md`](../../docs/adr/0028-fan-selection-names-and-cost-contract.md) | Decision history — immutable original | Pending |
| [`docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md`](../../docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md) | Decision history — immutable original | Pending |
| [`docs/adr/0030-compile-cost-is-a-contract.md`](../../docs/adr/0030-compile-cost-is-a-contract.md) | Decision history — immutable original | Pending |
| [`docs/adr/0031-raw-pointers-carry-provenance.md`](../../docs/adr/0031-raw-pointers-carry-provenance.md) | Decision history — immutable original | Pending |
| [`docs/adr/0032-a-response-content-type-is-its-body-type.md`](../../docs/adr/0032-a-response-content-type-is-its-body-type.md) | Decision history — immutable original | Pending |
| [`docs/adr/README.md`](../../docs/adr/README.md) | Decision history — immutable original | Pending |
| [`docs/benchmarks/README.md`](../../docs/benchmarks/README.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/benchmarks/fan-floor.md`](../../docs/benchmarks/fan-floor.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/benchmarks/float-printer.md`](../../docs/benchmarks/float-printer.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/bugfix-research-2026-09-09.md`](../../docs/bugfix-research-2026-09-09.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/bugfix-research-2026-09-12.md`](../../docs/bugfix-research-2026-09-12.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/contracts/.conformance-header.md`](../../docs/contracts/.conformance-header.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/.readme-header.md`](../../docs/contracts/.readme-header.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/C-006-fan-timeout-removed.md`](../../docs/contracts/C-006-fan-timeout-removed.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/C-008-009-010-repr.md`](../../docs/contracts/C-008-009-010-repr.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/C-033-cow-truth-table.md`](../../docs/contracts/C-033-cow-truth-table.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/README.md`](../../docs/contracts/README.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/conformance.md`](../../docs/contracts/conformance.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/contracts/proven-vs-trusted.md`](../../docs/contracts/proven-vs-trusted.md) | Evidence / generated blocks / gate inputs | Pending |
| [`docs/cross-target-decisions.md`](../../docs/cross-target-decisions.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/DESIGN.md`](../../docs/design/DESIGN.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/EQUIVALENCE.md`](../../docs/design/EQUIVALENCE.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/design/HIDDEN_OPERATIONS.md`](../../docs/design/HIDDEN_OPERATIONS.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/design/MEMORY-SAFETY.md`](../../docs/design/MEMORY-SAFETY.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/REJECTED_PATTERNS.md`](../../docs/design/REJECTED_PATTERNS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/fan-cross-language.md`](../../docs/design/fan-cross-language.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/fan-semantics-audit.md`](../../docs/design/fan-semantics-audit.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/design/fan-usability-eval.md`](../../docs/design/fan-usability-eval.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/diagnostics/E001.md`](../../docs/diagnostics/E001.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E002.md`](../../docs/diagnostics/E002.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E003.md`](../../docs/diagnostics/E003.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E004.md`](../../docs/diagnostics/E004.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E005.md`](../../docs/diagnostics/E005.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E006.md`](../../docs/diagnostics/E006.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E007.md`](../../docs/diagnostics/E007.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E008.md`](../../docs/diagnostics/E008.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E009.md`](../../docs/diagnostics/E009.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E010.md`](../../docs/diagnostics/E010.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E011.md`](../../docs/diagnostics/E011.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E012.md`](../../docs/diagnostics/E012.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E013.md`](../../docs/diagnostics/E013.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E014.md`](../../docs/diagnostics/E014.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E015.md`](../../docs/diagnostics/E015.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E016.md`](../../docs/diagnostics/E016.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E017.md`](../../docs/diagnostics/E017.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E018.md`](../../docs/diagnostics/E018.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E019.md`](../../docs/diagnostics/E019.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E020.md`](../../docs/diagnostics/E020.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E021.md`](../../docs/diagnostics/E021.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E022.md`](../../docs/diagnostics/E022.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E023.md`](../../docs/diagnostics/E023.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E024.md`](../../docs/diagnostics/E024.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E025.md`](../../docs/diagnostics/E025.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E026.md`](../../docs/diagnostics/E026.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E027.md`](../../docs/diagnostics/E027.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E028.md`](../../docs/diagnostics/E028.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E029.md`](../../docs/diagnostics/E029.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E030.md`](../../docs/diagnostics/E030.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E031.md`](../../docs/diagnostics/E031.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E032.md`](../../docs/diagnostics/E032.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E033.md`](../../docs/diagnostics/E033.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E034.md`](../../docs/diagnostics/E034.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E035.md`](../../docs/diagnostics/E035.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E036.md`](../../docs/diagnostics/E036.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E037.md`](../../docs/diagnostics/E037.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E038.md`](../../docs/diagnostics/E038.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E041.md`](../../docs/diagnostics/E041.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E042.md`](../../docs/diagnostics/E042.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E043.md`](../../docs/diagnostics/E043.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E044.md`](../../docs/diagnostics/E044.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E045.md`](../../docs/diagnostics/E045.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E046.md`](../../docs/diagnostics/E046.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E047.md`](../../docs/diagnostics/E047.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E048.md`](../../docs/diagnostics/E048.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E049.md`](../../docs/diagnostics/E049.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E050.md`](../../docs/diagnostics/E050.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E051.md`](../../docs/diagnostics/E051.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E052.md`](../../docs/diagnostics/E052.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E053.md`](../../docs/diagnostics/E053.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E054.md`](../../docs/diagnostics/E054.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E055.md`](../../docs/diagnostics/E055.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E056.md`](../../docs/diagnostics/E056.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E057.md`](../../docs/diagnostics/E057.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E058.md`](../../docs/diagnostics/E058.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E059.md`](../../docs/diagnostics/E059.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E060.md`](../../docs/diagnostics/E060.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E061.md`](../../docs/diagnostics/E061.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E062.md`](../../docs/diagnostics/E062.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E070.md`](../../docs/diagnostics/E070.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E071.md`](../../docs/diagnostics/E071.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E072.md`](../../docs/diagnostics/E072.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E073.md`](../../docs/diagnostics/E073.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E074.md`](../../docs/diagnostics/E074.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E075.md`](../../docs/diagnostics/E075.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E076.md`](../../docs/diagnostics/E076.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E077.md`](../../docs/diagnostics/E077.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E078.md`](../../docs/diagnostics/E078.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E080.md`](../../docs/diagnostics/E080.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E081.md`](../../docs/diagnostics/E081.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E082.md`](../../docs/diagnostics/E082.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E083.md`](../../docs/diagnostics/E083.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E084.md`](../../docs/diagnostics/E084.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E085.md`](../../docs/diagnostics/E085.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E086.md`](../../docs/diagnostics/E086.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E087.md`](../../docs/diagnostics/E087.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E088.md`](../../docs/diagnostics/E088.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E089.md`](../../docs/diagnostics/E089.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E090.md`](../../docs/diagnostics/E090.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E092.md`](../../docs/diagnostics/E092.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E093.md`](../../docs/diagnostics/E093.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E094.md`](../../docs/diagnostics/E094.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E095.md`](../../docs/diagnostics/E095.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E096.md`](../../docs/diagnostics/E096.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E097.md`](../../docs/diagnostics/E097.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E098.md`](../../docs/diagnostics/E098.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E099.md`](../../docs/diagnostics/E099.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E100.md`](../../docs/diagnostics/E100.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E101.md`](../../docs/diagnostics/E101.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E102.md`](../../docs/diagnostics/E102.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E103.md`](../../docs/diagnostics/E103.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E104.md`](../../docs/diagnostics/E104.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E109.md`](../../docs/diagnostics/E109.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E110.md`](../../docs/diagnostics/E110.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E111.md`](../../docs/diagnostics/E111.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E112.md`](../../docs/diagnostics/E112.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E113.md`](../../docs/diagnostics/E113.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E114.md`](../../docs/diagnostics/E114.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E116.md`](../../docs/diagnostics/E116.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E117.md`](../../docs/diagnostics/E117.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E118.md`](../../docs/diagnostics/E118.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E119.md`](../../docs/diagnostics/E119.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/E420.md`](../../docs/diagnostics/E420.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/diagnostics/README.md`](../../docs/diagnostics/README.md) | Diagnostic docs — embedded / scanned | Drafted in this PR (separate reader tree) |
| [`docs/fan-capture-ownership-review.md`](../../docs/fan-capture-ownership-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/fan-parallel-safety-review.md`](../../docs/fan-parallel-safety-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/grammar/README.md`](../../docs/grammar/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/how-it-works.md`](../../docs/how-it-works.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/mcp.md`](../../docs/mcp.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/native-projection-ownership-review.md`](../../docs/native-projection-ownership-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/native-read-borrow-review.md`](../../docs/native-read-borrow-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/BENCHMARKS.md`](../../docs/project/BENCHMARKS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/BREAKING_CHANGE_POLICY.md`](../../docs/project/BREAKING_CHANGE_POLICY.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/project/CLAUDE_TEMPLATE.md`](../../docs/project/CLAUDE_TEMPLATE.md) | Agent instructions / embedded template | Pending |
| [`docs/project/GENERATED-RUST-REVIEW.md`](../../docs/project/GENERATED-RUST-REVIEW.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/project/ISSUE-TAXONOMY.md`](../../docs/project/ISSUE-TAXONOMY.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/PARTITIONED-RUNTIME.md`](../../docs/project/PARTITIONED-RUNTIME.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/project/RUST-LINT-GATES.md`](../../docs/project/RUST-LINT-GATES.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/WCET-STORY.md`](../../docs/project/WCET-STORY.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/project/linguist-submission.md`](../../docs/project/linguist-submission.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/project/org-trust-status.md`](../../docs/project/org-trust-status.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/record-option-cells-verification.md`](../../docs/record-option-cells-verification.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/releases/README.md`](../../docs/releases/README.md) | Release history | Pending |
| [`docs/releases/v0.68.0-rc1.md`](../../docs/releases/v0.68.0-rc1.md) | Release history | Pending |
| [`docs/releases/v0.68.0-rc2.md`](../../docs/releases/v0.68.0-rc2.md) | Release history | Pending |
| [`docs/releases/v0.68.0.md`](../../docs/releases/v0.68.0.md) | Release history | Pending |
| [`docs/releases/v0.69.0-rc1.md`](../../docs/releases/v0.69.0-rc1.md) | Release history | Pending |
| [`docs/releases/v0.69.0.md`](../../docs/releases/v0.69.0.md) | Release history | Pending |
| [`docs/roadmap/ALL_AXES.md`](../../docs/roadmap/ALL_AXES.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/GRAND_PLAN.md`](../../docs/roadmap/GRAND_PLAN.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/PRODUCTION_READY.md`](../../docs/roadmap/PRODUCTION_READY.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/README.md`](../../docs/roadmap/README.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/ROAD_TO_1_0.md`](../../docs/roadmap/ROAD_TO_1_0.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/adr-0013-string-sharing.md`](../../docs/roadmap/active/adr-0013-string-sharing.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/almide-perceus-belt.md`](../../docs/roadmap/active/almide-perceus-belt.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/arena-breakthroughs.md`](../../docs/roadmap/active/arena-breakthroughs.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/async-inception.md`](../../docs/roadmap/active/async-inception.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/async-world-claim.md`](../../docs/roadmap/active/async-world-claim.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/behavioral-contract.md`](../../docs/roadmap/active/behavioral-contract.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/build-speed-runtime-rlib.md`](../../docs/roadmap/active/build-speed-runtime-rlib.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/camp4-general-heap-match.md`](../../docs/roadmap/active/camp4-general-heap-match.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/certificate-format-v1.md`](../../docs/roadmap/active/certificate-format-v1.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/certification-grade.md`](../../docs/roadmap/active/certification-grade.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/ci-critical-path.md`](../../docs/roadmap/active/ci-critical-path.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/ci-warnings-cleanup.md`](../../docs/roadmap/active/ci-warnings-cleanup.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/closure-cross-target-completeness.md`](../../docs/roadmap/active/closure-cross-target-completeness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/codegen-traversal-totality.md`](../../docs/roadmap/active/codegen-traversal-totality.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/completeness-by-construction.md`](../../docs/roadmap/active/completeness-by-construction.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/concurrency-stance.md`](../../docs/roadmap/active/concurrency-stance.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/continuous-improvement.md`](../../docs/roadmap/active/continuous-improvement.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/correctness-guarantee-gaps.md`](../../docs/roadmap/active/correctness-guarantee-gaps.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/cross-target-completeness.md`](../../docs/roadmap/active/cross-target-completeness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/determinism-belt.md`](../../docs/roadmap/active/determinism-belt.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/deterministic-bounds.md`](../../docs/roadmap/active/deterministic-bounds.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/edit-locality-theory.md`](../../docs/roadmap/active/edit-locality-theory.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/effect-inspection-harness.md`](../../docs/roadmap/active/effect-inspection-harness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/effect-system-capability.md`](../../docs/roadmap/active/effect-system-capability.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/effectful-27-blueprint.md`](../../docs/roadmap/active/effectful-27-blueprint.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/execution-inception.md`](../../docs/roadmap/active/execution-inception.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/fan-v2-examples.md`](../../docs/roadmap/active/fan-v2-examples.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/fan-v2.md`](../../docs/roadmap/active/fan-v2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-evidence-gaps.md`](../../docs/roadmap/active/flight-evidence-gaps.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-organization.md`](../../docs/roadmap/active/flight-organization.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-profile.md`](../../docs/roadmap/active/flight-profile.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-qualification.md`](../../docs/roadmap/active/flight-qualification.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-reference-app.md`](../../docs/roadmap/active/flight-reference-app.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-rust-ferrocene.md`](../../docs/roadmap/active/flight-rust-ferrocene.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-subset-spec.md`](../../docs/roadmap/active/flight-subset-spec.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/flight-wcet-loops.md`](../../docs/roadmap/active/flight-wcet-loops.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/fuzz-findings-triage.md`](../../docs/roadmap/active/fuzz-findings-triage.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/integer-literal-domain.md`](../../docs/roadmap/active/integer-literal-domain.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/interp-is-desugar-to-tostring.md`](../../docs/roadmap/active/interp-is-desugar-to-tostring.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/issue-inventory-2026-09-21.md`](../../docs/roadmap/active/issue-inventory-2026-09-21.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/issue-inventory-2026-09-21/issue-header-spec.md`](../../docs/roadmap/active/issue-inventory-2026-09-21/issue-header-spec.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/issue-ledger-burndown.md`](../../docs/roadmap/active/issue-ledger-burndown.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/lifted-carrier-abi.md`](../../docs/roadmap/active/lifted-carrier-abi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/llm-first-language.md`](../../docs/roadmap/active/llm-first-language.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/logical-time-async.md`](../../docs/roadmap/active/logical-time-async.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/logical-time-implementation.md`](../../docs/roadmap/active/logical-time-implementation.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/logical-time-proofs.md`](../../docs/roadmap/active/logical-time-proofs.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/map-data-structure-roadmap.md`](../../docs/roadmap/active/map-data-structure-roadmap.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/mir-caps-call-count-breach.md`](../../docs/roadmap/active/mir-caps-call-count-breach.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/mission-critical-attack-list.md`](../../docs/roadmap/active/mission-critical-attack-list.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/native-boxed-pattern-lowering.md`](../../docs/roadmap/active/native-boxed-pattern-lowering.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/native-trust-spine.md`](../../docs/roadmap/active/native-trust-spine.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/net-unix-shm-embedded.md`](../../docs/roadmap/active/net-unix-shm-embedded.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/outside-review-audit-2026-07.md`](../../docs/roadmap/active/outside-review-audit-2026-07.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/playground-revamp.md`](../../docs/roadmap/active/playground-revamp.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/post-0.29-improvement-sweep.md`](../../docs/roadmap/active/post-0.29-improvement-sweep.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/protocol-any-existentials.md`](../../docs/roadmap/active/protocol-any-existentials.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/receipt-logic.md`](../../docs/roadmap/active/receipt-logic.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/reconciliation-followup.md`](../../docs/roadmap/active/reconciliation-followup.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/result-family-from-type.md`](../../docs/roadmap/active/result-family-from-type.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/return-op-eradication.md`](../../docs/roadmap/active/return-op-eradication.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/rot-eradication.md`](../../docs/roadmap/active/rot-eradication.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/security-by-construction.md`](../../docs/roadmap/active/security-by-construction.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/selfhost-link-v2.md`](../../docs/roadmap/active/selfhost-link-v2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/shell-scripting-surface.md`](../../docs/roadmap/active/shell-scripting-surface.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/stdlib-redesign.md`](../../docs/roadmap/active/stdlib-redesign.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/test-surface-25x.md`](../../docs/roadmap/active/test-surface-25x.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/ticks-interface-audit.md`](../../docs/roadmap/active/ticks-interface-audit.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/trust-layer.md`](../../docs/roadmap/active/trust-layer.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/type-where-constraints.md`](../../docs/roadmap/active/type-where-constraints.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v0-unwrap-early-return-leak.md`](../../docs/roadmap/active/v0-unwrap-early-return-leak.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-adt-brick5-goal.md`](../../docs/roadmap/active/v1-adt-brick5-goal.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-adt-value-model.md`](../../docs/roadmap/active/v1-adt-value-model.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-backlog.md`](../../docs/roadmap/active/v1-backlog.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-bolt-backlog.md`](../../docs/roadmap/active/v1-bolt-backlog.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-develop-reflow.md`](../../docs/roadmap/active/v1-develop-reflow.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-heap-result-control-flow.md`](../../docs/roadmap/active/v1-heap-result-control-flow.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-join-completeness.md`](../../docs/roadmap/active/v1-join-completeness.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-kgi-kpi.md`](../../docs/roadmap/active/v1-kgi-kpi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-loop-ownership-cert.md`](../../docs/roadmap/active/v1-loop-ownership-cert.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-mir-architecture.md`](../../docs/roadmap/active/v1-mir-architecture.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-org-byte-verification.md`](../../docs/roadmap/active/v1-org-byte-verification.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-parser-tco-lever.md`](../../docs/roadmap/active/v1-parser-tco-lever.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-phase1-mir-core.md`](../../docs/roadmap/active/v1-phase1-mir-core.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-proof-architecture.md`](../../docs/roadmap/active/v1-proof-architecture.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-proof-spine-progress.md`](../../docs/roadmap/active/v1-proof-spine-progress.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-records-svg.md`](../../docs/roadmap/active/v1-records-svg.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-release-path.md`](../../docs/roadmap/active/v1-release-path.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-selfhost-machinery.md`](../../docs/roadmap/active/v1-selfhost-machinery.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-selfhost-print-floor.md`](../../docs/roadmap/active/v1-selfhost-print-floor.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-spine-correctness-holes.md`](../../docs/roadmap/active/v1-spine-correctness-holes.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-system-map.md`](../../docs/roadmap/active/v1-system-map.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-tco-self-recursion.md`](../../docs/roadmap/active/v1-tco-self-recursion.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-v0-parity.md`](../../docs/roadmap/active/v1-v0-parity.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/v1-value-model.md`](../../docs/roadmap/active/v1-value-model.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/value-rc-cert.md`](../../docs/roadmap/active/value-rc-cert.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/wasm-frees-ownership-discipline.md`](../../docs/roadmap/active/wasm-frees-ownership-discipline.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/wasm-ownership-emit-mechanization.md`](../../docs/roadmap/active/wasm-ownership-emit-mechanization.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/wasm-platform-frontier.md`](../../docs/roadmap/active/wasm-platform-frontier.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/active/zero-committed-shell.md`](../../docs/roadmap/active/zero-committed-shell.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/bundled-almide-dispatch.md`](../../docs/roadmap/done/bundled-almide-dispatch.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/bundled-almide-ideal-form.md`](../../docs/roadmap/done/bundled-almide-ideal-form.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/claim-wording-perceus-byte-identity.md`](../../docs/roadmap/done/claim-wording-perceus-byte-identity.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/closure-architecture-v2.md`](../../docs/roadmap/done/closure-architecture-v2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/closure-codegen-cross-target-gaps.md`](../../docs/roadmap/done/closure-codegen-cross-target-gaps.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/code-health-codopsy.md`](../../docs/roadmap/done/code-health-codopsy.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/codegen-ideal-form.md`](../../docs/roadmap/done/codegen-ideal-form.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/effect-system-phase1-2.md`](../../docs/roadmap/done/effect-system-phase1-2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/hkt-foundation-phase1.md`](../../docs/roadmap/done/hkt-foundation-phase1.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/playground-repair.md`](../../docs/roadmap/done/playground-repair.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/reimpl-lint.md`](../../docs/roadmap/done/reimpl-lint.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/stdlib-declarative-unification.md`](../../docs/roadmap/done/stdlib-declarative-unification.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/variant-exhaustiveness-refinement.md`](../../docs/roadmap/done/variant-exhaustiveness-refinement.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/wasm-engine-redesign.md`](../../docs/roadmap/done/wasm-engine-redesign.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/done/wasm-optimization-roadmap.md`](../../docs/roadmap/done/wasm-optimization-roadmap.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/almide-to-almide-ffi.md`](../../docs/roadmap/on-hold/almide-to-almide-ffi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/almide-ui.md`](../../docs/roadmap/on-hold/almide-ui.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/api-diff-auto-versioning.md`](../../docs/roadmap/on-hold/api-diff-auto-versioning.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/benchmark-next-phase.md`](../../docs/roadmap/on-hold/benchmark-next-phase.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/compile-time-contracts.md`](../../docs/roadmap/on-hold/compile-time-contracts.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/diagnostic-snippet-externalization.md`](../../docs/roadmap/on-hold/diagnostic-snippet-externalization.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/flow-design.md`](../../docs/roadmap/on-hold/flow-design.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/flow-spec-draft.md`](../../docs/roadmap/on-hold/flow-spec-draft.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/gpu-compute.md`](../../docs/roadmap/on-hold/gpu-compute.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/ir-optimization-tier2.md`](../../docs/roadmap/on-hold/ir-optimization-tier2.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/law-blocks-experiment.md`](../../docs/roadmap/on-hold/law-blocks-experiment.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/law-proofs-design.md`](../../docs/roadmap/on-hold/law-proofs-design.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/llm-integration.md`](../../docs/roadmap/on-hold/llm-integration.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/lsp-code-actions.md`](../../docs/roadmap/on-hold/lsp-code-actions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/ml-inference.md`](../../docs/roadmap/on-hold/ml-inference.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/mlir-backend-adoption.md`](../../docs/roadmap/on-hold/mlir-backend-adoption.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/package-registry.md`](../../docs/roadmap/on-hold/package-registry.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/package-version-resolution.md`](../../docs/roadmap/on-hold/package-version-resolution.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/performance-research.md`](../../docs/roadmap/on-hold/performance-research.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/porta-embedded.md`](../../docs/roadmap/on-hold/porta-embedded.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/process-composition.md`](../../docs/roadmap/on-hold/process-composition.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/rainbow-bridge.md`](../../docs/roadmap/on-hold/rainbow-bridge.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/region-inference.md`](../../docs/roadmap/on-hold/region-inference.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/research-modification-survival-rate-paper.md`](../../docs/roadmap/on-hold/research-modification-survival-rate-paper.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/rumbling.md`](../../docs/roadmap/on-hold/rumbling.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/secure-by-design.md`](../../docs/roadmap/on-hold/secure-by-design.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/shell-completions.md`](../../docs/roadmap/on-hold/shell-completions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/snapshot-testing.md`](../../docs/roadmap/on-hold/snapshot-testing.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/supervision-and-actors.md`](../../docs/roadmap/on-hold/supervision-and-actors.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/wasm-exception-handling.md`](../../docs/roadmap/on-hold/wasm-exception-handling.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/wasm-http-client.md`](../../docs/roadmap/on-hold/wasm-http-client.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/on-hold/web-framework.md`](../../docs/roadmap/on-hold/web-framework.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L0-conventions.md`](../../docs/roadmap/stdlib-redesign/L0-conventions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L1-categories.md`](../../docs/roadmap/stdlib-redesign/L1-categories.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-args.md`](../../docs/roadmap/stdlib-redesign/L2-args.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-base64.md`](../../docs/roadmap/stdlib-redesign/L2-base64.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-bigint.md`](../../docs/roadmap/stdlib-redesign/L2-bigint.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-bool.md`](../../docs/roadmap/stdlib-redesign/L2-bool.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-bytes.md`](../../docs/roadmap/stdlib-redesign/L2-bytes.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-compute.md`](../../docs/roadmap/stdlib-redesign/L2-compute.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-datetime.md`](../../docs/roadmap/stdlib-redesign/L2-datetime.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-deque.md`](../../docs/roadmap/stdlib-redesign/L2-deque.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-duration.md`](../../docs/roadmap/stdlib-redesign/L2-duration.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-env.md`](../../docs/roadmap/stdlib-redesign/L2-env.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-error.md`](../../docs/roadmap/stdlib-redesign/L2-error.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-ffi.md`](../../docs/roadmap/stdlib-redesign/L2-ffi.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-float.md`](../../docs/roadmap/stdlib-redesign/L2-float.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-fs.md`](../../docs/roadmap/stdlib-redesign/L2-fs.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-hash.md`](../../docs/roadmap/stdlib-redesign/L2-hash.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-heap.md`](../../docs/roadmap/stdlib-redesign/L2-heap.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-hex.md`](../../docs/roadmap/stdlib-redesign/L2-hex.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-html.md`](../../docs/roadmap/stdlib-redesign/L2-html.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-http.md`](../../docs/roadmap/stdlib-redesign/L2-http.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-int.md`](../../docs/roadmap/stdlib-redesign/L2-int.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-io.md`](../../docs/roadmap/stdlib-redesign/L2-io.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-json.md`](../../docs/roadmap/stdlib-redesign/L2-json.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-list.md`](../../docs/roadmap/stdlib-redesign/L2-list.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-map.md`](../../docs/roadmap/stdlib-redesign/L2-map.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-math.md`](../../docs/roadmap/stdlib-redesign/L2-math.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-matrix.md`](../../docs/roadmap/stdlib-redesign/L2-matrix.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-net.md`](../../docs/roadmap/stdlib-redesign/L2-net.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-option.md`](../../docs/roadmap/stdlib-redesign/L2-option.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-path.md`](../../docs/roadmap/stdlib-redesign/L2-path.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-process.md`](../../docs/roadmap/stdlib-redesign/L2-process.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-random.md`](../../docs/roadmap/stdlib-redesign/L2-random.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-regex.md`](../../docs/roadmap/stdlib-redesign/L2-regex.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-result.md`](../../docs/roadmap/stdlib-redesign/L2-result.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-secret.md`](../../docs/roadmap/stdlib-redesign/L2-secret.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-set.md`](../../docs/roadmap/stdlib-redesign/L2-set.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-sized-numbers.md`](../../docs/roadmap/stdlib-redesign/L2-sized-numbers.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-string.md`](../../docs/roadmap/stdlib-redesign/L2-string.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-testing.md`](../../docs/roadmap/stdlib-redesign/L2-testing.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-unix.md`](../../docs/roadmap/stdlib-redesign/L2-unix.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-url.md`](../../docs/roadmap/stdlib-redesign/L2-url.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-uuid.md`](../../docs/roadmap/stdlib-redesign/L2-uuid.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-value.md`](../../docs/roadmap/stdlib-redesign/L2-value.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/L2-zlib.md`](../../docs/roadmap/stdlib-redesign/L2-zlib.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/README.md`](../../docs/roadmap/stdlib-redesign/README.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/facts/categories.md`](../../docs/roadmap/stdlib-redesign/facts/categories.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/facts/naming-conventions.md`](../../docs/roadmap/stdlib-redesign/facts/naming-conventions.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/protocol-view.md`](../../docs/roadmap/stdlib-redesign/protocol-view.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/roadmap/stdlib-redesign/target-surface.md`](../../docs/roadmap/stdlib-redesign/target-surface.md) | Roadmap — scanned tree / generated index | Pending |
| [`docs/specs/CLAUDE.md`](../../docs/specs/CLAUDE.md) | Agent instructions / embedded template | Pending |
| [`docs/specs/als/README.md`](../../docs/specs/als/README.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/STANDARD.md`](../../docs/specs/als/STANDARD.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/bounded.md`](../../docs/specs/als/bounded.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/collections.md`](../../docs/specs/als/collections.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/data-formats.md`](../../docs/specs/als/data-formats.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/deterministic-time.md`](../../docs/specs/als/deterministic-time.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/expressions.md`](../../docs/specs/als/expressions.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/implementation.md`](../../docs/specs/als/implementation.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/runtime.md`](../../docs/specs/als/runtime.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/semantics.md`](../../docs/specs/als/semantics.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/strings.md`](../../docs/specs/als/strings.md) | Normative specification / grammar | Pending |
| [`docs/specs/als/text-and-numbers.md`](../../docs/specs/als/text-and-numbers.md) | Normative specification / grammar | Pending |
| [`docs/specs/cli.md`](../../docs/specs/cli.md) | Normative specification / grammar | Pending |
| [`docs/specs/codegen.md`](../../docs/specs/codegen.md) | Normative specification / grammar | Pending |
| [`docs/specs/edit-locality.md`](../../docs/specs/edit-locality.md) | Normative specification / grammar | Pending |
| [`docs/specs/effect-fn-call-semantics.md`](../../docs/specs/effect-fn-call-semantics.md) | Normative specification / grammar | Pending |
| [`docs/specs/effect-system.md`](../../docs/specs/effect-system.md) | Normative specification / grammar | Pending |
| [`docs/specs/language.md`](../../docs/specs/language.md) | Normative specification / grammar | Pending |
| [`docs/specs/module-system.md`](../../docs/specs/module-system.md) | Normative specification / grammar | Pending |
| [`docs/specs/package-system.md`](../../docs/specs/package-system.md) | Normative specification / grammar | Pending |
| [`docs/specs/perceus.md`](../../docs/specs/perceus.md) | Normative specification / grammar | Pending |
| [`docs/specs/result-option-effect.md`](../../docs/specs/result-option-effect.md) | Normative specification / grammar | Pending |
| [`docs/specs/scoped.md`](../../docs/specs/scoped.md) | Normative specification / grammar | Pending |
| [`docs/specs/type-system.md`](../../docs/specs/type-system.md) | Normative specification / grammar | Pending |
| [`docs/status.md`](../../docs/status.md) | Reader guides — translated batches | Drafted in this PR |
| [`docs/stdlib/README.md`](../../docs/stdlib/README.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/args.md`](../../docs/stdlib/args.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/base64.md`](../../docs/stdlib/base64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/bigint.md`](../../docs/stdlib/bigint.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/bool.md`](../../docs/stdlib/bool.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/bytes.md`](../../docs/stdlib/bytes.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/compute.md`](../../docs/stdlib/compute.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/datetime.md`](../../docs/stdlib/datetime.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/deque.md`](../../docs/stdlib/deque.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/duration.md`](../../docs/stdlib/duration.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/env.md`](../../docs/stdlib/env.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/ffi.md`](../../docs/stdlib/ffi.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/float.md`](../../docs/stdlib/float.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/float32.md`](../../docs/stdlib/float32.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/float64.md`](../../docs/stdlib/float64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/fs.md`](../../docs/stdlib/fs.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/hash.md`](../../docs/stdlib/hash.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/heap.md`](../../docs/stdlib/heap.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/hex.md`](../../docs/stdlib/hex.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/html.md`](../../docs/stdlib/html.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/http.md`](../../docs/stdlib/http.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int.md`](../../docs/stdlib/int.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int16.md`](../../docs/stdlib/int16.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int32.md`](../../docs/stdlib/int32.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int64.md`](../../docs/stdlib/int64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/int8.md`](../../docs/stdlib/int8.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/io.md`](../../docs/stdlib/io.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/json.md`](../../docs/stdlib/json.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/list.md`](../../docs/stdlib/list.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/map.md`](../../docs/stdlib/map.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/math.md`](../../docs/stdlib/math.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/matrix.md`](../../docs/stdlib/matrix.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/net.md`](../../docs/stdlib/net.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/option.md`](../../docs/stdlib/option.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/path.md`](../../docs/stdlib/path.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/process.md`](../../docs/stdlib/process.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/random.md`](../../docs/stdlib/random.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/regex.md`](../../docs/stdlib/regex.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/result.md`](../../docs/stdlib/result.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/secret.md`](../../docs/stdlib/secret.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/set.md`](../../docs/stdlib/set.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/string.md`](../../docs/stdlib/string.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/testing.md`](../../docs/stdlib/testing.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint16.md`](../../docs/stdlib/uint16.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint32.md`](../../docs/stdlib/uint32.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint64.md`](../../docs/stdlib/uint64.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uint8.md`](../../docs/stdlib/uint8.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/unix.md`](../../docs/stdlib/unix.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/url.md`](../../docs/stdlib/url.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/uuid.md`](../../docs/stdlib/uuid.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/value.md`](../../docs/stdlib/value.md) | Standard library — generated sections / gates | Pending |
| [`docs/stdlib/zlib.md`](../../docs/stdlib/zlib.md) | Standard library — generated sections / gates | Pending |
| [`docs/stream-fusion-safety-review.md`](../../docs/stream-fusion-safety-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm-decoder-reclaim-review.md`](../../docs/wasm-decoder-reclaim-review.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/JS-HOST-ASYNC-IMPORTS.md`](../../docs/wasm/JS-HOST-ASYNC-IMPORTS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/README.md`](../../docs/wasm/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/WASM-OUTPUT.md`](../../docs/wasm/WASM-OUTPUT.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/wasm/capability-system.md`](../../docs/wasm/capability-system.md) | Handwritten documentation — consumer audit pending | Pending |
| [`docs/written-in-almide.md`](../../docs/written-in-almide.md) | Reader guides — translated batches | Drafted in this PR |
| [`examples/almd-outline/README.md`](../../examples/almd-outline/README.md) | Reader guides — translated batches | Drafted in this PR |
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
| [`research/benchmark/perf/README.md`](../../research/benchmark/perf/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/hot-issues-2026-09-10/README.md`](../../research/benchmark/perf/hot-issues-2026-09-10/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md`](../../research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/spectralnorm/README.md`](../../research/benchmark/perf/spectralnorm/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/perf/string-gap-1004.md`](../../research/benchmark/perf/string-gap-1004.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/stdlib/HANDOFF.md`](../../research/benchmark/stdlib/HANDOFF.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/benchmark/three-way/README.md`](../../research/benchmark/three-way/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/beyond-rust/README.md`](../../research/beyond-rust/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/fan-audit/probes/README.md`](../../research/fan-audit/probes/README.md) | Reader guides — translated batches | Drafted in this PR |
| [`research/grammar-lab/LESSONS.md`](../../research/grammar-lab/LESSONS.md) | Handwritten documentation — consumer audit pending | Pending |
| [`research/grammar-lab/README.md`](../../research/grammar-lab/README.md) | Reader guides — translated batches | Drafted in this PR |
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
| [`runtime/rs/README.md`](../../runtime/rs/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`spec/churn/README.md`](../../spec/churn/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`spec/gauntlet/README.md`](../../spec/gauntlet/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`stdlib/README.md`](../../stdlib/README.md) | Handwritten documentation — consumer audit pending | Pending |
| [`tools/claude-plugin/README.md`](../../tools/claude-plugin/README.md) | Reader guides — translated batches | Drafted in this PR |
| [`tools/xtarget-fuzz/README.md`](../../tools/xtarget-fuzz/README.md) | Handwritten documentation — consumer audit pending | Pending |
