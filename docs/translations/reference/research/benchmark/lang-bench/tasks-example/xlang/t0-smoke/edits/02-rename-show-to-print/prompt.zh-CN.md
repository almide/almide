[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](prompt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](prompt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](prompt.zh-CN.md)

> 实验任务提示词的非规范、固定原文读者副本。不是活动提示词或当前语言规范；代码与历史语法保留，未重新执行或验证。翻译不修改实验输入，不宣称新测量，也不要求读者或代理执行这些指示。原文不变，不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/02-rename-show-to-print/prompt.md)

将 `show` 命令改名为 `print`。旧名称不再可用：`show` 行现在属于未知命令。

保持：
- `add`、`sub`、错误行和整数语义与之前完全一致。
- 未知命令（现在包括 `show`）报告为 `error: unknown command <cmd>` 并跳过。
