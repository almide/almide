# env

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](env.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](env.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](env.zh-CN.md)



> 原文：[env.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/env.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

> 翻译说明：关于秘密值的日志输出及常量时间比较，本文仅翻译原文说明，不构成本译文提供的独立安全保证，也不主张经 `secret.reveal` 取出的值仍受保护。OS 名称取决于宿主和运行方式。固定原文的正文与 API 注释对 Wasm OS 名称的说明范围不同，返回类型标题与 effect 签名也存在表述差异；译文未擅自统一。

环境与系统。import env，effect。

### `env.args() -> List[String]`

以字符串列表获取命令行参数：所有目标都原样返回 `argv[1..]`。程序参数中的 `--` 属于程序（`./p a -- b` 看到 `["a", "--", "b"]`）；`almide run app.almd -- a -- b` 只消耗自身分隔符，将其余内容原样转发。

```almd check
import env

effect fn main() -> Unit = {
  let args = env.args()
  println("${args}")
}
```

### `env.get(name: String) -> Option[String]`

获取环境变量值；未设置时为 none。

```almd check
import env

effect fn main() -> Unit = {
  let home = env.get("HOME") // => some("/Users/alice")
  println(home ?? "unset")
}
```

### `env.secret(name: String) -> Option[Secret[String]]`

读取存放凭据（API 令牌、webhook 密钥）的环境变量：值与 `env.get` 相同，但从创建时就是 `Secret[String]`（`import secret`）。它在所有位置打印为 `Secret(***)`，其 `==` 是常量时间比较（C-426），因此令牌不会进入日志，也不会通过泄漏信息的 `==` 比较。用 `secret.reveal` 读取内容；未设置时为 none（C-438）。

```almd check
import env
import secret

effect fn main() -> Unit = {
  match env.secret("API_TOKEN")! {
    some(token) => println("${token} ${token == secret.new("expected")}"),
    none => println("API_TOKEN is not set"),
  }
}
```

### `env.set(name: String, value: String) -> Unit`

设置环境变量。名称为空、含 `=` 或 NUL，或值含 NUL 时返回错误且不设置任何内容——这些组合无法保存在进程环境中（C-421）：`env.set: empty name`、`env.set: name contains '='`、`env.set: name contains NUL`、`env.set: value contains NUL`。

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  println(env.get("MY_VAR") ?? "unset")
}
```

### `env.remove(name: String) -> Result[Unit, String]`

取消此进程及其此后启动的子进程的环境变量；之后 `env.get` 返回 none，`env.vars` 不再包含该名称。删除未设置的名称也成功。名称遵循 `env.set` 的规则（C-421），并以自己的函数名返回同类错误：`env.remove: empty name`、`env.remove: name contains '='`、`env.remove: name contains NUL`。

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  env.remove("MY_VAR")!
  println(env.get("MY_VAR") ?? "unset") // => unset
}
```

### `env.vars() -> Map[String, String]`

此进程的全部环境变量，以名称为键并按名称排序，已反映程序自身的 `env.set` / `env.remove` 调用。值均为明文，包含秘密：读取凭据时，请用 `env.secret` 按名称获取，不要列举整个环境。调用写作 `env.vars()!`，这是目前所有具有 Almide 主体的标准库 effect fn 的调用形式（R-env-03）。

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  let all = env.vars()!
  println(all["MY_VAR"] ?? "unset") // => hello
}
```

### `env.cwd() -> Result[String, String]`

获取当前工作目录。

```almd check
import env

effect fn main() -> Unit = {
  let dir = env.cwd()!
  println(dir)
}
```

### `env.sleep_ms(ms: Int) -> Unit`

休眠指定毫秒数。零或负时长视为已到期，在所有目标上立即返回。截止时间不同：`process.ProcessCommand` 的 `timeout_ms` 为 `0` 表示完全没有截止时间。

```almd check
import env

effect fn main() -> Unit = {
  env.sleep_ms(1000) // sleep 1 second
  println("awake")
}
```

### `env.os() -> String`

获取宿主报告的操作系统名称（原生运行时为 `linux`、`macos`、`windows` 或 `unknown`）。嵌入式 Wasm 宿主也报告其宿主 OS；stock-WASI 适配报告 `wasi`。

```almd check
import env

effect fn main() -> Unit = {
  println(env.os()) // => "macos"
}
```



## 固定快照 API 索引（9个函数）

```
// Program arguments, argv[0] excluded.
// @since 0.5.0 or earlier
effect env.args() -> List[String]

// Value of name, or none if unset.
// @since 0.5.0 or earlier
effect env.get(name: String) -> Option[String]

// Value of name as a Secret (prints Secret(***), constant-time ==), or none if unset. For a token or key.
// @since unreleased
effect env.secret(name: String) -> Option[Secret[String]]

// Sets name for this process and later spawns; err if name is empty or holds `=` or NUL, or value holds NUL.
// @since 0.5.0 or earlier
effect env.set(name: String, value: String) -> Unit

// Removes name for this process and later spawns; ok if unset; err if name is empty or holds `=` or NUL.
// @since unreleased
effect env.remove(name: String) -> Unit

// Every environment variable of this process, keyed and ordered by name.
// Holds secrets as plain values: read a credential by name with `env.secret` instead.
// @since unreleased
effect env.vars() -> Map[String, String]

// Absolute current directory; err if unreadable.
// @since 0.5.0 or earlier
effect env.cwd() -> String

// Blocks ms milliseconds; zero or negative ms returns without waiting.
// @since 0.5.0 or earlier
effect env.sleep_ms(ms: Int) -> Unit

// Host OS: macos, linux, windows or unknown; "wasi" on wasm (C-189).
// @since 0.5.0 or earlier
env.os() -> String
```




## API 说明译文（按上述签名顺序）

- `effect env.args() -> List[String]` — 程序参数，不含 argv[0]。
- `effect env.get(name: String) -> Option[String]` — name 的值，未设置为 none。
- `effect env.secret(name: String) -> Option[Secret[String]]` — 以 Secret 返回 name 的值（打印为 `Secret(***)`，常量时间 ==）；未设置为 none。用于令牌或密钥。
- `effect env.set(name: String, value: String) -> Unit` — 为此进程及之后启动的子进程设置 name；名称为空或含 `=`、NUL，或值含 NUL 时为 err。
- `effect env.remove(name: String) -> Unit` — 为此进程及之后启动的子进程移除 name；未设置也成功；名称为空或含 `=`、NUL 时为 err。
- `effect env.vars() -> Map[String, String]` — 此进程的全部环境变量，以名称为键并排序。秘密也作为普通值包含在内：凭据请改用 `env.secret` 按名称读取。
- `effect env.cwd() -> String` — 当前目录的绝对路径；无法读取时为 err。
- `effect env.sleep_ms(ms: Int) -> Unit` — 阻塞 ms 毫秒；零或负值不等待即返回。
- `env.os() -> String` — 宿主 OS：macos、linux、windows 或 unknown；wasm 上为 "wasi"（C-189）。
