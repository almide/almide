# process

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](process.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](process.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](process.zh-CN.md)



> 原文：[docs/stdlib/process.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名和代码内注释保留原文，在代码外附说明译文。这是生成索引的固定副本，本译文不是生成目标。 原文开头称嵌入式 wasm 宿主支持所有函数，但后台运行一节又写“目前仅原生目标支持”。本译文保留此差异，未重新验证实现。

子进程，以及本程序自身的退出和 pid。`import process`、`effect fn`。

子进程的设置集中在一个记录 `ProcessCommand` 中：

```almd
type ProcessCommand = {
  cmd: String,          // the program
  args: List[String],   // its argument list
  dir: String = "",     // working directory; "" keeps this program's
  stdin: String = "",   // text fed to its stdin; "" gives it none
  timeout_ms: Int = 0,  // deadline; 0 means none
}
```

用类型名书写记录，就能省略有默认值的字段：`process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo" }`。没有类型名（`{ cmd: ..., args: ... }`）时必须提供所有字段。

程序及其参数始终是 argv 列表：这里不会通过 `sh -c` 执行 shell 字符串，因此参数不会被重新拆分或展开。`ProcessCommand` 有三种运行方式：

| 调用 | 是否等待 | 输出 | 返回值 |
|---|---|---|---|
| `output(c)` | 直到结束 | 捕获 | `ProcessStatus { code, stdout, stderr }` |
| `start(c)` | 否 | 捕获，随到随读 | 用于 `poll` / `wait` / `stop` 的 `ProcessChild` |
| `run(c)` | 直到结束 | 输出到本终端 | 退出码 |

唯一的快捷形式是 `exec(cmd, args)`：退出码为 0 时返回 stdout，否则返回包含 stderr 文本的 `err`。

这里所有可能失败的调用都返回 `Result`；写上 `!` 或处理 `err`（ADR-0008）。`poll`、`child_pid`、`read_new_stdout` 和 `read_new_stderr` 返回普通值，但由于是 `effect fn`，调用也产生 `Result`，所以同样需要 `!`：`process.child_pid(c)!`。

**目标**（[ADR-0025](../../../../adr/0025-wasm-process-is-a-private-host-capability.md)）。原生目标支持所有函数。`almide run --target wasm` 和 `almide test` 的 wasm 路径也支持所有函数，结果与原生一致：捕获文本、增量读取、退出码、截止时间及 err 字符串均相同。这里 `start` 的子进程是宿主资源：宿主持有管道和截止时间；一次运行结束时会停止既未等待也未停止的子进程，与原生退出时一样。`almide build
--target wasm` 能构建启动子进程的程序，但产物导入私有的 `almide:process/spawn` 接口，因此 `wasmtime run` 等标准运行时会在加载时拒绝；它可在实现该导入的宿主上运行。`--component` 构建以 E081 拒绝。

**`[permissions] proc`**：项目可列出此模块允许启动的命令。此时每个启动子进程的调用都必须用列表中的字符串字面量指定命令：即 `exec` 的第一个参数，或在 `output`、`run`、`start` 调用中书写的 `ProcessCommand` 的 `cmd` 字段；否则程序不能编译：

```toml
[permissions]
proc = ["git", "cargo"]
```

<a id="the-shortcut"></a>

## 快捷形式

### `process.exec(cmd: String, args: List[String]) -> Result[String, String]`

以 `args` 运行 `cmd`，若退出码为 0 则返回 stdout；否则返回包含 stderr 文本（可能为空）的 `err`。

```almd check
import process

effect fn main() -> Unit = {
  let listing = process.exec("ls", ["-la"])!
  println(listing)
}
```

<a id="run-once-capture-everything"></a>

## 运行一次并捕获全部输出

### `process.output(c: ProcessCommand) -> Result[ProcessStatus, String]`

运行 `c` 直至结束，返回退出码及其写出的全部内容：`ProcessStatus { code: Int, stdout: String, stderr: String }`；若子进程因信号结束，`code` 为 `-1`。所有退出码都是 `ok`，需自行检查 `code`。`err` 情况是无法启动（程序缺失或 `dir` 不存在）以及超过截止时间。

```almd check
import process

effect fn main() -> Unit = {
  let st = process.output(process.ProcessCommand { cmd: "git", args: ["status", "--short"], dir: "." })!
  if st.code == 0 then println(st.stdout) else println("git failed: ${st.stderr}")
}
```

`stdin` 向子进程提供文本；未指定时，子进程的 stdin 为空（不是终端）：

```almd check
import process

effect fn main() -> Unit = {
  let st = process.output(process.ProcessCommand { cmd: "sort", args: [], stdin: "b\na\n" })!
  println(st.stdout)
}
```

**截止时间。** `timeout_ms` 是唯一允许的超时（C-214）：启动进程本已不在字节同一性契约内，限制它不会额外增加非确定性。超过截止时间会杀死子进程，err 严格为 `exec timed out after <ms>ms`；是否超时取决于宿主。比较退出码的调用方通常将其映射到 124。

捕获输出的子进程在独立进程组中运行，因此截止时间到达时会杀死整个进程树；这也使它成为控制终端的后台作业。读取终端或修改终端设置（`stty -echo`）的子进程会被内核以 SIGTTIN / SIGTTOU 停止；此时立即返回 err（`...: the child was stopped by SIGTTOU: it tried to use the terminal from a
background process group; run terminal programs with process.run`），而不是等到截止时间。

```almd check
import process

effect fn main() -> Unit = {
  match process.output(process.ProcessCommand { cmd: "cargo", args: ["build"], timeout_ms: 60000 }) {
    ok(st) => println("exit ${st.code}"),
    err(e) => println(e),
  }
}
```

<a id="run-on-this-terminal"></a>

## 在本终端运行

### `process.run(c: ProcessCommand) -> Result[Int, String]`

在本程序的终端上运行 `c`：子进程继承 stdout 和 stderr（未设置 `c.stdin` 时也继承 stdin），并留在本程序的进程组中，因此分页器、编辑器、`stty` 等读取或重新配置终端的程序可工作，耗时构建也能边运行边输出。启动子进程前会刷新本程序的 stdout。不捕获任何输出；返回退出码（若被信号杀死则为 `-1`）。任何退出码都是 `ok`；`err` 情况为无法启动（`dir` 不存在时错误同时指出 `dir` 和 `cmd`）以及超过 `timeout_ms`。

`output` 捕获并返回输出；`run` 连接到终端并返回退出码。

```almd check
import process

effect fn main() -> Unit = {
  let code = process.run(process.ProcessCommand { cmd: "cargo", args: ["build"], dir: "." })!
  println("cargo build exited ${code}")
}
```

<a id="run-in-the-background"></a>

## 在后台运行

`start` 立即返回 `ProcessChild` 句柄。子进程在自己的进程组中运行；运行时捕获其 stdout 和 stderr（绝不混入本程序输出），通过 `read_new_stdout` / `read_new_stderr` 读取。目前仅原生目标支持（参见目标说明）。

### `process.start(c: ProcessCommand) -> Result[ProcessChild, String]`

启动 `c` 并立即返回；无法启动时为 `err`。

### `process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?`

永不阻塞：`c` 运行期间为 `none`，之后为 `some(ok(status))`；若超过截止时间则为 `some(err(e))`。与 `http.poll` 的形式相同。

### `process.wait(c: ProcessChild) -> Result[ProcessStatus, String]`

阻塞直到 `c` 结束，返回其结果。

### `process.stop(c: ProcessChild, grace_ms: Int) -> Result[ProcessStatus, String]`

向 `c` 的进程组发送 SIGTERM，给予 `grace_ms` 结束时间，然后发送 SIGKILL 并回收。信号发给整个组，因此子进程自己的子进程也一起停止。结束后再次调用会返回相同结果。

### `process.read_new_stdout(c: ProcessChild) -> String` / `process.read_new_stderr(c: ProcessChild) -> String`

`c` 自上次读取后写入该流的内容；永不阻塞。

### `process.child_pid(c: ProcessChild) -> Int`

`c` 的 OS 进程 ID，可用于 `process.kill` 或日志行。

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "ping", args: ["-c", "3", "127.0.0.1"] })!
  let pid = process.child_pid(c)!
  println("started pid ${pid}")
  match process.poll(c)! {
    none => println("still running"),
    some(ok(st)) => println("exited ${st.code}"),
    some(err(e)) => println(e),
  }
  let st = process.wait(c)!
  let errs = process.read_new_stderr(c)!
  println(st.stdout)
  println(errs)
}
```

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "sleep", args: ["30"] })!
  let st = process.stop(c, 500)!
  println("stopped, code ${st.code}")
}
```

<a id="signals-exit-and-pid"></a>

## 信号、退出与 pid

### `process.kill(pid: Int, signal: Int) -> Result[Unit, String]`

向进程 `pid` 发送 `signal`（例如 SIGTERM 为 15、SIGKILL 为 9）。信号通过 kill(2) 系统调用发送，不使用 `PATH` 中的 `kill` 程序。`pid` 必须在 1..=2147483647；拒绝 `0`（调用方的进程组）和 `-1`（用户可发信号的所有进程）。`signal` 须在 0..=31（0 仅询问进程是否存在）。每个 err 都是固定文本：`process.kill(<pid>, <signal>): pid must be in 1..=2147483647`、`…: signal must be in 0..=31`，或 kill(2) 失败的 `no such process`、`operation not permitted`、`invalid signal`。使用 `[permissions] proc` 时，`kill` 必须在列表中。对于本程序的子进程，`process.stop` 还会等待并回收；`kill` 面向任意 pid。

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "sleep", args: ["30"] })!
  process.kill(process.child_pid(c)!, 15)!
  let st = process.wait(c)!
  println("code ${st.code}")
}
```

### `process.exit(code: Int) -> Never`

以给定状态码退出，即 POSIX 退出状态 **0 到 255**。在原生目标及 `almide run --target wasm` 上代码原样传递，因此包装程序可使用子进程状态退出，包括 126、127 和 128+n（#2780）。其他值均为定义域错误，stderr 输出一行 `Error: exit code must be in 0..=255`，并以 1 退出；所有目标相同（C-350）。Rust、Go、Python 和 Node 则截取低 8 位，因此 `exit(256)` 会报告成功。

WASI preview-1 构建（`almide build --target wasm`，在 `wasmtime` 等标准运行时上运行）只能传递 0..=125。标准运行时会对 `proc_exit(126)` 及更高值触发陷阱，因此该构建自行拒绝 126..=255，输出 `Error: a WASI preview-1 build cannot exit with a code in 126..=255` 并以 1 退出。不会无提示地以 1 退出。

```almd check
import process

effect fn main() -> Unit = {
  println("fatal: giving up")
  process.exit(1)
}
```

### `process.pid() -> Int`

本程序的 OS 进程 ID。

```almd check
import process

effect fn main() -> Unit = {
  println(int.to_string(process.pid()))
}
```



<a id="signature-index-13-functions"></a>

## 签名索引（13 个函数）

```
// Stdout on exit 0; else err with stderr text.
// @since 0.5.0 or earlier
effect process.exec(cmd: String, args: List[String]) -> String

// Runs c to its end: any exit code is ok; err if it cannot start or passes its deadline.
// @since unreleased
effect process.output(c: ProcessCommand) -> ProcessStatus

// Runs c on this terminal (its stdin too, unless c.stdin is set); the exit code, -1 if signalled.
// @since unreleased
effect process.run(c: ProcessCommand) -> Int

// Starts c in its own process group and returns at once; its output is captured.
// @since unreleased
effect process.start(c: ProcessCommand) -> ProcessChild

// Never blocks: none while c runs, then its outcome (err past its deadline).
// @since unreleased
effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]

// Blocks until c ends; its outcome.
// @since unreleased
effect process.wait(c: ProcessChild) -> ProcessStatus

// SIGTERM to c's group, grace_ms to end, then SIGKILL; again after the end is the same outcome.
// @since unreleased
effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus

// What c wrote to stdout since the last read; never blocks.
// @since unreleased
effect process.read_new_stdout(c: ProcessChild) -> String

// What c wrote to stderr since the last read; never blocks.
// @since unreleased
effect process.read_new_stderr(c: ProcessChild) -> String

// The OS process id of c, for process.kill.
// @since unreleased
effect process.child_pid(c: ProcessChild) -> Int

// Sends signal (0..=31) to pid (> 0) by kill(2); err names why.
// @since 0.12.3 or earlier
effect process.kill(pid: Int, signal: Int) -> Unit

// Ends with code 0..=255; others abort, exit 1.
// @since 0.5.0 or earlier
effect process.exit(code: Int) -> Never

// OS process ID of this program.
// @since 0.12.3 or earlier
effect process.pid() -> Int
```

<a id="type-index-2-types"></a>

## 类型索引（2 个类型）

```
// Exit code plus captured stdout and stderr; code -1 if a signal ended the child.
// @since 0.15.0 or earlier
type process.ProcessStatus = { code: Int, stdout: String, stderr: String }

// A child's settings: dir "" keeps this one, stdin "" gives it none, timeout_ms 0 means no deadline.
// @since unreleased
type process.ProcessCommand = { cmd: String, args: List[String], dir: String, stdin: String, timeout_ms: Int }
```



## 索引注释的说明译文

- `effect process.exec(cmd: String, args: List[String]) -> String` — 退出码 0 时为 stdout，否则为含 stderr 文本的 err。 (@since 0.5.0 or earlier)
- `effect process.output(c: ProcessCommand) -> ProcessStatus` — 运行 c 直至结束；任何退出码都是 ok，无法启动或超过截止时间则 err。 (@since unreleased)
- `effect process.run(c: ProcessCommand) -> Int` — 在本终端运行 c（未设置 c.stdin 时包括 stdin）；返回退出码，因信号结束时为 -1。 (@since unreleased)
- `effect process.start(c: ProcessCommand) -> ProcessChild` — 在独立进程组启动 c 并立即返回；捕获输出。 (@since unreleased)
- `effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]` — 永不阻塞：运行时为 none，之后为结果（超时则 err）。 (@since unreleased)
- `effect process.wait(c: ProcessChild) -> ProcessStatus` — 阻塞直到 c 结束，返回其结果。 (@since unreleased)
- `effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus` — 向 c 的组发送 SIGTERM，给予 grace_ms 后 SIGKILL；结束后再调用结果相同。 (@since unreleased)
- `effect process.read_new_stdout(c: ProcessChild) -> String` — c 自上次读取后写入 stdout 的内容；不阻塞。 (@since unreleased)
- `effect process.read_new_stderr(c: ProcessChild) -> String` — c 自上次读取后写入 stderr 的内容；不阻塞。 (@since unreleased)
- `effect process.child_pid(c: ProcessChild) -> Int` — c 的 OS 进程 ID，用于 process.kill。 (@since unreleased)
- `effect process.kill(pid: Int, signal: Int) -> Unit` — 通过 kill(2) 向 pid（> 0）发送 signal（0..=31）；err 指明原因。 (@since 0.12.3 or earlier)
- `effect process.exit(code: Int) -> Never` — 以 0..=255 的 code 结束；其他值中止并以 1 退出。 (@since 0.5.0 or earlier)
- `effect process.pid() -> Int` — 本程序的 OS 进程 ID。 (@since 0.12.3 or earlier)
- `type process.ProcessStatus = { code: Int, stdout: String, stderr: String }` — 退出码及捕获的 stdout、stderr；子进程因信号结束时 code 为 -1。 (@since 0.15.0 or earlier)
- `type process.ProcessCommand = { cmd: String, args: List[String], dir: String, stdin: String, timeout_ms: Int }` — 子进程设置：dir "" 保持当前目录，stdin "" 不提供输入，timeout_ms 0 表示无截止时间。 (@since unreleased)
