# CLI Specification

> Last updated: 2026-03-28

## Overview

```
almide <command> [options] [arguments]
```

プロジェクトルートに `almide.toml` + `src/main.almd` があれば、ファイル引数は省略可能。自動的に `src/main.almd` が使われる。

---

## Commands

### `almide run`

コンパイルして即実行。内部で Rust ソースを生成 → `cargo build` → バイナリ実行。

```bash
almide run                              # src/main.almd を実行
almide run app.almd                     # 指定ファイルを実行
almide run -- --flag value              # -- 以降はプログラムの引数
almide run app.almd -- arg1 arg2        # ファイル指定 + プログラム引数
```

| オプション | 説明 |
|---|---|
| `--no-check` | 型チェックをスキップ |

プログラム内で `env.args()` を呼ぶと `--` 以降の引数が `List[String]` で返る。

---

### `almide build`

コンパイルしてバイナリを生成。

```bash
almide build                            # src/main.almd → パッケージ名のバイナリ
almide build app.almd -o myapp          # 出力ファイル名指定
almide build app.almd --target wasm     # WASM バイナリ（直接 emit、rustc 不要）
almide build --release                  # 最適化ビルド (opt-level=2)
almide build --fast                     # 最大性能 (opt-level=3, LTO, native CPU)
```

| オプション | 説明 |
|---|---|
| `-o <name>` | 出力ファイル名 |
| `--target wasm` | WASM バイナリを生成（直接 emit） |
| `--target npm` | npm パッケージとして出力 |
| `--release` | 最適化ビルド |
| `--fast` | 最大性能（`--release` を含む + LTO + native CPU） |
| `--unchecked-index` | 配列の境界チェックを無効化（unsafe） |
| `--no-check` | 型チェックをスキップ |
| `--repr-c` | struct/enum に `#[repr(C)]` を付与（C ABI 互換） |

出力ファイル名のデフォルト:
- `almide.toml` があれば `[package] name`
- なければソースファイル名から `.almd` を除いた名前

---

### `almide test`

`.almd` ファイル内の `test "name" { ... }` ブロックを検出・実行。

```bash
almide test                             # カレントディレクトリ以下を再帰スキャン
almide test spec/lang/                  # ディレクトリ指定
almide test spec/lang/expr_test.almd    # ファイル指定
almide test --run "pattern"             # テスト名でフィルタ
almide test --target wasm               # WASM ターゲットでテスト
almide test --json                      # 結果を JSON (1行1テスト) で出力
```

| オプション | 説明 |
|---|---|
| `-r, --run <pattern>` | テスト名のパターンフィルタ |
| `--no-check` | 型チェックをスキップ |
| `--json` | JSON 形式で結果出力 |
| `--target wasm` | wasmtime で実行 |

テストの書き方:

```almide
test "addition" {
  assert_eq(1 + 2, 3)
}

test "string concat" {
  assert_eq("a" + "b", "ab")
}
```

- `test` ブロックは任意の `.almd` ファイルに書ける
- `*_test.almd` サフィックスは慣習（強制ではない）
- `test` ブロック内は暗黙の effect context（I/O 呼び出し可能）

---

### `almide check`

型チェックのみ実行。バイナリ生成なし。CI やエディタ統合用。

```bash
almide check                            # src/main.almd をチェック
almide check app.almd                   # 指定ファイルをチェック
almide check --deny-warnings            # 警告をエラーとして扱う
almide check --json                     # 診断を JSON で出力
almide check --explain E001             # エラーコードの説明
almide check --effects                  # 各関数のエフェクト分析を表示
```

| オプション | 説明 |
|---|---|
| `--deny-warnings` | 警告をエラー扱い |
| `--json` | 診断を JSON で出力（エディタ統合用） |
| `--explain <code>` | エラーコード (E001〜E010) の説明 |
| `--effects` | 各関数のエフェクト/ケイパビリティ分析 |

エラーコード:

| コード | 説明 |
|---|---|
| E001 | 型の不一致 |
| E002 | 未定義の関数 |
| E003 | 未定義の変数 |
| E004 | 引数の数が違う |
| E005 | 引数の型が違う |
| E006 | 純粋関数から effect 関数を呼んでいる |
| E007 | 純粋関数内の fan ブロック |
| E008 | fan 内での var キャプチャ |
| E009 | let/パラメータへの代入 |
| E010 | 非網羅的 match |

---

### `almide fmt`

ソースファイルのフォーマット。

```bash
almide fmt                              # src/**/*.almd を整形
almide fmt app.almd                     # 指定ファイルを整形
almide fmt --check                      # 差分があれば非ゼロで終了（CI 用）
almide fmt --dry-run                    # 書き込みせず差分表示
```

---

### `almide compile`

Module Interface を生成。外部ツール（binding generator 等）が型情報を読むための JSON / `.almdi` アーティファクト。

```bash
almide compile                          # プロジェクト全体
almide compile parser                   # モジュール名指定
almide compile app.almd --json          # JSON 出力（stdout）
almide compile --dry-run                # 人間向け表示
almide compile -o target/compile        # 出力ディレクトリ指定
```

JSON 出力の構造:

```json
{
  "module": "mathlib",
  "types": [{
    "name": "Point",
    "kind": { "kind": "record", "fields": [{"name": "x", "type": {"kind": "float"}}] },
    "abi": { "size": 16, "align": 8, "fields": [{"name": "x", "offset": 0, "size": 8}] }
  }],
  "functions": [{
    "name": "distance",
    "params": [{"name": "a", "type": {"kind": "named", "name": "Point"}}],
    "return": {"kind": "float"},
    "effect": false
  }],
  "constants": [],
  "dependencies": []
}
```

`abi` フィールドは具象型（ジェネリックでない）にのみ付与。C ABI のレイアウト（size, align, field offset）。

---

### `almide init`

新しいプロジェクトを作成。

```bash
almide init
```

生成物:

```
almide.toml
src/
  main.almd
```

---

### `almide add`

依存パッケージを追加。`almide.toml` の `[dependencies]` に書き込み、即フェッチ。

```bash
almide add bindgen                      # github.com/almide/bindgen
almide add almide/almide-bindgen        # github.com/almide/almide-bindgen
almide add user/repo@v0.1.0             # バージョン指定
almide add --git https://example.com/repo.git --tag v1.0 mylib
```

短縮記法:
- `almide add name` → `https://github.com/almide/{name}`
- `almide add user/repo` → `https://github.com/{user}/{repo}`
- `@v0.1.0` → `tag = "v0.1.0"`

---

### `almide deps`

依存パッケージの一覧を表示。

```bash
almide deps
# bindgen = https://github.com/almide/almide-bindgen (v0.1.0)
# json = https://github.com/almide/json (main)
```

---

### `almide dep-path`

依存パッケージのローカルキャッシュディレクトリを出力。

```bash
almide dep-path bindgen
# /Users/you/.almide/cache/bindgen/a629eded8d20/src
```

用途: 依存パッケージの `.almd` ファイルを `process.exec("almide", ["run", path])` で実行する場合のパス取得。

---

### `almide clean`

依存キャッシュ (`~/.almide/cache/`) をクリア。

```bash
almide clean
```

---

## Legacy Mode

ファイル名が `.almd` で終わる引数を最初に指定すると、`emit` コマンドとして扱われる:

```bash
almide app.almd --target rust           # → almide emit app.almd --target rust
almide app.almd --target rust --repr-c  # #[repr(C)] 付き Rust 出力
almide app.almd --emit-ast              # AST を JSON で出力
almide app.almd --emit-ir               # 型付き IR を JSON で出力
```

---

## Exit Codes

| コード | 意味 |
|---|---|
| 0 | 成功 |
| 1 | コンパイルエラー、テスト失敗、依存解決失敗 |

---

## 環境変数

| 変数 | 説明 |
|---|---|
| `ALMIDE_DEBUG_TYPEVARS` | `1` にすると未解決 TypeVar の詳細を出力 |
