# hash

Checksums, digests (SHA-256/512, MD5/SHA-1 for compatibility) and HMAC. `import hash`.

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

### `hash.hmac_sha256(key: Bytes, b: Bytes) -> Bytes`

HMAC-SHA256 (RFC 2104) of `b` under `key` — **key first**. 32 bytes; the
`_hex` cell gives 64 lowercase hex characters, and `hmac_sha512` /
`hmac_sha512_hex` are the SHA-512 pair. Equal to the RFC 4231 vectors on
both targets (C-422). Never build a MAC as `hash.sha256(key ++ msg)`: that
is forgeable by length extension.

```almd run
import hash

fn main() -> Unit = {
  println(hash.hmac_sha256_hex(bytes.from_string("Jefe"), bytes.from_string("what do ya want for nothing?")))
}
```
```output
5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843
```

### `hash.is_valid_hmac_sha256(key: Bytes, b: Bytes, tag: Bytes) -> Bool`

Verify a MAC: true iff `tag` is the HMAC-SHA256 of `b` under `key`, compared
in constant time (C-423). This is the call for a webhook signature header — do
not compute the MAC and compare it with `==`, which returns at the first
differing byte and leaks the MAC through timing. A header carries the tag as
hex: decode it with `hex.decode` (either case). `?? bytes.from_list([])` turns
bad hex into the empty tag, which verifies `false`, never an abort. There is no
`_hex` verify cell: `_hex` only ever names a hex OUTPUT. The `sha512` pair has
the same shape.

```almd run
import hash
import hex

fn main() -> Unit = {
  let key = bytes.from_string("Jefe")
  let body = bytes.from_string("what do ya want for nothing?")
  let header = "5BDCC146BF60754E6A042426089575C75A003F089D2739839DEC58B964EC3843"
  let tag = hex.decode(header) ?? bytes.from_list([])
  println("${hash.is_valid_hmac_sha256(key, body, tag)}")
  println("${hash.is_valid_hmac_sha256(key, bytes.from_string("tampered"), tag)}")
}
```
```output
true
false
```

`bytes.is_equal_constant_time` / `string.is_equal_constant_time` are the
compare these use: a length mismatch is false at once, and otherwise every
byte is read with no branch on the contents.

`md5`, `md5_hex`, `sha1` and `sha1_hex` are for compatibility only: they
carry `@insecure_digest`, and `--profile critical` refuses a call to one
(E100).

Out of scope, deliberately: TLS, AEAD, signatures and asymmetric crypto
(#2596's `crypto` module).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (17 functions)

```
// 32-bit FNV-1a of b, in 0..4294967295.
// @since 0.58.0 or earlier
hash.fnv1a32(b: Bytes) -> Int

// 32-byte SHA-256 digest of b. Not a MAC: sha256(key ++ msg) is forgeable by length extension; use hash.hmac_sha256 / hash.is_valid_hmac_sha256.
// @since 0.2.2 or earlier
hash.sha256(b: Bytes) -> Bytes

// SHA-256 of b as 64 lowercase hex chars.
// @since 0.58.0 or earlier
hash.sha256_hex(b: Bytes) -> String

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

// 64-byte SHA-512 digest of b. Not a MAC: sha512(key ++ msg) is forgeable by length extension; use hash.hmac_sha512 / hash.is_valid_hmac_sha512.
// @since unreleased
hash.sha512(b: Bytes) -> Bytes

// SHA-512 of b as 128 lowercase hex chars.
// @since unreleased
hash.sha512_hex(b: Bytes) -> String

// 32-byte HMAC-SHA256 of b under key (RFC 2104); key first.
// @since unreleased
hash.hmac_sha256(key: Bytes, b: Bytes) -> Bytes

// HMAC-SHA256 of b under key as 64 lowercase hex chars.
// @since unreleased
hash.hmac_sha256_hex(key: Bytes, b: Bytes) -> String

// 64-byte HMAC-SHA512 of b under key (RFC 2104); key first.
// @since unreleased
hash.hmac_sha512(key: Bytes, b: Bytes) -> Bytes

// HMAC-SHA512 of b under key as 128 lowercase hex chars.
// @since unreleased
hash.hmac_sha512_hex(key: Bytes, b: Bytes) -> String

// True if tag is the HMAC-SHA256 of b under key, compared in constant time. Use this, not ==. A hex tag: hash.is_valid_hmac_sha256(key, b, hex.decode(sig) ?? bytes.from_list([])).
// @since unreleased
hash.is_valid_hmac_sha256(key: Bytes, b: Bytes, tag: Bytes) -> Bool

// True if tag is the HMAC-SHA512 of b under key, compared in constant time. Use this, not ==. A hex tag: hash.is_valid_hmac_sha512(key, b, hex.decode(sig) ?? bytes.from_list([])).
// @since unreleased
hash.is_valid_hmac_sha512(key: Bytes, b: Bytes, tag: Bytes) -> Bool
```

<!-- END GENERATED SIGNATURE INDEX -->
