<!-- description: Cryptographic functions (HMAC, encryption, signing, secure random) -->
# stdlib: crypto [Tier 2]

暗号機能。Almide は現在 `hash` モジュール（bundled .almd, SHA/MD5 のみ）を持つが、HMAC・暗号化・署名・安全な乱数がない。

## 他言語比較

### ハッシュ

| アルゴリズム | Go | Python | Rust | Deno | Almide |
|------------|-----|--------|------|------|--------|
| SHA-256 | `crypto/sha256` | `hashlib.sha256()` | `sha2` crate | `crypto.subtle.digest` | ✅ `hash.sha256` |
| SHA-512 | `crypto/sha512` | `hashlib.sha512()` | `sha2` crate | `crypto.subtle.digest` | ❌ |
| MD5 | `crypto/md5` | `hashlib.md5()` | `md5` crate | ❌ | ✅ `hash.md5` |
| BLAKE2 | `x/crypto/blake2b` | `hashlib.blake2b()` | `blake2` crate | ❌ | ❌ |
| インクリメンタル | `h.Write(); h.Sum()` | `h.update(); h.digest()` | `hasher.update(); finalize()` | ❌ (single-shot) | ❌ |

### HMAC

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| sign | `hmac.New(sha256.New, key)` | `hmac.new(key, msg, sha256)` | `Hmac::<Sha256>::new(key)` | `crypto.subtle.sign("HMAC", ...)` |
| verify | `hmac.Equal(a, b)` | `hmac.compare_digest(a, b)` | `hmac::verify(key, data, tag)` | `crypto.subtle.verify(...)` |

### 暗号化

| アルゴリズム | Go | Python | Rust | Deno |
|------------|-----|--------|------|------|
| AES-GCM | `crypto/cipher` | `cryptography` pkg | `aes_gcm` crate | `crypto.subtle.encrypt` |
| ChaCha20-Poly1305 | `x/crypto` | `cryptography` pkg | `chacha20poly1305` crate | ❌ |

### 署名

| アルゴリズム | Go | Python | Rust | Deno |
|------------|-----|--------|------|------|
| Ed25519 | `crypto/ed25519` | `cryptography` pkg | `ed25519_dalek` / ring | `crypto.subtle` |
| ECDSA P-256 | `crypto/ecdsa` | `cryptography` pkg | `p256` crate / ring | `crypto.subtle` |
| RSA | `crypto/rsa` | `cryptography` pkg | `rsa` crate | `crypto.subtle` |

### 安全な乱数

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| random bytes | `crypto/rand.Read(buf)` | `secrets.token_bytes(n)` | `rand::fill_bytes` | `crypto.getRandomValues` |
| random int (secure) | `rand.Int(reader, max)` | `secrets.randbelow(n)` | `thread_rng().gen_range()` | manual |
| random hex | manual | `secrets.token_hex(n)` | manual | manual |

### 鍵導出

| アルゴリズム | Go | Python | Rust | Deno |
|------------|-----|--------|------|------|
| PBKDF2 | `x/crypto/pbkdf2` | `hashlib.pbkdf2_hmac` | `pbkdf2` crate | `crypto.subtle.deriveBits` |
| HKDF | `x/crypto/hkdf` | `cryptography` pkg | `hkdf` crate | `crypto.subtle.deriveBits` |

## 追加候補 (~15 関数)

### P0 (基本)
- `crypto.random_bytes(n) -> List[Int]` — 暗号学的安全乱数
- `crypto.random_hex(n) -> String` — ランダム hex 文字列
- `crypto.hmac_sha256(key, data) -> String` — HMAC-SHA256
- `crypto.hmac_verify(key, data, signature) -> Bool` — HMAC 検証

### P1 (暗号化)
- `crypto.encrypt_aes_gcm(key, plaintext, nonce) -> List[Int]`
- `crypto.decrypt_aes_gcm(key, ciphertext, nonce) -> Result[List[Int], String]`
- `crypto.generate_key(bits) -> List[Int]` — AES 鍵生成

### P1 (ハッシュ拡充)
- `hash.sha512(data) -> String`
- `hash.blake2b(data) -> String`

### P2 (署名)
- `crypto.sign_ed25519(private_key, data) -> List[Int]`
- `crypto.verify_ed25519(public_key, data, signature) -> Bool`
- `crypto.generate_ed25519_keypair() -> { public: List[Int], private: List[Int] }`

### P2 (鍵導出)
- `crypto.pbkdf2(password, salt, iterations, key_length) -> List[Int]`
- `crypto.hkdf(key, salt, info, length) -> List[Int]`

## 実装戦略

@extern で Rust crate (ring or RustCrypto) をラップ。TS: Web Crypto API。hash モジュールの既存実装を crypto に統合するか、hash は crypto のサブセットとして残すか要検討。
