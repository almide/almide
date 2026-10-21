<!-- description: Stdlib redesign L2/L3 — hash: the home of keyless digests and checksums; CRC-32 and Adler-32 move in from zlib, MD5/SHA-1/SHA-512 join SHA-256, and every input is Bytes except the text-to-hex shortcuts -->
# L2/L3: hash

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-05）・関連 [L2-zlib](L2-zlib.md)
> 出典: Rust https://doc.rust-lang.org/std/hash/index.html ・ Go https://pkg.go.dev/hash/crc32 ・ https://pkg.go.dev/hash/fnv ・ https://pkg.go.dev/crypto/sha256 ・ Python https://docs.python.org/3/library/hashlib.html (F) ・ https://docs.python.org/3/library/zlib.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/digest ・ Gleam —（標準に無い。公式 pkg https://hexdocs.pm/gleam_crypto/ ）・ Elixir https://www.erlang.org/doc/apps/crypto/crypto.html ・ https://www.erlang.org/doc/apps/erts/erlang.html#crc32/1 ・ OCaml https://ocaml.org/manual/latest/api/Digest.html ・ Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/security/MessageDigest.html ・ Swift https://developer.apple.com/documentation/cryptokit/sha256
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-url/p1.almd`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| FNV-1a 32 bit | `fnv1a32`（String）、`fnv1a32_bytes`（Bytes） | — | `fnv.New32a` | — | — | — | — | — | — | — |
| CRC-32 | `zlib.crc32` | — | `crc32.ChecksumIEEE` | `zlib.crc32` | — | — | `:erlang.crc32` | — | `java.util.zip.CRC32`(JVM) | — |
| Adler-32 | `zlib.adler32` | — | `adler32.Checksum` | `zlib.adler32` | — | — | `:erlang.adler32` | — | `java.util.zip.Adler32`(JVM) | — |
| MD5 | **—** | — | `md5.Sum` | `hashlib.md5` (F) | —（subtle に無い） | pkg | `:crypto.hash(:md5, _)` | `Digest.string` | `MessageDigest`(JVM) | CryptoKit `Insecure.MD5` |
| SHA-1 | **—** | — | `sha1.Sum` | `hashlib.sha1` (F) | `digest("SHA-1")` | pkg | `:crypto.hash(:sha, _)` | — | `MessageDigest`(JVM) | CryptoKit `Insecure.SHA1` |
| SHA-256 | `sha256`（Bytes→Bytes）、`sha256_hex`（String→hex） | — | `sha256.Sum256` | `hashlib.sha256` (F) | `digest("SHA-256")` | pkg | `:crypto.hash(:sha256, _)` | — | `MessageDigest`(JVM) | CryptoKit `SHA256` |
| SHA-512 | **—** | — | `sha512.Sum512` | `hashlib.sha512` (F) | `digest("SHA-512")` | pkg | `:crypto.hash(:sha512, _)` | — | `MessageDigest`(JVM) | CryptoKit `SHA512` |
| 16 進の文字列で受け取る | `sha256_hex` | — | `hex.EncodeToString(sum[:])` | `hexdigest()` (F) | — | — | `Base.encode16` | `Digest.to_hex` | — | — |
| 鍵つき（HMAC など） | —（#2596 の `crypto`） | — | `crypto/hmac` | `hmac` | `subtle.sign` | pkg | `:crypto.mac` | — | `javax.crypto.Mac` | CryptoKit `HMAC` |

MD5・SHA-1・SHA-512 は 7 言語（● と ◐）が持っています（[facts/categories.md](facts/categories.md) の 157 行）。

### 完全性の規則

1. **入力は `Bytes`。** `hash` のすべての関数は `Bytes` を受け取ります。例外は `_hex` のセルだけです。
2. **要約（digest）の族**: アルゴリズム `md5`、`sha1`、`sha256`、`sha512` のそれぞれに、`<alg>(b: Bytes) -> Bytes`（生の要約）と `<alg>_hex(s: String) -> String`（文字列の UTF-8 を要約し、小文字の 16 進で返すテキスト用の近道）の 2 セルを置きます。いまの `sha256` / `sha256_hex` の形をそのまま族の規則にしたものです（R-L0-14 の 4: 既存の名前の型を変えない）。
3. **検査和（checksum）の族**: `crc32`、`adler32`、`fnv1a32` は `(b: Bytes) -> Int`（0 以上 2^32 未満）。`_hex` は持ちません（`int.to_hex` で書けます）。
4. **意図した抜け**: SHA-224・SHA-384・SHA-3・BLAKE2 は持ちません（R-L1-05 が足すのは MD5・SHA-1・SHA-512 だけ）。逐次に足していく hasher（Go の `hash.Hash`、Python の `update`）も持ちません。鍵を使うもの（HMAC・AEAD・署名）は #2596 の `crypto` に置きます。

### L2 の判定

- 置き場所: `zlib.crc32`・`zlib.adler32` は `hash` の検査和の族と同じ役割です → R-hash-02（R-L1-05）。
- 族の穴: MD5・SHA-1・SHA-512 が無い → R-hash-03（R-L1-05）。
- 綴りの衝突: 無印の名前が、`fnv1a32` では String、`sha256` では Bytes を受け取ります。`_bytes` という接尾辞は `hash` の中でこの 1 つだけです → R-hash-01。
- 実測: `fnv1a32("a")` と `fnv1a32_bytes(bytes.from_string("a"))` はどちらも `3826002220`、`zlib.crc32` は `3904355907`、`zlib.adler32` は `6422626`（native と wasm で同じ）。

## 案

### R-hash-01 FNV-1a を `fnv1a32(b: Bytes) -> Int` の 1 つにする — ○ 採用

- 内容: `fnv1a32` の名前のまま、引数を `Bytes` に変えます（`fnv1a32(b: Bytes) -> Int`。計算はいまの `fnv1a32_bytes` と同じ）。`fnv1a32_bytes` は同じ計算の 2 つ目の綴りになるので撤去します。
- 根拠の L0 規則: R-L0-00 の条件 1（無印の名前の入力の型が、同じモジュールで食い違っている）、R-L0-01（同じ計算に 2 つの綴り）、R-L0-14 の 4（旧い `fnv1a32(s)` は `String` を `Bytes` の位置に渡す型エラーになり、黙って別の結果にはならないので、同じ名前のまま型を変えてよい）。新しい綴りは作りません（R-L0-00: 多数派の綴りは無く、Go の `New32a` だけ）。[L2-fs](L2-fs.md) の `read_bytes` が名前を残して `Bytes` に変わるのと同じ扱いです。
- (a) Go だけが標準に持っています（`New32a`、`New64a`）。
- (b) 純粋な計算です。32 bit で毎回切り詰めるので、両 target で同じ値です（C-299）。
- (c) 規則 1（入力は Bytes）に例外がなくなります。文字列のときは `hash.fnv1a32(bytes.from_string(s))` と書きます。
- (d) 意味変更 1（引数の型）・撤去 1。`fnv1a32_bytes` に `@deprecated(since=15, use="hash.fnv1a32")` を付け、`removal_epoch = 16` で撤去します。`almide fix` の書き換え: `hash.fnv1a32(s)`（引数が `String`）→ `hash.fnv1a32(bytes.from_string(s))`、`hash.fnv1a32_bytes(b)` → `hash.fnv1a32(b)`。`fnv1a32` の型の変更は、ヒント（「`bytes.from_string(s)` を渡す」）と `almide fix` を付けて epoch 15 の `breaks` に入れます。使用数（2 関数）: spec 8 / tests 0 / docs 15 / stdlib 1。
- ゲート案: R-hash-03 の族ゲートが、`hash` の公開関数の入力が `Bytes`（`_hex` だけ `String`）であることを型から確かめます。
- issue: #3916

### R-hash-02 CRC-32 と Adler-32 を zlib から移す — ○ 採用

- 内容: `hash.crc32(b: Bytes) -> Int` と `hash.adler32(b: Bytes) -> Int` を追加し、`zlib.crc32`・`zlib.adler32` を撤去します（[L2-zlib](L2-zlib.md) の R-zlib-01）。計算はいまの self-host の実装をそのまま移します。
- 根拠の L0 規則: R-L1-05（要約を返す関数は `hash` に集める）、R-L0-02。L1 が名指ししたのは CRC-32 だけですが、Adler-32 も同じ「Bytes → 32 bit の検査和」なので、R-L1-05 と同じ理由で一緒に移します。
- (a) Go は `hash/crc32`・`hash/adler32` に、Python と Erlang は `zlib` に置いています。Almide は R-L1-05 で `hash` に決めています。
- (b) 純粋な計算です。いまも両 target で同じ値です（実測）。
- (c) 「CRC32 は zlib に」は Python 以外では当たりません。
- (d) 追加 2（zlib 側は撤去 2）。`zlib.crc32` に `@deprecated(since=15, use="hash.crc32")`、`zlib.adler32` に `@deprecated(since=15, use="hash.adler32")` を付け、`removal_epoch = 16` で撤去します。`almide fix` は 1 対 1 で書き換え、`import hash` を足します。使用数（2 関数）: spec 15 / tests 5 / docs 17 / stdlib 0。
- ゲート案: R-hash-03 と同じ族ゲート。既知の値（`"123456789"` の CRC-32 = `0xCBF43926`）の fixture を native と wasm で走らせます。
- issue: #3914

### R-hash-03 MD5・SHA-1・SHA-512 を足す — ○ 採用

- 内容: `md5(b: Bytes) -> Bytes`（16 バイト）、`md5_hex(s: String) -> String`、`sha1(b: Bytes) -> Bytes`（20 バイト）、`sha1_hex(s: String) -> String`、`sha512(b: Bytes) -> Bytes`（64 バイト）、`sha512_hex(s: String) -> String`。
- 根拠の L0 規則: R-L1-05、規則 2（要約の族）。
- (a) 7 言語が持っています。名前は Go（`md5.Sum`、`sha1.Sum`、`sha512.Sum512`）と Python（`hashlib.md5`、`sha1`、`sha512`）(F) と同じアルゴリズム名です。
- (b) 純粋な Almide で書き、両 target で同じバイト列にします（`sha256` と同じ形。`wasi-crypto` は使えない）。
- (c) MD5 と SHA-1 は衝突が見つかっています。doc の 1 行目に「互換のため。安全性には使わない」と書きます。
- (d) 追加 6。
- ゲート案: 族ゲート `tests/stdlib_hash_family_test.rs` が、規則 2・3 の全セルの名前と型を interface JSON と突き合わせます。FIPS 180-4・RFC 1321 のテストベクタを native と wasm の fixture にし、契約台帳に新しい C 番号を als で先に立てます。
- issue: #3915

### R-hash-04 `_hex` のセルの入力を Bytes に変える — × 不採用

- 理由: 旧い呼び出しは型エラーになるので R-L0-14.4 は満たしますが、使用数の多い `sha256_hex(s)`（spec 16 / docs 3）を全部 `bytes.from_string` で包むことになります。取り違えは型エラーで止まるので、R-L0-19 の型違いの案内（`bytes.from_string(s)`／`hex.encode(hash.sha256(b))`）で受け止めるほうが安上がりです。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `fnv1a32` | `hash.fnv1a32(s: String) -> Int` | Go `fnv.New32a` | 意味変更（引数を `Bytes` に。`fnv1a32(bytes.from_string(s))`） | R-hash-01 |
| `fnv1a32_bytes` | `hash.fnv1a32_bytes(b: Bytes) -> Int` | Go `fnv.New32a` | 撤去 → `fnv1a32` | R-hash-01 |
| `sha256` | `hash.sha256(b: Bytes) -> Bytes` | Go `sha256.Sum256` / Python `hashlib.sha256(b).digest()` (F) / JS `subtle.digest("SHA-256")` / Elixir `:crypto.hash(:sha256, b)` | 据え置き | — |
| `sha256_hex` | `hash.sha256_hex(s: String) -> String` | Python `hexdigest()` (F) / Go `hex.EncodeToString` | 据え置き | — |
| `crc32` | `hash.crc32(b: Bytes) -> Int`（案） | Go `crc32.ChecksumIEEE` / Python `zlib.crc32` / Erlang `crc32` | 追加 | R-hash-02 |
| `adler32` | `hash.adler32(b: Bytes) -> Int`（案） | Go `adler32.Checksum` / Python `zlib.adler32` | 追加 | R-hash-02 |
| `md5` | `hash.md5(b: Bytes) -> Bytes`（案） | Go `md5.Sum` / Python `hashlib.md5` (F) / OCaml `Digest` | 追加 | R-hash-03 |
| `md5_hex` | `hash.md5_hex(s: String) -> String`（案） | Python `md5(..).hexdigest()` (F) / OCaml `Digest.to_hex` | 追加 | R-hash-03 |
| `sha1` | `hash.sha1(b: Bytes) -> Bytes`（案） | Go `sha1.Sum` / Python `hashlib.sha1` (F) / JS `digest("SHA-1")` | 追加 | R-hash-03 |
| `sha1_hex` | `hash.sha1_hex(s: String) -> String`（案） | Python `hexdigest()` (F) | 追加 | R-hash-03 |
| `sha512` | `hash.sha512(b: Bytes) -> Bytes`（案） | Go `sha512.Sum512` / Python `hashlib.sha512` (F) / JS `digest("SHA-512")` | 追加 | R-hash-03 |
| `sha512_hex` | `hash.sha512_hex(s: String) -> String`（案） | Python `hexdigest()` (F) | 追加 | R-hash-03 |
