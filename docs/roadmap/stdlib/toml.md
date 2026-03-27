<!-- description: TOML parsing and generation for config files -->
# stdlib: toml [Tier 2]

TOML パース/生成。設定ファイルの標準フォーマット。Almide 自身も `almide.toml` を使う。

## 他言語比較

| 機能 | Go (`BurntSushi/toml`) | Python (`tomllib`) | Rust (`toml` crate) | Deno (`@std/toml`) |
|------|----------------------|--------------------|--------------------|-------------------|
| パース | `toml.Decode(s, &v)` | `tomllib.loads(s)` | `toml::from_str(s)` | `parse(text)` |
| 生成 | `toml.Marshal(v)` | `tomli_w.dumps(v)` | `toml::to_string(&v)` | `stringify(obj)` |
| 型 | struct mapping | dict | serde derives | plain object |
| 日時 | `time.Time` | `datetime` | `toml::value::Datetime` | `Date` |
| エラー | line/col info | line info | span info | line info |

## 追加候補 (~6 関数)

### P0
- `toml.parse(text) -> Result[Json, String]` — TOML → JSON 構造（Almide の Json 型を再利用）
- `toml.stringify(value) -> String` — JSON 構造 → TOML 文字列

### P1
- `toml.get(value, path) -> Option[Json]` — ドット区切りパスでアクセス（`"server.port"`）
- `toml.merge(base, override) -> Json` — 設定のマージ

### P2
- `toml.parse_file(path) -> Result[Json, String]` — ファイルから直接パース
- `toml.validate(text) -> Result[Unit, String]` — 構文検証のみ

## 実装戦略

self-host (.almd) が理想（両ターゲット対応）だが、TOML パーサーは複雑（日時、マルチライン文字列等）。
Phase 1 は @extern (Rust: `toml` crate, TS: `@std/toml`) で出荷し、後で self-host 化を検討。
