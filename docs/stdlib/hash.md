# hash

Non-cryptographic digests and SHA-256. `import hash`.

Pure — no capability, byte-identical on both targets (C-299). The wasm leg is
fully self-hosted; no new intrinsic.

### `hash.fnv1a32(b: Bytes) -> Int`

FNV-1a over the bytes — the **32-bit** variant (offset basis
2166136261, prime 16777619), returned as a non-negative Int in `0..2^32`.
32-bit on purpose: every step masks to 32 bits, so the arithmetic is exact in
i64 on both targets. Use it for cache keys, dedup, and bucketing — never for
integrity or security.

```almd run
import hash

fn main() -> Unit = {
  println("${hash.fnv1a32(bytes.from_string(""))}")
  println("${hash.fnv1a32(bytes.from_string("foobar"))}")
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

### `hash.sha256_hex(b: Bytes) -> String`

SHA-256 of the bytes as 64 lowercase hex characters — the common one-call
form. A string is hashed through its UTF-8: `hash.sha256_hex(bytes.from_string(s))`.

```almd run
import hash

fn main() -> Unit = {
  println(hash.sha256_hex(bytes.from_string("")))
}
```
```output
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

Out of scope, deliberately: TLS, AEAD, and asymmetric crypto (#1467 keeps
those a separate decision). Keyed hashing (HMAC) composes from `sha256` when
needed.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (12 functions)

```
// 32-bit FNV-1a of s's UTF-8; non-negative Int.
// @since 0.58.0 or earlier
hash.fnv1a32(s: String) -> Int

// 32-bit FNV-1a of b, in 0..4294967295.
// @since 0.58.0 or earlier
hash.fnv1a32_bytes(b: Bytes) -> Int

// 32-byte SHA-256 digest of b.
// @since 0.2.2 or earlier
hash.sha256(b: Bytes) -> Bytes

// SHA-256 of s's UTF-8, 64 lowercase hex chars.
// @since 0.58.0 or earlier
hash.sha256_hex(s: String) -> String

// CRC-32 (IEEE, reflected) as an unsigned Int; 0 for empty b.
// @since unreleased
hash.crc32(b: Bytes) -> Int

// Adler-32 as an unsigned Int; 1 for empty b.
// @since unreleased
hash.adler32(b: Bytes) -> Int

// 16-byte MD5 digest of b. For compatibility only: MD5 is broken, never use it for security.
// @since unreleased
hash.md5(b: Bytes) -> Bytes

// MD5 of b as 32 lowercase hex chars. For compatibility only, never for security.
// @since unreleased
hash.md5_hex(b: Bytes) -> String

// 20-byte SHA-1 digest of b. For compatibility only: SHA-1 is broken, never use it for security.
// @since unreleased
hash.sha1(b: Bytes) -> Bytes

// SHA-1 of b as 40 lowercase hex chars. For compatibility only, never for security.
// @since unreleased
hash.sha1_hex(b: Bytes) -> String

// 64-byte SHA-512 digest of b.
// @since unreleased
hash.sha512(b: Bytes) -> Bytes

// SHA-512 of b as 128 lowercase hex chars.
// @since unreleased
hash.sha512_hex(b: Bytes) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
