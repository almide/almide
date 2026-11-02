<!-- description: Remaining migration of diagnostic repair snippets from Rust literals to checked source templates -->
# Externalize `try:` Snippets from Rust Literals

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](diagnostic-snippet-externalization.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](diagnostic-snippet-externalization.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](diagnostic-snippet-externalization.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/diagnostic-snippet-externalization.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Current state

The prerequisite [bundled-Almide dispatch](../../../../../roadmap/done/bundled-almide-dispatch.md)
shipped on 2026-04-16. It is no longer a blocker. The snippet migration itself
remains a proposal: there is no `stdlib/diagnostics/` template directory.

Repair text is still constructed in Rust, for example:

- [`check/calls.rs`](../../../../../../crates/almide-frontend/src/check/calls.rs):
  `with_try` builds argument-count and rename repairs.
- [`check/solving.rs`](../../../../../../crates/almide-frontend/src/check/solving.rs):
  `unit_leak_snippet` supplies result-expression repairs.
- [`stdlib.rs`](../../../../../../crates/almide-frontend/src/stdlib.rs):
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
