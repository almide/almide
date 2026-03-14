# Serialization Format Output Types [ACTIVE]

## 問題

現在 `toml.parse()` が `Json` 型を返す。TOML の出力が「Json」なのは名前・意味論ともに不適切。

## 解答: Codec Protocol

codec-and-json.md で設計済みの **Codec protocol** がこの問題を根本解決する。

```almide
type Config = { host: String, port: Int } deriving Codec

// Phase 5 の姿: フォーマット非依存の typed decode
let config = toml.decode_from_string[Config](text)?
let config = yaml.decode_from_string[Config](text)?
```

### なぜ Json 型を直接返すのが悪いか

1. **名前の不一致** — TOML を「Json」で返すのは混乱する
2. **型安全性の欠如** — `Json` は untyped。`json.get_string(j, "host")` で Option を介して取り出す非人間的 API
3. **Almide の設計思想に反する** — Record / Map / Variant があるのに、全てを `Json` 箱に入れるのは退化

### 正しいアーキテクチャ

```
Text ──parse──▶ Json (内部表現) ──decode[T]──▶ T (typed record)
T ──encode[T]──▶ Json (内部表現) ──stringify──▶ Text
```

- `Json` は **内部中間表現** として使う（ユーザーに見せない）
- ユーザー API は `decode[T]` / `encode[T]` で typed record を扱う
- Swift の `Codable` に相当するが、repair + schema introspection でそれを超える

## 依存関係

1. **Phase 2: `deriving Codec`** が前提（codec-and-json.md）
2. Phase 2 完了後に toml/yaml を Codec 対応に書き直す
3. 当面の stdlib-toml は Json 型で実装済み（技術的負債として認識）

## 当面の方針

- stdlib-yaml は `deriving Codec` 完了まで blocked
- stdlib-toml は現状維持（Json 型）。Codec 後にリファクタ
- 新しいフォーマットモジュールは Codec 対応を前提に設計
