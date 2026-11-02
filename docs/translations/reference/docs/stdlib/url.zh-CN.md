# url

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](url.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](url.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](url.zh-CN.md)



> 原文：[url.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/url.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

URL 解析、构建和百分号编码（RFC 3986-lite）。`import url`。

纯函数，无需能力，两种目标一致。三对函数族各自在往返操作下封闭：`parse ⇄ to_string`、`encode_component ⇄
decode_component`、`parse_query ⇄ build_query`。失败是带原因的 `Result`（沿用 `int.parse` 的做法），没有会 panic 的变体。

范围（已记录的限制，不是偶然情况）：只支持带 authority 的 scheme（`scheme://…`）；不支持 userinfo、IPv6 方括号主机或 punycode。`+` 不会解码为空格——那是表单编码，不是 RFC 3986。query 或 fragment 存在但为空的 URL（`http://h?`），往返后成为不含该分隔符的形式。

### `url.parse(s: String) -> Result[Url, String]`

将 `scheme://host[:port][/path][?query][#fragment]` 解析成 `Url` 记录。省略时 `port` 为 `none`；authority 后没有 `/` 时 `path` 为 `""`；分隔符不存在时 `query`/`fragment` 为 `""`。拒绝以下输入，并在 `err` 中说明原因：没有 `scheme://` authority 的输入；空主机（`http://`、`http://:8`）；超出不含 pct-encoding 的 RFC 3986 reg-name 字符范围的主机（允许 unreserved、sub-delims，拒绝 `%`）；userinfo（`user@host`）和 IPv6 字面量（`[::1]`），后二者不在支持范围内，会明确指出并拒绝，而非读作主机；以及并非全由 ASCII 数字组成或超出 `0..=65535` 的端口。

如果浏览器可能将 URL 读作指向别处，`url.parse` 会像 WHATWG URL 解析器一样读取，或者拒绝它（ALS-T27、C-402、#4116）：

| 规则 | 示例 |
|---|---|
| 去掉首尾 C0 控制字符及空格；移除任意位置的制表符、LF、CR | `" http://good.com"` → 主机 `good.com` |
| scheme 是第一个 `:` 前的 `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`，随后为 `//` | `javascript:alert(1)//://good.com` → `url.parse: missing '://' scheme separator`（过去会解析为主机 `good.com`） |
| 拒绝反斜杠或其他控制字符 | `http:\\evil.com` → `url.parse: backslash in URL` |
| 数字主机必须是四个规范十进制标签 | `http://127.1/`、`http://0x7f.0.0.1/` → `url.parse: non-canonical IPv4 host: …` |
| scheme 和主机转为小写 | `HTTP://Good.COM/x` → `http://good.com/x` |
| 去掉 `.` 和 `..` 片段，也包括 `%2e` 写法 | `http://good.com/a/%2e%2e/b` → 路径 `/b` |

同一组探针的测量结果（2026-10-24）：WHATWG（Node 22 `URL`）和 Python 3.14 `urllib` 去掉空白，Go 1.27 `net/url` 拒绝它；WHATWG 将 `\` 读作 `/`，把 `127.1` 规范化为 `127.0.0.1`，而 `url.parse` 选择拒绝；WHATWG 去掉 `%2e` 片段，Go 只解码而不移除，Python 则保留。

```almd run
import url

fn host_of(s: String) -> String =
  match url.parse(s) {
    ok(u) => u.host,
    err(reason) => reason,
  }

fn main() -> Unit = {
  println(host_of("https://example.com:8080/a/b?x=1#top"))
  println(host_of("example.com/a/b"))
}
```
```output
example.com
url.parse: missing '://' scheme separator
```

### `url.to_string(u: Url) -> String`

`parse` 的逆操作：重新组装记录，仅在存在时输出 `:port`、`?query` 和 `#fragment`。

### `url.encode_component(s: String) -> String`

对 RFC 3986 unreserved 集合（ALPHA / DIGIT / `-` `.` `_` `~`）之外的每个字节进行百分号编码；先转换为 UTF-8，十六进制数字大写。

```almd run
import url

fn main() -> Unit = {
  println(url.encode_component("a b"))
}
```
```output
a%20b
```

### `url.decode_component(s: String) -> Result[String, String]`

`encode_component` 的逆操作。损坏的转义（`%zz`、被截断的 `%A`）为注明位置的 `err`；`+` 原样保留。

### `url.build_query(pairs: List[(String, String)]) -> String`

`parse_query` 的逆操作，用 `encode_component` 对每个键和值进行百分号编码。

```almd run
import url

fn main() -> Unit = {
  println(url.build_query([("q", "a b"), ("lang", "ja")]))
}
```
```output
q=a%20b&lang=ja
```



<a id="signature-index-9-functions"></a>

## 固定快照 API 索引（9个函数）

```
// All but A-Za-z0-9-._~ as UTF-8 %XX escapes.
// @since 0.60.0 or earlier
url.encode_component(s: String) -> String

// Percent-decoded s; err on bad escape/UTF-8; + kept.
// @since 0.60.0 or earlier
url.decode_component(s: String) -> Result[String, String]

// Url from s, cleaned as WHATWG does; err on a bad scheme, host or port, a backslash or control char, userinfo, IPv6.
// @since 0.6.0 or earlier
url.parse(s: String) -> Result[Url, String]

// URL text of u; empty query/fragment omitted.
// @since 0.6.0 or earlier
url.to_string(u: Url) -> String

// k=v joined by &, both sides percent-encoded.
// @since 0.60.0 or earlier
url.build_query(xs: List[(String, String)]) -> String

// Like encode_component, but a space becomes +.
// @since unreleased
url.encode_form(s: String) -> String

// + as space, then percent-decoded; err on bad escape/UTF-8.
// @since unreleased
url.decode_form(s: String) -> Result[String, String]

// Decoded (k, v) pairs; bare key gets ""; err on a bad escape.
// @since unreleased
url.parse_query(query: String) -> Result[List[(String, String)], String]

// ref cleaned as parse does, resolved against base (RFC 3986 5.2); err when the result is not a URL.
// @since unreleased
url.resolve(base: Url, ref: String) -> Result[Url, String]
```

<a id="type-index-1-types"></a>

## 固定快照类型索引（1个类型）

```
// Parsed URL; port none, other parts "" when absent.
// @since 0.6.0 or earlier
type url.Url = { scheme: String, host: String, port: Option[Int], path: String, query: String, fragment: String }
```




## API 说明译文（按上述签名顺序）

- `url.encode_component(s: String) -> String` — 除 A-Za-z0-9-._~ 外，均编码为 UTF-8 %XX 转义。
- `url.decode_component(s: String) -> Result[String, String]` — 百分号解码 s；错误转义/UTF-8 为 err；保留 +。
- `url.parse(s: String) -> Result[Url, String]` — 从 s 得到 Url，按 WHATWG 方式清理；错误 scheme、主机、端口，反斜杠或控制字符、userinfo、IPv6 均为 err。
- `url.to_string(u: Url) -> String` — u 的 URL 文本；省略空 query/fragment。
- `url.build_query(xs: List[(String, String)]) -> String` — 用 & 连接 k=v，两侧均百分号编码。
- `url.encode_form(s: String) -> String` — 类似 encode_component，但空格变为 +。
- `url.decode_form(s: String) -> Result[String, String]` — 先将 + 变为空格，再百分号解码；错误转义/UTF-8 为 err。
- `url.parse_query(query: String) -> Result[List[(String, String)], String]` — 解码后的 (k, v) 对；无值键得到 ""；错误转义为 err。
- `url.resolve(base: Url, ref: String) -> Result[Url, String]` — 像 parse 一样清理 ref，相对于 base 解析（RFC 3986 5.2）；结果不是 URL 则为 err。
- `type url.Url = { scheme: String, host: String, port: Option[Int], path: String, query: String, fragment: String }` — 解析后的 URL；省略时 port 为 none，其他部分为 ""。

> 关于原文中的比较：与 WHATWG 及其他解析器的比较，是原文所注明时点和探测样例的记录。本翻译没有重新测量这些比较，也未独立确认所有输入上的等价性。
