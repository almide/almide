# 标准库重新设计（三层）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](stdlib-redesign.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](stdlib-redesign.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](stdlib-redesign.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/stdlib-redesign.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：对照九种语言的标准库三层重新设计——规则、类别清单、各模块角色表、每个公开函数的判定，以及采纳提案转化成的 issue。

设计记录位于 [docs/roadmap/stdlib-redesign/](../../../../../roadmap/stdlib-redesign/README.md)：

- [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)：跨模块规则，涵盖命名、放置、原地写入、失败、转换和兼容性。
- [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)：对照 Rust、Go、Python、TypeScript/JavaScript、Gleam、Elixir、OCaml、Kotlin 和 Swift 的模块清单。
- `L2-<module>.md`：每个模块一页，包含角色比较和每个公开函数的判定（记录当时为1,001个）。
- [README](../../../../../roadmap/stdlib-redesign/README.md)：每个提案及其判定（○ / × / 保留）和对应 issue。
- [目标公开接口](../../../../../roadmap/stdlib-redesign/target-surface.md)：所有采纳提案落地后的标准库——按模块列出每个公开函数重新设计后的签名（记录当时1,001个 → 1,289个），以及每个移除函数的去向。
- [协议金字塔](../../../../../roadmap/stdlib-redesign/protocol-view.md)：参照 Swift 分层的标准协议；将每种类型表示为其遵循的协议加自身函数，并展示协议遵循关系暴露的缺口及由此必须作出的决策。

重命名和移除使用 epoch 15 的弃用窗口，在 epoch 16 移除（R-L0-14）。
