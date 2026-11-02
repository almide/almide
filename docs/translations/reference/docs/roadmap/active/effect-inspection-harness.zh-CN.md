<a id="effect-inspection-harness"></a>

# effect 检验框架

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-inspection-harness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-inspection-harness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](effect-inspection-harness.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-inspection-harness.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 下文保留原文注明日期的设计与采纳状态。

> 原文注意事项：应依据采纳／更新前言理解保留的 10 月 12 日设计。133/36 数量、全量跳过及末尾开放问题均为历史而非现状。ADR-0027 已解决政策问题；准入受支持签名并不证明所有 effect 通过。性质比较不声称宿主所选值相等，回环／无需 key 的隔离仅限 fixture。

<a id="adopted-design-and-implemented-harness"></a>

## 已采纳的设计与已实现的框架

下文 10 月 12 日的问题数量和工作顺序是设计快照。[ADR-0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) 于 10 月 17 日采纳检验规则；末尾面向 owner 的问题并非尚未决定的政策请求。当前 [catalogue](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tools/xtarget-fuzz/src/generator/catalogue.rs) 接受来自沙箱化 fs/env/io/args/process/net/http 模块、无宿主依赖 zlib 函数和性质判定函数的受支持签名。它并非跳过所有 effect 函数。准入仍受签名与目标过滤约束；这不表示每个 effect 函数都已通过检验。逐函数状态应查[检验台账](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/stdlib-inspection.toml)和[目标可用性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml)，而非旧的 133/36 数量。

**状态：** 设计，2026-10-12；由 [ADR-0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md)（2026-10-17）决定。
**跟踪：** #3552（stdlib 检验行动）。

<a id="problem"></a>

## 问题

stdlib 检验台账（`proofs/stdlib-inspection.toml`）要求每个公开函数接受五项标准检验。其中一项是函数在 native 和 wasm 上行为相同。标准及 `class = "effect"` 行字段已存在，对 effect 行要求在内嵌宿主和标准 WASI 运行时上的 parity。

程序生成器（`tools/xtarget-fuzz`）完全跳过 effect 函数（`signature_from_decl`）。因此，**458 个未检验函数中的 133 个**无论质量如何，根本无法检验。

| 模块 | 未检验 effect fn |
|---|---|
| fs | 34 |
| http | 28 |
| net | 24 |
| process | 13 |
| env | 8 |
| zlib（文件操作版本） | 8 |
| args | 6 |
| io | 6 |
| random | 4 |
| datetime | 2 |

<a id="what-was-consulted"></a>

## 查阅的资料

- **`../almide-references`。** `RESEARCH-verification.md` 涵盖差分 fuzzing（V-3、V-4），但仅针对纯程序。我没有找到关于 effect 测试、沙箱或 WASI 一致性的内容。**那里缺少本设计所需资料；设计依赖下述模式。**
- **本仓库现有跨目标 effect 测试。** `tests/wasm_runtime_test_parts/common.rs` 和 `corpus.rs` 在新的 `tempfile::tempdir()` 中运行每个 effect fixture，使用 `wasmtime --dir=/` 或预打开目录及 `-S inherit-env=y`。native 以该目录为 cwd。解释器有自己的沙箱化 fs（`crates/almide-interp/src/dispatch_vfs.rs`）。
- **`proofs/target-availability.toml`。** 它列出每个 wasm 执行端支持哪些 effect 函数。133 个中有 36 个（http streaming 和 `*_response`、全部 `net`、`process.stdin_lines`）在内嵌宿主上不可用，标准 p1 上有 62 个不可用。

<a id="design-four-tiers-by-what-makes-a-run-reproducible"></a>

## 设计：按使运行可复现的因素分四层

<a id="e1-sandboxable-and-deterministic-about-62-functions"></a>

### E1：可沙箱化且确定（约 62 个函数）

涵盖 fs、env（`get`、`set`、`cwd`、`temp_dir`、`args`）、args、io（stdin）及 zlib 文件操作版本。

- **每次运行的固定世界。** 每个程序在各执行端使用全新临时目录运行，其中预置固定文件树（几个内容已知的文件、子目录、宿主允许时的 symlink）。env、argv 和 stdin 也固定。
  - native：cwd = 临时目录；
  - 内嵌宿主：以临时目录为 cwd 执行 `almide run --target wasm`；
  - 标准 WASI：`wasmtime --dir=.` 加显式 `--env` 和 args。
- **parity 比较什么：** stdout、stderr、退出码，加运行后的临时文件树 snapshot（路径、种类、大小、字节）。即使没有打印内容，写入结果不同也是差异。
- **生成器：** `effect fn main`。fs 函数的路径参数来自预置文件树（存在和不存在的路径），因此测试错误路径，而非只有成功路径。
- **Complexity：** 规模参数为文件长度（`read_text`、`read_bytes`、`fold_lines`）或行数；判定器不变。成本在宿主侧的函数（`mkdir_p`、`exists`）声明为 1。
- **Diamond：** 每次运行采用相同沙箱设置。

<a id="e2-non-deterministic-random-datetimenow--monotonic_ns-envmillis--unix_timestamp--sleep_ms"></a>

### E2：非确定（random、`datetime.now` / `monotonic_ns`、`env.millis` / `unix_timestamp` / `sleep_ms`）

值无法跨执行端匹配。parity 按**性质**判定：退出码、类型和声明的范围（`random.int(lo, hi)` 在范围内、shuffle 为排列、两次 `monotonic_ns` 读取非递减、`sleep_ms(k)` 至少耗时 k ms）。台账 parity 字段注明所检查性质，让读者知道这弱于字节相等。

<a id="e3-process-13"></a>

### E3：process（13）

只启动固定且无副作用的命令：`/bin/echo`、`/usr/bin/true`、有 stdin 输入的 `/bin/cat`。parity 比较捕获的输出和状态。`process.exit` 仅按退出码判定。`process.kill` 和 `spawn` 使用由框架回收的子进程。

<a id="e4-network-http-28-net-24"></a>

### E4：network（http 28、net 24）

每次运行启动回环 fixture 服务器：提供预设响应（包括 streaming 和 chunked 响应）的 HTTP 服务器，以及 TCP / Unix 回显服务器。没有外部宿主，也无 API key（`anthropic_` / `openai_` streaming 调用通过 base URL 指向 fixture）。

**36 个函数在 wasm 上受阻**（`target-availability.toml`，内嵌执行端）：wasi:http@0.3 移植（#1710）落地之前，它们无法满足标准 1。它们保持未检验。标准不缩窄为“仅 native”。

<a id="alternatives-considered"></a>

## 考虑过的替代方案

- **为测试而 mock stdlib 内的 effect。** 拒绝：检验会判断 mock，而非各执行端真实的宿主绑定，跨目标 bug 恰恰存在于后者。
- **记录并回放宿主调用。** 对可在沙箱内真实运行的 E1 和 E3 拒绝。仅当回环 fixture 被证明不稳定时，对 E4 才有用。
- **把 effect 留给手写 `spec/wasm_cross` fixture。** 拒绝：它们是逐函数示例。标准 4 要求每次运行都生成新组合；今天这在一批纯函数中发现了九个缺陷。

<a id="compatibility"></a>

## 兼容性

- 仅工具：`tools/xtarget-fuzz` 和检验脚本。不改变语言、stdlib 或 codegen。
- 台账中 E2 行的 `parity` 字段注明其性质检查。

<a id="order-of-work"></a>

## 工作顺序

1. **E1：** 三个 runner 的沙箱设置；比较中加入文件树 snapshot；生成器中的 effect `main`；catalogue 中的 fs、env、args 和 io。
2. **E2：** 性质判定器。
3. **E3：** 使用固定命令集的 process。
4. **E4：** 回环 fixture，从 wasm 支持的 http 子集开始。

<a id="open-question-for-the-owner"></a>

## 向 owner 提出的开放问题

没有内嵌 wasm 支持的 36 个函数：在 #1710 之前保持未检验（上述设计），还是单独给予明确 native-only 的检验标记？此处尚未决定。
