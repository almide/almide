# unix

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](unix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](unix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](unix.zh-CN.md)



> 原文：[unix.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/unix.md)。这是面向读者的非规范性固定快照，不参与自动生成或自动同步。签名、类型、`@since` 及代码（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行或验证示例、性能、安全性或跨目标一致性。

> 平台支持和 WASI 说明以固定原文时点为准。保留正文 Result 写法与索引 effect 签名写法的差异，不作统一。不透明描述符及立即可见性的描述不是独立的隔离、同步或安全保证。

Unix 域套接字、它们传递的文件描述符，以及共享内存文件——Wayland、PipeWire 和 D-Bus 所依赖的描述符传递基础层。`import unix`、`effect fn`。

描述符是不透明的 `unix.Fd`，仅由本模块创建（`connect`、`listen`、`accept`、`take_fds`、`shm_create`），因此 TCP 句柄或裸 `Int` 不会传入这里的调用。`Fd` 支持 `Read`、`Write` 和 `Close`，所以其字节与其他句柄一样通过 [io](../../../../stdlib/io.md) 处理：

- 用 `io.read(fd, max)` 读取，阻塞到有字节到达；空结果表示对端已关闭。
- 用 `io.write(fd, data)` 写入，或用 `unix.write_fds` 将描述符随字节一起传递。
- 用 `io.close(fd)` 关闭，套接字、监听器、共享内存文件和收到的描述符均如此。关闭套接字也会关闭其已收到但尚未取走的描述符。

每次调用都返回 `Result`，请写 `!` 或处理 `err`（ADR-0008）。`take_fds` 返回普通列表，但它是 `effect fn`，所以调用也产生 `Result`：`unix.take_fds(sock)!`。

**目标。** 原生目标在 Unix 平台（Linux、macOS、各 BSD）提供所有函数，其他平台返回 `err`。`almide run --target wasm`（嵌入式宿主）通过临时宿主操作提供同样的代码（#3620；当 ADR-0023 步骤7将该通道作为 WASI 0.3 组件运行时退役）。原文指出 WASI 既没有 Unix 域套接字，也没有共享内存，因此标准产物（`almide build --target wasm`）在构建时拒绝此模块（E081）。

<a id="sockets"></a>

## 套接字

### `unix.connect(path: String) -> Result[Fd, String]`

连接到 `path` 处监听器的流式套接字。

### `unix.listen(path: String) -> Result[Fd, String]`

绑定在 `path` 的监听器，该路径必须尚不存在。

### `unix.accept(listener: Fd) -> Result[Fd, String]`

阻塞等待 `listener` 上的下一个连接，返回其套接字。

### `unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Result[Unit, String]`

发送全部 `data`，让描述符 `fds`（最多28个）随第一个字节一起传递（SCM_RIGHTS）。对端收到的是指向同一批已打开文件的新描述符。

### `unix.take_fds(sock: Fd) -> List[Fd]`

在 `sock` 上收到但尚未取走的描述符，最早的在前。当 `io.read` 收到承载它们的字节时，描述符按套接字排队。

### `unix.select(xs: List[Fd], timeout_ms: Int) -> Result[List[Fd], String]`

返回 `xs` 中在 `timeout_ms` 内可读（数据、连接或对端关闭）的描述符，保持传入顺序。一次等待可处理多个来源，例如 Wayland 套接字和合成器的事件套接字。负超时等待至至少一个可读；`0` 只检查不等待，适合不阻塞地推进事件循环。没有任何描述符变为可读时返回空。

```almd check
import io
import unix

effect fn main() -> Unit = {
  let l = unix.listen("/tmp/demo.sock")!
  let a = unix.connect("/tmp/demo.sock")!
  let b = unix.accept(l)!
  io.write(a, bytes.from_string("hello"))!
  let ready = unix.select([b], 1000)!
  println("${list.len(ready)} ready")
  let got = io.read(b, 64)!
  println(bytes.to_string_lossy(got))
  io.close(a)!
  io.close(b)!
  io.close(l)!
}
```

<a id="shared-memory-files"></a>

## 共享内存文件

填入内容并交给对端映射的无名文件，例如 Wayland 缓冲区的像素、音频环。写入会立即对持有该文件的每个进程可见，无论是否已映射。

### `unix.shm_create(size: Int) -> Result[Fd, String]`

新建一个 `size` 字节、填零的无名文件。

### `unix.shm_resize(fd: Fd, size: Int) -> Result[Unit, String]`

将文件扩大或缩小到 `size` 字节。

### `unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Result[Unit, String]`

在文件字节 `offset` 处写入 `data`。负偏移或越过文件末尾的写入会中止程序，请先 `shm_resize`。

### `unix.shm_read(fd: Fd, offset: Int, len: Int) -> Result[Bytes, String]`

从文件的 `offset` 读取最多 `len` 字节，到末尾时可能更少。

```almd check
import io
import unix

effect fn main() -> Unit = {
  let l = unix.listen("/tmp/demo.sock")!
  let a = unix.connect("/tmp/demo.sock")!
  let b = unix.accept(l)!
  let file = unix.shm_create(4096)!
  unix.shm_write(file, 0, bytes.from_string("pixels"))!
  unix.write_fds(a, bytes.from_string("take this"), [file])!
  let _ = io.read(b, 64)!
  let got = unix.take_fds(b)!
  match list.get(got, 0) {
    some(f) => println(bytes.to_string_lossy(unix.shm_read(f, 0, 6)!)),
    none => println("no descriptor arrived"),
  }
}
```


<a id="signature-index-10-functions"></a>

## 原文固定快照 API 索引（10个函数）

```
// Stream socket connected to the listener at path.
// @since unreleased
effect unix.connect(path: String) -> Fd

// Listener bound at path, which must not exist yet.
// @since unreleased
effect unix.listen(path: String) -> Fd

// Blocks for the next connection on listener.
// @since unreleased
effect unix.accept(listener: Fd) -> Fd

// Sends all of data, the descriptors fds riding with its first byte.
// @since unreleased
effect unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit

// Descriptors received on sock and not yet taken, oldest first.
// @since unreleased
effect unix.take_fds(sock: Fd) -> List[Fd]

// The descriptors of xs readable within timeout_ms, in order; < 0 waits for one, 0 only checks.
// @since unreleased
effect unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]

// New zero-filled unnamed file of size bytes.
// @since unreleased
effect unix.shm_create(size: Int) -> Fd

// Grows or shrinks the file fd to size bytes.
// @since unreleased
effect unix.shm_resize(fd: Fd, size: Int) -> Unit

// Writes data into fd at offset; a negative offset or a write past the end aborts (shm_resize first).
// @since unreleased
effect unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit

// Up to len bytes of fd from offset; fewer at its end.
// @since unreleased
effect unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes
```

## API 说明译文（按上述签名顺序）

- `effect unix.connect(path: String) -> Fd`：连接到 path 处监听器的流式套接字。
- `effect unix.listen(path: String) -> Fd`：绑定在 path 的监听器，path 必须尚不存在。
- `effect unix.accept(listener: Fd) -> Fd`：阻塞等待 listener 上的下一个连接。
- `effect unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit`：发送全部 data，描述符 fds 随第一个字节传递。
- `effect unix.take_fds(sock: Fd) -> List[Fd]`：sock 上收到但尚未取走的描述符，最早的在前。
- `effect unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]`：按顺序返回 xs 中在 timeout_ms 内可读的描述符；< 0 等待一个可读，0仅检查。
- `effect unix.shm_create(size: Int) -> Fd`：新建 size 字节的填零无名文件。
- `effect unix.shm_resize(fd: Fd, size: Int) -> Unit`：将文件 fd 扩大或缩小到 size 字节。
- `effect unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit`：在 fd 的 offset 处写入 data；负偏移或越过末尾的写入会中止（先 shm_resize）。
- `effect unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes`：从 fd 的 offset 读取最多 len 字节，末尾可能更少。
