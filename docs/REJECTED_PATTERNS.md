# Rejected Patterns

> Matz: "I should have thought more about the features I took from Perl."
> TypeScript: enum と namespace を後悔して 10 年。
>
> このリストに載っている機能は **意図的に採用しない**。再提案する前にここを読むこと。

---

## 構文

### `while` キーワード

**却下理由**: Canonicity 違反。Almide のループは 2 形態:
- `for x in xs { }` — コレクション走査
- `do { guard ... else break }` — 条件駆動ループ

`while` を追加すると 3 種のループになり、LLM が選択に迷う。`do { guard }` で同じ表現力がある。

### `??` (nullish coalescing) 演算子

**却下理由**: Canonicity 違反。`unwrap_or` が同じ機能を提供:
- `unwrap_or(opt, default)` — 関数呼び出し
- `opt.unwrap_or(default)` — UFCS

`??` を追加すると同じ意味に 3 つの書き方ができる。Vocabulary Economy の原則で不要。

### `return` キーワード

**却下理由**: 暗黙の最後の式が関数の戻り値。`return` は不要。
- `guard ... else <expr>` が早期脱出を構造的に表現
- 複数の return パスは `if/then/else` か `match` で表現

### `s[i]` (文字列インデックス)

**却下理由**: 演算子オーバーロード禁止 (SPEC §19)。`string.char_at(s, i)` を使う。

### match arm の `{}` 省略

**却下理由**: Rust と同じルール。LLM の既存知識と一致。Canonicity (ブロックは常に `{}` で囲む)。

### ternary `? :` 演算子

**却下理由**: `if/then/else` が式。ternary は不要で、LLM が条件式の構文を 2 つ覚える必要が生じる。

---

## 型システム

### null / nil / undefined

**却下理由**: `Option[T]` が唯一の「値がないかもしれない」表現。null は Go の nil panic、JavaScript の undefined 地獄を生む。

### implicit type conversion

**却下理由**: `int.to_string(n)`, `float.to_int(x)` で明示変換。暗黙の変換は Python 2→3 の `/` 演算子 (int→float) のような silent な挙動変更を引き起こす。

### operator overloading

**却下理由**: 演算子は固定セマンティクス。`+` は常に加算、`++` は常に連結。ユーザー定義演算子は解析の曖昧さと LLM の混乱を生む。

### algebraic effects (拡張 effect system)

**却下理由**: `effect fn` は I/O/副作用マーカーに留める。Gleam が effect system なしで成功したように、シンプルな 2 値分類 (pure / effect) で十分。typed effects (`effect[fs]`, `effect[http]`) は Security Layer 2-3 で将来検討。

### Higher-Kinded Types (HKT)

**却下理由**: Swift の PAT (Protocol with Associated Types) が 7 年かけて ergonomic にした複雑さ。LLM が HKT を正しく使えるエビデンスがない。

### type classes / implicit instances

**却下理由**: Haskell の orphan instances、Scala の implicit resolution — グローバル検索が必要で、LLM のローカル推論と相性が悪い。

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

**却下理由**: Zig の comptime、Rust の proc macros — 強力だが LLM が正しく生成できない。Almide は TOML 定義 + `build.rs` でコード生成。

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
