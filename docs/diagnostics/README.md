# Almide Diagnostic Codes

Reference for `EXXX` codes emitted by the checker and canonicalizer.
Use `almide explain <code>` to read these from the CLI.

| Code | Title |
|------|-------|
| [E001](E001.md) | Type mismatch (incl. Unit-leak in fn body / if-arm) |
| [E002](E002.md) | Undefined function (incl. cross-language idiom hallucinations) |
| [E003](E003.md) | Undefined variable (incl. missing `import` for Tier 2 stdlib) |
| [E004](E004.md) | Wrong number of arguments |
| [E005](E005.md) | Argument type mismatch (constructor / function call) |
| [E006](E006.md) | Effect isolation: pure fn calls effect fn |
| [E007](E007.md) | `fan` block outside effect fn |
| [E008](E008.md) | `fan` block captures mutable variable |
| [E009](E009.md) | Reassignment to immutable binding |
| [E010](E010.md) | Non-exhaustive match |
| [E011](E011.md) | Mutable var mutated inside closure in pure fn |
| [E012](E012.md) | Duplicate definition (fn / test) |
| [E013](E013.md) | Field access on non-record / missing field |

Codes in the 4-digit range (`E0001` and up) that leak into output
are **rustc** errors, not Almide ones — they indicate a codegen bug
in the compiler. Report these at <https://github.com/almide/almide/issues>.

## Authoring guide

Every doc should include:

1. **One-line summary** of what the code means.
2. **Common cases** — minimal snippets that trigger it.
3. **Diagnostic shape** — what the actual `error[EXXX]: ...` output
   looks like, especially the `hint:` and (when present) `try:`
   sections. Dojo and other harnesses rely on these shapes.
4. **Fix** — ordered by frequency / probability.
5. **Why** — the design rationale (when non-obvious), so LLMs with a
   large context window can reason about whether the rule applies to
   their case.
6. **Related** — cross-references to adjacent codes and cheatsheet
   sections.
