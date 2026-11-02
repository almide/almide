<!-- description: Stdlib redesign L2/L3 — unix (new, #3853): the Unix-socket and shared-memory IPC floor leaves net under an explicit import, with an opaque Fd type and one readiness wait -->
<a id="l2l3-unix新しいモジュール"></a>

# L2/L3：unix（新模块）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-unix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-unix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-unix.zh-CN.md)

> 非规范性、带日期的重设计快照。原本不变且为权威，不自动同步；未重新统计使用数或审计当前实现/host。按顺序保留采纳方案：初始wait/send/recv/close发展至select、泛型io及write_fds，不静默重写旧表及代码。临时embedded操作不代表stock WASI支持；迁移或改package仍有条件。跨语言完整性与零迁移成本为原文主张，不是独立复验保证。代码/注释保持原样，代码外附注释译文。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-26：将Unix socket及共享内存移到`unix`，#3853）
> 来源：Rust https://doc.rust-lang.org/std/os/unix/net/ ；Go https://pkg.go.dev/net#UnixConn ；Python https://docs.python.org/3/library/socket.html#socket.send_fds ；https://docs.python.org/3/library/multiprocessing.shared_memory.html ；TS/JS https://nodejs.org/api/net.html#ipc-support ；Gleam — 无；Elixir https://www.erlang.org/doc/apps/kernel/gen_tcp.html （`{:local, path}`）；OCaml https://ocaml.org/manual/latest/api/Unix.html ；Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/UnixDomainSocketAddress.html ；Swift — 标准库无。
> 实体：`crates/almide-rt-core/src/net_unix_core.rs`、`docs/roadmap/active/net-unix-shm-embedded.md`（#3620，临时op104～116）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide（当时→方案） | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 连接、监听、接受 | `net.unix_connect`、`unix_listen`、`unix_accept` → `unix.connect`、`listen`、`accept` | `UnixStream::connect`、`UnixListener::bind`、`accept` | `DialUnix`、`ListenUnix`、`AcceptUnix` | `socket(AF_UNIX)`、`bind`、`accept` | `net.connect({path})` | — | `:gen_tcp.connect({:local, p})` | `Unix.connect`（`ADDR_UNIX`） | `SocketChannel.open(UnixDomainSocketAddress)` | — |
| 携描述符发送 | `net.unix_send` → `unix.send` | `send_vectored_with_ancillary`（unstable） | `WriteMsgUnix` | `send_fds` | — | — | — | — | — | — |
| 接收并取出已到描述符 | `net.unix_recv`、`unix_take_fds` → `unix.recv`、`take_fds` | `recv_vectored_with_ancillary`（unstable） | `ReadMsgUnix` | `recv_fds` | — | — | — | — | — | — |
| 等待可读 | `net.unix_poll`、`unix_wait` → `unix.wait` | —（`mio`是外部库） | — | `select.select`、`poll` | — | — | — | `Unix.select` | `Selector` | — |
| 关闭 | `net.unix_close` → `unix.close` | drop | `Close` | `os.close` | `destroy` | — | `close` | `Unix.close` | `close` | — |
| 共享内存 | `net.shm_*` → `unix.shm_create`、`shm_resize`、`shm_write`、`shm_read` | — | — | `SharedMemory` | — | — | — | `Unix.map_file` | — | — |

<a id="完全性の規則"></a>

### 完整性规则

- 角色限定为#2912的Wayland、PipeWire、D-Bus描述符传递基础：{连接、监听、接受、携描述符发送、接收、取描述符、等待、关闭}，共享内存{创建、调整大小、写、读}。
- 描述符是不透明`unix.Fd`，不可与`net`的TCP handle（`Int`）混用，见#3853第2项。
- 等待函数只有`wait`，等待单个描述符写作`unix.wait([fd], t)`。返回Bool的`unix_poll`违反R-L0-09，且与`wait`同角色，因此不迁移。
- `shm_`前缀表示模块中的共享内存族，与`net`中`tcp_`、`udp_`同用法。
- 需要`import unix`，不自动import。
- 定位写入`shm_write`在`offset`为负或`offset + bytes.len(data)`超出大小时abort，遵循L0越界写入abort规则。要扩容先调`shm_resize`。读取`shm_read`按R-L0-12在末尾变短。

<a id="l2-の判定"></a>

### L2判定

- 新模块，无缺失角色。Rust、Go反而更少：Rust描述符传递unstable，且无共享内存。

<a id="案"></a>

## 方案

<a id="r-unix-01-unix-モジュールを作りnetunix_netshm_-を移す---採用"></a>

### R-unix-01 创建`unix`并迁移`net.unix_*`、`net.shm_*` — ○ 采纳

- 内容：
  ```almide
  type Fd                                                        // 不透明。Int に変換しない
  effect fn connect(path: String) -> Fd
  effect fn listen(path: String) -> Fd                            // path はまだ無いこと
  effect fn accept(listener: Fd) -> Fd
  effect fn send(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit    // fds は最初のバイトと一緒に渡る
  effect fn recv(sock: Fd, max: Int) -> Bytes                     // 相手が閉じたら空
  effect fn take_fds(sock: Fd) -> List[Fd]                        // 届いて未取得の記述子、古い順
  effect fn wait(fds: List[Fd], timeout_ms: Int) -> List[Fd]      // 読めるものを順に。< 0 は 1 つ読めるまで
  effect fn close(fd: Fd) -> Unit
  effect fn shm_create(size: Int) -> Fd                           // 0 で埋めた名前のないファイル
  effect fn shm_resize(fd: Fd, size: Int) -> Unit
  effect fn shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit
  effect fn shm_read(fd: Fd, offset: Int, len: Int) -> Bytes      // 末尾では短くなる
  ```
  代码注释按声明顺序译文：Fd不透明，不转Int；listen要求path尚不存在；描述符随第一个byte传递；对端关闭则recv为空；take_fds按旧到新取已到、尚未取得的描述符；wait按顺序返回可读项，负timeout等待至少一项可读；shm_create创建零填充无名文件；shm_read在末尾可变短。
- L0依据：R-L0-02（主语是描述符而非网络）、R-L0-09（不保留`unix_poll`）、R-L0-14第4项（仅新名称侧改为`Fd`）、R-L0-15。
- (a) 只有Go把Unix socket与TCP放同模块，Rust分为`std::os::unix::net`。只有Python在标准库提供共享内存。
- (b) Native使用同一个`net_unix_core.rs`。Wasm：WASI p1、0.2、0.3没有Unix socket或共享内存。嵌入host通过已裁定临时op104～116承载，在ADR-0023 step7退役；structural leg使用`stdlib/net_wasm.almd`本体。不带临时op的stock wasm host条目，与临时TCP op一样记录为“临时嵌入host op”，不列为native-only。用ADR-0027 D5的Unix echo fixture同时验收native和嵌入host，D8禁止native-only标签。R-L0-15规定退役路径：届时若有承载Unix socket／共享内存的WASI接口，则迁移过去；否则从标准库移除`unix`，改为package `almide/unix`。
- (c) 这13个函数不再排在`net.tcp_*`旁边。误把TCP handle传给`shm_read`会成为类型错误。
- (d) 新增模块，同时从`net`撤除，见L2-net R-net-01。给`net.unix_*`、`net.shm_*`标`@deprecated(since=15, use="unix.…")`、`removal_epoch = 16`。`Int`→`Fd`改变类型，因此`almide fix`只改名（`net.unix_send`→`unix.send`并添加`import unix`），类型不符由checker显示。`shm_write`越界改abort也只影响新名称侧。使用数按spec/tests/docs/stdlib、旧`net.`名称：connect 6/3/6/1、listen 4/1/5/1、accept 6/2/5/1、send 11/3/5/1、recv 10/2/5/1、take_fds 7/2/5/1、poll 9/3/6/1、wait 5/2/4/1、close 17/8/5/1、shm_create 6/4/5/1、shm_resize 5/1/4/1、shm_write 7/4/5/1、shm_read 14/4/5/1。
- 迁移内容，#3853清单：`proofs/target-availability.toml`的13行、semantics manifest的`file-descriptor`单位、验收台账、`docs/stdlib/net.md`相关节。
- Gate方案：checker测试`unix.Fd`不能与`Int`统一；`net`不留下`native-only-forever`行，见L2-net。
- issue：#3853，既有。

<a id="r-unix-02-unixwait-を-unixselect-にする---採用"></a>

### R-unix-02 将`unix.wait`改为`unix.select` — ○ 采纳

- 现状：`unix.wait(fds, timeout_ms) -> List[Fd]`等待出现可读描述符；`http.wait(c)`、`process.wait(c)`等待一个handle结束并返回结果。
- 方案：`effect fn unix.select(fds: List[Fd], timeout_ms: Int) -> List[Fd]`。`wait`仅用于等待handle结束。
- L0依据：R-L0-00第1项，同一个动词随模块改变含义。
- (a) Python `select.select`、OCaml `Unix.select`、JVM `Selector.select`；没有语言称其为`wait`。
- (d) `unix`是新模块，成本0。将`net.unix_wait`、`unix_poll`的`@deprecated`中`use`设为`unix.select`。
- issue：#3974。

<a id="r-unix-03-unixrecvunixsend-を-unixreadunixwrite-にする---採用"></a>

### R-unix-03 将`unix.recv`、`unix.send`改为`unix.read`、`unix.write` — ○ 采纳

- 现状：已连接stream读写，TCP为`net.tcp_read`、`tcp_write`，Unix socket为`unix.recv`、`send`。
- 方案：`effect fn unix.read(sock: Fd, max: Int) -> Bytes`、`effect fn unix.write(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit`。
- L0依据：R-L0-01，将L2-net规则“已连接TCP用`read`/`write`，无连接UDP用`send_to`/`recv_from`”也用于Unix socket。
- (a) Rust `UnixStream`实现`Read`、`Write`，没有`recv`、`send`（F）。Go `UnixConn`、OCaml也用`read`、`write`。
- (d) 新模块，成本0。
- 完整性规则：已连接stream的{connect, listen, accept, read, write, close}在TCP与Unix同形。
- issue：#3974。

<a id="r-unix-04-読み書きと閉じるを-io-の総称関数にし記述子つきの書き込みを-write_fds-にする---採用"></a>

### R-unix-04 读写及关闭改为`io`泛型函数，携描述符写入名为`write_fds` — ○ 采纳

- 内容：让`unix.Fd`遵循`Read`、`Write`、`Close`（R-L0-44），不放`unix.read`、`unix.close`，使用`io.read`、`io.close`。携描述符写入为`effect fn unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit`，`io.write(sock, data)`不携描述符。接收仍是`io.read`与`unix.take_fds`组合。
- L0依据：R-L0-44、R-L0-01。R-unix-03的TCP／Unix同形变为共享一组`io`，与TCP完全相同调用方式。
- (a) Rust `UnixStream`实现`Read`、`Write`，携描述符是另一函数`send_vectored_with_ancillary`。Python `send_fds`、Go `WriteMsgUnix`，均与无描述符读写分名。
- (d) `unix`新模块，成本0。
- issue：#4015。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（方案） | 其他语言主要写法 | 判定 | 方案 |
|---|---|---|---|---|
| `unix.connect` | `effect fn connect(path: String) -> Fd` | Rust `UnixStream::connect` / Go `DialUnix` | 新增，从`net.unix_connect`迁移 | R-unix-01 |
| `unix.listen` | `effect fn listen(path: String) -> Fd` | Rust `UnixListener::bind` / Go `ListenUnix` | 新增，迁移 | R-unix-01 |
| `unix.accept` | `effect fn accept(listener: Fd) -> Fd` | Rust、Python `accept` / Go `AcceptUnix` | 新增，迁移 | R-unix-01 |
| `unix.write_fds` | `effect fn write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` | Python `send_fds` / Go `WriteMsgUnix` | 新增，迁移 | R-unix-01、R-unix-04 |
| `unix.take_fds` | `effect fn take_fds(sock: Fd) -> List[Fd]` | — | 新增，迁移 | R-unix-01 |
| `unix.select` | `effect fn select(fds: List[Fd], timeout_ms: Int) -> List[Fd]` | Python `select.select` / OCaml `Unix.select` | 新增，迁移并吸收`unix_poll` | R-unix-01、R-unix-02 |
| `unix.shm_create` | `effect fn shm_create(size: Int) -> Fd` | Python `SharedMemory(create=True, size=)` | 新增，迁移 | R-unix-01 |
| `unix.shm_resize` | `effect fn shm_resize(fd: Fd, size: Int) -> Unit` | POSIX `ftruncate` | 新增，迁移 | R-unix-01 |
| `unix.shm_write` | `effect fn shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` | POSIX `pwrite` | 新增，迁移 | R-unix-01 |
| `unix.shm_read` | `effect fn shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` | POSIX `pread` | 新增，迁移 | R-unix-01 |
