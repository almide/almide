原文描述元数据：使用编译时模型特化的小型 ML 推理运行时。

<a id="tiny-ml-inference-runtime"></a>

# 小型 ML 推理运行时

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ml-inference.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ml-inference.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ml-inference.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/ml-inference.md)

> 原文注记：这是搁置中的提案，不是已交付 ML runtime。所有大小、阶段时长、性能目标及竞争特征描述仍是原始估计或主张，不是独立验证的结果。图中的 30–80 KB 与比较表的 30–500 KB 保留不调和；runtime 与权重必须分开报告。原文称前提具备前不开始任何阶段，但 Phase 0 包含添加 SIMD，恢复条件又要求 Phase 0 结果；保留这种阶段安排的张力。不恢复已删除的裸指针 API，也不宣称提议的加载接口及跨 host 支持已经存在。

**优先级：** graphics stack 达到 v1.0 之后
**前提：** Almide codegen 中的 `wasm-simd128` intrinsics、稳定的 `bytes` API、跨 package 边界的 DCE 测量
**原则：** 不为 ML 图写解释器。编译图。输入一个模型，输出一个微型 WASM binary——仅含该模型实际需要的 operator。
**假设：** 针对模型编译可能减少固定 workload 的 runtime overhead。本提案尚未证实所称的大小优势，也未证明竞争系统缺乏模型特化。

> 提议目标：一个模型专属 Wasm 制品，runtime bytes 与权重 bytes 分别报告。最初的 30–80 KB 估计未测量。

---

<a id="thesis"></a>

## 论点

提议范围比复现通用推理 stack 更窄。原 ORT operator / 大小数字是未注明版本的比较，不是已证实不可再降低的下界，也不是当前 benchmark。

有趣的做法是另一条方向：把模型文件视为*源文件*，通过 Almide 编译器而非 runtime interpreter 处理。模型使用的 operator 变成内联 Almide 函数。权重变成静态数据或外部加载的 `Bytes`。未使用的 operator 根本不进入 binary。期望结果是模型特化的可执行文件；其大小仍待测量。

此 roadmap 项描述通往该目标的分阶段路径，从简单 embedding runner 起步，逐渐进入 LLM 推理和完整的 model-as-code 编译。

---

<a id="shape-space"></a>

## 形态空间

此工作有四种可能形态，只有两种可行。

<a id="rejected-full-ort-clone"></a>

### 否决：完整 ORT 克隆

完整的 ONNX Runtime 克隆——300+ operator、图优化器、量化套件、多种 execution provider——需要 5–7 年工程工作，最终产品在所有维度都输给 Microsoft。它也直接违背 Almide 小而专注的 package 哲学。

<a id="rejected-for-now-compile-time-model-specialization-only-shape-d-below"></a>

### 暂时否决：只做编译时模型特化（下述形态 D）

这是最终形态。强大，但需要前面形态作为脚手架。

<a id="viable-first-target-embedding-model-runner-shape-c"></a>

### 可行的首个目标：embedding 模型 runner（形态 C）

范围窄、做得深。加载 embedding 模型（sentence-transformers、CLIP、MobileNet）并运行前向推理。无自回归循环、无 KV cache、不做 int8 以外的量化。op 集很小——`matmul`、`add`、`layernorm`、`gelu`、`softmax`、`mean_pool`——因此 runtime 保持微型。目标：`almide/embed`，≤ 150 KB WASM runtime 加外部加载权重。这以最低风险验证架构。

<a id="viable-second-target-tiny-llm-inference-shape-b"></a>

### 可行的第二个目标：微型 LLM 推理（形态 B）

Transformer 推理，15–20 个 operator 覆盖 matmul、attention、rotary embeddings、RMSNorm、KV cache 及 int4/int8 量化。模型格式：GGUF 或 safetensors（绝不用 ONNX protobuf）。Backend：CPU SIMD 与 WebGPU compute shader。目标：`almide/llm`，以 ≤ 500 KB WASM 运行 TinyLlama 级模型。这时 Almide 的大小优势开始对终端用户可见。

<a id="final-target-compile-time-model-specialization-shape-d"></a>

### 最终目标：编译时模型特化（形态 D）

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

图示翻译：ONNX / GGUF 模型 → 用 Almide 编写的模型 importer → 生成的 Almide 源码（使用的 operator 作为函数内联；权重作为静态数据或外部 Bytes；控制流〔自回归循环、attention〕展开为代码）→ Almide 编译器（DCE、单态化、内联、SIMD 生成）→ 微型 WASM（一个模型、一个 binary、30–80 KB）。

MLC / TVM 在编译器 stack 层面做类似事情，但依赖 TVM 的 C++ 基础设施并面向特定 hardware。Almide 在*语言*层面、同一条连续 toolchain 中完成。目标：`almide/ml-compile`。

---

<a id="original-competitive-estimates-unversioned-not-a-current-benchmark"></a>

## 最初的竞争估计（无版本，不是当前 benchmark）

下列数字保留提案动机。这里未统一 build 设置、模型权重和 host glue；Almide 形态 D 是目标，不是实现后的测量。

| Runtime | 大小 | 形态 | 与 Almide 形态 D 的差距 |
|---|---|---|---|
| ORT Web | 7–12 MB | 通用解释器、multi-EP | 解释器，庞大 |
| transformers.js | ORT Web backend | HuggingFace JS wrapper | 继承 ORT 大小 |
| WebLLM / MLC | 数 MB | TVM 编译、以 WebGPU 为重点 | 依赖 TVM toolchain |
| llama.cpp WASM | 约 2–3 MB | 手写 C、GGML、强 int4 量化 | 用 C 编写，非模型专属 |
| candle (Rust) | 约 5 MB | 清晰 API、HuggingFace | Rust runtime + std |
| TensorFlow.js | 约 2 MB | 通用、legacy | 图解释器 |
| **Almide 形态 D** | **30–500 KB** | **逐模型编译、DCE 裁剪** | **1 模型 = 1 WASM** |

小于 100 KB、可在浏览器运行的制品是否可行或有差异化，需要可复现原型及限定范围的比较；此处未确立。

---

<a id="technical-prerequisites"></a>

## 技术前提

这些条件具备之前，不开始任何 phase。

<a id="1-wasm-simd128-intrinsics"></a>

### 1. `wasm-simd128` intrinsics

选择 SIMD 工作之前先测量目标模型。旧路径 `crates/almide-codegen/src/emit_wasm/` 已删除；当前二进制 Wasm 生成在 `crates/almide-wasm/`。任何 SIMD 提案都必须在那里指定支持的操作与 host 要求。128-bit vector 可容纳四条 f32/i32 lane 或十六条 i8 lane，而不是十六条 f32 lane。旧有 4–10× 估计不是独立于 workload 的加速保证。

**最初优先级：** 最高。依赖顺序仍是提案，必须针对所选 workload 检查。

<a id="2-large-static-data-handling"></a>

### 2. 大型静态数据处理

下列原始 host / 数据加载草案不是已实现 ML 接口。必须通过模型大小及启动测量决定嵌入还是加载权重：

- 权重放在独立文件，运行时加载进 `Bytes` buffer。
- 提议的 view 可避免不必要的复制。`bytes.data_ptr` 已删除；当前外部内存设计必须使用 [Handle / Region](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md) 的所有权/边界规则，而非裸指针。
- host 的 loading/mmap 接口需要制定规格并实现；这里的 `bytes.load_file` 是提议扩展，不是现行 API。

原先与 obsid vertex buffer 的类比，不能证明大模型加载已实现、每条路径都是 zero-copy，或这只是小扩展。

<a id="3-quantization-support"></a>

### 3. 量化支持

int8 / int4 / q4_0 / q8_0 分块布局主导现代小型模型量化。Almide 需要这些布局的高效表示（很可能通过类型化 `Bytes` view）及反量化 kernel。两者都依赖 SIMD。

---

<a id="phases"></a>

## 阶段

<a id="phase-0--research-and-poc-12-months"></a>

### Phase 0——研究与 PoC（1–2 个月）

- 用 Almide 实现 scalar matmul，与手写 C WASM 做 benchmark 比较。
- 目标：在 C 的 2–3× 以内。若 Almide 慢 10×，调查编译器瓶颈。
- 向 Almide 编译器加入 `wasm-simd128` intrinsics。
- 端到端运行 MobileNet 的一层作为冒烟测试。

<a id="phase-1--almideembed-34-months"></a>

### Phase 1——`almide/embed`（3–4 个月）

- 用 Almide 编写 Safetensors loader。
- Operator：`matmul`、`add`、`layernorm`、`gelu`、`softmax`、`mean_pool`。
- 端到端运行 `sentence-transformers/all-MiniLM-L6-v2`（22 M 参数）。
- 目标：< 150 KB WASM runtime，权重从外部文件加载。
- 首个交付 release：`almide/embed` v0.1.0。

<a id="phase-2--almidellm-69-months"></a>

### Phase 2——`almide/llm`（6–9 个月）

- Rotary embeddings、KV cache、自回归 decode loop、grouped-query attention。
- int8 和 int4 量化 kernel。
- 端到端运行 `TinyLlama-1.1B` 或 `Qwen2.5-0.5B`。
- 目标：< 500 KB WASM runtime 加量化权重。

<a id="phase-3--almideml-compile-12-months"></a>

### Phase 3——`almide/ml-compile`（12+ 个月）

- 用 Almide 编写模型 importer（Safetensors 或 ONNX 子集）。
- 生成表示 forward pass 的 Almide 源码。
- 用现有 Almide DCE、单态化和内联裁去未使用 operator。
- 目标：每个编译后模型 30–80 KB。

---

<a id="scope-out"></a>

## 范围外

- **训练。** 只做推理。训练留在 PyTorch。
- **完整 ONNX operator 覆盖。** 现代 transformer 用 20 个 operator 即可。CNN / RNN 支持为第二优先级。
- **Native CUDA / Metal backend。** 先做 WebGPU 和 CPU SIMD。有需求后再做 native GPU backend。
- **Autotuning。** TVM 的问题空间。目标 workload 手动调优已足够。
- **动态 shape 推断。** 先固定 batch 和 sequence length。

---

<a id="differentiator-axes"></a>

## 差异化维度

与 graphics stack 和 `porta-embedded` 相同的五个维度。

| 维度 | 状态 |
|---|---|
| Binary size | 待针对等效 workload 测量的目标；未确立 10–100× 结果。 |
| LLM 编写 | 用于 Dojo 负责的任务比较的假设，不是测得的普遍优势。 |
| Zero-copy | 提议的 host/memory 布局；复制与所有权必须逐路径测量。 |
| Cross-host | 原始 `@extern(wasm, "ml", ...)` host 草案；adapter 和 ABI 支持需要逐 host 实现与验证。 |
| 编译时特化 | 提议的语言集成路径；新颖性及收益需要限定范围的证据。 |

---

<a id="relationship-to-porta-embedded"></a>

## 与 `porta-embedded` 的关系

两条路线依赖同一基础投入：在薄 WASI host 下运行的 Almide guest。`porta-embedded` 让 Almide guest 负责网络策略及业务规则；ML 推理让它负责 tensor 执行。它们可以共享：

- Almide stdlib 中的 WASI Preview 2 bindings。
- `wasm-simd128` intrinsics。
- 大小优化的 WASM emit pass。
- host runtime 选择（WAMR / wasm3）。

若二者成功，统一的 Almide 叙事将是：*“一个微型 WASI guest，在 desktop、browser、MCU 上相同——无论运行 MQTT agent、3D scene 还是 transformer forward pass。”*

---

<a id="resume-conditions"></a>

## 恢复条件

全部满足以下条件时，将本项移至 `active/`：

1. 至少一个 graphics stack package 已冻结为 v1.0（说明编译器足够稳定）。
2. `wasm-simd128` intrinsics 已进入 Almide codegen。
3. Phase 0 原型 matmul benchmark 在 C 的 2–3× 以内。

---

<a id="related-roadmap"></a>

## 相关 roadmap

- **`on-hold/porta-embedded.md`**——共享 WASI guest + 微型 host 架构。
- **`obsid/docs/strategy.md`**（外部 repo）——在 §C2 共享 cross-host 设计原则。
