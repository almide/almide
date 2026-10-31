# Almide Diagnostic Codes

Reference for `EXXX` codes emitted by the checker and canonicalizer.
Use `almide explain <code>` to read these from the CLI, and
`almide explain --list [--json]` for every code with its severity, first
release and fix-it verdict. A new code adds its doc AND a row in
[codes.toml](codes.toml) — `tests/explain_list_test.rs` keeps the listing
equal to the set of codes the compiler emits.

| Code | Title |
|------|-------|
| [E001](E001.md) | Type mismatch |
| [E002](E002.md) | Undefined function |
| [E003](E003.md) | Undefined variable |
| [E004](E004.md) | Wrong number of arguments |
| [E005](E005.md) | Argument type mismatch (constructor / function call) |
| [E006](E006.md) | Effect isolation: pure fn calls effect fn |
| [E007](E007.md) | `fan` block outside effect fn |
| [E008](E008.md) | a concurrently-run body reaches a `var` |
| [E009](E009.md) | Reassignment to immutable binding |
| [E010](E010.md) | Non-exhaustive match |
| [E011](E011.md) | Mutable var mutated inside closure in pure fn |
| [E012](E012.md) | Duplicate definition |
| [E013](E013.md) | Field access on a non-record / missing field |
| [E014](E014.md) | Unreachable match arm |
| [E015](E015.md) | Possible stdlib reimplementation |
| [E016](E016.md) | Function where equality or hashing is required |
| [E017](E017.md) | Record construction on an enum type name |
| [E018](E018.md) | Empty collection with an uninferable element type |
| [E019](E019.md) | Ambiguous constructor |
| [E020](E020.md) | Conflicting type declarations |
| [E021](E021.md) | Invalid record construction or pattern |
| [E022](E022.md) | `!` operator outside an error-propagating context |
| [E023](E023.md) | derived protocol not satisfied by a field type |
| [E024](E024.md) | integer literal out of range |
| [E025](E025.md) | cannot infer a concrete type (unconstrained type slot) |
| [E026](E026.md) | cannot index a String with `[]` |
| [E027](E027.md) | fan combinator tombstones: retired spellings that name their replacement |
| [E028](E028.md) | main() takes no parameters |
| [E029](E029.md) | unknown type name |
| [E030](E030.md) | type has no ordering |
| [E031](E031.md) | retired range spelling |
| [E032](E032.md) | immutable binding passed to `mut` parameter |
| [E033](E033.md) | opaque type constructed outside its defining module |
| [E034](E034.md) | error-channel operator on a non-Option/Result operand |
| [E035](E035.md) | branching on the text of an error message (warning) |
| [E036](E036.md) | map_err lambda never uses the error value (warning) |
| [E037](E037.md) | equality between incompatible types |
| [E038](E038.md) | `??` separated from its fallback |
| [E041](E041.md) | implicit propagation was removed |
| [E042](E042.md) | this statement discards a Result (must-use) |
| [E043](E043.md) | the try_* spellings, public and internal |
| [E044](E044.md) | main() returns Unit |
| [E045](E045.md) | tuple index on a non-tuple, or out of range |
| [E046](E046.md) | placeholder `_` in a call argument |
| [E047](E047.md) | invalid escape in a string literal |
| [E048](E048.md) | variant pattern the subject's type does not have |
| [E049](E049.md) | `let ... in <expr>` is OCaml/Haskell syntax |
| [E050](E050.md) | local `fn` or `let` collides with a selectively-imported name |
| [E051](E051.md) | file stamped for a dialect this compiler does not speak |
| [E052](E052.md) | calling a deprecated function |
| [E053](E053.md) | unknown attribute |
| [E054](E054.md) | fmt verification failed |
| [E055](E055.md) | `?.` on a Result |
| [E056](E056.md) | `?` on an Option is a no-op (warning) |
| [E057](E057.md) | fn has no body |
| [E058](E058.md) | unhashable Map key |
| [E059](E059.md) | non-iterable for head |
| [E060](E060.md) | import hygiene (warning) |
| [E061](E061.md) | top-level `let` bound to a function value |
| [E062](E062.md) | shebang without `-S` (warning) |
| [E070](E070.md) | `@bounded` requires counted loops |
| [E071](E071.md) | no allocation inside a counted loop |
| [E072](E072.md) | no `break`/`continue` in a `@bounded` loop |
| [E073](E073.md) | no recursion in the bounded profile |
| [E074](E074.md) | `@bounded` calls `@bounded` (or first-order pure stdlib) |
| [E075](E075.md) | no run-time-length heap construction |
| [E076](E076.md) | `@bounded` effects are capability-bounded |
| [E077](E077.md) | no Float arithmetic in the bounded profile (provisional) |
| [E078](E078.md) | no early exit inside a counted loop |
| [E080](E080.md) | or-pattern alternative binds a variable |
| [E081](E081.md) | the function is not available on the selected wasm target |
| [E082](E082.md) | the wasm leg cannot lower the program on --target wasm |
| [E083](E083.md) | compiler ownership defect: an emitted exit does not implement its checked ExitPlan |
| [E084](E084.md) | literal exit code outside the exit-status range |
| [E085](E085.md) | `@intrinsic` or `prim.*` outside the stdlib |
| [E086](E086.md) | a value would outlive its scoped allocation |
| [E087](E087.md) | an operation is not permitted in a scoped region |
| [E088](E088.md) | a recursive call retains the current scoped activation |
| [E089](E089.md) | the value has no defined string form |
| [E090](E090.md) | the `@extern(c)` signature has no C representation |
| [E092](E092.md) | a `@pure` fn is not pure |
| [E093](E093.md) | type applied to the wrong number of type arguments |
| [E094](E094.md) | type aliases form a cycle |
| [E095](E095.md) | the app passed to `http.serve` reads a local |
| [E096](E096.md) | a fn with a `mut` parameter used as a function value |
| [E097](E097.md) | `pub` before a `type`, `protocol` or `test` |
| [E098](E098.md) | the closure passed to `testing.assert_aborts` captures a local |
| [E099](E099.md) | `testing.assert_aborts` outside a direct call in a test block |
| [E100](E100.md) | outside the memory-safety guarantee |
| [E101](E101.md) | source that reads differently from what compiles |
| [E102](E102.md) | an invalid pattern in `regex.new` |
| [E103](E103.md) | `==` on a Secret that cannot compare in constant time |
| [E104](E104.md) | a skipped record field has no default |
| [E109](E109.md) | pure fn runs an effect fn through a function value |
| [E110](E110.md) | untrusted input decodes only into an input type |
| [E114](E114.md) | a generic fn reaches itself at a growing type |
| [E420](E420.md) | Function visibility violation |

Retired codes: **E039** (the result.collect/collect_map deprecation window — the fns are removed, `result.partition` is the substance) and **E040** (the json.*/value.* alias deprecation window) each fired
for one release and was retired when the aliases dropped (#1078) — a retired
spelling is an ordinary [E002](E002.md) now; the migration map is recorded in
[docs/stdlib/json.md](../stdlib/json.md#renamed-operations).
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
