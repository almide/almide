# almide-kernel

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-kernel/README.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> この翻訳はソース固定の非規範的スナップショットです（リビジョン：35297a612fb29e3f78f6587aba253aa91d51f466）。自動同期されません。測定値および主張は独立に再検証していません。英語原文を正とします。
>
> 原文上の注意: 冒頭で移転・削除を説明するカーネルの測定・証明・実装説明が本文に歴史的記録として残っています。現在の実装一覧や再検証済みの性能保証を示しません。

Almide 向けのターゲット別 SIMD 数値カーネル — **高速、かつ証明済み**。Wyve で証明されたカーネルの Rust 移植です。自動ベクトル化が構造上妨げられる箇所では明示的 SIMD を使い、構造が許す箇所では静的に検証します。

> **Dialect epoch 14 (2026-10-17):** Q1_0、GELU、SiLU、softmax カーネル
> (`q1_0`, `q1_0_packed`, `gelu`, `silu`, `softmax`) は、それらが支えていた `matrix` 関数とともにこの crate から離れ、パッケージ
> [almide-graphics/nn](https://github.com/almide-graphics/nn)（モジュール `nn.kernels`）へ移動しました。現在この crate にあるのは `matmul`、
> `scale`、`transpose`、`transpose_f64`、`bridge` です。以下の q1_0 の測定と証明はその作業の記録です。コードは git 履歴にあります。

<a id="at-a-glance"></a>

## 概要

```
Fast (4-quadrant — beats Rust AND Almide, on native AND wasm):
  transpose 8x8      native 5.18× / wasm 3.66×   (vs Rust)
  q1_0 block dot     native 3.73× / wasm 2.52×
  linear_q1_0 matmul native 6.74× / wasm 3.03×   (real inference hot path)
  scale              ties autovec → ships naive (no ceremony)

Proven (nothing left to "promised"):
  permutation (transpose) → static, all inputs   (index run = total proof)
  selection (q1_0 signs)  → static, exhaustive   (256 bytes = all bit patterns)
  reduction (q1_0 sum)    → bitwise to a named order + bounded error (n·u·Σ|x|)

Per-target: x86 AVX/AVX2 · wasm simd128 · naive fallback (one source, cargo --target)
Standalone: zero deps, 16 tests green, builds native + wasm
```

これは Exo の系譜（アルゴリズムとスケジュールの分離 + 構築による正しさ）を Rust で、Almide ネイティブとして実現したものです。置換・選択カーネルでは *Exo より軽量* です（SMT ソルバー不要。構造が決定可能です）。

<a id="the-form"></a>

## 構成

```
Wyve (Racket/Lean)      = research lab : design a kernel, prove it bitwise-exact
                                          and in-range (@bounds), in Lean
   ↓ port the proven kernel
almide-kernel (Rust)    = factory      : production SIMD (core::arch),
                                          Almide-native, zero dependencies
   ↑ pinned by
differential test       = bridge       : SIMD == the same naive reference Wyve
                                          proves against → the proof carries over
```

Wyve はリンクされません（Racket/wyvec/LLVM-clang へのビルド依存はありません）。Wyve はカーネルを *証明する* 場所で、この crate はそれを *出荷する* 場所です。新しいカーネルは、Wyve で設計・証明 → ここで Rust に移植 → 差分テストでビット単位の完全一致を確認、という流れです。

<a id="kernels"></a>

## カーネル

| カーネル | 実装 | 正しさ | 速度 |
|---|---|---|---|
| `transpose_8x8` | wasm simd128 シャッフル。それ以外では通常のタイル（x86 AVX の 3 パスネットワークはどのプログラムにも使われず、`unsafe` とともに削除、#4342） | ビット単位で完全一致、100 パターン | AVX の測定値は **デフォルト 1.57× / native 4.23×**。通常のタイルはそれより 1.28× 遅い（GCE Intel、2026-10-30） |
| `scale` | 素朴な実装（自動ベクトル化）。測定専用の `scale_avx` は削除済み（#4342） | ビット単位で完全一致（avx == naive） | **native 0.99×** → 素朴な実装を維持 |
| `q1_0_dot` | AVX2 ビット展開 + 符号付き加算 + リダクション | 許容誤差内（結合順序を変えるリダクション） | **3.5×（デフォルト、native とも）** |

<a id="beating-rust--where-and-the-right-correctness-bar"></a>

### Rust を上回る箇所と、適切な正しさの基準

```
transpose (shuffle)   : 4.23× native, bitwise-exact   — autovec can't shuffle
q1_0 (bit-unpack)     : 3.5× any target, reassoc       — autovec can't bit-address
scale (elementwise)   : 1.0× native, bitwise-exact     — autovec already wins → naive
```

勝るのは、まさに rustc の自動ベクトル化が *構造的に* 妨げられる箇所、つまりシャッフルネットワークとパックされたビットのアドレッシングです。q1_0 は target-cpu に関係なく Rust を 3.5× 上回ります。自動ベクトル化にできることがないため、ベースラインが追いつくことはありません。

正しさの基準は演算によります。**データ移動（transpose）→ ビット単位の完全一致**、**リダクション（q1_0）→ 許容誤差内**。SIMD スケジュールが浮動小数点加算の結合順序を変えるためです（誤差幅は |result| ではなく *各項の絶対値の和*。結果は相殺でほぼゼロにもなります）。演算ごとに適切な基準を選ぶことが大切です。

<a id="static-equivalence--exos-idea-lighter-for-permutations"></a>

### 静的等価性 — Exo の発想を、置換ではさらに軽量に

Exo はスケジュールとそのアルゴリズムの等価性を *静的に*（実行せずに）証明します。各文の読み書きの **効果** を計算し、**SMT ソルバー** で変換後と元のプログラムが可換であることを証明します。almide-kernel はこの考えを借り、**置換カーネル**（転置、シャッフル）では *ソルバーなし* で行います。

```
A permutation moves positions, never values. So running it on the index array
input[k]=k extracts the ENTIRE permutation in one shot. If that equals the
transpose spec, the SIMD schedule is correct for EVERY input — a total proof,
not 100 samples. (f32/f64 hold 0..N exactly, so the source index is recovered
losslessly.)
```

`schedule_is_the_transpose_permutation_for_all_inputs`（f32 8x8、f64 8x8、端数を含む任意サイズ）は、サンプルによる差分テストを、全入力に対する静的保証に置き換えます。Exo が SMT ソルバーを必要とするところで、置換は 1 回の実行で決定できます。

**リダクションは 2 層に分けます。** q1_0 自体は置換ではありませんが、符号の *配置* は置換なので、その部分も証明します。ビット展開（`apply_sign`）は値に依存しない符号ビットの XOR で、1 バイトが 8 レーンをカバーするため、**256 個のバイト値で全ビットパターンを網羅** します。`avx2_bit_unpack_total_proof` は *すべての* バイトとすべての入力について符号が正しいレーンに入ることを証明します。有限で完全な証明であり、ソルバーは不要です。そして浮動小数点の *和* も「許容誤差内」で終わらせず、2 層で証明します。

```
layer 1 (order is the spec) : the SIMD reduction is BITWISE-EXACT to a specified
                              tree-order reduction (8 lanes → lo+hi → hadd → hadd).
                              avx2_is_bitwise_exact_to_tree_order, 500 seeds, no tolerance.
layer 2 (the error is bounded): tree-order vs the idealized exact sum is within
                              n·u·Σ|xₖ| (n=128, u=2⁻²⁴) — a reassociation-error
                              theorem (Lean-provable; the test witnesses it holds).
```

浮動小数点は順序に依存するため、「和」は元来ただ 1 つではありません。順序を仕様として明示すれば、カーネルはそれを *厳密に* 実装し（第 1 層）、理想値との差は *期待ではなく上界で制限されます*（第 2 層）。浮動小数点リダクションについて、これは約束ではなく証明です。

```
q1_0 correctness:  sign placement → PROVEN (256 exhaustive)
                   reduction      → PROVEN (bitwise to tree-order + bounded error)
```

したがって、almide-kernel の正しさの全体像は次のとおりです。
- 置換（transpose）→ 静的、全入力（インデックスで 1 回実行）
- 選択・ビット展開（q1_0 の符号）→ 静的、網羅的（256 バイト）
- 浮動小数点リダクション（q1_0 の和）→ 指定順序にビット単位で一致 + 有界誤差
「約束」のまま残るものはありません。

<a id="4-quadrant-beat-rust-and-almide-on-native-and-wasm-q1_0"></a>

### 4 象限：native と wasm の両方で Rust と Almide の両方を上回る（q1_0）

|               | native (AVX2) | wasm (simd128) |
|---------------|---------------|----------------|
| Rust の素朴な実装比 | **3.73×**     | **2.52×**      |
| Almide 比     | **3.62×**     | **2.43×**      |

**transpose**（8x8、両ターゲットともビット単位で完全一致）：

|               | native (AVX) | wasm (simd128) |
|---------------|--------------|----------------|
| Rust の素朴な実装比 | **5.18×**    | **3.66×**      |
| Almide 比     | **2.70×**    | **3.50×**      |

almide-kernel は **ターゲット別 SIMD**（x86 AVX/AVX2、wasm simd128）と素朴なフォールバックを備えます。q1_0 では Rust の自動ベクトル化も Almide 自身の内積もスカラーです（Almide の SIMD は NEON のみ。パックされたビットのアドレッシングは自動ベクトル化を妨げます）。転置では、自動ベクトル化はシャッフルネットワークをまったく構築できません。そのため almide-kernel は両カーネルで 4 つのセルすべてに勝ち、native *でも* wasm *でも*、Rust *と* Almide より高速です。（Almide は f64、almide-kernel は f32。処理に合った幅の選択も優位性の一部です。）

**量子化線形演算全体** `linear_q1_0` — 推論のホットパス、W を Q1_0 とした `x @ Wᵀ`。1×2048×2048（2048→2048 層を 1 トークンが通過）で測定：

|               | native (AVX2) | wasm (simd128) |
|---------------|---------------|----------------|
| Rust の素朴な実装比 | **6.74×**     | **3.03×**      |

（最大相対誤差 6.7e-8 / 8.9e-8 — f32 の結合順序変更の許容誤差内。）行列積では単一ブロックの内積より差が *広がります*（3.5×→6.74×）。ブロックごとの SIMD スケジュールのコストが行列全体で償却されます。これはマイクロベンチではなく実際のワークロードです。

**任意サイズの転置** `transpose_matrix` — `almide_rt` が実際に必要とするものです（Almide の行列は 8x8 ではありません）。完全な 8x8 タイルは SIMD カーネルを通り、端数部分はスカラー処理します。どのサイズでもビット単位で完全一致します。512×512 で測定：

|               | native (AVX) | wasm (simd128) |
|---------------|--------------|----------------|
| Rust の素朴な実装比 | **3.50×**    | **3.15×**      |
| Almide f64 比 | **3.13×**    | **3.63×**      |

これが `almide_rt` との接続の中核です。任意の行列をタイルに分け、固定サイズの SIMD カーネルに載せます。

<a id="the-abi-shape-decides-the-simd-win-measured"></a>

### ABI の形が SIMD の優位性を決める（測定済み）

`bridge.rs` は完全な接続（`Vec<Vec<f64>>` → カーネル → `Vec<Vec<f64>>`）で、ビット単位で完全一致します。しかし 512×512 で測定すると、**同じカーネル** でも次の結果になります。

|                          | Almide の素朴な実装に対する高速化 |
|--------------------------|-------------------------|
| ネストした `Vec<Vec<f64>>`   | **0.44×**（遅い！）     |
| フラットな `Vec<f64>`          | **3.07×**               |

ABI だけで 7× の振れ幅です。ネスト→フラット / フラット→ネスト変換（全体コピー 2 回 + 行ごとの Vec 割り当て）は、SIMD 転置の節約分より高くつきます。したがって本番運用の教訓はカーネルではなく、Almide の **型** にあります。**フラットな** バッファ（SmallF32 / フラットな f64）を渡せば 3× を得られますが、`Vec<Vec<f64>>` を渡すと変換が利得を食いつぶします。接続が報われるのは、行列がフラットである場合に限ります。

本番向けの完全な接続に残る課題は、Almide がホットパスにフラットな行列 ABI を採用することと、Almide のビルドフローです（`almide_rt` には prelude の注入が必要で、`almide run`/`build` の内部でしかコンパイルされません）。

AVX 転置は Wyve の検証済み 3 パスシャッフルネットワーク（隣接行を unpack → 64-bit グループを shuffle → 128-bit レーンを permute）を反映します。

<a id="the-design-rule-confirmed-by-measurement"></a>

### 測定で確認された設計原則

```
data-movement (transpose) : autovec can't build a shuffle network → explicit
                            SIMD wins regardless of target-cpu (4.23× native)
elementwise (scale)       : autovec ties explicit AVX at target-cpu=native
                            (0.99×) → ship naive, no ceremony
```

*デフォルト* ビルドでの `scale` の 1.23× は見かけのものでした。自動ベクトル化が SSE2 ベースラインに制限されていたためです。`target-cpu=native` では互角です。したがって almide-kernel が明示的 SIMD を書くのは、自動ベクトル化が *構造上* 追随できないデータ移動だけです。それ以外は素朴な実装を保ちます。（要素単位の速度を直すのは Almide のビルドフラグであり、この crate の余計な仕組みではありません。）

<a id="exo-style-algorithm--schedule-separation"></a>

## Exo 流：アルゴリズムとスケジュールの分離

転置カーネルは Exo と同様に **アルゴリズム**（何を）と **スケジュール**（どうやって）を分離します。ただしこの crate の中で、通常の Rust で行います。

```rust
// algorithm: the spec
fn transpose_8x8_naive(..)  { out[j][i] = in[i][j] }

// schedule: named passes, composed — THIS line is the schedule
store(pass_permute(pass_shuffle(pass_unpack(load(input)))))
//    └─ recompose the passes to change how it runs;
//       the algorithm above is untouched.
```

| Exo | almide-kernel |
|---|---|
| アルゴリズムとスケジュールを分離 | 同じ — 素朴な仕様 + 合成されたパス |
| 効果解析によるプログラム等価性（静的） | ビット単位完全一致の差分テスト（動的、日常的）+ Lean（任意の、より強い裏付け） |
| 独自のバックエンド | Rust = LLVM、Almide ネイティブ |
| 人間がスケジュールを書く | 今は人間が書く → **後には Almide が導出する**（他にはない立ち位置） |

測定結果：名前付きパスに分割してもビット単位の完全一致を保ち、4.19x（ひとまとまりのときは 4.23x）でした。`#[inline(always)]` が合成を畳み込んで消します。スケジュールは明示的かつ再合成可能で、実行時コストは **ゼロ** です。

<a id="why-a-separate-crate"></a>

## crate を分ける理由

- **依存なし、prelude 注入なし** — 単独でビルド・テストできます。
  （`almide_rt` はできません。src が Almide の prelude による `use` 項目の注入を前提とし、完全なパイプライン内でのみコンパイルされるためです。この crate は通常の Rust です。）
- `almide_rt` は、native でも明示的 SIMD が自動ベクトル化を上回る **データ移動演算**（転置、シャッフル）にこれを呼びます。そこが実測で確認された Wyve の優位性の所在です。

<a id="bench-honesty"></a>

## ベンチマークの誠実さ

最初の 2 回の試行は落とし穴だったので、記録しておきます。
1. `out[1]` だけを使うと、rustc の DCE が素朴な転置を 1 回のコピーに削減しました（素朴な実装が 7× *速く* 見えましたが、転置していませんでした）。
2. 64 個すべての出力を合計すると `sum` が支配的になり、転置の差が埋もれました（1.03×）。

反復ごとに変わる入力と `black_box(&o)` により、転置全体の計算と格納を強制し、シャッフルネットワークだけを測定できます：**1.57×**。マイクロベンチが DCE や別のホットループを測っていないか、必ず確認してください。

<a id="run"></a>

## 実行

```
cargo test                              # naive-is-transpose + SIMD==naive bitwise
cargo run --release --example bench     # AVX vs naive
```

> 証拠の区分について：原文は「proven」と表現する一方、本文には500 seedsのテスト記録と「Lean-provable（Leanで証明可能）」という記述もあります。この翻訳は、それらを同じ証明区分として扱わず、Lean証明の完了やRust移植の全入力に対する形式検証を独立に確認したものではありません。
