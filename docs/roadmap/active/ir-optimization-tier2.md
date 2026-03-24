# IR Optimization Tier 2 [ACTIVE]

**優先度:** Medium — 全ターゲットに自動適用される最適化
**前提:** Tier 1 (constant folding, DCE) 完了済み、nanopass パイプライン確立済み
**目標:** IR レベルの最適化パスを拡充し、生成コードの品質を rustc/V8 に頼らず向上させる

> 「IR が賢ければ、全ターゲットが賢くなる。」

---

## Why

Tier 1 (constant folding, DCE) は完了済み。しかし IR レベルで解決すべき最適化がまだある:

- ループ内の不変式が毎回評価される (rustc は最適化するが WASM/TS では残る)
- 同一部分式の重複計算
- 使用回数 1 の小関数がコールオーバーヘッドを持つ
- エスケープしないオブジェクトのヒープ割当

IR で解決すれば Rust/TS/WASM 全ターゲットに恩恵がある。特に WASM は rustc の最適化を経由しないため、IR 最適化の効果が直接出る。

---

## Passes

### Pass 1: Loop-Invariant Code Motion (LICM)

ループ内の不変式をループ外に巻き上げる。

```almd
// Before
for i in range(0, 1000) {
    let scale = float(height - 1)  // 不変
    let y = float(i) / scale
}

// After (IR level)
let __licm_0 = float(height - 1)
for i in range(0, 1000) {
    let y = float(i) / __licm_0
}
```

**判定基準:** 式内の全変数がループ本体で変更されないこと。
**注意:** effect fn 呼び出しは巻き上げない (副作用)。

### Pass 2: Common Subexpression Elimination (CSE)

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

### Pass 3: Simple Inlining

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

### Pass 4: Escape Analysis (将来)

関数外にエスケープしないオブジェクトを検出。Rust ターゲットでは Box → stack、WASM ではヒープ割当回避に使う。

- borrow analysis の拡張として実装
- clone 削減パスと連携

### Pass 5: Strength Reduction (将来)

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
    │ Tier 2 (本ロードマップ)    │
    │  ├── LICMPass            │
    │  ├── CSEPass             │
    │  └── InlinePass          │
    ├─────────────────────────┤
    │ Tier 3 (将来)            │
    │  ├── EscapeAnalysisPass  │
    │  └── StrengthReduction   │
    └─────────────────────────┘
         │
         ▼
    mono() → nanopass → codegen
```

全パスは `NanoPass` trait を実装し、既存パイプラインに挿入。

---

## Phases

### Phase 1: LICM

- [ ] `LICMPass` 実装 (ループ不変判定 + 巻き上げ)
- [ ] effect fn 呼び出しの巻き上げ禁止
- [ ] テスト: ループ内不変式の巻き上げ確認
- [ ] WASM ターゲットでのベンチマーク (LICM の効果が直接出るため)

### Phase 2: CSE

- [ ] `CSEPass` 実装 (式の構造的等価判定 + 束縛挿入)
- [ ] 純粋性判定 (effect fn 呼び出しを含む式は対象外)
- [ ] テスト: stdlib 呼び出しの重複除去

### Phase 3: Inlining

- [ ] `InlinePass` 実装 (使用回数 + 本体サイズ判定)
- [ ] constant folding との連鎖テスト
- [ ] コードサイズ膨張の監視 (WASM バイナリサイズ回帰テスト)

---

## Success Criteria

- LICM, CSE, Inline の 3 パスが nanopass パイプラインに統合
- WASM バイナリサイズが回帰しない (DCE とのバランス)
- 既存テスト 2,681+ が全通過を維持
- ベンチマーク (Mandelbrot 等) で measurable な改善
