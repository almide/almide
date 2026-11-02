# hex

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](hex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](hex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](hex.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/hex.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


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

```almd check
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



## Signature index (3 functions)

```
// Lowercase hex, two chars per byte.
// @since 0.13.4 or earlier
hex.encode(b: Bytes) -> String

// Uppercase hex, two chars per byte.
// @since 0.13.4 or earlier
hex.encode_upper(b: Bytes) -> String

// Bytes from hex (any case); err if odd or bad char.
// @since 0.13.4 or earlier
hex.decode(s: String) -> Result[Bytes, String]
```


