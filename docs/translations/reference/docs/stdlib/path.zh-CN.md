# path

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](path.zh-CN.md)



> 原文：[path.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/path.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

路径字符串操作。`import path`。

以下所有函数都是纯函数：`path` 从不访问文件系统。它操作路径*字符串*，因此两种目标行为相同，无需能力。需要读写时使用 `fs`。

## 路径组成部分

### `path.join_within(base: String, child: String) -> Result[String, String]`

对于留在 `base` 内的相对 `child`，返回 `path.join(base, child)`；其他情况——绝对 child、任意一种分隔符下的 `..`、`~`、盘符或 NUL——返回 `err("path traversal rejected: <child>")`，使结果在词法上保持于 `base` 之下。这不是文件系统包含检查：符号链接可能通向别处。对不可信的文件系统访问，请使用限定根目录的 `fs` 操作，并审查其平台特有的包含限制。基于描述符的实现仅限受支持的64位 Linux/macOS 宿主；回退平台没有相同的抗竞争能力。

```almd run
import path

fn main() -> Unit = {
  println(path.join_within("/srv/uploads", "a/b.txt") ?? "refused")
  println(path.join_within("/srv/uploads", "/etc/passwd") ?? "refused")
  println(path.join_within("/srv/uploads", "../../etc/passwd") ?? "refused")
}
```
```output
/srv/uploads/a/b.txt
refused
refused
```

### `path.join(base: String, child: String) -> String`

连接两个片段，先从 `base` 移除一个末尾分隔符，再添加分隔符和 child。绝对 `child` 会替换 `base`，而 `child` 中的 `..` 会向外退出：对跨越信任边界而来的 child，使用 `path.join_within`。

```almd run
import path

fn main() -> Unit = {
  println(path.join("/usr", "bin"))
  println(path.join("/usr/", "bin"))
}
```
```output
/usr/bin
/usr/bin
```

### `path.dirname(path: String) -> String`

最后一个分隔符之前的全部内容。

```almd run
import path

fn main() -> Unit = {
  println(path.dirname("/usr/bin/node"))
}
```
```output
/usr/bin
```

### `path.basename(path: String) -> String`

最后一个非空路径片段；对于 `/` 或空路径返回空。

```almd run
import path

fn main() -> Unit = {
  println(path.basename("/usr/bin/node"))
}
```
```output
node
```

### `path.stem(path: String) -> String`

移除扩展名后的基本文件名。

```almd run
import path

fn main() -> Unit = {
  println(path.stem("/tmp/report.tar.gz"))
}
```
```output
report.tar
```

### `path.extension(path: String) -> Option[String]`

不带点号的扩展名，没有则为 `none`。以点开头的文件名（`.gitignore`）没有扩展名。

```almd run
import path

fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(path.extension("file.txt")))
  println(show(path.extension("Makefile")))
  println(show(path.extension(".gitignore")))
  println(show(path.extension(".bashrc.bak")))
}
```
```output
some("txt")
none
none
some("bak")
```

### `path.is_absolute(path: String) -> Bool`

路径从文件系统根开始时为 true。

```almd run
import path

fn main() -> Unit = {
  println("${path.is_absolute("/usr")}")
  println("${path.is_absolute("relative/path")}")
}
```
```output
true
false
```

### `path.normalize(path: String) -> String`

消去 `.` 片段，将 `..` 相对于前一片段解析，并合并重复分隔符。纯词法操作，不解析符号链接，也不访问文件系统，因此涉及符号链接时可能不同于 OS 的结果。成本与路径长度成线性关系。

```almd run
import path

fn main() -> Unit = {
  println(path.normalize("/a/./b/../c"))
}
```
```output
/a/c
```



## 固定快照 API 索引（11个函数）

```
// base/child; an absolute child replaces base.
// @since 0.2.0 or earlier
path.join(base: String, child: String) -> String

// base/child; err if child could leave base.
// @since unreleased
path.join_within(base: String, child: String) -> Result[String, String]

// path minus last segment; "" if no slash.
// @since 0.2.0 or earlier
path.dirname(path: String) -> String

// Last non-empty segment; "" for / or empty path.
// @since 0.2.0 or earlier
path.basename(path: String) -> String

// After basename's last dot; none if no dot or dotfile.
// @since 0.2.0 or earlier
path.extension(path: String) -> Option[String]

// True when path starts with /.
// @since 0.2.0 or earlier
path.is_absolute(path: String) -> Bool

// Basename minus last extension; dotfiles as-is.
// @since 0.3.0 or earlier
path.stem(path: String) -> String

// Resolves . and .. lexically; "" becomes ".".
// @since 0.3.0 or earlier
path.normalize(path: String) -> String

// Segments of normalize(path), led by "/" when absolute; ["."] for "" or ".".
// @since unreleased
path.components(path: String) -> List[String]

// path relative to base, lexically; "." when equal; none when not under base.
// @since unreleased
path.relative_to(path: String, base: String) -> Option[String]

// path with its last extension replaced by ext; "" removes it; unchanged without a file name.
// @since unreleased
path.with_extension(path: String, ext: String) -> String
```




## API 说明译文（按上述签名顺序）

- `path.join(base: String, child: String) -> String` — base/child；绝对 child 替换 base。
- `path.join_within(base: String, child: String) -> Result[String, String]` — base/child；child 可能离开 base 时为 err。
- `path.dirname(path: String) -> String` — 去掉最后片段的 path；没有斜杠时为 ""。
- `path.basename(path: String) -> String` — 最后一个非空片段；/ 或空路径为 ""。
- `path.extension(path: String) -> Option[String]` — 基本文件名最后一个点之后的内容；没有点或为点文件时为 none。
- `path.is_absolute(path: String) -> Bool` — path 以 / 开头时为 true。
- `path.stem(path: String) -> String` — 基本文件名去掉最后扩展名；点文件不变。
- `path.normalize(path: String) -> String` — 词法解析 . 和 ..；"" 变为 "."。
- `path.components(path: String) -> List[String]` — normalize(path) 的片段，绝对路径以 "/" 开头；"" 或 "." 得到 ["."]。
- `path.relative_to(path: String, base: String) -> Option[String]` — path 相对 base 的词法路径；相等为 "."，不在 base 下则为 none。
- `path.with_extension(path: String, ext: String) -> String` — 将 path 的最后扩展名替换为 ext；"" 表示移除；没有文件名则不变。
