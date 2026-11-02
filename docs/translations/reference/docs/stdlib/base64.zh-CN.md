# base64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](base64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](base64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](base64.zh-CN.md)



> 原文：[docs/stdlib/base64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/base64.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名及代码内注释保留原文，在代码外附上说明译文。

Base64 编码与解码。`import base64`。

pure——无需 capability，两个目标行为相同。编码接收 `Bytes` 并产生 `String`；解码可能失败，因此返回 `Result`。

载荷是文本而非二进制时，在边界使用 `bytes.from_string` / `bytes.to_string`。

### `base64.encode(b: Bytes) -> String`

标准字符集（RFC 4648 §4），带 `=` 填充。

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

解码标准字符集（`+` `/`）。仅接受 `encode` 输出的带填充规范形式，因此每个字节串恰有一种写法（#4119）：

- 长度必须为 4 的倍数——`"invalid base64 length: <n>"`；
- 最多两个 `=`，且只能在末尾——更多则为 `"invalid base64 padding"`；
- 其他字符都必须属于标准字符集——`-` 或 `_`（URL-safe 字符对）、末尾之前的 `=` 或任何其他字节均为 `"invalid base64 character"`；
- 被填充丢弃的位必须为零——`QR==`（也会读为 `A`）为 `"invalid base64 trailing bits"`。

按此顺序执行检查；首次失败即为结果。

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

URL 和文件名安全的字符集（RFC 4648 §5）：`-` 和 `_` 替代 `+` 和 `/`。保留填充，因此输入长度不是三的倍数时，输出仍以 `=` 结尾。

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

需要无填充 base64url 的场景（如 JWT 片段）应移除它：`string.replace(base64.encode_url(b), "=", "")`。

### `base64.decode_url(s: String) -> Result[Bytes, String]`

按与 `decode` 相同的规范规则解码 URL-safe 字符集（`-` `_`）：带填充、最多两个 `=`、尾部余位为零，且 `+` 或 `/` 为 `"invalid base64 character"`。它能与 `encode_url` 往返转换。无填充片段（如 JWT 的一部分）必须先恢复填充：

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

> 原文生成签名索引的固定副本（本译文不是生成目标）。

## 签名索引（4 个函数）

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



## 索引注释的说明译文

- `base64.encode(b: Bytes) -> String` — 标准 base64，使用 + / 和 = 填充。 (@since 0.13.4 or earlier)
- `base64.decode(s: String) -> Result[Bytes, String]` — 从带填充的规范 base64（+ /）得到字节；否则 err。 (@since 0.13.4 or earlier)
- `base64.encode_url(b: Bytes) -> String` — URL-safe base64（- _），仍保留 = 填充。 (@since 0.13.4 or earlier)
- `base64.decode_url(s: String) -> Result[Bytes, String]` — 从带填充的规范 URL-safe base64（- _）得到字节；否则 err。 (@since 0.13.4 or earlier)
