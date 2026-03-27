<!-- description: Constant folding, dead code elimination, and basic inlining passes -->
<!-- done: 2026-03-15 -->
# IR Optimization Passes

## Summary
IR-to-IR optimization passes: constant folding, DCE, and basic inlining.

## Current State
IR に最適化パスがない。生成された Rust コードは rustc が最適化するが、IR レベルの最適化で不要な clone やアロケーションを削減できる。

## Design

### Pass 1: Constant Folding
```
1 + 2        → 3
"a" ++ "b"   → "ab"
not true     → false
if true then a else b → a
```

### Pass 2: Dead Code Elimination
use-count が 0 のバインディングを除去。
```
let x = expensive()  // x が未使用
println("hello")
→
println("hello")
```

### Pass 3: Simple Inlining (future)
1 回だけ使われる小さな関数をインライン展開。

## Implementation
- `src/optimize.rs` (新ファイル) — `optimize_program(&mut IrProgram)`
- `src/main.rs` — lower 後、codegen 前にパスを挿入
- 各パスは `IrProgram` を in-place で変換
- テスト: 最適化前後の出力を比較

## Pipeline Position
```
Lower → IR → optimize() → mono() → codegen
```

## Files
```
src/optimize.rs (new, < 500 lines)
src/main.rs
src/lib.rs
```
