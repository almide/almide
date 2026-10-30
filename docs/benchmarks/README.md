# What is measured

Every claim on this page is either derived by a script or carries the date it was measured; `scripts/check-readme-numbers.sh` refuses a bare number in CI, and refuses an LLM-writability scorecard that is older than 90 days or that does not name the almide-dojo run it came from. The tables between `generated` markers are rendered from the committed ledgers in this directory (`*.txt`) and from `proofs/`; CI fails when a rendered block and its ledger disagree. Do not edit them by hand. The methodology and the per-benchmark scoreboard are in [docs/project/BENCHMARKS.md](../project/BENCHMARKS.md).

## LLM writability

Measured by [almide-dojo](https://github.com/almide/almide-dojo) on 2026-09-22 across its bank of 38 tasks (basic / intermediate / advanced), with the pinned compiler `almide 0.62.0`, by that repo's CI lane. Both runs are stamped **`comparable`** by the harness — every planned task reached the model, so each rate is a point and not an interval — and both were sampled at a fixed seed (`20260922`) and temperature 0, recorded in the run's manifest as what the provider actually put on the wire. The runs are committed — [`almide-dojo@8af34bc`](https://github.com/almide/almide-dojo/commit/8af34bc3) — so the table below can be recomputed from their `summary.md` rather than believed; later runs are on the [live dashboard](https://almide.github.io/almide-dojo/). No Anthropic or OpenAI key is in CI by decision, so the models here are the ones the lane can reach without one:

| Model | Pass Rate | 1-Shot Rate |
|---|---|---|
| Llama 3.3 70B (fp8-fast) | 65% (25/38) | 39% (15/38) |
| Llama 3.1 8B | 44% (17/38) | 34% (13/38) |

The most recent same-model comparison is the MiniGit bench: Sonnet 5 × 20 trials on 2026-07-15 with almide 0.29.0 (it has not been re-run on a later compiler), 100% pass, the most concise of 5 languages (233 LOC), and a faster agent wall-clock than Gleam and MoonBit but slower than Rust and TypeScript (573 s against 297 s) — an LLM-writability number, measured under 6–9× self-parallelism, **not** generated-code speed ([chart](../figures/lang-bench-snapshot-2026-07.png) · [method](../../research/benchmark/lang-bench/README.md) · [upstream](https://github.com/mame/ai-coding-lang-bench)).

## Byte-identical across targets

**Every program that compiles for both targets produces byte-identical observable output — stdout, stderr, exit code — whether it runs as a native binary or as WebAssembly.** Native is the oracle; `native == wasm` is a hard invariant, not a "target difference" to be documented around.

The guarantee is **continuous, with an explicit, ledger-managed scope**: "byte-identical" means the execution output, not the compiled artifacts; inherently nondeterministic sources certify deterministic *invariants* instead of exact bytes; APIs not yet implemented on wasm are compile- or run-time *refusals* — never wrong bytes; and exactly two fns are exempt because their job is to report the host — `env.os()` and `fs.temp_dir()`, bounded by C-189, since making them agree across targets would be the defect rather than the guarantee. Output printed from concurrent `fan { }` arms is inside the guarantee: each native arm writes into its own timeline, flushed in source order, so it reads as it does on wasm (C-004, ADR-0024 step 1).

This claim is not prose. Every observable promise is a named contract in the [behavior-contract ledger](../contracts/), each traceable to executable evidence, and the numbers below are regenerated from the ledger (`scripts/gen-claims.sh`, enforced by `scripts/check-contracts.sh` in CI):

<!-- claims:generated:start — derived from docs/contracts/contracts.toml by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
> <!-- counts:generated:start (as of 2026-10-21) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
> **Ledger: 388 contracts — 388 active, 0 flagged-for-revision.**
> <!-- counts:generated:end -->
>
> **Divergences awaiting a fix: none.** Every contract in the ledger is
> `active`, carrying executable evidence of class >= `fixture`. The one
> by-design carve-out in the law — the platform-reporting fns `env.os`
> and `fs.temp_dir` — is bounded by C-189.
<!-- claims:generated:end -->

Scope, ledger mechanics, and the evidence stack (contract ledger, cross-target fixture gate, differential fuzz, emit-time Σ-probes, Lean belt, org-wide byte-verify sweep): **[docs/design/EQUIVALENCE.md](../design/EQUIVALENCE.md)**.

## Memory safety — proven where it is proven, trusted where it is trusted

You write no ownership annotations, no lifetimes, no `free`: [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/)-style ownership inference in the compiler decides where every heap value is introduced, duplicated, and consumed — garbage-collector-free, pause-free. The checker for those decisions is **kernel-proven** (Rocq/Coq spine, 129 audited theorems and lemmas, axiom-clean, independently re-checked by `coqchk`; the count is asserted by `proofs/check.sh`), and `almide verify --emit` still produces the MIR ownership witness it checks. The per-build certificate used to ride the incumbent wasm leg, which #2761 deleted; the **structural wasm leg** (the only wasm renderer) and the **native leg** are trusted, certificate pending (#2755–#2760): their evidence is differential — byte-identical output against each other and the interpreter on the contract corpus, held by a grow-only floor and a semantic-mutation net — and the structural runtime's bytes are checked against the Coq decoder model by `proofs/check-structural-bytes.sh`. The boundary, stage by stage: **[proven-vs-trusted.md](../contracts/proven-vs-trusted.md)**; the full account, including the Lean 4 Perceus belt the design started from: **[docs/design/MEMORY-SAFETY.md](../design/MEMORY-SAFETY.md)**.

## Performance

No VM, no GC, no interpreter — native compiles through Rust to machine code, and WASM is emitted directly as self-contained modules (the runtime support a program reaches is linked in and the rest is pruned).

<!-- wasm-size:generated:start — rendered from docs/benchmarks/wasm-size.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
| Program (`almide build --target wasm`, as shipped) | develop | released v0.68.0-rc2 |
|---|---:|---:|
| Hello, world | **325 B** | **325 B** |

The develop column is the develop build at 437b8f127, before v0.68.0 is tagged (`almide --version`: `almide 0.68.0 (dev, 437b8f127)`), measured 2026-10-20; CI rebuilds it on every push and fails if the bytes move without a restamp of `docs/benchmarks/wasm-size.txt`. The released column is the compiler from the v0.68.0-rc2 release asset (`almide 0.68.0 (release, fcba2bcd5)`, `gh release download v0.68.0-rc2 -R almide/almide`), measured 2026-10-20. No post-hoc optimizer touches the shipped bytes (`--wasm-opt` is opt-in and its output is not the renderer's own module).
<!-- wasm-size:generated:end -->

Rust on the same wasm target is 37 KB+ for Hello, world even fully size-tuned: 37,541 B with `rustc 1.99.1`, `wasm32-wasip1`, `opt-level="z"`, `lto`, `strip`, `panic="abort"`, `codegen-units=1` (62,405 B with the default release profile), measured 2026-10-20 on GCE c4-standard-8 by the recipe at the bottom of [WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md). The native CLI-shaped benchmark (`research/benchmark/perf/native/cli_app.almd`, the build `research/benchmark/perf/native/measure.sh` makes) is 507,960 B stripped on x86_64 Linux (GCE c4-standard-8) and 463,720 B on aarch64 Linux (GCE c4a-standard-8), built 2026-10-20 by the v0.68.0-rc2 release binaries, with no crate dependencies (only the C library — `libc`, `libgcc_s` — is linked). The byte-by-byte dissection: **[docs/wasm/WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md)**.

Against handwritten Rust compiled with the same flags as the Almide side — the release profile plus the target CPU the native `--release` build uses (`x86-64-v3` on x86_64, the generic CPU on aarch64; `almide build --print-release-rustflags` prints them) — `bench.py`, full workloads, median of 5 interleaved runs, measured 2026-10-20 with the v0.68.0-rc2 release binaries (`almide 0.68.0 (release, fcba2bcd5)`) on GCE c4-standard-8 (x86) / c4a-standard-8 (arm): n-body **0.94×** / **1.02×**, spectral-norm **1.00×** / **0.99×**, fasta **1.16×** / **1.10×**, FFT **0.88×** / **0.94×**, wordfreq **0.73×** / **0.59×** (at 8M draws, its ratchet size — 2M runs under the 0.08 s spawn-noise floor), listbuild **0.87–0.89×** / **0.93–0.95×** ([scoreboard](../project/BENCHMARKS.md#runtime-scoreboard)). Where Almide has information Rust does not — a tree whose whole lifetime is one `check(make(depth))` expression, proven by the effect system — it is faster than the ordinary Rust for the same program:

<!-- native-victory:generated:start — rendered from docs/benchmarks/native-victory.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
| Workload (`bench.py` method, median of 9, interleaved) | optimization | Almide / ordinary Rust, x86 | Almide / ordinary Rust, arm | without it (`ALMIDE_REGION_OFF=1` / `ALMIDE_FAN_SEQUENTIAL=1`), x86 / arm |
|---|---|---:|---:|---:|
| binarytrees | region window (#1991) | **0.28 (d17)** / **0.27 (d19)** | **0.33 (d17)** / **0.32 (d19)** | 0.89 / 1.04 |
| treealloc | region window (#1991) | **0.22 (d20)** / **0.24 (d21)** | **0.27 (d20)** / **0.28 (d21)** | 0.91 / 1.03 |
| fannkuchredux | parallel fan (#2044) | **0.25 (n10)** / **0.23 (n11)** | **0.21 (n10)** / **0.18 (n11)** | 0.97 / 1.02 |

Measured 2026-10-20 with the v0.68.0-rc2 release assets (almide-linux-x86_64 / almide-linux-aarch64), built from main fcba2bcd5 (`almide 0.68.0 (release, fcba2bcd5)`) on x86 = GCE c4-standard-8 (Intel Xeon Platinum 8581C, 8 vCPU) and arm = GCE c4a-standard-8 (Neoverse-V2, 8 vCPU); the reference: rustc 1.99.1, with the release flags `almide build --print-release-rustflags` reports (-C target-cpu=x86-64-v3 on x86; -C llvm-args=-force-ordered-reductions=false on both). Two ratios per cell are the two input sizes (the size is in each cell); the Rust side is the ordinary program a person writes for it — a `Box` per node, one thread, no arena, no `unsafe`, no SIMD — compiled with the same `rustc` flags as the Almide side, and the "without it" column is the same Almide source with that one optimization turned off, at the larger size. The `perf-ratchet` job fails if a row reaches 1.0 or the ablation stops paying. Declaration and methodology: [docs/project/BENCHMARKS.md](../../docs/project/BENCHMARKS.md#faster-than-ordinary-rust-1330). Ledger: `docs/benchmarks/native-victory.txt`.
<!-- native-victory:generated:end -->

<!-- build-speed:generated:start — derived from docs/benchmarks/build-speed.txt by the almide-gates `bench` subcommand; DO NOT EDIT between the markers -->
Measured on almide 0.68.0 (dev), x86_64 Linux, INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8], `examples/lisp.almd` (297 lines), 2026-10-19. Every row is an N-run MEAN —
a single run of a 30ms process is scheduler noise. Cold clears BOTH `$TMPDIR/almide-run`
and the dependency cache before each repetition; clearing only the latter measures a warm
build. Regenerate with `almide run tools/almide-gates/src/main.almd -- bench`; the ratchet
(`-- bench docs/benchmarks --check`) fails at 1.5x on the machine class above and skips, saying so, on any other.

| scenario | time | runs |
|---|---|---|
| `almide check` | **25.2 ms** | 20 |
| build, warm (content-cache hit) | **92.9 ms** | 5 |
| build, cold | **4846.1 ms** | 3 |
| build, cold, `--target wasm` | **148.7 ms** | 3 |
<!-- build-speed:generated:end -->

`almide check` scales close to linearly: over a 2k → 33k-line ladder of this repo's own stdlib the log-log slope of check time against project lines is **1.10** on both machines (1.0 is linear, 2.0 quadratic) and the 10.7k-line rung costs **4.2×** (x86) / **4.3×** (arm) the empty-project floor — measured 2026-10-20 with the v0.68.0-rc2 release binaries on GCE c4-standard-8 / c4a-standard-8 (`research/benchmark/editloop/scale.py --runs 40`), held by `scripts/check-edit-loop-scale.sh`, table in [BENCHMARKS.md](../project/BENCHMARKS.md#edit-loop-scale-1334). The `perf-ratchet` job (`scripts/check-perf-ratio.sh`) re-measures every native row on each develop CI run and fails an anchored row (n-body, spectral-norm, fasta, FFT, wordfreq) that drifts more than 40% past its committed baseline. Wasm runtime, measured and gated (#1701):

<!-- wasm-runtime:generated:start — rendered from docs/benchmarks/wasm-runtime.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
| Benchmark (`almide bench`, verify-then-time, min of 2×5 interleaved) | wasm/native, `main` only | cold start (spawn vs compile + instantiate) |
|---|---:|---:|
| nbody | **1.21×** | 1.41× |
| spectralnorm | **1.18×** | 1.27× |
| binarytrees | **1.20×** | 1.34× |
| treealloc | **1.08×** | 1.13× |
| fasta | **0.90×** | 1.00× |
| fannkuchredux | **1.37×** | 1.48× |
| mandelbrot | **1.21×** | 1.30× |
| onebrc | **1.90×** | 2.56× |
| fft | **1.42×** | 1.52× |
| strchurn | **0.92×** | 0.95× |
| listbuild_append | **1.77×** | 1.98× |
| listbuild_combinator | **1.55×** | 1.74× |
| listbuild_prealloc | **1.36×** | 1.53× |
| mapbuild | **0.92×** | 1.08× |

Embedded wasm host (Perceus RC in linear memory) against the native binary, same machine, same run. The ratio times the program's own `main`, entry to return, on both legs (native in-process, wasm around the host call): process spawn and module compile/instantiate are outside it, and the cold-start column shows them (#2980). Small workloads run at a ledger-fixed size (`args=`) so `main` is long enough to time. Cross-engine ratios do NOT cancel hardware (a 2-core CI runner measures nbody ~10x worse), so the stamped ratio verdict runs on the stamping machine class; CI gates the STATUS taxonomy below and judges the wasm leg by a same-runner A/B against the latest release binary (interleaved, min-of-runs, `ab_band` in the ledger — #2143) (`scripts/check-wasm-runtime-ratio.sh`). Of these rows only fannkuchredux runs its `fan` in parallel: native on threads (#2044), the embedded wasm host on separate instances of the module (#3003, ADR-0011 §D2a; every other wasm host runs it sequentially, byte-identical); binarytrees' and mandelbrot's `fan.map` callbacks run sequentially on both legs (user time equals wall time on both, measured 2026-10-03). The unmeasured corpus cells stay honest instead of estimated: 0 wall on the wasm build path, 0 exhaust the embedded heap (#1729) — each re-measured every gate run, so a cell that starts benching fails the gate until its row is promoted. Ledger: `docs/benchmarks/wasm-runtime.txt` (a develop build carrying version 0.68.0, not the 0.68.0 release, 2026-10-19, measured on x86_64 Linux, INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz [GCE c4-standard-8]).
<!-- wasm-runtime:generated:end -->

## Derived counts

The stamped totals behind the [project status](../status.md), and the mutation score of the gate net.

<!-- stats:generated:start — derived from docs/stdlib/*.md, spec/, and docs/contracts/contracts.toml by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->
<!-- counts:generated:start (as of 2026-10-21) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
| Derived count | Value |
|---|---|
| Stdlib | 999 functions across 44 modules — self-hosted `.almd`, signature indexes regenerated from the compiler by `tools/gen-stdlib-doc-index.py` |
| Tests | 514 `.almd` test files under `spec/` (`almide test spec/`) + the 388-contract cross-target ledger |
<!-- counts:generated:end -->
<!-- stats:generated:end -->

<!-- mutation-score:generated:start (as of 2026-09-22) — stamped from proofs/mutation-score.toml by scripts/gen-mutation-score.sh; re-measured by every mutation-sweep run, refreshed only by `--from-run`; DO NOT EDIT between the markers -->
**Mutation score** — 41/41 mutants caught (100.0 %), 0 survived, 0 stale: the full release-shape net sweep of `ci/mutations/` ([`scripts/check-mutation-gate.sh`](../../scripts/check-mutation-gate.sh)), stamped 2026-09-22 from mutation-sweep run [35650396971](https://github.com/almide/almide/actions/runs/35650396971) at `3b02f7dc4`.
<!-- mutation-score:generated:end -->
