# Perf Suite — Native & WASM Runtime Scoreboard (#917)

Nine benchmark programs — seven Computer-Language-Benchmarks-Game-style
kernels, a scaled One Billion Row Challenge, and the three-shape `listbuild`
family — run on every leg the compiler ships, against handwritten Rust
references where a fair reference exists. This directory is the source of the
numbers in [docs/project/BENCHMARKS.md](../../../docs/project/BENCHMARKS.md) —
nothing is published that `bench.py` did not produce.

## Layout

| Path | Purpose |
|---|---|
| `<bench>/<bench>.almd` | Benchmark source (Almide) |
| `rust-ref/*.rs` | Handwritten Rust references, compiled with the exact flags the native leg uses |
| `bench.py` | Build + verify + time harness; writes `results/` |
| `results/<date>-<label>.json` | Dated raw results (committed) |
| `native/` | The README binary-size/CLI measurement (`measure.sh`, separate from the scoreboard) |

## Method

- **Legs**: `native` = `almide build --release` (cargo release profile:
  opt-level=3, LTO, 1 CGU); `wasm` = `almide build --target wasm` executed by
  the wasmtime CLI; `rust` = `rust-ref/*.rs` via
  `rustc -C opt-level=3 -C lto=yes -C codegen-units=1 -C overflow-checks=no`.
- **Correctness before speed**: every variant of a benchmark must produce
  byte-identical stdout on a small workload before anything is timed
  (fft compares only line 1 — line 2 is its self-reported time). A scoreboard
  entry that computes the wrong answer is not a scoreboard entry.
- **Timing**: wall-clock of the whole process, stdout to /dev/null, one warmup
  then N interleaved rounds (variant A, B, C, A, B, C…) so drift hits every
  variant equally. Median is the headline number; the JSON keeps every run.
- **Quiet machine**: close the browser, don't build the compiler in parallel,
  don't trust a run taken while anything heavy shares the box. Apple Silicon:
  wall-clock on a quiet box lands on P-cores; no pinning is attempted.
- **References are same-shape where the claim needs it**: `nbody_unrolled.rs`
  mirrors nbody.almd's fully-unrolled scalar locals to isolate codegen
  overhead; `nbody.rs` is the canonical array-of-bodies shape people actually
  write. Almide currently beats the latter (bounds checks) — that comparison
  is reported, not gated.
- `fannkuchredux`, `binarytrees`, `mandelbrot` use `fan` parallelism, so a
  scalar Rust reference would be a lie — they run Almide-native vs Almide-wasm
  only.
- `onebrc` is a scaled One Billion Row Challenge (`station;temp` lines →
  sorted per-station min/mean/max): the one row whose hot loop is file I/O,
  `string.split`, and map updates rather than arithmetic. Temperatures are
  integer tenths end-to-end, so output is byte-identical with no float
  formatting involved. The wasm leg is excluded — `wasmtime run` preopens no
  directory, so the leg cannot touch files. This row is the birthplace and the watch
  of the streaming line family (`fs.for_each_line` / `fs.fold_lines`, C-220).
  The original eager `fs.read_lines` shape measured RSS ~4× file size
  (2026-08-08, M4 Pro: 505 MB for a 126 MB / 10 M-row file, 2.2 GB for
  632 MB / 50 M — identical in the same-shape Rust ref, so the wall was the
  API's shape, not codegen), extrapolating to ~50 GB on the official
  1 B-row file. The aggregate phase now streams on both legs: RSS holds at
  1 MB at every scale. The first streaming cut paid a time regression
  (33.4 s at 50 M rows vs 15.6 s eager) traced to the fold accumulator's Map
  being cloned per line — the clone pass counted syntactic uses across
  mutually-exclusive match arms, so a branch tail could never be a last use
  (#1143). The full perf-war ledger (2026-08-08, 50 M rows / 632 MB, M4 Pro), each
  rung byte-identical to the last:
  eager 15.6 s / 2.2 GB → first streaming cut 33.4 s / 1 MB (the fold
  accumulator's Map cloned per line — the clone pass counted syntactic uses
  across mutually-exclusive match arms, so a branch tail could never be a
  last use; #1143) → sibling-deduction fix + consuming `map.set` 15.2 s →
  `string.split_once` + `map.upsert` (one lookup, no Vec, no key clones)
  8.2 s → `fs.fold_lines_chunked` (range workers on runtime threads,
  partials merged by the caller) **1.27 s at 8 workers / 0.95 s at 12,
  vs the same-shape single-thread Rust reference at 2.58 s** — the naive
  fold submission beats handwritten sequential Rust on structure, the
  binarytrees play repeated on native. Scaling is real (1w 8.5 s → 2w
  4.6 s → 12w 0.95 s) and RSS holds at 2 MB. Honest caveats: the reference
  is deliberately single-threaded (a hand-parallelized Rust would win
  again); the per-core gap (~3.3×) is per-line allocation vocabulary
  (split_once ×2 + strip_prefix + a per-line upsert closure) plus the
  linear-scan `AlmideMap`; and a Map captured by a closure
  (`for_each_line` + `var stats`) still clones on every READ through the
  `SharedMut` cell — aggregation belongs on `fold_lines`, which is what the
  CHEATSHEET teaches. Wall-clock ratios here are reported, not gated, while
  the war continues.
- The `fft-wasm` row exists because the wasm leg currently collapses on hot
  `data[i] = x` list writes (~3 orders of magnitude at 2^18) — the canonical
  2^22 workload would take hours on that leg. The cliff is the finding; it is
  recorded at a workload that terminates.
- **`listbuild` is three rows, one workload** (#1337): the same 2^23-element
  interleaved `Float` array built by (a) preallocate + indexed write,
  (b) `var` + `for` + `data = data + [x]`, (c) `list.range |> list.flat_map`.
  The three sources differ ONLY in the build loop — identical arithmetic,
  identical checksum consumer — so the spread between the rows IS the cost of
  the shape, and all three are verified byte-identical before timing. The
  family exists because (c) is the shape CLAUDE.md and docs/CHEATSHEET.md
  recommend, and it was the SLOWEST of the three: 1.67x the append loop it is
  documented to replace, all of it one heap allocation per element for
  `flat_map`'s intermediate list. That is an MSR problem before it is a perf
  problem — the idiom docs are the in-context material that steers generated
  code, so "recommended" and "fast" have to name the same shape. The three now
  sit within 2% of each other. `check-perf-ratio.sh` gates each row's ratio
  like every other benchmark AND gates the relation between them directly
  (`IDIOM_CEILING`), because the ratios drifting apart is the failure this
  family exists to catch.

## Run

```bash
python3 research/benchmark/perf/bench.py                 # full suite, all legs
python3 research/benchmark/perf/bench.py --runs 7 --label m4pro
python3 research/benchmark/perf/bench.py --legs native,rust --bench nbody
ALMIDE_BIN=target/release/almide python3 research/benchmark/perf/bench.py
```

Requires `almide` (or `ALMIDE_BIN`), `rustc`, and — for the wasm leg — the
`wasmtime` CLI.

## CI ratchet

`scripts/check-perf-ratio.sh` (CI `perf-ratchet` job) gates the
almide-native / handwritten-Rust ratio per benchmark against
`scripts/perf-ratio-baseline.txt` on the `--quick` workloads. Ratios, not
absolute times — the ratio cancels the runner. Regressing or improving
durably = move the baseline in the same change. See the script header for the
full policy.

## Not yet covered

- The 2–3 stdlib micro-benchmarks #917 asks for (string/map churn) — the
  scoreboard covers the whole-program benches plus the `listbuild` shape family
  so far.
- The residual **materialization** cost every listbuild row shares (~1.6x vs
  handwritten Rust for the identical build, whichever shape writes it) is
  #1004, and it is untouched by #1337: closing the shape spread only proved
  that the remaining gap is not about the shape.
- The wasm leg is measured but not ratcheted: the fft list-write cliff
  (~3,500× at 2^18) and the mandelbrot crater (~130× at 4000) have to be fixed
  first, otherwise the gate would just re-report two known craters. On the
  other kernels the wasm leg already sits within 1.1–1.2× of native — and
  binary-trees runs 3.5× *faster* than the native leg (thread fan-out plus
  allocator churn vs the wasm bump path), which deserves its own look.
