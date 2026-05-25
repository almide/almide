# Almide Perceus: Type-Guided Automatic Memory Management

> Zero annotations. Zero GC. Zero leaks.
> The compiler proves memory safety from types alone.

## Overview

Almide uses **Perceus-style reference counting** — the compiler
automatically inserts `rc_inc` and `rc_dec` operations based on
type analysis. Users write functional code without thinking about
memory. The compiler guarantees every allocation is freed exactly once.

## Three Rules

### Rule 1: Increment on Share
When a heap value gains a new reference (variable copy, closure capture),
the compiler inserts `rc_inc`. This tracks shared ownership.

```almide
let a = [1, 2, 3]
let b = a            // rc_inc(a) — shared reference
```

### Rule 2: Decrement on Release
When a reference ends (last use, scope exit, variable overwrite),
the compiler inserts `rc_dec`. When RC reaches 0, the value is freed.

```almide
let xs = list.map(data, f)
let ys = list.filter(xs, g)
// rc_dec(xs) inserted here — xs is never used again
// xs goes to free list, memory reused by next alloc
```

### Rule 3: Recursive Drop
When a compound value (List[String], etc.) reaches RC 0,
its children are recursively rc_dec'd before the parent is freed.

```almide
let names = ["alice", "bob"]
// When names is freed: rc_dec("alice"), rc_dec("bob"), then free list
```

## Optimizations

### In-Place Reuse
When a value is consumed exactly once (RC == 1), the compiler
rewrites functional operations into in-place mutations:

```almide
data |> list.map(f) |> list.filter(g) |> list.fold(0, h)
// Zero allocations — map and filter operate in-place on the same memory
```

### Linearity Detection
The compiler tracks `use_count` for every variable. Single-use
values skip RC operations entirely — no `rc_inc`, no `rc_dec`,
just direct consumption.

### Temporary Expressions
Call results (`f(x)`) are inherently single-use. The compiler
treats them as owned temporaries eligible for in-place reuse
without any RC overhead.

## Comparison

| Feature | Rust | Go | Almide |
|---------|------|----|--------|
| User annotation | Ownership + lifetimes | None | **None** |
| Runtime mechanism | None (compile-time) | GC | **RC (minimized)** |
| Guarantee | Borrow checker | None | **Type-guided insertion + verification** |
| In-place mutation | Manual (`&mut`) | N/A | **Automatic (Perceus)** |
| Pause-free | Yes | No (GC pauses) | **Yes** |

## Guarantee

After Perceus insertion, the compiler can verify:
1. Every `alloc` has a reachable `rc_dec` path
2. Every heap Var copy has `rc_inc`
3. Every mutable Assign of a heap var has old-value `rc_dec`
4. Every compound `rc_dec` recurses into children

If verification passes, memory safety is proven by construction.
