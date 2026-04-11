<!-- description: Tiny ML inference runtime using compile-time model specialization -->
# Tiny ML Inference Runtime

**Priority:** after graphics stack reaches v1.0
**Prerequisites:** `wasm-simd128` intrinsics in Almide codegen, stable `bytes` API, DCE measurement across package boundaries
**Principle:** Don't write an interpreter for ML graphs. Compile the graph. One model in, one tiny WASM binary out — containing only the operators that model actually needs.
**Differentiator:** Every other browser-side inference runtime (ORT Web, transformers.js, WebLLM, llama.cpp-wasm, candle) ships a *general-purpose* graph executor. Almide compiles the model directly into specialized code, giving a binary size nobody else in the space can match.

> "1 model = 1 WASM. 30–80 KB per model, not 7–12 MB for a runtime that runs any model."

---

## Thesis

ONNX Runtime is the wrong model to clone. It is a general-purpose graph interpreter with ~300 operators, multiple execution providers, and an irreducible 7–12 MB footprint on the browser. Almide cannot out-engineer Microsoft on that axis and shouldn't try.

The interesting move is orthogonal: treat a model file as a *source file* and run it through the Almide compiler instead of through a runtime interpreter. Operators used by the model become inlined Almide functions. Weights become static data or externally-loaded `Bytes`. Unused operators never enter the binary at all. The result is an executable specialized to exactly one model, measured in tens of kilobytes rather than megabytes.

This roadmap item describes the staged path there, starting from a simple embedding runner and graduating to LLM inference and full model-as-code compilation.

---

## Shape Space

Four possible shapes for this work. Only two are viable.

### Rejected: Full ORT clone

A complete ONNX Runtime clone — 300+ operators, graph optimizer, quantization suite, multiple execution providers — is 5–7 years of engineering that ends in a product that loses to Microsoft on every axis. It also directly contradicts the Almide philosophy of small, focused packages.

### Rejected for now: Compile-time model specialization only (Shape D below)

The final form. Powerful, but requires the previous shapes as scaffolding.

### Viable, first target: Embedding model runner (Shape C)

Narrow and deep. Loads an embedding model (sentence-transformers, CLIP, MobileNet) and runs forward inference. No autoregressive loop, no KV cache, no quantization beyond int8. Op set is small — `matmul`, `add`, `layernorm`, `gelu`, `softmax`, `mean_pool` — so the runtime stays tiny. Target: `almide/embed`, ≤ 150 KB WASM runtime plus externally-loaded weights. This proves the architecture with minimum risk.

### Viable, second target: Tiny LLM inference (Shape B)

Transformer inference, 15–20 operators covering matmul, attention, rotary embeddings, RMSNorm, KV cache, and int4/int8 quantization. Model format: GGUF or safetensors (never ONNX protobuf). Backend: CPU SIMD and WebGPU compute shaders. Target: `almide/llm`, ≤ 500 KB WASM to run TinyLlama-class models. This is where Almide's size advantage becomes visible to end users.

### Final target: Compile-time model specialization (Shape D)

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

MLC / TVM do something similar at the compiler-stack level but depend on TVM's C++ infrastructure and target specific hardware. Almide does it at the *language* level, in one continuous toolchain. Target: `almide/ml-compile`.

---

## Competitive Landscape

| Runtime | Size | Shape | Gap to Almide Shape D |
|---|---|---|---|
| ORT Web | 7–12 MB | General-purpose interpreter, multi-EP | Interpreter, enormous |
| transformers.js | ORT Web backend | HuggingFace JS wrapper | Inherits ORT's size |
| WebLLM / MLC | Several MB | TVM-compiled, WebGPU-focused | Depends on TVM toolchain |
| llama.cpp WASM | ~2–3 MB | Hand-written C, GGML, strong int4 quant | C-authored, not model-specific |
| candle (Rust) | ~5 MB | Clean API, HuggingFace | Rust runtime + std |
| TensorFlow.js | ~2 MB | General-purpose, legacy | Graph interpreter |
| **Almide Shape D** | **30–500 KB** | **Per-model compile, DCE-stripped** | **1 model = 1 WASM** |

The empty quadrant — "one model per binary, under 100 KB, browser-runnable" — is uncontested.

---

## Technical Prerequisites

None of the phases start until these exist.

### 1. `wasm-simd128` intrinsics

Matrix multiplication dominates the runtime cost of any inference engine. Scalar matmul is 4–10× slower than SIMD matmul and cannot be covered by compiler tricks alone. Almide's codegen currently emits only scalar operations; `crates/almide-codegen/src/emit_wasm/` needs a v128 type and the 16-lane f32 / i32 / i8 intrinsics surfaced to the language.

**Priority:** highest. Nothing below can start without this.

### 2. Large static data handling

A small transformer has tens to hundreds of megabytes of weights. Embedding them all in WASM data segments is non-viable. The realistic pattern is:

- Weights live in a separate file, loaded at runtime into a `Bytes` buffer.
- Almide treats the buffer as a view, indexing directly via `bytes.data_ptr` + `get_f32_le` without copying.
- The host provides either a `bytes.load_file(path)` extension or mmap-equivalent access.

The mechanism already exists for obsid (zero-copy vertex buffers). Extending it to multi-megabyte weight files is a small addition.

### 3. Quantization support

int8 / int4 / q4_0 / q8_0 block-wise layouts dominate modern tiny-model quantization. Almide needs efficient representation for these layouts (likely via typed `Bytes` views) and dequantize kernels. Both depend on SIMD.

---

## Phases

### Phase 0 — Research and PoC (1–2 months)

- Implement scalar matmul in Almide, benchmark against hand-written C WASM.
- Target: within 2–3× of C. If Almide is 10× slower, investigate compiler bottlenecks.
- Add `wasm-simd128` intrinsics to the Almide compiler.
- Run one layer of MobileNet end-to-end as a smoke test.

### Phase 1 — `almide/embed` (3–4 months)

- Safetensors loader written in Almide.
- Operators: `matmul`, `add`, `layernorm`, `gelu`, `softmax`, `mean_pool`.
- Run `sentence-transformers/all-MiniLM-L6-v2` (22 M parameters) end-to-end.
- Target: < 150 KB WASM runtime, weights loaded from external file.
- First shipped release: `almide/embed` v0.1.0.

### Phase 2 — `almide/llm` (6–9 months)

- Rotary embeddings, KV cache, autoregressive decode loop, grouped-query attention.
- int8 and int4 quantization kernels.
- Run `TinyLlama-1.1B` or `Qwen2.5-0.5B` end-to-end.
- Target: < 500 KB WASM runtime plus quantized weights.

### Phase 3 — `almide/ml-compile` (12+ months)

- Model importer (Safetensors or ONNX subset) written in Almide.
- Generates Almide source representing the forward pass.
- Uses existing Almide DCE, monomorphization, and inlining to strip unused operators.
- Target: 30–80 KB per compiled model.

---

## Scope Out

- **Training.** Inference only. Training stays in PyTorch.
- **Full ONNX operator coverage.** 20 operators suffice for modern transformers. CNN / RNN support is second priority.
- **Native CUDA / Metal backends.** WebGPU and CPU SIMD first. Native GPU backends later if demand appears.
- **Autotuning.** TVM's problem space. Manual tuning is sufficient for the target workloads.
- **Dynamic shape inference.** Start with fixed batch and sequence length.

---

## Differentiator Axes

Same five axes as the graphics stack and `porta-embedded`.

| Axis | Standing |
|---|---|
| Binary size | 10–100× smaller than competing runtimes at the equivalent workload. |
| LLM authoring | Writing a new operator or variant model in Almide is far easier than in C++. |
| Zero-copy | Weights and activations live in linear memory, no FFI serialization. |
| Cross-host | `@extern(wasm, "ml", ...)` keeps the host contract pluggable across browser, native, headless. |
| Compile-time specialization | Shape D is structurally unique — MLC / TVM do it but only at the compiler-stack level, not the language level. |

---

## Relationship to `porta-embedded`

Both tracks depend on the same underlying investment: Almide guests that run under a thin WASI host. `porta-embedded` puts the Almide guest in charge of network policy and business rules; ML inference puts it in charge of tensor execution. They can share:

- WASI Preview 2 bindings in Almide stdlib.
- `wasm-simd128` intrinsics.
- Size-optimized WASM emit passes.
- Host runtime choice (WAMR / wasm3).

If both succeed, the unified Almide story becomes: *"One tiny WASI guest, same across desktop, browser, and MCU — whether it's running an MQTT agent, a 3D scene, or a transformer forward pass."*

---

## Resume Conditions

Move this item to `active/` when all of:

1. At least one graphics stack package has frozen to v1.0 (signal that the compiler is stable enough).
2. `wasm-simd128` intrinsics have landed in Almide codegen.
3. Phase 0 prototype matmul benchmarks are within 2–3× of C.

---

## Related Roadmap

- **`on-hold/porta-embedded.md`** — shares the WASI guest + tiny host architecture.
- **`obsid/docs/strategy.md`** (external repo) — shares the cross-host design principle in its §C2.
