# Result, Option, Effect — 完全仕様 (DRAFT)

> 1.0.0 に向けた仕様確定ドラフト。全演算子・全関数形態で辻褄が合うことを保証する。

## 1. 型

### Result[T, E]
```
ok(v)   : Result[T, E]   // 成功値
err(e)  : Result[T, E]   // エラー値
```

### Option[T]
```
some(v) : Option[T]       // 値あり
none    : Option[T]       // 値なし
```

### Never (bottom type)
```
process.exit(n) : Never   // 戻らない関数の戻り値型
```
Never はどの型にも代入可能。guard else, if then, match arm で使える。

## 2. 演算子

### `expr!` — unwrap with propagation

| 入力型 | 出力型 | Rust 生成 | 制限 |
|---|---|---|---|
| `Result[T, E]` | `T` | `(expr)?` | effect fn 内のみ |
| `Option[T]` | `T` | `(expr).ok_or("none".to_string())?` | effect fn 内のみ |
| test 内 | `T` | `(expr).unwrap()` | テスト専用 |

effect fn の外で `!` を使うとコンパイルエラー。

### `expr?` — Result → Option 変換

| 入力型 | 出力型 | Rust 生成 |
|---|---|---|
| `Result[T, E]` | `Option[T]` | `(expr).ok()` |
| `Option[T]` | `Option[T]` | identity（変換なし） |

### `expr ?? fallback` — unwrap with fallback

| 入力型 | 出力型 | Rust 生成 |
|---|---|---|
| `Result[T, E]` | `T` | `match expr { Ok(v) => v, Err(_) => fallback }` |
| `Option[T]` | `T` | `match expr { Some(v) => v, None => fallback }` |

**型判定ルール**: `inner.ty` が `Option[T]` なら Option テンプレート、それ以外は Result テンプレート。

**⚠ 既知の問題**: 型が `Unknown` のとき Result 扱いになる。型推論が失敗すると壊れる。

## 3. effect fn

### 宣言

```almide
effect fn read_file(path: String) -> String = fs.read_text(path)!
```

ユーザーは `-> T` を書く。コンパイラが暗黙に `Result<T, String>` に変換する。

### Rust 生成ルール

| Almide | Rust |
|---|---|
| `effect fn f() -> T` | `fn f() -> Result<T, String>` |
| `effect fn f() -> Result[T, E]` | `fn f() -> Result<T, E>`（二重包装しない） |
| `fn f() -> T` | `fn f() -> T`（変換なし） |

### 変換の詳細 (pass_result_propagation)

1. 戻り値型を `T` → `Result<T, String>` に変換
2. body の tail expression を `Ok(...)` で包む
3. if/match の全分岐を再帰的に包む
4. 既に `ResultOk` / `ResultErr` の場合は包まない（二重包装防止）

## 4. fn main

| Almide | Rust codegen | 備考 |
|---|---|---|
| `fn main() = ...` | `fn main()` | 純粋。副作用なし |
| `effect fn main() -> Unit = ...` | `fn main() -> Result<(), String>` | 自動リフト。Termination trait で動作 |

### 仕様

- `effect fn main()` は `pass_result_propagation` により戻り値が `Unit` → `Result<(), String>` に自動リフトされる
- ユーザーが `-> Result[Unit, String]` を明示的に書いても動作するが、不要
- エラー時は `Err(msg)` で終了し、Rust ランタイムがメッセージを stderr に出力
- **main は引数を取らない。** コマンドライン引数は `process.args()` で取得する（Go 方式）

テスト: `spec/lang/result_option_matrix_test.almd`

```almide
effect fn main() -> Unit = {
  let args = process.args()
  let name = list.get(args, 1) ?? "world"
  println("Hello, ${name}!")
}
```

## 5. test ブロック

```almide
test "name" {
  assert_eq(f(), expected)
}
```

| 属性 | 値 |
|---|---|
| `is_effect` | `true`（effect fn の呼び出し可） |
| `is_test` | `true` |
| Result 包装 | なし（`-> ()` のまま） |
| `!` の挙動 | `.unwrap()`（`?` ではなく） |

test 内では `!` が `.unwrap()` に展開される。失敗時は panic でテスト失敗。

## 6. guard else

```almide
guard condition else { diverge_expr }
```

### 生成ルール

| コンテキスト | else が Unit/Break/Continue | else がそれ以外 |
|---|---|---|
| ループ内 | `if !(cond) { break }` or `continue` | `if !(cond) { return else_expr }` |
| 関数内 | N/A | `if !(cond) { return else_expr }` |

### else ブロックの型制約

`else_expr` の型は以下のいずれか:
1. 関数の戻り値型と一致（effect fn なら `Result<T, String>`）
2. `Never` 型（`process.exit()`, `panic()` 等）
3. `Unit`（ループ制御: break/continue に変換）

### ⚠ 現状の問題

- `process.exit()` の戻り値型が `-> !` (never) になったことで guard else での使用は修正済み
- ただし Almide の型システムに Never 型が存在しないため、型チェッカーレベルでの保証はない

## 7. テストで確認済みの動作

```
✅ effect fn から test 内で直接呼べる（test は is_effect=true）
✅ test 内で effect fn の結果に ! 不要（auto-unwrap ではなく test 特殊扱い）
✅ ? で Result → Option 変換
✅ ?? で Result ok/err の分岐
✅ ?? で Option some/none の分岐
✅ ?? で json.get() (Option) のフォールバック
✅ guard else { err("msg")! } で早期リターン
✅ effect fn が暗黙に Result 包装
```

## 8. 現状と仕様のギャップ

| # | 仕様 | 現状 | ステータス |
|---|---|---|---|
| 1 | `??` は Option/Result を正しく判別 | `Ty::Unknown` → Result 扱い | ⚠ 型推論が壊れると誤生成 |
| 2 | Never 型がある | `Ty::Never` 実装済み (v0.10.3) | ✅ |
| 3 | effect fn の戻り値変換は暗黙 | pass_result_propagation で変換 | ✅ 動作中 |
| 4 | test 内の `!` は `.unwrap()` | 実装済み | ✅ |
| 5 | guard else は Never/戻り値型を受け入れる | Rust codegen レベルで解決 | ⚠ 型チェッカーは未対応 |
| 6 | auto-unwrap は無効、`!` で明示 | コメントアウト済み | ✅ 意図的 |
| 7 | `effect fn main()` → `Result<(), String>` | Termination trait で動作 | ✅ |
| 8 | args は `process.args()` で取得 | `process.args()` 実装済み (v0.10.3) | ✅ |
| 9 | `?` 演算子のパース | `IdentQ` 廃止、`?` は常に独立トークン (v0.10.3) | ✅ |
| 10 | test 内で effect fn を `!` で呼ぶ | 不要（test は直接呼べる） | ✅ だが非直感的 |

## 9. 決定が必要な事項

### ~~9.1 `?` のパース~~ → 解決済み (v0.10.3)
`IdentQ` トークンを廃止。`?` は常に独立した後置演算子。`r?` は空白なしで動作する。

### 9.2 test 内での effect fn 呼び出し
- 現状: test は `is_effect=true` なので effect fn を直接呼べる。結果の型は T（Result 包装前）
- 問題: `!` を付けるとエラー（「String に ! は使えない」）
- 提案: test 内での effect fn 呼び出しは暗黙的に unwrap される仕様を明文化

### ~~9.3 Never 型~~ → 解決済み (v0.10.3)
`Ty::Never` を追加。bottom type として全ての型と互換。`process.exit()` は `Never` を返す。

### ~~9.4 `effect fn main(args: List[String])`~~ → 解決済み (v0.10.3)
main は引数を取らない。コマンドライン引数は `process.args()` で取得する（Go 方式）。
