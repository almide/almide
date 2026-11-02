Description: Breakthrough ledger for winning every axis — from measured neighboring arenas and reading the mechanisms of 18 neighboring compilers, record where we lose and what breakthroughs would make us world-leading, with victory conditions, gates and order
<a id="全軸制覇への突破点--アリーナと近隣比較から導いた設計指針"></a>

# Breakthroughs for winning every axis — design guidance from arena and neighbor comparisons

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](arena-breakthroughs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](arena-breakthroughs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](arena-breakthroughs.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md)

> Source-history caveat: rankings, competitor mechanisms, star counts, standards/proof descriptions and causal explanations are the source’s dated survey claims, not independent validation or present world-leadership evidence. External almide-references results and file:line citations were not reproduced here. Retain the 2026-10-31 navigation corrections alongside older absent/unfiled claims, the 09-22 Dojo row alongside five-month-old-measurement wording, and October remeasurements alongside original September tables. Debug/default versus release flags, different hosts/hardware, median 7/9 versus mean 21, own-host versus stock-WASI, verified versus wasm-opt, and floor versus stdlib-linked workloads are separate scopes. The original wasm 4/4-smallest wording conflicts with its float-printing loss; T6’s table limit 2.5 KB differs from its later ≤ 2,000 B mechanism, and T7’s < condition differs from later at-or-below wording. These are retained, not silently reconciled. Proposal victory conditions, mechanical-check results and finite comparisons do not establish universal safety, qualifications or current completion. Historical commands and issue-filing instructions are inert documentation.

> **Position**: A **successor delta** to [ALL_AXES.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ALL_AXES.md) (victory conditions on six axes, 2026-08-13). That document is authoritative for axis definitions and “axes we leave”; this one records only **measured rankings as of 2026-09-13** and **what must be overcome** to change them. Decade allocation in the ledger is in [ROAD_TO_1_0.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md).
>
> **Sources** (all measured and dated): `../almide-references/RESEARCH-neighbours.md` §7–§16 (the full-field arena of 2026-09-06–09-11, 15 toolchains); its companion `RESEARCH-championship.md` (2026-08-16, deep reading of nine compilers — **all 18 items revalidated here on 2026-09-13**); deep reading of clones of eight neighboring languages (2026-09-13; file:line citations in this document’s appendix); and additional stdlib-linked-regime measurements (2026-09-13, §2.4).
>
> **This document shares two principles with ALL_AXES**: “Do not start an axis whose victory condition cannot be written numerically,” and “Write that we won only when a CI gate or ratchet protects that claim.” It adds one principle: **put losses and unmeasured cases in the same table**. Unmeasured is not a win.

---

<a id="0-一枚の結論原調査時点"></a>

## 0. Conclusions on one page (at the original survey date)

**Current navigation (checked 2026-10-31)**: The rankings, neighbor comparisons and issue numbers below are survey records from 2026-09-13 (explicit remeasurements are from October), not today’s world rankings. This does not claim reproduction of external `almide-references` results using this repository alone. T2’s `survive` / `apply` have implementation entry points in `src/cli/mod.rs`; T3’s structured repair and mechanical gate exist in `src/diagnostic.rs` / `tests/diagnostic_coverage_test.rs`. T8’s distribution attestation / SHA256SUMS also exist in `.github/workflows/release.yml`, so “absent” below describes the original survey date. That alone does not establish achievement of each T’s numerical targets (such as Dojo 20%). Nor are other proposals, such as `query` or an independently versioned verifier, collectively treated as complete.

**The original survey judged evidence, performance and size to be strengths, but did not establish uncontested leadership including subjects outside the survey. Unmeasured LLM writability means unknown rank, not first place.** The star count (34) is last in the category: a symptom, not a cause.

Defining world leadership as “first place reproducible by third parties on every axis” narrows the breakthroughs to nine: three critical (losing invalidates the thesis), four that secure an advantage, and two defensive.

| # | Breakthrough | Axis | Type | Today’s rank | Victory condition (numbers) |
|---|---|---|---|---|---|
| **T1** | Make MSR the arena — public harness, discriminating task set, same conditions across languages | D | **Critical** | Claims first, **measurements 5 months old** | A third party runs 5 languages under identical conditions; Almide leads, and README carries those numbers with a ratchet |
| **T2** | Survival-delta API — return “will this edit survive?” before applying it | D/E | **Critical** | Nobody has it (1 neighbor has it at the contract layer) | `almide survive` (LSP/MCP/CLI) returns check+test+contract differences, reducing attempts per 1 Dojo loop by 20% |
| **T3** | Structure diagnostics → repair — mandatory repair fields; mechanical application 4/73 → all mechanical | D/E | **Critical** | 2nd (behind rustc), mechanical fixes for **4 codes** | Every code declaring `## Fix-it verdict: mechanical` emits `MachineApplicable`; repair ratchet including silent cases is green |
| **T4** | Stdlib uses its own abstraction machinery — retire 123 `_str` twins through protocols | D/F | Secure advantage | **10th/10** (only one with 0) | `_str` twin ratchet 123 → 0; ≥ 5 `protocol` declarations in stdlib |
| **T5** | Recommended idiom = fastest — extend spelling gates everywhere, ignite fusion, real fan parallelism, Map linear scans | B | Secure advantage | First at the floor, but **recommended forms are 1.4–4.3× slower; stdlib-linked native loses by Map 3.7× / JSON 2.2×** (§2.4) | idiomatic/imperative ≤ 1.15 on all 13 perf rows; CI adjudicates wasm ratchet; 3 stdlib-linked rows under ratchet |
| **T6** | Give size a story — native 488 KB (loses by 7–9×), float printer 3.5 KB, no size ratchet | B | Secure advantage | First on wasm / **middle on native** | `--release-small` with native hello < 200 KB; CI adjudicates wasm size ratchet; float printing ≤ 2.5 KB |
| **T7** | Incremental — nobody reads `.almdi`; native 134 ms loses to Go’s 92 | A | Secure advantage | **10th/10** (only 0) | Rechecking unchanged modules produces 0 IR; native edit-loop p95 < Go build of the same scale |
| **T8** | Portable trust — separate verifier binary, signed distribution, non-silent suppression, intrinsic authority | C | Defensive | First, but **no export format** | `almide-verify` judges certificates with an independent version; one-line install has attestation; authority gate on 798 intrinsic declarations |
| **T9** | Ecosystem asymmetry — public path where agents write, gates pass and humans approve | F | Defensive | **0 issues**; docs stale on hold | 10 external packages through this path (same as ALL_AXES axis F) |

T1–T3 describe the same thing in three stages: **turn “LLMs can write it” into numbers third parties can measure, and embed in the language mechanisms that move those numbers on every edit**. Without T1, T2/T3 are claims; without T2/T3, other languages catch up with T1 (indeed, a neighboring language has explicitly ported the MSR harness: Appendix A-5).

---

<a id="1-今日の順位表2026-09-13全て実測"></a>

## 1. Today’s rankings (2026-09-13, all measured)

<a id="11-アリーナ1314m4-pro出力照合済みmedian-9"></a>

### 1.1 Arena (§13/§14, M4 Pro, output checked, median 9)

> **Note (2026-10-12)**: This table’s Almide native column measured `almide build` without `--release`, so its values use opt-level=1 and no LTO (the rlib fast path in `src/cli/cargo_build.rs`). Peers use Zig ReleaseFast and Rust `-O`. Values remeasured with `--release` are in §1.1a.

**Native, ms** (lower is better)

| | hello | fib(35) | binarytrees d19×12 | spectralnorm n=1200 |
|---|---:|---:|---:|---:|
| **Almide (imperative)** | 2.3 | **15.4** | **51.4** | 39.3 |
| **Almide (recommended idiom)** | — | — | 49.7 | **62.9** |
| Rust | 2.0 | 18.3 | 193.7 | 38.8 |
| MoonBit | 2.0 | 19.1 | 62.2 | **38.0** |
| Zig | 2.2 | 19.7 | 66.5 | 38.1 |
| Go | 2.7 | 23.2 | 63.4 | 54.0 |
| Jacquard (via C) | 1.9 | 50.7 | 61.8 | — |
| Aver (via Rust) | 2.1 | 79.1 | 287.9 | 836.2 |
| Hand-written Rust arena (control) | | | **31.3** | |

Reading: only binarytrees reflects a **mechanism difference** (region window, #1991); other rankings lie within noise. **With the recommended idiom, spectralnorm loses to Go** (§15; #2098 resolved on 09-12 — remeasurement is T5’s first job).

**Stock-WASI, ms / bytes**

| | fib | binarytrees | spectralnorm | hello B (+opt) | fib B | btrees B | snorm B |
|---|---:|---:|---:|---:|---:|---:|---:|
| **Almide** | 37.0 | **67.1** | **82.3** | 1,337 (**407**) | **1,144** | **1,924** | 8,585 |
| MoonBit | 36.9 | 195.3 | 93.5 | 2,522 (2,106) | 3,797 | 3,826 | 11,198 |
| Zig (wasm, §11) | **41–45** | 124–126 | — | 46,640 | 47,651 | 48,132 | — |
| Wado `-Os` (own host) | — | — | — | 2,013 | 2,524 | 2,648 | **7,185** |
| vibe (own host) | — | — | impossible (heap 4.4 GiB) | 519 | **823** | 2,164 | — |

**Native binaries, KB**: Swift 51 (dynamic linking) · **Jacquard 53** · MoonBit 196–219 · Zig 387–404 · Rust 455–475 · **Almide 457–495** · Aver 467–557 · Go 2,373.

**Compilation, ms (one file, cache definitively excluded)**: Aver→Rust source 6 · **Almide→Rust source 23** · **Almide→wasm 33** · vibe→wasm 56 · rustc -O 59 · MoonBit→wasm 61 · go build 92 · **Almide→native 134** · swiftc 135 · Wado 183 · MoonBit→native 304 · Jacquard 682 · Gleam 728 · Grain 1,918 · Zig 3,612.

<a id="11a-対-zig-再計測2026-10-12gcealmide---release"></a>

### 1.1a Remeasurement against Zig (2026-10-12, GCE, Almide `--release`)

Measured on develop `3763aea` (including #3592 fusion and #3597 region handles). Almide native uses `--release` (opt 3, fat LTO, codegen-units=1), Zig 0.17.0 ReleaseFast, Rust 1.99 `-O`, and wasm uses wasmtime 49. Means of 21 hyperfine runs, with every cell’s output checked. x86 is c4-standard-8 (Xeon 8581C); ARM is c4a-standard-8 (Neoverse-V2).

**Native, ms**

| | x86 Almide | x86 Zig | x86 Rust | ARM Almide | ARM Zig | ARM Rust |
|---|---:|---:|---:|---:|---:|---:|
| hello | 0.41 | **0.22** | 0.40 | 0.37 | **0.30** | — |
| fib(35) | 19.80 | **18.92** | 21.26 | 17.64 | **17.25** | 17.86 |
| binarytrees | **61.3** | 212.9 | 232.6 | **66.4** | 232.7 | 224.3 |
| spectralnorm imperative | **48.40** | 59.49 | 59.77 | 50.14 | **49.81** | 50.16 |
| spectralnorm idioms C / D / E | 58.7 / 58.9 / 59.8 | | | 76.9 / 76.9 / 76.9 | | |

**Stock-WASI (wasmtime), ms**

| | x86 Almide | x86 Zig | ARM Almide | ARM Zig |
|---|---:|---:|---:|---:|
| hello | **2.53** | 3.64 | | |
| fib | **31.79** | 32.72 | **43.98** | 49.94 |
| binarytrees | **73.8** | 137.1 | **89.7** | 173.4 |
| spectralnorm imperative | 67.21 | **63.82** | 91.05 | **85.84** |
| spectralnorm idioms C / D / E | 73.5 / 108.7 / 111.8 | | 106.0 / 130.1 / 131.0 | |

**Sizes after stripping (x86)**: Native Almide 293–337 KB, Zig 546–565 KB, Rust 337–363 KB. Wasm (`wasm-tools strip`): Almide hello 325 B / fib 495 B / binarytrees 1,741 B / spectralnorm 8,289 B; Zig 67–84 KB.

Reading:
- Zig is slow on binarytrees because std’s `ArenaAllocator` takes 80% of the time. This is an allocator difference, not a difference in language speed.
- On ARM with `--release`, the three spectralnorm idiom spellings are 1.53× slower than the imperative form. LLVM loop vectorization is the cause: the generic aarch64 cost model vectorizes i64 multiplication by moving it lane by lane. This difference does not appear at opt-level=1 (#3604).

<a id="12-工学的性質championship-18-項目2026-09-13-再検証"></a>

### 1.2 Engineering properties (18 championship items, revalidated 2026-09-13)

| Item | 08-16 rank | 09-13 state | Evidence |
|---|---|---|---|
| Mechanically checked self-proof | 1st/10, uncontested | Maintained | Coq 25 files, 0 Admitted; Lean 3 belts, 0 sorry |
| compile-speed gate | 1st | Maintained | `check-edit-loop-scale.sh` slope 1.13 |
| Verified shipped artifacts | 1st (structurally) | Maintained | Explicitly places wasm-opt outside verification envelope |
| fmt AST-preservation verification | 1st | Maintained | `verify_format` always on |
| MCP / agent surface | 1st (0/9 have it) | Maintained, but neighbors caught up (Appendix A) | `almide mcp`, 5 tools |
| Diagnostic discipline | 2nd | Maintained. **Mechanical fixes for 4 codes / 73** | 6 non-test `with_machine_fix` call sites |
| `list.sort` O(n²) | 10th | **Resolved** | All 6 files use bottom-up mergesort |
| Pattern language (or/as/rest) | 10th | **Resolved** | `ast.rs:110-136` `As` `Or` `List{rest}` |
| MVS | 8th (documentation only) | **Resolved** | `project_fetch.rs:245-313` selects semver maximum |
| Lock format | Hand-written line format | **Resolved** | TOML, quoted |
| Snapshot / `test --json` | Not built in | **Resolved** | `snapshot.rs`, `test_report.rs:223` |
| Ablation leg | None | **Resolved** | `VICTORY_ABLATION_FLOOR`, `ALMIDE_REGION_OFF` |
| **Incremental** | **10th (only 0)** | **Not started** | **0** calls to `read_almdi` |
| **Stdlib protocol use** | **10th** | **Regressed**: `_str` twins 119 → **123** | Ratchet frozen; both 2 retirement proposals at stage 0 |
| LSP breadth | 9th | Partly resolved: 13 methods, INCREMENTAL sync, references/rename present | **1 kind** of code action (Gleam ≈ 60) |
| Doc generator | 8× thinner | `--check` only; 46 hand-written files | #1469 |
| Intrinsic boundary marker | None | Link gate only; **no declaration-authority gate** (798 declarations) | `check-intrinsic-boundary.sh` |
| Native size settings | — | **None** of `strip`/`panic=abort`/`opt-level=z` | `cargo_build.rs:38-45` |

<a id="13-テーゼの軸llm-writability"></a>

### 1.3 Thesis axis (LLM writability)

| Metric | Value | Freshness | Problem |
|---|---|---|---|
| Dojo MSR | 65% (25/38, Llama 3.3 70B) · 44% (17/38, Llama 3.1 8B) | 2026-09-22 | Both runs stamped `comparable` by harness (all 38 tasks reached the model, so rates are points, not intervals); manifest records fixed seed `20260922` and temperature 0 as measured values (`almide-dojo@8af34bc`, no CI/key needed). Sonnet rows are manual measurements under the “no Anthropic key in CI” ruling and are currently absent |
| MiniGit, same model | 20/20, 233 LOC (smallest of 5 languages), **573 s / $3.19** | 2026-07-15 | Rust 297 s / $1.45, Ruby 73 s / $0.36 — **8.9× more expensive, 7.8× slower** |
| Neighbor response | vibe explicitly ported the MSR harness | 2026-09 | We do not monopolize the metric; now is the time to establish the arena |
| Stars | almide 34 · vibe 35 · aver 60 · wado 116 · jacquard 119 · hexa 196 · vera 413 · moonbit/core 1,205 · gleam 21,907 | 2026-09-13 | Last in category |

573 s / $3.19 is precisely the cost of “having a language with no training data written using only the compiler loop.” **T2/T3 are the mechanisms to reduce this cost; T1 supplies numbers showing that it decreased.**

---

<a id="2-原調査時点の敗北と未計測の台帳"></a>

## 2. Ledger of losses and unmeasured cases at the original survey date

The other side of “we won.” Each row names cause, location and responsible issue. Rows without issues are filed in §4.

<a id="21-実測で負けている行"></a>

### 2.1 Rows with measured losses

| Row | Opponent | Gap | Cause (measured) | Issue |
|---|---|---|---|---|
| Native binarytrees | Hand-written Rust arena 31.3 ms | 1.6× | Inferred arena only rewinds the region 1 time; hand-written uses `Vec<u32>` indices | region-inference.md Phase 3 (on hold); #1997 `scoped:` |
| Native spectralnorm (recommended form) | Go 54.0 | 1.16× | Per-closure clone of captured lists + `enumerate` tuple materialization (#2098, resolved). 10-12 `--release` remeasurement: 1.21× on x86, 1.53× on ARM (both relative to its own imperative form, §1.1a) | T5; #3604 |
| Native spectralnorm recommended form (aarch64 `--release`) | Its own imperative form, 50.1 ms | 1.53× | Worsening due to LLVM loop vectorization. Disabling vectorization gives 53–54 ms; `target-cpu=native` gives 52.9 ms | #3604 |
| Wasm fib | ~~Zig 41–45 ms~~ | — | Remeasured 10-12: Almide faster (x86 31.8 vs 32.7, ARM 44.0 vs 49.9, §1.1a). Row retained to show history | #2150 |
| Wasm spectralnorm bytes | Wado `-Os` 7,185 | 1.19× | **Float printer adds +3,531 B** (§16). Math is free through `f64.sqrt` | #2099 |
| Wasm fib bytes (own-host class) | vibe 823 | 1.39× | Different class (own-host import); contents of the gap not yet decomposed | #2150 (class-gap decomposition is T6) |
| Native binary | Jacquard 53 KB | 7–9× | Backend-derived (bare rustc hello 466 KB). `strip`/`panic=abort`/`opt-level=z` not set | #2092 (P-low → promoted) |
| Native compilation | Go 92 ms | 1.46× | rustc in the path. 0.6x (cranelift) is the answer | #1005 |
| Rust source generation | Aver 6 ms | 3.8× | Breakdown of 23 ms outside frontend unmeasured | #2151 |
| Agent cost | Ruby $0.36 / 73 s | 8.9× / 7.8× | Compiler loop compensates for 0 training data | #1963, T2/T3 |
| Stars | gleam 21,907 | 640× | Lack of distribution and story (§3 T9) | T9 |

<a id="22-実測で未計測の行勝ちではない"></a>

### 2.2 Rows without measurements (not wins)

| Row | State | Who fills it |
|---|---|---|
| Speed and size in stdlib-linked regime (Map / String / JSON) | §16 explicitly says “unmeasured in both directions.” Ratchet sees only 7 of 13 rows (#2142); no size ratchet (#2141) | **First measurement in §2.4 completed** → #2154 #2155 #2156 #2157, T5/T6 |
| MSR under identical conditions across languages | Only measuring ourselves | T1 |
| MSR effect of effect declarations | dojo#2 should produce the first numbers in the literature, but not conducted | T1 |
| Cross-language **compile-time** comparison (equal LOC) | Axis A victory condition is only self-relative | T7 |
| Wasm perf holes (fft list-write cliff ~3,500×, mandelbrot ~130×) | Perf README: “needs fixing first” — **no issue** | T5 |
| Linear scans in `AlmideMap` | Perf README: “Not yet covered”; map-data-structure-roadmap treats HAMT as research | T5 |
| Whether recommended idioms match imperative speed on all 13 rows | `IDIOM_CEILING=1.15` applies to only 1 row, listbuild | T5 |
| 3 of 4 programs below 2.7 KB = floor measurement | “Small” is an observation of 4 files | T6 |

<a id="23-近隣が持っていて-almide-に無い機構付録-a-の要約"></a>

### 2.3 Mechanisms neighbors have and Almide lacks (Appendix A summary)

| Mechanism | Holder | Almide state | Destination |
|---|---|---|---|
| Speculative-edit **proof delta** (newly_broken/newly_fixed before applying) | 1 neighbor (custom LSP method) | Absent | **T2** |
| Fuse verification and application into one call (explicit `force`) | Same | Absent | T2 |
| Repair ratchet (`diag.grep` + **FAIL if a diagnostic appears in a silent case**) | 1 neighbor (the one that ported Almide’s MSR) | `tests/diagnostics` has positive cases only | **T3** |
| Required structured diagnostic fields (cause / exactly one next-step / contrast) | 1 neighbor (224 codes) | Hint is a mandatory String; repair is prose | T3 |
| `Repair{primary, alternatives, example}` in JSON diagnostics | 1 neighbor (direct competitor via Rust) | `MachineApplicable` for 4 codes | T3 |
| `explain --diagnostic` with no arguments **enumerates all codes** (mnemonic + state) | MoonBit | `almide explain E0xx` only handles individual codes | T3 |
| LSP features through CLI subcommands (hover/definition/references/inlay-hints) | 1 neighbor | LSP only | T2 |
| Language guide embedded with `include_str!`, idempotent output under owned markers | 1 neighbor | `llms.txt` lives in repository | T9 |
| Non-silent, expiring suppression (re-notify every compile + commit trailer) | 1 neighbor | No suppression syntax itself (good, but decide its future shape first) | T8 |
| Independently versioned certificate-verifier binary | 1 neighbor | Verifier bundled with compiler | T8 |
| Attestation / SHA256SUMS in one-line install | 1 neighbor | Absent | T8 |
| Build-time capability const-fold + DCE (unauthorized capabilities disappear from binary) | 1 neighbor | Check-time deny-all only | T8 |
| Public interface file (`.mbti` / package-interface JSON) | MoonBit, Gleam | `almide compile --json` exists, **not checked in as a diff target** | T7/T9 |
| Generative code actions (missing patterns / decoder generation / filling labels) | Gleam (≈ 60) | 1 kind | T3 |
| Mutation score as public badge | 1 neighbor | `check-mutation-gate.sh` exists; no published value | T8 |
| Hosted registry (account lifecycle in CLI) | MoonBit, Gleam, Wado (OCI) | Git dependencies only | T9 |

<a id="24-stdlib-連結レジームの初回計測2026-09-13"></a>

### 2.4 First stdlib-linked-regime measurements (2026-09-13)

Wrote 3 programs (wordfreq = count 2M words in a Map and take top 10 / strchurn = interpolation→split→trim→to_upper→join for 200,000 strings / jsonround = stringify→parse→Σ for 50,000 records) in Almide, MoonBit and Rust, checked stdout byte equality for all 26 cells, then measured (M4 Pro, median 7, almide 0.62.0, wasmtime 47). Corpus and runner: `../almide-references/bench-sources/stdlib-regime/`; interpretation: RESEARCH-neighbours §17.

**Native, ms**

| | Almide recommended | Almide imperative | MoonBit | Rust |
|---|---:|---:|---:|---:|
| wordfreq | 507 | 404 | **136** | **79** |
| strchurn | **166** | — | 216 | **82** |
| jsonround | 307 | — | **137** | **129** |

**Stock-WASI, ms / bytes (verified → `--wasm-opt`)**

| | Almide | MoonBit (→ `wasm-opt -Oz`) |
|---|---:|---:|
| wordfreq recommended (`group_by`) | **22,016** · 7,472 → 5,647 B | 262 · 16,132 → 11,380 B |
| wordfreq imperative (`var` Map) | **201** · 8,210 → 6,239 B | (same cell) |
| strchurn | **166** · 28,153 → 25,713 B | 726 · 22,010 → 14,564 B |
| jsonround | **132** · 25,575 → 17,704 B | 477 · 71,395 → 51,590 B |

**Reading (four points, all filed as issues)**:
1. **Native reverses §13**: with stdlib linked, Map loses to MoonBit by 3.7×, JSON by 2.2×, and every row loses to Rust by 2–6×. Only strchurn wins. “Among the leaders” referred to floor programs and allocation rows.
2. **Wasm is decisive in the other direction**: 1.3× / 4.4× / 3.6× faster than MoonBit, bytes at 0.5–1.3×. “Small” survives the linked regime; “fast” is a new fact.
3. **The wasm advantage is also an indictment of native**: the same program is 1.5–2.3× faster on wasmtime than native (wordfreq 404 → 201, jsonround 307 → 132). A control loop without Map takes native 163 ms, so native `map.get_or` + indexed assignment takes ~240 ms over 2M iterations — the structural leg’s Map completes the whole program in 201 ms. → **#2157** (native Map index / Value representation #1679).
4. **Recommended spelling is a wasm trap**: `list.group_by` takes ~11 µs per element on the structural leg (22 s versus 201 ms, linear in N, same output) → **#2156**. The tuple sort key `(0 - c, w)` (the only spelling for descending count and ascending word) **walls on the structural leg and miscompiles on the incumbent**: `almide check --target wasm` says "No errors found", build says "verified", wasmtime traps with `indirect call type mismatch` in `list.sort_by_rc`, while native prints the answer → **#2154 (I-divergence, release blocker)**; the wall and unlinked `list.group_by_x` are **#2155**.

**Current status (added 2026-10-20; the table and reading above remain the 09-13 record)**:
- Reading 1’s wordfreq is resolved. Native wordfreq’s ratio to Rust is 0.97 (`scripts/perf-ratio-baseline.txt`; reanchored after #3507: 0.98 on GCE c3-standard-8, 0.92 on c4a-standard-8). jsonround and strchurn versus Rust, and comparisons with MoonBit, have not been remeasured.
- Reading 3 is resolved. Native time ÷ wasm time for the same program is wordfreq 0.46, wordfreq-group 0.39, jsonround 0.69 (GCE c4-standard-8, 2026-10-19, `--quick --runs 9`), so native is faster. `check-perf-ratio.sh` monitors a ceiling of 1.0 in every CI run (machine-dependent variability is #3873).

**Consequences for T5/T6**: Make these three programs ratchet rows in `research/benchmark/perf/` (the very six unmonitored rows named by #2142); these three fill the String / Map / JSON steps of the size ladder (T6). Native’s remaining debt for “dominating every axis” is **Map and Value data structures, not codegen**.

---

<a id="3-原調査からの設計指針実装状態は-0-の現行ナビゲーションも参照"></a>

## 3. Design guidance from the original survey (also consult §0 current navigation for implementation state)

Each item covers **why critical / mechanism / victory condition / gate / dependencies / filing**. Use pointers for areas already covered by roadmaps/issues; put only new decisions here in the body.

<a id="t1-msr-を土俵にする致命"></a>

### T1. Make MSR the arena (critical)

**Why critical.** The mission says “the language LLMs write most accurately,” but only we measured that metric, five months ago. Now that a neighbor has ported the harness, **the role of definer shifts to whoever publishes first**. ROAD_TO_1_0’s 0.59 row still has `—` in the Issue column (no owner).

**Mechanism.**
1. **Discriminating task set** (#1963): MiniGit saturates at 20/20 across five languages. Tasks should be **survival through N specification changes to existing code**, not initial implementation, at a difficulty that separates language success rates. Difficulty metric: “60–80% survive with the strongest model.”
2. **Same conditions across languages**: Make Dojo’s runner language-pluggable (Almide / Rust / Go / TypeScript / Zig / Gleam / MoonBit). Zig’s std changes from 0.15→0.16, causing code written from training data to fail (observed in §7) — this demonstrates that MSR can separate training-data quantity from language stability.
3. **Third-party reproduction**: One command, `git clone && make msr`; model, prompt, temperature and seed in a manifest; result JSON committed to dojo.
4. Bundle **effect A/B** (dojo#2) as one condition in the same harness — the first numbers in the literature.
5. **README numbers are a ratchet**: Add an msr row to `check-readme-numbers.sh`; fail when freshness exceeds 90 days.

**Victory condition.** A table exists where a third party ran at least five languages under identical conditions, Almide leads, and README numbers are no more than 90 days old.

**Gate.** Dojo’s PR gate runs a subset in this repository’s CI (CLAUDE.md “The bridge”).

**Dependencies.** T2/T3 move the numbers. Order: T1 task set → T3 → T2 → T1 remeasurement.

**Filing.** #1963 / #1617 cover tasks and remeasurement. **Public harness (same cross-language conditions / third-party reproducibility)** not yet filed → #2146.

<a id="t2-生存デルタ-api致命"></a>

### T2. Survival-delta API (critical)

**Why critical.** The failures killing MSR are edits applied without verification. One neighboring language has LSP `speculativeEdit` (proof delta before application) and `proposeEdit` (fused verification→application): **MSR made into an API**. Almide lacks this shape, calling `almide check` and `almide test` separately.

**Mechanism.**
1. `almide survive <file> --with <patch-or-full-text> --json` (CLI): apply an edit to the current tree **in memory**, returning `check` (diagnostic differences), `test` (pass/fail differences), and contracts (`// @contract:` fixture differences) as `{unchanged, newly_broken, newly_fixed, removed}`. Do not write.
2. Expose the same function as **custom LSP method** `almide/survive` and **MCP tool** `almide_survive`. The policy for all five MCP tools is to read existing CLI JSON from subprocesses (docs/mcp.md), so CLI first.
3. `almide apply --if-survives` (explicit `force`; non-booleans fail closed): verification and application in one move.
4. `almide query hover|definition|references|inlay-hints --line --column --json`: expose existing LSP handlers through CLI. **Agents know types before editing**, preventing failure instead of diagnosing it.

**Victory condition.** Mean attempts per Dojo task are 20% lower with `survive` than without (same model, A/B).

**Gate.** `schema_version` in the `survive` JSON schema; golden fixtures for expected deltas of three known edits.

**Dependencies.** T3 structured diagnostics form the contents of `newly_broken`.

**Filing.** All unfiled → #2147, #2148.

<a id="t3-診断--修復の構造化致命"></a>

### T3. Structure diagnostics → repair (critical)

**Why critical.** Literature cited by ALL_AXES axis D ranks feedback quality as combined 63.6% > test failures 57.9% > minimal 53.1% > **raw compiler errors 49.2%**. Almide made hints required by type (`hint: String`), but **repair is prose** and mechanical application covers four of 73 codes (E013 E031 E049 E052). Six docs declare `## Fix-it verdict: mechanical`, but the gate only soft-reports.

**Mechanism.**
1. **Make repair a schema field**: `Diagnostic { hint, repair: Option<Repair{primary, alternatives, example}> }`. For codes with a `mechanical` verdict, make `diagnostic_coverage_test.rs` **hard-fail** unless `repair.primary` is `MachineApplicable` (remove the current soft behavior at `:234`).
2. **Repair ratchet (both directions)**: Besides `tests/diagnostics/<code>/repair.grep` (words diagnostics must contain), add `tests/diagnostics/silent/*.almd` — programs that **should compile**. FAIL if a new diagnostic appears. Cover “silently accepted errors” as a family (#2097 family, vera’s right→left slots, jacquard W0301 family).
3. `almide explain --list --json`: list all codes as `code | mnemonic | severity | since | fix-it verdict`. `since` is the version (`_since` table). Agents can orient without knowing a code.
4. Expand **generative code actions** beyond one kind. Among Gleam’s ≈ 60, in order of MSR impact: `Add missing patterns` (same source as exhaustiveness checker), `Fill labels`, `Generate decoder`, `Convert to pipe`. This row determines whether to move on-hold/lsp-code-actions.md back to active.

**Victory condition.** 100% of codes declaring mechanical are mechanically applicable; ≥ 30 silent cases green; `explain --list` row count = distinct `with_code` count (drift gate).

**Gate.** Add repair/silent to `check-diagnostic-code-coverage.sh`.

**Filing.** #2097 is one case. Schema, silent ratchet and `explain --list` unfiled → #2149.

<a id="t4-stdlib-が自分の抽象機構を使う優勢確定"></a>

### T4. Stdlib uses its own abstraction machinery (secure advantage)

**Why.** In the championship’s words, “the codebase’s largest MSR defect.” A model adding `list.chunk_by` must guess which of five representation suffixes to write. Count regressed from 119 → **123**; both retirement proposals (selfhost-link-v2.md, protocol-any-existentials.md) remain at stage 0.

**Mechanism.** No new mechanism here; **decide order and deadlines**:
1. Land selfhost-link-v2 stage 1 (parity harness = falsifier) in **the same PR group as T5’s spelling-gate expansion** (both measure “alternative spellings of the same workload give the same output” and can share instrumentation).
2. Write a shrink schedule **reducing `_str` ratchet baseline by −25% each quarter** beside `scripts/str-twin-baseline.txt` (ratchet enforces decline, not just a freeze).
3. First stdlib appearance of `protocol` starts with **documentation** of `Encode`/`Decode`/`Numeric` (protocol-any pre-step).

**Victory condition.** 123 → 0. ≥ 5 `protocol` declarations in stdlib.

**Filing.** #1460 / #1589 cover this; add shrink schedule as a comment to #1460.

<a id="t5-推奨イディオム--最速優勢確定"></a>

### T5. Recommended idiom = fastest (secure advantage)

**Why.** §15’s lesson: “a style guide is **a promise that measures nothing**.” CLAUDE.md’s recommended spelling was 1.4–4.3× slower, unnoticed because the corpus followed the guide. #2098 is resolved, but the gate covers only listbuild.

**Mechanism.**
1. **Extend spelling gates everywhere**: provide `imperative` and `idiomatic` spellings for every meaningful row among the 13 perf rows, applying `IDIOM_CEILING=1.15` per row. Permanently place §15’s four spectralnorm spellings in `spectralnorm/`.
2. **Adjudicate the wasm ratchet in CI** (#2143): the reason for `RATIO_VERDICT=0` (cross-engine ratios do not cancel hardware) is correct, so judge only **wasm/wasm ratios within the same run** (idiomatic/imperative, fan/sequential, region on/off) — the same nondimensionalization as axis A.
3. **Ratchet coverage 7/13 → 13/13** (#2142): the six Map / JSON / String rows first; move §2.4’s three programs into the perf suite.
4. **Ignite fusion** (#2045, 0.56 row) and **real fan parallelism** (#2044) — existing issues. Fusion first (it determines speed of recommended `|>` chains).
5. **Linear scans in `AlmideMap`**: unfiled. Index native `Vec<(K,V)>` lookups while retaining insertion order (the hash-index side of compact-ordered-dict). Wasm is already O(1). §2.4’s wordfreq row is the judge.
6. **Two wasm perf cliffs** (fft list-write ~3,500×, mandelbrot ~130×): unfiled. Prerequisite for the ratchet.
7. **Wasm fib call cost** (losing to Zig by 1.2–1.3×): unfiled. Decompose structural-leg call shapes through disassembly.
8. **Stdlib-linked regime** (§2.4): wasm constant cost of `group_by` (#2156), native Map/Value (#2157), tuple-sort-key wall and divergence (#2155 / #2154 — the latter is a release blocker and comes first).

**Victory condition.** idiomatic/imperative ≤ 1.15 on all 13 rows; three kinds of within-run wasm ratios can fail CI; three stdlib-linked rows under ratchet; two wasm cliffs ≤ 3×.

**Filing.** #2045 #2044 #2142 #2143 already exist. Map linear scan / wasm cliffs / wasm fib calls → #2150. Linked regime → #2154–#2157.

<a id="t6-サイズに物語を持つ優勢確定"></a>

### T6. Give size a story (secure advantage)

**Why.** Smallest on wasm in 4/4, but **three of four programs are below 2.7 KB**: only the floor is measured. The sole row printing Float loses (§16). Native is 488 KB, losing by 7–9× to a 53 KB neighbor; none of `strip`/`panic=abort`/`opt-level=z` appears in the emit profile (`cargo_build.rs:38-45`).

**Mechanism.**
1. **`almide build --release-small`** (or `[profile.small]`): `opt-level="z"`, `lto="fat"`, `codegen-units=1`, `panic="abort"`, `strip=true`. Bare rustc hello is 466 KB, so **measure first** how far the backend floor can fall (satisfying #2092’s “measured against anything”). Target: hello < 200 KB (MoonBit class). The 53 KB class concerns libc dynamic linking or a `no_std` runtime, joining flight-subset (`--profile critical`) moving to `no_std`; not a target here.
2. **Wasm size ratchet** (#2141): replace the five programs in `research/benchmark/perf/wasm-size/` with a ladder progressively pulling in stdlib — hello / Int printing / Float printing / String operations / List / Map / JSON / regex. Shrink-only for both verified and `--wasm-opt` at every step.
3. **Float printer** (#2099): 3,531 B → ≤ 2,000 B. Write Ryu/Grisu shortest-roundtrip directly for wasm; move Dragon4 to the `--exact` path. The neighboring equivalent adds +4,326 B, so −28% wins.
4. **Unconditional RC helper inclusion** resolved by #1962. Do not include the remaining 251 B of `hello` in the story (leave it aside).

**Victory condition.** Native hello < 200 KB (small profile), all eight size-ladder steps under ratchet, spectralnorm wasm ≤ 7,000 B (below Wado `-Os` 7,185).

**Filing.** #2092 (promote P-low → P-high), #2141, #2099. Add small profile to #2092.

<a id="t7-インクリメンタル優勢確定"></a>

### T7. Incremental (secure advantage)

**Why.** All 9/9 reference compilers have per-module rebuilds; Almide alone has zero. `.almdi` is written, but **`read_almdi` has zero callers**. Meanwhile axis A’s scale independence is achieved (slope 1.13) — **check is fast enough; the slow part is native’s 134 ms including rustc**, losing to Go’s 92 ms.

**Mechanism.**
1. **Coarse granularity is enough** (retain ALL_AXES’s decision: no salsa-class engine). Write an `.almdi` consumer: the near-peer’s four-part predicate (config digest matches ∧ source older than artifact ∧ interface digests of all imports match ∧ artifact readable) + **do not write if interface digest is unchanged** (15-line early cutoff that preserves mtime and stops downstream work). About 26 in-memory unit tests, not process launches.
2. **Native’s 134 ms** disappears only in 0.6x (cranelift, #1005). This document only determines order: **T7-1 (`.almdi` consumption) lands within 0.5x; cranelift stays in 0.6x**.
3. Report **breakdown of the 23 ms Rust source generation** via `--timings` (the 17 ms difference from Aver’s 6 ms could be frontend or render; unknown).
4. Add **equal-LOC cross-language compile times** as permanent arena rows (make §13’s table reproducible with the `bench-sources/` runner and subject to `check-readme-numbers.sh` freshness).

**Victory condition.** Re-`check` of unchanged modules builds no IR at all (confirmed with `--timings`); native edit-loop p95 at or below Go’s same-scale `go build`.

**Filing.** #1003 covers per-module cache, #1005 cranelift. File `.almdi` consumer and `--timings` under #1003 → #2151.

<a id="t8-信頼の可搬性防衛"></a>

### T8. Portable trust (defensive)

**Why.** Evidence is an uncontested axis, but there is **no export format**: verifier bundled with compiler, unsigned distribution, and 798 `@intrinsic` declarations writable from user code too (bypassing the capability story). A direct competitor puts its verifier in **a separate, independently versioned binary**, with `cert verify` only a subprocess shim.

**Mechanism.**
1. **`almide-verify`**: move the PCC checker (1,400 lines) to a separate crate, version and binary. `almide verify` is a shim only, no linked fallback. This makes trust-layer.md L3’s “third party runs `make verify`” hold for binary distributions too.
2. **Attestation in one-line install**: `SHA256SUMS.txt` + GitHub artifact attestation in release.yml; include `gh attestation verify --repo almide/almide` in README’s one-liner.
3. **Intrinsic declaration authority**: `@intrinsic` legal only in modules under `stdlib/` and `runtime/`; elsewhere, a new E-code (hint: “Put intrinsics in stdlib; wrap user code in an effect fn”). `check-intrinsic-boundary.sh` covers linking; this covers declarations.
4. **Decide suppression shape beforehand**: Almide has no suppression syntax (good). If an equivalent of `@allow` arrives, record four conditions in an ADR: **non-silent** (re-notify every compile), **expiring** (`until=`), **mandatory reason**, and **ack through commit trailer**.
5. **Publish mutation score**: stamp `check-mutation-gate.sh` results in `proofs/` and make a README badge.
6. **Build-time capability DCE**: assert through `check-wasi-pins.sh` that capability imports rejected by `--profile critical --allow IO` **disappear** from wasm (currently check-time denial only).

**Victory condition.** Independently released `almide-verify`; attestation verification in README’s one-liner; user-code `@intrinsic` declaration gets an E-code; mutation badge; DCE assertion gate.

**Filing.** All unfiled → #2152.

<a id="t9-エコシステムの非対称防衛"></a>

### T9. Ecosystem asymmetry (defensive)

**Why.** ALL_AXES axis F’s victory condition (agents write, gates pass, humans approve → ten external packages) has **zero issues**, and two on-hold documents are stale. The 34 stars are a symptom of absent distribution. Neighbors embed the language guide in the binary for idempotent output, publish per-commit benchmarks to Pages, and keep skills in a separate repo.

**Mechanism.**
1. **`almide agent-connect`**: embed `llms.txt` and CHEATSHEET in the binary with `include_str!`, writing into **marker regions** of `.claude/skills/almide/SKILL.md` and `AGENTS.md`. Reject inconsistent markers explicitly, without guessing; reruns produce no diff. **Compiler/prompt version skew becomes structurally impossible.**
2. **Public interface files**: check in `almide compile --json` output as `almide.interface.json`, feeding `check-interface-diff.sh` (turn the index already used at release time into a per-PR diff target).
3. **Agent-authored package path**: `almide/pkg-template` + Dojo MSR gate + human approval = publish. Registry **starts as GitHub org + tags** (git refs suffice for MVS). Hosted registry comes after ten external packages.
4. **Per-commit benchmark publication**: plot `research/benchmark/perf/results/*.json` on Pages. Changes in numbers become visible externally.

**Victory condition.** Same as ALL_AXES axis F (ten external packages through the path). Intermediate metric: number of external repositories using `agent-connect`.

**Filing.** `agent-connect` and package path unfiled → #2153.

---

<a id="4-起票2026-09-13"></a>

## 4. Issues filed (2026-09-13)

Issues filed alongside this document (2026-09-13).

| § | Title | Type | Breakthrough |
|---|---|---|---|
| 4-1 #2146 | Public MSR harness: same-condition runs across 5 languages, third-party reproducible, README freshness ratchet | enhancement, mob | T1 |
| 4-2 #2147 | `almide survive`: the survival delta of a proposed edit (check + test + contract) before it is applied, on CLI, LSP and MCP | enhancement | T2 |
| 4-3 #2148 | `almide query`: hover / definition / references / inlay hints as CLI subcommands over the existing LSP handlers | enhancement | T2 |
| 4-4 #2149 | Repair as a schema field, a two-way repair ratchet with silent cases, and `explain --list` | enhancement | T3 |
| 4-5 #2150 | The native Map is a linear scan, two wasm perf cliffs have no issue, and wasm fib pays a call cost the field does not | enhancement | T5 |
| 4-6 #2151 | Consume the `.almdi` interface artifact: the four-part freshness predicate and the unchanged-digest early cutoff | enhancement | T7 |
| 4-7 #2152 | Portable trust: an independently versioned verifier binary, attestation in the install line, and an authority gate on `@intrinsic` | enhancement, proof | T8 |
| 4-8 #2153 | `almide agent-connect`: the language guide shipped inside the binary, written out idempotently under owned markers | enhancement | T9 |

Filed from §2.4 measurements (same day):

| Title | Type | Breakthrough |
|---|---|---|
| #2154 Tuple sort key traps with `indirect call type mismatch` while still "verified" on incumbent; native returns the answer | **I-divergence**, P-high | T5 (release blocker) |
| #2155 Recommended word-count form cannot build on stock WASI: tuple-key wall + unlinked `list.group_by_x` | enhancement | T5 |
| #2156 `list.group_by` costs ~11 µs per 1 element on structural leg (110× imperative) | enhancement, P-high | T5 |
| #2157 Native Map and Value slower than wasm twins (same program 1.5–2.3× faster on wasmtime) | enhancement, P-high | T5 |

Additions to existing issues: #2092 (P-low → P-high, small-profile measurement procedure), #1460 (shrink schedule).

---

<a id="5-順序--何を先になぜ"></a>

## 5. Order — what comes first and why

```
T1 課題集(#1963) ─┐
T3 修復スキーマ+silent ratchet ─┼─→ T2 survive API ─→ T1 公開ハーネス再計測   … 致命の 3 つは一本の鎖
                                 │
T5 綴りゲート全面化 ─→ T4 stage 1(同じ器具) ─→ fusion(#2045) ─→ fan(#2044)
T6 size 梯子 + small profile ─→ float printer(#2099)
T7 .almdi 消費(0.5x) ────────────────────────→ cranelift(0.6x, 据え置き)
T8 / T9 は並走可（コンパイラ本体を触らない）
```

Translation of the preserved diagram: T1 task set (#1963) and T3 repair schema plus silent ratchet feed T2 survive API, then T1 public-harness remeasurement: the three critical items are one chain. T5 comprehensive spelling gates → T4 stage 1 (same instrumentation) → fusion (#2045) → fan (#2044). T6 size ladder + small profile → float printer (#2099). T7 .almdi consumption (0.5x) → cranelift (0.6x, unchanged). T8/T9 can proceed in parallel (do not touch the compiler body).

**Critical chain first.** §1.3’s row gives the reason: 573 s / $3.19 is 4–9 times other languages; without reducing it, “the language LLMs write most accurately” lacks numerical support. Performance, size and edit loop are already among leaders; just avoid losing ground (there are ratchets).

---

<a id="6-降りる軸の再入条件"></a>

## 6. Re-entry conditions for abandoned axes

ALL_AXES’s “explicitly abandoned axes” table had no re-entry conditions. Add them here.

| Abandoned axis | Measurement that triggers reconsideration |
|---|---|
| zero-shot / pass@1 | When a Dojo task set yields **matching** pass@1 and MSR rankings (= pass@1 becomes a cheap proxy for MSR) |
| Beating LLVM at general scalar codegen | When losses due to **the same mechanism** reproduce on at least 2 machines in the arena’s noise-range rows (fib / spectralnorm) |
| MLIR adoption | When `--target wgsl` (#1331) gives output identical to CPU on 1 real workload and another backend becomes necessary |
| salsa-class query engine | When `check-edit-loop-scale.sh` slope exceeds 1.5, or T7-1’s coarse cache cannot bring a full dogfood build below 2–3 s |
| Stack-switching-based concurrency | When WASI 0.3 async is stable and real fan parallelism (#2044) has accelerated native only |
| Huge stdlib | When T9’s external-package path has admitted 10 packages and shows it can absorb 8 tenths (80%) of stdlib addition requests |

---

<a id="7-この文書の維持"></a>

## 7. Maintaining this document

- All numbers are dated. When updating §1 tables, always give the source (RESEARCH-neighbours §number / BENCHMARKS.md / dojo).
- A breakthrough may be called “achieved” only once its §3 **gate** can fail CI.
- If ALL_AXES victory conditions change, update table 0 here in the same PR, and vice versa.
- Remeasure neighbors using the `bench-sources/` runner. **Do not trust compile times that do not state the cache was excluded** (§13’s trap).

---

<a id="付録-a-近隣機構の引用2026-09-13-深読みr--almide-references"></a>

## Appendix A. Neighbor-mechanism citations (deep reading 2026-09-13, `R = ../almide-references`)

Criticizing neighbors by name is not this document’s purpose, so record only **locations of mechanisms Almide can absorb**, with file:line. Stars and tip dates come from `gh api` on 2026-09-13.

**A-1. Direct philosophical competitor (via Rust, Lean certificates)** — `R/aver/`
- Guide embedded with `include_str!`; `agent-connect` writes idempotently under owned markers: `src/main/agent_connect.rs:29-46, 380-395`
- Diagnostics include `Repair{primary, alternatives, example}` / `conflict` / `related` / `intent`: `src/diagnostics/model.rs:63-105`
- 128-row slug catalog (Fires when / Repair columns): `docs/diagnostics-slugs.md`
- NDJSON + `schema_version` + trailing summary: `docs/diagnostics-schema.md:25-37`
- `context --budget 10kb --json` (structured map with token budget): `docs/cli.md:218-247`
- `audit --hostile` (hostile world in a separate slug): `docs/cli.md:210-217`
- Independently versioned certificate-verifier binary; main program is a shim: `docs/cli.md:330-372`, `aver-cert/src/verifier.rs`
- `--explain-passes --json` (per-function alloc-free / TCO can be asserted in CI): `docs/cli.md:315-325`
- Integer division is a function returning `Result`, not an operator: `docs/diagnostics-slugs.md:25`

**A-2. Language making MSR an API at the contract layer (no variable names)** — `R/vera/`
- `speculativeEdit` (proof delta before application): `LSP_SERVER.md:123-153`
- `proposeEdit` (fused verification→application, explicit `force`, fail-closed): `LSP_SERVER.md:155-177`
- `strengthenContract` (caller audit for contract changes): `LSP_SERVER.md:179-193`
- `builtins|effects|errors --json` reads registries directly (documentation counts are lengths): `vera/introspect.py:1-16`
- `since` version on all 158 codes: `vera/_since.py`
- Public Lark grammar + CI gate for “grammar ≡ specification prose ≡ editor grammar”: `scripts/check_grammar_alignment.py:1-30`
- Mutation score 83.3% published: `mutation.json`, `MUTATION.md:1-22`
- Right→left slot numbering creates errors that pass type checking (observed in §13) — removing variable names also removes the name-mismatch detector

**A-3. For “models write, humans review” (OCaml, via C)** — `R/jacquard/`
- 224 codes; never reuse or renumber `(domain, code)`; test that every emitted code is cataloged: `docs/errors.md:1-6`
- Diagnostics are structures: summary / `Cause:` / **exactly one** `Next step:` / `Contrast:`: `docs/errors.md:7-22`
- W0301 (notification of ambiguous binding across value kinds): `docs/errors.md:104`
- Runtime capability `--allow net`; `fs`/`eval` grants rejected by design: `README.md:48-53, 164-166`
- Canonical structural hashes + semantic diffs + content-addressed test cache: `bin/main.ml:2332, 2373`, `docs/warp-testing.md:298-336`
- Multiple-fault simulation (2ⁿ paths) and relational cases: `docs/warp-testing.md:207-297`
- 53 KB native binary (§14)

**A-4. Wasm GC + Component Model + WASI 0.3 only** — `R/wado/`
- LSP features through CLI (`query diagnostics|references|definition|hover|inlay-hints`): `wado-cli/src/query.rs:9-49`
- `SHA256SUMS.txt` + `gh attestation verify` in one-line install: `README.md:29-45`
- Publish to OCI artifacts, workspace-root only: `wado-cli/src/publish.rs:1-26`
- Diagnostic reason-chain design document cites literature (type-error ablation × agent): `docs/wep-2026-06-02-diagnostic-reason-chains.md:1-40`
- Per-commit benchmarks on Pages: `README.md:169-176`
- Already surveyed Almide: `docs/research-language-survey-almide.md`

**A-5. Self-hosted (effect rows), the one that ported Almide’s MSR** — `R/vibe/`
- MSR harness port (explicitly named): `eval/msr/README.md:1-12`
- AI language review with an eight-dimensional rubric every round: `eval/lang-review/rubric.md`
- **Two-way repair ratchet** (`diag.grep` + `silent` FAILs if diagnostics appear): `eval/lang-review/run_repair.sh:1-16`, `repair/09_silent_builtin_arity`
- Hard-fail the “which compiler answered?” trap: `run_repair.sh:24-40`
- Build-time capability-flag const-fold + DCE: `README.md:100-108`
- Executable book (embed output back into Markdown): `README.md:61-66`
- P0 = silently wrong > P1 = crash: `AGENTS.md:23-29`

**A-6. Citation-enforcing, self-hosted (stalled at 2026-07-19)** — `R/hexa/`
- `@grace(code, until=, reason=)` + HX9000 re-notification every compile + `Acked-grace:` trailer: `README.md:109, 333`
- Grammar, pitfalls and changelog in one JSONL SoT: `doc/grammar.jsonl`, `CHANGELOG.jsonl`
- Self-host fixed-point gate: gen3 ≡ gen4 byte equality: `README.md:24`

**A-7. Top two of the small mainstream cohort** — MoonBit (`~/.moon`), `R/gleam/`
- `moon explain --diagnostic` (no arguments lists all codes in 172 rows, mnemonic + state), `--attribute`
- `moon test -u --limit 256`, `--test-failure-json`, `moon check --patch-file`, `moon info` (`.mbti`), `moon prove`
- Gleam: ≈ 60 code actions (`Add missing patterns` / `Generate dynamic decoder` / `Fill labels` / `Convert to pipe`): `language-server/src/code_action.rs`
- Gleam: `export package-interface --out x.json`: `compiler-cli/src/lib.rs:695-700`
- Gleam: exhaustiveness checker shares the same source as code actions: `compiler-core/src/exhaustiveness/`
- Gleam: compile `test-community-packages/` in CI (regression gate against ecosystem breakage)

<a id="付録-b-08-16-サーベイの再検証で見つけた注記"></a>

## Appendix B. Notes found while revalidating the 08-16 survey

- Comments in `stdlib/list_sort_by_str_key.almd:3,104` and `list_sortby_float.almd:4` still say “insertion sort” (implementation is mergesort, #1459). Fix comments only.
- `cargo fmt --check` appears in no workflow (a clippy ratchet exists). Strict on `.almd`, ungated on Rust — opposite of mainstream practice.
- `check-pass-isolated.sh` explicitly names rustc’s `test-mir-pass` as its model but chooses **output identity** (intentional, `:23-28`). It has no MIR golden.
- ALL_AXES axis E’s #1312/#1313/#1314 are all closed. ROAD_TO_1_0 0.4x table’s #928/#999/#1334/#917 are also closed. Both documents annotated in this PR.
