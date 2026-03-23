# Stdlib v2 Specification

> Almide stdlib の最終仕様。旧 stdlib-1.0.md を破棄し、他言語との比較に基づいて再設計。

## 設計制約

1. **パッケージエコシステムがない** — 必要なものはstdlibに入れるしかない
2. **4ターゲット (Rust/TS/JS/WASM)** — 各ターゲットのネイティブライブラリに依存できない
3. **LLM最適化** — モジュールが多すぎると予測精度が下がる。各モジュールは明確な責務を持つ
4. **凍結コスト** — 1.0で凍結される。不要なものを入れると永久に維持する負債になる

## 判断基準

| 判定 | 条件 |
|------|------|
| **必須** | 6/8言語以上がstdlibに含む、または言語機能と直結 |
| **合理的** | パッケージエコシステムがないため必要 |
| **不要** | 他言語も含まない、または中途半端 |

---

## モジュール判定

### Tier 1: 必須 (全言語がstdlibに含む)

| Module | 関数数 | 根拠 |
|--------|--------|------|
| **string** | ~40 | 全言語。基本型操作 |
| **int** | ~17 | 全言語。基本型操作 |
| **float** | ~16 | 全言語。基本型操作 |
| **list** | ~54 | 全言語。主要コレクション |
| **map** | ~24 | 全言語(dict)。主要コレクション |
| **set** | ~20 | Gleam/Elm/Elixir/MoonBitがstdlib。コレクション三種の神器 |
| **option** | ~10 | 型あり言語は全て。Almideの型システムと直結 |
| **result** | ~11 | 型あり言語は全て。effect fnと直結 |
| **math** | ~21 | Go/Kotlin/MoonBit/Elixir/Elm。数学定数+関数 |
| **testing** | ~7 | Go/Rust/Kotlin/MoonBit/Elixir。テスト組み込み言語 |
| **error** | ~3 | Go/Rust/MoonBit。エラー操作 |

### Tier 2: 合理的 (パッケージエコシステム不在のため必要)

| Module | 関数数 | 根拠 |
|--------|--------|------|
| **json** | ~32 | Go/MoonBit/Elixir。パッケージなしではJSON扱えない |
| **value** | ~19 | Almide独自。Codec (encode/decode) の基盤 |
| **fs** | ~24 | Go/Rust/Kotlin/Elixir。CLIツールに必須 |
| **io** | ~3 | 全言語。stdin/stdout |
| **env** | ~9 | Go/Rust/Kotlin/Elixir。環境変数/OS情報 |
| **process** | ~6 | Go/Rust/Kotlin/Elixir。外部コマンド実行 |
| **http** | ~26 | Goのみstdlib。だがパッケージなしでは使えない |
| **datetime** | ~21 | Goのみstdlib。だがパッケージなしでは日時扱えない |
| **regex** | ~8 | Go/Kotlin/Elixir。パッケージなしでは正規表現使えない |
| **random** | ~4 | Go/Kotlin/MoonBit/Elixir。基本的なランダム |
| **log** | ~8 | Go/Elixir。構造化ログ |

### Tier 1.5: 言語機能と直結

| Module | 関数数 | 根拠 |
|--------|--------|------|
| **fan** | TBD | 言語機能 (`fan` 構文) と直結。並行処理の基盤。全ターゲットで必要 |

### 不要 (削除)

| Module | 理由 |
|--------|------|
| ~~**crypto**~~ | Go以外は全言語stdlib外。4関数では中途半端。必要ならパッケージで |
| ~~**uuid**~~ | 全言語がstdlib外。random.hexで代替可能 |

### bundled .almd

| Module | 判定 | 理由 |
|--------|------|------|
| **args** | **採用** | Go/MoonBitはstdlib。CLI言語として必要 |
| **path** | **採用** | Go/Rustはstdlib。OS差異の吸収はstring操作では面倒。CLI言語として必要 |
| ~~**csv**~~ | 削除 | Go以外はstdlib外。パーサーの品質保証が重い |
| ~~**encoding**~~ | 削除 | Go以外はstdlib外 |
| ~~**hash**~~ | 削除 | 分かれる。cryptoと同じ問題 |
| ~~**url**~~ | 削除 | Gleamはstdlib(uri)、他は分かれる。stringで代替可能 |
| ~~**time**~~ | 削除 | datetimeと重複 |
| ~~**value**~~ (.almd) | 削除 | value.toml と重複・混乱の原因 |

---

## 最終モジュールリスト

### Native (TOML定義): 22モジュール → **21モジュール**

削除: crypto, uuid
追加: fan (言語機能)

```
Core:     string, int, float, list, map, set, option, result, math
I/O:      fs, io, env, process, http, datetime, log, random
Data:     json, value, regex
Utility:  testing, error
Lang:     fan
```

### Bundled (.almd): 8モジュール → **1〜2モジュール**

採用: args, path
削除: csv, encoding, hash, url, time, value(.almd)

---

## 要検討事項

1. **http** — 26関数は多すぎないか？別途整理
2. **json** — 32関数は多すぎないか？別途整理
4. **fan** — 現在Rust only。全ターゲット対応 + TOML定義化が必要
5. **value.as_*** — 戻り値型を Result → Option に変更 (json.as_*と統一)

---

## 関数設計原則 (verb reform から継承)

1. **1 verb = 1 meaning** — 同じ動詞は全モジュールで同じ意味
2. **data-first** — 第一引数がデータ。UFCS と `|>` で自然に使える
3. **`to_*` は infallible、`parse` は fallible**
4. **`as_*` は動的型抽出** (Option を返す)
5. **`is_*` は Bool 述語**
6. **エイリアスなし** — 1つの操作に1つの名前 (Canonicity)
