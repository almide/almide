# compute

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](compute.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/compute.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


Deterministic compute-time constructors. Auto-available (checker surface — no
import, not a stdlib module). Normative rules:
[ADR-0001](../../../../adr/0001-deterministic-time-units.md).

A `Compute` value is a quantity of **deterministic compute time**: it meters
the program's own work on a frozen abstract machine (charge units at function
entries and loop heads), so a budget verdict is a function of the program and
its inputs alone — the same on every target and every host. This is the clock
`fan.bounded` reads, and the clock `fan.min(compute…)` selects by (the clock
argument of `fan.min` is required; its type says what "least" is measured on).
Wall-clock time is the OTHER clock — [duration](../../../../stdlib/duration.md).

The unit set is closed: `ns / us / ms / s / min / h`. There are no time
literals (`100ms` does not parse) and a bare `Int` is never a time.

### `compute.ns(n: Int) -> Compute` … `compute.h(n: Int) -> Compute`

```almd run
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  42
}

effect fn main() -> Unit = {
  let a = compute.ns(1500)   // nanoseconds
  let b = compute.us(50)     // microseconds
  let c = compute.ms(100)    // milliseconds
  let d = compute.s(2)       // seconds
  let e = compute.min(5)     // minutes
  let f = compute.h(1)       // hours
  // every unit is the same clock: the sum is a budget fan.bounded reads
  println(int.to_string(fan.bounded(a + b + c + d + e + f) { spin(1000) } ?? -1))
}
```
```output
42
```

A negative argument is a deterministic runtime abort
(`Error: negative time: compute.us(-5)`, exit 1 — identical on both targets);
an overflowing construction saturates to the maximum representable time.

### Algebra (ADR-0001 S3)

```almd run
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  42
}

effect fn main() -> Unit = {
  let sum = compute.ms(2) + compute.ms(3)      // Compute — saturating add
  let diff = compute.ms(5) - compute.ms(2)     // Compute — saturates at 0 (never negative)
  let scaled = compute.ms(2) * 3               // Compute — scale by Int (either order);
                                               // a negative factor aborts
  let less = compute.ms(2) < compute.ms(3)     // Bool — same-clock comparison
  let floor = compute.ms(1) - compute.ms(2)    // saturated to 0: admits no loop
  println(int.to_string(fan.bounded(sum) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(diff) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(scaled) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(3 * compute.ms(2)) { spin(1000) } ?? -1))
  println(int.to_string(fan.bounded(floor) { spin(1000) } ?? -1))
  println(if less then "less" else "not less")
}
```
```output
42
42
42
42
-1
less
```

`T * T` (time × time), any op mixing `Compute` with `Duration` or a bare
`Int`, and `/` are type errors — see the ADR for why each cell is closed.

### Consumers

```almd run
fn work(n: Int) -> Int = n * 2

fn fast() -> Int = 1

fn slow() -> Int = {
  var i = 0
  while i < 100000 {
    i = i + 1
  }
  2
}

effect fn main() -> Unit = {
  let input = 21
  let fallback = -1
  let r = fan.bounded(compute.ms(100)) { work(input) } ?? fallback
  let w = fan.min(compute.us(50)) { fast(), slow() } ?? fallback
  println(int.to_string(r))
  println(int.to_string(w))
}
```
```output
42
1
```

`almide run --time-report` prints a program's deterministic time next to the
measured wall clock (`time: 0.15ms deterministic (≈38.7ms wall here)`) — the
two are different clocks and never claim to be the same quantity.



## Signature index (7 functions)

```
// n nanoseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.ns(n: Int) -> Compute

// n microseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.us(n: Int) -> Compute

// n milliseconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.ms(n: Int) -> Compute

// n seconds of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.s(n: Int) -> Compute

// n minutes of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.min(n: Int) -> Compute

// n hours of deterministic compute time; negative n aborts.
// @since 0.51.0 or earlier
compute.h(n: Int) -> Compute

// Negative if a < b, 0 if equal, positive if a > b.
// @since unreleased
compute.compare(a: Compute, b: Compute) -> Int
```


