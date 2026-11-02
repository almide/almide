# Translation phase 2 — reader snapshot inventory

> Historical batch record: counts and placement descriptions below describe this phase at creation, not the current layout or publication state. The subsequent sibling migration uses `reference/<full-source-path-stem>.md` (English), `.ja.md` (Japanese), and `.zh-CN.md` (Simplified Chinese); `.en.md` and language-directory reader routing are retired. Canonical originals remain unchanged. Table destinations and labels below use the migrated paths; source-language snapshot copies added by migration are not additional translated originals. Current cumulative coverage is tracked in [TRANSLATION-INVENTORY](../project/TRANSLATION-INVENTORY.md).

Source commit: `35297a612fb29e3f78f6587aba253aa91d51f466`. This additive batch follows PR #4405; it does not change the original documentation, compiler, registries, generators, or CI.

- Completed in this batch: **24 source documents / 48 translated files**.
- Placement: `docs/translations/<language>/repository/<original-path>` for handwritten guides, and separate `stdlib/` reader snapshots for API documentation.
- Source language is assessed per page: English sources remain the English destination; Japanese sources remain the Japanese destination. Original/source links and language links are distinct.
- Standard-library signatures, types, since metadata, comments and runnable fences retain their original bytes. API explanation translations sit outside preserved fences. Generated-marker claims are removed: these translations are not generated or synchronized by the original generator.
- Snapshot prose and measurements are not newly verified claims. File-specific source caveats identify conflicting or historical wording rather than guessing implementation behavior.
- Code examples were not independently run in this translation task. Byte equality is checked; current source consumers and CI remain necessary.

## Validation and review

Mechanical checks cover pinned original hashes, code fences (including indentation), multiline inline code, relative links and original-file preservation. Existing contract-citation and diagnostic-code-coverage gates are run unchanged. Independent meaning review covers selected risk-heavy groups; it is not a new compiler audit. Details and all source caveats are retained in the handoff manifests.

## Files

| Original | Language | Reader translation |
|---|---|---|
| [`crates/almide-egg-lab/README.md`](../../crates/almide-egg-lab/README.md) | ja | [docs/translations/reference/crates/almide-egg-lab/README.ja.md](reference/crates/almide-egg-lab/README.ja.md) |
| [`crates/almide-egg-lab/README.md`](../../crates/almide-egg-lab/README.md) | zh-CN | [docs/translations/reference/crates/almide-egg-lab/README.zh-CN.md](reference/crates/almide-egg-lab/README.zh-CN.md) |
| [`crates/almide-kernel/README.md`](../../crates/almide-kernel/README.md) | ja | [docs/translations/reference/crates/almide-kernel/README.ja.md](reference/crates/almide-kernel/README.ja.md) |
| [`crates/almide-kernel/README.md`](../../crates/almide-kernel/README.md) | zh-CN | [docs/translations/reference/crates/almide-kernel/README.zh-CN.md](reference/crates/almide-kernel/README.zh-CN.md) |
| [`demo/make-verify/README.md`](../../demo/make-verify/README.md) | ja | [docs/translations/reference/demo/make-verify/README.ja.md](reference/demo/make-verify/README.ja.md) |
| [`demo/make-verify/README.md`](../../demo/make-verify/README.md) | zh-CN | [docs/translations/reference/demo/make-verify/README.zh-CN.md](reference/demo/make-verify/README.zh-CN.md) |
| [`docs/cross-target-decisions.md`](../cross-target-decisions.md) | ja | [docs/translations/reference/docs/cross-target-decisions.ja.md](reference/docs/cross-target-decisions.ja.md) |
| [`docs/cross-target-decisions.md`](../cross-target-decisions.md) | zh-CN | [docs/translations/reference/docs/cross-target-decisions.zh-CN.md](reference/docs/cross-target-decisions.zh-CN.md) |
| [`docs/design/DESIGN.md`](../design/DESIGN.md) | ja | [docs/translations/reference/docs/design/DESIGN.ja.md](reference/docs/design/DESIGN.ja.md) |
| [`docs/design/DESIGN.md`](../design/DESIGN.md) | zh-CN | [docs/translations/reference/docs/design/DESIGN.zh-CN.md](reference/docs/design/DESIGN.zh-CN.md) |
| [`docs/design/REJECTED_PATTERNS.md`](../design/REJECTED_PATTERNS.md) | en | [docs/translations/reference/docs/design/REJECTED_PATTERNS.md](reference/docs/design/REJECTED_PATTERNS.md) |
| [`docs/design/REJECTED_PATTERNS.md`](../design/REJECTED_PATTERNS.md) | zh-CN | [docs/translations/reference/docs/design/REJECTED_PATTERNS.zh-CN.md](reference/docs/design/REJECTED_PATTERNS.zh-CN.md) |
| [`docs/grammar/README.md`](../grammar/README.md) | ja | [docs/translations/reference/docs/grammar/README.ja.md](reference/docs/grammar/README.ja.md) |
| [`docs/grammar/README.md`](../grammar/README.md) | zh-CN | [docs/translations/reference/docs/grammar/README.zh-CN.md](reference/docs/grammar/README.zh-CN.md) |
| [`docs/project/ISSUE-TAXONOMY.md`](../project/ISSUE-TAXONOMY.md) | ja | [docs/translations/reference/docs/project/ISSUE-TAXONOMY.ja.md](reference/docs/project/ISSUE-TAXONOMY.ja.md) |
| [`docs/project/ISSUE-TAXONOMY.md`](../project/ISSUE-TAXONOMY.md) | zh-CN | [docs/translations/reference/docs/project/ISSUE-TAXONOMY.zh-CN.md](reference/docs/project/ISSUE-TAXONOMY.zh-CN.md) |
| [`docs/project/RUST-LINT-GATES.md`](../project/RUST-LINT-GATES.md) | ja | [docs/translations/reference/docs/project/RUST-LINT-GATES.ja.md](reference/docs/project/RUST-LINT-GATES.ja.md) |
| [`docs/project/RUST-LINT-GATES.md`](../project/RUST-LINT-GATES.md) | zh-CN | [docs/translations/reference/docs/project/RUST-LINT-GATES.zh-CN.md](reference/docs/project/RUST-LINT-GATES.zh-CN.md) |
| [`docs/project/linguist-submission.md`](../project/linguist-submission.md) | en | [docs/translations/reference/docs/project/linguist-submission.md](reference/docs/project/linguist-submission.md) |
| [`docs/project/linguist-submission.md`](../project/linguist-submission.md) | zh-CN | [docs/translations/reference/docs/project/linguist-submission.zh-CN.md](reference/docs/project/linguist-submission.zh-CN.md) |
| [`docs/stdlib/args.md`](../stdlib/args.md) | ja | [docs/translations/reference/docs/stdlib/args.ja.md](reference/docs/stdlib/args.ja.md) |
| [`docs/stdlib/args.md`](../stdlib/args.md) | zh-CN | [docs/translations/reference/docs/stdlib/args.zh-CN.md](reference/docs/stdlib/args.zh-CN.md) |
| [`docs/stdlib/bool.md`](../stdlib/bool.md) | ja | [docs/translations/reference/docs/stdlib/bool.ja.md](reference/docs/stdlib/bool.ja.md) |
| [`docs/stdlib/bool.md`](../stdlib/bool.md) | zh-CN | [docs/translations/reference/docs/stdlib/bool.zh-CN.md](reference/docs/stdlib/bool.zh-CN.md) |
| [`docs/stdlib/duration.md`](../stdlib/duration.md) | ja | [docs/translations/reference/docs/stdlib/duration.ja.md](reference/docs/stdlib/duration.ja.md) |
| [`docs/stdlib/duration.md`](../stdlib/duration.md) | zh-CN | [docs/translations/reference/docs/stdlib/duration.zh-CN.md](reference/docs/stdlib/duration.zh-CN.md) |
| [`docs/stdlib/env.md`](../stdlib/env.md) | ja | [docs/translations/reference/docs/stdlib/env.ja.md](reference/docs/stdlib/env.ja.md) |
| [`docs/stdlib/env.md`](../stdlib/env.md) | zh-CN | [docs/translations/reference/docs/stdlib/env.zh-CN.md](reference/docs/stdlib/env.zh-CN.md) |
| [`docs/stdlib/io.md`](../stdlib/io.md) | ja | [docs/translations/reference/docs/stdlib/io.ja.md](reference/docs/stdlib/io.ja.md) |
| [`docs/stdlib/io.md`](../stdlib/io.md) | zh-CN | [docs/translations/reference/docs/stdlib/io.zh-CN.md](reference/docs/stdlib/io.zh-CN.md) |
| [`docs/stdlib/json.md`](../stdlib/json.md) | ja | [docs/translations/reference/docs/stdlib/json.ja.md](reference/docs/stdlib/json.ja.md) |
| [`docs/stdlib/json.md`](../stdlib/json.md) | zh-CN | [docs/translations/reference/docs/stdlib/json.zh-CN.md](reference/docs/stdlib/json.zh-CN.md) |
| [`docs/stdlib/path.md`](../stdlib/path.md) | ja | [docs/translations/reference/docs/stdlib/path.ja.md](reference/docs/stdlib/path.ja.md) |
| [`docs/stdlib/path.md`](../stdlib/path.md) | zh-CN | [docs/translations/reference/docs/stdlib/path.zh-CN.md](reference/docs/stdlib/path.zh-CN.md) |
| [`docs/stdlib/url.md`](../stdlib/url.md) | ja | [docs/translations/reference/docs/stdlib/url.ja.md](reference/docs/stdlib/url.ja.md) |
| [`docs/stdlib/url.md`](../stdlib/url.md) | zh-CN | [docs/translations/reference/docs/stdlib/url.zh-CN.md](reference/docs/stdlib/url.zh-CN.md) |
| [`research/benchmark/perf/README.md`](../../research/benchmark/perf/README.md) | ja | [docs/translations/reference/research/benchmark/perf/README.ja.md](reference/research/benchmark/perf/README.ja.md) |
| [`research/benchmark/perf/README.md`](../../research/benchmark/perf/README.md) | zh-CN | [docs/translations/reference/research/benchmark/perf/README.zh-CN.md](reference/research/benchmark/perf/README.zh-CN.md) |
| [`runtime/rs/README.md`](../../runtime/rs/README.md) | ja | [docs/translations/reference/runtime/rs/README.ja.md](reference/runtime/rs/README.ja.md) |
| [`runtime/rs/README.md`](../../runtime/rs/README.md) | zh-CN | [docs/translations/reference/runtime/rs/README.zh-CN.md](reference/runtime/rs/README.zh-CN.md) |
| [`spec/churn/README.md`](../../spec/churn/README.md) | ja | [docs/translations/reference/spec/churn/README.ja.md](reference/spec/churn/README.ja.md) |
| [`spec/churn/README.md`](../../spec/churn/README.md) | zh-CN | [docs/translations/reference/spec/churn/README.zh-CN.md](reference/spec/churn/README.zh-CN.md) |
| [`spec/gauntlet/README.md`](../../spec/gauntlet/README.md) | ja | [docs/translations/reference/spec/gauntlet/README.ja.md](reference/spec/gauntlet/README.ja.md) |
| [`spec/gauntlet/README.md`](../../spec/gauntlet/README.md) | zh-CN | [docs/translations/reference/spec/gauntlet/README.zh-CN.md](reference/spec/gauntlet/README.zh-CN.md) |
| [`stdlib/README.md`](../../stdlib/README.md) | ja | [docs/translations/reference/stdlib/README.ja.md](reference/stdlib/README.ja.md) |
| [`stdlib/README.md`](../../stdlib/README.md) | zh-CN | [docs/translations/reference/stdlib/README.zh-CN.md](reference/stdlib/README.zh-CN.md) |
| [`tools/xtarget-fuzz/README.md`](../../tools/xtarget-fuzz/README.md) | ja | [docs/translations/reference/tools/xtarget-fuzz/README.ja.md](reference/tools/xtarget-fuzz/README.ja.md) |
| [`tools/xtarget-fuzz/README.md`](../../tools/xtarget-fuzz/README.md) | zh-CN | [docs/translations/reference/tools/xtarget-fuzz/README.zh-CN.md](reference/tools/xtarget-fuzz/README.zh-CN.md) |
