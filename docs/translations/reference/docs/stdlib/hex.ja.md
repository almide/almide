# hex

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](hex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](hex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](hex.zh-CN.md)



> 原文：[docs/stdlib/hex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/hex.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文のまま保持し、説明訳をコード外へ添える。

十六進数のエンコードとデコード。`import hex`。

pure で capability は不要、両ターゲットで同じ動作になる。1 バイトにつき 2 文字で、区切りも接頭辞もない。

### `hex.encode(b: Bytes) -> String`

小文字の数字表記。

```almd run
import hex

fn main() -> Unit = {
  println(hex.encode(bytes.from_list([222, 173, 190, 239])))
}
```
```output
deadbeef
```

### `hex.encode_upper(b: Bytes) -> String`

大文字の数字表記。チェックサム出力で通常使う表記である。

```almd run
import hex

fn main() -> Unit = {
  println(hex.encode_upper(bytes.from_list([222, 173])))
}
```
```output
DEAD
```

### `hex.decode(s: String) -> Result[Bytes, String]`

十六進文字列をデコードする。大文字・小文字を両方受理し、混在もできる。長さが奇数、または十六進文字以外を含む場合は `err`。

```almd check
import hex

fn show(r: Result[Bytes, String]) -> String = match r {
  ok(b) => "ok(${bytes.to_list(b)})",
  err(e) => "err(\"${e}\")",
}

fn verify(b: Bytes) -> Result[Bytes, String] =
  if bytes.len(b) == 2 then ok(b) else err("expected 2 bytes")

fn check(digest: String) -> Result[Bytes, String] =
  match hex.decode(digest) {
    ok(b) => verify(b),
    err(e) => err("bad hex: " + e),
  }

fn main() -> Unit = {
  println(show(check("DEad")))
  println(show(check("dea")))
  println(show(check("zz")))
}
```

> 原文の生成済みシグネチャ索引の固定コピー（この訳は生成対象ではない）。

## シグネチャ索引（3 関数）

```
// Lowercase hex, two chars per byte.
// @since 0.13.4 or earlier
hex.encode(b: Bytes) -> String

// Uppercase hex, two chars per byte.
// @since 0.13.4 or earlier
hex.encode_upper(b: Bytes) -> String

// Bytes from hex (any case); err if odd or bad char.
// @since 0.13.4 or earlier
hex.decode(s: String) -> Result[Bytes, String]
```



## 索引コメントの説明訳

- `hex.encode(b: Bytes) -> String` — 小文字の十六進表記。1 バイトにつき 2 文字。 (@since 0.13.4 or earlier)
- `hex.encode_upper(b: Bytes) -> String` — 大文字の十六進表記。1 バイトにつき 2 文字。 (@since 0.13.4 or earlier)
- `hex.decode(s: String) -> Result[Bytes, String]` — 十六進表記（大小文字どちらも）からバイト列へ。奇数長または不正文字なら err。 (@since 0.13.4 or earlier)
