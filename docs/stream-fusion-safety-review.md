# Stream fusion safety review (2026-09-09)

PR #2053 repairs the duplicated map-callback effects in #2054. Updating it to
current develop also preserves the DecodeErrFrame pass beside StreamFusion;
the only textual merge conflict was their pipeline documentation.

## Reference and decision

The pinned Gleam checkout at `../almide-references/gleam`
(`19bf207ebb7d953ea1391f041da48c214ee1440a`) documents in
`compiler-core/src/javascript/decision.rs::assign_subject` why a non-variable
subject must be evaluated once and bound before reuse. This supports preserving
observable evaluation, not assuming an arbitrary expression is safe to repeat,
reorder or omit. Almide's existing `crates/almide-mir/src/purity.rs` additionally
separates host-effectful modules, impure plain functions (including allocator
state), and higher-order calls. Those distinctions apply to fusion too.

Review found that the original fusion termination analysis used the greatest
fixed point for both purity and totality. A recursive cycle may have no external
side effect, but that is not evidence it terminates. The accepted program

```almide
fn diverge(n: Int) -> Int = if n == 0 then 0 else diverge(n) + 1
fn main() -> Unit = {
  let xs = [1] |> list.map((x) => diverge(x)) |> list.take(0)
  println(int.to_string(list.len(xs)))
}
```

previously emitted a single `.map(...).take(0).collect()` chain, so the recursive
call vanished. It now emits `list_take(map(...).collect(), 0)`: the original eager
map must finish before take. No test executes the divergent program; the emitted
shape was inspected and unit tests reject direct recursion, mutual recursion,
and callers of those cycles as total, while admitting acyclic total call chains.

Totality now grows from proven leaf functions (least fixed point). Purity retains
the greatest fixed point. Runtime calls additionally require a known stdlib module
outside the effectful set; function arguments require proofs for their callees;
mutable borrows are rejected. Arbitrary InlineRust cannot acquire purity merely
because it lacks suspicious words: only the compiler's exact numbered array
placeholder form is admitted. Tests cover opaque Rust, effectful/unknown runtime
modules, callback references, and mutable borrows.

The expression proof is partitioned into disjoint literal, operator, control,
aggregate, wrapper and call cases. `Some(false)` stops dispatch; unknown cases
return no proof. Codopsy 2.2.0 measures the fusion pass at **100/A** without rule
changes; the codegen crate's pre-existing aggregate remains **89/B**.

## Validation of the updated branch

- Both `almide test` and `almide test --target rust`: 438/438 files.
- Codegen crate: 56 unit tests and 10 integration tests passed.
- Nanopass / codegen snapshot / end-to-end render gates: 34 / 13 / 2 passed.
- Clippy over all codegen targets: passed; no warning names the changed fusion
  file. Existing unrelated warnings remain.
- Pass roster, file discipline and `git diff --check`: passed.

## Module names must not establish root-function purity

A further end-to-end audit found that a module's bare function name was
added to the same proof set as root functions. With a pure `helper.observe`
and a root `observe` that prints, `[1, 2] |> list.map((n) => observe(n)) |>
list.take(0)` incorrectly suppressed both prints. The original binary
printed `0, 0`; disabling fusion printed `0, 1, 2, 0`. Module functions
now contribute only qualified aliases, preventing cross-namespace purity
and totality proofs. `tests/stream_fusion_module_purity_test.rs` pins the
observable output through a real imported module.
