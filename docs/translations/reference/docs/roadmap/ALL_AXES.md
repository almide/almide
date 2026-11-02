<a id="all-axes--総合格闘技の勝ち筋"></a>

# All Axes — Paths to an all-around championship

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](ALL_AXES.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ALL_AXES.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ALL_AXES.zh-CN.md)

> Non-normative historical strategy snapshot, not current marketing or certification evidence. Pinned original authoritative and unchanged; no automatic synchronization, fresh source verification, independent benchmarking/Dojo/proof or certification audit. All rankings, exclusivity, external-project timelines, CompCert credit, MSR/LOC and performance figures are dated original-author claims, not verified here or generalized. Preserve later corrections to materialization/libm, fan/fusion and LSP alongside older rows; the apparently inconsistent payload/Int-only scope and structured/unstructured diagnostics also remain as written. Qualification/GPU/ecosystem goals are not achieved guarantees. Original dangling local review-heading fragment is retained as a source defect; no fictitious target added. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ALL_AXES.md)

> Strategy by axis for “winning both niche belts and the overall belt”.
>
> If [ROAD_TO_1_0.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md) is the **time axis** (0.65→0.99 version ladder, redrawn 2026-09-27), this document is the **evaluation axis**.
> It views the same work from another angle, not competing with the ledger: ledger rows are cited here as “evidence for an axis”.
> Before starting work on a new axis, **define its victory condition here, then allocate it to a ledger decade**.
>
> **Successor delta dated 2026-09-13**: [active/arena-breakthroughs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md) fixes **measured rankings, losses and unmeasured areas** plus breakthroughs T1–T9, from the all-field arena (15 toolchains) and mechanism readings of 8 neighboring languages. This document's victory conditions remain; that successor owns updated current values and re-entry conditions for axes we leave.

<a id="前提-2026-年に単一の最強は存在しない"></a>

## Premise: no single “strongest” exists in 2026

The external survey (2026-08-13, four directions, about 120 sources) had a clear conclusion: the compile-speed leaders (Zig/tsgo), runtime-performance leader (LLVM) and proof leader (CompCert) are **different implementations**. No implementation has conquered all axes simultaneously. Carbon, with world-class personnel and Google's backing, took about six years from announcement to 0.1.

Therefore, define **“strongest” separately by axis and choose different ways to win**. Some axes permit a head-on fight; others require changing the arena to survive. The distinction follows.

<a id="軸マップ"></a>

## Axis map

| Axis | Current value (measured) | Victory condition (measurable exit) | Winning route | Main arena |
|---|---|---|---|---|
| **A. Edit-loop speed** | check 22.7ms (268 lines) / **10k lines: p95 53.3ms, slope 1.13** (#1334) / warm build 266ms / cold 644ms | ~~Scale-independent at 10k lines~~ **achieved**; no rustc/linker in debug path | **Head-on** (known engineering) | 0.4x, 0.6x |
| **B. Runtime performance** | **1.00×** handwritten Rust (n-body, spectral-norm) to 1.18× (FFT); materialization-heavy workloads about 1.6× | ① Maintain ratchets ② **below 1.0× through semantic optimization** ③ GPU in another class | **Do not challenge LLVM in scalar codegen** | 0.5x, 0.6x |
| **C. Correctness/trust** | 260 contracts / 3-way oracle / Lean belt / 1,400-line PCC checker | Qualification dossier ready for third parties; TV certificate for every build | **Current route + generative fuzzing** | 0.7x, 0.8x |
| **D. LLM writability** | MSR 100%(30/30) / MiniGit 20/20, 233 LOC (smallest of 5 languages) | **MSR becomes the arena** (third-party reproduction; same conditions for other languages) | **Home ground; become the definer** | 0.5x |
| **E. Toolchain DX** | Single binary + built-in LSP + playground | Complete agent surface (JSON diagnostics + fix-it + MCP) | **Nearly secured; agent surface remains** | 0.5x |
| **F. Ecosystem** | MVS + git deps, no registry, self-hosted stdlib: 41 modules / 959 functions | External packages **written by agents**, passing MSR gates and published | **Cannot win by engineering; compound the thesis** | After 1.0 |

---

<a id="軸-a-編集ループ速度--正面から殴ってよい"></a>

## Axis A: Edit-loop speed — a head-on contest is appropriate

**Current values** (README generated block, 2026-08): `almide check` 22.7ms / warm build 266.1ms / cold build 643.9ms / cold wasm 299.1ms. But **the measured target is small**; this alone does not prove scale independence.

**The scaling side was measured on 2026-08-13** ([#1334](https://github.com/almide/almide/issues/1334), `scripts/check-edit-loop-scale.sh`). The corpus is this repository's stdlib itself (303 handwritten modules / 30,624 lines, no synthetic material), arranged as nested byte-order prefixes into a ladder. Each rung was interleaved 40 times:

| Scale | `check` p50 | p95 | µs/line |
|---:|---:|---:|---:|
| 2,047 lines | 18.4ms | 19.5ms | 2.87 |
| 5,456 lines | 31.2ms | 32.3ms | 3.29 |
| **10,151 lines** | **52.1ms** | **53.3ms** | 3.78 |
| 20,378 lines | 97.4ms | 99.5ms | 4.06 |
| 30,624 lines | 141.5ms | 145.1ms | 4.06 |

**Goal 1 achieved (10k lines, p95 53.3ms < 100ms). But milliseconds cannot be gated**: on the same commit/machine, p95 was 52.9ms at load average 6.0 and 332.4ms at load 42.4 (6×). This is the same trap #1337 encountered in the `listbuild` row, so the ratchet owns **dimensionless ratios within the same run**: marginal check-time log-log slope **1.13** (1.0 linear, 2.0 quadratic; moved only 0.6% across the above sevenfold load change), and the 10k-line rung's **4.4×** multiple over the empty-project floor. `run` is dominated by rustc/linker costs and does not belong on this ladder (eliminate those costs themselves in 0.66–0.67).

**Victory conditions**:
1. ~~10k-line dogfood (0.46/0.57), `check` p95 < 100ms~~ **Achieved (53.3ms, #1334)**; `run` p95 < 500ms awaits 0.66–0.67.
2. **rustc and linker disappear** from the debug path (0.66–0.67).
3. Numbers are public in README and ratcheted (0.69) — **achieved for check**.

**Basis and winning route**: all 2026 fast-compilation examples remove LLVM from the development loop (Zig self-build 75s→20s, incremental 228–288ms / Roc 3.4s→35ms / tsgo **10× without incremental compilation**, half from native implementation and half shared-memory parallelism). Almide is already pure Rust, so the first half is done. The original candidate was **Cranelift debug tier + module parallelism + flat index-based layout (#1316)**. The old 0.6x allocation changed in the successor ladder dated 2026-09-27; do not treat it as the current release schedule. All is known engineering; no invention required.

**Ledger rows**: 0.47–0.49 (queries/cache, but [coarse granularity suffices](#査読-047048-はクエリエンジンではない)); original 0.61–0.69 plan (cranelift → DWARF → default switch → JIT → function-level incremental) is historical. #1311 references the lines/second ratchet.

**Missing rows**: none ([#1334](https://github.com/almide/almide/issues/1334) landed on 2026-08-13). Starting 0.47/0.49 (query layer/module cache) awaited these numbers: slope 1.13 does not indicate collapse without a query engine, supporting the matklad 2026-02/Pyrefly line (**coarse-grained cache suffices**). Reconsider when the slope violates the ratchet.

---

<a id="軸-b-実行時性能--土俵をずらして勝つ"></a>

## Axis B: Runtime performance — win by changing the arena

**Current values** (`docs/project/BENCHMARKS.md`, CI-ratcheted): n-body 1.135s / spectral-norm 0.685s, **1.00× handwritten Rust**, FFT alone 1.18× (input-generation shape corrected 2026-08-13, #1338). The wasm leg is about 1.13–1.16× native.

**The 2026-08-13 measurements revised one assessment of this axis.** Decomposing FFT 2^22 (startup/input generation/kernel; 11 interleaved medians; output matches to 17 digits):

| Generation form | Generation | **Kernel** | Total wall | Kernel versus Rust |
|---|---:|---:|---:|---:|
| Preallocation (`list.repeat` + indexed assignment) | 66.4ms | **82ms** | 154.3ms | **0.872×** |
| `list.range \|> flat_map` | 116.0ms | 92ms | 213.9ms | 0.979× |
| Rust reference (handwritten) | 39.3ms | 94ms | 138.9ms | 1.000× |
| Current benchmark (append loop) | 69.1ms | 101ms | 175.9ms | 1.074× |

Two rows in this table were resolved that same day (2026-08-13). `list.range |> flat_map` generation stood out because **the flat_map lambda heap-allocated an intermediate list per element**. Lowering a fixed-length trailing literal to return a Rust array brought it level with the append loop ([#1337](https://github.com/almide/almide/issues/1337), permanently added/ratcheted as three `listbuild` perf-suite rows). The benchmark itself also switched to preallocation, reducing the FFT row 1.27× → 1.18× ([#1338](https://github.com/almide/almide/issues/1338)). **Materialization itself remains.**

**The kernel already beats handwritten Rust by 13%** (process startup 5.9ms versus 5.6ms, no difference, independently measured with hello-world). Thus the initial “scalar codegen is the ceiling” assessment was **overturned by measurement**: the actual bottleneck is **list materialization (even the best shape is 1.69× Vec)**, not codegen. Keep the policy of not competing with LLVM in instruction selection, but B's loss is concentrated in one issue: [#1004](https://github.com/almide/almide/issues/1004).

<a id="b1守り-ratchet-維持--materialize-コストの返済--この軸の最優先"></a>

### B1 (defense): Maintain ratchets and repay materialization cost — **highest priority on this axis**

Maintain the `scripts/check-perf-ratio.sh` culture and close the materialization gap (0.55–0.56 / [#1004](https://github.com/almide/almide/issues/1004)). As measured above, this is no longer merely avoiding defeat, but **a prerequisite to winning**: it entirely explains why a 0.872× kernel still yields 1.111× total wall time.

> **Correction from the 2026-08-13 profile** ([string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)): what this axis called “RcCow representation cost” does not exist. `String` lowers 1:1 to Rust `String`; `RcCow` is exclusively for Bytes/Matrix (`crates/almide-codegen/templates/rust.toml`). Moreover, the ~1.6× `listbuild` cost is not even materialization: it is entirely **deterministic software libm** (`sin`/`cos`). Substituting platform libm makes emitted code beat handwritten Rust by 1.17×. Materialization truly matters in **list pipelines with cheap lambda bodies**: `list.range` materialization alone causes 6.6× (30M elements; `IterChain` has never fired on the Rust target). That is what to close.

Related: [#1337](https://github.com/almide/almide/issues/1337) (recommended idiom paradoxically slowest), [#1338](https://github.com/almide/almide/issues/1338) (public benchmark itself uses the slowest form).

<a id="b2攻め-意味論が許す最適化で-handwritten-rust-を抜く--1330httpsgithubcomalmidealmideissues1330"></a>

<a id="b2攻め-意味論が許す最適化で-handwritten-rust-を抜く--1330"></a>

### B2 (offense): Beat handwritten Rust with semantics-enabled optimizations ← [#1330](https://github.com/almide/almide/issues/1330)

Almide has information Rust lacks: **value semantics, effect-based purity proofs, deterministic data parallelism (fan), HKT stream fusion (Phases 1–3 shipped)**. Where handwritten Rust uses `for` loops, Almide can fuse, parallelize and vectorize “because legality can be proved”. This is the same route by which Fortran beats C and Futhark beats handwritten CUDA, and **does not compete with LLVM** (it supplies semantics above LLVM).

**Victory condition (form of the claim)**: publish at least two named workloads **below 1.0×** (= Almide faster) in README with CI ratchets. Candidates: map/filter/fold chains (fusion benefits) and deterministic `fan` parallelism. Explicitly describe the handwritten Rust side as “ordinary Rust where the human has not manually written the same optimization” (do not claim superiority over `unsafe`-tuned Rust).

**Declared 2026-09-08** (`docs/project/BENCHMARKS.md` “Faster than ordinary Rust”, README native-victory block, `check-perf-ratio.sh` VICTORY rows): **binarytrees 0.35× / 0.32×, treealloc 0.31× / 0.30×** (M4 Pro, two sizes each, 9 interleaved medians). The same source with `ALMIDE_REGION_OFF=1` gives 1.25× / 1.11×: the entire win is **region windows** (#1991, rewinding `check(make(d))` once in a bump arena). CI runners yield 0.61× (glibc frees `Box` cheaply). Gates protect the claim itself (ratio < 1.0) and ablation lower bounds, not ± bands. Measurements showed **the original two candidates cannot win today**: native `fan` is sequential (`fan.map` uses sequential `Rc<dyn Fn>`, `fan { }` uses one thread for the whole block, AutoParallel is dead because it does not inspect `RuntimeCall`), yielding fannkuchredux 0.96–1.06× / mandelbrot 1.01–1.12×; `|>` stream fusion never fires on the Rust target (6.6× in perf README “Not yet covered”). The declaration explicitly says both wins use the same single mechanism. Next routes: actual `fan` parallelism and activating IterChain.

> **2026-10-20 addition**: both “cannot win today” issues above are resolved. Native `fan` is actually parallel, and fannkuchredux became the third VICTORY row (`scripts/perf-ratio-baseline.txt`: 0.326; ablation with `ALMIDE_FAN_SEQUENTIAL=1`: 3.07×). Native `|>` chains lower to counting iterators without constructing the `list.range` `Vec` (30M elements: 6.6× → 1.29×, peak RSS 1.5 MB; perf README “Not yet covered” remeasurement dated 2026-10-20). The latter still has no ratchet row.

<a id="b3別階級-fan--gpu-で桁を変える--1331httpsgithubcomalmidealmideissues1331"></a>

<a id="b3別階級-fan--gpu-で桁を変える--1331"></a>

### B3 (another class): fan → GPU to change orders of magnitude ← [#1331](https://github.com/almide/almide/issues/1331)

`--target wgsl` already exists. Given `fan` is deterministic-data-parallel syntax, a demo lowering the same source to GPU enters another class; no need to fight the CPU champion.

**Victory condition**: on one real workload, the same `.almd` source gives **identical output** on CPU and GPU legs (extending the 3-way oracle), with an order-of-magnitude difference versus single-threaded C.

**Explicit non-goal**: custom optimization passes challenging LLVM instruction selection for general scalar code. Worst return on investment; even Cranelift loses 15–50% to LLVM on numeric hot code.

---

<a id="軸-c-正しさ信頼--現路線が既に最強圏ただし穴が一つ判明した"></a>

## Axis C: Correctness and trust — the current approach is already in the top tier, but one gap emerged

**Current values**: 260 ledger contracts / native-wasm-interp 3-way oracle / Perceus Lean belt / 1,400-line PCC checker (trusted base compressed from 100,000 lines) / nightly fuzzing / 394 wasm_cross fixtures, frontier 0.

External survey conclusion: **no other language project with this combination (differential oracle + executable-evidence-linked contract ledger + theorems + PCC) was found**. In 2026-03, CompCert obtained the first-ever DO-178C/DO-333 compiler-use credit on ATR 42/72, establishing a commercial precedent for certification credit from a proved compiler. There is a real market for the 0.7x–0.8x qualification arc.

<a id="2026-08-13-に判明した構造的な穴--fixture-では塞げない"></a>

### Structural gap found on 2026-08-13 — fixtures cannot close it

The root fix for #1322 revealed: **native-v1 and wasm share almide-mir, so a MIR bug can make both legs wrong identically while all 394 parity fixtures remain green**. A scalar let-alias miscompile dormant since v0.57.0 had actually bypassed every gate this way. Only comparison against the v0 (`--no-verified`) leg found it.

**Consequences (most important items on this axis)**:
1. **Keep v0 and almide-interp as cross-examiners**. Verification without independently descended implementations is inherently blind to shared-layer bugs. Before retiring v0, decide who takes this role. But interp abstains often (cross-target corpus participation 281/402 = 69.9%); abstention is not a verdict. **For scalar Int control flow, the successor is already settled**: the constructive oracle below needs no second implementation, so cross-examination survives v0 retirement for this scope. Other areas (strings, floating point, collections, effects) remain undecided; v0/interp remain their only cross-examiners.
2. **Generative differential fuzzing (Rustlantis-style) over linked IR** ([#1332](https://github.com/almide/almide/issues/1332)). Fixtures catch regressions, generators catch unknown divergence. Rustlantis found 22 bugs (LLVM 12 / Cranelift 1). **First delivery (2026-08-13)**: **identity family** in `tools/xtarget-fuzz`: construct programs backward from the answer, embedding expected output as a literal in source. It can judge a **single leg**, not just a difference, so it can convict both legs even when they agree (the only judgment effective against the above structural hole). **Second delivery (2026-09-06)**: **composition family** with the same construction-time oracle. Sample the Cartesian product of operators (`??` `!` `?` `|>` match, tuple extraction) × payload classes (Int / String / heap-containing tuples / records) × positions (tail / let binding / call argument), round-trip literal K and print K. This covers the operator-interaction bug families (#1904 #1942 #1926 #1873 #1925) across the space where handwritten specs fill only touched cells. Deterministic sample: `spec/wasm_cross/composition_fuzz_batch.almd` (C-347). Soundness is constructive: each block has its inverse, a generation-time correction constant, or balanced branch arms. A conservative bound on `|acc|` additionally makes the sole truncating operation (closing `*m` / `/m`) exact. Coverage is scalar Int control flow only (strings/floats/collections intentionally excluded: no cheap inverse). **Initial measurement: 2,080 programs / 0 findings** (seed 13322026081301, 25 minutes, reject 0 / wall 1). This is **measurement with a sample size**, not a correctness proof; it incidentally detected one wasm subset debt (`int.bxor` conditionally unlinked). Next is not strings/floats, but fixing one nightly shard to `--family identity` to establish **a floor on oracle-backed coverage**.
3. The 0.73 translation-validation certificates act precisely at this layer (the Alive2/Crocus pattern).

**Ledger rows**: 0.71–0.79 (proof coverage → Critical profile), 0.81–0.89 (qualification-kit productization), program tracks #577/#579 (corporate/authority relations).

---

<a id="軸-d-llm-writability--ホームグラウンド階級の定義者になる"></a>

## Axis D: LLM writability — home ground, define the class

**Current values**: MSR 100% (30/30, Sonnet 4.6, almide-dojo). MiniGit Bench (same model `claude-sonnet-5`, 20 trials, same prompt/harness, 2026-07-15): **20/20 passed, 233 LOC smallest among 5 languages, shortest agent completion time** ([comparison](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/README.md#same-model-snapshot-2026-07)).

**Outside landscape**: agentlanguages.dev catalogs about 40 AI-oriented languages. Design ideas converge (unique spellings, explicit types, JSON diagnostics, contracts), but **only Almide has an end-to-end “does the edit survive?” metric**. There are zero academic adoptions of “modification survival rate”.

**Victory condition (0.59)**: MSR **becomes the arena**: third parties can reproduce using the public harness, and other languages can be measured under identical conditions. As long as only we measure the metric, it is a claim, not a ranking.

**Effective investments (with empirical support)**:
- **The loop beats the corpus**: Idris zero-shot 22/56 → compiler-error loop 54/56. Qwen3-4B 18.0% → 97.4% with compiler access. Lack of training data cannot disappear at pass@1 but can at MSR.
- **Feedback-quality ordering**: combined 63.6% > test failures 57.9% > minimal 53.1% > **raw compiler errors 49.2%** → #1312 (fix-its with applicability) and #1313 (structured test failures) directly affect MSR.
- **Syntax-constrained decoding**: best accuracy/cost for low-resource languages (CangjieBench) → #1310 (publish Lark/EBNF).
- **Nobody has measured effects' impact** → dojo#2 A/B will supply the literature's first numbers.

**Honest counterevidence**: in mame/ai-coding-lang-bench, dynamic languages (Ruby/Python/JS) are fastest/cheapest at prototype scale; retrofitted type checking costs 1.6–3.2× time. **We cannot win initial implementation speed.** Thus the claim must be edit survival, not initial speed, and needs the 0.59 numbers.

---

<a id="軸-e-ツールチェーン-dx--ほぼ手中"></a>

## Axis E: Toolchain DX — nearly secured

**Current values**: a single binary contains `run/build/test/check/fmt/compile/add/deps/dep-path/clean/install/self-update/
explain/docs-gen/fix/ide/repl` and **built-in LSP** (hover / completion / documentSymbol / formatting / definition / signatureHelp / codeAction). Playground, VS Code extension, tree-sitter grammar, one grammar source.

The 2026 table stakes (Gleam/Astral/Zig/Deno model: fmt/test/LSP/pkg in one binary) are **already met**.

> **Correction record**: the external research artifact dated 2026-08-13 called LSP the largest gap against 2026 table stakes. This was an error: the researchers did not know `src/cli/lsp.rs`. LSP is already built in.

**Remaining rows (agent-facing surface)**:
- ~~#1312 applicability-tagged fix-its → diagnostic-driven `almide fix`~~ — **landed** (2026-09-13 check: `Applicability` exists but only 4 codes support machine application; remainder: [arena-breakthroughs.md T3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md) / #2149).
- ~~#1313 structured `almide test` failures (expected/found diff + `--json`)~~ — **landed** (`test_report.rs`, one `--json` object per failure).
- ~~[#1333](https://github.com/almide/almide/issues/1333) **MCP server / Claude Code plugin definition**~~ — **landed**: `almide mcp` (stdio MCP, 5 tools), `tools/claude-plugin/` (MCP + LSP), `.claude-plugin/marketplace.json`. Each tool only reads existing CLI JSON output through a subprocess; no MCP-specific output path ([docs/mcp.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/mcp.md)). #1313 test-failure details and #1312 applicability tags remain unstructured.
- ~~#1314 built-in snapshot testing~~ — **landed** (`testing.assert_snapshot`, `--update-snapshots`, `--ci`).

---

<a id="軸-f-エコシステム--唯一工学では勝てない軸"></a>

## Axis F: Ecosystem — the one axis engineering cannot win

**Current values**: MVS + git dependencies + lockfile (2026 formal analysis reaffirmed MVS polynomiality). But install/build are not code-execution-free: `native/`, `[native-deps]` and its build scripts, and `@extern` are trust boundaries. Dependency `[permissions].native` and install `--allow-native` cover different scopes. See [CLI install](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md#almide-install) / [package system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md); do not claim a general structural impossibility of any particular worm route. No registry. The stdlib **self-hosts** 41 modules / 959 functions.

**Why engineering cannot win**: library volume is a function of history, not something compiler quality can buy. The day Almide has 10,000 packages will not arrive by conventional means.

**What to compete with: compounding the thesis**. If Almide's thesis is right, **agents can mass-produce libraries**. The rehearsal is done (41 self-hosted stdlib modules). Next is the pipeline “external packages written by agents, passing MSR gates, then published”. This is an asymmetry other languages cannot create (their agents depend on training data; more training data correlates with more existing libraries, hence no need for newcomers).

**Victory condition (after 1.0)**: a registry and a working publication pipeline “agents write, gates pass, humans approve”, with 10 external packages published through it.

**Until then**: endure until niche belts (axes C/D) bring customers. Qualification (axis C) is **a market not chosen by ecosystem size**, and will be the first revenue source.

---

<a id="依存順序--なぜこの順か"></a>

## Dependency order — why this sequence

```
軸D (MSR土俵化) ──┐
軸E (agent面)  ──┼→ 0.5x: 指標と道具を先に固める（測れないものは改善できない）
軸C (ファザー) ──┘

軸A (cranelift) ──→ 編集ループの当初案（旧0.6x割付、現在の出荷予定ではない）。flat IR規律(#1316)が前提
軸B2 (意味論最適化) ─┘     RcCow(0.55-56)で表現を確定してから

軸C (証明・資格化) ──→ 0.7x-0.8x: codegen が安定してから積む

軸F (エコシステム) ──→ 1.0以降: 1.0 の信用が前提
軸B3 (GPU別階級)  ──┘
```

Preserved diagram labels: D (make MSR the arena), E (agent surface), C (fuzzer) → 0.5x: establish metrics/tools first; unmeasurable things cannot be improved. A (cranelift) → original edit-loop proposal, old 0.6x allocation, not current shipping schedule; flat IR discipline (#1316) prerequisite. B2 (semantic optimization) joins after representation is fixed with RcCow (0.55–56). C (proof/qualification) → 0.7x–0.8x after codegen stabilizes. F (ecosystem), B3 (GPU class) → after 1.0, requiring 1.0 credibility.

Same principle as the ledger: **“instrumentation before surgery.”** One additional rule: **do not start an axis whose victory condition cannot be numerical**. Not “make it fast”, but “10k lines, p95 < 100ms”; not “win performance”, but “below 1.0× on named workloads”.

---

<a id="明示的に降りる軸"></a>

## Axes explicitly abandoned

| Axis abandoned | Reason |
|---|---|
| Winning zero-shot / pass@1 | Training-data volume dominates. Dynamic languages are fastest/cheapest at prototype scale (mame measurements). The arena is MSR. |
| Beating LLVM in general scalar codegen | Nobody has achieved it. Even Cranelift loses 15–50% on numeric hot code. Invest in semantic optimization instead. |
| Adopting MLIR | Trading pure-Rust `cargo build` for C++ monorepo dependencies and unstable APIs. Chosen only by languages whose core value is heterogeneous computing. |
| Fine-grained query engine (salsa-class) | Migration tax in matklad 2026-02 / Pyrefly / rust-analyzer. Coarse granularity suffices for explicit-import languages. |
| Concurrency model premised on stack switching | Not standardized. WASI 0.3 async is the answer. |
| Huge stdlib | Heavy freeze cost (retain GRAND_PLAN judgment). |

---

<a id="この文書の維持"></a>

## Maintaining this document

- If an axis's victory condition changes, **update its decade in the ledger (ROAD_TO_1_0.md) in the same PR**.
- When new numbers arrive, update current-value fields and name sources (README generated block / BENCHMARKS.md / dojo).
- Write “won” **only when a CI gate or ratchet protects the claim**.
