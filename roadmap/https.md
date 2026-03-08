# HTTPS Client via System TLS

## 方針
TLS は自前実装しない。OS 標準の TLS ライブラリを `rustc -l` で直接リンク。
cargo 不要、バイナリサイズ増加なし（動的リンク）。

## アーキテクチャ

```
Almide code
  → http.get("https://...")
  → 生成 Rust: almide_https_get(url)
  → FFI: system TLS library
  → rustc -l framework=Security (macOS) / -l ssl -l crypto (Linux)
```

## 実装ステップ

### Step 1: URL パーサーの HTTPS 対応
- `https://` スキームを認識
- ポートデフォルト 443

### Step 2: macOS 実装 (Security.framework)
- `SSLCreateContext` → `SSLSetIOFuncs` → `SSLHandshake`
- CFNetwork の `CFReadStream` / `CFWriteStream` が簡単かも
- もしくは macOS 標準の `URLSession` を FFI で呼ぶ
- 証明書検証は OS に任せる（自前検証しない）

### Step 3: Linux 実装 (OpenSSL)
- `SSL_CTX_new` → `SSL_new` → `SSL_connect` → `SSL_read/write`
- `extern "C"` で OpenSSL の関数を宣言
- 証明書検証は OpenSSL のデフォルト設定に任せる

### Step 4: 統合
- コンパイル時に OS 検出（`#[cfg(target_os)]`）
- macOS / Linux で異なる FFI を使い分け
- `rustc` のリンクフラグを almide CLI で自動設定

### Step 5: テスト
- `https://httpbin.org/get` への GET テスト
- 証明書エラーのハンドリング（自己署名証明書など）
- HTTP / HTTPS 両方が同じ `http.get` API で動くことを確認

## 生成コード例

```rust
// macOS
#[cfg(target_os = "macos")]
fn almide_https_get(url: &str) -> Result<String, String> {
    // Security.framework FFI
}

// Linux
#[cfg(target_os = "linux")]
fn almide_https_get(url: &str) -> Result<String, String> {
    // OpenSSL FFI
}
```

## rustc リンクフラグ

```bash
# macOS
rustc main.rs -l framework=Security -l framework=CoreFoundation

# Linux
rustc main.rs -l ssl -l crypto
```

## almide CLI の変更
- `almide build` / `almide run` で HTTPS 関数が使われてたら自動でリンクフラグ追加
- WASM ターゲットでは HTTPS 未サポート（ホスト側が提供）

## リスク
- OS 間の FFI 差異が大きい（macOS と Linux で別実装が必要）
- Windows 未対応（将来 Schannel で対応可能）
- OpenSSL のバージョン差異（1.1 vs 3.x）

## 代替案（フォールバック）
System TLS が複雑すぎる場合、curl FFI へフォールバック：
```rust
fn almide_https_get(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .arg("-s").arg(url)
        .output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```
これなら全 OS で動く。パフォーマンスは劣るがセキュリティは確保。

## Priority
Step 1 → Step 4 (curl fallback) → Step 2 (macOS) → Step 3 (Linux)
curl fallback を先に入れて動くようにし、system TLS は段階的に置換。
