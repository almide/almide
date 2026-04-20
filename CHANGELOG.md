# Changelog

All notable changes to this project are documented here. This file was
started at `0.14.6`; earlier versions are summarized retrospectively
under "Before this file existed" at the bottom.

Format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
each entry groups by diagnostic-/tooling-/language-/stdlib-facing intent
because that's what downstream consumers (LLM harnesses, editors, users)
care about.

## [Unreleased]

## [0.15.0] — 2026-04-20

Cross-module `let` reaches Swift/TypeScript parity, WASM dispatch gaps
are closed (both existing bugs and the last three matrix-op blockers),
and a new CI gate prevents future dispatch drift. Changes are additive
— no breaking API or surface removals — but the module-system surface
change is large enough to cross a minor boundary.

### Cross-module `let` now works for every type

Module-scoped `let` bindings (`let FOO = ... ` in `util.almd`,
`util.FOO` in `main.almd`) previously broke in several shapes:

- Scalar consts (`let MAGIC: Int = 42`) compiled as
  `(*ALMIDE_RT_UTIL_MAGIC)` where the generated Rust was
  `const ALMIDE_RT_UTIL_MAGIC: i64 = 42i64;` — `i64` is not a reference,
  rustc rejected the deref.
- Compound references (`let RES: Result[Int, String] = ok(7)`) lost the
  ascription during module lowering. The generated Rust became
  `LazyLock<Result<i64, _>>`, which rustc rejects under `E0121`.
  Even after ascription restoration, `*LAZY_VAR` moves out of the
  deref; consuming callers (`result.unwrap_or((*LAZY_VAR), ...)`) hit
  `E0507` because the clone-insertion pass missed the synthetic
  cross-module Var.
- Ascription-less literals (`let ANON = { a: 1, b: "hi" }` /
  `let MAP = ["a": 1]`) type-checked to `Unknown` at the use site in
  `main`, producing `LazyLock<_>` (rustc) or `ConcretizeTypes`
  post-condition violation (WASM).

Four independent fixes, landing in sequence:

- `walker/expressions.rs` — the synthetic `ALMIDE_RT_<MOD>_<NAME>` Var
  is no longer assumed to be `LazyLock`-backed. `CodegenAnnotations`
  gains `lazy_top_let_names: HashSet<String>`, pre-indexed from every
  module's `TopLetKind::Lazy` entries; scalar `Const` top_lets emit as
  bare names.
- `frontend/src/lower/mod.rs` — the `TopLet` branch's type lookup falls
  back `{prefixed, unprefixed, expr_ty}` so module-scoped bindings find
  their registration-seeded ascribed type instead of silently
  re-inferring a generic-less `Result[Int, ?]`.
- `pass_clone.rs::split_clone_ids` — names prefixed with `ALMIDE_RT_`
  are now always-clone; synthetic cross-module Vars materialize via
  `.clone()` and never move out of the LazyLock.
- `canonicalize/registration.rs::infer_literal_type` now recurses
  through `Record` / `List` / `Tuple` / `MapLiteral` / `Some` / `None`
  / `Ok` / `Err` / `Paren`, returning concrete types at registration
  time. `Checker` also gains `current_module_prefix`, which
  `infer_module` sets so the `TopLet` write-back hits both prefixed
  (`util.ANON`) and unprefixed (`ANON`) keys regardless of
  module check order.

All eleven tested categories work on both Rust and WASM: Int, Float,
Bool, String, List, Map, Set, Tuple, Option, Result, nominal record,
anonymous record, variant — with and without ascription.

### WASM dispatch hardening

- **Mono `Discover` module-name guard** — when the same fn name lived
  in multiple modules (`option.filter[A]`, `list.filter[A]`,
  `result.filter[A, E]`), the first name match silently stole the
  specialization, registering it under the wrong `(module, fn, suffix)`
  key. The `Rewriter` then missed its lookup and the call site stayed
  as `Module { m, f }`, triggering a WASM ICE. `Discover` now filters
  by module name the same way `Rewriter` does. `stdlib/result.almd`'s
  `filter_with(r, pred, () -> E)` thunk workaround is reverted to the
  spec-shape `filter[A, E](r, pred, err_val: E)`.
- **`emit_<module>_call` Named dispatch fallback** — the Module arms in
  `emit_wasm/calls.rs` now try `func_map["almide_rt_<m>_<f>"]` before
  panicking. Mirrors the unknown-module `_ =>` arm's fallback chain.
  Applied to `int / float / string / option / result / list / bytes /
  matrix / base64 / hex / map / math / set` (12 modules).
- **`error.message` / `error.context` ICE** — `dispatch_runtime_fallback`
  gains an `"error"` arm that re-enters `emit_call` via
  `CallTarget::Module`. The inline emit exists on the Module side;
  the RuntimeCall path just needed a redirect.

### Matrix WASM ops

Three functions that were Rust-runtime-only and caused
`[ICE] emit_wasm: RuntimeCall almide_rt_matrix_<op>` on the WASM
target now have inline WASM emit arms:

- `rms_norm_rows(m, gamma, eps)` — layer-norm variant without mean/beta.
- `attention_weights(q, kt, scale)` — fused `softmax_rows(scale * q×kt)`.
  Phase-1 scratch locals (`q`, `kt`, `scale`, `k`, `acc`, `inner`) are
  freed between phases so Phase 2 (`row_off`, `maxv`, `sumv`, `v`)
  reuses the same WASM local slots.
- `swiglu_gate(x, w_gate, w_up)` — `out[i,j] = g · σ(g) · u` with both
  dot products sharing the same `k` loop.

The `ScratchAllocator` per-function i32 cap is raised from 32 → 48 so
nested matrix pipelines (SDPA, Llama 1-block) fit.

### New CI gate: WASM dispatch coverage

`tests/wasm_dispatch_coverage_test.rs` drives
`almide test spec/stdlib/ --target wasm`, parses the summary, and
asserts `failed == 0 && skipped <= SKIP_BASELINE`. Baseline is 3 — only
legitimate `// wasm:skip` files (native-only `process` APIs,
`testing.assert_throws` which needs panic catching).

### Numbers

| Surface | Before | After |
|---|---|---|
| WASM spec | 214 passed / 7 skipped | **218 passed / 3 skipped** |
| Rust spec | 221/221 | 221/221 (unchanged) |
| Cross-module `let` | scalar-only, broken for refs | full parity with Swift/TS |
| ICE hits in `stdlib/` WASM sweep | 4 | **0** |

### S2 flip — ConcretizeTypes postcondition is now a hard contract

`expr.ty` is trustworthy by contract. The Pipeline harness
(`crates/almide-codegen/src/pass.rs`) no longer reads `ALMIDE_CHECK_IR` /
`ALMIDE_VERIFY_IR`; inter-pass IR verification and every pass's
`Postcondition` list run on every build. Debug builds panic on
violation (so CI and local `cargo test` never ship a program with an
unresolved type); release builds print the same
`[POSTCONDITION VIOLATION]` / `[IR CHECK]` diagnostic to stderr and keep
compiling, so an end-user `almide build` does not crash on a compiler
bug.

After this flip, downstream passes (closure conversion, WASM emit,
stdlib dispatch, `resolve_list_elem`, etc.) may rely on non-Unknown
`IrExpr.ty` unconditionally. Defensive "if the type is still
Unknown, try emit-time fallback X" shims added under v0.14.6–0.14.7
become deletable as a follow-up.

Residual WASM-target lifted-lambda TypeVars produced by
`ClosureConversion` remain as a pre-existing pass-boundary issue
tracked in S3 (`pass_resolve_calls` Phase 1b-c); they do not surface
under the default spec/ sweep.

Removed surface:
- `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` env vars (no replacement —
  contract always enforced).

### S3 Phase 1d — delete `emit_stub_call` / `emit_stub_call_named` helpers

The two helpers sitting in `emit_wasm/calls.rs` existed only to `panic!`
with an `[ICE]` message when a stdlib dispatcher's `_` arm was reached.
Every call site has been replaced with an inline
`panic!("[ICE] emit_wasm: no WASM dispatch for `<m>.<func>` — ...")`
carrying the specific module / dispatcher name so the diagnostic points
straight at the missing arm. With no remaining callers, the helpers
themselves are deleted — `calls.rs`, `calls_io.rs`, `calls_value.rs`,
`calls_http.rs`, `calls_datetime.rs`, `calls_process.rs`, `calls_fs.rs`,
`calls_env.rs`, `calls_random.rs`, `calls_regex.rs`,
`calls_list_helpers.rs` all go through inline panics now.

Behavior is unchanged: reaching a dispatcher fallback was already a
compile-time ICE under S2 flip. The cleanup removes one layer of
indirection and surfaces the correct module / dispatcher in the error
message.

## [0.14.8] — 2026-04-17

Hotfix for a v0.14.7 regression: external package dispatch (e.g. `almai`
loaded from `[dependencies]` in `almide.toml`) failed to link because the
Phase 1b `ResolveCallsPass` rewrote every `CallTarget::Module` — including
user-package modules — to a non-versioned mangled name. The walker then
emitted those functions under their versioned identifier
(`almide_rt_<pkg>_v<major>_<fn>`) while the call sites referenced the
unversioned form (`almide_rt_<pkg>_<fn>`), producing Rust E0425 at
compile time.

### Fix

`ResolveCallsPass` now gates its Module → Named rewrite on
`stdlib_info::is_any_stdlib(m)`. Only bundled-Almide stdlib modules
(`args`, `path`, `list`, …) — which never carry a `versioned_name` — are
rewritten. User-package calls stay as `CallTarget::Module` and continue
to flow through `StdlibLoweringPass::rewrite_module_names` (Rust) or the
WASM `func_map` bare-name fallback, both of which know how to look up
the versioned symbol.

No surface changes. `almide test` (206 files) and `cargo test` pass;
downstream `almide-dojo` (13 tests using `almai.defaults` /
`almai.with_system` / `almai.call_with`) compiles and runs again.

## [0.14.7] — 2026-04-17

Phase 3 "Ideal Form Migration" arc. Six ship points (`-phase3.1`
through `-phase3.5` plus the interim B fix-up) are merged into this
release. Combined goal: drive every patch-layer special case in the
bundled-Almide / codegen dispatch to zero. After this release, every
`CallTarget::Module` either resolves to a TOML stdlib fn (per-target
inline emit) or is rewritten to `Named` (bundled-Almide path);
unresolved stdlib calls are compile-time ICE; monomorphization drops
every generic source post-pass; the audit catches residue with a
fn-locator. See `docs/roadmap/done/bundled-almide-ideal-form.md` and
`docs/roadmap/active/codegen-ideal-form.md §Phase 3 Arc` for the
plan/closure narrative.

### Patch layer status at release

- bundled `option.almd` / `result.almd` (signature override): **gone** (S1)
- WASM `func_map` per-module fallback for bundled fns: **gone** (A)
- mono `is_bundled_module` filter at prune step: **gone** (S4)
- `monomorphize_module_fns` early-return that skipped the prune: **gone** (B)
- `emit_stub_call*` runtime traps: **gone** (S3, now compile-time ICE)

### S1 — Option/Result signature normalization

Removed the bundled `stdlib/option.almd` / `stdlib/result.almd` that
silently overrode TOML signatures for `option.unwrap_or_else` and
`option.or_else`. The root cause — TOML declared `Fn[Unit] -> X` while
callers write `() => x` — is fixed at the source: TOML now uses
`Fn[] -> X`, and the `stdlib_codegen.rs` TOML parser handles the empty
params case.

Surface changes:

- `stdlib/defs/option.toml` `unwrap_or_else.f` / `or_else.f`: `Fn[Unit] -> X` → `Fn[] -> X`
- `stdlib/option.almd` / `stdlib/result.almd`: deleted (no longer needed
  for signature override; runtime dispatch was always TOML-backed)
- `BUNDLED_MODULES` / `AUTO_IMPORT_BUNDLED` / `get_bundled_source`:
  `option` / `result` entries removed. Tier-1 auto-import continues via
  `import_table.rs`'s hardcoded list.

No caller-visible breakage: `option.or_else(o, () => ...)` now type-checks
directly against the TOML signature instead of going through the bundled
override. `spec/stdlib/coverage_misc_test.almd` (the gatekeeper for this
co-dependence) passes unchanged.

### S2 — ConcretizeTypes audit always-on; bundled-stdlib generic cleanup

`ConcretizeTypesPass::postconditions` no longer gates the audit on
`ALMIDE_AUDIT_TYPES=1`; the `Custom(audit_remaining_unresolved)` check
runs on every build. Violations print as
`[POSTCONDITION VIOLATION] [ConcretizeTypes] N expressions remain ...`
and escalate to `panic!` under `ALMIDE_CHECK_IR=1`.

`spec/` is clean on the Rust target with `ALMIDE_CHECK_IR=1`. WASM
target on `ALMIDE_CHECK_IR=1` still trips on lifted-lambda TypeVar
residue produced by `ClosureConversion`; closing that gap is S3 work.
Default behavior (no `ALMIDE_CHECK_IR`) is unchanged — both targets
pass spec/ as before.

Bundled-stdlib mono cleanup: `monomorphize_module_fns` previously kept
unused generic source fns inside `program.modules`, which reached the
WASM emitter with TypeVars intact. Now drops every generic fn in
`is_bundled_module(name)` after the specialization round — specialized
instances live alongside in `module.functions`, the generic source is
no longer needed.

### S3 — WASM `emit_stub_call*` panics at compile time

`emit_stub_call_named` and `emit_stub_call` no longer drop arguments and
emit a runtime `unreachable` instruction. They now `panic!()` with a
`[ICE]` prefix — reaching either at WASM emission time means a
`module.func` call survived `pass_resolve_calls` without a TOML or
bundled IR target, which is a compiler bug to fix at the resolver, not a
runtime trap to debug.

The `ALMIDE_WASM_STUB_PANIC` / `ALMIDE_WASM_STUB_VERBOSE` /
`ALMIDE_WASM_STUB_TRACE` env vars are removed; v0.14.6's stub-panic
sweep already proved spec/ + nn never reach the stub. Phase 1c of the
codegen-ideal-form roadmap is closed by this step.

### S4 — Mono drops generic source fns from every module, not just bundled

`monomorphize_module_fns` already discovered and specialized generic
fns across every module in `program.modules` (the bundled-only filter
introduced in v0.14.6 was applied only at the post-specialize prune
step). The prune is now uniform across all modules: every generic
source fn is dropped after the specialization round, not only those
inside `is_bundled_module(...)`. User package modules carrying generic
fns (e.g. `pkg.helper[T](x: T) -> List[T]`) get the same post-mono
invariant as bundled stdlib modules.

### B — Mono prune always runs; ConcretizeTypes audit is more locatable

`monomorphize_module_fns` previously early-returned when no generic
specialization was discovered. The post-loop prune was therefore
skipped in the very case where it matters most: a program that imports
a bundled stdlib module but never calls any of its generic fns. The
unused generic source survived to codegen, carried `TypeVar(T)` in its
body, and tripped the `ConcretizeTypes` audit on WASM. Fix: the prune
always runs; only the rewrite loop (no-op when `rename` is empty) is
conditionally skipped.

`audit_remaining_unresolved` (the `Custom` postcondition) now reports
each violating expression's enclosing fn name + a short `kind` label
("[list::iterate] List ty=...") instead of opaque
`Discriminant(NN)` numbers.

After this fix, spec/ on WASM with `ALMIDE_CHECK_IR=1` is **191/206
passing, 15 skipped**. The remaining 15 are independent type-inference
gaps (empty-list `Applied(List, [Unknown])`, OpenRecord propagation,
codec-derived list fields, generic chain-b argument, etc.) tracked in
`codegen-ideal-form.md §#4`. Default behavior (no `ALMIDE_CHECK_IR`)
is unchanged — both targets pass spec/ as before.

### A — Phase 1b: ResolveCalls rewrites bundled stdlib calls to `Named`

`pass_resolve_calls.rs` is no longer verification-only. For every
`CallTarget::Module { module, func }` it now does:

- TOML stdlib (e.g. `list.map`, `option.unwrap_or_else`): leave as
  `Module { module, func }` so per-target dispatchers can apply arg
  decoration / inline emit (`pass_stdlib_lowering` on Rust, `emit_call`
  on WASM).
- bundled-Almide stdlib (e.g. `list.split_at`, `list.iterate` defined
  in `stdlib/list.almd` and specialized by mono): rewrite to
  `CallTarget::Named { name: "almide_rt_<m>_<f>" }`, the codegen-
  registered mangled symbol. Both backends already register bundled fns
  under that name, so no further dispatch logic is needed.
- neither TOML nor bundled IR fn: postcondition violation — the
  unresolved-stdlib gap that previously deferred to a runtime trap is
  now a compile-time ICE under `ALMIDE_CHECK_IR=1`.

Removed: the WASM `_ if module == "list"` arm's bundled-fn fallback
(`func_map.get("almide_rt_list_*")`) added in v0.14.6 as a patch. With
the rewrite above, bundled fns never reach the Module dispatch arm in
the first place, so the fallback was dead.

`bundled-almide-ideal-form.md` is moved to `done/` — all 5 catalogued
items are closed.

## [0.14.6] — 2026-04-16

Phase 2 of the "LLM-first language" roadmap. **Focus**: make the compiler
produce copy-pasteable fix snippets in diagnostics, so LLM retries converge
faster. Measured against [almide-dojo] 30-task benchmark.

### LLM writability (dojo MSR, 2026-04-16)

| Model | v0.14.5 baseline | 0.14.6 | Δ |
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

### Added — bundled-Almide stdlib dispatch (infrastructure)

`stdlib/<module>.almd` files can now extend TOML-backed stdlib modules with
new fns written in Almide. Previously this silently failed: the codegen
emitted `almide_rt_<m>_<f>` for every `module.func` call, so a bundled
`fn binary_search_v2` ended up calling a non-existent runtime function.

The fix is four-layer:

1. **Frontend resolve** (`src/main.rs`) lowers bundled stdlib modules to
   IR even when `is_stdlib_module(name) == true`. TOML-only stdlib still
   short-circuits.
2. **TOML duplicate prune** (`src/main.rs`) drops bundled fns whose name
   collides with the TOML runtime — those go through the rt_ path,
   bundled-only fns survive.
3. **IR verify** (`almide-ir`) skips bundled stdlib modules in
   `known_module_functions`, so calls to TOML fns from bundled bodies
   (e.g. `result.collect` calling `list.is_empty`) don't error as
   "unknown function".
4. **Codegen dispatch** (`pass_stdlib_lowering`) builds a per-pass
   registry of bundled-only `(module, func)` pairs; for those, the
   `Module → almide_rt_*` rewrite is suppressed and the call stays as a
   `Module` call so the walker emits a normal user-fn invocation.

The pre-existing bundled `option`/`result` `.almd` sources turn out to
serve a hidden role: codegen prunes them (TOML wins for runtime
dispatch), but the type checker reads them and **uses their signatures
in preference to the TOML's** — so `option.or_else(o, () => ...)`
type-checks against the bundled `fn() -> X` rather than the TOML's
`Fn[Unit] -> X`. Removing them breaks the test suite. See
`roadmap/active/option-result-bundled-cleanup.md` for the path to
unify these.

### Added — bundled `list.*` fns (real users of the dispatch path)

`stdlib/list.almd` ships three fns covered by
`spec/stdlib/list_bundled_test.almd` on both Rust and WASM targets:

- `list.bundled_probe(n)` — smoke regression guard.
- `list.split_at(xs, n) -> (List[T], List[T])` — splits a list at index
  `n`. Demonstrates a bundled fn calling existing TOML fns
  (`list.take`, `list.drop`).
- `list.iterate(seed, f, n) -> List[T]` — builds
  `[seed, f(seed), f(f(seed)), ...]` of length `n`. Pure-Almide
  recursion through the bundled-dispatch path end-to-end.

### Changed — `monomorphize` extended to module-defined generics

Generic fns declared inside IR modules (e.g. `list.split_at[T]` in
`stdlib/list.almd`) are now specialized by the monomorphization pass,
not only top-level `program.functions`. The call target stays
`CallTarget::Module { module, func }`, so codegen continues to route
through the same stdlib dispatch on every backend. This closes the
WASM gap where `list.split_at([1, 2], 2)` was reaching the emitter
with a `TypeVar` and falling back to i32 element sizing. Self-recursive
`Named` calls inside specialized bodies are rewritten by
`specialize_function` itself; top-level rewrite_calls remains the
source of truth for top-level fns.

Roadmap: see `active/bundled-almide-ideal-form.md` for the remaining
debt (unified dispatch entry, retire `stub_call → unreachable`,
`ConcretizeTypes` hard postcondition, option/result signature
normalization).

No MSR delta expected (infrastructure only); downstream work
(`diagnostic-snippet-externalization`, auto-rewrite rules in Almide) is
unblocked.

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
