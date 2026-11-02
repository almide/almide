[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-lang-bench.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-lang-bench.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-lang-bench.reader.zh-CN.md)

> 历史仓库命令的非活动、非规范读者快照。权威原文不变。此参考路径和 .reader 文件名不注册代理命令。引用指示只是文档，不授权执行、花费、发布、推送或删除。不自动同步，也未独立验证当前 workflow 或测量主张。代码与注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-lang-bench.md)

<a id="lang-bench-chart-generation"></a>

# 生成 Lang Bench 图表

重新生成 AI Coding Language Benchmark 图表，并可选择追加 Almide 试验。

<a id="regenerate-charts-only"></a>

## 仅重新生成图表

如果 `data.json` 已是最新：

```bash
python3 research/benchmark/lang-bench/plot.py
```

输出 `docs/figures/lang-bench-{time,loc,pass-rate}.png`。脚本也尝试更新对应 README 图片 URL 以避开缓存；当前首页 README 没有此图表章节，因此该步骤可能无操作。

<a id="run-additional-almide-trials"></a>

## 追加 Almide 试验

```bash
# One-time setup
git submodule update --init research/benchmark/lang-bench/upstream

# Run N trials (each ~20-30 min, appended to raw/almide.jsonl)
ruby research/benchmark/lang-bench/runner.rb --trials 10

# Recompute Almide stats in data.json
ruby research/benchmark/lang-bench/aggregate.rb

# Regenerate charts
python3 research/benchmark/lang-bench/plot.py
```

上方原文注释依次意为“首次设置”“运行 N 次试验（每次约20～30分钟，追加到 raw/almide.jsonl）”“重新计算 data.json 中 Almide 统计”“重新生成图表”。

详情见 `research/benchmark/lang-bench/README.md`。

<a id="data-source"></a>

## 数据来源

- `research/benchmark/lang-bench/data.json`——所有语言的汇总统计（plot.py 输入）
- `research/benchmark/lang-bench/raw/almide.jsonl`——仅追加的 Almide 原始试验记录
- 其他15种语言来自 [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench)（Opus 4.6，每种20次试验）
- 本地 runner 请求不带版本的 `sonnet` 别名。应记录解析后的模型及运行来源信息；该别名本身无法确立固定模型版本，也无法说明是否纳入训练数据。

<a id="notes"></a>

## 注记

- 当前公开测量位于 `docs/benchmarks/README.md`。其中带日期的快照图表不同于这些重新生成的实时图表。未记录对应来源信息时，不要用新运行替换历史比较。
- Almide 用橙色强调，其他语言为蓝色。
- 提交信息：`Update lang-bench charts: {brief summary}`
