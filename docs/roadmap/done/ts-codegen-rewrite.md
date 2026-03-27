<!-- description: Rewrite TS codegen to two-stage pipeline (IR to TsIR to String) -->
<!-- done: 2026-03-15 -->
# TS/JS Codegen Rewrite

## Summary
`src/emit_ts/` を書き直し。Rust codegen と同じ 2 段パイプライン (IR → TsIR → String) に統一。

## Current State
- 旧コードのまま（4 ファイル）
- Rust codegen は RustIR パイプラインに移行済みだが、TS は未対応
- stdlib が空のため、TS runtime (`emit_ts_runtime.rs`) も大幅に簡素化可能

## Goal
- IR → TsIR → String の 2 段パイプライン
- 各ファイル < 500 行
- Rust codegen と同じ設計原則:
  - Lower で全判定
  - Render は pure pattern match

## Design
```
emit_ts/
  ts_ir.rs      — TsIR データ型
  lower_ts.rs   — IR → TsIR (Result erasure, ok→value, err→throw)
  render_ts.rs  — TsIR → TypeScript/JavaScript source
  mod.rs        — エントリポイント
```

## Files
```
src/emit_ts/ts_ir.rs (new)
src/emit_ts/lower_ts.rs (new)
src/emit_ts/render_ts.rs (new)
src/emit_ts/mod.rs (rewrite)
src/emit_ts_runtime.rs (simplify)
```
