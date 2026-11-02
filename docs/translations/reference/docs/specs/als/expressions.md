<a id="als-e--expression-semantics-normative"></a>

# ALS §E — Expression Semantics (normative)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](expressions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](expressions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](expressions.zh-CN.md)

> Non-normative reader translation of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md). The normative status and implementation obligations stated below belong to the canonical original, not this translation. This is a commit-pinned snapshot, not generator input or an automatically synchronized document. Original sources and machine consumers remain unchanged. Examples, measurements, target parity, and conformance/proof claims have not been independently executed or revalidated here.

> Last updated: 2026-10-24

> **Status**: normative. These sections are an implementation-independent **normative specification**; both v0 (native) and v1 (MIR/wasm) are obliged to conform. Evidence of conformance is supplied by the corresponding fixtures in `spec/wasm_cross/` (three observations: stdout, stderr, exit code). This file is the first group of sections along the **syntactic-element dimension** (proofs/als-element-coverage.toml). Each section uses a four-part template: “grammar of accepted forms,” “normative values,” “rulings,” and “fixtures.” All claims were measured on both targets when enacted (making conjecture normative violates F1).

<a id="als-e1-整数リテラルexprkindint"></a>

## ALS-E1 Integer literals (`ExprKind::Int`)

**Accepted forms**:

```ebnf
int     := dec | hex | oct | bin
dec     := digit (digit | "_")*
hex     := "0x" hexdigit (hexdigit | "_")*
oct     := "0o" octdigit (octdigit | "_")*
bin     := "0b" bindigit (bindigit | "_")*
```

Underscores are accepted as digit separators in every radix and do not affect the value.

**Normative values**: An integer literal has type `Int` = **64-bit signed two’s-complement integer** (i64). Its representable range is −9223372036854775808 to 9223372036854775807.

**Range ruling**: An out-of-range literal is a **check-time error E024** (`integer literal
'…' is out of range for Int`); silent wrapping or saturation is nonconforming. A **literal immediately following unary minus** is range-checked with its sign included. Thus the minimum value `-9223372036854775808` can be written directly as a literal (whitespace between the minus and digits is allowed). Without this ruling, i64::MIN could only be written as `-9223372036854775807 - 1`, making transcription of constant tables error-prone.

**Display round trip**: `int.to_string` returns decimal notation over the entire range, including the minimum (including the i64::MIN digit sequence `-9223372036854775808` — an execution pin verifies this edge case in v1 wasm’s `$itoa`).

Test: `spec/wasm_cross/literal_int_forms.almd` (contract C-231).

<a id="als-e2-真偽リテラルexprkindbool"></a>

## ALS-E2 Boolean literals (`ExprKind::Bool`)

**Accepted forms**: Keywords `true` / `false` (reserved words, not usable as identifiers).

**Normative values**: The type is `Bool`. At every observation point, including string interpolation and operands of `==`, the display forms are the lowercase spellings `true` / `false`.

Test: `spec/wasm_cross/literal_int_forms.almd` (contract C-232).

<a id="als-e4-ユニットリテラルexprkindunit"></a>

## ALS-E4 Unit literal (`ExprKind::Unit`)

**Accepted form**: `()` (an empty pair of parentheses).

**Normative values**: The type is `Unit`. `Unit` has exactly one value, and `()` is its only literal notation. It is accepted in expression position, in a `let` binding, and as the body of a `-> Unit` function; as a block’s tail expression it becomes the function’s return value.

**Equality ruling**: `==` on `Unit` is **reflexively true** (`() == ()` → `true`, `!=` → `false`). Since a type with one inhabitant has no runtime read, v1 lowering folds this comparison to a constant Bool **only for call-free operand pairs**. Operands containing calls (`f() == ()`) are not folded: erasing the call’s effects would be a miscompile, so these hit the existing wall and are rejected (honest lack of support > a silent wrong answer). Before this folding, Eq-over-Unit hit a v1 wall in every position, and equality evidence was native-only; it now has execution pins on both targets.

Tests: `spec/wasm_cross/literal_unit.almd` (contract C-233), `spec/lang/unit_literal_test.almd` (assert form, executed on the wasm leg).

> **Old unwritten-status note for ALS-E3 (floating-point literals; a partial section appears later)**: The fixture cannot pass the `almide fmt` spec gate (fmt normalizes exponent/underscore spellings through an f64 value — #1261). Until a ruling is made and fmt preserves spelling, the `ExprKind::Float` row in proofs/als-element-coverage.toml remains UNWRITTEN (F1: no normative claim without a fixture).

<a id="als-e6-括弧式exprkindparen"></a>

## ALS-E6 Parenthesized expressions (`ExprKind::Paren`)

**Accepted form**: `( expr )`.

**Normative values**: A parenthesized expression has the same value, type, and effects as its inner expression. Parentheses have no meaning other than overriding binding precedence. `(1 + 2) * 3` is `9`; `1 + 2 * 3` is `7` (measured on both targets). fmt **preserves** redundant parentheses (does not strip them on its own).

**Distinction from 1-tuples**: `(e)` is a parenthesized expression; `(e,)` is a **one-element tuple**. The trailing comma carries meaning. fmt round-trips a one-element tuple’s trailing comma (fixed in #1265; regression test `fmt_one_tuple_keeps_trailing_comma`). Known limitation: the native backend cannot lower `.0` when string interpolation and a 1-tuple coexist (#1267; loud rejection, not a wrong value).

Tests: `spec/wasm_cross/grouping_unary.almd`, `spec/wasm_cross/tuple_single.almd` (both contract C-234).

<a id="als-e7-単項演算子exprkindunary"></a>

## ALS-E7 Unary operators (`ExprKind::Unary`)

**Accepted forms**: Logical negation uses keyword `not` (`not b`; repeated `not not b` is allowed). Arithmetic negation uses prefix `-` (applicable to literals, variables, parenthesized expressions, and floating-point values).

**Ruling on `!`**: Prefix `!` is **not accepted**; a compile error directs the user to `Use 'not' for boolean negation` (postfix `!` is unwrap, ADR-0008). Expression-statement positions and interpolation `${...}` follow different lexical paths but return the same guidance (`tests/unary_not_diag_test.rs` pins both positions).

**Normative values**: `not true` → `false`, `not not b` ≡ `b`. `-` immediately before a literal is folded into signed range checking (ALS-E1’s ruling). `-x` (variable), `-(3 + 4)` (parenthesized expression) → `-7`, `-(1.5)` → `-1.5` — all identical on both targets.

Tests: `spec/wasm_cross/grouping_unary.almd` (contract C-235), `tests/unary_not_diag_test.rs` (negative cases).

<a id="als-e8-タプルexprkindtuple--exprkindtupleindex"></a>

## ALS-E8 Tuples (`ExprKind::Tuple` / `ExprKind::TupleIndex`)

**Accepted forms**: Literal `(e1, e2, …)` (heterogeneous element types allowed). One-element tuples require a trailing comma, `(e,)` (ALS-E6’s distinction). Positional access is `.k` (k is a zero-based decimal digit sequence). Destructuring binding: `let (x, y) = t`. `(A, B)` can be used as a function parameter or return type.

**Chained-index ruling**: `n.0.1` is **not accepted**: lexical analysis reads `0.1` as a floating-point literal. 0.59.1 explains this with a dedicated diagnostic (“`.0.1` lexes as the float literal '0.1', not two projections”; the hint requests parentheses as in `(x.0).1`). Write nested access as `(n.0).1` (measurement pin).

**Normative values**: Tuples are fixed-length values with a type for each position. `==` is elementwise structural equality. Observed values are identical on both targets through destructuring bindings, `.k`, parameter passing, and return values.

**Diagnostic update**: Out-of-range `.k` and non-tuple subjects produce check-time E045. The old #1266 description of rejection only at build time no longer applies (`tests/tupleindex_diag_test.rs`; tuple-index checking in `crates/almide-frontend/src/check/infer_members.rs`). The following native 1-tuple limitation is a historical record and has not been revalidated as a current limitation: the native backend cannot lower a 1-tuple’s `.0` when it coexists with string interpolation (#1267).

Test: `spec/wasm_cross/tuple_ops.almd` (contract C-236).

<a id="als-e9-optionresult-コンストラクタexprkindsome--exprkindnone--exprkindok--exprkinderr"></a>

## ALS-E9 Option/Result constructors (`ExprKind::Some` / `ExprKind::None` / `ExprKind::Ok` / `ExprKind::Err`)

**Accepted forms**: `some(e)`, `none` (**bare value** — calling `none()` is check-time E001), `ok(e)`, `err(e)`. All use lowercase spellings.

**Normative values**: `some(e)` constructs `Option[T]`, with the element type inferred from `e`. The type of `none` flows from an annotation (`let n: Int? = none`) or a consumption site (`n ?? 7`). `ok(e)` / `err(e)` construct Result in contexts expecting a Result type (annotated bindings, return positions, direct consumption).

**ok/err context ruling**: In an effect fn, **binding `ok(e)` / `err(e)` to an unannotated `let` is rejected at check time** (E041, ADR-0008’s explicit-propagation rule; hints point to `!`, `??`, `?`, or match). There is no route through which Result implicitly passes untouched. Negative cases in `tests/ctor_diag_test.rs` pin both rulings.

**Consumption**: `??` returns the value on the Ok/Some side and the fallback on the Err/None side (`ok(3) ?? 0` → `3`, `err("boom") ?? -9` → `-9`, measured on both targets). See the R series (error surface) for the full propagation and branching specification.

**Nesting rule**: `some(none)` / `some(some(v))` are ordinary Option values and can be peeled one layer at a time with `??` using an Option-typed default (`sn ?? none` → inner Option; measured: none side → 9, some side → 5, identical on both targets — #1270 closed the v1 wall).

Tests: `spec/wasm_cross/option_result_ctors.almd` (contract C-237), `tests/ctor_diag_test.rs` (negative cases).

<a id="als-e10-レンジ式exprkindrange"></a>

## ALS-E10 Range expressions (`ExprKind::Range`)

**Accepted forms**: Exclusive end `start ..< end`; inclusive end `start ... end`. Bounds are scalar Int expressions (literals, variables, calls).

**Retired-spelling ruling**: `..` and `..=` are **retired**: check-time E031 points to the current spellings, and `almide fix` migrates them mechanically. Historically, `..` was exclusive and `..=` inclusive, so silently reinterpreting either introduces an off-by-one error; rejection alone is safe (negative cases in `tests/range_spelling_diag_test.rs` pin both spellings).

**Normative values**: Ranges are first-class values: bind `let r = 0..<3` and then iterate with `for i in r` to produce the same sequence as the inline head `for i in 0..<3`. A binding used only as a for-in head is **not materialized** (#1400, C-238 — pinned by `range_bind_no_materialize.almd` / `range_bind_huge.almd` (2^32 iterations, 32 GB not materialized) / `range_materialize_oom.almd`; #1272’s deferred empty value with 0 iterations is the preceding regression pin). Other uses materialize an actual list. Exclusive `0..<3` → 0,1,2; inclusive `0...5` → 0–5. Nonzero starts and call bounds also agree on both targets (the fixture pins sums 3 / 15 / 9 / 6).

Tests: `spec/wasm_cross/range_first_class.almd` (contract C-238), `tests/range_spelling_diag_test.rs` (negative cases).

<a id="als-e11-リストリテラルと索引exprkindlist--exprkindindexaccess"></a>

## ALS-E11 List literals and indexing (`ExprKind::List` / `ExprKind::IndexAccess`)

**Accepted forms**: Literal `[e1, e2, …]`, empty list `[]` (element type from annotation or context); nesting is allowed. Access is `xs[i]` (zero-based). Concatenation uses `+` (ALS-wide operator overloading: `+` on strings and lists means concatenation).

**Normative values**: `xs[i]` returns a value of the element type directly (not Option). **Out-of-bounds access aborts at runtime**: both targets emit the unified message `Error: index out of bounds` + exit 1, matching all three observations (stdout, stderr, exit code). This is the same ruling as the existing write-side contract C-067; silently filling with 0 or ignoring the access is nonconforming. `+` returns a new list with the left and right elements in order.

Tests: `spec/wasm_cross/collection_literals.almd` (contract C-239); out-of-bounds abort: `spec/wasm_cross/index_bounds_write_heap.almd` (C-067).

<a id="als-e12-マップリテラルexprkindmapliteral--exprkindemptymap"></a>

## ALS-E12 Map literals (`ExprKind::MapLiteral` / `ExprKind::EmptyMap`)

**Accepted forms**: `["k1": v1, "k2": v2]` — **square brackets + colon**. An empty map is `[:]` (type from annotation). Braces `{"k": v}` are **not accepted** (parse error — transcription mistakes from JSON/other languages are stopped during checking).

**Normative values**: `m[k]` returns `Option[V]` (unlike direct list access), with `??` supplying a default. A missing key yields `none` (measured: `m["zz"] ?? -1` → `-1`). Insertion order is deterministically preserved (the AlmideMap specification; iteration order is insertion order).

Test: `spec/wasm_cross/collection_literals.almd` (contract C-240).

<a id="als-st1-束縛文stmtlet--stmtvar--stmtassign"></a>

## ALS-ST1 Binding statements (`Stmt::Let` / `Stmt::Var` / `Stmt::Assign`)

**Accepted forms**: Immutable binding `let x = e` (optional type annotation `let x: T = e`), mutable binding `var x = e`, reassignment `x = e` (`var` only).

**Shadowing ruling**: Same-name rebinding with `let` is **accepted**. The new binding’s initializer sees the old binding (after `let x = 1`, `let x = x + 10` makes `x` equal to 11). This is neither an error nor a warning (successive refinement of values is idiomatic).

**Immutability ruling**: Assignment to `let` is **check-time E009**, with a hint pointing to a `var` declaration. Reassignment of `var` is fixed to the declared type; assigning a different type is **E001** (no silent widening). `tests/binding_diag_test.rs` pins negative cases.

**Normative values**: Assignment to `var` is observably reflected across control flow (loop bodies and branches; fixture: two sequential assignments → 10, loop accumulation → 6). A `let` initializer may also be a conditional expression (`let y = if … then … else …`).

Tests: `spec/wasm_cross/binding_stmts.almd` (contract C-241), `tests/binding_diag_test.rs` (negative cases).

<a id="als-e13-条件式exprkindif"></a>

## ALS-E13 Conditional expressions (`ExprKind::If`)

**Accepted forms**: `if cond then arm` (else optional in statement position), `if cond then arm
else arm`; arms are single expressions or brace blocks `then { … } else { … }`; chaining with `else if`. The condition is `Bool`.

**Ruling on `then`**: `then` is **mandatory**. The brace-language transcription `if cond { … }` is a parse error with guidance `if requires 'then'` (negative cases pinned by `tests/if_then_diag_test.rs`).

**Normative values**: Conditional expressions have values (`let g = if … then "low" else if … then
"mid" else "high"`). The form without else is allowed only in statement position and has value `Unit`. Only the selected arm triggers effects (observing an unselected arm’s effects is nonconforming; v1 lowering’s wall against linearizing both arms defends this rule).

Tests: `spec/wasm_cross/if_block_forms.almd` (contract C-242), `tests/if_then_diag_test.rs` (negative cases).

<a id="als-e14-ブロック式exprkindblock"></a>

## ALS-E14 Block expressions (`ExprKind::Block`)

**Accepted form**: `{ stmt* tail? }` — a sequence of statements with an optional tail expression.

**Normative values**: A block’s value is **its tail expression’s value** (`Unit` if absent). A block is an expression and may appear as a `let` initializer, an `if` arm, or a function body. Bindings inside a block are block-scoped (fixture: `let b = { let t = a * 2
t + 1 }` → 11, block in an if arm → 9, identical on both targets).

Test: `spec/wasm_cross/if_block_forms.almd` (contract C-243).

<a id="als-e15-while-文exprkindwhile"></a>

## ALS-E15 while statements (`ExprKind::While`)

**Accepted form**: `while cond { body }` — the condition is `Bool`; the body is a brace block. It is pre-tested (0 body executions if initially false).

**Normative values**: This is a statement with value `Unit`. Assignments to mutable state (`var`) accumulate observably across iterations (fixture: counter sum 10 / terminal value 5; data-dependent iteration count — Collatz(16) → 4 steps — identical on both targets). The condition is reevaluated at the start of each iteration.

**break/continue**: Guard forms in statement position execute on both targets (ALS-E24; the brick landed in #1306). Crossing heap frames still hits a wall.

Test: `spec/wasm_cross/while_loops.almd` (contract C-244).

<a id="als-e16-文字列補間exprkindinterpolatedstring"></a>

## ALS-E16 String interpolation (`ExprKind::InterpolatedString`)

**Accepted form**: `"text ${expr} text"` — any number of segments; `expr` is an expression (variable references and arithmetic expressions have measurement pins). Escape literal `${` with `\${` (measured: `"escaped \${not_interp}"` → `escaped ${not_interp}`).

**Normative values**: Each segment’s value is embedded in its type’s **canonical display form**: String unchanged, Int decimal (ALS-E1), Bool lowercase (ALS-E2), Float canonical (`1.5` is pinned by measurement; a comprehensive float-display specification awaits ALS-E3/T2 rulings). The entire interpolation has type `String`.

**Declared repr**: Every value is displayed structurally. If its type declares `fn T.repr`, however, the value is displayed as that repr string at any depth (inside records, variants, lists, Option, Result, tuples, or Map). The same rule applies to `expected:` / `found:` in the failure blocks of `assert_eq` / `assert_ne` (ALS-T18). `Secret[A]` from `import secret` displays as `Secret(***)` regardless of `A`; only `secret.reveal(s)` exposes the contents.

**Ruling on `!`**: Prefix `!` inside interpolation is rejected with the same guidance toward `not` as in expression-statement position (ALS-E7; interpolation cases in `tests/unary_not_diag_test.rs`).

Tests: `spec/wasm_cross/string_interpolation.almd` (contract C-245), `spec/wasm_cross/secret_never_rendered.almd` (contract C-425).

<a id="als-e17-識別子exprkindident"></a>

## ALS-E17 Identifiers (`ExprKind::Ident`)

**Accepted forms**: Bound names. Resolution chooses the **nearest binding** (shadowing is specified in ALS-ST1).

**Ruling**: An unresolved name is **check-time E003**. The diagnostic includes the very name whose resolution failed (the diagnostic alone identifies the incorrect identifier; pinned by `tests/ident_diag_test.rs`). There is no runtime name-resolution failure (a name that passes checking always resolves to a binding; v1 covers this with the NameTotality proof).

**Lexical rules** (#1456): Identifiers are UAX #31 default identifiers: XID_Start or `_` initially, then XID_Continue (the same rules as Rust RFC 2457, Python PEP 3131, and ECMAScript). Characters whose compatibility decomposition differs from their canonical decomposition (fullwidth alphanumerics, ligatures, enclosed characters, etc., folded only by NFKC) are not identifier characters; in name positions they are syntax errors, not silently folded (`ａｂｃ` cannot pass as a name distinct from `abc`). Names are stored in NFC; canonically equivalent spellings (`é` and `e` + U+0301) are the same name. Keywords are ASCII-only.

**Type side and value side**: Names starting with uppercase letters belong to the type side (types, cases, constructors); names starting with lowercase or uncased characters (kanji, kana, Hangul, etc.) belong to the value side. However, even an uncased name belongs to the type side throughout a module if that module declares it immediately after `type` / `protocol`, or as a variant case or alias target (immediately after a declaration’s `=` or `|`, outside parentheses, and not immediately followed by `.`). After `type 色 = | 赤 | 緑`, `赤` cannot be a binding name, just like ASCII `Red`. Module names (each path segment of `module` / `import` and the alias in `import … as`) are ASCII; non-ASCII ones are syntax errors, so qualifiers never become uncased names. If a module-qualified final segment is uncased (`geo.住所`), it is read as type-side in type and pattern positions and before a named record’s `{`. Programs using non-ASCII let, fn, argument, record-field, type, and case names are byte-identical on both targets, including declaration names spelled out in record and variant displays.

Tests: `spec/wasm_cross/string_interpolation.almd` (contract C-246), `tests/ident_diag_test.rs` (negative cases), `spec/wasm_cross/unicode_identifiers.almd` (contract C-387).

<a id="als-e18-match-式exprkindmatch"></a>

## ALS-E18 match expressions (`ExprKind::Match`)

**Accepted form**: `match subject { pattern [if guard] => expr, … }`. Patterns include literals (strings, integers), wildcard `_`, binder `x`, guarded binder `x if cond`, Option’s `some(x)` / `none`, Result’s `ok(x)` / `err(e)`, and list patterns `[]` / `[a, b]` (matching length plus elementwise binding/literal matching). Their **rest forms**, `[h, ..t]` / `[h, ..]` (#1461), relax the length test to `>=`; everything after the prefix elements is bound by `..name` with **the same list type as the subject** (`..` binds nothing). Rest is allowed **only as the last element** of a list pattern (anything following it is the parse error “Rest pattern must be last”).
**As-patterns**, `name @ pattern` (#1461), bind **the entire value** at that position while the inner pattern destructures/matches the same value (`c @ Circle(r)`). `@` binds more tightly than arm-level `|`; as within an or alternative is E080, like other binders.
**Or-patterns**, `a | b | c` (#1461), select the arm if any alternative matches. Alternatives **cannot have binders** (E080, since the body cannot depend on which alternative matched; split the arms or use `_` when a payload is needed). Exhaustiveness treats the arm as covering the **union** of alternatives. Lowering desugars it into one IR arm per alternative, so both backends and interp see only single-pattern arms (cross-target agreement is by construction).
Arm bodies are expressions (including interpolation).

**Selection ruling**: Arms are tried **from the beginning, taking the first match** (fixture: `n = 7` passes `x if x < 0` and `x if x % 2 == 0` and falls into `_`; measurement pins first-match-wins). Only the selected arm is evaluated.

**Exhaustiveness ruling**: A non-exhaustive match is **check-time E010**. The diagnostic **names** the missing cases (`missing none`), with a hint to add an arm (pinned by `tests/match_diag_test.rs`). A match that passes type checking cannot encounter “no arm matches” at runtime.

Tests: `spec/wasm_cross/match_forms.almd` (contract C-247), `spec/wasm_cross/list_rest_pattern.almd` (contract C-332 — all-leg agreement for rest forms: recursive sum, a natural form made terminating by the coverage ladder, guarded rest, string tail), `spec/wasm_cross/as_pattern.almd` (contract C-333 — as forms: forwarding a whole variant, as×rest composition, option carrier), `tests/match_diag_test.rs` (negative cases).

<a id="als-e19-for-in-文exprkindforin"></a>

## ALS-E19 for-in statements (`ExprKind::ForIn`)

**Accepted form**: `for pat in iterable { body }`. Iterables are lists, ranges (ALS-E10), and maps (ALS-E12’s `(k, v)` iteration). `pat` is a binder or destructuring `(i, v)` over a list of tuple elements.

**Normative values**: This is a statement with value `Unit`. Elements are bound once each, in order from the beginning; assignments to mutable state in the body accumulate across iterations (fixture: List[Int] accumulation → 8, List[String] element output, List[(Int, Int)] destructuring head → 53). The iterable is evaluated exactly once before the loop starts.

Guarded `break` / `continue` have landed across both targets (C-256 / #1306: `loop_break_continue.almd` pins for-range break, `spec/lang/loop_break_continue_test.almd` pins for-list continue; accepted forms in ALS-E24).

Tests: `spec/wasm_cross/for_in_forms.almd` (contract C-248); range head: `range_first_class.almd` (C-238); map iteration: `collection_literals.almd` (C-240).

<a id="als-st2-分解束縛stmtletdestructure"></a>

## ALS-ST2 Destructuring bindings (`Stmt::LetDestructure`)

**Accepted form**: `let (a, b, …) = e` — the right-hand side is a tuple. Each component is immutably bound to the value at the corresponding position.

**Normative values**: Each component’s type is the corresponding positional type in the right-hand tuple (fixture: after `let (p, q) =
(1, 2)`, `p + q` participates in addition — part of the measured 53). This shares the same rules as tuple destructuring in function-parameter positions (`((idx, case)) =>`) and for-in heads (ALS-E19).

Tests: `spec/wasm_cross/for_in_forms.almd` (contract C-249), `spec/wasm_cross/tuple_ops.almd` (C-236’s `let (x, y) = t`).

<a id="als-e20-パイプと合成exprkindpipe--exprkindcompose"></a>

## ALS-E20 Pipes and composition (`ExprKind::Pipe` / `ExprKind::Compose`)

**Accepted forms**: `x |> f` (single stage, chain `x |> f |> g`, feeding into a HOF `xs |> list.map((x) => …) |> list.len`). Composition is `f >> g`; it returns a callable value, and `(f >> g)(x)` is `g(f(x))`.

**Normative values**: `x |> f` ≡ `f(x)` (observational equivalence; fixture: `5 |> double` → 10, `5 |> double |> inc` → 11, HOF chain → 3). `>>` applies left to right (4 into `double >> inc` → `inc(double(4))` = 9, identical on both targets).

Test: `spec/wasm_cross/pipe_compose_forms.almd` (contract C-250).

<a id="als-e21-if-letexprkindiflet"></a>

## ALS-E21 if let (`ExprKind::IfLet`)

**Accepted form**: `if let x = o { … } else { … }` — the binder is a **bare identifier**. `if let some(x) = o` is a parse error (no pattern in binder position; the design is Swift-style implicit unwrap).

**Normative values**: The subject is Option. For some, the inner value is bound to `x` and the then arm runs; for none, the else arm runs (fixture: some(5) → 10, none → "empty", identical on both targets).

Test: `spec/wasm_cross/if_let_forms.almd` (contract C-251).

<a id="als-st3-式文とコメントstmtexpr--stmtcomment"></a>

## ALS-ST3 Expression statements and comments (`Stmt::Expr` / `Stmt::Comment`)

**Accepted forms**: Expressions in statement position. Calls returning `Unit` (such as `println(…)`) directly become statements. Intentional discard of a pure expression’s value uses `let _ = e`.

**Must-use ruling**: Leaving a Result-returning call bare in statement position is **check-time E042** (ADR-0008); errors cannot be silently swallowed. There are only two exits, and the hint spells out both: `expr!` (propagation) and `let _ = expr` (explicit discard; err does not propagate — C-217). Negative cases are pinned by `tests/expr_stmt_diag_test.rs`.

**Accepted forms (comments)**: Line comments `// …` (line start or end), block comments `/* … */` (may span lines, before declarations or between statements).

**Normative values**: Comments are **semantically invisible**: no form at any position contributes any observable behavior (fixture: output with every form interspersed is identical to the comment-free version on both targets). fmt preserves comments (comment_map / doc_map carry them per declaration).

Tests: `spec/wasm_cross/expr_stmt_comment.almd` (contract C-252), `tests/expr_stmt_diag_test.rs` (negative cases).

<a id="als-st4-場所代入stmtindexassign--stmtfieldassign"></a>

## ALS-ST4 Place assignment (`Stmt::IndexAssign` / `Stmt::FieldAssign`)

**Accepted forms**: List element `xs[i] = v`, record field `p.x = v`, map key `m[k] = v` (overwrite existing keys, insert absent ones — upsert). The target is a `var` binding (place assignment to `let` violates immutability, the ALS-ST1 E009 family).

**Normative values**: A read after assignment observes the new value (fixture: 21 / 12 / 6 / 100, identical on both targets). Value semantics: assignment does not leak through sharing (COW). **An out-of-bounds indexed write aborts at runtime** (unified message + exit 1, C-067’s ruling).

Tests: `spec/wasm_cross/place_assign_ascription.almd` (contract C-253); out of bounds: `spec/wasm_cross/index_bounds_write_heap.almd` (C-067).

<a id="als-e22-型注釈式exprkindtypeascription"></a>

## ALS-E22 Type-annotated expressions (`ExprKind::TypeAscription`)

**Accepted form**: `(e: T)` — a type annotation in expression position.

**Normative values**: The value remains `e`; `T` supplies the expected type for checking (fixture: `(7: Int)` → 7). Runtime representation is unaffected.

Test: `spec/wasm_cross/place_assign_ascription.almd` (contract C-254 — shares the fixture with C-253).

<a id="als-e23-レコードexprkindrecord--exprkindspreadrecord--exprkindmember"></a>

## ALS-E23 Records (`ExprKind::Record` / `ExprKind::SpreadRecord` / `ExprKind::Member`)

**Accepted forms**: Literal `{ x: 1, y: 2, tag: "a" }` (type from declaration `type Pt = { … }` or context), field read `p.x`, spread update `{ ...p, y: 20 }` (unlisted fields are inherited).

**Normative values**: A field read returns a value of the declared type. Spread creates a **new value**, leaving **the original unchanged** (fixture: even after `q = { ...p, y: 20 }`, `p.y` is 2 — value semantics, no mutation observable through sharing). Field assignment is allowed only on `var` bindings (ALS-ST4).

Test: `spec/wasm_cross/record_forms.almd` (contract C-255).

<a id="als-e24-break-と-continueexprkindbreak--exprkindcontinue"></a>

## ALS-E24 break and continue (`ExprKind::Break` / `ExprKind::Continue`)

**Accepted forms**: `break` / `continue` in statement position inside a loop body (`while` / `for-in`), bare or guarded as `if cond then break` / `if cond then continue`.

**Normative values**: `continue` skips the rest of the current iteration and goes to the next (**for-range always executes the step**; lowering structurally excludes the infinite-loop trap caused by skipping it). `break` exits the loop immediately. **Mid-body break takes effect immediately at that position** (fixture: `if k > 3 then break; last = k` → 3 — the error that produced 4 by deferring it into a flag was fixed in #1306; v0, interp, v1 native, and v1 wasm all agree).

**3-way ruling**: This fixture intentionally lives in spec/wasm_cross. The #1306 discovery (both v1 legs agreeing on the same wrong value, invisible to the 2-way gate) is a class detectable only by 3-way voting with interp. A break crossing a heap frame still hits a loud wall because it skips per-iteration Drop.

Tests: `spec/wasm_cross/loop_break_continue.almd` (contract C-256), `spec/lang/loop_break_continue_test.almd` (wasm leg).

<a id="als-e25-エラー演算子exprkindunwrap--exprkindtooption--exprkindunwrapor--exprkindtry"></a>

## ALS-E25 Error operators (`ExprKind::Unwrap` / `ExprKind::ToOption` / `ExprKind::UnwrapOr` / `ExprKind::Try`)

**Accepted forms**: Postfix `e!` (context-dependent unwrap / propagation; includes fallible fn and fallible lambdas, not just effect fn; ADR-0008, [context-specific explanation](../../../../../specs/result-option-effect.md)), postfix `e?` (Result → Option conversion), `e ?? d` (Ok/Some value, or `d` on Err/None). Postfix `?` / `?.` cannot cross a newline (`?` at line start is a separate expression).

**Normative values**: `half(10)!` → 5 (Ok path; Err propagates to the caller, to ALS-R1’s abort form). `err(_)?` → `none`, `ok(4)?` → `some(4)` (fixture: -1 / 4). Consumption by `??` is shared with ALS-E9. Detailed propagation rules are pinned by ADR-0006/0008 contracts (C-216, C-217, C-222).

**Try ruling**: `ExprKind::Try` is **dead vocabulary never generated by the parser**; surface postfix `?` constructs `ToOption`. The node remains in the AST but has no syntactic counterpart (removal is a vocabulary-cleanup follow-up, like #1300’s CallArg::Label).

**`?.` is already specified by ALS-E28** (C-264 pins scalar cases). E28’s limitations section records the wasm-side wall for record subjects.

Test: `spec/wasm_cross/error_operators.almd` (contract C-257).

<a id="als-e26-呼び出しとラムダexprkindcall--exprkindlambda"></a>

## ALS-E26 Calls and lambdas (`ExprKind::Call` / `ExprKind::Lambda`)

**Accepted forms**: Named calls `f(a, b)` (positional arguments), lambdas `(x: Int) => e` (optional type annotation), `(x) => e`, zero-argument `() => e`, and inline lambdas in HOF argument positions. Consumption rules for effect calls are ALS-ST3/E25 (E041/E042).

**Normative values**: Arguments are evaluated once each, from the beginning in order, and passed by value (value semantics). Lambdas are function values: binding and calling one or passing it as an argument yields the same observations (fixture: 5 / 10 / 42 / 12, identical on both targets). Arity mismatch is check-time E004.

Test: `spec/wasm_cross/call_lambda_ctor.almd` (contract C-258).

<a id="als-e27-コンストラクタ参照exprkindtypename"></a>

## ALS-E27 Constructor references (`ExprKind::TypeName`)

**Accepted forms**: Case names from type declaration `type Color = Red | Green | Rgb(Int, Int, Int)`: payload cases use call form `Rgb(1, 2, 3)`; zero-argument cases use bare reference `Green`.

**Normative values**: Constructed variant values are consumed by match case patterns (fixture: `Rgb(r, _, b) => r + b` → 4, `Green` → 20, identical on both targets). Match exhaustiveness is ALS-E18 (E010).

**Constructors are function values** (#1111, yes/no ratification on 2026-08-05): A bare reference to a payload-carrying case name is a function value: `(A) -> T` for a one-argument case, `(A, B, …) -> T` for a multi-argument case (no partial application; use a lambda when currying is needed). A bare zero-argument case is the value itself, not a function value. Built-in constructors `some` / `ok` / `err` have the same status: bare `some` is a function value equivalent to `(x) => some(x)` (the parser synthesizes eta expansion; all legs use the lambda path, so target agreement is by construction). `[1, 2] |> list.map(some)` == `[some(1), some(2)]`, `list.map(xs, Circle)` == `list.map(xs, (x) =>
Circle(x))`.

Tests: `spec/wasm_cross/call_lambda_ctor.almd` (contract C-259), `spec/wasm_cross/ctor_fn_value.almd` (contract C-322), `spec/lang/ctor_fn_value_test.almd`. Or-patterns: `spec/wasm_cross/or_pattern.almd` (contract C-323), `spec/lang/or_pattern_test.almd`, `tests/diagnostics/e080-or-pattern-binder/`.

<a id="als-dl1-宣言declmodule--declimport--decltype--declfn--decltoplet--declprotocol--decltest--decltestwheredef"></a>

## ALS-DL1 Declarations (`Decl::Module` / `Decl::Import` / `Decl::Type` / `Decl::Fn` / `Decl::TopLet` / `Decl::Protocol` / `Decl::Test` / `Decl::TestWhereDef`)

**Accepted forms**: `module name` (optional, at the beginning) / `import mod` (required for non-auto-imported stdlib modules and external packages; ALS import rules are in the module-system spec) / `type T = Case | Case(payload)` and `type T = { fields }` / `fn f(…) -> T =
…` and `effect fn …` / top-level `let NAME = value` (referencable from fn bodies) / `protocol P { fn sig }` and `fn T.method(…)` implementations / `test "name" { … }` blocks and `local test where` / `mod test where` definitions.

**Normative values**: One fixture spans import (json parse→stringify round trip), variant types, top-level let (Int/String), and fn/effect fn, with identical results on both targets (300 / hi / careful / [1,2]). The fixture begins with a `module` header immediately below a comment block, a form made unformattable by #1323 (the fmt(valid)→invalid family); this also pins that module headers with comments pass fmt. Multi-module execution evidence remains the `spec/integration/modules/` group (run on every CI). Existing contracts C-094/C-126 (`protocol_ufcs_inferred_lambda.almd`) pin protocol-resolution rules. Test blocks are always executed on the wasm leg of `almide test` (all of spec/lang is evidence; `test where` is `spec/lang/test_where_test.almd`).

**Strict has been removed**: `strict <mode>` was a declaration that was merely accepted and consumed nothing (#1321). The ruling of 2026-08-13 **removed it from the grammar**: the language must not retain a form promising a checking mode that does nothing. Thereafter `strict` is an ordinary identifier; writing it as a declaration is a parse error.

**Record-field attributes**: Fields accept only four attributes: `@builtin`, `@location`, `@skip`, and `@skip_decode`. Any other attribute on a field is rejected with check error E053 (`unknown attribute `@<name>` on record field '<field>'`), dialect epoch 15. Attributes outside the fn attribute vocabulary remain E053 warnings as before.

Tests: `spec/wasm_cross/declaration_forms.almd` (contract C-260), `tests/diagnostics/e053-unknown-field-attribute-serde-skip/` and `tests/diagnostics/e053-unknown-field-attribute-redact/` (contract C-430).

<a id="als-e3-浮動小数点リテラルexprkindfloat-部分節"></a>

## ALS-E3 Floating-point literals (`ExprKind::Float`) — partial section

**Accepted forms (settled)**: Decimal fractions `1.5` and `0.5` (digits required on both sides of the decimal point; `.5`/`1.` are parse errors, measured), and negative zero `-0.0`. Exponent forms (`1e10`/`1E10`/`1.5e-3`) and underscore separators (`1_000.25`) **are accepted**, but cannot be pinned by fixtures because fmt reprints their spelling through their values; **normative spelling remains OPEN (awaiting the #1261 ruling)**. Handling of out-of-range literals (`1e999`) is a branch of the same issue (currently saturating to inf, with fmt producing unparseable `inf.0`).

**Normative values (settled)**: The type is binary64 (ALS-T2). Canonical display uses `float.to_string`; the fixture pins `1.5` / `0.5` / **`-0.0` (sign preserved)** / arithmetic result `1.75` identically on both targets.

Test: `spec/wasm_cross/string_float_stable.almd` (contract C-261).

<a id="als-e5-文字列リテラルexprkindstring-部分節"></a>

## ALS-E5 String literals (`ExprKind::String`) — partial section

**Accepted forms (settled)**: Double-quoted literals, empty string `""`, escapes `\t`, `\n`, `\\` (spellings stable under fmt, values pinned by the fixture). `\u{…}` (replaced by its value), heredoc `"""…"""` (collapsed to one line), and quote selection (automatic switching) are accepted, but fmt does not preserve their forms, so **normative spelling remains OPEN (#1263)**. Unknown escapes (`\q`) are rejected during checking with **E047** (unknown escape sequence; #1264 landed, measured on 0.59.1).

**Normative values (settled)**: `\t` → tab, `\n` → newline, `\\` → one backslash character (pinned by the fixture with visible delimiters). The empty string is a value of length 0.

Test: `spec/wasm_cross/string_float_stable.almd` (contract C-262).
<a id="als-dl2-回復ノードexprkinderror--stmterror"></a>

## ALS-DL2 Recovery nodes (`ExprKind::Error` / `Stmt::Error`)

**Ruling**: Both nodes are **recovery vocabulary**. They exist only to occupy parse-error positions so that collection of subsequent diagnostics can continue, and **never occur in accepted programs**. They have no syntactic counterparts (the same treatment as ALS-E25’s `Try`, except these are actually generated on error paths).

**Verification**: `tests/parser_recovery_test.rs` pins both directions: (1) **zero** recovery nodes in ASTs throughout the spec/wasm_cross corpus (accepted programs executed on both targets); (2) malformed files report errors while **parsing continues beyond the error** (later declarations remain visible).

Tests: `tests/parser_recovery_test.rs` (structural verification), `spec/wasm_cross/declaration_forms.almd` (accepted side, contract C-263).

<a id="als-e28-オプショナルチェーンexprkindoptionalchain"></a>

## ALS-E28 Optional chaining (`ExprKind::OptionalChain`)

**Accepted form**: `expr?.field` — postfix, without crossing a newline (`?` at line start is a separate expression). The subject is `Option[レコード]` (Option of a record).

**Normative values**: If the subject is `some(v)`, the result is `some(v.field)`; if `none`, it is `none` (fixture: `some({x:3,y:4})?.x ?? 0` → 3, `none?.x ?? -9` → -9, identical on both targets). The entire chain has type `Option[フィールド型]` (Option of the field type): `?.` returns “read if present” wrapped in Option (unwrapping the value is the job of `??`/match).

**Scope of C-264 and record of the old path**: C-264 covers scalar fields. The following heap-payload wall is a diagnostic record from when that contract was established; it does not mean that every String field is currently rejected. See also the `name: String` read in `spec/lang/unwrap_operators_test.almd`. The v1 wall for scalar fields was removed in #1329 (desugaring moved to the shared optimize cutpoint and synthesized helpers became Named calls, enabling lowering in all positions). **Heap payloads (String fields) remain on the existing path**: measurement showed that even a user-written helper of the same shape hits the `heap-result match は空の遅延ヒープ値を move out する` (a heap-result match moves out an empty deferred heap value) brick; this is not specific to OptionalChain. Removal awaits that brick.

Tests: `spec/wasm_cross/optional_chain_scalar.almd` (contract C-264), `spec/lang/optional_chain_test.almd` (4 wasm-leg cases).

<a id="als-st5-guard-文stmtguard"></a>

## ALS-ST5 guard statements (`Stmt::Guard`)

**Accepted form**: `guard cond else raise-expr` — in statement position. `raise-expr` is `err(e)` or `err(e)!` (both spellings normalize identically, #1336). This is not `if` without else but an **early exit**; subsequent statements belong only to the path where cond is true.

**Normative values**: If cond is true, execution passes through to subsequent statements (fixture: `checked(5)!` → 10). If false, the else arm’s err propagates to the caller, received by its `!` or match (fixture: match catches the err from `checked(-3)` and produces "must be positive"). Identical on both targets.

**Scalar/heap boundary**: Scalar-value guards are live on both legs as of #1336. `guard let x = e else …` (`Stmt::GuardLet`) is a **different class**: the frontend desugars it to a variant match, which had hit the tail-position variant-match brick. With ALS-ST6 (#1340 / C-266) landed, guard let has its own section; the Stmt::GuardLet coverage row points to ALS-ST6. The else arm accepts `continue` as well as raise (pinned by `spec/lang/guard_test.almd`).

Tests: `spec/wasm_cross/guard_statement.almd` (contract C-265), `spec/lang/guard_test.almd` (wasm leg).

<a id="als-st6-guard-let-文stmtguardlet"></a>

## ALS-ST6 guard let statements (`Stmt::GuardLet`)

**Accepted form**: `guard let x = scrut else else-expr` — in statement position. `scrut` is Result / Option; `x` binds the Ok / Some payload and is visible throughout subsequent statements. Besides raise (`err(e)` / `err(e)!`), the else arm accepts a **value form**, whose value is returned from the function (`guard let head = list.first(xs) else { d }`; `spec/lang/guard_let_test.almd` pins this accepted form).

**Normative values**: Success polarity binds and passes through (fixture: `int.parse("21")` → v=21 → 42). Failure polarity propagates the else arm’s err to the caller (fixture: match catches the err from `doubled("x")` and produces "not a number"). Identical on both targets. Chained guard let statements fold into a single carrier binding (#1340).

**Block-nesting ruling (discovery in #1340)**: The frontend desugars guard let into a **variant match nested in a block**. Bare tail match was already live on both legs, but one level of block nesting hit a wall because `auto_wrap_abi_body` retypes **only the root** of the body to a Result carrier. Thus the cause was **position**, not polarity or a wildcard arm; the fix was teaching variant match to the recognizer that normalizes into the proven bind form (another face of the same mechanism as ALS-ST5).

Tests: `spec/wasm_cross/guard_let_statement.almd` (contract C-266), `spec/lang/guard_let_test.almd` (wasm leg).

<a id="als-e29-二項演算子exprkindbinary"></a>

## ALS-E29 Binary operators (`ExprKind::Binary`)

**Accepted forms**: Arithmetic `+ - * / % **` (the exponentiation spelling `^` is synonymous; the cited fixture pins `2 ^ 10`, with the same value 1024 as `2 ** 10`), comparison `< <= > >=`, equality `== !=`, logical `and` / `or` (keywords; there is no `&&`/`||` — negation is `not`, ALS-E7).

**Int division/remainder ruling**: `/` **truncates toward zero** (fixture: `-7 / 3` → **-2**, `7 / 3` → 2); `%` takes **the dividend’s sign** (`-7 % 3` → **-1**). This is the C/Rust convention, not floor division. Division by zero aborts at runtime (C-001’s ruling).

**Overloading of `+`**: Besides numeric addition, it performs **String and List concatenation** (`"cat" + "dog"` → `catdog`, `[1,2] + [3]` has length 3). Mixed-type `+` is a check-time type error.

**Ruling on `**`**: Exponentiation has no hardware instruction, so it is desugared into a call to an internal stdlib power function (Float transcribes libm; Int uses exponentiation by squaring). The operator node is counted as one call in the caps ledger (mir==ir).

**Normative values**: All forms are byte-identical on both targets (the fixture pins 14 lines of observations). Equality semantics are type-specific: scalars use primitive comparisons; String / List / Value use deep comparison (ALS-E11, the `list.eq_*` family).

Test: `spec/wasm_cross/binary_operators.almd` (contract C-267).

<a id="als-e30-ホールと未実装マーカーexprkindhole--todo--placeholder"></a>

## ALS-E30 Holes and unimplemented markers (`ExprKind::Hole` / `Todo` / `Placeholder`)

**Accepted forms**: `_` (`ExprKind::Hole`) and `todo("メッセージ")` (`ExprKind::Todo`) in expression position, and **`_` in call-argument position** (`ExprKind::Placeholder`). The first two are **typed holes**: they pass type checking, with their types inferred from context (`let y: Int = _` is Int). The third is **not accepted** (ruling below).

**Normative values (native)**: Both holes and `todo` **abort if evaluation reaches them**: a Rust panic emits `not yet implemented: <メッセージ>` to stderr with **exit code 101**. A hole’s message identifies the Almide-side line (`hole at line 4`). `todo` carries its own string unchanged (even quotes do not break it through escaping, #1346). Output already produced before the abort remains (fixture: `io.print("before")` followed by `todo` → `before` on stdout, then 101).

**Ruling 1: This has no cross-target contract**. The wasm renderer **rejects at build time** functions containing holes/`todo` (loud wall). Rejection is also **independent of reachability**: measured programs containing `_ => todo(...)` in a match arm that is never evaluated terminate normally on native (exit 0), but wasm rejects the entire function as outside the subset. A `spec/wasm_cross/` fixture is therefore impossible in principle, and this section **does not claim identical behavior on both targets**. Holes are a native-only sketching workflow, not something to leave in shipped programs. The contract ledger covers only the part in ruling 2 below.

**Ruling 2: `_` in a call argument is check-time error E046** (contract C-268). `add(_, 10)` is **not** partial application: binding it gives it the return type; in pipe position it becomes an extra argument and produces E004. In other words, it was a form that allowed a “hole without a value” to flow straight into codegen (#1266-class invalid Rust). E046 identifies the position (which argument) and directs the user to a lambda (`(x) => add(x, 10)`). Since type checking runs before target selection, this error is identical on native and wasm (the same structure as `fan.timeout`’s E027 tombstone C-006).

**Ruling 3: interp abstains**. The native abort is panic (exit 101), while the R1 abort modeled by interp’s `Flow::Abort` is `Error: <msg>` + exit 1, a **different observation**. It is better for the third judge to abstain than to cast a wrong vote (almide-interp’s rule), so `Hole` / `Todo` return `Flow::Unsupported` with a reason.

Tests: `tests/diagnostics/e046-call-arg-placeholder/` (E046, contract C-268), `spec/lang/typed_hole_test.almd` and `tests/typed_hole_diag_test.rs` (native abort form and line numbers, message preservation for `todo`).

<a id="als-e31-スコープ付き領域exprkindscoped--scoped-fn"></a>

## ALS-E31 Scoped regions (`ExprKind::Scoped` / `scoped fn`)

**Accepted forms**:

```ebnf
scoped_block := "scoped" block
fn_decl      := [visibility] ["scoped"] ["effect"] "fn" name …
```

`scoped` is a **contextual keyword**. In expression position it is read as a block modifier only when `scoped` is immediately followed by `{` on the same line; in declaration position it is a modifier only when immediately followed by `fn` / `effect`. The **heads** of `match` / `while` / `for … in` are exceptions: the following `{` opens the construct’s own body, so `scoped` remains an identifier (`match scoped { … }` keeps the same meaning as before #1997). Thus existing programs containing an identifier named `scoped` keep their meaning; the dialect epoch does not increase.

**Normative values**: The value of `scoped { body }` is the value of `body`; the presence or absence of `scoped` **does not change observations (stdout, stderr, exit code)**. `scoped fn` is an ordinary function when called outside a region, and yields the same result as when called inside one. Thus the modifier declares a **memory-reclamation boundary**, not value semantics (contracts C-362, C-363).

**Reclamation rule**: Memory allocated for the block is reclaimed at the boundary. Implementation may differ by leg: the structural wasm leg saves allocator state at block entry and rewinds it at exit (`crates/almide-wasm/src/region.rs`); the native leg runs region clones (`__rgn_` twins) in an arena, and for closures that cannot be twinned, ownership release happens at the same closing point. **Reclamation itself is unobservable**; this section imposes only the value rules above and acceptance rules below on both targets. It claims neither an upper bound on peak memory, nor termination, nor an upper bound on allocations before the boundary.

**Acceptance rules (stage 1 fragment)**: Accepted forms comprise **scalars** (`Int` / `Float` / `Bool` / `Unit`), **managed algebraic data** — non-generic variant types declared in the same file, each case either unit or positional fields only, each field a scalar or a variant meeting the same conditions; records with scalar fields only; their tuples and `Option` — and **ordinary workers** and **directly tail-recursive workers**. Calls are allowed only to other `scoped fn`, scalar stdlib calls from `int` / `float` / `math` / `bool`, and constructors.

**Rejection rules**: Forms outside the fragment are rejected **at check time**. Checking precedes target selection, so decisions are identical for native and wasm (contract C-364):

- **E086** — a region value crosses the boundary. `{name} would outlive its scoped
  allocation` is accompanied by the two lines `= allocated at <file:line:col>` and `= scope ends at
  <file:line:col>`, and by `help:`.
- **E087** — an operation disallowed inside the region: calls to functions that are not `scoped`, globals, host resources (output, `effect fn`), unknown indirect calls, captures of outer heap values, and types outside the fragment. `<構文> is not permitted in a scoped
  {function,block}` is accompanied by a line stating what may be retained and a line identifying the requesting declaration.
- **E088** — recursion retaining a continuation. `recursive call retains the current scoped
  activation` is accompanied by a line stating what remains after the call. Single non-tail self-recursion and mutual recursion fall into this category. Tree traversal (two or more self-calls in one arm) is accepted as an ordinary worker because it cannot be rewritten into an accumulator.

`scoped effect fn` is grammatically accepted but rejected with E087 in stage 1: effects are host resources, and this fragment has no boundary finalization.

Tests: `spec/wasm_cross/scoped_region_value.almd` (contract C-362), `spec/wasm_cross/scoped_worker_in_and_out.almd` (contract C-363), `tests/diagnostics/e086-scope-escape-tail/` and other E086 / E087 / E088 negative fixtures (contract C-364).


## Reader note on source limitations

The early ALS-E3 UNWRITTEN note and the later partial section coexist in the source; the OPEN spelling issues remain unresolved here. ALS-E6 calls the native 1-tuple/interpolation issue a known limitation, whereas ALS-E8 marks that record historical and not revalidated as a current limitation. Both descriptions are retained without reconciliation. ALS-E28 limits the scalar-field contract and explicitly identifies the heap-payload wall as a historical diagnostic, not rejection of every current String field. ALS-E30 distinguishes native panic (exit 101), wasm build rejection regardless of reachability, and interpreter abstention, and makes no cross-target behavioral contract. ALS-E31 retains its stage 1 fragment and does not claim peak-memory bounds, termination, or allocation bounds before the boundary. Dates, version-specific measurements, implementation histories, and proof/fixture references are source claims, not fresh verification by this reader translation.
