> Description (fixed history): Stdlib redesign L2/L3 — net: only what wasi:sockets can carry; the unix_*/shm_* family moves to unix (#3853), UDP, DNS lookup and an IP address type join (R-L1-06), tcp_is_open becomes an effect.
<a id="l2l3-net"></a>

# L2/L3: net

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-net.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-net.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-net.zh-CN.md)

> Non-normative historical redesign snapshot. The pinned original is authoritative and unchanged; no automatic synchronization, fresh retrieval, independent runtime/security testing or current API verification. (F), probe results and the 2026-11-01 shim/status account are original-author reports. Preserve the 2026-10-21 header alongside later updates, temporary host ops versus planned retirement/stock p3 routing, initial UDP close/triple versus later generic io close/record, and rejected-then-accepted opaque handles. Table old Int signatures and R-net-01 attribution of udp_recv_from remain historical inconsistencies, not corrected API guidance. Permission display is not a universal security guarantee. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-net.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-06: add UDP, DNS and IP; R-L1-26: move Unix sockets and shared memory to `unix`).
> Sources: Rust https://doc.rust-lang.org/std/net/ / Go https://pkg.go.dev/net / Python https://docs.python.org/3/library/socket.html / TS/JS https://nodejs.org/api/net.html / https://nodejs.org/api/dgram.html / Gleam https://gleam-stdlib.hexdocs.pm/ (absent) / Elixir https://www.erlang.org/doc/apps/kernel/gen_udp.html / OCaml https://ocaml.org/manual/latest/api/Unix.html / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/ (JVM java.net) / Swift https://developer.apple.com/documentation/network
> WASI: https://github.com/WebAssembly/wasi-sockets (`tcp`, `udp`, `ip-name-lookup`). (F) = cells fetched and checked in this original investigation.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| TCP connection, reading/writing, closing | `tcp_connect`・`tcp_read`・`tcp_write`・`tcp_read_exact`・`tcp_close` | `TcpStream::connect`・`read`・`write_all`・`read_exact` | `Dial`・`Read`・`Write`・`Close` | `create_connection`・`recv`・`sendall` | `net.connect`・`socket.write` | — | `:gen_tcp.connect`・`recv`・`send` | `Unix.connect`・`read`・`write` | `Socket` | `NWConnection` |
| TCP timeout / available amount | `tcp_read_timeout`・`tcp_set_timeout`・`tcp_available` | `set_read_timeout` | `SetDeadline` | `settimeout` | `setTimeout` | — | `recv(…, timeout)` | `setsockopt_float` | `soTimeout` | — |
| TCP listening | `tcp_listen`・`tcp_accept`・`tcp_local_port`・`tcp_close_listener` | `TcpListener::bind`・`accept`・`local_addr` | `Listen`・`Accept`・`Addr` | `bind`・`listen`・`accept` | `net.createServer` | — | `:gen_tcp.listen`・`accept` | `Unix.listen`・`accept` | `ServerSocket` | `NWListener` |
| Whether open | `tcp_is_open` | — | — | `fileno() != -1` | `socket.destroyed` | — | — | — | `isClosed` | — |
| UDP | — | `UdpSocket::bind`・`send_to`・`recv_from`・`local_addr` (F) | `ListenUDP`・`WriteTo`・`ReadFrom` (F) | `SOCK_DGRAM`・`sendto`・`recvfrom` | `dgram.createSocket`・`send`・`on('message')` | — | `:gen_udp.open`・`send`・`recv` | `Unix.sendto`・`recvfrom` | `DatagramSocket` | `NWConnection(.udp)` |
| Name lookup (DNS) | —(implicit inside connect) | `ToSocketAddrs` | `LookupHost` (F) | `getaddrinfo` | `dns.lookup` | — | `:inet.gethostbyname` | `Unix.getaddrinfo` | `InetAddress.getAllByName` | — |
| IP address type | — | `IpAddr`(`"…".parse()`) | `ParseIP` (F)・`netip.ParseAddr` | `ipaddress.ip_address` | `net.isIP` | — | `:inet.parse_address` | `Unix.inet_addr_of_string` | `InetAddress` | `IPv4Address` |
| Unix sockets / shared memory | `unix_*`・`shm_*` | `os::unix::net` | `net`(`UnixConn`) | `AF_UNIX`・`shared_memory` | `net`(path) | — | `{:local, path}` | `Unix` | JVM 16 onward | — |

<a id="完全性の規則"></a>

### Completeness rules

- `net` contains only what `wasi:sockets` can carry (R-L1-26): TCP, UDP, name resolution and IP addresses. Unix sockets and shared memory move to `unix` ([L2-unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md)).
- Socket families have the same shape for TCP and UDP: {open, own port `<proto>_local_port`}. Reading, writing and closing use `io.read`, `io.write` and `io.close`, regardless of handle kind (R-L0-44). Only connectionless UDP has `udp_send_to`/`udp_recv_from`, which specify the peer.
- Failure representation (R-L0-06 items 1 and 4; no sentinels): reading returns empty `Bytes` when the peer closes (the current `tcp_read` contract). Failed name lookup or an unreadable IP string produces err (with a reason).
- The `set_` in `tcp_set_timeout` is not a verb for changing an Almide value. It is an effect fn changing an OS-side socket setting, with no `mut` argument and no change to the handle value. R-L0-04's “`set` is the copy-side verb” rule concerns values (records and containers), so there is no conflict.
- Function-name prefixes (`tcp_`, `udp_`) distinguish two protocols in one module. Existing spellings are unchanged (R-L0-00).

<a id="l2-の判定"></a>

### L2 verdict

- Misplaced: 13 `unix_*`/`shm_*` functions (not in WASI; within `net`, “not yet on wasm” is mixed with “never on stock”) → R-net-01.
- Missing roles: UDP, DNS and an IP address type (7/9) → R-net-02, R-net-03.
- Declaration gap: `tcp_is_open` reads the handle table as a pure `fn` (measured callable from a pure fn; probe-io) → R-net-04.
- Types: `TcpStream`/`TcpListener` are aliases of `Int`; mixing up streams and listeners is not stopped by types, and they cannot conform to I/O protocols → R-net-05.
- I/O layer: reading, writing and closing have separate names for TCP, UDP, Unix sockets and standard I/O → R-net-07 (R-L0-44).
- R-L0-09: `net.unix_poll` is an exception explicitly named by L0 → R-net-01 (remove it during the move, consolidating on `unix.select`).

<a id="案"></a>

## Proposals

<a id="r-net-01-unix_shm_-を-unix-モジュールへ移す---採用"></a>

### R-net-01 Move `unix_*`/`shm_*` to the `unix` module — ○ Accepted

- Content: move 13 functions to `unix.*` and remove `net.unix_*`/`net.shm_*`. New spellings and the type (`unix.Fd`) are determined by R-unix-01 in [L2-unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md). Do not move `net.unix_poll`; consolidate on `unix.select([fd], t)`.
- Supporting L0 rules: R-L0-02 (Unix IPC is not the subject of `net`), R-L0-09 (`unix_poll`), R-L0-15, R-L1-26.
- (b) Move the existing implementation unchanged. On wasm, carry it with the adjudicated temporary embedded host ops 104–116 (retired in ADR-0023 step 7). WASI has neither Unix sockets nor shared memory, so if a WASI interface is still absent at retirement, remove `unix` from the standard library and make it a package (R-L0-15's third path). Native-only is not the endpoint. See R-unix-01 in [L2-unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md).
- (c) These 13 functions do not appear in CHEATSHEET or llms.txt. Removing them from beside `net.tcp_*` avoids confusion in ordinary networking code.
- (d) Move. `@deprecated(since=15, use="unix.…")`, `removal_epoch = 16`. The type change `Int` → `unix.Fd` occurs only under the new names; old names retain their meaning (R-L0-14 item 4). Usage counts (spec/tests/docs/stdlib): for example, `unix_send` 11/3/5/1, `unix_close` 17/8/5/1, `shm_read` 14/4/5/1. Totals for all 13 functions are in the L2-unix table.
- Proposed gate: no public `net` function has a `native-only-forever` row in `target-availability.toml`.
- Issue: #3853 (existing)

<a id="r-net-02-udp-を足す---採用"></a>

### R-net-02 Add UDP — ○ Accepted

- Content:
  ```almide
  type UdpSocket                                                    // 不透明（R-net-05）
  effect fn udp_bind(host: String, port: Int) -> UdpSocket          // port 0 で OS が選ぶ
  effect fn udp_local_port(sock: UdpSocket) -> Int
  effect fn udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit
  effect fn udp_recv_from(sock: UdpSocket, max: Int) -> (Bytes, String, Int)   // (データ, 相手の IP, 相手のポート)
  effect fn udp_close(sock: UdpSocket) -> Unit                      // 2 回目も ok
  ```
- Preserved code comments: UdpSocket is opaque (R-net-05); port 0 lets the OS choose; the received tuple is (data, peer IP, peer port); a second close also succeeds.
- Spelling: `bind`, `send_to` and `recv_from` match Rust(F), Python and OCaml. `local_port`/`close` follow the `tcp_` family.
- Supporting L0 rules: L1 R-L1-06, R-L0-15, R-L0-03 (socket first).
- (a) Present in 7/9.
- (b) The destination is `wasi:sockets/udp` (name resolution: `wasi:sockets/ip-name-lookup`), carried by a p3 component. From 2026-11-01, the p3 component shim answers ops 140..=145 using the `udp-socket` and `ip-name-lookup` of `wasi:sockets@0.3` (`crates/almide-wasm-run/src/wasi_p3_net.rs`). The shim parses IP strings by the same rules as Rust's `IpAddr`, passing only names to the resolver. Until then, the embedded host answers using native rt-core (`net_core`) directly through **temporary** host ops 140..=145, following the TCP ruling (2026-10-12) (#4306). Like TCP's 91..=103, these ops are removed when the embedded host runs p3 components in ADR-0023 step 7. Stock p1 has no sockets, so `almide build --target wasm` builds programs whose op set includes the TCP/UDP family (`almide_wasm::host_ops::SOCKET_FAMILIES`) as p3 components by default, as with `http.serve`, and displays the required permission (`-S inherit-network`). They were therefore removed from target-availability's stock p1 rows (2026-11-01, pending_port 23→9). Inspection uses `spec/embedded_cross/net_udp_loopback.almd` and `net_udp_errors.almd` (C-442 / C-443).
- (c) Without this, an LLM cannot write a “send via UDP” task.
- (d) Addition. Usage count 0.
- Proposed gate: a matrix test checks that TCP and UDP families (open, port, close) share the same shape. Native/wasm cross fixture.
- Issue: #3930

<a id="r-net-03-名前の解決と-ip-アドレスの型を足す---採用"></a>

### R-net-03 Add name resolution and an IP address type — ○ Accepted

- Content:
  ```almide
  type IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])   // V6 は 16 ビットの値 8 つ
  effect fn lookup_host(host: String) -> List[IpAddr]   // 見つからなければ err
  fn parse_ip(s: String) -> Result[IpAddr, String]
  fn ip_to_string(ip: IpAddr) -> String                  // V6 は RFC 5952 の短い形
  fn is_loopback(ip: IpAddr) -> Bool
  ```
- Preserved code comments: V6 contains eight 16-bit values; lookup returns err if not found; V6 string output uses RFC 5952's short form.
- Spelling: `lookup_host` follows Go(F); `parse_ip` follows Go's `ParseIP`(F) and the “construct from a string = parse” convention of Almide's `int.parse`/`url.parse`.
- Ordering: `IpAddr` is not declared ordered, so no `compare` is provided (R-L0-16 applies only to ordered types). Equality uses `==`.
- Supporting L0 rules: R-L1-06, R-L0-06 item 4 (unreadable strings yield err, not sentinels), R-L0-09 (`is_loopback`).
- (a) Present in 7/9.
- (b) `lookup_host` uses `resolve-addresses` in `wasi:sockets/ip-name-lookup`. The host determines result order, so inspection uses ADR-0027 D3's verdict (`localhost` includes a loopback address). `parse_ip`, `ip_to_string` and `is_loopback` are pure.
- (c) Currently, lookup happens only implicitly inside `tcp_connect`; there is no way to inspect addresses.
- (d) Addition. Usage count 0.
- Proposed gate: round-trip property tests for `parse_ip` and `ip_to_string`.
- Issue: #3929

<a id="r-net-04-tcp_is_open-を-effect-fn-にする---採用"></a>

### R-net-04 Make `tcp_is_open` an effect fn — ○ Accepted

- Content: turn `fn tcp_is_open(stream: Int) -> Bool` into an `effect fn`.
- Supporting L0 rules: R-L0-13, ADR-0022 D3.
- (b) Checker only. (c) It is impossible to know, without reading declarations, that this alone among functions on the same handle is pure.
- (d) Semantic change (pure → effect). No name change, so no `@deprecated`. Calls from pure fns become E006; the hint says to make the calling function an `effect fn`. Old code stops at type checking instead of silently producing a different result (R-L0-14 item 4). Epoch 15 `breaks`. Usage count 10/0/5/1.
- Issue: same new issue as R-process-04 (pure fns reading host state)
- Issue: #3907

<a id="r-net-05-tcp-のハンドルを不透明な型にする---採用十三巡目に見直しr-l0-44"></a>

### R-net-05 Make TCP handles opaque types — ○ Accepted (reconsidered in round thirteen; R-L0-44)

- Proposal: make `type TcpStream`, `type TcpListener` and `type UdpSocket` opaque (not `Int` aliases). `tcp_connect`/`tcp_accept` return `TcpStream`, `tcp_listen` returns `TcpListener`, and other functions accept them.
- Reason for reconsideration: concrete types are expressed as combinations of protocols (R-L0-37). As `Int`, handles cannot conform to `Read`/`Write`/`Close` (R-L0-44). Making `Int` conform to `Read` would let ordinary numbers be passed to `io.read`. This also removes the inconsistency that UDP has a type while only TCP is `Int` within the same `net` module. `unix.Fd` is already opaque (R-unix-01).
- (a) Typed handles are the majority: Rust `TcpStream`/`TcpListener`, Go `net.Conn`/`net.Listener`, Python `socket`, Kotlin `Socket`/`ServerSocket`, Swift (Network) `NWConnection`/`NWListener`.
- (d) Semantic change (type change). Old code only produces type errors where handles were stored in `Int` fields or lists; it does not silently produce different values (R-L0-14 item 4). `almide fix` cannot rewrite `Int` type annotations, so a hint advises using `net.TcpStream`.
- Previous verdict (× Rejected):

- Reason: all correctly working user code, such as records with handles in `Int` fields, would need rewriting; no reports of stream/listener confusion have yet been received. Although compatibility rules (R-L0-14 item 4) are not violated (`TcpStream`/`TcpListener` are `Int` aliases, so opacity merely makes old code a type error), the benefit does not justify the rewrite volume. There are currently no reports of stream/listener confusion. The “mixing TCP handles with OS descriptors” issue raised by #3853 is prevented by making `unix.Fd` opaque in R-net-01.
- Issue: none

<a id="r-net-06-udp_recv_from-の-3-つ組をレコードにする---採用"></a>

### R-net-06 Replace the `udp_recv_from` triple with a record — ○ Accepted

- Current: `net.udp_recv_from(sock, max) -> (Bytes, String, Int)`. It makes users memorize the positions of two fields that are not of the same type.
- Proposal: return `type UdpPacket = { data: Bytes, host: String, port: Int }`. `host` is a `String` so it can be passed directly to `udp_send_to(host: String, …)` and TCP functions.
- Supporting L0 rule: R-L0-01. Multiple return values use tuples only for groups of the same kind (the two lists of `list.partition`); three or more differently purposed fields use records (like `ProcessStatus`/`FileStat`).
- (a) Ordering varies: Python `(bytes, (host, port))`, Rust `(usize, SocketAddr)`, Erlang `{Address, Port, Packet}`.
- (d) Cost is 0 because this is a new function.
- Issue: #3974

<a id="r-net-07-tcpudp-の読み書きと閉じるを-io-の総称関数にする---採用"></a>

### R-net-07 Use generic `io` functions for TCP/UDP reading, writing and closing — ○ Accepted

- Content: move `net.tcp_read` → `io.read`, `tcp_write` → `io.write`, `tcp_read_exact` → `io.read_exact`, `tcp_close`/`tcp_close_listener` → `io.close`, removing five `net` functions. Do not introduce R-net-02's `udp_close`; use `io.close`.
- Retain: `tcp_read_timeout`, `tcp_set_timeout`, `tcp_available`, `tcp_is_open` (TCP-specific operations); `tcp_connect`, `tcp_listen`, `tcp_accept`, `tcp_local_port`, `udp_*` (opening/inspection).
- Supporting L0 rules: R-L0-44, R-L0-01 (one spelling per role).
- (d) Remove 5. With `@deprecated(since=15, use="io.read")` and similar annotations, `almide fix` rewrites one-to-one (`net.tcp_read(s, n)` → `io.read(s, n)`). Do not reuse old names (R-L0-43).
- Issue: #4015

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `net.tcp_connect` | `effect fn tcp_connect(host: String, port: Int) -> Int` | Rust `TcpStream::connect` / Go `Dial` / Python `create_connection` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_read` | `effect fn tcp_read(stream: Int, len: Int) -> Bytes` | Rust `read` / Python `recv` | Remove (→ `io.read`) | R-net-07 |
| `net.tcp_write` | `effect fn tcp_write(stream: Int, data: Bytes) -> Unit` | Rust `write_all` / Python `sendall` | Remove (→ `io.write`) | R-net-07 |
| `net.tcp_read_exact` | `effect fn tcp_read_exact(stream: Int, len: Int) -> Bytes` | Rust `read_exact` / Go `io.ReadFull` | Remove (→ `io.read_exact`) | R-net-07 |
| `net.tcp_close` | `effect fn tcp_close(stream: Int) -> Unit` | Go `Close` / Python `close` | Remove (→ `io.close`) | R-net-07 |
| `net.tcp_is_open` | `fn tcp_is_open(stream: Int) -> Bool` | Kotlin `isClosed` negated | Semantic change (make effect fn and use `stream: TcpStream`) | R-net-04、R-net-05 |
| `net.tcp_read_timeout` | `effect fn tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes` | Elixir `recv(…, timeout)` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_set_timeout` | `effect fn tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit` | Rust `set_read_timeout` / Python `settimeout` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_available` | `effect fn tcp_available(stream: Int) -> Int` | Java `available` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_listen` | `effect fn tcp_listen(host: String, port: Int) -> Int` | Rust `TcpListener::bind` / Go `Listen` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_local_port` | `effect fn tcp_local_port(listener: Int) -> Int` | Rust `local_addr` / Go `Addr` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_accept` | `effect fn tcp_accept(listener: Int) -> Int` | Rust・Go・Python `accept` | Semantic change (handles become `TcpStream`・`TcpListener`) | R-net-05 |
| `net.tcp_close_listener` | `effect fn tcp_close_listener(listener: Int) -> Unit` | Go `Listener.Close` | Remove (→ `io.close`) | R-net-07 |
| `net.udp_bind` | `effect fn udp_bind(host: String, port: Int) -> UdpSocket`(proposed) | Rust `UdpSocket::bind`(F) / Go `ListenUDP`(F) | Add | R-net-02 |
| `net.udp_local_port` | `effect fn udp_local_port(sock: UdpSocket) -> Int`(proposed) | Rust `local_addr`(F) | Add | R-net-02 |
| `net.udp_send_to` | `effect fn udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit`(proposed) | Rust `send_to`(F) / Go `WriteTo`(F) / Python `sendto` | Add | R-net-02 |
| `net.udp_recv_from` | `effect fn udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket`(proposed) | Rust `recv_from`(F) / Go `ReadFrom`(F) / Python `recvfrom` | Add (`UdpPacket` returned) | R-net-01、R-net-06 |
| `net.lookup_host` | `effect fn lookup_host(host: String) -> List[IpAddr]`(proposed) | Go `LookupHost`(F) / Python `getaddrinfo` / Node `dns.lookup` | Add | R-net-03 |
| `net.parse_ip` | `fn parse_ip(s: String) -> Result[IpAddr, String]`(proposed) | Go `ParseIP`(F) / Rust `parse::<IpAddr>` / Elixir `parse_address` | Add | R-net-03 |
| `net.ip_to_string` | `fn ip_to_string(ip: IpAddr) -> String`(proposed) | Rust `to_string` / Go `String` | Add | R-net-03 |
| `net.is_loopback` | `fn is_loopback(ip: IpAddr) -> Bool`(proposed) | Rust `is_loopback` / Go `IsLoopback` / Python `is_loopback` | Add | R-net-03 |
| `net.unix_connect` | `effect fn unix_connect(path: String) -> Int` | Rust `UnixStream::connect` | Remove (→ `unix.connect`) | R-net-01 |
| `net.unix_listen` | `effect fn unix_listen(path: String) -> Int` | Rust `UnixListener::bind` | Remove (→ `unix.listen`) | R-net-01 |
| `net.unix_accept` | `effect fn unix_accept(listener: Int) -> Int` | Rust `accept` | Remove (→ `unix.accept`) | R-net-01 |
| `net.unix_send` | `effect fn unix_send(sock: Int, data: Bytes, fds: List[Int]) -> Unit` | Python `send_fds` | Remove (→ `unix.write_fds`) | R-net-01 |
| `net.unix_recv` | `effect fn unix_recv(sock: Int, max: Int) -> Bytes` | Python `recv_fds` | Remove (→ `io.read`) | R-net-01 |
| `net.unix_take_fds` | `effect fn unix_take_fds(sock: Int) -> List[Int]` | — | Remove (→ `unix.take_fds`) | R-net-01 |
| `net.unix_poll` | `effect fn unix_poll(fd: Int, timeout_ms: Int) -> Bool` | Python `select.poll` | Remove (→ `unix.select([fd], t)`) | R-net-01 |
| `net.unix_wait` | `effect fn unix_wait(fds: List[Int], timeout_ms: Int) -> List[Int]` | Python `select.select` / Go —  | Remove (→ `unix.select`) | R-net-01 |
| `net.unix_close` | `effect fn unix_close(fd: Int) -> Unit` | Python `os.close` | Remove (→ `io.close`) | R-net-01 |
| `net.shm_create` | `effect fn shm_create(size: Int) -> Int` | Python `SharedMemory(create=True)` | Remove (→ `unix.shm_create`) | R-net-01 |
| `net.shm_resize` | `effect fn shm_resize(fd: Int, size: Int) -> Unit` | POSIX `ftruncate` | Remove (→ `unix.shm_resize`) | R-net-01 |
| `net.shm_write` | `effect fn shm_write(fd: Int, offset: Int, data: Bytes) -> Unit` | POSIX `pwrite` | Remove (→ `unix.shm_write`) | R-net-01 |
| `net.shm_read` | `effect fn shm_read(fd: Int, offset: Int, len: Int) -> Bytes` | POSIX `pread` | Remove (→ `unix.shm_read`) | R-net-01 |
