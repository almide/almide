# net

TCP, UDP, host-name lookup and IP addresses — what `wasi:sockets` carries.
`import net`, `effect fn`. Unix-domain sockets and shared-memory files are in
[unix](./unix.md).

**Handles.** `TcpStream`, `TcpListener` and `UdpSocket` are opaque handle
types: only this module makes one, so a bare `Int` or the wrong kind of handle
never reaches a call. Reading, writing and closing go through [io](./io.md),
the same functions every handle uses:

| Handle | Made by | Use with |
|---|---|---|
| `net.TcpStream` | `tcp_connect`, `tcp_accept` | `io.read`, `io.read_exact`, `io.read_to_end`, `io.write`, `io.close` |
| `net.TcpListener` | `tcp_listen` | `tcp_accept`, `tcp_local_port`, `io.close` |
| `net.UdpSocket` | `udp_bind` | `udp_send_to`, `udp_recv_from`, `udp_local_port`, `io.close` |

So a program that moves bytes over a socket imports both `net` and `io`.
Every call that touches a socket is an `effect fn` and returns a `Result` —
write `!` or handle the `err` (ADR-0008). `tcp_is_open` returns a plain `Bool`,
but as an `effect fn` its call yields a `Result` too: write
`net.tcp_is_open(s)!`. `parse_ip`, `ip_to_string` and `is_loopback` are plain
`fn`s: they compute over text and need no capability.

**Targets.** Native serves every function. `almide run --target wasm` (the
embedded host) serves the TCP family and the address functions; UDP and
`lookup_host` are native only for now. A stock artifact
(`almide build --target wasm`) refuses the socket functions at build time (the
p1 standard has no sockets). `parse_ip`, `ip_to_string` and `is_loopback` are
pure and run everywhere.

## TCP client

### `net.tcp_connect(host: String, port: Int) -> Result[TcpStream, String]`

Open a connection to `host:port`; an `err` if it is refused. Write and read
the stream with `io.write` and `io.read`, and close it with `io.close`.

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

`io.read(s, n)` returns UP TO `n` bytes — a short read is normal, and an empty
result means the peer closed. `io.read_exact(s, n)` reads exactly `n` bytes or
fails naming how many arrived: the call for a length-prefixed protocol.

### `net.tcp_is_open(stream: TcpStream) -> Bool`

`false` once the stream was closed with `io.close`. A close by the peer is not
seen here; it shows as an empty `io.read`.

## Timeouts and readiness

### `net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Result[Bytes, String]`

Up to `len` bytes, giving up after `timeout_ms` (which must be `> 0`). The
stream's own timeout holds again afterwards.

### `net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Result[Unit, String]`

Set the timeout for later reads and writes on this stream; `<= 0` clears it.

### `net.tcp_available(stream: TcpStream) -> Result[Int, String]`

Bytes readable now without blocking, capped at 65536; `0` if none.

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

## TCP server

### `net.tcp_listen(host: String, port: Int) -> Result[TcpListener, String]`

Bind and listen on `host:port`; an `err` if the port is in use. Port `0` lets
the OS choose one.

### `net.tcp_local_port(listener: TcpListener) -> Result[Int, String]`

The port the listener is bound to — the one the OS chose when it listened on
port `0`.

### `net.tcp_accept(listener: TcpListener) -> Result[TcpStream, String]`

Block until a client connects; its stream. Stop listening with
`io.close(listener)`.

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

Native only for now (see Targets).

### `net.udp_bind(host: String, port: Int) -> Result[UdpSocket, String]`

A UDP socket bound to `host:port`; port `0` lets the OS choose.

### `net.udp_local_port(sock: UdpSocket) -> Result[Int, String]`

The port `sock` is bound to.

### `net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Result[Unit, String]`

Send `data` as one datagram to `host:port`.

### `net.udp_recv_from(sock: UdpSocket, max: Int) -> Result[UdpPacket, String]`

Block for the next datagram: at most `max` bytes of it, and its sender, as
`UdpPacket { data: Bytes, host: String, port: Int }`.

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

## Names and addresses

An address is a value of `type IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])`:
four octets, or eight 16-bit groups.

### `net.lookup_host(host: String) -> Result[List[IpAddr], String]`

The addresses `host` resolves to, each once; an `err` if it resolves to none.
Native only for now (see Targets).

```almd check
import net

effect fn main() -> Unit = {
  let addrs = net.lookup_host("localhost")!
  println(addrs |> list.map((a) => net.ip_to_string(a)) |> list.join(", "))
}
```

### `net.parse_ip(s: String) -> Result[IpAddr, String]`

The address `s` spells: dotted IPv4 (`192.168.0.1`, no leading zeros), or IPv6
with `::` and an IPv4 tail allowed (`::ffff:10.0.0.1`). Anything else is an
`err` (`net.parse_ip: not an IP address: "..."`).

### `net.ip_to_string(ip: IpAddr) -> String`

`ip` as text: dotted IPv4, or the RFC 5952 short IPv6 form — lowercase hex, no
leading zeros, the longest run of two or more zero groups (the first when tied)
written `::`.

### `net.is_loopback(ip: IpAddr) -> Bool`

`true` for `127.0.0.0/8` and `::1`.

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (16 functions)

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

## Type index (2 types)

```
// One received datagram and where it came from.
// @since unreleased
type net.UdpPacket = { data: Bytes, host: String, port: Int }

// An IP address: four octets, or eight 16-bit groups.
// @since unreleased
type net.IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])
```

<!-- END GENERATED SIGNATURE INDEX -->
