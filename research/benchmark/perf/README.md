# Perf Suite — Native & WASM Runtime Scoreboard (#917)

Seven Computer-Language-Benchmarks-Game-style programs, run on every leg the
compiler ships, against handwritten Rust references where a fair reference
exists. This directory is the source of the numbers in
[docs/BENCHMARKS.md](../../../docs/BENCHMARKS.md) — nothing is published that
`bench.py` did not produce.

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
- The `fft-wasm` row exists because the wasm leg currently collapses on hot
  `data[i] = x` list writes (~3 orders of magnitude at 2^18) — the canonical
  2^22 workload would take hours on that leg. The cliff is the finding; it is
  recorded at a workload that terminates.

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
  scoreboard covers the seven whole-program benches only so far.
- The wasm leg is measured but not ratcheted: the fft list-write cliff
  (~3,500× at 2^18) and the mandelbrot crater (~130× at 4000) have to be fixed
  first, otherwise the gate would just re-report two known craters. On the
  other kernels the wasm leg already sits within 1.1–1.2× of native — and
  binary-trees runs 3.5× *faster* than the native leg (thread fan-out plus
  allocator churn vs the wasm bump path), which deserves its own look.
