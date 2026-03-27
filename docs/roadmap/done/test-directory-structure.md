<!-- description: Reorganize tests into spec/ (lang/stdlib/integration) and tests/ -->
# Test Directory Structure Redesign

テスト関連がルートに散らばっていた（`lang/`, `stdlib/`, `exercises/`, `tests/`）問題を解決。

## Final Structure

```
spec/                  ← Almide 言語テスト（almide test spec/）
├── lang/              言語機能テスト（式、変数、関数、パターン、型、スコープ、エラー処理）
├── stdlib/            stdlibモジュールテスト（string, list, map, int, float, math, json, regex, ...）
└── integration/       マルチファイル・システム統合テスト（generics, modules, extern）

tests/                 ← Rust compiler テスト（cargo test, Cargo 自動検出）
├── lexer_test.rs
├── parser_test.rs
├── checker_test.rs
└── ...

stdlib/                ← ソースのみ（テスト混在解消）
├── defs/*.toml
├── args.almd
└── (テストなし)
```

## Why `spec/` + `tests/`

- `tests/` は Cargo の慣習。auto-discovery が使え、`[[test]]` 個別指定が不要
- `spec/` は `tests/` と明確に区別できる命名。`test/` と `tests/` が並ぶ混乱を回避
- ルートで「テストどこ？」→ Rust なら `tests/`、Almide なら `spec/`

## コマンド対応

```bash
almide test                    # 全 .almd テスト（再帰検索）
almide test spec/lang/         # 言語テスト
almide test spec/stdlib/       # stdlib テスト
almide test spec/integration/  # 統合テスト
cargo test                     # Rust compiler テスト
```

## Migration Log

| Step | What | Status |
|------|------|--------|
| 1 | `lang/*_test.almd` → `spec/lang/` | done |
| 2 | `stdlib/*_test.almd` → `spec/stdlib/` | done |
| 3 | `exercises/{generics,mod,extern}-test/` → `spec/integration/` | done |
| 4 | Rust tests: `tests/` に残す（Cargo auto-discovery） | done |
| 5 | `Cargo.toml` から `autotests = false` + `[[test]]` 削除 | done |
| 6 | `CLAUDE.md` テストパス更新 | done |
| 7 | 空ディレクトリ削除（`lang/`, `test/`） | done |
