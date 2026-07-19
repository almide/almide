<!-- description: Auto-hoist reads from mut args to avoid Rust borrow conflicts -->
<!-- done: 2026-05-22 -->
# Borrow Read Hoisting

> **Target: v0.22**
> **Status: Done**

## Problem

```almide
var buf = bytes.new(16)
bytes.set_at(buf, i, int.bxor(bytes.read_u8(buf, j), val))
```

This is valid Almide — reading and writing the same buffer in one expression. But the generated Rust has a borrow conflict:

```rust
almide_rt_bytes_set_at(&mut buf, i, int_bxor(almide_rt_bytes_read_u8(&buf, j), val))
//                     ^^^^^^^^                                       ^^^^
//                     &mut borrow                                    & borrow
//                     conflict!
```

Users must manually separate read from write:

```almide
let v = int.bxor(bytes.read_u8(buf, j), val)  // read first
bytes.set_at(buf, i, v)                         // then write
```

This is Rust leaking into Almide. Users shouldn't need to know about Rust's borrow checker.

## Solution

The borrow insertion pass should auto-hoist reads when a `mut` argument conflicts.

### Before (current codegen)
```rust
almide_rt_bytes_set_at(&mut buf, i, almide_rt_bytes_read_u8(&buf, j))
```

### After (with read hoisting)
```rust
let __tmp = almide_rt_bytes_read_u8(&buf, j);
almide_rt_bytes_set_at(&mut buf, i, __tmp);
```

### Detection

In the borrow insertion pass, when emitting a call with a `mut` parameter:
1. Scan other arguments for references to the same variable
2. If found, hoist those sub-expressions into `let` bindings before the call

### Scope

Affects any `mut` stdlib function where other args reference the same var:
- `bytes.set_at(buf, i, bytes.read_u8(buf, j))`
- `bytes.push(buf, bytes.read_u8(buf, i))`
- `list.push(xs, list.get(xs, i))`

### Not needed for

- Different variables: `bytes.set_at(a, i, bytes.read_u8(b, j))` — no conflict
- Immutable functions: `bytes.set(b, i, v)` — no `&mut`, no conflict
