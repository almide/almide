# WASM Output — What's in the Binary, and Why

Almide emits WebAssembly **directly** — no LLVM, no Cranelift, no wasm-bindgen,
and no compiled standard-library object code inside the module. Since the
commissioning (#1599) there are **two verified renderers**: the **structural
leg** (crates/almide-wasm, wasm-encoder, the default `--target wasm` path) and
the **incumbent v1 leg** (the certified MIR→WAT renderer, reachable with
`ALMIDE_WASM_INCUMBENT=1` and as the automatic reroute for shapes the
structural leg declines). This document dissects real modules from BOTH legs
byte by byte and states exactly what the size claims mean.

The headline Hello, world bytes are CI-derived: `docs/benchmarks/wasm-size.txt`
(re-measured by `scripts/gen-readme-stats.sh`, gated since #1605). Everything
else below was measured 2026-08-27 on the current `develop` compiler with
`wasm-opt` (Binaryen) 132 and `wasm-objdump` (WABT) 1.0.41; the Rust comparison
rows retain their 2026-07-23 measurement (`rustc 1.94.1`). Reproduce any number
with the commands at the bottom.

## The headline numbers (measured 2026-08-27)

| Program | structural (default, verified) | structural + `-Oz` | incumbent v1 (verified) | incumbent + `-Oz` |
|---|---:|---:|---:|---:|
| Hello, world | 4,459 B | **364 B** | 1,096 B | 788 B |
| FizzBuzz 1–100 | 4,716 B | **1,162 B** | 2,168 B | 1,346 B |
| Fibonacci (recursive) | 4,490 B | **894 B** | 1,791 B | 1,035 B |
| Recursive-generic ADT repr¹ | **14,088 B** | **9,320 B** | 34,723 B | 21,883 B |

¹ `spec/wasm_cross/compound_repr_recursive_interp.almd` — recursive ADTs,
mutually recursive records, generic instantiations, and their full `${…}` repr
machinery.

**The crossover, in one paragraph.** The structural leg carries a fixed
~3.5 KB runtime preamble (allocator family, COW gates, the itoa scratch, OOM
path — 38 support functions in the Hello, world module), so on near-empty
programs its raw module is larger than the incumbent's. On real programs the
relationship inverts — the ADT-repr row is 14.1 KB structural vs 34.7 KB
incumbent, and over the full 600+-fixture corpus the greenfield VERDICT
records **4.11 MB aggregate structural vs 10.54 MB incumbent**. And after
`wasm-opt -Oz`, the structural module is the smallest of all four columns on
every row measured: the preamble is statically reachable (a table-driven
allocator that the module's own call graph mostly never enters), and `-Oz`'s
whole-module DCE deletes what the in-renderer reachability pass must
conservatively keep. Hello, world drops 4,459 → 364 bytes: **one** function
survives.

Two honest framings:

- The July Rust comparison stands as context: Rust `wasm32-wasip1` Hello,
  world is 64,430 B default / 40,754 B with the full size profile
  (`opt-level="z"`, `lto`, `strip`, `panic="abort"`, `codegen-units=1`,
  measured 2026-07-23). The gap is a *toolchain floor* difference — Rust links
  `std`'s formatting machinery into every `println!` — not a statement about
  language quality.
- The **shipped** Almide binary is the *verified* one (see below). The
  `wasm-opt` column requires the explicit `--wasm-opt` opt-in, which takes
  the module outside the verified envelope.

## Section anatomy — Hello, world, both legs

Structural leg, as shipped (4,459 B, via `wasm-objdump -h`):

| Section | Size | Contents |
|---|---:|---|
| Type | 113 B | 19 signatures |
| Import | 179 B | 5 WASI imports (`fd_write`, `proc_exit`, `random_get`, `clock_time_get`, `fd_read`) |
| Function | 40 B | index table for 39 functions |
| Table + Elem | 6 B | funcref table (empty here — no closures) |
| Memory | 3 B | one linear memory |
| Global | 97 B | 14 globals (heap cursor, free-list heads, park-buffer state, …) |
| Export | 35 B | `_start` + memory |
| Code | 3,694 B | 39 bodies: `main` + the 38-function preamble (allocator family with size classes, RC/COW gates, itoa, string equality, the WASI park-buffer glue) |
| Data | 261 B | the string pool (`"Hello, world!"` + the OOM/trap messages) |

The same module after `wasm-opt -Oz` (364 B): Type 12 B, Import 35 B
(`fd_write` alone survives), Function 2 B, Memory 3 B, Global 8 B, Export
35 B, Code 92 B (**one** function), Data 149 B. That is the whole story of the
structural preamble: it is *linked* support code, and a whole-module optimizer
proves almost all of it dead for a program this small. (The "tier-0 preamble"
idea — dropping the allocator family in-renderer when DCE proves no dynamic
allocation is reachable — would move the shipped 4.4 KB most of the way toward
that 364 B without leaving the verified envelope; this document is where it
gets measured if it lands.)

Incumbent v1 leg (1,096 B): Type 26 B, Import 70 B (2 WASI imports), Function
9 B, Memory 3 B, Global 38 B, Export 40 B, Code 739 B (8 functions — `alloc`,
`rc_dec`, `main`, `print_str`, `_start` and three helpers), Data 46 B, plus a
98-byte `name` custom section (function names only; locals stripped — a
wasmtime trap backtrace prints them, worth the bytes). The incumbent's +393 B
since the July measurement (703 → 1,096 B) is the deterministic-meter and
stdin plumbing that landed with the C-320 arc.

### Where the stdlib went

Almide's stdlib is 971 functions across 43 modules — but they are **self-hosted
in Almide** and linked *on demand*. The compiler scans the lowered program for
called dispatch names (`string.len`, `map.set`, `list.sort_by`, …) and links
only the matching self-host sources, iterating to a fixpoint so a linked
function's own callees follow. Hello, world links **zero** stdlib functions;
FizzBuzz links the handful behind `int.to_string`. An unused module
contributes nothing — and anything it *does* pull in that turns out
unreachable is swept by reachability DCE before assembly.

### Why the value model stays small

- **i64-uniform slots.** Every scalar is an i64; every heap value is a
  length-prefixed block addressed by an i32 handle. No per-type layouts, no
  metadata.
- **Variants are `tag @ slot 0`.** A `match` compiles to integer compares —
  no vtables, no type descriptors.
- **Monomorphization → direct calls.** Generics are specialized at compile
  time; the only indirect calls are closures, through a single funcref table.
- **Reachability DCE, in two layers.** The demand linker decides *which
  self-hosted stdlib sources* enter the program; the renderer's reachability
  pass then drops unreached helpers, imports, and data segments before the
  module is assembled.

## What's still not the smallest possible module, and why

The verified pipeline **ships the bytes its own rendering process produced**.
Every module built on the default path is emit-time validated, and the
incumbent leg additionally carries a machine-checked ownership/refcount
certificate re-verified by the Rocq-checked kernel each build. `wasm-opt` is a
different kind of thing: an **external, unverified transform applied to the
renderer's finished output** — running it replaces bytes the pipeline produced
with bytes a separate, un-certified tool rewrote. That line is why it stays
opt-in:

```bash
almide build app.almd --target wasm --wasm-opt   # runs: wasm-opt -Oz
#   --enable-nontrapping-float-to-int --enable-tail-call --enable-bulk-memory
#   --enable-mutable-globals
#   (the features the two legs actually emit — no SIMD in the default output;
#    bulk-memory is the structural leg's memory.copy and mutable-globals its
#    exported globals, #1616)
```

**`-Oz` trades speed for those bytes.** The incumbent renderer versions hot
loops into a guarded fast path with bounds checks discharged up front;
`-Oz`'s code folding merges the near-identical copies back into one checked
loop, measured ~3× slower on spectralnorm. Use `-Oz` for size-critical cold
code; benchmark before applying it to compute kernels.

## Determinism and the cross-target contract

The emitted bytes are **deterministic across host architectures**: the
compiler built natively (x86-64/aarch64) and the compiler built as wasm32 (the
playground) produce byte-identical modules for the cross-target fixture corpus
— a CI gate (`scripts/check-host-determinism.sh`), not an aspiration. And
every program that compiles for both targets produces **byte-identical
stdout/stderr/exit code** native ⇄ wasm, tracked contract-by-contract in
[docs/contracts/](../contracts).

## Reproducing the measurements

```bash
# Almide — structural (default) and incumbent legs, plain and -Oz
printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > hello.almd
almide build hello.almd --target wasm -o hello.wasm                    # structural, verified
almide build hello.almd --target wasm --wasm-opt -o hello.min.wasm     # structural, -Oz
ALMIDE_WASM_INCUMBENT=1 almide build hello.almd --target wasm -o hello_v1.wasm
wasm-objdump -h hello.wasm                                             # the section tables above

# Rust (same target, full size profile — 2026-07-23 numbers)
cargo new rhello && cd rhello
# [profile.release] opt-level="z", lto=true, strip=true, panic="abort", codegen-units=1
cargo build --release --target wasm32-wasip1
```
