<!-- description: Stdlib redesign L2/L3 — unix (new, #3853): the Unix-socket and shared-memory IPC floor leaves net under an explicit import, with an opaque Fd type and one readiness wait -->
<a id="l2l3-unix新しいモジュール"></a>

# L2/L3: unix (New Module)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-unix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-unix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-unix.zh-CN.md)

> Non-normative dated redesign snapshot. Original unchanged and authoritative; no automatic synchronization, new usage census or current implementation/host audit. Accepted proposals are retained in their sequence: initial wait/send/recv/close names lead to select and generic io plus write_fds; earlier tables and code are not silently rewritten. Interim embedded operations are not stock-WASI support; migration or removal to a package remains conditional. Cross-language completeness and zero migration-cost statements are source claims, not independently rechecked guarantees. Code/comments stay unchanged, with comment translations outside fences. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-26: move Unix sockets and shared memory to `unix`, #3853)
> Sources: Rust https://doc.rust-lang.org/std/os/unix/net/ ; Go https://pkg.go.dev/net#UnixConn ; Python https://docs.python.org/3/library/socket.html#socket.send_fds ; https://docs.python.org/3/library/multiprocessing.shared_memory.html ; TS/JS https://nodejs.org/api/net.html#ipc-support ; Gleam — none; Elixir https://www.erlang.org/doc/apps/kernel/gen_tcp.html (`{:local, path}`); OCaml https://ocaml.org/manual/latest/api/Unix.html ; Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/UnixDomainSocketAddress.html ; Swift — none in the standard library.
> Implementation: `crates/almide-rt-core/src/net_unix_core.rs`, `docs/roadmap/active/net-unix-shm-embedded.md` (#3620, interim ops 104–116).

<a id="l2-役割の比較"></a>

## L2: Role Comparison

| Role | Almide (then → proposal) | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Connect, listen, accept | `net.unix_connect`, `unix_listen`, `unix_accept` → `unix.connect`, `listen`, `accept` | `UnixStream::connect`, `UnixListener::bind`, `accept` | `DialUnix`, `ListenUnix`, `AcceptUnix` | `socket(AF_UNIX)`, `bind`, `accept` | `net.connect({path})` | — | `:gen_tcp.connect({:local, p})` | `Unix.connect` (`ADDR_UNIX`) | `SocketChannel.open(UnixDomainSocketAddress)` | — |
| Send with descriptors | `net.unix_send` → `unix.send` | `send_vectored_with_ancillary` (unstable) | `WriteMsgUnix` | `send_fds` | — | — | — | — | — | — |
| Receive and take delivered descriptors | `net.unix_recv`, `unix_take_fds` → `unix.recv`, `take_fds` | `recv_vectored_with_ancillary` (unstable) | `ReadMsgUnix` | `recv_fds` | — | — | — | — | — | — |
| Wait for readability | `net.unix_poll`, `unix_wait` → `unix.wait` | — (`mio` is external) | — | `select.select`, `poll` | — | — | — | `Unix.select` | `Selector` | — |
| Close | `net.unix_close` → `unix.close` | drop | `Close` | `os.close` | `destroy` | — | `close` | `Unix.close` | `close` | — |
| Shared memory | `net.shm_*` → `unix.shm_create`, `shm_resize`, `shm_write`, `shm_read` | — | — | `SharedMemory` | — | — | — | `Unix.map_file` | — | — |

<a id="完全性の規則"></a>

### Completeness Rules

- Roles are closed to #2912's descriptor-passing floor for Wayland, PipeWire and D-Bus: {connect, listen, accept, send with descriptors, receive, take descriptors, wait, close}, and shared memory {create, resize, write, read}.
- Descriptors use opaque `unix.Fd`. They cannot mix with `net` TCP handles (`Int`), #3853 item 2.
- There is one wait function, `wait`; waiting for one descriptor is `unix.wait([fd], t)`. The Bool-returning `unix_poll` violates R-L0-09 and duplicates `wait`, so it is not moved.
- The `shm_` prefix denotes this module's shared-memory family, like `net`'s `tcp_` and `udp_`.
- Requires `import unix`; no automatic import.
- Positional `shm_write` aborts when `offset` is negative or `offset + bytes.len(data)` exceeds size, following L0's out-of-bounds-write rule. Call `shm_resize` first to grow it. Reads via `shm_read` shorten at the end, per R-L0-12.

<a id="l2-の判定"></a>

### L2 Verdict

- A new module with no missing roles. Rust and Go have fewer: descriptor passing is unstable in Rust, and shared memory is absent.

<a id="案"></a>

## Proposals

<a id="r-unix-01-unix-モジュールを作りnetunix_netshm_-を移す---採用"></a>

### R-unix-01 Create `unix` and Move `net.unix_*` / `net.shm_*` — ○ Accepted

- Content:
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
  Code-comment translation, in declaration order: Fd is opaque and does not convert to Int; listen requires a nonexistent path; descriptors travel with the first byte; recv is empty when the peer closes; take_fds yields delivered, not-yet-taken descriptors oldest first; wait returns readable descriptors in order, with a negative timeout waiting until one is readable; shm_create makes an unnamed zero-filled file; shm_read may shorten at the end.
- L0 basis: R-L0-02 (the subject is descriptors, not networking), R-L0-09 (do not retain `unix_poll`), R-L0-14 item 4 (only the new names change to `Fd`), R-L0-15.
- (a) Only Go puts Unix sockets in the TCP module; Rust separates them into `std::os::unix::net`. Only Python has shared memory in its standard library.
- (b) Native uses the same `net_unix_core.rs`. Wasm: WASI p1, 0.2 and 0.3 have neither Unix sockets nor shared memory. The embedded host carries them with the approved interim ops 104–116, retired in ADR-0023 step 7; the structural leg uses `stdlib/net_wasm.almd` bodies. The stock-wasm-host row, without those ops, is recorded as “interim embedded host op,” just like interim TCP ops, not native-only. ADR-0027 D5's Unix echo fixture inspects native and embedded host, with D8 forbidding a native-only label. R-L0-15 determines the retirement route: if WASI interfaces for Unix sockets/shared memory exist then, move to them; otherwise remove `unix` from stdlib and make it package `almide/unix`.
- (c) These 13 functions no longer sit beside `net.tcp_*`. Passing a TCP handle to `shm_read` becomes a type error.
- (d) Addition of a module and withdrawal from `net`, L2-net R-net-01. Put `@deprecated(since=15, use="unix.…")` and `removal_epoch = 16` on `net.unix_*` / `net.shm_*`. Because `Int` → `Fd` changes the type, `almide fix` only renames, `net.unix_send` → `unix.send`, and adds `import unix`; the checker reports type mismatches. Out-of-bounds `shm_write` aborting also changes only the new spelling. Usage counts in spec/tests/docs/stdlib, under old `net.` names: connect 6/3/6/1, listen 4/1/5/1, accept 6/2/5/1, send 11/3/5/1, recv 10/2/5/1, take_fds 7/2/5/1, poll 9/3/6/1, wait 5/2/4/1, close 17/8/5/1, shm_create 6/4/5/1, shm_resize 5/1/4/1, shm_write 7/4/5/1, shm_read 14/4/5/1.
- Move, per #3853: 13 rows of `proofs/target-availability.toml`, semantics manifest's `file-descriptor` unit, inspection ledger, relevant sections of `docs/stdlib/net.md`.
- Proposed gate: checker test that `unix.Fd` does not unify with `Int`; no `native-only-forever` rows left in `net`, L2-net.
- Issue: #3853, existing.

<a id="r-unix-02-unixwait-を-unixselect-にする---採用"></a>

### R-unix-02 Rename `unix.wait` to `unix.select` — ○ Accepted

- Current state: `unix.wait(fds, timeout_ms) -> List[Fd]` waits until readable descriptors appear. `http.wait(c)` and `process.wait(c)` wait for one handle to finish and return its result.
- Proposal: `effect fn unix.select(fds: List[Fd], timeout_ms: Int) -> List[Fd]`. Reserve `wait` for waiting for a handle to finish.
- L0 basis: R-L0-00 item 1, same verb with different meanings across modules.
- (a) Python `select.select`, OCaml `Unix.select`, JVM `Selector.select`; no language calls this `wait`.
- (d) Zero cost because `unix` is new. Set `use` in the `@deprecated` annotation of `net.unix_wait` / `unix_poll` to `unix.select`.
- Issue: #3974.

<a id="r-unix-03-unixrecvunixsend-を-unixreadunixwrite-にする---採用"></a>

### R-unix-03 Rename `unix.recv` / `unix.send` to `unix.read` / `unix.write` — ○ Accepted

- Current state: connected-stream reads/writes are `net.tcp_read` / `tcp_write` for TCP but `unix.recv` / `send` for Unix sockets.
- Proposal: `effect fn unix.read(sock: Fd, max: Int) -> Bytes`, `effect fn unix.write(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit`.
- L0 basis: R-L0-01. Apply L2-net's rule to Unix sockets: connected TCP uses `read`/`write`, unconnected UDP uses `send_to`/`recv_from`.
- (a) Rust `UnixStream` implements `Read` and `Write`, with no `recv` or `send` (F). Go `UnixConn` and OCaml also use `read` and `write`.
- (d) Zero cost because the module is new.
- Completeness rule: connected streams have the same {connect, listen, accept, read, write, close} shape for TCP and Unix.
- Issue: #3974.

<a id="r-unix-04-読み書きと閉じるを-io-の総称関数にし記述子つきの書き込みを-write_fds-にする---採用"></a>

### R-unix-04 Use Generic `io` Functions for Reading/Writing/Closing; Name Descriptor Writes `write_fds` — ○ Accepted

- Content: make `unix.Fd` conform to `Read`, `Write`, `Close` (R-L0-44). Use `io.read` / `io.close` rather than adding `unix.read` / `unix.close`. Writing with descriptors is `effect fn unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit`; `io.write(sock, data)` carries no descriptors. Reception remains the combination of `io.read` and `unix.take_fds`.
- L0 basis: R-L0-44 and R-L0-01. R-unix-03's same shape for TCP/Unix now means exactly the same calling form, one `io` set.
- (a) Rust `UnixStream` implements `Read`/`Write`, with a separate descriptor function, `send_vectored_with_ancillary`. Python uses `send_fds` and Go `WriteMsgUnix`; all distinguish the name from descriptor-free reads/writes.
- (d) Zero cost because `unix` is new.
- Issue: #4015.

<a id="l3-関数表"></a>

## L3: Function Table

| Function | Signature (proposed) | Common spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `unix.connect` | `effect fn connect(path: String) -> Fd` | Rust `UnixStream::connect` / Go `DialUnix` | Add, moved from `net.unix_connect` | R-unix-01 |
| `unix.listen` | `effect fn listen(path: String) -> Fd` | Rust `UnixListener::bind` / Go `ListenUnix` | Add, moved | R-unix-01 |
| `unix.accept` | `effect fn accept(listener: Fd) -> Fd` | Rust/Python `accept` / Go `AcceptUnix` | Add, moved | R-unix-01 |
| `unix.write_fds` | `effect fn write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` | Python `send_fds` / Go `WriteMsgUnix` | Add, moved | R-unix-01, R-unix-04 |
| `unix.take_fds` | `effect fn take_fds(sock: Fd) -> List[Fd]` | — | Add, moved | R-unix-01 |
| `unix.select` | `effect fn select(fds: List[Fd], timeout_ms: Int) -> List[Fd]` | Python `select.select` / OCaml `Unix.select` | Add, moved and absorbs `unix_poll` | R-unix-01, R-unix-02 |
| `unix.shm_create` | `effect fn shm_create(size: Int) -> Fd` | Python `SharedMemory(create=True, size=)` | Add, moved | R-unix-01 |
| `unix.shm_resize` | `effect fn shm_resize(fd: Fd, size: Int) -> Unit` | POSIX `ftruncate` | Add, moved | R-unix-01 |
| `unix.shm_write` | `effect fn shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` | POSIX `pwrite` | Add, moved | R-unix-01 |
| `unix.shm_read` | `effect fn shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` | POSIX `pread` | Add, moved | R-unix-01 |
