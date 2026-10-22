# Benchmarks

## WASM Binary Size

Almide emits WASM bytecode directly (no LLVM, no Cranelift). Each binary is self-contained — allocator, string handling, and runtime are all included. No external GC or host runtime dependency. Since the unverified v0 emitter was retired (#782), every wasm build comes from a verified renderer — the certified MIR spine, or since commissioning (#1599) the structural engine, whichever the router picks — and **the shipped binary is the exact module that renderer produced** (on the incumbent leg, the exact module the certificate was checked against): reachability DCE inside the renderer prunes unreached preamble helpers, imports, and data segments before assembly, and the debug-name section keeps only function names (for trap backtraces) — but no post-hoc optimizer touches the shipped bytes.

| Program | Verified, as shipped | After `wasm-opt -Oz --all-features` |
|---------|-----:|-----:|
| Hello World | **703 B** | **545 B** |
| FizzBuzz 1–100 | **1,793 B** | **1,092 B** |
| Fibonacci (recursive) | **1,441 B** | **771 B** |
| Closure + call_indirect | **2,744 B** | **1,672 B** |
| Variant (match + float) | **11,965 B** | **6,868 B** |

The "as shipped" column is raw `almide build --target wasm` output (measured 2026-07-23). Running `wasm-opt` is an explicit opt-in that leaves the verified envelope — it goes beyond the renderer's own reachability DCE with instruction-level rewrites (local coalescing, inlining, more aggressive dead-code removal). The float row is dominated by the self-hosted Dragon4 shortest-round-trip printer that `float.to_string` demand-links; programs that never display a Float never pay for it. Full dissection: [WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md).

## Native Performance

Almide compiles to Rust, which then compiles to native machine code. No runtime, no GC, no interpreter.

| Metric | Value |
|--------|-------|
| Binary size (`perf/native/cli_app.almd`, the minigit-shaped CLI) | **507,960 B** stripped on x86_64 Linux (GCE c4-standard-8), **463,720 B** on aarch64 Linux (GCE c4a-standard-8) — built 2026-10-20 by the v0.68.0-rc2 release binaries, the build `research/benchmark/perf/native/measure.sh` makes |
| Dependencies | **0** crates (only the platform C library is dynamically linked: `libc` and `libgcc_s` on Linux) |
| WASM target | `almide build app.almd --target wasm` |

### Runtime scoreboard

Benchmarks-Game-style programs, Almide `--release` vs handwritten Rust compiled
with the same flags (opt-level=3, LTO, 1 CGU, and the target CPU and LLVM
options the native `--release` build adds: `-C target-cpu=x86-64-v3` on x86_64,
the generic CPU on aarch64, as `almide build --print-release-rustflags` prints
them), median of 5 interleaved runs, byte-identical stdout verified across
every variant before timing. Measured 2026-10-20 with the v0.68.0-rc2 release
binaries (`almide 0.68.0 (release, fcba2bcd5)`, rustc 1.99.1 for both sides) on
GCE c4-standard-8 (x86, Intel Xeon Platinum 8581C, 8 vCPU) and c4a-standard-8
(arm, Neoverse-V2, 8 vCPU). Produced
by [research/benchmark/perf/bench.py](../../research/benchmark/perf/README.md)
(`--runs 5 --legs native,rust`); raw per-run data:
[x86](../../research/benchmark/perf/results/2026-10-20-gce-c4-standard-8.json),
[arm](../../research/benchmark/perf/results/2026-10-20-gce-c4a-standard-8.json).

| Benchmark (workload) | Almide native, x86 / arm | Handwritten Rust, x86 / arm | Ratio, x86 / arm |
|---|---:|---:|---:|
| n-body (50M steps) | **2.040s** / **2.766s** | 2.172s / 2.720s same-shape; 1.812s / 3.470s array-based | **0.94×** / 1.02× same-shape; 1.13× / **0.80×** array-based |
| spectral-norm (n=5500) | **0.966s** / **1.099s** | 0.969s / 1.107s | **1.00×** / **0.99×** |
| fasta (25M) | **3.176s** / **3.232s** | 2.748s / 2.934s | 1.16× / 1.10× |
| FFT (2^22) | **0.309s** / **0.346s** | 0.351s / 0.369s | **0.88×** / **0.94×** |
| wordfreq (8M draws) † | **0.215s** / **0.198s** | 0.293s / 0.338s | **0.73×** / **0.59×** |
| listbuild prealloc / append / combinator (2^23) | **0.269 / 0.276 / 0.270s** / **0.333 / 0.340 / 0.338s** | 0.307s / 0.358s | **0.87 / 0.89 / 0.87×** / **0.93 / 0.95 / 0.94×** |

† wordfreq is timed at its ratchet size (`bench.py --quick`, 8M draws): at the
suite's 2M the Almide row runs in ~0.05 s, under the 0.08 s spawn-noise floor
the ratchet refuses to judge on. Raw rows:
[x86](../../research/benchmark/perf/results/2026-10-20-gce-c4-standard-8-wordfreq-quick.json),
[arm](../../research/benchmark/perf/results/2026-10-20-gce-c4a-standard-8-wordfreq-quick.json).

The same-shape n-body reference (`nbody_unrolled.rs`) isolates codegen
overhead: Almide is 2% behind it on arm and 6% ahead on x86. The `perf-ratchet` CI job
([scripts/check-perf-ratio.sh](../../scripts/check-perf-ratio.sh)) gates these
ratios against a committed baseline so they can only move on purpose.

### Faster than ordinary Rust (#1330)

The declaration axis B2 asked for: named workloads where the almide-native /
handwritten-Rust ratio is **below 1.0**, CI-ratcheted, with the honesty
constraints stated up front.

**Methodology.** The Rust side is the *ordinary* program a competent person
writes for the workload — `rust-ref/binarytrees.rs` and `rust-ref/treealloc.rs`
are a `Box`-linked recursive enum with `make` / `check` by value,
`rust-ref/fannkuchredux.rs` is the sequential program; one thread, no arena,
no custom allocator, no `unsafe`, no SIMD — compiled with the same `rustc`
flags as Almide's emitted Rust (opt-level=3, LTO, 1 CGU, and the release
target CPU and LLVM options named in the scoreboard above). Both sides print
byte-identical stdout (verified before timing), every number is the median of
9 interleaved runs after a warmup, and each row is measured at two input sizes
so a win that exists at one size only would show. The ablation column is the
*same Almide source* rebuilt and run with the one optimization turned off
(`ALMIDE_REGION_OFF=1`, `ALMIDE_FAN_SEQUENTIAL=1`), timed in the same
interleaved loop, so the gap between the two Almide columns is that
optimization and nothing else. Measured 2026-10-20 with the v0.68.0-rc2
release binaries (rustc 1.99.1) on GCE c4-standard-8 (x86) and c4a-standard-8
(arm); raw rows in
[x86](../../research/benchmark/perf/results/2026-10-20-gce-c4-standard-8-victory.json) /
[arm](../../research/benchmark/perf/results/2026-10-20-gce-c4a-standard-8-victory.json).

| Workload | size | machine | Almide native | ordinary Rust | **ratio** | Almide, optimization off | ratio, off |
|---|---:|---|---:|---:|---:|---:|---:|
| binarytrees | d17 | x86 | **118 ms** | 417 ms | **0.28×** | 382 ms | 0.92× |
| binarytrees | d19 | x86 | **541 ms** | 2021 ms | **0.27×** | 1800 ms | 0.89× |
| binarytrees | d17 | arm | **150 ms** | 450 ms | **0.33×** | 471 ms | 1.05× |
| binarytrees | d19 | arm | **682 ms** | 2108 ms | **0.32×** | 2188 ms | 1.04× |
| treealloc | d20 | x86 | **170 ms** | 766 ms | **0.22×** | 630 ms | 0.82× |
| treealloc | d21 | x86 | **371 ms** | 1558 ms | **0.24×** | 1416 ms | 0.91× |
| treealloc | d20 | arm | **135 ms** | 498 ms | **0.27×** | 499 ms | 1.00× |
| treealloc | d21 | arm | **279 ms** | 980 ms | **0.28×** | 1007 ms | 1.03× |
| fannkuchredux | n10 | x86 | **46 ms** | 185 ms | **0.25×** | 182 ms | 0.98× |
| fannkuchredux | n11 | x86 | **502 ms** | 2182 ms | **0.23×** | 2116 ms | 0.97× |
| fannkuchredux | n10 | arm | **35 ms** | 168 ms | **0.21×** | 171 ms | 1.01× |
| fannkuchredux | n11 | arm | **376 ms** | 2034 ms | **0.18×** | 2074 ms | 1.02× |

**Which optimization did it.** binarytrees and treealloc spend their time in
`check(make(depth))` — a tree whose whole lifetime is one expression. The
effect system proves the pair region-pure (no effect, no global, no `mut`
param, a scalar result), so the native leg runs it over `__rgn_` twin fns in a
thread-local bump arena and rewinds once per tree (the region window, #1991);
the ordinary Rust pays a `malloc` per node in and a `free` per node out, and
cannot know it is allowed not to. With the window off, the same Almide source
reads 1.00–1.05× the reference on arm and 0.82–0.92× on x86 — the emitted
`Box` code is what the reference already is — and the window alone is worth
3.1–3.8× on both machines. fannkuchredux's `fan { list.map }` over Int
captures is the Send-safe-scalar subset the native leg runs on a thread per
core (#2044); with `ALMIDE_FAN_SEQUENTIAL=1` it reads 0.97–1.02× the
one-thread reference, so the whole win is the parallel map, and it scales with
the core count. The win holds at both sizes of every row on both machines.

**What the numbers are not.** The absolute ratio is machine- and
allocator-dependent (the region rows read lower on x86, the fan row lower on
arm). The direction is machine-stable, which is what the ratchet gates: the
VICTORY rows of
[scripts/check-perf-ratio.sh](../../scripts/check-perf-ratio.sh) fail the
build if a row reaches 1.0, if it regresses past +40% of the committed
runner-class baseline, or if the ablation stops costing the row at least 1.3×
(the declaration would then be attributing the win to the wrong thing). Two
mechanisms, three workloads: this is stated rather than dressed up — the
second region row exists to show the window is a property of the shape
`consume(produce(scalars))`, not of one benchmark.

**The other candidate the issue named, measured and not claimed.** Same run,
ordinary sequential Rust reference (`rust-ref/mandelbrot.rs`, byte-identical
output):

| Workload | size | Almide native, x86 / arm | ordinary Rust (one thread), x86 / arm | ratio, x86 / arm |
|---|---:|---:|---:|---:|
| mandelbrot (`fan.map`) | 1000 | 49 ms / 63 ms | 50 ms / 63 ms | 0.97× / 1.00× |
| mandelbrot (`fan.map`) | 4000 | 721 ms / 985 ms | 740 ms / 985 ms | 0.98× / 1.00× |

- *Deterministic data parallelism (`fan`)* gives mandelbrot nothing on the
  native leg: its `fan.map` callback returns `Bytes`, whose native repr is an
  `Rc`, so #2044's Send-safe-scalar routing declines it and the map runs
  sequentially (`ALMIDE_FAN_SEQUENTIAL=1` reads the same time). A
  single-thread reference is therefore the honest one, and the row sits at
  parity. The ratchet REPORTS it so that a real parallel win becomes a number
  the day it lands.
- *Stream fusion of `|>` chains* does not fire on the Rust target
  (`IterChain` never lowers there; see the perf suite's "Not yet covered"),
  and ordinary Rust iterator chains are already fused by LLVM, so there is no
  fusion win to claim; the listbuild rows are in the scoreboard above.

### Allocation: the region window on the native leg (2026-09-08, #1991)

`binarytrees` is the allocation row: `check(make(depth))` builds a tree whose
whole lifetime is that one expression, and the emitted Rust used to be exactly
what a person writes — a `Box` per node in, a free per node out. That program
and a hand-written Rust `Box` program measured the same, and both lost to an
arena by 2.6× (issue #1991: 228 ms vs Zig's 88 ms, d16, arm64). The structural
wasm leg had already closed that gap with a region window (#1961/#1980); the
native leg now has the same recogniser (`RegionWindowPass`): the pair runs over
`__rgn_` twin fns whose recursive fields are handles into a thread-local bump
arena in the prelude, and the site rewinds the arena once instead of freeing
per node. Same box, same session, interleaved, median of 9 (arg 16 — the
issue's workload; arg 18 in brackets):

| binarytrees | before | after | |
|---|---:|---:|---|
| Almide native | 230 ms [1027 ms] | **72 ms** [309 ms] | 3.2× faster than itself |
| Rust, same-shape `Box` (`rust-ref/binarytrees.rs`) | 222 ms | 222 ms | Almide now 0.33× |
| Zig 0.16 arena (issue #1991, same shape) | 88 ms | 88 ms | Almide 0.82× — inside the 1.3× acceptance |

The row is a VICTORY row of the ratchet (`binarytrees=rust:binarytrees`,
quick arg 17; see "Faster than ordinary Rust" above), not a ±band-anchored
one: the same commit reads 0.31 on the M4 Pro and 0.61 on the ubuntu-latest
runner, because what differs between the two machines is how cheaply the
*reference* frees a `Box` (glibc malloc vs macOS), not the window — the same
allocator-dependence that keeps the listbuild rows reported. The window's own
A/B is `ALMIDE_REGION_OFF=1` on the same source and machine (3.5× here), and
that A/B is what the ratchet gates. What fires: every
`consume(produce(scalars))` site whose pair is region-pure and whose produced
type is a root variant enum with scalar / region-enum payloads. What does not:
a held tree (`let t = make(d)` read twice keeps its `Box`), a consumer that
returns the tree's shape as a `String`, anything reaching a global, a `mut`
param, a lambda or a non-scalar stdlib module. Output is byte-identical across
native, wasm and the reference on every shape in
`spec/lang/region_window_test.almd`.

### Build shape: what the recommended idiom costs (2026-08-13)

`listbuild` is one workload written three ways — the same 2^23-element
interleaved `Float` array, identical arithmetic, identical checksum consumer,
differing only in the build loop. It exists because the shape the idiom docs
recommend was the slowest of the three, which is an MSR problem before it is a
perf problem: the idiom docs are the in-context material that steers generated
code, so "recommended" and "fast" have to name the same shape.

| Build shape (2^23) | Almide native | Handwritten Rust | Ratio |
|---|---:|---:|---:|
| preallocate + indexed write | **0.166s** | 0.105s | 1.57× |
| `var` + `for` + `data = data + [x]` | **0.176s** | 0.110s | 1.60× |
| `list.range \|> list.flat_map` (recommended) | **0.172s** | 0.105s | 1.64× |

Median of 15 interleaved runs; all three shapes produce byte-identical output,
checked before timing. The three ratios are within run-to-run noise of each
other on this box — the point of the table is that the spread is gone.

**Read the ratio column as machine-specific, not as a property of the
language.** Unlike the arithmetic kernels above, this workload's cost is
allocation, and the two allocators disagree: the same commit measured on the
ubuntu-latest CI runner reads **0.91×** on the preallocated row — Almide
*beating* the reference, because glibc `calloc` hands back zero pages that
`Vec::with_capacity` + push has to fault in — against 1.57× here. What is
stable across both machines, and what CI therefore gates, is the *relation*
between the three shapes (1.018× here, 1.045× on the runner), not any row's
absolute ratio.

It was not. The A/B at 2^22 (median of 11, same run):

| Build shape (2^22) | Before | After |
|---|---:|---:|
| preallocate + indexed write | 0.084s | 0.083s |
| `var` + `for` + append | 0.084s | 0.083s |
| `list.range \|> list.flat_map` | **0.140s** | **0.085s** |

`flat_map`'s lambda was forced by its runtime signature to heap-allocate its
intermediate list on every element — 44 ms of a 79 ms build, the whole of the
gap. It now returns a fixed-size array instead, and the recommended idiom went
from **1.67× the append loop it replaces to 1.02×**.

The ~1.6× all three shapes share against handwritten Rust was long attributed
to materialization (#1004). It is **not** (measured 2026-08-13): swapping only
`almide_rt_libm_sin`/`_cos` — the deterministic software libm the cross-target
byte-identity contract requires — for the platform ones in the emitted Rust
takes the row from 194.5 ms to 105.5 ms against a 123.9 ms reference. The
whole 1.6× is transcendental determinism, and without it the emitted code
*beats* the handwritten reference by 1.17×. The build shape is innocent:
`list.repeat` + bounds-checked indexed writes measure 75.5 ms against
`Vec::with_capacity` + push at 90.6 ms. Evidence and method:
[string-gap-1004.md](../../research/benchmark/perf/string-gap-1004.md).
`check-perf-ratio.sh` reports these rows and gates the relation between them,
so the idiom cannot silently become the slow path again.

### Strings: what the stdlib's contract costs (2026-08-13)

`strchurn` is the allocation-heavy string workload from #1004:
`int.to_string` → `string.join` → `string.split` → `string.len` → `list.sum`.
It carries two references, and which one you compare against is the whole
story (`bench.py --quick`, 2M, median of 3, M4 Pro):

| strchurn (2M) | Time | Ratio |
|---|---:|---:|
| Almide native | **0.123s** | — |
| Rust, same shape + same semantics (owned `String`s from `split`, `chars().count()`) | 0.109s | **1.12×** |
| Rust, idiomatic (borrowed `&str`, byte `len()`) | 0.059s | 2.08× |

The second reference does what a Rust programmer writes; the first does what
Almide's stdlib obliges the program to do. Almide is within **1.12×** of
same-work Rust, and the remaining spread to idiomatic Rust is **the API
contract, not codegen** — 75% of it is `string.split` returning owned
`String`s (`List[String]` has no borrowed element type) and 11% is `string.len`
being a character count. The `RcCow` representation the issue title blamed is
not on this path at all: Almide's `String` lowers to Rust's `String`, and
`RcCow` is used only for `Bytes` and `Matrix`. Full ladder, the refuted
rlib-boundary hypothesis, and the resulting work list:
[string-gap-1004.md](../../research/benchmark/perf/string-gap-1004.md).
Like the listbuild rows, this one is reported rather than anchored — an
allocation-dominated ratio is an allocator comparison first.

### Codec decode: what the derived `T.decode` costs (2026-09-08, #1679)

`decode` is the #1673/#1679 workload: one fixed 8-field `User` document (a
nested `Address`, a 3-element `List[String]`), parsed once, then N derived
`User.decode`s whose records feed an accumulator. The reference
(`rust-ref/decode.rs`) is the ordinary hand-written decode against the SAME
`AlmideValue` shape — a borrowed linear field scan into owned `String` record
fields, the issue's 168 ns row — not the borrowed-`&str` record (53 ns) the
language cannot express. Same box, same session, interleaved, median of 7,
2M decodes, arm64; process floor subtracted:

| decode (per op) | ns/op | |
|---|---:|---|
| Almide native, field scan (before the slot hint) | 422 | — |
| Almide native, slot-indexed lookup (`_field_ref_at`, this change) | **412** | 2.0× the reference |
| Almide native, the same emit with the `___erratw_*` frames collapsed | 205 | level with the reference |
| Rust, same shape (`rust-ref/decode.rs`) | 205 | — |
| zod 4.5.2 `safeParse` / `z.compile`, same schema (issue #1679, same box) | 78 / 17 | context, not the same data model |

The slot hint is the issue's "shape specialization" step: a derived decode
knows each field's declaration index — the slot `T.encode` writes it to — and
`almide_rt_value_field_ref_at` tries that slot before falling back to the scan
(`spec/wasm_cross/codec_decode_field_order.almd` drives the reordered, shifted,
sparse and missing-key fallbacks on both legs). It buys 10 ns on an 8-field
record because the scan was never the cost: the row's whole 2× is the #1675
error-path frames (`T___erratw_<field>`) that clone the `Ok` payload and
allocate the path segment `"name".to_string()` on the SUCCESS path — eleven
of them per decode, ~200 ns. Collapsing them in the emitted Rust is the third
row, and it reads exactly the reference. That frame is the row's next move;
the representation half (owned `String` fields vs a shared string) stays the
ADR-level decision #1679 records. The row joins the ratchet's REPORTED rows
(`decode=rust:decode`, quick arg 1M): eight short-string allocations per op on
both sides make it an allocator reading first, like strchurn. The ratchet's own
reading of the same build the day the row landed was 2.46 (whole-process wall
clock, no floor subtracted, alongside other work on the box); the rlib-linked
release build and the monolithic one measure the same, so the spread to the
2.0 above is load, not linking. The wasm leg is off the row until #2046
closes: a derived decode in a loop retains ~800 B per call there (786 MB
peak at 1M, out of memory at the 5M timing arg), so the leg would time the
leak. Its output is byte-identical at small N.

The wasm leg is measured in the same dated results file: within 1.1–1.2× of
native on the compute kernels (n-body 1.278s, spectral-norm 0.764s,
fannkuch-redux 1.892s) and *faster* than native on binary-trees (0.239s vs
0.835s), but it craters on hot list index writes (FFT: ~3,500× at 2^18) and on
mandelbrot (~130×) — those two cliffs are the current wasm perf arc, tracked in
#917's follow-up.

### Map: what the native keyed lookup costs (2026-09-14, #2150, #2157)

`wordfreq` is the keyed-aggregation row: 2M draws from a 5 000-word vocabulary
counted in a `Map[String, Int]`, top 10 by count desc / word asc — the shape
of every word-count, group-by and histogram program. #2157 found the native
leg 1.5–2.3× SLOWER than the same program under wasmtime, and its probe put
the whole native gap on the Map: the same loop with the Map removed ran in
87 ms, with it in 180 ms (`almide bench`, median of 5–7, M4 Pro, a loaded
box). Three changes, one row (`research/benchmark/perf/wordfreq/`, both
spellings, against `rust-ref/wordfreq.rs` — a `HashMap<String, i64>` with an
owned key per draw):

| wordfreq, imperative (2M) | before | after | |
|---|---:|---:|---|
| Almide native | 180 ms | **142 ms** | |
| … of which the Map (loop minus the Map-free probe) | 93 ms | **55 ms** | 46 ns → 27 ns per `get_or` + `m[w] = …` pair |
| Almide wasm (structural leg) | 127 ms | 130 ms | its Map: ~73 ms over its 57 ms probe |
| Rust, same shape (`rust-ref/wordfreq.rs`) | 80 ms | 80 ms | |

1. **The index is a compact-ordered-dict** (CPython 3.6+, Roc's `Dict`):
   the insertion-ordered entry vector stays the source of truth for order,
   equality and repr, and beside it sits an open-addressing slot table of
   entry POSITIONS with the full 64-bit hash cached per entry, one
   multiply-fold hash per operation. The previous sidecar SipHashed the key
   through `dyn Any`, then SipHashed the fingerprint again inside a
   `HashMap<u64, u32>` — four SipHash passes per `get_or` + insert pair.
   Threshold (16 entries), insertion order, order-independent equality, the
   Float/NaN linear path: all unchanged and pinned by the 77 Map/Set fixtures
   in `spec/wasm_cross`, byte-identical native == wasm.
2. **The read side borrows its key.** `map.get` / `get_or` / `contains` and
   `set.contains` are `@borrow_ref(key)`; the runtime takes `&Q` with
   `K: Borrow<Q>`, so a `&str` parameter probes a `Map[String, _]` without
   materializing a `String` and `map.get_or(counts, w, 0)` no longer
   consumes `w`.
3. **The write moves the key.** `m[w] = f(&w)` with a plain-variable key
   evaluates the value FIRST (a var read has no effect, so the order is
   unobservable and stays byte-identical with the wasm leg's key-first
   ANF), which makes the key position the var's last use: the clone pass
   moves `w` into the insert instead of cloning it. `tests/map_key_move_test.rs`
   pins both the borrow and the move in the emitted Rust.

Measured in isolation on the emitted Rust (bare `rustc`, same source): the
old shape 152 ms, the borrowed lookup alone 128 ms, a single-lookup
get-or-insert 110 ms — the key clone and the second lookup each cost more
than the hashing did. The native Map is now cheaper than the structural wasm
leg's (55 vs ~73 ms for the same 2M operations); what keeps the native row
above the wasm row is the Map-FREE loop (87 vs 57 ms: the `let w = vocab[i]`
read clones twice, `list.range` is a real 16 MB `Vec`), which is codegen and
is #2157's open probe, not the Map. The recommended spelling
(`wordfreq-group`: `list.group_by` + `map.map`) reads 229 ms natively: its
cost is the per-element key clone `group_by`'s `Rc<dyn Fn(A) -> B>` callback
forces plus the intermediate `List[String]`, not the lookups — the T5
relation this row now watches. `mapbuild` (20k Int + 20k String keys, build
then read back) reads 5.6 ms from 6.6. Both wordfreq rows joined the ratchet's
REPORTED rows (`wordfreq=rust:wordfreq wordfreq-group=rust:wordfreq`, quick
arg 1M): a hash-map row compares an allocator and a hasher before it
compares codegen, like strchurn. Since #2150 `wordfreq` is ANCHORED in the
gated PAIRS (baseline 1.90 at a 4M quick arg): the runner read 1.66-1.75 on
five green develop runs at 2M against 1.77 on an M4 Pro, so the ratio travels
between the two machine classes where listbuild's does not. `wordfreq-group`
stays reported; its relation to `wordfreq` is the gated wasm idiom row.

### Ablation: what the IR optimizer buys (2026-08-18)

Koka's benchmark methodology publishes ablation legs (`std` = no FIP reuse
optimization) so a performance claim is about the *optimizer*, not just the
language — `#1466` adopts the same axis. `ALMIDE_DISABLE_OPT=1` skips the IR
perf passes (constant fold, DCE, constant propagation; the lowering enablers
and the unsigned-lane correctness re-fold stay), and the perf gate runs every
anchored benchmark both ways each CI round. The delta (ablated / optimized)
is anchored in `scripts/perf-ratio-baseline.txt` with a floor at 0.90 and the
standard re-anchor-on-purpose ceiling.

| bench | ablated / optimized |
|---|---:|
| nbody | 1.00 |
| spectralnorm | 1.00 |
| fasta | 1.00 |
| fft | 1.00 |

**The measured finding, stated plainly:** on the rustc-backed native leg the
IR perf passes buy ~0% on these benchmarks — LLVM runs the same folds
downstream and subsumes them. That is the honest ablation answer today, and
it is exactly what the axis is FOR: a pass that earns will show up as the
delta leaving 1.00 (and gets re-anchored, visibly); a pass that starts
*costing* trips the 0.90 floor. The passes' present value is compile-time
shape and the wasm leg (which has no LLVM behind it — ablating there costs a
file its lowering, not its speed; see `optimize_program`'s header note).

## Edit-loop scale (#1334)

The build-speed row on the [measurements page](../benchmarks/README.md) is measured on a 268-line file, which does not establish
that the edit loop is *scale-independent* — a fast check on a small file is also exactly
what a quadratic compiler produces. So the same command is measured over a ladder of
nested prefixes of this repo's own stdlib (285 hand-written modules, 31,729 lines;
nothing synthesized), 40 interleaved runs per rung:

| project size | `almide check` p50, x86 / arm | p95, x86 / arm | µs/line, x86 / arm |
|---:|---:|---:|---:|
| 2,480 lines | 37.5 / 41.4 ms | 38.4 / 42.1 ms | 5.50 / 6.35 |
| 5,577 lines | 56.3 / 63.7 ms | 58.3 / 64.3 ms | 5.81 / 6.74 |
| **10,669 lines** | **97.4 / 109.6 ms** | **99.9 / 110.6 ms** | 6.75 / 7.75 |
| 21,080 lines | 175.3 / 195.5 ms | 178.5 / 197.3 ms | 6.87 / 7.96 |
| 32,666 lines | 260.2 / 295.3 ms | 268.3 / 297.7 ms | 7.03 / 8.12 |

Measured 2026-10-20 with the v0.68.0-rc2 release binaries
(`scale.py --runs 40`) on GCE c4-standard-8 (x86) and c4a-standard-8 (arm). **The
p95 column is an observation, not a promise**: on a loaded machine the tail
tracks the load, not the compiler, so the CI ratchet
(`scripts/check-edit-loop-scale.sh`) anchors two dimensionless same-run ratios instead:
the log-log slope of marginal check time against project lines (**1.10** on both
machines, where 1.0 is linear and 2.0 quadratic) and the cost of the
10k-line rung in units of the empty-project floor (**4.2×** x86 / **4.3×** arm). Reproduce with
`python3 research/benchmark/editloop/scale.py`.

## AI Coding Language Benchmark

Based on [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (MiniGit implementation task: v1 implement, v2 extend).

### Same-model snapshot (2026-07)

Five languages, one model (Claude Sonnet 5), 20 trials each, identical prompts and harness — Almide vs its modern peer group (Gleam, MoonBit) plus mainstream anchors (Rust, TypeScript):

![Same-model snapshot](../figures/lang-bench-snapshot-2026-07.png?v=1784109014)

> Almide is the only language absent from training data (the model learns it in-context from CHEATSHEET.md), yet it passes 40/40 phases, produces the most concise code of all five languages (233 LOC), and completes faster than both modern peers. Methodology, retry policy, and raw per-trial records: [research/benchmark/lang-bench](../../research/benchmark/lang-bench/README.md).

### Historic comparison vs 15 established languages

![Execution Time](../figures/lang-bench-time.png?v=1784109014)
![Code Size](../figures/lang-bench-loc.png?v=1784109014)
![Pass Rate](../figures/lang-bench-pass-rate.png?v=1784109014)

> The Almide row was refreshed 2026-07-15 (Sonnet 5, 20 trials, from the snapshot above); the other 15 languages use the upstream Opus 4.6 runs. Almide achieves 100% pass rate with fewer lines of code than most languages, despite needing more time because the model has no prior training data for the language.

## MSR — Modification Survival Rate

The language's core metric, measured daily by [almide-dojo](https://github.com/almide/almide-dojo) across 30 tasks (basic / intermediate / advanced). The headline scorecard lives on the [measurements page](../benchmarks/README.md#llm-writability).
