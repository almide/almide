# Result, Option, Effect — 完全仕様

> Last updated: 2026-08-13
> 2026-08-05 の 287 セル実測 matrix(#1122)に基づき全面改訂。設計判断の出典は
> ADR-0002〜0009。本文はすべて「動くコード」の記述であり、各節に検証テストを明記する。
> 2026-08-13: §8.0(二層モデル)と §9(非目標)を ADR-0012 D1/D4 から本文へ昇格。

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

### Option 糖衣 `T?`(ADR-0010)

`T?` ≡ `Option[T]`。**全型位置**で有効(`!` は戻り位置マーカーだが `?` は値の属性)。
`?` は直前の型アトム(名前+ジェネリクス、または括弧で閉じた型)に最結合し、
`->` をまたがない:

```almide
fn f(v: Int?) -> Int? = v          // 引数・戻りどちらも可
f: (Int) -> Int?                    // fn 型 slot: Option[Int] を返す fn
on_tick: ((Int) -> Unit)?           // fn 値そのものが Option — 括弧必須
pair: (String, Int)?                // Option[タプル]
nested: (Int?)?                     // 入れ子(`Int??` は ?? にレクスされ不可)
fn g(s: String) -> Int?!            // Result[Option[Int], String](? が先、! は戻りマーカー)
```

正準形は `T?`: `almide fmt` は `Option[T]` を `T?` へ正規化する(D3)。
stdlib も `fmt --no-import-edit` で正規化済み(2026-08-07 — 単一化の証人は
spec/lang/option_marker_test.almd が担う)。

### Never (bottom type)
```
process.exit(n) : Never   // 戻らない関数の戻り値型
```
Never はどの型にも代入可能。guard else, if then, match arm で使える。

### 可謬マーカー `-> T!`(ADR-0002 Phase 1 + 1b)

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
`!` 演算子は本体内で伝搬し、**値 tail は ok(...) に自動 lift** される
(Phase 1b — effect fn の lift と同じ人間工学。Result 型の exit は不変):

```almide
fn parse_port(s: String) -> Int! = int.parse(s)     // パススルー(既に Result)
fn double_port(s: String) -> Int! = int.parse(s)! * 2  // 値 tail → ok(...) に lift
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! が T! 本体で伝搬
  guard n > 0 else err("must be positive")
  n                                                  // 値 tail lift(ok(n) でも可)
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

### 伝搬は全明示(ADR-0008 — auto-`?` は削除済み)

かつて effect fn 本体には位置依存の暗黙伝搬(auto-`?`)があった(5 位置で効き、
5 位置で効かない・注釈やパターン形状で挙動が反転する)。**0.55 で機構ごと削除**:
失敗しうる呼び出しは**どの位置でも Result 値**を生み、伝搬したければ `!` を書く。

| 書き方 | 意味 |
|---|---|
| `let v = int.parse(s)!` | 伝搬(v は Int、err で早期 return) |
| `let r: Result[Int, String] = int.parse(s)` | Result 値を保持 |
| `let _ = fail()` | **意図的破棄** — err は伝搬しない(C-217) |
| `let v = int.parse(s)` | **E041**(旧・暗黙位置は全てエラー、`!` 挿入 hint 付き) |
| `fail()`(文の位置) | **E042**(must-use — `!` か `let _ =` の 2 綴りを hint) |
| `match get() { ok(v) => …, err(e) => … }` | Result は普通の値 — 普通に match |
| `match get()! { 42 => … }` | 値の層で match(`!` が層を明示) |

`e()?` / `e() ?? fb` は ADR-0005 の普通の値演算(特例則なし):
`e()?` ≡ Result→Option 変換、`e() ?? fb` ≡ unwrap_or_else。

回帰テスト: `spec/lang/explicit_propagation_test.almd`(2 ハザードの消滅 +
`let _` の A/B)、`spec/wasm_cross/let_wildcard_discard.almd`(C-217)、
`spec/wasm_cross/effect_option_explicit_bang.almd`(C-216)。

### lambda の可謬性(ADR-0006 D1 / ADR-0009 — #1108 Phase 2b)

lambda は「ミニ可謬 fn」— 本体の `!` は **lambda 自身の失敗チャネル**
(`Result[T, String]`)に落ち、クロージャ境界は越えない(#489 の不変条件は保存)。
使用駆動で可謬性が推論される(L1〜L9、2026-08-07 批准):

```almide
let g = (x) => int.parse(x)! * 2      // g: (String) -> Result[Int, String] — 第一級
g("21") ?? -1                          // 呼べば Result 値、普通に消費
list.map(xs, (s) => halve(parse(s)!)!) // 複合可謬 callback → first-err 形(L6)
fn retry(op: (Int) -> Int!) -> Int! = op(1)!   // fn 型 slot の `!`(L7/L8)
```

- E は String 固定(L3)・Option operand の none は err("none")(L4)・値 tail は
  ok(...) に lift(L5)
- **test ブロック内は例外**: lambda の `!` は unwrap のまま、HOF dispatch も
  総形のまま(L9 — test 世界は pre-#1108 意味論を丸ごと保持)
- 素の `(A) -> B` slot への bit 透過(user HOF)は未実装 — E005 が
  2 つの解決綴りを名指しする targeted hint を運ぶ(Phase 2b-iii)

テスト: `spec/lang/fallible_lambda_test.almd`(L1〜L9 のピン)

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

テスト: `tests/wasm_runtime_test_parts/p3.rs::unhandled_main_error_terminates_consistently`,
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

### 8.0 二層モデル — どちらの層に置くか(ADR-0012 D1)

エラー型 `E` の選択は好みではなく**層の割り当て**である。この段落が標準規則
であり、以前は ADR-0002(チャネル)/ 0003(変換)/ 0004(ドクトリン)/ 0012
(終状態)の四本を読まないと復元できなかった。

| 層 | `E` | 何のチャネルか | いつ選ぶか |
|---|---|---|---|
| **消去層(既定)** | `String` | **報告** — 人間と LLM が読む | 既定。呼び出し側が内容で分岐しない全ての場合 |
| **精製層(オプトイン)** | variant | **分岐** — プログラムが内容で挙動を変える | 「内容で分岐したい」とき(ADR-0004 D1)。かつ適用領域が**閉じている**こと |

- **消去層**の文脈前置は `map_err` の正準形、区切りは `": "`(ADR-0004 D2)。
- **精製層**の「閉じた領域」とは、そのエラーを**面倒を見きるモジュールまたは
  パッケージ**の内側を指す(SE-0413 の三条件と同じ範囲の取り方)。領域の外へ
  出す時点で variant を保つ理由は消える。
- **降格**(variant `E` → String)は境界で**必ず `map_err` として見える**
  (ADR-0003)。暗黙の変換フックは無いので、降格が起きた場所はコードに残る。

これは**終状態**として述べる。覆すには ADR-0004 / ADR-0012 の Falsifiers が
指名する**実測証拠**が要る — 新しい意見では足りない。

### 8.1 規則

- エラー値は String に収束する(タグ・error set は導入しない)
- **エラーメッセージの本文で分岐しない** — `string.contains(e, ...)` / 等値比較の
  分岐条件は E035 警告。分岐が要るなら variant E を定義して構造で match する
- `map_err` のラムダがエラー引数を使わないと E036 警告(意図的破棄は `(_) =>`)
- 文脈前置の正準形: `r |> result.map_err((e) => "context: ${e}")`(区切り `": "`)
- 全エラー収集は `result.partition`(`result.collect` / `collect_map` は E039 で
  非推奨 — 削除予定)
- fs の「不在」は値: `fs.read_text_if_exists(p)! ?? default`(C-215)

テスト: `tests/diagnostics/e035-*/`, `e036-*/`, `e039-*/`, `spec/stdlib/fs_if_exists_test.almd`

## 9. 非目標 — 検討して採らなかったもの(ADR-0012 D4)

以下は「まだやっていない」ではなく**採らない**と決めたものである。いずれも
2026 年の他言語の実測・実務報告を根拠に再確認されている(出典は ADR-0012 の
References)。ここに列挙するのは、同じ提案が周期的に戻ってくるためで、
戻ってきた提案は下の理由に対して反証を出す必要がある。

1. **stdlib の `E` は String のまま**。MoonBit の最新の推奨 parse API 自体が
   素の `raise` + メッセージ文字列であり、構造化するか否かは彼らのリポジトリで
   今も未決(core PR #3997)。Unison のエコシステムは包括的な `Failure` に収束した。
2. **変換フック(`From` 相当)も union / row / error set も導入しない**。
   Koka の row 推論は内部型変数名をユーザー向けエラーに漏らす(2025 年の実務
   報告)、Roc の open/closed union は同言語で最も難しい推論概念、Zig の error
   set は閉世界コンパイルに依存し、Rust の provider API は RFC 3885(2026-02)
   に置き換えられて破棄された。
3. **伝搬は明示 `!` のまま**(ADR-0008)。MoonBit は伝搬を暗黙にし、IDE の
   下線で補った —「IDE が throw しうる関数を自動で*下線*する」。その補償は
   **生テキストには存在しない**。生テキストは LLM と diff レビュアが持つ唯一の
   インタフェースである。
4. **ラムダの失敗チャネルは String のまま**(ADR-0009 L3)。使用箇所から `E` を
   推論すると Koka 級の「推論由来のエラーメッセージ」問題を輸入することになる。
5. **`main` の `E` は String のまま**。境界での降格 `map_err` が可視化点であり、
   そこを消すと降格が見えなくなる。

テスト: `spec/lang/fallible_marker_test.almd`, `tests/diagnostics/e035-*/`,
`e036-*/`(`T!E` の綴り自体の仕様は #1193 が入るまで書かない — 動くコードが
無いものは書かない、`docs/specs/CLAUDE.md`)
