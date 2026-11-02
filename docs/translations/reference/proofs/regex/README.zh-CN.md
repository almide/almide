<a id="the-regex-reference-table"></a>

# regex 参考表

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/regex/README.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。以正本为准；此副本不自动同步，也不是生成器或证明输入。未独立执行命令、证明、测量或资格鉴定评估。来源引用、生成标记与带日期数字作为历史资料保留，并非认证、资格鉴定、主管机构接受或发布的证据。

`reference-answers.tsv` 记录本项目以外的引擎对正则表达式语言公共子集的回答，每个案例对应一行 `op<TAB>pattern<TAB>subject<TAB>answer`。`scripts/check-regex-reference.sh` 向 Almide 两条路径提出相同问题，只要有任何差异就失败。

<a id="why-a-table-and-not-a-fuzz-against-a-linked-engine"></a>

## 为什么使用表，而非链接参考引擎做 fuzz

C-032 已经对引擎做 fuzz，但 oracle 是 `runtime/rs/src/regex.rs`，即*我们自己的* native 引擎。这能确认两条路径一致，却看不到两边对某条规则作出相同而且同样错误的理解。#2129 正是这个缺口，它发现的两个缺陷对所有既有关卡都不可见：

- `[]]` — 字符类首位的 `]` 是字面成员（POSIX 以及 PCRE / Python / Rust / Go / JS 都遵守）。若将其读作终止符，模式会变成空类加一个多余的 `]`，静默地永不匹配。没有错误，没有匹配，从外部无从察觉。
- 对能匹配空串的模式执行 `split` — `split("^", "abc")` 返回 `["a", "bc"]`，把一个字符移出了它所属的字段。

使用已提交的表而不链接参考引擎，是因为关卡必须封闭：不访问网络、不增加运行时依赖，并在每台机器上永远得到相同答案。代价是子集扩大时需要手动刷新表，这与 `crates/almide-spine/tests/golden/` 中 oracle manifest 已采用的取舍一致。

<a id="what-is-in-the-subset-and-what-is-deliberately-not"></a>

## 子集包含什么，以及刻意不包含什么

包含：字面量、`.`、字符类（含 `]` 位于首位和类内 `[` 的形式）、锚点、包括花括号在内的贪婪及惰性量词、分组及非捕获组、选择、`\d \w \s \D \W \S`、`\b`，以及六个公开操作。

排除以下内容，因为参考引擎彼此就有分歧，差异无法说明正确性：

- 空的选择分支（`ab|`、`|ab`）。Python 3.7+ 会在空匹配的位置重试非空匹配；Rust 和 Go 不会。Almide 与 Rust / Go 一致，已在 #2129 中对照 `ripgrep`（Rust 的 `regex`）验证。
- `split` 返回捕获组。Python 包含它们，Rust、Go 和 Almide 不包含。子集需要带分组的模式时，表生成器会在询问前将分组改写为非捕获组。

<a id="regenerating"></a>

## 重新生成

生成器记录在 `docs/bugfix-research-2026-09-12.md` 的 #2129 节。扩展子集意味着增加模式或待匹配字符串，并针对参考引擎重新运行；绝不能编辑某行来迎合 Almide 恰好返回的答案，否则关卡就变成了镜子。
