> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — hash：无密钥摘要及校验和的归属；CRC-32、Adler-32 从 zlib 移入，MD5/SHA-1/SHA-512 加入 SHA-256，除文本转十六进制捷径外输入均为 Bytes
<a id="l2l3-hash"></a>

# L2/L3: hash

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-hash.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-hash.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-hash.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取来源、独立安全/常数时间测试或核实现行API。(F)及实测为原作者报告。保留MD5/SHA-1初始新增到2026-10-26移除、_hex从String改Bytes、旧Bytes HMAC示例与后Secret签名、旧insecure_digest/E100规定及其删除，不将历史示例当现行API指南。常数时间主张仅针对公开长度及所述结构检查下的内容，不保证所有侧信道不存在。reveal值传hash的拒绝规则仍属延后。原文Erlang/Python归属说法及上下文张力原样保留。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md)

> 最后更新：2026-10-26
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-05）・相关 [L2-zlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md)
> 来源：Rust https://doc.rust-lang.org/std/hash/index.html ・Go https://pkg.go.dev/hash/crc32 ・https://pkg.go.dev/hash/fnv ・https://pkg.go.dev/crypto/sha256 ・Python https://docs.python.org/3/library/hashlib.html (F) ・https://docs.python.org/3/library/zlib.html ・TS/JS https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto/digest ・Gleam —（非标准，官方 pkg https://hexdocs.pm/gleam_crypto/ ）・Elixir https://www.erlang.org/doc/apps/crypto/crypto.html ・https://www.erlang.org/doc/apps/erts/erlang.html#crc32/1 ・OCaml https://ocaml.org/manual/latest/api/Digest.html ・Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/security/MessageDigest.html ・Swift https://developer.apple.com/documentation/cryptokit/sha256
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。probe 位于 scratchpad 的 `probe-url/p1.almd`。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| FNV-1a 32 bit | `fnv1a32`（String）、`fnv1a32_bytes`（Bytes） | — | `fnv.New32a` | — | — | — | — | — | — | — |
| CRC-32 | `zlib.crc32` | — | `crc32.ChecksumIEEE` | `zlib.crc32` | — | — | `:erlang.crc32` | — | `java.util.zip.CRC32`(JVM) | — |
| Adler-32 | `zlib.adler32` | — | `adler32.Checksum` | `zlib.adler32` | — | — | `:erlang.adler32` | — | `java.util.zip.Adler32`(JVM) | — |
| MD5 | **—** | — | `md5.Sum` | `hashlib.md5` (F) | —（subtle 中没有） | pkg | `:crypto.hash(:md5, _)` | `Digest.string` | `MessageDigest`(JVM) | CryptoKit `Insecure.MD5` |
| SHA-1 | **—** | — | `sha1.Sum` | `hashlib.sha1` (F) | `digest("SHA-1")` | pkg | `:crypto.hash(:sha, _)` | — | `MessageDigest`(JVM) | CryptoKit `Insecure.SHA1` |
| SHA-256 | `sha256`（Bytes→Bytes）、`sha256_hex`（String→hex） | — | `sha256.Sum256` | `hashlib.sha256` (F) | `digest("SHA-256")` | pkg | `:crypto.hash(:sha256, _)` | — | `MessageDigest`(JVM) | CryptoKit `SHA256` |
| SHA-512 | **—** | — | `sha512.Sum512` | `hashlib.sha512` (F) | `digest("SHA-512")` | pkg | `:crypto.hash(:sha512, _)` | — | `MessageDigest`(JVM) | CryptoKit `SHA512` |
| 以十六进制字符串接收 | `sha256_hex` | — | `hex.EncodeToString(sum[:])` | `hexdigest()` (F) | — | — | `Base.encode16` | `Digest.to_hex` | — | — |
| 带密钥（HMAC 等） | —（#2596 的 `crypto`） | — | `crypto/hmac` | `hmac` | `subtle.sign` | pkg | `:crypto.mac` | — | `javax.crypto.Mac` | CryptoKit `HMAC` |

7 种语言（● 与 ◐）提供 MD5・SHA-1・SHA-512（[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) 第 157 行）。

<a id="完全性の規則"></a>

### 完整性规则

1. **输入为 `Bytes`。** `hash` 全部函数接收 `Bytes`，仅 `_hex` 单元例外。
2. **摘要（digest）族**：对算法 `sha256`、`sha512` 各设 2 个单元（`md5`・`sha1` 在 R-hash-07 移除）：`<alg>(b: Bytes) -> Bytes`（原始摘要）和 `<alg>_hex(s: String) -> String`（摘要字符串 UTF-8 并返回小写十六进制的文本捷径）。将现有 `sha256` / `sha256_hex` 形式直接作为族规则（R-L0-14 的 4：不改变既有名称的类型）。
3. **校验和（checksum）族**：`crc32`、`adler32`、`fnv1a32` 为 `(b: Bytes) -> Int`（大于等于 0，小于 2^32）。没有 `_hex`（可用 `int.to_hex`）。
4. **有意遗漏**：不提供 SHA-224・SHA-384・SHA-3・BLAKE2（R-L1-05 仅新增 MD5・SHA-1・SHA-512）。不提供增量 hasher（Go `hash.Hash`、Python `update`）。带密钥内容中的 HMAC 放在 `hash`（R-hash-06 变更）。AEAD 和签名具有密钥或 nonce 类型，不呈摘要形式，归 #2596 的 `crypto`。

<a id="l2-の判定"></a>

### L2 判定

- 归属：`zlib.crc32`・`zlib.adler32` 与 `hash` 校验和族同角色 → R-hash-02（R-L1-05）。
- 族缺口：缺 MD5・SHA-1・SHA-512 → R-hash-03（R-L1-05）。
- 拼写冲突：无后缀名称 `fnv1a32` 接收 String，`sha256` 接收 Bytes。`_bytes` 后缀在 `hash` 中仅此 1 个 → R-hash-01。
- 实测：`fnv1a32("a")` 与 `fnv1a32_bytes(bytes.from_string("a"))` 均为 `3826002220`，`zlib.crc32` 为 `3904355907`，`zlib.adler32` 为 `6422626`（native 与 wasm 相同）。

<a id="案"></a>

## 提案

<a id="r-hash-01-fnv-1a-を-fnv1a32b-bytes---int-の-1-つにする---採用"></a>

### R-hash-01 将 FNV-1a 统一为 `fnv1a32(b: Bytes) -> Int` — ○ 采纳

- 内容：保持 `fnv1a32` 名称，参数改为 `Bytes`（`fnv1a32(b: Bytes) -> Int`，计算同当前 `fnv1a32_bytes`）。`fnv1a32_bytes` 会成为同计算第 2 种拼写，因此移除。
- 依据的 L0 规则：R-L0-00 条件 1（同模块无后缀名输入类型不一致）、R-L0-01（同计算 2 拼写）、R-L0-14 的 4（旧 `fnv1a32(s)` 将 `String` 传至 `Bytes` 位置会类型错误，不会静默得不同结果，故可同名改类型）。不造新拼写（R-L0-00：无多数，只有 Go `New32a`）。与 [L2-fs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md) 的 `read_bytes` 保留名称改为 `Bytes` 同处理。
- (a) 仅 Go 标准提供（`New32a`、`New64a`）。
- (b) 纯计算，每次截至 32 bit，因此两个 target 同值（C-299）。
- (c) 规则 1（输入 Bytes）不再有此例外。字符串写作 `hash.fnv1a32(bytes.from_string(s))`。
- (d) 语义变更 1（参数类型）・移除 1。`fnv1a32_bytes` 加 `@deprecated(since=15, use="hash.fnv1a32")`，在 `removal_epoch = 16` 移除。`almide fix`：`hash.fnv1a32(s)`（参数为 `String`）→ `hash.fnv1a32(bytes.from_string(s))`、`hash.fnv1a32_bytes(b)` → `hash.fnv1a32(b)`。`fnv1a32` 类型变更配提示“传入 `bytes.from_string(s)`”与 `almide fix`，记入 epoch 15 `breaks`。使用数（2 函数）：spec 8 / tests 0 / docs 15 / stdlib 1。
- 拟议门禁：R-hash-03 族门禁从类型检查 `hash` 公开函数输入为 `Bytes`（仅 `_hex` 是 `String`）。
- issue: #3916

<a id="r-hash-02-crc-32-と-adler-32-を-zlib-から移す---採用"></a>

### R-hash-02 将 CRC-32 和 Adler-32 从 zlib 移入 — ○ 采纳

- 内容：新增 `hash.crc32(b: Bytes) -> Int` 与 `hash.adler32(b: Bytes) -> Int`，移除 `zlib.crc32`・`zlib.adler32`（[L2-zlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md) R-zlib-01）。计算原样移入现有 self-host 实现。
- 依据的 L0 规则：R-L1-05（返回摘要的函数集中到 `hash`）、R-L0-02。L1 只指名 CRC-32，但 Adler-32 同为“Bytes → 32 bit 校验和”，故按 R-L1-05 同理由一起移动。
- (a) Go 放在 `hash/crc32`・`hash/adler32`，Python 和 Erlang 放在 `zlib`。Almide 在 R-L1-05 定为 `hash`。
- (b) 纯计算，目前两个 target 也同值（实测）。
- (c) “CRC32 在 zlib”除 Python 外猜不准。
- (d) 新增 2（zlib 侧移除 2）。`zlib.crc32` 加 `@deprecated(since=15, use="hash.crc32")`，`zlib.adler32` 加 `@deprecated(since=15, use="hash.adler32")`，于 `removal_epoch = 16` 移除。`almide fix` 1 对 1 改写并添加 `import hash`。使用数（2 函数）：spec 15 / tests 5 / docs 17 / stdlib 0。
- 拟议门禁：同 R-hash-03 族门禁。native 与 wasm 上运行已知值 fixture（`"123456789"` 的 CRC-32 = `0xCBF43926`）。
- issue: #3914

<a id="r-hash-03-md5sha-1sha-512-を足す---採用"></a>

### R-hash-03 添加 MD5・SHA-1・SHA-512 — ○ 采纳

- 2026-10-26：MD5・SHA-1 已在 R-hash-07 移除，SHA-512 保留。

- 内容：`md5(b: Bytes) -> Bytes`（16 字节）、`md5_hex(s: String) -> String`、`sha1(b: Bytes) -> Bytes`（20 字节）、`sha1_hex(s: String) -> String`、`sha512(b: Bytes) -> Bytes`（64 字节）、`sha512_hex(s: String) -> String`。
- 依据的 L0 规则：R-L1-05、规则 2（摘要族）。
- (a) 7 种语言提供，名称与 Go（`md5.Sum`、`sha1.Sum`、`sha512.Sum512`）和 Python（`hashlib.md5`、`sha1`、`sha512`）(F) 一样使用算法名。
- (b) 用纯 Almide 编写，两个 target 相同字节序列（同 `sha256` 形式；不能用 `wasi-crypto`）。
- (c) MD5 与 SHA-1 已发现碰撞。文档首行写“用于兼容，不用于安全”。
- (d) 新增 6。
- 拟议门禁：族门禁 `tests/stdlib_hash_family_test.rs` 将规则 2・3 全单元名称类型对照 interface JSON。FIPS 180-4・RFC 1321 测试向量作为 native/wasm fixture，契约台账新 C 编号先在 als 建立。
- issue: #3915

<a id="r-hash-04-_hex-のセルの入力を-bytes-に変える---不採用-r-hash-05-で採用に変更"></a>

### R-hash-04 将 `_hex` 单元输入改为 Bytes — × 不采纳（→ R-hash-05 改为采纳）

- 理由：旧调用类型错误，满足 R-L0-14.4，但高使用量 `sha256_hex(s)`（spec 16 / docs 3）需全部用 `bytes.from_string` 包装。混用会被类型错误阻止，用 R-L0-19 类型不符提示（`bytes.from_string(s)`／`hex.encode(hash.sha256(b))`）承接成本更低。
- issue：无

<a id="r-hash-05-_hex-も-bytes-を受け取る完全性の規則-2-の見直し---採用"></a>

### R-hash-05 `_hex` 也接收 `Bytes`（重审完整性规则 2）— ○ 采纳

- 现状：`<alg>(b: Bytes) -> Bytes` 与 `<alg>_hex(s: String) -> String` 中，`_hex` 同时改变“输出编码”和“输入类型”2 件事。它是规则 1“输入 `Bytes`”的唯一例外，`hash.fnv1a32` 已统一 `Bytes` 却只剩 `_hex`。规则 2 引用 R-L0-14 的 4，但类型变更会通过类型错误被察觉，规则并不禁止。
- 提案：改为 `<alg>_hex(b: Bytes) -> String`。摘要字符串时写 `hash.sha256_hex(bytes.from_string(s))`。
- 依据的 L0 规则：R-L0-00（`_hex` 仅一种含义）、规则 1。
- (a) Python `hashlib.sha256(b).hexdigest()`、Go `sha256.Sum256(b)`、Rust sha2 crate 均收字节。
- (d) 语义变更（类型错误可察觉）。`almide fix` 插入 `bytes.from_string(…)`。
- 完整性规则（替换 1・2）：`hash` 全部函数接收 `Bytes`，`_hex` 仅表示输出为小写十六进制。
- issue: #3985

<a id="r-hash-06-hmac-と定数時間の比較を-hash-に置く完全性の規則-4-の見直し---採用"></a>

### R-hash-06 将 HMAC 和常数时间比较放在 `hash`（重审完整性规则 4）— ○ 采纳

- 现状：没有带密钥摘要，模型会写 `hash.sha256(key ++ body)`，可通过 length extension 伪造。若再用 `==` 比较计算出的 MAC，会在首个不同字节返回，时间可逐 1 字节泄露 MAC。
- 提案：`hash.hmac_sha256(key: Bytes, b: Bytes) -> Bytes`、`hmac_sha256_hex`、`hmac_sha512`、`hmac_sha512_hex`，以及验证单元 `hash.is_valid_hmac_sha256(key, b, tag: Bytes) -> Bool`，sha512 设相同配对。十六进制 tag（webhook 头 `sha256=<hex>`）先 `hex.decode` 再传入：`hash.is_valid_hmac_sha256(key, body, hex.decode(sig) ?? bytes.from_list([]))`。非十六进制字符串成为空 tag，长度不同故 false。验证单元用 `bytes.is_equal_constant_time` 比较；比较函数共 2 个：`bytes.is_equal_constant_time(a, b)` 与 `string.is_equal_constant_time(a, b)`（R-bytes-29・R-string-19）。
- 密钥在前（RFC 2104 HMAC(K, m)，Go・Rust・Python・Node・Erlang・JVM 的 6/8）。两者都是 `Bytes`，类型无法发现交换错误，RFC 4231 向量固定顺序。
- 归属：R-L1-05 规定“返回摘要的函数归 `hash`”。HMAC 是 `Bytes -> Bytes` 摘要，写出错误 `sha256(key ++ msg)` 的模型已经在 `hash` 内，将正确单元放在旁边。
- md5・sha1 在 R-hash-07 已移除（初案是 `@insecure_digest` 与 critical profile 的 E100）。
- 常数时间主张：对于输入内容，源码及输出代码都无秘密依赖分支和下标。长度公开（长度不同则不扫描立即 false）。native 是 runtime 函数，每次经过 `core::hint::black_box` 累积 OR；wasm 是固定循环，仅下标比较构成分支。`tests/constant_time_compare_structure_test.rs` 检查两者输出代码（也确认提前退出的对照会失败）。HMAC 本体为 SHA 固定轮数，仅对公开长度分支（密钥是否长于块）。
- 完整性规则（替换 4，增加 5）：
  4. **有意遗漏**：SHA-224・SHA-384・SHA-3・BLAKE2、增量 hasher、带密钥的 AEAD 与签名（#2596 的 `crypto`）。
  5. **带密钥摘要族**：{sha256, sha512} × {计算, `is_valid_`} 共 4 单元（R-hash-07 移除 `_hex`，密钥和 tag 改为 `Secret[Bytes]`）。有意遗漏 `hmac_<alg>_hex`、`is_valid_hmac_<alg>_hex` 及 `hmac_md5`・`hmac_sha1`。不设 `is_valid_` 的 `_hex` 版，因为会赋予 `_hex`“输入编码”第 2 种含义（R-hash-05 规定 `_hex` 仅输出编码，按 R-L0-25 十六进制字符串用 `hex.decode` 还原）。至于 `hmac_md5`・`hmac_sha1`（Twilio 和 GitHub 旧头使用 SHA-1，所以若添加需附同样 `@insecure_digest`）。`tests/hash_hmac_family_gate_test.rs` 检查族。
- 依据的 L0 规则：R-L0-09（Bool 用 `is_`）、R-L0-30（每个角色 1 参数名：`key`・`b`・`tag`）、规则 1（输入 `Bytes`）、R-hash-05（`_hex` 仅输出编码）、R-L0-25（十六进制字符串用 `hex.decode`）、R-L0-01（不为 `is_valid_` 与 `hex.decode` 组合另起拼写）。
- (a) Swift CryptoKit `HMAC<SHA256>.isValidAuthenticationCode`、Rust `Mac::verify_slice`（验证单元）；Go `hmac.Equal`、Python `hmac.compare_digest`、Node `crypto.timingSafeEqual`（常数时间比较）。
- (d) 新增。md5・sha1 的 E100 仅在 `--profile critical` 拒绝，记入 epoch 15 breaks。
- 契约：C-422（RFC 4231）、C-423（常数时间比较）、C-424（R-hash-07 改为“stdlib 没有 MD5・SHA-1”）。
- issue: #4106

<a id="r-hash-07-秘密は-secret-で受け渡し壊れた要約を撤去するr-hash-03r-hash-06-の見直し---採用2026-10-26"></a>

### R-hash-07 通过 `Secret` 传递秘密，移除已破坏的摘要（重审 R-hash-03・R-hash-06）— ○ 采纳（2026-10-26）

- 现状：`hmac_sha256` 接收 `Bytes` 密钥、返回 `Bytes` tag，`hmac_sha256_hex` 返回 `String`。两种 tag 都能用 `==` 比较，而 `==` 在首个不同字节返回，时间泄露 MAC。默认 profile 可写 `md5`・`sha1`（安全表行 20）。
- 提案：
  1. `hmac_sha256` / `hmac_sha512` 为 `(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`。tag 的 `==` 仅采用常数时间比较（C-426）。不写 `secret.reveal` 就无法写出泄露比较。
  2. 移除 `hmac_sha256_hex` / `hmac_sha512_hex`。签名头使用 `hex.encode(secret.reveal(tag))`。尚未发布（0.69.0 新增），故没有 epoch 行。
  3. 保留 `is_valid_hmac_sha256` / `is_valid_hmac_sha512`，`tag` 也改为 `Secret[Bytes]`。
  4. 新增密钥来源 `env.secret(name) -> Secret[String]?`（L2-env R-env-05）。
  5. 移除 `md5`・`md5_hex`・`sha1`・`sha1_hex`。stdlib 内无调用。epoch 15 breaks 记 4 行（提示 `hash.sha256`）。不创建外部 package，`@insecure_digest` 和 critical E100 一并删除。
- 延后：不加入在检查时拒绝 `hash.sha256(secret.reveal(key) + body)` 的规则（把 reveal 值传给哈希的形式）。
- 门禁：`tests/hash_hmac_family_gate_test.rs`（声明接口的密钥和 tag 是 `Secret[Bytes]`、没有 `_hex`、`is_valid_` 本体经过常数时间比较）、`tests/removed_spellings_stay_removed_test.rs`（dialect 台账记录已移除的拼写不回到公开接口）。
- 契约：修改 C-422・C-424，新增 C-438（`env.secret`）。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（现行，mut 标注） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `fnv1a32` | `hash.fnv1a32(s: String) -> Int` | Go `fnv.New32a` | 语义变更（参数改为 `Bytes`；`fnv1a32(bytes.from_string(s))`） | R-hash-01 |
| `fnv1a32_bytes` | `hash.fnv1a32_bytes(b: Bytes) -> Int` | Go `fnv.New32a` | 移除 → `fnv1a32` | R-hash-01 |
| `sha256` | `hash.sha256(b: Bytes) -> Bytes` | Go `sha256.Sum256` / Python `hashlib.sha256(b).digest()` (F) / JS `subtle.digest("SHA-256")` / Elixir `:crypto.hash(:sha256, b)` | 保持 | — |
| `sha256_hex` | `hash.sha256_hex(b: Bytes) -> String` | Python `hexdigest()` (F) / Go `hex.EncodeToString` | 语义变更（`Bytes` 输入） | R-hash-05 |
| `crc32` | `hash.crc32(b: Bytes) -> Int`（提案） | Go `crc32.ChecksumIEEE` / Python `zlib.crc32` / Erlang `crc32` | 新增 | R-hash-02 |
| `adler32` | `hash.adler32(b: Bytes) -> Int`（提案） | Go `adler32.Checksum` / Python `zlib.adler32` | 新增 | R-hash-02 |
| `md5` | —（R-hash-03 新增、R-hash-07 移除） | — | 移除（不可用于安全；改用 `hash.sha256`） | R-hash-07 |
| `md5_hex` | —（R-hash-03 新增、R-hash-07 移除） | — | 移除（不可用于安全；改用 `hash.sha256`） | R-hash-07 |
| `sha1` | —（R-hash-03 新增、R-hash-07 移除） | — | 移除（不可用于安全；改用 `hash.sha256`） | R-hash-07 |
| `sha1_hex` | —（R-hash-03 新增、R-hash-07 移除） | — | 移除（不可用于安全；改用 `hash.sha256`） | R-hash-07 |
| `sha512` | `hash.sha512(b: Bytes) -> Bytes`（提案） | Go `sha512.Sum512` / Python `hashlib.sha512` (F) / JS `digest("SHA-512")` | 新增 | R-hash-03 |
| `sha512_hex` | `hash.sha512_hex(b: Bytes) -> String` | Python `hexdigest()` (F) | 新增（`Bytes` 输入） | R-hash-03、R-hash-05 |
| `hmac_sha256` | `hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`（提案） | Go `hmac.New(sha256.New, key)` / Python `hmac.digest(key, msg, "sha256")` / Node `createHmac("sha256", key)` / Erlang `crypto:mac(hmac, sha256, Key, Data)` | 新增 | R-hash-06 |
| `hmac_sha256_hex` | —（R-hash-06 新增、R-hash-07 移除） | Python `hexdigest()` | 移除（`hex.encode(secret.reveal(tag))`） | R-hash-07 |
| `hmac_sha512` | `hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]`（提案） | Go `hmac.New(sha512.New, key)` / Python `hmac.digest(key, msg, "sha512")` | 新增 | R-hash-06 |
| `hmac_sha512_hex` | —（R-hash-06 新增、R-hash-07 移除） | Python `hexdigest()` | 移除（`hex.encode(secret.reveal(tag))`） | R-hash-07 |
| `is_valid_hmac_sha256` | `hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool`（提案） | Swift `HMAC<SHA256>.isValidAuthenticationCode` / Rust `Mac::verify_slice` | 新增 | R-hash-06 |
| `is_valid_hmac_sha512` | `hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool`（提案） | Swift `HMAC<SHA512>.isValidAuthenticationCode` | 新增 | R-hash-06 |
