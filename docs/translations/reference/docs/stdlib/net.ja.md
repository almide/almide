# net

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](net.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](net.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](net.zh-CN.md)



> 原文：[docs/stdlib/net.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/net.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文を保持し、説明訳をコード外へ添える。生成索引の固定コピーであり、この訳は生成対象ではない。

TCP、UDP、ホスト名検索と IP アドレス。`wasi:sockets` が扱うもの。`import net`、`effect fn`。Unix ドメインソケットと共有メモリファイルは [unix](../../../../stdlib/unix.md) にある。

**ハンドル。** `TcpStream`、`TcpListener`、`UdpSocket` は不透明なハンドル型。このモジュールだけが生成できるので、裸の `Int` や種類の異なるハンドルは呼出しに届かない。読取り・書込み・クローズは [io](../../../../stdlib/io.md) を通し、全ハンドルが同じ関数を使う。

| ハンドル | 生成元 | 使用する関数 |
|---|---|---|
| `net.TcpStream` | `tcp_connect`, `tcp_accept` | `io.read`, `io.read_exact`, `io.read_to_end`, `io.write`, `io.close` |
| `net.TcpListener` | `tcp_listen` | `tcp_accept`, `tcp_local_port`, `io.close` |
| `net.UdpSocket` | `udp_bind` | `udp_send_to`, `udp_recv_from`, `udp_local_port`, `io.close` |

したがってソケットでバイトを送受信するプログラムは `net` と `io` の両方をインポートする。ソケットに触れる呼出しはすべて `effect fn` で `Result` を返すので、`!` を書くか `err` を処理する（ADR-0008）。`tcp_is_open` は通常の `Bool` を返すが、`effect fn` なので呼出しも `Result` になる：`net.tcp_is_open(s)!` と書く。`parse_ip`、`ip_to_string`、`is_loopback` は通常の `fn` で、テキスト上の計算でありケイパビリティ不要。

**ターゲット。** ネイティブは全関数に対応する。`almide run --target wasm`（組込みホスト）も同じ。`almide build --target wasm` は TCP、UDP、`lookup_host` 関数を呼ぶプログラムを、`wasi:sockets@0.3` を使う WASI 0.3 コンポーネントとしてビルドし、必要な権限を表示する。WASI preview 1 にはソケットがないためフラグは不要。ネットワークを許可して実行する（`wasmtime run -S inherit-network -S allow-ip-name-lookup app.wasm`）。そうしなければ各 bind と connect は `<call>(<host>:<port>): permission denied` となる。ソケット呼出しがないプログラムは従来通り preview 1 モジュールになる。`parse_ip`、`ip_to_string`、`is_loopback` は純粋で、どこでも実行できる。

<a id="tcp-client"></a>

## TCP クライアント

### `net.tcp_connect(host: String, port: Int) -> Result[TcpStream, String]`

`host:port` に接続する。拒否された場合は `err`。`io.write` と `io.read` でストリームを読み書きし、`io.close` で閉じる。

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

`io.read(s, n)` は最大 `n` バイトを返す。短い読取りは正常で、空の結果は相手側が閉じたことを示す。`io.read_exact(s, n)` は厳密に `n` バイトを読むか、到着したバイト数を示して失敗する。長さプレフィックス付きプロトコル向けの呼出し。

### `net.tcp_is_open(stream: TcpStream) -> Bool`

`io.close` でストリームを閉じた後は `false`。相手側のクローズはここでは分からず、`io.read` が空になることで現れる。

<a id="timeouts-and-readiness"></a>

## タイムアウトと読取り可能状態

### `net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Result[Bytes, String]`

最大 `len` バイト。`timeout_ms`（`> 0` が必須）経過で断念する。その後はストリーム本来のタイムアウトが再び適用される。

### `net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Result[Unit, String]`

このストリームの以後の読取り・書込みのタイムアウトを設定する。`<= 0` なら解除。

### `net.tcp_available(stream: TcpStream) -> Result[Int, String]`

現在ブロックせずに読めるバイト数。上限 65536。なければ `0`。

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

<a id="tcp-server"></a>

## TCP サーバー

### `net.tcp_listen(host: String, port: Int) -> Result[TcpListener, String]`

`host:port` にバインドして待受ける。ポート使用中なら `err`。ポート `0` なら OS が選ぶ。

### `net.tcp_local_port(listener: TcpListener) -> Result[Int, String]`

リスナーがバインドされたポート。ポート `0` で待受けた場合は OS が選んだもの。

### `net.tcp_accept(listener: TcpListener) -> Result[TcpStream, String]`

クライアント接続までブロックし、そのストリームを返す。待受け終了は `io.close(listener)`。

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

全ターゲットが対応する（ターゲット節参照）。

### `net.udp_bind(host: String, port: Int) -> Result[UdpSocket, String]`

`host:port` にバインドした UDP ソケット。ポート `0` なら OS が選ぶ。

### `net.udp_local_port(sock: UdpSocket) -> Result[Int, String]`

`sock` がバインドされているポート。

### `net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Result[Unit, String]`

`data` を 1 つのデータグラムとして `host:port` に送る。

### `net.udp_recv_from(sock: UdpSocket, max: Int) -> Result[UdpPacket, String]`

次のデータグラムを待ってブロックする。最大 `max` バイトと送信元を `UdpPacket { data: Bytes, host: String, port: Int }` として返す。

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

<a id="names-and-addresses"></a>

## 名前とアドレス

アドレスは `type IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])` の値。4 オクテット、または 8 個の 16 ビットグループ。

### `net.lookup_host(host: String) -> Result[List[IpAddr], String]`

`host` が解決されるアドレスを重複なく返す。1 つも解決できなければ `err`。全ターゲットが対応する（ターゲット節参照）。

```almd check
import net

effect fn main() -> Unit = {
  let addrs = net.lookup_host("localhost")!
  println(addrs |> list.map((a) => net.ip_to_string(a)) |> list.join(", "))
}
```

### `net.parse_ip(s: String) -> Result[IpAddr, String]`

`s` が表すアドレス。ドット区切り IPv4（`192.168.0.1`、先行ゼロなし）、または `::` と IPv4 末尾を許す IPv6（`::ffff:10.0.0.1`）。それ以外は `err`（`net.parse_ip: not an IP address: "..."`）。

### `net.ip_to_string(ip: IpAddr) -> String`

`ip` のテキスト表現。ドット区切り IPv4、または RFC 5952 の短縮 IPv6 形式。小文字の十六進数、先行ゼロなし、2 個以上のゼログループが最長で連続する箇所（同長なら最初）を `::` と書く。

### `net.is_loopback(ip: IpAddr) -> Bool`

`127.0.0.0/8` と `::1` に対して `true`。

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



<a id="signature-index-16-functions"></a>

## シグネチャ索引（16 関数）

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

<a id="type-index-2-types"></a>

## 型索引（2 型）

```
// One received datagram and where it came from.
// @since unreleased
type net.UdpPacket = { data: Bytes, host: String, port: Int }

// An IP address: four octets, or eight 16-bit groups.
// @since unreleased
type net.IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])
```



## 索引コメントの説明訳

- `effect net.tcp_connect(host: String, port: Int) -> TcpStream` — host:port に接続したストリーム。拒否なら err。 (@since 0.20.0 or earlier)
- `effect net.tcp_is_open(stream: TcpStream) -> Bool` — io.close 後 false。相手のクローズは見えない。 (@since 0.20.0 or earlier)
- `effect net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes` — timeout_ms（> 0）以内に最大 len バイト。その後は元のタイムアウトに戻る。 (@since 0.20.0 or earlier)
- `effect net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit` — 読書きタイムアウトを設定。<= 0 で解除。 (@since 0.20.0 or earlier)
- `effect net.tcp_available(stream: TcpStream) -> Int` — 現在読めるバイト数、最大 65536。なければ 0。 (@since 0.20.0 or earlier)
- `effect net.tcp_listen(host: String, port: Int) -> TcpListener` — host:port にバインドしたリスナー。使用中なら err。0 は OS が選ぶ。 (@since 0.20.0 or earlier)
- `effect net.tcp_local_port(listener: TcpListener) -> Int` — リスナーのバインド先ポート。0 で待受けた場合は OS が選んだ値。 (@since unreleased)
- `effect net.tcp_accept(listener: TcpListener) -> TcpStream` — 次のクライアントを待ってブロックし、そのストリームを返す。 (@since 0.20.0 or earlier)
- `effect net.udp_bind(host: String, port: Int) -> UdpSocket` — host:port にバインドした UDP ソケット。0 は OS が選ぶ。 (@since unreleased)
- `effect net.udp_local_port(sock: UdpSocket) -> Int` — sock のバインド先ポート。 (@since unreleased)
- `effect net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` — data を 1 データグラムとして host:port に送る。 (@since unreleased)
- `effect net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` — 次のデータグラムを待ち、最大 max バイトと送信元を返す。 (@since unreleased)
- `effect net.lookup_host(host: String) -> List[IpAddr]` — host の解決先アドレスを各 1 回。なければ err。 (@since unreleased)
- `net.parse_ip(s: String) -> Result[IpAddr, String]` — s が表す IP。ドット区切り IPv4 または IPv6（`::` と IPv4 末尾可）。 (@since unreleased)
- `net.ip_to_string(ip: IpAddr) -> String` — ip のテキスト。ドット区切り IPv4 または RFC 5952 短縮 IPv6。 (@since unreleased)
- `net.is_loopback(ip: IpAddr) -> Bool` — 127.0.0.0/8 と ::1 なら true。 (@since unreleased)
- `type net.UdpPacket = { data: Bytes, host: String, port: Int }` — 受信した 1 データグラムと送信元。 (@since unreleased)
- `type net.IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])` — IP アドレス。4 オクテットまたは 8 個の 16 ビットグループ。 (@since unreleased)
