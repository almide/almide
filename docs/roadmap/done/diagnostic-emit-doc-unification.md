<!-- description: Unify diagnostic emission sites with their docs/diagnostics/*.md files -->
<!-- done: 2026-04-20 -->
# Unify Diagnostic Emission with Docs

## Completion status (2026-04-20) — MVP via soft gate

The drift the arc was designed to catch (emission uses E0XX, doc
registry silently missing or out of sync) is now blocked by the
Phase 4 soft gate from the `diagnostics-here-try-hint` arc:

- `tests/diagnostic_coverage_test.rs` scans every `with_code("E###")`
  under `crates/` and enforces that a matching
  `docs/diagnostics/E###.md` exists (currently 15/15 codes covered,
  including E014 unreachable-arm and E015 reimpl-lint that landed
  in the same session).
- Reverse gate `every_fixture_meta_declares_known_code` catches
  fixture metadata referring to codes that don't exist in source.
- New codes landed through this session went through the coverage
  report + doc authoring in one pass, so the workflow naturally
  couples code addition to doc addition.

## Deferred to a future arc

- **Registry TOML / attribute macro** (Option A / B from the
  original plan): the formal `code → title → doc` table would be
  the definitive source of truth, but the soft-gate already prevents
  drift at CI time. Promote to a hard gate and / or formal registry
  when the code set grows past ~30, or when code metadata
  (try_replace support, auto-fix flags, severity overrides) gets
  structured enough to deserve its own schema.

## Original plan (retained for history)

Trigger: implement after `diagnostic-snippet-externalization`. Prevents
E010/E011-class drift (emission code and `docs/diagnostics/EXXX.md`
disagreeing on what the code means).

## Current state

- Emission: scattered `super::err(msg, hint, ctx).with_code("E0XX")` sites
  across `check/`, `canonicalize/`, `parser/`.
- Docs: `docs/diagnostics/EXXX.md`, one per code.
- **No structural link between them**. The author-side process for
  adding a code is:
  1. Pick an unused code.
  2. Emit it somewhere.
  3. Remember to also write the doc.

Step 3 gets forgotten (E012 and E013 were emitted but undocumented
until Phase 3). And when the emission message changes, the doc drifts.

## Proposed structure

Option A — **attribute macro**:

```rust
#[diagnostic(
    code = "E001",
    title = "Type mismatch",
    doc = "docs/diagnostics/E001.md",
)]
fn emit_type_mismatch(...) -> Diagnostic { ... }
```

Build-time check: every `code` has a matching doc file, and the doc's
title front-matter matches the `title`.

Option B — **registry file** (less magic):

`crates/almide-base/src/diagnostics/registry.toml` lists every code,
title, and doc path. `with_code("E001")` at runtime cross-refs the
registry for title lookup / test harness enumeration.

Build step verifies the registry against the doc directory (no orphan
docs, no missing docs).

## Test invariant

CI fails if:

- An `EXXX` code is used in `with_code(...)` but the registry lacks it.
- A `docs/diagnostics/EXXX.md` exists but the registry lacks it.
- The registry title doesn't match the H1 title in the doc.

## Recommendation

**Option B (registry)**. Less compiler magic, easier to audit, easier
to extend (e.g. "which codes have `try:` snippets?" becomes a TOML
query). Attribute macros are harder to refactor once entrenched.

## Estimated scope

~2 hours for MVP registry + CI checks + migrate existing 13 codes.
