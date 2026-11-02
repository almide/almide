# io

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](io.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](io.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](io.zh-CN.md)



> 原文：[io.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

标准流，以及适用于任意句柄的字节 I/O。`import io`，`effect fn`。

`io` 分为两层。**文本快捷函数**——`read_line`、`read_all`、`print`、`eprint`——以 `String` 读写标准流。**句柄函数**——`read`、`read_exact`、`read_to_end`、`write`、`close`——通过满足 `Read`、`Write` 或 `Close` 的任意值传送 `Bytes`：标准流（`io.stdin()`、`io.stdout()`、`io.stderr()`）、`net.TcpStream`、`unix.Fd`。无论来源是什么，读取字节只有一种方式；没有模块自带专用的 `tcp_read` 或 `unix_recv`。

| 句柄 | 创建方式 | Read | Write | Close |
|---|---|---|---|---|
| `io.Stdin` | `io.stdin()` | 是 | | |
| `io.Stdout`, `io.Stderr` | `io.stdout()`, `io.stderr()` | | 是 | |
| `net.TcpStream` | `net.tcp_connect`, `net.tcp_accept` | 是 | 是 | 是 |
| `net.TcpListener`, `net.UdpSocket` | `net.tcp_listen`, `net.udp_bind` | | | 是 |
| `unix.Fd` | `unix.connect`, `unix.listen`, `unix.accept`, ... | 是 | 是 | 是 |

将句柄传给其类型不支持的函数会导致类型错误，因此 `io.close(io.stdout()!)` 无法编译。不能从 `Int` 创建句柄，只有句柄所属模块才能构造它。

**调用流函数。** `io.stdin()`、`io.stdout()` 和 `io.stderr()` 是 `effect fn`，每次调用产生 `Result`，所以要写 `io.stdout()!`。每个句柄函数也返回 `Result`（ADR-0008：显式传播），因此写入写作 `io.write(io.stdout()!, data)!`。

**stdout 只有一个缓冲区**（#2245）。`println`、`io.print` 和 `io.write(io.stdout()!, ...)` 都通过同一个64 KiB缓冲区写入，因此按程序顺序出现。它采用行缓冲（#3417）：结束一行的写入会刷新，无论是终端、管道还是文件，因此长时间运行的程序（监视器、服务器）会在打印时显示每行。stdout 是终端时，每次写入都会刷新。退出时、程序 panic 或 `main` 返回错误时、子进程运行前、每次读取 stdin 前也会刷新；`io.print` 总是刷新，因此 `io.print("")` 是显式刷新。`eprintln`、`io.eprint` 和对 `io.stderr()` 的写入，在刷新 stdout 后不经缓冲地写入 stderr。stdout 不是终端时，不保留两条流之间的相对顺序，但保留各流内部字节的顺序。wasm 端每次调用直接写给宿主，以相同顺序产生相同字节（C-162）。

**目标。** 原生与 wasm 都提供标准流和句柄函数。其他模块的句柄在该模块函数可用的环境中工作：见 [net](../../../../stdlib/net.md) 和 [unix](../../../../stdlib/unix.md)。

## 文本快捷函数

### `io.read_line() -> String?`

标准输入的下一行；输入结束时为 `none`。换行符（及末尾的 `\r`）会被去掉。空行为 `some("")`，因此循环可以跳过空行，同时在输入结束（Ctrl+D 或管道末尾）时停止。

行内字节按 UTF-8 解码；每个无效序列变成一个 `U+FFFD`（`�`），所有目标一致（#4142）。读取错误像文件结束一样终止输入。

```almd check
import io

effect fn main() -> Unit = {
  var open = true
  while open {
    match io.read_line() {
      none => {
        open = false
      },
      some("") => (),
      some(line) => println("> ${line}"),
    }
  }
}
```

程序只需要一行时，`??` 可为输入结束提供一个值：

```almd check
import io

effect fn main() -> Unit = {
  let name = io.read_line() ?? "stranger"
  println("Hello, ${name}!")
}
```

### `io.read_all() -> String`

把标准输入剩余内容全部作为一个字符串；输入结束时为空。字节按 UTF-8 解码，每个无效序列变成一个 `U+FFFD`（`�`），所有目标一致（#4142）。读取错误会结束输入，但保留此前读取的字节。若需未经解码的字节，使用 `io.read_to_end(io.stdin()!)`。

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_all()
  println("${list.len(string.lines(input))} line(s)")
}
```

### `io.print(s: String) -> Unit`

将 `s` 写到 stdout，不添加末尾换行，然后刷新。

```almd run
import io

effect fn main() -> Unit = {
  io.print("Enter name: ")
  println("Alice")
}
```
```output
Enter name: Alice
```

### `io.eprint(s: String) -> Unit`

将 `s` 写到 stderr，不添加末尾换行——即不换行的 `eprintln`，用于 stderr 上的提示或进度行。

```almd check
import io

effect fn main() -> Unit = {
  io.eprint("working... ")
  eprintln("done")
}
```

## 流句柄

### `io.stdin() -> Stdin` / `io.stdout() -> Stdout` / `io.stderr() -> Stderr`

以句柄形式提供标准流，供下列函数使用。每次调用都需要 `!`。它们与文本快捷函数共享流：`io.read_line` 和 `io.read(io.stdin()!, n)` 读取相同输入，`println` 和 `io.write(io.stdout()!, b)` 使用相同缓冲区。

## 任意句柄上的字节操作

### `io.read(h, max: Int) -> Result[Bytes, String]`

从 `h` 读取最多 `max` 字节，阻塞直到有部分数据到达。短读是正常的：结果含已到达的内容，可能少于 `max` 字节。`h` 结束（输入结束或对端关闭）后结果为空。负的 `max` 为 `err`。

```almd check
import io

effect fn main() -> Unit = {
  let head = io.read(io.stdin()!, 4)!
  println("${bytes.len(head)} byte(s)")
}
```

### `io.read_exact(h, n: Int) -> Result[Bytes, String]`

从 `h` 恰好读取 `n` 字节，按需要多次读取。如果 `h` 先结束，结果为说明已到达字节数的 `err`（`io.read_exact: the stream ended after 3 of 8 bytes`）。适合长度前缀协议或固定大小报头。

```almd check
import io

effect fn main() -> Unit = {
  let header = io.read_exact(io.stdin()!, 8)!
  println("header: ${bytes.len(header)} bytes")
}
```

### `io.read_to_end(h) -> Result[Bytes, String]`

读取所有字节，直到 `h` 结束。

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_to_end(io.stdin()!)!
  println("${bytes.len(input)} bytes in")
}
```

### `io.write(h, data: Bytes) -> Result[Unit, String]`

把 `data` 全部写给 `h`。对于 stdout，字节原样进入共享缓冲区——不转换 UTF-8，也不添加换行。

```almd run
import io

effect fn main() -> Unit = {
  io.write(io.stdout()!, bytes.from_string("Hi\n"))!
}
```
```output
Hi
```

### `io.close(h) -> Result[Unit, String]`

关闭 `h`；关闭两次也没问题。只有拥有资源的句柄才是 `Close`——即套接字和描述符，不含标准流。

```almd check
import io
import net

effect fn main() -> Unit = {
  let s = net.tcp_connect("example.com", 80)!
  io.write(s, bytes.from_string("HEAD / HTTP/1.0\r\n\r\n"))!
  let reply = io.read(s, 4096)!
  io.close(s)!
  println("${bytes.len(reply)} bytes")
}
```



## 固定快照 API 索引（12个函数）

```
// Next stdin line, newline cut, bad UTF-8 as U+FFFD; none at EOF, some("") for an empty line.
// @since 0.5.0 or earlier
effect io.read_line() -> Option[String]

// Rest of stdin as text, bad UTF-8 as U+FFFD; empty at EOF.
// @since 0.5.0 or earlier
effect io.read_all() -> String

// Writes s to stdout, no newline, then flushes.
// @since 0.5.0 or earlier
effect io.print(s: String) -> Unit

// Writes s to stderr, no newline.
// @since unreleased
effect io.eprint(s: String) -> Unit

// Standard input, for io.read / read_exact / read_to_end.
// @since unreleased
effect io.stdin() -> Stdin

// Standard output, for io.write.
// @since unreleased
effect io.stdout() -> Stdout

// Standard error, for io.write.
// @since unreleased
effect io.stderr() -> Stderr

// Up to max bytes from h; empty once h has ended; err if max < 0.
// @since unreleased
effect io.read(h: H, max: Int) -> Bytes

// Exactly n bytes from h; err if h ends first, naming how many arrived.
// @since unreleased
effect io.read_exact(h: H, n: Int) -> Bytes

// Every byte until h ends.
// @since unreleased
effect io.read_to_end(h: H) -> Bytes

// Writes all of data to h.
// @since 0.9.8 or earlier
effect io.write(h: H, data: Bytes) -> Unit

// Closes h; closing twice is ok.
// @since unreleased
effect io.close(h: H) -> Unit
```




## API 说明译文（按上述签名顺序）

- `effect io.read_line() -> Option[String]` — stdin 的下一行，去掉换行，无效 UTF-8 为 U+FFFD；EOF 为 none，空行为 some("")。
- `effect io.read_all() -> String` — stdin 的剩余文本，无效 UTF-8 为 U+FFFD；EOF 为空。
- `effect io.print(s: String) -> Unit` — 将 s 写到 stdout，不加换行，然后刷新。
- `effect io.eprint(s: String) -> Unit` — 将 s 写到 stderr，不加换行。
- `effect io.stdin() -> Stdin` — 标准输入，供 io.read / read_exact / read_to_end 使用。
- `effect io.stdout() -> Stdout` — 标准输出，供 io.write 使用。
- `effect io.stderr() -> Stderr` — 标准错误，供 io.write 使用。
- `effect io.read(h: H, max: Int) -> Bytes` — 从 h 读取最多 max 字节；h 结束后为空；max < 0 为 err。
- `effect io.read_exact(h: H, n: Int) -> Bytes` — 从 h 恰好读取 n 字节；h 先结束则为 err，并注明已到达字节数。
- `effect io.read_to_end(h: H) -> Bytes` — 读取全部字节直到 h 结束。
- `effect io.write(h: H, data: Bytes) -> Unit` — 将 data 全部写给 h。
- `effect io.close(h: H) -> Unit` — 关闭 h；关闭两次也成功。
