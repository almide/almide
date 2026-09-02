# compute

Deterministic compute-time constructors. Auto-available (checker surface — no
import, not a stdlib module). Normative rules:
[ADR-0001](../adr/0001-deterministic-time-units.md).

A `Compute` value is a quantity of **deterministic compute time**: it meters
the program's own work on a frozen abstract machine (charge units at function
entries and loop heads), so a budget verdict is a function of the program and
its inputs alone — the same on every target and every host. This is the clock
`fan.bounded` and `fan.race` read. Wall-clock time is the OTHER clock —
[duration](duration.md).

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
  let w = fan.race(compute.us(50)) { fast(), slow() } ?? fallback
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
