Description: Stdlib redesign L2/L3 — url: owns all percent coding (http.url_decode moves in as decode_form), parses queries decoded, and gains resolve.
<a id="l2l3-url"></a>

# L2/L3: url

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-url.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-url.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-url.zh-CN.md)

> Non-normative dated redesign snapshot, fixed original authoritative and unchanged. No automatic synchronization, new probing or present-day API verification. Accepted proposals and epoch windows are historical decisions, not newly confirmed releases. Preserve component/form differences, invalid-UTF-8 fallback changes and the deliberate exclusion of parse_query from automated fixes. Cross-language F marks and usage counts are original investigation results. Userinfo representation is not a credential-handling security guarantee. The later userinfo proposal and earlier keep-field verdict remain as recorded. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-28).
> Sources: Rust — (not in std, https://doc.rust-lang.org/std/); Go https://pkg.go.dev/net/url (F); Python https://docs.python.org/3/library/urllib.parse.html (F); TS/JS https://developer.mozilla.org/en-US/docs/Web/API/URL and https://developer.mozilla.org/en-US/docs/Web/API/URLSearchParams; Gleam https://gleam-stdlib.hexdocs.pm/gleam/uri.html; Elixir https://elixir.hexdocs.pm/URI.html (F); OCaml — (not in std); Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/URI.html; Swift https://developer.apple.com/documentation/foundation/urlcomponents.
> Measurements: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. Probe: scratchpad `probe-url/p1.almd`.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Percent-encode a component (`+` stays `+` unchanged) | `encode_component` / `decode_component` | — | `PathEscape` / `PathUnescape` (F) | `quote` / `unquote` (F) | `encodeURIComponent` / `decodeURIComponent` | `percent_encode` / `percent_decode` | `encode` / `decode` (F) | — | — | `addingPercentEncoding` |
| Form mode (space ⇄ `+`） | `http.url_decode`（decode only, in http） | — | `QueryEscape` / `QueryUnescape`（`+` → space）(F) | `quote_plus` / `unquote_plus` (F) | `URLSearchParams` | — | `encode_www_form` / `decode_www_form`（`+` → space）(F) | — | `URLEncoder` / `URLDecoder`(JVM) | — |
| Decompose/build URL | `parse` / `to_string`（`Url` type） | — | `Parse` / `String` | `urlparse` / `urlunparse` (F) | `new URL` / `href` | `parse` / `to_string` | `parse` / `to_string` (F) | — | `URI` | `URLComponents` |
| Query to sequence of pairs (decoded) | `query_pairs`（**does not decode**） | — | `ParseQuery` (F) | `parse_qsl`（decodes）(F) | `URLSearchParams` | `parse_query` | `decode_query` (F) | — | — | `queryItems` |
| Build query from pair sequence | `build_query` | — | `Values.Encode` | `urlencode` (F) | `URLSearchParams.toString` | `query_to_string` | `encode_query` (F) | — | — | `queryItems` |
| Resolve relative URL against base | **—** | — | `ResolveReference` (F) | `urljoin` (F) | `new URL(ref, base)` | `merge` | `merge` (F) | — | `URI.resolve`(JVM) | `URL(string:relativeTo:)` |

Six languages provide relative URL resolution (Go, Python, JS, Gleam, Elixir, Swift; Kotlin is JVM ◐).

<a id="完全性の規則"></a>

### Completeness rules

1. Only `url` owns percent coding (R-L1-28, R-L0-01).
2. There are two coding modes, each with an `encode_<方式>` / `decode_<方式>` pair: `component` (treat `+` literally; RFC 3986) and `form` (spaces become `+`; application/x-www-form-urlencoded). The placeholder 方式 means mode.
3. Decoding returns `Result[String, String]`. Invalid escapes and invalid UTF-8 are err. Do not use sentinels (returning the input unchanged or substituting U+FFFD; R-L0-06 item 4).
4. Pairs: `parse` ⇄ `to_string` (URL), `parse_query` ⇄ `build_query` (query). `parse_query(build_query(ps)) == ok(ps)` holds.
5. **Intentional omission**: no query-pair variant returning a Map, because keys may repeat; use `map.from_list`.

<a id="l2-の判定"></a>

### L2 verdict

- An apparent duplicate: `http.url_decode` and `url.decode_component` are **not the same** (measured).

  | Input | `url.decode_component` | `http.url_decode` |
  |---|---|---|
  | `a+b%20c` | `ok("a+b c")` | `"a b c"`（`+` to space） |
  | `%zz` | err（invalid percent escape） | `"%zz"`（returns unchanged） |
  | `%ff` | err（invalid UTF-8） | `"�"`（replacement character） |
  | `50%` | err（truncated） | `"50%"` |

  `http.url_decode` is form decoding and hides failures with sentinels (unchanged input or replacement characters). Keep R-L1-28's removal decision, but replace it with a new form-mode function → R-url-01.
- Semantic mismatch: `query_pairs` does not decode (measurement: `"c=x%20y&d=p+q"` → `("c", "x%20y")`, `("d", "p+q")`), whereas `build_query` encodes. They are not a pair. All five other languages' equivalent role decodes → R-url-02.
- Missing role: relative URL resolution → R-url-03.
- `Url` fields: only `port` is Option; other absent fields are `""`. This represents a successful `parse`, so R-L0-06 item 4 (no sentinel on failure) does not apply. Keep it.

<a id="案"></a>

## Proposals

<a id="r-url-01-フォーム形式の対-encode_form--decode_form-を足しhttpurl_decode-を撤去する---採用"></a>

### R-url-01 Add the form pair `encode_form` / `decode_form`, remove `http.url_decode` — ○ Accepted

- Content:
  - Add `url.encode_form(s: String) -> String` (spaces become `+`; otherwise same as `encode_component`), and `url.decode_form(s: String) -> Result[String, String]` (convert `+` to spaces, then decode by the same rules as `decode_component`).
  - Remove `http.url_decode(s) -> String`. **Finalize R-L1-28 here: remove it, replacing it with `url.decode_form`, not `url.decode_component`**; the former has matching `+` treatment.
- L0 basis: R-L1-28, R-L0-01, R-L0-02 (percent coding strings belongs to url), R-L0-06 item 4 (no sentinels), R-L0-14.
- (a) Go (`PathUnescape`/`QueryUnescape`) (F), Python (`unquote`/`unquote_plus`) (F), Elixir (`decode`/`decode_www_form`) (F), JS (`decodeURIComponent`/`URLSearchParams`), and Kotlin (JVM `URLDecoder`) name the two modes separately.
- (b) Pure computation. Use the same self-hosted implementation as today's `url.decode_component`, yielding the same answer on both targets.
- (c) Whether `+` becomes a space is not apparent from the name. Include the mode name so the call's meaning can be read.
- (d) Add 2, remove 1 on the http side. Mark `http.url_decode` with `@deprecated(since=15, use="url.decode_form")`, remove at `removal_epoch = 16`. `almide fix` rewrites `http.url_decode(s)` → `url.decode_form(s) ?? s` (original input on invalid input). The only difference is invalid UTF-8 such as `%ff`: original input instead of the current replacement character. State this difference in the hint. Usage (`http.url_decode`): spec 9 / tests 0 / docs 9 / stdlib 2. Rewrite the 2 stdlib occurrences (`__decode_part` in `http.almd:366` and `http_url_decode.almd`) in the same change.
- Proposed gate: put the four-input table above in `spec/stdlib/url_test.almd`, for 2 modes × 2 targets. Property-test the round trip `decode_form(encode_form(s)) == ok(s)`.
- Issue: #3950.

<a id="r-url-02-クエリの読みを復号する-parse_query-にする---採用"></a>

### R-url-02 Make query reading a decoding `parse_query` — ○ Accepted

- Content: add `url.parse_query(query: String) -> Result[List[(String, String)], String]`. Decode keys and values by `decode_form` rules. A key without a value gets `""` (same as current `query_pairs`). Remove the non-decoding `query_pairs`.
- L0 basis: R-L0-00 condition 1 (`query_pairs` and `build_query` are not a pair), R-L0-14 item 4. R-L0-14 item 4 permits keeping a name if old code becomes a type error. Nevertheless, do not retain `query_pairs`: merely fixing the return-type error with `?? []` leaves later `decode_component` calls type-correct, silently decoding twice so that `%2520` becomes a space. A new name makes the writer read the hint and rewrite. Follow Go's `ParseQuery` (F) and Gleam's `parse_query`, using `parse_<対象>` (“parse target”) like `parse` ⇄ `to_string`.
- (a) All five languages in the same role (Go, Python, JS, Gleam, Elixir) decode.
- (b) Pure computation.
- (c) Today, forgetting to apply `decode_component` to `query_pairs` results silently leaves values containing `%20`.
- (d) Remove 1, add 1. `@deprecated(since=15, use="url.parse_query")`, `removal_epoch = 16`. Since the result becomes `Result` and values are decoded, do not rewrite with `almide fix`. Hint: “`url.parse_query(q) ?? []` returns decoded pairs. Remove `decode_component` calls that expected undecoded values.” Usage (`query_pairs`): spec 3 / tests 0 / docs 4 / stdlib 5 (definitions and comments).
- Proposed gate: property-test `parse_query(build_query(ps)) == ok(ps)`.
- Issue: #3951.

<a id="r-url-03-相対-url-の解決-resolve-を足す---採用"></a>

### R-url-03 Add relative URL resolution `resolve` — ○ Accepted

- Content: `url.resolve(base: Url, ref: String) -> Result[Url, String]`, following RFC 3986 §5 (reference resolution). An absolute `ref` returns the result of parsing it unchanged.
- L0 basis: L2 criterion (a role present in at least 5 languages but absent in Almide), R-L0-02 (subject `Url`), R-L0-03 (subject first).
- (a) Six languages have it. Spellings split among `ResolveReference` (F), `urljoin` (F), `merge` (F), `resolve`, with no majority. Choose the JVM `URI.resolve` verb, matching RFC 3986's “resolve”.
- (b) Pure computation. Use RFC 3986 §5.4 examples (5.4.1 normal, 5.4.2 abnormal) as native and wasm fixtures.
- (c) Manually joining relative links mishandles `../` and `?`.
- (d) Add 1.
- Proposed gate: turn RFC 3986 §5.4's example table directly into tests.
- Issue: #3952.

<a id="r-url-04-url-の欄を-option-にする---不採用"></a>

### R-url-04 Make `Url` fields Option — × Rejected

- Reason: distinguishing absent from empty `query` and `fragment` is rarely needed; changing field types breaks every read of `u.query` (R-L0-14 item 4). `port` is already Option.
- Issue: none.

<a id="r-url-05-url-に-userinfo-を足す---採用"></a>

### R-url-05 Add userinfo to `Url` — ○ Accepted

- Current: `type Url = { scheme, host, port: Int?, path, query, fragment }`. `url.parse("https://user:pw@example.com/x")` returns `err("userinfo is not supported")`.
- Proposal: add `userinfo: String` (`""` when absent, like `query` and `fragment`); support it in `parse`, `to_string`, and `resolve`.
- (a) Six languages read it: Go `URL.User`, Python `urlparse().username`, JS `URL.username`, Elixir `URI.userinfo`, Gleam `Uri.userinfo`, JVM `URI.getUserInfo`.
- (c) Reading `DATABASE_URL` (`postgres://user:pass@host/db`) is a frequent task; today it produces err only at runtime.
- (d) Type change. Record construction `Url { ... }` becomes a missing-field type error, satisfying R-L0-14.4 (hint: add `userinfo: ""`). Read-only callers remain unchanged.
- Proposed gate: use RFC 3986 authority examples as fixtures on both targets and include userinfo in property tests of `parse(to_string(u)) == ok(u)`.
- Issue: #3971.

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `encode_component` | `url.encode_component(s: String) -> String` | JS `encodeURIComponent` / Go `PathEscape` (F) / Python `quote` (F) / Elixir `encode` (F) / Gleam `percent_encode` | Keep | R-url-01 |
| `decode_component` | `url.decode_component(s: String) -> Result[String, String]` | JS `decodeURIComponent` / Go `PathUnescape` (F) / Python `unquote` (F) / Elixir `decode` (F) | Keep | R-url-01 |
| `parse` | `url.parse(s: String) -> Result[Url, String]` | Go `url.Parse` / Python `urlparse` (F) / JS `new URL` / Elixir `URI.parse` (F) / Gleam `uri.parse` | Semantic change (accept userinfo; add a field to `Url`, named `userinfo`) | R-url-05 |
| `to_string` | `url.to_string(u: Url) -> String` | Go `String` / Elixir `to_string` (F) / Gleam `to_string` / JS `href` | Keep | — |
| `query_pairs` | `url.query_pairs(query: String) -> List[(String, String)]` | Python `parse_qsl`（decodes）(F) / Go `ParseQuery` (F) / Elixir `decode_query` (F) | Remove → `parse_query` | R-url-02 |
| `build_query` | `url.build_query(pairs: List[(String, String)]) -> String` | Python `urlencode` (F) / Elixir `encode_query` (F) / Go `Values.Encode` / Gleam `query_to_string` | Keep | R-url-02 |
| `encode_form` | `url.encode_form(s: String) -> String`（Proposal） | Go `QueryEscape` (F) / Python `quote_plus` (F) / Elixir `encode_www_form` (F) | Add | R-url-01 |
| `decode_form` | `url.decode_form(s: String) -> Result[String, String]`（Proposal） | Go `QueryUnescape` (F) / Python `unquote_plus` (F) / Elixir `decode_www_form` (F) | Add（`http.url_decode` replacement） | R-url-01 |
| `parse_query` | `url.parse_query(query: String) -> Result[List[(String, String)], String]`（Proposal） | Go `ParseQuery` (F) / Gleam `parse_query` / Python `parse_qsl` (F) | Add | R-url-02 |
| `resolve` | `url.resolve(base: Url, ref: String) -> Result[Url, String]`（Proposal） | Go `ResolveReference` (F) / Python `urljoin` (F) / JS `new URL(ref, base)` / Elixir `merge` (F) / JVM `URI.resolve` | Add | R-url-03 |

(Reference: row from another module.)

| Function | Signature (current, mut shown) | Main spellings elsewhere | Verdict | Proposal |
|---|---|---|---|---|
| `http.url_decode` | `http.url_decode(s: String) -> String` | Go `QueryUnescape` (F) / Python `unquote_plus` (F) | Remove → `url.decode_form(s) ?? s` | R-url-01 |
