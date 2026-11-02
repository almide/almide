<a id="hot-native-issues-2026-09-10"></a>

# Native 重点问题，2026-09-10

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 固定原文的非规范性历史阅读快照，以原本为准。不自动同步。所述测试、测量、实现状态和保证是原文主张，不是独立重跑或当前验证。未执行引用代码或命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/hot-issues-2026-09-10/README.md)
> 保留历史本地测量及验证数量，未重跑。math 替换是明确不得交付的归因实验；显示 checksum 一致不确立一般逐位等价。

#2077–#2080 的设计与测量笔记。机器：Apple M3、macOS。编译器基于 `14e667fdc`，加上本 PR 的变更。研究结果为本地实验，不是普遍加速主张。

<a id="decisions"></a>

## 决策

- **#2077：** `string.slice` 保留钳制后的 codepoint 语义，但停止扫描未请求的后缀；仍会遍历前缀。新的 `string.byte_slice` 返回拥有的 `String?`：负数、逆序、越界或非 UTF-8 边界偏移返回 `none`。有效边界检查为 O(1)，复制为 O(window)。native 使用 Rust `str::get`；self-host 使用检查过的字节边界及 UTF-8 continuation bit。由于结果为 Option，它通过 structural SUM-builder tier 准入，而非绕过 layout certificate。
- **#2079：** 仅在已有 BOM 去除后的源码逻辑开头接受 `#!`。将其保留为 trivia 以保全源码位置，格式化在 dialect stamp 和 import 之前。OS 执行仍需要可执行位、有效 interpreter path，以及支持所选 `env` 调用的 OS。
- **#2080：** 默认 native build 曾通过 MIR 的逐次 WASM lowering 接受 fan，尽管 `emit` 显示 scoped thread。在 lowering 消除边界前拒绝 fan block 和 fan.map / fan.any_map，让已有 native codegen 处理并发。这不声称所有含效果的 map callback 都可并行化，也不完成范围更广的 #2044 工作。

参考源码从 `../almide-references/rust` 的 `0d31508599a7814a7044e9a7a871e3dc5f037753` 读取：`compiler/rustc_lexer/src/lib.rs`（`strip_shebang`）及 `library/core/src/str`（检查边界的 slicing）。Zig 参考固定为 0.15.2，commit `e4cbd752c8c05f131051f8c873cff7823177d7d3`。

<a id="native-fan-reproduction"></a>

## Native fan 复现

用基线及修复后编译器构建 `fanblock.almd`；预热各二进制，再用 `/usr/bin/time -p` 测量。基线 real/user：0.52/0.51–0.52 s。修复后 warm real/user：0.09/0.64 s。两者都打印 `fanblock 4389696`。修复后首次 cold run 为 0.60 s，不混入 warm 比较。这是小型手动探针，不是 CI 中稳定的时间阈值。路由和行为有回归测试。

<a id="2078-remains-open"></a>

## #2078 仍未解决

`gramide.json` 记录三次交错 warm run、fixture/revision/compiler hash，以及匹配的完整 token 输出 hash。Go fixture 为 2,973,586 bytes，并非 issue 中的 3.95 MB fixture。`gramide-compact-spans.patch` 适用于 gramide `c6933d2ef93b9c5890540b1b57910da9d90d8b73`，是一项实验，而非已发布 client 变更（用 `git apply --unidiff-zero` 应用）。各 variant 用相同编译器构建；运行 `gramide check`，并比较所记录 Go 源码上的 `gramide tokens` 输出。

| 变体 | real 中位数 | 峰值 RSS 中位数 |
|---|---:|---:|
| 原 List[Int] 源码缓冲区 | 0.49 s | 122.75 MiB |
| Bytes 源码缓冲区 | 0.49 s | 116.44 MiB |
| Bytes 缓冲区与用于 span 的 bytes.slice | 0.48 s | 103.09 MiB |

Bytes 已存储紧凑 u8 元素，其 string 转换复制一次，并不会把整个 process 缩小到八分之一。这几个 sample 没有显示令人信服的运行时间优势，RSS 也随 sample 波动。拥有的 token string 和 record 分配仍需 profiling 及表示方式工作。文档现在正确描述复制，而不再声称零复制转换。

<a id="zig-arena-attribution-not-a-semantic-change"></a>

## Zig arena 归因，不是语义变更

`math-attribution.json` 包含每个较大尺寸的七次打乱顺序交错运行、seed 42，以及尺寸 10、23、24 上匹配的显示 checksum。输入为 `../listbuild/listbuild_prealloc.almd`。比较默认 build、`almide emit` 加 rustc，以及仅将 libm sin/cos wrapper 改成 `x.sin()` / `x.cos()` 的实验性生成源码 variant。Rust flags：`--edition 2021 -C opt-level=3
-C lto=yes -C codegen-units=1 -C overflow-checks=no`。

尺寸 24 的中位数依次为 0.40986、0.41457、0.21267 s。改变 native producer 影响很小；math 实现解释了此 workload 的大部分开销。这些是归因运行，不是新的交错 Zig 比较。显示 checksum 不确立所有输入的逐位等价。`runtime/rs/src/math.rs` 为跨平台确定性而刻意使用 vendored musl libm。不要交付 platform-math 替换。后续必须优化确定性 sin/cos（例如共享 range reduction），并提供数值及 cross-target 证据，才能声称 arena 优势。

<a id="validation"></a>

## 验证

- `tests/hot_native_regressions_test.rs`：仅开头 shebang、位置、格式化和 native concurrency 路由；`tests/fan_surface_matrix_test.rs`。
- `spec/cli/shebang.almd`：check、compile、test、format 和直接 OS 执行。
- `spec/wasm_cross/string_byte_slice.almd`：在 native 与 WASM 上验证 C-348 Unicode 字节边界、无效范围、空范围及既有 codepoint slicing。
- 整体 sweep 与九次启用网络的重试共通过 440 个 native 测试文件。默认 sweep 加两次启用网络的重试也通过全部 440 个文件。
- release allocation ledger 与 structural witness floor 通过；既有 allocation 行不变，新增一个 fixture 行。docs generation check 通过。
- Codopsy 的 codegen 和 WASM 均保持 A（90/100），沿用 20/30 阈值。

<a id="ci-integration-follow-up"></a>

## CI 集成后续

新 fixture 使用显式 Option match，使 incumbent MIR host harness 也能编译全部情况；预期结果和无效边界情况不变。native、wasm32-wasip1 与 browser ABI 为它生成相同的 17,620-byte incumbent module，没有新增拒绝 fixture。structural emitted / WASI 大小为 5,098 / 5,608 bytes。size baseline 只增加它的行；既有行与 refusal ceiling 不变。新 API 在 exercised surface、IDE snapshot 和 contract table 中登记为 pure。native concurrency 拒绝现在携带 source span，由回归测试和 spanless-wall ratchet 检查。
