<!-- description: Stdlib redesign L2/L3 — base64: keep encode/decode/_url, make each decoder accept only its own alphabet so decode and decode_url stop being twins -->
# L2/L3: base64

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-base64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-base64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-base64.zh-CN.md)

> Non-normative historical reader snapshot of the [canonical Japanese source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-base64.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. Not automatically synchronized. Accepted proposals, measurements and cross-language comparisons are the source’s historical record, not independently rerun tests or claims of current implementation. No referenced commands were executed.
> The old shared-decoder comparison and later #4119 implementation correction coexist in the source. Preserve the explicit correction of Python padding behavior and the fact that the proposed cross-alphabet error hints did not land. The retained Japanese placeholder means “variant”; (F) labels remain as written.

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md), and companion page [L2-hex](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)
> Sources: Rust — (not in the standard library, https://doc.rust-lang.org/std/ ); Go https://pkg.go.dev/encoding/base64 ; Python https://docs.python.org/3/library/base64.html (F); TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/fromBase64 (F); Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F); Elixir https://elixir.hexdocs.pm/Base.html (F); OCaml — (not in the standard library, https://ocaml.org/manual/latest/api/index.html ); Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io.encoding/-base64/ (F); Swift https://developer.apple.com/documentation/foundation/data/base64encodedstring(options:)
> Measurement: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. Probe: `probe-url/p1.almd` in the scratchpad.

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Encode with the standard alphabet (`+/`) | `encode` | — | `StdEncoding.EncodeToString` | `b64encode` (F) | `toBase64()` | `base64_encode(b, padding)` (F) | `encode64` (F) | — | `Base64.Default.encode` (F) | `base64EncodedString()` |
| Decode the standard alphabet | `decode` (also accepts `-_`) | — | `StdEncoding.DecodeString` | `b64decode` (F) | `fromBase64()` (`-_` is a SyntaxError) (F) | `base64_decode` (F) | `decode64` (F) | — | `Base64.Default.decode` (`-_` is an exception) (F) | `Data(base64Encoded:)` |
| Encode with the URL alphabet (`-_`) | `encode_url` (with `=`) | — | `URLEncoding` | `urlsafe_b64encode` (with `=`) (F) | `toBase64({alphabet: "base64url"})` (F) | `base64_url_encode` (F) | `url_encode64` (F) | — | `Base64.UrlSafe` (F) | — |
| Decode the URL alphabet | `decode_url` (same body as `decode`) | — | `URLEncoding.DecodeString` | `urlsafe_b64decode` (F) | `fromBase64({alphabet: "base64url"})` (F) | `base64_url_decode` (F) | `url_decode64` (F) | — | `Base64.UrlSafe.decode` (F) | — |
| Omit `=` | — (`string.replace(s, "=", "")`) | — | `RawURLEncoding` | — | `omitPadding` | `padding: False` (F) | `padding: false` (F) | — | `PaddingOption` (F) | — |

<a id="完全性の規則"></a>

### Completeness rules

1. The only verbs are the `encode` / `decode` pair. Alphabet variants append `_<変種>` to both (`encode_url` and `decode_url`). This has the same shape as hex ([L2-hex](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)).
2. Provide `decode_<変種>` only when the variant **changes the alphabet itself** (base64's `-_`). **Spelling differences** such as upper/lowercase are all accepted by the unsuffixed `decode` (this is why hex has `encode_upper` but no `decode_upper`).
3. Each `decode` accepts only its own alphabet. Characters from the other alphabet produce err. Accept `=` only in the form emitted by encode: length a multiple of 4, at most two trailing `=`, and zero bits hidden beneath `=` (#4119).
4. **Intentional omissions**: encoding without `=` (JWT, etc.) is expressible as `encode_url(b) |> string.replace("=", "")`, so it gets no function. Do not include base32/base85 because they do not reach five languages ([facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)).

<a id="l2-の判定"></a>

### L2 findings

- Duplicate: `decode` and `decode_url` have the same body. Both accept both alphabets (measured: `base64.decode("-_8A")` and `base64.decode_url("+/8A")` both return `[251, 255, 0]`) → R-base64-01.
- Spelling: `encode`/`decode`/`_url` align with hex (`encode`/`decode`/`encode_upper`). As R-L1-23 specifies, do not merge the modules.
- Handling `=`: encoding adds `=`. Before #4119, decoding accepted omission, excess `=`, and nonzero bits beneath `=` (`QQ==`, `QR==`, `QQ`, and `QQ========` all yielded `A`; RFC 4648 §12 malleability). This page formerly said Python accepts omission, but measurement shows that was wrong: Python 3.14 rejects `base64.b64decode("QQ", validate=True)` with `Incorrect padding`, `QQ========` and `QUJD=` with `Excess padding not allowed`, and `=` and `====` with `Leading padding not allowed`. It accepts only the nonzero bits beneath `=`: `QR==` becomes `A`. Go's `StdEncoding.Strict()` rejects omission, excess `=`, and nonzero bits alike. RFC 4648 §3.2 requires `=` unless the referencing specification removes that requirement; §3.5 permits rejecting nonzero bits. Following the majority, #4119 changed this to accept only canonical forms with `=`.

<a id="案"></a>

## Proposals

<a id="r-base64-01-decode-は--だけdecode_url-は--_-だけを受け取る---採用"></a>

### R-base64-01 Let `decode` accept only `+/`, and `decode_url` only `-_` — ○ Accepted

- Content: make `base64.decode` return err for `-` and `_`, and `base64.decode_url` return err for `+` and `/`. Names and signatures do not change.
- Supporting L0 rules: R-L0-01 (split two spellings of the same body into separate roles), R-L0-00. This narrows the domain without changing the type or answers for accepted inputs, so it is not R-L0-14 item 4's “different result under the same name.”
- (a) Four languages reject the URL alphabet in standard decoding: Go, JS (SyntaxError) (F), Kotlin (IllegalArgumentException) (F), and Elixir. Python's `b64decode` **silently discards** characters outside the alphabet by default (F). Do not follow silent discarding.
- (b) Pure computation. A single self-host implementation looks up the alphabet table, so both targets give the same answer.
- (c) Currently `decode_url` and `decode` give the same result, so confusing them goes unnoticed. Separating them stops a mix-up with err.
- (d) Semantic change (domain narrowing). Keep the names, so do not use `@deprecated`. Do not include it in the epoch's `breaks`; record it in dialect epoch 15's `notes`. Put a hint in the rejected-input err message: “use `decode_url` for the URL alphabet,” and vice versa. Usage counts: `decode` 16/2/6/1, `decode_url` 4/2/5/0 (spec/tests/docs/stdlib). Before the change, count calls in spec and stdlib that pass the other alphabet.
- Proposed gate: a 2 × 2 acceptance/rejection table for the alphabets in `spec/stdlib/base64_test.almd`. Verify the same results on native and wasm through the wasm test route.
- Issue: #3894.
- Implementation: landed in #4119 together with canonical `=` requirements. The err-message hints in (d) were not added: characters from the other alphabet produce the same `invalid base64 character` as other invalid characters (one fixed message per case on both legs).

<a id="r-base64-02--を付けない符号化を足す---不採用"></a>

### R-base64-02 Add encoding without `=` — × Rejected

- Content: do not add a function such as `encode_url_raw`.
- Reason: five languages offer this, but all use an argument or setting (`padding: false`, `omitPadding`), not a separate name. Adding an argument in Almide would make every existing call that wants `=` a type error requiring a rewrite. It is expressible as `string.replace(s, "=", "")`, so that cost is not justified (R-L0-01: do not add spellings for roles expressible by composition).
- Supporting L0 rules: R-L0-00, R-L0-01.
- Issue: none.

<a id="r-base64-03-hex-とモジュールをまとめる---不採用r-l1-23"></a>

### R-base64-03 Merge the module with hex — × Rejected (R-L1-23)

- Reason: as R-L1-23 states, there is no majority name for a merged module. Function spellings (`encode`/`decode`/`encode_<変種>`) already align across both modules, so make no further move.
- Issue: none.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Decision | Proposal |
|---|---|---|---|---|
| `encode` | `base64.encode(b: Bytes) -> String` | Go `StdEncoding.EncodeToString` / Python `b64encode` (F) / JS `toBase64` / Gleam `base64_encode` (F) / Elixir `encode64` (F) / Kotlin `Base64.Default.encode` (F) | Keep | — |
| `decode` | `base64.decode(s: String) -> Result[Bytes, String]` | Go `DecodeString` / Python `b64decode` (F) / JS `fromBase64` (F) / Gleam `base64_decode` (F) / Elixir `decode64` (F) | Semantic change (accept only `+/`) | R-base64-01 |
| `encode_url` | `base64.encode_url(b: Bytes) -> String` | Go `URLEncoding` / Python `urlsafe_b64encode` (F) / Gleam `base64_url_encode` (F) / Elixir `url_encode64` (F) / Kotlin `Base64.UrlSafe` (F) | Keep | — |
| `decode_url` | `base64.decode_url(s: String) -> Result[Bytes, String]` | Python `urlsafe_b64decode` (F) / JS `fromBase64({alphabet: "base64url"})` (F) / Gleam `base64_url_decode` (F) / Elixir `url_decode64` (F) | Semantic change (accept only `-_`) | R-base64-01 |
