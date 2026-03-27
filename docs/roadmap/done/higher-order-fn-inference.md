<!-- description: Type inference for higher-order functions returning closures -->
# Higher-Order Function Type Inference

**Test:** `spec/lang/function_test.almd`
**Status:** 0 rustc errors, 6 checker errors

## Current State

```almide
fn adder(n: Int) -> (Int) -> Int = (x) => x + n

test "higher-order function" {
  let add3 = adder(3)
  assert_eq(add3(10), 13)  // checker: expected Int but got fn() -> Int
}
```

## What's Broken

The checker sees `adder(3)` as returning `fn(Int) -> Int`. When `add3(10)` is called, it should recognize `add3` as a callable value (function type) and apply the argument. Instead it reports "type mismatch in method call: expected Int but got fn() -> Int".

## Why It Happens

The error says "method call" — this suggests the parser might create a `Member` access instead of a `Call` in some cases. Or the checker's UFCS resolution interferes.

Possible scenarios:
1. `add3(10)` is parsed correctly as `Call { callee: Ident("add3"), args: [10] }` — but the checker's `check_call` for `Ident` looks up `add3` as a function name (not found in `env.functions`), then falls to the variable lookup path
2. The variable lookup finds `add3: fn(Int) -> Int` — but the call inference doesn't properly handle calling a variable with function type
3. The checker's `_` catch-all in `check_call` constrains `callee_type = Fn { params: [Int], ret: ?N }` which should unify with `fn(Int) -> Int` — but the `from_ty` conversion may lose the Fn structure

## Investigation Needed

1. Verify AST: `echo 'fn id(x: Int) -> Int = x; fn main() -> Unit = { let f = id; println(int.to_string(f(42))) }' | ./target/debug/almide --emit-ast`
2. Check if `check_call` for `Ident("add3")` finds the variable type `fn(Int) -> Int` and returns the return type
3. Check if the issue is specifically in double-call `adder(3)(10)` vs single `f(10)` where `f` is bound

## Expected Result

```almide
let add3 = adder(3)   // add3: (Int) -> Int
assert_eq(add3(10), 13)  // should type-check and evaluate to 13
```

## Proposed Fix

In `check_call` (`src/check/calls.rs`), the `Ident` handler should check if the name resolves to a variable with `Fn` type. If so, constrain the function type against the arguments and return the result type. Currently it only checks `env.functions` and `env.top_lets`.

```rust
ast::Expr::Ident { name, .. } => {
    // ... existing function/top_let lookup ...
    // Fallback: check if name is a variable with Fn type
    if let Some(var_ty) = self.env.lookup_var(name) {
        if let Ty::Fn { params, ret } = var_ty {
            // Constrain args against params, return ret
        }
    }
}
```

**Effort:** ~15 lines in `src/check/calls.rs`
