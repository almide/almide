# string

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](string.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](string.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](string.zh-CN.md)



> 原文：[docs/stdlib/string.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/string.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文のまま保持し、説明訳をコード外へ添える。 原文の slice 説明は終了インデックスを任意とする一方、見出しシグネチャは必須引数として記載する。count_matches の引数名も本文見出し sub と索引 needle で異なる。この訳では原文を改変しない。大小文字変換は一般的な識別子安全性の保証ではなく、同一性比較用でもない。

文字列操作。自動 import される。

### `string.trim(s: String) -> String`

先頭と末尾の空白を除去する。

```almd run
fn main() -> Unit = {
  println(string.trim("  hello  "))
}
```
```output
hello
```

### `string.split(s: String, sep: String) -> List[String]`

区切り文字で文字列を分割し、部分文字列のリストにする。

```almd run
fn main() -> Unit = {
  println("${string.split("a,b,c", ",")}")
}
```
```output
["a", "b", "c"]
```

### `string.len(s: String) -> Int`

文字列の文字数を返す。

```almd run
fn main() -> Unit = {
  println(int.to_string(string.len("hello")))
}
```
```output
5
```

### `string.contains(s: String, needle: String) -> Bool`

文字列が部分文字列を含むかを調べる。

```almd run
fn main() -> Unit = {
  println("${string.contains("hello world", "world")}")
}
```
```output
true
```

### `string.starts_with(s: String, prefix: String) -> Bool`

文字列が指定の接頭辞で始まるかを調べる。

```almd run
fn main() -> Unit = {
  println("${string.starts_with("hello", "hel")}")
}
```
```output
true
```

### `string.ends_with(s: String, suffix: String) -> Bool`

文字列が指定の接尾辞で終わるかを調べる。

```almd run
fn main() -> Unit = {
  println("${string.ends_with("hello", "llo")}")
}
```
```output
true
```

### `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]`

UTF-8 バイトの半開区間を取り出す。端点が負、逆順、範囲外、またはコードポイント境界でない場合は `none` を返す。有効な空区間は `some("")` を返す。要求したバイトだけをコピーし、残りの文字列は走査しない。`string.byte_slice("aé🦀z", 1, 3)` は `some("é")` を返す。

### `string.slice(s: String, start: Int, end: Int) -> String`

コードポイントの開始インデックスと任意の終了インデックスで部分文字列を取り出す。

```almd run
fn main() -> Unit = {
  println(string.slice("hello", 1, 4))
}
```
```output
ell
```

### `string.pad_start(s: String, n: Int, fill: String) -> String`

目標の長さに達するまで文字列の左側を埋める。

```almd run
fn main() -> Unit = {
  println(string.pad_start("42", 5, "0"))
}
```
```output
00042
```

### `string.capitalize(s: String) -> String`

文字列の最初の文字を大文字にする。最初の文字は `string.to_upper` と同じ規則で大文字化するため、大文字化後に ASCII を含む非 ASCII の先頭文字（`ı`、`ß`、`ﬁ`）はそのまま残す。

```almd run
fn main() -> Unit = {
  println(string.capitalize("hello"))
}
```
```output
Hello
```

### `string.to_upper(s: String) -> String`

Unicode の既定の（ロケール非依存の）完全な大小文字マッピングを使い、すべての文字を大文字にする。文字列が長くなることがある。

意図的な例外が一つある。大文字化後に ASCII 文字を含む非 ASCII 文字は変更せず、テキストが ASCII へ変換されないようにする。`string.to_upper("adm\u{131}n")` は `"ADMIN"` ではなく `"ADMıN"` となり、`ß`、`ſ`、合字 `ﬀ`–`ﬆ` はそのまま残る。Rust、Python、JavaScript はこれらを変換する（Go は `ı` と `ſ` を変換する）が、Almide は変換しないため、生のテキストの検査を通った名前が後で `"ADMIN"` になることはない。このマッピングはロケール非依存であり、同一性の比較用ではない。識別子は ASCII として比較するか、明示的に正規化すること。

```almd run
fn main() -> Unit = {
  println(string.to_upper("hello"))
}
```
```output
HELLO
```

### `string.to_lower(s: String) -> String`

Unicode の既定の（ロケール非依存の）完全な大小文字マッピングを使い、すべての文字を小文字にする。`Σ` に対する語末の `ς` 規則も含む。

`string.to_upper` と同様、小文字化後に ASCII 文字を含む非 ASCII 文字は変更しない。`string.to_lower("\u{212A}ey")`（ケルビン記号）は `"key"` ではなく `"\u{212A}ey"` となり、`İ`（U+0130）は `İ` のままとなる。このマッピングはロケール非依存であり、同一性の比較用ではない。

```almd run
fn main() -> Unit = {
  println(string.to_lower("HELLO"))
}
```
```output
hello
```

### `string.replace(s: String, from: String, to: String) -> String`

部分文字列のすべての出現を置き換える。

```almd run
fn main() -> Unit = {
  println(string.replace("aabbcc", "bb", "XX"))
}
```
```output
aaXXcc
```

### `string.get(s: String, i: Int) -> Option[String]`

指定されたコードポイントインデックスの文字を取得する。範囲外なら none。

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

文字列を行に分割する。

```almd run
fn main() -> Unit = {
  println("${string.lines("a\nb\nc")}")
}
```
```output
["a", "b", "c"]
```

### `string.to_list(s: String) -> List[String]`

文字列を個々の文字に分割する。

```almd run
fn main() -> Unit = {
  println("${string.to_list("abc")}")
}
```
```output
["a", "b", "c"]
```

### `string.index_of(s: String, needle: String) -> Option[Int]`

部分文字列の最初のコードポイントインデックスを探す。なければ none。

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

文字列を n 回繰り返す。

```almd run
fn main() -> Unit = {
  println(string.repeat("ab", 3))
}
```
```output
ababab
```

### `string.is_digit(s: String) -> Bool`

すべての文字が ASCII 数字かを調べる。

```almd run
fn main() -> Unit = {
  println("${string.is_digit("123")}")
}
```
```output
true
```

### `string.is_alpha(s: String) -> Bool`

すべての文字が字母かを調べる。

```almd run
fn main() -> Unit = {
  println("${string.is_alpha("abc")}")
}
```
```output
true
```

### `string.is_alphanumeric(s: String) -> Bool`

すべての文字が字母または数字かを調べる。

```almd run
fn main() -> Unit = {
  println("${string.is_alphanumeric("abc123")}")
}
```
```output
true
```

### `string.is_whitespace(s: String) -> Bool`

すべての文字が空白かを調べる。

```almd run
fn main() -> Unit = {
  println("${string.is_whitespace("  ")}")
}
```
```output
true
```

### `string.is_upper(s: String) -> Bool`

少なくとも一つの字母があり、すべての字母が大文字なら true。字母以外の文字は無視する。

```almd run
fn main() -> Unit = {
  println("${string.is_upper("ABC")}")
}
```
```output
true
```

### `string.is_lower(s: String) -> Bool`

少なくとも一つの字母があり、すべての字母が小文字なら true。字母以外の文字は無視する。

```almd run
fn main() -> Unit = {
  println("${string.is_lower("abc")}")
}
```
```output
true
```

### `string.to_codepoint(s: String) -> Option[Int]`

文字列がちょうど一つのコードポイントを含む場合、その Unicode スカラー値を返す。空文字列または複数のコードポイントなら `none` を返す。

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

Unicode コードポイントから一文字の文字列を作る。n が Unicode スカラー値でない場合（負、サロゲート、U+10FFFF 超）は none。

```almd run
fn main() -> Unit = {
  println(string.from_codepoint(65) ?? "")
}
```
```output
A
```

### `string.pad_end(s: String, n: Int, fill: String) -> String`

目標の長さに達するまで文字列の右側を埋める。

```almd run
fn main() -> Unit = {
  println(string.pad_end("hi", 5, "."))
}
```
```output
hi...
```

### `string.trim_start(s: String) -> String`

先頭の空白を除去する。

```almd run
fn main() -> Unit = {
  println(string.trim_start("  hello"))
}
```
```output
hello
```

### `string.trim_end(s: String) -> String`

末尾の空白を除去する。

```almd run
fn main() -> Unit = {
  println(string.trim_end("hello  "))
}
```
```output
hello
```

### `string.count_matches(s: String, sub: String) -> Int`

部分文字列の出現回数を数える。

```almd run
fn main() -> Unit = {
  println(int.to_string(string.count_matches("banana", "an")))
}
```
```output
2
```

### `string.is_empty(s: String) -> Bool`

文字列が空かを調べる。

```almd run
fn main() -> Unit = {
  println("${string.is_empty("")}")
}
```
```output
true
```

### `string.reverse(s: String) -> String`

文字列の文字の順序を反転する。

```almd run
fn main() -> Unit = {
  println(string.reverse("hello"))
}
```
```output
olleh
```

### `string.strip_prefix(s: String, prefix: String) -> Option[String]`

接頭辞があれば除去し、なければ none を返す。

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

接尾辞があれば除去し、なければ none を返す。

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

部分文字列の最初の出現を置き換える。

```almd run
fn main() -> Unit = {
  println(string.replace_first("aabaa", "a", "X"))
}
```
```output
Xabaa
```

### `string.last_index_of(s: String, needle: String) -> Option[Int]`

部分文字列の最後のコードポイントインデックスを探す。なければ none。

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

文字列の最初の文字を取得する。

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

文字列の最後の文字を取得する。

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

先頭の N 文字を取る。

```almd run
fn main() -> Unit = {
  println(string.take("hello", 3))
}
```
```output
hel
```

### `string.take_end(s: String, n: Int) -> String`

末尾の N 文字を取る。

```almd run
fn main() -> Unit = {
  println(string.take_end("hello", 3))
}
```
```output
llo
```

### `string.drop(s: String, n: Int) -> String`

先頭の N 文字を落とす。

```almd run
fn main() -> Unit = {
  println(string.drop("hello", 2))
}
```
```output
llo
```

### `string.drop_end(s: String, n: Int) -> String`

末尾の N 文字を落とす。

```almd run
fn main() -> Unit = {
  println(string.drop_end("hello", 2))
}
```
```output
hel
```

> 原文の生成済みシグネチャ索引の固定コピー（この訳は生成対象ではない）。

## シグネチャ索引（87 関数）

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



## 索引コメントの説明訳

- `string.trim(s: String) -> String` — s の先頭/末尾の空白を除く。 (@since 0.5.0 or earlier)
- `string.split(s: String, sep: String) -> List[String]` — sep の前後の断片。空の断片も保持する。 (@since 0.5.0 or earlier)
- `string.split_once(s: String, sep: String) -> Option[(String, String)]` — `sep` の最初の出現で分割して `(before, after)` を返す。`sep` がない場合に限り none。 (@since 0.57.0)
- `string.len(s: String) -> Int` — コードポイント数（バイト数ではない）。 (@since 0.5.0 or earlier)
- `string.length(s: String) -> Int   (deprecated — use string.len)` — string.len の別名。 (@since 0.19.2 or earlier)
- `string.contains(s: String, needle: String) -> Bool` — s 内に needle があるか。needle が空なら true。 (@since 0.6.0 or earlier)
- `string.is_equal_constant_time(a: String, b: String) -> Bool` — a と b が同じ UTF-8 バイトを持つなら true。時間は長さだけに依存し、最初の相違バイトで早期終了しない。トークンや署名ヘッダーの比較に使う。`==` は最初の相違で返る。長さが異なれば即座に false（長さは公開情報）。 (@since unreleased)
- `string.starts_with(s: String, prefix: String) -> Bool` — s が prefix で始まるか。"" は常に一致。 (@since 0.6.0 or earlier)
- `string.ends_with(s: String, suffix: String) -> Bool` — s が suffix で終わるか。"" は常に一致。 (@since 0.6.0 or earlier)
- `string.slice(s: String, start: Int, end: Int) -> String` — コードポイント区間 [start, end)。s の範囲に収める。 (@since 0.5.0 or earlier)
- `string.byte_slice(s: String, start: Int, end: Int) -> Option[String]` — バイト区間 [start, end)。不正または文字の途中なら none。 (@since 0.63.0)
- `string.pad_start(s: String, n: Int, fill: String) -> String` — fill で左を埋め n 文字にする。切り詰めない。 (@since 0.6.1 or earlier)
- `string.capitalize(s: String) -> String` — 最初の文字を to_upper と同じ方法で大文字化し、残りはそのまま。 (@since 0.5.13 or earlier)
- `string.to_upper(s: String) -> String` — Unicode 大文字化。非 ASCII を ASCII へ変換しない。同一性比較用ではない。 (@since 0.5.0 or earlier)
- `string.to_lower(s: String) -> String` — Unicode 小文字化。非 ASCII を ASCII へ変換しない。同一性比較用ではない。 (@since 0.5.0 or earlier)
- `string.replace(s: String, from: String, to: String) -> String` — すべての from を to に置き換える。 (@since 0.5.0 or earlier)
- `string.get(s: String, i: Int) -> Option[String]` — i のコードポイント。範囲外なら none。 (@since 0.6.1 or earlier)
- `string.lines(s: String) -> List[String]` — \n または \r\n で分割し、行末は除く。 (@since 0.5.0 or earlier)
- `string.to_list(s: String) -> List[String]` — 各コードポイントを一文字の文字列にする。 (@since 0.5.0 or earlier)
- `string.index_of(s: String, needle: String) -> Option[Int]` — 最初の needle のコードポイントインデックス。なければ none。 (@since 0.5.0 or earlier)
- `string.repeat(s: String, n: Int) -> String` — s を n 回繰り返す。n <= 0 なら空。 (@since 0.5.0 or earlier)
- `string.is_digit(s: String) -> Bool` — すべて ASCII 0-9 か。s が空なら false。 (@since 0.6.0 or earlier)
- `string.is_alpha(s: String) -> Bool` — すべて Unicode の字母か。s が空なら false。 (@since 0.6.0 or earlier)
- `string.is_alphanumeric(s: String) -> Bool` — すべて字母/数字か。s が空なら false。 (@since 0.6.0 or earlier)
- `string.is_whitespace(s: String) -> Bool` — すべて空白か。s が空なら true。 (@since 0.6.0 or earlier)
- `string.is_upper(s: String) -> Bool` — 字母があり、すべて大文字。その他は無視。 (@since 0.6.0 or earlier)
- `string.is_lower(s: String) -> Bool` — 字母があり、すべて小文字。その他は無視。 (@since 0.6.0 or earlier)
- `string.to_codepoint(s: String) -> Option[Int]` — s がちょうど一コードポイントならスカラー値。それ以外は none。 (@since 0.6.0 or earlier)
- `string.from_codepoint(n: Int) -> Option[String]` — n の一文字文字列。n が Unicode スカラー値でなければ none。 (@since 0.6.0 or earlier)
- `string.pad_end(s: String, n: Int, fill: String) -> String` — fill で右を埋め n 文字にする。切り詰めない。 (@since 0.6.1 or earlier)
- `string.trim_start(s: String) -> String` — s の先頭空白を除く。 (@since 0.5.0 or earlier)
- `string.trim_end(s: String) -> String` — s の末尾空白を除く。 (@since 0.5.0 or earlier)
- `string.count_matches(s: String, needle: String) -> Int` — 重なり合わない needle の出現数。needle が空なら len+1。 (@since unreleased)
- `string.count(s: String, f: (String) -> Bool) -> Int` — f が true になる文字の数。 (@since 0.5.0 or earlier)
- `string.is_empty(s: String) -> Bool` — "" だけで true。" " は空でない。 (@since 0.6.0 or earlier)
- `string.reverse(s: String) -> String` — コードポイントを逆順にする。書記素を考慮しない。 (@since 0.5.0 or earlier)
- `string.strip_prefix(s: String, prefix: String) -> Option[String]` — s から prefix を除く。なければ none。 (@since 0.5.0 or earlier)
- `string.strip_suffix(s: String, suffix: String) -> Option[String]` — s から suffix を除く。なければ none。 (@since 0.5.0 or earlier)
- `string.replace_first(s: String, from: String, to: String) -> String` — from があれば最初の出現を to へ置換。 (@since 0.5.0 or earlier)
- `string.last_index_of(s: String, needle: String) -> Option[Int]` — 最後の needle のコードポイントインデックス。なければ none。 (@since 0.5.0 or earlier)
- `string.first(s: String) -> Option[String]` — 先頭コードポイント。空なら none。 (@since 0.7.0 or earlier)
- `string.last(s: String) -> Option[String]` — 末尾コードポイント。空なら none。 (@since 0.7.0 or earlier)
- `string.take(s: String, n: Int) -> String` — 先頭 n コードポイント。n < 0 なら ""。 (@since 0.7.0 or earlier)
- `string.take_end(s: String, n: Int) -> String` — 末尾 n コードポイント。n < 0 なら ""。 (@since 0.7.0 or earlier)
- `string.drop(s: String, n: Int) -> String` — s の先頭 n コードポイントを除く。n < 0 なら s。 (@since 0.7.0 or earlier)
- `string.drop_end(s: String, n: Int) -> String` — s の末尾 n コードポイントを除く。n < 0 なら s。 (@since 0.7.0 or earlier)
- `string.push(mut s: String, x: String) -> Unit` — s に x をその場で追加する。 (@since 0.23.3 or earlier)
- `string.pop(mut s: String) -> Option[String]` — 最後の文字を除去して返す。s が空なら none。 (@since unreleased)
- `string.clear(mut s: String) -> Unit` — s をその場で空にする。 (@since 0.23.3 or earlier)
- `string.compare(a: String, b: String) -> Int` — コードポイント順で a が b より前なら負、等しければ 0、後なら正。 (@since unreleased)
- `string.byte_len(s: String) -> Int` — UTF-8 バイト数（string.len はコードポイント数）。 (@since unreleased)
- `string.to_utf16(s: String) -> List[Int]` — UTF-16 コード単位。U+FFFF を超えるコードポイントはサロゲートペア。 (@since unreleased)
- `string.from_utf16(xs: List[Int]) -> Result[String, String]` — UTF-16 コード単位から文字列を作る。対にならないサロゲート、または 0..65535 外の単位なら err。 (@since unreleased)
- `string.filter(s: String, f: (String) -> Bool) -> String` — s のうち f が true になる文字。 (@since unreleased)
- `string.fold(s: String, init: B, f: (B, String) -> B) -> B` — s の文字に左から右へ f を fold する。 (@since unreleased)
- `string.insert(s: String, i: Int, x: String) -> String` — i 番目の文字の前に x を挿入した s。i が 0..=len 外なら abort。 (@since unreleased)
- `string.remove_at(s: String, i: Int) -> String` — i 番目の文字を除いた s。i が 0..len 外なら abort。 (@since unreleased)
- `string.new() -> String` — 空文字列。 (@since unreleased)
- `string.from_list(xs: List[String]) -> String` — xs の文字列を間に何も挟まず結合する。 (@since unreleased)
- `string.enumerate(s: String) -> List[(Int, String)]` — 0 からの (index, character) の組。 (@since unreleased)
- `string.zip(s: String, ys: List[B]) -> List[(String, B)]` — 文字を ys と組にする。短い方で終了。 (@since unreleased)
- `string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` — 文字と ys の組に f を適用する。短い方で終了。 (@since unreleased)
- `string.filter_map(s: String, f: (String) -> Option[B]) -> List[B]` — 各文字に f を適用した some の結果を順に返す。 (@since unreleased)
- `string.find_map(s: String, f: (String) -> Option[B]) -> Option[B]` — 各文字に f を適用した最初の some の結果。なければ none。 (@since unreleased)
- `string.reduce(s: String, f: (String, String) -> String) -> Option[String]` — 最初の文字を起点に f で fold。s が空なら none。 (@since unreleased)
- `string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` — 文字を順に fold した途中結果。init は含まない。 (@since unreleased)
- `string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` — (f が true の文字、それ以外)。それぞれ順序を保持。 (@since unreleased)
- `string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` — f のキーで文字をグループ化し、各群の順序を保持。 (@since unreleased)
- `string.sort(s: String) -> List[String]` — コードポイント順の文字。 (@since unreleased)
- `string.sort_by(s: String, f: (String) -> B) -> List[String]` — f のキー順の文字。同じキーは順序を保持。 (@since unreleased)
- `string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` — cmp 順の文字（負なら第一引数が先）。安定ソート。 (@since unreleased)
- `string.min(s: String) -> Option[String]` — コードポイント順で最小の文字。空なら none。 (@since unreleased)
- `string.max(s: String) -> Option[String]` — コードポイント順で最大の文字。空なら none。 (@since unreleased)
- `string.min_by(s: String, f: (String) -> K) -> Option[String]` — 最小のキーを持つ最初の文字。空なら none。 (@since unreleased)
- `string.max_by(s: String, f: (String) -> K) -> Option[String]` — 最大のキーを持つ最後の文字。空なら none。 (@since unreleased)
- `string.min_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — cmp による最小の最初の文字。空なら none。 (@since unreleased)
- `string.max_with(s: String, cmp: (String, String) -> Int) -> Option[String]` — cmp による最大の最後の文字。空なら none。 (@since unreleased)
- `string.find_index(s: String, f: (String) -> Bool) -> Option[Int]` — f が true になる最初の文字のインデックス。なければ none。 (@since unreleased)
- `string.take_while(s: String, f: (String) -> Bool) -> String` — すべての文字が f を満たす最長の接頭部分。 (@since unreleased)
- `string.drop_while(s: String, f: (String) -> Bool) -> String` — すべての文字が f を満たす最長の接頭部分を除いた s。 (@since unreleased)
- `string.chunk(s: String, n: Int) -> List[String]` — n 文字ずつの連続部分。最後は短くてもよい。n <= 0 なら abort。 (@since unreleased)
- `string.window(s: String, n: Int) -> List[String]` — 連続する n 文字のすべての区間。n > len なら []。n <= 0 なら abort。 (@since unreleased)
- `string.split_at(s: String, n: Int) -> (String, String)` — (先頭 n 文字、残り)。n が末尾を超えると (s, "")、n < 0 なら ("", s)。 (@since unreleased)
- `string.dedup(s: String) -> String` — 隣接する同じ文字の各連続を一つにまとめた s。 (@since unreleased)
- `string.intersperse(s: String, sep: String) -> String` — s の各隣接文字間に sep を挟む。 (@since unreleased)
- `string.unique(s: String) -> String` — 各文字の最初の出現を順に返す。 (@since unreleased)
- `string.unique_by(s: String, f: (String) -> K) -> String` — f が与える各キーについて最初の文字を順に返す。 (@since unreleased)
