# Almide Roadmap

## モジュールシステム設計 (決定済み)

### プロジェクト構造

```
myapp/
  almide.toml
  src/
    main.almd              # エントリポイント
    utils.almd             # import self.utils
    http/
      mod.almd             # import self.http
      client.almd          # import self.http.client
      server.almd          # import self.http.server
  tests/
    utils_test.almd
```

### almide.toml

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
json = { git = "https://github.com/almide/json", tag = "v1.0.0" }
```

### import 構文

```almide
// ローカルモジュール — self で始まる
import self.utils              // utils.add(1, 2)
import self.http.client        // client.get(url)
import self.http.client as c   // c.get(url)

// stdlib — コンパイラ組み込み
import string                  // string.trim(s) or "s".trim()
import list                    // list.map(xs, fn(x) => ...)

// 外部依存 — almide.toml から解決
import json                    // json.parse(text)
import csv.reader              // reader.read(file)
```

- `self.xxx` = ローカルモジュール、それ以外 = stdlib or 外部依存
- ローカルと外部が構文で区別されるため名前衝突がない
- 呼び出しは末尾セグメント: `import self.http.client` → `client.get(url)`
- `as` でエイリアス可能

### モジュール解決の優先順位

```
1. stdlib      → string, list, int, float, fs, env 等 (コンパイラ組み込み)
2. ローカル    → self.xxx → src/ 配下のファイル
3. 依存        → almide.toml の [dependencies]
```

### モジュールファイルの対応

| import文 | 探す場所 |
|---|---|
| `import self.utils` | `src/utils.almd` |
| `import self.http` | `src/http/mod.almd` |
| `import self.http.client` | `src/http/client.almd` |
| `import json` | 依存: almide.toml → `~/.almide/cache/` |
| `import json.parser` | 依存パッケージ内の `parser.almd` |

### module 宣言

ファイルパスと一致必須:

```almide
// src/utils.almd
module utils

fn add(a: Int, b: Int) -> Int = a + b
```

不一致ならエラー:
```
error: module declaration 'foo' doesn't match file path 'src/utils.almd'
  hint: change to 'module utils' or rename the file to 'src/foo.almd'
```

### 可視性: デフォルト public + `local` で隠す

```almide
module utils

fn add(a: Int, b: Int) -> Int = a + b           // public (デフォルト)
fn multiply(a: Int, b: Int) -> Int = a * b      // public

local fn helper(x: Int) -> Int = x + 1          // このファイル内のみ
local type InternalState = { count: Int }        // このファイル内のみ
```

| 書き方 | 意味 |
|---|---|
| `fn f()` | public — import した誰でもアクセス可 |
| `local fn f()` | local — このファイル内でのみアクセス可 |

設計理由:
- モジュールは「使われるために存在する」→ デフォルト public が自然
- 隠すのは意識的な判断 → `local` で明示
- `local` は正しい英語で「局所的」の意味がそのまま伝わる
- `pub pub pub` のノイズを避ける
- 2段階でシンプル。internal は必要になったら将来追加

外部から `local` にアクセスしたらエラー:
```
error: function 'validate' is local in module 'utils'
  hint: local functions cannot be accessed from other modules
```

### self キーワード

- `self` = 自己モジュール参照（ローカルモジュールの import 用）
- class/interface がないため `self` がインスタンス参照と衝突しない
- trait/impl は将来検討。導入時に `self` の用途拡張を再検討

### テスト

```almide
// tests/utils_test.almd — public API のテスト
import self.utils

test "add works" {
  assert_eq(utils.add(1, 2), 3)
}
```

```almide
// src/utils.almd — local 関数の内部テスト
local fn validate(x: Int) -> Bool = x > 0

test "validate works" {
  assert(validate(5))
}
```

- `tests/` 配下: public API のテスト。`local` 関数にはアクセスできない
- 同一ファイル内 `test` ブロック: `local` 関数もテスト可能

### almide add (現状維持)

```bash
almide add json --git https://github.com/almide/json --tag v1.0.0
almide add csv --git https://github.com/almide/csv --tag v2.0.0
almide remove json
almide deps
```

git URL 明示方式。パッケージレジストリは将来検討。

### 不採用にしたもの

- **selective import** (`import x.{ a, b }`): 一旦保留。必要性を見極める
- **class / interface**: Almide は関数型。型 + モジュール関数 + パターンマッチで対応
- **デフォルト private + `pub`**: ノイジー。デフォルト public + `local` を採用
- **expose リスト方式** (Elm/Roc): 関数追加のたびヘッダ更新が面倒
- **re-export**: mod.almd からの自動 re-export はしない。明示的に import する

---

## 実装 TODO

### Phase 1: モジュール可視性の実装
- [ ] `local` キーワードを lexer/parser に追加
- [ ] checker で `local` 関数/型の外部アクセスをエラーにする
- [ ] emitter で `local` を反映 (Rust: pub なし, TS: export なし)
- [ ] エラーメッセージ: 「function 'helper' is local in module 'utils'」

### Phase 2: self import の実装
- [ ] `import self.xxx` 構文を parser に追加
- [ ] resolver で `self.xxx` → `src/xxx.almd` の解決
- [ ] module 宣言とファイルパスの一致チェック
- [ ] `as` エイリアスの実装

### Phase 3: ユーザーモジュール分割の e2e 確認
- [ ] ユーザー定義モジュール間の import が全ターゲット (Rust/TS/JS) で動くことを確認
- [ ] exercise として multi-file サンプルを追加
- [ ] `resolve.rs` の整理 (現在 dead code 警告あり)

### Phase 4: TS/JS emitter のマルチファイル対応
- [ ] 単一ファイルインライン展開で統一
- [ ] playground (単一ファイル出力) との整合
- [ ] import されたモジュールの関数を正しく呼び出せるようにする

### Phase 5: ネストしたモジュールパス
- [ ] `import self.foo.bar` → `src/foo/bar.almd` の解決
- [ ] ディレクトリモジュール (`mod.almd`) の解決

## その他

- [ ] ユーザー定義ジェネリクス
- [ ] trait / impl の実装検討
- [ ] パッケージレジストリ (将来検討)
