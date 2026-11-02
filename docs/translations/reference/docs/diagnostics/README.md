# Almide Diagnostic Codes

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/9a95b28519b90ae535dc9918bfed04805dc4675c/docs/diagnostics/README.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


Reference for `EXXX` codes emitted by the checker and canonicalizer.
Use `almide explain <code>` to read these from the CLI, and
`almide explain --list [--json]` for every code with its severity, first
release and fix-it verdict. A new code adds its doc AND a row in
[codes.toml](../../../../diagnostics/codes.toml) — `tests/explain_list_test.rs` keeps the listing
equal to the set of codes the compiler emits.

| Code | Title |
|------|-------|
| [E001](../../../../diagnostics/E001.md) | Type mismatch |
| [E002](../../../../diagnostics/E002.md) | Undefined function |
| [E003](../../../../diagnostics/E003.md) | Undefined variable |
| [E004](../../../../diagnostics/E004.md) | Wrong number of arguments |
| [E005](../../../../diagnostics/E005.md) | Argument type mismatch (constructor / function call) |
| [E006](../../../../diagnostics/E006.md) | Effect isolation: pure fn calls effect fn |
| [E007](../../../../diagnostics/E007.md) | `fan` block outside effect fn |
| [E008](../../../../diagnostics/E008.md) | a concurrently-run body reaches a `var` |
| [E009](../../../../diagnostics/E009.md) | Reassignment to immutable binding |
| [E010](../../../../diagnostics/E010.md) | Non-exhaustive match |
| [E011](../../../../diagnostics/E011.md) | Mutable var mutated inside closure in pure fn |
| [E012](../../../../diagnostics/E012.md) | Duplicate definition |
| [E013](../../../../diagnostics/E013.md) | Field access on a non-record / missing field |
| [E014](../../../../diagnostics/E014.md) | Unreachable match arm |
| [E015](../../../../diagnostics/E015.md) | Possible stdlib reimplementation |
| [E016](../../../../diagnostics/E016.md) | Function where equality or hashing is required |
| [E017](../../../../diagnostics/E017.md) | Record construction on an enum type name |
| [E018](../../../../diagnostics/E018.md) | Empty collection with an uninferable element type |
| [E019](../../../../diagnostics/E019.md) | Ambiguous constructor |
| [E020](../../../../diagnostics/E020.md) | Conflicting type declarations |
| [E021](../../../../diagnostics/E021.md) | Invalid record construction or pattern |
| [E022](../../../../diagnostics/E022.md) | `!` operator outside an error-propagating context |
| [E023](../../../../diagnostics/E023.md) | derived protocol not satisfied by a field type |
| [E024](../../../../diagnostics/E024.md) | integer literal out of range |
| [E025](../../../../diagnostics/E025.md) | cannot infer a concrete type (unconstrained type slot) |
| [E026](../../../../diagnostics/E026.md) | cannot index a String with `[]` |
| [E027](../../../../diagnostics/E027.md) | fan combinator tombstones: retired spellings that name their replacement |
| [E028](../../../../diagnostics/E028.md) | main() takes no parameters |
| [E029](../../../../diagnostics/E029.md) | unknown type name |
| [E030](../../../../diagnostics/E030.md) | type has no ordering |
| [E031](../../../../diagnostics/E031.md) | retired range spelling |
| [E032](../../../../diagnostics/E032.md) | immutable binding passed to `mut` parameter |
| [E033](../../../../diagnostics/E033.md) | opaque type constructed outside its defining module |
| [E034](../../../../diagnostics/E034.md) | error-channel operator on a non-Option/Result operand |
| [E035](../../../../diagnostics/E035.md) | branching on the text of an error message (warning) |
| [E036](../../../../diagnostics/E036.md) | map_err lambda never uses the error value (warning) |
| [E037](../../../../diagnostics/E037.md) | equality between incompatible types |
| [E038](../../../../diagnostics/E038.md) | `??` separated from its fallback |
| [E041](../../../../diagnostics/E041.md) | implicit propagation was removed |
| [E042](../../../../diagnostics/E042.md) | this statement discards a Result (must-use) |
| [E043](../../../../diagnostics/E043.md) | the try_* spellings, public and internal |
| [E044](../../../../diagnostics/E044.md) | main() returns Unit |
| [E045](../../../../diagnostics/E045.md) | tuple index on a non-tuple, or out of range |
| [E046](../../../../diagnostics/E046.md) | placeholder `_` in a call argument |
| [E047](../../../../diagnostics/E047.md) | invalid escape in a string literal |
| [E048](../../../../diagnostics/E048.md) | variant pattern the subject's type does not have |
| [E049](../../../../diagnostics/E049.md) | `let ... in <expr>` is OCaml/Haskell syntax |
| [E050](../../../../diagnostics/E050.md) | local `fn` or `let` collides with a selectively-imported name |
| [E051](../../../../diagnostics/E051.md) | file stamped for a dialect this compiler does not speak |
| [E052](../../../../diagnostics/E052.md) | calling a deprecated function |
| [E053](../../../../diagnostics/E053.md) | unknown attribute |
| [E054](../../../../diagnostics/E054.md) | fmt verification failed |
| [E055](../../../../diagnostics/E055.md) | `?.` on a Result |
| [E056](../../../../diagnostics/E056.md) | `?` on an Option is a no-op (warning) |
| [E057](../../../../diagnostics/E057.md) | fn has no body |
| [E058](../../../../diagnostics/E058.md) | unhashable Map key |
| [E059](../../../../diagnostics/E059.md) | non-iterable for head |
| [E060](../../../../diagnostics/E060.md) | import hygiene (warning) |
| [E061](../../../../diagnostics/E061.md) | top-level `let` bound to a function value |
| [E062](../../../../diagnostics/E062.md) | shebang without `-S` (warning) |
| [E070](../../../../diagnostics/E070.md) | `@bounded` requires counted loops |
| [E071](../../../../diagnostics/E071.md) | no allocation inside a counted loop |
| [E072](../../../../diagnostics/E072.md) | no `break`/`continue` in a `@bounded` loop |
| [E073](../../../../diagnostics/E073.md) | no recursion in the bounded profile |
| [E074](../../../../diagnostics/E074.md) | `@bounded` calls `@bounded` (or first-order pure stdlib) |
| [E075](../../../../diagnostics/E075.md) | no run-time-length heap construction |
| [E076](../../../../diagnostics/E076.md) | `@bounded` effects are capability-bounded |
| [E077](../../../../diagnostics/E077.md) | no Float arithmetic in the bounded profile (provisional) |
| [E078](../../../../diagnostics/E078.md) | no early exit inside a counted loop |
| [E080](../../../../diagnostics/E080.md) | or-pattern alternative binds a variable |
| [E081](../../../../diagnostics/E081.md) | the function is not available on the selected wasm target |
| [E082](../../../../diagnostics/E082.md) | the wasm leg cannot lower the program on --target wasm |
| [E083](../../../../diagnostics/E083.md) | compiler ownership defect: an emitted exit does not implement its checked ExitPlan |
| [E084](../../../../diagnostics/E084.md) | literal exit code outside the exit-status range |
| [E085](../../../../diagnostics/E085.md) | `@intrinsic` or `prim.*` outside the stdlib |
| [E086](../../../../diagnostics/E086.md) | a value would outlive its scoped allocation |
| [E087](../../../../diagnostics/E087.md) | an operation is not permitted in a scoped region |
| [E088](../../../../diagnostics/E088.md) | a recursive call retains the current scoped activation |
| [E089](../../../../diagnostics/E089.md) | the value has no defined string form |
| [E090](../../../../diagnostics/E090.md) | the `@extern(c)` signature has no C representation |
| [E092](../../../../diagnostics/E092.md) | a `@pure` fn is not pure |
| [E093](../../../../diagnostics/E093.md) | type applied to the wrong number of type arguments |
| [E094](../../../../diagnostics/E094.md) | type aliases form a cycle |
| [E095](../../../../diagnostics/E095.md) | the app passed to `http.serve` reads a local |
| [E096](../../../../diagnostics/E096.md) | a fn with a `mut` parameter used as a function value |
| [E097](../../../../diagnostics/E097.md) | `pub` before a `type`, `protocol` or `test` |
| [E098](../../../../diagnostics/E098.md) | the closure passed to `testing.assert_aborts` captures a local |
| [E099](../../../../diagnostics/E099.md) | `testing.assert_aborts` outside a direct call in a test block |
| [E100](../../../../diagnostics/E100.md) | outside the memory-safety guarantee |
| [E101](../../../../diagnostics/E101.md) | source that reads differently from what compiles |
| [E102](../../../../diagnostics/E102.md) | an invalid pattern in `regex.new` |
| [E103](../../../../diagnostics/E103.md) | `==` on a Secret that cannot compare in constant time |
| [E104](../../../../diagnostics/E104.md) | a skipped record field has no default |
| [E109](../../../../diagnostics/E109.md) | pure fn runs an effect fn through a function value |
| [E110](../../../../diagnostics/E110.md) | untrusted input decodes only into an input type |
| [E111](../../../../diagnostics/E111.md) | type too large |
| [E112](../../../../diagnostics/E112.md) | too many generic instances |
| [E113](../../../../diagnostics/E113.md) | match too complex to check for exhaustiveness |
| [E114](../../../../diagnostics/E114.md) | a generic fn reaches itself at a growing type |
| [E116](../../../../diagnostics/E116.md) | a response's Content-Type is written from its body's type |
| [E117](../../../../diagnostics/E117.md) | `http.typed` takes a literal media type no safe module owns |
| [E118](../../../../diagnostics/E118.md) | a media type's owner is not granted, not unique, or not a literal |
| [E119](../../../../diagnostics/E119.md) | a Region lent to an `@export(c)` fn escapes its call |
| [E420](../../../../diagnostics/E420.md) | Function visibility violation |

Retired codes: **E039** (the result.collect/collect_map deprecation window — the fns are removed, `result.partition` is the substance) and **E040** (the json.*/value.* alias deprecation window) each fired
for one release and was retired when the aliases dropped (#1078) — a retired
spelling is an ordinary [E002](../../../../diagnostics/E002.md) now; the migration map is recorded in
[docs/stdlib/json.md](../../../../stdlib/json.md#renamed-operations).
**E091** (a pattern after `var`, 0.66.0) was retired in 0.67.0 when `var`
began to take the `let` patterns (#3149): `var (x, y) = p` and
`var { a, b } = r` are legal and bind every name as a `var`, so nothing is
left for the code to report.

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
| `.with_alternative(line, col, end_col, text)` | `maybe-incorrect` | a human or a model — another reading, never preferred |
| `.with_try(text)` (no span) | `unspecified` | nobody — display only |

Every builder writes the diagnostic's `repair` field (#2149):
`primary` (the span-exact edit), `alternatives`, and `example` (a display-only
snippet). `almide fix`, the LSP quickfixes and `check --json` (`"repair"`) all
read that one field.

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

The `## Fix-it verdict` of each doc is checked against the binary in both
directions (#2149). Every **mechanical** code must emit a machine-applicable
`repair.primary` on at least one of its fixtures, and `almide fix` must turn
that fixture into a program that checks
(`tests/diagnostic_coverage_test.rs`). No other code may emit one
(`tests/diagnostic_harness_test.rs`).

## The two-way repair ratchet (#2149)

- **Loud side** — `tests/diagnostics/<case>/repair.grep`: one needle per
  line, each of which must appear in what the case's diagnostic tells the
  writer to do (message, hint, `try:`, any `repair` replacement). It pins the
  repair itself, so a hint that drifts back to "Fix the argument type" (#2097)
  fails the build.
- **Silent side** — `tests/diagnostics/silent/*.almd`: valid programs in the
  shapes that drew a wrong or misdirecting diagnostic before (#2345, #2097,
  #2254, #2096, #2607, #2093/#2095, and the near-miss of every mechanical
  fix-it). Each must check with no diagnostic at all, warnings included
  (`tests/diagnostic_silent_test.rs`). To add one, write the program with a
  header comment naming the shape it guards, and raise `SILENT_FLOOR`.
