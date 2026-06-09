# almide-kernel

Per-target SIMD numeric kernels for Almide — **fast AND proven**. Rust ports of
Wyve-proven kernels: explicit SIMD where the autovectorizer is structurally
blocked, verified statically where the structure allows.

## At a glance

```
Fast (4-quadrant — beats Rust AND Almide, on native AND wasm):
  transpose 8x8      native 5.18× / wasm 3.66×   (vs Rust)
  q1_0 block dot     native 3.73× / wasm 2.52×
  linear_q1_0 matmul native 6.74× / wasm 3.03×   (real inference hot path)
  scale              ties autovec → ships naive (no ceremony)

Proven (nothing left to "promised"):
  permutation (transpose) → static, all inputs   (index run = total proof)
  selection (q1_0 signs)  → static, exhaustive   (256 bytes = all bit patterns)
  reduction (q1_0 sum)    → bitwise to a named order + bounded error (n·u·Σ|x|)

Per-target: x86 AVX/AVX2 · wasm simd128 · naive fallback (one source, cargo --target)
Standalone: zero deps, 16 tests green, builds native + wasm
```

This is the Exo lineage (algorithm/schedule separation + correct-by-construction)
done in Rust, Almide-native — and for permutation/selection kernels, *lighter
than Exo* (no SMT solver; the structure is decidable).

## The form

```
Wyve (Racket/Lean)      = research lab : design a kernel, prove it bitwise-exact
                                          and in-range (@bounds), in Lean
   ↓ port the proven kernel
almide-kernel (Rust)    = factory      : production SIMD (core::arch),
                                          Almide-native, zero dependencies
   ↑ pinned by
differential test       = bridge       : SIMD == the same naive reference Wyve
                                          proves against → the proof carries over
```

Wyve isn't linked in (no Racket/wyvec/LLVM-clang build dependency). It's the
place kernels are *proven*; this crate is where they're *shipped*. A new kernel:
design + prove in Wyve → port to Rust here → differential-test bitwise-exact.

## Kernels

| kernel | impl | correctness | speed |
|---|---|---|---|
| `transpose_8x8` | AVX 3-pass shuffle network + naive fallback | bitwise-exact, 100 patterns | **1.57× default / 4.23× native** |
| `scale` | naive (autovec); `scale_avx` measurement only | bitwise-exact (avx == naive) | **0.99× native** → naive kept |
| `q1_0_dot` | AVX2 bit-unpack + signed sum + reduce | within-tolerance (reassoc reduction) | **3.5× (default AND native)** |

### Beating Rust — where, and the right correctness bar

```
transpose (shuffle)   : 4.23× native, bitwise-exact   — autovec can't shuffle
q1_0 (bit-unpack)     : 3.5× any target, reassoc       — autovec can't bit-address
scale (elementwise)   : 1.0× native, bitwise-exact     — autovec already wins → naive
```

The wins are exactly where rustc's autovectorizer is *structurally* blocked:
shuffle networks and packed-bit addressing. q1_0 beats Rust 3.5× regardless of
target-cpu — there's nothing for autovec to do, so the baseline never catches up.

Correctness bar depends on the op: **data-movement (transpose) → bitwise-exact**;
**reductions (q1_0) → within-tolerance**, because the SIMD schedule reassociates
the float sum (the error bar is the *sum of term magnitudes*, not |result| — the
result can cancel to near zero). Picking the right bar per op is the discipline.

### Static equivalence — Exo's idea, lighter for permutations

Exo proves a schedule equivalent to its algorithm *statically* (no execution):
it computes each statement's read/write **effect** and uses an **SMT solver** to
prove the transformed and original programs commute. almide-kernel borrows this —
and for **permutation kernels** (transpose, shuffles) does it *without a solver*:

```
A permutation moves positions, never values. So running it on the index array
input[k]=k extracts the ENTIRE permutation in one shot. If that equals the
transpose spec, the SIMD schedule is correct for EVERY input — a total proof,
not 100 samples. (f32/f64 hold 0..N exactly, so the source index is recovered
losslessly.)
```

`schedule_is_the_transpose_permutation_for_all_inputs` (f32 8x8, f64 8x8, and
arbitrary sizes incl. ragged) replaces sampled differential tests with a static
all-inputs guarantee. Where Exo needs an SMT solver, a permutation is decidable
in one run.

**Reductions split into two layers.** q1_0 isn't a permutation, but its sign
*placement* is — so we prove that part too. The bit-unpack (`apply_sign`) is an
XOR of the sign bit, value-independent, and one byte covers 8 lanes, so **256
byte values exhaust all bit patterns**: `avx2_bit_unpack_total_proof` proves the
sign goes to the right lane for *every* byte and every input — a finite, total
proof, no solver. And the float *sum* — instead of leaving it at "within-tolerance" — is proven in
two layers:

```
layer 1 (order is the spec) : the SIMD reduction is BITWISE-EXACT to a specified
                              tree-order reduction (8 lanes → lo+hi → hadd → hadd).
                              avx2_is_bitwise_exact_to_tree_order, 500 seeds, no tolerance.
layer 2 (the error is bounded): tree-order vs the idealized exact sum is within
                              n·u·Σ|xₖ| (n=128, u=2⁻²⁴) — a reassociation-error
                              theorem (Lean-provable; the test witnesses it holds).
```

Floats are order-dependent, so "the sum" was never one thing — once the order is
named as the spec, the kernel implements it *exactly* (layer 1), and the distance
to the ideal is *bounded, not hoped* (layer 2). That's proven, not promised, for
a float reduction.

```
q1_0 correctness:  sign placement → PROVEN (256 exhaustive)
                   reduction      → PROVEN (bitwise to tree-order + bounded error)
```

So almide-kernel's correctness map, end to end:
- permutations (transpose)        → static, all inputs (one index run)
- selections/bit-unpack (q1_0 signs) → static, exhaustive (256 bytes)
- float reductions (q1_0 sum)     → bitwise to a specified order + bounded error
Nothing is left to "promised."

### 4-quadrant: beat Rust AND Almide, on native AND wasm (q1_0)

|               | native (AVX2) | wasm (simd128) |
|---------------|---------------|----------------|
| vs Rust naive | **3.73×**     | **2.52×**      |
| vs Almide     | **3.62×**     | **2.43×**      |

**transpose** (8x8, bitwise-exact both targets):

|               | native (AVX) | wasm (simd128) |
|---------------|--------------|----------------|
| vs Rust naive | **5.18×**    | **3.66×**      |
| vs Almide     | **2.70×**    | **3.50×**      |

almide-kernel carries **per-target SIMD** (x86 AVX/AVX2, wasm simd128) + naive
fallback. For q1_0 both Rust's autovec and Almide's own dot are scalar (Almide's
SIMD is NEON-only; the packed-bit addressing defeats the autovectorizer); for
transpose the autovectorizer can't build a shuffle network at all. So
almide-kernel wins all four cells of both kernels — faster than Rust *and* than
Almide, on native *and* wasm. (Almide is f64, almide-kernel f32 — choosing the
right width for the work is part of the edge.)

**Full quantized linear** `linear_q1_0` — the inference hot path, `x @ Wᵀ` with
W in Q1_0, measured at 1×2048×2048 (one token through a 2048→2048 layer):

|               | native (AVX2) | wasm (simd128) |
|---------------|---------------|----------------|
| vs Rust naive | **6.74×**     | **3.03×**      |

(max relative error 6.7e-8 / 8.9e-8 — within f32 reassoc tolerance.) The matmul
opens *wider* than the single block dot (3.5×→6.74×): the per-block SIMD schedule
amortizes across the whole matrix. This is the real workload, not a microbench.

**Arbitrary-size transpose** `transpose_matrix` — what `almide_rt` actually needs
(Almide's matrices aren't 8x8). Full 8x8 tiles go through the SIMD kernel, ragged
edges scalar; bitwise-exact at any size. Measured at 512×512:

|               | native (AVX) | wasm (simd128) |
|---------------|--------------|----------------|
| vs Rust naive | **3.50×**    | **3.15×**      |
| vs Almide f64 | **3.13×**    | **3.63×**      |

This is the core of the `almide_rt` wiring: tiling an arbitrary matrix onto the
fixed-size SIMD kernel.

### The ABI shape decides the SIMD win (measured)

`bridge.rs` is the full wiring (`Vec<Vec<f64>>` → kernel → `Vec<Vec<f64>>`),
bitwise-exact. But measured at 512×512, the **same kernel** gives:

|                          | speedup vs Almide naive |
|--------------------------|-------------------------|
| nested `Vec<Vec<f64>>`   | **0.44×** (slower!)     |
| flat `Vec<f64>`          | **3.07×**               |

A 7× swing from the ABI alone. The nested→flat / flat→nested conversions (two
full copies + per-row Vec allocs) cost more than the SIMD transpose saves. So
the production lesson isn't about the kernel — it's about Almide's **type**:
hand the kernel a **flat** buffer (SmallF32 / flat f64) and the 3× lands; hand it
`Vec<Vec<f64>>` and the conversion eats the win. Wiring pays off iff the matrix
is flat.

What remains for full production wiring: Almide adopting a flat matrix ABI on the
hot paths, and Almide's build flow (`almide_rt` needs prelude injection — it only
compiles inside `almide run`/`build`).

The AVX transpose mirrors Wyve's verified 3-pass shuffle network (unpack
adjacent rows → shuffle 64-bit groups → permute 128-bit lanes).

### The design rule, confirmed by measurement

```
data-movement (transpose) : autovec can't build a shuffle network → explicit
                            SIMD wins regardless of target-cpu (4.23× native)
elementwise (scale)       : autovec ties explicit AVX at target-cpu=native
                            (0.99×) → ship naive, no ceremony
```

`scale`'s 1.23× on a *default* build was a mirage: autovec was capped at the
SSE2 baseline. At `target-cpu=native` it's a tie. So almide-kernel only writes
explicit SIMD where the autovectorizer is *structurally* unable to follow —
data movement. Everything else stays naive. (The fix for elementwise speed is
Almide's build flags, not ceremony in this crate.)

## Exo-style: algorithm / schedule separation

The transpose kernel separates **algorithm** (what) from **schedule** (how),
the way Exo does — but inside this crate, in plain Rust:

```rust
// algorithm: the spec
fn transpose_8x8_naive(..)  { out[j][i] = in[i][j] }

// schedule: named passes, composed — THIS line is the schedule
store(pass_permute(pass_shuffle(pass_unpack(load(input)))))
//    └─ recompose the passes to change how it runs;
//       the algorithm above is untouched.
```

| Exo | almide-kernel |
|---|---|
| algorithm + schedule separated | same — naive spec + composed passes |
| program-equivalence via effect analysis (static) | differential test bitwise-exact (dynamic, everyday) + Lean (optional stronger backstop) |
| own backend | Rust = LLVM, Almide-native |
| human writes the schedule | human writes it now → **Almide derives it later** (the one place no one else stands) |

Measured: splitting into named passes kept it bitwise-exact and 4.19x (vs 4.23x
as one blob) — `#[inline(always)]` folds the composition away. The schedule is
explicit and recomposable at **zero** runtime cost.

## Why a separate crate

- **No dependencies, no prelude injection** — builds and tests standalone.
  (`almide_rt` can't: its src assumes Almide's prelude injects `use` items, so
  it only compiles inside the full pipeline. This crate is plain Rust.)
- `almide_rt` calls it for **data-movement ops** (transpose, shuffles) where
  explicit SIMD beats the autovectorizer even on native — the empirically
  confirmed home of Wyve's edge.

## Bench honesty

The first two bench attempts were traps, worth recording:
1. Using only `out[1]` let rustc DCE the naive transpose to a single copy
   (naive looked 7× *faster* — it wasn't transposing).
2. Summing all 64 outputs let the `sum` dominate, burying the transpose
   difference (1.03×).

`black_box(&o)` with a per-iteration changing input forces the full transpose
to be computed and stored, measuring the shuffle network alone: **1.57×**.
Always check a microbench isn't measuring DCE or the wrong hot loop.

## Run

```
cargo test                              # naive-is-transpose + SIMD==naive bitwise
cargo run --release --example bench     # AVX vs naive
```
