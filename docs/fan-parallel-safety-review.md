# Explicit fan routing safety review

The #2055 integration retains StreamFusion and DecodeErrFrame and removes
the obsolete AutoParallel pass. Codegen unit and integration tests pass
after resolving the pipeline conflicts.

## Plain functions can contain effects

The following program checks successfully but the original routing pass
emits `almide_rt_fan_map_par`, allowing its printed element order to change:

```almide
fn observe(n: Int) -> Int = {
  println(int.to_string(n))
  n
}
effect fn main() -> Unit = {
  let values = fan.map([1, 2, 3], (n) => ok(observe(n)))!
  println(int.to_string(list.len(values)))
}
```

The `is_effect` declaration flag alone cannot establish purity. Routing
now recomputes the same conservative, transitive purity analysis used by
StreamFusion on its own input. It does not depend on whether fusion ran.
The existing mutable-capture and Send-safe type checks still apply.
Unknown calls and opaque Rust cannot establish purity. A regression in
`tests/nanopass_test.rs` covers a plain helper reaching the random runtime.

This follows the evaluation-order reasoning in
`docs/stream-fusion-safety-review.md`, including the pinned Gleam source
(`19bf207ebb7d953ea1391f041da48c214ee1440a`) that binds non-variable
subjects once to preserve effects. References are cloned under
`../almide-references`; purity analysis is a correctness admission rule,
not an inference from callback parameter types.

## Invocation-local mutation and integration

StreamFusion preserves operations directly under explicit fan blocks, while
still fusing callback internals and implicit list pipelines. This keeps the
parallel list call available to the later fan routing pass.

Fan's proof admits local imperative helper bodies through an analysis-only IR
view. Only closed functions with scalar parameters receive this treatment;
heap parameters retain the strict proof because they can alias caller storage.
Writes to invocation-local bindings are replaced by their operand expressions,
and loop bodies and conditions are retained. Calls in those expressions must
still pass the transitive effect check. Parameter writes and enclosing-state
access cannot gain admission. The transformed view is never emitted and never
establishes totality: StreamFusion retains its original proof and termination
requirements. A regression puts a random call in an erased local assignment's
RHS inside a loop, and verifies that the function remains rejected.

This ownership boundary agrees with the pinned Rust reference
`0d31508599a7814a7044e9a7a871e3dc5f037753`,
`library/std/src/thread/scoped.rs`: scoped spawning still requires `F: Send`;
the scope's lifetime does not duplicate captured ownership. Fan keeps its
existing Send and mutable-capture checks in addition to purity.

## Remeasured parallel execution

The repaired compiler emits `almide_rt_list_par_map` in fannkuchredux's user
function. At n=10, parallel, `ALMIDE_FAN_SEQUENTIAL=1`, and the handwritten
Rust reference all print `73196` and `Pfannkuchen(10) = 38`.
Seven interleaved measurements after one warmup, using `almide build --release`
and the benchmark harness's standard Rust flags, give medians of 54.16 ms
parallel, 179.27 ms sequential, and 172.47 ms Rust on this development machine.
The native/Rust ratio is 0.314 and the sequential/parallel ratio is 3.310.
These are local validation results, not a replacement for the CI runner's
performance gate or a claim about the earlier benchmark revision.
Raw measurements: `research/benchmark/perf/results/2026-09-09-fan-repair.json`.

The effectful plain-helper reproducer stays sequential and prints 1, 2, 3, 3
on native and structural WASM. Codopsy 2.2.0 reports A for the routing and
analysis files with unchanged thresholds.
