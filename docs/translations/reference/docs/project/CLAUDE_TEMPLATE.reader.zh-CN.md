# Almide (.almd)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE_TEMPLATE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE_TEMPLATE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE_TEMPLATE.reader.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/CLAUDE_TEMPLATE.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范、不激活的阅读快照。这是历史参考资料，不是有效的代理指令或用于生成的项目模板，不授予运行命令的权限，也不自动同步。原文主张不是独立的当前测量；未执行命令或调用模型。

<a id="stop--read-these-rules-first"></a>

## STOP — 先阅读这些规则

Almide 不是 Rust／TypeScript／Python。以下是最常见的错误：

```
WRONG                          RIGHT
─────────────────────────────  ──────────────────────────────
a && b                         a and b
a || b                         a or b
!x                             not x
if cond { ... }                if cond then expr else expr
|x| x + 1                     (x) => x + 1
let mut x = 0                  var x = 0
return expr                    (last expression IS the return)
fn foo() -> Int { ... }        fn foo() -> Int = { ... }
import { json }                import json
struct Foo { x: Int }          type Foo = { x: Int }
x.len()                        WORKS! (UFCS — auto-resolves by type)
try { } catch(e) { }          match result { Ok(v) => ..., Err(e) => ... }
while cond { ... }             while cond { ... }  (SAME — while IS supported)
```

表中英文说明：左侧为错误写法，右侧为正确写法。最后一个表达式就是返回值。len 方法形式可用，UFCS 会按类型自动解析。while 已受支持，所示写法相同。

<a id="quick-patterns"></a>

## 常用模式

```almide
// Function
fn add(a: Int, b: Int) -> Int = a + b

// Effect function (does I/O — returns Result; add `import fs` to the file)
effect fn read(path: String) -> Result[String, String] = {
  let text = fs.read_text(path)!
  ok(text)
}

// Main entry point (args via env.args(), Result is auto-wrapped)
effect fn main() -> Unit = {
  println("hello")
}

// If-then-else (omitting else gives Unit)
let x = if n > 0 then "pos" else "neg"

// Match
let name = match list.get(args, 1) {
  some(v) => v,
  none => "default",
}

// Lambda — params in parens, NO `fn` keyword (single or multi-arg)
let doubled = list.map(xs, (x) => x * 2)
let total = list.fold(xs, 0, (acc, x) => acc + x)

// For loop
for item in items {
  println(item)
}
for (i, item) in list.enumerate(items) {
  println(int.to_string(i) + ": " + item)
}

// While loop
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}

// Top-level constant
let PI = 3.14159265358979323846

// Mutable state
var count = 0
count = count + 1

// List index read
let xs = [10, 20, 30]
let second = xs[1]       // 20 (direct indexing returns T; use list.get for Option[T])

// Guard (early exit)
guard list.len(args) > 1 else err("need args")

// String interpolation
let msg = "Hello ${name}, you are ${int.to_string(age)} years old"

// Record type
type Point = { x: Int, y: Int }
let p = Point { x: 1, y: 2 }

// Variant type
type Shape =
  | Circle(Float)
  | Rect(Float, Float)

// List/String concat
let combined = [1, 2] + [3, 4]
let greeting = "hello" + " " + "world"

// UFCS — the checker resolves by receiver type and scope, not an unconditional rewrite
let n = "hello".len()          // same as string.len("hello")
let m = [1, 2, 3].len()       // same as list.len([1, 2, 3])
let r = "hello".split(" ").reverse()  // chaining works too
```

代码注释说明（代码本身原样保留）：

- 展示普通函数与进行 I/O 的 effect 函数。后者返回 Result，并应在文件中添加 fs 导入。
- main 是入口点，参数通过 env.args() 获取，Result 自动包装。
- if-then-else 省略 else 时得到 Unit，之后为 match 示例。
- lambda 参数放在括号内；单参数或多参数都不使用 fn 关键字。
- 展示 for 循环、while 循环、顶层常量与可变状态。
- 列表直接索引读取返回 T；需要 Option[T] 时使用 list.get。示例的第二个元素为 20。
- guard 用于提前退出，之后依次为字符串插值、记录类型、变体类型以及列表／字符串连接。
- UFCS 由检查器根据接收者类型和作用域解析，并非无条件改写。字符串 len 等同 string.len，列表 len 等同 list.len，也支持链式调用。

<a id="stdlib-quick-reference"></a>

## 标准库速查

当前签名请使用[生成的标准库参考](https://github.com/almide/almide/blob/develop/docs/stdlib/README.md)，导入与调用示例参见 [CHEATSHEET](https://github.com/almide/almide/blob/develop/docs/CHEATSHEET.md)。核心模块以模块名自动导入，并非把每个函数都导入为非限定名称。
平台／opt-in 模块需要显式导入。旧 `error` 模块已经退役。`io.read_all()` 仍然可用，返回 `String`；其 effect 上下文调用语法与目标行为请查阅 IO 参考。

这个精选名称索引由 CI 对照标准库源码检查；签名与可用性仍以生成的参考为准。

```
AUTO-IMPORTED (no import needed):
  string: len trim split lines contains starts_with ends_with replace
          index_of slice to_upper to_lower to_list pad_start pad_end
  list:   len get first last map flat_map filter find fold enumerate
          zip sort sort_by reverse any all take drop unique join sum
  map:    new get set contains remove keys values to_list from_list merge len
  set:    new insert contains remove union intersection len
  int:    to_string parse parse_radix abs min max clamp pow to_hex from from_checked from_saturating
  float:  to_string from abs min max pow round floor ceil
  math:   sqrt log sin cos pi e
  value:  get get_string get_int get_bool get_float get_array get_path as_object
  datetime, value, option, result, bytes — also auto-imported

IMPORT REQUIRED (effectful / platform / opt-in):
  import fs      — read_text write_text read_lines append_text exists mkdir_p remove list_dir stat temp_dir
  import env     — get set args cwd os sleep_ms
  import process — exec output run
  import io      — read_line print read_all
  import json    — parse stringify stringify_pretty
  import random  — int float shuffle
  import regex   — is_match find find_all replace split captures
  import path    — join dirname basename extension stem normalize is_absolute
  import args    — has_flag option positional
```

索引标题含义：前半为“自动导入（无需 import）”，后半为“需要 import（effectful／平台／opt-in）”。datetime、value、option、result、bytes 也自动导入。

---

这不是可用的 CLAUDE.md 或生成器输入。只输出的 effect main、WRONG/RIGHT 草图中的大小写和旧 API 名称等历史示例原样保留，不擅自更新。指向 develop 的动态链接作为原文链接保留，并非当前签名的固定证据。
