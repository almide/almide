<!-- description: Stdlib redesign L2/L3 — unix (new, #3853): the Unix-socket and shared-memory IPC floor leaves net under an explicit import, with an opaque Fd type and one readiness wait -->
# L2/L3: unix（新しいモジュール）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-26: Unix ソケットと共有メモリを `unix` に移す、#3853）
> 出典: Rust https://doc.rust-lang.org/std/os/unix/net/ ・ Go https://pkg.go.dev/net#UnixConn ・ Python https://docs.python.org/3/library/socket.html#socket.send_fds ・ https://docs.python.org/3/library/multiprocessing.shared_memory.html ・ TS/JS https://nodejs.org/api/net.html#ipc-support ・ Gleam —（無い）・ Elixir https://www.erlang.org/doc/apps/kernel/gen_tcp.html （`{:local, path}`）・ OCaml https://ocaml.org/manual/latest/api/Unix.html ・ Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/UnixDomainSocketAddress.html ・ Swift —（標準に無い）
> 実体: `crates/almide-rt-core/src/net_unix_core.rs`、`docs/roadmap/active/net-unix-shm-embedded.md`（#3620、暫定 op 104〜116）

## L2: 役割の比較

| 役割 | Almide（いま → 案） | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| つなぐ・待ち受ける・受け入れる | `net.unix_connect`・`unix_listen`・`unix_accept` → `unix.connect`・`listen`・`accept` | `UnixStream::connect`・`UnixListener::bind`・`accept` | `DialUnix`・`ListenUnix`・`AcceptUnix` | `socket(AF_UNIX)`・`bind`・`accept` | `net.connect({path})` | — | `:gen_tcp.connect({:local, p})` | `Unix.connect`（`ADDR_UNIX`） | `SocketChannel.open(UnixDomainSocketAddress)` | — |
| 送る（記述子つき） | `net.unix_send` → `unix.send` | `send_vectored_with_ancillary`（unstable） | `WriteMsgUnix` | `send_fds` | — | — | — | — | — | — |
| 受け取る・届いた記述子を取る | `net.unix_recv`・`unix_take_fds` → `unix.recv`・`take_fds` | `recv_vectored_with_ancillary`（unstable） | `ReadMsgUnix` | `recv_fds` | — | — | — | — | — | — |
| 読めるまで待つ | `net.unix_poll`・`unix_wait` → `unix.wait` | —（`mio` は外部） | — | `select.select`・`poll` | — | — | — | `Unix.select` | `Selector` | — |
| 閉じる | `net.unix_close` → `unix.close` | drop | `Close` | `os.close` | `destroy` | — | `close` | `Unix.close` | `close` | — |
| 共有メモリ | `net.shm_*` → `unix.shm_create`・`shm_resize`・`shm_write`・`shm_read` | — | — | `SharedMemory` | — | — | — | `Unix.map_file` | — | — |

### 完全性の規則

- 役割は #2912 が置いた「Wayland・PipeWire・D-Bus の記述子渡しの床」に閉じます: {つなぐ, 待ち受ける, 受け入れる, 記述子つきで送る, 受け取る, 記述子を取る, 待つ, 閉じる} と、共有メモリ {作る, 大きさを変える, 書く, 読む}。
- 記述子は不透明な型 `unix.Fd` にします。`net` の TCP のハンドル（`Int`）と混ぜられません（#3853 の 2）。
- 待つ関数は `wait` の 1 つだけです。1 つの記述子を待つのは `unix.wait([fd], t)`。Bool を返す `unix_poll` は R-L0-09 に反するうえ、`wait` と同じ役割なので移しません。
- `shm_` は「このモジュールの中の共有メモリの族」を表す接頭辞です。`net` の `tcp_`・`udp_` と同じ使い方です。
- `import unix` が要ります（自動では import しません）。
- 位置を指定した書き込み `shm_write` は、`offset` が負か、`offset + bytes.len(data)` が大きさを超えると abort します（L0 の「範囲外の書き込みは abort」）。大きさを広げたいときは先に `shm_resize` を呼びます。読み取り `shm_read` は、R-L0-12 のとおり末尾で短くなります。

### L2 の判定

- 新しいモジュールです。抜けている役割はありません。Rust・Go のほうが少ない（記述子渡しは Rust で unstable、共有メモリは無い）。

## 案

### R-unix-01 `unix` モジュールを作り、`net.unix_*`・`net.shm_*` を移す — ○ 採用

- 内容:
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
- 根拠の L0 規則: R-L0-02（主語は記述子で、ネットワークではない）、R-L0-09（`unix_poll` を残さない）、R-L0-14 の 4（型を `Fd` にするのは新しい名前の側だけ）、R-L0-15。
- (a) Unix ソケットを TCP と同じモジュールに置くのは Go だけで、Rust は `std::os::unix::net` に分けています。共有メモリを標準に持つのは Python だけです。
- (b) native: 同じ `net_unix_core.rs`。wasm: WASI（p1・0.2・0.3）には Unix ソケットも共有メモリもありません。組み込み host は裁定済みの暫定 op 104〜116 で運び（ADR-0023 step 7 で退役）、structural leg は `stdlib/net_wasm.almd` の本体を使います。stock の wasm host（暫定 op を持たない host）の行は、TCP の暫定 op と同じ「暫定の組み込み host op」の扱いで記録します。native だけの行にはしません。ADR-0027 D5 の Unix echo の fixture で、native と組み込み host の両方を検品します（D8: native だけの印は付けない）。暫定 op が ADR-0023 step 7 で退役する時の道は、R-L0-15 で決まっています。その時点で Unix ソケットと共有メモリを運ぶ WASI のインタフェースがあれば、それに乗せ替えます。無ければ `unix` を標準から外し、package（`almide/unix`）にします。
- (c) `net.tcp_*` の隣にこの 13 関数が並ばなくなります。TCP のハンドルを `shm_read` に渡す誤りが型エラーになります。
- (d) 追加（新しいモジュール）と、`net` の側の撤去（L2-net R-net-01）。`net.unix_*`・`net.shm_*` に `@deprecated(since=15, use="unix.…")`、`removal_epoch = 16`。`Int` → `Fd` は型が変わるので、`almide fix` は名前だけを書き換え（`net.unix_send` → `unix.send` と `import unix` の追加）、型の不一致は検査器が示します。`shm_write` の範囲外が abort になるのも新しい名前の側だけの変化です。使用数（spec/tests/docs/stdlib、旧 `net.` の名前で）: connect 6/3/6/1、listen 4/1/5/1、accept 6/2/5/1、send 11/3/5/1、recv 10/2/5/1、take_fds 7/2/5/1、poll 9/3/6/1、wait 5/2/4/1、close 17/8/5/1、shm_create 6/4/5/1、shm_resize 5/1/4/1、shm_write 7/4/5/1、shm_read 14/4/5/1。
- 移すもの（#3853 の一覧）: `proofs/target-availability.toml` の 13 行、semantics manifest の `file-descriptor` の単位、検品の台帳、`docs/stdlib/net.md` の該当節。
- ゲート案: `unix.Fd` が `Int` と単一化しないことの checker テスト。`net` に `native-only-forever` の行が残らないこと（L2-net）。
- issue: #3853（既存）

## L3: 関数表

| 関数 | シグネチャ(案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `unix.connect` | `effect fn connect(path: String) -> Fd` | Rust `UnixStream::connect` / Go `DialUnix` | 追加（`net.unix_connect` から移動） | R-unix-01 |
| `unix.listen` | `effect fn listen(path: String) -> Fd` | Rust `UnixListener::bind` / Go `ListenUnix` | 追加（移動） | R-unix-01 |
| `unix.accept` | `effect fn accept(listener: Fd) -> Fd` | Rust・Python `accept` / Go `AcceptUnix` | 追加（移動） | R-unix-01 |
| `unix.send` | `effect fn send(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` | Python `send_fds` / Go `WriteMsgUnix` | 追加（移動） | R-unix-01 |
| `unix.recv` | `effect fn recv(sock: Fd, max: Int) -> Bytes` | Python `recv_fds` / Go `ReadMsgUnix` | 追加（移動） | R-unix-01 |
| `unix.take_fds` | `effect fn take_fds(sock: Fd) -> List[Fd]` | — | 追加（移動） | R-unix-01 |
| `unix.wait` | `effect fn wait(fds: List[Fd], timeout_ms: Int) -> List[Fd]` | Python `select.select` / OCaml `Unix.select` | 追加（移動。`unix_poll` を吸収） | R-unix-01 |
| `unix.close` | `effect fn close(fd: Fd) -> Unit` | Python `os.close` / OCaml `Unix.close` | 追加（移動） | R-unix-01 |
| `unix.shm_create` | `effect fn shm_create(size: Int) -> Fd` | Python `SharedMemory(create=True, size=)` | 追加（移動） | R-unix-01 |
| `unix.shm_resize` | `effect fn shm_resize(fd: Fd, size: Int) -> Unit` | POSIX `ftruncate` | 追加（移動） | R-unix-01 |
| `unix.shm_write` | `effect fn shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` | POSIX `pwrite` | 追加（移動） | R-unix-01 |
| `unix.shm_read` | `effect fn shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` | POSIX `pread` | 追加（移動） | R-unix-01 |
