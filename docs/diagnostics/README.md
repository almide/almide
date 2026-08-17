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
| [E014](E014.md) | Unreachable match arm |
| [E015](E015.md) | Possible stdlib reimplementation (warning, delegation shim) |
| [E028](E028.md) | `main()` takes no parameters (use `env.args()`) |
| [E029](E029.md) | Unknown type name in an annotation or record literal |
| [E030](E030.md) | Type has no ordering (sort/min/max over Map/Set/Fn or compound Float) |
| [E031](E031.md) | Retired range spelling (`..`/`..=` → `..<`/`...`, fix-it + `almide fix`) |
| [E032](E032.md) | Immutable binding passed to `mut` parameter |
| [E033](E033.md) | Opaque type constructed outside its defining module |
| [E034](E034.md) | Error-channel operator on non-Option/Result operand |
| [E035](E035.md) | (warning) Branching on error message text |
| [E036](E036.md) | (warning) map_err lambda discards the error value |
| [E037](E037.md) | Equality between incompatible types |
| [E038](E038.md) | `??` separated from its fallback |
| [E041](E041.md) | Implicit propagation removed — the value stays a Result; write `!` |
| [E042](E042.md) | Statement-position Result discarded (must-use) — `expr!` or `let _ = expr` |
| [E043](E043.md) | `list.try_*` removed, and `list.__fallible_*` is not a spelling — the core HOF is fallibility-polymorphic |
| [E044](E044.md) | A pure `main` returns Unit — a program's result is its output, not a return value |
| [E045](E045.md) | Tuple index `.k` on a non-tuple, or out of range |
| [E046](E046.md) | `_` in a call argument — not a value, not partial application (the typed hole `_` is expression-position only) |
| [E047](E047.md) | Invalid escape in a string literal — an undefined escape, or `\u{…}` outside the Unicode scalar range |
| [E048](E048.md) | Variant pattern the subject's type does not have — wrong family (`ok`/`err` on an Option), a builtin carrier over a user variant, or a foreign user case |
| [E049](E049.md) | `let ... in <expr>` is OCaml/Haskell syntax — bindings chain by newline (machine-applicable fix-it + `almide fix`) |
| [E420](E420.md) | Function visibility violation (placeholder code, renumber candidate) |

Retired codes: **E039** (the result.collect/collect_map deprecation window — the fns are removed, `result.partition` is the substance) and **E040** (the json.*/value.* alias deprecation window) each fired
for one release and was retired when the aliases dropped (#1078) — a retired
spelling is an ordinary [E002](E002.md) now; the migration map is recorded in
[docs/stdlib/json.md](../stdlib/json.md#renamed-operations).

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

## Fix-its and applicability (#1312)

A diagnostic that knows the span and the replacement **is** the fix.
`almide fix` carries no rewrite table of its own for these: it collects
every fix-it the compiler emitted, applies the machine-applicable ones,
re-checks, and iterates. A new diagnostic that can state its fix exactly
gets auto-fix the day it lands.

Attaching one is a two-way choice, and the builder makes you take it —
there is no way to attach a replacement span without naming an
applicability:

| Builder | Applicability | Who applies it |
|---|---|---|
| `.with_machine_fix(line, col, end_col, text)` | `machine-applicable` | `almide fix`, unattended |
| `.with_suggested_fix(line, col, end_col, text)` | `maybe-incorrect` | a human or a model, after choosing |
| `.with_try(text)` (no span) | `unspecified` | nobody — display only |

**Use `with_machine_fix` only for a re-spelling**: same value, same type,
same evaluation, exactly one reading of what the author meant. Deleting a
keyword the language does not have (E049) qualifies. A rename picked by
edit distance does not — it compiles and it may call the wrong function.
Nor does anything that changes `T` into `Option[T]`: applying it moves the
error instead of closing it.

**The span must be exact.** Derive it from a real token or `Span`; never
from searching the text for what you expect to be there. A fix-it anchored
to a guessed range rewrites the wrong bytes silently, which is strictly
worse than no fix-it at all — so if the exact range is not available,
fall back to `with_try` and let a human read the snippet. `with_machine_fix`
refuses a range that cannot name real source (line/col 0, inverted) and
degrades it to display-only rather than trust it.

Both halves are gated: `tests/diagnostic_harness_test.rs` asserts that every
span-anchored fix-it in the fixture corpus declares an applicability, applies
cleanly, compiles, and reproduces `fixed.almd`.
