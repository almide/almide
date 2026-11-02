原文 description 元数据：嵌入式 WASI agent runtime 提案；大小及跨 host 目标需要 hardware 证据
<a id="porta-embedded--sub-10kb-almide-iot-agents-on-wasi-hosts"></a>

# Porta Embedded——WASI host 上小于 10KB 的 Almide IoT agent

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](porta-embedded.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](porta-embedded.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](porta-embedded.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/porta-embedded.md)

> 原文注记：这是搁置设计，不是已部署 MCU runtime，也不是独立测得的大小或安全结果。翻译未重新执行原文有日期的外部检查。保留原 desktop 比较及 Porta native process 的明确更正。runtime 候选不代表已选部署；共享 schema 或 WASI 名称不确立等效强制执行。Phase 0 binding 同时列入恢复前提，此规划依赖在这里仍未解决。


**优先级：** `porta` 达到 v1.0 且 graphics stack 稳定之后
**前提：** Almide stdlib 的 WASI Preview 2 sockets binding、`wasi-tls` 提案落地、大小优化 WASM emit pass
**设计目标：** 在实现相同所需 interface 和语义的 desktop 与 MCU host 间复用 Almide agent。“符合 WASI”本身并不确立这种兼容性。
**提议差异化：** 在选定 desktop/MCU host 间不改 guest 即部署。这里未作展示，也未确立市场范围的独占性。

> **未测量目标，而非结果：** guest ≤ 10 KB，host + guest ≤ 110 KB。
> 总量必须包括所选 TLS/network stack、asset 和 platform glue；
> flash 大小与 peak RAM 是不同测量。

<a id="external-baseline-checked-on-2026-10-31"></a>

## 2026-10-31 检查的外部基线

[Porta 的 `123bef31`](https://github.com/almide/porta/blob/123bef319c99610dd94fb73c28f6443c5a03fc73/README.md) 描述 native process sandbox（macOS Seatbelt；Linux Landlock/seccomp/namespaces）以及独立 Wasm mode。它不只是 desktop Wasmtime host。把任何一种 mode 扩展到 MCU 都需要工作，不能继承其安全保证。

下方 Obsid 的 3.8 KB 数字来自 [2026-04-11 战略草案](https://github.com/almide-graphics/obsid/blob/7e31406ee983f43d21cb95ddcb63861bed6c6648/docs/strategy.md)，描述的是使用薄 JS bridge 的 browser WebGL engine。这是历史数据，并非完整 native/embedded graphics runtime 测量。本次 review 没有 build 外部 project 或测量 hardware；恢复条件仍是未满足/未验证，而非自动满足。

---

<a id="thesis"></a>

## 论点

Almide 确有通往 10 KB IoT agent 的路径，但前提是不再试图*成为* RTOS，而是*运行于*提供 TLS、TCP/IP、scheduling 和 crypto import 的 WASI 兼容 host 内。提案把 TLS cipher、certificate parse 和 TCP state 委托给 host。原 64–85 KB engine 估计未确立包含这些 service 的 host 大小。Almide 负责 application layer、MQTT framing、business rule 和 capability policy，即语言的 LLM 编写与 compile-time 特化真正发挥作用的部分。

这是把 `porta` 哲学扩展到 MCU：*由 capability 控制的 WASM guest、提供 service 的 host、可移植 agent binary。*

---

<a id="why-not-a-bare-metal-rtos-clone"></a>

## 为什么不克隆 bare-metal RTOS

下列原规划估计促成了委托 system service 的选择。它们不是数学下界：code size 取决于 feature subset、hardware offload、toolchain 及测量所包含的内容。

| 原估计中的 subsystem | 示例大小（未重测）| 理由 |
|---|---|---|
| AES-128-GCM（software）| ~1.5 KB | S-box + Galois 域乘法 |
| SHA-256（software）| ~0.8 KB | round 常量 + mixing 函数 |
| X25519 | ~2 KB | 多精度整数算术 |
| X.509 DER parser | ~5 KB | ASN.1 decoder + OID table |
| TCP state machine | ~5 KB | 重传 + 拥塞 + sequence |
| MQTT 3.1.1 QoS 2 | ~3 KB | 4-way handshake 状态跟踪 |

原 ~20–25 KB 估计不是已证明的纯 software 下限。可调查的方案：

1. 对 crypto 和 TCP 做 **hardware offload**（STM32 CryptoCell、ESP32 hardware AES、NXP MAC+IP）。可行，但把 agent 绑定到特定 MCU 家族。
2. **委托给 WASI host**，由其已有重型 stack 提供功能。必须在所选 host 上证明可移植性与等效强制执行。

本 roadmap 选择路径 2。

---

<a id="where-almides-size-advantage-comes-from"></a>

## Almide 大小优势来自何处

历史 Obsid/browser 数字未确立旧有相对 Rust 的 5×–8× 比较，也未证明能迁移到嵌入式系统。实际 guest 上待测的候选机制包括：

- **无 libc。** Almide WASM emit 不引入 crt0、printf 或 format parse 机制。
- **whole-program DCE。** 默认执行函数级 dead-code elimination，不会仅因*可能*被调用就让内容留在 binary 中。
- **单态化。** 用 Almide 写一次的 TLS library 特化为实际使用的 ciphersuite；其他 code 路径在 compile-time 消失。
- **compile-time 常量传播。** 比 C 的 `#ifdef` 细得多，配置选择变成直接替换。
- **无 vtable 层。** `effect fn` 提前重写为普通 Result early-return，没有 dynamic dispatch 成本。

Wasm/Thumb-2 密度与优化收益需要同一 program、同一 feature 的测量；此处未确立固定比例或必然抵消的保证。

---

<a id="architecture"></a>

## 架构

```
┌──────────────────────────────────────────┐
│  Almide IoT agent (< 10 KB WASM)          │  business logic, MQTT framing
├──────────────────────────────────────────┤
│  Almide stdlib (0–4 KB)                   │  bytes, string, math
├──────────────────────────────────────────┤
│  WASI Preview 2 imports                   │  sockets, clocks, random, tls
├──────────────────────────────────────────┤
│  Tiny WASI host (separate binary)         │  WAMR / wasm3 + lwIP + mbedTLS
├──────────────────────────────────────────┤
│  MCU hardware                             │  ESP32-S3 / STM32U5 / RP2350
└──────────────────────────────────────────┘
```

图示翻译：Almide IoT agent（< 10 KB WASM，business logic、MQTT framing）／Almide stdlib（0–4 KB，bytes、string、math）／WASI Preview 2 imports（sockets、clocks、random、tls）／微型 WASI host（独立 binary，WAMR / wasm3 + lwIP + mbedTLS）／MCU hardware（ESP32-S3 / STM32U5 / RP2350）。

这是提议的 interface 布局，不是当前可运行 binding 配方。在宣称同一 artifact 能在两个 host 运行之前，必须共同验证 Preview 2/component ABI 支持、adapter、host operation 及 TCP/TLS 语义。

<a id="original-runtime-estimates-not-a-current-compatibility-or-footprint-matrix"></a>

### 原 runtime 估计（不是当前兼容性或占用矩阵）

| Host runtime | 大小 | 注释 |
|---|---|---|
| `wasm3` | ~64 KB | interpreter 候选；完整 host 占用未验证 |
| `wasm-micro-runtime`（WAMR）| ~85 KB+ | Interp + AOT、Bytecode Alliance、更广 WASI 覆盖 |
| wasmtime | ~10 MB | desktop 候选；Porta 还有 native process mode |
| 用 Almide 编写的自定义 runtime | — | 延后；见“范围外” |

WAMR 和 wasm3 仍是候选，等待固定 version 的 interface、flash/RAM 及 hardware 测试；本文不选定部署 runtime。

---

<a id="original-desktopmcu-comparison-porta-architecture-update-above"></a>

## 原 desktop/MCU 比较（Porta 架构更新见上文）

| 维度 | `porta`（现有）| `porta-embedded` |
|---|---|---|
| Target | desktop / server | MCU / IoT device |
| Host runtime | wasmtime（~10 MB）| WAMR / wasm3（~64–85 KB）|
| OS 层 | macOS / Linux + `sandbox-exec` | bare metal，无 RTOS 或薄 RTOS |
| 用途 | AI agent sandbox、Claude Code 控制 | sensor / actuator / telemetry agent |
| Capability | FS / Network / Exec | GPIO / I²C / SPI / Network |
| Manifest | `porta.toml` + `manifest.json` | 相同格式，共享 parser |

共享 manifest 与 capability model 是设计目标。等效强制执行与 artifact 不变的可移植性，需要在每个选定 host 上做正向、负向测试；二者均不能由共享 schema 推导。

---

<a id="phases"></a>

## 阶段

<a id="phase-0--wasi-preview-2-bindings-23-weeks"></a>

### Phase 0——WASI Preview 2 bindings（2–3 周）

- 向 Almide stdlib 添加 `wasi-sockets-tcp`、`wasi-clocks`、`wasi-random` binding。
- 在 desktop wasmtime 上运行 Almide 明文 TCP echo client。
- 目标：证明 Almide ↔ WASI Preview 2 接线端到端可用。

<a id="phase-1--mqtt-publish-agent-poc-34-weeks"></a>

### Phase 1——MQTT publish agent PoC（3–4 周）

- 用 pure Almide 实现 MQTT 3.1.1 QoS 0 publisher（暂不含 TLS）。
- 目标：< 8 KB WASM guest。
- 从 desktop wasmtime 向公开 broker publish，测量 message rate 与 binary size。
- 与等效 C + lwIP 实现比较。

<a id="phase-2--tls-integration-blocked-on-wasi-tls"></a>

### Phase 2——TLS 集成（被 `wasi-tls` 阻塞）

- `wasi-tls` 提案稳定后添加 Almide stdlib binding。
- 使用 host 侧 TLS，在 TLS 上叠加 MQTT。
- 目标：相同 agent logic，WASM guest < 10 KB。

<a id="phase-3--mcu-host-and-first-hardware-run-12-months"></a>

### Phase 3——MCU host 与首次 hardware 运行（1–2 个月）

- benchmark 比较 WAMR 与 wasm3 的大小、RAM 占用及 WASI Preview 2 operation 覆盖。
- 在 ESP32-S3、STM32U5、RP2350 之一上启动所选 runtime。
- 记录 message rate、耗电与 peak RAM。

<a id="phase-4--porta-embedded-public-release-23-months"></a>

### Phase 4——`porta-embedded` 公开 release（2–3 个月）

- 从 `porta` 拆出 `porta-embedded`（独立 repo 或 subdirectory，待定）。
- 统一 `porta.toml` schema，覆盖两种形式。
- agent 示例（sensor publisher、actuator subscriber、telemetry relay）。
- 首个 `v0.1.0` tag。

<a id="phase-5--vendor-hal-wrappers-ongoing"></a>

### Phase 5——vendor HAL wrapper（持续）

- ESP-IDF、STM32Cube、nRF Connect SDK 的薄 adapter。
- host 侧 WAMR 集成 template。
- 让开发体验与 desktop `porta` 保持一致。

---

<a id="differentiator-axes"></a>

## 差异化维度

graphics stack 战略文档与 `ml-inference.md` 使用相同五个维度评估每个新 package。`porta-embedded` 如下：

| 维度 | 状态 |
|---|---|
| Binary size | 目标：guest < 10 KB、host + guest < 110 KB；此处未测量。|
| LLM 编写 | 通过受控编辑任务与 C 比较的假设。|
| Zero-copy | copy 与 ABI serialization 取决于所选 host 边界，须测量。|
| Cross-host | 希望在明确测试过的 desktop/MCU host 上实现同一 artifact 行为。|
| Capability security | 提议的 MCU host 强制执行；desktop 控制并不蕴含它。|

---

<a id="scope-out"></a>

## 范围外

- **编写 bare-metal RTOS。** 不属于此 application/host 分工；上述大小估计并非数学上的不可能性证明。
- **用 Almide 重写 software TLS。** 提案委托 crypto，而非宣称需要新实现。
- **用 Almide 编写自定义 WASI runtime。** 长期有趣，但需要 `no_std` codegen、inline asm、linker section 控制——当时全部缺失。改用 WAMR / wasm3。
- **MCU 上 ML 推理。** 位于 `ml-inference.md`，以后可共享基础设施。
- **MCU 上 graphics。** 位于 `obsid/docs/strategy.md`，以后可共享基础设施。
- **Ethernet / Wi-Fi driver。** 属于 host 责任，不是 Almide 的。

---

<a id="resume-conditions"></a>

## 恢复条件

全部满足以下条件时，将本项移至 `active/`：

1. 现有 `porta` 已发布 v1.0。
2. 至少一个 host runtime 的 WASI Preview 2 sockets 已稳定。
3. Almide stdlib 已有 WASI Preview 2 binding（Phase 0 前提）。
4. 至少一个 graphics stack package 冻结为 v1.0（compiler 足够稳定的信号）。

---

<a id="related-roadmap"></a>

## 相关 roadmap

- **`on-hold/ml-inference.md`**——相同 Almide 差异化维度用于 ML 推理。两条路线都依赖 WASI guest + 小型 host 架构，可共享 `wasm-simd128` intrinsic 和大小优化 codegen 工作。
- **`obsid/docs/strategy.md`**（外部 repo）——graphics stack 战略。其 §C2“抽象 host interface”与 `porta-embedded` 所需设计原则相同。

若三条路线全部落地，Almide 的统一定位将是：**“在 desktop、browser、embedded 上运行相同 code 的微型 WASI guest——适用于 agent、renderer 和推理。”**
