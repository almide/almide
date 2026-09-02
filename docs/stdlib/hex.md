# hex

Hexadecimal encoding and decoding. `import hex`.

Pure — no capability, identical on both targets. Two characters per byte, no
separators and no prefix.

### `hex.encode(b: Bytes) -> String`

Lowercase digits.

```almd run
import hex

fn main() -> Unit = {
  println(hex.encode(bytes.from_list([222, 173, 190, 239])))
}
```
```output
deadbeef
```

### `hex.encode_upper(b: Bytes) -> String`

Uppercase digits — the spelling checksum output usually takes.

```almd run
import hex

fn main() -> Unit = {
  println(hex.encode_upper(bytes.from_list([222, 173])))
}
```
```output
DEAD
```

### `hex.decode(s: String) -> Result[Bytes, String]`

Decode a hex string. Both cases are accepted, and they may be mixed. `err` on an
odd length or a non-hex character.

```almd run
import hex

fn show(r: Result[Bytes, String]) -> String = match r {
  ok(b) => "ok(${bytes.to_list(b)})",
  err(e) => "err(\"${e}\")",
}

fn verify(b: Bytes) -> Result[Bytes, String] =
  if bytes.len(b) == 2 then ok(b) else err("expected 2 bytes")

fn check(digest: String) -> Result[Bytes, String] =
  match hex.decode(digest) {
    ok(b) => verify(b),
    err(e) => err("bad hex: " + e),
  }

fn main() -> Unit = {
  println(show(check("DEad")))
  println(show(check("dea")))
  println(show(check("zz")))
}
```
```output
ok([222, 173])
err("bad hex: hex string has odd length: 3")
err("bad hex: invalid hex char at 0")
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
hex.encode(b: Bytes) -> String
hex.encode_upper(b: Bytes) -> String
hex.decode(s: String) -> Result[Bytes, String]
```

<!-- END GENERATED SIGNATURE INDEX -->
