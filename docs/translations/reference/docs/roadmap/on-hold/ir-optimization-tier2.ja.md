<a id="ir-optimization-tier-2"></a>

# IR 最適化 Tier 2

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ir-optimization-tier2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ir-optimization-tier2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ir-optimization-tier2.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ir-optimization-tier2.md)
> 保留中のロードマップで、既存の保守的な scalar/list-sink インライン化と提案中の拡張を区別する。図は当初の配置案で現行パイプライン全体ではなく、ベンチマーク改善は成功条件であって実測結果ではない。


> 原文の説明：クロスターゲット IR 最適化のための CSE とインライン化パス

**優先度：** 中 — 全ターゲットに自動適用する最適化
**前提条件：** Tier 1（定数畳み込み、DCE）完了、nanopass パイプライン確立済み
**目標：** 下流の native または Wasm エンジンの最適化だけに頼らず、IR レベルの最適化パスを拡張して生成コードの品質を高める

> 「IR が賢ければ、全ターゲットが賢くなる。」

---

<a id="why"></a>

## 理由

Tier 1（定数畳み込み、DCE）は完了している。ただし、まだ IR レベルで解決すべき最適化がある：

- 同一部分式の重複計算
- 現在認められている scalar／list-sink 部分集合を超えるインライン化
- 外へ逃げないオブジェクトのヒープ割り当て
- 低コストの代替に置き換えられる高コストの演算

共有 IR 最適化は native と Wasm の両ターゲットに役立ちうる。TypeScript は現在のコンパイラターゲットではない。Wasm は rustc を経由せず、下流の Wasm エンジンによる最適化はこれらのコンパイラパスと別物である。

---

<a id="passes"></a>

## パス

<a id="-completed"></a>

### ✅ 完了済み

| パス | ファイル | 概要 |
|------|------|---------|
| LICM | `pass_licm.rs` | ループ不変コードの移動。effect fn 呼び出しを除外 |
| StreamFusion | `pass_stream_fusion.rs` | map/filter/fold チェーンの融合。中間リストを除去 |
| スカラーのインライン化 | `crates/almide-wasm/src/inline_calls.rs`, `crates/almide-mir/src/lower/inline_scalar_fns.rs` | 保守的な純粋スカラーの部分集合。任意の関数ではない |
| List-sink のインライン化 | `crates/almide-optimize/src/optimize/sink_inline.rs` | 適格なルート list sink をターゲット固有の融合に見せる |

以前の暗黙の `AutoParallelPass` は廃止された。明示的な `fan` の lowering は `pass_rust_lowering_fan.rs` にあり、一般的な自動並列化パスではない。

<a id="remaining-proposals"></a>

### 残る提案

<a id="pass-1-common-subexpression-elimination-cse"></a>

### Pass 1：共通部分式除去（CSE）

同一式の重複計算を let 束縛へまとめる。

```almd
// Before
let a = list.len() * 2
let b = list.len() * 3

// After (IR level)
let __cse_0 = list.len()
let a = __cse_0 * 2
let b = __cse_0 * 3
```

**条件：** 式が純粋（effect なし）で、構造的に同一。

<a id="pass-2-simple-inlining"></a>

### Pass 2：単純なインライン化

既存の scalar／list-sink パスが適用されない範囲だけでインライン化を拡張する。以下の当初の使用回数／単一式ポリシーは拡張案であって、Almide が現在インライン化を全く行わない証拠ではない。

```almd
fn double(x: Int) -> Int { x * 2 }
let y = double(5)
// → let y = 5 * 2 → 10 (chains with constant folding)
```

**制約：**
- 本体は単一の式だけ
- 再帰しない
- 使用回数 ≤ しきい値（初期値：1）

<a id="pass-3-strength-reduction-future"></a>

### Pass 3：強度削減（将来）

高コストの演算を低コストの代替へ置き換える。

```
x * 2      → x << 1      (integer)
x / 4      → x >> 2      (positive integer)
x % 2 == 0 → x & 1 == 0  (integer)
```

---

<a id="architecture"></a>

## アーキテクチャ

```
Lower → IR
         │
         ▼
    ┌─────────────────────────┐
    │ Tier 1 (done)             │
    │  ├── ConstFoldPass       │
    │  └── DCEPass             │
    ├─────────────────────────┤
    │ Tier 1.5 (done)          │
    │  ├── LICMPass            │
    │  ├── StreamFusionPass    │
    │  └── (explicit fan separately) │
    ├─────────────────────────┤
    │ Tier 2 (this roadmap)    │
    │  ├── CSEPass             │
    │  └── InlinePass          │
    ├─────────────────────────┤
    │ Tier 3 (future)          │
    │  └── StrengthReduction   │
    └─────────────────────────┘
         │
         ▼
    mono() → nanopass → codegen
```

これは当初提案されたパス配置で、現行パイプラインの完全な図ではない。既存のインライン化処理は異なる IR／lowering 箇所で動作する。共有の拡張には、実装前に配置と受け入れ規則の明示が必要となる。

---

<a id="phases"></a>

## フェーズ

<a id="phase-1-cse"></a>

### Phase 1：CSE

- [ ] `CSEPass` を実装（構造的同一性チェック＋束縛挿入）
- [ ] 純粋性チェック（effect fn 呼び出しを含む式を除外）
- [ ] テスト：stdlib 呼び出しの重複除去

<a id="phase-2-inlining"></a>

### Phase 2：インライン化

- [ ] 既存の部分集合を超えるインライン化を定義・実装し、重複パスを避ける（使用回数＋本体サイズのポリシーは評価対象）
- [ ] 定数畳み込みとの連鎖テスト
- [ ] コードサイズ膨張を監視（WASM バイナリサイズの回帰テスト）

---

<a id="success-criteria"></a>

## 成功条件

- CSE と Inline パスを nanopass パイプラインへ統合
- WASM バイナリサイズが悪化しない（DCE と釣り合わせる）
- 既存テストがすべて引き続き通る
- ベンチマーク（Mandelbrot など）で測定可能な改善
