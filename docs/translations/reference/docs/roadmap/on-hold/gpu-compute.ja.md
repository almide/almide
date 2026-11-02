<a id="gpu-compute--matrix-type-and-compiler-driven-gpu-execution"></a>

# GPU 計算 — Matrix 型とコンパイラ駆動の GPU 実行

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](gpu-compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](gpu-compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](gpu-compute.zh-CN.md)

> 非規範の固定読者用スナップショット。変更しない原文を正本とし、機械入力として置き換えない。自動同期や独立実行はしない。歴史的測定・提案・原文時点の実装更新を区別して保持し、現在の正しさや性能の独立証明にはしない。コードとコメントは原文どおり保持する。 10月31日の状態は、出荷済み Matrix/CPU 基盤と既存 GPU 経路を、未実装の CUDA/grad/学習提案から区別する。歴史的チェックリストの未チェック項目は変更しない。バグ除去や backend サポートの表現は設計上の目標であり、検証済みの普遍的保証ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/gpu-compute.md)

> 元 metadata の非活性要約：コンパイラ駆動の CPU/GPU 実行を伴う Matrix プリミティブ型。

**優先度:** Phase 3（ランタイム基盤）
**前提:** Bytes 型完了、WASM export 完了、nanopass パイプライン完了
**原則:** 利用者は普通の Almide コードを書く。GPU を意識しない。`--target cuda` なら GPU、それ以外は CPU で動く。
**構文上の制約:** 新しいキーワードは設けない。追加するのは `Matrix` プリミティブ型と `grad` 組み込み関数だけ。

> 「書くのは Almide。CPU で動くか GPU で動くかは、コンパイラが決める。」

---

<a id="current-foundation-and-remaining-proposal-2026-10-31"></a>

## 現在の基盤と残る提案（2026-10-31）

`Matrix` と CPU 行列演算は既に存在する。コンストラクタ、shape/transpose、演算子 lowering helper を含む [`stdlib/matrix.almd`](../../../../../../stdlib/matrix.almd) を参照。ランタイムは [`runtime/rs/src/matrix.rs`](../../../../../../runtime/rs/src/matrix.rs) にある。リポジトリには別の GPU 実行経路 [`almide-gpu-run`](../../../../../../crates/almide-gpu-run) もあるが、その存在は以下の提案 `--target cuda`、`grad`、自動学習ループの実装を意味しない。

以下のチェックリストは**元の段階設計**であり、現在不足する Matrix 機能の一覧ではない。特に Step 1 の型・CPU・演算子基盤は `.almd` stdlib を通じて出荷済みである（旧 `stdlib/defs/*.toml` ソース形式は廃止済み）。CUDA/autograd 統合と提案された型レベル shape 設計は、独立した決定事項のままであり、完了を推定しない。

<a id="design-philosophy"></a>

## 設計思想

<a id="why"></a>

### なぜか

Almide の使命「LLM が最も正確に書ける言語」にとって最大の未開拓領域は ML である。LLM が ML コードを書くときに最も頻繁なバグは次のもの。
1. テンソル次元の不一致
2. デバイス配置エラー（CPU/GPU）
3. 勾配フローの断絶

Almide の型システムとコンパイラ最適化は、これらを除去できる。

<a id="design-principles"></a>

### 設計原則

1. **新しい概念を加えない** — `Matrix` は `Bytes` と同じくプリミティブであり、`grad` は `println` と同じく組み込み。
2. **演算子で書く** — `x * w` は行列積であり、`tensor.matmul(x, w)` ではない。
3. **GPU を隠す** — target フラグでバックエンドが変わり、コードは変わらない。
4. **既存の Rust エコシステムを利用する** — burn/candle は既に autograd、GPU 実行、メモリ管理を提供する。再発明しない。
5. **nanopass で融合する** — `map |> map` 融合と同じ機構が、要素ごとの演算を融合 kernel にする。

---

<a id="phases"></a>

## フェーズ

<a id="step-1-matrix-primitive-type-historical-foundation"></a>

### Step 1: Matrix プリミティブ型（歴史的基盤）

Bytes 型と同じパターン。コンパイラ変更は最小限。

**言語側:**
```
let w = matrix.zeros(512, 1536)
let x = matrix.randn(32, 512)
let y = x * w + bias
let shape = matrix.shape(y)  // → (32, 1536)
```

**コンパイラ実装:**
- [ ] `Ty` enum に `Ty::Matrix` を追加
- [ ] `TypeConstructorId::Matrix` を登録
- [ ] parser/checker の型解決に `"Matrix" => Ty::Matrix` を追加
- [ ] Rust codegen: `ndarray::Array2<f64>`（CPU）
- [ ] `stdlib/defs/matrix.toml` — zeros、randn、shape、transpose、from_lists、to_lists
- [ ] `runtime/rs/src/matrix.rs` — ndarray wrapper
- [ ] テスト

**演算子オーバーロード:**
- [ ] `(Matrix, Matrix)` の `*` → 行列積
- [ ] `(Matrix, Matrix)` の `+` `-` → 要素ごとの演算
- [ ] `(Matrix, Float)` の `*` `/` → scalar 演算
- [ ] codegen Binary op dispatch に Matrix パターンを追加

**現在の基盤:** Matrix 演算は `almide run` で CPU 実行される。上の旧実装チェックリストは初期計画の記録である。

<a id="step-2-gpu-backend"></a>

### Step 2: GPU バックエンド

`--target cuda` で burn/candle に切り替える。利用者コードの変更はない。

**codegen dispatch:**
```
Ty::Matrix codegen →
  target == Rust:  ndarray::Array2<f64>      (CPU)
  target == Cuda:  burn::Tensor<CudaBackend, 2>  (GPU)
```

**実装:**
- [ ] `codegen::pass::Target::Cuda` を追加
- [ ] Rust codegen の Matrix 型 render を target に基づき分岐
- [ ] `runtime/rs/src/matrix_gpu.rs` — burn wrapper（matmul、add、transpose など）
- [ ] `Cargo.toml` テンプレートに burn 依存を追加
- [ ] `almide build model.almd --target cuda` が GPU バイナリを生成
- [ ] CUDA 環境でテスト

**検証:** 同じ .almd が CPU と GPU の両方で同一結果を返す

<a id="step-3-grad-and-training-loops"></a>

### Step 3: grad と学習ループ

`grad(loss, w)` を burn の autodiff へ展開する。

**言語側:**
```
fn train_step(x: Matrix, w: Matrix, target: Matrix, lr: Float) -> Matrix =
  let pred = x * w |> leaky_relu(0.5) |> square
  let loss = cross_entropy(pred, target)
  let dw = grad(loss, w)
  w - lr * dw
```

**コンパイラ実装:**
- [ ] `grad(loss, param)` を組み込み関数として認識
- [ ] codegen: `loss.backward()` + `grads.get(&param)` へ展開（burn autodiff）
- [ ] 勾配を追跡する変数をマーク（burn の `require_grad`）
- [ ] stdlib に活性化関数 leaky_relu、relu、softmax、sigmoid、tanh を追加
- [ ] stdlib に損失関数 cross_entropy、mse を追加
- [ ] テスト: 小さな MLP の学習が収束

**検証:** XOR 問題の学習が動く

<a id="step-4-nanopass-fusion"></a>

### Step 4: Nanopass 融合

要素ごとの演算チェーンを単一 kernel に融合する。Parameter Golf に直接影響する。

**例:**
```
x * w |> leaky_relu(0.5) |> square
// ↓ nanopass rewrite
fused_elementwise(x * w, [leaky_relu(0.5), square])
// ↓ codegen
// 1 fused CUDA kernel (3 kernel launches → 1)
```

**実装:**
- [ ] IR パターン: `map(map(x, f), g)` → `map(x, f >> g)`（既存）
- [ ] 要素ごとの数値パターン検出パス
- [ ] 融合 kernel codegen（Triton または CUDA C）
- [ ] ベンチマーク: 融合あり／なしの速度比較

**検証:** Parameter Golf ベースラインの forward pass が速くなる

<a id="step-5-future-type-level-dimension-checking"></a>

### Step 5（将来）: 型レベル次元検査

```
fn linear(x: Matrix[B, 512], w: Matrix[512, 1536]) -> Matrix[B, 1536] =
  x * w
```

コンパイル時に次元不一致を検出する。依存型の部分集合。Phase 3 以降。

---

<a id="target-mapping"></a>

## ターゲット対応

```
Almide type    Rust (CPU)              CUDA (GPU)              WASM
──────────────────────────────────────────────────────────────────────
Int            i64                     i64                     i64
Float          f64                     f64                     f64
String         String                  String                  i32 ptr
Bytes          Vec<u8>                 Vec<u8>                 i32 ptr
Matrix         ndarray::Array2<f64>    burn::Tensor<Cuda, 2>   loops (small scale)
```

<a id="dependency-crates"></a>

## 依存 crate

| Step | CPU | GPU |
|------|-----|-----|
| 1 | ndarray | — |
| 2 | — | burn (burn-cuda or burn-candle) |
| 3 | — | burn-autodiff |
| 4 | — | burn-fusion or Triton |

<a id="non-goals-for-now"></a>

## 現時点の対象外

| 項目 | 理由 |
|------|--------|
| N 次元テンソル | Matrix（2D）で十分。3D+ は将来作業 |
| カスタム CUDA kernel | burn/candle の kernel で十分 |
| 分散学習（multi-GPU） | Parameter Golf は 8xH100 を使うが、まず 1 GPU から |
| カスタム autograd | burn に委ねる。コンパイラ AD は将来検討 |
| TPU/ROCm サポート | burn がサポートすれば自動的に利用可能になる |

---

<a id="in-one-sentence"></a>

## 1 文で言うと

> Bytes のようなプリミティブとして Matrix 型を加え、演算子で行列演算を書き、grad で勾配を取り、コンパイラが target に応じて CPU/GPU コードを出力する。利用者は GPU の存在を意識しない。

コード内の英語注記の訳：nanopass rewrite は nanopass による書き換え、codegen はコード生成。融合 CUDA kernel を 1 個にし、kernel 起動回数を 3 から 1 にするという提案例である。ターゲット表の loops (small scale) は小規模向けのループを指す。
