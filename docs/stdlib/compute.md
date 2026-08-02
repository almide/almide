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

```almd
compute.ns(1500)   // nanoseconds
compute.us(50)     // microseconds
compute.ms(100)    // milliseconds
compute.s(2)       // seconds
compute.min(5)     // minutes
compute.h(1)       // hours
```

A negative argument is a deterministic runtime abort
(`Error: negative time: compute.us(-5)`, exit 1 — identical on both targets);
an overflowing construction saturates to the maximum representable time.

### Algebra (ADR-0001 S3)

```almd
compute.ms(2) + compute.ms(3)   // Compute — saturating add
compute.ms(5) - compute.ms(2)   // Compute — saturates at 0 (never negative)
compute.ms(2) * 3               // Compute — scale by Int (either order);
                                // a negative factor aborts
compute.ms(2) < compute.ms(3)   // Bool — same-clock comparison
```

`T * T` (time × time), any op mixing `Compute` with `Duration` or a bare
`Int`, and `/` are type errors — see the ADR for why each cell is closed.

### Consumers

```almd
let r = fan.bounded(compute.ms(100)) { work(input) } ?? fallback
let w = fan.race(compute.us(50)) { fast(); slow() } ?? fallback
```

`almide run --time-report` prints a program's deterministic time next to the
measured wall clock (`time: 0.15ms deterministic (≈38.7ms wall here)`) — the
two are different clocks and never claim to be the same quantity.
