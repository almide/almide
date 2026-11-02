<a id="adr-0027-effect-functions-are-inspected-in-a-fixed-host-world--a-seeded-sandbox-property-verdicts-for-what-the-host-decides-and-loopback-fixtures-for-the-network"></a>

# ADR-0027：在固定的宿主世界中检验 effect 函数 — 预置沙箱、对宿主决定的值进行性质判定，以及网络回环 fixture

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0027-effect-functions-are-inspected-in-a-fixed-host-world.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0027-effect-functions-are-inspected-in-a-fixed-host-world.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0027-effect-functions-are-inspected-in-a-fixed-host-world.zh-CN.md)

> [英文（含日文引用）正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原 ADR 仍是权威且不可变的正本。本快照不自动同步，也不是检查器或生成器输入。下文决定、日期、测量、实现状态和主张均来自固定的历史原文，不代表独立验证或当前版本保证。代码块及其注释保持原样。

> 原文注意事项：行数、检验状态和工作顺序是注明日期的历史记述，不是检验行动已完成的证据。性质 parity 判定不是宿主所选值相等，也不是普遍统计保证。回环／无需 key 的隔离仅适用于这些 fixture，不扩展到任意生产网络操作。

- **状态**：已接受（D1–D8，2026-10-17）。
- **日期**：2026-10-17
- **范围**：`xtarget-fuzz inspect` 如何把五项检验标准（parity、complexity、alloc、fuzz、diamond）应用于结果依赖宿主的 stdlib 函数：fs、env、args、io、process、random、datetime、http、net、`bytes` 和 `mem` 的宿主地址函数，以及 `list.shuffle`。
- **基础**：[effect-inspection-harness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-inspection-harness.md)（2026-10-12 的设计，E1–E4 层）。本 ADR 将其转为检验框架的决定，并补齐该设计未解决的问题：执行端、如何比较非确定值、宿主地址函数，以及每项标准对 effect 行的含义。
- **相关**：#3552（检验行动）、#1710（wasi:http@0.3）、#1827（wasm 未支持的函数）、[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)、[ADR-0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md)。

<a id="context"></a>

## 背景

2026-10-16 这轮工作之后，`proofs/stdlib-inspection.toml` 有 **150** 个未检验行。其中 140 个依赖宿主：

| 模块 | 行数 | 决定结果的因素 |
|---|---|---|
| fs | 34 | 文件树 |
| http | 28 | 服务器 |
| net | 26 | 对端、套接字、共享内存 |
| process | 17 | 子进程、进程自身的 pid、其退出 |
| env | 9 | 环境变量、argv、cwd、时钟、OS 名称 |
| args | 6 | argv |
| io | 6 | stdin、stdout |
| bytes | 6 | 堆地址 |
| random | 4 | 熵源 |
| datetime | 2 | 时钟 |
| mem | 2 | 堆标记 |

`list.shuffle`（一个纯 `fn`，文档说明“无 seed，每次运行不同”）是第 141 个。

目前检验框架（`tools/xtarget-fuzz/src/inspect.rs`）构建每个 probe 程序，并作为裸进程运行：native 用 `./bin`，wasm 用 `wasmtime run`。它没有独立的 cwd，也没有 argv、env 或 stdin，并逐字节比较 stdout。因此 effect probe 要么根本无法在 wasm 上运行（无 preopen），要么打印每次运行都不同的值（pid、时间戳）。

程序 fuzzer 已有一半机制：`oracle/sandbox.rs` 创建预置文件树和固定 argv、env、stdin 的全新环境，`oracle/runner.rs` 在其中运行每个执行端，并比较运行后的文件树 snapshot（E1 层）。检验框架尚未使用它。

<a id="decision"></a>

## 决定

<a id="d1-the-legs"></a>

### D1. 执行端

effect 行把 native 与**内嵌 wasm 宿主**（`almide run --target wasm`、`almide-wasm-run`）比较。对于 `proofs/target-availability.toml` 表明标准运行时支持的函数，还与**标准运行时**（`wasmtime run`）比较。

行的 `parity` 字段写明覆盖的执行端。内嵌宿主不支持的函数先移植到 wasm，再在两端检验（D8）。

<a id="d2-a-fixed-world-per-run-tier-e1-fs-env-args-io-processargs--env--stdin_lines"></a>

### D2. 每次运行都有固定世界（E1 层：fs、env、args、io、process.args / env / stdin_lines）

每个 probe 的每次运行，无论哪个执行端、哪个规模，都从 `oracle/sandbox.rs` 获得独立的新沙箱：
- 预置文件树，用作 cwd；
- 固定的 argv、env 和 stdin。

检验框架复用 fuzzer 的 runner，而非另做一份。

parity 比较 stdout、stderr、退出码，**以及运行后的文件树 snapshot**。即便没有打印任何内容，写入结果不同也是差异。

配方指定文件树中的路径，使被测调用始终走成功路径。这是 #3671 的规则：在 err 或 none 路径上取得的检验标记无效。

<a id="d3-what-the-host-decides-is-compared-as-a-verdict-not-as-a-value-tier-e2"></a>

### D3. 宿主决定的东西按判定比较，而非按值比较（E2 层）

宿主选择的值（随机数、时间戳、pid、地址）无法跨执行端或运行匹配。其 probe 不打印该值，而打印**判定行**：由 probe 自身使用已检验的纯函数计算该函数所声明的性质。

两端必须打印相同的判定，且每个判定都必须为 `true`。比较仍是字节相等，只是对象变成了判定。

| 函数 | 性质 |
|---|---|
| `random.int(lo, hi)` | m 次抽取中 lo ≤ r ≤ hi，且 hi − lo 较小时两个端点都被命中 |
| `random.float()` | m 次抽取中 0 ≤ r < 1 |
| `random.choice(xs)` | 结果是 xs 的一个元素；只有 `[]` 才产生 `none` |
| `random.shuffle(xs)`, `list.shuffle(xs)` | 是 xs 的一个排列（排序后的副本相等），并进行频率检查：m 次洗牌中，每个元素在每个位置出现的次数都在 m/n 的 6σ 范围内。配方固定 m 时，正确洗牌大约每 10⁹ 次运行有一次不通过该检查 |
| `datetime.now()`, `env.unix_timestamp()`, `env.millis()` | 位于框架传入的 `[t₀, t₁]` 时间窗内，该窗口在运行前后即时读取 |
| `datetime.monotonic_ns()` | 两次读取非递减 |
| `env.sleep_ms(k)`, `process.sleep(k)` | 调用前后的单调时间差 ≥ k ms |
| `process.pid()` | > 0，且 `process.is_alive(process.pid())` |
| `process.is_alive(p)` | 子进程运行时为 true，回收后为 false |
| `env.os()` | 该执行端的 contract 为其运行宿主指定的值 |
| `bytes.as_ptr` / `as_mut_ptr` / `data_ptr` / `copy_to_ptr` / `from_raw_ptr` / `heap_restore` | 往返：经指针复制出去再读回的字节相等，且值存活期间指针稳定 |
| `mem.save` / `mem.restore` | 其间进行的分配被释放（存活数量回到标记处） |
| `net.tcp_is_open(c)` | 连接 fixture（D5）时为 true，`tcp_close` 后为 false |

此类行的 `parity` 字段以 `PASS (properties: …)` 开头，避免读者误认为值相等。

<a id="d4-processes-run-a-fixed-command-set-tier-e3"></a>

### D4. 进程运行固定命令集（E3 层）

probe 只启动：
- `/bin/echo`；
- 从 stdin 接收输入的 `/bin/cat`；
- `/usr/bin/true`；
- 用于失败状态的 `/bin/sh -c 'exit 3'`；
- 用于 timeout 形式的 `/bin/sleep`。

框架回收每个子进程。`process.exit(c)` 仅按退出码判断。

<a id="d5-the-network-is-a-loopback-fixture-tier-e4"></a>

### D5. 网络使用回环 fixture（E4 层）

每次运行，框架在 127.0.0.1 和沙箱内的 Unix 套接字启动 fixture，并在运行后停止。fixture 提供：
- HTTP 服务器，回显 method、path、headers 和 body，并提供预设状态码、chunked 响应和 SSE 流；
- TCP 回显服务器；
- Unix 回显服务器。

程序从固定环境变量读取地址。`anthropic_` / `openai_` 流式调用通过其 base URL 到达 SSE endpoint。不联系外部宿主，也不使用 API key。

<a id="d6-how-each-criterion-reads-for-an-effect-row"></a>

### D6. effect 行中各项标准的含义

- **Complexity。** 规模参数是流经调用的数据：文件长度、行数、payload 字节数、列表长度。工作为固定宿主操作的函数（`exists`、`mkdir_p`、`tcp_close`）声明为 order 1。计数型度量（`instructions:u` 加移动的字节数）测量程序自身的工作；内核时间不在范围内，检验标记明确说明这一点。
- **Alloc。** 不变：调用不留下存活对象。例外是它返回的资源句柄，probe 在计数前释放该句柄。
- **Fuzz。** 生成器从世界中选取这些函数的参数：文件树路径、fixture 地址、`lo..hi` 范围。生成的程序到达 E2 值时打印该值的判定，而非值本身。
- **Diamond。** 每条路径在各自全新的沙箱（或 fixture）中运行，三条路径的环境相同。

<a id="d7-the-table-is-one-place"></a>

### D7. 表只有一个存放位置

每个函数的层级以及 D3 的性质集中在 `tools/xtarget-fuzz` 的一个表中。测试拒绝台账中未被该表分类的 effect 行。同一规则也用于纯函数：不得在声明之外按名称作特例处理。

<a id="d8-nothing-is-stamped-native-only"></a>

### D8. 不给任何函数盖 native-only 标记

目前没有 wasm 执行端支持的函数，先移植，再在两端检验。这包括 http streaming family、`http.*_response`、`net.unix_*`、`net.shm_*`、量化矩阵内核，以及 test 块内的 `testing.assert_throws`。2026-10-12 裁定的 native-only 标记（plan B）不用于实现“未检验 = 0”（owner，2026-10-17：“未支持的当然要先让它在 wasm 上也能运行，然后在两端检验”）。

<a id="alternatives-considered"></a>

## 考虑过的替代方案

- **为检验而 mock stdlib 中的 effect。** 拒绝（与 2026-10-12 的设计相同）：检验会判断 mock，而不是真实宿主绑定；跨目标 bug 正存在于这些绑定中。
- **添加 seed 开关（`ALMIDE_RANDOM_SEED`），使随机运行逐字节重复。** 拒绝。这会增加一种普通程序可能依赖的隐藏运行时模式。相同 seed 的两端还必须共享一个 PRNG，为框架的需要约束运行时。D3 判定在不改变运行时的情况下检查 API 承诺的性质（界限、排列、均匀性）。
- **记录并回放宿主调用。** 拒绝：它检验的是记录。仅当回环 fixture 被证明不稳定时，才作为 E4 的备选保留。
- **将宿主地址函数标为“无法检验”。** 无需如此：D3 为这 14 个函数各给出可观察性质。若发现某个没有，则作为独立问题重新提出。

<a id="compatibility"></a>

## 兼容性

- 仅工具：`tools/xtarget-fuzz` 和检验脚本。不改变语言、stdlib 或 codegen。
- 框架发现的缺陷，与每轮检验一样，先写测试再修复。

<a id="order-of-work"></a>

## 工作顺序

1. 在检验框架中实现 E1（沙箱运行、parity 中包含文件树 snapshot）；给 fs、env、args、io 加检验标记。
2. E2 判定 probe；给 random、时钟、shuffle、地址及堆标记函数加检验标记。
3. E3；给 process 加检验标记。
4. E4 fixture；给内嵌宿主支持的 http 和 net 函数加检验标记。

<a id="open"></a>

## 开放事项

1. ~~整体采纳 D1–D7。~~ 已于 2026-10-17 接受（○）。
2. ~~内嵌宿主不支持的 24 行。~~ 于 2026-10-17 决定：先移植，再检验两端（D8）。
3. ~~test 块内的 `testing.assert_throws`。~~ 同一裁决（D8）决定：先让它在 wasm 上运行，再检验两端。
