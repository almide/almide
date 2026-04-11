<!-- description: Tiny ML inference runtime using compile-time model specialization -->
# Tiny ML Inference Runtime

**Status:** on-hold / draft memo
**Priority:** after graphics stack reaches v1.0
**Prerequisites:** wasm-simd128 intrinsics in Almide codegen, stable `bytes` API, DCE measurement across package boundaries

このドキュメントは「Almide で ONNX Runtime 相当を作るはどうだ」という問いに対する戦略メモ。結論は **「literal な ORT クローンは避ける、ただし Shape D (compile-time model specialization) として進めれば Almide 独自のニッチがある」**。今すぐ着手すべきではないが、graphics stack が安定したあとの第2の柱候補として保存しておく。

---

## 1. 取り得る 4 つの形

### Shape A — 完全な ORT クローン (却下)

- ONNX 300+ ops、全 Execution Provider、graph optimizer、quantization suite をフルに実装
- **見積もり:** 5〜7 年の engineering
- Almide の tiny/focus 哲学と真逆、ORT に勝てる軸がない
- **判定:** **やめろ**

### Shape B — Tiny LLM inference runtime (GGML-like)

- Transformer 推論に限定
- 必要 op は 15〜20 程度: `matmul, softmax, layernorm, rmsnorm, rotary, attention, gelu, silu, add, concat, reshape, embed_lookup`
- Model format: **GGUF / Safetensors** (ONNX protobuf は避ける)
- Backend: CPU SIMD + WebGPU compute shader
- **目標サイズ:** < 500 KB WASM (llama.cpp.wasm の ~1/5)
- **判定:** Almide 哲学と一致、でかい勝負ができる

### Shape C — 狭く深く、embedding model runner だけ

- sentence-transformers / 画像 embedding / 画像分類限定
- 自己回帰ループ無し、KV cache 無し、quantize は int8 程度
- 対応モデル: `all-MiniLM-L6-v2` (22M), CLIP, MobileNet
- **目標サイズ:** < 150 KB WASM
- 用途: semantic search, RAG, on-device similarity
- **判定:** 最も安全な第一歩、需要が明確

### Shape D — Compile-time model specialization (本命)

Almide の AOT 性をそのまま ML inference に持ち込む。ORT が graph **interpreter** なのに対し、Almide は graph **compiler** になる。

```
ONNX / GGUF model
    │
    ▼
Almide model importer (written in Almide)
    │
    ▼
Generated Almide source
  ├─ used ops のみ inline
  ├─ weights を data segment or 外部 bytes として埋め込み
  ├─ control flow (autoregressive loop, attention) もコードで展開
    │
    ▼
Almide compiler (DCE, mono, inline, SIMD emit)
    │
    ▼
tiny WASM (1 model = 1 binary, 使わない op は 1 byte も入らない)
```

- 類似思想: MLC / TVM (Python で書かれた compiler stack)
- 違い: Almide は言語から一貫。ユーザーが model-as-code を Almide で書き換え可能 (LLM authoring と完全一致)
- **目標サイズ:** 30〜80 KB per model
- **判定:** 唯一無二のニッチ、Almide の 5 軸がフルに乗る

---

## 2. 競合マップ

| 選手 | サイズ | 特徴 | Shape D との差 |
|---|---|---|---|
| ORT Web | 7〜12 MB | 汎用 interpreter、多 EP | interpreter、巨大 |
| transformers.js | (ORT Web を呼ぶ) | HF モデル JS runner | ORT 依存 |
| WebLLM / MLC | 数 MB〜 | TVM コンパイル、WebGPU 特化 | TVM 依存 |
| llama.cpp WASM | ~2〜3 MB | C、GGML 派生、int4 quant 強い | C 手書き、model hardcoded |
| candle (Rust) | ~5 MB | HuggingFace 公式、clean API | Rust runtime + std |
| TensorFlow.js | ~2 MB | 汎用、老舗 | graph interpreter |
| **Almide Shape D** | **30〜500 KB** | **Almide code gen、モデル単位で compile** | **1 model = 1 WASM** |

**空白地帯:** 「モデル 1 個あたり 50 KB 以下で browser で動く inference」— ここは誰もいない。

---

## 3. 技術前提 (prerequisites)

どれも未達なら Phase 0 research が必要。

### 前提 1: Almide に wasm-simd128 intrinsics

matmul / convolution / dot product の速度は SIMD ありきで決まる。

- 現状: `crates/almide-codegen/src/emit_wasm/` は scalar のみ
- 必要: `v128` 型と 16-lane f32 / i32 / i8 の intrinsic を Almide 言語レベルで露出
- 影響: compiler 側の工事 (op code emit + stdlib binding)
- **優先度:** 最高。これなしでは Shape B/C/D どれもスタートできない

### 前提 2: 巨大な静的データの扱い

モデル weights = 数 MB 〜 数百 MB。WASM data segment に全部入れるのは非現実的。

- 現実解: 別 file で bytes として runtime load、Almide 側で view として扱う
- 既存: `bytes.data_ptr` + `bytes.set_f32_le` / `get_f32_le` のパターン (obsid が既に使っている zero-copy)
- 追加必要かもしれないもの: `bytes.load_file(path)` (host に延長)、mmap 相当 API

### 前提 3: Quantization 対応

現代 tiny model は int8 / int4 / q4_0 / q8_0 の block-wise quantization layout を使う。

- 必要: Almide に int8 / uint8 tensor の効率的な扱い、dequantize kernel
- SIMD ありき (前提 1 に依存)

---

## 4. 段階的ロードマップ

### Phase 0 — Research PoC (1〜2 ヶ月)

- Almide で scalar matmul 実装、C WASM matmul と比較
  - 2〜3x 遅い程度なら SIMD 追加で埋まる範囲
  - 10x 以上なら compiler 側に別問題あり
- wasm-simd128 intrinsic を Almide compiler に追加
- MobileNet の 1 層を Almide で再現して走らせる (end-to-end feasibility check)

### Phase 1 — `almide/embed` (3〜4 ヶ月)

- Safetensors loader
- Op: `matmul, add, layernorm, gelu, softmax, mean_pool`
- `all-MiniLM-L6-v2` を完走
- **目標:** < 150 KB WASM runtime + 22 MB external weights

### Phase 2 — `almide/llm` (6〜9 ヶ月)

- Rotary, KV cache, autoregressive loop, GQA attention
- int8 / int4 quantization
- TinyLlama-1.1B / Qwen2.5-0.5B 等を完走
- **目標:** < 500 KB WASM + quantized weights

### Phase 3 — `almide/ml-compile` (12 ヶ月〜)

- ONNX / Safetensors → Almide source generator
- 使用 op の DCE、model-specific optimization pass
- **目標:** 1 model = 30〜80 KB

---

## 5. スコープアウト (やらないことを最初に決める)

- **Training** — inference のみ。training は PyTorch に任せる
- **ONNX op 完全 coverage** — Transformer に必要な ~20 op で止める。CNN/RNN は second priority
- **CUDA / Metal 独自 backend** — browser WebGPU + CPU SIMD からスタート。native GPU backend は後
- **Autotuning** — TVM の領域。手動チューニングで十分
- **Dynamic shape** — 固定 batch / seq_len から始める。dynamic は後

---

## 6. Almide ML の差別化軸 (graphics stack と同じ 5 軸)

| 軸 | Almide ML の強み |
|---|---|
| Binary size | Shape D なら 1/10〜1/100 — 破壊的 |
| LLM authoring | 新規 op / モデル変種を LLM に書かせやすい (C++ より Almide) |
| Zero-copy | linear memory 直書き、tensor → FFI 変換無し |
| Cross-host | `@extern(wasm, "ml", ...)` で browser / native / headless 同契約 |
| Compile-time specialization | 他の runtime は MLC/TVM のみ、Almide は言語から一貫 |

**graphics stack と完全に同じ 5 軸で勝負できる。** Almide の核心思想が graphics と ML 両方に延びる、という意味でこの方向は示唆深い。

---

## 7. 戦略上の順序

1. **現在:** Almide compiler + graphics stack (obsid, canvas2d, etc.) を v1.0 付近まで固める
2. **次:** compiler に wasm-simd128 intrinsic を追加 (compiler roadmap へ)
3. **次の次:** `almide/embed` で試作。最小リスクで Almide ML inference の可能性を証明
4. **成功したら:** `almide/llm` → `almide/ml-compile` へ発展

**避けるべき誤り:**

- 「ORT クローンやる」と言って Shape A に突っ込むこと (全敗する)
- graphics stack と ML を同時進行で中途半端にすること
- SIMD 無しで matmul を書いて遅さに絶望すること

---

## 8. Graphics stack との関係

同じ `@extern(wasm, ...)` 抽象契約の上に graphics と ML を乗せる、という意味で思想的には 1 本の柱。

```
Application (Almide)
  │
  ├─ graphics layer: obsid / canvas2d / chart / graph / ui
  │     └─ gfx (mesh, texture, shader, FBO)
  │
  ├─ ml layer: embed / llm / ml-compile
  │     └─ tensor (bytes view, SIMD kernel, quantize)
  │
  └─ shared: bytes, math, wasm-webgl
        │
        ▼
   host via @extern(wasm, ...)
   (browser | native | headless)
```

graphics stack strategy doc (obsid repo の `docs/strategy.md`) で論じている C2 (abstract host interface) と同じ設計原則を ML 側にも適用する。**cross-host moat は graphics と ML の両方で効く**ので、strategic bet として補強し合う関係になる。

---

## 9. 保留理由 (なぜ今ではないか)

- graphics stack の 論点 D1〜D7 (obsid strategy doc 参照) がまだ詰まっていない
- Almide compiler が SIMD をまだ emit できない
- Almide 本体の stdlib / package system / v1.0 凍結ポリシーが未確定
- ML inference は 6〜12 ヶ月フル commit が必要な領域、graphics と並列では不可能

**再開条件:**

1. graphics stack で少なくとも 1 パッケージが v1.0 凍結される
2. wasm-simd128 intrinsic が Almide compiler に landing
3. Phase 0 PoC に着手できる時間的余裕がある

これらが揃ったら `active/` へ移動。
