# WASM Output — What's in the Binary, and Why It's Small

Almide emits WebAssembly **directly**: the certified MIR renders to WAT and assembles
to a binary. There is no LLVM, no Cranelift, no wasm-bindgen, and no compiled
standard-library object code inside the module. This document dissects a real
module byte by byte, explains where the size comes from, and states exactly what
the size claims mean.

All numbers below were measured on 2026-07-23 with the current `develop` compiler,
`rustc 1.94.1` (for the Rust comparison), and `wasm-opt` (Binaryen) version 129.
Reproduce any of them with the commands shown.

## The headline numbers

| Program | Almide (verified, as shipped) | Almide + `wasm-opt -Oz` | Rust `--release` (default) | Rust size-tuned¹ |
|---|---:|---:|---:|---:|
| Hello, world | **770 B** | **548 B** | 64,430 B | 40,754 B |
| FizzBuzz 1–100 | **1,793 B** | **1,092 B** | — | 42,434 B |
| Fibonacci (recursive) | 1,441 B | 771 B | — | — |
| Closure + `call_indirect` | 2,744 B | 1,672 B | — | — |
| Variant match + Float display² | 11,965 B | 6,868 B | — | — |
| Recursive-generic ADT repr³ | 32,264 B | 17,941 B | — | — |

¹ `wasm32-wasip1`, `opt-level = "z"`, `lto = true`, `strip = true`, `panic = "abort"`,
`codegen-units = 1` — the full size-minimization profile, which most projects do not enable.
² The jump comes from `float.to_string`: printing a Float links the self-hosted
Dragon4 shortest-round-trip printer — the single largest thing the demand linker
ever pulls in. Programs that never display a Float never pay for it.
³ `spec/wasm_cross/compound_repr_recursive_interp.almd` — recursive ADTs, mutually
recursive records, generic instantiations, and their full `${…}` repr machinery.

Two honest framings for that table:

- The comparison is against **Rust on the same `wasm32-wasip1` target**, because
  Rust is Almide's native-side backend and the closest apples-to-apples toolchain.
  The gap is a *toolchain floor* difference — Rust links `std`'s formatting
  machinery into every `println!`; Almide's runtime is a few hundred bytes of
  hand-written WAT, reachability-pruned per program — not a statement about
  language quality.
- The **shipped** Almide binary is the *verified* one (see below). The
  `wasm-opt` column requires running it yourself, which takes the module
  outside the verified envelope.

## What is actually inside a module

Section layout of the 770-byte Hello, world (via `wasm-objdump -h`):

| Section | Size | Contents |
|---|---:|---|
| Type | 21 B | 4 function signatures |
| Import | 35 B | 1 WASI import (`fd_write`) |
| Function | 6 B | index table for 5 functions |
| Memory | 3 B | one linear memory |
| Global | 23 B | 4 globals (bump pointer, env cache, …) |
| Export | 19 B | `_start` + memory |
| Code | 567 B | the 5 function bodies |
| Data | 19 B | 1 segment — the `"Hello, world!"` string |
| Custom `name` | 50 B | function names only (locals stripped — see below) |

The 5 functions are `alloc`, `rc_dec`, `main`, `print_str`, `_start` — exactly
what this program's call graph reaches, out of a **~50-function hand-written
runtime preamble** (bump allocator, refcounts, list primitives, `itoa`,
division-by-zero traps, WASI glue for args/env/fs/…) plus the self-hosted
stdlib. **Reachability DCE inside the verified renderer** prunes every import,
preamble helper, data segment, and program function the compiled call graph
doesn't reach — before the module is assembled, not as a post-processing pass
(see below). That preamble **is the entire runtime**: there is no GC, no
interpreter, no reflection metadata, and no compiled stdlib object code.

### Where the stdlib went

Almide's stdlib is 834 functions across 39 modules — but they are **self-hosted
in Almide** and linked *on demand*. The compiler scans the lowered program for
called dispatch names (`string.len`, `map.set`, `list.sort_by`, …) and links only
the matching self-host sources, iterating to a fixpoint so a linked function's
own callees follow. Hello, world links **zero** stdlib functions; FizzBuzz links
the handful behind `int.to_string`. An unused module contributes nothing — and
anything it *does* pull in that turns out unreachable (e.g. a helper the linked
source calls only on a branch this program's monomorphization never takes) is
then swept by the same reachability DCE described above.

### Why the value model stays small

- **i64-uniform slots.** Every scalar is an i64; every heap value is a
  length-prefixed block of i64 slots addressed by an i32 handle. No per-type
  layouts to describe, no metadata to carry.
- **Variants are `tag @ slot 0`.** A `match` compiles to integer compares —
  no vtables, no type descriptors.
- **Monomorphization → direct calls.** Generics are specialized at compile
  time; the only indirect calls are closures, through a single funcref table.
- **Reachability DCE, in two layers.** The demand linker above decides *which
  self-hosted stdlib sources* enter the program at all; the wasm renderer's own
  reachability pass then decides which of the resulting preamble helpers,
  imports, data segments, and functions the final call graph actually reaches,
  and drops the rest before assembling the module.

## What's still not the smallest possible module, and why

The verified pipeline **ships the bytes its own rendering process produced**.
Every module built on the default path carries a machine-checked
ownership/refcount certificate (re-verified by the Rocq-checked kernel on each
build). Reachability DCE and the name-section trim above happen *inside* that
same certified renderer, before the module is ever assembled — the shipped
bytes are still exactly what the trust-spine produced, nothing external has
touched them. `wasm-opt` is a different kind of thing: an **external,
unverified transform applied to the renderer's finished output**, so running
it would replace bytes the trust-spine produced with bytes a separate,
un-certified tool rewrote. That line is why it stays opt-in:

- The **name section** keeps one entry per function (~50 B here) — that's
  what a wasmtime trap backtrace prints (`<unknown>!funcname`), and the
  diagnostics value is worth the few dozen bytes it costs. Only *local* names
  (`$v1`, `$v2`, …, zero backtrace value, and often the single largest part of
  an un-trimmed name section) are dropped.
- `wasm-opt`'s further gains — instruction selection, local coalescing,
  cross-function inlining, and more aggressive DCE than a reachability BFS
  can safely do inline — go beyond what the verified renderer itself
  guarantees, so they stay a separate, explicit step.

If you want minimum bytes and accept leaving the verified envelope:

```bash
wasm-opt -Oz --all-features app.wasm -o app.min.wasm
```

(`--all-features` is required — the runtime's fs helpers return multi-value
pairs, and the float printer uses post-MVP integer ops. Binaryen then squeezes
further: Hello, world drops from 770 B to 548 B.)

## Determinism and the cross-target contract

The emitted bytes are **deterministic across host architectures**: the compiler
built natively (x86-64/aarch64) and the compiler built as wasm32 (the
playground) produce byte-identical modules for all 270 cross-target fixtures —
a CI gate (`scripts/check-host-determinism.sh`), not an aspiration. And every
program that compiles for both targets produces **byte-identical stdout/stderr/
exit code** native ⇄ wasm, tracked contract-by-contract in
[docs/contracts/](./contracts/).

## Reproducing the measurements

```bash
# Almide
echo 'fn main() -> Unit = println("Hello, world!")' > hello.almd
almide build hello.almd --target wasm -o hello.wasm      # 770 B (verified)
wasm-opt -Oz --all-features hello.wasm -o hello.min.wasm  # 548 B
wasm-objdump -h hello.wasm                                # the section table above

# Rust (same target, full size profile)
cargo new rhello && cd rhello
# [profile.release] opt-level="z", lto=true, strip=true, panic="abort", codegen-units=1
cargo build --release --target wasm32-wasip1             # 40,754 B
```
