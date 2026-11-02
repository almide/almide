<a id="als-b--the-bounded-profile-normative"></a>

# ALS §B — The Bounded Profile (normative)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](bounded.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bounded.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bounded.zh-CN.md)

> Authoritative source: [docs/specs/als/bounded.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot, not a replacement for the original ALS or checker/ledger/generator input; it is not automatically synchronized. Requirements, historical status, evidence and safety claims keep the source scope. No independent execution, measurement, certification or proof was performed for this edition. The source-language edition is a reader copy; other languages are translations.

> Source-scope note: the guarantees below are the dated normative requirements, including the explicit 0.58.0 implementation gap and the six stated limits, not independent certification of a current build. Module classifications and old fan syntax are preserved as written; no current API or effect status has been inferred. Float restrictions are provisional under both stated revision conditions.


> Last updated: 2026-10-26

> **Status**: normative (ADR-0017, decision of 2026-08-21). This chapter defines the **bounded profile**—a **subset** of the language, not a dialect. For a function marked `@bounded`, the implementation guarantees **as a checker verdict** that execution time and storage are **statically bounded**, its call graph is **closed**, and effects are **bounded by capabilities**. The verdict is observable (acceptance / rejection code); each section's rejection rules are executed through broken / fixed pairs in `tests/diagnostics/e07x-bounded-*/`. This chapter became normative ahead of the implementation (CONTRIBUTING: “requirements first”): the 0.58.0 release as of 2026-08-21 warns about `@bounded` as an unknown attribute (E053) and has no rejection rules, so this chapter's rejection tests are red against that release. That is the state in which the judge describes the release; they turn green when the implementation advances the pin.
>
> “Bounded” is one word in this language meaning **having an upper bound**: `fan.bounded(c) { … }` (ALS-DT2) bounds a computation **at runtime with a budget**, while `@bounded` (this chapter) bounds a function **at compile time by proof**. The mechanisms differ; the meaning is the same.
>
> **What this chapter does not express (self-declared limits, to avoid mistaking them for claims)**: (1) bounded-depth recursion (entirely prohibited, B6); (2) fixed-point and deterministic Float operations (Float operations are provisionally prohibited, B10—the conditions for lifting the prohibition are both normative status for the ALS-T19 family and a Float instruction set at the certification seat); (3) statically sized arrays (all heap containers are dynamically sized; B8 rejects construction with runtime lengths); (4) a memory bound in bytes (only an object-count bound); (5) early exits below the bound (`≤ B`) (B5/B11 prohibit them to preserve exact bounds); (6) traceability of functional correctness—the profile guarantees **safety** (memory, names, capabilities, bounds), not that a control law is **correct**.

<a id="als-b1-bounded-属性と有界プロファイル"></a>

## ALS-B1 The `@bounded` attribute and bounded profile

`@bounded` is an attribute on an `fn` / `effect fn` declaration, declaring that the function belongs to the bounded profile. It may appear **only on function declarations** (there is no syntactic sugar for placing it on module declarations). A `@bounded` function may call only `@bounded` functions and first-order functions in pure standard-library modules (B7); hence the call graph reachable from a `@bounded` function is closed. The attribute affects neither types nor values: a `@bounded` function remains directly callable as an ordinary function.

A `@bounded` function violating a profile rule is **rejected during type checking**. Diagnostic codes are **E070–E079** (reserved for this chapter), with messages of the form `<construct> is not admissible in a @bounded function` and the hint named by each section. Violations have no effect on functions without `@bounded` (the subset narrows only the region inside the attribute).

```almide
@bounded
fn scale(x: Int) -> Int = x * 3

test "a @bounded function is an ordinary function" {
  assert_eq(scale(4), 12)
}
```


Tests: `spec/wasm_cross/bounded_kernel.almd`, `spec/stdlib/bounded_profile_test.almd`.
Contracts: C-308.

<a id="als-b2-サブセットであって方言ではない"></a>

## ALS-B2 A subset, not a dialect

A program marked `@bounded` is **the same program** with every attribute removed: observable behavior (stdout, stderr, exit code) is unchanged by the attribute's presence (the same relationship as SPARK ⊂ Ada). The attribute changes only “what else the checker rejects.” `spec/wasm_cross/bounded_kernel.almd` and `bounded_kernel_plain.almd` are identical programs differing only in the attributes, and print the same output.

```almide
@bounded
fn grid_sum() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

fn grid_sum_plain() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

test "the attribute changes nothing observable" {
  assert_eq(grid_sum(), grid_sum_plain())
  assert_eq(grid_sum(), 18)
}
```


Tests: `spec/wasm_cross/bounded_kernel.almd`, `spec/wasm_cross/bounded_kernel_plain.almd`.
Contracts: C-309.

<a id="als-b3-回数付きループのみ"></a>

## ALS-B3 Counted loops only

Loops in a `@bounded` function are limited to **counted loops**: `for i in a..<b` / `for i in a...b` where `a` and `b` are **compile-time constants** (integer literals or bindings that can be constant-folded from literals). The iteration count is statically determined by the range length. Nesting is allowed (the bound is the product of the loop counts). `while`, `for x in xs` over a container, and ranges with runtime endpoints are rejected with **E070** (hint: `counted range`).

```almide
@bounded
fn triangle() -> Int = {
  var acc = 0
  for i in 1...10 { acc = acc + i }
  acc
}

test "counted loops with literal bounds are admissible" {
  assert_eq(triangle(), 55)
}
```


```almide check-fail=E070
@bounded
fn count_down(n: Int) -> Int = {
  var k = n
  while k > 0 { k = k - 1 }
  k
}

fn main() -> Unit = println("${count_down(3)}")
```


Tests: `tests/diagnostics/e070-bounded-unbounded-loop/`,
`tests/diagnostics/e070-bounded-for-over-container/`. Contracts: C-310.

<a id="als-b4-ループ内確保の禁止"></a>

## ALS-B4 No allocation inside loops

Expressions that **allocate a new heap object** in a counted-loop body—list / string / Map / Set construction, concatenation, `push`, and closure creation—are rejected with **E071** (hint: `allocate outside the loop`). Operations that only read, borrow, or increment the reference count of existing heap values (index reads, field reads, passing the same value) are allowed. Allocation outside the loop is allowed.

```almide
@bounded
fn dot(xs: List[Int], ys: List[Int]) -> Int = {
  var acc = 0
  for i in 0..<3 { acc = acc + (list.get(xs, i) ?? 0) * (list.get(ys, i) ?? 0) }
  acc
}

test "reading heap values inside a counted loop is admissible" {
  assert_eq(dot([1, 2, 3], [4, 5, 6]), 32)
}
```


```almide check-fail=E071
@bounded
fn squares() -> List[Int] = {
  var xs: List[Int] = []
  for i in 0..<8 { xs = xs + [i * i] }
  xs
}

fn main() -> Unit = println("${list.len(squares())}")
```


Tests: `tests/diagnostics/e071-bounded-alloc-in-loop/`. Contracts: C-311.

<a id="als-b5-break--continue-の禁止"></a>

## ALS-B5 No `break` / `continue`

A loop body in a `@bounded` function cannot contain `break` / `continue`—**E072** (hint: `single exit`). The iteration count is exactly the range length; early exit would weaken it to “at most the bound” and is therefore disallowed (the same discipline as MISRA's single-exit rule).

```almide check-fail=E072
@bounded
fn first_cube_over(limit: Int) -> Int = {
  var found = 0
  for i in 0..<100 {
    if i * i * i > limit then break
    found = i
  }
  found
}

fn main() -> Unit = println("${first_cube_over(100)}")
```


Tests: `tests/diagnostics/e072-bounded-break/`. Contracts: C-312.

<a id="als-b6-再帰の禁止--呼び出しグラフの非循環"></a>

## ALS-B6 No recursion—an acyclic call graph

The call graph reachable from a `@bounded` function must be a **DAG**: self-recursion and mutual recursion (back edges) are rejected with **E073** (hint: `call graph`). Unresolved callees (unresolved functions in another file) are **conservatively rejected** to avoid admitting hidden recursion edges. Recursive **types** (`Boxed`) are allowed, but recursive traversal is not.

```almide
@bounded
fn factorial_10() -> Int = {
  var acc = 1
  for i in 1...10 { acc = acc * i }
  acc
}

test "iteration replaces recursion" {
  assert_eq(factorial_10(), 3628800)
}
```


```almide check-fail=E073
@bounded
fn fact(n: Int) -> Int = if n <= 1 then 1 else n * fact(n - 1)

fn main() -> Unit = println("${fact(5)}")
```


Tests: `tests/diagnostics/e073-bounded-recursion/`. Contracts: C-313.

<a id="als-b7-呼び出し閉包--呼べるもの"></a>

## ALS-B7 Call closure—what may be called

A `@bounded` function may call only (a) `@bounded` functions and (b) **first-order** functions in pure standard-library modules (`int` `float` `string` `list` `map` `set` `math` `option` `result` `value` `json` `bytes` `regex` `matrix`, etc.). The following are rejected with **E074** (hint: `@bounded callee`): calls to user functions without `@bounded`, creation and invocation of closures / lambdas, higher-order functions (calls passing function values, such as `list.map(xs, f)`), function references, and dynamic dispatch through `any P`. (Why first-order only: indirect callees lose the static call graph, capabilities, and bounds.)

Variant constructor application (`Circle(r)`) is not a call. Like a record literal (`P { x: 1 }`) or tuple, it is a value-construction expression, outside B7 and not included in the loop allocations enumerated by B4. Every payload expression is checked against all rules.

```almide
type Shape = | Circle(Int) | Dot

@bounded
fn classify(n: Int) -> Shape = if n > 0 then Circle(n) else Dot

@bounded
fn radius(s: Shape) -> Int = match s {
  Circle(r) => r,
  Dot => -1,
}

test "a @bounded function builds variant values" {
  assert_eq(radius(classify(2)), 2)
  assert_eq(radius(classify(0)), -1)
}
```


```almide
@bounded
fn double(x: Int) -> Int = x * 2

@bounded
fn quadruple(x: Int) -> Int = double(double(x))

test "@bounded calls @bounded and first-order stdlib" {
  assert_eq(quadruple(5), 20)
  assert_eq(int.max(quadruple(1), 3), 4)
}
```


```almide check-fail=E074
fn helper(x: Int) -> Int = x + 1

@bounded
fn uses_plain(x: Int) -> Int = helper(x)

fn main() -> Unit = println("${uses_plain(1)}")
```


Tests: `tests/diagnostics/e074-bounded-unbounded-callee/`,
`tests/diagnostics/e074-bounded-higher-order/`, `spec/stdlib/bounded_profile_test.almd`
(constructors). Contracts: C-314.

<a id="als-b8-実行時長のヒープ構築の禁止"></a>

## ALS-B8 No heap construction with runtime lengths

Heap construction whose length or size is determined by a **runtime value**—`string.repeat(s, n)`, `list.with_capacity(n)`, `list.range(a, b)` with nonconstant `n`/`a`/`b`, and creation of runtime-length strings / lists—is rejected with **E075** (hint: `compile-time size`). Literals (`[1, 2, 3]`, `"abc"`) and constant-size construction are allowed. `push` outside a loop is allowed (B4 prohibits it inside loops).

```almide check-fail=E075
@bounded
fn pad(n: Int) -> String = string.repeat("x", n)

fn main() -> Unit = println(pad(3))
```


Tests: `tests/diagnostics/e075-bounded-runtime-length-heap/`. Contracts: C-315.

<a id="als-b9-効果と-capability"></a>

## ALS-B9 Effects and capabilities

A `@bounded effect fn` may have effects only within its declared capabilities; permitted effects are limited to standard output (the `println` family). The following calls are rejected with **E076** (hint: `declared capability`): effect modules `env` `fs` `http` `io` (except print) `net` `process` `random` `zlib`, modules reaching the host without `effect`—`datetime` `args` `mem` `testing`—and `fan.*` (nondeterministic scheduling).

```almide check-fail=E076
import env

@bounded
effect fn which_os() -> String = env.os()

effect fn main() -> Unit = println(which_os() ?? "unknown")
```


Tests: `tests/diagnostics/e076-bounded-effect-outside-capability/`. Contracts: C-316.

<a id="als-b10-浮動小数演算の禁止暫定"></a>

## ALS-B10 No floating-point operations (provisional)

Operators on Float (arithmetic / comparison) and calls to standard-library functions accepting Float inside a `@bounded` function are rejected with **E077** (hint: `Int arithmetic`). Merely holding and passing Float-typed values is allowed. This rule is **provisional** and will be revised when the ALS-T19 numeric-determinism family is normative **and** the certification seat has a Float instruction set—it will not be silently relaxed.

```almide check-fail=E077
@bounded
fn scale(x: Float) -> Float = x * 2.0

fn main() -> Unit = println(float.to_string(scale(1.5)))
```


Tests: `tests/diagnostics/e077-bounded-float-arithmetic/`. Contracts: C-317.

<a id="als-b11-早期脱出の制限"></a>

## ALS-B11 Restrictions on early exit

Early exits **inside the body** of a counted loop—propagation through `!` (effect fn), and `guard … else`—are rejected with **E078** (hint: `outside the loop`). Propagation and guards outside loops are allowed (early return from the whole function is bounded). (`?` converts Result → Option and is not an exit—ADR-0008.) The reason is the same as B5: exiting midway through the body skips per-iteration heap reclamation and breaks the exact iteration bound.

```almide
@bounded
fn sum_or_zero(xs: List[Int]) -> Int = {
  var acc = 0
  for i in 0..<3 { acc = acc + (list.get(xs, i) ?? 0) }
  acc
}

test "defaulting inside the loop is admissible" {
  assert_eq(sum_or_zero([4, 5]), 9)
}
```


```almide check-fail=E078
@bounded
effect fn sum_first(xs: List[Int]) -> Int! = {
  var acc = 0
  for i in 0..<3 { acc = acc + list.get(xs, i)! }
  ok(acc)
}

effect fn main() -> Unit = println("${sum_first([1, 2, 3]) ?? -1}")
```


Tests: `tests/diagnostics/e078-bounded-early-exit-in-loop/`. Contracts: C-318.

<a id="als-b12-md5sha-1-は-stdlib-にない"></a>

## ALS-B12 MD5 and SHA-1 are not in stdlib

`hash.md5`, `hash.md5_hex`, `hash.sha1`, and `hash.sha1_hex` are not public stdlib functions. As digests unsuitable for security use, they were removed at dialect epoch 15 with neither a deprecation period nor a migration package. Calls produce check-time error E002 in every profile; the hint comes from the removal row in `proofs/dialect-epochs.toml` (removed at epoch 15; rewrite to `hash.sha256(b)` or `hex.encode(hash.sha256(b))`. These are different digests). The removed set is derived from this ledger, not from a list of names. Spellings recorded as removed in the ledger never return to the public surface.

Tests: `tests/diagnostics/e002-epoch15-md5-removed/`,
`tests/removed_spellings_stay_removed_test.rs`. Contracts: C-424.
