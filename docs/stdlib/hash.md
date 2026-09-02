# hash

Non-cryptographic digests and SHA-256. `import hash`.

Pure — no capability, byte-identical on both targets (C-299). The wasm leg is
fully self-hosted; no new intrinsic.

### `hash.fnv1a32(s: String) -> Int`

FNV-1a over the string's UTF-8 bytes — the **32-bit** variant (offset basis
2166136261, prime 16777619), returned as a non-negative Int in `0..2^32`.
32-bit on purpose: every step masks to 32 bits, so the arithmetic is exact in
i64 on both targets. Use it for cache keys, dedup, and bucketing — never for
integrity or security.

```almd run
import hash

fn main() -> Unit = {
  println("${hash.fnv1a32("")}")
  println("${hash.fnv1a32("foobar")}")
}
```
```output
2166136261
3214735720
```

### `hash.fnv1a32_bytes(b: Bytes) -> Int`

The same digest over raw bytes.

```almd run
import hash

fn main() -> Unit = {
  println("${hash.fnv1a32_bytes(bytes.from_string("foobar"))}")
}
```
```output
3214735720
```

### `hash.sha256(b: Bytes) -> Bytes`

The FIPS 180-4 SHA-256 digest: 32 bytes. Content addressing and integrity
checks; pair with `hex.encode` for the printable form.

```almd run
import hash
import hex

fn main() -> Unit = {
  println(hex.encode(hash.sha256(bytes.from_string("abc"))))
}
```
```output
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
```

### `hash.sha256_hex(s: String) -> String`

SHA-256 of the string's UTF-8 bytes as 64 lowercase hex characters — the
common one-call form.

```almd run
import hash

fn main() -> Unit = {
  println(hash.sha256_hex(""))
}
```
```output
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

Out of scope, deliberately: TLS, AEAD, and asymmetric crypto (#1467 keeps
those a separate decision). Keyed hashing (HMAC) composes from `sha256` when
needed.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (4 functions)

```
hash.fnv1a32(s: String) -> Int
hash.fnv1a32_bytes(b: Bytes) -> Int
hash.sha256(b: Bytes) -> Bytes
hash.sha256_hex(s: String) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
