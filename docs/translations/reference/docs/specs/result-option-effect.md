# Result, Option, Effect — complete specification

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](result-option-effect.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](result-option-effect.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](result-option-effect.zh-CN.md)



> Authoritative source: [docs/specs/result-option-effect.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/result-option-effect.md), fixed at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot translation, not automatically synchronized. The original specifications and their stated authority remain unchanged. Model proofs, implementation claims, historical status and evidence bounds retain their source scope; this translation supplies no independent execution, measurement, conformance or safety proof. Code and comments inside fences remain verbatim.

> Last updated: 2026-10-31
> Fully revised based on the 287-cell measured matrix of 2026-08-05 (#1122). Design decisions come from ADR-0002–0009. All body text describes “working code”, and each section names verification tests.
> 2026-08-13: §8.0 (two-layer model) and §9 (non-goals) promoted into the text from ADR-0012 D1/D4.
> 2026-09-27: L3 revised to ADR-0021 (E of a lambda's failure channel); non-goal 4 in §9 withdrawn.

> Source caveat: the §2 table broadly renders propagation from Option as `err("none")`; the separate [effect-call specification](../../../../specs/effect-fn-call-semantics.md) distinguishes an enclosing Option channel, which propagates `none`. The table below preserves its original scope and wording rather than silently reconciling the two sources. The measured-matrix and working-code claims above are historical source claims, not an independent run performed for this translation.

## 1. Types

### Result[T, E]
```text
ok(v)   : Result[T, E]   // 成功値
err(e)  : Result[T, E]   // エラー値
```

### Option[T]
```text
some(v) : Option[T]       // 値あり
none    : Option[T]       // 値なし
```

### Option sugar `T?` (ADR-0010)

`T?` ≡ `Option[T]`. Valid in **all type positions** (`!` is a return-position marker, whereas `?` is an attribute of a value).
`?` binds most tightly to the immediately preceding type atom (a name plus generics, or a parenthesized type), and does not cross `->`:

```almide
fn f(v: Int?) -> Int? = v                     // 引数・戻りどちらも可
type Slots = {
  f: (Int) -> Int?,                           // fn 型 slot: Option[Int] を返す fn
  on_tick: ((Int) -> Unit)?,                  // fn 値そのものが Option — 括弧必須
  pair: (String, Int)?,                       // Option[タプル]
  nested: (Int?)?,                            // 入れ子(`Int??` は ?? にレクスされ不可)
}
fn g(s: String) -> Int?! = ok(int.parse(s)?)  // Result[Option[Int], String](? が先、! は戻りマーカー)

test "T? in every type position" {
  assert_eq(f(some(1)), some(1))
  assert_eq(g("7"), ok(some(7)))
  assert_eq(g("x"), ok(none))
}
```

The canonical form is `T?`: `almide fmt` normalizes `Option[T]` to `T?` (D3). The stdlib has also been normalized with `fmt --no-import-edit` (2026-08-07; spec/lang/option_marker_test.almd witnesses the unification).

### Never (bottom type)
```text
process.exit(n) : Never   // 戻らない関数の戻り値型
```
Never is assignable to any type. It can be used in guard else, if then, and match arms.

### Fallibility markers `-> T!` / `-> T!E`

In return position in fn declarations and function types, `T!` is sugar for `Result[T, String]`, and `T!E` for `Result[T, E]`. Fallibility and effect are two orthogonal axes; all four quadrants can be written:

```almide
fn        total()      -> Int  = 1          // pure ・総
fn        fallible()   -> Int! = ok(1)      // pure ・可謬(= Result[Int, String])
effect fn e_total()    -> Int  = 1          // effect・総(暗黙 lift、§3)
effect fn e_fallible() -> Int! = ok(1)      // effect・可謬

test "the four quadrants" {
  assert_eq(total(), 1)
  assert_eq(fallible(), ok(1))
  assert_eq(e_total(), ok(1))       // 呼び出し型は lift 後の Result(§3)
  assert_eq(e_fallible(), ok(1))
}
```

Bare `T!` defaults to the error type `String`. Spell a custom error type as `T!MyError` or explicit `Result[T, MyError]` (ADR-0012 D2). Function types `(A) -> B!` / `(A) -> B!E` are also legal (ADR-0009 D1). The marker cannot be used outside function return position, such as the ordinary value type annotation `let x: Int! = ...`.
The `!` operator propagates inside the body, and **value tails are automatically lifted to ok(...)** (Phase 1b — the same ergonomics as effect fn lifting; Result-typed exits are unchanged):

```almide
fn parse_port(s: String) -> Int! = int.parse(s)     // パススルー(既に Result)
fn double_port(s: String) -> Int! = int.parse(s)! * 2  // 値 tail → ok(...) に lift
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! が T! 本体で伝搬
  guard n > 0 else err("must be positive")
  n                                                  // 値 tail lift(ok(n) でも可)
}
```

Tests: `spec/lang/fallible_marker_test.almd`, `spec/lang/fallible_typed_error_test.almd`, `spec/lang/fallible_lambda_test.almd`.

## 2. Operators

All value-level operators are defined as desugarings into named stdlib functions (ADR-0005). Definition table:

```text
x ?? d      ≡  option.unwrap_or_else(x, () => d)   // Option operand
r ?? d      ≡  result.unwrap_or_else(r, (_) => d)   // Result operand
r?          ≡  result.to_option(r)
o?.x        ≡  option.map(o, (v) => v.x)
```

`!` is control flow (early return), with no function implementation (the explicit boundary of ADR-0005).

### `expr!` — unwrap with propagation

| Input type | Output | On failure |
|---|---|---|
| `Result[T, E]` | `T` | Propagate err to the enclosing function's failure channel |
| `Option[T]` | `T` | Convert to `err("none")` and propagate (lossless embedding — ADR-0003 D3) |
| Inside a test | `T` | Unwrap (panic fails the test) |

Contexts where `!` is legal (**not limited to effect fn** — C-211):

1. An effect fn body.
2. A test block.
3. **A pure fn whose return type resolves to Result / Option / `T!`**.

Other contexts produce `E022`. An operand that is neither Option nor Result (`5!`) produces `E034`. E is not converted at a propagation point — a mismatch is a check error (ADR-0003, implemented in #1103). When E agrees, custom error types propagate structurally and can be matched (ADR-0003 D1).

```almide
fn twice(s: String) -> Result[Int, String] = ok(int.parse(s)! * 2)   // pure + !
```

Tests: `spec/lang/unwrap_operators_test.almd`, `spec/lang/fallible_marker_test.almd`, `spec/wasm_cross/pure_bang_propagation.almd`.

### `expr?` — Result → Option conversion

| Input type | Output type |
|---|---|
| `Result[T, E]` | `Option[T]` (err → none; E is discarded) |
| `Option[T]` | `Option[T]` (identity — no diagnostic) |

Conversion affects **only the outermost layer**: `Result[Result[T, E], E]?` is `Option[Result[T, E]]`. It does not flatten (flattening belongs to `result.flatten` / `option.flatten`). An operand that is neither Option nor Result produces `E034`.

### `expr ?? fallback` — unwrap with fallback

| Input type | Output type |
|---|---|
| `Result[T, E]` | `T` (fallback on err; E is discarded) |
| `Option[T]` | `T` (fallback on none) |

- **Lazy evaluation**: fallback is evaluated only for none / err (`o ?? expensive()` does not call expensive() if o is some). This follows from the definition `≡ unwrap_or_else` (ADR-0005).
- **The fallback type unifies with the unwrapped T at check time** (mismatch is E001 — `n ?? "hello"` and `n ?? some(1)` are check errors).
- **Precedence is highest (postfix family)**: `a ?? 1 + 2` is `(a ?? 1) + 2`. This is **the opposite of C-style `?:` (near the bottom)**; use parentheses when combining it with arithmetic.
- **Chains nest to the right**: `a ?? b ?? c` is `a ?? (b ?? c)` (observed in the AST). The meaning is “first success wins”.
- **No line crossing (E038)**: at statement level, put the fallback on the same line as `??`. Like an infix operator, `??` itself can begin a continuation line (#3155): `xs\n  |> list.first\n  ?? 0`. For a multiline fallback on the next line, use parentheses + postfix `??`:
  ```almide
  fn parse_or_minus_five(s: String) -> Int = {
    let v = (int.parse(s) ??
      -5)
    v
  }
  ```
- An operand that is neither Option nor Result produces `E034`.

Tests: `spec/lang/unwrap_operators_test.almd`, `tests/diagnostics/e038-qq-line-crossing/`, `tests/diagnostics/e034-off-type-bang/`.

### `expr?.field` — optional chaining (Option only)

```almide
type P = { x: Int }
fn getx(o: Option[P]) -> Int = o?.x ?? 0    // some(P{x:5}) → 5、none → 0

test "optional chaining" {
  assert_eq(getx(some(P { x: 5 })), 5)
  assert_eq(getx(none), 0)
}
```

Safely access a field of `Option[レコード]` (Option[record]), returning `Option[フィールド型]` (Option[field type]).
**Option only** — applying it to Result is refused with a dedicated diagnostic (compose from Result as `(r?)?.x`).

Tests: `spec/lang/unwrap_operators_test.almd`.

## 3. effect fn

### Declaration and lifting

```almide
import fs

effect fn read_file(path: String) -> String = fs.read_text(path)!
```

| Almide | Generated Rust |
|---|---|
| `effect fn f() -> T` | `fn f() -> Result<T, String>` (implicit lift) |
| `effect fn f() -> Result[T, E]` | `fn f() -> Result<T, E>` (no double wrapping) |
| `fn f() -> T` | `fn f() -> T` (no conversion) |

### All propagation is explicit (ADR-0008 — auto-`?` removed)

Effect fn bodies once had position-dependent implicit propagation (auto-`?`): it worked in 5 positions and not in 5 others, with behavior reversing depending on annotations or pattern shapes. **The mechanism was removed entirely in 0.55**. A fallible call produces **a Result value in every position**; write `!` to propagate.

| Spelling | Meaning |
|---|---|
| `let v = int.parse(s)!` | Propagation (v is Int; early return on err) |
| `let r: Result[Int, String] = int.parse(s)` | Retain the Result value |
| `let _ = fail()` | **Intentional discard** — err does not propagate (C-217) |
| `let v = int.parse(s)` | **E041** (all former implicit positions are errors, with a hint to insert `!`) |
| `fail()` (statement position) | **E042** (must-use — hints the two spellings `!` or `let _ =`) |
| `match get() { ok(v) => …, err(e) => … }` | Result is an ordinary value — match normally |
| `match get()! { 42 => … }` | Match at the value layer (`!` makes the layer explicit) |

`e()?` / `e() ?? fb` are ordinary ADR-0005 value operations (no exceptional rule): `e()?` ≡ Result→Option conversion; `e() ?? fb` ≡ unwrap_or_else.

Regression tests: `spec/lang/explicit_propagation_test.almd` (removal of two hazards + A/B for `let _`), `spec/wasm_cross/let_wildcard_discard.almd` (C-217), `spec/wasm_cross/effect_option_explicit_bang.almd` (C-216).

### Lambda fallibility (ADR-0006 D1 / ADR-0009 — #1108 Phase 2b)

A lambda is a “mini fallible fn”: `!` in its body enters **the lambda's own failure channel** (`Result[T, E]`; E is decided by L3 below), and does not cross the closure boundary (the #489 invariant is preserved). Fallibility is inferred from use (L1–L9, ratified 2026-08-07):

```almide
fn halve(n: Int) -> Int! = if n % 2 == 0 then ok(n / 2) else err("odd")
fn retry(op: (Int) -> Int!) -> Int! = op(1)!          // fn 型 slot の `!`(L7/L8)

fn demo(xs: List[String]) -> Int! = {
  let g = (x) => int.parse(x)! * 2      // g: (String) -> Result[Int, String] — 第一級
  let doubled = g("21") ?? -1           // 呼べば Result 値、普通に消費
  let halves = list.map(xs, (s) => halve(int.parse(s)!)!)!   // 複合可謬 callback → first-err 形(L6)
  ok(doubled + list.sum(halves))
}

test "fallible lambdas (in fn bodies — test bodies keep unwrap semantics, L9)" {
  assert_eq(demo(["4", "6"]), ok(47))
  assert_eq(demo(["4", "7"]), err("odd"))
  assert_eq(retry((n) => ok(n + 1)), ok(2))
}
```

- The channel's E (ε) is determined by ADR-0021 (L3): first the context into which the lambda flows (a typed slot `(A) -> B!E`, typed `let`, a fn propagating the result, or consumption such as `err(e) => err(e)`). If the context does not determine it, join the E of `!` operands in the body (and `err(..)` in guards / value joins): if all agree, use that E; otherwise String. String is top (any E enters String via its repr string — printing `"${e}"`, #2725), so the join always has an answer. When ε is fixed to a typed E, a `!` with another E produces E022 at that site. An Option operand's none becomes err("none") (L4); value tails lift to ok(...) (L5).
- **Exception inside test blocks**: a lambda's `!` remains unwrap, and HOF dispatch remains the total form (L9 — the test world retains the entire pre-#1108 semantics).
- Bit transparency to a plain `(A) -> B` slot (user HOF) is unimplemented. E005 carries a targeted hint naming two resolution spellings (Phase 2b-iii).

Tests: `spec/lang/fallible_lambda_test.almd` (pins L1–L9), `spec/lang/typed_error_callback_channel_test.almd` (L3 — all #2601 spellings, typed slots and typed `let`), `tests/diagnostics/e022-callback-bangs-disagree/`, `tests/diagnostics/e022-typed-slot-callback-string-bang/`.

## 4. fn main

| Almide | Rust codegen | Notes |
|---|---|---|
| `fn main() = ...` | `fn main()` | `fn` effect rules. For the output/assert/panic builtin exceptions, see ADR-0022 / [effect-system.md](../../../../specs/effect-system.md) §2.1 |
| `effect fn main() -> Unit = ...` | `fn main() -> Result<(), String>` | Automatic lift |

- **main takes no arguments.** Command-line arguments come from `env.args()` (argv without the program name).
- An unhandled Err reaching main gives **exit 1 + stderr `Error: <msg>` on both targets** (Display, without quotes). An Option none propagated with `!` (`err("none")`) follows the same route and agrees across targets. The old statement about “a remaining divergence where wasm silently exits 0” has been resolved: measurements confirmed exit 1 + `Error:` on both targets.

Tests: `tests/wasm_runtime_test_parts/p3.rs::unhandled_main_error_terminates_consistently`, `spec/lang/result_option_matrix_test.almd`.

## 5. Test blocks

```almide
fn f() -> Int = 42

test "name" {
  assert_eq(f(), 42)
}
```

| Property | Value |
|---|---|
| `is_effect` | `true` (can call effect fn) |
| Result wrapping | None |
| Result of an effect call | **Explicit Result value** (auto-`?` does not operate in tests) |
| Behavior of `!` | Unwrap (panic fails the test) |

Calling an effect fn inside a test returns the Result as-is: unwrap with `!` or handle with `??` / match (the old “tests do not need !” statement is wrong).

Failure output for `assert_eq` / `assert_ne` is based on Almide repr. Operands use the ALS-R2 display form escaped by C-418's rules, shared across native / wasm execution and test routes (not Rust `Debug`). Regression test: `tests/assert_operand_escape_lanes_test.rs`.

Tests: `spec/lang/expr_test.almd` and all other `*_test.almd`.

## 6. guard else

```almide
import process
import env

fn positive(n: Int) -> Int! = {
  guard n > 0 else err("must be positive")    // guard condition else diverge_expr — else は戻りチャネルへ
  ok(n)
}

effect fn main() -> Unit = {
  let n = list.len(env.args()!)
  guard n > 0 else process.exit(3)             // else: Never(process.exit)
  println(int.to_string(n))
}

test "guard else returns the else value" {
  assert_eq(positive(0), err("must be positive"))
  assert_eq(positive(2), ok(2))
}
```

- The condition is constrained to Bool.
- The type of `else_expr` is **checked at check time** (#1118):
  1. Unify with the function's return channel (declared type for pure; lifted Result also allowed for effect fn).
  2. `Never` (process.exit / panic family).
  3. `break` / `continue` inside a loop.
  A type mismatch (`else "nope"` / `else { println(..) }` in an `-> Int` fn) is E001.
- Even with a constant condition, the else effect is not lost (`guard false else process.exit(3)` exits 3 — miscompilation fixed in #1117, pinned to the absolute result by `tests/guard_exit_gate_test.rs`).

Tests: `spec/lang/fallible_marker_test.almd` (guard else err), `tests/guard_exit_gate_test.rs`.

## 7. Match exhaustiveness

Missing match arms, including Result / Option (no err / none arm), are **hard error E010**, not a warning, with a hint to add an arm.

Tests: `tests/diagnostics/` (E010-family fixtures).

## 8. Error-value doctrine (ADR-0004)

### 8.0 The two-layer model — choosing the layer (ADR-0012 D1)

Choosing error type `E` is **layer assignment**, not taste. This paragraph is the standard rule; previously it required reconstructing the rule from four documents: ADR-0002 (channels), 0003 (conversion), 0004 (doctrine), and 0012 (terminal state).

| Layer | `E` | Channel purpose | When to choose it |
|---|---|---|---|
| **Erased layer (default)** | `String` | **Reporting** — read by people and LLMs | Default; whenever the caller does not branch on content |
| **Refined layer (opt-in)** | variant | **Branching** — the program changes behavior based on content | When content-based branching is wanted (ADR-0004 D1), and the domain is **closed** |

- Context prefixing in the **erased layer** uses the canonical `map_err` form, separated by `": "` (ADR-0004 D2).
- The **refined layer's** “closed domain” means inside the **module or package that can fully take care of that error** (the same scope choice as SE-0413's three conditions). Once it leaves the domain, the reason to retain a variant disappears.
- **Demotion** (variant `E` → String) is **always visible as `map_err` at the boundary** (ADR-0003). There is no implicit conversion hook, so the place of demotion remains in the code.

This is stated as the **terminal state**. Overturning it requires **measured evidence** named by the Falsifiers in ADR-0004 / ADR-0012, not just a new opinion.

### 8.1 Rules

- Error values converge on String (no tags or error sets introduced).
- **Do not branch on error-message text**: branch conditions using `string.contains(e, ...)` / equality produce warning E035. If branching is needed, define variant E and match its structure.
- A `map_err` lambda that does not use its error argument produces warning E036 (intentional discard is `(_) =>`).
- Canonical context prefixing: `r |> result.map_err((e) => "context: ${e}")` (separator `": "`).
- Collect all errors with `result.partition`. `result.collect` / `collect_map` have been removed (ADR-0007).
- fs “absence” is a value: `fs.read_text_if_exists(p)! ?? default` (C-215).

Tests: `tests/diagnostics/e035-*/`, `e036-*/`, `spec/stdlib/fs_if_exists_test.almd`.

## 9. Non-goals — considered and rejected (ADR-0012 D4)

These are not “not done yet”, but things **decided against**. All were reconfirmed using 2026 measurements and practitioner reports from other languages (sources in ADR-0012 References). They are listed because the same proposals return periodically; a returning proposal must rebut the reasons below.

1. **Stdlib `E` stays String**. MoonBit's latest recommended parse API itself uses plain `raise` + message strings, and whether to structure it remains undecided in its repository (core PR #3997). Unison's ecosystem converged on a comprehensive `Failure`.
2. **No conversion hook (equivalent to `From`), union / row / error set**. Koka's row inference leaks internal type-variable names into user-facing errors (2025 practitioner report); Roc's open/closed unions are its hardest inference concept; Zig error sets depend on closed-world compilation; Rust's provider API was replaced and discarded by RFC 3885 (2026-02).
3. **Propagation stays explicit `!`** (ADR-0008). MoonBit makes propagation implicit and compensates with IDE underlining — “the IDE automatically *underlines* functions that may throw”. That compensation **does not exist in raw text**, the only interface available to an LLM or diff reviewer.
4. ~~A lambda's failure channel stays String~~ — **withdrawn by ADR-0021** (see L3). Determine it in the two-stage order context → operand join → String, without row variables or generalization.
5. **`main`'s `E` stays String**. Boundary demotion via `map_err` is the visibility point; removing it makes demotion invisible.

Tests: `spec/lang/fallible_marker_test.almd`, `tests/diagnostics/e035-*/`, `e036-*/`, `spec/lang/fallible_typed_error_test.almd` (the implemented `T!E` specification is in §1).
