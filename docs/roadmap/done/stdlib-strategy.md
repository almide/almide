<!-- description: Stdlib expansion strategy via Rust ecosystem wrapping -->
# Stdlib Strategy [ACTIVE]

普及には「書きたいものがすぐ書ける」stdlib の厚みが必要。現在 15 モジュール 266 関数。主要言語と比較すると：

| 言語 | stdlib モジュール数 | 関数/メソッド数 |
|------|-------------------|----------------|
| Go | ~150 packages | 数千 |
| Python | ~200 modules | 数万 |
| Rust (std) | ~50 modules | 数千 |
| Deno (std) | ~30 modules | 数百 |
| **Almide** | **15 + 6 bundled** | **~282** |

Almide が Rust にコンパイルする特性を活かし、**Rust エコシステムをラップする**のが最速の拡充戦略。

---

## 現状 (v0.5.13)

### Layer 1: core（全ターゲット、WASM OK）
| Module | 関数数 | 状態 |
|--------|--------|------|
| string | 36 | ✅ 充実 |
| list | 54 | ✅ 充実 |
| int | 21 | ✅ 十分 |
| float | 16 | ✅ 十分 |
| map | 16 | △ 基本のみ |
| math | 21 | ✅ 十分 |
| json | 36 | ✅ 充実 |
| regex | 8 | △ 基本のみ |
| result | 9 | ✅ 完備 |

### Layer 2: platform（native only）
| Module | 関数数 | 状態 |
|--------|--------|------|
| fs | 19 | △ 基本のみ |
| process | 6 | △ 最小限 |
| io | 3 | △ 最小限 |
| env | 9 | △ 基本のみ |
| http | 8 | △ 基本のみ |
| random | 4 | △ 最小限 |

### Bundled .almd
| Module | 状態 |
|--------|------|
| path | ✅ 十分 |
| time | △ 基本のみ |
| hash | △ SHA/MD5 のみ |
| encoding | △ base64/hex のみ |
| args | △ 基本のみ |
| term | △ 基本のみ |

---

## 拡充戦略

### 戦略 1: TOML + ランタイムで追加（現行方式）

新関数の追加コスト:
1. `stdlib/defs/<module>.toml` に定義追加
2. `src/emit_rust/<xxx>_runtime.txt` に Rust 実装追加
3. `src/emit_ts_runtime.rs` に TS 実装追加
4. `cargo build` で自動生成

**メリット:** コンパイラ本体の変更不要、型安全、TOML 定義が LLM にも読みやすい
**デメリット:** 2ターゲット分の実装が必要、Rust crate の機能をラップするのに手動翻訳が要る

**適用先:** core 層の関数追加（string, list, map, math の拡充）

### 戦略 2: @extern で Rust crate をラップ

`@extern(rs, "crate", "function")` で Rust crate の関数を直接呼ぶ。

```almide
@extern(rs, "chrono", "Utc::now().to_rfc3339")
@extern(ts, "Date", "new Date().toISOString")
fn now_iso() -> String
```

**メリット:** Rust エコシステムの全機能にアクセス、実装コスト最小
**デメリット:** TS 側も別途実装が要る、型マッピングは trust-based（安全性はユーザー責任）

**適用先:** platform 層の新モジュール（datetime, crypto, database, etc.）

### 戦略 3: Almide で self-host

Pure な計算ロジックを Almide 自体で書く。

**メリット:** 両ターゲット自動対応、テストが Almide で書ける
**デメリット:** パフォーマンスが Almide の生成コード品質に依存

**適用先:** csv, toml パーサー、データ変換、バリデーション

### 戦略 4: 公式拡張パッケージ (x/)

stdlib とは独立してバージョン管理。`almide.toml` に依存追加。

**メリット:** stdlib のバージョンロックから解放、コミュニティ貢献しやすい
**デメリット:** パッケージレジストリが前提（現在 on-hold）

**適用先:** 大きい機能（web フレームワーク、ORM、テンプレートエンジン）

---

## 足りないモジュール（優先度順）

### Tier 1: これがないと実用プログラムが書けない

| モジュール | 内容 | 戦略 | 見積り |
|-----------|------|------|--------|
| **datetime** | 日時パース/フォーマット/タイムゾーン/比較 | TOML + runtime (Rust: chrono, TS: Intl) | 20-30 関数 |
| **fs (拡充)** | ディレクトリ走査, 再帰削除, 権限, temp, watch | TOML + runtime | +15 関数 |
| **http (拡充)** | ヘッダ操作, ステータスコード, cookie, multipart | TOML + runtime | +20 関数 |
| **error** | エラー型の構造化, スタックトレース, chain | TOML + runtime | 10 関数 |

### Tier 2: 多くのアプリケーションで必要

| モジュール | 内容 | 戦略 | 見積り |
|-----------|------|------|--------|
| **csv** | parse/stringify, ヘッダ付き, ストリーミング | self-host (.almd) | 8-10 関数 |
| **toml** | parse/stringify | self-host (.almd) | 6-8 関数 |
| **yaml** | parse/stringify | @extern (serde_yaml / js-yaml) | 4-6 関数 |
| **url** | パース/構築/エンコード/クエリパラメータ | self-host (.almd) | 10 関数 |
| **crypto** | HMAC, AES, RSA, random bytes | @extern (ring / Web Crypto) | 10-15 関数 |
| **uuid** | v4 生成, パース, フォーマット | @extern (uuid / crypto.randomUUID) | 4 関数 |

### Tier 3: エコシステム成長に必要

| モジュール | 内容 | 戦略 | 見積り |
|-----------|------|------|--------|
| **sql** | パラメタライズドクエリ, SQLite/PostgreSQL | @extern + x/ package | 15 関数 |
| **websocket** | client/server, メッセージ送受信 | @extern | 8 関数 |
| **log** | 構造化ログ, レベル, フォーマッタ | TOML + runtime | 6 関数 |
| **test** | モック, スパイ, ベンチマーク | TOML + runtime | 10 関数 |
| **compress** | gzip/zstd 圧縮・展開 | @extern | 4 関数 |
| **image** | 基本的な画像操作 | x/ package | 15 関数 |

---

## 数値目標

| マイルストーン | モジュール数 | 関数数 | 基準 |
|---------------|-------------|--------|------|
| 現在 (v0.5.13) | 21 | ~282 | — |
| **v0.6 (実用最小限)** | 25 | 400+ | Tier 1 完了 |
| **v0.8 (アプリ開発可能)** | 32 | 550+ | Tier 2 完了 |
| **v1.0 (プロダクションレディ)** | 38+ | 700+ | Tier 3 の主要モジュール |

比較: Go 1.0 は ~100 packages で出荷。Deno 1.0 は ~30 modules。Almide は Rust にコンパイルするため、@extern 経由で Rust crate を使えばモジュール数以上の機能にアクセスできる。

---

## LLM 適性の観点

### 一貫した命名規則

```
module.verb_noun(args)     — 基本形
module.verb_noun?(args)    — Bool を返す
module.try_verb(args)      — Result を返す
```

全モジュールでこのパターンを徹底する。LLM は一貫したパターンを学習しやすい。

### UFCS で自然な呼び出し

```almide
// 両方書ける
string.trim(s)
s.trim()

// LLM は UFCS を好む傾向がある（メソッドチェーンが読みやすい）
text
  |> string.trim()
  |> string.split(",")
  |> list.map(fn(x) => string.trim(x))
```

### stdlib ドキュメントの機械可読性

将来の `almide doc` が LLM 向けにも使えるよう、TOML 定義に `description` フィールドを追加：

```toml
[trim]
description = "Remove leading and trailing whitespace from a string"
params = [{ name = "s", type = "String" }]
return = "String"
```

これにより LLM のプロンプトに stdlib リファレンスを自動注入できる。

---

## 実装の順番

```
1. Tier 1 (datetime, fs拡充, http拡充, error)  ← v0.6 目標
   ↓
2. TOML に description フィールド追加          ← LLM 適性
   ↓
3. Tier 2 (csv, toml, url, crypto, uuid)       ← v0.8 目標
   ↓
4. @extern の型安全性強化                      ← Phase 0 前提
   ↓
5. Tier 3 (sql, websocket, log, test)          ← v1.0 目標
   ↓
6. x/ パッケージ分離                           ← パッケージレジストリ前提
```

## 自動収集ツール（Almide で作る）

stdlib 設計のための他言語 API リファレンスを自動収集するツールを **Almide 自体で実装** する。dogfooding + 実用ツール + stdlib 充実度ベンチマークの一石三鳥。

### 構想

```almide
// 言語の stdlib/lib 情報を統一フォーマットで取得
effect fn main() =
  let go_time = fetch_module("go", "time")
  let py_datetime = fetch_module("python", "datetime")
  let report = compare([go_time, py_datetime])
  fs.write_text("docs/roadmap/stdlib/auto/time.md", render_markdown(report))
```

### 統一出力フォーマット

```json
{
  "language": "go",
  "module": "time",
  "functions": [
    {
      "name": "Now",
      "params": [],
      "return": "Time",
      "description": "returns the current local time"
    }
  ]
}
```

### データソース

**推奨: 各言語のリフレクション/ドキュメントツール経由（スクレイピング不要）**

```bash
# Python: inspect で全関数のシグネチャを JSON 化
python3 -c "import inspect, json, datetime; print(json.dumps([
  {'name': n, 'params': str(inspect.signature(f))}
  for n, f in inspect.getmembers(datetime, inspect.isfunction)
]))"

# Go: go doc -json でパッケージ情報を構造化出力
go doc -all -json time

# Rust: rustdoc が JSON 出力をサポート
rustdoc --output-format json --edition 2021 src/lib.rs

# Deno: deno doc が JSON 出力をサポート
deno doc --json https://deno.land/std/csv/mod.ts

# Node: TypeScript 型定義 (.d.ts) をパース
# or: Object.keys(require('fs')) で関数一覧取得
```

Almide の `process.exec` でこれらのコマンドを叩き、JSON を統合するだけで正確な API リファレンスが取れる。HTTP スクレイピングよりも確実で正確。

| 言語 | ツール | 形式 | 精度 |
|------|--------|------|------|
| Python | `inspect` module | JSON (自前変換) | 完全（シグネチャ + docstring） |
| Go | `go doc -json` | JSON | 完全（型情報含む） |
| Rust | `rustdoc --output-format json` | JSON | 完全（crate 単位） |
| Deno | `deno doc --json` | JSON | 完全（モジュール単位） |
| Node/npm | `.d.ts` ファイルパース | TypeScript AST | 高精度 |
| Swift | `swift-symbolgraph-extract` | JSON (Symbol Graph) | 完全（モジュール単位） |
| Kotlin | `dokka` or `kotlin-reflect` | JSON / リフレクション | 高精度 |
| Ruby | `ri --format=json` or `RDoc::RI` | JSON | 完全（メソッド + docstring） |

```bash
# Swift: Symbol Graph で全 API を JSON 出力
swift-symbolgraph-extract -module-name Foundation -target x86_64-apple-macosx

# Kotlin: kotlin-reflect でクラス/関数一覧
kotlinc -script -e "kotlin.io.path.Path::class.members.forEach { println(it) }"
# or: Dokka で JSON ドキュメント生成

# Ruby: ri でメソッド一覧
ri --format=json File
# or: Ruby リフレクション
ruby -e "puts File.methods(false).sort"
```

**フォールバック: Web API / HTML スクレイピング**

ローカルにツールがない場合のフォールバック：

| 言語 | ソース | 形式 |
|------|--------|------|
| Go | `pkg.go.dev` | HTML スクレイピング |
| Python | `docs.python.org` | HTML スクレイピング |
| Rust | `docs.rs` | HTML スクレイピング |
| Deno | `doc.deno.land` | JSON API |
| npm | `registry.npmjs.org` | JSON API |
| Swift | `developer.apple.com/documentation` | HTML スクレイピング |
| Kotlin | `kotlinlang.org/api` | HTML スクレイピング |
| Ruby | `ruby-doc.org` | HTML スクレイピング |

### 拡張

- stdlib → third-party lib にも同じ仕組みで展開
- `almide stdlib-compare datetime` で Go/Python/Rust/Deno の datetime 相当を一覧表示
- CI で定期実行 → `docs/roadmap/stdlib/auto/` に自動更新

### 前提条件

- Almide の http + json + string で十分実装可能（追加機能不要）
- HTML パース用の簡易セレクタがあると便利（将来 stdlib 候補）

## ベンチマーク対象言語

### 従来言語（stdlib 機能比較）

| 言語 | リフレクション手段 | stdlib 規模 |
|------|-------------------|-------------|
| Go | `go doc -json` | ~150 packages |
| Python | `inspect` module | ~200 modules |
| Rust | `rustdoc --output-format json` | ~50 modules + crates |
| Deno | `deno doc --json` | ~30 modules |
| Swift | `swift-symbolgraph-extract` | Foundation + 標準 |
| Kotlin | `dokka` / `kotlin-reflect` | kotlin-stdlib + kotlinx |
| Ruby | `ri --format=json` | ~100 modules |

### LLM 時代の言語（修正成功率・設計思想比較）

| 言語 | 登場 | 特徴 | Almide との比較ポイント |
|------|------|------|----------------------|
| **Mojo** | 2023 | Python スーパーセット、AI/ML 向け、コンパイル型 | 性能 vs 書きやすさのバランス、LLM が Python 知識を転用できる設計 |
| **Moonbit** | 2023 | WASM-first、core/x の 2 層 stdlib、AI 支援前提設計 | stdlib 設計が最も参考になる。Almide の 3 層設計の元ネタ |
| **Gleam** | 2024 (1.0) | 型安全、BEAM + JS マルチターゲット、シンプル構文 | マルチターゲット codegen、エラー設計、@extern パターンの参考元 |
| **Pkl** | 2024 | Apple 発、設定言語、型付き構造化データ | 設定ファイル用途での DSL 設計 |
| **Bend** | 2024 | 超並列関数型、GPU 自動並列化 | 並列計算モデル、関数型最適化 |
| **Roc** | 開発中 | 関数型、ランタイム例外ゼロ、platform 分離 | platform 分離、エラーなし設計、LLM 向けシンプルさ |

### LLM 修正成功率で直接比較すべき 3 言語

1. **Mojo** — Python 互換の知識転用 vs Almide の独自構文。LLM の既存知識量で Mojo が有利な可能性
2. **Moonbit** — WASM-first + AI 支援前提。stdlib 設計思想が最も近い。直接競合
3. **Gleam** — シンプル構文 + 強い型。マルチターゲットの先行事例。`@extern` パターンを Almide が参考にした

### 計測方法

Grammar Lab の A/B テスト基盤で、同じタスクを Almide / Mojo / Moonbit / Gleam で LLM に書かせ、修正成功率を比較する。

```
タスク例:
- FizzBuzz → JSON API サーバー（段階的に複雑化）
- CSV → JSON 変換ツール
- TODO アプリ（CRUD + テスト）

計測項目:
- 初回正答率（一発で正しく書けるか）
- 修正成功率（エラーから自力修復できるか）
- コード量（同じ機能を書くのに何行か）
- エラーメッセージの有用性（LLM がエラーから修正できるか）
```

### API 自動収集の対象

LLM 時代の言語も収集対象に含める：

| 言語 | ツール | 備考 |
|------|--------|------|
| Mojo | `mojo doc` (開発中) | Python の `inspect` でも部分的に取得可能 |
| Moonbit | `moon doc` | 公式ドキュメントツール |
| Gleam | `gleam docs` | HTML 生成、JSON 出力は未対応 |
| Roc | `roc docs` | 開発中 |

## 関連ロードマップ

- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) — Layer 分離の詳細設計
- [Package Registry](../on-hold/package-registry.md) — x/ パッケージの配布基盤
- [Rainbow FFI Gate](../on-hold/rainbow-gate.md) — 多言語 FFI（@extern の発展形）
- [Codec Protocol & JSON](active/codec-and-json.md) — JSON の次のフォーマット対応
