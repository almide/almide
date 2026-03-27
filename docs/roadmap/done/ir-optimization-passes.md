<!-- description: IR-to-IR transform passes applied before codegen for all targets -->
# IR Optimization Passes

IR-to-IR transform passes inserted before codegen to improve generated code quality.

## Why

IR redesign (Phase 1-5) により codegen は `&IrProgram` のみを入力とする。つまり IR を変換してから codegen に渡せば、全ターゲット (Rust/TS/JS) に最適化が自動適用される。現在は borrow analysis と use-count のみ。

## Passes

### Tier 1: Low-hanging fruit ✅

| Pass | Effect | Status |
|------|--------|--------|
| **Constant folding** | `1 + 2` → `LitInt(3)`, `"a" ++ "b"` → `LitStr("ab")` | ✅ |
| **Dead code elimination** | 到達不能ブランチ、使用されない let 束縛の除去 | ✅ |
| **Constant propagation** | `let x = 5; x + 1` → `5 + 1` → `6` | future |

### Tier 2: Medium complexity

| Pass | Effect |
|------|--------|
| **Inlining** | 小さな関数 (body が単一式) のインライン展開 |
| **Common subexpression elimination** | 同一式の重複計算を let 束縛にまとめる |
| **Loop-invariant code motion** | for/while ループ内の不変式をループ外に移動 |

### Tier 3: Advanced

| Pass | Effect |
|------|--------|
| **Tail call optimization** | 自己再帰末尾呼出 → labeled loop (既存 roadmap と統合可能) |
| **Escape analysis** | ヒープ割当の回避 (borrow analysis の発展) |
| **Specialization** | 型引数に基づく関数特殊化 (monomorphization の前段) |

## Architecture

```
Lowering → IrProgram
              │
              ▼
         ┌─────────────────┐
         │  Optimization    │   IR → IR transforms (pipeline of passes)
         │  ├── const_fold  │
         │  ├── dce         │
         │  └── inline      │
         └─────────────────┘
              │
              ▼
         Codegen (Rust / TS / JS)
```

各パスは `fn(ir: &mut IrProgram)` シグネチャ。パスの適用順序は固定。`--opt-level` フラグで制御。

## Unlocked by

IR Redesign Phase 5 完了。codegen が IR のみを参照するため、IR 変換が codegen 出力に直接反映される。

## Affected files

| File | Change |
|------|--------|
| `src/opt/` (new) | 最適化パスモジュール |
| `src/main.rs` | パイプラインにパス挿入 |
| `src/cli.rs` | `--opt-level` フラグ |
