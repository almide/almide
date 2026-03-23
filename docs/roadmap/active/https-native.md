# HTTPS Native Support [ACTIVE]

**目標**: `http.get("https://...")` が全ターゲットで動く

## 現状

| ターゲット | HTTP | HTTPS | 実装方法 | 問題 |
|---|---|---|---|---|
| Rust | OK | **NG** | `TcpStream` 直叩き | TLS 未実装。`parse_url` が `https://` を剥いで port 80 に平文接続 |
| TS (Deno) | OK | OK | `fetch` ネイティブ | なし |
| WASM | NG | NG | WASI にソケット API なし | ネットワーク自体が不可 |

**修正が必要なのは Rust ターゲットのみ。** TS は既に動く。

## Rust ターゲットの問題箇所

`runtime/rs/src/http.rs` line 200-210:

```rust
fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let url = url.strip_prefix("http://").unwrap_or(url);  // https:// を無視
    // ... port 80 で TcpStream に接続
}
```

TLS ハンドシェイクなしで HTTPS サーバーに平文 HTTP リクエストを送っている。サーバーは応答しないか、接続を切る。

## 選択肢

### A. rustls (pure Rust TLS) — 推奨

```
Cargo.toml に rustls + webpki-roots を追加
runtime/rs/src/http.rs で URL scheme に応じて TcpStream or TlsStream を分岐
```

**Pros:**
- Pure Rust — C 依存なし、クロスプラットフォーム
- `almide build` で生成されるバイナリにも TLS が入る
- 信頼性が高い（Let's Encrypt のバックエンドで使われている）

**Cons:**
- crate 依存が増える (rustls + webpki-roots)
- バイナリサイズ +300-500KB (strip 後)
- Almide の「zero external crate」方針からの逸脱

**判断:** Almide の zero-dep は「ユーザーが rustc 以外を入れなくていい」という意味であって、コンパイラ自体の Cargo.toml に crate を追加することは許容範囲。rustls は Almide バイナリに静的リンクされるので、ユーザーは何もインストールしない。

### B. native-tls (OS の TLS を使う)

**Pros:**
- OS の TLS を使うのでバイナリサイズ増が少ない
- macOS: Security.framework、Linux: OpenSSL、Windows: SChannel

**Cons:**
- C 依存 — クロスコンパイルが面倒
- Linux で OpenSSL のヘッダーが必要 (`apt install libssl-dev`)
- ユーザーの環境に依存する

**判断:** NG。ユーザー環境への依存は許容しない。

### C. curl 外部コマンド fallback (現行方針)

**Pros:**
- 実装が簡単

**Cons:**
- curl がない環境で動かない (Windows デフォルト等)
- 外部プロセス起動のオーバーヘッド
- エラーハンドリングが不安定
- 現時点で動いていない

**判断:** NG。根本解決にならない。

### D. 自前 TLS 実装

**判断:** 論外。

## 推奨: 方針 A (rustls)

### 実装計画

#### Phase 1: Rust ランタイムの修正

1. **Cargo.toml** に追加:
   ```toml
   [dependencies]
   rustls = { version = "0.23", features = ["ring"] }
   webpki-roots = "0.26"
   ```

2. **runtime/rs/src/http.rs** の `parse_url` を修正:
   - `https://` を認識して scheme, host, port, path を返す
   - デフォルト port: http → 80, https → 443

3. **runtime/rs/src/http.rs** の `almide_http_request` を修正:
   - scheme が `https` なら `rustls::ClientConnection` + `rustls::StreamOwned` で TLS 接続
   - scheme が `http` なら従来通り `TcpStream`
   - リクエスト/レスポンスの読み書きは共通化 (`Read + Write` trait で抽象化)

4. **テスト**:
   - `http.get("https://httpbin.org/get")` が動くことを確認
   - `http.get("http://httpbin.org/get")` が引き続き動くことを確認

#### Phase 2: WASM ターゲット (将来)

WASM ではソケット自体がないので、ホストインポートで解決:

```
// WASI に HTTP が来たら (wasi:http/outgoing-handler)
import wasi:http/outgoing-handler@0.2.0

// それまでは WASM ターゲットで http.get は未サポート
```

これは WASI の HTTP 仕様が安定するまで待つ。当面は Rust/TS/JS ターゲットで HTTPS が動けば十分。

## 影響範囲

| 変更 | ファイル |
|------|---------|
| Cargo.toml | rustls + webpki-roots 追加 |
| runtime/rs/src/http.rs | parse_url + almide_http_request 修正 |
| 既存テスト | 影響なし (HTTP は引き続き動く) |
| バイナリサイズ | +300-500KB (strip 後の almide バイナリ) |
| TS/JS ランタイム | 変更なし (既に動く) |
| WASM | 変更なし (将来対応) |

## 優先度

High — ハンズオン記事で `https://dummyjson.com` が動かないのは致命的。
