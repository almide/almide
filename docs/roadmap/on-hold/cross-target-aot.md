# Cross-Target AOT Compilation [PLANNED]

## Motivation

Almide は既に複数の emit ターゲット（Rust / TS / JS / 将来 WASM）を持ち、`@extern` でターゲット別の実装を宣言的に切り替えられる。この構造を活かし、`almide build` 一発で複数ターゲットの成果物を同時に生成する AOT クロスコンパイルシステムを構築する。

## 現状の土台

Almide のコンパイルパイプラインは既にクロスターゲット対応の構造:

```
Source (.almd)
  → Lexer → Parser → AST
  → Checker → IR (共通)
  → emit_rust/  (native CLI / cargo crate)
  → emit_ts/    (TS/JS / npm パッケージ)
  → emit_wasm/  (将来: WASM 直接出力 + JS グルー)
```

- チェッカー・IR は全ターゲット共通
- `@extern(rs, ...)` / `@extern(ts, ...)` でターゲット別の実装を言語レベルでサポート
- ターゲット固有のコードは emit レイヤーに閉じている

## ゴール

```bash
almide build --target all
```

```
dist/
├── native/     Rust → バイナリ (macOS / Linux / Windows)
├── web/        WASM + JS グルー (ブラウザ向け)
├── npm/        JS パッケージ (npm publish 可能)
└── deno/       TS モジュール (Deno / Bun 向け)
```

## 他言語との比較

| 言語/フレームワーク | クロスターゲット戦略 | Almide との違い |
|---|---|---|
| **Kotlin Multiplatform** | `commonMain` → JVM / iOS / JS / WASM | `expect`/`actual` でターゲット分岐。Almide の `@extern` と同じ設計思想 |
| **Rust** | `#[cfg(target)]` + cross-compile | 条件コンパイルは強力だが、JS/TS ターゲットはない |
| **Go** | `GOOS`/`GOARCH` でクロスコンパイル | ネイティブのみ。Web ターゲットは TinyGo 経由 |
| **Dart (Flutter)** | AOT (iOS/Android) + JIT (dev) | プラットフォームチャネルで分岐。言語レベルではない |
| **Zig** | `comptime` + ターゲット判定 | ゼロコスト抽象化。ただし Web ターゲットは限定的 |

**Almide の優位性**: `@extern` が言語の第一級機能であるため、ターゲット分岐がアドホックな条件分岐ではなく、型安全かつ宣言的。さらに、ネイティブ (Rust) と Web (TS/JS/WASM) の両方を一級ターゲットとして持つ言語は稀。

## Implementation Phases

### Phase 1: `almide build --target` の拡張

- [ ] `--target all` で全ターゲットを順次ビルド
- [ ] `--target native,npm` のようなカンマ区切り複数指定
- [ ] `almide.toml` の `[build]` セクションでデフォルトターゲットを設定

```toml
[build]
targets = ["native", "npm"]
```

### Phase 2: ターゲット別最適化

各ターゲットに特化した最適化パスを IR → emit 間に挿入:

| ターゲット | 最適化 |
|-----------|--------|
| Rust (native) | borrow analysis、ライフタイム推論、ゼロコピー最適化 |
| TS/JS (web) | tree shaking、minification-friendly な命名 |
| WASM (direct) | サイズ最適化、線形メモリレイアウト最適化 |
| npm (package) | 使用 stdlib のみバンドル、ESM/CJS デュアル出力 |

### Phase 3: 成果物パッケージング

- [ ] `dist/` ディレクトリへの統一出力
- [ ] npm: `package.json` 自動生成（既に `emit_npm_package` で部分実装済み）
- [ ] WASM: `.wasm` + `.js` グルーの同梱（emit-wasm-direct.md 参照）
- [ ] native: ターゲットトリプル別バイナリ（`x86_64-apple-darwin` 等）

### Phase 4: CI/CD 統合

```yaml
# GitHub Actions で全ターゲットビルド
- run: almide build --target all
- uses: actions/upload-artifact@v4
  with:
    path: dist/
```

- [ ] `almide publish` コマンドで npm + crates.io + GitHub Release を一括公開
- [ ] GitHub Actions テンプレート提供

## @extern とクロスターゲットの関係

`@extern` はこのシステムの中核。ターゲット分岐を型安全に宣言する:

```almide
// ネイティブは OS API、Web は fetch API を使う
@extern(rs, "reqwest", "get")
@extern(ts, "fetch_wrapper", "get")
fn http_get(url: String) -> Result[String, String]

// 共通ロジックはそのまま全ターゲットで使える
fn parse_response(body: String) -> Data =
  json.parse(body) |> unwrap_or(default_data())
```

クロスターゲットの難しさは「どこでターゲット分岐するか」だが、Almide では `@extern` 境界が明確なので、共通コードとターゲット固有コードの切り分けがコンパイラレベルで保証される。

## 依存関係

- emit-wasm-direct.md: WASM ターゲットが Phase 3 以降の前提
- package-registry.md: `almide publish` は registry が必要
- stdlib-architecture-3-layer-design.md: platform 分離がクロスターゲットの安全性を担保
