# net

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](net.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](net.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](net.zh-CN.md)



> 原文：[docs/stdlib/net.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/net.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名和代码内注释保留原文，在代码外附说明译文。这是生成索引的固定副本，本译文不是生成目标。

TCP、UDP、主机名查找和 IP 地址——即 `wasi:sockets` 承载的内容。`import net`、`effect fn`。Unix 域套接字和共享内存文件在 [unix](../../../../stdlib/unix.md) 中。

**句柄。** `TcpStream`、`TcpListener` 和 `UdpSocket` 是不透明句柄类型：只有本模块能创建，因此普通 `Int` 或类型错误的句柄不会传入调用。读、写和关闭通过 [io](../../../../stdlib/io.md)，所有句柄使用同一组函数：

| 句柄 | 创建方式 | 配合使用 |
|---|---|---|
| `net.TcpStream` | `tcp_connect`, `tcp_accept` | `io.read`, `io.read_exact`, `io.read_to_end`, `io.write`, `io.close` |
| `net.TcpListener` | `tcp_listen` | `tcp_accept`, `tcp_local_port`, `io.close` |
| `net.UdpSocket` | `udp_bind` | `udp_send_to`, `udp_recv_from`, `udp_local_port`, `io.close` |

因此通过套接字传递字节的程序同时导入 `net` 和 `io`。所有接触套接字的调用都是 `effect fn` 并返回 `Result`：写上 `!` 或处理 `err`（ADR-0008）。`tcp_is_open` 返回普通 `Bool`，但它是 `effect fn`，调用也产生 `Result`：写作 `net.tcp_is_open(s)!`。`parse_ip`、`ip_to_string` 和 `is_loopback` 是普通 `fn`，仅对文本计算，不需要能力授权。

**目标。** 原生目标支持所有函数，`almide run --target wasm`（嵌入式宿主）也支持全部函数。`almide build --target wasm` 将调用 TCP、UDP 或 `lookup_host` 函数的程序构建为使用 `wasi:sockets@0.3` 的 WASI 0.3 组件，并打印所需授权；无需标志，因为 WASI preview 1 没有套接字。运行时须授予网络权限（`wasmtime run -S inherit-network -S allow-ip-name-lookup app.wasm`），否则每次 bind 和 connect 都返回 `<call>(<host>:<port>): permission denied`。不调用套接字的程序仍构建为原来的 preview 1 模块。`parse_ip`、`ip_to_string` 和 `is_loopback` 是纯函数，可在所有目标运行。

## TCP 客户端

### `net.tcp_connect(host: String, port: Int) -> Result[TcpStream, String]`

连接 `host:port`；被拒绝时为 `err`。用 `io.write` 和 `io.read` 读写流，用 `io.close` 关闭。

```almd check
import io
import net

effect fn fetch(host: String) -> Result[Bytes, String] = {
  let s = net.tcp_connect(host, 80)!
  io.write(s, bytes.from_string("GET / HTTP/1.0\r\nHost: ${host}\r\n\r\n"))!
  let reply = io.read_to_end(s)!
  io.close(s)!
  ok(reply)
}

effect fn main() -> Unit = {
  let reply = fetch("example.com")!
  println("${bytes.len(reply)} bytes")
}
```

`io.read(s, n)` 返回最多 `n` 字节：短读是正常情况，空结果表示对端已关闭。`io.read_exact(s, n)` 精确读取 `n` 字节，否则失败并指出已收到多少字节，适用于带长度前缀的协议。

### `net.tcp_is_open(stream: TcpStream) -> Bool`

通过 `io.close` 关闭流后为 `false`。此处不反映对端关闭；对端关闭表现为空的 `io.read`。

## 超时与就绪状态

### `net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Result[Bytes, String]`

最多 `len` 字节，经过 `timeout_ms`（必须 `> 0`）后放弃。之后恢复流原有的超时。

### `net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Result[Unit, String]`

设置此流后续读写的超时；`<= 0` 清除超时。

### `net.tcp_available(stream: TcpStream) -> Result[Int, String]`

当前可不阻塞读取的字节数，上限 65536；没有则为 `0`。

```almd check
import io
import net

effect fn main() -> Unit = {
  let s = net.tcp_connect("127.0.0.1", 6379)!
  net.tcp_set_timeout(s, 2000)!
  io.write(s, bytes.from_string("PING\r\n"))!
  let reply = net.tcp_read_timeout(s, 64, 500)!
  println(bytes.to_string_lossy(reply))
  io.close(s)!
}
```

## TCP 服务端

### `net.tcp_listen(host: String, port: Int) -> Result[TcpListener, String]`

绑定并监听 `host:port`；端口已使用时为 `err`。端口 `0` 让 OS 选择。

### `net.tcp_local_port(listener: TcpListener) -> Result[Int, String]`

监听器绑定的端口，即在端口 `0` 上监听时由 OS 选择的端口。

### `net.tcp_accept(listener: TcpListener) -> Result[TcpStream, String]`

阻塞直到客户端连接，返回其流。用 `io.close(listener)` 停止监听。

```almd check
import io
import net

effect fn main() -> Unit = {
  let l = net.tcp_listen("127.0.0.1", 0)!
  let port = net.tcp_local_port(l)!
  println("listening on ${port}")
  let s = net.tcp_accept(l)!
  io.write(s, bytes.from_string("hi\n"))!
  io.close(s)!
  io.close(l)!
}
```

## UDP

所有目标均支持（见目标说明）。

### `net.udp_bind(host: String, port: Int) -> Result[UdpSocket, String]`

绑定到 `host:port` 的 UDP 套接字；端口 `0` 让 OS 选择。

### `net.udp_local_port(sock: UdpSocket) -> Result[Int, String]`

`sock` 绑定的端口。

### `net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Result[Unit, String]`

将 `data` 作为一个数据报发送到 `host:port`。

### `net.udp_recv_from(sock: UdpSocket, max: Int) -> Result[UdpPacket, String]`

阻塞等待下一个数据报：返回其中最多 `max` 字节及发送方，形式为 `UdpPacket { data: Bytes, host: String, port: Int }`。

```almd check
import io
import net

effect fn main() -> Unit = {
  let a = net.udp_bind("127.0.0.1", 0)!
  let b = net.udp_bind("127.0.0.1", 0)!
  let port = net.udp_local_port(b)!
  net.udp_send_to(a, bytes.from_string("ping"), "127.0.0.1", port)!
  let p = net.udp_recv_from(b, 1500)!
  println("${bytes.to_string_lossy(p.data)} from ${p.host}:${p.port}")
  io.close(a)!
  io.close(b)!
}
```

## 名称与地址

地址是 `type IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])` 类型的值：四个八位组，或八个 16 位组。

### `net.lookup_host(host: String) -> Result[List[IpAddr], String]`

`host` 解析得到的地址，每个仅一次；无地址可解析时为 `err`。所有目标支持（见目标说明）。

```almd check
import net

effect fn main() -> Unit = {
  let addrs = net.lookup_host("localhost")!
  println(addrs |> list.map((a) => net.ip_to_string(a)) |> list.join(", "))
}
```

### `net.parse_ip(s: String) -> Result[IpAddr, String]`

`s` 表示的地址：点分 IPv4（`192.168.0.1`，不允许前导零），或允许 `::` 和 IPv4 尾部的 IPv6（`::ffff:10.0.0.1`）。其他均为 `err`（`net.parse_ip: not an IP address: "..."`）。

### `net.ip_to_string(ip: IpAddr) -> String`

`ip` 的文本形式：点分 IPv4，或 RFC 5952 的短 IPv6 形式——小写十六进制、无前导零，将连续两个及以上零组中最长的一段（并列时取第一段）写为 `::`。

### `net.is_loopback(ip: IpAddr) -> Bool`

对于 `127.0.0.0/8` 和 `::1` 返回 `true`。

```almd run
import net

effect fn main() -> Unit = {
  let ip = net.parse_ip("2001:0DB8:0000:0000:0000:0000:0000:0001")!
  println(net.ip_to_string(ip))
  println(if net.is_loopback(net.parse_ip("127.0.0.1")!) then "loopback" else "remote")
}
```
```output
2001:db8::1
loopback
```



## 签名索引（16 个函数）

```
// A stream connected to host:port; err if refused.
// @since 0.20.0 or earlier
effect net.tcp_connect(host: String, port: Int) -> TcpStream

// False after io.close; a peer close is not seen.
// @since 0.20.0 or earlier
effect net.tcp_is_open(stream: TcpStream) -> Bool

// Up to len bytes within timeout_ms (> 0); the stream's own timeout holds again after.
// @since 0.20.0 or earlier
effect net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes

// Sets read/write timeout; <= 0 clears it.
// @since 0.20.0 or earlier
effect net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit

// Bytes readable now, capped at 65536; 0 if none.
// @since 0.20.0 or earlier
effect net.tcp_available(stream: TcpStream) -> Int

// A listener bound to host:port; err if in use. Port 0 lets the OS choose.
// @since 0.20.0 or earlier
effect net.tcp_listen(host: String, port: Int) -> TcpListener

// The port a listener is bound to: the one the OS chose when it listened on port 0.
// @since unreleased
effect net.tcp_local_port(listener: TcpListener) -> Int

// Blocks for the next client; its stream.
// @since 0.20.0 or earlier
effect net.tcp_accept(listener: TcpListener) -> TcpStream

// A UDP socket bound to host:port; port 0 lets the OS choose.
// @since unreleased
effect net.udp_bind(host: String, port: Int) -> UdpSocket

// The port sock is bound to.
// @since unreleased
effect net.udp_local_port(sock: UdpSocket) -> Int

// Sends data as one datagram to host:port.
// @since unreleased
effect net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit

// Blocks for the next datagram: at most max bytes of it, and its sender.
// @since unreleased
effect net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket

// The addresses host resolves to, each once; err if none.
// @since unreleased
effect net.lookup_host(host: String) -> List[IpAddr]

// The IP address s spells: dotted IPv4, or IPv6 (`::` and an IPv4 tail allowed).
// @since unreleased
net.parse_ip(s: String) -> Result[IpAddr, String]

// ip as text: dotted IPv4, or the RFC 5952 short IPv6 form.
// @since unreleased
net.ip_to_string(ip: IpAddr) -> String

// True for 127.0.0.0/8 and ::1.
// @since unreleased
net.is_loopback(ip: IpAddr) -> Bool
```

## 类型索引（2 个类型）

```
// One received datagram and where it came from.
// @since unreleased
type net.UdpPacket = { data: Bytes, host: String, port: Int }

// An IP address: four octets, or eight 16-bit groups.
// @since unreleased
type net.IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])
```



## 索引注释的说明译文

- `effect net.tcp_connect(host: String, port: Int) -> TcpStream` — 连接到 host:port 的流；被拒绝则 err。 (@since 0.20.0 or earlier)
- `effect net.tcp_is_open(stream: TcpStream) -> Bool` — io.close 后为 false；不反映对端关闭。 (@since 0.20.0 or earlier)
- `effect net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes` — 在 timeout_ms（> 0）内最多 len 字节；随后恢复流原来的超时。 (@since 0.20.0 or earlier)
- `effect net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit` — 设置读写超时；<= 0 清除。 (@since 0.20.0 or earlier)
- `effect net.tcp_available(stream: TcpStream) -> Int` — 当前可读字节数，上限 65536；没有则 0。 (@since 0.20.0 or earlier)
- `effect net.tcp_listen(host: String, port: Int) -> TcpListener` — 绑定到 host:port 的监听器；已使用则 err，端口 0 由 OS 选择。 (@since 0.20.0 or earlier)
- `effect net.tcp_local_port(listener: TcpListener) -> Int` — 监听器绑定的端口，即在端口 0 监听时 OS 选择的值。 (@since unreleased)
- `effect net.tcp_accept(listener: TcpListener) -> TcpStream` — 阻塞等待下一个客户端，返回其流。 (@since 0.20.0 or earlier)
- `effect net.udp_bind(host: String, port: Int) -> UdpSocket` — 绑定到 host:port 的 UDP 套接字，端口 0 由 OS 选择。 (@since unreleased)
- `effect net.udp_local_port(sock: UdpSocket) -> Int` — sock 绑定的端口。 (@since unreleased)
- `effect net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` — 将 data 作为一个数据报发往 host:port。 (@since unreleased)
- `effect net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` — 阻塞等待下一个数据报，返回最多 max 字节及发送方。 (@since unreleased)
- `effect net.lookup_host(host: String) -> List[IpAddr]` — host 解析出的地址，每个一次；没有则 err。 (@since unreleased)
- `net.parse_ip(s: String) -> Result[IpAddr, String]` — s 表示的 IP 地址：点分 IPv4 或 IPv6（允许 `::` 和 IPv4 尾部）。 (@since unreleased)
- `net.ip_to_string(ip: IpAddr) -> String` — ip 的文本：点分 IPv4 或 RFC 5952 短 IPv6 形式。 (@since unreleased)
- `net.is_loopback(ip: IpAddr) -> Bool` — 127.0.0.0/8 和 ::1 时为 true。 (@since unreleased)
- `type net.UdpPacket = { data: Bytes, host: String, port: Int }` — 一个收到的数据报及其来源。 (@since unreleased)
- `type net.IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])` — IP 地址：四个八位组或八个 16 位组。 (@since unreleased)
