# hash

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](hash.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](hash.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](hash.zh-CN.md)



> 原文：[hash.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/hash.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文注意：Secret 的显示保护不代表加密或所有路径防泄漏。恒定时间比较存在正文所述长度不匹配立即返回的条件。引用标准或陈述测试向量一致不表示取得认证或经过本译文的独立审计。

校验和、摘要（SHA-256/512）和 HMAC。`import hash`。

pure，无需 capability，两个目标逐字节相同（C-299）。wasm 路径完全自托管，没有新增 intrinsic。

### `hash.fnv1a32(b: Bytes) -> Int`

对字节执行 FNV-1a，**32-bit** 版本（offset basis 2166136261、prime 16777619），返回 `0..2^32` 内非负 Int。32-bit 是有意选择：每一步屏蔽为 32 位，因此在两个目标的 i64 上算术精确。用于 cache key、dedup 和 bucketing，绝不用于完整性或 security。

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

FIPS 180-4 SHA-256 摘要：32 字节。用于 content addressing 与完整性检查，配合 `hex.encode` 得到可打印形式。

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

字节的 SHA-256，表示为 64 个小写十六进制字符，是常用的一次调用形式。字符串通过 UTF-8 哈希：`hash.sha256_hex(bytes.from_string(s))`。

```almd run
import hash

fn main() -> Unit = {
  println(hash.sha256_hex(bytes.from_string("")))
}
```
```output
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

### `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`

在 `key` 下对 `b` 求 HMAC-SHA256（RFC 2104），**key 在前**。key 与 tag 都为 `Secret[Bytes]`（`import secret`）：tag 打印为 `Secret(***)`，其 `==` 为恒定时间比较（C-426），因此 MAC 不会用泄漏信息的 `==` 比较。reveal 后的 tag 是 32 字节；`hmac_sha512` 是 SHA-512 单元（64 字节）。两个目标均与 RFC 4231 vector 相同（C-422）。没有 `_hex` 单元；签名 header 值用 `hex.encode(secret.reveal(tag))`。绝不要用 `hash.sha256(key ++ msg)` 构造 MAC，因为它可被 length extension 伪造。

```almd run
import hash
import hex
import secret

fn main() -> Unit = {
  let key = secret.new(bytes.from_string("Jefe"))
  let tag = hash.hmac_sha256(key, bytes.from_string("what do ya want for nothing?"))
  println("${tag}")
  println(hex.encode(secret.reveal(tag)))
}
```
```output
Secret(***)
5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843
```

### `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool`

验证 MAC：当且仅当 `tag` 是 `key` 下 `b` 的 HMAC-SHA256 时为 true，使用恒定时间比较（C-423）。这是 webhook 签名 header 应使用的调用。header 以十六进制携带 tag；用 `hex.decode`（大小写均可）解码，再用 `secret.new` 包装。`?? bytes.from_list([])` 将错误十六进制变为空 tag，验证为 `false`，不会 abort。没有 `_hex` 验证单元：`_hex` 始终只表示十六进制输出。`sha512` 对具有相同形式。

```almd run
import hash
import hex
import secret

fn main() -> Unit = {
  let key = secret.new(bytes.from_string("Jefe"))
  let body = bytes.from_string("what do ya want for nothing?")
  let header = "5BDCC146BF60754E6A042426089575C75A003F089D2739839DEC58B964EC3843"
  let tag = secret.new(hex.decode(header) ?? bytes.from_list([]))
  println("${hash.is_valid_hmac_sha256(key, body, tag)}")
  println("${hash.is_valid_hmac_sha256(key, bytes.from_string("tampered"), tag)}")
}
```
```output
true
false
```

这些调用使用 `bytes.is_equal_constant_time` / `string.is_equal_constant_time` 比较：长度不匹配立即为 false，否则读取每个字节，不按内容分支。

MD5 和 SHA-1 不在 stdlib 中：其 security 性质已被攻破，`hash.md5`、`md5_hex`、`sha1`、`sha1_hex` 于 dialect epoch 15 移除（C-424）。调用时在 check 阶段报错，hint 指向 `hash.sha256`。

有意排除的范围：TLS、AEAD、签名和非对称密码学（#2596 的 `crypto` module）。



## 固定快照 API 索引 (11 个函数)

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

// 64-byte SHA-512 digest of b. Not a MAC: sha512(key ++ msg) is forgeable by length extension; use hash.hmac_sha512 / hash.is_valid_hmac_sha512.
// @since unreleased
hash.sha512(b: Bytes) -> Bytes

// SHA-512 of b as 128 lowercase hex chars.
// @since unreleased
hash.sha512_hex(b: Bytes) -> String

// 32-byte HMAC-SHA256 of b under key (RFC 2104), key first; the tag is a Secret, so its == is constant time. Hex: hex.encode(secret.reveal(tag)).
// @since unreleased
hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]

// 64-byte HMAC-SHA512 of b under key (RFC 2104), key first; the tag is a Secret, so its == is constant time. Hex: hex.encode(secret.reveal(tag)).
// @since unreleased
hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]

// True if tag is the HMAC-SHA256 of b under key, compared in constant time. A hex tag: hash.is_valid_hmac_sha256(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([]))).
// @since unreleased
hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool

// True if tag is the HMAC-SHA512 of b under key, compared in constant time. A hex tag: hash.is_valid_hmac_sha512(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([]))).
// @since unreleased
hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool
```



## API 说明译文（按上述签名顺序）

- `hash.fnv1a32(b: Bytes) -> Int` — b 的 32-bit FNV-1a，范围 0..4294967295。
- `hash.sha256(b: Bytes) -> Bytes` — b 的 32-byte SHA-256 摘要。不是 MAC：sha256(key ++ msg) 可通过 length extension 伪造；使用 hash.hmac_sha256 / hash.is_valid_hmac_sha256。
- `hash.sha256_hex(b: Bytes) -> String` — b 的 SHA-256，64 个小写十六进制字符。
- `hash.crc32(b: Bytes) -> Int` — CRC-32（IEEE，反射型），作为 unsigned Int；空 b 为 0。
- `hash.adler32(b: Bytes) -> Int` — Adler-32，作为 unsigned Int；空 b 为 1。
- `hash.sha512(b: Bytes) -> Bytes` — b 的 64-byte SHA-512 摘要。不是 MAC：sha512(key ++ msg) 可通过 length extension 伪造；使用 hash.hmac_sha512 / hash.is_valid_hmac_sha512。
- `hash.sha512_hex(b: Bytes) -> String` — b 的 SHA-512，128 个小写十六进制字符。
- `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key 下 b 的 32-byte HMAC-SHA256（RFC 2104），key 在前；tag 是 Secret，因此 == 为恒定时间。十六进制：hex.encode(secret.reveal(tag))。
- `hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key 下 b 的 64-byte HMAC-SHA512（RFC 2104），key 在前；tag 是 Secret，因此 == 为恒定时间。十六进制：hex.encode(secret.reveal(tag))。
- `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag 是 key 下 b 的 HMAC-SHA256 时为 true，恒定时间比较。十六进制 tag：hash.is_valid_hmac_sha256(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。
- `hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag 是 key 下 b 的 HMAC-SHA512 时为 true，恒定时间比较。十六进制 tag：hash.is_valid_hmac_sha512(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。
