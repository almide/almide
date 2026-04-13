<!-- description: Whisper speech recognition implemented entirely in Almide -->
# Whisper in Pure Almide

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

- **Phase 1-3**: 実装完了、Rust/WASM 両ターゲットで全テスト通過 (nn/tensor 13, activations 13, attention 5, wav 7, fft 6, mel 7, gguf 9 = 60 tests)
- **Phase 4+**: 未着手

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
