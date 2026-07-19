# WASM Output — What's in the Binary, and Why It's Small

Almide emits WebAssembly **directly**: the certified MIR renders to WAT and assembles
to a binary. There is no LLVM, no Cranelift, no wasm-bindgen, and no compiled
standard-library object code inside the module. This document dissects a real
module byte by byte, explains where the size comes from, and states exactly what
the size claims mean.

All numbers below were measured on 2026-07-20 with the current `develop` compiler,
`rustc 1.94.1` (for the Rust comparison), and `wasm-opt` (Binaryen) version 129.
Reproduce any of them with the commands shown.

## The headline numbers

| Program | Almide (verified, as shipped) | Almide + `wasm-opt -Oz` | Rust `--release` (default) | Rust size-tuned¹ |
|---|---:|---:|---:|---:|
| Hello, world | **8,713 B** | **874 B** | 64,430 B | 40,754 B |
| FizzBuzz 1–100 | **10,515 B** | **1,580 B** | — | 42,434 B |
| Fibonacci (recursive) | 10,044 B | 1,139 B | — | — |
| Closure + `call_indirect` | 11,414 B | 1,898 B | — | — |
| Variant match + Float display² | 34,407 B | 6,460 B | — | — |
| Recursive-generic ADT repr³ | 53,928 B | 20,963 B | — | — |

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
  machinery into every `println!`; Almide's runtime is a few KB of hand-written
  WAT — not a statement about language quality.
- The **shipped** Almide binary is the *verified* one (see below). The 874 B
  figure requires running `wasm-opt` yourself, which takes the module outside
  the verified envelope.

## What is actually inside a module

Section layout of the 8,713-byte Hello, world (via `wasm-objdump -h`):

| Section | Size | Contents |
|---|---:|---|
| Type | 121 B | 19 function signatures |
| Import | 658 B | 17 WASI imports (`fd_write`, `proc_exit`, `random_get`, `clock_time_get`, `path_open`, …) |
| Function | 41 B | index table for 40 functions |
| Memory | 3 B | one linear memory |
| Global | 23 B | 4 globals (bump pointer, env cache, …) |
| Export | 19 B | `_start` + memory |
| Code | 4,856 B | the 40 function bodies |
| Data | 344 B | 12 segments — the string literals and runtime constants |
| Custom `name` | 2,618 B | debug names for every function/local (kept — see below) |

The 40 functions are: `main`, `_start`, and a **38-function hand-written runtime
preamble** — `alloc`/`alloc8` (bump allocator), `rc_inc`/`rc_dec` (refcounts),
list primitives (`list_new`/`list_get`/`list_set`/`list_push`/`elem_addr` with
bounds checks), `itoa_append`/`print_int`/`print_str`, division-by-zero traps,
and the WASI glue (`args_get_list`, `env_get`, `path_norm`, `read_text_file`,
`read_dir`, …). That preamble **is the entire runtime**: there is no GC, no
interpreter, no reflection metadata, and no compiled stdlib object code.

### Where the stdlib went

Almide's stdlib is 834 functions across 39 modules — but they are **self-hosted
in Almide** and linked *on demand*. The compiler scans the lowered program for
called dispatch names (`string.len`, `map.set`, `list.sort_by`, …) and links only
the matching self-host sources, iterating to a fixpoint so a linked function's
own callees follow. Hello, world links **zero** stdlib functions; FizzBuzz links
the handful behind `int.to_string`. An unused module contributes nothing.

### Why the value model stays small

- **i64-uniform slots.** Every scalar is an i64; every heap value is a
  length-prefixed block of i64 slots addressed by an i32 handle. No per-type
  layouts to describe, no metadata to carry.
- **Variants are `tag @ slot 0`.** A `match` compiles to integer compares —
  no vtables, no type descriptors.
- **Monomorphization → direct calls.** Generics are specialized at compile
  time; the only indirect calls are closures, through a single funcref table.
- **Reachability DCE.** Emission is rooted at the exports; unreached functions
  are dropped before rendering.

## Why the shipped module is not the smallest possible module

The verified pipeline **ships the bytes it certified**. Every module built on
the default path carries a machine-checked ownership/refcount certificate
(re-verified by the Rocq-checked kernel on each build), and any post-hoc
rewrite — including `wasm-opt` — is an *unverified transform* that would
invalidate the correspondence between the certificate and the artifact. So:

- The **name section (~2.6 KB)** stays: it names every function in a trap
  backtrace, which the diagnostics contract values more than the bytes.
- The **fixed WASI import block and fs/env runtime helpers** stay even when
  unused: the preamble is one audited text, not a per-program assembly.

If you want minimum bytes and accept leaving the verified envelope:

```bash
wasm-opt -Oz --all-features app.wasm -o app.min.wasm
```

(`--all-features` is required — the runtime's fs helpers return multi-value
pairs, and the float printer uses post-MVP integer ops. Binaryen's DCE then
strips the unused preamble and the name section: Hello, world drops from
8,713 B to 874 B.)

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
almide build hello.almd --target wasm -o hello.wasm      # 8,713 B (verified)
wasm-opt -Oz --all-features hello.wasm -o hello.min.wasm  # 874 B
wasm-objdump -h hello.wasm                               # the section table above

# Rust (same target, full size profile)
cargo new rhello && cd rhello
# [profile.release] opt-level="z", lto=true, strip=true, panic="abort", codegen-units=1
cargo build --release --target wasm32-wasip1             # 40,754 B
```
