# WASM Output — What's in the Binary, and Why

Almide emits WebAssembly **directly** — no LLVM, no Cranelift, no wasm-bindgen,
and no compiled standard-library object code inside the module. The one
renderer is the **structural leg** (crates/almide-wasm, wasm-encoder, the
`--target wasm` path). The **incumbent v1 leg** (the certified MIR→WAT
renderer) lost its last route in #2752 and was deleted in #2761; the
measurements below that name it are historical and cannot be reproduced on the
current compiler. This document dissects real modules byte by byte and states
exactly what the size claims mean.

The headline Hello, world bytes are CI-derived: `docs/benchmarks/wasm-size.txt`
(re-measured by `scripts/gen-readme-stats.sh`, gated since #1605). Everything
else below was measured 2026-08-27 on the current `develop` compiler with
`wasm-opt` (Binaryen) 132 and `wasm-objdump` (WABT) 1.0.41; the Rust comparison
rows retain their 2026-07-23 measurement (`rustc 1.94.1`). Reproduce any number
with the commands at the bottom.

## The headline numbers (measured 2026-08-27; the incumbent columns are historical, retired by #2761)

| Program | structural (shipped) | structural + `-Oz` | incumbent v1 (retired) | incumbent + `-Oz` (retired) |
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

## Section anatomy — Hello, world (structural; incumbent row historical)

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

Incumbent v1 leg (historical — retired by #2761; 1,096 B): Type 26 B, Import 70 B (2 WASI imports), Function
9 B, Memory 3 B, Global 38 B, Export 40 B, Code 739 B (8 functions — `alloc`,
`rc_dec`, `main`, `print_str`, `_start` and three helpers), Data 46 B, plus a
98-byte `name` custom section (function names only; locals stripped — a
wasmtime trap backtrace prints them, worth the bytes). The incumbent's +393 B
since the July measurement (703 → 1,096 B) is the deterministic-meter and
stdin plumbing that landed with the C-320 arc.

### Where the stdlib went

Almide's stdlib is 1019 functions across 43 modules — but they are **self-hosted
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
Every module built on the default path is emit-time validated. (The retired
incumbent leg additionally carried a machine-checked ownership/refcount
certificate re-verified by the Rocq-checked kernel each build; the structural
leg's certificate is pending, #2755–#2760.) `wasm-opt` is a
different kind of thing: an **external, unverified transform applied to the
renderer's finished output** — running it replaces bytes the pipeline produced
with bytes a separate, un-certified tool rewrote. That line is why it stays
opt-in:

```bash
almide build app.almd --target wasm --wasm-opt   # runs: wasm-opt -Oz
#   --enable-nontrapping-float-to-int --enable-tail-call --enable-bulk-memory
#   --enable-mutable-globals
#   (the features the structural leg actually emits — no SIMD in the default output;
#    bulk-memory is the structural leg's memory.copy and mutable-globals its
#    exported globals, #1616)
```

**`-Oz` trades speed for those bytes.** On the (now retired, #2761) incumbent
renderer, which versioned hot loops into a guarded fast path with bounds checks
discharged up front, `-Oz`'s code folding merged the near-identical copies back
into one checked loop, measured ~3× slower on spectralnorm. Use `-Oz` for size-critical cold
code; benchmark before applying it to compute kernels.

## Determinism and the cross-target contract

The emitted bytes are **deterministic across host architectures**: the
compiler built natively (x86-64/aarch64) and the compiler built as wasm32 (the
playground) produce byte-identical modules for the cross-target fixture corpus
— a CI gate (`scripts/check-host-determinism.sh`), not an aspiration. And
every program that compiles for both targets produces **byte-identical
stdout/stderr/exit code** native ⇄ wasm, tracked contract-by-contract in
[docs/contracts/](../contracts).

### The embedded host's call-stack budget (#3435)

Recursion past a target's call-stack resources is a resource limit, not a
cross-target promise (C-196). Where the limit sits is still a choice for the
lane we ship: the embedded host behind `almide run --target wasm`, `almide
bench --target wasm` and the wasm leg of `almide test` sets wasmtime's
`max_wasm_stack` to **8 MiB** (`EMBEDDED_WASM_STACK` in
`crates/almide-wasm-run/src/host.rs`), the size of native's main-thread stack.
It runs each guest — and each `fan` worker instance — on a host thread with a
64 MiB native stack, since wasmtime needs the host stack to exceed the wasm
budget plus host frames. wasmtime's own default is 512 KiB, which the host
used until #3435.

Measured 2026-10-06 (macOS aarch64, release build; the deepest argument that
still answers):

| recursion shape | native | embedded, 512 KiB | embedded, 8 MiB |
|---|---:|---:|---:|
| non-tail tree walk returning `T!` (#3434's `infer`) | ~43,000 | ~5,400 | ~87,000 |
| recursion with four heap locals per frame | ~32,500 | ~5,400 | ~87,000 |
| `1 + depth(f)` over a tree | no limit (LLVM makes it a loop) | ~32,500 | ~523,000 |

Stock runtimes keep their own limits: `wasmtime run` defaults to 512 KiB
(`-W max-wasm-stack=N` raises it), and browsers set theirs.

## Measuring allocation: the watermark and the counter (#2407)

Every structural-leg module exports its bump-heap pointer as the `__heap`
global (`__heap_high` too when a region window rewinds it). The embedded host
reads it after the run and `crates/almide-wasm/tests/alloc_ledger.rs` pins it
per corpus fixture in `crates/almide-wasm/tests/golden/alloc-baseline.txt` —
the **watermark**. It costs nothing (a global the module maintains anyway) and
it is a **peak**: the row is `pool size + the highest the heap ever reached`.
A peak cannot show churn. The allocator keeps size-class free lists, so a
loop that allocates and releases a fresh block per iteration reaches almost
the same watermark as a single allocation.

The **counter** is the instrument for that, the wasm twin of native's
`ALMIDE_ALLOC_COUNT` (#2228). It is an *emit-time* switch:

```bash
ALMIDE_WASM_ALLOC_COUNT=1 almide run app.almd --target wasm
# stderr, after the program's own output:
# __ALMD_WASM_ALLOC allocs=2000 reused=1996 bytes=9780 frees=2000 heap_end=65872
```

| field | what it counts |
|---|---|
| `allocs` | every `$alloc` call (free-list hit or bump) |
| `reused` | the `$alloc` calls a size-class free-list pop served (the rest advanced the bump head) |
| `bytes` | the sum of the payload lengths requested |
| `frees` | every `$free` call (filed into a free list, or abandoned as too small / too large) |
| `reclaimed` | blocks a region window's restore took back wholesale, without a `$free` (#1961) |
| `live` | `allocs − frees − reclaimed`: the heap blocks never released (see below) |
| `heap_end` | the watermark, on the same line, so the pair can be read together |

Under the switch the emitter appends five `i64` globals **after** the top-let
globals (no existing index moves, no byte of the heap layout changes) and
`$alloc` / `$free` / a region window's restore bump them; the host reads them
as `__alloc_count`, `__alloc_reused`, `__alloc_bytes`, `__free_count`,
`__region_reclaimed`. **Off — the default — none
of it is emitted**: the module is byte-identical to a build that never heard
of the switch, so neither the size ratchet nor the alloc ledger moves, and the
proof-transcribed runtime trees (`proofs/StructuralRuntime.v`) stay the
shipped ones. An unarmed module reports the counters as *absent*, never as
zero (`RunResult::alloc_count` is `None`).

The counter is ledgered beside the watermark: the same
`ALMIDE_UPDATE_ALLOC=1` regeneration writes
`crates/almide-wasm/tests/golden/alloc-count-baseline.txt` (`allocs reused
bytes frees` per fixture, `~` calibrated out, `!` refused), and the check run
also holds the armed module to the unarmed one — same stdout, same watermark
— so the instrument is proven not to perturb what it measures.

**Live at exit.** Native drops every value when it goes out of scope; the
wasm leg releases by hand, so a missing release is a wasm-only defect that
output parity cannot see (#2932, #2944). `live` is the number of blocks never
released. A top-let global is live *by design* until exit, so the armed `main`
releases every droppable top-let right before its epilogue — a shipped module
never does — and `live` after a normal exit is exactly the leaked blocks. The
same update run writes `crates/almide-wasm/tests/golden/live-at-exit-baseline.txt`:
only the fixtures whose `live` is not 0, each naming the issue that owns its
mechanism, shrink-only (an unlisted fixture must read 0; a row may only go down
and must be lowered when it does; a new row carries `#?` until an issue is
named, which the check refuses). Runs that do not exit 0 are not judged. The
generative fuzzer applies the same measurement to every clean program (the
`LeakAtExit` finding kind, its own nightly class under the `fuzz-leak` label).

**Reading the pair.** When a watermark row moves, the count row says which
half moved: more `allocs` / `bytes` is *allocation*, the same counts with a
higher watermark is *liveness* (something held longer, so the free lists had
nothing to hand back). When the watermark does not move at all, the count
row still can — that is churn. Three programs where the two disagree:

| program | `allocs` | `reused` | `bytes` | watermark delta |
|---|---:|---:|---:|---:|
| one list literal (`let xs = [1, 2, 3]`) | 1 | 0 | 24 | +0 (the base) |
| the same plus a second literal | 2 | 0 | 48 | +64 |
| 1000 × (`"row " + int.to_string(i)`, dropped each iteration) | 2000 | 1996 | 9780 | +48 |

The third line is the point: 2000 allocations of 9,780 bytes in total moved
the watermark by 48 bytes, because 1,996 of them were served from the free
lists. On the watermark alone that loop is indistinguishable from two
allocations. The corpus has the same shape at scale (rows of the two ledgers,
2026-09-23):

| fixture | `allocs` | `reused` | `bytes` requested | `__heap` row |
|---|---:|---:|---:|---:|
| `variant_result_one_credit_churn` | 1,638,000 | 1,629,810 | 104,832,000 | 1,114,096 |
| `mut_param_alias_cow` | 8,170 | 8,049 | 72,057,178 | 209,056 |
| `grain_gc_shapes` | 5,059 | 5,035 | 16,030,437 | 131,360 |

Each requested a hundred to a thousand times more bytes than its watermark
shows, and its watermark row would not move if the churn doubled. The
instrument covers the structural leg, which since #2761 is every wasm program.

## JS host (`--host js`, #2265)

`almide build app.almd --target wasm --host js -o dist/app.wasm` writes
`dist/app.js` and `dist/app.d.ts` next to the module — the host glue the
compiler already has every fact to write, so the artifact runs in a page or
under node without a hand-written WASI stub, import object or String
marshalling. `app.js` is a dependency-free ES module:

- `init(source?, hooks?)` compiles and instantiates. `source` may be omitted
  (the `.wasm` next to the `.js` is loaded: `fs.readFile` under node, `fetch`
  in a page), or be bytes, a `Response`, a promise of either, or a compiled
  `WebAssembly.Module`. `hooks.stdout` / `hooks.stderr` receive raw
  `Uint8Array` chunks (default: `process.stdout` under node, `console.log`
  per line in a page).
- Only the `wasi_snapshot_preview1` imports the SHIPPED module names are
  shimmed (`fd_write`, `proc_exit` → a thrown `AlmideExit`, the clock /
  random / read floor). Nothing else is linked, so nothing else is emitted
  (#2276): the glue is derived from the bytes after the optional `--wasm-opt`
  rewrite, so a `--host js --wasm-opt` build of a `println`-only program
  carries exactly `fd_write` and `proc_exit`. An import outside the shim
  table is a build-time refusal naming it, not a `LinkError` in the page.
- `@extern(wasm, "js", "name")` imports are wired through
  `init(source, { js: { name } })`; a `String` argument is decoded from the
  block header before the user function runs, a `String` return is encoded
  into a fresh block the guest owns. The structural leg lowers each
  declaration to a declared import (#2275): the fn's slot is emitted as a
  loud stub, and a post-pass over the finished bytes (`imports.rs`) turns
  every stub into `(import module name (func ...))` behind the five
  `almide.*` imports, renumbering calls, exports and table entries through
  one map. The ABI is the one exports use (`Int` → i64, `Float` → f64,
  `Bool` → i32, `String` → i32 block, `Unit` → no result); a `String`
  argument is borrowed across the call (the host releases nothing) and a
  `String` result is a fresh block the guest owns. A `rs`/`rust` extern has
  no wasm host and stays a wall (E082). The `// @leg: structural` line
  of `spec/wasm_host_js/extern_js.almd` is the ratchet row that makes a
  route change visible; `almide run --target wasm` refuses such a program by
  name (it has no host for the import) and points at `--host js`.
- Every `pub fn` gets a wrapper: `Int` ↔ `number` (a `RangeError` outside
  ±2^53 rather than a silent truncation — pass a `BigInt`-aware hook to keep a
  wider value exact), `Float`, `Bool`, `String`, `Unit`, and (#3354) the block
  shapes, as params and returns: `Bytes` ↔ `Uint8Array` (a copy), `List[T]` ↔
  `Array<T>` (any element type here, nested lists included), `Option[T]` ↔
  `T | undefined` (`none` is `undefined`; `null` is accepted going in), and a
  record ↔ a plain object with the record's fields in declared order (a
  missing field is a `TypeError` before the call). Each block is built and
  read by the layout the emitter records per export
  (`almide_wasm::host_exports::export_params_noted` / `export_rets`: element
  stride, field offsets, record size), never re-derived by the host; a record
  that disagrees with the source type is a build-time refusal. Ownership
  follows the recorded param ownership: a block the callee owns is its to
  release, a borrowed one the host releases after the call. Because the
  module's `__release` is flat, the host walks the shape when it drops the
  last credit on a block and releases the children that block held. Variants,
  `Map`, `Set`, tuples and functions are still a build-time refusal naming the
  function and the type. `memoryBytes()` (shipped with the block helpers)
  reports the linear memory size, so a host can check for leaks.
- An `effect fn` export (and a fn declaring `Result[T, String]`) returns one
  `Result` block (tag at payload+0, 0 = ok; value slot at payload+8), whatever
  its declared `T` (#3352). Its wrapper unwraps it: ok is `T` marshalled as
  above, err throws `AlmideError` (an `Error`) whose `message` is the err
  String. The block is released through `__release`, and the slot's own
  String block too when the Result held the last reference. The wrapper is
  chosen from the return ABI the emitter records for each export
  (`almide_wasm::host_exports::export_rets`), not from the source type, and a
  record that disagrees with the source type (or an err that is not a
  String) is a build-time refusal naming the function. The `.d.ts` gives `T`
  and declares `AlmideError`, which ships only when some export unwraps a
  Result. Gate: `spec/wasm_host_js/effect_exports.almd` runs every
  marshalled type × {fn, effect fn} × {ok, err} under node.
- A hook that throws (or, for one marked `returns: promise`, rejects) (#3356). An
  exception that unwinds through wasm skips every release the unwound frames
  would have run, so the glue never lets one through:
  - A **fallible** extern is an `effect fn` or one declaring
    `Result[T, String]` (`T` a scalar, `String` or `Unit`). It imports as an
    i32 `Result` block the host builds. The hook's value is ok and its throw
    is err (the message). The Almide caller propagates the err with `!`
    through its own release path, and an export that propagates it rejects or
    throws `AlmideError`. Failing calls leave memory flat:
    `spec/wasm_host_js/hook_errors{,_async}.almd` pin it over 3,000 and 600
    rounds. Unwinding instead grew linear memory from 0.9 MB to 7.3 MB over
    the same 3,000 rounds.
  - An **infallible** extern (a plain `fn` returning `T`) cannot fail by its
    declaration, so a throw there is a trap. The glue rethrows an error that
    names the import and the fix (declare it `effect fn`), and *abandons* the
    instance. Every later call throws `… abandoned … call init() again`
    instead of running on a heap whose unwound frames kept their blocks.
    `init()` starts a fresh instance.
- A hook that returns a Promise is marked on its declaration,
  `@extern(wasm, "js", "name", returns: promise)` (#3353, #3371), and the
  glue suspends on it through JSPI. An UNMARKED hook that returns a thenable
  throws `almide: hooks.js.<name> returned a Promise; mark its @extern with
  returns: promise` and abandons the instance, for a fallible extern too.
  Design: [JS-HOST-ASYNC-IMPORTS.md](./JS-HOST-ASYNC-IMPORTS.md).
- `main` is `run()`; `_start` is not called by `init`.

String marshalling reads the block layout (`almide-layout`:
rc @0, len @4, cap @8, payload @12) and the module's own signatures for the
valtype of each slot (`Bool` is `i32` on the structural leg). A block the host builds goes through the module's exported
allocator (`__alloc`) and a block the host takes out is released through its
exported release (`__release`); both exports exist only under `--host js`
and only when some marshalled signature carries a `String` (#2276), so every
other build keeps its bytes — a scalar-only surface's module is
byte-identical to the build without the switch, and the glue then carries no
string helpers either. The structural leg also records which
exported params the callee owns, so the host releases exactly the credits
it still holds.

Gate: `scripts/check-js-host.sh` builds every `spec/wasm_host_js/*.almd`
with `--host js`, runs it under node, byte-compares stdout to
`<fixture>.expected`, runs the fixture's `<fixture>.host.mjs` (the `js`
hooks and an `after(module)` exercising the wrappers), and for a fixture
without `@extern` also compares the native binary's stdout. It also asserts
what ships (#2276): the module is byte-identical to the build without
`--host js` unless the surface marshals a `String` (then it differs by
exactly the two exports), the glue's `wasi.<name>` shim set equals the
shipped module's `wasi_snapshot_preview1` imports (pre- and post-`wasm-opt`),
and each glue's byte size is at or under its row in
`spec/wasm_host_js/glue-ceiling.txt` (shrinking is silent, growing is a
ledger edit). CI runs it in the `checks` job.

## Reproducing the measurements

```bash
# Almide — the structural leg, plain and -Oz (the incumbent columns cannot be reproduced since #2761)
printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > hello.almd
almide build hello.almd --target wasm -o hello.wasm                    # structural, as shipped
almide build hello.almd --target wasm --wasm-opt -o hello.min.wasm     # structural, -Oz
wasm-objdump -h hello.wasm                                             # the section tables above

# Rust (same target, full size profile — 2026-07-23 numbers)
cargo new rhello && cd rhello
# [profile.release] opt-level="z", lto=true, strip=true, panic="abort", codegen-units=1
cargo build --release --target wasm32-wasip1
```
