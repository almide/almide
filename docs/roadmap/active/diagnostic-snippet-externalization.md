<!-- description: Move try: snippet text out of Rust literals into stdlib/diagnostics/*.almd -->
# Externalize `try:` Snippets from Rust Literals

Trigger: blocked on `bundled-almide-dispatch`. Once bundled Almide can
extend stdlib modules, snippet text (currently 15+ string literals
scattered across the compiler) moves into source files the compiler
can load and type-check.

## Current state (the drift risk)

`try:` snippet text lives in Rust string literals in:

- `crates/almide-frontend/src/check/calls.rs` — E002 method, E002 fuzzy,
  E002 alias, E004 arg count
- `crates/almide-frontend/src/check/infer.rs` — E003 undef var, E009 reassign
- `crates/almide-frontend/src/check/solving.rs` — E001 Unit-leak (fn
  body + if arm + match arm variants)
- `crates/almide-frontend/src/stdlib.rs` — `try_snippet_for_alias`
  (int.sqrt conversion, int.gt operator mapping)
- `crates/almide-syntax/src/parser/statements.rs` — let-in chain hint
- `crates/almide-syntax/src/parser/primary.rs` — while-do recursion hint
- `crates/almide-syntax/src/parser/patterns.rs` — rest-pattern hint
- `crates/almide-syntax/src/parser/helpers.rs` — test-cascade, cons-pattern

That's 10+ files. Every language-surface change risks making a snippet
stale. **The E010/E011 doc bugs caught in Phase 3 were the same class of
issue** — different location but same root cause (detached text).

## Proposed structure

```
stdlib/diagnostics/
  E001_fn_body_unit_leak.almd
  E001_if_arm_unit_leak.almd
  E002_method_call.almd
  E002_int_gt_operator.almd
  E002_int_sqrt_conversion.almd
  E003_missing_import.almd
  E009_let_to_var.almd
  let_in_chain.almd
  while_do_recursion.almd
  rest_pattern_list_first.almd
  cons_pattern_list_first.almd
  test_cascade.almd
```

Each file is a valid Almide snippet (compile-time checked) with a
top-of-file comment marking placeholder slots:

```almide
// placeholders: {fn_name}, {binding_name}, {ret_ty}
// fn body ends with `let {binding_name} = ...` (a statement, returns Unit).
// Add `{binding_name}` as the trailing expression so the fn returns {ret_ty}:
//
//   let {binding_name} = <computation>
//   {binding_name}                         // <-- add this line
```

Compiler loads the file at diagnostic emission, substitutes the
placeholders, emits. Result: snippets are authored as real Almide
(syntax-highlighted, checkable, diff-friendly).

## Test invariant

Every `stdlib/diagnostics/*.almd` snippet (with placeholders
substituted to concrete examples) must:

1. Parse successfully.
2. Optionally type-check (some snippets contain `<placeholder>`-style
   sentinels that are deliberately invalid — those get an `// invalid`
   marker and skip type-check).

A `cargo test` fixture verifies this, so a snippet can never silently
break the thing it's supposed to teach.

## Non-goals

- Not making snippets pluggable / user-extensible. Still hardcoded to
  the compiler's diagnostic codes.
- Not localizing snippets. English only.

## Dependency

`bundled-almide-dispatch.md` must land first — the snippets are
easier to load via the bundled-Almide path than via raw `include_str!`.

## Estimated scope

~3 hours for the migration + test harness, assuming bundled-dispatch
is done.
