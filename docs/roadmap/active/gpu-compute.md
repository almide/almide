<!-- description: Matrix primitive type with compiler-driven CPU/GPU execution -->
# GPU Compute — Matrix Type and Compiler-Driven GPU Execution

**優先度:** Phase 3 (Runtime Foundation)
**前提:** Bytes型完了、WASM export完了、nanopass pipeline完了
**原則:** ユーザーは普通のAlmideコードを書く。GPUの存在を意識しない。`--target cuda` でGPU、なければCPU。
**構文制約:** 新しいキーワードなし。`Matrix` プリミティブ型と `grad` 組み込み関数のみ追加。

> 「書くのはAlmide。動くのがCPUかGPUかはコンパイラが決める。」

---

## 設計思想

### なぜやるか

Almideのミッション「LLMが最も正確に書ける言語」の最大の未開拓領域がML。LLMがMLコードを書く時の最頻出バグ:
1. テンソル次元の不一致
2. デバイス配置ミス (CPU/GPU)
3. 勾配フロー切断

Almideの型システム + コンパイラ最適化でこれらを消せる。

### 設計原則

1. **新しい概念を足さない** — `Matrix` は `Bytes` と同じプリミティブ。`grad` は `println` と同じ組み込み関数
2. **演算子で書く** — `x * w` が行列積。`tensor.matmul(x, w)` ではない
3. **GPUを隠蔽する** — ターゲットフラグでバックエンドが変わる。コードは同一
4. **既存のRustエコシステムに乗る** — burn/candle が autograd・GPU実行・メモリ管理を持つ。車輪の再発明しない
5. **nanopass で融合** — `map |> map` の融合と同じ仕組みで element-wise op を fused kernel に

---

## フェーズ

### Step 1: Matrix プリミティブ型

Bytes型と同じパターン。コンパイラ変更最小。

**言語側:**
```
let w = matrix.zeros(512, 1536)
let x = matrix.randn(32, 512)
let y = x * w + bias
let shape = matrix.shape(y)  // → (32, 1536)
```

**コンパイラ実装:**
- [ ] `Ty::Matrix` を `Ty` enum に追加
- [ ] `TypeConstructorId::Matrix` 登録
- [ ] パーサ/チェッカーの型解決に `"Matrix" => Ty::Matrix` 追加
- [ ] Rust codegen: `ndarray::Array2<f64>` (CPU)
- [ ] `stdlib/defs/matrix.toml` — zeros, randn, shape, transpose, from_lists, to_lists
- [ ] `runtime/rs/src/matrix.rs` — ndarray ラッパー
- [ ] テスト

**演算子オーバーロード:**
- [ ] `*` で `(Matrix, Matrix)` → 行列積
- [ ] `+` `-` で `(Matrix, Matrix)` → element-wise
- [ ] `*` `/` で `(Matrix, Float)` → スカラー演算
- [ ] codegen の Binary op 分岐に Matrix パターン追加

**検証:** `almide run` でCPU上の行列演算が動く

### Step 2: GPU バックエンド

`--target cuda` で burn/candle に切り替え。ユーザコード変更なし。

**codegen 分岐:**
```
Ty::Matrix の codegen →
  target == Rust:  ndarray::Array2<f64>      (CPU)
  target == Cuda:  burn::Tensor<CudaBackend, 2>  (GPU)
```

**実装:**
- [ ] `codegen::pass::Target::Cuda` 追加
- [ ] Rust codegen の Matrix 型レンダリングを target で分岐
- [ ] `runtime/rs/src/matrix_gpu.rs` — burn ラッパー (matmul, add, transpose, etc.)
- [ ] `Cargo.toml` テンプレートに burn 依存を追加
- [ ] `almide build model.almd --target cuda` で GPU バイナリ生成
- [ ] CUDA 環境でのテスト

**検証:** 同じ .almd が CPU と GPU 両方で同じ結果を返す

### Step 3: grad と学習ループ

`grad(loss, w)` で burn の autodiff に展開。

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
- [ ] codegen: `loss.backward()` + `grads.get(&param)` に展開 (burn autodiff)
- [ ] 勾配追跡対象の変数をマーク (burn の `require_grad`)
- [ ] activation 関数を stdlib に追加: leaky_relu, relu, softmax, sigmoid, tanh
- [ ] 損失関数を stdlib に追加: cross_entropy, mse
- [ ] テスト: 小さいMLPの学習が収束する

**検証:** XOR問題の学習が動く

### Step 4: nanopass 融合

element-wise op 列を 1 kernel に融合。Parameter Golf で直接効く。

**例:**
```
x * w |> leaky_relu(0.5) |> square
// ↓ nanopass rewrite
fused_elementwise(x * w, [leaky_relu(0.5), square])
// ↓ codegen
// 1 fused CUDA kernel (3 kernel launches → 1)
```

**実装:**
- [ ] IR パターン: `map(map(x, f), g)` → `map(x, f >> g)` (既存)
- [ ] element-wise 数値パターン検出 pass
- [ ] fused kernel の codegen (Triton or CUDA C)
- [ ] ベンチマーク: 融合 vs 非融合の速度比較

**検証:** Parameter Golf ベースラインの forward pass が高速化

### Step 5 (将来): 型レベル次元チェック

```
fn linear(x: Matrix[B, 512], w: Matrix[512, 1536]) -> Matrix[B, 1536] =
  x * w
```

コンパイル時に次元不整合を検出。dependent types のサブセット。Phase 3 以降。

---

## ターゲットマッピング

```
Almide type    Rust (CPU)              CUDA (GPU)              WASM
──────────────────────────────────────────────────────────────────────
Int            i64                     i64                     i64
Float          f64                     f64                     f64
String         String                  String                  i32 ptr
Bytes          Vec<u8>                 Vec<u8>                 i32 ptr
Matrix         ndarray::Array2<f64>    burn::Tensor<Cuda, 2>   ループ (小規模)
```

## 依存クレート

| Step | CPU | GPU |
|------|-----|-----|
| 1 | ndarray | — |
| 2 | — | burn (burn-cuda or burn-candle) |
| 3 | — | burn-autodiff |
| 4 | — | burn-fusion or Triton |

## 非ゴール (今は)

| 項目 | 理由 |
|------|------|
| N次元テンソル | Matrix (2D) で十分。3D以上は将来 |
| 独自 CUDA kernel | burn/candle のカーネルで十分 |
| 分散学習 (multi-GPU) | Parameter Golf は 8xH100 だが、まず 1GPU |
| 独自 autograd | burn に任せる。コンパイラADは将来検討 |
| TPU/ROCm 対応 | burn が対応すれば自動的に使える |

---

## 一文で

> Matrix型を Bytes と同じプリミティブとして追加し、演算子で行列計算を書き、grad で勾配を取り、コンパイラがターゲットに応じて CPU/GPU コードを吐く。ユーザーは GPU の存在を知らない。
