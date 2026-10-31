<!-- description: Remaining generated refactorings and import-management coverage beyond shipped diagnostic quickfixes -->
# LSP Code Actions — Remaining Refactorings

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
[the arena breakthrough plan](../active/arena-breakthroughs.md).

## Design references

Gleam's `code_action.rs` provides examples of import management, pattern
extraction and annotation insertion; Nickel's `analysis.rs` is a comparison
for type- and contract-aware editor assistance. These are design references,
not claims about Almide's current coverage.
