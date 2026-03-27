<!-- description: Five-layer security model making web vulnerabilities compile-time errors -->
# Secure by Design [ON HOLD]

## Thesis

Almide は Rust がメモリ安全であるのと同じ意味で **Web 安全** な言語になる。「気をつけて書けば安全」ではなく「普通に書いたら安全。意図的に壊そうとしない限り壊れない」。

```
Rust:  unsafe を書かなければメモリ安全
Almide: @extern を書かなければ Web 安全
```

## Security Model

Almide のセキュリティは 5 層で構成される。各層が独立に機能し、全層が揃うと supply chain 含めた構造的安全性が成立する。

### Layer 1: Effect Isolation — pure fn は I/O 不可能

```almide
fn parse(s: String) -> Value = ...          // pure。I/O 不可能
effect fn load(path: String) -> String = ... // I/O 可能
```

- `fn` は `effect fn` を呼べない。コンパイラが検証
- pure fn は外界に一切アクセスできない。データ窃取も外部通信も型エラー
- **セキュリティ上の意味**: パッケージが pure fn しか export してなければ、そのパッケージは原理的に無害

**Status: ✅ 言語に実装済み。** effect system は動いている。

### Layer 2: Single Bridge — @extern が外界との唯一の接点

```almide
@extern(platform: web, "fetch")
effect fn fetch(url: String) -> Response
```

- ネイティブ API を呼ぶ唯一の方法が `@extern`
- `eval()`, `require()`, dynamic `import()` は言語に存在しない
- **セキュリティ上の意味**: コードベース内の `@extern` を grep すれば、外界との全接点が列挙できる

**Status: ✅ @extern は実装済み。** ❌ platform タグは未実装 (→ platform-target-separation.md)。

### Layer 3: Opaque Types — 危険な出力の構築手段を限定

```almide
type SafeHtml = opaque String
type SafeSql  = opaque String
type SafePath = opaque String
```

- `opaque` 型は外部から直接構築できない
- `SafeHtml` を作る唯一の方法が builder (auto-escape 付き)
- `SafeSql` を作る唯一の方法がパラメタライズドクエリ関数
- stdlib の I/O API が opaque 型のみ受け付ける: `Response.html(body: SafeHtml)`
- **セキュリティ上の意味**: XSS, SQL injection, command injection, path traversal が型エラーになる

```almide
// コンパイルエラー: Response.html は SafeHtml を要求、String は渡せない
let html = "<p>" ++ user_input ++ "</p>"
Response.html(html)  // ← type error: expected SafeHtml, got String

// OK: builder が自動 escape
let doc = Html { p { user_input } }
Response.html(doc |> render)  // ← SafeHtml が返る
```

**Status: ❌ opaque 型は未実装。** 言語機能として parser + checker + codegen への追加が必要。

### Layer 4: Capability Inference — パッケージの権限をコンパイラが推論

コンパイラが関数の呼び出しグラフを辿り、各関数が推移的にどの `@extern` に到達するかを追跡する。

```
json-parser パッケージ
├── parse()      → fn (pure) → @extern なし
├── stringify()  → fn (pure) → @extern なし
└── 推論結果: capabilities = [] (pure)

http-client パッケージ
├── get()        → effect fn → fetch → @extern(platform: web, "fetch")
├── post()       → effect fn → fetch → @extern(platform: web, "fetch")
└── 推論結果: capabilities = [network]

sketchy-logger パッケージ
├── log()        → effect fn → write_file → @extern(platform: node, "fs", ...)
├── report()     → effect fn → http.post → @extern(platform: web, "fetch")
└── 推論結果: capabilities = [fs, network] ⚠️
```

利用側で capability を制限:

```toml
# almide.toml
[dependencies.json-parser]
version = "1.0"
capabilities = []              # pure のみ許可

[dependencies.sketchy-logger]
version = "1.0"
capabilities = ["fs"]          # fs のみ許可、network は不許可
```

```
error: sketchy-logger requires capability "network", but only ["fs"] granted
  --> almide.toml:7:1
   |
   = note: sketchy-logger/src/report.almd:5 calls http.post
   = note: http.post uses @extern(platform: web, "fetch")
   = hint: add "network" to capabilities, or use a different package
```

**Status: ❌ 未実装。** 必要なもの:
- @extern の platform タグ (→ platform-target-separation.md)
- コンパイラの capability 推論パス (effect 伝播の延長)
- almide.toml の capabilities フィールド
- 推移的 capability 検証と診断メッセージ

### Layer 5: Supply Chain Integrity — パッケージの改ざん検出

```toml
[dependencies.http-client]
version = "2.0"
hash = "sha256:a1b2c3d4e5f6..."
capabilities = ["network"]
```

- パッケージはソースの content hash で固定
- 同じバージョンでもハッシュが変われば**コンパイルエラー**
- capability の変化も検出: v2.0 で pure だったパッケージが v2.1 で network を要求 → 明示的な承認が必要

```
warning: http-client 2.0 → 2.1 adds new capability "fs"
  --> almide.toml:3:1
   |
   = note: http-client@2.1/src/cache.almd uses fs.write_file
   = hint: add "fs" to capabilities, or pin to version 2.0
```

**Status: ❌ 未実装。** パッケージレジストリ自体が未構築 (→ package-registry.md)。

## Attack Surface Elimination

全 5 層が揃ったとき、各攻撃がどの層で止まるか:

| 攻撃 | Layer 1 | Layer 2 | Layer 3 | Layer 4 | Layer 5 |
|---|---|---|---|---|---|
| XSS (文字列注入) | | | **opaque SafeHtml** | | |
| SQL injection | | | **opaque SafeSql** | | |
| Command injection | | | **opaque SafeCmd** | | |
| Path traversal | | | **opaque SafePath** | | |
| パッケージがデータ窃取 | **effect 隔離** | **@extern 限定** | | **capability 検出** | |
| install 時コード実行 | | **eval なし** | | | **hash 検証** |
| 依存の依存が汚染 | | | | **推移的追跡** | **hash 検証** |
| prototype pollution | **immutable** | **prototype なし** | | | |
| eval injection | | **eval なし** | | | |
| バージョン上書き攻撃 | | | | | **content hash** |
| typosquatting | | | | **capability 不一致** | **hash 検証** |

**1 つの攻撃が複数の層で止まる = defense in depth。**

## Implementation Order

依存関係に基づく実装順序:

```
Phase 1: opaque 型
  ← 言語機能の追加。parser + checker + codegen
  ← SafeHtml, SafeSql, SafePath の基盤
  ← builder の lift が SafeHtml を返すようにする
  ← これだけで XSS/SQLi/Command injection が型エラーになる

Phase 2: @extern platform タグ
  ← platform-target-separation.md
  ← @extern(platform: web, ...) / @extern(platform: node, ...) 等
  ← capability 推論の前提

Phase 3: capability 推論
  ← Phase 2 の上に構築
  ← コンパイラが @extern の推移的到達を追跡
  ← パッケージごとの capability を自動算出
  ← almide.toml の capabilities フィールド
  ← これで supply chain の capability 検証が動く

Phase 4: supply chain integrity
  ← package-registry.md
  ← content-addressed hash
  ← capability 変化の検出
  ← @extern 使用制限ポリシー
```

**Phase 1 だけで XSS/SQLi/Command injection/Path traversal が型エラーになる。** 最大のインパクトが最小の実装コストで得られる。

Phase 2-3 で supply chain security が加わる。Phase 4 はインフラ (レジストリ) 依存。

## Prerequisites

| Phase | 依存する roadmap item | 理由 |
|---|---|---|
| Phase 1 | なし (言語本体の追加) | opaque は独立した型システム拡張 |
| Phase 2 | platform-target-separation.md | @extern の platform タグ |
| Phase 3 | Phase 2 + effect system (既存) | capability = platform タグの推移的推論 |
| Phase 4 | package-registry.md | content hash にはレジストリが必要 |

## What Already Works (Layer 0)

これらは既に言語に焼き付いており、変更不要:

- ✅ `fn` は I/O 不可能 (effect system)
- ✅ `@extern` が唯一の FFI bridge
- ✅ `eval()` / dynamic import が存在しない
- ✅ prototype chain が存在しない
- ✅ immutable by default
- ✅ 静的型で全コードパスが見える
- ✅ パッケージは .almd ソースファイル (install 時コード実行なし)

**この Layer 0 が最も重要であり、最も変更が困難な部分。** Almide はこれを既に持っている。npm/Node.js がこれを後から得ることは不可能 (`require()` が全権限を持つ設計が根本にある)。

## Design Principle

**「安全でないコードを書けなくする」のではなく「普通に書くと安全になる」。**

- builder で HTML を書く → 自動 escape (安全)
- `sql()` で SQL を書く → 自動パラメタライズ (安全)
- パッケージを使う → capability が自動推論される (安全)
- `@extern` を書く → ここだけが「意図的に安全の外に出る」行為

Rust で `unsafe` は「ここから先は自分が責任を持つ」マーカー。Almide で `@extern` も同じ。**言語のデフォルトが安全で、危険は明示的。**

## Why ON HOLD

Phase 1 (opaque 型) は言語コアの安定化後に着手可能。Phase 2 以降は platform-target-separation.md とパッケージレジストリに依存。

ただし:

- **Layer 0 は既に完成している** — 最も重要で変更困難な部分
- **Phase 1 (opaque) だけで XSS/SQLi/Path traversal が型エラーになる** — 最小投資で最大効果
- **全体の設計に未解決の研究課題がない** — 既知の技術の組み合わせ

Rust がメモリ安全を言語の性質にしたように、Almide が Web 安全を言語の性質にする。技術的には可能。順序の問題。

## Coverage — 全 Phase 完了後に何が解決し、何が残るか

### 構造的にゼロになるもの (言語が保証)

| カテゴリ | 解決度 | メカニズム |
|---|---|---|
| インジェクション系 (XSS, SQLi, CMDi) | **100%** | opaque 型。危険な sink に String を渡せない。型エラー |
| Path traversal | **100%** | opaque SafePath。構築時にバリデーション強制 |
| prototype pollution | **100%** | prototype chain が言語に存在しない |
| eval injection | **100%** | eval / dynamic import が言語に存在しない |
| install 時コード実行 | **100%** | パッケージは .almd ソースファイル。実行フックが存在しない |
| supply chain (悪意あるパッケージ) | **95%** | capability 推論でコンパイル時検出。ただし許可した capability 内での悪用は残る |
| バージョン上書き / typosquatting | **100%** | content-addressed hash。ハッシュ不一致でコンパイルエラー |

### 構造的に解決しないもの (どの言語でも残る)

| カテゴリ | 解決度 | 理由 |
|---|---|---|
| ロジックバグ (認可漏れ、IDOR 等) | **0%** | 「この操作を許可すべきか」はビジネスロジック。型で表現不能 |
| 検証関数の中身の正しさ | **0%** | opaque 型の構築手段は制限できるが、その構築関数自体の正しさは人間の責任 |
| サイドチャネル / タイミング攻撃 | **0%** | 実行時間の均一性はコンパイラの保証範囲外 |
| SSRF (完全な防止) | **部分的** | SafeUrl 型 + allowlist で軽減できるが、allowlist 自体の正しさは人間の責任 |

### 意味

**OWASP Top 10 の過半数が構造的にゼロになる。** 残るのは「どの言語で書いても残る問題」のみ。Almide で書いたら気にしなくていい問題と、どの言語で書いても気にすべき問題が明確に分かれる。
