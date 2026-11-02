<a id="l2l3-hex"></a>

# L2/L3: hex

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-hex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-hex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-hex.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)
> The comparisons, measured results, and acceptance/rejection decisions belong to the dated original. The scratchpad probe was not rerun; no present-day compatibility or completeness guarantee is added.


> Source description: Stdlib redesign L2/L3 — hex: encode/encode_upper/decode stay as they are; the names mirror base64
> Last updated: 2026-10-21
> Prerequisites: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md), [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md); counterpart page [L2-base64](../../../../../roadmap/stdlib-redesign/L2-base64.md)
> Sources: Rust — (not in the standard library; https://doc.rust-lang.org/std/ ); Go https://pkg.go.dev/encoding/hex ; Python https://docs.python.org/3/library/base64.html (F) and https://docs.python.org/3/library/stdtypes.html#bytes.hex ; TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/toHex ; Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F); Elixir https://elixir.hexdocs.pm/Base.html (F); OCaml https://ocaml.org/manual/latest/api/Digest.html ; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/-hex-format/ ; Swift — (not in the standard library)
> Measurements: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. Probe: `probe-url/p1.almd` in the scratchpad.

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Encode in lowercase | `encode` | — | `EncodeToString` | `b.hex()` | `toHex()` | — | `encode16(case: :lower)` (F) | `Digest.to_hex` | `toHexString()` | — |
| Encode in uppercase | `encode_upper` | — | — | `b16encode` (F) | — | `base16_encode` (F) | `encode16` (uppercase by default) | — | `HexFormat { upperCase }` | — |
| Decode | `decode` (either case) | — | `DecodeString` | `bytes.fromhex`, `b16decode` (uppercase only by default) (F) | `fromHex()` | `base16_decode` (F) | `decode16(case: :mixed)` (F) | `Digest.of_hex` | `hexToByteArray()` | — |
| Integer ⇄ hexadecimal string | `int.to_hex`, `int.from_hex` (on the int side) | `format!("{:x}")` | `strconv.FormatInt(n, 16)` | `hex(n)` | `n.toString(16)` | `int.to_base16` | `Integer.to_string(n, 16)` | `Printf "%x"` | `toString(16)` | `String(n, radix: 16)` |

<a id="完全性の規則"></a>

### Completeness rules

1. Use the same verbs as base64, `encode` / `decode`; variants are `encode_<変種>` (variant; rules 1 and 2 of [L2-base64](../../../../../roadmap/stdlib-redesign/L2-base64.md)).
2. Uppercase and lowercase are **different spellings**, so `decode` accepts both; there is no `decode_upper` (measured: `hex.decode("FfA0")` → `[255, 160]`).
3. Hexadecimal conversion whose subject is `Int` belongs in `int` (R-L0-02). `hex` covers only `Bytes` ⇄ `String`.

<a id="l2-の判定"></a>

### L2 verdict

- No missing roles, duplicates, or gaps. `hex.encode` (Bytes) and `int.to_hex` (Int) have different subject types, so they are not duplicates under R-L0-01.
- A `0x` prefix is err (measured: `hex.decode("0x01")` gives `invalid hex char at 1`). This matches Go, JS and Elixir. Keep it unchanged.

<a id="案"></a>

## Proposals

<a id="r-hex-01-3-関数を据え置く---採用変更なし"></a>

### R-hex-01 Keep the 3 functions — ○ Accepted (no change)

- Content: retain `encode`, `encode_upper`, and `decode`.
- Supporting L0 rules: R-L0-00 (no majority spelling, no collision within Almide), R-L1-23.
- (a) Spellings across the 9 languages are split among `EncodeToString`, `hex()`, `toHex`, `encode16`, and `base16_encode`.
- (b) Pure computation. Both targets produce the same result (measured).
- (c) The verbs match base64, so knowing one lets you infer how to write the other.
- Issue: none

<a id="r-hex-02-decode_upper-を足す---不採用"></a>

### R-hex-02 Add `decode_upper` — × Rejected

- Reason: only Python's `b16decode` makes case-sensitive rejection the standard default (F). As rule 2 states, `decode` accepts differences of spelling.
- Issue: none

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `encode` | `hex.encode(b: Bytes) -> String` | Go `EncodeToString` / Python `b.hex()` / JS `toHex` / Elixir `encode16(case: :lower)` (F) / Kotlin `toHexString` | Retain | R-hex-01 |
| `encode_upper` | `hex.encode_upper(b: Bytes) -> String` | Python `b16encode` (F) / Gleam `base16_encode` (F) / Elixir `encode16` (F) | Retain | R-hex-01 |
| `decode` | `hex.decode(s: String) -> Result[Bytes, String]` | Go `DecodeString` / Python `bytes.fromhex` / JS `fromHex` / Gleam `base16_decode` (F) / Elixir `decode16` (F) | Retain | R-hex-01 |
