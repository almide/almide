<!-- description: Stdlib redesign L2/L3 — net: only what wasi:sockets can carry; the unix_*/shm_* family moves to unix (#3853), UDP, DNS lookup and an IP address type join (R-L1-06), tcp_is_open becomes an effect -->
# L2/L3: net

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-06: UDP・DNS・IP を足す、R-L1-26: Unix ソケットと共有メモリを `unix` へ）
> 出典: Rust https://doc.rust-lang.org/std/net/ ・ Go https://pkg.go.dev/net ・ Python https://docs.python.org/3/library/socket.html ・ TS/JS https://nodejs.org/api/net.html ・ https://nodejs.org/api/dgram.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （無い）・ Elixir https://www.erlang.org/doc/apps/kernel/gen_udp.html ・ OCaml https://ocaml.org/manual/latest/api/Unix.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/ （JVM の java.net）・ Swift https://developer.apple.com/documentation/network
> WASI: https://github.com/WebAssembly/wasi-sockets （`tcp`、`udp`、`ip-name-lookup`）。(F) = 今回取り寄せて確かめたセル。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| TCP 接続・読み書き・閉じる | `tcp_connect`・`tcp_read`・`tcp_write`・`tcp_read_exact`・`tcp_close` | `TcpStream::connect`・`read`・`write_all`・`read_exact` | `Dial`・`Read`・`Write`・`Close` | `create_connection`・`recv`・`sendall` | `net.connect`・`socket.write` | — | `:gen_tcp.connect`・`recv`・`send` | `Unix.connect`・`read`・`write` | `Socket` | `NWConnection` |
| TCP の時間切れ・残量 | `tcp_read_timeout`・`tcp_set_timeout`・`tcp_available` | `set_read_timeout` | `SetDeadline` | `settimeout` | `setTimeout` | — | `recv(…, timeout)` | `setsockopt_float` | `soTimeout` | — |
| TCP で待ち受け | `tcp_listen`・`tcp_accept`・`tcp_local_port`・`tcp_close_listener` | `TcpListener::bind`・`accept`・`local_addr` | `Listen`・`Accept`・`Addr` | `bind`・`listen`・`accept` | `net.createServer` | — | `:gen_tcp.listen`・`accept` | `Unix.listen`・`accept` | `ServerSocket` | `NWListener` |
| 開いているか | `tcp_is_open` | — | — | `fileno() != -1` | `socket.destroyed` | — | — | — | `isClosed` | — |
| UDP | — | `UdpSocket::bind`・`send_to`・`recv_from`・`local_addr` (F) | `ListenUDP`・`WriteTo`・`ReadFrom` (F) | `SOCK_DGRAM`・`sendto`・`recvfrom` | `dgram.createSocket`・`send`・`on('message')` | — | `:gen_udp.open`・`send`・`recv` | `Unix.sendto`・`recvfrom` | `DatagramSocket` | `NWConnection(.udp)` |
| 名前を引く（DNS） | —（connect の中で暗黙） | `ToSocketAddrs` | `LookupHost` (F) | `getaddrinfo` | `dns.lookup` | — | `:inet.gethostbyname` | `Unix.getaddrinfo` | `InetAddress.getAllByName` | — |
| IP アドレスの型 | — | `IpAddr`（`"…".parse()`） | `ParseIP` (F)・`netip.ParseAddr` | `ipaddress.ip_address` | `net.isIP` | — | `:inet.parse_address` | `Unix.inet_addr_of_string` | `InetAddress` | `IPv4Address` |
| Unix ソケット・共有メモリ | `unix_*`・`shm_*` | `os::unix::net` | `net`（`UnixConn`） | `AF_UNIX`・`shared_memory` | `net`（path） | — | `{:local, path}` | `Unix` | JVM 16〜 | — |

### 完全性の規則

- `net` は `wasi:sockets` が運べるものだけを持ちます（R-L1-26）。TCP、UDP、名前の解決、IP アドレス。Unix ソケットと共有メモリは `unix` に移します（[L2-unix](L2-unix.md)）。
- ソケットの族は、TCP と UDP で同じ形にそろえます: {開く, 自分のポート `<proto>_local_port`, 閉じる `<proto>_close`}。読み書きは、つながる TCP は `read`/`write`、つながらない UDP は相手を指定する `send_to`/`recv_from` です。
- 失敗の表し方（R-L0-06 の 1 と 4。番兵を返さない）: 読み取りは相手が閉じたら空の `Bytes`（いまの `tcp_read` の約束）。名前を引けないとき・IP の文字列が読めないときは err（理由がある）。
- `tcp_set_timeout` の `set_` は、Almide の値を書き換える動詞ではありません。OS の側にあるソケットの設定を変える effect fn で、`mut` 引数も持たず、ハンドルの値は変わりません。R-L0-04 の「`set` はコピー側の動詞」は値（レコード・入れ物）についての規則なので、これとはぶつかりません。
- 関数名の接頭辞（`tcp_`、`udp_`）は、1 つのモジュールに 2 つのプロトコルを置くための区別です。いまの綴りを変えません（R-L0-00）。

### L2 の判定

- 置き場所違い: `unix_*`・`shm_*` の 13 関数（WASI に無いもので、`net` の中で「まだ wasm に無い」と「永久に stock には無い」が混ざる）→ R-net-01
- 抜けている役割: UDP、DNS、IP アドレスの型（7/9）→ R-net-02、R-net-03
- 宣言の穴: `tcp_is_open` が純粋な `fn` でハンドルの表を読みます（実測で純粋な fn から呼べる。probe-io）→ R-net-04
- 型: `TcpStream`・`TcpListener` が `Int` の別名で、ストリームと待ち受けを取り違えても型で止まりません → R-net-05（× と理由）
- R-L0-09: `net.unix_poll` は L0 が名指しした例外 → R-net-01（移動の際に `unix.wait` に一本化して消す）

## 案

### R-net-01 `unix_*`・`shm_*` を `unix` モジュールへ移す — ○ 採用

- 内容: 13 関数を `unix.*` に移し、`net.unix_*`・`net.shm_*` を撤去します。新しい綴りと型（`unix.Fd`）は [L2-unix](L2-unix.md) の R-unix-01 で決めます。`net.unix_poll` は移さず、`unix.wait([fd], t)` に一本化します。
- 根拠の L0 規則: R-L0-02（Unix のプロセス間通信は `net` の主語ではない）、R-L0-09（`unix_poll`）、R-L0-15、R-L1-26。
- (b) いまの実体のまま移します。wasm では、裁定済みの暫定の組み込み host op 104〜116 で運びます（ADR-0023 step 7 で退役）。WASI には Unix ソケットも共有メモリもないので、退役の時に WASI のインタフェースがまだ無ければ、`unix` は標準から外して package にします（R-L0-15 の 3 つめの道）。native だけを終点にはしません。詳しくは [L2-unix](L2-unix.md) の R-unix-01。
- (c) CHEATSHEET と llms.txt にこの 13 関数は出ていません。`net.tcp_*` の隣に並ばなくなれば、ふつうのネットワークのコードで迷いません。
- (d) 移動。`@deprecated(since=15, use="unix.…")`、`removal_epoch = 16`。`Int` → `unix.Fd` の型の変更は新しい名前の側だけで起き、古い名前の意味は変えません（R-L0-14 の 4）。使用数（spec/tests/docs/stdlib）: 例 `unix_send` 11/3/5/1、`unix_close` 17/8/5/1、`shm_read` 14/4/5/1。13 関数の合計は L2-unix の表。
- ゲート案: `net` の公開関数が、`target-availability.toml` で `native-only-forever` の行を 1 つも持たないこと。
- issue: #3853（既存）

### R-net-02 UDP を足す — ○ 採用

- 内容:
  ```almide
  type UdpSocket = Int
  effect fn udp_bind(host: String, port: Int) -> UdpSocket          // port 0 で OS が選ぶ
  effect fn udp_local_port(sock: UdpSocket) -> Int
  effect fn udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit
  effect fn udp_recv_from(sock: UdpSocket, max: Int) -> (Bytes, String, Int)   // (データ, 相手の IP, 相手のポート)
  effect fn udp_close(sock: UdpSocket) -> Unit                      // 2 回目も ok
  ```
- 綴り: `bind`・`send_to`・`recv_from` は Rust(F)・Python・OCaml と同じです。`local_port`・`close` は `tcp_` 族に合わせます。
- 根拠の L0 規則: L1 R-L1-06、R-L0-15、R-L0-03（ソケットが先頭）。
- (a) 7/9 が持っています。
- (b) `wasi:sockets/udp` で wasm に乗ります。ADR-0023 の経路（p3 shim）に乗せ、新しい host op は足しません。組み込み host では ADR-0023 step 7（`wasmtime_wasi::p3` を組み込む）の後に動きます。それまでの行は `pending-port`。ADR-0027 D5 の loopback fixture に UDP の echo を足して検品します。
- (c) 「UDP で送る」課題で、無ければ LLM は書けません。
- (d) 追加。使用数 0。
- ゲート案: TCP と UDP の族（開く・ポート・閉じる）が同じ形であることを確かめる行列テスト。native と wasm の cross fixture。
- issue: #3930

### R-net-03 名前の解決と IP アドレスの型を足す — ○ 採用

- 内容:
  ```almide
  type IpAddr = V4(Int, Int, Int, Int) | V6(List[Int])   // V6 は 16 ビットの値 8 つ
  effect fn lookup_host(host: String) -> List[IpAddr]   // 見つからなければ err
  fn parse_ip(s: String) -> Result[IpAddr, String]
  fn ip_to_string(ip: IpAddr) -> String                  // V6 は RFC 5952 の短い形
  fn is_loopback(ip: IpAddr) -> Bool
  ```
- 綴り: `lookup_host` は Go(F)、`parse_ip` は Go の `ParseIP`(F) と Almide の `int.parse`・`url.parse` の「文字列から作る＝parse」に合わせます。
- 順序: `IpAddr` は順序のある型として宣言しないので、`compare` は置きません（R-L0-16 は順序のある型だけが対象）。等しいかどうかは `==`。
- 根拠の L0 規則: R-L1-06、R-L0-06 の 4（読めない文字列は err で、番兵を返さない）、R-L0-09（`is_loopback`）。
- (a) 7/9 が持っています。
- (b) `lookup_host` は `wasi:sockets/ip-name-lookup` の `resolve-addresses`。結果の並び順は host が決めるので、ADR-0027 D3 の verdict（`localhost` が loopback を含む）で検品します。`parse_ip`・`ip_to_string`・`is_loopback` は純粋です。
- (c) いまは `tcp_connect` の中で暗黙に引くだけで、アドレスを調べる手段がありません。
- (d) 追加。使用数 0。
- ゲート案: `parse_ip` と `ip_to_string` の往復の性質テスト。
- issue: #3929

### R-net-04 `tcp_is_open` を effect fn にする — ○ 採用

- 内容: `fn tcp_is_open(stream: Int) -> Bool` を `effect fn` にします。
- 根拠の L0 規則: R-L0-13、ADR-0022 D3。
- (b) 検査器だけ。(c) 同じハンドルの関数の中でこれだけ純粋なのは、宣言を読まないと分かりません。
- (d) 意味変更（純粋 → effect）。名前は変えないので `@deprecated` は付けません。純粋な fn から呼んでいる箇所は E006 になり、ヒントで「呼び出す関数を `effect fn` にする」と示します。旧い書き方は型の検査で止まり、黙って違う結果にはなりません（R-L0-14 の 4）。epoch 15 の `breaks`。使用数 10/0/5/1。
- issue: R-process-04 と同じ新規 issue（host の状態を読む純粋な fn）
- issue: #3907

### R-net-05 TCP のハンドルを不透明な型にする — × 不採用

- 理由: `TcpStream`・`TcpListener` は `Int` の別名で、取り違えは型で止まりません。不透明な型にする変更は、旧い書き方を型エラーにするだけで黙って結果を変えないので、R-L0-14 の 4 では許されます。それでも、ハンドルを `Int` の欄に持つレコードなど、正しく動いている利用者のコードをすべて書き換えさせることになります。ストリームと待ち受けを取り違えたという報告は、いまはありません。#3853 が問題にした「TCP のハンドルと OS の記述子を混ぜる」は、R-net-01 で `unix.Fd` を不透明にすれば止まります。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `net.tcp_connect` | `effect fn tcp_connect(host: String, port: Int) -> Int` | Rust `TcpStream::connect` / Go `Dial` / Python `create_connection` | 据え置き | — |
| `net.tcp_read` | `effect fn tcp_read(stream: Int, len: Int) -> Bytes` | Rust `read` / Python `recv` | 据え置き | — |
| `net.tcp_write` | `effect fn tcp_write(stream: Int, data: Bytes) -> Unit` | Rust `write_all` / Python `sendall` | 据え置き | — |
| `net.tcp_read_exact` | `effect fn tcp_read_exact(stream: Int, len: Int) -> Bytes` | Rust `read_exact` / Go `io.ReadFull` | 据え置き | — |
| `net.tcp_close` | `effect fn tcp_close(stream: Int) -> Unit` | Go `Close` / Python `close` | 据え置き | — |
| `net.tcp_is_open` | `fn tcp_is_open(stream: Int) -> Bool` | Kotlin `isClosed` の否定 | 意味変更（effect fn） | R-net-04 |
| `net.tcp_read_timeout` | `effect fn tcp_read_timeout(stream: Int, len: Int, timeout_ms: Int) -> Bytes` | Elixir `recv(…, timeout)` | 据え置き | — |
| `net.tcp_set_timeout` | `effect fn tcp_set_timeout(stream: Int, timeout_ms: Int) -> Unit` | Rust `set_read_timeout` / Python `settimeout` | 据え置き | — |
| `net.tcp_available` | `effect fn tcp_available(stream: Int) -> Int` | Java `available` | 据え置き | — |
| `net.tcp_listen` | `effect fn tcp_listen(host: String, port: Int) -> Int` | Rust `TcpListener::bind` / Go `Listen` | 据え置き | — |
| `net.tcp_local_port` | `effect fn tcp_local_port(listener: Int) -> Int` | Rust `local_addr` / Go `Addr` | 据え置き | — |
| `net.tcp_accept` | `effect fn tcp_accept(listener: Int) -> Int` | Rust・Go・Python `accept` | 据え置き | — |
| `net.tcp_close_listener` | `effect fn tcp_close_listener(listener: Int) -> Unit` | Go `Listener.Close` | 据え置き | — |
| `net.udp_bind` | `effect fn udp_bind(host: String, port: Int) -> UdpSocket`（案） | Rust `UdpSocket::bind`(F) / Go `ListenUDP`(F) | 追加 | R-net-02 |
| `net.udp_local_port` | `effect fn udp_local_port(sock: UdpSocket) -> Int`（案） | Rust `local_addr`(F) | 追加 | R-net-02 |
| `net.udp_send_to` | `effect fn udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit`（案） | Rust `send_to`(F) / Go `WriteTo`(F) / Python `sendto` | 追加 | R-net-02 |
| `net.udp_recv_from` | `effect fn udp_recv_from(sock: UdpSocket, max: Int) -> (Bytes, String, Int)`（案） | Rust `recv_from`(F) / Go `ReadFrom`(F) / Python `recvfrom` | 追加 | R-net-02 |
| `net.udp_close` | `effect fn udp_close(sock: UdpSocket) -> Unit`（案） | Go `Close` | 追加 | R-net-02 |
| `net.lookup_host` | `effect fn lookup_host(host: String) -> List[IpAddr]`（案） | Go `LookupHost`(F) / Python `getaddrinfo` / Node `dns.lookup` | 追加 | R-net-03 |
| `net.parse_ip` | `fn parse_ip(s: String) -> Result[IpAddr, String]`（案） | Go `ParseIP`(F) / Rust `parse::<IpAddr>` / Elixir `parse_address` | 追加 | R-net-03 |
| `net.ip_to_string` | `fn ip_to_string(ip: IpAddr) -> String`（案） | Rust `to_string` / Go `String` | 追加 | R-net-03 |
| `net.is_loopback` | `fn is_loopback(ip: IpAddr) -> Bool`（案） | Rust `is_loopback` / Go `IsLoopback` / Python `is_loopback` | 追加 | R-net-03 |
| `net.unix_connect` | `effect fn unix_connect(path: String) -> Int` | Rust `UnixStream::connect` | 撤去（→ `unix.connect`） | R-net-01 |
| `net.unix_listen` | `effect fn unix_listen(path: String) -> Int` | Rust `UnixListener::bind` | 撤去（→ `unix.listen`） | R-net-01 |
| `net.unix_accept` | `effect fn unix_accept(listener: Int) -> Int` | Rust `accept` | 撤去（→ `unix.accept`） | R-net-01 |
| `net.unix_send` | `effect fn unix_send(sock: Int, data: Bytes, fds: List[Int]) -> Unit` | Python `send_fds` | 撤去（→ `unix.send`） | R-net-01 |
| `net.unix_recv` | `effect fn unix_recv(sock: Int, max: Int) -> Bytes` | Python `recv_fds` | 撤去（→ `unix.recv`） | R-net-01 |
| `net.unix_take_fds` | `effect fn unix_take_fds(sock: Int) -> List[Int]` | — | 撤去（→ `unix.take_fds`） | R-net-01 |
| `net.unix_poll` | `effect fn unix_poll(fd: Int, timeout_ms: Int) -> Bool` | Python `select.poll` | 撤去（→ `unix.wait([fd], t)`） | R-net-01 |
| `net.unix_wait` | `effect fn unix_wait(fds: List[Int], timeout_ms: Int) -> List[Int]` | Python `select.select` / Go —  | 撤去（→ `unix.wait`） | R-net-01 |
| `net.unix_close` | `effect fn unix_close(fd: Int) -> Unit` | Python `os.close` | 撤去（→ `unix.close`） | R-net-01 |
| `net.shm_create` | `effect fn shm_create(size: Int) -> Int` | Python `SharedMemory(create=True)` | 撤去（→ `unix.shm_create`） | R-net-01 |
| `net.shm_resize` | `effect fn shm_resize(fd: Int, size: Int) -> Unit` | POSIX `ftruncate` | 撤去（→ `unix.shm_resize`） | R-net-01 |
| `net.shm_write` | `effect fn shm_write(fd: Int, offset: Int, data: Bytes) -> Unit` | POSIX `pwrite` | 撤去（→ `unix.shm_write`） | R-net-01 |
| `net.shm_read` | `effect fn shm_read(fd: Int, offset: Int, len: Int) -> Bytes` | POSIX `pread` | 撤去（→ `unix.shm_read`） | R-net-01 |
