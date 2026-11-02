# regex

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](regex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](regex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](regex.zh-CN.md)



> 原文：[docs/stdlib/regex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/regex.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文を保持し、説明訳をコード外へ添える。生成索引の固定コピーであり、この訳は生成対象ではない。 原文の時間・状態数・ターゲット同一性は当該コミットの記述であり、ここでは再測定していない。計算量の制限はマッチング部分に限られ、コールバックや出力構築を含む呼出し全体の無条件な時間保証ではない。

正規表現。import regex。

パターンを一度 `Regex` に変換し、どの操作も第 1 引数に `Regex`、第 2 引数に対象文字列を取る（#3942）。

- `regex.new(pattern)` — プログラムが有効と分かっているパターン向け。文字列リテラルは `almide check` が検査する（E102）。実行時に組み立てたパターンが拒否されると `Error: invalid regex: <reason>` で abort し、終了 1。
- `regex.compile(pattern)` — 入力から得たパターン向け。`ok(re)`、拒否時は `err(reason)`。

第 1 引数が `Regex` なので、対象文字列を操作にパイプすると型エラーになる。以前の `s |> regex.find("[0-9]+")` は警告せずパターンの中から対象文字列を検索していたが、今は `regex.find(regex.new("[0-9]+"), s)` を示す。`Regex` は一度作り、ループ内で再利用する。

<a id="regexnewpattern-string---regex-and-regexcompilepattern-string---resultregex-string"></a>

### `regex.new(pattern: String) -> Regex` および `regex.compile(pattern: String) -> Result[Regex, String]`

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
理由は左から最初の違反：`unclosed group`、`unmatched )`、`unclosed character class`、`nothing to repeat`、`malformed repetition`、`repetition minimum exceeds maximum`、`invalid class range`、`trailing backslash`、`unsupported escape \X`（および `… in a class`）、`named groups are not supported`、`lookaround is not supported`、`flags are only supported at the start of the pattern`、`unsupported group syntax`、`groups nested deeper than 250`、`pattern too large: more than 20000 states`（C-395）。

<a id="regexis_matchre-regex-s-string---bool-and-regexis_full_matchre-regex-s-string---bool"></a>

### `regex.is_match(re: Regex, s: String) -> Bool` および `regex.is_full_match(re: Regex, s: String) -> Bool`

`is_match`：re が s のどこかに一致する。`is_full_match`：re が s 全体に一致する（`regex.full_match` は非推奨の旧名で、epoch 16 で削除予定）。

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

### `regex.find(re: Regex, s: String) -> Option[String]` および `regex.index_of(re: Regex, s: String) -> Option[Int]`

最初のマッチとその開始位置。位置は `string.index_of` と同じコードポイント添字。

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

重ならないすべてのマッチを順に返す。

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

置換は {すべて、最初} × {リテラルテキスト、マッチに対する関数}。リテラルテキストは書いたまま挿入され、`$1` は解釈しない。関数は `[whole, group 1, ...]` を受け取り、参加しなかったグループは `none`。

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

マッチ間のテキスト。マッチがなければ `[s]`。

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

### `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` および `regex.captures_all`

最初のマッチの `[whole, group 1, ...]`。マッチがなければ `none`。添字 0 はマッチ全体で、常に `some`。参加しなかったグループは `none`（R-regex-08）だが、グループの空マッチはそうではない。`captures_all` は各マッチに対して同じ形式のリストを順に返す。

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

`s` のメタ文字をすべてエスケープし、そのテキストから作るパターンがそのテキストに一致するようにする。

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

## 対応構文

マッチは最左のものを選び、同じ位置から始まるものの中では次の順で最初のものを選ぶ。選択肢は左から右、貪欲量指定子は長い順、非貪欲量指定子は短い順。グループまたは量指定子がパターンの残りを失敗させる場合、次の選択肢で再試行する。繰り返すグループのキャプチャは最後のインスタンス。空文字列にマッチしたインスタンスも数えるが、繰り返しはそこで終わる。ネイティブと wasm はバイト単位で一致する。

パターンによらず、マッチング時間はパターンの状態数と対象文字列長の積に比例する（#4019、#4083）。受理するパターンは最大 20,000 状態。この制限はマッチング処理に対するもので、置換コールバックや出力構築など、呼出しの全処理には及ばない。100,000 文字に対する `(a+)+$` や `(a|aa)*b` は即座に返り、最悪ケース（全文字で全状態が生きている、100,000 個の `a` に対する `a{0,9000}b`）はネイティブで約 8 秒、wasm で 1 分。各呼出しはまず、4 ×（パターン長 + 1）×（対象文字列長 + 1）ステップを上限にバックトラッキングマッチャを動かす。通常のパターンではこちらが速い。上限を使い切ると、同じ優先順位を持ち 1 状態あたり最大 1 スレッドの Pike VM（Thompson NFA）が同じ答えを返す。パターンの状態数は VM プログラムの命令数（およそパターン長で、回数指定の繰り返しは繰り返す対象のコピーを回数分寄与する）に、空にマッチできる繰り返しグループの最大入れ子深さ + 1 を掛けたもの。`a{0,9999}b` は厳密に 20,000 状態で、入れ子の回数は乗算されるため `((a){200}){200}` は約 120,000 となり拒否される。状態数はパターンを展開せずに数える。

| 構文 | 意味 |
|---|---|
| `a`, `\.`, `\\` | リテラル文字（メタ文字は `\` でエスケープ） |
| `.` | `\n` 以外の任意の文字 |
| `[abc]`, `[a-z]`, `[^0-9]` | 文字クラス、範囲、否定クラス（先頭の `]` はメンバー） |
| `\d` `\w` `\s` と `\D` `\W` `\S` | 数字、単語文字（`[A-Za-z0-9_]`）、空白（`[ \t\n\v\f\r]`、ASCII 空白集合）、およびその否定。クラス内では `\d` `\w` `\s` `\n` `\t` |
| `\n` `\t` `\r` | 制御文字 |
| `^`, `$` | 文字列の先頭／末尾 |
| `\b`, `\B` | 単語境界／非境界（ASCII 単語文字） |
| `x|y` | 選択 |
| `(x)`, `(?:x)` | キャプチャ／非キャプチャグループ。入れ子は最大 250 段 |
| `x*` `x+` `x?` `x{n}` `x{n,}` `x{n,m}` | 貪欲量指定子 |
| `x*?` `x+?` `x??` `x{n,m}?` | 非貪欲量指定子 |
| パターン先頭の `(?i)`, `(?m)`, `(?im)` | `i`：ASCII 英字の大小文字どちらにも一致。`m`：`^` と `$` は `\n` の直後／直前にも一致 |

拒否するもの（C-395）：先読み・後読み、後方参照と上にない英数字のエスケープ、名前付きグループ、先頭以外のフラグ、完全な量指定子になっていない `{`、繰り返す対象のない量指定子、20,000 を超える状態数。



<a id="signature-index-16-functions"></a>

## シグネチャ索引（16 関数）

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

## 型索引（1 型）

```
// A pattern accepted by `regex.new` or `regex.compile`.
// @since unreleased
type regex.Regex = { pattern: String }
```



## 索引コメントの説明訳

- `regex.new(pattern: String) -> Regex` — `pattern` の Regex。拒否時は `Error: invalid regex: <reason>` で abort。 (@since unreleased)
- `regex.compile(pattern: String) -> Result[Regex, String]` — `pattern` の Regex、拒否時は err(reason)。 (@since unreleased)
- `regex.is_match(re: Regex, s: String) -> Bool` — re が s のどこかに一致する。 (@since 0.6.0 or earlier)
- `regex.is_full_match(re: Regex, s: String) -> Bool` — re が s 全体に一致する。 (@since unreleased)
- `regex.full_match(re: Regex, s: String) -> Bool   (deprecated — use regex.is_full_match)` — re が s 全体に一致する（regex.is_full_match へ改名）。 (@since 0.6.0 or earlier)
- `regex.find(re: Regex, s: String) -> Option[String]` — s 内の re の最左マッチ、なければ none。 (@since 0.5.0 or earlier)
- `regex.find_all(re: Regex, s: String) -> List[String]` — 重ならない全マッチを順に返す。 (@since 0.5.0 or earlier)
- `regex.index_of(re: Regex, s: String) -> Option[Int]` — 最左マッチ開始位置のコードポイント添字、なければ none。 (@since unreleased)
- `regex.replace(re: Regex, s: String, rep: String) -> String` — 全マッチをリテラル rep で置換（`$1` は解釈しない）。 (@since 0.5.0 or earlier)
- `regex.replace_first(re: Regex, s: String, rep: String) -> String` — 最初のマッチをリテラル rep で置換。 (@since 0.5.0 or earlier)
- `regex.replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 全マッチを f([whole, group1, ...]) で置換。 (@since unreleased)
- `regex.replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` — 最初のマッチを f([whole, group1, ...]) で置換。 (@since unreleased)
- `regex.split(re: Regex, s: String) -> List[String]` — マッチ間のテキスト。なければ [s]。 (@since 0.5.0 or earlier)
- `regex.captures(re: Regex, s: String) -> Option[List[Option[String]]]` — 最初のマッチの [whole, group1, ...]、なければ none。参加しないグループは none。 (@since 0.5.0 or earlier)
- `regex.captures_all(re: Regex, s: String) -> List[List[Option[String]]]` — 各マッチの [whole, group1, ...] を順に返す。 (@since unreleased)
- `regex.escape(s: String) -> String` — s の正規表現メタ文字をすべてエスケープし、自身に一致させる。 (@since unreleased)
- `type regex.Regex = { pattern: String }` — `regex.new` または `regex.compile` が受理したパターン。 (@since unreleased)
