<!-- description: Systematic language feature test suite for regression detection -->
<!-- done: 2026-03-11 -->
# Language Test Suite

Almide の言語機能を体系的にテストするスイート。`lang/` に配置。

## Goal

コンパイラ変更時のリグレッション検知。全機能を網羅するテストがあれば、安心してリファクタできる。

## Structure

カテゴリごとに1ファイル。各ファイルは `test "..." { assert_eq(...) }` で構成。

```
exercises/lang-test/
  expr_test.almd           基本式（算術、比較、論理、文字列）
  control_flow_test.almd   制御フロー（if, match, for, do/guard）
  data_types_test.almd     データ型（record, tuple, variant, Option, Result）
  function_test.almd       関数（純粋fn, effect fn, lambda, 再帰, UFCS）
  variable_test.almd       変数（let, var, 代入, index assign, field assign）
  pattern_test.almd        パターンマッチ（リテラル, コンストラクタ, ガード, ネスト）
  operator_test.almd       演算子（++, |>, ==, !=, 比較, ビット演算）
  type_system_test.almd    型（type alias, generics, newtype, variant）
  error_test.almd          エラー処理（ok/err, effect fn, auto-?, do block）
  string_test.almd         文字列（interpolation, heredoc, エスケープ）
  scope_test.almd          スコープ（シャドウイング, クロージャ, ネストブロック）
  edge_cases_test.almd     エッジケース（空コレクション, 大きな数, 境界値）
```

## Categories

### 1. expr_test — 基本式
- [ ] 整数四則演算（+, -, *, /, %）
- [ ] 浮動小数点演算
- [ ] 演算子優先順位（`2 + 3 * 4 == 14`）
- [ ] 単項マイナス（`-x`）
- [ ] 比較演算子（<, >, <=, >=, ==, !=）Int/String/Bool
- [ ] 論理演算子（and, or, not）
- [ ] 短絡評価（and/or）
- [ ] 文字列結合（++）
- [ ] リスト結合（++）
- [ ] 括弧によるグルーピング

### 2. control_flow_test — 制御フロー
- [ ] if/then/else（値を返す）
- [ ] ネストしたif
- [ ] if/then/else with ブロック
- [ ] match with リテラルパターン（Int, String, Bool）
- [ ] match with Option（some/none）
- [ ] match with Result（ok/err）
- [ ] match with wildcard（_）
- [ ] match with ガード条件
- [ ] for...in リスト
- [ ] for...in with タプル分解（enumerate, zip）
- [ ] for...in with var 蓄積
- [ ] do { guard } ループ
- [ ] do block 内のguard else break
- [ ] ネストしたfor/do

### 3. data_types_test — データ型
- [ ] レコードリテラル・フィールドアクセス
- [ ] ネストしたレコード
- [ ] レコードスプレッド（{ ...base, field: v }）
- [ ] タプルリテラル・インデックスアクセス（.0, .1）
- [ ] タプル分解（let (a, b) = ...）
- [ ] variant型の定義と構築
- [ ] variant with ゼロ引数・タプル・レコードペイロード
- [ ] Option[T]の生成と分解
- [ ] Result[T, E]の生成と分解
- [ ] List[T]の基本操作
- [ ] Map[K, V]の基本操作

### 4. function_test — 関数
- [ ] 単純な関数定義と呼び出し
- [ ] 複数引数
- [ ] 引数なし関数
- [ ] 再帰（factorial, fibonacci）
- [ ] 相互再帰
- [ ] 高階関数（関数を引数に取る）
- [ ] 関数を返す関数
- [ ] lambda（fn(x) => expr）
- [ ] lambda with ブロック本体
- [ ] lambda in map/filter/fold
- [ ] クロージャ（外側変数のキャプチャ）
- [ ] UFCS（`x.f(y)` == `f(x, y)`）
- [ ] UFCSチェイン（`x.f().g()`）

### 5. variable_test — 変数
- [ ] let（不変束縛）
- [ ] let with 型注釈
- [ ] var（可変束縛）と再代入
- [ ] index assign（xs[i] = v）
- [ ] field assign（r.f = v）
- [ ] var in for ループ
- [ ] let シャドウイング（同名の再束縛）

### 6. pattern_test — パターンマッチ
- [ ] リテラルパターン（Int, String, Bool）
- [ ] 識別子パターン（束縛）
- [ ] ワイルドカード（_）
- [ ] some(x) / none パターン
- [ ] ok(x) / err(e) パターン
- [ ] コンストラクタパターン（ユーザー定義variant）
- [ ] タプルパターン（(a, b)）
- [ ] レコードパターン（{ field1, field2 }）
- [ ] ネストパターン（some((a, b))）
- [ ] ガード付きパターン（pattern if condition）

### 7. operator_test — 演算子
- [ ] ++ 文字列結合
- [ ] ++ リスト結合
- [ ] |> パイプ演算子
- [ ] |> チェイン
- [ ] == / != 深い等値性（リスト、レコード）
- [ ] ^ べき乗
- [ ] ビット演算（int.band, int.bor, int.bxor, int.bshl, int.bshr, int.bnot）

### 8. type_system_test — 型システム
- [ ] type alias（基本型）
- [ ] type alias（ジェネリック型）
- [ ] variant型の定義
- [ ] variant with 複数コンストラクタ
- [ ] ジェネリックvariant
- [ ] newtype（deriving付き）
- [ ] ジェネリックレコード型

### 9. error_test — エラー処理
- [ ] ok(value) / err(error) の構築
- [ ] match on Result
- [ ] effect fn 内のauto-?（エラー伝播）
- [ ] do block 内のResult自動アンラップ
- [ ] guard else err(...)
- [ ] effect fn chain（複数のfallible操作）

### 10. string_test — 文字列
- [ ] 文字列補間（${expr}）
- [ ] 補間内の式（${x + 1}）
- [ ] 補間内の関数呼び出し（${f(x)}）
- [ ] heredoc（"""..."""）
- [ ] heredocのインデント除去
- [ ] エスケープシーケンス（\n, \t, \\, \"）

### 11. scope_test — スコープ
- [ ] ブロックスコープ
- [ ] ネストブロックでのシャドウイング
- [ ] for ループ変数のスコープ
- [ ] match arm 内の束縛のスコープ
- [ ] クロージャの変数キャプチャ

### 12. edge_cases_test — エッジケース
- [ ] 空リスト操作（len, get, map, filter）
- [ ] 空文字列操作
- [ ] 空マップ操作
- [ ] list.get 範囲外 → none
- [ ] 0除算
- [ ] 大きな整数
- [ ] ネストした複合型（List[List[Int]], Map[String, List[Int]]）

## Implementation Order

1. expr_test + control_flow_test（基盤）
2. data_types_test + variable_test（データ）
3. function_test + pattern_test（関数とパターン）
4. operator_test + type_system_test（演算子と型）
5. error_test + string_test（エラーと文字列）
6. scope_test + edge_cases_test（スコープとエッジ）

## Principles

- 1テスト = 1つの振る舞いを検証
- テスト名は検証内容を英語で簡潔に
- assert_eq を基本に、assert は条件チェックのみ
- 各ファイルは独立して `almide test file.almd` で実行可能
- CI で全ファイル実行
