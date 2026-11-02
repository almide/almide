Original description metadata: Plan to make Almide the language LLMs write most accurately, measured by dojo MSR
<a id="llm-first-language"></a>

# LLM-first Language

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](llm-first-language.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](llm-first-language.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](llm-first-language.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md)

> Source caveat: this preserves April 2026 design history, not current benchmark results. The stated mission and model-superiority claims are goals or historical interpretations, not independently established rankings. A finite 30-task 100% result does not establish defect absence, causal attribution to model ability, a true-value interval or sufficient retries for other tasks; the source itself withdraws those generalizations. UFCS and bundled-source corrections supersede old rejection/backlog text. Historical scoring/task-bank changes, proposed CI gates and llms generation must not be inferred to be current or executed. Thresholds are retained as written, including the ambiguous negative-threshold token and differing historical UFCS criteria.


> **Scope update**: The phases and MSR below are design and measurement records from 2026-04. UFCS is now implemented (see method-call syntax in SPEC); deciding whether to adopt it is not pending. Bundled stdlib also uses `.almd` as its current source; do not treat old TOML-routing failures as current blockers. Remaining tooling / parser / MSR improvement proposals need individual validation. Without remeasurement in the external Dojo, do not carry the scores or achievement of exit targets below forward to the current version.

Almide's **mission**: “The language LLMs can write most accurately.” This roadmap collects design decisions along 5 axes and their staged implementation order. **By design, almide-dojo MSR delta informs decisions** (not a claim that every PR was actually measured) — the criterion for adding a language feature is “does retry-success rise?”

<a id="design-rule"></a>

## Design rule

> Do not adopt syntax that is beautiful for humans but a trap for LLMs.
>
> Measure a feature's value by **MSR delta** (dojo retry-success changes for 70b / 8b / Sonnet). Reject a beautiful feature that lowers MSR by 3pt or more; reconsider it even if already present.

<a id="5-軸"></a>

## 5 axes

<a id="axis-1--間違える機会を減らす"></a>

### Axis 1 — Reduce opportunities for mistakes
- **UFCS** (automatic resolution of `n.abs()` → `int.abs(n)`): the biggest bet, automatically resolving 30-40% of method-call + list-field issues. Treated as a separate proposal (below).
- **Auto-import top 30 stdlib**: expand current Tier 1, reducing E002-family errors. Trade-off with side effects (import transparency).
- **One canonical form**: documentation / lint level. No language change.

<a id="axis-2--パーサが救う"></a>

### Axis 2 — Let the parser rescue mistakes
- **Error-recovering parser**: one missing `)` must not cause a 30-line cascade. Tree-sitter-like panic-mode.
- **Cascade suppression** (implemented ahead in v0.14.5 ✓): suppress `undefined function` beneath parse errors. Room to extend toward type errors.

<a id="axis-3--エラーが自己修復可能"></a>

### Axis 3 — Errors support self-repair
- **`try:` code snippet** (Elm style): include pasteable repair code in errors, not just hint text.
- **`almide fix`**: if try snippets are machine-applicable, offer a `cargo fix` equivalent. The compiler performs the LLM's retry.
- **Single source of truth for cheat-sheet**: auto-generate `SYSTEM_PROMPT` / `CHEATSHEET.md` / `llms.txt` from `SPEC.md`, so dojo, the main project and external tools see the same wording.

<a id="axis-4--modification-を安全に-msr-の-m"></a>

### Axis 4 — Make modification safe (the “M” in MSR)
- **Block boundary marker** (like MoonBit's `///|`): modifying one function does not break another. Touches the parser.
- **Per-fn dependency declarations**: `fn name(...) uses [list, int]` — make LLM local edits safe. ROI unclear; defer.
- **Long migration through `#deprecated` + `#alias`**: language evolution does not become catastrophic.

<a id="axis-5--msr-を-first-class-metric-に"></a>

### Axis 5 — Make MSR a first-class metric
- **Embed dojo in main-project CI**: each PR computes MSR delta; auto-block a regression of `-2pt 以上` (interpreted here as a decline of 2pt or more; the source token is ambiguous; initially trial warning-only operation).
- **MSR scores in release notes**: every release takes responsibility for retry-success.
- **Decision axis for new features**: do not adopt a feature that is “beautiful but MSR -3pt.”

<a id="当初の実装-phase-順2026-04-の履歴現在の-backlog-ではない"></a>

## Original implementation phase order (2026-04 history, not the current backlog)

MoonBit's lesson: **an “LLM-friendly language” is not enough; an “LLM-friendly development environment”** is at least as effective. Almide likewise builds tooling before extending the language.

<a id="phase-1-完了"></a>

### Phase 1 (complete)
- ✅ `try:` snippet + Diagnostic extension (f87b7d0b)
- ✅ Explicit DESIGN rule (9a583e72)
- ✅ Roadmap organization

<a id="phase-2--tooling-moonbit-inspired-実質完了-2026-04-16"></a>

### Phase 2 — Tooling (MoonBit-inspired, substantially complete 2026-04-16)
2-1. ✅ **`almide ide outline <file>`** — list package pub fn / type / let one per line. Eliminate grep and E002/E003 hallucinations.
2-2. ✅ **`almide ide doc <symbol> [--file <f>]`** — return stdlib / user fn signature + docstring. No `grep` needed to find `string.to_upper`.
2-3. **`almide ide peek-def <symbol>`** — return only the definition snippet (including body). **On hold** — dojo contexts do not require peeking at existing bodies, so MSR contribution is small. Reconsider when refactoring tasks enter the task bank.
2-4. **`almide ide find-refs <symbol>`** — reference list. **On hold** — same reason.
2-5. **Include AGENTS.md in `almide new`** — distribute dojo SYSTEM_PROMPT to every project.
2-6. **Runnable `*.almd.md` cheat-sheet** — guarantee passage through `almide check *.almd.md`. Specifications do not drift.

<a id="phase-2-実測-try-snippet--hint-改善による-msr-押上げ-2026-04-16"></a>

### Phase 2 measurements: MSR improvement from try: snippets + better hints (2026-04-16)

Starting from baseline (v0.14.5 = before phase 2), implemented the following on llm-first-phase2 and measured continuously with dojo:

**Diagnostic improvements implemented**:
- `almide ide outline / doc / stdlib-snapshot` + `@stdlib/<module>` + `--json`
- Add try: snippets to 6 paths for E002/E003/E004/E009 and 2 paths for E001 Unit-leak (fn body / if arm)
- Specialize fn-body Unit-leak: extract the trailing `let` name from AST and propose a concrete rewrite
- Specialize if-arm Unit-leak: extract the assignment target of the Unit-returning arm, guiding toward `let new_x = if ...`
- Extend let-in detection across newlines; preserve partial Let after Err to relay into downstream diagnostics
- Rich conversion / operator snippets for hallucinated int.sqrt / int.gt etc.
- Harness-context hint for a misplaced `test` keyword
- Hints toward list.first/drop for rest/cons patterns (`[h, ...t]` / `head :: tail`)
- Wrap leaked four-digit rustc E-codes (also enclose rustc errors without a `src/main.rs` path mention in a bug banner)

**Numbers (Δ = baseline at phase 2 start → final)**:

| Release / Version | Model | retry-success | 1-shot | Δ baseline |
|---|---|---|---|---|
| v0.14.5 baseline | llama-3.3-70b | 17/30 (57%) | — | — |
| v0.14.5 baseline | llama-3.1-8b | 13/30 (43%) | — | — |
| 0.14.6 (14d1a973) | llama-3.3-70b | **23/30 (77%)** | 12/30 | **+6 (+20pt)** |
| 0.14.6 (14d1a973) | llama-3.1-8b | 10/30 (33%) | 10/30 | -3 (variance) |
| 0.14.6 (14d1a973) | **Sonnet 4.6** | **30/30 (100%)** | 26/30 (87%) | — |

**Strategic judgment at the time (the following generalizations are unverified)**:
These 30 tasks do not prove absence of defects in the whole language, that model capability alone caused failures, a true-value interval, or a future retry budget. Preserve the original judgment as history.
- Sonnet 30/30 = **Almide's language design has no structural defects**. The remaining 7 failures for 70b are purely model-capability limits (ADT / state tracking / complex accumulators).
- **Path A (UFCS / imperative-loop concessions) is conclusively unnecessary** — strong models complete the current design. There is no reason to contaminate the language.
- 70b 1-shot 12/30 vs Sonnet 26/30 = pure LLM-capability difference rather than pre-retry prompt quality. Limited room to narrow it by tweaking SYSTEM_PROMPT.
- 8b variance should be absorbed with n=3. `12±2` is the true-value range.

**Phase 2 completion criteria**: ✅ at least baseline +5pt for 70b, and 90%+ with a strong model (SOTA equivalent), while maintaining the DESIGN rule.

<a id="phase-3--fix-tooling--stdlib-expansion-mvp-着地-2026-04-16"></a>

### Phase 3 — Fix tooling + stdlib expansion (MVP landed 2026-04-16)
3-1. ✅ **`almide fix` MVP** — auto-apply auto-import (E003 “Add `import json`”, etc.) + report remaining `try:` snippets as manual fixes. Supports `--dry-run` preview. Extend AST-level machine rewrites such as `let-in` → newline chain / `head :: tail` → list.first-drop in Phase 3-1.2.
3-2. ✅ **Stdlib primitives**: added `list.binary_search(List[Int], Int) -> Option[Int]` and `string.run_length_encode(String) -> List[(String, Int)]` — turn dojo binary-search / run-length-encoding tasks from algorithm synthesis into API calls. Decide on other candidates (`list.window`, `list.partition`, `list.chunks`) after dojo remeasurement.
3-3. ✅ **llms.txt** — at repository root, about ~5K lines collecting mission/CLI/core idioms/error codes/stdlib pointers, fetchable by LLM tools through one URL. Updated manually (automatic generation from SPEC.md requires a fmt-idempotent converter, deferred to Phase 3-3.2).

<a id="phase-3-の残タスク"></a>

#### Remaining Phase 3 tasks
- **3-1.2 AST-level rewrites in `almide fix`** — ✅ let-in (source-level), ✅ operator-form (`int.gt(a, b)` → `a > b`, AST-level Call → Binary). Only cons patterns (`head :: tail` → list.first/drop) remain, requiring parser recovery to preserve trailing bodies.
- **3-2.2 Stdlib bundled-Almide dispatch** — change codegen so Almide-source extensions such as `stdlib/list.almd` work as the `list.*` module. TOML modules currently route rigidly to `almide_rt_list_<fn>`, so adding functions in `stdlib/list.almd` causes a codegen bug rather than an `E002` equivalent. Found through dogfooding (see examples/almd-outline README).
- **3-3.2 Auto-generate llms.txt** — tooling like `almide docs-gen` to regenerate llms.txt with SPEC.md / cheatsheet / stdlib-snapshot as source of truth.

<a id="phase-4--language-level-changes-tooling-で不足分を見てから"></a>

### Phase 4 — Language-level changes (after seeing what tooling leaves unmet)
4-1. **UFCS adoption decision**: informed by phase 2–3 MSR gains. GO if dojo 8b still has many parse errors. Assume tooling delivers 70b gains.
4-2. **Strengthen error-recovering parser**: extend cascade suppression toward type errors.
4-3. **Strengthen the `?` operator / Option chains** — address 9 type errors in 70b.

<a id="phase-5-長期"></a>

### Phase 5 (long term)
5-1. **Embed dojo in CI**: automatically compute PR-level MSR delta.
5-2. **`///|` block boundary** (MoonBit style): guarantee local refactoring through syntax.
5-3. **Skill marketplace / playbook**: make playbooks for refactor / bug-fix / new-feature modes into plugins.

<a id="当初の-ufcs-proposal当時の保留理由現在は実装済み"></a>

## Original UFCS proposal (reasons for holding then; now implemented)

- **Current state (v0.14.3+)**: `n.to_uppercase()` emits error + hint `Write 'string.to_upper(x)'`. v0.14.4 dojo data shows retry-success ±0.
- **Issue**: error improvement has a ceiling. UFCS changes language semantics to “accept the mistaken form as correct,” an effect on a different dimension.
- **Cost**: parser interprets `.method()` as UFCS sugar; checker dispatches modules. Estimated 5-10 iterations.
- **Risk**: LLM training data could disperse across 3 forms (`int.abs(n)`, `n.abs()`, `n |> int.abs`) and fail to converge. Abandoning One canonical form.
- **Decision criterion**: phase 1–2 MSR less than +10pt → seek the remainder through UFCS. More than +10pt → reject UFCS, retain canonical form.

<a id="計測"></a>

## Measurement

- **Dojo runs**: after each release, run 30 tasks at T=0 three times for 70b / 8b / (optional) Sonnet; use the median.
- **MSR delta table** (for release notes):
  | Release | Sonnet 4.6 | 70b | 8b | Notes |
  |---|---|---|---|---|
  | v0.14.5 | — | 17/30 | 13/30 | retro — baseline for llm-first roadmap |
  | 0.14.6 | **30/30** | **23/30** | 10/30 | Phase 2 complete (2026-04-16). Sonnet 30/30 achieved validation of the language design. |
- **Retry budget**: dojo data confirmed `pass-3 = 0 across all runs`. `max_retries=3` is wasteful; **2 suffice**. Use the leftover budget for two passes through the tasks to remove variance.

<a id="dojo-data-から当時推測した構造-2026-04-15"></a>

## Structure inferred from dojo data at the time (2026-04-15)

“Always,” “guarantee” and “evidence” below describe observations / interpretations of the task bank then, not guarantees for new tasks, models or the current version.

<a id="両モデル必ず通る-12-タスク--sweet-spot"></a>

### 12 tasks “both models always pass” = sweet spot
`clamp`, `fizzbuzz`, `factorial`, `flatten-nested`, `repeat-string`, etc. — single function / primitive I/O / shallow recursion. 100% pass. **The baseline guarantee line for an MSR-first language.**

<a id="両モデル必ず落ちる-11-タスク--設計の中心課題"></a>

### 11 tasks “both models always fail” = central design challenge
Concentrated in 3 feature groups:
- **ADT / sum types**: `anagram-check`, `custom-linked-list`, `mini-json-query`
- **Stateful loops** (low/high pointers, etc.): `binary-search`, `matrix-ops`
- **String-algorithm pipelines**: `balanced-parens`, `caesar-cipher`, `roman-numeral`, `run-length-encoding`, `string-reverse`, `result-pipeline`

→ **Do not discard features.** Add more “common pattern as a function” to stdlib so LLMs compose without writing algorithms:
- `list.binary_search`, `string.run_length_encode`, `list.window`, `list.partition`, etc.
- Decision to add a new phase: **Phase 1.5: stdlib pattern expansion**.

<a id="8b--70b-逆転--one-canonical-form-の証拠"></a>

### 8b > 70b reversal = evidence for one canonical form
8b passes `max-of-list` and `sum-digits`, while 70b fails. Being smarter leads 70b to overproduce `xs.head` method chains / `let rec`. **Writable by a small model = a large model also does not overproduce on its own.** Elm's “only one way” aligns with MSR-first.

<a id="parse-error-vs-type-error-の-model-size-依存"></a>

### Model-size dependence of parse-error vs type-error
| | parse | type |
|---|---|---|
| 8b | 9 | 3 |
| 70b | 0 | 9 |

→ **For small models**: syntax forgiveness (UFCS, auto-import, optional parentheses?) helps.
→ **For large models**: better type inference, Option/Result chains (`?` operator) help.
→ Build both in separate phases.

<a id="当時の-ufcs-再考-2026-04-data-を見た後"></a>

## UFCS reconsidered at the time (after seeing 2026-04 data)

- 8b 9 parse errors vs 70b 0 = **UFCS helps 8b strongly, almost irrelevant for 70b**.
- 70b 9 type errors = stronger type inference / Option chains help.
- Conclusion: UFCS is valuable for raising 8b's floor. Breaking 70b's ceiling needs another approach (type-inference improvements, `?` chains).
- **Decision**: after phase 1–2, consider UFCS if 8b MSR rises less than +5pt. Treat 70b gains as a separate type-inference phase.

<a id="非目標-明示"></a>

## Explicit non-goals

- Pursuing Haskell-class type-system beauty (Linear / Dependent / etc.).
- Purity branding such as “Almide is pure FP.”
- An Elm-strict “no method syntax” worldview (discard if adopting UFCS).
- “Full-stack framework” lock-in.
