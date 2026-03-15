# Tail Call Optimization [ACTIVE]

## Summary

Self-recursive tail calls → labeled loop transformation in Rust codegen.

Eliminates stack overflow for recursive functions by converting tail-position self-calls into `'_start: loop { ... continue '_start; }`.

## Motivation

Inspired by [lean4-rust-backend](https://github.com/O6lvl4/lean4-rust-backend)'s TCO implementation. Almide users write recursive functions naturally; without TCO, deep recursion causes stack overflow in generated Rust.

## Design

### Detection

In `emit_rust`, detect `IrExprKind::Call` where:
1. `CallTarget::Named { name }` matches the current function name
2. The call is in **tail position** (last expression in function body, or last in if/match branches)

### Transformation

```rust
// Before (recursive):
fn factorial(n: i64, acc: i64) -> i64 {
    if n <= 1 { acc } else { factorial(n - 1, acc * n) }
}

// After (TCO):
fn factorial(mut n: i64, mut acc: i64) -> i64 {
    '_start: loop {
        if n <= 1 { return acc; } else {
            let _tmp_0 = n - 1;
            let _tmp_1 = acc * n;
            n = _tmp_0;
            acc = _tmp_1;
            continue '_start;
        }
    }
}
```

Key details:
- Temporary variables prevent aliasing during parameter reassignment
- Non-tail returns become explicit `return`
- Only self-recursion (not mutual recursion) in Phase 1

### Implementation Scope

1. **Tail position analysis**: Walk IR to identify tail-position calls
2. **Codegen transform**: Emit loop wrapper + continue pattern for detected functions
3. **Effect fn support**: TCO with `Result` return type (tail call in Ok path)

## Phase

- Phase 1: Self-recursive tail calls (single function)
- Phase 2: Mutual tail calls (multiple functions, trampoline) — future

## Testing

- `spec/lang/tco_test.almd`: factorial, fibonacci, list traversal
- Verify no stack overflow for deep recursion (100,000+ depth)
- Verify correctness matches non-TCO version
