说明：stdlib 重设计 L2/L3 — url 统一拥有百分号编码（http.url_decode 迁入为 decode_form），查询解析后解码，并增加 resolve。
<a id="l2l3-url"></a>

# L2/L3: url

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-url.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-url.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-url.zh-CN.md)

> 非规范性带日期重设计资料，固定原本为权威且不变。不自动同步、无新probe、未验证现行API。采纳提案和epoch窗口属于历史决定，不是新确认发布。保留component/form差异、无效UTF-8的fallback变化及parse_query不自动fix。他语言F标记与使用数为原调查结果。userinfo表示不是凭据安全处理保证。后段userinfo提案与前段字段保持判定均照录。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-28）。
> 来源：Rust —（标准库没有，https://doc.rust-lang.org/std/）；Go https://pkg.go.dev/net/url (F)；Python https://docs.python.org/3/library/urllib.parse.html (F)；TS/JS https://developer.mozilla.org/en-US/docs/Web/API/URL 、https://developer.mozilla.org/en-US/docs/Web/API/URLSearchParams；Gleam https://gleam-stdlib.hexdocs.pm/gleam/uri.html；Elixir https://elixir.hexdocs.pm/URI.html (F)；OCaml —（标准库没有）；Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/URI.html；Swift https://developer.apple.com/documentation/foundation/urlcomponents。
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。probe 为 scratchpad 的 `probe-url/p1.almd`。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 对部件百分号编码（`+` 保持 `+` 不变） | `encode_component` / `decode_component` | — | `PathEscape` / `PathUnescape` (F) | `quote` / `unquote` (F) | `encodeURIComponent` / `decodeURIComponent` | `percent_encode` / `percent_decode` | `encode` / `decode` (F) | — | — | `addingPercentEncoding` |
| 表单形式（空格 ⇄ `+`） | `http.url_decode`（仅解码，在 http 中） | — | `QueryEscape` / `QueryUnescape`（`+` → 空格）(F) | `quote_plus` / `unquote_plus` (F) | `URLSearchParams` | — | `encode_www_form` / `decode_www_form`（`+` → 空格）(F) | — | `URLEncoder` / `URLDecoder`(JVM) | — |
| URL 分解与构造 | `parse` / `to_string`（`Url` 类型） | — | `Parse` / `String` | `urlparse` / `urlunparse` (F) | `new URL` / `href` | `parse` / `to_string` | `parse` / `to_string` (F) | — | `URI` | `URLComponents` |
| 查询转为对序列（解码） | `query_pairs`（**不解码**） | — | `ParseQuery` (F) | `parse_qsl`（解码）(F) | `URLSearchParams` | `parse_query` | `decode_query` (F) | — | — | `queryItems` |
| 从对序列构建查询 | `build_query` | — | `Values.Encode` | `urlencode` (F) | `URLSearchParams.toString` | `query_to_string` | `encode_query` (F) | — | — | `queryItems` |
| 相对基准解析相对 URL | **—** | — | `ResolveReference` (F) | `urljoin` (F) | `new URL(ref, base)` | `merge` | `merge` (F) | — | `URI.resolve`(JVM) | `URL(string:relativeTo:)` |

6 种语言具备相对 URL 解析（Go、Python、JS、Gleam、Elixir、Swift；Kotlin 是 JVM 的 ◐）。

<a id="完全性の規則"></a>

### 完整性规则

1. 百分号编码仅由 `url` 拥有（R-L1-28、R-L0-01）。
2. 编码有两种方式，各自具备 `encode_<方式>` / `decode_<方式>` 对：`component`（把 `+` 当字符，RFC 3986）和 `form`（空格变 `+`，application/x-www-form-urlencoded）。
3. 解码返回 `Result[String, String]`。无效转义和无效 UTF-8 都是 err，不用哨兵（原样返回输入或替换成 U+FFFD；R-L0-06 第 4 项）。
4. 成对：`parse` ⇄ `to_string`（URL）、`parse_query` ⇄ `build_query`（查询），满足 `parse_query(build_query(ps)) == ok(ps)`。
5. **有意留空**：不提供返回 Map 的查询对版本，因为键可以重复；可用 `map.from_list`。

<a id="l2-の判定"></a>

### L2 判定

- 看似重复：`http.url_decode` 与 `url.decode_component` **不同**（实测）。

  | 输入 | `url.decode_component` | `http.url_decode` |
  |---|---|---|
  | `a+b%20c` | `ok("a+b c")` | `"a b c"`（`+` 变空格） |
  | `%zz` | err（invalid percent escape） | `"%zz"`（原样返回） |
  | `%ff` | err（invalid UTF-8） | `"�"`（替换字符） |
  | `50%` | err（truncated） | `"50%"` |

  `http.url_decode` 是表单解码，用哨兵（原输入或替换字符）隐藏失败。保留 R-L1-28 的撤去决定，但替代者改成新的表单函数 → R-url-01。
- 语义不一致：`query_pairs` 不解码（实测：`"c=x%20y&d=p+q"` → `("c", "x%20y")`、`("d", "p+q")`），而 `build_query` 编码，两者不成对。其他 5 种语言同角色均解码 → R-url-02。
- 缺失角色：相对 URL 解析 → R-url-03。
- `Url` 字段：仅 `port` 是 Option，其余缺失时为 `""`。这是 `parse` 成功后的表示，不属于 R-L0-06 第 4 项的失败时不用哨兵；保持不变。

<a id="案"></a>

## 提案

<a id="r-url-01-フォーム形式の対-encode_form--decode_form-を足しhttpurl_decode-を撤去する---採用"></a>

### R-url-01 增加表单对 `encode_form` / `decode_form`，撤去 `http.url_decode` — ○ 采纳

- 内容：
  - 增加 `url.encode_form(s: String) -> String`（空格为 `+`，其余同 `encode_component`）、`url.decode_form(s: String) -> Result[String, String]`（先把 `+` 变空格，再按 `decode_component` 同规则解码）。
  - 撤去 `http.url_decode(s) -> String`。**在此确定 R-L1-28：撤去，替代者是 `url.decode_form` 而非 `url.decode_component`**，前者对 `+` 的处理才相同。
- L0 依据：R-L1-28、R-L0-01、R-L0-02（字符串百分号编码属于 url）、R-L0-06 第 4 项（不返回哨兵）、R-L0-14。
- (a) Go（`PathUnescape`/`QueryUnescape`）(F)、Python（`unquote`/`unquote_plus`）(F)、Elixir（`decode`/`decode_www_form`）(F)、JS（`decodeURIComponent`/`URLSearchParams`）、Kotlin（JVM 的 `URLDecoder`）分开命名两种方式。
- (b) 纯计算。采用现有 `url.decode_component` 同一 self-host 实现，两 target 答案相同。
- (c) 名称本身看不出是否把 `+` 变空格。把方式名加入函数名，让调用可读。
- (d) 增加 2，http 侧撤去 1。给 `http.url_decode` 标记 `@deprecated(since=15, use="url.decode_form")`，在 `removal_epoch = 16` 撤去。`almide fix` 把 `http.url_decode(s)` 改成 `url.decode_form(s) ?? s`（无效输入返回原字符串）。唯一差异是 `%ff` 等无效 UTF-8：由现有替换字符变为原字符串。在提示中说明差异。使用数（`http.url_decode`）：spec 9 / tests 0 / docs 9 / stdlib 2。stdlib 两处（`http.almd:366` 的 `__decode_part` 和 `http_url_decode.almd`）同次改写。
- 门禁提案：在 `spec/stdlib/url_test.almd` 放入上面 4 输入表，覆盖 2 方式 × 2 target。性质测试往返 `decode_form(encode_form(s)) == ok(s)`。
- issue：#3950。

<a id="r-url-02-クエリの読みを復号する-parse_query-にする---採用"></a>

### R-url-02 把查询读取改为解码的 `parse_query` — ○ 采纳

- 内容：增加 `url.parse_query(query: String) -> Result[List[(String, String)], String]`，按 `decode_form` 规则解码键和值。无值键取 `""`（同现有 `query_pairs`）。撤去不解码的 `query_pairs`。
- L0 依据：R-L0-00 条件 1（`query_pairs` 与 `build_query` 不成对）、R-L0-14 第 4 项。后者允许让旧写法变成类型错误的变更保留同名，但仍不保留 `query_pairs`：若只用 `?? []` 修好返回类型错误，后面的 `decode_component` 调用仍能通过类型检查，悄悄二次解码，使 `%2520` 变空格。改名会促使阅读提示并改写。名称遵循 Go 的 `ParseQuery`(F) 和 Gleam 的 `parse_query`，与 `parse` ⇄ `to_string` 一样采用 `parse_<対象>`（解析对象）形式。
- (a) 同角色的 5 种语言（Go、Python、JS、Gleam、Elixir）全部解码。
- (b) 纯计算。
- (c) 当时忘了对 `query_pairs` 结果调用 `decode_component`，就会静默留下含 `%20` 的值。
- (d) 撤去 1、增加 1。`@deprecated(since=15, use="url.parse_query")`、`removal_epoch = 16`。返回类型变为 `Result` 且值会解码，所以不由 `almide fix` 改写。提示：“`url.parse_query(q) ?? []` 返回已解码的对。删除预期未解码值的 `decode_component` 调用。”使用数（`query_pairs`）：spec 3 / tests 0 / docs 4 / stdlib 5（定义与注释）。
- 门禁提案：性质测试 `parse_query(build_query(ps)) == ok(ps)`。
- issue：#3951。

<a id="r-url-03-相対-url-の解決-resolve-を足す---採用"></a>

### R-url-03 增加相对 URL 解析 `resolve` — ○ 采纳

- 内容：`url.resolve(base: Url, ref: String) -> Result[Url, String]`，遵循 RFC 3986 第 5 章（引用解析）。若 `ref` 为绝对 URL，返回直接解析它的结果。
- L0 依据：L2 标准（至少 5 种语言具备、Almide 缺失的角色）、R-L0-02（主语 `Url`）、R-L0-03（主语在首位）。
- (a) 6 种语言具备。名称分为 `ResolveReference`(F)、`urljoin`(F)、`merge`(F)、`resolve`，无多数派。选择 JVM `URI.resolve` 同一动词，也是 RFC 3986 “解析”的用语。
- (b) 纯计算。RFC 3986 5.4 示例（5.4.1 正常、5.4.2 异常）作为 native/wasm fixture。
- (c) 手工拼接相对链接易误处理 `../` 与 `?`。
- (d) 增加 1。
- 门禁提案：把 RFC 3986 5.4 示例表原样做成测试。
- issue：#3952。

<a id="r-url-04-url-の欄を-option-にする---不採用"></a>

### R-url-04 把 `Url` 字段改成 Option — × 不采纳

- 理由：需要区分 `query`、`fragment` 缺失与空的情形少；改字段类型会破坏所有 `u.query` 读取（R-L0-14 第 4 项）。`port` 已是 Option。
- issue：无。

<a id="r-url-05-url-に-userinfo-を足す---採用"></a>

### R-url-05 向 `Url` 增加 userinfo — ○ 采纳

- 当前：`type Url = { scheme, host, port: Int?, path, query, fragment }`。`url.parse("https://user:pw@example.com/x")` 返回 `err("userinfo is not supported")`。
- 提案：增加 `userinfo: String`（缺失时 `""`，同 `query`、`fragment`），让 `parse`、`to_string`、`resolve` 处理它。
- (a) 6 种语言可读取：Go `URL.User`、Python `urlparse().username`、JS `URL.username`、Elixir `URI.userinfo`、Gleam `Uri.userinfo`、JVM `URI.getUserInfo`。
- (c) 读取 `DATABASE_URL`（`postgres://user:pass@host/db`）是常见任务，当时只有运行后才出现 err。
- (d) 类型变更。记录构造 `Url { ... }` 因缺字段产生类型错误，满足 R-L0-14.4（提示：增加 `userinfo: ""`）。只读调用不变。
- 门禁提案：RFC 3986 authority 示例作为两 target fixture，在 `parse(to_string(u)) == ok(u)` 性质测试中加入 userinfo。
- issue：#3971。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当时，显示 mut） | 其他语言主要名称 | 判定 | 提案 |
|---|---|---|---|---|
| `encode_component` | `url.encode_component(s: String) -> String` | JS `encodeURIComponent` / Go `PathEscape` (F) / Python `quote` (F) / Elixir `encode` (F) / Gleam `percent_encode` | 保持 | R-url-01 |
| `decode_component` | `url.decode_component(s: String) -> Result[String, String]` | JS `decodeURIComponent` / Go `PathUnescape` (F) / Python `unquote` (F) / Elixir `decode` (F) | 保持 | R-url-01 |
| `parse` | `url.parse(s: String) -> Result[Url, String]` | Go `url.Parse` / Python `urlparse` (F) / JS `new URL` / Elixir `URI.parse` (F) / Gleam `uri.parse` | 语义变更（接受 userinfo，在 `Url` 中增加 `userinfo` 字段） | R-url-05 |
| `to_string` | `url.to_string(u: Url) -> String` | Go `String` / Elixir `to_string` (F) / Gleam `to_string` / JS `href` | 保持 | — |
| `query_pairs` | `url.query_pairs(query: String) -> List[(String, String)]` | Python `parse_qsl`（解码）(F) / Go `ParseQuery` (F) / Elixir `decode_query` (F) | 撤去 → `parse_query` | R-url-02 |
| `build_query` | `url.build_query(pairs: List[(String, String)]) -> String` | Python `urlencode` (F) / Elixir `encode_query` (F) / Go `Values.Encode` / Gleam `query_to_string` | 保持 | R-url-02 |
| `encode_form` | `url.encode_form(s: String) -> String`（提案） | Go `QueryEscape` (F) / Python `quote_plus` (F) / Elixir `encode_www_form` (F) | 增加 | R-url-01 |
| `decode_form` | `url.decode_form(s: String) -> Result[String, String]`（提案） | Go `QueryUnescape` (F) / Python `unquote_plus` (F) / Elixir `decode_www_form` (F) | 增加（`http.url_decode` 的替代者） | R-url-01 |
| `parse_query` | `url.parse_query(query: String) -> Result[List[(String, String)], String]`（提案） | Go `ParseQuery` (F) / Gleam `parse_query` / Python `parse_qsl` (F) | 增加 | R-url-02 |
| `resolve` | `url.resolve(base: Url, ref: String) -> Result[Url, String]`（提案） | Go `ResolveReference` (F) / Python `urljoin` (F) / JS `new URL(ref, base)` / Elixir `merge` (F) / JVM `URI.resolve` | 增加 | R-url-03 |

（参考：其他模块的行。）

| 函数 | 签名（当时，显示 mut） | 其他语言主要名称 | 判定 | 提案 |
|---|---|---|---|---|
| `http.url_decode` | `http.url_decode(s: String) -> String` | Go `QueryUnescape` (F) / Python `unquote_plus` (F) | 撤去 → `url.decode_form(s) ?? s` | R-url-01 |
