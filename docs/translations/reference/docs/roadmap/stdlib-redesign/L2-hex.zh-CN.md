<a id="l2l3-hex"></a>

# L2/L3: hex

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-hex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-hex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-hex.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)
> 语言比较、实测结果及采纳／拒绝决定属于注明日期的原文记录。未重新运行 scratchpad 探针，不新增对当前兼容性或完整性的保证。


> 原文描述：标准库重设计 L2/L3 — hex：encode/encode_upper/decode 保持不变；命名与 base64 对应
> 最后更新：2026-10-21
> 前提：[L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)、[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)；对应页面 [L2-base64](../../../../../roadmap/stdlib-redesign/L2-base64.md)
> 来源：Rust —（标准库没有；https://doc.rust-lang.org/std/ ）；Go https://pkg.go.dev/encoding/hex ；Python https://docs.python.org/3/library/base64.html (F)、https://docs.python.org/3/library/stdtypes.html#bytes.hex ；TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/toHex ；Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F)；Elixir https://elixir.hexdocs.pm/Base.html (F)；OCaml https://ocaml.org/manual/latest/api/Digest.html ；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/-hex-format/ ；Swift —（标准库没有）
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。探针为 scratchpad 中的 `probe-url/p1.almd`。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 编码为小写 | `encode` | — | `EncodeToString` | `b.hex()` | `toHex()` | — | `encode16(case: :lower)` (F) | `Digest.to_hex` | `toHexString()` | — |
| 编码为大写 | `encode_upper` | — | — | `b16encode` (F) | — | `base16_encode` (F) | `encode16`（默认为大写） | — | `HexFormat { upperCase }` | — |
| 解码 | `decode`（大小写均可） | — | `DecodeString` | `bytes.fromhex`、`b16decode`（默认仅大写）(F) | `fromHex()` | `base16_decode` (F) | `decode16(case: :mixed)` (F) | `Digest.of_hex` | `hexToByteArray()` | — |
| 整数 ⇄ 十六进制字符串 | `int.to_hex`、`int.from_hex`（位于 int） | `format!("{:x}")` | `strconv.FormatInt(n, 16)` | `hex(n)` | `n.toString(16)` | `int.to_base16` | `Integer.to_string(n, 16)` | `Printf "%x"` | `toString(16)` | `String(n, radix: 16)` |

<a id="完全性の規則"></a>

### 完整性规则

1. 动词与 base64 相同，为 `encode` / `decode`；变体使用 `encode_<変種>`（变体；[L2-base64](../../../../../roadmap/stdlib-redesign/L2-base64.md) 的规则 1、2）。
2. 大小写是**写法差异**，所以 `decode` 接受两者，不设置 `decode_upper`（实测：`hex.decode("FfA0")` → `[255, 160]`）。
3. 主体为 `Int` 的十六进制转换放在 `int`（R-L0-02）。`hex` 只处理 `Bytes` ⇄ `String`。

<a id="l2-の判定"></a>

### L2 判定

- 没有缺失职责、重复或空缺。`hex.encode`（Bytes）与 `int.to_hex`（Int）的主体类型不同，不构成 R-L0-01 的重复。
- `0x` 前缀为 err（实测：`hex.decode("0x01")` 产生 `invalid hex char at 1`）。与 Go、JS、Elixir 相同，保持不变。

<a id="案"></a>

## 方案

<a id="r-hex-01-3-関数を据え置く---採用変更なし"></a>

### R-hex-01 保留 3 个函数 — ○ 采纳（无变更）

- 内容：保留 `encode`、`encode_upper`、`decode`。
- 所依据的 L0 规则：R-L0-00（没有多数派写法，在 Almide 内无冲突）、R-L1-23。
- (a) 9 种语言的写法分散为 `EncodeToString`、`hex()`、`toHex`、`encode16`、`base16_encode`。
- (b) 纯计算。两个 target 结果相同（实测）。
- (c) 与 base64 使用相同动词，知道一方就能推测另一方的写法。
- issue：无

<a id="r-hex-02-decode_upper-を足す---不採用"></a>

### R-hex-02 添加 `decode_upper` — × 不采纳

- 理由：只有 Python 的 `b16decode` 将区分大小写并拒绝不符形式的解码作为标准默认行为（F）。依规则 2，`decode` 接受写法差异。
- issue：无

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（现行，显示 mut） | 其他语言的主要写法 | 判定 | 方案 |
|---|---|---|---|---|
| `encode` | `hex.encode(b: Bytes) -> String` | Go `EncodeToString` / Python `b.hex()` / JS `toHex` / Elixir `encode16(case: :lower)` (F) / Kotlin `toHexString` | 保留 | R-hex-01 |
| `encode_upper` | `hex.encode_upper(b: Bytes) -> String` | Python `b16encode` (F) / Gleam `base16_encode` (F) / Elixir `encode16` (F) | 保留 | R-hex-01 |
| `decode` | `hex.decode(s: String) -> Result[Bytes, String]` | Go `DecodeString` / Python `bytes.fromhex` / JS `fromHex` / Gleam `base16_decode` (F) / Elixir `decode16` (F) | 保留 | R-hex-01 |
