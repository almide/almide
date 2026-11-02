
> 本文是面向读者的参考译文。以上固定提交的原文为权威版本，本译文不修订规范。状态、数字及实现情况均为作出决定时的记录。Accepted 表示设计已采纳，不表示实现已经完成或当前验证已通过。本译文不会自动同步。原文中的外部调研 ../almide-references、QUALIFICATION.md 等均为原文引用，翻译时未另行获取或复核。

# ADR-0014：固定的预期结果必须由机器推导，否则不固定；绝不手工维护

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0014-expectations-are-derived.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0014-expectations-are-derived.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0014-expectations-are-derived.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0014-expectations-are-derived.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。



- **状态**：Accepted（已采纳）
- **日期**：2026-08-20
- **背景**：`spec/wasm_cross` 断言跨目标的一致性，不提交预期输出；诊断语料则在 `meta.toml` 中固定预期结果。参考调研将 Roc 的两部分样例形式（人工编写的前半部分、机器推导的预期结果后半部分、互斥的 check/bless）列为最有力的语料设计之一。QUALIFICATION.md 声明了本仓库的共同漂移盲点：一致性无法发现两个目标以相同方式出错。2026-08-20 的全新实现切换中，还发现相邻会话正在用占位哈希手写黄金结果行——这正是本决定禁止的失败方式。
- **决定**：在本仓库固定预期结果时，必须由工具从结构化数据中推导（当时包括：运行器断言的诊断 `meta.toml` 字段，以及经过检查的求值器输出的 λ_almd 内核一致性轨迹）；禁止手写预期输出文件。`spec/wasm_cross` 向 Roc 形式迁移（为每个样例提交预期轨迹），只能在参考求值器落地时，作为它的输出格式进行，不能提前进行。该求值器是为解决限制 #2 而指定的工具，由 λ_almd belt 发展而来。
- **理由**：预期结果文件的价值恰恰在于其来源。Roc 的关键机制是从报告数据生成 EXPECTED，绝不重新解析或手工编辑。若只采用文件格式而没有推导工具，就会新增 591 个需要手工维护的文件——这正是调研中的负面对照（Koka、MoonBit）所记录的漂移来源。
- **替代方案**：(a) 现在用手工填入的初始预期结果迁移——基于上述理由拒绝。(b) 永不固定轨迹，永远只检查一致性——拒绝：这会让共同漂移的盲点永久存在。
- **后果**：“两部分样例形式”并入参考求值器的工作，不再单独跟踪。在此之前，一致性检查与各实现之外的第三条验证支撑共同承担跨目标正确性的判断，而这一限制继续在 QUALIFICATION.md 中明确声明。
- **反证条件**：如果共同漂移的缺陷进入发布版本，就证明一致性检查加上已声明的限制仍然不足，必须提前推进求值器。
- **参考**：../almide-references 调研（B2，第 2 项）、QUALIFICATION.md 限制 2、C-280（λ_almd 内核一致性）。
