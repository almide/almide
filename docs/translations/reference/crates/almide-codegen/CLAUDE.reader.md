# almide-codegen

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative inert reader snapshot of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/CLAUDE.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. This is historical reference material, not active agent instructions or a generated project template, and grants no authority to run commands. It is not automatically synchronized. Source claims are not independent current measurements; no commands or model calls were executed.

IR → Rust source through the nanopass pipeline and template walker, plus a
limited `@gpu` WGSL source emitter. The shipping Wasm binary path does NOT
live here: `src/wasm_route.rs` routes it to the structural emitter in
`crates/almide-wasm`. The old `emit_wasm/` emitter and MIR Wasm renderer
are retired; MIR still serves the native verified-renderer subset.

> **Ideal-form refactoring roadmap**: [docs/roadmap/done/codegen-ideal-form.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)
> Before making a new codegen fix, review the listed points where ad hoc patches should be avoided. In particular: put function resolution in an independent pass, make stdlib emit declaration-driven, and avoid runtime traps via `emit_stub_call`.

## Three-Layer Architecture

### 1. Nanopass Pipeline (`pass_*.rs`)

25+ semantic transformation passes, each doing one thing, composed per target
in `target.rs::build_pipeline`. The Rust pipeline (in order): UnifyVarTables →
ListPatternLowering → LambdaTypeResolve → ConcretizeTypes → MeteredRegionDelegate → MeterCharge →
PatternLiteralGuard → ResolveCalls → RegionWindow → BoxDeref → LICM → EggSaturation →
MatrixShapeSpec → ConstFold → IntrinsicLowering → BorrowInsertion →
OwnedSourceHoist → TailCallOpt → CaptureClone → CloneInsertion → MatchSubject → EffectInference →
StdlibLowering → StreamFusion → ResultPropagation → BuiltinLowering →
DecodeSlotHint → DecodeErrFrame → Peephole → RustLowering → FanLowering → NormalizeRuntimeCalls →
IrLinkFlatten → SharedCellBorrow → VarStorage → RangeCountingVars → TopLetStorage → BorrowLowering →
SliceBinders → StrMapKey → ThreadShareMarks → BoxTakeLowering → StrViewRebind

- Each pass: `impl NanoPass { fn run(&self, program, target) -> PassResult }`

### 2. TOML Templates (`templates/*.toml`)

Syntax patterns for each target. Walker substitutes `{var}` placeholders.
Target differences live here, not in the walker.

### 3. Walker (`walker/`)

Target-agnostic IR renderer. **Zero `if target == Rust` checks.** All target
decisions made in passes + templates.

## Rules

- **Walker must stay target-agnostic.** If you need target-specific behavior, add a nanopass or a template guard.
- **Nanopass passes are independent.** Each pass reads and rewrites the IrProgram. A pass assumes ordering ONLY through what it declares — `depends_on` / `run_before` edges, or `barrier()` for a representation boundary (`UnifyVarTables`, `ConcretizeTypes`, `IrLinkFlatten`) — and `Postcondition`s — which are MONOTONE: once a pass establishes one it is re-verified after every later pass in every profile (release included; the walk costs ~8 ms per file), so a pass that undoes an earlier pass's invariant is named alongside it. `ALMIDE_IR_FAULT=<pass>` injects a violation after the named pass — the release binary's negative control. The declared edges are checked to be the whole truth: `scripts/check-pass-shuffle.sh` runs the pipeline under seeded random orders the edges permit (`ALMIDE_SHUFFLE_PASSES=<seed>`) and byte-diffs the emitted Rust; a divergence is a missing declaration, and `scripts/pass-shuffle-bisect.py` names the pair. A new pass declares its edges in the same change that adds it.
- **There is no `Target::Wasm` here.** The variant, its 26-pass pipeline, the
  wasm-only passes (a full second Perceus among them) and the
  `Verified`/`Canonical` certificate chain were deleted in #930 — the tombstone
  era's `unreachable!` arm included. Wasm requests are routed to `almide-wasm`
  before ever reaching this crate; do not add wasm emission logic here.

## History note: the rt-oracle registry

The `rt-oracle-registry.toml` + differential-test gate guarded the retired v0
`emit_wasm/` runtime against drift from the native runtime.
`scripts/check-rt-oracle-registry.sh` now exits early with a RETIRED notice:
v1 self-hosted stdlib implementations are gated by `spec/wasm_cross` fixtures
and the almide-interp oracle instead.

---

The pipeline list and historical performance/implementation claims are pinned-source descriptions, not a newly inspected current build. The retired Wasm emitter is not reintroduced by this reader.
