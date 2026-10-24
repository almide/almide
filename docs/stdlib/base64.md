# base64

Base64 encoding and decoding. `import base64`.

Pure — no capability, identical on both targets. Encoding takes `Bytes` and
produces a `String`; decoding is fallible, so it returns a `Result`.

Use `bytes.from_string` / `bytes.to_string` at the edges when the payload is
text rather than binary.

### `base64.encode(b: Bytes) -> String`

Standard alphabet (RFC 4648 §4), with `=` padding.

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

Decode the standard alphabet (`+` `/`). Only the canonical padded form that
`encode` emits is accepted, so one byte string has exactly one spelling (#4119):

- the length must be a multiple of 4 — `"invalid base64 length: <n>"`;
- at most two `=`, only at the end — `"invalid base64 padding"` for more;
- every other character must be in the standard alphabet — a `-` or `_` (the
  URL-safe pair), a `=` before the end, or any other byte is
  `"invalid base64 character"`;
- the bits the padding drops must be zero — `QR==` (which would also read as
  `A`) is `"invalid base64 trailing bits"`.

The checks run in that order; the first failure is the answer.

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

URL- and filename-safe alphabet (RFC 4648 §5): `-` and `_` replace `+` and `/`.
Padding is KEPT, so the output still ends in `=` when the input length is not a
multiple of three.

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

Contexts that want unpadded base64url — JWT segments, for instance — should
strip it: `string.replace(base64.encode_url(b), "=", "")`.

### `base64.decode_url(s: String) -> Result[Bytes, String]`

Decode the URL-safe alphabet (`-` `_`) under the same canonical rules as
`decode`: padded, at most two `=`, zero trailing bits, and `+` or `/` is
`"invalid base64 character"`. It round-trips `encode_url`. An unpadded segment
(a JWT part, for instance) must have its padding restored first:

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (4 functions)

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

<!-- END GENERATED SIGNATURE INDEX -->
