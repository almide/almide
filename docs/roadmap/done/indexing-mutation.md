<!-- description: buf[i] = val indexing syntax for bytes and list mutation -->
<!-- done: 2026-05-22 -->
# Indexing Mutation Syntax

> **Target: v0.22**
> **Status: Done**

## Problem

`bytes.set(b, i, val) -> Bytes` clones the entire buffer on every call. For AES-128-CFB8 (1 block encryption per byte), 4096 bytes = 4096 clones = unusable performance.

The current `mut` operations (`bytes.push`, `bytes.clear`) are in-place, but `bytes.set` is pure/immutable. This inconsistency forces users to choose between clean code and performance.

## Interim Fix (v0.21)

- `bytes.set_at(mut b, i, val) -> Unit` — in-place mutation
- `bytes.copy_within(mut b, src_start, src_end, dst) -> Unit` — block copy

These are sufficient for the AES use case but the API is ad-hoc.

## Ideal: Indexing Syntax

```almide
var buf = bytes.new(16)
buf[0] = 0xFF              // in-place mutation (COW via RcCow)
let x = buf[0]             // read access

var xs = [1, 2, 3]
xs[0] = 10                 // list index mutation
```

Every major language uses `buf[i] = val` for byte/array mutation. Almide should too.

### Semantics

- `var` binding: `buf[i] = val` → in-place mutation via `RcCow::make_mut` (COW, like Swift)
- `let` binding: `buf[i] = val` → compile error (E009: cannot mutate immutable binding)
- Read: `buf[i]` → `Int` (direct access, panic on OOB — matches Rust/Go/Swift/Zig/Python/Kotlin/Elixir)
- Safe read: `bytes.get(buf, i)` → `Option[Int]` (bounds-checked, no panic)

### Desugaring

```
buf[i] = val
  ↓ checker
IndexAssign { target: buf, index: i, value: val }
  ↓ lowering
IrStmtKind::IndexAssign { var: buf_id, index: i_expr, value: val_expr }
  ↓ walker (Rust)
buf[i as usize] = val as u8;    // for bytes
buf[i as usize] = val;           // for list
```

### Implementation

1. **Parser**: Parse `expr[expr] = expr` as `Stmt::IndexAssign`
2. **Checker**: Verify target is `var`, element type matches
3. **Lowering**: `IrStmtKind::IndexAssign` already exists (used for list)
4. **Walker**: Emit direct index write for bytes/list

### What becomes deprecated

- `bytes.set(b, i, val) -> Bytes` — use `buf[i] = val`
- `bytes.set_at(mut b, i, val)` — use `buf[i] = val`
- `list.set(xs, i, val) -> List` — use `xs[i] = val`

### Performance impact

AES-128-CFB8 inner loop:
```
Current:  bytes.set(out, i, val)      → clone + write = O(n) per call
Interim:  bytes.set_at(mut out, i, val) → direct write = O(1)
Ideal:    out[i] = val                 → direct write = O(1)
```

For 4096-byte CFB8: O(n²) → O(n). Minecraft keep alive timeout eliminated.
