# Stdlib Additions — 完了

**優先度:** 1.x — 1.0後に段階的追加
**リサーチ:** [stdlib-module-matrix.md](../../research/stdlib-module-matrix.md)

## 完了: set モジュール (20関数)

11 → 20 関数に拡充。Gleam set (20関数) と同等。

### 追加した関数 (9件)

| 関数 | 説明 | 他言語 |
|------|------|--------|
| `symmetric_difference(a, b)` | 排他的和 | Rust, Gleam, Python, Elixir |
| `is_subset(a, b)` | a ⊆ b | Rust, Gleam, Python, Elixir |
| `is_disjoint(a, b)` | 共通要素なし | Rust, Gleam, Python, Elixir |
| `filter(s, f)` | 述語一致のみ保持 | Gleam, Elm, Kotlin, Elixir |
| `map(s, f)` | 要素変換 | Gleam, Elm, Kotlin |
| `fold(s, init, f)` | 畳み込み | Gleam, Elm, Kotlin, Elixir |
| `each(s, f)` | 副作用付き反復 | list/map と一貫性 |
| `any(s, f)` | いずれかが真 | list/map と一貫性 |
| `all(s, f)` | すべてが真 | list/map と一貫性 |

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
