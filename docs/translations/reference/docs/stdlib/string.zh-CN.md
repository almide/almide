# string

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](string.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](string.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](string.zh-CN.md)



> 原文：[docs/stdlib/string.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/string.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名及代码内注释保留原文，在代码外附上说明译文。 原文 slice 说明称结束索引可选，但标题签名将其列为必需参数；count_matches 的参数名在正文标题为 sub、索引为 needle。本译文不修改这些差异。大小写转换不是一般标识符安全性保证，也不用于身份比较。

字符串操作。自动导入。

### `string.trim(s: String) -> String`

移除开头和结尾的空白。

```almd run
fn main() -> Unit = {
  println(string.trim("  hello  "))
}
```
```output
hello
```

### `string.split(s: String, sep: String) -> List[String]`

按分隔符将字符串拆为子字符串列表。

```almd run
fn main() -> Unit = {
  println("${string.split("a,b,c", ",")}")
}
```
```output
["a", "b", "c"]
```

### `string.len(s: String) -> Int`

返回字符串中的字符数。

```almd run
fn main() -> Unit = {
  println(int.to_string(string.len("hello")))
}
```
```output
5
```

### `string.contains(s: String, needle: String) -> Bool`

检查字符串是否包含子字符串。

```almd run
fn main() -> Unit = {
  println("${string.contains("hello world", "world")}")
}
```
```output
true
```

### `string.starts_with(s: String, prefix: String) -> Bool`

检查字符串是否以指定前缀开头。

```almd run
fn main() -> Unit = {
  println("${string.starts_with("hello", "hel")}")
}
```
```output
true
```

### `string.ends_with(s: String, suffix: String) -> Bool`

检查字符串是否以指定后缀结尾。

```almd run
fn main() -> Unit = {
  println("${string.ends_with("hello", "llo")}")
}
```
```output
true
```

### `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]`

提取 UTF-8 字节半开区间。端点为负、顺序颠倒、越界或不在码点边界时返回 `none`。有效空区间返回 `some("")`。仅复制请求的字节，不扫描字符串的其余部分。`string.byte_slice("aé🦀z", 1, 3)` 返回 `some("é")`。

### `string.slice(s: String, start: Int, end: Int) -> String`

按码点起始索引和可选结束索引提取子字符串。

```almd run
fn main() -> Unit = {
  println(string.slice("hello", 1, 4))
}
```
```output
ell
```

### `string.pad_start(s: String, n: Int, fill: String) -> String`

在字符串左侧填充，达到目标长度。

```almd run
fn main() -> Unit = {
  println(string.pad_start("42", 5, "0"))
}
```
```output
00042
```

### `string.capitalize(s: String) -> String`

将字符串首字符大写化。首字符使用与 `string.to_upper` 相同的规则，因此若非 ASCII 首字符的大写形式会包含 ASCII（`ı`、`ß`、`ﬁ`），就保持原样。

```almd run
fn main() -> Unit = {
  println(string.capitalize("hello"))
}
```
```output
Hello
```

### `string.to_upper(s: String) -> String`

使用 Unicode 默认的（与区域设置无关的）完整大小写映射，将所有字符转为大写；可能使字符串变长。

有一个有意的例外：若非 ASCII 字符的大写形式会包含 ASCII 字符，则保持不变，避免文本折叠为 ASCII。`string.to_upper("adm\u{131}n")` 是 `"ADMıN"`，不是 `"ADMIN"`，而 `ß`、`ſ` 和连字 `ﬀ`–`ﬆ` 保持原样。Rust、Python 和 JavaScript 会折叠这些字符（Go 会折叠 `ı` 和 `ſ`）；Almide 不会，因此通过原始文本检查的名称不会随后变成 `"ADMIN"`。此映射与区域设置无关，不用于身份比较：应将标识符作为 ASCII 比较，或显式规范化。

```almd run
fn main() -> Unit = {
  println(string.to_upper("hello"))
}
```
```output
HELLO
```

### `string.to_lower(s: String) -> String`

使用 Unicode 默认的（与区域设置无关的）完整大小写映射，将所有字符转为小写，包括 `Σ` 的词尾 `ς` 规则。

与 `string.to_upper` 一样，若非 ASCII 字符的小写形式会包含 ASCII 字符，则保持不变：`string.to_lower("\u{212A}ey")`（开尔文符号）是 `"\u{212A}ey"`，不是 `"key"`，而 `İ`（U+0130）仍为 `İ`。此映射与区域设置无关，不用于身份比较。

```almd run
fn main() -> Unit = {
  println(string.to_lower("HELLO"))
}
```
```output
hello
```

### `string.replace(s: String, from: String, to: String) -> String`

替换子字符串的所有出现。

```almd run
fn main() -> Unit = {
  println(string.replace("aabbcc", "bb", "XX"))
}
```
```output
aaXXcc
```

### `string.get(s: String, i: Int) -> Option[String]`

获取给定码点索引处的字符，越界则为 none。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.get("hello", 1)))
  println(show(string.get("hello", 9)))
}
```
```output
some("e")
none
```

### `string.lines(s: String) -> List[String]`

将字符串拆分为行。

```almd run
fn main() -> Unit = {
  println("${string.lines("a\nb\nc")}")
}
```
```output
["a", "b", "c"]
```

### `string.to_list(s: String) -> List[String]`

将字符串拆分为单个字符。

```almd run
fn main() -> Unit = {
  println("${string.to_list("abc")}")
}
```
```output
["a", "b", "c"]
```

### `string.index_of(s: String, needle: String) -> Option[Int]`

查找子字符串第一次出现的码点索引，未找到则为 none。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.index_of("hello", "ll")))
  println(show(string.index_of("hello", "z")))
}
```
```output
some(2)
none
```

### `string.repeat(s: String, n: Int) -> String`

将字符串重复 n 次。

```almd run
fn main() -> Unit = {
  println(string.repeat("ab", 3))
}
```
```output
ababab
```

### `string.is_digit(s: String) -> Bool`

检查所有字符是否都是 ASCII 数字。

```almd run
fn main() -> Unit = {
  println("${string.is_digit("123")}")
}
```
```output
true
```

### `string.is_alpha(s: String) -> Bool`

检查所有字符是否都是字母。

```almd run
fn main() -> Unit = {
  println("${string.is_alpha("abc")}")
}
```
```output
true
```

### `string.is_alphanumeric(s: String) -> Bool`

检查所有字符是否都是字母或数字。

```almd run
fn main() -> Unit = {
  println("${string.is_alphanumeric("abc123")}")
}
```
```output
true
```

### `string.is_whitespace(s: String) -> Bool`

检查所有字符是否都是空白。

```almd run
fn main() -> Unit = {
  println("${string.is_whitespace("  ")}")
}
```
```output
true
```

### `string.is_upper(s: String) -> Bool`

至少存在一个字母且所有字母都为大写时为 true；忽略非字母字符。

```almd run
fn main() -> Unit = {
  println("${string.is_upper("ABC")}")
}
```
```output
true
```

### `string.is_lower(s: String) -> Bool`

至少存在一个字母且所有字母都为小写时为 true；忽略非字母字符。

```almd run
fn main() -> Unit = {
  println("${string.is_lower("abc")}")
}
```
```output
true
```

### `string.to_codepoint(s: String) -> Option[Int]`

字符串恰含一个码点时返回其 Unicode 标量值；空字符串或包含多个码点时返回 `none`。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.to_codepoint("A")))
  println(show(string.to_codepoint("")))
}
```
```output
some(65)
none
```

### `string.from_codepoint(n: Int) -> String?`

由 Unicode 码点创建单字符字符串；n 不是 Unicode 标量值（负数、代理项或超过 U+10FFFF）时为 none。

```almd run
fn main() -> Unit = {
  println(string.from_codepoint(65) ?? "")
}
```
```output
A
```

### `string.pad_end(s: String, n: Int, fill: String) -> String`

在字符串右侧填充，达到目标长度。

```almd run
fn main() -> Unit = {
  println(string.pad_end("hi", 5, "."))
}
```
```output
hi...
```

### `string.trim_start(s: String) -> String`

移除开头的空白。

```almd run
fn main() -> Unit = {
  println(string.trim_start("  hello"))
}
```
```output
hello
```

### `string.trim_end(s: String) -> String`

移除结尾的空白。

```almd run
fn main() -> Unit = {
  println(string.trim_end("hello  "))
}
```
```output
hello
```

### `string.count_matches(s: String, sub: String) -> Int`

统计子字符串出现次数。

```almd run
fn main() -> Unit = {
  println(int.to_string(string.count_matches("banana", "an")))
}
```
```output
2
```

### `string.is_empty(s: String) -> Bool`

检查字符串是否为空。

```almd run
fn main() -> Unit = {
  println("${string.is_empty("")}")
}
```
```output
true
```

### `string.reverse(s: String) -> String`

反转字符串中的字符顺序。

```almd run
fn main() -> Unit = {
  println(string.reverse("hello"))
}
```
```output
olleh
```

### `string.strip_prefix(s: String, prefix: String) -> Option[String]`

存在前缀则移除，未找到则返回 none。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.strip_prefix("hello", "hel")))
  println(show(string.strip_prefix("hello", "lo")))
}
```
```output
some("lo")
none
```

### `string.strip_suffix(s: String, suffix: String) -> Option[String]`

存在后缀则移除，未找到则返回 none。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.strip_suffix("hello", "llo")))
  println(show(string.strip_suffix("hello", "he")))
}
```
```output
some("he")
none
```

### `string.replace_first(s: String, from: String, to: String) -> String`

替换子字符串的首次出现。

```almd run
fn main() -> Unit = {
  println(string.replace_first("aabaa", "a", "X"))
}
```
```output
Xabaa
```

### `string.last_index_of(s: String, needle: String) -> Option[Int]`

查找子字符串最后一次出现的码点索引，未找到则为 none。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.last_index_of("abcabc", "bc")))
  println(show(string.last_index_of("abcabc", "z")))
}
```
```output
some(4)
none
```

### `string.first(s: String) -> Option[String]`

获取字符串的首字符。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.first("hello")))
  println(show(string.first("")))
}
```
```output
some("h")
none
```

### `string.last(s: String) -> Option[String]`

获取字符串的末字符。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(string.last("hello")))
  println(show(string.last("")))
}
```
```output
some("o")
none
```

### `string.take(s: String, n: Int) -> String`

取前 N 个字符。

```almd run
fn main() -> Unit = {
  println(string.take("hello", 3))
}
```
```output
hel
```

### `string.take_end(s: String, n: Int) -> String`

取后 N 个字符。

```almd run
fn main() -> Unit = {
  println(string.take_end("hello", 3))
}
```
```output
llo
```

### `string.drop(s: String, n: Int) -> String`

丢弃前 N 个字符。

```almd run
fn main() -> Unit = {
  println(string.drop("hello", 2))
}
```
```output
llo
```

### `string.drop_end(s: String, n: Int) -> String`

丢弃后 N 个字符。

```almd run
fn main() -> Unit = {
  println(string.drop_end("hello", 2))
}
```
```output
hel
```

> 原文生成签名索引的固定副本（本译文不是生成目标）。

<a id="signature-index-87-functions"></a>

## 签名索引（87 个函数）

```
// s without leading/trailing whitespace.
// @since 0.5.0 or earlier
string.trim(s: String) -> String

// Pieces around sep; empty pieces kept.
// @since 0.5.0 or earlier
string.split(s: String, sep: String) -> List[String]

// Split at the FIRST occurrence of `sep`: `(before, after)`, none ⇔ `sep` absent.
// @since 0.57.0
string.split_once(s: String, sep: String) -> Option[(String, String)]

// Codepoint count (not bytes).
// @since 0.5.0 or earlier
string.len(s: String) -> Int

// Alias of string.len.
// @since 0.19.2 or earlier
string.length(s: String) -> Int   (deprecated — use string.len)

// needle occurs in s; true for empty needle.
// @since 0.6.0 or earlier
string.contains(s: String, needle: String) -> Bool

// True if a and b hold the same UTF-8 bytes, in time that depends only on
// their lengths: no early exit on the first differing byte. Use it to compare
// a token or signature header; `==` returns at the first difference. A length
// mismatch is false at once (lengths are public).
// @since unreleased
string.is_equal_constant_time(a: String, b: String) -> Bool

// s begins with prefix; "" always matches.
// @since 0.6.0 or earlier
string.starts_with(s: String, prefix: String) -> Bool

// s ends with suffix; "" always matches.
// @since 0.6.0 or earlier
string.ends_with(s: String, suffix: String) -> Bool

// Codepoints [start, end), clamped to s.
// @since 0.5.0 or earlier
string.slice(s: String, start: Int, end: Int) -> String

// Byte range [start, end); none if bad or mid-char.
// @since 0.63.0
string.byte_slice(s: String, start: Int, end: Int) -> Option[String]

// Left-padded to n chars with fill; never truncates.
// @since 0.6.1 or earlier
string.pad_start(s: String, n: Int, fill: String) -> String

// First char uppercased as to_upper does, rest untouched.
// @since 0.5.13 or earlier
string.capitalize(s: String) -> String

// Unicode uppercase; never folds non-ASCII into ASCII; not for identity.
// @since 0.5.0 or earlier
string.to_upper(s: String) -> String

// Unicode lowercase; never folds non-ASCII into ASCII; not for identity.
// @since 0.5.0 or earlier
string.to_lower(s: String) -> String

// Every from replaced by to.
// @since 0.5.0 or earlier
string.replace(s: String, from: String, to: String) -> String

// Codepoint at i, or none if out of range.
// @since 0.6.1 or earlier
string.get(s: String, i: Int) -> Option[String]

// Split on \n or \r\n; endings dropped.
// @since 0.5.0 or earlier
string.lines(s: String) -> List[String]

// Each codepoint as a one-char string.
// @since 0.5.0 or earlier
string.to_list(s: String) -> List[String]

// Codepoint index of first needle, or none.
// @since 0.5.0 or earlier
string.index_of(s: String, needle: String) -> Option[Int]

// s repeated n times; empty for n <= 0.
// @since 0.5.0 or earlier
string.repeat(s: String, n: Int) -> String

// All ASCII 0-9; false for empty s.
// @since 0.6.0 or earlier
string.is_digit(s: String) -> Bool

// All Unicode letters; false for empty s.
// @since 0.6.0 or earlier
string.is_alpha(s: String) -> Bool

// All letters/digits; false for empty s.
// @since 0.6.0 or earlier
string.is_alphanumeric(s: String) -> Bool

// All whitespace; true for empty s.
// @since 0.6.0 or earlier
string.is_whitespace(s: String) -> Bool

// Has letters, all uppercase; others ignored.
// @since 0.6.0 or earlier
string.is_upper(s: String) -> Bool

// Has letters, all lowercase; others ignored.
// @since 0.6.0 or earlier
string.is_lower(s: String) -> Bool

// Scalar value of s when s is exactly one codepoint; none otherwise.
// @since 0.6.0 or earlier
string.to_codepoint(s: String) -> Option[Int]

// One-char string of n; none if n is not a Unicode scalar value.
// @since 0.6.0 or earlier
string.from_codepoint(n: Int) -> Option[String]

// Right-padded to n chars with fill; never truncates.
// @since 0.6.1 or earlier
string.pad_end(s: String, n: Int, fill: String) -> String

// s without leading whitespace.
// @since 0.5.0 or earlier
string.trim_start(s: String) -> String

// s without trailing whitespace.
// @since 0.5.0 or earlier
string.trim_end(s: String) -> String

// Non-overlapping needle count; len+1 if needle empty.
// @since unreleased
string.count_matches(s: String, needle: String) -> Int

// Number of characters for which f is true.
// @since 0.5.0 or earlier
string.count(s: String, f: (String) -> Bool) -> Int

// True only for ""; " " is not empty.
// @since 0.6.0 or earlier
string.is_empty(s: String) -> Bool

// Codepoints reversed; not grapheme-aware.
// @since 0.5.0 or earlier
string.reverse(s: String) -> String

// s without prefix, or none if s lacks it.
// @since 0.5.0 or earlier
string.strip_prefix(s: String, prefix: String) -> Option[String]

// s without suffix, or none if s lacks it.
// @since 0.5.0 or earlier
string.strip_suffix(s: String, suffix: String) -> Option[String]

// First from replaced by to, if any.
// @since 0.5.0 or earlier
string.replace_first(s: String, from: String, to: String) -> String

// Codepoint index of last needle, or none.
// @since 0.5.0 or earlier
string.last_index_of(s: String, needle: String) -> Option[Int]

// Leading codepoint, or none if empty.
// @since 0.7.0 or earlier
string.first(s: String) -> Option[String]

// Final codepoint, or none if empty.
// @since 0.7.0 or earlier
string.last(s: String) -> Option[String]

// First n codepoints; "" if n < 0.
// @since 0.7.0 or earlier
string.take(s: String, n: Int) -> String

// Last n codepoints; "" if n < 0.
// @since 0.7.0 or earlier
string.take_end(s: String, n: Int) -> String

// s minus first n codepoints; s if n < 0.
// @since 0.7.0 or earlier
string.drop(s: String, n: Int) -> String

// s minus last n codepoints; s if n < 0.
// @since 0.7.0 or earlier
string.drop_end(s: String, n: Int) -> String

// Appends x to s in place.
// @since 0.23.3 or earlier
string.push(mut s: String, x: String) -> Unit

// Removes and returns the last character; none if s is empty.
// @since unreleased
string.pop(mut s: String) -> Option[String]

// Empties s in place.
// @since 0.23.3 or earlier
string.clear(mut s: String) -> Unit

// Negative if a sorts before b, 0 if equal, positive after; codepoint order.
// @since unreleased
string.compare(a: String, b: String) -> Int

// UTF-8 byte count (string.len counts codepoints).
// @since unreleased
string.byte_len(s: String) -> Int

// UTF-16 code units; a codepoint past U+FFFF is a surrogate pair.
// @since unreleased
string.to_utf16(s: String) -> List[Int]

// String from UTF-16 code units; err on an unpaired surrogate or a unit outside 0..65535.
// @since unreleased
string.from_utf16(xs: List[Int]) -> Result[String, String]

// The characters of s for which f is true.
// @since unreleased
string.filter(s: String, f: (String) -> Bool) -> String

// f folded over the characters of s, left to right.
// @since unreleased
string.fold(s: String, init: B, f: (B, String) -> B) -> B

// s with x inserted before character i; i outside 0..=len aborts.
// @since unreleased
string.insert(s: String, i: Int, x: String) -> String

// s without character i; i outside 0..len aborts.
// @since unreleased
string.remove_at(s: String, i: Int) -> String

// The empty string.
// @since unreleased
string.new() -> String

// The strings of xs joined with nothing between.
// @since unreleased
string.from_list(xs: List[String]) -> String

// (index, character) pairs from 0.
// @since unreleased
string.enumerate(s: String) -> List[(Int, String)]

// Characters paired with ys; stops at the shorter.
// @since unreleased
string.zip(s: String, ys: List[B]) -> List[(String, B)]

// f over characters paired with ys; stops at the shorter.
// @since unreleased
string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]

// The some results of f over the characters, in order.
// @since unreleased
string.filter_map(s: String, f: (String) -> Option[B]) -> List[B]

// The first some result of f over the characters, or none.
// @since unreleased
string.find_map(s: String, f: (String) -> Option[B]) -> Option[B]

// f folded over the characters from the first; none if s is empty.
// @since unreleased
string.reduce(s: String, f: (String, String) -> String) -> Option[String]

// The running fold over the characters, init excluded.
// @since unreleased
string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]

// (characters where f is true, the rest), each in order.
// @since unreleased
string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])

// Characters grouped by f's key, each group in order.
// @since unreleased
string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]

// The characters in codepoint order.
// @since unreleased
string.sort(s: String) -> List[String]

// The characters ordered by f's key; equal keys keep their order.
// @since unreleased
string.sort_by(s: String, f: (String) -> B) -> List[String]

// The characters ordered by cmp (negative = first argument first); stable.
// @since unreleased
string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]

// The smallest character in codepoint order; none if empty.
// @since unreleased
string.min(s: String) -> Option[String]

// The largest character in codepoint order; none if empty.
// @since unreleased
string.max(s: String) -> Option[String]

// The first character with the smallest key; none if empty.
// @since unreleased
string.min_by(s: String, f: (String) -> K) -> Option[String]

// The last character with the largest key; none if empty.
// @since unreleased
string.max_by(s: String, f: (String) -> K) -> Option[String]

// The first smallest character by cmp; none if empty.
// @since unreleased
string.min_with(s: String, cmp: (String, String) -> Int) -> Option[String]

// The last largest character by cmp; none if empty.
// @since unreleased
string.max_with(s: String, cmp: (String, String) -> Int) -> Option[String]

// Index of the first character where f is true, or none.
// @since unreleased
string.find_index(s: String, f: (String) -> Bool) -> Option[Int]

// The longest prefix whose characters all satisfy f.
// @since unreleased
string.take_while(s: String, f: (String) -> Bool) -> String

// s after the longest prefix whose characters all satisfy f.
// @since unreleased
string.drop_while(s: String, f: (String) -> Bool) -> String

// Runs of n characters, the last may be shorter; n <= 0 aborts.
// @since unreleased
string.chunk(s: String, n: Int) -> List[String]

// Every run of n consecutive characters; [] if n > len; n <= 0 aborts.
// @since unreleased
string.window(s: String, n: Int) -> List[String]

// (first n characters, the rest); n past the end gives (s, ""), n < 0 gives ("", s).
// @since unreleased
string.split_at(s: String, n: Int) -> (String, String)

// s with each run of equal adjacent characters collapsed to one.
// @since unreleased
string.dedup(s: String) -> String

// The characters of s with sep between each pair.
// @since unreleased
string.intersperse(s: String, sep: String) -> String

// The first occurrence of each character, in order.
// @since unreleased
string.unique(s: String) -> String

// The first character of each key f gives, in order.
// @since unreleased
string.unique_by(s: String, f: (String) -> K) -> String
```



## 索引注释的说明译文

- `string.trim(s: String) -> String` — 移除 s 开头/结尾的空白。 (@since 0.5.0 or earlier)
- `string.split(s: String, sep: String) -> List[String]` — sep 周围的片段；保留空片段。 (@since 0.5.0 or earlier)
- `string.split_once(s: String, sep: String) -> Option[(String, String)]` — 在 `sep` 首次出现处拆分为 `(before, after)`；当且仅当不存在 `sep` 时为 none。 (@since 0.57.0)
- `string.len(s: String) -> Int` — 码点数（不是字节数）。 (@since 0.5.0 or earlier)
- `string.length(s: String) -> Int   (deprecated — use string.len)` — string.len 的别名。 (@since 0.19.2 or earlier)
- `string.contains(s: String, needle: String) -> Bool` — needle 是否出现在 s 中；空 needle 为 true。 (@since 0.6.0 or earlier)
- `string.is_equal_constant_time(a: String, b: String) -> Bool` — a 与 b 含相同 UTF-8 字节时为 true，时间仅取决于长度：不在首个不同字节处提前退出。用于比较令牌或签名头；`==` 在首处差异即返回。长度不同时立即为 false（长度是公开信息）。 (@since unreleased)
- `string.starts_with(s: String, prefix: String) -> Bool` — s 以 prefix 开头；"" 始终匹配。 (@since 0.6.0 or earlier)
- `string.ends_with(s: String, suffix: String) -> Bool` — s 以 suffix 结尾；"" 始终匹配。 (@since 0.6.0 or earlier)
- `string.slice(s: String, start: Int, end: Int) -> String` — 码点区间 [start, end)，限制在 s 范围内。 (@since 0.5.0 or earlier)
- `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]` — 字节区间 [start, end)；无效或落在字符中间则为 none。 (@since 0.63.0)
- `string.pad_start(s: String, n: Int, fill: String) -> String` — 用 fill 左填充至 n 字符；不截断。 (@since 0.6.1 or earlier)
- `string.capitalize(s: String) -> String` — 按 to_upper 的规则大写化首字符，其余不变。 (@since 0.5.13 or earlier)
- `string.to_upper(s: String) -> String` — Unicode 大写；不将非 ASCII 折叠为 ASCII；不用于身份比较。 (@since 0.5.0 or earlier)
- `string.to_lower(s: String) -> String` — Unicode 小写；不将非 ASCII 折叠为 ASCII；不用于身份比较。 (@since 0.5.0 or earlier)
- `string.replace(s: String, from: String, to: String) -> String` — 将所有 from 替换为 to。 (@since 0.5.0 or earlier)
- `string.get(s: String, i: Int) -> Option[String]` — 索引 i 处的码点，越界为 none。 (@since 0.6.1 or earlier)
- `string.lines(s: String) -> List[String]` — 按 \n 或 \r\n 拆分；去除行尾。 (@since 0.5.0 or earlier)
- `string.to_list(s: String) -> List[String]` — 将每个码点表示为单字符字符串。 (@since 0.5.0 or earlier)
- `string.index_of(s: String, needle: String) -> Option[Int]` — 首次 needle 的码点索引，或 none。 (@since 0.5.0 or earlier)
- `string.repeat(s: String, n: Int) -> String` — s 重复 n 次；n <= 0 时为空。 (@since 0.5.0 or earlier)
- `string.is_digit(s: String) -> Bool` — 全部为 ASCII 0-9；空 s 为 false。 (@since 0.6.0 or earlier)
- `string.is_alpha(s: String) -> Bool` — 全部为 Unicode 字母；空 s 为 false。 (@since 0.6.0 or earlier)
- `string.is_alphanumeric(s: String) -> Bool` — 全部为字母/数字；空 s 为 false。 (@since 0.6.0 or earlier)
- `string.is_whitespace(s: String) -> Bool` — 全部为空白；空 s 为 true。 (@since 0.6.0 or earlier)
- `string.is_upper(s: String) -> Bool` — 存在字母且全部大写；忽略其他字符。 (@since 0.6.0 or earlier)
- `string.is_lower(s: String) -> Bool` — 存在字母且全部小写；忽略其他字符。 (@since 0.6.0 or earlier)
- `string.to_codepoint(s: String) -> Option[Int]` — s 恰有一个码点时的标量值；否则为 none。 (@since 0.6.0 or earlier)
- `string.from_codepoint(n: Int) -> Option[String]` — n 对应的单字符字符串；n 不是 Unicode 标量值时为 none。 (@since 0.6.0 or earlier)
- `string.pad_end(s: String, n: Int, fill: String) -> String` — 用 fill 右填充至 n 字符；不截断。 (@since 0.6.1 or earlier)
- `string.trim_start(s: String) -> String` — 移除 s 开头的空白。 (@since 0.5.0 or earlier)
- `string.trim_end(s: String) -> String` — 移除 s 结尾的空白。 (@since 0.5.0 or earlier)
- `string.count_matches(s: String, needle: String) -> Int` — 不重叠的 needle 数量；空 needle 为 len+1。 (@since unreleased)
- `string.count(s: String, f: (String) -> Bool) -> Int` — f 为 true 的字符数。 (@since 0.5.0 or earlier)
- `string.is_empty(s: String) -> Bool` — 仅 "" 为 true；" " 不为空。 (@since 0.6.0 or earlier)
- `string.reverse(s: String) -> String` — 反转码点；不考虑字素。 (@since 0.5.0 or earlier)
- `string.strip_prefix(s: String, prefix: String) -> Option[String]` — 移除 s 的 prefix，不存在则为 none。 (@since 0.5.0 or earlier)
- `string.strip_suffix(s: String, suffix: String) -> Option[String]` — 移除 s 的 suffix，不存在则为 none。 (@since 0.5.0 or earlier)
- `string.replace_first(s: String, from: String, to: String) -> String` — 若有 from，将首次出现替换为 to。 (@since 0.5.0 or earlier)
- `string.last_index_of(s: String, needle: String) -> Option[Int]` — 最后一次 needle 的码点索引，或 none。 (@since 0.5.0 or earlier)
- `string.first(s: String) -> Option[String]` — 首码点；空则为 none。 (@since 0.7.0 or earlier)
- `string.last(s: String) -> Option[String]` — 末码点；空则为 none。 (@since 0.7.0 or earlier)
- `string.take(s: String, n: Int) -> String` — 前 n 个码点；n < 0 时为 ""。 (@since 0.7.0 or earlier)
- `string.take_end(s: String, n: Int) -> String` — 后 n 个码点；n < 0 时为 ""。 (@since 0.7.0 or earlier)
- `string.drop(s: String, n: Int) -> String` — 移除 s 前 n 个码点；n < 0 时为 s。 (@since 0.7.0 or earlier)
- `string.drop_end(s: String, n: Int) -> String` — 移除 s 后 n 个码点；n < 0 时为 s。 (@since 0.7.0 or earlier)
- `string.push(mut s: String, x: String) -> Unit` — 原地将 x 追加到 s。 (@since 0.23.3 or earlier)
- `string.pop(mut s: String) -> Option[String]` — 移除并返回末字符；s 为空则为 none。 (@since unreleased)
- `string.clear(mut s: String) -> Unit` — 原地清空 s。 (@since 0.23.3 or earlier)
- `string.compare(a: String, b: String) -> Int` — 按码点顺序，a 在 b 前为负，相等为 0，在后为正。 (@since unreleased)
- `string.byte_len(s: String) -> Int` — UTF-8 字节数（string.len 计算码点）。 (@since unreleased)
- `string.to_utf16(s: String) -> List[Int]` — UTF-16 代码单元；超过 U+FFFF 的码点为代理项对。 (@since unreleased)
- `string.from_utf16(xs: List[Int]) -> Result[String, String]` — 从 UTF-16 代码单元生成字符串；未配对代理项或超出 0..65535 的单元导致 err。 (@since unreleased)
- `string.filter(s: String, f: (String) -> Bool) -> String` — s 中 f 为 true 的字符。 (@since unreleased)
- `string.fold(s: String, init: B, f: (B, String) -> B) -> B` — 用 f 从左到右折叠 s 的字符。 (@since unreleased)
- `string.insert(s: String, i: Int, x: String) -> String` — 在字符 i 前插入 x 后的 s；i 超出 0..=len 则 abort。 (@since unreleased)
- `string.remove_at(s: String, i: Int) -> String` — 移除字符 i 后的 s；i 超出 0..len 则 abort。 (@since unreleased)
- `string.new() -> String` — 空字符串。 (@since unreleased)
- `string.from_list(xs: List[String]) -> String` — 将 xs 的字符串直接连接，不插入内容。 (@since unreleased)
- `string.enumerate(s: String) -> List[(Int, String)]` — 从 0 开始的 (index, character) 对。 (@since unreleased)
- `string.zip(s: String, ys: List[B]) -> List[(String, B)]` — 字符与 ys 配对；以较短者为限。 (@since unreleased)
- `string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` — 对字符与 ys 的配对应用 f；以较短者为限。 (@since unreleased)
- `string.filter_map(s: String, f: (String) -> Option[B]) -> List[B]` — 按顺序返回对各字符应用 f 得到的 some 结果。 (@since unreleased)
- `string.find_map(s: String, f: (String) -> Option[B]) -> Option[B]` — 对各字符应用 f 的首个 some 结果，或 none。 (@since unreleased)
- `string.reduce(s: String, f: (String, String) -> String) -> Option[String]` — 从首字符开始用 f 折叠；s 为空则为 none。 (@since unreleased)
- `string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` — 对字符的逐步 fold 结果，不含 init。 (@since unreleased)
- `string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` — (f 为 true 的字符，其余字符)，各自保持顺序。 (@since unreleased)
- `string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` — 按 f 的键分组字符，各组保持顺序。 (@since unreleased)
- `string.sort(s: String) -> List[String]` — 按码点顺序排列的字符。 (@since unreleased)
- `string.sort_by(s: String, f: (String) -> B) -> List[String]` — 按 f 的键排序的字符；相同键保持顺序。 (@since unreleased)
- `string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` — 按 cmp 排序的字符（负值表示第一参数在前）；稳定。 (@since unreleased)
- `string.min(s: String) -> Option[String]` — 码点顺序最小的字符；空则为 none。 (@since unreleased)
- `string.max(s: String) -> Option[String]` — 码点顺序最大的字符；空则为 none。 (@since unreleased)
- `string.min_by(s: String, f: (String) -> K) -> Option[String]` — 具有最小键的首个字符；空则为 none。 (@since unreleased)
- `string.max_by(s: String, f: (String) -> K) -> Option[String]` — 具有最大键的末个字符；空则为 none。 (@since unreleased)
- `string.min_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — 按 cmp 最小的首个字符；空则为 none。 (@since unreleased)
- `string.max_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — 按 cmp 最大的末个字符；空则为 none。 (@since unreleased)
- `string.find_index(s: String, f: (String) -> Bool) -> Option[Int]` — f 为 true 的首字符索引，或 none。 (@since unreleased)
- `string.take_while(s: String, f: (String) -> Bool) -> String` — 所有字符均满足 f 的最长前缀。 (@since unreleased)
- `string.drop_while(s: String, f: (String) -> Bool) -> String` — 从 s 移除所有字符均满足 f 的最长前缀。 (@since unreleased)
- `string.chunk(s: String, n: Int) -> List[String]` — 每段 n 个字符，末段可更短；n <= 0 则 abort。 (@since unreleased)
- `string.window(s: String, n: Int) -> List[String]` — 所有连续 n 字符的窗口；n > len 时为 []；n <= 0 则 abort。 (@since unreleased)
- `string.split_at(s: String, n: Int) -> (String, String)` — (前 n 个字符，其余部分)；n 超出末尾为 (s, "")，n < 0 为 ("", s)。 (@since unreleased)
- `string.dedup(s: String) -> String` — 将 s 中每段相同相邻字符合并为一个。 (@since unreleased)
- `string.intersperse(s: String, sep: String) -> String` — 在 s 的每对相邻字符间插入 sep。 (@since unreleased)
- `string.unique(s: String) -> String` — 按顺序保留每个字符的首次出现。 (@since unreleased)
- `string.unique_by(s: String, f: (String) -> K) -> String` — 按顺序保留 f 给出的每个键的首个字符。 (@since unreleased)
