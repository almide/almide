原文の説明メタデータ：コンパイル時のモデル特殊化を使う小型 ML 推論ランタイム。

<a id="tiny-ml-inference-runtime"></a>

# 小型 ML 推論ランタイム

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ml-inference.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](ml-inference.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ml-inference.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ml-inference.md)

> 原文注記：これは保留中の提案で、出荷済み ML runtime ではない。サイズ、段階の期間、性能目標、競合の特徴付けは原文の見積り・主張として保持し、独立に検証した結果ではない。図の 30–80 KB と比較表の 30–500 KB は整合化せず保持し、runtime と重みは別々に報告する必要がある。原文は前提が揃うまで全段階を開始しないとする一方、Phase 0 に SIMD 追加を含め、再開条件に Phase 0 の結果を要求する。この段取り上の緊張も保持する。削除された生 pointer API を復活させず、提案 loading interface や cross-host 対応の存在を主張しない。

**優先度：** graphics stack が v1.0 に到達した後
**前提条件：** Almide codegen の `wasm-simd128` intrinsics、安定した `bytes` API、package 境界をまたぐ DCE 測定
**原則：** ML graph の interpreter を書かない。graph をコンパイルする。モデル 1 つを入力し、小さい WASM binary 1 つを出力する。そのモデルが実際に必要とする operator だけを含める。
**仮説：** モデル固有のコンパイルは固定 workload の runtime overhead を減らす可能性がある。この提案は主張するサイズ上の優位を実証しておらず、競合システムにモデル特殊化がないことも立証していない。

> 提案する目標：モデル固有の Wasm 成果物 1 つ。runtime bytes と重み bytes は別々に報告する。当初の 30–80 KB という見積りは未測定。

---

<a id="thesis"></a>

## 主張

提案の範囲は汎用推論 stack の再現より狭い。当初の ORT operator 数・サイズの数値は版を指定しない比較であり、実証された削減不能な下限でも、現在の benchmark でもない。

興味深い方策は別の方向にある。モデルファイルを*ソースファイル*として扱い、runtime interpreter ではなく Almide コンパイラを通す。モデルが使う operator はインライン化された Almide 関数になる。重みは静的データまたは外部から読み込む `Bytes` になる。未使用 operator は binary にまったく入らない。望む結果はモデルに特殊化した実行ファイルであり、サイズは今後測定する必要がある。

この roadmap 項目は、単純な embedding runner から始め、LLM 推論と完全な model-as-code コンパイルへ進む段階的な道筋を述べる。

---

<a id="shape-space"></a>

## 形の選択肢

この作業には 4 つの形があり、実行可能なのは 2 つだけである。

<a id="rejected-full-ort-clone"></a>

### 却下：完全な ORT clone

完全な ONNX Runtime clone、すなわち 300 以上の operator、graph optimizer、量子化 suite、複数の execution provider は 5–7 年の開発を要し、あらゆる軸で Microsoft に負ける製品に終わる。また、小さく焦点を絞った package という Almide の哲学にも真っ向から反する。

<a id="rejected-for-now-compile-time-model-specialization-only-shape-d-below"></a>

### 現時点では却下：コンパイル時モデル特殊化のみ（以下の形 D）

最終形である。強力だが、それ以前の形を足場として必要とする。

<a id="viable-first-target-embedding-model-runner-shape-c"></a>

### 実行可能な最初の目標：embedding モデル runner（形 C）

狭く深く取り組む。embedding モデル（sentence-transformers、CLIP、MobileNet）を読み込んで forward 推論を行う。自己回帰 loop なし、KV cache なし、int8 を超える量子化なし。op 集合は `matmul`、`add`、`layernorm`、`gelu`、`softmax`、`mean_pool` と小さいので、runtime も小さく保てる。目標：`almide/embed`、WASM runtime ≤ 150 KB に外部読込みの重みを加える。これにより最小のリスクでアーキテクチャを実証する。

<a id="viable-second-target-tiny-llm-inference-shape-b"></a>

### 実行可能な第 2 の目標：小型 LLM 推論（形 B）

Transformer 推論。matmul、attention、rotary embeddings、RMSNorm、KV cache、int4/int8 量子化をカバーする 15–20 operator。モデル形式は GGUF または safetensors（ONNX protobuf は使わない）。backend は CPU SIMD と WebGPU compute shader。目標：`almide/llm`、TinyLlama 級モデルを動かす WASM ≤ 500 KB。ここで Almide のサイズ上の優位がエンドユーザーに見えるようになる。

<a id="final-target-compile-time-model-specialization-shape-d"></a>

### 最終目標：コンパイル時モデル特殊化（形 D）

```
ONNX / GGUF model
    │
    ▼
Almide model importer (written in Almide)
    │
    ▼
Generated Almide source
  ├─ used operators inlined as functions
  ├─ weights as static data or external Bytes
  ├─ control flow (autoregressive loop, attention) expanded as code
    │
    ▼
Almide compiler (DCE, mono, inline, SIMD emit)
    │
    ▼
tiny WASM (one model, one binary, 30–80 KB)
```

図の訳：ONNX / GGUF モデル → Almide で書いたモデル importer → 生成 Almide ソース（使う operator を関数としてインライン化、重みを静的データまたは外部 Bytes に、制御フロー〔自己回帰 loop、attention〕をコードへ展開）→ Almide コンパイラ（DCE、単相化、インライン化、SIMD 生成）→ 小型 WASM（モデル 1 つ、binary 1 つ、30–80 KB）。

MLC / TVM はコンパイラ stack レベルで似たことを行うが、TVM の C++ 基盤に依存し、特定 hardware を対象にする。Almide はこれを*言語*レベルで、連続した 1 つの toolchain 内で行う。目標：`almide/ml-compile`。

---

<a id="original-competitive-estimates-unversioned-not-a-current-benchmark"></a>

## 当初の競合比較見積り（版指定なし、現在の benchmark ではない）

以下の数値は提案の動機を保持したもの。build 設定、モデル重み、host 接続コードはここでは揃えていない。Almide の形 D は目標であり、実装の測定値ではない。

| Runtime | サイズ | 形 | Almide の形 D との差 |
|---|---|---|---|
| ORT Web | 7–12 MB | 汎用 interpreter、multi-EP | interpreter、巨大 |
| transformers.js | ORT Web backend | HuggingFace JS wrapper | ORT のサイズを継承 |
| WebLLM / MLC | 数 MB | TVM コンパイル、WebGPU 重視 | TVM toolchain に依存 |
| llama.cpp WASM | 約 2–3 MB | 手書き C、GGML、強い int4 量子化 | C で記述、モデル固有ではない |
| candle (Rust) | 約 5 MB | 明快な API、HuggingFace | Rust runtime + std |
| TensorFlow.js | 約 2 MB | 汎用、legacy | graph interpreter |
| **Almide の形 D** | **30–500 KB** | **モデルごとにコンパイル、DCE で除去** | **1 モデル = 1 WASM** |

ブラウザで動く 100 KB 未満の成果物が実現可能か、差別化になるかは、再現可能な試作と範囲を限定した比較が必要であり、ここでは立証していない。

---

<a id="technical-prerequisites"></a>

## 技術的前提条件

これらが揃うまで、どの phase も開始しない。

<a id="1-wasm-simd128-intrinsics"></a>

### 1. `wasm-simd128` intrinsics

SIMD 作業を選ぶ前に対象モデルを測定する。旧 path `crates/almide-codegen/src/emit_wasm/` は削除済みで、現在の binary Wasm 生成は `crates/almide-wasm/` にある。SIMD 提案はそこで対応操作と host 要件を指定しなければならない。128-bit vector は f32/i32 lane を 4 つ、または i8 lane を 16 個保持するのであり、f32 lane を 16 個ではない。旧 4–10× 見積りは workload に依存しない高速化保証ではない。

**当初の優先度：** 最優先。依存順序は依然として提案であり、選択した workload に照らして確認しなければならない。

<a id="2-large-static-data-handling"></a>

### 2. 大きな静的データの扱い

以下の当初の host / data loading 概略は、実装済み ML interface ではない。重みを埋め込むか読み込むかは、モデルサイズと起動の測定で決めなければならない：

- 重みを別ファイルに置き、実行時に `Bytes` buffer へ読み込む。
- 提案する view は不要なコピーを避ける。`bytes.data_ptr` は削除済み。現在の外部メモリ設計では生 pointer ではなく [Handle / Region](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md) の所有権・境界規則を使わなければならない。
- host の loading/mmap interface は仕様化と実装が必要である。ここでの `bytes.load_file` は拡張案であり、現行 API ではない。

当初の obsid vertex buffer への類推は、大型モデル読込みが実装済みである、すべての経路で zero-copy である、小さな拡張である、という証拠ではない。

<a id="3-quantization-support"></a>

### 3. 量子化対応

int8 / int4 / q4_0 / q8_0 の block ごとの layout は、現代の小型モデル量子化の主流である。Almide にはこれらの layout の効率的表現（おそらく型付き `Bytes` view による）と逆量子化 kernel が必要であり、両方が SIMD に依存する。

---

<a id="phases"></a>

## 段階

<a id="phase-0--research-and-poc-12-months"></a>

### Phase 0 — 調査と PoC（1–2 か月）

- Almide で scalar matmul を実装し、手書き C WASM と benchmark 比較する。
- 目標：C の 2–3× 以内。Almide が 10× 遅ければコンパイラのボトルネックを調べる。
- Almide コンパイラへ `wasm-simd128` intrinsics を追加する。
- smoke test として MobileNet の 1 layer を端から端まで動かす。

<a id="phase-1--almideembed-34-months"></a>

### Phase 1 — `almide/embed`（3–4 か月）

- Almide で書く Safetensors loader。
- Operator：`matmul`、`add`、`layernorm`、`gelu`、`softmax`、`mean_pool`。
- `sentence-transformers/all-MiniLM-L6-v2`（22 M parameters）を端から端まで動かす。
- 目標：WASM runtime < 150 KB、重みは外部ファイルから読み込む。
- 最初の出荷 release：`almide/embed` v0.1.0。

<a id="phase-2--almidellm-69-months"></a>

### Phase 2 — `almide/llm`（6–9 か月）

- Rotary embeddings、KV cache、自己回帰 decode loop、grouped-query attention。
- int8 と int4 の量子化 kernel。
- `TinyLlama-1.1B` または `Qwen2.5-0.5B` を端から端まで動かす。
- 目標：WASM runtime < 500 KB に量子化済み重みを加える。

<a id="phase-3--almideml-compile-12-months"></a>

### Phase 3 — `almide/ml-compile`（12+ か月）

- Almide で書くモデル importer（Safetensors または ONNX subset）。
- forward pass を表す Almide ソースを生成する。
- 既存 Almide の DCE、単相化、インライン化で未使用 operator を除去する。
- 目標：コンパイル済みモデルごとに 30–80 KB。

---

<a id="scope-out"></a>

## 範囲外

- **学習。** 推論のみ。学習は PyTorch に残す。
- **ONNX operator の完全対応。** 現代の transformer には 20 operator で十分。CNN / RNN 対応は第 2 優先。
- **Native CUDA / Metal backend。** まず WebGPU と CPU SIMD。native GPU backend は需要が現れたら後で行う。
- **Autotuning。** TVM の問題領域である。対象 workload には手動調整で十分。
- **動的な shape 推論。** 固定 batch と sequence length から始める。

---

<a id="differentiator-axes"></a>

## 差別化の軸

graphics stack と `porta-embedded` と同じ 5 軸。

| 軸 | 立ち位置 |
|---|---|
| Binary size | 等価な workload と比較測定する目標。10–100× の結果は未確立。 |
| LLM による記述 | Dojo 所管の task 比較のための仮説であり、測定済みの普遍的優位ではない。 |
| Zero-copy | host/memory layout の提案。コピーと所有権は経路ごとに測定する必要がある。 |
| Cross-host | 当初の `@extern(wasm, "ml", ...)` host 概略。adapter と ABI 対応は host ごとに実装・検証が必要。 |
| コンパイル時特殊化 | 言語統合経路の提案。新規性と利益には範囲を限定した証拠が必要。 |

---

<a id="relationship-to-porta-embedded"></a>

## `porta-embedded` との関係

両方の取り組みは同じ基盤投資に依存する。薄い WASI host の下で動く Almide guest である。`porta-embedded` は Almide guest に network policy と business rule を任せ、ML 推論は tensor 実行を任せる。以下を共有できる：

- Almide stdlib の WASI Preview 2 bindings。
- `wasm-simd128` intrinsics。
- サイズ最適化 WASM emit pass。
- host runtime の選択（WAMR / wasm3）。

両方が成功すれば、統一した Almide の説明はこうなる：*「小さな WASI guest 1 つが desktop、browser、MCU で同じように動く。MQTT agent でも 3D scene でも transformer の forward pass でも。」*

---

<a id="resume-conditions"></a>

## 再開条件

以下すべてを満たしたら、この項目を `active/` へ移す：

1. graphics stack package が少なくとも 1 つ v1.0 に固定された（コンパイラが十分安定した兆候）。
2. `wasm-simd128` intrinsics が Almide codegen に入った。
3. Phase 0 試作 matmul benchmark が C の 2–3× 以内。

---

<a id="related-roadmap"></a>

## 関連 roadmap

- **`on-hold/porta-embedded.md`** — WASI guest + 小型 host アーキテクチャを共有。
- **`obsid/docs/strategy.md`**（外部 repo）— §C2 で cross-host の設計原則を共有。
