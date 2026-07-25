# hex

Hexadecimal encoding and decoding. `import hex`.

Pure — no capability, identical on both targets. Two characters per byte, no
separators and no prefix.

### `hex.encode(b: Bytes) -> String`

Lowercase digits.

```almd
hex.encode(bytes.from_list([222, 173, 190, 239]))  // "deadbeef"
```

### `hex.encode_upper(b: Bytes) -> String`

Uppercase digits — the spelling checksum output usually takes.

```almd
hex.encode_upper(bytes.from_list([222, 173]))  // "DEAD"
```

### `hex.decode(s: String) -> Result[Bytes, String]`

Decode a hex string. Both cases are accepted, and they may be mixed. `err` on an
odd length or a non-hex character.

```almd
match hex.decode(digest) {
  ok(b) => verify(b),
  err(e) => err("bad hex: " + e),
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
hex.encode(b: Bytes) -> String
hex.encode_upper(b: Bytes) -> String
hex.decode(s: String) -> Result[Bytes, String]
```

<!-- END GENERATED SIGNATURE INDEX -->
