# xtarget-fuzz — Almide generative differential fuzzer

Stage 3 of the completeness roadmap: the machine that hunts the
composition space continuously for **native ↔ WASM observable
divergences** and compiler failures.

It generates well-typed Almide programs (synthesis + corpus mutation),
runs each through an oracle ladder that compiles and executes it on both
targets, and byte-compares the results. Every program is reproducible
from a `(seed, index)` pair. Findings are delta-debugged to a minimal
repro and written to `findings/`.

This is a standalone crate (its own `[workspace]`, like
`tools/wasmgen-harness`). It path-deps the parent `almide` crate for the
AST / parser / formatter, and **shells out** to the freshly built
`almide` binary for the oracle ladder — so a compiler ICE crashes a
child process we can observe, not the fuzzer.

## Build & run

```bash
# 1. Build the compiler (the fuzzer drives this binary).
cargo build --release --bin almide          # from the repo root

# 2. Build the fuzzer.
cd tools/xtarget-fuzz && cargo build --release

# 3. Run a campaign (time-budgeted or fixed count).
./target/release/xtarget-fuzz run --minutes 60          # 60-minute hunt
./target/release/xtarget-fuzz run --count 200 --jobs 8  # 200 programs, 8 workers

# Inspect / reproduce one program deterministically.
./target/release/xtarget-fuzz gen    --seed 100 --index 42   # print source
./target/release/xtarget-fuzz replay --seed 100 --index 42   # re-run the ladder

# Catalogue / corpus sizes.
./target/release/xtarget-fuzz stats
```

`wasmtime` must be on `PATH` for the WASM execution rung. The repo root
and the `almide` binary are autodetected; override with `--repo` /
`--almide`.

## The generator

- **Type-directed synthesis (~70%)** — `src/generator/{term,program}.rs`.
  Builds programs well-typed *by construction* from a typed term grammar:
  pick a goal type, generate an expression of that type from literals,
  in-scope variables, stdlib calls whose return type unifies with the
  goal, inline lambdas for HOF arguments, and `if` arms. Fuel-bounded for
  termination. Ambiguous literals (`[]`, `none`, `ok`/`err`) are hoisted
  into annotated `let` bindings so they type-check.

- **Stdlib catalogue** — `src/generator/catalogue.rs`. Signatures are
  extracted from a **machine source**: the bundled `stdlib/*.almd`
  declaration files, parsed with the real Almide parser. A curated
  *weight table* overlays the parsed surface to bias selection toward the
  historic divergence clusters (string/Unicode, float formatting,
  closures/HOFs).

- **Value pools** — `src/generator/pools.rs`. Named, commented tables of
  divergence-prone literals: multibyte strings (`日本語`, emoji, `é`,
  combining marks, `ß`), float boundaries (`-0.0`, `5e-324`, `1e300`,
  `0.1+0.2` shapes), int extremes (`i64::MIN/MAX`, width boundaries).

- **Mutation (~30%)** — `src/generator/mutate.rs`. Parses the
  `main`-bearing corpus (`spec/wasm_cross`, `examples`, …), strips `test`
  blocks, and applies type-preserving AST mutations (literal
  perturbation from the pools, equal-kind subexpression swap, statement
  duplication). `// wasm:skip` files are excluded (known divergences).

Determinism: a single `SplitMix64` (`src/rng.rs`) seeded
`for_program(seed, index)`. No wall-clock / fs / process calls appear in
generated programs (effects whitelist: `println`/`print`).

## The oracle ladder

`src/oracle/` — cheap→expensive, first failure classifies the program:

| Rung | Check | Failure means |
|------|-------|---------------|
| a | `almide check` accepts | **generator bug** (we promised well-typed) — counted, not a finding |
| b | `parse∘fmt` is idempotent | formatter instability |
| c | native build + run (no ICE) | native codegen failure |
| d | wasm build + validate | wasm codegen failure |
| e | run both, byte-compare stdout/exit | **divergence** (or a hang) |

A future **reference-interpreter** rung slots in behind the
`ReferenceOracle` trait (so a divergence can be pinned to *which* target
is wrong). This crate does not depend on it.

## Minimizer

`src/minimize.rs` — delta-debugging: statement removal then expression
simplification, re-running the ladder and keeping a shrink only if the
same finding kind reproduces. Output lands in `findings/<kind>/`:
`repro.almd`, `original.almd`, `meta.txt` (seed/index/replay command),
`native.out`, `wasm.out`. Findings are deduplicated by `(kind, summary)`.

## Throughput

The WASM build + native cargo build are the bottleneck. Workers each own
an **isolated** build scratch dir (`ALMIDE_RUN_PROJECT_DIR`), so the
shared-`/tmp` build flock never serializes them — throughput scales with
cores. Measured locally (Apple M-series, see the campaign summary line):
roughly **40–120 programs/min** depending on `--jobs` and the
native-build cache warmth (the per-program cost is dominated by the
native `cargo` rebuild; warm caches are ~0.25 s/program).

## Nightly CI

`.github/workflows/fuzz-nightly.yml` runs a time-budgeted campaign at
03:00 UTC (and on manual dispatch), uploads `findings/` as an artifact,
and opens/updates a `fuzz`-labelled tracking issue when findings > 0. PR
CI is intentionally left untouched — the fuzzer is nightly only.
