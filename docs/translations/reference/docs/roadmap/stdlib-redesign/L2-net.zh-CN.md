> 说明（固定历史）：标准库重设计 L2/L3 — net：仅保留 wasi:sockets 能承载的内容；unix_*/shm_* 移入 unix（#3853），新增 UDP、DNS 查询与 IP 地址类型（R-L1-06），tcp_is_open 改为 effect。
<a id="l2l3-net"></a>

# L2/L3: net

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-net.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-net.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-net.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取、独立runtime/安全测试或核实现行API。(F)、probe实测及2026-11-01 shim/状态叙述均为原作者报告。保留2026-10-21页首日期与后续更新、临时host op与退役计划/stock p3路径、初始UDP close/三元组与后来io.close/记录、不透明类型先拒绝后采纳。表中旧Int签名及udp_recv_from的R-net-01归属仍保留原文不一致，不是现行API指南。权限显示不构成普遍安全保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-net.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-06：新增 UDP、DNS、IP；R-L1-26：Unix socket 与共享内存移入 `unix`）。
> 来源：Rust https://doc.rust-lang.org/std/net/ ・ Go https://pkg.go.dev/net ・ Python https://docs.python.org/3/library/socket.html ・ TS/JS https://nodejs.org/api/net.html ・ https://nodejs.org/api/dgram.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （没有）・ Elixir https://www.erlang.org/doc/apps/kernel/gen_udp.html ・ OCaml https://ocaml.org/manual/latest/api/Unix.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/ （JVM 的 java.net）・ Swift https://developer.apple.com/documentation/network
> WASI：https://github.com/WebAssembly/wasi-sockets （`tcp`、`udp`、`ip-name-lookup`）。(F) = 原调查中获取并核对的单元格。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| TCP 连接、读写、关闭 | `tcp_connect`・`tcp_read`・`tcp_write`・`tcp_read_exact`・`tcp_close` | `TcpStream::connect`・`read`・`write_all`・`read_exact` | `Dial`・`Read`・`Write`・`Close` | `create_connection`・`recv`・`sendall` | `net.connect`・`socket.write` | — | `:gen_tcp.connect`・`recv`・`send` | `Unix.connect`・`read`・`write` | `Socket` | `NWConnection` |
| TCP 超时、可用量 | `tcp_read_timeout`・`tcp_set_timeout`・`tcp_available` | `set_read_timeout` | `SetDeadline` | `settimeout` | `setTimeout` | — | `recv(…, timeout)` | `setsockopt_float` | `soTimeout` | — |
| TCP 监听 | `tcp_listen`・`tcp_accept`・`tcp_local_port`・`tcp_close_listener` | `TcpListener::bind`・`accept`・`local_addr` | `Listen`・`Accept`・`Addr` | `bind`・`listen`・`accept` | `net.createServer` | — | `:gen_tcp.listen`・`accept` | `Unix.listen`・`accept` | `ServerSocket` | `NWListener` |
| 是否打开 | `tcp_is_open` | — | — | `fileno() != -1` | `socket.destroyed` | — | — | — | `isClosed` | — |
| UDP | — | `UdpSocket::bind`・`send_to`・`recv_from`・`local_addr` (F) | `ListenUDP`・`WriteTo`・`ReadFrom` (F) | `SOCK_DGRAM`・`sendto`・`recvfrom` | `dgram.createSocket`・`send`・`on('message')` | — | `:gen_udp.open`・`send`・`recv` | `Unix.sendto`・`recvfrom` | `DatagramSocket` | `NWConnection(.udp)` |
| 名称查询（DNS） | —（在 connect 内隐式进行） | `ToSocketAddrs` | `LookupHost` (F) | `getaddrinfo` | `dns.lookup` | — | `:inet.gethostbyname` | `Unix.getaddrinfo` | `InetAddress.getAllByName` | — |
| IP 地址类型 | — | `IpAddr`（`"…".parse()`） | `ParseIP` (F)・`netip.ParseAddr` | `ipaddress.ip_address` | `net.isIP` | — | `:inet.parse_address` | `Unix.inet_addr_of_string` | `InetAddress` | `IPv4Address` |
| Unix socket、共享内存 | `unix_*`・`shm_*` | `os::unix::net` | `net`（`UnixConn`） | `AF_UNIX`・`shared_memory` | `net`（path） | — | `{:local, path}` | `Unix` | JVM 16 起 | — |

<a id="完全性の規則"></a>

### 完备性规则

- `net` 只包含 `wasi:sockets` 能承载的内容（R-L1-26）：TCP、UDP、名称解析、IP 地址。Unix socket 与共享内存移入 `unix`（[L2-unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md)）。
- TCP 与 UDP 的 socket 族保持相同形状：{打开、自身端口 `<proto>_local_port`}。无论句柄种类，读、写、关闭均使用 `io.read`、`io.write`、`io.close`（R-L0-44）。只有无连接的 UDP 提供指定对端的 `udp_send_to`/`udp_recv_from`。
- 失败表达（R-L0-06 第 1、4 条，不返回哨兵）：对端关闭时，读取返回空 `Bytes`（当前 `tcp_read` 的约定）。名称解析失败、IP 字符串无法解析时返回 err（有原因）。
- `tcp_set_timeout` 中的 `set_` 并非修改 Almide 值的动词。它是改变 OS 侧 socket 设置的 effect fn，无 `mut` 参数，句柄值也不改变。R-L0-04 中“`set` 是复制侧动词”针对值（记录、容器），两者不冲突。
- 函数名前缀（`tcp_`、`udp_`）用于区分同一模块中的两种协议。不改现有拼写（R-L0-00）。

<a id="l2-の判定"></a>

### L2 判定

- 位置不当：13 个 `unix_*`、`shm_*` 函数（WASI 不包含它们；在 `net` 内混淆“wasm 尚未提供”与“stock 永远不提供”）→ R-net-01。
- 缺少职责：UDP、DNS、IP 地址类型（7/9）→ R-net-02、R-net-03。
- 声明漏洞：`tcp_is_open` 是纯 `fn`，却读取句柄表（实测可从纯 fn 调用，probe-io）→ R-net-04。
- 类型：`TcpStream`、`TcpListener` 是 `Int` 别名，混淆流与监听器不会被类型阻止，也无法符合输入输出 protocol → R-net-05。
- 输入输出层：TCP、UDP、Unix socket、标准输入输出各自使用不同的读、写、关闭名称 → R-net-07（R-L0-44）。
- R-L0-09：`net.unix_poll` 是 L0 点名的例外 → R-net-01（迁移时统一为 `unix.select` 并移除）。

<a id="案"></a>

## 提案

<a id="r-net-01-unix_shm_-を-unix-モジュールへ移す---採用"></a>

### R-net-01 将 `unix_*`、`shm_*` 移入 `unix` 模块 — ○ 采纳

- 内容：13 个函数移入 `unix.*`，移除 `net.unix_*`、`net.shm_*`。新拼写与类型（`unix.Fd`）由 [L2-unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md) 的 R-unix-01 决定。不迁移 `net.unix_poll`，统一为 `unix.select([fd], t)`。
- 依据的 L0 规则：R-L0-02（Unix 进程间通信不是 `net` 的主体）、R-L0-09（`unix_poll`）、R-L0-15、R-L1-26。
- (b) 原样迁移现有实现。wasm 由已裁定的临时内置 host op 104～116 承载（ADR-0023 step 7 退役）。WASI 没有 Unix socket 或共享内存，所以退役时若仍无 WASI 接口，就把 `unix` 移出标准库，改为 package（R-L0-15 的第三条路）。不以仅支持 native 为终点。详见 [L2-unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md) 的 R-unix-01。
- (c) CHEATSHEET、llms.txt 没有这 13 个函数。不再与 `net.tcp_*` 并列后，普通网络代码就不易混淆。
- (d) 迁移。`@deprecated(since=15, use="unix.…")`、`removal_epoch = 16`。`Int` → `unix.Fd` 类型变化仅发生在新名称侧，旧名称语义不变（R-L0-14 第 4 条）。使用数（spec/tests/docs/stdlib）：例如 `unix_send` 11/3/5/1、`unix_close` 17/8/5/1、`shm_read` 14/4/5/1。13 个函数的合计见 L2-unix 表。
- 门禁提案：`net` 公开函数在 `target-availability.toml` 中不存在任何 `native-only-forever` 行。
- issue：#3853（已有）

<a id="r-net-02-udp-を足す---採用"></a>

### R-net-02 新增 UDP — ○ 采纳

- 内容：
  ```almide
  type UdpSocket                                                    // 不透明（R-net-05）
  effect fn udp_bind(host: String, port: Int) -> UdpSocket          // port 0 で OS が選ぶ
  effect fn udp_local_port(sock: UdpSocket) -> Int
  effect fn udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit
  effect fn udp_recv_from(sock: UdpSocket, max: Int) -> (Bytes, String, Int)   // (データ, 相手の IP, 相手のポート)
  effect fn udp_close(sock: UdpSocket) -> Unit                      // 2 回目も ok
  ```
- 保留代码注释的译文：UdpSocket 不透明（R-net-05）；port 0 由 OS 选择；接收三元组为（数据、对端 IP、对端端口）；第二次关闭也成功。
- 拼写：`bind`、`send_to`、`recv_from` 与 Rust(F)、Python、OCaml 相同。`local_port`、`close` 对齐 `tcp_` 族。
- 依据的 L0 规则：L1 R-L1-06、R-L0-15、R-L0-03（socket 在前）。
- (a) 7/9 提供。
- (b) 目标是 `wasi:sockets/udp`（名称解析为 `wasi:sockets/ip-name-lookup`），由 p3 component 承载。自 2026-11-01 起，p3 component shim 通过 `wasi:sockets@0.3` 的 `udp-socket` 和 `ip-name-lookup` 响应 op 140..=145（`crates/almide-wasm-run/src/wasi_p3_net.rs`）。shim 按与 Rust `IpAddr` 相同的规则解析 IP 字符串，只将名称交给 resolver。在此之前，内置 host 按 TCP 裁定（2026-10-12）的方式，以**临时** host op 140..=145 直接使用 native rt-core（`net_core`）响应（#4306）。这些 op 与 TCP 的 91..=103 一样，在 ADR-0023 step 7 内置 host 运行 p3 component 时移除。stock p1 没有 socket，因此 `almide build --target wasm` 对 op 集合含 TCP、UDP 族（`almide_wasm::host_ops::SOCKET_FAMILIES`）op 的程序，与 `http.serve` 一样默认构建为 p3 component，并显示所需权限（`-S inherit-network`）。因此已从 target-availability 的 stock p1 行移除（2026-11-01，pending_port 23→9）。检验使用 `spec/embedded_cross/net_udp_loopback.almd`、`net_udp_errors.almd`（C-442 / C-443）。
- (c) 若没有它，LLM 就无法编写“通过 UDP 发送”的任务。
- (d) 新增。使用数 0。
- 门禁提案：矩阵测试确认 TCP、UDP 两族（打开、端口、关闭）形状一致。native 与 wasm 的 cross fixture。
- issue：#3930

<a id="r-net-03-名前の解決と-ip-アドレスの型を足す---採用"></a>

### R-net-03 新增名称解析和 IP 地址类型 — ○ 采纳

- 内容：
  ```almide
  type IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])   // V6 は 16 ビットの値 8 つ
  effect fn lookup_host(host: String) -> List[IpAddr]   // 見つからなければ err
  fn parse_ip(s: String) -> Result[IpAddr, String]
  fn ip_to_string(ip: IpAddr) -> String                  // V6 は RFC 5952 の短い形
  fn is_loopback(ip: IpAddr) -> Bool
  ```
- 保留代码注释的译文：V6 包含 8 个 16 位值；找不到时 lookup 返回 err；V6 字符串采用 RFC 5952 短形式。
- 拼写：`lookup_host` 对齐 Go(F)，`parse_ip` 对齐 Go 的 `ParseIP`(F) 和 Almide `int.parse`、`url.parse` 的“从字符串构造＝parse”。
- 顺序：不将 `IpAddr` 声明为有序类型，因此不提供 `compare`（R-L0-16 仅针对有序类型）。相等性使用 `==`。
- 依据的 L0 规则：R-L1-06、R-L0-06 第 4 条（无法解析的字符串返回 err，不返回哨兵）、R-L0-09（`is_loopback`）。
- (a) 7/9 提供。
- (b) `lookup_host` 使用 `wasi:sockets/ip-name-lookup` 的 `resolve-addresses`。结果顺序由 host 决定，因此按 ADR-0027 D3 的 verdict（`localhost` 包含 loopback）检验。`parse_ip`、`ip_to_string`、`is_loopback` 是纯函数。
- (c) 当前只在 `tcp_connect` 内隐式解析，没有查询地址的手段。
- (d) 新增。使用数 0。
- 门禁提案：`parse_ip` 与 `ip_to_string` 往返性质测试。
- issue：#3929

<a id="r-net-04-tcp_is_open-を-effect-fn-にする---採用"></a>

### R-net-04 将 `tcp_is_open` 改为 effect fn — ○ 采纳

- 内容：将 `fn tcp_is_open(stream: Int) -> Bool` 改为 `effect fn`。
- 依据的 L0 规则：R-L0-13、ADR-0022 D3。
- (b) 仅检查器。(c) 不读声明，就无法知道同一句柄的函数中唯独它是纯函数。
- (d) 语义变化（纯 → effect）。名称不变，因此不加 `@deprecated`。纯 fn 中的调用产生 E006，提示“将调用者函数改为 `effect fn`”。旧写法在类型检查处停止，不会悄悄得到不同结果（R-L0-14 第 4 条）。epoch 15 的 `breaks`。使用数 10/0/5/1。
- issue：与 R-process-04 相同的新 issue（读取 host 状态的纯 fn）
- issue：#3907

<a id="r-net-05-tcp-のハンドルを不透明な型にする---採用十三巡目に見直しr-l0-44"></a>

### R-net-05 将 TCP 句柄改为不透明类型 — ○ 采纳（第十三轮重新考虑，R-L0-44）

- 提案：`type TcpStream`、`type TcpListener`、`type UdpSocket` 改为不透明类型（不是 `Int` 别名）。`tcp_connect`、`tcp_accept` 返回 `TcpStream`，`tcp_listen` 返回 `TcpListener`，其他函数接收它们。
- 重新考虑的理由：具体类型以 protocol 的组合表达（R-L0-37）。保持 `Int` 就无法让句柄符合 `Read`、`Write`、`Close`（R-L0-44）。若令 `Int` 符合 `Read`，普通数字也能传入 `io.read`。同一个 `net` 模块内 UDP 有类型、只有 TCP 是 `Int` 的不一致也随之消失。`unix.Fd` 已是不透明类型（R-unix-01）。
- (a) 带类型句柄占多数：Rust `TcpStream`、`TcpListener`，Go `net.Conn`、`net.Listener`，Python `socket`，Kotlin `Socket`、`ServerSocket`，Swift（Network）`NWConnection`、`NWListener`。
- (d) 语义变化（更换类型）。旧写法仅在把句柄放进 `Int` 字段或列表处产生类型错误，不会悄悄得到不同值（R-L0-14 第 4 条）。`almide fix` 无法重写类型注解中的 `Int`，因此提示“改为 `net.TcpStream`”。
- 以前的判定（× 不采纳）：

- 理由：让所有原本正确运行的用户代码（如用 `Int` 字段保存句柄的记录）都重写，而尚无混淆流与监听器的报告。虽不违反兼容规则（R-L0-14 第 4 条）（`TcpStream`、`TcpListener` 是 `Int` 别名，不透明化只让旧写法出现类型错误），但收益不抵重写量。目前没有混淆流与监听器的报告。#3853 指出“混用 TCP 句柄与 OS 描述符”，在 R-net-01 将 `unix.Fd` 不透明化即可阻止。
- issue：无

<a id="r-net-06-udp_recv_from-の-3-つ組をレコードにする---採用"></a>

### R-net-06 将 `udp_recv_from` 三元组改为记录 — ○ 采纳

- 当前：`net.udp_recv_from(sock, max) -> (Bytes, String, Int)`。它让人按位置记住两个类型不同的字段。
- 提案：返回 `type UdpPacket = { data: Bytes, host: String, port: Int }`。`host` 使用 `String`，可直接传给 `udp_send_to(host: String, …)` 和 TCP 函数。
- 依据的 L0 规则：R-L0-01。多返回值仅在同类值成组时采用元组（`list.partition` 的两个列表）；职责不同的三个以上字段采用记录（与 `ProcessStatus`、`FileStat` 相同）。
- (a) 排列顺序各异：Python `(bytes, (host, port))`、Rust `(usize, SocketAddr)`、Erlang `{Address, Port, Packet}`。
- (d) 新函数，成本为 0。
- issue：#3974

<a id="r-net-07-tcpudp-の読み書きと閉じるを-io-の総称関数にする---採用"></a>

### R-net-07 TCP、UDP 的读写与关闭改用 `io` 泛型函数 — ○ 采纳

- 内容：迁移 `net.tcp_read` → `io.read`、`tcp_write` → `io.write`、`tcp_read_exact` → `io.read_exact`、`tcp_close`、`tcp_close_listener` → `io.close`，移除 `net` 的 5 个函数。不提供 R-net-02 的 `udp_close`，改用 `io.close`。
- 保留：`tcp_read_timeout`、`tcp_set_timeout`、`tcp_available`、`tcp_is_open`（TCP 专属操作），以及 `tcp_connect`、`tcp_listen`、`tcp_accept`、`tcp_local_port`、`udp_*`（打开、查询）。
- 依据的 L0 规则：R-L0-44、R-L0-01（相同职责仅一个拼写）。
- (d) 移除 5 个。使用 `@deprecated(since=15, use="io.read")` 等，由 `almide fix` 一对一重写（`net.tcp_read(s, n)` → `io.read(s, n)`）。旧名称不再复用（R-L0-43）。
- issue：#4015

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，标明 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `net.tcp_connect` | `effect fn tcp_connect(host: String, port: Int) -> Int` | Rust `TcpStream::connect` / Go `Dial` / Python `create_connection` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_read` | `effect fn tcp_read(stream: Int, len: Int) -> Bytes` | Rust `read` / Python `recv` | 移除（→ `io.read`） | R-net-07 |
| `net.tcp_write` | `effect fn tcp_write(stream: Int, data: Bytes) -> Unit` | Rust `write_all` / Python `sendall` | 移除（→ `io.write`） | R-net-07 |
| `net.tcp_read_exact` | `effect fn tcp_read_exact(stream: Int, len: Int) -> Bytes` | Rust `read_exact` / Go `io.ReadFull` | 移除（→ `io.read_exact`） | R-net-07 |
| `net.tcp_close` | `effect fn tcp_close(stream: Int) -> Unit` | Go `Close` / Python `close` | 移除（→ `io.close`） | R-net-07 |
| `net.tcp_is_open` | `fn tcp_is_open(stream: Int) -> Bool` | Kotlin `isClosed` 的否定 | 语义变化（改为 effect fn，并使用 `stream: TcpStream`） | R-net-04、R-net-05 |
| `net.tcp_read_timeout` | `effect fn tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes` | Elixir `recv(…, timeout)` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_set_timeout` | `effect fn tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit` | Rust `set_read_timeout` / Python `settimeout` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_available` | `effect fn tcp_available(stream: Int) -> Int` | Java `available` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_listen` | `effect fn tcp_listen(host: String, port: Int) -> Int` | Rust `TcpListener::bind` / Go `Listen` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_local_port` | `effect fn tcp_local_port(listener: Int) -> Int` | Rust `local_addr` / Go `Addr` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_accept` | `effect fn tcp_accept(listener: Int) -> Int` | Rust・Go・Python `accept` | 语义变化（句柄改为 `TcpStream`・`TcpListener`） | R-net-05 |
| `net.tcp_close_listener` | `effect fn tcp_close_listener(listener: Int) -> Unit` | Go `Listener.Close` | 移除（→ `io.close`） | R-net-07 |
| `net.udp_bind` | `effect fn udp_bind(host: String, port: Int) -> UdpSocket`（提案） | Rust `UdpSocket::bind`(F) / Go `ListenUDP`(F) | 新增 | R-net-02 |
| `net.udp_local_port` | `effect fn udp_local_port(sock: UdpSocket) -> Int`（提案） | Rust `local_addr`(F) | 新增 | R-net-02 |
| `net.udp_send_to` | `effect fn udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit`（提案） | Rust `send_to`(F) / Go `WriteTo`(F) / Python `sendto` | 新增 | R-net-02 |
| `net.udp_recv_from` | `effect fn udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket`（提案） | Rust `recv_from`(F) / Go `ReadFrom`(F) / Python `recvfrom` | 新增（`UdpPacket` 返回） | R-net-01、R-net-06 |
| `net.lookup_host` | `effect fn lookup_host(host: String) -> List[IpAddr]`（提案） | Go `LookupHost`(F) / Python `getaddrinfo` / Node `dns.lookup` | 新增 | R-net-03 |
| `net.parse_ip` | `fn parse_ip(s: String) -> Result[IpAddr, String]`（提案） | Go `ParseIP`(F) / Rust `parse::<IpAddr>` / Elixir `parse_address` | 新增 | R-net-03 |
| `net.ip_to_string` | `fn ip_to_string(ip: IpAddr) -> String`（提案） | Rust `to_string` / Go `String` | 新增 | R-net-03 |
| `net.is_loopback` | `fn is_loopback(ip: IpAddr) -> Bool`（提案） | Rust `is_loopback` / Go `IsLoopback` / Python `is_loopback` | 新增 | R-net-03 |
| `net.unix_connect` | `effect fn unix_connect(path: String) -> Int` | Rust `UnixStream::connect` | 移除（→ `unix.connect`） | R-net-01 |
| `net.unix_listen` | `effect fn unix_listen(path: String) -> Int` | Rust `UnixListener::bind` | 移除（→ `unix.listen`） | R-net-01 |
| `net.unix_accept` | `effect fn unix_accept(listener: Int) -> Int` | Rust `accept` | 移除（→ `unix.accept`） | R-net-01 |
| `net.unix_send` | `effect fn unix_send(sock: Int, data: Bytes, fds: List[Int]) -> Unit` | Python `send_fds` | 移除（→ `unix.write_fds`） | R-net-01 |
| `net.unix_recv` | `effect fn unix_recv(sock: Int, max: Int) -> Bytes` | Python `recv_fds` | 移除（→ `io.read`） | R-net-01 |
| `net.unix_take_fds` | `effect fn unix_take_fds(sock: Int) -> List[Int]` | — | 移除（→ `unix.take_fds`） | R-net-01 |
| `net.unix_poll` | `effect fn unix_poll(fd: Int, timeout_ms: Int) -> Bool` | Python `select.poll` | 移除（→ `unix.select([fd], t)`） | R-net-01 |
| `net.unix_wait` | `effect fn unix_wait(fds: List[Int], timeout_ms: Int) -> List[Int]` | Python `select.select` / Go —  | 移除（→ `unix.select`） | R-net-01 |
| `net.unix_close` | `effect fn unix_close(fd: Int) -> Unit` | Python `os.close` | 移除（→ `io.close`） | R-net-01 |
| `net.shm_create` | `effect fn shm_create(size: Int) -> Int` | Python `SharedMemory(create=True)` | 移除（→ `unix.shm_create`） | R-net-01 |
| `net.shm_resize` | `effect fn shm_resize(fd: Int, size: Int) -> Unit` | POSIX `ftruncate` | 移除（→ `unix.shm_resize`） | R-net-01 |
| `net.shm_write` | `effect fn shm_write(fd: Int, offset: Int, data: Bytes) -> Unit` | POSIX `pwrite` | 移除（→ `unix.shm_write`） | R-net-01 |
| `net.shm_read` | `effect fn shm_read(fd: Int, offset: Int, len: Int) -> Bytes` | POSIX `pread` | 移除（→ `unix.shm_read`） | R-net-01 |
