# duration

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](duration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](duration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](duration.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/duration.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


Wall-clock time constructors. Auto-available (checker surface — no import,
not a stdlib module). Normative rules:
[ADR-0001](../../../../adr/0001-deterministic-time-units.md).

A `Duration` value is a quantity of **wall-clock time** — what a stopwatch
measures on the machine the program happens to run on. It is the clock of the
oracle tier — `fan.timeout(duration.ms(n)) { body }` checks it cooperatively
at charge sites (effect-surface deadlines reserve the same type, future);
the deterministic tier reads the other clock — [compute](../../../../stdlib/compute.md). The two
types never mix and there is no conversion between them: that firewall is the
reason both can share the familiar `ms` vocabulary safely.

The unit set is closed: `ns / us / ms / s / min / h`. There are no time
literals (`5s` does not parse) and a bare `Int` is never a time.

### `duration.ns(n: Int) -> Duration` … `duration.h(n: Int) -> Duration`

```almd
duration.ms(5000)  // the conventional timeout spelling
duration.s(30)
duration.min(10)
```

A negative argument is a deterministic runtime abort; an overflowing
construction saturates to the maximum representable time.

### Algebra (ADR-0001 S3)

Same face as `Compute`, within the clock:

```almd
duration.s(1) + duration.ms(500)   // Duration — saturating add
duration.s(2) - duration.s(3)      // Duration — saturates at 0
duration.ms(100) * 3               // Duration — scale by Int
duration.ms(1) < duration.s(1)     // Bool
```

`Duration ⊕ Compute` in any operator is a type error — a wall-clock deadline
can never leak into a deterministic budget (`fan.bounded(duration.ms(5))` is
rejected with a pointer to `compute.ms(...)` / `fan.timeout`).



## Signature index (7 functions)

```
// n nanoseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.ns(n: Int) -> Duration

// n microseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.us(n: Int) -> Duration

// n milliseconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.ms(n: Int) -> Duration

// n seconds of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.s(n: Int) -> Duration

// n minutes of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.min(n: Int) -> Duration

// n hours of wall-clock time; negative n aborts.
// @since 0.51.0 or earlier
duration.h(n: Int) -> Duration

// Negative if a < b, 0 if equal, positive if a > b.
// @since unreleased
duration.compare(a: Duration, b: Duration) -> Int
```


