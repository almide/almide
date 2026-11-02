原文描述元数据：net 的 Unix 域套接字和共享内存文件使用的临时 embedded host ops 104..=116，以及在 ADR-0023 step 7 的退役。

<a id="net-unix-sockets-and-shm-on-the-embedded-wasm-lane-interim-ops-104116"></a>

# embedded wasm 路径上的 net Unix 套接字与 shm（临时 ops 104..=116）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](net-unix-shm-embedded.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](net-unix-shm-embedded.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](net-unix-shm-embedded.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/net-unix-shm-embedded.md)

> 原文注记：平台支持、实现、测试通过及一致性主张描述原文时点的临时设计，并非当前保证或所有执行的证明。原始描述符编号明确不属于跨进程相等范围；持有描述符检查与从不失败的 take 操作保持各自规则。step 7 退役是承诺，不是已发生的证据；不宣称提议的 component 接口已经存在。WASI 的宽泛措辞保留在此 Unix 套接字／共享内存文件家族的语境内，不扩展为对所有 WebAssembly 内存功能的主张。

Issue [#3620](https://github.com/almide/almide/issues/3620)，route 4（“unix / fd passing / shm”）。这是 net/tcp 设计的同类方案（TCP steps 1 和 2：共享 `net_core`、ops 91..=103），以相同形式构建。Owner 裁定 2026-10-17：○。

<a id="decision"></a>

## 决策

13 个函数 `net.unix_connect`、`unix_listen`、`unix_accept`、`unix_send`、`unix_recv`、`unix_take_fds`、`unix_poll`、`unix_wait`、`unix_close`、`shm_create`、`shm_resize`、`shm_write` 和 `shm_read` 获得**临时 embedded host ops**，与 TCP 完全相同：

| 执行侧 | 之前 | 之后 |
|---|---|---|
| native | `runtime/rs/src/net.rs`，原始描述符 | 共享核心，每个线程一张表 |
| embedded（`almide run --target wasm`） | E081 | 同一核心上的 ops 104..=116，每次运行一张表 |
| structural（`ALMIDE_WASM_SKIP_STOCK_AUDIT`） | 阻断 | lowering `stdlib/net_wasm.almd` 的函数体 |
| stock-p1（`almide build --target wasm`） | E081 | E081，不变 |

stock 制品仍为 E081，因为 **WASI 既没有 Unix 域套接字，也没有共享内存**：任何 preview（1、0.2、0.3）都未定义它们，因此 stock runtime 无法提供这些调用。`proofs/target-availability.toml` 为每行只保留 `stock-p1` 侧，并注明该原因。

<a id="one-implementation"></a>

## 一份实现

`crates/almide-rt-core/src/net_unix_core.rs` 包含整个函数族。Native 通过 `include!` 引入它（`runtime/rs/src/net.rs`，薄的 `almide_rt_net_unix_*` / `almide_rt_net_shm_*` 包装），`almide-wasm-run` 则链接它（`net_unix_host.rs`）。所以字节、收到的描述符及所有 err 文本由构造方式保证一致；`tests/net_unix_embedded_cross_test.rs` 对此进行观察。

**`unsafe`。** 四个 syscall 在 stable Rust 上没有安全的 std 形式：带 SCM_RIGHTS 的 `sendmsg` / `recvmsg`、`poll` 以及 `fcntl(F_SETFD, FD_CLOEXEC)`。workspace 禁止 `unsafe`，`almide-wasm-run` 继承该 lint，因此 embedded host 不能携带一份副本。两份副本也会成为两种行为。替代方案是：

- `almide-rt-core` 有自己的 lint：`unsafe_code = "deny"`（不是 `forbid`）。
- 恰有一个 module，`src/net_unix_sys.rs`，使用 `#[allow(unsafe_code)]`。每个 block 都带 SAFETY 行，收到的每个原始描述符都借自表在整个调用期间持有的 `OwnedFd`。
- `crates/almide-rt-core/tests/net_unix_core.rs` 在 crate 其他任何文件出现 `unsafe`，或该 module 中某个 block 缺少 SAFETY 行时失败。

核心其余部分（connect、listen、accept、文件读写、close）都是安全的 std 代码。添加 `rustix` 等 crate 本可去掉 `unsafe`，但也会给每个使用 `net` 的生成 native project 增加 crates.io 依赖，使其走 cargo 路径而非预构建 runtime。

<a id="descriptors"></a>

## 描述符

guest 收到**原始 OS 描述符编号**，与 native 返回的相同。native 进程与 embedded host 进程的编号不同，因此 fixture 只打印描述符性质，不打印编号。

核心维护程序所持描述符的表：自行打开的（`unix_listen`、`unix_connect`、`unix_accept`、`shm_create`）及经套接字收到的（`unix_recv`），直到 `unix_close`。指定任何其他编号的调用，都在任何系统调用之前被拒绝：

- 负数或大于 `i32::MAX`：`<fn>: not a file descriptor: <n>`（此次变更前 native 的文本）；
- 否则：`<fn>: descriptor <n> is not open in this program`。

在 embedded host 上，这阻止 guest 接触或关闭 host 自己的描述符（wasmtime 的、host 的 stdio）。Native 回答相同，因此这是 **native 行为变化**：此前 `net.unix_close(0)` 会关闭进程 stdin，`net.unix_poll(2, 0)` 会 poll stderr。现在关闭套接字还会关闭它收到但从未取出的描述符（native 原先会泄漏它们）。

<a id="operand-encoding-ops-104116"></a>

## 操作数编码（ops 104..=116）

每个 op 两个操作数，与 TCP 相同：`a` 是文本，`b` 是文本或原始字节。回答为 ok / err。

| op | 函数 | a | b | ok 回答 |
|---|---|---|---|---|
| 104 | `unix_connect` | path | — | 描述符 |
| 105 | `unix_listen` | path | — | 描述符 |
| 106 | `unix_accept` | listener | — | 描述符 |
| 107 | `unix_send` | socket，随后为各个待发送描述符，每行一个 | 原始字节 | — |
| 108 | `unix_recv` | socket | max | 原始字节 |
| 109 | `unix_take_fds` | socket | — | 描述符，每行一个 |
| 110 | `unix_poll` | 描述符 | ms | `1` / `0` |
| 111 | `unix_wait` | 描述符，每行一个 | ms | 已就绪描述符，每行一个 |
| 112 | `unix_close` | 描述符 | — | — |
| 113 | `shm_create` | size | — | 描述符 |
| 114 | `shm_resize` | 描述符 | size | — |
| 115 | `shm_write` | `<fd>\n<offset>` | 原始字节 | — |
| 116 | `shm_read` | 描述符 | `<offset>\n<len>` | 原始字节 |

编号紧接 tcp 家族（91..=103）。其他表均未使用 104..=116：已检查 p1 op 集（`almide-wasi` 的 `P1_SERVED_OPS`、`FS_SERVICE_OPS`）、http 43..=65、serve 70..=72、73 / 74、process 80..=90，以及 `component_availability::operation_name`。

<a id="retirement-commitment"></a>

## 退役承诺

**当 ADR-0023 step 7 将 embedded 路径作为 p3 component 运行时，ops 104..=116 退役。** 同一步还会退役 tcp 家族的 ops 91..=103，届时由 `wasi:sockets` 提供。WASI 没有此家族的对应接口，因此在 step 7，要么通过 component world 中 Almide 自己的接口提供它（#3620 的计划），要么在该接口存在前，wasm 上不可用。无论哪种情况，这些 core-module ops 都不会存续到 step 7 之后。该承诺也写在 op 表处：`crates/almide-wasm-run/src/net_unix_host.rs` 与 `crates/almide-wasm/src/net_call.rs`。

<a id="evidence"></a>

## 证据

- `tests/net_unix_embedded_cross_test.rs` 与 `spec/embedded_cross` 扫描：`spec/embedded_cross/net_unix_loopback.almd`（C-382，每个函数的成功路径，双向传递描述符）及 `net_unix_errors.almd`（C-383，每个函数的错误路径）在 native 和 `almide run --target wasm` 上打印同样的行，且两侧均到达末尾的 `done` 行。第三个测试固定 native 的错误文本。
- `crates/almide-rt-core/tests/net_unix_core.rs`：表的拒绝行为、未取出描述符的关闭，以及 `unsafe` 的许可范围。
- `tests/net_unix_test.rs`（native，未变）仍通过。

<a id="contracts"></a>

## 契约

ALS-R5 下的 C-382（一致）与 C-383（错误）连同两个 fixture 先落地到 judge（almide/als#145），此树在 `spec/embedded_cross/` 中逐字保留它们。每个 fixture 在临时目录中的全新随机 path 上 listen，并在最后一行前移除，所以扫描无需参数即可运行。声明如下：

1. **一致。** ALS-R5 (#3620)：`net.unix_*` / `net.shm_*` 家族（`unix_connect`、`unix_listen`、`unix_accept`、`unix_send`、`unix_recv`、`unix_take_fds`、`unix_poll`、`unix_wait`、`unix_close`、`shm_create`、`shm_resize`、`shm_write`、`shm_read`）在 Unix host 的 native 侧与 EMBEDDED wasm 路径（`almide run --target wasm`）回答相同。收到的字节一致。收到的描述符数量和发送顺序一致，各自都是同一已打开文件的新描述符（经一个写入可由另一个读取）。`unix_poll` / `unix_wait` 的就绪状态一致（数据、待处理连接或 peer 关闭；`unix_wait` 按给定顺序返回就绪描述符）。`shm_read` 返回相同字节（创建及增长时填零，文件末尾返回较少字节）。描述符是原始 OS 编号，可能在两侧不同；其性质（非负、打开时彼此不同）一致。stock 制品（`almide build --target wasm`）在构建时拒绝所有这些操作（E081），因为 WASI 既没有 Unix 域套接字，也没有共享内存。
2. **错误。** ALS-R5 (#3620)：`net.unix_*` / `net.shm_*` 调用的每次失败都是 `err`，绝不是 trap，且 native 与 embedded wasm 路径文本相同。调用只作用于程序持有的描述符（由 `unix_listen`、`unix_connect`、`unix_accept` 或 `shm_create` 返回，或由 `unix_recv` 接收，直到 `unix_close`），并在任何系统调用之前拒绝其他描述符：n < 0 或 n > 2^31-1 时为 `<fn>: not a file descriptor: <n>`，否则为 `<fn>: descriptor <n> is not open in this program`，包括标准流与已关闭描述符。参数定义域有 Almide 自己的文本，在描述符之后检查：`shm_create: negative size <n>`、`shm_resize: negative size <n>`、`shm_write: negative offset <n>`、`shm_read: negative offset or length (<o>, <n>)`、`unix_send: <k> descriptors in one message (at most 28)`。OS 拒绝显示为 `<fn>: <the platform's error text>`，`unix_connect(<path>)` 与 `unix_listen(<path>)` 带上 path，且两侧从同一 host 获取文本。`unix_take_fds` 从不失败：没有收到描述符的套接字，或未持有的套接字，返回空 list。关闭套接字会关闭它收到但从未取出的描述符。
