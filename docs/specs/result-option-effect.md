# Result, Option, Effect — 完全仕様

> Last updated: 2026-08-06
> 2026-08-05 の 287 セル実測 matrix(#1122)に基づき全面改訂。設計判断の出典は
> ADR-0002〜0009。本文はすべて「動くコード」の記述であり、各節に検証テストを明記する。

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

### 可謬マーカー `-> T!`(ADR-0002 Phase 1)

fn 宣言の戻り位置に限り、`T!` は `Result[T, String]` の糖衣。可謬性(fallibility)と
効果(effect)は直交する 2 軸であり、4 象限すべてが綴れる:

```almide
fn        f() -> Int      // pure ・総
fn        f() -> Int!     // pure ・可謬(= Result[Int, String])
effect fn f() -> Int      // effect・総(暗黙 lift、§3)
effect fn f() -> Int!     // effect・可謬
```

E は常に String(ADR-0002 D2)。カスタムエラー型は明示の `Result[T, MyError]` で綴る。
`!` マーカーは fn 宣言の戻り位置**のみ**で合法 — let 注釈などでは parse エラー。
本体は Result を直接書く(パススルー / ok / err)。`!` 演算子は本体内で伝搬する:

```almide
fn parse_port(s: String) -> Int! = int.parse(s)     // パススルー
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! が T! 本体で伝搬
  guard n > 0 else err("must be positive")
  ok(n)
}
```

テスト: `spec/lang/fallible_marker_test.almd`

## 2. 演算子

すべての値レベル演算子は名前付き stdlib 関数への脱糖として定義される(ADR-0005)。
定義表:

```
x ?? d      ≡  option.unwrap_or_else(x, () => d)   // Option operand
r ?? d      ≡  result.unwrap_or_else(r, (_) => d)   // Result operand
r?          ≡  result.to_option(r)
o?.x        ≡  option.map(o, (v) => v.x)
```

`!` と effect fn の auto-`?` は制御フロー(早期 return)であり、関数実体を持たない
(ADR-0005 の明示的な境界)。

### `expr!` — unwrap with propagation

| 入力型 | 出力 | 失敗時 |
|---|---|---|
| `Result[T, E]` | `T` | err を囲む関数の失敗チャネルへ伝搬 |
| `Option[T]` | `T` | `err("none")` へ変換して伝搬(無損失の埋め込み — ADR-0003 D3) |
| test 内 | `T` | unwrap(panic でテスト失敗) |

`!` が合法な文脈(**effect fn 専用ではない** — C-211):

1. effect fn の本体
2. test ブロック
3. **pure fn で戻り値型が Result / Option / `T!` に解決されるもの**

それ以外の文脈は `E022`。Option/Result でない被演算子(`5!`)は `E034`。
伝搬点で E は変換されない — 不一致は check エラー(ADR-0003。実装 #1103)。
E が一致していればカスタムエラー型も構造のまま伝搬し match 可能(ADR-0003 D1)。

```almide
fn twice(s: String) -> Result[Int, String] = ok(int.parse(s)! * 2)   // pure + !
```

テスト: `spec/lang/unwrap_operators_test.almd`, `spec/lang/fallible_marker_test.almd`,
`spec/wasm_cross/pure_bang_propagation.almd`

### `expr?` — Result → Option 変換

| 入力型 | 出力型 |
|---|---|
| `Result[T, E]` | `Option[T]`(err → none。E は破棄) |
| `Option[T]` | `Option[T]`(恒等 — 診断なし) |

変換は**外側 1 層のみ**: `Result[Result[T, E], E]?` は `Option[Result[T, E]]`。
flatten はしない(ネストの平坦化は `result.flatten` / `option.flatten` の領分)。
Option/Result でない被演算子は `E034`。

### `expr ?? fallback` — unwrap with fallback

| 入力型 | 出力型 |
|---|---|
| `Result[T, E]` | `T`(err で fallback。E は破棄) |
| `Option[T]` | `T`(none で fallback) |

- **遅延評価**: fallback は none / err のときだけ評価される
  (`o ?? expensive()` は o が some なら expensive() を呼ばない)。
  定義 `≡ unwrap_or_else`(ADR-0005)の帰結。
- **fallback の型は unwrap 後の T と check 時に unify される**(不一致は E001 —
  `n ?? "hello"`、`n ?? some(1)` は check エラー)。
- **優先順位は最高位(後置族)**: `a ?? 1 + 2` は `(a ?? 1) + 2`。
  **C 系の `?:`(ほぼ最下位)と逆**なので、算術と混ぜるときは括弧を推奨。
- **チェーンは右入れ子**: `a ?? b ?? c` は `a ?? (b ?? c)`(AST 実測)。
  意味は「最初の成功が勝つ」。
- **行またぎ禁止(E038)**: 文レベルでは fallback は `??` と同じ行に置く。
  複数行は括弧 + `??` 後置:
  ```almide
  let v = (int.parse(s) ??
    -5)
  ```
- Option/Result でない被演算子は `E034`。

テスト: `spec/lang/unwrap_operators_test.almd`,
`tests/diagnostics/e038-qq-line-crossing/`, `tests/diagnostics/e034-off-type-bang/`

### `expr?.field` — optional chaining(Option 専用)

```almide
fn getx(o: Option[P]) -> Int = o?.x ?? 0    // some(P{x:5}) → 5、none → 0
```

`Option[レコード]` のフィールドへ安全にアクセスし `Option[フィールド型]` を返す。
**Option 専用** — Result に付けると専用診断で拒否(Result からは `(r?)?.x` と合成する)。

テスト: `spec/lang/unwrap_operators_test.almd`

## 3. effect fn

### 宣言と lift

```almide
effect fn read_file(path: String) -> String = fs.read_text(path)!
```

| Almide | Rust 生成 |
|---|---|
| `effect fn f() -> T` | `fn f() -> Result<T, String>`(暗黙 lift) |
| `effect fn f() -> Result[T, E]` | `fn f() -> Result<T, E>`(二重包装しない) |
| `fn f() -> T` | `fn f() -> T`(変換なし) |

### auto-`?` の位置マトリクス(現行実装の実測)

effect fn 本体では、失敗しうる呼び出しの暗黙伝搬(auto-`?`)が**位置により**効く:

| 暗黙伝搬が効く | 効かない(明示 `!` が必要) |
|---|---|
| 文の位置(`fail()` 単独) | 関数の引数(`double(get())` → E005) |
| 注釈なし let | パイプ段(`get() \|> double` → E005) |
| if 条件 | record フィールド(→ E001) |
| match(値パターンの腕のとき) | リスト要素(→ E001) |
| 文字列補間(呼び出し形) | タプル成分 |

注釈・パターン形状で挙動が変わる点に注意:

```almide
let r = int.parse(s)                          // 暗黙伝搬(r は Int、err で早期 return)
let r: Result[Int, String] = int.parse(s)     // 保持(r は Result 値)
match get() { ok(v) => ..., err(e) => ... }   // Result のまま受ける
match get() { 42 => ..., _ => ... }           // 伝搬が差し込まれる
```

> この位置依存の暗黙は **ADR-0008 で廃止決定済み**(伝搬は `!` 全明示へ、警告窓 #1123)。
> 本節は移行完了まで現行実装を記述する。

### lambda 境界(#489 / #1051)

lambda は囲む fn の効果資格を継承するが、auto-`?` はクロージャ境界を**越えない**:
effect fn 内の lambda が effect fn を呼んだ結果は明示の Result 値であり、
lambda 内の `!` は E022(`??` / match で処理するか、呼び出しを lambda の外へ)。

テスト: `spec/lang/result_option_matrix_test.almd`, `spec/stdlib/` の effect 系各種

## 4. fn main

| Almide | Rust codegen | 備考 |
|---|---|---|
| `fn main() = ...` | `fn main()` | 純粋。副作用なし |
| `effect fn main() -> Unit = ...` | `fn main() -> Result<(), String>` | 自動リフト |

- **main は引数を取らない。** コマンドライン引数は `process.args()`(Go 方式)
- 未処理の Err が main に到達したら、**両ターゲットで exit 1 + stderr `Error: <msg>`**
  (Display、引用符なし)。`!` による Option の none 由来(`err("none")`)も同じ経路で
  両ターゲット一致(旧記述の「wasm が exit 0 で黙殺する残存乖離」は解消済み — 実測で
  両ターゲット exit 1 + `Error:` を確認)

テスト: `tests/wasm_runtime_test.rs::unhandled_main_error_terminates_consistently`,
`spec/lang/result_option_matrix_test.almd`

## 5. test ブロック

```almide
test "name" {
  assert_eq(f(), expected)
}
```

| 属性 | 値 |
|---|---|
| `is_effect` | `true`(effect fn を呼べる) |
| Result 包装 | なし |
| effect 呼び出しの結果 | **明示の Result 値**(auto-`?` は test では効かない) |
| `!` の挙動 | unwrap(失敗は panic でテスト失敗) |

test 内で effect fn を呼ぶと結果は Result のまま返る — `!` で unwrap するか、
`??` / match で処理する(「test では ! 不要」という旧記述は誤り)。

`assert_eq` の失敗出力は現状 Rust debug 形(`left: Ok(1) / right: Err("x")`)。
almide repr 形(ok/err)への統一は未実施(既知の表示形差)。

テスト: `spec/lang/expr_test.almd` ほか全 `*_test.almd`

## 6. guard else

```almide
guard condition else { diverge_expr }
```

- 条件は Bool に constrain される
- `else_expr` の型は **check 時に検査される**(#1118):
  1. 関数の戻りチャネルと unify(pure は宣言型、effect fn は lift 後の Result も可)
  2. `Never`(process.exit / panic 系)
  3. ループ内の `break` / `continue`
  型不一致(`-> Int` fn での `else "nope"` / `else { println(..) }`)は E001。
- 定数条件でも else の効果は失われない(`guard false else process.exit(3)` は
  exit 3 — #1117 で miscompile を修正、`tests/guard_exit_gate_test.rs` が絶対値で固定)

テスト: `spec/lang/fallible_marker_test.almd`(guard else err)、
`tests/guard_exit_gate_test.rs`

## 7. match の網羅性

Result / Option を含む match の腕の欠落(err / none 腕なし)は**ハードエラー E010**
(警告ではない)。腕追加の hint 付き。

テスト: `tests/diagnostics/`(E010 系 fixture)

## 8. エラー値のドクトリン(ADR-0004)

- エラー値は String に収束する(タグ・error set は導入しない)
- **エラーメッセージの本文で分岐しない** — `string.contains(e, ...)` / 等値比較の
  分岐条件は E035 警告。分岐が要るなら variant E を定義して構造で match する
- `map_err` のラムダがエラー引数を使わないと E036 警告(意図的破棄は `(_) =>`)
- 文脈前置の正準形: `r |> result.map_err((e) => "context: ${e}")`(区切り `": "`)
- 全エラー収集は `result.partition`(`result.collect` / `collect_map` は E039 で
  非推奨 — 削除予定)
- fs の「不在」は値: `fs.read_text_if_exists(p)! ?? default`(C-215)

テスト: `tests/diagnostics/e035-*/`, `e036-*/`, `e039-*/`, `spec/stdlib/fs_if_exists_test.almd`
