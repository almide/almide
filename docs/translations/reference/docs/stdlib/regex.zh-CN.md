# regex

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](regex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](regex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](regex.zh-CN.md)



> 原文：[docs/stdlib/regex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/regex.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名和代码内注释保留原文，在代码外附说明译文。这是生成索引的固定副本，本译文不是生成目标。 原文的时间、状态数和目标一致性是该提交的陈述，此处未重新测量。复杂度限制仅适用于匹配部分，并非包含回调或输出构建的整个调用的无条件时间保证。

正则表达式。import regex。

模式只转换为 `Regex` 一次，每个操作都先接收 `Regex`，再接收目标字符串（#3942）：

- `regex.new(pattern)`——用于程序已知有效的模式。字符串字面量由 `almide check` 检查（E102）；运行时构造的模式若被拒绝，将以 `Error: invalid regex: <reason>` 中止，退出码为 1。
- `regex.compile(pattern)`——用于来自输入的模式：`ok(re)`，被拒绝时为 `err(reason)`。

由于第一个参数是 `Regex`，将目标字符串管道传入操作会产生类型错误：以前 `s |> regex.find("[0-9]+")` 会不作提示地在模式中搜索目标字符串，现在会指出 `regex.find(regex.new("[0-9]+"), s)`。创建一次 `Regex`，在循环中复用。

<a id="regexnewpattern-string---regex-and-regexcompilepattern-string---resultregex-string"></a>

### `regex.new(pattern: String) -> Regex` 与 `regex.compile(pattern: String) -> Result[Regex, String]`

```almd run
import regex

fn reason(p: String) -> String = match regex.compile(p) {
  ok(_) => "ok",
  err(e) => e,
}

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println("${regex.is_match(digits, "abc123")}")
  println(reason("[0-9]+"))
  println(reason("(a|b"))
  println(reason("a{"))
  println(reason("(?P<year>\\d{4})"))
  println(reason("((a){200}){200}"))
}
```
```output
true
ok
unclosed group
malformed repetition
named groups are not supported
pattern too large: more than 20000 states
```
原因取自左侧第一个违规位置：`unclosed group`、`unmatched )`、`unclosed character class`、`nothing to repeat`、`malformed repetition`、`repetition minimum exceeds maximum`、`invalid class range`、`trailing backslash`、`unsupported escape \X`（以及 `… in a class`）、`named groups are not supported`、`lookaround is not supported`、`flags are only supported at the start of the pattern`、`unsupported group syntax`、`groups nested deeper than 250`、`pattern too large: more than 20000 states`（C-395）。

<a id="regexis_matchre-regex-s-string---bool-and-regexis_full_matchre-regex-s-string---bool"></a>

### `regex.is_match(re: Regex, s: String) -> Bool` 与 `regex.is_full_match(re: Regex, s: String) -> Bool`

`is_match`：re 匹配 s 的某处。`is_full_match`：re 匹配整个 s（`regex.full_match` 是其已弃用的旧名称，计划在 epoch 16 删除）。

```almd run
import regex

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println("${regex.is_match(digits, "abc123")}")
  println("${regex.is_full_match(digits, "123")}")
  println("${regex.is_full_match(digits, "abc123")}")
}
```
```output
true
true
false
```

<a id="regexfindre-regex-s-string---optionstring-and-regexindex_ofre-regex-s-string---optionint"></a>

### `regex.find(re: Regex, s: String) -> Option[String]` 与 `regex.index_of(re: Regex, s: String) -> Option[Int]`

第一个匹配及其起点——使用与 `string.index_of` 相同的码点索引单位。

```almd run
import regex

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println("${regex.find(digits, "abc123def")} ${regex.find(digits, "abcdef")}")
  println("${regex.index_of(digits, "日本123")} ${regex.index_of(digits, "abc")}")
}
```
```output
some("123") none
some(2) none
```

### `regex.find_all(re: Regex, s: String) -> List[String]`

按顺序返回所有不重叠匹配。

```almd run
import regex

fn main() -> Unit = {
  println("${regex.find_all(regex.new("[0-9]+"), "a1b22c333")}")
}
```
```output
["1", "22", "333"]
```

### `regex.replace` / `replace_first` / `replace_with` / `replace_first_with`

替换为 {全部，第一个} × {字面文本，基于匹配的函数}。字面文本原样插入，不解释 `$1`。函数接收 `[whole, group 1, ...]`，未参与的组为 `none`。

```almd run
import regex

fn main() -> Unit = {
  let num = regex.new("([0-9])([0-9])?")
  println(regex.replace(num, "a1b22", "#"))
  println(regex.replace_first(num, "a1b22", "$1"))
  println(regex.replace_with(num, "a1b22", (g) => "<" + ((list.get(g, 2) ?? none) ?? "-") + ">"))
  println(regex.replace_first_with(num, "a1b22", (g) => "[" + ((list.get(g, 0) ?? none) ?? "") + "]"))
}
```
```output
a#b#
a$1b22
a<->b<2>
a[1]b22
```

### `regex.split(re: Regex, s: String) -> List[String]`

匹配之间的文本；无匹配时为 `[s]`。

```almd run
import regex

fn main() -> Unit = {
  println("${regex.split(regex.new("[,;]"), "a,b;c")}")
}
```
```output
["a", "b", "c"]
```

<a id="regexcapturesre-regex-s-string---optionlistoptionstring-and-regexcaptures_all"></a>

### `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` 与 `regex.captures_all`

第一个匹配的 `[whole, group 1, ...]`，没有匹配则 `none`。索引 0 是整个匹配，始终为 `some`；未参与的组为 `none`（R-regex-08），组匹配到空字符串则不是这种情况。`captures_all` 按顺序为每个匹配返回同样形式的列表。

```almd run
import regex

fn show(cs: List[String?]) -> String = "[" + (cs |> list.map((c) => c ?? "none") |> list.join(",")) + "]"

fn main() -> Unit = {
  println("${regex.captures(regex.new("(\\w+)@(\\w+)"), "user@host") |> option.map((cs) => show(cs))}")
  println("${regex.captures(regex.new("(x)?(y)"), "y") |> option.map((cs) => show(cs))}")
  println("${regex.captures(regex.new("(a*)b"), "b") |> option.map((cs) => show(cs))}")
  println("${regex.captures(regex.new("(z)"), "abc") |> option.map((cs) => show(cs))}")
  println("${regex.captures_all(regex.new("(\\d)(\\d)?"), "1 23") |> list.map((cs) => show(cs))}")
}
```
```output
some("[user@host,user,host]")
some("[y,none,y]")
some("[b,]")
none
["[1,1,none]", "[23,2,3]"]
```

### `regex.escape(s: String) -> String`

转义 `s` 的所有元字符，使从该文本构造的模式匹配该文本。

```almd run
import regex

fn main() -> Unit = {
  let user = "1+1=2?"
  println(regex.escape(user))
  println("${regex.is_match(regex.new(regex.escape(user)), "is 1+1=2?")}")
}
```
```output
1\+1=2\?
true
```

<a id="supported-syntax"></a>

## 支持的语法

匹配取最左侧；从该位置开始的匹配中，按以下顺序取第一个：分支从左到右，贪婪量词最长优先，懒惰量词最短优先；导致模式其余部分失败的组或量词以其下一选择重试。重复组的捕获取最后一次实例；没有匹配任何内容的实例也计入，但会结束重复。原生和 wasm 逐字节一致。

无论何种模式，匹配时间都与模式状态数乘以目标字符串长度成正比（#4019、#4083），被接受的模式最多有 20,000 个状态。这限制的是匹配工作，不是调用中的所有工作（如替换回调或输出构建）：`(a+)+$` 或 `(a|aa)*b` 对 100,000 个字符会立即返回；最坏情况——每个字符处所有状态都活跃，即用 `a{0,9000}b` 匹配 100,000 个 `a`——原生约需 8 秒，wasm 约一分钟。每次调用先以 4 ×（模式长度 + 1）×（目标字符串长度 + 1）的步数预算运行回溯匹配器；普通模式下它更快。预算用尽时，Pike VM（具有相同优先规则、每状态最多一个线程的 Thompson NFA）给出相同答案。模式状态数为其 VM 程序指令数——约等于模式长度，每个计数重复按次数贡献其重复内容的副本——再乘以可匹配空内容的重复组最深嵌套层数加一。`a{0,9999}b` 恰好有 20,000 个状态；嵌套次数相乘，因此 `((a){200}){200}` 约有 120,000 个状态，会被拒绝。计数在不展开模式的情况下完成。

| 语法 | 含义 |
|---|---|
| `a`, `\.`, `\\` | 字面字符（用 `\` 转义元字符） |
| `.` | 除 `\n` 外的任意字符 |
| `[abc]`, `[a-z]`, `[^0-9]` | 字符类、范围、否定字符类（开头的 `]` 是成员） |
| `\d` `\w` `\s` 和 `\D` `\W` `\S` | 数字、单词字符（`[A-Za-z0-9_]`）、空白（`[ \t\n\v\f\r]`，ASCII 空白集合）及其否定；类内支持 `\d` `\w` `\s` `\n` `\t` |
| `\n` `\t` `\r` | 控制字符 |
| `^`, `$` | 字符串开头／结尾 |
| `\b`, `\B` | 单词边界／非边界（ASCII 单词字符） |
| `x|y` | 分支选择 |
| `(x)`, `(?:x)` | 捕获／非捕获组，最多嵌套 250 层 |
| `x*` `x+` `x?` `x{n}` `x{n,}` `x{n,m}` | 贪婪量词 |
| `x*?` `x+?` `x??` `x{n,m}?` | 懒惰量词 |
| 模式开头的 `(?i)`, `(?m)`, `(?im)` | `i`：ASCII 字母匹配任意大小写；`m`：`^` 和 `$` 也匹配 `\n` 后／前的位置 |

拒绝（C-395）：环视、反向引用及上面未列出的字母或数字转义、命名组、非开头位置的标志、不构成完整量词的 `{`、没有可重复对象的量词，以及超过 20,000 个状态。



<a id="signature-index-16-functions"></a>

## 签名索引（16 个函数）

```
// The Regex for `pattern`; aborts with `Error: invalid regex: <reason>` when it is refused.
// @since unreleased
regex.new(pattern: String) -> Regex

// The Regex for `pattern`, or err(reason) when it is refused.
// @since unreleased
regex.compile(pattern: String) -> Result[Regex, String]

// re matches somewhere in s.
// @since 0.6.0 or earlier
regex.is_match(re: Regex, s: String) -> Bool

// re matches the whole of s.
// @since unreleased
regex.is_full_match(re: Regex, s: String) -> Bool

// re matches the whole of s (renamed regex.is_full_match).
// @since 0.6.0 or earlier
regex.full_match(re: Regex, s: String) -> Bool   (deprecated — use regex.is_full_match)

// Leftmost match of re in s, or none.
// @since 0.5.0 or earlier
regex.find(re: Regex, s: String) -> Option[String]

// All non-overlapping matches, in order.
// @since 0.5.0 or earlier
regex.find_all(re: Regex, s: String) -> List[String]

// Code-point index where the leftmost match starts, or none.
// @since unreleased
regex.index_of(re: Regex, s: String) -> Option[Int]

// All matches replaced by literal rep (`$1` is not read).
// @since 0.5.0 or earlier
regex.replace(re: Regex, s: String, rep: String) -> String

// First match replaced by literal rep.
// @since 0.5.0 or earlier
regex.replace_first(re: Regex, s: String, rep: String) -> String

// All matches replaced by f([whole, group1, ...]).
// @since unreleased
regex.replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String

// First match replaced by f([whole, group1, ...]).
// @since unreleased
regex.replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String

// Text between matches; [s] if no match.
// @since 0.5.0 or earlier
regex.split(re: Regex, s: String) -> List[String]

// [whole, group1, ...] of the first match, or none; a group that took no part is none.
// @since 0.5.0 or earlier
regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]

// [whole, group1, ...] of every match, in order.
// @since unreleased
regex.captures_all(re: Regex, s: String) -> List[List[Option[String]]]

// s with every regex metacharacter escaped, so it matches itself.
// @since unreleased
regex.escape(s: String) -> String
```

<a id="type-index-1-types"></a>

## 类型索引（1 个类型）

```
// A pattern accepted by `regex.new` or `regex.compile`.
// @since unreleased
type regex.Regex = { pattern: String }
```



## 索引注释的说明译文

- `regex.new(pattern: String) -> Regex` — `pattern` 的 Regex；拒绝时以 `Error: invalid regex: <reason>` 中止。 (@since unreleased)
- `regex.compile(pattern: String) -> Result[Regex, String]` — `pattern` 的 Regex；拒绝时为 err(reason)。 (@since unreleased)
- `regex.is_match(re: Regex, s: String) -> Bool` — re 匹配 s 的某处。 (@since 0.6.0 or earlier)
- `regex.is_full_match(re: Regex, s: String) -> Bool` — re 匹配整个 s。 (@since unreleased)
- `regex.full_match(re: Regex, s: String) -> Bool   (deprecated — use regex.is_full_match)` — re 匹配整个 s（已更名为 regex.is_full_match）。 (@since 0.6.0 or earlier)
- `regex.find(re: Regex, s: String) -> Option[String]` — re 在 s 中最左侧的匹配，没有则 none。 (@since 0.5.0 or earlier)
- `regex.find_all(re: Regex, s: String) -> List[String]` — 按顺序返回所有不重叠匹配。 (@since 0.5.0 or earlier)
- `regex.index_of(re: Regex, s: String) -> Option[Int]` — 最左侧匹配起点的码点索引，没有则 none。 (@since unreleased)
- `regex.replace(re: Regex, s: String, rep: String) -> String` — 用字面文本 rep 替换所有匹配（不解释 `$1`）。 (@since 0.5.0 or earlier)
- `regex.replace_first(re: Regex, s: String, rep: String) -> String` — 用字面文本 rep 替换第一个匹配。 (@since 0.5.0 or earlier)
- `regex.replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 用 f([whole, group1, ...]) 替换所有匹配。 (@since unreleased)
- `regex.replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 用 f([whole, group1, ...]) 替换第一个匹配。 (@since unreleased)
- `regex.split(re: Regex, s: String) -> List[String]` — 匹配之间的文本；无匹配则 [s]。 (@since 0.5.0 or earlier)
- `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` — 第一个匹配的 [whole, group1, ...]，没有则 none；未参与的组为 none。 (@since 0.5.0 or earlier)
- `regex.captures_all(re: Regex, s: String) -> List[List[Option[String]]]` — 按顺序返回每个匹配的 [whole, group1, ...]。 (@since unreleased)
- `regex.escape(s: String) -> String` — 转义 s 的所有正则元字符，使其匹配自身。 (@since unreleased)
- `type regex.Regex = { pattern: String }` — 由 `regex.new` 或 `regex.compile` 接受的模式。 (@since unreleased)
