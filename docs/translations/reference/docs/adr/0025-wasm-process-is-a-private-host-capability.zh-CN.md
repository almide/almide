<a id="adr-0025-wasm-process-is-a-private-host-capability--almideprocessspawn-shipped-only-when-the-op-set-names-it-refused-at-load-by-a-stock-host-bounded-by-permissions-proc"></a>

# ADR-0025：Wasm 的 `process` 是私有宿主能力 — `almide:process/spawn` 仅在 op 集合包含它时随产物提供，标准宿主在加载时拒绝，并受 `[permissions] proc` 限制

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0025-wasm-process-is-a-private-host-capability.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0025-wasm-process-is-a-private-host-capability.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0025-wasm-process-is-a-private-host-capability.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。以正本为准，本资料不自动同步。以下 Accepted 状态、测量与实现主张均为原文记录，并非独立的当前验证结果。未执行所述命令。

- **状态**：Accepted。所有者于 2026-10-01（#2589）裁定采用 **A**：在 wasm 上，通过私有 `almide:process/spawn` WIT 导入提供 `process`。
- **日期**：2026-10-03
- **范围**：`--target wasm` 上 `process` 的子进程系列（`exec`、`exec_in`、`exec_with_stdin`、`exec_status`、`exec_status_timeout`、`run`（#3379 之前为 `exec_attached`）、`run_in`、`spawn`、`kill`、`is_alive`、`pid`）；嵌入式宿主（`almide run --target wasm`、`almide test` 的 wasm 路径）；p1 构建产物（`almide build --target wasm`）；`almide.toml` 中的 `[permissions] proc`；`proofs/target-availability.toml`。
- **例外所针对的规定**：[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) §2.6（“面向标准运行时的实现不提供私有宿主 ABI”）及 §3.1 第 1 点（“不由宿主提供任何新 op”）。本记录是设立该例外的唯一位置，§3 限定其边界。
- **相关**：#2589（本决定）、#2587（句柄形式的 process API）、#1423（目标可用性）、#1628（组件模型）、#2588（`wasi:http/service`，同样采用“宿主填充 world”的结构）。

<a id="1-context"></a>

## 1. 背景

<a id="11-two-models-of-process-lived-side-by-side"></a>

### 1.1 `process` 的两种模型并存

- `proofs/target-availability.toml` 将子进程系列归类为 **native-only-forever**：“标准 WASI（0.2 和 0.3）没有子进程 API”（#1423 的 bucket C，2026-08-31）。所有此类 fn 在 structural、stock-p1 和 embedded 路径上都遇到壁；`spec/stdlib/process_exec_status_test.almd`、`process_ext_test.almd` 和 `process_timeout_test.almd` 在 `proofs/wasm-fallback-baseline.txt` 中被列为 HOST_CAPABILITY。
- `docs/wasm/capability-system.md`（2026-07-31）将 `Proc` 视为宿主授予的能力，并指定 `almide_host` 模块中的 `almide_host_proc` 导入。该导入从未存在：`git grep almide_host_proc` 只在这份文档中找到它。

<a id="12-what-the-platform-offers-2026"></a>

### 1.2 平台提供什么（2026）

- WASI 0.3.0（2026-06-11）标准化了 cli、clocks、filesystem、http、random 和 sockets。没有子进程接口，WASI 提案列表中也没有候选。启动子进程会离开沙箱，所以不太可能出现标准接口。
- WASIX（Wasmer）有非标准的 `proc_spawn`；wasmtime 不运行它。
- wasm 运行时会在第一条指令执行前解析所有导入。导入了宿主未定义内容的模块会在实例化时被拒绝。因此，“此环境不能启动子进程”可以成为加载时判定，而非运行时意外。

<a id="13-the-goal-behind-the-ruling"></a>

### 1.3 裁定背后的目标

每个能够为 wasm 构建的程序都应当能这样做。`almide test` 的 wasm 路径未能通过的 11 个文件中，有 3 个就是这些子进程文件。

<a id="2-decision"></a>

## 2. 决定

<a id="21-one-interface-written-in-wit"></a>

### 2.1 用 WIT 编写的单一接口

`crates/almide-wasm-run/wit/process/spawn.wit`：

```wit
package almide:process;

interface spawn {
  enum op { exec, exec-in, exec-with-stdin, exec-status, exec-status-timeout,
            run, spawn, kill, is-alive, pid, run-in }
  call: func(op: op, a: string, b: string) -> result<string, string>;
}
```

每个操作接收两个文本操作数，并返回 ok(text) 或 err(text)。参数列表以十进制 CHAR 长度单元（`<chars>\n<payload>`，http_framed 单元格式）传递；需要第三个操作数的操作将其放入第一个单元。WIT 注释定义各操作数及响应；`crates/almide-wasm-run/src/tests/host_process_test.rs` 中的 WIT 测试将分支顺序及 `call` 形式与宿主 op 编号固定对应。（#3379 将分支 5 从 `exec-attached` 改名为 `run`，并追加 `run-in` 作为 op 90，因此之前的分支保留原编号。）

分支索引等于宿主 op 减去 80：在生成代码与宿主边界上使用 op 80..=90（`crates/almide-wasm/src/fs_meta.rs`、`PROC_LEAVES`）。客体部分是 Almide（`stdlib/process_wasm.almd`，通过 self-host registry 链接到 wasm 路径）；它构建单元，并将 `exec_status` 的响应 `<code>\n<stdout chars>\n<stdout><stderr>` 解码为 `ProcessStatus`。

<a id="22-who-serves-it"></a>

### 2.2 谁提供它

- **嵌入式宿主**像其他嵌入式服务一样，通过 `almide.fs_call` 向原始模块提供 op 80..=90，并在自身链接器上定义导入 `almide:process/spawn.call`（`almide_wasm_run::link_spawn_import`）。两者均运行 `crates/almide-rt-core/src/process_core.rs`，而 `runtime/rs/src/process.rs` 将其原样 `include!`。因此 native 与嵌入式宿主使用同一份代码生成捕获的文本、退出码和每个 err 字符串。唯一仅限 native 的部分是识别在终端上停止的子进程的 `waitpid` probe（#2540）。它需要工作区禁止的 `unsafe`，所以 core 将轮询作为参数接收。在嵌入式宿主上，这类子进程会改为触及截止时间。
- **p1 产物**（`almide build --target wasm`）。op 集合包含 process op 时，`to_wasi` 添加导入 `almide:process/spawn.call`。这是 `call` 的 canonical ABI lowering：`(op, a_ptr, a_len, b_ptr, b_len, retptr)`，响应字节通过导出的 `cabi_realloc` 放置。它还在 `fs_call` 后添加一个转发器，将 staging pair 指向响应（`crates/almide-wasi/src/proc_service.rs`）。不包含 process op 的程序不会获得上述任何内容。这种选择与 environ pair 根据 op 26 获得的选择，以及 `wants_http` 为 p3 world 做出的选择相同。
- **组件构建**（`--component`，p2 或 p3）在构建时以指明 `almide:process/spawn` 的 E081 拒绝。目前没有 component world 声明该接口（§5）。

<a id="23-what-a-stock-host-does"></a>

### 2.3 标准宿主的行为

它在加载时、运行 `_start` 之前拒绝模块。2026-10-03，使用 wasmtime 47.0.2，对一个三行 `exec_status` 程序的产物进行了测量：

```
$ almide build p.almd --target wasm -o p.wasm
note: p.wasm imports almide:process/spawn (process.*, ADR-0025): a stock WASI runtime refuses it at load; it runs on a host that implements that import — `almide run p.almd --target wasm` is one
Built p.wasm (6644 bytes, structural leg, …)
$ wasmtime run p.wasm
Error: failed to run main module `p.wasm`
Caused by:
    0: failed to instantiate "p.wasm"
    1: unknown import: `almide:process/spawn::call` has not been defined
$ almide run p.almd --target wasm
start
code=0 out=hi
```

构建过程明确提示，运行时在加载时拒绝，程序不会在运行时给出错误答案。

<a id="24-permissions-proc"></a>

### 2.4 `[permissions] proc`

`almide.toml` 可以列出子进程系列允许启动的命令：

```toml
[permissions]
proc = ["git", "cargo"]
```

- **静态检查**（`cli::check_proc_allowlist`：在两个目标上，`run`、`build` 和 `test` 对已链接 IR 运行此检查；`almide check` 对链接前 IR 运行，能看到直接调用）：每个启动子进程的调用都必须用列表内的字符串字面量指定命令。列表外的字面量会以 `process.exec("cargo") (line N): `cargo` is not in [permissions] proc` 拒绝。在运行时计算的命令，或作为值传递的启动 fn，无法检查，也会被拒绝。因此，能编译的程序在任何目标上都不会启动列表外命令。
- **嵌入式宿主运行时**（`almide_wasm_run::set_proc_allowlist`）：同一列表限制每个启动子进程的操作。列表外命令会在执行任何内容之前收到一个标明命令的 err。这也约束了静态关卡未检查过的产物。
- 没有该键时，与此前一样不作限制。`proc = []` 不允许任何命令。

`Proc` 尚不是检查器中的独立 effect。`pass_effect_inference` 将 `process` 映射为 `Env`，因此目前允许 process 调用的是 `allow = ["Env"]`。`proc` 列表在这一授权内进一步缩小范围。将 `Proc` 从 `Env` 分离，会改变哪些现有清单能够编译，不属于本决定。

<a id="25-the-availability-ledger"></a>

### 2.5 可用性记录表

`proofs/target-availability.toml` 增加 **host-capability** 类别：fn 仅在宿主授予了标准 WASI 未定义的能力时工作。它在 native 和嵌入式宿主上运行；面向标准宿主的产物可以构建，但携带私有 `almide:*` 导入，标准运行时会在加载时拒绝。可用性 probe 从产物读取导入，因此 stock-p1 的壁是测量所得，而非声明所得。十个子进程条目移入此类别，且仅指定 stock-p1 路径。`process.env` 和 `process.sleep` 的条目移除：它们 lowering 到每个宿主都提供的 host op 26 和 36。

<a id="3-the-exception-and-its-bounds"></a>

## 3. 例外及其边界

ADR-0023 §2.6 拒绝私有宿主 ABI，是因为标准运行时会运行一个由工具链为其并不具备的宿主构建的产物。存在可 lowering 到的标准时，该理由成立：HTTP 有 `wasi:http`。子进程既无标准也无提案，因此没有 lowering 目标。在 wasm 上拒绝整个系列，会使原本宿主能运行的程序无法构建。本 ADR 在四个条件下允许私有接口；移除任一条件，例外即失效。

1. **由 op 集合选择。** 仅当生成的 op 集合包含 process op 时，产物才携带该导入。不使用 process 的程序，其产物与本决定之前逐字节相同；大小与分配记录表提供证据。
2. **加载时明确拒绝。** 缺少该导入的宿主在运行前拒绝模块（§2.3）。它不会静默失败，也不会以错误答案失败。
3. **受清单限制。** `[permissions] proc` 在静态检查和嵌入式宿主上都成立（§2.4）。
4. **一个封闭接口。** 例外仅为 `almide:process/spawn`，不包含其他内容。另一个私有 `almide:*` 接口需要单独的记录。在该接口中添加操作时，应作为新的 `enum op` 分支，与其 WIT 测试、宿主分支和 cross fixture 在同一次变更中落地。

<a id="4-evidence"></a>

## 4. 证据

- `spec/stdlib/process_exec_status_test.almd`、`process_ext_test.almd` 和 `process_timeout_test.almd` 在嵌入式宿主上通过 `almide test` 的 wasm 路径，并从 `proofs/wasm-fallback-baseline.txt` 移除。
- `spec/embedded_cross/process_spawn_family.almd` 覆盖所有操作：成功、非零状态、启动失败、截止时间触发、大量输出的子进程、`spawn` / `is_alive` / `kill`，以及通过 CHAR 单元传递的非 ASCII 输出。`almide run` 和 `almide run --target wasm` 将其逐字节一致地输出（`tests/embedded_cross_test.rs`）。
- `crates/almide-wasm-run/src/tests/host_process_test.rs`：canonical import 通过 `cabi_realloc` 放置响应；包含 op 83 的 `to_wasi` 产物携带导入，并在定义了该导入及两个 preview-1 调用的宿主上运行得到正确输出；不定义它的宿主拒绝模块；没有 process op 的 op 集合不携带该导入；WIT 分支顺序与 host op 一致。
- `tests/proc_allowlist_test.rs`：对 `check`、native `run`、wasm `run` 和 wasm `build` 的静态关卡进行测试，覆盖列表外字面量、计算生成的命令和作为值传递的启动 fn。

<a id="5-alternatives"></a>

## 5. 替代方案

- **保留 native-only-forever。** 即使宿主可以提供支持，三个 spec 文件和每个调用外部命令的程序仍无法为 wasm 构建。裁定否决了这一方案。
- **WASIX `proc_spawn`。** 这是另一供应商具有自身语义的私有 ABI，wasmtime 不运行它。采用它会增加第二个私有 ABI，而不移除第一个。
- **在构建时（E081）而非加载时拒绝。** 这是旧行为。拒绝很明确，但使可提供支持的宿主也无法获得产物；裁定要求标准宿主在加载时拒绝。组件构建尚无产物形式，因此保留 E081。
- **现在引入 component world 导入。** p2/p3 shim 通过遵循预留规则的 `cabi_realloc` 放置每个宿主生成的值（#2119）。这能够成立，是因为 shim 在调用前选择上界。子进程响应没有客体可以选定的上界。要将 `almide:process/spawn` 带入组件，首先需要流式响应（`stream<u8>`，#2587 的句柄形式）。在此之前，组件路径以 E081 拒绝。
- **现在引入资源形式的接口**（`resource child { wait, stdout, stop }`，#2589 草图）。当前接口是同步的，#2587 决定句柄 API 及其 `Exit` 类型。带有 `enum op` 的 `call` 覆盖当前接口。句柄可以作为同一包中的新分支或新 `resource` 加入，而不破坏 `call`。

<a id="6-consequences"></a>

## 6. 后果

收益：

- 每个子进程 fn 都能在嵌入式 wasm 宿主上构建并运行，具有与 native 相同的可观测结果；wasm 回退列表减少三个文件。
- 不使用 process 的产物不变。
- 能力在产物中可见：导入列表说明它是否能启动子进程，不授予该能力的宿主在加载时拒绝。

成本：

- 嵌入式宿主提供十个新 op，而这原本被 ADR-0023 §3.1 第 1 点禁止（§3 限定其范围）。
- process 程序的 p1 产物只能在实现了 `almide:process/spawn` 的宿主上运行。标准运行时按设计拒绝它。
- p1 产物中的 `cabi_realloc` 推进堆边界，从不释放。每个响应在实例的整个生命周期中都会占用其长度对应的空间。
- 在终端上停止的子进程在 native 上会被识别，在嵌入式宿主上则触及截止时间（§2.2）。

<a id="7-falsifiers"></a>

## 7. 反证条件

观察到以下任一情况时，撤回或修订本决定：

1. WASI 标准（或已批准的提案）定义子进程接口。届时该系列 lowering 到它，并由采用该接口的 ADR 废止本私有接口。
2. 标准运行时在不定义 `almide:process/spawn` 的情况下实例化携带它的产物，或不拒绝而是运行并产生错误答案。此时加载时保证不成立。
3. op 集合不含 process op 的程序携带了该导入，或其产物字节因这一机制发生变化。此时选择机制已损坏（大小棘轮会测量这一点）。
4. `spec/embedded_cross/process_spawn_family.almd` 在 native 与嵌入式宿主间出现分歧。此时共享 core 的主张不成立。
5. 在 `[permissions] proc` 下构建的程序在任一目标上启动了列表外命令。

<a id="8-references"></a>

## 8. 参考资料

- WASI 0.3：https://wasi.dev/releases/wasi-p3、https://bytecodealliance.org/articles/WASI-0.3
- WASI 提案：https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md
- wasmtime 上的 Component Model：https://component-model.bytecodealliance.org/running-components/wasmtime.html
- Canonical ABI（展平、`cabi_realloc`、`MAX_FLAT_RESULTS`）：https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md
- Deno `--allow-run`：https://docs.deno.com/runtime/fundamentals/security/

---

原文列出 11 个 op 编号（80..=90），同时保留旧的“十个”行／op 数量；未擅自统一这些历史表述。“无提案／不太可能”是当时的平台判断，而非当前标准调查。缺少 proc 键时不受该限制；私有宿主导入不等于通用沙箱保证。
