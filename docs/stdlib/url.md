# url

URL parsing, building, and percent-encoding (RFC 3986-lite). `import url`.

Pure — no capability, identical on both targets. Three family pairs, each
closed under round-trip: `parse ⇄ to_string`, `encode_component ⇄
decode_component`, `parse_query ⇄ build_query`. Failures are `Result`s with
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
absent. Rejects, with the reason in the `err`: inputs without a `scheme://`
authority; an empty host (`http://`, `http://:8`); a host outside RFC 3986's
reg-name characters without pct-encoding (unreserved, sub-delims — a `%` is
refused); userinfo (`user@host`)
and IPv6 literals (`[::1]`), which are out of scope and refused by name rather
than read as a host; and a port that is not all ASCII digits or is outside
`0..=65535`.

Where a browser could read the URL as pointing somewhere else, `url.parse`
reads it as the WHATWG URL parser does or refuses it (ALS-T27, C-402, #4116):

| Rule | Example |
|---|---|
| Leading/trailing C0 control and space are stripped; tab, LF, CR are removed anywhere | `" http://good.com"` → host `good.com` |
| The scheme is `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` before the FIRST `:`, then `//` | `javascript:alert(1)//://good.com` → `url.parse: missing '://' scheme separator` (it used to parse with host `good.com`) |
| A backslash or another control character is refused | `http:\\evil.com` → `url.parse: backslash in URL` |
| A numeric host must be four canonical decimal labels | `http://127.1/`, `http://0x7f.0.0.1/` → `url.parse: non-canonical IPv4 host: …` |
| Scheme and host are lowercased | `HTTP://Good.COM/x` → `http://good.com/x` |
| `.` and `..` segments are removed, `%2e` spellings too | `http://good.com/a/%2e%2e/b` → path `/b` |

Measured on the same probes (2026-10-24): WHATWG (Node 22 `URL`) and Python
3.14 `urllib` strip the whitespace, Go 1.27 `net/url` refuses it; WHATWG reads
`\` as `/` and canonicalises `127.1` to `127.0.0.1`, which `url.parse` refuses
instead; WHATWG removes `%2e` segments, Go decodes them without removing them,
Python keeps them.

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

### `url.build_query(pairs: List[(String, String)]) -> String`

The inverse of `parse_query`, percent-encoding each key and value with
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

## Signature index (9 functions)

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

## Type index (1 types)

```
// Parsed URL; port none, other parts "" when absent.
// @since 0.6.0 or earlier
type url.Url = { scheme: String, host: String, port: Option[Int], path: String, query: String, fragment: String }
```

<!-- END GENERATED SIGNATURE INDEX -->
