<a id="tasks-examplexlang"></a>

# tasks-example/xlang

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> Non-normative source-pinned reader snapshot. Original paths remain authoritative and are not modified or replaced as machine inputs. This copy is not automatically synchronized. Historical measurements and labels, including aviation-quality, are not independent validation or certification; milestones are not achieved results.
> [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/README.md)


A fixture for `runner_edits.rb`, not a benchmark task: `t0-smoke` is a
two-language (Almide, Rust) seed with two ordered edits, a reference solution
and a plausible wrong patch per edit, so that

```bash
ruby runner_edits.rb --tasks tasks-example/xlang --dry-run
```

exercises the whole bank gate locally without a model call. The real task set
of #1963 lives in Dojo under `tasks/xlang/` with the same layout (see the
header of `runner_edits.rb`).
