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

Fixed 2026-10-19 (#3604). Two causes, both general:

- **The vectorizer took the in-order float reduction.** Almide's `Float` `+`
  is strict, so `sum + eval_a(i, j) * v[j]` is an ordered reduction. x86's
  cost model never vectorizes one; AArch64's opts in, and its generic tuning
  then paired `eval_a`'s divide across NEON lanes while the i64 multiply (no
  64-bit vector multiply on NEON) and the ordered adds ran lane by lane
  around it (`fdiv v.2d` in the hot loop). A release build now passes
  `-C llvm-args=-force-ordered-reductions=false` on every target
  (`RELEASE_LLVM_ARGS` in `src/cli/native_target.rs`): these loops stay
  scalar, integer reductions and reduction-free loops vectorize as before.
  On x86 it is what the cost model already did.
- **A range literal heading a fold was built.** `list.fold(0..<n, …)`
  materialized a `Vec<i64>` per row, and an index read out of a `Vec` told
  LLVM nothing about its range, so `v[j]` kept its bounds check in the loop.
  The literal now counts like `list.range(a, b)` on both legs (#3810). A
  collecting chain (`list.map(0..<n, f)`) keeps the built head: its failure on
  a span no machine holds stays C-197's out-of-memory abort.

Before → after, GCE, n=1200, 21 samples (hyperfine median, ms), every
artifact printing `1.274224150`:

| Arch | Profile | Imperative | Indexed fold | Enumerate fold |
| --- | --- | ---: | ---: | ---: |
| aarch64 (Neoverse-V2) | release | 76.91 → 53.26 | 69.59 → 57.65 | 76.90 → 54.22 |
| aarch64 (Neoverse-V2) | default | 53.76 → 53.76 | 90.65 → 53.47 | 62.87 → 62.83 |
| x86_64 (Xeon 8581C) | release | 48.17 → 48.13 | 63.47 → 47.52 | 49.88 → 52.30 |
| x86_64 (Xeon 8581C) | default | 60.04 → 60.31 | 70.81 → 59.53 | 59.32 → 58.46 |

x86 enumerate release is the same machine code before and after (the option
is inert there and the source did not change); its two readings are this
VM's run-to-run spread. What remains on aarch64 is the indexed fold at 1.08x
its imperative twin: with the outer index read from the built list and the
inner one counted, LLVM re-associates `eval_a`'s `ij * (ij + 1) / 2` around
the one index whose range it knows (the same effect, mirrored, measured for
enumerate with the outer head counted: a `mul` + `sub` pair where the
other build has one `madd`). Counting the outer head too measured 53.45 ms for
indexed but 62.0 ms for enumerate, and would change a collecting chain's
failure form, so it is not taken.

Counting the outer head and the enumerate index, measured 2026-10-20 and not
taken (#3604 direction 1). Two candidate lowerings were measured:

- counting a COLLECTING range-literal head (`list.map(0..<n, f)`) into one
  vector reserved for the span, with the C-197 abort;
- zipping the enumerate adapter's index from `0i64..i64::MAX` instead of
  casting `.enumerate()`'s `usize`.

Apple M4 Pro (aarch64-apple-darwin, the host's default `apple-m1` tuning),
`--release`, n=5500, 11 round-robin samples, median ms, every artifact printing
`1.274224153`. "range" is `list.map(list.range(0, n), (i) => list.fold(list.range(0, n), …))`,
whose head was already counted:

| Profile | Imperative | Indexed fold | Enumerate fold | Range fold |
| --- | ---: | ---: | ---: | ---: |
| release, develop | 727.8 | 694.1 (0.95x) | 710.6 (0.98x) | 712.0 (0.98x) |
| release, both lowerings | 730.7 | 709.6 (0.97x) | 707.7 (0.97x) | 714.9 (0.98x) |
| default, develop | 749.1 | 786.1 (1.05x) | 725.7 (0.97x) | 747.0 (1.00x) |
| default, both lowerings | 748.3 | 751.2 (1.00x) | 709.6 (0.95x) | 747.7 (1.00x) |

Under LLVM's GENERIC aarch64 tuning, which is what a Linux aarch64 `--release`
build gets, the outer count costs. The same emitted Rust was built with rustc
`-C opt-level=3 -C codegen-units=1 -C panic=abort -C target-cpu=generic
-C llvm-args=-force-ordered-reductions=false` and run on the M4 (n=3000, 9
samples, ms):

- imperative: 238.3;
- enumerate: 189.0 on develop, 234.5 with both lowerings;
- indexed: 210.4 on develop, 237.8 with the outer count.

Taking the enumerate changes one at a time: the zipped index alone reads 188.9
(neutral), and the counted outer head alone reads 215.2. A Rust microbenchmark
of only the outer loop reads 205 ms for a built `Vec<i64>` against 228 ms for
`extend`, `push` or `collect` over `0..n` under generic tuning, and 207–212 ms
for all four under `apple-m1`. LLVM schedules the inner loop differently when
the outer index's range is known: a `madd` of `i + j` instead of a `mul` of the
strength-reduced `ij`.

So under generic tuning, counting the outer head would take the idiom
spellings from 12–20% faster than the imperative down to its speed, and under
the host's tuning it gains nothing. The zipped index is neutral everywhere. Neither
is taken: the 1.53x itself was fixed by #3824. What did land is the failure
form of the chains that were ALREADY counted. `list.range(a, b) |> list.map(f)`
collected through `.collect()`, which panicked with `capacity overflow` on a
span no machine holds. It now fills a vector reserved with the C-197 abort,
the same as `almide_rt_list_range` and the wasm leg
(`tests/counted_index_chain_test.rs`). Spectralnorm is not affected.

Earlier readings kept for the record: before the wasm scalar enumerate/fold
lowering, wasm enumerate measured 365.81 ms (3.21x); after it, 129.72 ms
(1.17x). Before native scalar-fold capture borrowing, native indexed measured
97.39 ms (2.36x); after it, 58.28 ms (1.42x).
