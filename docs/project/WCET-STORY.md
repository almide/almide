# WCET analyzability of Critical-shape native output (#569)

Hard-real-time acceptance needs worst-case execution time analysis. This
is the documented WCET story for Almide's `--target rust` output in the
CRITICAL shape, demonstrated end to end on `examples/pid-kernel.almd`
(gated by `tests/wcet_kernel_test.rs`).

## The three-layer story

1. **Static shape (what the analyzer sees).** The Critical-shape kernel
   compiles to straight-line scalar Rust: statically bounded loops
   (compile-time literal bounds), NO allocation in the loop body, no
   indirect calls, no recursion, f64/i64 locals only. The emitted
   `run_loop` of the PID kernel is nine statements of scalar arithmetic
   under one `for … in 1..=N` — the exact input class aiT-class static
   analyzers are built for. The gate asserts these properties on the
   SHIPPED text every build (no `Vec`/`Box`/`String`/`clone`/`format!`
   tokens inside the kernel's loop; the literal bound present).
2. **The machine-independent bound (what the compiler enforces).** The
   deterministic compute meter (C-320) charges every loop head and
   dynamic operation in DEFINED units, identically on native and wasm —
   `fan.bounded(compute.ms(…))` budgets are enforced against modeled charges.
   A charge bound is not a hardware WCET proof: cooperative checks do not
   bound every individual operation or blocking host call. Translating charges
   into time would require justified per-operation ceilings and platform /
   toolchain analysis; no such calibration or WCET result is claimed here.
3. **Hardware WCET (what remains external).** Cache/pipeline-aware
   static WCET on a concrete MCU stays the seat of a qualified analyzer
   over generated source and the deployed object code. The current #573
   lane uses stock rustc at a recorded compatibility pin, not qualified
   Ferrocene object code. What
   Almide contributes to that analysis is layer 1's input class and the
   `--trace-map` correspondence (#572) for annotating analysis results
   back onto source.

## What follows from the language

- Bounded iteration: `--profile critical` already rejects `while` and
  recursion; `tests/critical_profile_test.rs` covers those refusals. The PID
  example uses literal-bounded `for` ranges.
- Bounded RC: the kernel shape keeps values scalar — no RC traffic at
  all in the loop. This scalar example does not establish a bound on drop
  work for arbitrary heap values or recursive data types.
- No hidden allocation: `+` on scalars is arithmetic; string/list
  concatenation (which allocates) simply does not appear in the shape,
  and the gate would catch it in the emitted text if it crept in.

## Measurement caveat (deliberate omission)

Process-level wall timings are dominated by spawn noise and are NOT a
WCET instrument; this document intentionally carries none. The
demonstrable artifacts are the static properties (gated), the meter
mechanism (contracted), and the emitted listing (reviewable via
`--trace-map`).
