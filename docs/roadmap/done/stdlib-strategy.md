<!-- description: Stdlib expansion strategy via Rust ecosystem wrapping -->
<!-- done: 2026-03-18 -->
# Stdlib Strategy

Proliferation requires a stdlib thick enough to "write what you want immediately." Currently 15 modules, 266 functions. Compared to major languages:

| Language | stdlib Module Count | Function/Method Count |
|------|-------------------|----------------|
| Go | ~150 packages | Thousands |
| Python | ~200 modules | Tens of thousands |
| Rust (std) | ~50 modules | Thousands |
| Deno (std) | ~30 modules | Hundreds |
| **Almide** | **15 + 6 bundled** | **~282** |

Leveraging the fact that Almide compiles to Rust, **wrapping the Rust ecosystem** is the fastest expansion strategy.

---

## Current State (v0.5.13)

### Layer 1: core (all targets, WASM OK)
| Module | Functions | Status |
|--------|-----------|--------|
| string | 36 | ✅ Comprehensive |
| list | 54 | ✅ Comprehensive |
| int | 21 | ✅ Sufficient |
| float | 16 | ✅ Sufficient |
| map | 16 | △ Basic only |
| math | 21 | ✅ Sufficient |
| json | 36 | ✅ Comprehensive |
| regex | 8 | △ Basic only |
| result | 9 | ✅ Complete |

### Layer 2: platform (native only)
| Module | Functions | Status |
|--------|-----------|--------|
| fs | 19 | △ Basic only |
| process | 6 | △ Minimal |
| io | 3 | △ Minimal |
| env | 9 | △ Basic only |
| http | 8 | △ Basic only |
| random | 4 | △ Minimal |

### Bundled .almd
| Module | Status |
|--------|--------|
| path | ✅ Sufficient |
| time | △ Basic only |
| hash | △ SHA/MD5 only |
| encoding | △ base64/hex only |
| args | △ Basic only |
| term | △ Basic only |

---

## Expansion Strategy

### Strategy 1: Add via TOML + Runtime (current approach)

Cost of adding a new function:
1. Add definition to `stdlib/defs/<module>.toml`
2. Add Rust implementation to `src/emit_rust/<xxx>_runtime.txt`
3. Add TS implementation to `src/emit_ts_runtime.rs`
4. Auto-generated via `cargo build`

**Pros:** No compiler core changes needed, type-safe, TOML definitions readable by LLMs
**Cons:** Implementation needed for 2 targets, manual translation required to wrap Rust crate features

**Applicable to:** Core layer function additions (expanding string, list, map, math)

### Strategy 2: Wrap Rust crates with @extern

Call Rust crate functions directly with `@extern(rs, "crate", "function")`.

```almide
@extern(rs, "chrono", "Utc::now().to_rfc3339")
@extern(ts, "Date", "new Date().toISOString")
fn now_iso() -> String
```

**Pros:** Access to full Rust ecosystem features, minimal implementation cost
**Cons:** TS side also needs separate implementation, type mapping is trust-based (safety is user's responsibility)

**Applicable to:** New platform layer modules (datetime, crypto, database, etc.)

### Strategy 3: Self-host in Almide

Write pure computation logic in Almide itself.

**Pros:** Automatic support for both targets, tests writable in Almide
**Cons:** Performance depends on Almide's generated code quality

**Applicable to:** csv, toml parsers, data conversion, validation

### Strategy 4: Official extension packages (x/)

Versioned independently from stdlib. Add dependencies in `almide.toml`.

**Pros:** Free from stdlib version lock, easier for community contributions
**Cons:** Requires package registry (currently on-hold)

**Applicable to:** Large features (web frameworks, ORM, template engines)

---

## Missing Modules (by priority)

### Tier 1: Cannot write practical programs without these

| Module | Content | Strategy | Estimate |
|--------|---------|----------|----------|
| **datetime** | Date parsing/formatting/timezone/comparison | TOML + runtime (Rust: chrono, TS: Intl) | 20-30 functions |
| **fs (expansion)** | Directory traversal, recursive delete, permissions, temp, watch | TOML + runtime | +15 functions |
| **http (expansion)** | Header manipulation, status codes, cookie, multipart | TOML + runtime | +20 functions |
| **error** | Structured error types, stack trace, chain | TOML + runtime | 10 functions |

### Tier 2: Needed by many applications

| Module | Content | Strategy | Estimate |
|--------|---------|----------|----------|
| **csv** | parse/stringify, with headers, streaming | self-host (.almd) | 8-10 functions |
| **toml** | parse/stringify | self-host (.almd) | 6-8 functions |
| **yaml** | parse/stringify | @extern (serde_yaml / js-yaml) | 4-6 functions |
| **url** | Parse/build/encode/query parameters | self-host (.almd) | 10 functions |
| **crypto** | HMAC, AES, RSA, random bytes | @extern (ring / Web Crypto) | 10-15 functions |
| **uuid** | v4 generation, parse, format | @extern (uuid / crypto.randomUUID) | 4 functions |

### Tier 3: Needed for ecosystem growth

| Module | Content | Strategy | Estimate |
|--------|---------|----------|----------|
| **sql** | Parameterized queries, SQLite/PostgreSQL | @extern + x/ package | 15 functions |
| **websocket** | client/server, message send/receive | @extern | 8 functions |
| **log** | Structured logging, levels, formatters | TOML + runtime | 6 functions |
| **test** | Mock, spy, benchmark | TOML + runtime | 10 functions |
| **compress** | gzip/zstd compression/decompression | @extern | 4 functions |
| **image** | Basic image manipulation | x/ package | 15 functions |

---

## Numerical Targets

| Milestone | Modules | Functions | Baseline |
|-----------|---------|-----------|----------|
| Current (v0.5.13) | 21 | ~282 | -- |
| **v0.6 (minimum practical)** | 25 | 400+ | Tier 1 complete |
| **v0.8 (app development ready)** | 32 | 550+ | Tier 2 complete |
| **v1.0 (production ready)** | 38+ | 700+ | Key Tier 3 modules |

Comparison: Go 1.0 shipped with ~100 packages. Deno 1.0 with ~30 modules. Since Almide compiles to Rust, using Rust crates via @extern provides access to more capabilities than the module count suggests.

---

## LLM Suitability Perspective

### Consistent naming conventions

```
module.verb_noun(args)     — basic form
module.verb_noun?(args)    — returns Bool
module.try_verb(args)      — returns Result
```

Enforce this pattern across all modules. LLMs learn consistent patterns more easily.

### Natural calling with UFCS

```almide
// Both forms work
string.trim(s)
s.trim()

// LLM は UFCS を好む傾向がある（メソッドチェーンが読みやすい）
text
  |> string.trim()
  |> string.split(",")
  |> list.map(fn(x) => string.trim(x))
```

### Machine-readability of stdlib documentation

Add a `description` field to TOML definitions so that future `almide doc` is also usable for LLMs:

```toml
[trim]
description = "Remove leading and trailing whitespace from a string"
params = [{ name = "s", type = "String" }]
return = "String"
```

This enables automatic injection of stdlib reference into LLM prompts.

---

## Implementation Order

```
1. Tier 1 (datetime, fs expansion, http expansion, error)  ← v0.6 target
   ↓
2. Add description field to TOML                           ← LLM suitability
   ↓
3. Tier 2 (csv, toml, url, crypto, uuid)                   ← v0.8 target
   ↓
4. Strengthen @extern type safety                           ← Phase 0 prerequisite
   ↓
5. Tier 3 (sql, websocket, log, test)                      ← v1.0 target
   ↓
6. x/ package separation                                    ← package registry prerequisite
```

## Auto-Collection Tool (Built in Almide)

**Implement in Almide itself** a tool that automatically collects API references from other languages for stdlib design. Three birds with one stone: dogfooding + practical tool + stdlib completeness benchmark.

### Concept

```almide
// Get stdlib/lib info in a unified format per language
effect fn main() =
  let go_time = fetch_module("go", "time")
  let py_datetime = fetch_module("python", "datetime")
  let report = compare([go_time, py_datetime])
  fs.write_text("docs/roadmap/stdlib/auto/time.md", render_markdown(report))
```

### Unified Output Format

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

### Data Sources

**Recommended: Via each language's reflection/documentation tools (no scraping needed)**

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

### Auto-Collection Targets

Include LLM-era languages in the collection targets:

| Language | Tool | Notes |
|----------|------|-------|
| Mojo | `mojo doc` (in development) | Partially available via Python's `inspect` |
| Moonbit | `moon doc` | Official documentation tool |
| Gleam | `gleam docs` | HTML generation, JSON output not yet supported |
| Roc | `roc docs` | In development |

## Related Roadmap

- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) — Detailed layer separation design
- [Package Registry](../on-hold/package-registry.md) — Distribution infrastructure for x/ packages
- [Rainbow FFI Gate](../on-hold/rainbow-gate.md) — Multi-language FFI (evolution of @extern)
- [Codec Protocol & JSON](active/codec-and-json.md) — Next format support beyond JSON
