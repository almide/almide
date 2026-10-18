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

Remeasured on Linux on 2026-10-12, develop `1df5391`, GCE c4-standard-8 (x86_64) and c4a-standard-8
(aarch64, Neoverse-V2), n=1200, nine samples. The table gives the native leg, release median ÷ default median for each spelling. Both
profiles were timed round-robin with `compare.py --profile-pair`:

| Arch | Imperative | Indexed fold | Enumerate fold |
| --- | ---: | ---: | ---: |
| x86_64 | 76.0 / 76.2 ms (1.00x) | 85.3 / 93.3 ms (0.91x) | 76.0 / 76.1 ms (1.00x) |
| aarch64 | 76.9 / 53.8 ms (**1.43x**) | 73.6 / 90.8 ms (0.81x) | 76.9 / 63.0 ms (**1.22x**) |

On aarch64, `--release` is slower for every spelling. The cause is not a spelling tax. LLVM's loop
vectorizer, costed for a generic aarch64 core, splits `eval_a`'s i64 multiply
across NEON lanes. With `-C llvm-args=-vectorize-loops=false` or
`-C target-cpu=native`, the slowdown goes away (#3604). A ratio between
spellings cannot see a tax that every spelling pays alike. The gate's profile leg
compares each spelling with itself across the two profiles. The `perf-profile-arm` CI job runs that leg on
`ubuntu-24.04-arm`.

Earlier readings kept for the record: before the wasm scalar enumerate/fold
lowering, wasm enumerate measured 365.81 ms (3.21x); after it, 129.72 ms
(1.17x). Before native scalar-fold capture borrowing, native indexed measured
97.39 ms (2.36x); after it, 58.28 ms (1.42x).
