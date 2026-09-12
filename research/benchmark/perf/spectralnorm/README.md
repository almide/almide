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

General capture borrowing and enumerate fusion remain implementation work.
Synchronous scalar folds now borrow captures proven to be read-only through
indexing, explicit borrowing or cloning, with no escaping nested closure.
Scalar enumerate/fold now snapshots the source into a scalar list and
reuses one private tuple slot when the callback cannot expose that tuple.
It removes the intermediate tuple list and per-element tuple allocation.

Measured on 2026-09-12 with the local v0.62.0 release build and Wasmtime
47.0.3, n=1200, nine samples after warmup:

| Target | Imperative | Indexed fold | Enumerate fold |
| --- | ---: | ---: | ---: |
| Native median | 41.34 ms | 97.39 ms (2.36×) | 84.10 ms (2.03×) |
| Wasm median | 114.11 ms | 149.04 ms (1.31×) | 365.81 ms (3.21×) |

Every run printed `1.274224150` plus a newline, with empty stderr. Wasm
samples were noisy (imperative 97.92–161.90 ms); these are local comparative
measurements, not a portable timing guarantee. The n=80, three-sample smoke
run also passed output agreement.

After scalar enumerate/fold lowering, the same command measured wasm
enumerate at **129.72 ms**, versus **110.60 ms** imperative (**1.17×**);
the prior enumerate median was 365.81 ms. Artifact size fell from 12,905
to 12,775 bytes. Outputs remain identical. Native code was not changed
(indexed 2.34×, enumerate 2.05× in this run). Wasm samples remain noisy,
so the timing comparison is supporting evidence, not an exact speedup
guarantee or completion of the full #2098 optimization work.

After native scalar-fold capture borrowing, the indexed median is
**58.28 ms / 1.42×**, down from 97.39 ms / 2.36× before optimization.
The native enumerate median is **68.88 ms / 1.68×**. Native indexed
artifact size drops from 510,184 to 491,848 bytes. The repeated six-way
output check still passes. This removes per-row capture copies for the
indexed fold; outer map captures and more general combinators remain
outside the current borrowing rule.
