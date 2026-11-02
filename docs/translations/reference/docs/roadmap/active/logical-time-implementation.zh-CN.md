说明：实现蓝图：Op::Charge、fuel ABI、计量克隆、race lowering、门禁。
<a id="logical-time-async--the-implementation-blueprint"></a>

# 逻辑时间 Async — 实现蓝图

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](logical-time-implementation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](logical-time-implementation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](logical-time-implementation.zh-CN.md)

> 非规范性历史实现蓝图固定参照版。不自动同步、未核实当前实现、无独立测量。2026-08-02状态与正文将来阶段并存，保留strict cap/unwind、metered clone、Dyn charge剩余事项。旧race/bounded语法、证明及零全局开销是原文当时主张，不是当前提供或全系统保证。原文ticks/fuel名称混用、不建新unwinding却列unwind为剩余项，均不暗中统一。命令仅引用计划，未执行。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-implementation.md)

> **实现状态（worktree-stage1-charge-probe，2026-08-02）**：已实现 Stage 1 probe / Stage 2 bounded / Stage 3 race / Wave 1 表层统一 / 全部 T2 / T1-3 native Res carrier（CM-1 v0.3 = 3ns/unit、unit-exact 边界 fixture）。剩余 = strict cap + unwind、metered clone、Dyn charge。台账：research/spike/charge-probe/BURNDOWN.md（出现分歧时以此台账为准）。

> 章程：[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md)。语义见 [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md)，证明见 [logical-time-proofs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md)。本文说明“在编译器的哪里，用什么数据结构，如何贯通”——是立足真实代码库的工程设计。

<a id="0-接地--コードベースが既に持っているもの"></a>

## 0. 基础 — 代码库已有的东西

把设计文档词汇翻译为实现词汇，如下所示。

| 设计文档用语 | 实现实体 |
|---|---|
| 共享 MIR | `MirFunction { ops: Vec<Op> }` — **平坦的 op 流**。没有 basic block，以 `IfThen` / `LoopStart` / `LoopBreakUnless` / `LoopEnd` 结构化标记表达控制流（WAT 形状） |
| 两个 renderer | `render_native.rs`（v1 native → Rust 源码）和 `render_wasm/`（→ WAT）。native 仍保留 v0 codegen fallback |
| 证明形态的 validator | `certificate.rs` 系列：把 MIR ops 投影为事件序列，再由**经 Coq 证明的 checker**（`proofs/`、`check_all` / `check_names`）复核的现有框架 |
| exhaustion 传播路径 | lifted effect fn ABI（can-err = 始终 wrap，已统一）。`Err(String)` 是唯一错误通道 |
| 特化机制 | almide-optimize 的 mono（在现有复制维度上加入 fuel） |
| 三点比较基础 | `spec/wasm_cross` fixture 集合 + emit 时 Σ-probe 这一现有证据类别 |

右列全部实际存在，这是后续设计并非纸上谈兵的依据。

<a id="1-opcharge--時計の最小単位"></a>

## 1. `Op::Charge` — 时钟的最小单位

只向 MIR 增加一个 op。

```rust
/// 論理時計の 1 事象。site は CM-1 の charge site 台帳上の安定 id。
/// cost は静的（Const）か、実行時値に線形（Dyn: base + per_unit × len(v)）。
Charge { site: ChargeSiteId, cost: ChargeCost /* Const(u32) | Dyn { base, per_unit, len_of: ValueId } */ },
```

代码注释表示一个逻辑时钟事件：site 是 CM-1 charge-site 台账中的稳定 ID；cost 为静态 Const，或与运行时值线性相关的 Dyn（base + per_unit × len(v)）。

**插入点**（以此布局满足 W1/W2）：

- 函数入口一个（cost = 从入口开始的串行 op 数）。
- `LoopStart` 后紧接一个（cost = 一轮循环的串行 op 数）——所有环必定经过它（W1）。
- 可变成本 intrinsic（list 拼接、string 构造等）之前插入 `Dyn` charge。
- `IfThen` 分支成本明显不对称时，在分支侧追加（初版只从入口和循环开始，粒度与确定 CM 常数在同一 PR 决定）。

插入发生在 **IR→MIR lowering，即 MIR 诞生时**。之后所有 MIR pass（`pipeline*.rs`，包括 `render_wasm_fuse.rs` 等 peephole）都必须**不跨 Charge 融合**。

**细化（章程 §4 支柱 1 的精确实现形式）**：“优化前赋予”的赋予点是 IR→MIR 边界。上游 IR pass（optimize → mono → ir_link）位于 CM-1 **定义内部**。那里修改导致 charge 数变化，就会让固定 fixture 的 `consumed_fuel` 变红——ratchet 捕捉编译器改版带来的语义变化，强制判断 CM 版本。用户能接触的优化旋钮只有两个，且都已覆盖：native `--release` 是 rustc 优化，Charge 渲染为计数器运算，不会消失；`--wasm-opt` 同理，因为计数器进入现有差分 parity gate 的观测范围。

<a id="2-fuel-の実行機構--新しい-abi-を作らない"></a>

## 2. Fuel 执行机制 — 不创建新 ABI

- **wasm**：模块 global `$__ticks: i64`。Charge 渲染为 `global.get → i64.const cost → i64.sub → global.set` 加耗尽分支。`Dyn` 读取 len 后做乘加。
- **native**：runtime 的 thread-local `FUEL: Cell<i64>`。渲染为同构的减法与分支。
- **耗尽通过现有 effect ABI 传播**。计量克隆（§3）渲染成 can-err 形式，耗尽作为带台账常量消息的 `Err`，沿通常的 `?` 链传到 `fan.bounded` / `fan.race` 边界。**不创建**新 unwinding、新返回值约定或新 wrapper 网络——#840/#841 已统一的 ABI 可直接使用。
- 用于 fixture 的 charge trace 观测，**仅在 probe 构建中**在 `$__fuel` 旁加入 trace 累加器（site ID 序列的哈希）。通常构建的 race / bounded 不携带哈希（观测胜负和 Err 已足够）。

<a id="3-計測の特殊化--mono-の追加次元"></a>

## 3. 计量特化 — mono 的追加维度

设计要求只在 bounded region 内计量。实现方式是 mono 的复制维度。

1. checker 从 `fan.bounded` / `fan.race` body 取可达函数集合的闭包（闭包字面量可沿现有 closure table 路径追踪）。
2. 带 `metered` 标志复制此集合（与 mono 为泛型所做的事相同；名称采用 `__fuel$` 前缀等稳定约定）。
3. 只有计量克隆在 IR→MIR 接受 Charge 插入，并以 can-err ABI 渲染。非计量侧与当时保持逐字节相同——**全局开销为零**这一主张由此结构推出。
4. **仅 v1 限制**：native v0 codegen fallback 没有 Charge 概念。因此 bounded region 中使 v1 native render 撞墙的语法成为**编译错误**（与 wasm hard wall 同风格诊断）。fuel 只建在 trust-spine 上。

race 分支再叠加 `speculative` 标志：可能 trap 的 op（div/rem、数组边界）渲染为 checked 形式，把 trap 转为“分支终止记录”（语义中的延迟 trap 裁定）。bounded 只有计量（trap 仍照常立即终止——因为它不投机）。

<a id="4-stage-1----fuel-probe-表面ゼロで反証器を先に立てる"></a>

## 4. Stage 1 — `--fuel-probe`：先建立无表层反证器

特化（§3）是 Stage 2 的重型机制，Stage 1 不创建。隐藏标志 `--fuel-probe` 把**所有函数**按等同计量的方式渲染，在程序结束时以约定的 stderr 行打印 `(consumed_fuel, trace_hash)`。

- fixture：在两个目标运行 `spec/wasm_cross/fuel_probe_*.almd`，比较 **result / consumed_fuel / trace_hash 三点**。一个单位或一个位置的偏差就直接反证 charge-trace 保持。
- 此 probe 构建直接成为面向用户的校准器 **`--ticks-report`**：打印每个 region / 程序消耗的 tick，**并列本机实测时间**（`52,000 ticks (≈0.4ms here)`——由工具连接数字直觉，与 EVM estimateGas 同一种运用形式）。从 Stage 2 起还用同一机制提供 **`almide run --ticks n` / `almide test --ticks n`**：只是用 bounded 包裹 main 的 host 边界预算。数字由 harness / CI 持有，成为不像墙钟那样 flaky 的确定性 hang killer（[ticks-interface-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md) 修正 2）。对首要嫌疑 `render_wasm_fuse.rs` 系 peephole，在同一 PR 加入拒绝跨 Charge 融合的保护。
- **charge certificate**：向 `certificate.rs` 系列追加同形机制——`charge_certificate(f)` 投影 MIR 的 Charge 序列（site ID 有序序列），检查与 render 后再投影一致。它与 ownership certificate → Coq checker 同形，因此未来向 `proofs/` 添加 `ChargeTotality.v`（序列等长、同序）的扩展点也在同处开放。
- 向 `tests/` 添加生成 MIR 上的 property test（随机 op 序列 → 两个 render → trace 一致）。

<a id="5-stage-2--fanbounded-表面と-cm-1-定数の確定"></a>

## 5. Stage 2 — `fan.bounded`：确定表层与 CM-1 常数

- parser：fan v2 head 语法（`fan.bounded(<expr>) { body }`）。只需把 `parse_fan_primary` 的 member-access 分支扩展为 head-args + block 分支。**不需要标签或字面量后缀**——预算只是表达式（`compute.ms(100)`），lexer 不变（ADR-0001）。
- checker：向 `static_dispatch.rs` 的 fan arm 表添加 `bounded`。body 要求 pure（现有 purity 机制）。类型为 `Result[T, String]`，auto-unwrap 遵循已有 race/any/settle 契约。
- lowering：把 body 闭包注册到计量集合 → §3 复制 → region 入口处 `FUEL = min(n, 外側remaining)`（EIP-150 式 min-cap 只需“进入时赋值，退出时从外层减去消耗”两项 op——这就是不需要 region 标签的依据）。式中外側remaining 指外层剩余预算。
- **在此 PR 确定 CM-1 常数**，以 versioned 对象写入契约台账。fixture 用绝对值固定代表程序的 consumed_fuel（ratchet）。

<a id="6-stage-3--fanrace-まず逐次で出荷する"></a>

## 6. Stage 3 — `fan.race`：先以顺序方式交付

语义合流定理保证“顺序 scan 与并行具有相同观测”。换成实现顺序，**初版 race 在两个目标上都可使用顺序 scan**：

1. 按列表顺序运行分支，cap = `min(n, 最良決定事象.time − 1)`（直接复用 Charge 的耗尽分支——无须另一套 cap 机制，只需把 cap 放进 `FUEL`）。式中最良決定事象 指最佳决定性事件。
2. 记录分支终止（Complete / Err / trap 记录 / 耗尽），scan 后以 merge 顺序中最小的决定性事件裁定。若 trap 在可见窗口内，就在那里**重新抛出**记录的 trap。
3. native 线程并行独立为**后续性能路线**。语义已允许并行（已证明），初版不引入并行只意味着推迟不影响观测的优化，没有其他损失。wasm 本来就是顺序的。

这样 Stage 3 实现风险缩至 checked-trap 渲染和 scan driver。E027 修订为签名迁移提示也在同一 PR。

<a id="7-fan-v2-wave-1--既存-desugar-の簡略化として実装する"></a>

## 7. fan v2 Wave 1 — 作为既有脱糖的简化来实现

block 形式 any/settle 实际上比当时的 `desugar_fan.rs` **更简单**。现状追踪 thunk 列表的字面量/let 绑定，再内联恢复 body（#599 路径）。block 形式的 arm 表达式**一开始就在手边**——整套追踪机制不再需要，match 链脱糖可原样复用。mapper 形式 `fan.any(xs, f)` 与 `fan.map` 一样是“数据 + 一个闭包”，不会撞上 `List[funcref]` 墙（fan-v2.md 主张的实现依据）。

修改点：`parser/primary.rs`（head 分支）、`static_dispatch.rs`（arm 表 + thunk 列表形式的 E0xx tombstone）、`desugar_fan.rs`（简化）、`fmt`、interp 桥、8 个 spec 文件迁移、清除 `ExprKind::Await` / `r#async` 遗留死代码。

<a id="8-interp第三の審級の扱い"></a>

## 8. interp（第三裁定方）的处理

almide-interp 遍历 IR（位于 MIR 上游）。fuel 定义在 MIR，所以 Stage 1–3 期间明确 interp **对 fuel 语法弃权**（包含 race/bounded 的程序，3-way oracle 降为 2-way）。不采用在 IR 侧镜像 CM-1 的方案，因为双重定义会导致漂移。在 fixture 注记中让弃权范围机器可读，未来若改为“interp 遍历 MIR”，再取消弃权。

<a id="9-ゲート配線の一覧"></a>

## 9. 门禁接线清单

| 门禁 | 固定什么 | 位置 |
|---|---|---|
| 三点 fixture | 两目标 result + consumed_fuel + trace_hash 一致 | `spec/wasm_cross/fuel_*` |
| charge certificate | render 保持 Charge 序列等长、同序 | `certificate.rs` 系列 + 未来 `proofs/` |
| consumed ratchet | 代表程序绝对 fuel 值（检测编译器改版语义变化） | fixture 内 pin |
| 合流门禁 | 语义模型穷举检查（设计侧回归） | `research/spike/logical-time-race/run-gate.sh` |
| Lean belt | 选择代数 7 个定理 | CI `lean-proofs`（已接线） |
| wasm-opt parity | `-Oz` 不破坏计数器 | 现有 parity gate（仅扩大观测） |
| 契约台账 | CM-1 versioned 对象 + 新 C-NNN 集合 | `contracts.toml` + `check-contracts.sh` |

<a id="10-未決実装が答えを出す点"></a>

## 10. 未决问题（由实现给出答案）

- Charge 粒度（是否只从入口 + 循环开始、分支不对称阈值放在哪里）：由 Stage 1 probe 测量决定，与 CM-1 常数在同一 PR 固定。
- `Dyn` charge 的 per_unit 常数表：同上。
- 计量克隆命名约定和 closure table 中计量 slot 的表示：在 Stage 2 实现 PR 确定（设计答案是“可能流入 region 的闭包有计量变体”）。
- trace_hash 哈希函数：为了 fixture 诊断，应对顺序敏感，而不是简单求和；另设 probe 顺序 dump 模式以定位位置。

任何章节都可由相应实现 PR 覆写，但必须作为本文档的修订进行，不要让设计与实现各持一套真相。
