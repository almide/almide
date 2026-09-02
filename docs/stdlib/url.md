# url

URL parsing, building, and percent-encoding (RFC 3986-lite). `import url`.

Pure — no capability, identical on both targets. Three family pairs, each
closed under round-trip: `parse ⇄ to_string`, `encode_component ⇄
decode_component`, `query_pairs ⇄ build_query`. Failures are `Result`s with
reasons (the `int.parse` precedent) — no panicking variants.

Scope (documented limits, not accidents): schemes with an authority only
(`scheme://…`); no userinfo, no IPv6 bracket hosts, no punycode. `+` is NOT
decoded as space — that is form-encoding, not RFC 3986. A URL whose query or
fragment is present but empty (`http://h?`) round-trips to the
separator-free form.

### `url.parse(s: String) -> Result[Url, String]`

Parses `scheme://host[:port][/path][?query][#fragment]` into the `Url`
record. `port` is `none` when absent; `path` is `""` when the authority is
not followed by `/`; `query`/`fragment` are `""` when their separators are
absent. Rejects inputs without a `scheme://` authority and ports outside
`0..=65535`, with the reason in the `err`.

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

The inverse of `parse`: reassembles the record, emitting `:port`, `?query`
and `#fragment` only when present.

### `url.encode_component(s: String) -> String`

Percent-encodes every byte outside RFC 3986's unreserved set (ALPHA / DIGIT
/ `-` `.` `_` `~`), UTF-8 first, uppercase hex digits.

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

The inverse of `encode_component`. A broken escape (`%zz`, truncated `%A`)
is an `err` naming the position; `+` passes through unchanged.

### `url.query_pairs(query: String) -> List[(String, String)]`

Splits a raw query string on `&` and `=` into ordered pairs. A key without
`=` yields `(key, "")`; the empty string yields `[]`. Values are NOT
percent-decoded (compose with `decode_component` when needed).

### `url.build_query(pairs: List[(String, String)]) -> String`

The inverse of `query_pairs`, percent-encoding each key and value with
`encode_component`.

```almd run
import url

fn main() -> Unit = {
  println(url.build_query([("q", "a b"), ("lang", "ja")]))
}
```
```output
q=a%20b&lang=ja
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (6 functions)

```
url.encode_component(s: String) -> String
url.decode_component(s: String) -> Result[String, String]
url.parse(s: String) -> Result[Url, String]
url.to_string(u: Url) -> String
url.query_pairs(query: String) -> List[()]
url.build_query(pairs: List[()]) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
