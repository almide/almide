# duration

Wall-clock time constructors. Auto-available (checker surface — no import,
not a stdlib module). Normative rules:
[ADR-0001](../adr/0001-deterministic-time-units.md).

A `Duration` value is a quantity of **wall-clock time** — what a stopwatch
measures on the machine the program happens to run on. It is the clock of the
oracle tier — `fan.timeout(duration.ms(n)) { body }` checks it cooperatively
at charge sites (effect-surface deadlines reserve the same type, future);
the deterministic tier reads the other clock — [compute](compute.md). The two
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
