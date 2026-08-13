# CLI Specification

> Last updated: 2026-08-14

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
almide run app.almd                     # 指定ファイルを実行 (native)
almide run app.almd --target wasm       # wasm をビルドして wasmtime で実行
almide run -- --flag value              # -- 以降はプログラムの引数
almide run app.almd -- arg1 arg2        # ファイル指定 + プログラム引数
```

| オプション | 説明 |
|---|---|
| `--target rust\|wasm` | 実行ターゲット。`rust`（既定、ネイティブバイナリ）または `wasm`（v1 trust-spine が直接 WASM を生成し `wasmtime` CLI で実行。rustc 不要）。両ターゲットは同一の観測可能挙動（stdout/stderr/exit）を出す（クロスターゲット等価性保証）。`wasm` には PATH に `wasmtime` が必要 |
| `--no-check` | 型チェックをスキップ |
| `--release` | 最適化ビルド (cargo --release) |

`almide` 自身のフラグ（`--target` / `--no-check` / `--release`）は `--` の前で解釈され、`--` 以降はそのままプログラムに渡る（`cargo run` と同じ規約）。プログラム内で `env.args()` を呼ぶと `--` 以降の引数が `List[String]` で返る。

テスト: `tests/run_target_flag_test.rs`

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
almide test --json                      # 結果を JSONL (1行1ファイル) で出力
```

| オプション | 説明 |
|---|---|
| `-r, --run <pattern>` | テスト名のパターンフィルタ |
| `--no-check` | 型チェックをスキップ |
| `--json` | JSON 形式で結果出力 |
| `--target wasm` | wasmtime で実行 |

失敗の報告は**構造化ブロック**（`FAILED: <file>` に続けて `test:` / `at:` /
`hint:` / `diff:` または `expected:` `found:`）。複数行文字列・リスト・レコードは
単位ごとの実差分になる。ファイル内の失敗は**ソース行順**に並ぶので、2回の実行を
そのまま diff できる。

`--json` は1ファイル1行の JSONL:

```json
{"file":"spec/lang/x_test.almd","status":"fail","exit_code":101,
 "failures":[{"name":"string mismatch","file":"spec/lang/x_test.almd","line":4,
              "op":"assert_eq","expected":"\"a\\nB\"","found":"\"a\\nb\"",
              "diff":"      a\n    - B\n    + b\n","message":"…"}]}
```

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
almide check --timings                  # フロントエンドの phase 別内訳
```

| オプション | 説明 |
|---|---|
| `--deny-warnings` | 警告をエラー扱い |
| `--json` | 診断を JSON で出力（1 行 1 診断、エディタ/エージェント統合用） |
| `--explain <code>` | エラーコード (E001〜E030, E420) の説明 |
| `--effects` | 各関数のエフェクト/ケイパビリティ分析 |
| `--timings` | lex / parse / check の phase 別 wall time（#1311） |

#### `--timings`

フロントエンドの時間を lex / parse / check に分解して stderr に 2 行出す。
1 行目は人間向け、2 行目は機械可読（キー名は API — `scripts/check-edit-loop-scale.sh`
の per-phase ratchet が読む）:

```
$ almide check --timings spec/lang/expr_test.almd
timings: lex 1.8ms (16.9%, 2443k lines/s) parse 1.6ms (15.3%, 2692k lines/s) check 3.9ms (35.9%, 1149k lines/s) | other 3.4ms | total 10.7ms over 4426 lines in 43 sources
almide-timings {"lex_ns":1812250,"parse_ns":1644211,"check_ns":3851626,"total_ns":10717583,"lines":4426,"bytes":109057,"sources":43}
```

- 計上は `--timings` を付けた時だけ有効。付けない実行は clock を一切読まない
  （計測が計測対象を動かさないため — 実測 A/B は `research/benchmark/editloop/scale.py` の冒頭）。
- `lines` / `sources` は **実際に lex したもの全部**。エントリと自プロジェクトの
  モジュールに加え、どのチェックも必ず払う auto-import 済み bundled stdlib を含む。
  lines/sec の分母はこれ。
- `other` は 3 phase 以外の残り（ファイル I/O、import 解決、canonicalize、
  unused 警告用の lowering）。残余に名前を与えないと任意の回帰を吸ってしまう。
- エラーで終了した check では出さない。時間が診断レンダラに行っているため。

テスト: `tests/check_timings_test.rs`（キー一式、各 phase 非ゼロ、二重計上なし、
`--timings` なしでは無出力）。ratchet 側は `scripts/check-edit-loop-scale.sh`。

`--json` の 1 行は
`{level, code, message, hint, here, try, try_replace, context, file, line, col, end_col, secondary}`。
`try` は貼り付け可能な修正スニペット、`try_replace` はそれが置換するスパン。

**構文エラーも JSON で出る**: トップレベル宣言が 1 つも成立しないファイルは
`Parser::parse` が `Err` を返し、共有の `parse_file` は人間向けテキストを stderr に出して
終了する。`--json` はパーサから診断を直接取り出してこの経路でも JSON を出す
（LLM が最も多く出すエラー種別が唯一テキストのままだった）。終了コードは従来通り
`1`（パースできなかったファイル）— 既存ゲートが黙って成功に反転しないため。
型エラーのみの場合は従来通り終了コード `0` で、判定は JSON の `level` を読む。

テスト: `tests/mcp_test.rs`（`json_check_reports_a_total_parse_failure_as_json`）

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

E001〜E030 + E420 の全コード解説は [../diagnostics/](../diagnostics/) を参照（上表は代表例）。

---

### `almide fmt`

ソースファイルのフォーマット。

```bash
almide fmt                              # src/**/*.almd を整形
almide fmt app.almd                     # 指定ファイルを整形
almide fmt --check                      # 差分があれば非ゼロで終了（CI 用）
almide fmt --check --json               # 同じゲートを JSON 1 オブジェクトで（機械可読）
almide fmt --dry-run                    # 書き込みせず差分表示
almide fmt --no-import-edit stdlib/     # import 行を一切触らず整形(splice-context ソース用)
```

| オプション | 説明 |
|---|---|
| `--check` | 比較のみ。未整形ファイルを stderr に列挙し、あれば終了コード 1 |
| `--json` | `--check` の機械可読版（`--check` を含意）。stdout に 1 オブジェクト、終了コードは同じ |
| `--dry-run` | 整形結果を stdout に出すだけ。書き込まない |
| `--no-import-edit` | import 行を一切編集しない |

`--json` の出力:

```json
{"checked":2,"unformatted":["src/a.almd"],"unreadable":[],"verify_failed":false,"ok":false}
```

`--json` も `--check` も書き込みは一切しない。整形の適用は `almide fmt <path>`。

テスト: `tests/mcp_test.rs`（`fmt_check_json_reports_drift_and_keeps_the_gate_exit_code`）

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

### `almide mcp`

Model Context Protocol サーバを stdio で起動する。エージェント（Claude Code 等）が
コンパイラを **型付きツール呼び出し** として使うための口。人間向け出力を
モデルに読み解かせる工程を挟まないことが目的で、この工程こそが精度の漏れ口。

```bash
almide mcp                              # stdio で JSON-RPC 2.0（改行区切り）を待つ
```

対応メソッド: `initialize` / `tools/list` / `tools/call` / `ping`。
それ以外は JSON-RPC `-32601`（`capabilities` は `tools` のみを宣言する）。

ツール（5 つ。すべて CLI の既存の機械可読出力を経由する）:

| ツール | 実体 | 返すもの |
|---|---|---|
| `almide_check` | `almide check --json` | 構造化診断の配列（`try`/`try_replace` 込み） |
| `almide_test` | `almide test --json` | ファイル単位の pass/fail + ランナー生出力 |
| `almide_api` | `almide ide outline --json` / `stdlib-snapshot --json` | 公開宣言のシグネチャ一覧 |
| `almide_explain` | `almide explain <CODE>` | 診断コードの解説（markdown） |
| `almide_fmt_check` | `almide fmt --check --json` | 未整形ファイル一覧（書き込みなし） |

設計上の制約:

- **コンパイラへの入口は 1 本**。各ツールは `current_exe()`（= 自分自身）を
  サブプロセスとして起動し、CLI が既に出している JSON を読む。MCP 専用の
  出力経路を作らない（作れば必ず CLI とドリフトし、しかも誰も目で読まないので
  ドリフトが見えない）
- **人間向けテキストは決してパースしない**。機械可読な形が無い箇所
  （テストの個別失敗詳細 = #1313）は `*_unstructured` という名前のフィールドに
  そのまま入れて返す
- **書き込みツールは無い**。`fmt` は `--check` 形のみ。適用は CLI（`almide fmt` /
  `almide fix`）で行う — エージェント自身のトランスクリプトに編集が残る

Claude Code プラグイン定義（MCP + LSP）: `tools/claude-plugin/`、
マーケットプレイス: `.claude-plugin/marketplace.json`、導入手順: [../mcp.md](../mcp.md)

テスト: `tests/mcp_test.rs`

---

### `almide init`

新しいプロジェクトを作成。

```bash
almide init
```

生成���:

```
almide.toml               [package] name, version, edition
src/
  main.almd               effect fn main テンプレート
tests/                    テスト用ディレクトリ
CLAUDE.md                 AI 向けプロジェクト説明
```

`name` はカレントディレクトリ名から自動生成。

---

### `almide update`

ロック済み git 依存を、その ref の**現在の remote head** へ前進させる(#1131)。

```bash
almide update almai      # 指定の依存だけ
almide update            # tag 固定でない全依存
```

- `almide.lock` は意図的に sticky(`fetch_all_deps` が pin を再利用して再現性を守り、
  既存依存への `almide add` も同じ pin を書き戻す)。前進の唯一の正規手段が本コマンド。
- **tag 固定の依存は動かさない**(マニフェストの要求そのものが変わるため。skip を報告)。
- 変更した entry だけを書き換え、他の pin はバイト単位で不変。
- 前進時は当該 ref のキャッシュディレクトリを破棄し、次のフェッチで再取得させる。
- `git ls-remote` 1 回で解決(clone しない)。

出力は `name <old12> -> <new12>`(初回ロックは `name -> <new12>`)。

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

## Global Options

| オプション | 説明 |
|---|---|
| `-h, --help` | ヘルプ表示（各コマンドにも付く） |
| `-V, --version` | バージョン表示（例: `almide 0.34.2`） |

---

## 環境変数

| 変数 | 説明 |
|---|---|
| `ALMIDE_DEBUG_TYPEVARS` | `1` にすると未解決 TypeVar の詳細を出力 |
