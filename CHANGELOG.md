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
- **`while cond do ... done`** (Pascal/Ruby/OCaml loop form) detection
  now emits a richer `try:` snippet offering BOTH the recursion form
  (preferred for pure/effect fn) AND the Almide `while cond { ... }`
  form (for `var` accumulators). Motivated by dojo binary-search /
  matrix-ops fails where `do ... done` was consistently the first
  attempt, and the recursion form wins on retry.

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

### Added — Phase 3 MVP

- **`almide ide doc`** accepts `@stdlib/<module>.<fn>` prefix as an alias for
  `<module>.<fn>`, for ergonomic symmetry with `almide ide outline @stdlib/<module>`.
- **`almide fix` exit code**: returns 0 when the file is clean after
  auto-fixes, 1 when `manual_pending` is non-empty. Harnesses can gate
  retry on the exit code without parsing output.
- **Diagnostic explain docs enriched** (`docs/diagnostics/E001.md`
  through `E013.md` — full set): each now includes the actual `try:`
  snippet shape the compiler emits, conversion tables for common type
  mismatches, and cross-references to `llms.txt` / `CHEATSHEET.md`.
  Several docs had mismatched content (E010 described "scope error"
  but the code means **non-exhaustive match**; E011 described
  "init issue" but means **mutable var in closure in pure fn**);
  those are now corrected. E012 (duplicate definition) and E013
  (field access on non-record) added — both were emitted by the
  checker but undocumented.
- **`almide fix <file> [--dry-run] [--json]`** — applies `auto_imports`
  (adds missing `import json` / `import fs` / etc), removes OCaml-style
  `let x = expr in <body>` keywords (the body stays), rewrites
  comparison-function calls (`int.gt(n, 0)` → `n > 0`, same for lt /
  gte / lte / eq / neq on int / float / string / bool), **removes
  `return` keywords** (Go/Rust/JS habit — Almide uses trailing
  expression; iterates to fixpoint for multiple occurrences), and
  reports any remaining diagnostics that carry `try:` snippets as
  manual-fix pointers. `--json` emits a stable-schema report
  (`schema_version`, imports_added, letin_removed, operator_rewrites,
  **return_removed**, manual_pending, changed, dry_run) for LLM
  harness retry-loop integration. Cons-pattern rewrite is still manual
  (needs AST-level pattern transformation + parser recovery of the
  dropped body).
- **`list.binary_search(List[Int], Int) -> Option[Int]`** — sorted-list
  binary search. Dojo binary-search task was previously a 70b fail; this
  reduces it to an API call.
- **`string.run_length_encode(String) -> List[(String, Int)]`** — RLE pairs.
  Same motivation.
- **`llms.txt`** at repo root — mission, CLI reference, core idioms, stdlib
  pointer, diagnostic codes, anti-patterns. 1-URL fetch point for LLM tools.

### Known gaps (documented, not blockers)

- Extending existing `list.*` / `string.*` modules via `stdlib/<m>.almd`
  bundled source doesn't work today: `list.*` lowering is hardcoded to emit
  `almide_rt_list_<fn>` regardless of whether the fn came from TOML or Almide
  source. Workaround: add new fns as TOML + runtime (`stdlib/defs/<m>.toml`
  + `runtime/rs/src/<m>.rs`). Full Almide-source dispatch is Phase 3-2.2.
- `almide fix` does not yet mechanically apply `let-in` / `head :: tail` /
  operator-style rewrites — the try: snippets are shown but require manual
  edits. Phase 3-1.2.
- `llms.txt` is hand-written; not yet auto-generated from canonical docs
  (SPEC / cheatsheet). Phase 3-3.2.

### Added — `almide docs-gen --check` (doc-drift guard)

A consistency check that verifies `llms.txt` and `docs/diagnostics/`
track their canonical sources. MVP covers four axes:

- **Version**: `Cargo.toml` version string must appear in `llms.txt`.
- **Diagnostic codes referenced in llms.txt**: every `EXXX` file
  under `docs/diagnostics/` must be named in `llms.txt`.
- **Auto-imported stdlib**: every module in
  `almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED` must be mentioned
  in `llms.txt`'s "Fast facts".
- **Diagnostic registry bijection**: every `with_code("EXXX")` in the
  compiler source must have a matching `docs/diagnostics/EXXX.md`, and
  every doc must correspond to a code that's actually emitted.

Exits 1 with a bulleted drift report on failure. `cargo test`
integration test `docs_gen_check_passes_on_clean_checkout` makes
every PR that changes a source-of-truth but forgets the docs fail CI.

Full generation (not just drift-check) is scoped in
`docs/roadmap/active/llms-txt-autogen.md`; registry-vs-emit
unification strategy in `docs/roadmap/active/diagnostic-emit-doc-unification.md`.

Real drifts found & fixed on first run:
- `E010-E013` range row in `llms.txt` didn't contain `E011` / `E012`
  as substrings (range-compression masquerading as content). Expanded.
- `E420` (function visibility violation) was emitted by the compiler
  but had no doc. Added `docs/diagnostics/E420.md`, noted that the
  code number is out-of-sequence and a renumber candidate for a
  future release.

### Internal refactors (no behavior change, no MSR effect)

- **`almide fix` keyword-removal rules** consolidated into a single
  `KeywordRemoval { keyword, diag_matches, max_iter }` engine. `let-in`
  and `return` rules are now data-driven; adding a third keyword
  deletion rule is one const entry plus a call site. `word_boundary_ok`
  extracted from the two previous copies.
- **Comparison operator table** (`int.gt` / `.lt` / `.eq` / etc. → `>` /
  `<` / `==`) consolidated behind `almide::stdlib::comparison_operator_of`.
  Previously the same mapping was duplicated across `suggest_alias`,
  `try_snippet_for_alias`, and `cli/fix.rs::comparison_fn_to_operator`;
  now each derives from the single canonical function. As a side
  effect, `string.eq` and `bool.eq` now get the "Did you mean" hint
  (previously missing from `suggest_alias` — a gap that surfaced when
  consolidating).
- **while-do `try:` snippet** shortened from the previous Option A/B
  block (≈15 lines) to 7 lines: one concise `while`-form + one
  `fn loop` recursion scaffold. Feedback from the reflection: the
  longer form risked paradox-of-choice and bloated retry context.

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
