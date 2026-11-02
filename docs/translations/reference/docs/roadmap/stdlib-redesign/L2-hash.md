> Description (inactive source index metadata): Stdlib redesign L2/L3 — hash: the home of keyless digests and checksums; CRC-32 and Adler-32 move in from zlib, MD5/SHA-1/SHA-512 join SHA-256, and every input is Bytes except the text-to-hex shortcuts
<a id="l2l3-hash"></a>

# L2/L3: hash

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-hash.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-hash.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-hash.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh source retrieval, independent security/constant-time testing or current API verification. (F) and measurements are the original author’s reports. Preserve initial MD5/SHA-1 additions and their 2026-10-26 removal; String-to-Bytes _hex revisions; old Bytes HMAC examples versus later Secret signatures; old insecure_digest/E100 provisions versus their removal. Do not treat those historical examples as present API guidance. Constant-time claims apply to contents with public lengths under the stated structural checks, not universal absence of side channels; deferred reveal-to-hash rejection remains deferred. Original Erlang/Python placement statements and surrounding tensions are retained. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md)

> Last updated: 2026-10-26
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-05); related [L2-zlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md)
> Sources: Rust https://doc.rust-lang.org/std/hash/index.html / Go https://pkg.go.dev/hash/crc32 / https://pkg.go.dev/hash/fnv / https://pkg.go.dev/crypto/sha256 / Python https://docs.python.org/3/library/hashlib.html (F) / https://docs.python.org/3/library/zlib.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/digest / Gleam — (not standard; official pkg https://hexdocs.pm/gleam_crypto/ ) / Elixir https://www.erlang.org/doc/apps/crypto/crypto.html / https://www.erlang.org/doc/apps/erts/erlang.html#crc32/1 / OCaml https://ocaml.org/manual/latest/api/Digest.html / Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/security/MessageDigest.html / Swift https://developer.apple.com/documentation/cryptokit/sha256
> Measurements: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`; probe in scratchpad `probe-url/p1.almd`.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| FNV-1a 32 bit | `fnv1a32`（String）、`fnv1a32_bytes`（Bytes） | — | `fnv.New32a` | — | — | — | — | — | — | — |
| CRC-32 | `zlib.crc32` | — | `crc32.ChecksumIEEE` | `zlib.crc32` | — | — | `:erlang.crc32` | — | `java.util.zip.CRC32`(JVM) | — |
| Adler-32 | `zlib.adler32` | — | `adler32.Checksum` | `zlib.adler32` | — | — | `:erlang.adler32` | — | `java.util.zip.Adler32`(JVM) | — |
| MD5 | **—** | — | `md5.Sum` | `hashlib.md5` (F) | —（absent from subtle） | pkg | `:crypto.hash(:md5, _)` | `Digest.string` | `MessageDigest`(JVM) | CryptoKit `Insecure.MD5` |
| SHA-1 | **—** | — | `sha1.Sum` | `hashlib.sha1` (F) | `digest("SHA-1")` | pkg | `:crypto.hash(:sha, _)` | — | `MessageDigest`(JVM) | CryptoKit `Insecure.SHA1` |
| SHA-256 | `sha256`（Bytes→Bytes）、`sha256_hex`（String→hex） | — | `sha256.Sum256` | `hashlib.sha256` (F) | `digest("SHA-256")` | pkg | `:crypto.hash(:sha256, _)` | — | `MessageDigest`(JVM) | CryptoKit `SHA256` |
| SHA-512 | **—** | — | `sha512.Sum512` | `hashlib.sha512` (F) | `digest("SHA-512")` | pkg | `:crypto.hash(:sha512, _)` | — | `MessageDigest`(JVM) | CryptoKit `SHA512` |
| Receive as hexadecimal string | `sha256_hex` | — | `hex.EncodeToString(sum[:])` | `hexdigest()` (F) | — | — | `Base.encode16` | `Digest.to_hex` | — | — |
| Keyed (HMAC etc.) | —（#2596 `crypto`） | — | `crypto/hmac` | `hmac` | `subtle.sign` | pkg | `:crypto.mac` | — | `javax.crypto.Mac` | CryptoKit `HMAC` |

7 languages (● and ◐) have MD5/SHA-1/SHA-512 (line 157 of [facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)).

<a id="完全性の規則"></a>

### Completeness rules

1. **Input is `Bytes`.** All `hash` functions take `Bytes`. Only `_hex` cells are exceptions.
2. **Digest family**: for each algorithm `sha256`, `sha512` (`md5` / `sha1` removed in R-hash-07), provide 2 cells: `<alg>(b: Bytes) -> Bytes` (raw digest) and `<alg>_hex(s: String) -> String` (text shortcut hashing the string's UTF-8 and returning lowercase hexadecimal). This turns the existing `sha256` / `sha256_hex` form directly into a family rule (R-L0-14 item 4: do not change types under existing names).
3. **Checksum family**: `crc32`, `adler32`, `fnv1a32` have `(b: Bytes) -> Int` (at least 0 and below 2^32). No `_hex` (use `int.to_hex`).
4. **Intentional omissions**: no SHA-224/SHA-384/SHA-3/BLAKE2 (R-L1-05 adds only MD5/SHA-1/SHA-512). No incremental hasher (Go `hash.Hash`, Python `update`). Among keyed operations, HMAC belongs in `hash` (changed in R-hash-06). AEAD and signatures have key/nonce types and do not have a digest shape, so belong in #2596's `crypto`.

<a id="l2-の判定"></a>

### L2 verdict

- Location: `zlib.crc32` / `zlib.adler32` have the same role as the `hash` checksum family → R-hash-02 (R-L1-05).
- Family gaps: no MD5/SHA-1/SHA-512 → R-hash-03 (R-L1-05).
- Spelling conflict: unsuffixed `fnv1a32` takes String, while `sha256` takes Bytes. `_bytes` occurs only once in `hash` → R-hash-01.
- Measured: `fnv1a32("a")` and `fnv1a32_bytes(bytes.from_string("a"))` both return `3826002220`; `zlib.crc32` returns `3904355907`, `zlib.adler32` returns `6422626` (same on native and wasm).

<a id="案"></a>

## Proposals

<a id="r-hash-01-fnv-1a-を-fnv1a32b-bytes---int-の-1-つにする---採用"></a>

### R-hash-01 Consolidate FNV-1a into `fnv1a32(b: Bytes) -> Int` — ○ Accepted

- Content: retain the name `fnv1a32` but change its argument to `Bytes` (`fnv1a32(b: Bytes) -> Int`; same computation as current `fnv1a32_bytes`). Remove `fnv1a32_bytes` as a second spelling of the same computation.
- Supporting L0 rules: R-L0-00 condition 1 (unsuffixed input types differ within one module), R-L0-01 (2 spellings for one computation), R-L0-14 item 4 (old `fnv1a32(s)` becomes a type error passing `String` where `Bytes` is expected, not a silent changed result, so changing type under the same name is allowed). Create no new spelling (R-L0-00: no majority spelling; only Go `New32a`). Same treatment as [L2-fs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md)'s `read_bytes` changing to `Bytes` under its existing name.
- (a) Only Go has it standard (`New32a`, `New64a`).
- (b) Pure computation. Truncating to 32 bit each time gives identical values on both targets (C-299).
- (c) Rule 1 (Bytes input) loses an exception. For strings, write `hash.fnv1a32(bytes.from_string(s))`.
- (d) 1 semantic change (argument type), 1 removal. Mark `fnv1a32_bytes` with `@deprecated(since=15, use="hash.fnv1a32")` and remove at `removal_epoch = 16`. `almide fix`: `hash.fnv1a32(s)` (argument is `String`) → `hash.fnv1a32(bytes.from_string(s))`; `hash.fnv1a32_bytes(b)` → `hash.fnv1a32(b)`. Record the `fnv1a32` type change in epoch 15 `breaks`, with the hint “pass `bytes.from_string(s)`” and `almide fix`. Usage (2 functions): spec 8 / tests 0 / docs 15 / stdlib 1.
- Proposed gate: R-hash-03's family gate checks from types that public `hash` inputs are `Bytes` (only `_hex` takes `String`).
- issue: #3916

<a id="r-hash-02-crc-32-と-adler-32-を-zlib-から移す---採用"></a>

### R-hash-02 Move CRC-32 and Adler-32 from zlib — ○ Accepted

- Content: add `hash.crc32(b: Bytes) -> Int` and `hash.adler32(b: Bytes) -> Int`, remove `zlib.crc32` / `zlib.adler32` ([L2-zlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md) R-zlib-01). Move current self-host computations unchanged.
- Supporting L0 rules: R-L1-05 (gather digest-returning functions in `hash`), R-L0-02. L1 named only CRC-32, but Adler-32 is also “Bytes → 32 bit checksum,” so move both for the same R-L1-05 reason.
- (a) Go places them in `hash/crc32` / `hash/adler32`; Python and Erlang in `zlib`. Almide chose `hash` in R-L1-05.
- (b) Pure computation. Already identical on both targets (measured).
- (c) “CRC32 is in zlib” does not work outside Python.
- (d) Add 2 (remove 2 on zlib side). Mark `zlib.crc32` with `@deprecated(since=15, use="hash.crc32")`, `zlib.adler32` with `@deprecated(since=15, use="hash.adler32")`, remove at `removal_epoch = 16`. `almide fix` rewrites 1-to-1 and adds `import hash`. Usage (2 functions): spec 15 / tests 5 / docs 17 / stdlib 0.
- Proposed gate: same family gate as R-hash-03. Run known-value fixtures (CRC-32 of `"123456789"` = `0xCBF43926`) on native and wasm.
- issue: #3914

<a id="r-hash-03-md5sha-1sha-512-を足す---採用"></a>

### R-hash-03 Add MD5 / SHA-1 / SHA-512 — ○ Accepted

- 2026-10-26: MD5/SHA-1 were removed in R-hash-07. SHA-512 remains.

- Content: `md5(b: Bytes) -> Bytes` (16 bytes), `md5_hex(s: String) -> String`, `sha1(b: Bytes) -> Bytes` (20 bytes), `sha1_hex(s: String) -> String`, `sha512(b: Bytes) -> Bytes` (64 bytes), `sha512_hex(s: String) -> String`.
- Supporting L0 rules: R-L1-05, rule 2 (digest family).
- (a) 7 languages have them. Names use the algorithms, like Go (`md5.Sum`, `sha1.Sum`, `sha512.Sum512`) and Python (`hashlib.md5`, `sha1`, `sha512`) (F).
- (b) Write in pure Almide with identical bytes on both targets (same form as `sha256`; `wasi-crypto` unavailable).
- (c) MD5 and SHA-1 have known collisions. The docs' first line says “for compatibility; do not use for security.”
- (d) Add 6.
- Proposed gate: family gate `tests/stdlib_hash_family_test.rs` compares names/types of all rule 2/3 cells against interface JSON. Make FIPS 180-4 / RFC 1321 test vectors native/wasm fixtures, and first establish new C contract numbers in als for the contract ledger.
- issue: #3915

<a id="r-hash-04-_hex-のセルの入力を-bytes-に変える---不採用-r-hash-05-で採用に変更"></a>

### R-hash-04 Change `_hex` inputs to Bytes — × Rejected (→ changed to accepted in R-hash-05)

- Reason: old calls become type errors, meeting R-L0-14.4, but all frequently used `sha256_hex(s)` (spec 16 / docs 3) would need wrapping with `bytes.from_string`. Mix-ups are caught by type errors; handling them through R-L0-19 type-mismatch guidance (`bytes.from_string(s)` / `hex.encode(hash.sha256(b))`) is cheaper.
- issue: none

<a id="r-hash-05-_hex-も-bytes-を受け取る完全性の規則-2-の見直し---採用"></a>

### R-hash-05 Make `_hex` take `Bytes` too (revise completeness rule 2) — ○ Accepted

- Now: `<alg>(b: Bytes) -> Bytes` and `<alg>_hex(s: String) -> String` make `_hex` change 2 things at once: output encoding and input type. It is the only exception to rule 1, “input is `Bytes`”; `hash.fnv1a32` was aligned to `Bytes`, but `_hex` remains. Rule 2 cited R-L0-14 item 4, yet type changes are noticed as type errors, so that rule does not forbid them.
- Proposal: use `<alg>_hex(b: Bytes) -> String`. To hash strings, write `hash.sha256_hex(bytes.from_string(s))`.
- Supporting L0 rule: R-L0-00 (one meaning for `_hex`), rule 1.
- (a) Python `hashlib.sha256(b).hexdigest()`, Go `sha256.Sum256(b)`, Rust sha2 crate all take bytes.
- (d) Semantic change (noticed by type error). `almide fix` inserts `bytes.from_string(…)`.
- Completeness rule (replaces 1/2): all `hash` functions take `Bytes`. `_hex` means only lowercase-hex output.
- issue: #3985

<a id="r-hash-06-hmac-と定数時間の比較を-hash-に置く完全性の規則-4-の見直し---採用"></a>

### R-hash-06 Put HMAC and constant-time comparison in `hash` (revise completeness rule 4) — ○ Accepted

- Now: there is no keyed digest. Models write `hash.sha256(key ++ body)`, forgeable through length extension. Comparing a computed MAC using `==` returns at the first differing byte, allowing timing to reveal the MAC 1 byte at a time.
- Proposal: `hash.hmac_sha256(key: Bytes, b: Bytes) -> Bytes`, `hmac_sha256_hex`, `hmac_sha512`, `hmac_sha512_hex`, plus verification cell `hash.is_valid_hmac_sha256(key, b, tag: Bytes) -> Bool`; provide the same pair for sha512. Decode hex tags (webhook header `sha256=<hex>`) with `hex.decode` before passing: `hash.is_valid_hmac_sha256(key, body, hex.decode(sig) ?? bytes.from_list([]))`. A non-hex string becomes an empty tag, returning false for differing length. Verification cells compare through `bytes.is_equal_constant_time`. There are 2 comparison functions: `bytes.is_equal_constant_time(a, b)` and `string.is_equal_constant_time(a, b)` (R-bytes-29 / R-string-19).
- Key first (RFC 2104 HMAC(K, m); Go/Rust/Python/Node/Erlang/JVM, 6/8). Both are `Bytes`, so type checking cannot detect a swap. RFC 4231 vectors fix the order.
- Location: R-L1-05 says “digest-returning functions belong in `hash`.” HMAC is a `Bytes -> Bytes` digest, and a model writing broken `sha256(key ++ msg)` is already inside `hash`. Put the correct cell next to it.
- md5/sha1 were removed in R-hash-07 (the initial proposal used `@insecure_digest` and E100 in the critical profile).
- Constant-time claim: for input contents, neither source nor emitted code has secret-dependent branches or indices. Lengths are public (different lengths return false immediately without scanning). Native uses a runtime function, passing through `core::hint::black_box` each time and accumulating OR. wasm uses a fixed loop with only index comparisons as branches. `tests/constant_time_compare_structure_test.rs` checks emitted code for both (also checks that an early-exit control fails). HMAC itself uses SHA's fixed-round count, branching only on public length (whether the key exceeds the block).
- Completeness rules (replace 4, add 5):
  4. **Intentional omissions**: SHA-224/SHA-384/SHA-3/BLAKE2, incremental hashers, keyed AEAD/signatures (#2596's `crypto`).
  5. **Keyed-digest family**: {sha256, sha512} × {compute, `is_valid_`}, 4 cells (R-hash-07 removes `_hex` and changes key/tag to `Secret[Bytes]`). Intentional omissions: `hmac_<alg>_hex`, `is_valid_hmac_<alg>_hex`, `hmac_md5` / `hmac_sha1`. No `_hex` version of `is_valid_`, as it would give `_hex` a second meaning, “input encoding” (R-hash-05 restricts `_hex` to output encoding; decode hexadecimal with `hex.decode` under R-L0-25). As for `hmac_md5` / `hmac_sha1` (old Twilio/GitHub headers use SHA-1, so if added they get the same `@insecure_digest`). `tests/hash_hmac_family_gate_test.rs` checks the family.
- Supporting L0 rules: R-L0-09 (Bool names use `is_`), R-L0-30 (one argument name per role: `key`, `b`, `tag`), rule 1 (`Bytes` input), R-hash-05 (`_hex` only output encoding), R-L0-25 (`hex.decode` from hex strings), R-L0-01 (do not give the composition of `is_valid_` and `hex.decode` another spelling).
- (a) Swift CryptoKit `HMAC<SHA256>.isValidAuthenticationCode`, Rust `Mac::verify_slice` (verification cells); Go `hmac.Equal`, Python `hmac.compare_digest`, Node `crypto.timingSafeEqual` (constant-time comparison).
- (d) Addition. E100 for md5/sha1 rejects only under `--profile critical`; record in epoch 15 breaks.
- Contracts: C-422 (RFC 4231), C-423 (constant-time comparison), C-424 (revised in R-hash-07 to “MD5/SHA-1 are absent from stdlib”).
- issue: #4106

<a id="r-hash-07-秘密は-secret-で受け渡し壊れた要約を撤去するr-hash-03r-hash-06-の見直し---採用2026-10-26"></a>

### R-hash-07 Pass secrets through `Secret` and remove broken digests (revise R-hash-03 / R-hash-06) — ○ Accepted (2026-10-26)

- Now: `hmac_sha256` accepts a `Bytes` key and returns a `Bytes` tag; `hmac_sha256_hex` returns `String`. Both tags can be compared with `==`, and `==` returns at the first differing byte, leaking MAC through timing. `md5` / `sha1` are usable in the default profile (security-table row 20).
- Proposal:
  1. `hmac_sha256` / `hmac_sha512`: `(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`. Tag `==` uses only constant-time comparison (C-426). A leaking comparison cannot be written without `secret.reveal`.
  2. Remove `hmac_sha256_hex` / `hmac_sha512_hex`. Signature headers use `hex.encode(secret.reveal(tag))`. Unreleased (added in 0.69.0), so no epoch rows.
  3. Keep `is_valid_hmac_sha256` / `is_valid_hmac_sha512`, changing `tag` to `Secret[Bytes]` too.
  4. Add `env.secret(name) -> Secret[String]?` as a key source (L2-env R-env-05).
  5. Remove `md5`, `md5_hex`, `sha1`, `sha1_hex`. No calls exist inside stdlib. Add 4 epoch 15 breaks rows (hint: `hash.sha256`). Do not create an external package. Remove `@insecure_digest` and critical E100 alongside them.
- Deferred: do not add an inspection rule rejecting `hash.sha256(secret.reveal(key) + body)` (passing revealed values to hashing).
- Gates: `tests/hash_hmac_family_gate_test.rs` (declared surface uses `Secret[Bytes]` for key/tag, lacks `_hex`, and `is_valid_` bodies go through constant-time comparison); `tests/removed_spellings_stay_removed_test.rs` (spellings recorded as removed in the dialect ledger do not return to the public surface).
- Contracts: revise C-422/C-424, add C-438 (`env.secret`).

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut notation) | Main spellings in other languages | Verdict | Proposed |
|---|---|---|---|---|
| `fnv1a32` | `hash.fnv1a32(s: String) -> Int` | Go `fnv.New32a` | Semantic change (argument becomes `Bytes`; `fnv1a32(bytes.from_string(s))`） | R-hash-01 |
| `fnv1a32_bytes` | `hash.fnv1a32_bytes(b: Bytes) -> Int` | Go `fnv.New32a` | Remove → `fnv1a32` | R-hash-01 |
| `sha256` | `hash.sha256(b: Bytes) -> Bytes` | Go `sha256.Sum256` / Python `hashlib.sha256(b).digest()` (F) / JS `subtle.digest("SHA-256")` / Elixir `:crypto.hash(:sha256, b)` | Unchanged | — |
| `sha256_hex` | `hash.sha256_hex(b: Bytes) -> String` | Python `hexdigest()` (F) / Go `hex.EncodeToString` | Semantic change（`Bytes` input） | R-hash-05 |
| `crc32` | `hash.crc32(b: Bytes) -> Int`（Proposed） | Go `crc32.ChecksumIEEE` / Python `zlib.crc32` / Erlang `crc32` | Add | R-hash-02 |
| `adler32` | `hash.adler32(b: Bytes) -> Int`（Proposed） | Go `adler32.Checksum` / Python `zlib.adler32` | Add | R-hash-02 |
| `md5` | —（added in R-hash-03, removed in R-hash-07） | — | Remove（not for security; use `hash.sha256`） | R-hash-07 |
| `md5_hex` | —（added in R-hash-03, removed in R-hash-07） | — | Remove（not for security; use `hash.sha256`） | R-hash-07 |
| `sha1` | —（added in R-hash-03, removed in R-hash-07） | — | Remove（not for security; use `hash.sha256`） | R-hash-07 |
| `sha1_hex` | —（added in R-hash-03, removed in R-hash-07） | — | Remove（not for security; use `hash.sha256`） | R-hash-07 |
| `sha512` | `hash.sha512(b: Bytes) -> Bytes`（Proposed） | Go `sha512.Sum512` / Python `hashlib.sha512` (F) / JS `digest("SHA-512")` | Add | R-hash-03 |
| `sha512_hex` | `hash.sha512_hex(b: Bytes) -> String` | Python `hexdigest()` (F) | Add（`Bytes` input） | R-hash-03、R-hash-05 |
| `hmac_sha256` | `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`（Proposed） | Go `hmac.New(sha256.New, key)` / Python `hmac.digest(key, msg, "sha256")` / Node `createHmac("sha256", key)` / Erlang `crypto:mac(hmac, sha256, Key, Data)` | Add | R-hash-06 |
| `hmac_sha256_hex` | —（added in R-hash-06, removed in R-hash-07） | Python `hexdigest()` | Remove（`hex.encode(secret.reveal(tag))`） | R-hash-07 |
| `hmac_sha512` | `hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`（Proposed） | Go `hmac.New(sha512.New, key)` / Python `hmac.digest(key, msg, "sha512")` | Add | R-hash-06 |
| `hmac_sha512_hex` | —（added in R-hash-06, removed in R-hash-07） | Python `hexdigest()` | Remove（`hex.encode(secret.reveal(tag))`） | R-hash-07 |
| `is_valid_hmac_sha256` | `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool`（Proposed） | Swift `HMAC<SHA256>.isValidAuthenticationCode` / Rust `Mac::verify_slice` | Add | R-hash-06 |
| `is_valid_hmac_sha512` | `hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool`（Proposed） | Swift `HMAC<SHA512>.isValidAuthenticationCode` | Add | R-hash-06 |
