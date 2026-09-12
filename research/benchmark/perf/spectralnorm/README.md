# Spectralnorm spelling comparison (#2098)

Run from the repository root after building the compiler:

```sh
ctxgate exec python3 research/benchmark/perf/spectralnorm/compare.py --n 1200 --runs 9
```

The existing imperative program is compared with indexed-fold and
enumerate-fold variants. Only the two matrix-vector multiplication functions
differ. All six native/wasm artifacts must return identical stdout and stderr.
Each receives one warmup followed by round-robin samples; JSON includes every
sample, median, ratio to the imperative form, and artifact size. Measurements
include process startup and, for wasm, runtime compilation. Small inputs are
smoke tests, not evidence about steady-state performance.

Use `--max-ratio R` to fail when any ratio exceeds a specified budget.
`scripts/check-spelling-ratio.sh` is the CI form of exactly that (2.5x, the
perf-ratchet job): the ratio compares spellings on ONE leg in one round-robin
run, so a slow shared runner moves the numbers and not the verdict — the same
discipline the native/Rust ratio gate follows. Lower the budget when the native
capture copy lands; never raise it to clear a red build. This
corpus baseline uses `for` and growing output lists; it is not the preallocated
`while` baseline in the issue's external experiment, so its ratios must not be
reported as reproducing those exact measurements. The variants intentionally
retain the same iteration and floating-point accumulation order.

Synchronous scalar folds borrow captures proven to be read-only through
indexing, explicit borrowing or cloning, with no escaping nested closure.
Scalar enumerate/fold on the wasm leg snapshots the source into a scalar list
and reuses one private tuple slot when the callback cannot expose that tuple.
On the native leg `list.enumerate` in SOURCE position is now a chain adapter
(`.iter().cloned().enumerate()`) rather than a `Vec<(Int, T)>` the next stage
walks and drops; the clone that fed the consumed runtime call goes with it.

`--release` builds with opt-level 3 and LTO; without it `almide build` uses the
project's default profile, which is opt-level 1 (load-bearing for correctness —
see `src/cli/cargo_build.rs`). The two profiles answer different questions and
both are recorded below: the default profile is what a plain `almide build`
ships, and the release profile isolates the lowering from what LLVM would have
cleaned up anyway.

Measured on 2026-09-13 with the local v0.62.0 release build and Wasmtime
47.0.3, n=1200, five samples after warmup. Every run printed `1.274224150`
plus a newline with empty stderr, on all six artifacts.

Default profile (`almide build`, opt-level 1):

| Target | Imperative | Indexed fold | Enumerate fold |
| --- | ---: | ---: | ---: |
| Native | 41.09 ms | 58.29 ms (1.42x) | 41.66 ms (1.01x) |
| Wasm | 93.20 ms | 118.51 ms (1.27x) | 96.12 ms (1.03x) |

Release profile (`almide build --release`, opt-level 3 + LTO):

| Target | Imperative | Indexed fold | Enumerate fold |
| --- | ---: | ---: | ---: |
| Native | 47.95 ms | 48.23 ms (1.01x) | 48.14 ms (1.01x) |
| Wasm | — | 1.13x | 0.91x |

What moved, and what it says:

- **Native enumerate 1.68x → 1.01x** is the source-adapter fusion. The old
  emit copied the captured 1,200-element vector and built a `Vec<(i64, f64)>`
  per row — 48,000 of each per run — and both are gone.
- **Native indexed is 1.42x at the default profile and 1.01x at release**, and
  the release column is what says why: at opt-level 3 all three spellings
  converge on ~48 ms. The indexed residual is what LLVM declines to do to an
  iterator chain at opt-level 1, not a tax the lowering emits.
- The release column is NOT a recommendation. Measured round-robin against the
  same sources, `--release` is *slower* than the default profile for the two
  spellings the default already handles well (imperative 47.95 vs 41.93 ms,
  enumerate 48.14 vs 41.77 ms) and faster only for indexed (48.23 vs 59.65 ms).
  Whichever profile a program ships with, its spellings should cost the same —
  that is the property this corpus gates, and both profiles now hold it within
  measurement noise except for the indexed case at opt-level 1.
- Wasm samples remain noisy; these are local comparative measurements, not a
  portable timing guarantee.

Earlier readings kept for the record: before the wasm scalar enumerate/fold
lowering, wasm enumerate measured 365.81 ms (3.21x); after it, 129.72 ms
(1.17x). Before native scalar-fold capture borrowing, native indexed measured
97.39 ms (2.36x); after it, 58.28 ms (1.42x).
