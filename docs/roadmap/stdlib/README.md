# Stdlib Module References

モジュールごとの他言語比較と Almide への追加候補。

判定基準: **Go/Python/Rust/Deno のうち 2 言語以上の stdlib に存在するものは追加対象**。

## 現状カバレッジ

Almide は Go/Python/Rust/Deno の stdlib 機能の **約 25-30%** をカバー。

## Tier 1: これがないと実用プログラムが書けない

| モジュール | ファイル | 関数数 | 他言語 |
|-----------|---------|--------|--------|
| [datetime](datetime.md) | datetime.md | ~25 | Go, Python, Deno (3/4) |
| [fs 拡充](fs.md) | fs.md | ~15 | Go, Python, Rust, Deno (4/4) |
| [http 拡充](http-expansion.md) | http-expansion.md | ~20 | Go, Python, Deno (3/4) |
| [error](error.md) | error.md | ~10 | Go, Python, Rust, Deno (4/4) |

## Tier 2: 多くのアプリケーションで必要

| モジュール | ファイル | 関数数 | 他言語 |
|-----------|---------|--------|--------|
| [csv](csv.md) | csv.md | ~8 | Go, Python, Deno (3/4) |
| [toml](toml.md) | toml.md | ~6 | Python, Rust, Deno (3/4) |
| [yaml](yaml.md) | yaml.md | ~4 | Go(ext), Python(ext), Deno (2-3/4) |
| [url](url.md) | url.md | ~12 | Go, Python, Rust, Deno (4/4) |
| [crypto](crypto.md) | crypto.md | ~15 | Go, Python, Deno (3/4) |
| [uuid](uuid.md) | uuid.md | ~6 | Python, Deno (2/4) |
| [html](html.md) | html.md | ~12 | Go, Python, Deno (3/4) |
| [set](set.md) | set.md | ~12 | Python, Rust, JS (3/4) |

## Tier 3: エコシステム成長に必要

| モジュール | ファイル | 関数数 | 他言語 |
|-----------|---------|--------|--------|
| [sql](sql.md) | sql.md | ~15 | Go, Python (2/4) |
| [websocket](websocket.md) | websocket.md | ~8 | Deno (1/4 std, 広く使われる) |
| [log](log.md) | log.md | ~8 | Go, Python, Deno (3/4) |
| [test](test.md) | test.md | ~10 | Go, Python, Rust, Deno (4/4) |
| [compress](compress.md) | compress.md | ~6 | Go, Python (2/4) |
| [net](net.md) | net.md | ~10 | Go, Python, Rust, Deno (4/4) |
| [mime](mime.md) | mime.md | ~4 | Go, Python, Deno (3/4) |
| [unicode](unicode.md) | unicode.md | ~8 | Go, Python, Rust (3/4) |
| [sorted](sorted.md) | sorted.md | ~10 | Go, Python, Rust, Deno (4/4) |

## 合計

| | モジュール数 | 関数数 |
|---|---|---|
| 現在 | 21 | ~282 |
| Tier 1 追加 | +4 | +70 |
| Tier 2 追加 | +8 | +75 |
| Tier 3 追加 | +9 | +79 |
| **合計** | **42** | **~506** |

※ 既存モジュールの拡充（string に unicode 追加等）を含めると 700+ 到達可能。
