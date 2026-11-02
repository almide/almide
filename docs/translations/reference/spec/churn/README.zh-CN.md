<a id="frees-churn-gates-almide_wasm_frees1"></a>

# 释放 churn 门禁 (ALMIDE_WASM_FREES=1)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[spec/churn/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/churn/README.md)。这是供读者参考的非规范性快照译文，不会自动同步。未对测量值及主张进行独立复核，以原文为准。

用于真实 RC wasm 模式的长循环回收测试样例。不属于默认语料（数百万次迭代），通过以下命令运行：

```
scripts/check-frees-churn.sh
```

脚本要求在其 600 秒 watchdog 内成功完成，并将捕获的 native 输出与 Wasm 输出（合并了 stderr）比较。它不测量 RSS，而且 shell 命令替换会删除末尾换行，因此这不是完整的逐字节 stdout/stderr/退出状态比较。仅靠语料曾两次让仍存在实际 double-free 的构建通过；这些形态承担了语料无法承担的门禁作用。

- record_churn.almd — 2M 次构造＋销毁，带有新建的字符串字段（free-list 重用、sentinel/resurrection 防护带）。
- tco_loop_churn.almd — 200k 次随循环变化的 TCO 调用（暴露 M2 managed-param OOB 的形态；每次迭代 TCO 回收的验收测试）。

复合递归 drop 样例最初是 v1 stage-2 迁移基线，针对当时的 `emit_typed_rc_dec` 分支和提议中的扁平 MIR Drop 节点。该迁移理由属于历史背景；这些样例仍是当前结构化 Wasm 路径的回收回归测试。每个样例都刻意避开 `?? ` (UnwrapOr)，使其与尚未解决的 #643 some-box 泄漏保持不同形态。

- map_entry_churn.almd — 200k 次构造＋销毁 Map[String, String]（分别销毁堆上的键和值）。
- closure_env_churn.almd — 200k 次构造＋销毁捕获堆上 String 的闭包（环境必须递归 drop 捕获值）。
- nested_named_churn.almd — 200k 次构造＋销毁嵌套 List[String] 的记录（record → list → string 递归 drop）。
