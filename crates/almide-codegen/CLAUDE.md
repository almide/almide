# almide-codegen

IR → source code: Rust (primary) and WGSL (`emit_wgsl/`). The wasm binary path
does NOT live here — it is the v1 MIR trust-spine in `crates/almide-mir`
(the v0 direct emitter `emit_wasm/` was retired in #782).

> **理想形リファクタのロードマップ**: [docs/roadmap/active/codegen-ideal-form.md](../../docs/roadmap/active/codegen-ideal-form.md)
> 新しい codegen の修正を入れる前に、そこに挙げられた「場当たり修正を避けるべきポイント」を確認すること。特に: 関数解決は独立パスに、stdlib emit は宣言駆動、`emit_stub_call` による実行時 trap は避ける。

## Three-Layer Architecture

### 1. Nanopass Pipeline (`pass_*.rs`)

25+ semantic transformation passes, each doing one thing, composed per target
in `target.rs::build_pipeline`. The Rust pipeline (in order): UnifyVarTables →
ListPatternLowering → LambdaTypeResolve → ConcretizeTypes →
PatternLiteralGuard → ResolveCalls → BoxDeref → LICM → EggSaturation →
MatrixShapeSpec → ConstFold → IntrinsicLowering → BorrowInsertion →
TailCallOpt → CaptureClone → CloneInsertion → MatchSubject → EffectInference →
StdlibLowering → AutoParallel → ResultPropagation → BuiltinLowering →
Peephole → RustLowering → FanLowering → NormalizeRuntimeCalls →
IrLinkFlatten → TopLetStorage

- Each pass: `impl NanoPass { fn run(&self, program, target) -> PassResult }`

### 2. TOML Templates (`templates/*.toml`)

Syntax patterns for each target. Walker substitutes `{var}` placeholders.
Target differences live here, not in the walker.

### 3. Walker (`walker/`)

Target-agnostic IR renderer. **Zero `if target == Rust` checks.** All target
decisions made in passes + templates.

## Rules

- **Walker must stay target-agnostic.** If you need target-specific behavior, add a nanopass or a template guard.
- **Nanopass passes are independent.** Each pass reads and rewrites the IrProgram. Passes must not assume ordering except through declared `Postcondition`s.
- **`Target::Wasm` is a tombstone.** Its codegen arm is `unreachable!` — wasm
  requests are routed to `almide-mir` before ever reaching this crate. Do not
  add wasm emission logic here.

## History note: the rt-oracle registry

The `rt-oracle-registry.toml` + differential-test gate guarded the retired v0
`emit_wasm/` runtime against drift from the native runtime.
`scripts/check-rt-oracle-registry.sh` now exits early with a RETIRED notice:
v1 self-hosted stdlib implementations are gated by `spec/wasm_cross` fixtures
and the almide-interp oracle instead.
