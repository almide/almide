# base64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](base64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](base64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](base64.zh-CN.md)



> 原文：[docs/stdlib/base64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/base64.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文のまま保持し、説明訳をコード外へ添える。

Base64 のエンコードとデコード。`import base64`。

pure で capability は不要、両ターゲットで同じ動作になる。エンコードは `Bytes` を受け取り `String` を生成する。デコードは失敗し得るため `Result` を返す。

ペイロードがバイナリでなくテキストなら、入出力の境界で `bytes.from_string` / `bytes.to_string` を使う。

### `base64.encode(b: Bytes) -> String`

標準文字集合（RFC 4648 §4）を使い、`=` でパディングする。

```almd run
import base64

fn main() -> Unit = {
  println(base64.encode(bytes.from_string("hello")))
}
```
```output
aGVsbG8=
```

### `base64.decode(s: String) -> Result[Bytes, String]`

標準文字集合（`+` `/`）をデコードする。`encode` が出力する正準のパディング付き形式だけを受理するため、一つのバイト列にはちょうど一つの表記しかない（#4119）。

- 長さは 4 の倍数でなければならない — `"invalid base64 length: <n>"`。
- `=` は末尾だけに最大二つ — それより多ければ `"invalid base64 padding"`。
- ほかの文字はすべて標準文字集合に属さなければならない — `-` または `_`（URL-safe の対）、末尾より前の `=`、それ以外のバイトは `"invalid base64 character"`。
- パディングで捨てるビットはゼロでなければならない — `QR==`（これも `A` と読める）は `"invalid base64 trailing bits"`。

検査はこの順序で行い、最初の失敗を結果とする。

```almd run
import base64

fn show(r: Result[String, String]) -> String = match r {
  ok(s) => "ok(\"${s}\")",
  err(e) => "err(\"${e}\")",
}

fn process(b: Bytes) -> Result[String, String] = bytes.to_string(b)

fn decode(payload: String) -> Result[String, String] =
  match base64.decode(payload) {
    ok(b) => process(b),
    err(e) => err("bad base64: " + e),
  }

fn main() -> Unit = {
  println(show(decode("aGVsbG8=")))
  println(show(decode("aGVs*G8=")))
  println(show(decode("aGVsb")))
}
```
```output
ok("hello")
err("bad base64: invalid base64 character")
err("bad base64: invalid base64 length: 5")
```

### `base64.encode_url(b: Bytes) -> String`

URL とファイル名に安全な文字集合（RFC 4648 §5）：`+` と `/` を `-` と `_` に置き換える。パディングは維持するため、入力長が三の倍数でなければ出力は引き続き `=` で終わる。

```almd run
import base64

fn main() -> Unit = {
  println(base64.encode_url(bytes.from_string("hi?>")))
  println(base64.encode(bytes.from_string("hi?>")))
}
```
```output
aGk_Pg==
aGk/Pg==
```

JWT のセグメントなど、パディングなしの base64url が必要な場合は `string.replace(base64.encode_url(b), "=", "")` で取り除く。

### `base64.decode_url(s: String) -> Result[Bytes, String]`

`decode` と同じ正準規則で URL-safe 文字集合（`-` `_`）をデコードする。パディング付き、`=` は最大二つ、末尾の余剰ビットはゼロであり、`+` または `/` は `"invalid base64 character"` となる。`encode_url` と往復できる。パディングのないセグメント（JWT の一部分など）は、先にパディングを復元する必要がある。

```almd run
import base64

fn pad(s: String) -> String = s + string.repeat("=", (4 - string.len(s) % 4) % 4)

effect fn main() -> Unit = {
  println(bytes.to_string(base64.decode_url(pad("aGk_Pg"))!)!)
}
```
```output
hi?>
```

> 原文の生成済みシグネチャ索引の固定コピー（この訳は生成対象ではない）。

<a id="signature-index-4-functions"></a>

## シグネチャ索引（4 関数）

```
// Standard base64 with + / and = padding.
// @since 0.13.4 or earlier
base64.encode(b: Bytes) -> String

// Bytes from padded canonical base64 (+ /); err otherwise.
// @since 0.13.4 or earlier
base64.decode(s: String) -> Result[Bytes, String]

// URL-safe base64 (- _), still = padded.
// @since 0.13.4 or earlier
base64.encode_url(b: Bytes) -> String

// Bytes from padded canonical URL-safe base64 (- _); err otherwise.
// @since 0.13.4 or earlier
base64.decode_url(s: String) -> Result[Bytes, String]
```



## 索引コメントの説明訳

- `base64.encode(b: Bytes) -> String` — + / と = パディングを使う標準 base64。 (@since 0.13.4 or earlier)
- `base64.decode(s: String) -> Result[Bytes, String]` — パディング付き正準 base64（+ /）からバイト列へ。それ以外は err。 (@since 0.13.4 or earlier)
- `base64.encode_url(b: Bytes) -> String` — URL-safe base64（- _）。= パディングを維持。 (@since 0.13.4 or earlier)
- `base64.decode_url(s: String) -> Result[Bytes, String]` — パディング付き正準 URL-safe base64（- _）からバイト列へ。それ以外は err。 (@since 0.13.4 or earlier)
