Add a `sub <n>` command that subtracts n from the running total.

Preserve:
- `add`, `show` and the error lines behave exactly as before.
- The total is a plain integer: it may go negative, and it is never clamped.
- A non-numeric amount is reported as `error: bad amount <arg>` and skipped, as for `add`.
