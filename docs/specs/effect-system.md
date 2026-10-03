> Last updated: 2026-10-03

# Effect System

Almide's effect system enforces a hard boundary between pure computation and side-effecting operations. The compiler tracks effect context at the function level and rejects violations at compile time.

## 1. `fn` vs `effect fn`

Every function in Almide is either **pure** (`fn`) or **effectful** (`effect fn`). The `effect` modifier is part of the function signature and propagates through the call graph.

```almide
import fs

fn add(a: Int, b: Int) -> Int = a + b

effect fn read_config(path: String) -> Result[String, String] =
  fs.read_text(path)
```

A pure `fn` guarantees no I/O, no concurrency and no environment access, with one exception: it may write to stdout/stderr or abort through the six builtins of §2.1. An `effect fn` may perform any of these.

The checker sets `can_call_effect = true` when entering an `effect fn` body and `can_call_effect = false` for plain `fn`. This flag gates all effect-related operations.

Test: `spec/lang/effect_fn_test.almd`

## 2. Effect Isolation (E006)

A pure function cannot call an effect function. The compiler enforces this at every call site by checking the callee's `is_effect` flag against the caller's `can_call_effect` context.

```almide check-fail=E006
import http

effect fn fetch() -> Result[String, String] = http.get("https://example.com")

// Compile error E006: cannot call effect function 'fetch' from a pure function
fn bad() -> String = fetch()!
```

The diagnostic includes a secondary span pointing to the effect function's declaration site.

This rule applies uniformly to user-defined functions, stdlib effect functions (e.g., `fs.read_text`, `http.get`), and cross-module effect function calls.

Test: `spec/integration/modules/vis_effect_test.almd`

### 2.1 Output and abort builtins are admissible in a pure fn

*Added 2026-09-27 ([ADR-0022](../adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)). This writes down behaviour the checker has always had. It changes nothing.*

Six builtins may be called from a pure `fn`: **`println`, `eprintln`,
`panic`, `assert`, `assert_eq`, `assert_ne`**. The set is closed. Every other
output path is a stdlib effect fn and stays E006 from a pure fn, including
`io.print`, `io.write` and `io.write_bytes`.

1. **They write, or they abort. They never read.** `println` appends to stdout
   and `eprintln` to stderr. `panic` and a failing assert end the process
   (stderr block, exit 1; for the assert family the form is C-153). None of
   them returns information from outside the program, so a pure fn's result is
   still a function of its arguments.
2. **They do not make the caller effectful, and they do not propagate.** They
   return `Unit` (`println`, `eprintln`, the assert family) or `Never`
   (`panic`), never a `Result`. `println(s)!` is E034 in every context, a
   pure fn that prints keeps a pure signature in its module interface, and an
   effect fn calls it as a plain value, with no `!`.
3. **Served handlers (ADR-0020 §5.5 / §7.2).** Output from a pure helper that a
   handler calls follows the handler rule: each call's bytes reach their
   stream contiguously (a line is atomic), lines from one request keep program
   order, and the order of lines from different in-flight requests is
   unspecified.
4. **`fan` bodies.** A pure fn called from a `fan` arm prints as part of that
   arm. The exemption adds no ordering promise beyond §5's determinism
   contract: C-004 fixes list/arm order for `fan.any` and `fan.map`, while the
   interleaving of `fan.settle` thunks' side effects on native is outside the
   contract. `fan` itself still requires an effect context (E007).
5. **Reference evaluators.** The in-process interpreter and the judge's
   reference evaluator treat these six names as prelude builtins with no effect
   context. They evaluate them the same way wherever they are called, so the
   three-way oracle compares a pure fn's output the same way it compares an
   effect fn's. Only the checker enforces the pure/effect distinction.
6. **Buffering (C-162, and the stream statement in C-367).** A pure fn's output
   uses the same streams as an effect fn's. Native stdout is one buffer,
   flushed on every write when stdout is a terminal and 64 KiB-buffered
   otherwise; stderr is unbuffered. The cross-target promise is **per
   stream**: stdout bytes, stderr bytes and the exit code are byte-identical
   between native and wasm. How the two streams interleave when both go to one
   file is not promised. An abort flushes the stdout written before it.
7. **`scoped` is stricter.** `println` inside a `scoped fn` stays E087. A
   `@bounded` fn admits the output builtins (C-316).

```almide check
fn show(label: String, n: Int) -> Unit = println("${label}=${n}")

fn half(n: Int) -> Int = {
  assert_eq(n % 2, 0)
  eprintln("halving ${n}")
  n / 2
}

fn main() -> Unit = show("half", half(8))
```

A read from the same place is still E006:

```almide check-fail=E006
import fs

fn load(p: String) -> String = fs.read_text(p) ?? ""
```

`main` follows the same rule: a plain `fn main() -> Unit` is correct when
`main` performs no real effect. Write `effect fn main()` only when it reads,
writes a file, uses the network, uses `fan`, or propagates with `!`.

Tests: `tests/checker_test.rs` (`pure_fn_admits_output_and_abort_builtins`,
`pure_fn_still_rejects_a_real_read_with_e006`),
`spec/lang/pure_fn_output_builtins_test.almd`,
`spec/wasm_cross/pure_fn_output_and_assert_abort.almd` (C-153).

### 2.2 Stdlib readers are effect fns

*Dialect epoch 8 (#3248).* `io.read_byte`, `io.read_n_bytes` and
`process.args` read stdin or argv, and are `effect fn`s like `io.read_line`
and `env.args`. They were plain `fn`s, so a pure fn could read the outside
world through them; a call from a pure fn is now E006. The `args` module's
argv readers are the one exception still pending its own ruling (#848).

```almide check-fail=E006
import io

fn first_byte() -> Int = io.read_byte()
```

Tests: `tests/diagnostics/e006-io-read-byte-in-pure-fn/`,
`tests/diagnostics/e006-io-read-n-bytes-in-pure-fn/`,
`tests/diagnostics/e006-process-args-in-pure-fn/`.

### 2.3 `@pure`: the empty effect set (E092)

*Added with #3250.* `@pure` on a fn is checked: the fn, and everything it
calls, has no effect category (§8), no output and no declared abort. It is
E092 when the fn is an `effect fn` or an `@extern`, or when anything it
reaches calls one of the six builtins of §2.1, a stdlib function that carries
a category, or an `@extern` fn. The error is located at the call that leads
there and names the path.

A call through a fn-typed parameter is judged where the argument is written,
so `@pure fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)` is pure. A
language-defined trap — division by zero, an index out of bounds — is not a
declared abort and does not count.

```almide check
@pure
fn area(w: Int, h: Int) -> Int = w * h

fn main() -> Unit = println("${area(2, 3)}")
```

```almide check-fail=E092
@pure
fn area(w: Int, h: Int) -> Int = {
  println("w=${w}")
  w * h
}
```

ADR-0027 (draft) proposes a `pure fn` modifier with this meaning and retiring
the attribute in its favour; `@pure` means the same set, so the two never
disagree.

Tests: `tests/diagnostics/e092-pure-calls-println/`,
`tests/diagnostics/e092-pure-reaches-assert-via-helper/`,
`tests/diagnostics/e092-pure-calls-categorised-stdlib/`,
`tests/diagnostics/e092-pure-on-effect-fn/`.

## 3. Return Type Wrapping

In the Rust target, `effect fn` return types are lifted to `Result[T, String]` during codegen if they are not already `Result`. The `ResultPropagationPass` performs this transformation:

1. If an effect fn declares `-> T` where T is not Result, the codegen rewrites it to `-> Result[T, String]`
2. The body's tail expression is wrapped in `ok(...)`
3. Already-Result return types (e.g., `-> Result[Int, String]`) are left unchanged

```almide
// Source: returns String
effect fn greet(name: String) -> String = "hello ${name}"

// After ResultPropagationPass (Rust codegen): returns Result<String, String>
// Body becomes: Ok("hello ${name}".to_string())
```

The type checker handles this flexibility through `constrain_effect_body`, which accepts:

- `Unit` body (control-flow returns via guard)
- Unwrapped `T` (auto-wrapped to `ok(T)`)
- Full `Result[T, E]` (passed through as-is)

Test: `spec/lang/effect_fn_test.almd` -- `safe_div`, `require_positive`

## 4. Explicit Propagation: the `!` Operator

Almide uses the `!` operator for explicit Result/Option unwrapping with error propagation. Inside an `effect fn`, `expr!` unwraps the value and propagates the error to the caller.

```almide
effect fn add_strings(a: String, b: String) -> Result[Int, String] = {
  let x = int.parse(a)!   // unwrap or propagate err
  let y = int.parse(b)!
  ok(x + y)
}
```

### How it works

**Type checker (`infer.rs`):** The `Unwrap` expression extracts the inner type -- `Result[T, E]` becomes `T`, `Option[T]` becomes `T`. Implicit narrowing of un-annotated `let` bindings was REMOVED with ADR-0008 (v0.55.0): a fallible call yields a Result VALUE in every position, the formerly-implicit sites are the hard errors E041/E042, and `let _ = f()` is the sanctioned discard (C-217). See docs/specs/result-option-effect.md §3.

**Codegen (`pass_result_propagation.rs`):** The `ResultPropagationPass` translates `!` into `?` (Rust's try operator). For match subjects, Try is **not** inserted -- you match on `ok`/`err` variants directly.

```almide
fn require_positive(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("must be positive")

effect fn classify(s: String) -> Result[String, String] = {
  let n = int.parse(s)!           // ! propagates parse error
  let valid = require_positive(n)! // ! propagates validation error
  ok(if valid > 100 then "big" else "small")
}

test "each ! is a propagation point" {
  assert_eq(classify("500"), ok("big"))
  assert_eq(classify("-1"), err("must be positive"))
  assert_eq(classify("x"), err("invalid digit found in string"))
}
```

Test: `spec/lang/effect_fn_test.almd` -- `parse_num`, `add_strings`, `double_parsed`, `classify_str`

## 5. `fan` Blocks

`fan` blocks execute expressions concurrently. They have two restrictions enforced at compile time:

### E007: `fan` requires effect context

A `fan` block can only appear inside an `effect fn` or `test` block. Using `fan` in a pure `fn` produces error E007.

```almide check-fail=E007
effect fn compute_a() -> Result[Int, String] = ok(1)
effect fn compute_b() -> Result[Int, String] = ok(2)

// Compile error E007: fan block can only be used inside an effect fn
fn bad() -> (Int, Int) = fan { compute_a(), compute_b() }
```

The same check applies to `fan.map()` and `fan.any()` calls via `static_dispatch.rs`.

### E008: No mutable variable capture

A `fan` block cannot capture `var` bindings from the enclosing scope. This prevents data races. Only `let` bindings may be captured.

```almide check-fail=E008
effect fn use(n: Int) -> Result[Int, String] = ok(n)

effect fn bad() -> Result[Unit, String] = {
  var count = 0
  // Compile error E008: cannot capture mutable variable 'count' inside fan block
  let (a, b) = fan { use(count), use(count) }
  ok(())
}
```

### Type behavior

A `fan` ALL-block / `fan.settle` yields its tuple (no Result wrapper — nothing to unwrap). The Result-yielding forms (`fan.any`, the mappers, `fan.race`) follow ADR-0008: the Result is a VALUE, and a binding spells its propagation explicitly (`let v = fan.any { ... }!`). Test: `spec/wasm_cross/fan_any_early_winner.almd` and the fan fixtures migrated at the 0.55 switch.

```almide
effect fn add(a: Int, b: Int) -> Result[Int, String] = ok(a + b)
effect fn mul(a: Int, b: Int) -> Result[Int, String] = ok(a * b)

effect fn example() -> Result[Unit, String] = {
  let (sum, product) = fan {
    add(3, 4)    // Result[Int, String] -> Int
    mul(3, 4)    // Result[Int, String] -> Int
  }
  // sum: Int, product: Int
  assert_eq(sum, 7)
  assert_eq(product, 12)
  ok(())
}

test "the tuple carries the unwrapped values" {
  assert_eq(example(), ok(()))
}
```

### Determinism contract (C-004)

`fan` の並行 API は**決定的核**を契約として持つ(native ⇄ wasm byte-identical、
契約台帳 C-004、fixture: `spec/wasm_cross/fan_deterministic.almd`,
`spec/wasm_cross/fan_pure_thunks.almd`):

- `fan.race` は 0.42.0 で撤去された(E027 トンボストーン)。決定的モデルの下では
  `thunks[0]()` そのものであり、名前だけが言語にないタイミング意味論を約束していた。
  thunk[0] **のみ**が評価される: 副作用順序も決定的。
  (Go の select が仕様で RANDOM を要求するのと対照的な設計選択。)
- `fan.any(thunks)` — リスト順に逐次試行し最初の Ok。副作用順序も決定的。
- `fan.settle(thunks)` — **結果リストの順序のみ**を契約する。native は実スレッドで
  実行するため thunk 内副作用の交互順は wall-clock(契約外)。wasm は逐次。
- `fan.timeout` — **存在しない**(0.29.0 で削除、C-006)。壁時計デッドラインは可搬な
  意味を持たないため、参照は check 時 tombstone エラー(E027)。ホスト境界で課す。

### Thunk typing

`race` / `any` / `settle` の thunk は純粋(`fn() -> T`)でも effect
(`fn() -> Result[T, String]`)でもよい — 非 Result thunk は FanLowering が
両ターゲット共通で `Ok` アダプタに包む(v0.27.3+、#514)。
**`fan.map` だけは mapper が明示的に Result を返す必要がある**
(`(x) => ok(x * 10)`)— mapper の戻り型推論は thunk と違い困難なため、
純粋 mapper は check 時に拒否される(意図的な非対称、診断改善は #547)。

Test: `spec/lang/fan_test.almd`, `spec/lang/fan_map_test.almd`, `spec/wasm_cross/fan_pure_thunks.almd`, `spec/wasm_cross/fan_block_err_list_order.almd` (C-199), `spec/wasm_cross/fan_sibling_trap.almd` (C-200)

## 6. `test` Blocks

Test blocks are implicitly in effect context. The checker sets `can_call_effect = true` when entering a test body, and the lowerer emits test functions with `is_effect: true`.

This means test blocks can:

- Call effect functions directly
- Use `fan` blocks
- Use the `!` operator for unwrapping

```almide
import fs

effect fn load_config() -> Result[String, String] = ok("port=8080")

test "calls effect functions directly" {
  let content = load_config()!
  assert(string.len(content) > 0)
  assert_eq(fs.exists("/nonexistent/almide-doctest"), false)
}
```

Test functions do not return `Result` -- they return `Unit`. An effect fn called inside a test yields its EXPLICIT Result value: unwrap it with `!` (panics the test on err), or consume it with `??` / an ok/err match. See docs/specs/result-option-effect.md §5.

Test: `spec/lang/effect_fn_test.almd` -- all test blocks call effect functions directly

## 7. Cross-Module Effects

Effect function signatures are preserved across module boundaries. When module A exports an `effect fn`, any module that imports A can call it -- but only from an effect context.

```almide project
// file: effectlib/mod.almd
effect fn read_config() -> Result[String, String] = ok("config_value")
fn pure_() -> String = "pure"
// file: main.almd
import effectlib

effect fn main() -> Unit = {
  let config = effectlib.read_config()!  // ok: effect context — the Result is a value, `!` propagates
  assert_eq(config, "config_value")
  assert_eq(effectlib.pure_(), "pure")   // pure fn also callable
}

test "the imported effect fn keeps its signature" {
  assert_eq(effectlib.read_config(), ok("config_value"))
  assert_eq(effectlib.pure_(), "pure")
}
```

The `is_effect` flag is part of the function signature stored in `TypeEnv.functions` and propagated through module interfaces. The E006 check at call sites works identically for local and imported functions.

Test: `spec/integration/modules/vis_effect_test.almd`

## 8. Permissions

The `[permissions]` section in `almide.toml` restricts which effect categories a package may use. This is Security Layer 2 of Almide's capability system.

### Configuration

```toml
[permissions]
allow = ["IO", "Net"]
```

If `[permissions]` is absent or `allow` is empty, all capabilities are permitted (backwards compatible).

### Effect categories

The `EffectInferencePass` classifies each call into one of six categories. The
classification is one table, `STDLIB_MODULE_EFFECTS` in
`crates/almide-ir/src/effect.rs`; a gate beside it refuses a stdlib module with
no row, and an `@intrinsic` whose runtime symbol classifies unlike the function
that declares it (#3246).

| Category | What carries it |
|----------|-----------------|
| `IO` | `fs`, `io` (files and the standard streams) |
| `Net` | `net`, and the `effect fn`s of `http` (its builders and router are pure) |
| `Env` | `env`, `process`, `args` |
| `Time` | the `effect fn`s of `datetime` (`now`, `monotonic_ns`); the calendar math is pure |
| `Rand` | `random` |
| `Fan` | `fan` |

Every other stdlib module — `path`, `url`, `json`, `regex`, `zlib`, the
collections and numerics — carries no category.

A call to an **`@extern` fn is every category** (⊤, #3245). Its body is foreign,
inference cannot see into it, and no bound can be declared on it yet (ADR-0027
§5 proposes one), so it passes `[permissions]` only when `allow` lists all six.

Tests: `tests/effect_permissions_test.rs`; the table's gate is the
`classification_gate` module in `crates/almide-ir/src/effect.rs`.

### Unknown names

`allow` accepts exactly these six names. Any other name, including a
different case (`io`), is an error on the manifest line that writes it, before
any command runs (#3247):

```
error: almide.toml:6: unknown capability `Fil` in [permissions].allow — grantable capabilities are IO, Net, Env, Time, Rand, Fan
  hint: did you mean `IO`?
```

The hint names the nearest capability, the same way `almide check --profile
critical --allow` reports an unknown name (`docs/specs/cli.md`).

Test: `tests/manifest_permissions_test.rs`, `tests/diagnostics/permissions-unknown-capability/`

### Enforcement

After codegen, the compiler runs `EffectInferencePass` to compute per-function transitive effects (direct stdlib calls + effects from called functions, via fixpoint iteration). If any function uses a capability not listed in `allow`, the build fails with a capability violation error.

```almide project build-fail=capability
// file: almide.toml
[package]
name = "permdemo"
version = "0.1.0"

[permissions]
allow = ["IO"]
// file: main.almd
import http

// This function uses Net (http.get) -> build error
effect fn fetch() -> Result[String, String] = http.get("https://example.com")
//   error: capability violation in `fetch`
//     Net is not in [permissions].allow
```

The gate runs after codegen: `almide check` and `almide test` accept the file,
`almide build` refuses it.

### Design layers

- **Layer 1** (implemented): `effect fn` vs `fn` -- the type checker enforces this
- **Layer 2** (implemented): `[permissions].allow` in almide.toml -- restricts stdlib capabilities per package
- **Layer 3** (future): Consumer restricts dependency capabilities

Test: Effect inference unit tests in `crates/almide-codegen/src/pass_effect_inference.rs` (`#[cfg(test)]` module)

## 9. Error Codes

| Code | Name | Trigger | Fix |
|------|------|---------|-----|
| E006 | Effect isolation violation | Pure `fn` calls an `effect fn` | Mark the caller as `effect fn` |
| E007 | Fan block in pure function | `fan { ... }` used outside effect context | Mark the enclosing function as `effect fn` |
| E008 | Mutable variable capture in fan | `fan` block references a `var` binding | Change `var` to `let`, or copy the value into a `let` before the `fan` |
| E092 | `@pure` fn is not pure | A `@pure` fn reaches output, an abort, a categorised stdlib call or an `@extern` | Move the call to a caller, or remove `@pure` |
