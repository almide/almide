[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-msr.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-msr.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-msr.reader.zh-CN.md)

> 历史仓库命令的非活动、非规范读者快照。权威原文不变。此参考路径和 .reader 文件名不注册代理命令。引用指示只是文档，不授权执行、花费、发布、推送或删除。不自动同步，也未独立验证当前 workflow 或测量主张。代码与注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-msr.md)

<a id="msr-modification-survival-rate-benchmark"></a>

# MSR（Modification Survival Rate）基准

**MSR 在 [almide/almide-dojo](https://github.com/almide/almide-dojo) 测量，不在此处。**

此 repo 负责编译器正确性：`spec/` 测试、`cargo test`、grammar-lab、lang-bench。Dojo 负责每日 MSR 测量、任务库、恶意提示检测及诊断反馈循环。本 repo 测量页（`docs/benchmarks/README.md`）的数字*由 Dojo* 发布，不应在此重新生成。

以前位于 `research/benchmark/msr/` 的本地测试框架（通过 `claude --model haiku` 驱动25个 Exercism 风格提示）于 2026-08-08 移除。自2026年4月移交 Dojo 后，其 `results/` 一直为空，因此运行它会用本地测量值覆盖 README 中 Dojo 的测量值。可从 git 历史恢复：

```bash
git log --diff-filter=D -- research/benchmark/msr/
```

<a id="to-run-msr"></a>

## 如何运行 MSR

克隆 Dojo 并遵循其 README：

```bash
gh repo clone almide/almide-dojo
```

<a id="still-in-this-repo"></a>

## 此 repo 仍包含

- `/almide-lang-bench`——lang-bench 图表生成（`research/benchmark/lang-bench/`）
- `/almide-native-perf`——native 性能（`research/benchmark/perf/`，由 `scripts/check-perf-ratio.sh` 设门禁）
- `/almide-wasm-size`——WASM 二进制大小
- `/almide-test`——分层测试套件
