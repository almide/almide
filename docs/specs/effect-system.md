> Last updated: 2026-03-28

# Effect System

Almide's effect system enforces a hard boundary between pure computation and side-effecting operations. The compiler tracks effect context at the function level and rejects violations at compile time.

## 1. `fn` vs `effect fn`

Every function in Almide is either **pure** (`fn`) or **effectful** (`effect fn`). The `effect` modifier is part of the function signature and propagates through the call graph.

```
fn add(a: Int, b: Int) -> Int = a + b

effect fn read_config(path: String) -> Result[String, String] =
  fs.read_text(path)
```

A pure `fn` guarantees no I/O, no concurrency, no environment access. An `effect fn` may perform any of these.

The checker sets `can_call_effect = true` when entering an `effect fn` body and `can_call_effect = false` for plain `fn`. This flag gates all effect-related operations.

Test: `spec/lang/effect_fn_test.almd`

## 2. Effect Isolation (E006)

A pure function cannot call an effect function. The compiler enforces this at every call site by checking the callee's `is_effect` flag against the caller's `can_call_effect` context.

```
effect fn fetch() -> Result[String, String] = http.get("https://example.com")

// Compile error E006: cannot call effect function 'fetch' from a pure function
fn bad() -> String = fetch()!
```

The diagnostic includes a secondary span pointing to the effect function's declaration site.

This rule applies uniformly to user-defined functions, stdlib effect functions (e.g., `fs.read_text`, `http.get`), and cross-module effect function calls.

Test: `spec/integration/modules/vis_effect_test.almd`

## 3. Return Type Wrapping

In the Rust target, `effect fn` return types are lifted to `Result[T, String]` during codegen if they are not already `Result`. The `ResultPropagationPass` performs this transformation:

1. If an effect fn declares `-> T` where T is not Result, the codegen rewrites it to `-> Result[T, String]`
2. The body's tail expression is wrapped in `ok(...)`
3. Already-Result return types (e.g., `-> Result[Int, String]`) are left unchanged

```
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

## 4. Auto-`?` Propagation and the `!` Operator

Almide uses the `!` operator for explicit Result/Option unwrapping with error propagation. Inside an `effect fn`, `expr!` unwraps the value and propagates the error to the caller.

```
effect fn add_strings(a: String, b: String) -> Result[Int, String] = {
  let x = int.parse(a)!   // unwrap or propagate err
  let y = int.parse(b)!
  ok(x + y)
}
```

### How it works

**Type checker (`infer.rs`):** The `Unwrap` expression extracts the inner type -- `Result[T, E]` becomes `T`, `Option[T]` becomes `T`. The `auto_unwrap` flag (set to `true` in effect fn bodies) also allows `let` bindings to automatically narrow `Result[T, E]` to `T` when no explicit type annotation is given.

**Codegen (`pass_result_propagation.rs`):** The `ResultPropagationPass` translates `!` into `?` (Rust's try operator). For match subjects, Try is **not** inserted -- you match on `ok`/`err` variants directly.

```
effect fn classify(s: String) -> Result[String, String] = {
  let n = int.parse(s)!           // ! propagates parse error
  let valid = require_positive(n)! // ! propagates validation error
  ok(if valid > 100 then "big" else "small")
}
```

Test: `spec/lang/effect_fn_test.almd` -- `parse_num`, `add_strings`, `double_parsed`, `classify_str`

## 5. `fan` Blocks

`fan` blocks execute expressions concurrently. They have two restrictions enforced at compile time:

### E007: `fan` requires effect context

A `fan` block can only appear inside an `effect fn` or `test` block. Using `fan` in a pure `fn` produces error E007.

```
// Compile error E007: fan block can only be used inside an effect fn
fn bad() -> (Int, Int) = fan { compute_a(); compute_b() }
```

The same check applies to `fan.map()` and `fan.race()` calls via `static_dispatch.rs`.

### E008: No mutable variable capture

A `fan` block cannot capture `var` bindings from the enclosing scope. This prevents data races. Only `let` bindings may be captured.

```
effect fn bad() -> Result[Unit, String] = {
  var count = 0
  // Compile error E008: cannot capture mutable variable 'count' inside fan block
  let (a, b) = fan { use(count); use(count) }
  ok(())
}
```

### Type behavior

Each `fan` expression's Result type is auto-unwrapped: `Result[T, E]` becomes `T`. A single-expression `fan` returns `T`; multiple expressions return a tuple `(T1, T2, ...)`.

```
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
```

Test: `spec/lang/fan_test.almd`, `spec/lang/fan_race_test.almd`, `spec/lang/fan_map_test.almd`

## 6. `test` Blocks

Test blocks are implicitly in effect context. The checker sets `can_call_effect = true` when entering a test body, and the lowerer emits test functions with `is_effect: true`.

This means test blocks can:

- Call effect functions directly
- Use `fan` blocks
- Use the `!` operator for unwrapping

```
test "reads a file" {
  let content = fs.read_text("test.txt")!
  assert(string.len(content) > 0)
}
```

Test functions do not return `Result` -- they return `Unit`. When an effect fn's return type is lifted to `Result` by the codegen, calls to that function inside tests are auto-unwrapped via `.unwrap()` rather than `?`.

Test: `spec/lang/effect_fn_test.almd` -- all test blocks call effect functions directly

## 7. Cross-Module Effects

Effect function signatures are preserved across module boundaries. When module A exports an `effect fn`, any module that imports A can call it -- but only from an effect context.

```
// effectlib module:
effect fn read_config() -> Result[String, String] = ok("config_value")
fn pure_() -> String = "pure"

// consumer:
import effectlib

effect fn main(_args: List[String]) -> Result[Unit, String] = {
  let config = effectlib.read_config()  // ok: effect context
  assert_eq(config, "config_value")
  assert_eq(effectlib.pure_(), "pure")  // pure fn also callable
  ok(())
}
```

The `is_effect` flag is part of the function signature stored in `TypeEnv.functions` and propagated through module interfaces. The E006 check at call sites works identically for local and imported functions.

Test: `spec/integration/modules/vis_effect_test.almd`

## 8. Permissions

The `[permissions]` section in `almide.toml` restricts which effect categories a package may use. This is Security Layer 2 of Almide's capability system.

### Configuration

```toml
[permissions]
allow = ["IO", "Net", "Log"]
```

If `[permissions]` is absent or `allow` is empty, all capabilities are permitted (backwards compatible).

### Effect categories

The `EffectInferencePass` maps stdlib module usage to seven categories:

| Category | Stdlib modules |
|----------|---------------|
| `IO` | `fs`, `path` |
| `Net` | `http`, `url` |
| `Env` | `env`, `process` |
| `Time` | `time`, `datetime` |
| `Rand` | (reserved) |
| `Fan` | `fan` |
| `Log` | `log` |

### Enforcement

After codegen, the compiler runs `EffectInferencePass` to compute per-function transitive effects (direct stdlib calls + effects from called functions, via fixpoint iteration). If any function uses a capability not listed in `allow`, the build fails with a capability violation error.

```
// almide.toml: allow = ["IO"]
// This function uses Net (http.get) -> build error
effect fn fetch() -> Result[String, String] = http.get("https://example.com")
//   error: capability violation in `fetch`
//     Net is not in [permissions].allow
```

### Design layers

- **Layer 1** (implemented): `effect fn` vs `fn` -- the type checker enforces this
- **Layer 2** (implemented): `[permissions].allow` in almide.toml -- restricts stdlib capabilities per package
- **Layer 3** (future): Consumer restricts dependency capabilities

Test: Effect inference unit tests in `src/codegen/pass_effect_inference.rs` (`#[cfg(test)]` module)

## 9. Error Codes

| Code | Name | Trigger | Fix |
|------|------|---------|-----|
| E006 | Effect isolation violation | Pure `fn` calls an `effect fn` | Mark the caller as `effect fn` |
| E007 | Fan block in pure function | `fan { ... }` used outside effect context | Mark the enclosing function as `effect fn` |
| E008 | Mutable variable capture in fan | `fan` block references a `var` binding | Change `var` to `let`, or copy the value into a `let` before the `fan` |
