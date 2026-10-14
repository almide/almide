# ADR-0013: native String as a shared buffer — implementation

**Status:** stage 1 landed (2026-10-15); stage 2 next.
**Decision:** [ADR-0013](../../adr/0013-string-shared-immutable-buffer.md) (accepted 2026-09-04).
**Owner ruling 2026-10-15:** implement this right after the stdlib inspection's individual fixes. Do not change borrow inference for conditionally consumed parameters instead.

## Why now

The stdlib inspection measured `result.filter` at O(n) per call on native, and O(1) on wasm. The call passes a live `String` to a parameter only the failing branch consumes, and native renders `x.clone()` at the call site.

We ran the same shape in six languages (macOS arm64, µs per call at n = 20,000 → 320,000):

| Model | Per call | Grows with n |
|---|---|---|
| Rust, owned parameter + caller clone (Almide native today) | 0.22 → 4.25 | yes (19x) |
| Rust, borrowed + clone on the consuming branch | 0.000 → 0.000 | only on that branch |
| Rust `Rc<str>`; Swift; Go; Python; Node; Ruby | flat | no |

Only the owned-copy model pays for the argument's size. The wasm leg already shares (rc blocks). This is ADR-0013's finding, met again from a different direction.

## Stage 1: the ratchet (landed)

The falsifier: stage 2 is reverted if strchurn or fasta gets **5% or more** slower and the thread-shared variant does not recover it. That is only decidable with a measurement whose noise is well under 5%.

CI's `check-perf-ratio.sh` reports strchurn and decode without anchoring them, because their ratio depends on the architecture. So the falsifier is judged on one fixed machine instead:

- `research/benchmark/perf/gce_ab.sh <ref-a> <ref-b> <out>` builds both refs on one single-use GCE VM. It then runs `bench.py` (full workloads, native + rust legs) alternately A, B, A, B over the same bench sources.
- `research/benchmark/perf/ab_summary.py <out>/res` summarises the run. Per bench it gives the median native time, the median almide/rust ratio of each side, B against A, and the spread of the ratio over all rounds.

**Noise floor:** develop 0d93660ea as both A and B. GCE c4-standard-4 (Intel Xeon Platinum 8581C, 4 vCPU), Ubuntu 24.04, Linux 7.0.0-1014-gcp, rustc 1.99.0. 3 rounds × 5 runs, medians, 2026-10-15.

| Bench | Ratio to the handwritten Rust | B vs A (same build) | Spread over 6 rounds |
|---|---:|---:|---:|
| fasta | 1.164 | +0.07% | 1.18% |
| strchurn (same-shape reference) | 1.126 | −0.67% | 1.38% |
| decode | 0.783 | +2.03% | 3.11% |

- The falsifier rows (fasta, strchurn) repeat within about 1.4%, so a 5% regression is clearly visible.
- decode is noisier (about 3%). It is reported, not used as a falsifier, which matches the ADR.
- These are the stage 2 baseline numbers.

## Stage 2: the representation (next)

Inventory at develop 0d93660ea:

| Surface | Count |
|---|---:|
| `pub fn almide_rt_*` in `runtime/rs/src` | 809 |
| … returning `String` (or `Result<String, …>`) | 61 |
| … taking `String` by value | 17 |
| `Vec<String>` mentions in the runtime | 32 |
| codegen / MIR sites that spell the `String` type | 17 |

Open design points, to settle before writing code:

1. **The type.** `Rc<str>` copies a `String` into a new allocation on conversion. `Rc<String>` moves it, at the cost of one extra indirection. A Koka-style single block (count + bytes) avoids both, but has to be written by hand. Measure all three on strchurn / decode before choosing.
2. **The `mut` surface.** `string.push` / `string.clear` use RcCow (`Rc::make_mut` semantics): in place when unique, copy when shared. This is the same rule List / Map already follow.
3. **The fan boundary.** Use the Koka thread-shared flip, so only values captured into a scoped thread become atomic. The stage 2 ratchet is the judge: a strchurn regression from the flip stops the stage.
4. **The ABI.** Runtime functions keep taking `&str`, which works through `Deref`. Functions that return `String` return the shared type, converted once at their boundary.

Stage 3 (seamless slices for `split` / `slice` / `trim` / `lines`) follows stage 2. Stage 4 (inline small strings) is out of scope here.
