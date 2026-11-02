<a id="tasks-examplexlang"></a>

# tasks-example/xlang

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 非规范、固定原文的读者快照。原路径仍为权威来源，不修改或替换机器输入。本副本不自动同步。历史测量及 aviation-quality 等名称不代表独立验证或认证；目标不等于已实现结果。
> [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/README.md)


这是 `runner_edits.rb` 的测试夹具，而非基准任务。`t0-smoke` 是两种语言（Almide、Rust）的初始代码，含两次有序编辑，以及每次编辑的参考解和看似合理的错误补丁。因此，

```bash
ruby runner_edits.rb --tasks tasks-example/xlang --dry-run
```

可在本地运行整个任务库门禁，无需调用模型。#1963 的实际任务集在 Dojo 的 `tasks/xlang/`，布局相同（见 `runner_edits.rb` 文件开头）。
