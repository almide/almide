[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](prompt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](prompt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](prompt.zh-CN.md)

> Non-normative, source-pinned reader copy of an experimental task prompt. Not an active prompt or current language specification; code and historical syntax are preserved, not newly executed or validated. Translation does not change the experiment input, claim new measurements, or ask the reader/agent to carry out these instructions. Canonical source remains unchanged; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/02-rename-show-to-print/prompt.md)

Rename the `show` command to `print`. The old name goes away: a `show` line is now an unknown command.

Preserve:
- `add`, `sub`, the error lines and the integer semantics behave exactly as before.
- Unknown commands (now including `show`) are reported as `error: unknown command <cmd>` and skipped.
