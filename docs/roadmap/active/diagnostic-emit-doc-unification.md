<!-- description: Unify diagnostic emission sites with their docs/diagnostics/*.md files -->
# Unify Diagnostic Emission with Docs

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
