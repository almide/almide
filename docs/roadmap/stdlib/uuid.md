# stdlib: uuid [Tier 2]

UUID の生成・パース・フォーマット。小さいが頻出するモジュール。

## 他言語比較

| 操作 | Go (`google/uuid`) | Python (`uuid`) | Rust (`uuid`) | Deno (`crypto`) |
|------|-------------------|-----------------|---------------|-----------------|
| v4 生成 | `uuid.New()` | `uuid.uuid4()` | `Uuid::new_v4()` | `crypto.randomUUID()` |
| パース | `uuid.Parse(s)` | `uuid.UUID(s)` | `Uuid::parse_str(s)` | manual |
| フォーマット | `.String()` | `str(u)` | `.to_string()` | already string |
| v5 (name-based) | `uuid.NewSHA1(ns, name)` | `uuid.uuid5(ns, name)` | `Uuid::new_v5(ns, name)` | manual |
| nil UUID | `uuid.Nil` | `uuid.UUID(int=0)` | `Uuid::nil()` | manual |
| バージョン取得 | `.Version()` | `.version` | `.get_version()` | manual |
| 比較 | `==` | `==` | `==` | `===` |
| バリデーション | `uuid.Validate(s)` | try parse | try parse | manual regex |

## 追加候補 (~6 関数)

- `uuid.v4() -> String` — ランダム UUID v4 生成
- `uuid.v5(namespace, name) -> String` — 名前ベース UUID v5
- `uuid.parse(s) -> Result[String, String]` — UUID 文字列のバリデーション + 正規化
- `uuid.is_valid?(s) -> Bool` — UUID 形式チェック
- `uuid.nil() -> String` — nil UUID (`00000000-0000-0000-0000-000000000000`)
- `uuid.version(s) -> Option[Int]` — UUID バージョン取得

## 実装戦略

@extern。Rust: `uuid` crate。TS: `crypto.randomUUID()` + 手動パース。
小さいモジュールなので self-host (.almd) も可能（v4 は `crypto.random_bytes` + フォーマット）。
