# ADR-0030: Compile cost is a contract — every phase has a counted budget, the source cannot raise it, and exceeding it is a defined diagnostic

- **Status**: Accepted (2026-10-28, the owner's ruling Q5: accepted once the GCE round on the implementing branch was green). The direction is the owner's ruling of 2026-10-24 (○): "treat the compiler as a program that processes untrusted input, and promise its resource use as a contract".
- **Date**: 2026-10-24
- **Scope**: the budgets of `almide check` / `build` / `fmt` / `lsp` / `ide` over their input (source files, `almide.toml`, `almide.lock`), the diagnostics for exceeding them, and the gate that keeps them true. Not covered: the run time of the compiled program, which `docs/roadmap/active/security-by-construction.md` row 9 owns.
- **Implementation**: #4136.
- **Related**:
  - #4114: inferred types grow 2^n; 22 lines take `almide check` to 3.7 GB.
  - The expression-nesting cap (500).
  - `docs/roadmap/active/security-by-construction.md` rows 9 and 25.
  - #4128: the LSP acting on an untrusted project.
  - #4129: environment switches.
  - ADR-0017: the flight profile is a normative subset.

## Context

The runtime side of Almide promises that untrusted input ends in a value, a defined error, or a defined abort within documented bounds. The compiler makes no such promise, yet it runs on untrusted input all the time:

- CI on a pull request;
- the LSP the moment a file is opened;
- graders and model-evaluation harnesses (Dojo);
- the playground.

What the 2026-10-24 red-team round measured:

| shape | bounded? | measured |
|---|---|---|
| expression / list nesting `((((…))))` | yes, "nesting too deep (max 500)" | 0.06 s, 48 MB at depth 50,000 |
| `let t(i) = (t(i-1), t(i-1))` | **no** | depth 22: 3.7 GB; depth 39: stopped at 5.4 GB (#4114) |
| the same doubling through a generic `fn dup[T](x: T) -> (T, T)` | **no** | depth 18: 295 MB, ×2 per line |
| a wide tuple `match`, width 24, for exhaustiveness | yes, flat | 0.02 s, 23 MB |

Other compilers bound these shapes after the fact, case by case. Background only; this ADR does not depend on their numbers:

- Rust's `recursion_limit` and `type_length_limit`, which the **source** can raise with `#![...]`.
- TypeScript's "type instantiation is excessively deep".
- Swift's time-based "expression too complex".
- GHC's `-freduction-depth`.

Three properties none of them gives together:

- every phase is bounded;
- the source cannot change the bound;
- the verdict does not depend on the machine.

## Decision

**D1. One budget table.** Every phase that grows with its input has a named budget in one registry, with the diagnostic it ends in. The registry is a sibling of the `ALMIDE_*` switch registry (#2205), and is rendered into `docs/specs/cli.md`. Initial rows:

- syntactic nesting;
- inferred type size (nodes);
- generic instantiations per program;
- exhaustiveness work;
- manifest and lock size;
- dependency count and depth.

**D2. Counted, not timed.** Budgets count nodes, instantiations, bytes or steps, never seconds or free memory. The same input gives the same verdict:

- on every machine;
- on the native and the wasm-hosted compiler;
- in the CLI and the LSP.

(See the lesson "a verdict that tracks free memory".)

**D3. The source cannot raise a budget.** There is no attribute, manifest key or lock entry that changes one. The person running the compiler can, with `--budget NAME=N`. `--profile critical` refuses that flag. Environment variables cannot change budgets either; that follows #4129.

**D4. Exceeding a budget is a defined diagnostic.** It is an E-code with a source location, the measured figure, the budget, and a hint written for the model that has to fix it (for example, "type too large (N nodes, max M): name the intermediate type or use a record"). It is never a crash, an out-of-memory kill, or a hang.

**D5. Structure first, budget last.** Where a representation can make the blowup impossible, use it. Types are shared (hash-consed or arena-interned), so `(t, t)` costs O(1); type printing in diagnostics elides repeated structure. A budget is the backstop for what structure cannot prevent.

**D6. Budgets are set from measurement.** Each figure is about 10× the maximum observed over `spec/`, `stdlib/`, the exercises corpus and the Dojo task bank. The measurement script and its output are committed. Model-written code never meets a budget; only adversarial input does.

**D7. The gate.** CI runs a generator per known blowup shape (the #4114 doubling, its generic form, nesting, wide match, long interpolation chains, many imports), and asserts two things:

- each ends within its budget's diagnostic;
- `almide check`'s counted work grows at most linearly with source size over each generator's range.

Negative control: disabling type sharing and the type-size budget makes the #4114 generator exceed the budget, and the gate fails.

## Consequences

- With row 9's runtime bounds, the claim "whatever input Almide is given, it ends in a defined result within documented bounds" extends from programs to the toolchain.
- Budgets are a compatibility surface. Raising one is additive. Lowering one can refuse code that compiled before, so it needs a `proofs/dialect-epochs.toml` entry.
- The LSP and graders can run on untrusted source without an OS-level sandbox for resource exhaustion. Isolation from side effects is separate (#4128).

## Rulings (2026-10-27)

1. The figures (D6) are measured: `scripts/measure-compile-budgets.sh` writes `proofs/compile-budgets.toml`, and each new default is about ten times the corpus maximum. The two budgets that predate this ADR keep their earlier figures: `nesting` stays at 500 (15× the corpus maximum of 33), and `manifest_bytes` stays at 1 MiB (#4194).
2. The budget table is one spec table (`almide budgets --md`, embedded in `docs/specs/cli.md`) with a single contract, C-440: "every budget in the table holds".
3. `almide fmt` shares the table. It only parses, so the budget it meets is `nesting`.
4. "Emitted code size per function" is dropped from D1. Emitted code is linear in the IR, which `nesting` and `type_size` already bound.
5. D5 lands with the budget, not instead of it. Composite types share their argument lists (`almide_types::TyVec`): a clone is a pointer copy, and each list caches its node count and whether it holds an inference variable. Resolution memoizes per variable and per shared list. With the type-size budget lifted, the #4114 chain checks in time and memory linear in its length (`tests/compile_budget_test.rs`). Its negative control, `ALMIDE_CONTROL_NO_TYPE_SHARING`, makes every clone copy the tree again.
