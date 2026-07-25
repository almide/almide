# base64

Base64 encoding and decoding. `import base64`.

Pure — no capability, identical on both targets. Encoding takes `Bytes` and
produces a `String`; decoding is fallible, so it returns a `Result`.

Use `bytes.from_string` / `bytes.to_string` at the edges when the payload is
text rather than binary.

### `base64.encode(b: Bytes) -> String`

Standard alphabet (RFC 4648 §4), with `=` padding.

```almd
base64.encode(bytes.from_string("hello"))  // "aGVsbG8="
```

### `base64.decode(s: String) -> Result[Bytes, String]`

Decode the standard alphabet. `err` on an invalid character or a bad length.

```almd
match base64.decode(payload) {
  ok(b) => process(b),
  err(e) => err("bad base64: " + e),
}
```

### `base64.encode_url(b: Bytes) -> String`

URL- and filename-safe alphabet (RFC 4648 §5): `-` and `_` replace `+` and `/`.
Padding is KEPT, so the output still ends in `=` when the input length is not a
multiple of three.

```almd
base64.encode_url(bytes.from_string("hi?>"))  // "aGk_Pg=="
```

Contexts that want unpadded base64url — JWT segments, for instance — should
strip it: `string.replace(base64.encode_url(b), "=", "")`.

### `base64.decode_url(s: String) -> Result[Bytes, String]`

Decode the URL-safe alphabet. Accepts input with or without padding, so it round-
trips both `encode_url` and the unpadded JWT form.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (4 functions)

```
base64.encode(b: Bytes) -> String
base64.decode(s: String) -> Result[Bytes, String]
base64.encode_url(b: Bytes) -> String
base64.decode_url(s: String) -> Result[Bytes, String]
```

<!-- END GENERATED SIGNATURE INDEX -->
