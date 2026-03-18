# Stdlib Additions

**優先度:** 1.x — 1.0後に段階的追加
**リサーチ:** [stdlib-module-matrix.md](../../research/stdlib-module-matrix.md)

## 追加候補

| Module | 根拠 | 優先度 |
|--------|------|--------|
| **set** | Gleam✅ Elm✅ MoonBit✅ Elixir✅。コレクション型として標準的。list.unique で代替可能だが O(n²) | High |

## 保留候補 (2.x以降)

| Module | 根拠 |
|--------|------|
| **net** (TCP/UDP) | Rust✅ Go✅。http より低レイヤー。需要があれば |
| **encoding** (base64等) | Go✅。現在 .almd で提供中。TOML昇格は需要次第 |
| **channel** | Go✅ Rust✅。fan の拡張として検討 |

## 設計原則

- **追加は慎重に** — 一度 stdlib に入ると凍結される
- **パッケージで試してからstdlibに昇格** — Deno model
- **multi-target で動作するもののみ** — Rust + TS 両方で意味があること
