<a id="gramide-allocation-experiment-for-almide-2078"></a>

# 面向 Almide #2078 的 gramide 分配实验

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 固定原文的非规范性历史阅读资料。下述测试、分支状态、测量及修复均为带日期报告，不是新执行的验证或普遍正确性保证。保留原代码及历史测量，未运行命令或基准测试。不自动同步，以原本为准。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/hot-issues-2026-09-10/gramide-cached-operators/README.md)

2026-09-10，Apple M3/macOS。对 gramide `c6933d2ef93b9c5890540b1b57910da9d90d8b73` 用 `git apply --unidiff-zero` 应用 `gramide-cached-operators.patch`。客户端补丁已发布为 [gramide PR #1](https://github.com/O6lvl4/gramide/pull/1)。
它不重新设计 Almide String 或 Token 表示，而是使用现有紧凑 Bytes API，在每次分词中只预计算一次运算符字节模式。只有成功匹配的运算符才克隆文本。源码辅助函数命名为 `source_byte_at`，避免与 balance.almd 的 List[Int] 辅助函数 `at` 在测试编译时冲突。

词法分析器的格式化工具拒绝该文件，因为安全检查报告丢失六条注释；不要强制格式化或删除安全检查。

<a id="measurements"></a>

## 测量

测试文件为 Go 1.26.1 的 `src/cmd/compile/internal/ssa/opGen.go`，2,973,586字节。两个 JSON 文件均记录 SHA256。`check-results.json` 记录预热后打乱顺序、交错进行的七次 `/usr/bin/time -l` 运行（seed 2078）。基线、紧凑 span 和缓存运算符三种版本的完整 token 输出哈希一致。分配探针显示的校验和均为1142484379396。

| 变体 | check 墙钟时间中位数 | 峰值 RSS 中位数 |
|---|---:|---:|
| 原版 | 0.51 s | 123.34 MiB |
| 紧凑源码与 span 缓冲区 | 0.51 s | 103.25 MiB |
| 再缓存运算符模式 | 0.25 s | 103.16 MiB |

计时二进制没有插桩。测量早于仅改名的辅助函数冲突修复；最终 token 输出一致性和项目测试另行检查。这是本地试点测量，不是语言整体加速的声明。

<a id="allocation-attribution"></a>

## 分配归因

把 `bench_lex.almd` 复制到 gramide/src。它读取同一文件，对 Go 做词法分析，并消费所有 token 位置及 text/kind 长度；不编译文法、不做语法分析。
用 Almide 输出，将 `fn main()` 改名为 `fn probe_main()`，追加 `counting-allocator.rs`，再用 rustc `--edition 2021 -C opt-level=3
-C lto=yes -C codegen-units=1 -C overflow-checks=no` 编译。分配器统计 alloc 和 realloc 请求及请求大小；大小是累计量，不是活跃内存峰值。原子插桩会改变计时，因此不要对这些二进制计时。计数器包含进程初始化、源码读取和校验和输出。

原版：22,961,527次请求 / 307,280,840请求字节。
紧凑 span：22,743,986 / 268,361,639。
缓存运算符：2,612,801 / 117,801,931（请求数约减少88.6%）。

显著下降表明，在此输入上反复构建运算符候选是主要成本。拥有所有权的 token kind/text 字符串仍会分配；实验不声称解决了 #2078 中所有表示问题。

最终辅助函数改名补丁：全部9个项目测试文件通过（2个 WASM，7个原生回退）；完整 token 输出 SHA256 仍匹配。见 `final-validation.json`。

<a id="reference-checkpoint"></a>

## 参照检查点

选择下一步表示变更前检查了本地参照编译器：Rust `0d31508599a7814a7044e9a7a871e3dc5f037753` 的 `compiler/rustc_lexer/src/lib.rs`（`Token`、`TokenKind`、`advance_token`），以及 Zig 0.15.2 `e4cbd752c8c05f131051f8c873cff7823177d7d3` 的 `lib/std/zig/tokenizer.zig`（`Token`、`Tag`、`Tokenizer.next`）。两者都通过 token 标签与直接字符／字节分支识别固定标点，而不在扫描循环中重建候选字符串。Gramide 将语言规范作为值接收，因此预计算字节模式可保留其可配置顺序，而无需引入固定语言扫描器。tag/span token 表示是另一项更广泛的 API 变更，本次测得的减少并不需要它。

Hew 检查点：将 hew-lang/hew 克隆到 ../almide-references/hew，提交为 `a78df3b9e3b9e0558e63caa76e14d2aba4f39e4f`。`hew-lexer/src/lib.rs` 使用 Logos 编译期 DFA 分词及 `Token<'src>`，字符串字面量载荷为借用的 `&'src str`。这是消除逐 token 拥有文本的第三个具体参照，但不能证明 Hew 开发使用 gramide。

Hew 语料兼容性：在该修订全部1,282个 Rust 源文件上比较原版和最终补丁版 gramide 的 `tokens` 输出、stderr 与退出状态。全部1,282个结果匹配；两版均拒绝其中1个文件。这是 Rust 源码兼容性检查，不代表支持 Hew 语言文法。见 `hew-token-equivalence.json`。
