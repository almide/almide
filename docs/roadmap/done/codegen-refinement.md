<!-- description: Small independent optimizations improving generated Rust code quality -->
# Codegen Refinement

Small, independent optimizations that improve generated Rust code quality. Each is low-difficulty and can be implemented independently.

## 1. let mut → let Refinement

**Problem:** Variables declared with Almide's `var` emit `let mut` in Rust. Some are never reassigned after initial binding.

**Fix:** Post-emission analysis (or IR-level pass) that checks whether a `var` binding is ever assigned to after its initial `let`. If not, emit `let` instead of `let mut`.

**Implementation:**
- In `lower.rs`: track whether each `VarId` with `Mutability::Var` has any `IrStmtKind::Assign` targeting it
- In codegen: emit `let` if no assignment exists

**Impact:** Eliminates `unused mut` warnings in generated Rust. Cleaner output.

## 2. `#[inline]` Hints

**Problem:** Small helper functions (1-3 expressions) don't carry `#[inline]` hints. While LLVM can inline them at `-O2`, at lower opt levels the call overhead remains.

**Fix:** Emit `#[inline]` for:
- Functions with a body that is a single expression (no blocks)
- Generated lambda wrappers
- Stdlib runtime helper functions in `core_runtime.txt`

**Impact:** Better performance at `opt-level=1` (`almide run`).

## 3. Constant Folding

**Problem:** Compile-time-constant arithmetic is emitted verbatim:
```rust
// Almide: let x = (1 + 2) * 3
let x: i64 = ((1i64 + 2i64) * 3i64);  // could be 9i64
```

**Fix:** During lowering or as an IR optimization pass, evaluate constant expressions:
- `BinOp` where both sides are `LitInt` or `LitFloat` → fold to single literal
- `UnOp` on literal → fold
- `BinOp` identity: `x + 0`, `x * 1`, `x * 0` → simplify
- `BoolOp`: `true && x` → `x`, `false || x` → `x`

**Implementation:** Add `fn try_fold_constant(expr: &IrExpr) -> Option<IrExpr>` in `ir.rs` or `lower.rs`.

**Impact:** Smaller generated code, enables further optimizations.

## 4. String Literal Context

**Problem:** Every string literal emits `.to_string()`, even when used in a context that accepts `&str`:
```rust
format!("{}", "hello".to_string())  // unnecessary allocation
```

**Fix:** Track whether a string literal is used in:
- `format!()` / `StringInterp` context → emit bare `"hello"` (already `&str`)
- Value context (let binding, return, collection element) → keep `.to_string()`
- `println!()` argument → emit bare `"hello"`

**Implementation:** Pass context flag to `gen_ir_expr` for `LitStr`.

**Impact:** Fewer allocations in string-heavy code.

## 5. Dead Code Elimination (Light)

**Problem:** Generated code can contain unreachable match arms and unused variant constructors.

**Fix:** Light DCE pass on IR:
- Remove match arms after an irrefutable pattern
- Skip emission of variant constructor helpers if never called in user code
- Skip `impl` blocks for types never instantiated

**Note:** Heavy DCE is deferred. This covers only obvious cases.

## Priority

All items are independent and low-medium difficulty:

| Item | Lines of change | Impact |
|------|----------------|--------|
| 1. let mut | ~20 | Low (cleanliness) |
| 2. #[inline] | ~10 | Low-Medium |
| 3. Constant folding | ~60 | Medium |
| 4. String context | ~30 | Medium |
| 5. Light DCE | ~40 | Low-Medium |
