<!-- description: Low-level TCP/UDP networking and DNS lookup -->
# stdlib: net (TCP/UDP) [Tier 3]

低レベルネットワーキング。Go, Python, Rust に標準で存在。

## 他言語比較

| 操作 | Go (`net`) | Python (`socket`) | Rust (`std::net`) | Deno |
|------|-----------|-------------------|--------------------|------|
| TCP 接続 | `net.Dial("tcp", addr)` | `socket.create_connection(addr)` | `TcpStream::connect(addr)` | `Deno.connect({hostname, port})` |
| TCP リッスン | `net.Listen("tcp", addr)` | `socket.bind(); socket.listen()` | `TcpListener::bind(addr)` | `Deno.listen({hostname, port})` |
| TCP 受理 | `listener.Accept()` | `socket.accept()` | `listener.accept()` | `listener.accept()` |
| UDP | `net.ListenPacket("udp", addr)` | `socket.socket(AF_INET, SOCK_DGRAM)` | `UdpSocket::bind(addr)` | `Deno.listenDatagram({port})` |
| DNS lookup | `net.LookupHost(host)` | `socket.getaddrinfo(host, port)` | `ToSocketAddrs` trait | `Deno.resolveDns(name, type)` |
| 読み書き | `conn.Read(buf)`, `conn.Write(buf)` | `sock.recv(n)`, `sock.send(data)` | `stream.read()`, `stream.write()` | `conn.read(buf)`, `conn.write(buf)` |
| タイムアウト | `conn.SetDeadline(t)` | `sock.settimeout(s)` | `stream.set_read_timeout(d)` | `AbortSignal.timeout(ms)` |
| 閉じる | `conn.Close()` | `sock.close()` | `drop(stream)` | `conn.close()` |

## 追加候補 (~10 関数)

### P0 (TCP クライアント)
- `net.connect(host, port) -> Result[Connection, String]`
- `net.read(conn, n) -> Result[List[Int], String]`
- `net.write(conn, data) -> Result[Unit, String]`
- `net.close(conn)`

### P1 (TCP サーバー)
- `net.listen(host, port) -> Result[Listener, String]`
- `net.accept(listener) -> Result[Connection, String]`

### P2 (UDP)
- `net.udp_bind(host, port) -> Result[UdpSocket, String]`
- `net.udp_send(sock, data, addr) -> Result[Unit, String]`
- `net.udp_recv(sock, n) -> Result[(List[Int], String), String]`

### P2 (DNS)
- `net.resolve(hostname) -> Result[List[String], String]`

## 実装戦略

@extern。Rust: `std::net`。TS: `Deno.connect` / Node `net`。async/await (Phase D) が前提だが、同期ブロッキング版を先行実装可能。
