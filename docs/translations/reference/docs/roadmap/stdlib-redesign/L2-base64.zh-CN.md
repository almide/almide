<!-- description: 标准库重新设计 L2/L3 — base64：保留 encode/decode/_url，使每个解码器仅接受自身字母表，不再让 decode 与 decode_url 成为孪生函数 -->
# L2/L3: base64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-base64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-base64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-base64.zh-CN.md)

> [日文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-base64.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照，不自动同步。已采纳提案、测量与跨语言比较是原文历史记录，并非独立重跑的测试或当前实现完成的主张。未执行所述命令。
> 原文同时保留旧的共用解码器比较与后文 #4119 实现修正。保留 Python padding 行为的明确更正，以及提议的跨字母表错误提示未实现这一事实。保留的日文占位符意为“变体”；(F) 标记按原文保留。

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)、配对页面 [L2-hex](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)
> 来源：Rust —（标准库没有。https://doc.rust-lang.org/std/ ）；Go https://pkg.go.dev/encoding/base64 ；Python https://docs.python.org/3/library/base64.html (F)；TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/fromBase64 (F)；Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F)；Elixir https://elixir.hexdocs.pm/Base.html (F)；OCaml —（标准库没有。https://ocaml.org/manual/latest/api/index.html ）；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io.encoding/-base64/ (F)；Swift https://developer.apple.com/documentation/foundation/data/base64encodedstring(options:)
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。probe 位于 scratchpad 的 `probe-url/p1.almd`。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 用标准字母表（`+/`）编码 | `encode` | — | `StdEncoding.EncodeToString` | `b64encode` (F) | `toBase64()` | `base64_encode(b, padding)` (F) | `encode64` (F) | — | `Base64.Default.encode` (F) | `base64EncodedString()` |
| 解码标准字母表 | `decode`（也接受 `-_`） | — | `StdEncoding.DecodeString` | `b64decode` (F) | `fromBase64()`（`-_` 为 SyntaxError）(F) | `base64_decode` (F) | `decode64` (F) | — | `Base64.Default.decode`（`-_` 为异常）(F) | `Data(base64Encoded:)` |
| 用 URL 字母表（`-_`）编码 | `encode_url`（有 `=`） | — | `URLEncoding` | `urlsafe_b64encode`（有 `=`）(F) | `toBase64({alphabet: "base64url"})` (F) | `base64_url_encode` (F) | `url_encode64` (F) | — | `Base64.UrlSafe` (F) | — |
| 解码 URL 字母表 | `decode_url`（与 `decode` 同一函数体） | — | `URLEncoding.DecodeString` | `urlsafe_b64decode` (F) | `fromBase64({alphabet: "base64url"})` (F) | `base64_url_decode` (F) | `url_decode64` (F) | — | `Base64.UrlSafe.decode` (F) | — |
| 不附加 `=` | —（`string.replace(s, "=", "")`） | — | `RawURLEncoding` | — | `omitPadding` | `padding: False` (F) | `padding: false` (F) | — | `PaddingOption` (F) | — |

<a id="完全性の規則"></a>

### 完备性规则

1. 动词只有 `encode` / `decode` 一对。字母表变体在两者后都加 `_<変種>`（`encode_url` 与 `decode_url`）。与 hex 形式相同（[L2-hex](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)）。
2. 仅当变体**改变字母表本身**（base64 的 `-_`）时，才设置 `decode_<変種>`。大小写等**书写差异**均由不带后缀的 `decode` 接受（因此 hex 有 `encode_upper` 而无 `decode_upper`）。
3. 每个 `decode` 只接受自己的字母表，其他字母表的字符返回 err。`=` 仅接受 encode 生成的形式：长度为 4 的倍数，末尾至多两个 `=`，`=` 下隐藏的位为 0（#4119）。
4. **有意省略**：不带 `=` 的编码（JWT 等）可写为 `encode_url(b) |> string.replace("=", "")`，不单设函数。base32/base85 未达到五种语言，因此不提供（[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)）。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`decode` 与 `decode_url` 函数体相同，都接受两种字母表（实测：`base64.decode("-_8A")` 与 `base64.decode_url("+/8A")` 都得到 `[251, 255, 0]`）→ R-base64-01。
- 写法：`encode`/`decode`/`_url` 与 hex（`encode`/`decode`/`encode_upper`）形式一致。依 R-L1-23，不合并模块。
- `=` 处理：编码会添加 `=`。在 #4119 之前，解码接受省略、多余 `=`、以及 `=` 下非零的位（`QQ==`、`QR==`、`QQ`、`QQ========` 均为 `A`，即 RFC 4648 §12 的可塑性）。此处曾写“Python 接受省略”，但实测证明错误：Python 3.14 的 `base64.b64decode("QQ", validate=True)` 以 `Incorrect padding` 拒绝，`QQ========` 与 `QUJD=` 以 `Excess padding not allowed` 拒绝，`=` 与 `====` 以 `Leading padding not allowed` 拒绝。它仅接受 `=` 下非零的位，`QR==` 成为 `A`。Go 的 `StdEncoding.Strict()` 对省略、多余 `=`、非零位全部拒绝。RFC 4648 §3.2 要求必须有 `=`（除非引用它的规范取消要求），§3.5 允许拒绝非零位。遵循多数，#4119 改为仅接受带 `=` 的规范形式。

<a id="案"></a>

## 提案

<a id="r-base64-01-decode-は--だけdecode_url-は--_-だけを受け取る---採用"></a>

### R-base64-01 `decode` 仅接受 `+/`，`decode_url` 仅接受 `-_` — ○ 采纳

- 内容：`base64.decode` 对 `-` 和 `_` 返回 err，`base64.decode_url` 对 `+` 和 `/` 返回 err。不改变名称和签名。
- 依据的 L0 规则：R-L0-01（将同一函数体的两种写法拆为不同职责）、R-L0-00。这是缩小定义域，不改变类型与可接受输入的答案，不属于 R-L0-14 第 4 项“同名却产生不同结果”。
- (a) 标准解码拒绝 URL 字母表的语言有四种：Go、JS（SyntaxError）(F)、Kotlin（IllegalArgumentException）(F)、Elixir。Python 的 `b64decode` 默认**静默丢弃**字母表以外的字符 (F)。不采用静默丢弃的形式。
- (b) 纯计算。通过单一 self-host 实现查询字母表，因此两个 target 答案相同。
- (c) 目前写 `decode_url` 或 `decode` 都得同样结果，混淆不会被发现。区分后，混淆会以 err 停止。
- (d) 语义变更（缩小定义域）。保留名称，不用 `@deprecated`。不放入 epoch 的 `breaks`，写入 dialect epoch 15 的 `notes`。在被拒绝输入的 err 消息中提示“URL 字母表请用 `decode_url`”（反方向同理）。使用数：`decode` 16/2/6/1、`decode_url` 4/2/5/0（spec/tests/docs/stdlib）。变更前统计 spec 与 stdlib 中是否有传入另一种字母表的调用。
- 检查提案：在 `spec/stdlib/base64_test.almd` 设置字母表接受／拒绝的 2 × 2 表，通过 wasm 测试路径确认 native 与 wasm 结果相同。
- issue：#3894。
- 实现：已在 #4119 落地，同时要求 `=` 规范形式。没有加入 (d) 的 err 消息提示：另一字母表的字符与其他非法字符一样，均为 `invalid base64 character`（两条路径每种情况只有一个固定消息）。

<a id="r-base64-02--を付けない符号化を足す---不採用"></a>

### R-base64-02 增加不带 `=` 的编码 — × 不采纳

- 内容：不添加如 `encode_url_raw` 的函数。
- 原因：五种语言提供它，但均为参数或配置（`padding: false`、`omitPadding`），并非单独名称。Almide 若增加参数，所有希望保留 `=` 的既有调用都会类型错误，需要改写。既然可用 `string.replace(s, "=", "")` 表达，就不值得这一成本（R-L0-01：可组合表达的职责不增加写法）。
- 依据的 L0 规则：R-L0-00、R-L0-01。
- issue：无。

<a id="r-base64-03-hex-とモジュールをまとめる---不採用r-l1-23"></a>

### R-base64-03 与 hex 合并模块 — × 不采纳（R-L1-23）

- 原因：如 R-L1-23 所述，合并后的模块名称没有多数派。两模块函数写法（`encode`/`decode`/`encode_<変種>`）已对齐，不再调整。
- issue：无。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `encode` | `base64.encode(b: Bytes) -> String` | Go `StdEncoding.EncodeToString` / Python `b64encode` (F) / JS `toBase64` / Gleam `base64_encode` (F) / Elixir `encode64` (F) / Kotlin `Base64.Default.encode` (F) | 保持 | — |
| `decode` | `base64.decode(s: String) -> Result[Bytes, String]` | Go `DecodeString` / Python `b64decode` (F) / JS `fromBase64` (F) / Gleam `base64_decode` (F) / Elixir `decode64` (F) | 语义变更（仅接受 `+/`） | R-base64-01 |
| `encode_url` | `base64.encode_url(b: Bytes) -> String` | Go `URLEncoding` / Python `urlsafe_b64encode` (F) / Gleam `base64_url_encode` (F) / Elixir `url_encode64` (F) / Kotlin `Base64.UrlSafe` (F) | 保持 | — |
| `decode_url` | `base64.decode_url(s: String) -> Result[Bytes, String]` | Python `urlsafe_b64decode` (F) / JS `fromBase64({alphabet: "base64url"})` (F) / Gleam `base64_url_decode` (F) / Elixir `url_decode64` (F) | 语义变更（仅接受 `-_`） | R-base64-01 |
