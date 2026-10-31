<!-- description: Tiny ML inference runtime using compile-time model specialization -->
# Tiny ML Inference Runtime

**Priority:** after graphics stack reaches v1.0
**Prerequisites:** `wasm-simd128` intrinsics in Almide codegen, stable `bytes` API, DCE measurement across package boundaries
**Principle:** Don't write an interpreter for ML graphs. Compile the graph. One model in, one tiny WASM binary out — containing only the operators that model actually needs.
**Hypothesis:** model-specific compilation may reduce runtime overhead for a
fixed workload. This proposal has not demonstrated the claimed size advantage,
and it does not establish that competing systems lack model specialization.

> Proposed target: one model-specific Wasm artifact, with runtime bytes and
> weight bytes reported separately. The original 30–80 KB estimate is unmeasured.

---

## Thesis

The proposed scope is narrower than reproducing a general-purpose inference
stack. The original ORT operator/size figures are unversioned comparisons, not
a demonstrated irreducible lower bound or current benchmark.

The interesting move is orthogonal: treat a model file as a *source file* and run it through the Almide compiler instead of through a runtime interpreter. Operators used by the model become inlined Almide functions. Weights become static data or externally-loaded `Bytes`. Unused operators never enter the binary at all. The desired result is a model-specialized executable; its size remains to be measured.

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

## Original competitive estimates (unversioned, not a current benchmark)

The figures below preserve the proposal's motivation. Build settings, model
weights and host glue were not normalized here; Almide Shape D is a target,
not an implemented measurement.

| Runtime | Size | Shape | Gap to Almide Shape D |
|---|---|---|---|
| ORT Web | 7–12 MB | General-purpose interpreter, multi-EP | Interpreter, enormous |
| transformers.js | ORT Web backend | HuggingFace JS wrapper | Inherits ORT's size |
| WebLLM / MLC | Several MB | TVM-compiled, WebGPU-focused | Depends on TVM toolchain |
| llama.cpp WASM | ~2–3 MB | Hand-written C, GGML, strong int4 quant | C-authored, not model-specific |
| candle (Rust) | ~5 MB | Clean API, HuggingFace | Rust runtime + std |
| TensorFlow.js | ~2 MB | General-purpose, legacy | Graph interpreter |
| **Almide Shape D** | **30–500 KB** | **Per-model compile, DCE-stripped** | **1 model = 1 WASM** |

Whether a sub-100-KB browser-runnable artifact is feasible or differentiated
requires a reproducible prototype and a scoped comparison; it is not established here.

---

## Technical Prerequisites

None of the phases start until these exist.

### 1. `wasm-simd128` intrinsics

Measure the target model before selecting SIMD work. The old
`crates/almide-codegen/src/emit_wasm/` path is removed; current binary Wasm
emission is in `crates/almide-wasm/`. Any SIMD proposal must specify supported
operations and host requirements there. A 128-bit vector holds four f32/i32
lanes or sixteen i8 lanes, not sixteen f32 lanes. The old 4–10× estimate is
not a workload-independent speedup guarantee.

**Original priority:** highest. The dependency order remains a proposal and must be checked against the selected workload.

### 2. Large static data handling

The original host/data-loading sketch below is not an implemented ML interface.
Model size and startup measurements must determine whether to embed or load weights:

- Weights live in a separate file, loaded at runtime into a `Bytes` buffer.
- A proposed view would avoid unnecessary copies. `bytes.data_ptr` is removed;
  current foreign-memory designs must use the ownership/bounds rules of
  [Handle / Region](../../adr/0031-raw-pointers-carry-provenance.md), not raw pointers.
- A host loading/mmap interface would need to be specified and implemented;
  `bytes.load_file` here is a proposed extension, not a current API.

The original analogy to obsid vertex buffers is not evidence that large-model
loading is implemented, zero-copy on every route, or a small extension.

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
| Binary size | Target to measure against equivalent workloads; no 10–100× result established. |
| LLM authoring | Hypothesis for a Dojo-owned task comparison, not a measured universal advantage. |
| Zero-copy | Proposed host/memory layout; copies and ownership must be measured per route. |
| Cross-host | Original `@extern(wasm, "ml", ...)` host sketch; adapters and ABI support require implementation and validation for each host. |
| Compile-time specialization | Proposed language-integrated route; novelty and benefit require scoped evidence. |

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
