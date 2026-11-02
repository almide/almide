<a id="adr-0016-numeric-determinism-is-one-normative-family-written-once--not-a-property-the-targets-happen-to-share"></a>

# ADR-0016：数值确定性是统一书写一次的规范性家族——不是各目标碰巧共有的性质

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0016-numeric-determinism-is-normative.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0016-numeric-determinism-is-normative.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0016-numeric-determinism-is-normative.zh-CN.md)

> 固定历史决策的非规范性读者副本，以原文为准，不自动同步。保留记录中的测量、比较和实现主张，但未独立重新验证。代码示例及注释逐字保留，包括原有语言。 这里记录规范要求与原文当时的证据主张，不是独立符合性测试、数值证明或航空认证。须区分 sqrt 的正确舍入与其他函数的所述精度上界；有条件讨论 fma 不表示已实现该 API。保留原文背景中“尚未写下三项规则”与五个已批准决策标记两种记述。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0016-numeric-determinism-is-normative.md)

- **状态**：Accepted — 于 2026-08-21 裁定（[#28](https://github.com/almide/als/issues/28)）：五项 ⚖ 决策均按原文批准。保留 ⚖ 标记，以便读者区分哪些句子是决策，哪些是整理。
- **日期**：2026-08-21（同日提出并接受）
- **背景**：浮点是“每个目标逐字节相同”最容易丢失、又最难察觉的领域：NaN 符号位在 x86 与 aarch64 间不同；引擎可能传播 payload，也可能不传播；主机 libm 对 `sin` 的舍入可能不同于随附实现；编译器可能在一个目标上将 `a*b+c` 收缩为一次 FMA 舍入，而在另一个目标上不这样做；进程还可能传入 flush-to-zero 模式。ALS 已按节固定了其中若干项：`float.parse` 正确舍入（ALS-T2、C-024），`float.to_string` 是可往返还原的最短十进制表示（ALS-T13），`float.to_fixed` 对精确二进制值使用中点取偶舍入（ALS-T9），NaN 在每个观察边界规范化（ALS-T2 下的 C-210：`float.to_bits` 返回 `0x7FF8000000000000`），符号及 `min/max` 的 NaN 规则（ALS-T15），排序采用 `-0 < +0` 的 `totalOrder`（ALS-C9），相等比较为 `NaN != NaN`（ALS-M10，collections.md），非有限常量显示为 `inf`/`-inf`/`NaN`（ALS-R4），`-0.0` 带符号显示（ALS-E3），实现跨目标共享单一随附 libm（“依赖主机 libm 不符合规范”，text-and-numbers.md）。尚不存在的是家族整体：读者无法在一处找到数值确定性原则，三项规则从未写下，而航空视角（ADR / greenfield ARCHITECTURE §6.6(b)：“在规范中固定数值确定性——每个边界规范化 NaN，interp 与 backend 采用一个随附 libm/softfloat 策略，±0 和 fmin/fmax 的同值处理规则统一写一次”）将其列为 unit-6 义务，不应让实现自行发明。DO-178C 项目把浮点作为需求项，而不是平台细节（实现的 flight-evidence-gaps 台账中 F7 恰是此类案例：`float.parse` 的 1-ulp 边界偏差，由精确算法关闭）。这也是双仓库顺序下首个能以需求先行方式编写的规范性批次；按此流程，其契约在 `proofs/contract-provenance.toml` 中自然成为 `requirements-first`。
- **决策**：数值确定性是 ALS 中的一个规范性家族——在 `T` 前缀下命名一组章节，各有契约与 fixture，在任何实现更改之前先落在这里。它由已具规范性的上述规则（从家族首节重新引用，不搬移）以及首次写下的以下规则组成：
  1. ⚖ **舍入模式与收缩。** 每个 Float 运算都舍入到最近值，中点取偶；不可观察到其他舍入模式。不做收缩：`a * b + c` 在每个目标上都是两次舍入（wasm 没有融合乘加；原生 backend 不得发射融合乘加，除非源码明确使用 `math.fma`；该函数若存在，须有自己的章节）。
  2. ⚖ **保留次正规数。** 任何目标都不得 flush-to-zero，也不得 denormals-are-zero；`5e-324` 作为最小次正规数解析、输出和计算（ALS-T2 已固定解析规则）。
  3. ⚖ **单一 libm，明确精度。** 每个超越函数及幂函数族成员（`math.sin`、`cos`、`tan`、`exp`、`log`、`pow`、`sqrt`、`tanh`、`atan`、…）都由所有目标共享的单一随附实现计算，绝不使用主机实现。`sqrt` 正确舍入（IEEE 强制要求）；其他函数附带明确的精度上界（≤ 1 ulp，由 fixture 家族逐函数测量并固定，不是“正确舍入”，不作后者声明）。跨目标逐字节相同是规则；与真值的接近程度是所述上界。
  4. ⚖ **有符号零。** 算术按 IEEE 754 保留零的符号；`0.0 == -0.0` 为 true；`-0.0` 显示为 `-0.0`（ALS-E3）；`float.min(-0.0, 0.0)` 为 `-0.0`，`float.max` 为 `0.0`。这是 IEEE 754-2019 的 `minimum`/`maximum` 零排序，与 `totalOrder`（ALS-C9）及 ALS-T15 的忽略 NaN 规则一致（min/max 的两个参数中一个为 NaN → 另一个；两个均 NaN → NaN）。
  5. ⚖ **Float → Int 转换明确其舍入和范围规则。** 每个转换函数用确定的陈述写明截断／舍入，以及越界或 NaN 输入是饱和、报错还是 abort，并在每个边界放置 fixture（`±inf`、NaN、`±2^63`、`-0.0`）。
  6. **每个观察边界都使用规范 NaN**——按 C-210 原文，从家族首节重新引用，以在一处列出边界（`to_bits`、`to_string`、JSON/Value 编码、Float 为键时的哈希），并给每个边界配置 fixture。
  家族首节用一句话陈述原则：*Float 计算的可观察结果仅是程序本身的函数*，并列出每个成员章节。
- **理由**：(1) 这些规则在实现中作为行为存在，在分散章节中作为文字存在；航空审查者要求的是整个家族，而不是各成员。(2) 先在这里书写，是让来历台账出现有一定规模的 `requirements-first` 批次的唯一方式；目前全部只有 C-300/C-301。(3) 每项 ⚖ 规则都是 wasm 引擎与原生 backend 间可能悄然分歧的规则；尤其收缩和 FTZ 是编译器／进程标志，而非语言语义，不禁止它们的规范就没有理由把分歧称作 bug。(4) “单一 libm，明确精度”是诚实的；“正确舍入的超越函数”则是无人能为此实现支撑的声明。CR-libm 级实现确实存在，但此实现交付的是转写的随附 libm；fixture 家族固定实际存在的性质。
- **备选方案**：(a) 保持数值规则分散于各函数章节，让一致性作判断——否决：一致性看不到两个目标共同具有的偏差（QUALIFICATION.md 限制 2），也看不到从未写下的规则。(b) 要求超越函数正确舍入——目前无依据，暂时否决；下述可推翻条件会重启讨论。(c) cert profile 仅支持定点数，不固定 Float——否决：Float 属于所有使用者的语言，而非仅 bounded profile；ADR-0017 将该 profile 中 Float 的可接纳性作为另一个问题，且依赖本家族存在。
- **后果**：在 `text-and-numbers.md` 中新增 `## ALS-T<n>` 家族首节和约 5 个成员章节、约 6 项契约，并在 `spec/wasm_cross` 为上述每个边界配置 fixture，均在实现固定该提交之前落地。实现随后要么已符合（多数规则），要么增加 gate（不收缩、不 FTZ 属于构建标志审计；libm 精度上界是实测表）。成本是 fixture 工作，以及每个超越函数一次诚实的测量。
- **可推翻条件**：任何目标上某个超越函数的实测误差超过固定上界，就重审规则 3（收紧上界或 libm）。若有文档证明需要第二种舍入模式或 `fma` 语义，则重审规则 1（成为命名函数，绝不成为环境模式）。若此家族落地后，来历台账的 `requirements-first` 数量没有变化，说明双仓库顺序未被遵守；这是流程问题，而非数值问题。
- **参考资料**：IEEE 754-2019 §5.3.1（minimum/maximum）、§4.3（舍入）、§7.2；WebAssembly core spec 数值部分（`fmin`/`fmax` 的 NaN 与零规则；MVP 数值指令集中无 FMA）；ALS-T2/T9/T13/T15、ALS-C9、ALS-E3、ALS-M10、ALS-R4；C-024、C-210；greenfield `ARCHITECTURE.md` §6.6(b)；almide/almide `proofs/libm-determinism-audit.toml`；`docs/roadmap/active/flight-evidence-gaps.md` F7。
