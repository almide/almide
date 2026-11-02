# hex

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](hex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](hex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](hex.zh-CN.md)



> 原文：[docs/stdlib/hex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/hex.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名及代码内注释保留原文，在代码外附上说明译文。

十六进制编码与解码。`import hex`。

pure——无需 capability，两个目标行为相同。每字节两个字符，无分隔符，无前缀。

### `hex.encode(b: Bytes) -> String`

小写数字形式。

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

大写数字形式——校验和输出通常采用的写法。

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

解码十六进制字符串。大小写均接受，且可混用。长度为奇数或含非十六进制字符时返回 `err`。

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

> 原文生成签名索引的固定副本（本译文不是生成目标）。

<a id="signature-index-3-functions"></a>

## 签名索引（3 个函数）

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



## 索引注释的说明译文

- `hex.encode(b: Bytes) -> String` — 小写十六进制，每字节两个字符。 (@since 0.13.4 or earlier)
- `hex.encode_upper(b: Bytes) -> String` — 大写十六进制，每字节两个字符。 (@since 0.13.4 or earlier)
- `hex.decode(s: String) -> Result[Bytes, String]` — 从十六进制（任意大小写）解码字节；长度为奇数或字符无效则 err。 (@since 0.13.4 or earlier)
