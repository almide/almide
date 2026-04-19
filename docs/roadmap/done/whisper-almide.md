<!-- description: Whisper speech recognition implemented entirely in Almide -->
<!-- done: 2026-04-20 -->
# Whisper in Pure Almide

## Completion status (2026-04-20)

Phase 1-5 完遂 (2026-04-13):
- native (Rust target): ggml-tiny.bin + hello.wav → 正確転写 (~16s)
- WASM: 同一パイプラインを Almide→WASM 後 Node.js (WASI) で実行
  - hello.wav → `" Hello World, this is a test of whisker speech recognition."` (36s, 30 tokens)
  - jfk.wav → `" And so my fellow Americans ask not what your country can do for you ask what you can do for your country"` (47s, 30 tokens)

決定的だった修正: `gelu` の tanh `(e^2x-1)/(e^2x+1)` で `Inf/Inf=NaN` → inner を `[-20, 20]` にクランプ。

Phase 6 (optimization — WASM SIMD matmul / quantization / ndarray・Burn backend) は arc の success criteria に含まれない optional。「速度は runtime swap から、コード変更ではない」という方針に従い、別アークとして立てる場合のみ再着手。

Memory: `project_wasm_whisper.md` に詳細。

## Goal

Implement OpenAI Whisper inference entirely in `.almd` — no C FFI, no external ML framework. Prove that Almide can handle real ML workloads.

Target: Whisper tiny (39M params), ~1 minute of English audio → text.

## Architecture

```
┌─────────────────────────────────────────────┐
│  whisper.almd                               │
│                                             │
│  audio.wav                                  │
│    ↓                                        │
│  ① Audio loading (PCM from WAV)             │  ← fs + bytes
│    ↓                                        │
│  ② Log-Mel spectrogram (FFT → mel → log)    │  ← nn.fft + math
│    ↓                                        │
│  ③ Encoder (4 layers, multi-head attention)  │  ← nn.attention + nn.linear
│    ↓                                        │
│  ④ Decoder (4 layers, cross-attention)       │  ← nn.attention + nn.linear
│    ↓                                        │
│  ⑤ Token → text (BPE decode)                │  ← nn.tokenizer
│    ↓                                        │
│  "Hello, world."                            │
└─────────────────────────────────────────────┘

Dependencies:
  matrix (stdlib)  ← matmul, transpose, scale, map, add — ALREADY EXISTS
  nn (new package) ← neural network building blocks on top of matrix
  lumen (existing) ← math constants
```

## Status (2026-04-13)

- **Phase 1-5 完了**: WASM Whisper エンドツーエンド成功
  - native: ggml-tiny.bin + hello.wav → 正確な転写 (~16s)
  - WASM: 同じパイプラインを Almide→WASM コンパイル後 Node.js (WASI) で実行
    - hello.wav → `" Hello World, this is a test of whisker speech recognition."` (36s, 30 tokens)
    - jfk.wav → `" And so my fellow Americans ask not what your country can do for you ask what you can do for your country"` (47s, 30 tokens)
  - 鍵となった修正: gelu の tanh `(e^2x-1)/(e^2x+1)` で `Inf/Inf=NaN` 発生 → inner を [-20,20] にクランプ
  - matrix WASM 演算追加: `gather_rows`, `row_dot`, `from_bytes_f64_le`, `conv1d`, `multi_head_attention`, `softmax_rows`, `layer_norm_rows`, etc.
  - bytes WASM 演算追加: `read_u32_le`, `f16→f64`, `append_*_le`, etc.
- **Phase 6** (最適化): 未着手。WASM SIMD matmul, quantization, ndarray/Burn backend

## Dependencies

- **Codegen 基盤**: [codegen-ideal-form](./codegen-ideal-form.md) — Phase 4 以降で必要になる深層ネットワーク実装では、今の emit 層の弱さ (関数解決の曖昧さ、stdlib 関数の個別実装) がボトルネックになる見込み。同時並行で進める。

## Roadmap

### Phase 1: nn foundations (matrix extensions)

Build the missing matrix operations and neural network primitives.
These are useful beyond Whisper — any ML inference in Almide needs them.

| Component | What | Built on | Tests |
|---|---|---|---|
| **1a. matrix extensions** | `slice`, `concat_rows`, `broadcast_add`, `sum_rows` | matrix stdlib | unit tests with known values |
| **1b. activations** | `softmax`, `gelu`, `relu`, `layer_norm` | matrix.map + sum_rows | compare with reference values |
| **1c. linear layer** | `linear(x, weight, bias)` = matmul + broadcast_add | matrix.mul + broadcast_add | forward pass test |
| **1d. attention** | single-head + multi-head attention | linear + softmax + matmul | small known-input test |

### Phase 2: Audio preprocessing

| Component | What | Built on |
|---|---|---|
| **2a. WAV reader** | Parse WAV header, extract PCM samples | fs + bytes |
| **2b. FFT** | Radix-2 Cooley-Tukey FFT | math (sin, cos) |
| **2c. Mel filterbank** | 80 mel-scale triangular filters | FFT output |
| **2d. Log-Mel spectrogram** | Full audio → Matrix pipeline | 2a + 2b + 2c |

### Phase 3: Model loading

| Component | What | Built on |
|---|---|---|
| **3a. GGUF parser** | Read GGUF file header, tensor metadata | fs + bytes |
| **3b. F16 → F64 conversion** | Dequantize half-precision weights | bytes + int bitwise |
| **3c. Weight loader** | Map GGUF tensors to Matrix values | 3a + 3b + matrix.from_bytes |

### Phase 4: Transformer architecture

| Component | What | Built on |
|---|---|---|
| **4a. Encoder block** | self-attention + FFN + layernorm | Phase 1 components |
| **4b. Decoder block** | causal self-attention + cross-attention + FFN | Phase 1 + encoder output |
| **4c. Full encoder** | Stack 4 encoder blocks | 4a |
| **4d. Full decoder** | Stack 4 decoder blocks + token embedding | 4b |

### Phase 5: Decoding

| Component | What | Built on |
|---|---|---|
| **5a. BPE tokenizer** | Byte-pair encoding vocabulary + merge rules | string + map |
| **5b. Greedy decode** | argmax loop with stop token | decoder + tokenizer |
| **5c. Integration** | audio → encoder → decoder → text | all above |

### Phase 6: Optimization (optional)

| Component | What |
|---|---|
| **6a. Quantized inference** | Q8/Q4 matmul using int bitwise ops |
| **6b. WASM SIMD matmul** | v128 vectorized matrix.mul |
| **6c. ndarray backend** | Swap matrix runtime for ndarray (Rust target) |
| **6d. Burn backend** | GPU inference via Burn (--target cuda) |

## Principles

1. **One piece at a time.** Each component has its own tests and works in isolation.
2. **Pure Almide.** No `@extern(c)`, no FFI. stdlib matrix + math only.
3. **Reference values.** Every numeric test compares against values from PyTorch reference.
4. **Reusable.** The `nn` package is not Whisper-specific — it's a general neural network toolkit.
5. **Slow is OK.** Correctness first. Speed comes from runtime swaps (ndarray, burn), not code changes.

## First step

Phase 1a: matrix extensions (`slice`, `concat_rows`, `broadcast_add`, `sum_rows`).
Package: `nn` at `github.com/almide/nn`.
