# Test Directory Structure Redesign [ACTIVE]

テスト関連がルートに散らばっている（`lang/`, `stdlib/`, `exercises/`, `tests/`）。Gleam式に **ルートは `test/` 一本** にまとめ、その中で Rust compiler テストと .almd 言語テストを明確に分離する。

## Current → Proposed

```
BEFORE (5 dirs at root)          AFTER (1 dir at root)
─────────────────────            ────────────────────
lang/*_test.almd                 test/
stdlib/*_test.almd + source混在     ├── almide/            ← almide test test/almide/
exercises/generics-test/...         │   ├── lang/           ← lang/ から移動
tests/*.rs                          │   │   ├── expr_test.almd
                                    │   │   ├── variable_test.almd
                                    │   │   └── ...
                                    │   ├── stdlib/         ← stdlib/*_test.almd から移動
                                    │   │   ├── string_test.almd
                                    │   │   ├── list_test.almd
                                    │   │   ├── map_test.almd
                                    │   │   └── ...
                                    │   └── integration/    ← exercises/ の compiler tests から移動
                                    │       ├── generics/
                                    │       ├── modules/
                                    │       └── extern/
                                    └── rust/              ← cargo test (tests/ から移動)
                                        ├── lexer_test.rs
                                        ├── parser_test.rs
                                        ├── checker_test.rs
                                        └── ...

stdlib/                          stdlib/                ← ソースのみ残る（テスト混在解消）
  ├── defs/*.toml                  ├── defs/*.toml
  ├── args.almd (source)           ├── args.almd
  └── string_test.almd (test)      └── (テストなし)

exercises/                       exercises/             ← Exercism練習問題のみ
  ├── bob/                         ├── bob/
  ├── generics-test/ (compiler)    └── ... (compiler tests は移動済)
  └── ...
```

## Why This Structure

### ルート `test/` 一本

Gleam, Zig, Swift と同じパターン。ルートがすっきりし、「テストどこ？」→「`test/`」で終わる。

### `test/almide/` vs `test/rust/` の分離

| | `test/almide/` | `test/rust/` |
|--|---|---|
| 言語 | .almd | .rs |
| 実行方法 | `almide test test/almide/` | `cargo test` |
| テスト対象 | 言語仕様・stdlib・ランタイム | コンパイラ内部（lexer, parser, checker, IR, codegen） |
| 名前のルール | `test "name" { }` ブロック | `#[test] fn name()` |

名前だけで何が何のテストかすぐわかる。

### `test/almide/` のサブディレクトリ

```
test/almide/
├── lang/          言語機能テスト（式、変数、関数、パターン、型、スコープ、エラー処理）
├── stdlib/        stdlibモジュールテスト（string, list, map, int, float, math, json, regex, ...）
└── integration/   マルチファイル・システム統合テスト（generics, modules, extern）
```

- **lang/** — 「Almideの文法はこう動く」を保証
- **stdlib/** — 「203個の標準関数はこう動く」を保証、モジュール単位でファイル分割
- **integration/** — 「複数ファイルのimport、generics、externは動く」を保証

## Cargo制約の解決

Cargoはデフォルトで `tests/` を integration test として自動検出する。`test/rust/` に移動するには `Cargo.toml` に明示指定が必要：

```toml
[[test]]
name = "lexer_test"
path = "test/rust/lexer_test.rs"

[[test]]
name = "parser_test"
path = "test/rust/parser_test.rs"

# ... 各ファイル
```

あるいは `autobenches = false` のようにデフォルト検出を無効化：

```toml
[package]
autobins = true
autoexamples = true
autotests = false   # tests/ の自動検出を無効化
```

→ `autotests = false` + `[[test]]` エントリで `test/rust/*.rs` を指定するのがクリーン。

## 追加の整理

### `main()`ベースのテストファイルを変換

14ファイルが `fn main()` + `println()` でテストしており `almide test` に見えない。`test` ブロック化する：

対象: `args_test`, `csv_test`, `encoding_test`, `error_type_test`, `fs_process_test`, `fs_walk_stat_test`, `hash_test`, `heredoc_test`, `io_test`, `path_test`, `range_test`, `term_test`, `time_format_test`, `ufcs_test`

### モノリシックテストファイルの分割

`stdlib-test.almd`, `stdlib_v2_test.almd`, `stdlib_phase5_test.almd`, `stdlib_phase6_test.almd` → モジュール別ファイルに分配して削除

## Migration Plan

### Phase 1: ディレクトリ作成 + ファイル移動

```bash
# 構造作成
mkdir -p test/almide/lang test/almide/stdlib test/almide/integration
mkdir -p test/rust

# lang/ テスト移動
mv lang/*_test.almd test/almide/lang/

# stdlib/ テスト移動（ソースは残す）
mv stdlib/*_test.almd test/almide/stdlib/
mv stdlib/*-test.almd test/almide/stdlib/

# Rust テスト移動
mv tests/*.rs test/rust/

# integration テスト移動
mv exercises/generics-test test/almide/integration/generics
mv exercises/mod-test test/almide/integration/modules
mv exercises/extern-test test/almide/integration/extern
```

### Phase 2: Cargo.toml 更新

```toml
[package]
autotests = false

[[test]]
name = "lexer_test"
path = "test/rust/lexer_test.rs"
# ... 9ファイル分
```

### Phase 3: main()テストの変換 + モノリシックファイル分割

### Phase 4: ドキュメント更新

- `CLAUDE.md` のテストパスを更新
- `stdlib/README.md` 更新
- roadmap のパス参照更新

### Phase 5: 空ディレクトリ削除

```bash
rmdir lang/        # テスト移動後、空なら削除
rmdir tests/       # Rust テスト移動後、空なら削除
```

## コマンド対応

```bash
# 移行前                    # 移行後
almide test                 almide test test/almide/      # 全 .almd テスト
almide test lang/           almide test test/almide/lang/
almide test stdlib/         almide test test/almide/stdlib/
cargo test                  cargo test                   # 変わらず（Cargo.toml で path 指定済）
```

## Reference

| Language | Structure | Note |
|----------|----------|------|
| **Gleam** | `test/` mirroring `src/` | ルート `test/` 一本 |
| **Zig** | `test/` with subdirs | カテゴリ別サブディレクトリ |
| **Swift** | `Tests/ModuleTests/` | モジュール別 |
| **Rust** | `tests/` (Cargo慣習) | `autotests = false` でカスタマイズ可 |

Almide: **Gleam + Zig ハイブリッド** — ルート `test/` 一本、`almide/`と`rust/`で言語分離、サブディレクトリでカテゴリ分け。
