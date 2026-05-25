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

## Formal Guarantee

**Theorem** (Type-Guided Memory Safety):
If the Almide type checker accepts program P, then the
Perceus-annotated output P' satisfies:
1. Every heap allocation is freed exactly once
2. No use-after-free (RC prevents premature deallocation)
3. No double-free (RC counts references precisely)
4. Compound values are recursively freed (no child leaks)

**Proof sketch**:
Each Perceus rule is a function of the type alone:

```
rc_inc(x)        ⟺  Bind(y, Var(x)) ∧ is_heap(typeof(x))
rc_dec(x)        ⟺  last_use(x) ∧ is_heap(typeof(x))
rc_dec_old(x)    ⟺  Assign(x, _) ∧ is_heap(typeof(x))
drop_children(x) ⟺  RC(x) → 0 ∧ has_heap_children(typeof(x))
in_place(x)      ⟺  is_heap(typeof(x)) ∧ use_count(x) = 1
```

Since these rules are purely type-directed and the type checker
is sound (every expression has a unique resolved type), the
insertion is correct by construction. The RC invariant
`RC(a) = |{v : v points to a ∧ v is live}|` is maintained at
every program point.

**Comparison with Rust**:
```
Rust:   user writes &/&mut/lifetime → borrow checker verifies → safe
Almide: type checker resolves types → Perceus generates from types → safe
```
Rust requires user annotation. Almide derives safety from types alone.

## Recursive Drop Coverage

| Type | Drop behavior |
|------|--------------|
| `String` | rc_dec (no children) |
| `List[T]` | iterate elements → rc_dec each if `is_heap(T)` |
| `Record { fields }` | rc_dec each heap-typed field at offset |
| `Option[T]` | if Some → rc_dec inner if `is_heap(T)` |
| `Result[T, E]` | check tag → rc_dec matching heap variant |
| `Map[K, V]` | walk Swiss Table → rc_dec heap keys/values |
| `Fn` (closure) | rc_dec env allocation |

## References

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Tofte & Talpin, "Region-Based Memory Management" (POPL 1997)
