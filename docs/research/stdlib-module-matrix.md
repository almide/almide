# Stdlib Module Matrix: Almide vs 他言語

各モジュールが他言語の1.0 stdlibに入ってるか。

✅ = stdlib に入ってる
📦 = 公式パッケージ/first-party (stdlib外)
❌ = なし / community package

## Core Types

| Module | Almide | Gleam | Elm | Go | Rust | Kotlin | MoonBit | Elixir | 判定 |
|--------|--------|-------|-----|-----|------|--------|---------|--------|------|
| **string** | ✅ 44 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **必須** |
| **int** | ✅ 21 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **必須** |
| **float** | ✅ 16 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **必須** |
| **list** | ✅ 57 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | **必須** |
| **map** | ✅ 22 | ✅ dict | ✅ dict | ✅ | ✅ | ✅ | ✅ | ✅ | **必須** |
| **option** | ✅ 12 | ✅ | ✅ maybe | ❌ | ✅ | ✅ | ✅ | ❌ | **必須** (型あり言語) |
| **result** | ✅ 9 | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | **必須** (型あり言語) |
| **math** | ✅ 21 | ❌ | ✅ | ✅ | ❌📦 | ✅ | ✅ | ✅ | **標準的** |

## I/O

| Module | Almide | Gleam | Elm | Go | Rust | Kotlin | MoonBit | Elixir | 判定 |
|--------|--------|-------|-----|-----|------|--------|---------|--------|------|
| **fs** | ✅ 24 | ❌📦 | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | **CLI言語なら必須** |
| **io** | ✅ 3 | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | **必須** |
| **env** | ✅ 9 | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | **CLI言語なら必須** |
| **process** | ✅ 6 | ❌📦 | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | **標準的** |
| **http** | ✅ 26 | ❌📦 | ❌📦 | ✅ | ❌📦 | ❌📦 | ❌ | ❌📦 | **Almide判断** |

## Data Format

| Module | Almide | Gleam | Elm | Go | Rust | Kotlin | MoonBit | Elixir | 判定 |
|--------|--------|-------|-----|-----|------|--------|---------|--------|------|
| **json** | ✅ 30 | ❌📦 | ❌📦 | ✅ | ❌📦 | ❌📦 | ✅ | ✅ | **Almide判断** (パッケージなし) |
| **value** | ✅ 19 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | **Almide独自** (codec用) |
| **regex** | ✅ 8 | ❌📦 | ❌📦 | ✅ | ❌📦 | ✅ | ❌ | ✅ | **標準的** |
| **datetime** | ✅ 21 | ❌📦 | ❌📦 | ✅ | ❌📦 | ❌📦 | ❌ | ❌📦 | **Almide判断** |

## Utility

| Module | Almide | Gleam | Elm | Go | Rust | Kotlin | MoonBit | Elixir | 判定 |
|--------|--------|-------|-----|-----|------|--------|---------|--------|------|
| **random** | ✅ 4 | ❌📦 | ❌📦 | ✅ | ❌📦 | ✅ | ✅ | ✅ | **分かれる** |
| **crypto** | ✅ 4 | ❌ | ❌ | ✅ | ❌📦 | ❌📦 | ❌ | ❌📦 | **Go以外はstdlib外** |
| **uuid** | ✅ 6 | ❌ | ❌ | ❌📦 | ❌📦 | ❌📦 | ❌ | ❌📦 | **どの言語もstdlib外** |
| **log** | ✅ 8 | ❌ | ❌ | ✅ | ❌📦 | ❌📦 | ❌ | ✅ | **Go/Elixirのみ** |
| **testing** | ✅ 7 | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | **大半がstdlib** |
| **error** | ✅ 3 | ❌ | ❌ | ✅ | ✅ | ❌ | ✅ | ❌ | **標準的** |

## Bundled .almd (凍結対象外)

| Module | 関数数 | 他言語stdlib | 判定 |
|--------|--------|-------------|------|
| **path** | 7 | Go✅ Rust✅ Elixir❌ | **標準的** |
| **url** | 21 | Go✅ Rust❌ Elixir✅ | **標準的** |
| **args** | 6 | Go✅ MoonBit✅ | **CLI言語なら標準** |
| **time** | 20 | datetime と重複？ | **要整理** |
| **csv** | 9 | Go✅ Rust❌ Elixir❌ | **Go以外はstdlib外** |
| **toml** | 14 | Go❌ Rust❌ Elixir❌ | **どの言語もstdlib外** |
| **encoding** | 10 | Go✅ Rust❌ | **Go以外はstdlib外** |
| **hash** | 3 | Go✅ Rust✅ | **分かれる** |
| **compress** | 4 | Go✅ Rust❌ | **Go以外はstdlib外** |
| **term** | 21 | Go❌ Rust❌ | **どの言語もstdlib外** |
| **value** (.almd) | 17 | ❌ | **Almide独自** |

## 結論

### 削除候補 (stdlibから外してfirst-party packageに)

| Module | 理由 |
|--------|------|
| **uuid** | **全言語がstdlib外**。crypto.random_hex で代替可能 |
| **crypto** | Go以外は全てstdlib外。4関数では中途半端 |
| **toml** (.almd) | **全言語がstdlib外** |
| **compress** (.almd) | Go以外はstdlib外。4関数では中途半端 |
| **term** (.almd) | **全言語がstdlib外**。TS targetで意味薄い |

### 追加候補

| Module | 理由 |
|--------|------|
| **set** | Gleam✅ Elm✅ Go❌ Rust❌ MoonBit✅ Elixir✅。コレクション型として標準的 |

### 維持で問題ないもの

http, json, datetime, log, random — パッケージエコシステムがないため stdlib に含める合理性がある。
パッケージエコシステムが成熟したら外に出す選択肢もある。
