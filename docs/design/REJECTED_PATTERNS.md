# Rejected Patterns

> Matz: "I should have thought more about the features I took from Perl."
> TypeScript: enum と namespace を後悔して 10 年。
>
> このリストに載っている機能は **意図的に採用しない**。再提案する前にここを読むこと。

---

## 構文

### ~~`??` (nullish coalescing) 演算子~~ — **撤回済み (ADR-0005)**

当初は「`unwrap_or` が同じ機能を提供するため Canonicity 違反」として却下した。
**この判断は ADR-0005 で逆転した**: `??` が正準形であり、desugar 定義
(`a ?? b ≡ unwrap_or_else`) は [docs/specs/result-option-effect.md §"演算子の定義"](../specs/result-option-effect.md) にある。
むしろ `unwrap_or` 関数の側が削除予定 (ADR-0005 D4、almide#1107)。
却下履歴として残す — 「3 つの書き方」問題は演算子を削る方向ではなく、関数を削る方向で解消される。

### `return` キーワード

**却下理由**: 暗黙の最後の式が関数の戻り値。`return` は不要。
- `guard ... else <expr>` が早期脱出を構造的に表現
- 複数の return パスは `if/then/else` か `match` で表現

### `s[i]` (文字列インデックス)

**現行 API**: 文字列の文字取得には `string.get(s, i)` を使う。これは、既存演算子の protocol dispatch まで禁止するという意味ではない。

### match arm の `{}` 省略 — 現行構文では許可

旧却下記述は現行 parser と一致しない。arm body は式なので `some(x) => x` の
ように書ける。複数文の block には `{ ... }` を使う
（`crates/almide-syntax/src/parser/compounds.rs::parse_match_arm`）。

### ternary `? :` 演算子

**却下理由**: `if/then/else` が式。ternary は不要で、LLM が条件式の構文を 2 つ覚える必要が生じる。

---

## 型システム

### null / nil / undefined

**却下理由**: `Option[T]` が唯一の「値がないかもしれない」表現。null は Go の nil panic、JavaScript の undefined 地獄を生む。

### implicit type conversion

**却下理由**: `int.to_string(n)`, `int.from_saturating(x)` で明示変換。暗黙の変換は Python 2→3 の `/` 演算子 (int→float) のような silent な挙動変更を引き起こす。

### custom operator syntax — protocol dispatch とは区別

利用者が新しい演算子記号や構文を定義する機能はない。一方、既存演算子は
宣言された protocol に従い requirement method へ dispatch できる。たとえば
`Eq` の `==` は `Type.eq`、`Numeric` の `+` は `add` に対応する。
旧「演算子オーバーロードを全面禁止」という説明は現行実装と一致しない
（`canonicalize/protocols.rs`、`spec/lang/operator_protocol_test.almd`、
`spec/stdlib/bigint_operators_test.almd`）。

### algebraic effects (拡張 effect system)

**却下理由**: `effect fn` は I/O/副作用マーカーに留める。Gleam が effect system なしで成功したように、シンプルな 2 値分類 (pure / effect) で十分。表面構文 `effect[fs]` / `effect[http]` は採用していない。一方、関数値を通した
capability category set の推論と `[permissions].allow` は実装済み
（[capability system](../wasm/capability-system.md)）。

### Higher-Kinded Types (HKT)

**却下理由**: Swift の PAT (Protocol with Associated Types) が 7 年かけて ergonomic にした複雑さ。LLM が HKT を正しく使えるエビデンスがない。

### type classes / implicit instances

**却下理由**: Haskell の orphan instances、Scala の implicit resolution — グローバル検索が必要で、LLM のローカル推論と相性が悪い。

### `trait` / `interface` キーワード

**却下理由**: Almide は `protocol` キーワードを採用。理由:
- `trait` は Rust 固有の語彙。associated types、trait objects、orphan rules を連想させ、LLM が Rust のパターンを持ち込む
- `interface` は Go/Java/TS で広く使われるが、Go の暗黙的満足と Java の明示的 `implements` で意味が分裂している
- `protocol` は Swift/Python (PEP 544) の語彙。Almide の既存 `: Convention` 構文（`type Dog: Eq`）と Swift の `: Protocol` 構文が一致する
- 2024-2026年の新しい言語が選ぶ傾向にある語彙

### `impl` ブロック

**却下理由**: Almide は convention methods (`fn Type.method()`) でプロトコル実装する。`impl Protocol for Type { ... }` ブロックは不要:
- convention methods はトップレベル関数 — フラット、ネストなし
- LLM が正確に生成しやすい（インデント深度が増えない）
- 既存の convention method システムと完全に一貫

### 暗黙的プロトコル満足 (Go 式 structural interfaces)

**却下理由**: Go ではメソッドがあれば暗黙的にインターフェースを満たす。Almide は `type Foo: Protocol` の明示的宣言を要求:
- LLM がコードを読むだけで実装関係がわかる
- コンパイラが「メソッド不足」を正確に報告できる
- 既存の `: Eq, Repr` 構文と一貫

### dynamic dispatch / protocol objects

**却下理由**: Almide のプロトコルはモノモーフィゼーションで解決。vtable や dynamic dispatch は不要:
- コンパイル時に全て具体型に展開される
- ランタイムオーバーヘッドゼロ
- Go の interface 値のような boxing が不要

---

## ランタイム・並行処理

### mutable by default

**却下理由**: `let` が immutable、`var` が mutable。Ruby の mutable-by-default は Ractor との非互換性を生んだ。`fan` ブロック内での `var` キャプチャは禁止。

### async / await キーワード

**却下理由**: `effect fn` + `fan` で並行処理を表現。async/await は:
- Python: 関数カラーリング問題でエコシステム分断
- Rust: Pin/Unpin, Send + 'static, runtime 選択の複雑さ
- Swift: 7 年遅れで retrofit

Almide の `fan` は「並行の意図」だけを書く。コンパイラがターゲットごとに最適な実装を生成。

### goroutine / green thread / actor

**却下理由**: `fan` の構造化された並行で十分。goroutine leak (Go)、Ractor の非互換 (Ruby) を避ける。Supervision/Actor は on-hold で将来検討。

### `Future[T]` / `Promise` 型の露出

**却下理由**: ユーザーに見せない。`effect fn foo() -> Int` の戻り型は `Int` であって `Future[Int]` ではない。LLM が `Future[Future[Int]]` で混乱するのを防ぐ。

---

## メタプログラミング

### macros

**却下理由**: Zig の comptime、Rust の proc macros — 強力だが LLM が正しく生成できない。compiler 内部の生成処理と、利用者が書く macro 構文は別物。stdlib の定義元は
現在 `stdlib/*.almd` であり、旧 TOML stdlib 定義ではない。

### monkey patching / open classes

**却下理由**: Ruby の monkey patching は「developer happiness」だが、LLM にとっては「メソッドが定義元以外の場所で変更される」という解析不能な動作。

### reflection / runtime type inspection

**却下理由**: 静的型でコンパイル時に全て解決。ランタイム型情報は不要。

---

## パッケージ・設定

### 複数の設定ファイル形式

**却下理由**: `almide.toml` が唯一の設定。Python は `setup.py` → `setup.cfg` → `pyproject.toml` に 23 年かかった。

### 実行可能な build script

**却下理由**: `almide.toml` は宣言的 TOML のみ。Python の `setup.py` のような実行可能設定は再現性を破壊する。

---

## このリストの運用

- **追加**: 機能を検討して却下したとき、理由と共にここに追加する
- **削除**: 言語の方向性が根本的に変わった場合のみ（edition 変更レベル）
- **参照**: PR やイシューで「この機能が欲しい」と提案されたとき、ここを指す
