<!-- description: Stage 2 progressive lowering — dialect walker passes all 227 spec tests -->
<a id="mlir-backend--egg-rewrite-engine"></a>

# MLIR Backend + Egg Rewrite Engine

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](mlir-backend-adoption.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](mlir-backend-adoption.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](mlir-backend-adoption.zh-CN.md)

> Non-normative historical reader snapshot. Pinned original unchanged and authoritative; no automatic synchronization or fresh implementation, benchmark, novelty or external-project validation. Opening On Hold status governs the old plan: Stage 1 milestones and PoC evidence do not establish full walker migration, GPU support or completed PGO. Old version dates, test counts and pass-removal reports are historical, not current deadlines or a complete pass inventory. Best/first/optimal ambitions and predecessor claims are attributed source text, not newly substantiated superiority. Diagram code stays literal, with translated labels outside fences. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/mlir-backend-adoption.md)

> **Status: On Hold** — egg rewriter shipped (Stage 1 done), dialect backend deferred.
> Stage 1 (egg equality saturation): ✅ Done (0.16.0 historical milestone).
> The current native pipeline includes EggSaturationPass and target-specific
> StreamFusion; the old replacement/line-count report is not a complete current pass inventory.
> Stage 2 (dialect): PoC complete (SSA lowering, Rust/LLVM emit, 7 equiv tests). Full walker migration deferred.
> Stage 3-4 (GPU/PGO): on-hold.

<a id="decision"></a>

## Decision

Almide aims to build “the world's best dispatch infrastructure” on two pillars:

1. A front-end optimizer applying an **egg (e-graph) equality-saturation rewriter** at the Almide dialect level.
2. **MLIR progressive lowering** as the back end, formally handling Rust/WASM/native/GPU targets.

First complete the Stdlib Declarative Unification arc ([stdlib-declarative-unification.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md)). Declarative rules and typed intrinsics translate directly into e-graph rewrite rules / MLIR FunctionImport, preserving that investment.

<a id="全体アーキテクチャ"></a>

## Overall Architecture

```
Source (.almd)
     ↓
AST + Type Check (既存)
     ↓
Almide IR (既存、強化)
     ↓
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Front-end optimizer: egg e-graph equality saturation
  - 宣言的 rewrite rule (@rewrite attribute から自動変換)
  - algebraic identities, stream fusion, monad laws
  - cost function (target-aware: Rust/WASM/GPU で重みが違う)
  - 等価クラス全探索 → 最適形を extract
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     ↓
Almide dialect (MLIR)
     ↓
  ┌────────── Rust emit (direct, LLVM 経由しない) ──→ Rust source
  │
  └─ Progressive lowering (MLIR pass manager)
       ↓ affine / arith / memref / scf dialects
       ↓ LLVM dialect  ──→ native binary
       ↓ wasm dialect  ──→ WebAssembly binary
       ↓ SPIR-V / NVVM ──→ GPU kernel
```

Diagram annotation translations: AST/type checking already exist; Almide IR exists and is enhanced. Declarative rewrite rules are automatically converted from rewrite attributes. Target-aware costs have different weights for Rust/WASM/GPU. Explore equivalence classes and extract the best form. Direct Rust emission does not go through LLVM.

**Important boundary**: the source places egg at/below the Almide dialect, at the high level. MLIR passes lower from that dialect. Their responsibilities are clearly separated.

<a id="なぜ-egg--mlir-の組合せか"></a>

## Why Combine egg and MLIR?

<a id="egg-単体ではダメな理由"></a>

### Why egg Alone Is Insufficient

- egg is only a rewrite engine. Delegate target-specific back-end optimization—vectorization, inlining, register allocation—to LLVM.
- Writing GPU code generation in egg is impractical; use existing toolchains for SPIR-V generation.

<a id="mlir-単体ではダメな理由"></a>

### Why MLIR Alone Is Insufficient

- MLIR's PDL (Pattern Description Language) uses priority-ordered rewriting, not equality saturation: **retain all equivalent forms simultaneously, then choose the best**.
- Confluence and phase-ordering issues remain, such as fusion before inlining or the reverse; egg addresses these structurally.

<a id="両者を組み合わせる理由"></a>

### Why Combine Them?

- egg discovers the optimal **form** through algebraic search.
- MLIR generates optimal **code** through target lowering and LLVM integration.
- This division is intended to make pure Almide code lower to optimal forms on every target.

<a id="先行事例"></a>

### Precedents

- **Cranelift ISLE**: described in the source as using egg-style e-graphs for instruction selection, Rust native.
- **Herbie**: uses egg for exploring numerical-expression equivalence.
- **egglog / eggcc**: research on compiler optimization through e-graphs plus datalog.
- **torch-mlir**: multi-target lowering of PyTorch through MLIR.

The intended position is to be “**the first serious realization of egg + MLIR integration as a language compiler**,” at the intersection of research and practice.

<a id="rust-source-output-option-b-採択"></a>

## Rust Source Output: Option B Adopted

<a id="選択肢の比較"></a>

### Comparing the Options

**Option A**: abandon Rust source output and consolidate on MLIR → LLVM → native/WASM.
- Lose readable emitted Rust and source-level Cargo-ecosystem integration.
- Gain full MLIR power and reduced complexity.

**Option B (adopted)**: emit Rust directly from the Almide dialect, not through LLVM.
- Retain an Almide dialect → Rust source walker for the Rust target.
- Use MLIR progressive lowering for WASM/native/GPU.
- Complexity: two emission paths.
- Benefit: retain existing trust and debuggability.

Rust source output is integral to Almide's mission of being “the language LLMs write most accurately.” Readable emission for LLMs/users, source-level Rust-crate interop, and Cargo as shared infrastructure outweigh the unified aesthetic of egg + MLIR.

<a id="サブステージ-stage-1-4"></a>

## Substages, Stage 1–4

<a id="stage-1--egg-rewriter--almide-dialect-3-ヶ月--0160"></a>

### Stage 1 — egg Rewriter + Almide Dialect (3 Months / `0.16.0`)

<a id="stage-1a--egg-rewriter-単独駆動--完了-2026-04-19-pr-217-merged-on-develop-636b8540"></a>

#### Stage 1A — egg Rewriter Alone ✅ **Complete** (2026-04-19, PR #217 Merged on develop `636b8540`)

Stage 1A completed in pure Rust without FFI. Two imperative fusion-pass layers were removed, making egg the sole fusion driver in this historical report.

- [x] Add dependency `egg` to `almide-egg-lab` and `almide-codegen`.
- [x] Expose Almide IR through egg's `Language` trait (`AlmideExpr`: list + matrix = 13 + 6 variants).
- [x] Compile stdlib `@rewrite` declarations into egg rules, autogenerated via `crates/almide-egg-lab/buildscript/egg_rules.rs`, stdlib as single source.
- [x] Cost function v1: atomic 200 / fused 110 / compose 1; extractor always chooses fused form.
- [x] Insert equality saturation at the beginning of Nanopass.
- [x] Matrix bridge (IR ↔ RecExpr): round-trip with 13 variants, slot opaque leaves, and let-split inlining.
- [x] List bridge: map/filter/fold/flat_map/filter_map and beta reduction for four compose markers (`compose-fold` / `compose-flatmap` / `compose-map-filter` / `compose-fm-fold`).
- [x] Remove MatrixFusionPass (`pass_matrix_fusion.rs`, `pass_matrix_fusion_rules.rs`, and generated files).
- [x] Remove StreamFusionPass, five files in `pass_stream_fusion/`.
- [x] Also remove `--opt egg`, `ALMIDE_OPT_EGG`, and `CodegenOptions.opt_egg`; egg is always enabled by default.
- [x] spec/ 218/218 green, net -1183 lines, PR #217.

**Stage 1A remainder, to reintroduce with Stage 4 PGO**:
- [ ] Reintroduce fma/fma3 optimization through egg. Requires schema extension to autogenerate coefficient-aware Appliers from `@rewrite`. Reported as not affecting Transformer/Llama 1-block benchmarks, which do not use those chains.

<a id="stage-1b--mlir-dialect--完了-2026-05-11-option-b-採択"></a>

#### Stage 1B — MLIR Dialect ✅ **Complete** (2026-05-11, Option B Adopted)

Implement a pure-Rust dialect schema as crate `almide-dialect`, without MLIR FFI.

- [x] Create `almide-dialect` as workspace member.
- [x] `types.rs`: `DialectType` enum and `Ty→DialectType` converter covering all Almide types.
- [x] `ops.rs`: `OpKind` with 30+ operations (const/binop/call/match/lambda/fan/for/while/record/list/map/result/option/computed_call).
- [x] `lower.rs`: `IrProgram → Module` SSA lowering (ValueId/BlockId/Terminator).
- [x] `verify.rs`: well-formedness checker for undefined values and Unknown types.
- [x] `dump.rs`: MLIR-like textual pretty-printer, `almide emit --emit-dialect`.
- [x] `emit_rust.rs`: dialect → Rust emitter with NameMap, use-count clone, borrow insertion, variant qualification.
- [x] `emit_llvm.rs`: dialect → LLVM IR through Inkwell, feature-gated `llvm = ["inkwell"]`, requiring LLVM 22.
- [x] CLI: `--emit-dialect`, `--target dialect` (Rust with runtime), `--target llvm` (LLVM IR).
- [x] Seven equivalence tests pass: variants, list ops, Result match, closures/HOF, records, arithmetic, maps.
- [x] CI green; LLVM is opt-in and absent from default-build dependencies.

Known gaps:
- [ ] Stdlib borrow insertion uses module/function-name heuristics, not types.
- [ ] Dialect emitter does not handle test blocks, `test "name" { }`.
- [ ] LLVM back end supports only i64/f64/bool, not yet String/List/Map/records.
- [ ] Dialect emission skips convention methods, Type.method.

<a id="stage-2--progressive-lowering--rust-emit-再配線-partially-done"></a>

### Stage 2 — Progressive Lowering + Rewire Rust Emission, Partially Done

- [x] Dialect → Rust source with embedded runtime: `codegen_dialect()` + `embed_rust_runtime()`.
- [x] Simple programs compile and run correctly through the dialect pipeline.
- [ ] Lower Almide dialect to arith/scf/memref, deferred to melior adoption.
- [ ] Fully migrate existing walker through the dialect, gradually and conditional on the full test suite passing.
- [ ] Reimplement nanopass pipeline on the dialect: StdlibLowering, BorrowInsertion, etc.

<a id="stage-3--wasm--native--gpu-3-ヶ月--0170"></a>

### Stage 3 — WASM / Native / GPU (3 Months / `0.17.0`)

- LLVM back end → native binary (Linux/macOS AOT): Inkwell `emit_object()` implemented; stdlib-runtime integration remains.
- LLVM back end → WASM: experiments possible with Inkwell `target-webassembly` feature.
  - Benchmark against existing direct WASM emission.
  - Direct emission has a light runtime, about 10KB; retain the decision whether to keep a hybrid.
- **First GPU PoC**: offload `matrix.multiply` through SPIR-V or NVVM dialect.
  - Schedule DSL works as `@schedule(device=gpu)`.
  - Expose `almide build --target cuda matrix_kernel.almd` behind an experimental flag.

<a id="stage-4--安定化と最適化-2-ヶ月--017x"></a>

### Stage 4 — Stabilization and Optimization (2 Months / `0.17.x`)

- Extend egg's cost function to profile-guided optimization, PGO.
- Integrate LLVM standard optimizations—inline, LICM, vectorize—into the default pipeline.
- Remove all imperative Nanopasses after migration to egg rules and MLIR patterns.
- Measure pre/post-arc differences with dojo MSR and confirm zero regressions.
- The old plan to default-enable `--opt egg` is already complete in the Stage 1 record; distinguish it from remaining PGO/MLIR research.

The initial estimate totals **10 months**, a historical estimate, not a current deadline: Stdlib Unification in 0.15.0, egg + MLIR foundations in 0.16.0, GPU PoC in 0.17.0.

<a id="stdlib-unification-arc-との関係"></a>

## Relationship to the Stdlib Unification Arc

This is also the **payoff stage** for [stdlib-declarative-unification.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md):

| Stdlib Unification output | Treatment in this arc |
|---|---|
| Pure Almide body in `stdlib/<m>.almd` | Input to the Almide dialect |
| Typed intrinsic (`@intrinsic(rust=..., wasm=...)`) | Expressed as MLIR FunctionImport |
| Declarative `@rewrite` rule | **Automatically compiled** to an egg rewrite rule |
| `@schedule` block | Converted to an affine-dialect schedule attribute |

Stdlib Unification prepares the **input format**. Skipping is possible, but the source estimates twice the MLIR-porting work if stdlib retains three-layer definitions. Keep the order.

<a id="非目標"></a>

## Non-goals

- **Do not reproduce Rust's borrow checker in MLIR**; let rustc check Rust-path borrowing.
- **Do not import the whole MLIR ecosystem**; TensorFlow dialect, Torch-MLIR, CIRCT, etc. are out of near-term scope.
- **Do not expose LLVM IR as Almide's user-facing surface**; LLVM dialect is internal only.
- **Do not expose egg in frontend compilation**; users write rules with `@rewrite`; egg stays internal.

<a id="未決事項"></a>

## Open Questions

1. **egg Language-trait design**: embedding Almide IR into egg nodes; single `Expr` type or split by node kind?
2. **Cost function v1**: start from node count plus target coefficients; profile-guided work waits for Stage 4. Allow user-defined costs in between?
3. **TableGen versus builder API** for the Almide dialect: pure-Rust builder API is faster initially; reassess TableGen once stable.
4. **Future of direct WASM emission**: compare LLVM-WASM performance in Stage 3, then decide hybrid versus replacement.
5. **First GPU runtime target**: CUDA, ROCm, Vulkan, WebGPU. WebGPU naturally extends WASM but the source judges ecosystem maturity low.
6. **Equality-saturation termination**: egg uses iteration limits; observe acceptable timeouts in Almide measurements.
7. **Incremental-compilation integration**: design the salsa-style / MLIR-pass-manager boundary.

<a id="成功判定"></a>

## Success Criteria

- `almide build --target cuda matrix_multiply.almd` works in `0.17.0`.
- Zero Rust/WASM regressions: all spec/ tests pass and dojo MSR unchanged.
- Zero three-layer stdlib definitions, alongside Stdlib Unification completion.
- `cargo build --release` compilation speed within ±20% of the baseline, without degradation through egg + MLIR.
- Demonstrate in benchmarks that egg equality saturation discovers strictly more optimal forms than existing StreamFusion.

<a id="当初のバージョンスケジュール履歴現在のリリース計画ではない"></a>

## Initial Version Schedule — Historical, Not the Current Release Plan

```
0.14.x         — 前段完了 (v0.14.7 Dispatch Ideal Form)、Stdlib Unification 準備
0.15.0         — Stdlib Unification 完了
0.16.0 ✅      — Stage 1A (egg rewriter) + Stage 1B (dialect schema + LLVM PoC)
                 + bit introspection + Hashable protocol
0.16.x (当初の次段)  — Stage 2 continuation: dialect walker を既存テストスイートで検証、
                 borrow insertion の型駆動化、nanopass の dialect 上再実装
0.17.0         — Stage 3 (LLVM native binary / GPU PoC)
0.17.x         — Stage 4 の当初案 (PGO、imperative pass 全廃。egg default 化は Stage 1 で完了)
1.0 前提       — 本 arc 完走
```

Schedule translation: 0.14.x completes the preceding v0.14.7 Dispatch Ideal Form and prepares Stdlib Unification; 0.15.0 completes it; 0.16.0 completes Stage 1A/1B plus bit introspection and Hashable. The originally next 0.16.x phase validates the dialect walker against the existing suite, makes borrow insertion type-driven, and reimplements nanopass on the dialect. 0.17.0 covers LLVM native/GPU PoC; 0.17.x originally covers PGO and removal of imperative passes, with egg defaulting already complete. Completing this arc was a prerequisite for 1.0 in this historical schedule.

The intended endpoint is a compiler where “**pure Almide is reorganized by egg into optimal forms and lowers to optimal code across Rust/WASM/native/GPU**”: the dispatch/optimization endpoint of the “world's best ideal form” ambition.
