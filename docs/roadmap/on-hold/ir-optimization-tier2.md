<!-- description: CSE and inlining passes for cross-target IR optimization -->
# IR Optimization Tier 2

**優先度:** Medium — 全ターゲットに自動適用される最適化
**前提:** Tier 1 (constant folding, DCE) 完了済み、nanopass パイプライン確立済み
**目標:** IR レベルの最適化パスを拡充し、生成コードの品質を rustc/V8 に頼らず向上させる

> 「IR が賢ければ、全ターゲットが賢くなる。」

---

## Why

Tier 1 (constant folding, DCE) は完了済み。しかし IR レベルで解決すべき最適化がまだある:

- 同一部分式の重複計算
- 使用回数 1 の小関数がコールオーバーヘッドを持つ
- エスケープしないオブジェクトのヒープ割当
- 高コスト演算の低コスト置換

IR で解決すれば Rust/TS/WASM 全ターゲットに恩恵がある。特に WASM は rustc の最適化を経由しないため、IR 最適化の効果が直接出る。

---

## Passes

### ✅ 完了済み

| Pass | ファイル | 概要 |
|------|----------|------|
| LICM | `pass_licm.rs` | ループ不変式の巻き上げ。effect fn 呼び出しは対象外 |
| StreamFusion | `pass_stream_fusion.rs` | map/filter/fold チェーンの融合。中間リスト除去 |
| Auto Parallel | `pass_auto_parallel.rs` | 純粋ラムダの自動並列化 (Rust ターゲット) |

### 未実装

### Pass 1: Common Subexpression Elimination (CSE)

同一式の重複計算を let 束縛にまとめる。

```almd
// Before
let a = list.len() * 2
let b = list.len() * 3

// After (IR level)
let __cse_0 = list.len()
let a = __cse_0 * 2
let b = __cse_0 * 3
```

**判定基準:** 式が純粋 (effect なし) かつ構造的に同一。

### Pass 2: Simple Inlining

使用回数 1 かつ本体が単一式の関数をインライン展開。

```almd
fn double(x: Int) -> Int { x * 2 }
let y = double(5)
// → let y = 5 * 2 → 10 (constant folding と連鎖)
```

**制約:**
- 本体が 1 式のみ
- 再帰でない
- 使用回数 ≤ 閾値 (初期値: 1)

### Pass 3: Strength Reduction (将来)

高コスト演算を低コスト演算に置換。

```
x * 2      → x << 1      (整数)
x / 4      → x >> 2      (正の整数)
x % 2 == 0 → x & 1 == 0  (整数)
```

---

## Architecture

```
Lower → IR
         │
         ▼
    ┌─────────────────────────┐
    │ Tier 1 (完了)            │
    │  ├── ConstFoldPass       │
    │  └── DCEPass             │
    ├─────────────────────────┤
    │ Tier 1.5 (完了)          │
    │  ├── LICMPass            │
    │  ├── StreamFusionPass    │
    │  └── AutoParallelPass    │
    ├─────────────────────────┤
    │ Tier 2 (本ロードマップ)    │
    │  ├── CSEPass             │
    │  └── InlinePass          │
    ├─────────────────────────┤
    │ Tier 3 (将来)            │
    │  └── StrengthReduction   │
    └─────────────────────────┘
         │
         ▼
    mono() → nanopass → codegen
```

全パスは `NanoPass` trait を実装し、既存パイプラインに挿入。

---

## Phases

### Phase 1: CSE

- [ ] `CSEPass` 実装 (式の構造的等価判定 + 束縛挿入)
- [ ] 純粋性判定 (effect fn 呼び出しを含む式は対象外)
- [ ] テスト: stdlib 呼び出しの重複除去

### Phase 2: Inlining

- [ ] `InlinePass` 実装 (使用回数 + 本体サイズ判定)
- [ ] constant folding との連鎖テスト
- [ ] コードサイズ膨張の監視 (WASM バイナリサイズ回帰テスト)

---

## Success Criteria

- CSE, Inline の 2 パスが nanopass パイプラインに統合
- WASM バイナリサイズが回帰しない (DCE とのバランス)
- 既存テスト全通過を維持
- ベンチマーク (Mandelbrot 等) で measurable な改善
