原文描述元数据：信任层的形式基础——回执逻辑：主张类型、威胁模型、信任基底、反证程序及相对于用途的完整性。

<a id="receipt-logic--受領書の論理"></a>

# Receipt Logic——回执逻辑

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](receipt-logic.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](receipt-logic.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](receipt-logic.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md)

> 原文注记：这是包含宽泛主张模式及后续范围纠正的带日期草案，并非任意执行或程序均已验证的证据。制品 hash 标识字节，不证明见证忠实性、完整内存安全、capability 约束或 sandbox 配置。C-FAITHFUL 表中一般性公式及 native 信任之桥措辞须结合明确范围限制、fixture、decline 和信任边界阅读；样本相等不是对所有程序的证明。107 fixtures、5/n 及季度估计仍是当时主张。C-SAFE 的证书措辞指本文验证机制，不是第三方安全适格性认定。构想中的本地重推导命令予以保留，未执行或独立验证。

> **目标**：形式化细化 [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) 的 L0-L4 和 receipts。把“人类不阅读却信任机器所写软件”定义成一组可反驳主张，固定“使 rustc 以外全部完整”的达成条件。
> **定位**：trust-layer.md 的元层（主张逻辑）。certification-grade.md（CG-4 translation validation）及 completeness-by-construction.md（语义台账）依附于本文 §6。
> **状态**：Draft——起草于 2026-06-11，数字为同日验证 origin/develop 时的数值。
> 2026-09-30 修订（#2760）：证明器为 Rocq（Coq）内核（不是 Lean——回执 `proofs/receipt.sh` 和 `proofs/check.sh` 使用 coqc + coqchk）。C-SAFE / C-PROVEN 绑定到 structural 路径（交付的 wasm）的见证以及制品 SHA-256。

<a id="1-定義--rustc-以外完全の形式化"></a>

## §1 定义——形式化“rustc 以外完整”

- **回执 R(a)**——附于制品 a 的带类型主张集合。每项主张是四元组 **(主张语句, 验证器, 信任基底, 反证程序)**。
- **信任基底 TB(claim)**——消费者接受该主张时，未经验证而信任的事物集合。
- **公理集 Axioms** = { rustc/LLVM、Rocq（Coq）内核 + OCaml 抽取、wasmtime + wasm 规范、OS/硬件、ALS 有效性 }。
- **“使 rustc 以外全部完整”** ≝ 对 R(a) 中每项主张 TB(claim) ⊆ Axioms，且主张台账相对于用途闭合（§6）。

明确的设计约束：**CI/GitHub 不列入公理。** 所有主张都须可在消费者机器上通过 `git clone && make verify` 重新推导。CI 只是礼貌性的预先运行，不是信任依据。

公理的最后一项（ALS 有效性——“规格是否符合我们的意图”）不可证明，只能通过 interp、dojo 和使用经验性确认。**明确不可消除的底层，才有资格将其余全部称为“完整”。**

<a id="2-主張の型システム--論理形式が違うものを混ぜない"></a>

## §2 主张的类型系统——不混合不同逻辑形式

L0-L4 的主张按不同逻辑形式分为 5 类。类型不同，则 TB、验证器、反证程序和消费者不同。混合主张经不起细查。

| 类型 | 逻辑形式 | 验证器 | TB | 对应层级 |
|---|---|---|---|---|
| **C-SAFE** | ∀执行：a 不做 X（制品 a = 由其 SHA-256 指定的字节序列） | 抽取检查器 + Rocq 内核 + almide-verify 判定 structural 路径见证（所有权、名称全域性、capability 上界、调用模式）；bundle 固定 a 的 SHA-256，验证器重算 | Rocq 内核 + 记录纪律（见证 ⟺ 指令序列）+ `to_wasi` + wasm 规范 | L0/L1 |
| **C-REPRO** | compile(s) = a（字节相等） | 重新编译 + diff | 整套工具链 | L2 |
| **C-FAITHFUL** | behave(a) ≡ sem_ALS(s) | interp 三方比较 + translation-validation 性质序列 | ALS + interp + 公理 | L3 |
| **C-PROVEN** | ∀程序：pass P 具有性质 Q（全称、窄范围） | Rocq 内核（coqc + coqchk、`Print Assumptions` 审计、`proofs/check.sh`） | Rocq 内核 | L3 |
| **C-MEASURED** | 关于生成过程的统计主张 | dojo 复现运行 | 公开流程 + 整套固定版本 | L4 |

C-SAFE 是**无需源码、仅凭制品即可验证的最强类型**。C-FAITHFUL 是最重类型，信任基底包含 ALS。回执从轻类型开始叠加（§7）。

<a id="3-脅威モデル--各主張が誰を止めるか"></a>

## §3 威胁模型——每项主张阻止谁

| 威胁 | 内容 | 阻止它的主张 | 健全性承担者 |
|---|---|---|---|
| **T1** 善意但有 bug 的 AI 作者 | 主流情况 | 类型/effect 系统 + 测试 + MSR 循环 | 检查器 |
| **T2** 对抗性作者（被 prompt 注入的 agent） | 故意设计 capability 逃逸 | C-SAFE | 下述 3 层 |
| **T3** 有 bug 的编译器 | 接受 ⟹ 错误代码 | C-FAITHFUL + 性质验证序列 | 验证器（无需信任编译器自身） |
| **T4** 恶意编译器（Thompson 攻击本体） | 自我传播的污染 | C-REPRO → DDC → selfhost | 多样性 |
| **T5** 被污染的验证基础设施 | CI/检查器自身被攻破 | 本地重验证 + 检查器的小型化/多样性 | 消费者 |

<a id="t2-の精密化--l1-の健全性-3-層"></a>

### 细化 T2——L1 健全性的 3 层

为立即回答“作者骗过编译时检查怎么办？”所作的分离：

1. **import 是与 host 接点的一部分。** 未导入的函数不能直接调用，但仅靠 import 名不能限制共享 gateway 内部操作、参数或 host 授予的权限。对应检查 artifact import 与 host 实现/许可。
2. **语言 capability 检查是另一项检查。** 在其模型及分析范围内确认可达性包含于声明中，并非证明 host/FFI 的任意执行语义。
3. **runtime 配置与 native-code trust。** 分开 Wasm host 配置和 native build / dependency / FFI 的信任边界。仅有 manifest/hash 不等于完成 sandbox。

artifact hash 绑定“指向哪个制品”。witness 是否忠实表示该制品执行是另一义务，不作超出 receipt 的 C-FAITHFUL scope 的主张。

<a id="t3-の精密化--検査の非対称性"></a>

### 细化 T3——检查的不对称性

不需要证明十万行编译器。**不验证生成者，而是每次验证输出**（proof-carrying code 的经典不对称性）。信任从“编译器 + rustc”缩小到“小型检查器 + 构建它的 rustc”。

原 Perceus/StackBalance 表是旧 emitter 的检查台账，不可将其用作当前所有编译路径的保证清单。当前证据入口如下：

| 对象 | 证据入口 | 限制 |
|---|---|---|
| Wasm 路径选择 | `src/wasm_route.rs` | structural-only。不可将旧 `--emit-unverified` waiver 当作当前绕行方案 |
| IR pass 不变量 | `crates/almide-codegen/src/pass.rs` | 违反时致命的 postcondition 检查，不是全部翻译忠实性的证明 |
| 证明/接受/字节对应 | `proofs/receipt.sh` | 各 C-PROVEN/C-SAFE/C-FAITHFUL 的对象及 trusted boundary，伴有 fixture 数及 decline |

此表是证据导览，并不主张内核证明在所有普通构建中自动执行。重新推导流程及各证据适用范围见 receipt。

通往“完整”的道路是扩展此台账（§6 U₂），而非证明整个编译器（CompCert 级，已用数据否决）。

<a id="4-二重-rustc-非対称性--native-の信頼は-wasm-経由で導出される"></a>

## §4 双重 rustc 不对称性——native 信任经 wasm 推导

- **native 制品**：rustc 介入 **2 次**（构建编译器 + 构建输出 Rust）+ runtime/rs。
- **wasm 制品**：rustc 介入 1 次（只构建编译器）。从 IR 到字节序列的整条路径由 Almide 所有。

结论 1：**TCB 最小的制品是 wasm。** 先为 wasm 制品附加回执。sandbox/agent 战略与信任战略在此汇聚到相同答案。

结论 2：**xtarget 字节相等关卡（spec/wasm_cross、107 fixtures / 3-way）是信任之桥。** TCB 较大的 native 制品经由“与 wasm 观察等价”引入信任。字节一致既是质量关卡，也是 native 信任推导路径本身。

<a id="5-反証手続き--主張の負空間を公開する"></a>

## §5 反证程序——公开主张的负空间

只有公开反证方法的主张才是资产（不可反驳的主张在信任层是负债）。每种主张类型附上“观察到这个就为假”：

| 主张 | 构成反证的观察 |
|---|---|
| C-SAFE | 在通过证书检查的制品（SHA-256 匹配）中发生双重释放、未定义引用或执行未声明 capability 的 PoC（1 例即可推翻） |
| C-REPRO | 不同 host 上字节不一致（1 例即可推翻） |
| C-FAITHFUL | 已接受程序中 interp/native/wasm 三方不一致 |
| C-PROVEN | 混入 `Admitted` / 公理（`Print Assumptions` 不是 Closed）、coqc / coqchk 失败，或反例程序 |
| C-MEASURED | 按公开流程复现运行，数字不一致 |

发现的反证成为台账下一行（与 bug bounty 相同的动力机制）。

<a id="6-完全性の定義--使用目的に相対化して閉じる"></a>

## §6 完整性的定义——相对于用途闭合

“性质列表覆盖观察语义”并不精确（覆盖绝对语义会扩张到 CompCert 级）。准确说是：

> **完整性(U)** ≝ 用途 U 所需主张集 C(U) 已枚举，每项主张均有验证器，且所有 TB 均包含于公理集中的状态。

- **U₁ = sandbox 内 agent 执行**：
  C(U₁) = { capability 上界、出错时终止行为、确定性、可复现性 }——主要由 C-SAFE/C-REPRO 构成，凭现有资产可能在一个季度内闭合。
- **U₂ = 无监督生产运作**：
  C(U₂) = C(U₁) + C-FAITHFUL 覆盖——§3 性质台账的长征（当前 5/n）+ [completeness-by-construction](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md) 语义台账全部清偿 + CG-1 oracle 反转。

**不说“Almide 完整”，而说“相对于 U₁ 完整，相对于 U₂ 为 5/n”。** 这种相对化使主张可反驳且能渐进达成。

预期异议：“将完整性相对化不是逃避吗？”立场：不相对化的完整性主张不可反驳，而不可反驳的主张才是信任层的逃避。

<a id="7-着手順受領書の積み方--軽い型から"></a>

## §7 着手顺序（叠加回执——从轻类型起）

1. **Receipt v1**（C-SAFE + C-REPRO + C-PROVEN 当前范围）：wasm import 上界 manifest + 字节复现 hash + Coq 证明引用（明确 RC 子系统范围）。以 capability Phase 1-2 为前提。2026-09-30（#2760）：C-SAFE 绑定 structural 构建见证及 `almide build --target wasm` 交付字节的 SHA-256（`proofs/structural-artifacts.sha256`、bundle v2 的 `artifact` 记录），回执指明该台账文件本身的 digest。
2. **Receipt v2**（+ C-FAITHFUL）：CG-1 oracle 反转 + 性质台账公开和扩展。
3. **Receipt v3**（应对 T4）：可复现构建 → 使用 2 个 backend 的 DDC 式交叉验证 → selfhost（旧 `research/selfhost/`、现 `research/spike/v1-mir/`）。针对 rustc 本身的攻击只有在 v1/v2 完成后才有意义。

<a id="trust-layermd-への反映追補-4-点"></a>

## 反映到 trust-layer.md（4 项增补）

修订 trust-layer.md 时纳入（本文保留为其形式基础）：

1. 威胁模型节（T1-T5）
2. 在层级表添加 **TB 列**（接受各层主张时信任什么）
3. 反证程序节（§5）
4. 公理节（明确排除 CI + §4“信任之桥”）

<a id="何をしないか"></a>

## 不做什么

- **绝对语义的完整覆盖**——扩张到 CompCert 级。完整性相对于用途（§6）。
- **隐藏公理**——rustc/LLVM、Rocq 内核、wasmtime、ALS 有效性不会消失。明确不可消除的底层，才有资格将其余称为“完整”。
- **验证器膨胀**——检查应始终比生成简单。验证器过度膨胀，自身就成为下一个信任问题（T5）。用小型化及多样性防御。
