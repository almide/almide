<a id="almide-wasm-vm--requirements"></a>

# almide-wasm-vm — 要求

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REQUIREMENTS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REQUIREMENTS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](REQUIREMENTS.zh-CN.md)

> 固定原文的非规范性历史阅读快照，以原本为准。不自动同步。所述测试、测量、实现状态和保证是原文主张，不是独立重跑或当前验证。未执行引用代码或命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm-vm/REQUIREMENTS.md)
> 资格鉴定讨论表达原文理由与要求，不表示已经获得资格鉴定或认证。fuel 默认无限；有限 fuel 保证需要部署参数。所述 Critical capability 限制与五个 import 边界按原文要求保留，不扩展为对所有 host 或 artifact 的保证。
> 保留原文差异：此历史要求文本称 IO 授权可触及文件系统，而固定版 capability-system 文档将 IO 与 FS 区分。不要由这句旧陈述推断当前权限行为，应查阅权威能力规范。

可纳入资格鉴定范围的 Almide wasm 输出执行环境（#865）。通用运行时没有现实可行的资格鉴定路径：它是 JIT，功能范围远超 Almide 所生成的内容。此 crate 转而缩小范围，就像 Critical profile 缩小语言范围一样。它恰好解释 Almide 已交付 artifact 可能包含的指令和模块特性，在运行任何内容前拒绝其余一切。

每项要求都列出支撑它的证据。没有可执行证据的要求不作主张。

| Id | 要求 | 证据 |
|---|---|---|
| REQ-VM-1 | **输入是交付的 artifact。** VM 不经修改地运行 `almide build --target wasm` 写出的字节（structural emitter 的模块经过 `to_wasi` 变换后的结果）。 | `tests/wasm_vm_parity_test.rs` 用产品 CLI 构建每个 `spec/wasm_cross` fixture，并运行其写出的文件。 |
| REQ-VM-2 | **封闭集合。** 接受的指令恰好是 emitter 与 `to_wasi` shim 能写出的 122 条。接受的模块特性也构成封闭集合。类型最多接收 1,000 个参数，最多返回一个值。声明类型仅有 i32、i64 和 f64；f32 只暂时存在于操作数栈上。块类型仅限单字节形式。最多一个 funcref table，最多 2^20 个条目。恰好存在一个非共享 32-bit memory，按 WASI command 要求导出为 `memory`。global 使用常量初始化器。element 与 data segment 只能是 active，针对 table 和 memory 0，data count 与 data 一致。export 以互不相同的名称指向已有索引。custom section 被跳过。没有 start section，`_start` 不接收也不返回任何值。其他情况均在加载时拒绝；若一个字节即可决定，则指出该字节偏移。 | `tests/allowlist.rs` 双向比较 VM 集合与 emitter 源码。`tests/refuse.rs` 覆盖拒绝情形，包括文本汇编器从不写出的手工编码。 |
| REQ-VM-3 | **在有界加载中，于运行前验证。** 每个函数体以一次遍历通过限制在封闭集合内的标准验证算法。同一遍历将其转换为扁平形式，分支携带已解析目标、操作数高度及保留值。未验证函数体的任何指令都不执行。加载时间和内存与模块大小成比例。声明 locals 按连续组保存，因此很大的数量也只占一个条目。类型在线性时间内规范化。所有会分配的大小均有上限：参数、每个函数体的 locals、table 条目与 memory pages。 | `tests/refuse.rs` 覆盖类型错误函数体、伪造大小、垃圾或截断输入；必须拒绝且不得 panic。 |
| REQ-VM-4 | **宿主范围。** 仅链接五个 WASI preview-1 import，均使用精确签名。`fd_write` 服务 stdout 与 stderr。`fd_read` 服务 stdin，每次调用填充第一个非空 buffer，与标准运行时相同。`proc_exit` 以其 code 结束运行。`random_get` 与 `clock_time_get` trap 并报出自身名称。它们支撑 Rand 与 Time capability，Critical program 绝不会获得这些能力。错误文件描述符返回 EBADF。超出 memory 的指针，或指向 4-byte 值但未对齐的指针，会在写入任何内容前 trap，与标准运行时相同。 | `src/wasi.rs` 单元测试，包括读入自身 iovec 数组。`tests/run.rs` 覆盖 `stdin_is_served_and_the_unserved_calls_trap_by_name` 和 `a_bad_pointer_handed_to_the_host_traps`。 |
| REQ-VM-5 | **固定内存。** 所有 buffer 都在构建 instance 时分配：达到上限的 linear memory、value stack 与 frame stack。运行期间不分配任何内存。调用在进入前检查被调函数全部栈需求，即其 locals 加已验证的最大操作数高度。超过任一固定边界都会以 `call stack exhausted` trap。超过上限的 `memory.grow` 返回 -1。 | `tests/run.rs` 覆盖 `recursion_past_the_fixed_depth_traps`、`memory_grows_to_its_cap_and_no_further` 及在四个 frame 中执行一百万次尾调用的 `a_tail_call_reuses_its_frame`。 |
| REQ-VM-6 | **Fuel。** 扁平形式中的每条指令执行时消耗一个单位 fuel，耗尽即 trap。block、loop、end 标记不生成指令，不消耗 fuel。循环回边是一条 branch，消耗一个单位，因此 fuel 为每次运行设限。host call 或 bulk memory 指令不论长度也消耗一个单位，其工作量受 buffer 和 memory 大小限制。 | `tests/run.rs` 覆盖 `fuel_bounds_every_run`。 |
| REQ-VM-7 | **Trap 遵循规范。** 整数除零及溢出、内存越界、不合法间接调用均按 WebAssembly 规范 trap。不合法调用包括 null 条目、table 外索引和签名不匹配。trap 后不再执行指令，也不再进行 host call。 | `tests/run.rs` 覆盖数值、memory 和间接调用情形。`src/numeric.rs` 单元测试覆盖边界情况。 |
| REQ-VM-8 | **Runner 的契约。** 返回的运行以 0 退出。`proc_exit` 按给定 code 退出，与 native 和 embedded host 一样；操作系统保留其低 8 位。VM 不缩窄 status。C-350 要求编译器将 preview-1 artifact 的 `proc_exit` 保持在 0..=125，此 runner 的职责是运行交给它的内容。trap 以 1 退出，并写入一行 stderr：`Error: wasm trap: <reason>`，拼写与 embedded host 相同。若 program 最后一行 stderr 已以 `Error: ` 开头，且 trap 为 `unreachable`，则省略此行。这是 embedded host 遵循的 die 约定。拒绝的模块以 2 退出。 | `tests/run.rs` 覆盖 exit-code 与 trap-line 测试。`src/wasi.rs` 覆盖 `the_last_line_is_judged_as_str_lines_would`。 |
| REQ-VM-9 | **与标准运行时及 native 等价。** 每个交付 artifact 都满足三种结果之一。若 import 全部属于那五个，则加载并运行，stdout、stderr 和 exit code 与标准运行时相同。trap 比较其原因和 program 自身的 stderr，因为 VM 以 1 退出并附一行文本，标准运行时则以自己的 status 与 error block 退出。或者，在输出标准运行时输出的前缀后，因 clock 或 entropy 调用而停止于具名 trap。其他 artifact 均在加载时拒绝。任何交付 artifact 都不能以标准运行时拒绝的 126 或更大 status 调用 `proc_exit`。C-350 要求 preview-1 build 本身在 `proc_exit` 之前，以自己的 stderr 行和 exit 1 拒绝 126..=255（#2303、#2780）。因此这种 status 意味着编译器违反自身承诺，门禁会判为分歧。每个通过该 profile deny-all 默认设置 `almide check --profile critical` 的 fixture 都运行相等，并且也与 native 二进制运行相等。 | `tests/wasm_vm_parity_test.rs`，仅在 release 下运行，在 CI 的 commissioned wasm gates job 中执行。 |
| REQ-VM-10 | **可信基。** 除 Rust 标准库外没有其他依赖，workspace lint 禁止 `unsafe_code`。仅测试使用的 crate，即测试模块的文本汇编器，不属于可信基。 | `Cargo.toml` 有空的 `[dependencies]` 及 `[lints] workspace = true`。 |

<a id="scope-and-what-it-leaves-out"></a>

## 范围及排除项

- **一个 artifact 家族。** VM 运行 structural emitter 产生的 preview-1 artifact。component（`--component`）是不同格式，不在范围内。
- **Critical profile 的默认范围。** 即 console output、exit code 和 stdin。授予 `--allow IO` 也会触及文件系统，而文件系统、环境及参数访问都 import 那五个之外的调用，因此这些 artifact 在加载时拒绝。clock 和 entropy 以名称 trap。这些情况不会成为无声的错误答案。支持文件系统会成倍扩大可信宿主范围，与此 crate 的目的相反。
- **限制是参数，不是常量。** 默认值为 32-bit memory 上限、4 Mi-cell value stack、256 Ki frames 及无限 fuel。这些设置使 corpus 每个 program 都像在标准运行时上一样运行。部署通过 `--fuel`、`--memory-pages`、`--stack-cells` 和 `--call-depth` 声明自己的边界。
- **未来抽离。** 一旦朝实际资格鉴定推进，就迁往拥有冻结 release lifecycle 的独立仓库。allowlist test 是必须随之迁移的耦合点；它将 VM 集合固定到 emitter 集合。
