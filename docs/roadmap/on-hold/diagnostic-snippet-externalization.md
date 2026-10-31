<!-- description: Remaining migration of diagnostic repair snippets from Rust literals to checked source templates -->
# Externalize `try:` Snippets from Rust Literals

## Current state

The prerequisite [bundled-Almide dispatch](../done/bundled-almide-dispatch.md)
shipped on 2026-04-16. It is no longer a blocker. The snippet migration itself
remains a proposal: there is no `stdlib/diagnostics/` template directory.

Repair text is still constructed in Rust, for example:

- [`check/calls.rs`](../../../crates/almide-frontend/src/check/calls.rs):
  `with_try` builds argument-count and rename repairs.
- [`check/solving.rs`](../../../crates/almide-frontend/src/check/solving.rs):
  `unit_leak_snippet` supplies result-expression repairs.
- [`stdlib.rs`](../../../crates/almide-frontend/src/stdlib.rs):
  `try_snippet_for_alias` supplies richer alias repairs.

These are examples, not a frozen file/count inventory. The drift risk remains:
text detached from the language's parser can teach an obsolete repair.

## Remaining design and work

1. Inventory current repair producers and decide which complete snippets can
   become checked source templates rather than dynamic Rust formatting.
2. Introduce a template directory (proposed: `stdlib/diagnostics/*.almd`),
   explicit placeholder substitution and a compiler loading path. Bundled
   dispatch being available does not implement this loading path by itself.
3. Migrate the selected repairs without changing their diagnostic meaning or
   making snippets user-extensible or localized.
4. Add a test harness that substitutes concrete values, parses every template
   and type-checks each one advertised as a complete repair. Mark intentional
   fragments explicitly; do not count them as type-checked programs.

For example, a template could explain a missing trailing value:

```almide
// placeholders: {fn_name}, {binding_name}, {ret_ty}
// Add {binding_name} as the trailing expression so the fn returns {ret_ty}:
//
//   let {binding_name} = <computation>
//   {binding_name}
```

This is proposed template notation, not an existing loader or passing test.
The migration is complete only when its templates and validation harness land
together; the completed dispatch prerequisite alone is not completion evidence.
