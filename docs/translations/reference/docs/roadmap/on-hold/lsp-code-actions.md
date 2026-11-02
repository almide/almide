<!-- description: Remaining generated refactorings and import-management coverage beyond shipped diagnostic quickfixes -->
# LSP Code Actions — Remaining Refactorings

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](lsp-code-actions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](lsp-code-actions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](lsp-code-actions.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/lsp-code-actions.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Shipped foundation

The LSP advertises and handles `textDocument/codeAction` in `src/cli/lsp.rs`.
`src/cli/lsp_code_actions.rs` already provides:

- E003 import insertion for a limited known-module list, not general
  auto-import resolution for every module.
- E006 conversion of the enclosing function to `effect fn`.
- Quickfix edits from a diagnostic's primary repair and alternatives;
  machine-applicable repairs are marked preferred.

`tests/lsp_test.rs::lsp_try_fix_reaches_client_as_code_action` and
`lsp_machine_applicable_repair_is_the_preferred_quickfix` pin the repair
transport. Adding another diagnostic repair does not require a second,
LSP-only implementation of the same edit.

## Remaining proposal — on hold

- Broader import management derived from compiler module information,
  rather than the current limited E003 list.
- Safe unused-binding refactorings, preserving any effects in the initializer.
- Generated missing-match arms from the exhaustiveness checker's case set.
- Function-signature type-annotation insertion from checked types.

These are coverage and generated-refactoring goals, not a claim that all
code actions are absent. Each action needs an editor-protocol regression and
an accepted/refused edit test before being described as available. Priorities
and the criterion for resuming this work are in
[the arena breakthrough plan](../../../../../roadmap/active/arena-breakthroughs.md).

## Design references

Gleam's `code_action.rs` provides examples of import management, pattern
extraction and annotation insertion; Nickel's `analysis.rs` is a comparison
for type- and contract-aware editor assistance. These are design references,
not claims about Almide's current coverage.
