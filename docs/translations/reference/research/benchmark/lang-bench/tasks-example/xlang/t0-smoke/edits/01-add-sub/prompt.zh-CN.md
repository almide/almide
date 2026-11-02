[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](prompt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](prompt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](prompt.zh-CN.md)

> 实验任务提示词的非规范、固定原文读者副本。不是活动提示词或当前语言规范；代码与历史语法保留，未重新执行或验证。翻译不修改实验输入，不宣称新测量，也不要求读者或代理执行这些指示。原文不变，不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/01-add-sub/prompt.md)

添加 `sub <n>` 命令，从当前累计总数中减去 n。

保持：
- `add`、`show` 和错误行的行为与之前完全一致。
- 总数是普通整数：可以为负，绝不截断到某个范围。
- 非数值数量与 `add` 一样，报告为 `error: bad amount <arg>` 并跳过。
