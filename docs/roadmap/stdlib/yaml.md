# stdlib: yaml [Tier 2]

YAML パース/生成。設定ファイル・CI 定義で広く使われる。

## 他言語比較

| 機能 | Go (`gopkg.in/yaml.v3`) | Python (`PyYAML`) | Rust (`serde_yaml`) | Deno (`@std/yaml`) |
|------|------------------------|--------------------|--------------------|-------------------|
| パース | `yaml.Unmarshal(data, &v)` | `yaml.safe_load(s)` | `serde_yaml::from_str(s)` | `parse(text)` |
| 生成 | `yaml.Marshal(v)` | `yaml.dump(v)` | `serde_yaml::to_string(&v)` | `stringify(obj)` |
| マルチドキュメント | `yaml.NewDecoder(r)` | `yaml.safe_load_all(s)` | `serde_yaml::Deserializer::from_str` | ❌ |
| 型変換 | struct tags | auto-detect | serde derives | auto-detect |
| safe load | ❌ (always safe) | `safe_load` vs `load` | always safe | always safe |

## 追加候補 (~4 関数)

### P0
- `yaml.parse(text) -> Result[Json, String]` — YAML → JSON 構造
- `yaml.stringify(value) -> String` — JSON 構造 → YAML 文字列

### P1
- `yaml.parse_all(text) -> Result[List[Json], String]` — マルチドキュメント
- `yaml.parse_file(path) -> Result[Json, String]` — ファイルから直接

## 実装戦略

@extern。Rust: `serde_yaml`。TS: `@std/yaml` (Deno) / `js-yaml` (Node)。
YAML パーサーの self-host は複雑すぎるため非推奨。
