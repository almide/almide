# Changelog

All notable changes to this project are documented here. This file was
started at `0.14.6-phase2`; earlier versions are summarized retrospectively
under "Before this file existed" at the bottom.

Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
each entry groups by diagnostic-/tooling-/language-/stdlib-facing intent
because that's what downstream consumers (LLM harnesses, editors, users)
care about.

## [0.14.6-phase2] — Unreleased (develop/llm-first-phase2 branch)

Phase 2 of the "LLM-first language" roadmap. **Focus**: make the compiler
produce copy-pasteable fix snippets in diagnostics, so LLM retries converge
faster. Measured against [almide-dojo] 30-task benchmark.

### LLM writability (dojo MSR, 2026-04-16)

| Model | v0.14.5 baseline | 0.14.6-phase2 | Δ |
|---|---|---|---|
| Sonnet 4.6 | — | **30/30 (100%)** | — |
| llama-3.3-70b | 17/30 (57%) | **23/30 (77%)** | **+6 (+20pt)** |
| llama-3.1-8b | 13/30 (43%) | 10/30 (33%) | -3 (within noise band, σ≈2) |

Sonnet 30/30 validates the core design: no language concessions (UFCS,
imperative loops) were required to hit SOTA. See
[docs/roadmap/active/llm-first-language.md](docs/roadmap/active/llm-first-language.md)
for the full analysis.

### Added — diagnostics

Every entry below attaches a `try:` block with copy-pasteable code:

- **E001 fn-body Unit-leak**: extract the trailing `let`/`var` name from the
  AST so the snippet reads `let <real_name> = ... / <real_name>` instead of
  a `<computation>` placeholder.
- **E001 if-arm Unit-leak**: when one arm is a bare assignment, the snippet
  cites the real variable (`let new_val = if cond then … else val`).
- **E002 method-call syntax**: `x.to_upper()` → `string.to_upper(x)` — snippet
  emitted only when a fuzzy suggestion is available (not on blind misses).
- **E002 undefined function**: fuzzy-match suggestion + clean-name fix in the
  snippet. Free-text aliases (e.g. `xs + [x]`) correctly suppress the snippet
  to avoid pasting a non-call blob into a call position.
- **E003 undefined variable**: suggests `import json` for import-suggestable
  stdlib modules, or a fuzzy rename otherwise.
- **E004 arg-count mismatch**: snippet shows the full signature with
  `<name: Type>` placeholders (`string.join(<list: List[String]>, <sep: String>)`).
- **E009 let-reassign**: suggests `var <name> = ...` with the real binding name.
- **Hallucination-specific snippets**:
  - `int.sqrt(n)` → convert-sqrt-convert (`float.sqrt(int.to_float(n))`).
  - `int.gt(a, b)` / `int.lt` / `int.gte` / `int.lte` / `int.eq` / `int.neq`
    (and the float/string/bool variants) → operator mapping table.
- **Misplaced `test` keyword**: hint identifies both possible causes (prior
  decl unclosed, OR harness-submitted code shouldn't contain tests).
- **Rest/cons patterns**: `[head, ...tail]` / `[h, ..t]` / `head :: tail`
  emit a targeted hint pointing to `list.first` / `list.drop` recursion —
  the only idiomatic shape in Almide.

### Added — tooling

- **`almide ide outline <file|@stdlib/<module>>`** — one-line-per-decl summary
  (fn / type / let). Targets replace `grep` for LLM API discovery.
- **`almide ide doc <symbol> [--file <f>]`** — signature + docstring for a
  stdlib or user symbol. `string.to_upper` / `greet` work uniformly.
- **`almide ide stdlib-snapshot [--modules m1,m2,...] [--json]`** —
  concatenated text/JSON of core stdlib outlines. One subprocess instead
  of N. Designed for harnesses to embed in SYSTEM_PROMPT; measured at
  ~3.5K tokens text (14.5K JSON) for the default 7 modules.
- **`--json` flag** on `almide ide outline` for dashboard/automation use.
- **Snapshot tests** locking the format of `almide ide outline @stdlib/*`
  and `stdlib-snapshot` output so downstream harness SYSTEM_PROMPT embeds
  don't silently rot when stdlib changes.

### Fixed — parser

- **let-in detection across newlines**: `let x = expr\n  in <body>`
  now triggers the OCaml/Haskell hint instead of cascading into
  "Expected expression (got In 'in')". The partial `Stmt::Let` is
  preserved in the AST so downstream diagnostics (E001) can still cite
  the real binding name.
- **rustc error-code leak wrapping**: 4-digit `error[E\d{4}]` codes from
  rustc (which Almide doesn't emit — Almide tops out at 3-digit E001..E099)
  are now always wrapped in the bug-report banner, even when the output
  doesn't mention `src/main.rs`. Prevents harness classifiers from
  mistaking codegen bugs for user-facing language errors.

### Changed

- `almide ide outline`'s `--filter` now matches substrings (was documented
  as "prefix", but the implementation was always substring — documentation
  now matches behavior).

### Deferred (evidence-based)

- `almide ide peek-def` / `find-refs`: dojo context doesn't exercise
  "inspect existing body" workflows, so no MSR uplift expected. Revisit
  when the task bank grows refactor-style tasks.
- UFCS adoption: dojo data showed UFCS is a weak-model-only win
  (8b parse-err 9 vs 70b parse-err 0) — and Sonnet 30/30 on current
  syntax proves strong models don't need it. `Path A` on
  [docs/roadmap/active/llm-first-language.md](docs/roadmap/active/llm-first-language.md)
  is now formally declined.

---

## Before this file existed

- **v0.14.5** — baseline for the LLM-first roadmap (dojo 70b 17/30, 8b 13/30).
- **v0.14.x and earlier** — see git history; release notes for tagged
  versions live on GitHub Releases.

[almide-dojo]: https://github.com/almide/almide-dojo
