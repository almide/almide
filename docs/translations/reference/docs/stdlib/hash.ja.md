# hash

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](hash.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](hash.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](hash.zh-CN.md)



> 原文：[hash.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/hash.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の注意: Secret の表示保護は暗号化や全経路の漏えい防止を意味しません。定時間比較には本文の長さ不一致時の早期終了条件があります。規格への言及やテストベクトル一致の記述は、認証取得やこの翻訳による独立監査を意味しません。

checksum、digest（SHA-256/512）、HMAC。`import hash`。

pure で capability は不要、両ターゲットでバイト同一です（C-299）。wasm 側は完全にセルフホストされ、新しい intrinsic はありません。

### `hash.fnv1a32(b: Bytes) -> Int`

バイト列の FNV-1a。**32-bit** 版（offset basis 2166136261、prime 16777619）で、`0..2^32` の非負 Int を返します。32-bit は意図的です。各 step で 32 bit にマスクし、両ターゲットの i64 で正確な算術になります。cache key、dedup、bucketing に使い、完全性や security には決して使わないでください。

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

FIPS 180-4 SHA-256 digest、32 バイト。content addressing と完全性検査に使い、表示形式には `hex.encode` と組み合わせます。

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

バイト列の SHA-256 を 64 小文字十六進文字にします。よく使う一回呼出し形式です。文字列は UTF-8 を通じて hash します。`hash.sha256_hex(bytes.from_string(s))`。

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

`key` の下での `b` の HMAC-SHA256（RFC 2104）。**key が先**です。key と tag は `Secret[Bytes]`（`import secret`）です。tag は `Secret(***)` と表示され、その `==` は定時間比較（C-426）なので、MAC を情報が漏れる `==` で比較しません。reveal した tag は 32 バイト。`hmac_sha512` は SHA-512 のセル（64 バイト）です。両ターゲットで RFC 4231 vector と一致します（C-422）。`_hex` セルはありません。署名の header 値は `hex.encode(secret.reveal(tag))` です。MAC を `hash.sha256(key ++ msg)` として構築してはいけません。length extension で偽造可能です。

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

MAC を検証します。`tag` が `key` の下での `b` の HMAC-SHA256 と定時間比較で一致する場合に限り true（C-423）。webhook の署名 header にはこの呼出しを使います。header は tag を十六進で保持するため、`hex.decode`（大小文字可）で decode し、`secret.new` で包みます。`?? bytes.from_list([])` は不正な十六進を空 tag に変え、検証は `false` となり abort にはなりません。`_hex` 検証セルはありません。`_hex` は十六進出力だけを指します。`sha512` 対も同じ形です。

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

これらが使う比較は `bytes.is_equal_constant_time` / `string.is_equal_constant_time` です。長さ不一致は即座に false、それ以外は内容による分岐なしで全バイトを読みます。

MD5 と SHA-1 は stdlib にありません。security 用途では破られているため、`hash.md5`、`md5_hex`、`sha1`、`sha1_hex` は dialect epoch 15 で削除されました（C-424）。呼出しは check 時エラーとなり、hint は `hash.sha256` を示します。

TLS、AEAD、署名、非対称暗号は意図的に対象外です（#2596 の `crypto` module）。



<a id="signature-index-11-functions"></a>

## 固定スナップショットの API 索引 (11 関数)

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



## API 説明の訳（上記署名順）

- `hash.fnv1a32(b: Bytes) -> Int` — b の 32-bit FNV-1a、0..4294967295。
- `hash.sha256(b: Bytes) -> Bytes` — b の 32-byte SHA-256 digest。MAC ではありません。sha256(key ++ msg) は length extension で偽造可能です。hash.hmac_sha256 / hash.is_valid_hmac_sha256 を使います。
- `hash.sha256_hex(b: Bytes) -> String` — b の SHA-256 を 64 小文字十六進文字で返します。
- `hash.crc32(b: Bytes) -> Int` — CRC-32（IEEE、反射型）を unsigned Int で返します。空の b は 0。
- `hash.adler32(b: Bytes) -> Int` — Adler-32 を unsigned Int で返します。空の b は 1。
- `hash.sha512(b: Bytes) -> Bytes` — b の 64-byte SHA-512 digest。MAC ではありません。sha512(key ++ msg) は length extension で偽造可能です。hash.hmac_sha512 / hash.is_valid_hmac_sha512 を使います。
- `hash.sha512_hex(b: Bytes) -> String` — b の SHA-512 を 128 小文字十六進文字で返します。
- `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key の下での b の 32-byte HMAC-SHA256（RFC 2104）、key が先。tag は Secret なので == は定時間です。十六進: hex.encode(secret.reveal(tag))。
- `hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` — key の下での b の 64-byte HMAC-SHA512（RFC 2104）、key が先。tag は Secret なので == は定時間です。十六進: hex.encode(secret.reveal(tag))。
- `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag が key の下での b の HMAC-SHA256 なら true。定時間で比較します。十六進 tag: hash.is_valid_hmac_sha256(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。
- `hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` — tag が key の下での b の HMAC-SHA512 なら true。定時間で比較します。十六進 tag: hash.is_valid_hmac_sha512(key, b, secret.new(hex.decode(sig) ?? bytes.from_list([])))。
