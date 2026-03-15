# Remaining 5 Codegen Issues — 36/41 → 41/41

Status: 36/41 spec/lang tests pass. These 5 require design-level fixes.

---

## 1. Generic Variant Type Instantiation (`type_system_test`)

**Current state:**
`type Maybe[T] = | Just(T) | Nothing` + `let y: Maybe[Int] = Nothing()` generates:
```rust
let y = Maybe::Nothing;  // Rust error: type annotations needed for Maybe<_>
```

**What's broken:**
The checker resolves `Nothing()` to `Maybe(TypeVar("T"))` — the generic param is NOT instantiated to `Int` even though the let binding has an explicit `Maybe[Int]` annotation. The lower pass stores the unresolved type in the IR var table. The codegen sees `Maybe<T>` and `contains_typevar` blocks the type annotation.

**Why it happens:**
- Checker's `check_named_call` for variant constructors returns `Ty::Named(type_name, vec![])` — empty generic args
- The let binding's annotation `Maybe[Int]` is resolved correctly, but the constraint between annotation and value doesn't propagate the `Int` into the constructor's return type
- `InferTy::from_ty(&Ty::Named("Maybe", []))` → `InferTy::Concrete(Ty::Named("Maybe", []))` — no inference variables to solve

**Expected result:**
```rust
let y: Maybe<i64> = Maybe::Nothing;
```

**Proposed fix:**
In `check_named_call` for variant constructors (`src/check/calls.rs`), when the constructor belongs to a generic type, create fresh inference variables for the type params and return `Ty::Named(type_name, [Var(?N), ...])` instead of `Ty::Named(type_name, [])`. This allows the constraint solver to unify `Maybe(?N)` with `Maybe(Int)` from the annotation.

**Effort:** ~20 lines in `src/check/calls.rs`

---

## 2. Recursive Type Box Insertion (`eq_protocol_test`)

**Current state:**
`type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])` generates:
```rust
pub enum Tree<T> {
    Leaf(T),
    Node(Tree<T>, Tree<T>),  // Rust error: recursive type has infinite size
}
```

**What's broken:**
Rust requires `Box<Tree<T>>` for recursive type members. The codegen emits direct recursive references without indirection.

**Why it happens:**
- The IR `IrTypeDeclKind::Variant` stores variant fields as direct `Ty` references
- The Rust codegen (`lower_rust.rs` lines 50-66) converts fields directly via `lty()` without checking for self-reference
- No cycle detection exists in the codegen pipeline

**Expected result:**
```rust
pub enum Tree<T> {
    Leaf(T),
    Node(Box<Tree<T>>, Box<Tree<T>>),
}
```

**Proposed fix:**
In `lower_rust.rs` when emitting enum variants, detect self-referential fields (where the field type contains the enum's own name) and wrap them in `Box<T>`. Also need to wrap construction sites in `Box::new()` and add `*` dereference at pattern match sites.

Steps:
1. `emit_enum_def`: if field type contains the enum name → `Type::Box(inner)`
2. `lower_expr` for `Record/Constructor` with recursive args → `Expr::Call { func: "Box::new", args: [inner] }`
3. `lower_pat` for recursive patterns → add deref

**Effort:** ~50 lines across lower_rust.rs, lower_rust_expr.rs, render.rs (add Box type + Box::new expr)

---

## 3. Guard `ok(value)` Value Loss in Effect Do-Block (`error_test`)

**Current state:**
```almide
effect fn hamming_distance(a: String, b: String) -> Result[Int, String] = {
  // ...
  do {
    guard i < len else ok(count)   // count is Int variable
    // ...
  }
}
```
Generates:
```rust
if !(j < almide_rt_list_len(&xs)) { return Ok(()) };
//                                            ^^ should be Ok(count)
```

**What's broken:**
The guard's `else ok(count)` produces `return Ok(())` instead of `return Ok(count)`. The `count` variable reference is lost during lowering.

**Why it happens:**
Suspected: the guard's else expression `ok(count)` goes through auto-try processing which strips the `Ok` wrapper, then the guard codegen re-wraps it in `Ok(...)` but with the wrong inner value. Or the IR lower pass for the guard's else expression loses the variable reference when `ok()` is involved.

**Investigation needed:**
1. Check what IR the guard `else ok(count)` produces — is it `ResultOk { expr: Var(count) }` or `ResultOk { expr: Unit }`?
2. Trace through `lower_stmt` → `Guard` handler → `self.lower_expr(else_)` to see where `count` gets replaced with `()`

**Expected result:**
```rust
if !(i < len) { return Ok(count) };
```

**Proposed fix:**
Debug the IR lower pass for `guard else ok(expr)` in effect functions. The issue is likely in how the guard's else expression is lowered when it contains `ok()` — the auto-try or Ok-wrap logic may interfere.

**Effort:** ~10 lines once root cause identified, but investigation needed

---

## 4. Higher-Order Function Type Inference (`function_test`)

**Current state:**
```almide
fn adder(n: Int) -> (Int) -> Int = (x) => x + n
// Usage: adder(3)(10)  → checker error: expected Int but got fn() -> Int
```

**What's broken:**
The checker sees `adder(3)` as returning `fn(Int) -> Int`. When this is used in a method call context (UFCS `adder(3)(10)`), the checker tries to find a method `(10)` on type `fn(Int) -> Int`, which fails. It should recognize that a function-typed value followed by `(args)` is a function call, not a method call.

**Why it happens:**
- `check_call` handles `callee(args)` but when `callee` is itself a call result (`adder(3)`), the outer call `adder(3)(10)` is parsed as `Call { callee: Call { callee: adder, args: [3] }, args: [10] }`
- The inner call `adder(3)` is inferred first → type is `fn(Int) -> Int`
- The outer call has callee type `fn(Int) -> Int` and args `[10]` → should work via the generic call handler
- But the checker's `check_call` for non-Ident/non-TypeName callees goes to the `_` catch-all which constrains callee type to `Fn { params: arg_tys, ret: fresh_var }`
- The constraint `fn(Int) -> Int` = `Fn { params: [Int], ret: ?N }` should unify, giving `?N = Int`

**Investigation needed:**
Check if the issue is in the checker or the test. The checker error says "type mismatch in **method call**" — this suggests the parser creates a `Member` + `Call` instead of nested `Call`. If `adder(3)(10)` is parsed as `Call { callee: Member { object: Call { callee: adder, args: [3] }, field: "10" } }`, that's a parser bug.

**Expected result:**
```almide
let f = adder(3)
let result = f(10)  // → 13
```

**Proposed fix:**
1. First: verify the AST structure of `adder(3)(10)` — is it nested Call or Member+Call?
2. If parser issue: fix postfix parsing to recognize `expr(args)` as a call, not member access
3. If checker issue: ensure the `_` catch-all in `check_call` properly handles callable types

**Effort:** ~15 lines in parser or checker, depending on root cause

---

## 5. Open Record / Structural Typing (`open_record_test`)

**Current state:**
```almide
fn greet(who: { name: String, .. }) -> String = "hello ${who.name}"
```
Generates:
```rust
pub fn greet_named(who: Named) -> String {  // error: cannot find type Named
```

**What's broken:**
Open records (`{ name: String, .. }`) use structural subtyping — any record with at least the specified fields is accepted. The codegen emits `Named` as the type, which doesn't exist in Rust. Rust has no structural typing.

**Why it happens:**
- The IR function param type is `Ty::TypeVar("Named")` or `Ty::OpenRecord { fields: [...] }`
- `lower_ty` converts `TypeVar("Named")` → `Type::Named("Named")` — which doesn't exist as a Rust struct
- Open record structural typing has no direct Rust equivalent

**Expected result:**
For the Rust target, open records need to be monomorphized — each call site generates a concrete struct type. Or use a trait-based approach:
```rust
trait HasName { fn name(&self) -> &str; }
fn greet(who: &impl HasName) -> String { format!("hello {}", who.name()) }
```

**Proposed fix (two approaches):**

**A. Monomorphization (simpler, already partially done in `src/mono.rs`):**
- For each call to a function with open record params, create a specialized version with the concrete struct type
- `greet({ name: "Alice", age: 30 })` → `greet_AlmdRec0(rec)` where `AlmdRec0` has all fields

**B. Trait-based (idiomatic Rust, more complex):**
- Generate traits for each field combination: `trait HasName { fn name(&self) -> String; }`
- Implement the trait for each concrete struct
- Function params use `impl HasName`

The monomorphizer (`src/mono.rs`) already exists and handles row-polymorphic functions. The issue is that it doesn't fully monomorphize open record params for all call sites.

**Effort:** ~100 lines to fix the monomorphizer, or ~200 lines for the trait approach

---

## Priority Order

1. **#4 Function test** — likely a parser/checker issue, smallest code change
2. **#1 Type system** — checker fix for generic variant instantiation, ~20 lines
3. **#3 Error test** — guard value loss, needs investigation but small fix
4. **#2 Eq protocol** — Box insertion, well-defined algorithm
5. **#5 Open record** — largest scope, monomorphizer fix
