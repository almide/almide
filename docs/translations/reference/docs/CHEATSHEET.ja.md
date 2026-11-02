<a id="almide-quick-reference-for-ai-code-generation"></a>

# Almide クイックリファレンス（AI コード生成向け）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CHEATSHEET.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CHEATSHEET.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CHEATSHEET.zh-CN.md)



> 原文：[CHEATSHEET.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md)。この訳は非規範的な、コミットに固定された読者向けスナップショットです。規範的な英語原文を正とし、この訳は仕様・契約・チェッカー・生成処理の入力を置き換えません。自動生成・自動同期は行いません。コード例とコメント、インラインコードは原文どおり保持しています。コード、動作、ターゲット間同等性、セキュリティ、時間・証明に関する主張を独立に実行・再検証したものではありません。

> 原文の不整合について：動的 json.* API を「29 関数」とする記述は、同じ固定版の json モジュール索引が 3 関数を示し、value.* が移動済み操作を説明することと一致しません。また「同義語はない」という助言と、list.length などの非推奨別名を含む生成索引も併存しています。本文はこれらを勝手に更新・統一せず保持しています。過去の名称・削除時期・ターゲット制限や証明の範囲も、原文時点の主張としてお読みください。

ファイル拡張子：`.almd`

<a id="file-structure"></a>

## ファイル構造
```
@dialect(3)          // optional: the language dialect this file was verified against
import <module>
// declarations...
```

関数が移動または削除される際は、呼び出し側に修正方法が伝わるよう古い関数に印を付けます。コンパイラは両方のシグネチャを比較し、名前の置き換えだけで修正が完了する場合にのみ `almide fix` に編集を提示します。

```almide
@deprecated(since = 3, use = "string.trim_start")   // renamed
@deprecated(since = 3, note = "unsound on surrogate pairs")  // removed, no successor
```

`@dialect(N)` は任意で、すべての要素より前に置きます。ファイルが最後にエラーなく検査された言語方言を記録するもので、リリース番号ではありません。epoch が進むのは、既存コードを壊し得る言語表面の変更時だけです（`proofs/dialect-epochs.toml` に各 epoch が壊したものを列挙）。コンパイラより古いスタンプには何も表示しませんが、新しいスタンプは `E051` です。ここではそのファイルを何も検証していないためです。スタンプは `almide check <file> --stamp` で記入・更新し、後戻りはしません。

<a id="project-layout-multi-file"></a>

## プロジェクト構成（複数ファイル）
1 ファイルを超えるプロジェクトではパッケージを作ります。
```
mypkg/
  almide.toml          # [package] name = "mypkg"
  src/
    main.almd          # entry: imports siblings via `import self.<name>`
    classifier.almd    # sibling module: bare fn / let / type
    bindings/
      python.almd      # nested namespace → mypkg.bindings.python
```
兄弟モジュールのインポート：
```
import self.classifier                          // → classifier.fn(), classifier.LET
import self.classifier.{classify, NUMBERS}      // selective: bare names
import self.classifier as cls                   // alias
```
`import x from "./x.almd"` は**書かないでください**。ファイルパスはサポートされません。常に `import self.<sibling>` を使います。

<a id="types"></a>

## 型
```
type Name = { field: Type, ... }                     // record
type Name = | Case1(Type) | Case2 | Case3{f: Type}  // variant (leading |)
type Name[A, B] = { first: A, second: B }            // generic (use [] not <>)
type Name = Type                                     // type alias (transparent)
type Name = Case1(Type) | Case2(Type)                // inline variant (no leading |)
type Handler = (String) -> String                    // function type alias
```

<a id="conventions"></a>

### 規約
```almide check
type Color: Eq, Repr = Red | Green | Blue   // convention after type name with :
```

**variant のシリアライズ — 推奨パターン。** variant 型がシリアライズ境界（JSON / ファイル IO / dojo フィクスチャ）を越える場合、手書きの `match` の連鎖ではなく `: Codec` を宣言して `Type.encode` / `Type.decode` を自動生成します。導出 Codec は外部タグ方式です。コンストラクター名が唯一のキーとなり、レコードペイロードはフィールド名を保持し、タプルペイロードは位置順の配列、unit ケースは `null` になります。
```almide check
type Event: Codec = | Click(Int, Int) | Scroll{dy: Int} | Quit
// Event.encode(Click(10, 20))    → {"Click": [10, 20]}
// Event.encode(Scroll { dy: 5 }) → {"Scroll": {"dy": 5}}
// Event.encode(Quit)             → {"Quit": null}
```
`: Codec` を省くのは、決してシリアライズしない variant（プログラムが書き出すことのない内部 enum）だけにしてください。それ以外はすべて有効にすべきです。往復変換コードの手書きは LLM が誤りやすいためです。

<a id="names-any-script"></a>

### 名前（任意の文字体系）
識別子は Unicode（UAX #31、NFC で保持）、キーワードは ASCII のままです。全角の `ａ`/`１` は正規化せず拒否します。大文字始まりは型側、小文字または大文字・小文字の区別がない文字（漢字/かな/한글）は値側です。区別のない名前をモジュールが型またはケースとして宣言した場合、そのモジュール内では型側になります。別モジュールからは修飾して書きます（`geo.住所`）。
```almide check
type 住所 = { 都市: String }

type 色 = | 赤 | 緑

fn 名前(c: 色) -> String = match c {
  赤 => "赤",
  緑 => "緑",
}

effect fn main() -> Unit = {
  let 場所 = 住所 { 都市: "東京" }
  println("${場所.都市} ${名前(赤)}")
}
```

<a id="built-in-types"></a>

### 組み込み型
- プリミティブ：`Int`、`Float`、`String`、`Bool`、`Unit`、`Path`
- コレクション：`List[T]`、`Map[K, V]`、`Set[T]`
- エラー：`Result[T, E]`（`ok(v)` / `err(e)`）、`Option[T]`（`some(v)` / `none`）
- Option の短縮形：すべての型位置で `T?` ≡ `Option[T]`（正規の表記。`almide fmt` がこれに正規化）

<a id="integer-literals"></a>

### 整数リテラル
```
255  1_000_000       // decimal, `_` separators anywhere between digits
0xFF  0x1_00         // hex     (0x / 0X)
0b1010  0b1111_0000  // binary  (0b / 0B)
0o17                 // octal   (0o / 0O)
```
基数によらず通常の `Int` 値です。算術は同じで、両ターゲットで同一です。数字のない接頭辞（数字なしの `0x`）はコンパイルエラーであり、黙って `0` にはなりません。

<a id="functions"></a>

## 関数
```
fn name(x: Type, y: Type) -> RetType = expr
fn name(x: Type) -> Int!                             // pure-fallible: sugar for Result[Int, String]
effect fn name(x: Type) -> T = expr                  // touches the world; a call is written name(x)! — see Effects
```

`=` の後の本体は、下の行がインデントされていても 1 つの式です。複数の文からなる本体には波括弧が必要です：`fn main() -> Unit = { ... }`（「ブロック」を参照）。

<a id="option-shorthand-t-adr-0010"></a>

### Option の短縮形 `T?`（ADR-0010）

`T?` ≡ `Option[T]` はすべての型位置で有効です（矢印をマークする `!` とは異なります）。`?` は直前の型アトムに結び付き、`->` を越えません。

```almide
fn first_even(xs: List[Int]) -> Int? = xs |> list.find((x) => x % 2 == 0)
fn or_zero(v: Int?) -> Int = v ?? 0            // parameter position
type Config = { port: Int? }                   // field position
fn cells(row: List[Int?]) -> Int = ...         // generic-arg position
f: (Int) -> Int?                               // fn slot: a fn RETURNING Option[Int]
on_tick: ((Int) -> Unit)?                      // optional fn VALUE needs parens
pair: (String, Int)?                           // Option of tuple
nested: (Int?)?                                // nested Option (`Int??` lexes as ??)
fn parse_opt(s: String) -> Int?!               // Result[Option[Int], String]
```

`T?` が正規の表記で、`almide fmt` は `Option[T]` をこれに書き換えます。

<a id="visibility-optional-prefix-before-fntype"></a>

### 可視性（fn/type 前の任意の接頭辞）
- `fn f()` — 公開（デフォルト）
- `mod fn f()` — 同一プロジェクト内のみ（Rust では `pub(crate)`）
- `local fn f()` — このファイル内のみ（非公開）
- 型も同じ接頭辞を取ります（`mod type T = …`、`local type T = …`）。単なる `type` は公開です。`pub type` はありません（E097：`pub` を削除）

<a id="modifiers-order-matters-localmod-effect-fn"></a>

### 修飾子（順序が重要）：`[local|mod]? effect? fn`

<a id="predicate-fn-emptyxs-listt---bool-bool-return-only"></a>

### 述語：`fn empty(xs: List[T]) -> Bool`（戻り値は Bool のみ）

<a id="hole--todo"></a>

### ホール / Todo
```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```
どちらも文脈が要求する型を取るため、プログラムの残りは型検査を続けられます。実行が到達すると、どちらも PANIC になります（native：終了コード 101、`not yet implemented: hole at line N` / `not yet implemented: <your message>`）。これは native 専用のワークフローです。構造的 wasm エミッターはこれらのノードをサポートしないため、残っているプログラムは拒否されます（`almide run --target wasm` は、非対応の形を示す診断とともに終了コード 1）。wasm 用にビルドする前にホールを埋めてください。

**呼び出し引数**の `_` は別物であり、拒否されます（E046）。
```
let v = add(_, 10)          // ✗ E046 — not partial application, no `_` sections
let add10 = (x) => add(x, 10)   // ✓ name the missing value with a lambda
```

<a id="mutable-parameters"></a>

### 可変パラメーター
```almide check
fn incr(mut x: Int) -> Unit = { x = x + 1 }

fn stamp(mut b: Bytes, v: Int) -> Unit = bytes.put_u8(b, 0, v)  // a helper that fills its caller's buffer

fn main() -> Unit = {
  var n = 5
  incr(n)          // n is now 6 -- mutated in place, not returned
  var buf = bytes.zeros(4)   // `var`, not `let`: the writers take `mut b`
  bytes.append_u8(buf, 7)
  stamp(buf, 9)
  println("${n} ${bytes.to_list(buf)}")
}
```
呼び出し側は `var` 束縛を渡す必要があります（`let`、非 `mut` パラメーター、一時値は E032）。`mut` は任意の位置の任意のパラメーターに付けられます。標準ライブラリのインプレース操作はすべてこの仕組みです（`list.push`、`list.pop`、`list.clear`、`map.clear`、`string.push`、`string.clear`、そして `bytes` 書き込みファミリー全体 — `push`、`append_*`、`put_*`、`fill`、`clear`、`copy_from`）。レシーバーは `mut` パラメーターなので、ローカルバッファは `var`、呼び出し元のバッファを書くヘルパーは `mut b: Bytes` を宣言します。それ以外は値の意味論で、`let c = a` はコピーし、呼び出された関数は通常のパラメーター経由で呼び出し元の束縛にアクセスできません。`mut` パラメーター付き関数は呼び出すもので、値としては使えません。`let f = incr` や `list.map(xs, incr)` は E096 です。関数型が書き戻しを表せないためです。`var` に対して呼び出してください（`incr(n)`、値としては `() => incr(n)`）。コールバックスロットではローカル `var` にコピーして返します：`list.map(xs, (item) => { var copy = item; incr(copy); copy })`。

<a id="effects-failure-and-"></a>

## 効果、失敗、`!`

<a id="effect-fn-and-the--at-its-call-sites-adr-0002-d6-adr-0008"></a>

### `effect fn` と呼び出し箇所の `!`（ADR-0002 §D6、ADR-0008）

`effect fn f() -> T` は外界に触れます（fs、http、process、random、io など）。ユーザー宣言の effect fn は**常に失敗可能**です。本体が失敗しなくても、呼び出しは `Result[T, String]` を返します。`-> T` が正規の表記です。`effect` がすでに失敗可能性を表しているためです。

純粋な `fn` でも `println`、`eprintln`、`panic`、`assert` / `assert_eq` / `assert_ne` を呼べます。これらは書き込みまたは中断を行いますが読み取らず、関数を `effect` にはしません（ADR-0022）。純粋な関数から他の効果（`fs`、`env`、`io.*`、`http`、`fan` など）を使うと E006 です。標準ライブラリの読み取り関数も effect fn です。`io.stdin()!` に対する `io.read` / `io.read_exact` / `io.read_to_end`、`env.args`、`args` の読み取り関数（`args.has_flag`、`args.option`、`args.positional`）は stdin または argv を読むため、`io.read_line` と同様に `effect fn` の呼び出し元が必要です（dialect epoch 8。純粋な関数からの呼び出しは E006）。`String?` を返すもの（`args.option`）では、デフォルトの前に `!` を置きます：`args.option("out")! ?? "out.txt"`。`!` はエラーを伝播し、`??` は none のデフォルトを与えます（#3522）。

effect 呼び出しの `!` は Swift の `await` のように読んでください。この特定の呼び出しが失敗し得るかに関係なく、外界に触れる地点を示します（Almide に `await` はなく、あくまで読み方です）。これは Swift の `try` であり、`try!` **ではありません**。外側の関数にエラーを伝播し、トラップしません。

| 呼び出し | 書き方 |
|---|---|
| ユーザー宣言の effect fn、または失敗し得る標準ライブラリ関数（`fs.read_text` など） | 伝播には `f()!`、意図的な破棄には `let _ = f()`、または `ok`/`err` に対する `match`。単なる `let x = f()` は E041、単独の文 `f()` は E042 |
| 失敗しない標準ライブラリ effect fn（`random.int`、`io.read_line`、`fs.exists` など） | 値は通常の値で、それへの `!` は無害な何もしない操作 |
| Almide で書かれ、通常の値を返す標準ライブラリ effect fn（`io.stdin()`、`io.stdout()`、`uuid.v4()`、`process.child_pid(c)` など） | ユーザー宣言の場合と同様、呼び出しは `Result` を返す。`!` を書く：`io.write(io.stdout()!, data)!` |

常に正しい 1 つの規則：**すべての effect 呼び出しに `!` を書いてください。** 標準ライブラリの effect fn が失敗するかを知らなくても付けられます。

```almide check
import fs
import random

effect fn roll() -> Int = random.int(1, 6)      // body cannot fail, but a call still yields a Result

effect fn main() -> Unit = {
  let a = roll()!                                // user-declared effect fn: `!` required
  let b = random.int(1, 6)                       // stdlib effect fn that cannot fail: plain Int
  let c = random.int(1, 6)!                      // ...and a `!` on it is a harmless no-op
  let _ = fs.remove("scratch.txt")               // discard an error on purpose
  println("${a} ${b} ${c}")
}
```

<a id="pure-fallible-marker---t-adr-0002-phase-1"></a>

### 純粋・失敗可能マーカー `-> T!`（ADR-0002 Phase 1）

`-> T!` は失敗し得る純粋な関数を宣言し、戻り型は `Result[T, String]` です。内部で `!` が伝播し（effect fn は不要）、末尾の値は自動的に `ok(...)` に持ち上がります。ペイロードを書いても、Result を明示しても、どちらも機能します。

```almide check
fn parse_port(s: String) -> Int! = int.parse(s)      // pass-through (already a Result)
fn double_port(s: String) -> Int! = int.parse(s)! * 2  // value tail — lifts into ok(...)
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! propagates in a T! body
  guard n > 0 else err("must be positive")
  n                                                  // value tail lifts (ok(n) also fine)
}
```

本体で `!` を使うラムダは、失敗可能なクロージャ `(A) -> Result[T, E]` になります（第一級の値。任意の list / map / set / option 高階関数のコールバックとして、`list.all(xs, (x) => p(x)!)!`、`map.fold`、`set.map`、`option.map` などで、高階関数を最初の err で止まる失敗可能な形にします。関数型スロットでは `(A) -> B!` または `(A) -> B!E` と書きます）。E は文脈、つまり型付きスロット `(A) -> B!E`、型付き `let`、結果の伝播先関数から決まり、それがなければ `!` オペランドの結合から決まります。共通の E はそのまま、不一致は String になります（ADR-0021）。test ブロックではラムダの `!` は通常の unwrap のままです。戻り値マーカーとしての `!` は関数宣言の戻り位置と関数型スロットにのみ書けます。

マーカーは型付きエラーも運びます（ADR-0012 D2）。名前付きエラー型 E に対して `-> T!E` ≡ `Result[T, E]` で、`T!` は引き続き `T!String` の短縮形です。`?` が先に結合するため、`-> B?!E` は `Result[Option[B], E]` です。伝播は変わらず、`!` は E を変換しません（ADR-0003/0004：メッセージテキストではなく構造で分岐）。E の不一致は検査エラーのままです。

```almide check
type ConfigError = | Missing(String) | BadValue(String)
fn load(path: String) -> String!ConfigError = {
  guard path != "" else err(Missing(path))
  "text"                          // value tail lifts into ok(...) with E=ConfigError
}
```

<a id="unwrap-operators"></a>

### unwrap 演算子
```
expr!              // unwrap Result/Option, propagate the failure (effect fn, or a pure fn returning Result/Option)
expr ?? fallback   // unwrap or use fallback value
expr?              // Result → Option (err → none)
expr?.field        // optional chaining (Option[Record] → Option[FieldType])
o?.x ?? default    // the idiom: read a field if present, else default (ADR-0005; `?.` is Option-only — on a Result write `(r?)?.x`)
```

各値レベルの演算子は、名前付き標準ライブラリ関数への糖衣展開です（ADR-0005）：`x ?? d` ≡ `option.unwrap_or_else(x, () => d)` / `result.unwrap_or_else(r, (_) => d)`（フォールバックは遅延評価）、`r?` ≡ `result.to_option(r)`、`o?.x` ≡ `option.map(o, (v) => v.x)`。すでに Option の値に対する `o?` は何もせず、警告します（E056）。

<a id="reading-a-file-that-may-not-exist-adr-0004-d4"></a>

### 存在しないかもしれないファイルの読み取り（ADR-0004 D4）

不存在はエラーではなく値です。エラーテキストで分岐しないでください。

```almide
let cfg = fs.read_text_if_exists(path)! ?? "default"
//  ok(none) = absent (missing parents too) / err = permission, IO — real failures
// family: read_text / read_bytes / read_lines + _if_exists
```

<a id="error-handling-doctrine-adr-0004"></a>

### エラー処理の原則（ADR-0004）

**`E` の選択は好みではなく、層の割り当てです。** `E = String` は**消去**層、つまり人間やモデルが読む*報告*チャネルで、デフォルトです。variant の `E` は**精緻化**層、つまり内容でプログラムの挙動を変える*分岐*チャネルです。そのコストに見合うのは、自らエラーを処理するモジュールやパッケージという**閉じた領域**内だけです。その領域を出ると variant は String に**降格**し、それは必ず `map_err` として見えます（変換フックがなく、暗黙には起こりません）。完全な規則：[docs/specs/result-option-effect.md §8.0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/result-option-effect.md)。

**エラーメッセージのテキストで分岐してはいけません**（`string.contains(e, …)`、`e == "No such file"`）。テキストは報告であり API ではなく、E035 が警告します。呼び出し側が失敗の*種類*で分岐する必要があるなら、variant エラー型を定義して構造に match する合図です。

```almide
type LoadError = | NotFound(String) | BadValue(String)

match load(p) {
  ok(v)               => v,
  err(NotFound(_))    => default_value,     // branch on structure
  err(BadValue(msg))  => process.exit(1),
}
```

種類によらないフォールバックならエラーを読まないでください：`load(p) ?? default`。

**エラーに文脈を加える** — 正規の表記です（別表記を考案せず、区切りは `": "`、末尾は `${e}` にします）：

```almide
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!
// chained calls read as the failure's story:
//   Error: starting server: loading config: No such file or directory

// deliberate replacement is spelled with the discard parameter:
fs.read_text(path) |> result.map_err((_) => "friendly message")
// forgetting ${e} with a NAMED parameter warns (E036) — the original error
// would be silently destroyed
```

<a id="built-in-protocols"></a>

## 組み込みプロトコル
Eq と Hash は自動です（コンパイラが型構造から導出）。注釈は不要です。
```
// Eq: all value types support == (except Fn)
let same = color_a == color_b  // just works
```
<a id="protocols-user-defined-conventions"></a>

### プロトコル（ユーザー定義の規約）
```
// Define a protocol
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
}

// Satisfy via convention methods -- no impl block, this is the only way.
// The checker validates the signature against the protocol (arity, types).
type GreetAction: Action = { greeting: String }
fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok(a.greeting)

// Use as generic bound
fn run_action[T: Action](action: T, ctx: Context) -> Result[String, String] =
  action.execute(ctx)
```
組み込み規約（Eq、Repr、Ord、Hash、Codec）もプロトコルです。

独自の型も同様に、通常のメソッド名（`__` 接頭辞は付けない）で、組み込みのハンドル・メッセージプロトコルを満たせます。

| プロトコル | 型が実装するメソッド |
|---|---|
| `Read` | `effect fn T.read(self, max: Int) -> Result[Bytes, String]` |
| `Write` | `effect fn T.write(self, data: Bytes) -> Result[Unit, String]` |
| `Close` | `effect fn T.close(self) -> Result[Unit, String]` |
| `HttpMessage` | `header`、`header_values`、`headers`、`body`、`body_bytes`、`set_header`、`set_body`、`set_body_bytes` |
| `Body` | `fn T.media_type(self) -> String`（文字列リテラル）、`fn T.to_text(self) -> String` |

すると `io.read` / `io.write` / `io.close`、`http.header` / `http.body`、`http.response` がその型を受け取れます。メディア型がスクリプトを実行する `Body`（HTML、SVG、XML）は、`media_type` に `@media_owns(...)` も宣言します。それを所有する依存先には、アプリケーションの `[trust] media` 許可が必要です（E118）。

最初のパラメーターには名前と型を明示しても（`a: GreetAction`）、単なる `self` としてもかまいません（`self: Self` の糖衣）。規約メソッドでは、どちらも `protocol { ... }` 宣言内と同様に宣言元の型へ解決します。変更を行うレシーバーは `mut self`（`mut self: Self` の糖衣）です。

<a id="generic-protocols-explicit-conformance"></a>

### ジェネリックプロトコル（明示的な適合）
プロトコルは型パラメーターを取れます。型が適合するのは宣言で指定する 1 つのインスタンス化で、ジェネリック関数は適用済みプロトコルで制約します。呼び出しはその型自身のメソッドを実行し、他を探しません。
```almide check
protocol Repository[K, V] {
  fn find(self, key: K) -> V?
  fn put(mut self, key: K, value: V) -> Unit
}

type User = { id: Int, name: String }
type Users: Repository[Int, User] = { rows: List[User] }

fn Users.find(self, key: Int) -> User? = self.rows |> list.find((u) => u.id == key)

fn Users.put(mut self, key: Int, value: User) -> Unit = {
  self.rows = (self.rows |> list.filter((u) => u.id != key)) + [value]
}

fn rename[R: Repository[Int, User]](mut repo: R, id: Int, name: String) -> Unit =
  repo.put(id, User { id: id, name: name })

fn lookup[K, V, R: Repository[K, V]](repo: R, key: K) -> V? = repo.find(key)

fn main() -> Unit = {
  var users = Users { rows: [] }
  rename(users, 1, "ada")
  println(lookup(users, 1).map((u) => u.name) ?? "-")
}
```
- 適合宣言と境界で、各パラメーターに型をちょうど 1 つ与えます。`Repository[Int, User]` とし、単なる `Repository` にはしません。
- 型ごと・プロトコルごとに適合は 1 つです。2 つ目のインスタンス化には型を包んでください。
- 実装はプロトコルメソッドの `effect` と `mut` に厳密に一致させます。
- 別モジュールのプロトコルは型と同様、モジュール付きで指定します：`[R: ports.Repository[K, V]]`、`type Mem: ports.Repository[Id, User]`。
- プロトコルは制約であって型ではありません。`List[Repository[Int, User]]` はエラーです。

<a id="expressions"></a>

## 式

<a id="if-an-omitted-else-returns-unit"></a>

### If（`else` を省くと Unit を返す）
```
if cond then expr else expr
if a then x else if b then y else z
```
**`else` のない `if` は Unit を返します。** 早期リターンには代わりに `guard` を使います。

<a id="match-exhaustive-supports-guards"></a>

### Match（網羅的、ガード対応）
```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

<a id="patterns"></a>

### パターン
```
_                          // wildcard (match only — NOT a valid variable name)
name                       // bind
ok(inner) / err(inner)     // Result
some(inner) / none         // Option
TypeName(args...)          // constructor
TypeName{ field1, field2 } // record pattern
literal                    // int, float, string, bool
a | b | c                  // or-pattern: any alternative matches (binder-free)
[h, ..t]                   // list-rest: len >= 1, t binds the tail (a list)
[a, b, ..]                 // rest ignored: len >= 2, rest slot is LAST-only
c @ Circle(r)              // as-pattern: c binds the whole, r the payload
```
**`_` は match パターン、`let _ = x`（破棄）、`for _ in xs`、ラムダ引数 `(_ ) => expr` に書けます。**

**パターンで非対応のもの：** 範囲パターン（`1..5`）はなく、リストの最後のスロット以外に rest は置けません（`[..t, x]` は拒否）。or パターンの選択肢（`"a" | "an" | "the" => …`）はアームの最上位で使えますが、変数を束縛できません。本体でペイロードが必要ならアームを分けてください。リスト rest（`[h, ..t]`）は対応しており、先頭と残りを分ける慣用的な方法です。

```almide check
fn sum(xs: List[Int]) -> Int = match xs {
  [] => 0,
  [h, ..t] => h + sum(t),
}

fn main() -> Unit = println(int.to_string(sum([1, 2, 3])))
```

<a id="lambda"></a>

### ラムダ
```
(x) => expr
(x, y) => expr
items.map((x) => x + 1)

// multi-line: use a block as the body
let f = (x) => {
  let y = x * 2
  y + 1
}

// a tuple parameter: destructure it with a pattern
pairs |> list.map(((a, b)) => a + b)
// map callbacks take the entry as ONE (K, V) tuple
map.filter(m, ((k, v)) => v > 0)
map.fold(m, 0, (acc, (k, v)) => acc + v)
// where the ONLY parameter is a tuple, its elements may be written as separate parameters
map.filter(m, (k, v) => v > 0)
```

**ラムダと効果**：ラムダは外側の関数の効果能力を継承します。これはすべての高階呼び出し先（`list.map` など）に共通の規則です。`effect fn` 内ではラムダも effect fn を呼べますが、その結果は**明示的な `Result` 値**のままです（自動 `?` はクロージャ境界を越えません）。`?? fallback` や `match` で処理するか、`!` で**ラムダ自身の**失敗チャネルへ伝播します。外側の関数には伝播しません。通常のラムダ内の `!` は失敗可能ラムダを推論し、末尾の値を `ok(...)` に持ち上げます。明示的な effect 型のスロットは不要です。test ブロックのラムダは test の unwrap 意味論を維持します。純粋な関数内ではラムダから effect fn を呼ぶとエラーですが、純粋な失敗可能ラムダは使えます。例外として、計量リージョン（`fan.bounded` / `fan.min` の本体）は設計上純粋なので、effect fn 内でも効果呼び出しは拒否されます。

**effect 関数型のスロット**（`effect (A) -> B`）：高階関数はコールバックが効果を実行すると宣言できます：`effect fn serve(port: Int, handler: effect
(HttpRequest) -> HttpResponse) -> Unit`。`effect (…) -> …` スロットに対して検査されるラムダは、effect fn 本体と同じ書きやすさを得ます。効果呼び出しが許され、`!` が伝播します（ハンドラー自身の失敗チャネルへ。失敗した `http.serve` ハンドラーは 500 レスポンスになります）。次の両表記を一様に受け付けます。

```
http.serve(8080, (req) => {
  let body = fs.read_text("index.html")!   // ← `!` works: the slot is effect-typed
  http.response(200, body)
})!
```

effect 関数型の値の呼び出しも効果呼び出しです（高階関数内の `h(x)!`）。純粋な関数から行うと E006 です。effect fn を引数として渡す `list.map(xs, fs.read_text)`、`apply(fs.read_text, p)` も効果呼び出しになります（通常の関数型パラメーターは透過的）ので、純粋な関数からは E006 です。レコードフィールド、リスト要素、effect fn が構築したクロージャなどの値経由で得た effect fn を純粋な関数が実行すると E109 です。効果ビットは値に付随するので、その関数を `effect fn` としてください（#4151）。通常の `(A) -> B` スロットは依然として失敗可能ラムダを拒否します（E005）。スロットを `(A) -> B!` または `effect (A) -> B` と宣言してください。単純な矢印形式が正規表記で、`almide fmt` は従来の `fn(A) -> B` をこれに正規化します。

名前付き関数もラムダと同様にスロット値になります（#1148）。値として参照された effect fn は `(A) -> Result[B, String]` クロージャです。束縛する（`let h: effect (String) -> Int = parse_pos`、注釈は任意）、パイプする（`(s |> h)!`）、effect スロットに直接渡す、のいずれも可能です。純粋な名前付き関数も effect スロットを満たします（結果を ok で包みます）。UFCS メソッド構文は関数値に適用されません（`x.h()` は E002。`h(x)` または `x |> h` と書きます）。

<a id="block-last-expression-is-the-value"></a>

### ブロック（最後の式が値）
```
{
  let x = 1
  let y = 2
  x + y
}
```

<a id="forin-loop"></a>

### For...in ループ
```
for x in xs {
  println(x)
}

for (k, v) in config {
  println(k + " = " + v)
}

for key in m {
  println(key)           // iterates keys only
}
```
<a id="while-loop"></a>

### While ループ
```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

<a id="range"></a>

### 範囲
```
0..<5           // [0, 1, 2, 3, 4]  (exclusive end)
1...5           // [1, 2, 3, 4, 5]  (inclusive end)
for i in 0..<n { ... }   // optimized: no list allocation
let xs = list.map(0..<10, (i) => i * i)  // range as List[Int]
```
反復回数が事前に分かる場合は `for i in 0..<n` を優先します（`var` で状態を引き継ぎます）。ループは `break` / `continue` に対応します。再帰的データや状態遷移では、アルゴリズムをより明確に表すなら再帰を使います。「✗ 回数指定の再帰」を参照してください。

<a id="pipe"></a>

### パイプ
```
text |> string.trim |> string.split(",")   // = string.split(string.trim(text), ",")
xs |> list.filter((x) => x > 0)            // the left side is always argument 1
```
パイプのプレースホルダーはありません。`xs |> list.filter(_, f)` は E046 なので `_` を除きます。別の引数位置にはラムダへパイプします：`x |> (v) => f(a, v)`。

<a id="record--spread"></a>

### レコードとスプレッド
```
{ name: "alice", age: 30 }
{ ...base, name: "bob" }
User { name: "alice" }     // named record construction — canonical form
User(name: "alice")        // alias: named-argument call syntax, normalized to the brace form
User("alice")              // WRONG: records take named fields, not positional (E021)
```
括弧の別表記は、通常のレコード型のマッチングにも使えます（`User(name) => ...`）。variant のレコードペイロードケースには**使えません**。`Circle { radius }`（`type Shape = Circle { radius: Float } | ...` のケース）は `Circle { radius }` でのみマッチし、`Circle(radius)` ではマッチしません（E021）。

<a id="list"></a>

### リスト
```
[1, 2, 3]
[]                         // empty list (there is NO list.new())
xs[0]                      // index read
xs[i] = value              // index write (var only)
xs |> list.find_map(f)     // first some(..) that f gives, or none — f is not called past the hit
```

<a id="map"></a>

### マップ
```
["a": 1, "b": 2]          // map literal
[:]                        // empty map (requires type annotation)
let m: Map[String, Int] = [:]
m["key"]                   // index read (returns Option[V])
m["key"] = value           // index write (var only)
```

<a id="string-interpolation"></a>

### 文字列補間
```
"hello ${name}, result=${1 + 1}"
```
数値、`Bool`、`String` は値を表示します。`List`、`Map`、`Set`、`Option`、`Result`、タプル、レコード、variant は Almide リテラル形式で表示します（`[1, 2]`、`["a": 1]`、`some(3)`、`P { n: 1 }`）。文字列表現のない値、つまり `Bytes`、`Unit`、`Matrix`、ハンドル、関数値、またはそれらを含むコンテナーは [E089](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/diagnostics/E089.md) です。意図するものを補間してください（`${bytes.to_list(b)}`、`${matrix.to_lists(m)}`、`${f(x)}`）。`fn T.repr` を宣言する型は、何を持っていてもそれを介して表示するため、関数フィールド付きレコードも宣言後は補間できます。

<a id="string-escapes"></a>

### 文字列エスケープ
```
"\n \t \r \\ \" \$"   // newline, tab, return, backslash, quote, dollar
"\x1b"                // \xNN  — two hex digits, codepoint 0x00..0xFF (ESC here)
"\u{1F600}"           // \u{…} — 1..6 hex digits, any Unicode scalar (😀)
```
不正な数値エスケープ（例：`\xzz`、`\u{}`）はリテラルのまま残ります。

<a id="heredoc-multi-line-strings"></a>

### ヒアドキュメント（複数行文字列）
```almide check
let sql = """
  SELECT *
  FROM users
"""
// Leading whitespace stripped based on minimum indent
// Interpolation ${expr} works the same
// Raw heredoc: r"""...""" (no escapes)
```

<a id="statements"></a>

## 文

<a id="top-level-let-module-scope-constant"></a>

### トップレベル let（モジュールスコープ定数）
```almide check
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```
トップレベル `let` はコンパイル時（const）、または `LazyLock` 経由（String のような非 const 式）で評価します。

<a id="let--var"></a>

### let / var
```
let x = 1                   // immutable
let x: Int = 1              // with type annotation
var y = 2                   // mutable
y = y + 1                   // reassign (var only)
f([]: List[Int])            // type ascription in call args
f([:]: Map[String, Int])    // typed empty map in call args
```

<a id="destructuring"></a>

### 分割代入
```
let { name, age } = user    // record destructure (1 level only)
var (x, y) = pair           // var takes the let patterns; every name is a var
```

<a id="processing-a-file-line-by-line-large-files"></a>

### ファイルを行単位で処理する（大きなファイル）

`fs.read_lines` はファイル全体を具体化します。小さいファイルには適しますが、大きいものではメモリの壁になります。大きなファイルの集計には `fs.fold_lines` を使います（メモリ O(最長行)、行の意味論は read_lines と同じ）。

```almide
// ✓ aggregate: fold_lines carries the accumulator through; split_once +
//   map.upsert is the hot-loop form (one lookup, no List, no re-split)
let stats = fs.fold_lines(path, map.new(), (acc, line) =>
  match string.split_once(line, ";") {
    some((key, _)) => map.upsert(acc, key, 1, (n) => n + 1),
    none => acc,
  })!

// ✓ side-effecting walk: for_each_line (callback may mutate captured vars —
//   but keep MAP accumulation on fold_lines: reading a Map captured in a
//   closure clones it per read)
var count = 0
fs.for_each_line(path, (line) => { count = count + 1 })!

// ✓ FALLIBLE body: the callback's `!` instantiates the fallible form here too
//   (first-err short-circuit, ADR-0006) — the callback is never called again
//   after the failing line. Same name, one extra `!`.
let totals = fs.fold_lines(path, map.new(), (acc, line) => add_row(acc, line)!)!

// ✗ avoid for large files: fs.read_lines(path)! |> list.fold(...)
```

分割走査（`fs.fold_lines_range` / `fs.fold_lines_chunked`）には失敗可能な形がありません。分割走査では最初の err が定義されないため、エラーを起こすチャンク本体が自身のエラーを処理します。

`result.collect` / `collect_map` は削除されました（ADR-0007。名前は Rust の最初の err での短絡を約束していましたが、逆をしていました）。partition ですべてのエラーを集めます。

```almide
let (oks, errs) = result.partition(results)
if list.is_empty(errs) then ok(oks) else err(errs)   // Result[List[T], List[E]]
```

<a id="guard-early-return--loop-break"></a>

### Guard（早期リターン / ループ脱出）
```
guard x > 0 else err("must be positive")
guard fs.exists(path) else err(NotFound(path))

// with block body:
guard not fs.exists(path) else {
  println("already exists")
  ok(())
}
```

<a id="concurrency--deterministic-time-fan"></a>

## 並行処理と決定論的な時間（fan）

すべての `fan.*` 形式には `effect fn` 文脈が必要です。Almide に `async`/`await` はありません。

```
// Dynamic mappers — a list + one callback returning Result (the mapper matrix
// covers every A→B pairing with A, B in {Int, Float, String}):
let results = fan.map(urls, (u) => http.get(u))!          // Result[List[B], String]: EVERY element runs, the lowest-index Err propagates
let report  = fan.settle(jobs, (j) => run(j))             // List[Result[B, String]]: EVERY element's Result, Errs captured
// The first Ok in LIST order is NOT a fan form: it is sequential, so it is list.find_map
// (`?` turns an element's Err into none, so the element is skipped; later elements never run)
let winner  = list.find_map(mirrors, (m) => fetch(m)?) ?? fb   // B: first Ok in LIST order; fb when every element fails
// The callback may be an EFFECT fn, in either spelling — an inline lambda that
// calls one, or a bare effect-fn value. Same rule as the block heads' arms.
let checked = fan.map(paths, read_meta)                   // read_meta: an `effect fn`, passed by name
// To stop at the FIRST failure instead, write a `for` loop with `!` — `fan.map` never stops early.
// A heavy PURE callback (over scalars, Strings or Lists) is parallelised (a thread per core, results in list
// order) ONLY by writing `fan.map` / `fan { list.map(...) }` — the compiler never threads
// an implicit `|>` chain (those are fused sequentially).
let sums = fan.map(chunks, (c) => ok(heavy(c)))!            // parallel natively; sequential and byte-identical on wasm

// fan { } — named arms run in parallel natively; the value is a record read by name
// (see the record form below). Unnamed arms give a tuple: `let (a, b) = fan { f()!, g()! }`
// Block heads — arms separated by `,` or newline (`fan { }` runs them in parallel natively;
// fan.settle / fan.min run them in source order)
// (`;` between arms is an error: it means sequencing, and stays legal only
//  INSIDE a block arm — `{ let x = f(); g(x) }`)
let all   = fan.settle { job_a(), job_b() }               // TUPLE of per-arm Results (heterogeneous arms allowed)
// The first Ok in SOURCE order is a `??` chain, not a fan form: the right side runs only
// when the left side failed, so later candidates never run once one succeeds
let first = fetch_a() ?? fetch_b() ?? fallback            // T: first Ok in SOURCE order (not the first to finish)

// fan.min(clock) — pick ONE arm by the clock you pass; the clock argument is REQUIRED
// (it says what "least" is measured on; `fan.min { 3, 5 }` would read as the smallest value)
let w = fan.min(compute.us(50)) { a(), b() } ?? -1          // Compute: the Ok that spent the LEAST deterministic compute; tie → source order; arms over budget are excluded; arms are PURE
// fan.min(duration.ms(n)) { … } — the first to FINISH in real time, arms may do I/O, the losers are
// cancelled at their next charge site or I/O boundary and awaited — is decided (ADR-0028) and lands
// in a follow-up (#3788); today it is a type error

// Budget: a deterministic compute-time limit on ONE body, built with compute.* constructors
let r = fan.bounded(compute.ms(100)) { work(input) } ?? -1   // Err if work exceeds 100ms of deterministic compute

// Mapper form: fan.min(clock, xs, f) races ONE pure lambda over a dynamic list (winner = cheapest, tie → list order)
let n = fan.min(compute.us(50), xs, (x) => ok(solve(x))) ?? fallback  // mapper returns Result: err(...) disqualifies; per-element budget

// Wall-clock deadline (oracle tier): checked cooperatively at charge sites
let t = fan.timeout(duration.ms(5000)) { work(input) } ?? -1 // Err if the wall deadline fires first (body pure today; an I/O body lands with #3788)
```

<a id="fan-record-form--parallel-fields-read-by-name"></a>

### fan のレコード形式 — 並列フィールドを名前で読む

`fan { name: expr, … }` はフィールドを並列の兄弟として実行し、匿名レコード `{ name: value, … }` を返します。`fan` を除いても出力と値は同じです（フィールドはソース順に表示）。失敗可能なフィールドには `!` が必要で、そのフィールドの Err がブロック全体の Err になります。すべてのフィールドは実行され、ソース順で最初の Err が採用されます。`!` のない Result フィールドは E041 です。ブロック内では兄弟フィールドはスコープ外です。

```almide check
effect fn add(a: Int, b: Int) -> Result[Int, String] = ok(a + b)

effect fn mul(a: Int, b: Int) -> Result[Int, String] = ok(a * b)

effect fn main() -> Unit = {
  let r = fan {
    sum: add(3, 4)!
    product: mul(3, 4)!
    label: "3 and 4"
  }
  println(r.label + ": " + int.to_string(r.sum + r.product))
  let { sum, product } = fan {
    sum: add(1, 2)!
    product: mul(1, 2)!
  }
  println(int.to_string(sum * product))
}
```

<a id="time-constructors-closed-set"></a>

### 時間コンストラクター（閉じた集合）

2 つの時計型に、それぞれ 6 単位：`ns / us / ms / s / min / h`。

```
compute.ms(100)     // Compute — deterministic compute-time (fan.bounded budgets, fan.min's deterministic clock)
duration.ms(5000)   // Duration — wall-clock time (fan.timeout deadlines; fan.min's real-time clock, #3788)
```

- 単なる `Int` は時間ではありません。`fan.bounded(5000) {...}` は型エラーで、`compute.ms(5000)` と書きます。
- `Compute` と `Duration` は混ぜられません。`fan.bounded(duration.ms(5)) {...}` は型エラーです。
- リテラル接尾辞はありません。`100ms` は解析できず、`compute.ms(100)` と書きます。
- 負の引数は実行時に中断します（`Error: negative time: ...`）。構築時のオーバーフローは最大値に飽和します。
- `fan.min(compute…)` / `fan.bounded` の結果は決定論的です。同じプログラム + 同じ入力 = すべてのターゲット・マシンで同じ勝者 / 判定。ただし計量器の同一バージョン内です（課金表と校正はバージョン管理され、変更はバージョンとともに告知）。
- 時計の型が階層を決めます。`compute.*` は純粋な本体で、どこでも同じ答え。`duration.*` はオラクル階層で、判定はホスト入力として ω とともに記録・再生します。
<a id="scoped--a-declared-reclamation-boundary"></a>

## scoped — 宣言された解放境界

`scoped { ... }` はブロックが割り当てるストレージの終端を宣言します。`scoped fn` は「リージョン内で実行可能」をワーカーのシグネチャの一部にします。どちらもヒントではなく検査される義務です。ブロックの値は本体の値で、許容部分集合外の形は両ターゲットで検査時に拒否されます。

```almide check
type Chain = Nil | Cons(Int, Chain)

scoped fn build(n: Int, acc: Chain) -> Chain =
  if n == 0 then acc else build(n - 1, Cons(n, acc))

scoped fn total(c: Chain, acc: Int) -> Int =
  match c {
    Nil => acc,
    Cons(h, t) => total(t, acc + h),
  }

fn sum_to(n: Int) -> Int = scoped { total(build(n, Nil), 0) }

fn main() -> Unit = println(int.to_string(sum_to(100)))
```

- 内部で許されるもの：スカラー（`Int`/`Float`/`Bool`/`Unit`）、同じファイルで宣言されたスカラーからなる variant 型、スカラーのレコード、それらのタプルと `Option`。他の `scoped fn`、コンストラクター、`int`/`float`/`math`/`bool` メンバーへの呼び出し。
- 境界を越えられるのはスカラーだけ。内部で構築した値は **E086**。
- `scoped fn` ではない呼び出し先、グローバル、`println`、ラムダ、キャプチャされたヒープ値、`!` / `?` / `guard` は **E087**。
- 1 回の非末尾自己呼び出し、または相互再帰は **E088**（アキュムレータを引き継ぐ。アームごとに 2 回自己呼び出しする木走査は許可）。
- `scoped` は文脈依存です。`scoped` という変数名も使え、`match scoped { ... }` は引き続きその値にマッチします。

<a id="test"></a>

## テスト
```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

<a id="testing-effect-fn-error-cases"></a>

### effect fn のエラーケースのテスト

test ブロック内では `effect fn` 呼び出しは `Result[T, String]` を返し、自動 unwrap はありません。値には `!` を使うか、`ok`/`err` を直接アサートします。

```almide check
effect fn validate(n: Int) -> Int = {
  guard n > 0 else err("bad")!
  n
}

test "ok value" { assert_eq(validate(5)!, 5) }          // explicit unwrap
test "ok result" { assert_eq(validate(5), ok(5)) }      // Result-aware
test "err" { assert_eq(validate(-1), err("bad")) }      // natural
```

<a id="testing-that-code-aborts"></a>

### コードが中断することのテスト

`testing.assert_aborts(expected, () => …)` はクロージャだけを子プロセス（native）または新しいインスタンス（wasm）で実行し、`expected` を含むテキストで中断したら成功します。対象は `panic`、範囲外インデックス、ゼロ除算、失敗した assert です。その後もテストは続きます。クロージャは呼び出し内に書くラムダで、test の何もキャプチャせず（E098）、呼び出しは `test` ブロック直下に置きます（E099）。`assert_throws` はありません（dialect epoch 14 で削除）。

```almide check
import testing

fn nth(xs: List[Int], i: Int) -> Int = xs[i]

test "aborts" {
  testing.assert_aborts("index out of bounds", () => nth([1, 2, 3], 5))
  testing.assert_aborts("PANIC: oh no", () => panic("oh no"))
  assert_eq(nth([1, 2, 3], 0), 1)
}
```

<a id="built-in-functions"></a>

## 組み込み関数
```
println(s)                 // print line to stdout
eprintln(s)                // print line to stderr
assert_eq(a, b)            // assert equal
assert_ne(a, b)            // assert not equal
assert(cond)               // assert true
panic(msg)                 // abort: message on stderr, exit 1
```
**`print` 関数はありません。** すべての出力（ユーザー向けエラーメッセージを含む）には `println` を使います。
6 つすべて純粋な `fn` から呼べます。出力と中断に `effect` は不要です。
`eprintln` はデバッグ / 内部エラー専用で、ユーザー向けメッセージには必ず `println` を使います。

<a id="stdin--parsing"></a>

### 標準入力と解析
```almide check
import io                                     // io is NOT auto-imported
effect fn main() -> Unit = {
  let line = io.read_line() ?? ""             // String? — none at end of input; it cannot fail, so NOT a Result
  let n = int.parse(string.trim(line)) ?? 0   // int.parse(s) -> Result[Int, String]
  println(int.to_string(n))
}
```
`io.read_line()` は末尾の改行を除いた標準入力 1 行を読み、`String?` を返します。入力終端で `none`、空行で `some("")` です。呼び出し元には `effect fn` が必要です。バイトにはハンドルを読みます：`io.read(io.stdin()!, n)!`、`io.read_to_end(io.stdin()!)!`。`int.parse` / `float.parse` は `Result` を返すので、`??`、失敗可能な文脈の `!`、または `?` で unwrap します。`int.from_string` はありません。

<a id="entry-point"></a>

## エントリーポイント
```
effect fn main() -> Unit = {
  let args = env.args()              // command-line args, without the program name
  let name = list.get(args, 0) ?? "world"
  let content = fs.read_text("config.txt")!   // propagate error with !
  println("Hello, ${name}: ${content}")
}
```
`effect fn main()` は `Result<(), String>` を返すよう自動で包まれます。`ok(())` や `-> Result[...]` は不要です。
`effect fn main()` と書くのは、`main` が実際の効果（読み取り、ファイル、ネットワーク、`fan`、`!`）を行う場合だけです。計算と表示だけの `main` は `fn main() -> Unit` です。

**一時ファイル**：`fs.create_temp_file(prefix)` / `fs.create_temp_dir(prefix)` で作り、`fs.temp_dir()` 配下の固定名は使わないでください。一時ディレクトリは共有なので、固定名は別ユーザーが先に作れます（あなたのファイルへのシンボリックリンクとして）。作成関数はランダム名を選び、排他的に作成し（既存エントリを再利用も追跡もしない）、本人だけに非公開とし、パスを返します。`fs.temp_dir()` は場所であって安全な名前ではありません。`/tmp` のハードコードも避けてください。Windows には存在しません。

```almide
let path = fs.create_temp_file("report")!
fs.write_text(path, data)!
```
コマンドライン引数は main のパラメーターではなく `env.args()` で取得します。プログラム名 argv[0] はリストに含まれません。

<a id="operators-precedence-highlow"></a>

## 演算子（優先順位：高→低）
`. () [] ! ? ?. ??`（後置）> `not -`（前置）> `>>` > `^`（累乗）> `* / %` > `+ -` > `..< ...` > `|>` > `== != < > <= >=`（非結合）> `and` > `or`

`^` は累乗です（右結合、`**` も可）。`+` は文字列とリストでは連結です（加算とオーバーロード）。XOR は `int.bxor(a, b)` で使えます。

`|>` は非対称です。右辺は 1 つの呼び出し / 合成チェーンなので、その後の演算子はパイプ結果に適用されます。`xs |> list.map(f) + ys` は `(xs |> list.map(f)) + ys` で、`xs |> f >> g` は合成 `f >> g` へパイプします。

<a id="ufcs"></a>

## UFCS
メソッド構文はレシーバーを第 1 引数として渡します。例えば `xs.map(f)` は `list.map(xs, f)` に解決し、通常のユーザー関数 `fn twice(n: Int) -> Int` は属性なしで `n.twice()` と呼べます。すべての `f(x, y)` と `x.f(y)` が無条件に等価なわけではありません。関数値レコードフィールド、レシーバーの標準ライブラリモジュール、規約 / プロトコル、通常のユーザー関数、型の定義モジュールの順に解決します。[UFCS の解決](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-frontend/src/check/calls_ufcs.rs)を参照してください。

<a id="standard-library-modules"></a>

## 標準ライブラリモジュール

現在の関数シグネチャと件数：[生成された標準ライブラリリファレンス](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)。以下の表は案内であり、2 つ目の API 一覧ではありません。

| モジュール | 説明 | インポート |
| --- | --- | --- |
| [string](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/string.md) | 文字列操作 | 自動インポート |
| [list](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/list.md) | リスト操作 | 自動インポート |
| [map](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/map.md) | マップ（辞書）操作 | 自動インポート |
| [set](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/set.md) | 集合操作 | 自動インポート |
| [deque](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/deque.md) | 両端キュー：両端で push/pop | 自動インポート |
| [heap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/heap.md) | 優先度付きキュー、最小が先 | 自動インポート |
| [int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int.md) | 整数算術とビット演算 | 自動インポート |
| [float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float.md) | 浮動小数点操作 | 自動インポート |
| [bool](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bool.md) | 真偽値の解析・比較・表示 | 自動インポート |
| [value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/value.md) | 動的値の操作 | 自動インポート |
| [result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md) | Result 型の操作 | 自動インポート |
| [option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/option.md) | Option 型の操作 | 自動インポート |
| [json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/json.md) | JSON の解析とクエリ | `import json` |
| [math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/math.md) | 数学関数 | 自動インポート |
| [regex](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/regex.md) | コンパイル済み `Regex` に対する正規表現（`regex.new(pat)` / `regex.compile(pat)`） | `import regex` |
| [datetime](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/datetime.md) | 日付と時刻 | 自動インポート |
| [compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/compute.md) | 決定論的な計算時間コンストラクター（fan の予算） | 自動（チェッカーの表面） |
| [duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/duration.md) | 実時間コンストラクター | 自動（チェッカーの表面） |
| [bytes](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bytes.md) | バイナリデータ | 自動インポート |
| [matrix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/matrix.md) | 2 次元行列操作 | 自動インポート |
| [testing](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/testing.md) | テストアサーション | `import testing` |
| [fs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/fs.md) | ファイルシステム | `import fs` |
| [env](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/env.md) | 環境とシステム | `import env` |
| [process](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md) | 子プロセス（`process.ProcessCommand` + `output` / `run` / `start`）、終了、シグナル | `import process` |
| [io](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md) | 標準ストリーム、任意のハンドルの `read` / `write` / `close` | `import io` |
| [net](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/net.md) | TCP、UDP、名前検索、IP アドレス | `import net` |
| [unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/unix.md) | Unix ドメインソケット、記述子受け渡し、共有メモリ | `import unix` |
| [uuid](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uuid.md) | RFC 9562 UUID：v4、v7、解析、比較 | `import uuid` |
| [http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/http.md) | HTTP クライアントとサーバー。`HttpRequest` と `HttpResponse` は `header` / `headers` / `body` / `set_header` / `set_body` を共有 | `import http` |
| [random](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/random.md) | 乱数生成。秘密には `random.bytes` を使用 | `import random` |

<a id="regex-compile-once-pass-the-regex-first"></a>

### 正規表現：一度コンパイルし、`Regex` を先頭に渡す

すべての `regex` 操作はパターン文字列ではなく `Regex` を取ります。`regex.new` はリテラルパターンを取り（`almide check` が検査、E102。拒否されたものは中断）、`regex.compile` は実行時に構築したものを取り `err(reason)` を返します。参加しなかったキャプチャグループは `none` です。

```almide check
import regex

fn main() -> Unit = {
  let digits = regex.new("[0-9]+")
  println(regex.find(digits, "order 66") ?? "none")
  println(regex.find_all(digits, "1 22 333") |> list.join(","))
  let date = regex.new("(\\d+)-(\\d+)(-x)?")
  let groups = regex.captures(date, "2026-10") ?? []
  println(groups |> list.map((g) => g ?? "-") |> list.join("|"))
  match regex.compile("a(b") {
    ok(_) => println("valid"),
    err(reason) => println(reason),
  }
  println(regex.replace_with(digits, "a1b22", (cs) => "<${(list.get(cs, 0) ?? none) ?? ""}>"))
}
```

<a id="json--the-wire"></a>

## JSON と通信形式

型付き Codec は、すべての JSON 作業のデフォルトです。動的 `json.*` API（29 関数）は探索とスキーマなしの透過処理専用です。形が分かるなら型を宣言してください。

**規則 1 — 通信型には通信形式の名前を使います。** 外部 JSON 形式を表す型は、Almide の snake_case 規約から外れていてもフィールド名をそのまま写します。型自体が通信形式の文書となり、覚え違えるリネーム層はありません。

```almide check
type OtlpSpan: Codec = {
  traceId: String,           // camelCase because the WIRE says traceId
  spanId: String,
  parentSpanId: Option[String],  // none = key omitted (proto3 unset)
  startTimeUnixNano: String,
}
// send:    http.request("POST", url, json.stringify(OtlpSpan.encode(s)), [("Content-Type", "application/json")])!
// receive: OtlpSpan.decode(json.parse(body) ?? value.null())
```
Almide 風の名前を必要とするドメインロジックは、独自の型を定義して通常のコードで写像します（コンストラクター呼び出し。チェッカーが全フィールドを検証）。フィールド別名 `name as "wire": T` は、Almide 識別子になれないキー（`@type`、`$ref`、`user-id`）にのみ使います。

**規則 2 — none はキーを省きます。** encode は `none` の Option フィールドを完全に省略し（`null` にしない）、decode は欠落キーまたは明示的 `null` を `none` に戻します。常に `decode(encode(x)) == x` です。`= default` 値を持つフィールドは引き続き出力します。デフォルト値はスカラー（`Int`/`Float`/`Bool`/`String`）、`Option[T]`、`List[<scalar>]` フィールドでサポートします。それ以外の型のデフォルトは、現在 native ビルドで拒否します（#1520 でリストファミリーを拡張）。これは proto3 JSON と一致するため、protobuf 定義 API（OTLP など）を直接扱えます。proto3 の `oneof` は、すべてが Option の対応型にすぎません。

```almide
type AnyValue: Codec = {
  stringValue: Option[String] = none,
  intValue: Option[String] = none,    // proto3 JSON: int64 rides as string
  boolValue: Option[Bool] = none,
}
AnyValue { stringValue: some("hi") }  // → {"stringValue":"hi"} — exactly one key
```

**規則 3 — `Value` は抜け道です。** `Value` フィールドは双方向にそのまま通します（ネストした文書、明示的 null）。`Option[Value]` は不存在と null が異なる唯一の場所です。欠落 → `none`、明示的 `null` → `some(value.null())`。RFC-7386 風パッチの意味論に使います。

**規則 4 — 外部 variant 形式には手書き codec を使います。** 導出 variant 形式は外部タグ方式です（`{"Click": {...}}`）。内部タグ方式（`{"type": "click", ...}`）の API では、ケースごとに Codec レコードを用意し、振り分けだけを手書きします。規約メソッドはすべての導出 Codec と自動で合成できます。

```almide check
type Click: Codec = { x: Int, y: Int }
type Scroll: Codec = { dy: Int }
type Event = | C(Click) | S(Scroll)

fn Event.encode(e: Event) -> Value = match e {
  C(c) => value.merge(Click.encode(c), value.object([("type", value.string("click"))])),
  S(s) => value.merge(Scroll.encode(s), value.object([("type", value.string("scroll"))])),
}
fn Event.decode(v: Value) -> Result[Event, String] = {
  let tag = value.as_string(value.get(v, "type")!)!
  match tag {
    "click"  => result.map(Click.decode(v), (c) => C(c)),
    "scroll" => result.map(Scroll.decode(v), (s) => S(s)),
    _ => err("unknown Event tag: ${tag}"),
  }
}
// Event now nests in derived Codec types like any other: { evt: Event, at: Int }
```
手書き codec には必ず往復テストを組み合わせます：`test "Event roundtrips" { assert_eq(Event.decode(Event.encode(C(Click{x:1,y:2}))), ok(C(Click{x:1,y:2}))) }`

**規則 5 — リクエストは入力型（`: Decode`）へデコードし、送信や保存にも使う型（`: Codec`）にはデコードしません。** `Codec` は `Encode + Decode` です。`: Decode` は `T.decode` と `T.decode_strict` だけを、`: Encode` は `T.encode` だけを導出します。`: Decode` のみの型は入力型で、全フィールドが入力なので、クライアントが設定してよいものだけを正確に列挙します。`http.decode_json[T](req)` が受け付けるのは入力型のレコードまたは variant のみです（`Codec` 型は E110 なので、`{"is_admin": true}` は `User` に届きません）。厳密にデコードし（どの深さでも `T` 外のキーは、`…, at address` のようなパス付きの `400 Bad Request: unknown key '<k>'`）、ハンドラーがコードでドメイン値を構築します。入力型にネストするレコードも入力型です。それ以外では `T.decode` は寛容なまま（未知のキーを無視）で、`T.decode_strict` はすべての深さで拒否します。

```almide check
import http
import secret

// Only what the client may set. Every field of a `: Decode` type is input.
// The password stays a Secret: it prints as Secret(***) in logs and asserts.
type SignupInput: Decode = { name: String, email: String, password: Secret[String] }
type User = { name: String, email: String, password: Secret[String], is_admin: Bool }

effect fn signup(req: HttpRequest) -> HttpResponse = match http.decode_json[SignupInput](req) {
  ok(i) => {
    let user = User { name: i.name, email: i.email, password: i.password, is_admin: false }
    http.response(201, user.name)
  },
  err(bad) => bad,   // unknown key, bad JSON, wrong field type → 400
}

// A type that must be Codec both ways (persisted, mirrored) keeps input off a
// field with @skip_decode (never read, still encoded) — it needs a default.
type Account: Codec = { name: String, @skip_decode is_admin: Bool = false }
```
`@skip` はフィールドを `decode` と `encode` の両方から除外します。フィールド属性は厳密に `@skip`、`@skip_decode`（および WGSL の `@builtin`/`@location`）です。serde / pydantic 風の表記（`@serde(skip)`、`@json_ignore`、`@redact`）はエラー E053 で、黙って無視しません。

**規則 6 — レスポンスの Content-Type はボディの型です。** `http.response(status, body)` は `Body` を取ります。String は `text/plain; charset=utf-8`、Bytes は `application/octet-stream`、`Value` は `application/json`、`html.*` は `text/html` を構築します。HTML は `html.escape`（テキスト）、`html.attr` / `html.url_attr`（属性）、`html.script_json`（`<script>` 内データ）で作り、`html.raw(s)` は信頼済みマークアップだけに使います。別のメディア型には型をリテラルで書いた `http.typed("text/csv", bytes)` を使います（計算された型、HTML / XML / SVG は E117。SVG は almide/svg などのパッケージから得て、アプリケーションが `almide.toml` の `[trust]
media = { "almide/svg" = ["image/svg+xml"] }` で許可します。それがなければ E118）。Content-Type は名前で書き込みません。`set_header` / `with_headers` は拒否し（E116）、`set_body` / `http.with_body(resp, body)` はボディと型を一緒に置き換えます。取得したレスポンスは到着時の型を維持します。

```almide check
import html
import http

fn greet(req: HttpRequest) -> HttpResponse = {
  let name = http.param(req, "name") ?? "guest"
  http.response(200, html.escape("Hello, ${name}"))
}

fn report(rows: String) -> HttpResponse = http.with_headers(200, http.typed("text/csv", bytes.from_string(rows)), [("X-Rows", "1")])
```

Codec のフィールド型は `String`/`Int`/`Float`/`Bool`/`Value`、他の Codec 型、それらを任意にネストした `Option`/`List` です（`List[Option[Int]]` は要素の null を保持し、`Option[List[T]]` は none のとき省略）。宣言時に拒否するものは、`Option[Option[T]]`（通信上区別不能）、フィールド位置以外の `Option[Value]`、通信形式にできない末端（Map、Bytes、タプル、サイズ指定整数、関数型、Result）です。これらは名前付き Codec 型に包むか、境界で変換します。

**秘密：`Secret[A]`（`import secret`）。** 型が repr を宣言しない限りすべての値は構造的に表示するため、通常の `String` フィールドのパスワードは `"${user}"`、`assert_eq` の失敗、`almide test` レポートに現れます。`Secret[String]` として保持してください。すべてのレンダラーは `Secret(***)` を表示し、外へ出す唯一の方法は `secret.reveal(s)` です。`Secret[String]` / `Secret[Bytes]` の `==` は定数時間で（他の `Secret` に `==` はなく E103）、Secret フィールドを持つレコードは読み取りのみで書き込みません。`: Decode` を宣言し（`: Codec` でも `decode` のみ導出し、`T.encode` は E023）、送信用 Value は `secret.reveal` を使って手で構築します。環境からのトークンは最初から Secret で、`env.secret(name)` は `Option[Secret[String]]` を返します。

```almide check
import json
import secret

type LoginInput: Decode = { user: String, password: Secret[String] }

effect fn main() -> Unit = {
  let input = LoginInput.decode(json.parse("{\"user\":\"ana\",\"password\":\"hunter2\"}")!)!
  println("${input}")   // LoginInput { user: "ana", password: Secret(***) }
  println(if input.password == secret.new("hunter2") then "ok" else "denied")
}
```

<a id="verifying-a-webhook-signature-hmac"></a>

## Webhook 署名の検証（HMAC）

`import hash` と `import secret` を使います。キーとタグは `Secret[Bytes]` です。キーは `Option[Secret[String]]` を返す `env.secret(name)!` で読みます。取り出した文字列をキーバイトへ変換する前に、不存在を処理します。`is_valid_` の項目で、キーを先頭にして検証します。HMAC-SHA256 を計算し、定数時間で比較します。タグ自身の `==` も定数時間なので（`Secret[Bytes]` のため）、比較のために 2 つのタグを `secret.reveal` してはいけません。また `hash.sha256(key + body)` も使いません（長さ拡張攻撃で偽造可能）。ヘッダーの 16 進数は `hex.decode` に通し、不正な 16 進数は空タグになって `false` と検証され、中断しません。署名時のヘッダー値は `hex.encode(secret.reveal(hash.hmac_sha256(key, body)))` です。

```almide check
import env
import hash
import hex
import secret

fn is_signed(key: Secret[Bytes], body: String, header: String) -> Bool = {
  let sig = string.strip_prefix(header, "sha256=") ?? header
  let tag = secret.new(hex.decode(sig) ?? bytes.from_list([]))   // bad hex -> empty tag -> false
  hash.is_valid_hmac_sha256(key, bytes.from_string(body), tag)
}

effect fn main() -> Unit = {
  match env.secret("WEBHOOK_SECRET")! {
    some(s) => {
      let key = secret.new(bytes.from_string(secret.reveal(s)))
      println("${is_signed(key, "{}", "sha256=00")}")
    },
    none => println("WEBHOOK_SECRET is not set"),
  }
}
```

自分で 2 つのトークンを比較するなら、両方を `Secret[String]`（`env.secret`、`secret.new`）で保持し、定数時間の `==` で比較します（C-426）。`hash.md5` / `hash.sha1` は標準ライブラリにありません。dialect epoch 15 で削除されたので `hash.sha256` を使います。

<a id="key-rules"></a>

## 重要な規則
- 改行 = 文の区切り（セミコロン不要）
- ジェネリクスは `[]` で、`<>` ではない
- `<` `>` は常に比較演算子
- 副作用には `effect fn`。`fn name!()` ではない
- すべての effect 呼び出しは `f()!` と書く。Swift の `await` のように読み、意味は Swift の `try` であって `try!` ではない
- 述語関数は `Bool` を返す（特別な接尾辞なし）
- 例外はなく、常に `Result[T, E]` を使う
- null はなく、`Option[T]` を使う
- 継承はなく、合成を使う
- マクロやユーザー定義の演算子宣言はない。組み込み演算子のディスパッチは型 / プロトコルで決まる。暗黙の数値変換はない
- 空リスト = `[]`、空マップ = `[:]`（型注釈付き）
- `_` はパターンや無視する束縛・引数のワイルドカード / 破棄、または式位置の型付きホールであり、通常の変数名ではない
- 上のモジュール表は案内のみ。公開 API 全体は[生成された標準ライブラリリファレンス](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)を使う
- 反復には `for x in xs { ... }` を使う
- **関数のネストは不可。** すべての `fn` はトップレベル。ローカルヘルパーにはラムダを使う
- **`let mut` は不可。** 可変束縛には `var`。`mut` 自体は本物のキーワードだがパラメーター修飾子のみ（`fn f(mut x: Int)`）。「関数」を参照
- Almide は Rust ではない。`&`、`trait`（`protocol` を使う）、`impl`（規約メソッド `fn Type.method(...)` を使う）はない。可視性は `local` / `mod`。`pub` はデフォルト公開の関数・束縛の任意の明示的同義語であり、型には使わない

<a id="naming-conventions-across-stdlib"></a>

## 標準ライブラリ共通の命名規約

- `bytes`：型付きバイト IO は 1 つの対応表です。`get_<t>(b, pos) -> T?` は位置で読み（範囲外は none）、`next_<t>(b, pos) -> (T?, next_pos)` は読んで進め（何も読めなければ位置はそのまま）、`put_<t>(b, pos, x)` はその場で上書きし（バッファ外のウィンドウは中断）、`append_<t>(b, x)` は拡張します。一括処理は `get_<t>_array(b, pos, count) -> List[T]?`。2+ バイトの全型には `_le` / `_be` が付き、`<t>` ∈ `u8|i8|u16|i16|u32|i32|i64|f16|f32|f64|bool` です（u64 はなく、`get_i64_*` が同じビットを読みます）。`bytes.new()` は空、`bytes.zeros(n)` は n 個のゼロバイトです。
- `matrix` は汎用の密行列操作を持ちます。ニューラルネットワークのカーネル（`gelu`、`softmax_rows`、attention、RoPE、norm、Q1_0/Q8_0、Qwen3 ブロック）は dialect epoch 14 以降、パッケージ `almide-graphics/nn` のモジュール `nn.kernels`、OpenAI / Anthropic のストリーミングクライアントはパッケージ `almide/llm` にあります（`import nn.kernels`、`kernels.gelu(m)`）。
- `matrix`：行単位の操作は `_rows` 接尾辞（`slice_rows`、`gather_rows`）、*1 行*を消費 / 生成する操作は単数 `_row`（`dot_row`、`broadcast_add_row`）、列操作は `_cols`（`split_cols_even`、`concat_cols`）です。
- `from_X` は別表現から構築し、`to_X` はその逆です（例：`from_bytes_f64_le` ↔ `to_bytes_f64_le`）。
- 数値変換は生成する型の名前です（`int`、`float`、`int8`…`int64`、`uint8`…`uint64`、`float32`、`float64`）。`T.from(x)` は任意の数値 `x` を取り、値が `T` に収まらなければ中断します（Float は先にゼロ方向へ切り捨て）。`T.from_checked(x)` → `Option`（`T` が x を厳密に保持できなければ `none`）。`T.from_saturating(x)` は範囲内に収めます（NaN は 0）。`T.from_wrapping(x)` は整数の下位ビットを保持します（2 の補数）。例：`float.from(n)`、`int.from_saturating(f)`、`uint8.from_wrapping(n)`。すべての整数の格納型に `min_value()`/`max_value()` があります。

<a id="common-mistakes-do-not"></a>

## よくある誤り（禁止事項）
- `list[1, 2, 3]` → **誤り**。`[1, 2, 3]` と書く。`list` はモジュールであり型コンストラクターではない
- `map(xs, f)` → **誤り**。`list.map(xs, f)` と書く。全標準ライブラリ関数にはモジュール接頭辞が必要
- `list.each(xs, f)` / `list.for_each(xs, f)` → **誤り**。通常の `each` はない。要素ごとの副作用には `for x in xs { ... }` を使う
- 値としての `map[K, V]` → **誤り**。空マップは型注釈付きの `[:]` で作る
- `List.new()` → **誤り**。`[]` と書く。List に `new()` はない
- マップとしての `{"a": 1}` → **誤り**。`["a": 1]` と書く。波括弧 `{}` はレコード / ブロック、角括弧 `[]` はリストとマップ
- `string.length(s)` → **誤り**。`string.len(s)` と書く。同義語はない
- `list.length(xs)` → **誤り**。`list.len(xs)` と書く。同義語はない
- `matrix.row_dot(m, r, v)` / `matrix.concat_cols_many(ms)` → **誤り**。`matrix.dot_row(m, r, v)` / `matrix.concat_cols(ms)` と書く。同義語はない
- `string.to_lowercase(s)` → **誤り**。`string.to_lower(s)` と書く。同義語はない
- `string.to_uppercase(s)` → **誤り**。`string.to_upper(s)` と書く。同義語はない
- `string.substring(s, i, j)` → **誤り**。`string.slice(s, i, j)` と書く。同義語はない
- x が Int のときの `println(x)` → **誤り**。`println(int.to_string(x))` と書く。暗黙変換はない
- `1 :: 2 :: []` → **誤り**。`[1, 2]` と書く。cons 演算子 `::` はない
- `fn foo<T>(x: T)` → **誤り**。`fn foo[T](x: T)` と書く。ジェネリクスは `[]` であり `<>` ではない
- `let mut x = 1` → **誤り**。`var x = 1` と書く。`mut` はパラメーター修飾子（`fn f(mut x: Int)`）のみで、束縛修飾子ではない
- `let err = f()` → **誤り**。`ok`/`err`/`some`/`none`（および先頭大文字形）は組み込み Result/Option コンストラクターで、予約語。束縛名を変える（`let msg = f()`）か、名前を保つならバッククォートでエスケープする（`let `err` = f()`）
- 関数内のネストした `fn` → **誤り**。すべての `fn` はトップレベル。ローカル関数には `let helper = (x) => ...` を使う
- `...` を含む `match x { ... pattern => expr }` → **誤り**。パターンにスプレッドはない
- トークン、キー、パスワード用のループ内の `random.int(0, 255)` → **誤り**。`random.int` / `float` / `choice` / `shuffle` は秘密用ではない。秘密には OS の暗号学的ソース `random.bytes(n)` を使う
- effect 呼び出しでの `try f()` / `try! f()` / `f()?` → **誤り**。`f()!` と書く。Almide の `!` は Swift の `try`（伝播）で、`try!`（トラップ）ではない。`?` は Result → Option 変換
- `async fn` / `await` → **誤り**。Almide に async/await はない。`fan { }` / `fan.map` / `fan.settle` / `fan.min(clock)` / `fan.bounded` / `fan.timeout` を使う
- `fan.race { … }` → **誤り**（dialect epoch 14 で改名、E027）。`fan.min(compute.ms(5)) { … }` と書く。時計引数は必須で、「最小」を何で測るかを示す
- `fan.any { a(), b() }` / `fan.any(xs, f)` / `fan.first_ok` → **誤り**（dialect epoch 14 で削除、E027）。順序内の最初の Ok は逐次であり fan ではない。`a() ?? b() ?? fallback`、リストなら `list.find_map(xs, (x) => f(x)?) ?? fallback` と書く
- `fan.min { a(), b() }` / `fan.min(xs, f)` → **誤り**。時計引数が必須：`fan.min(compute.ms(5)) { a(), b() }` / `fan.min(compute.ms(5), xs, f)`
- `fan.settle([...])` → **誤り**。サンクリスト形式は削除された。`fan.settle { a(), b() }` と書く
- `fan.bounded(100) {...}` / `fan.min(5000) {...}` → **誤り**。単なる Int は時間ではない。`compute.ms(100)` と書く
- `compute.msec(5)` / `compute.sec(5)` / `compute.m(5)` → **誤り**。単位集合は閉じている：`ns / us / ms / s / min / h`
- リテラルとしての `100ms` / `5s` → **誤り**。時間リテラルはない。`compute.ms(100)` / `duration.s(5)` と書く
- `let list = [1, 2]` の後の `list.len(list)` → **誤り**。ローカル束縛はメンバー位置でも同名モジュールを隠すため、これはローカルの `len` メソッドにレシーバーを第 1 引数、書いた引数を第 2 引数として渡すと解釈する（E004）。まだ必要なモジュールと同じ名を束縛に使わない。誤って書きがちなのは `list`、`string`、`map`、`set`。修正は 2 つあり、どちらも受け付ける。束縛を改名する（`let xs = [1, 2]` の後 `list.len(xs)`）か、引数を除いてローカルのメソッドを呼ぶ。`list.len()` は `2` と表示する。隠した名前はローカルそのものだから。自動インポートのモジュールは無言で隠れる。未使用として報告する import 文がなく、何も警告しない
- `io.read_line_opt()` → **誤り**（削除）。`io.read_line()` 自体が `String?` を返し、入力終端は `none`
- `io.read_byte()` / `io.read_n_bytes(n)` / `io.write_bytes(xs)` / `io.write(data)` → **誤り**（削除）。バイトはハンドル経由：`io.read(io.stdin()!, 1)!`、`io.read_exact(io.stdin()!, n)!`、`io.write(io.stdout()!, data)!`（`data: Bytes`）
- `net.tcp_read(s, n)` / `net.tcp_read_exact` / `net.tcp_write` / `net.tcp_close` / `net.tcp_close_listener` → **誤り**（削除）。`net.TcpStream` は io ハンドル：`io.read(s, n)!`、`io.read_exact(s, n)!`、`io.write(s, data)!`、`io.close(s)!`。`import net` に加えて `import io`
- `net.unix_connect` / `net.unix_send` / `net.shm_create` / … → **誤り**（移動）。`import unix`：`unix.connect`、`unix.write_fds`、`unix.select`（旧 `unix_wait`）、`unix.shm_create`。バイトは `unix.Fd` に対する `io.read` / `io.write` / `io.close` 経由
- `process.exec_status` / `exec_status_timeout` / `exec_in` / `exec_with_stdin` → **誤り**（削除）。1 つのレコード：`process.output(process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo", stdin: "", timeout_ms: 5000 })!` → `{code, stdout, stderr}`。`dir` / `stdin` / `timeout_ms` を省けるよう型名を書く
- `process.run(cmd, args)` / `process.run_in(dir, cmd, args)` / `process.exec_attached` → **誤り**。`process.run(process.ProcessCommand { cmd: "cargo", args: ["build"] })!` が終了コードを返す
- バックグラウンド子プロセス：`let c = process.start(process.ProcessCommand { cmd, args })!`、その後 `process.poll(c)!`（実行中は `none`）、`process.wait(c)!`、`process.stop(c, 500)!`、`process.child_pid(c)!`
- シェルを得るための `process.exec("sh", ["-c", "git status | head"])` は避ける。プログラムは argv リストを受け取るので、`process.exec("git", ["status"])!` とし、パイプ処理は Almide で行う

<a id="complete-example"></a>

## 完全な例
```almide check
import fs
import env

type AppError =
  | NotFound(String)
  | Io(String)

effect fn greet(name: String) -> Result[Unit, AppError] = {
  guard string.len(name) > 0 else err(NotFound("empty name"))
  println("Hello, ${name}!")
  ok(())
}

effect fn main() -> Result[Unit, AppError] = {
  let args = env.args()
  let cmd = list.get(args, 0) ?? "help"
  match cmd {
    "greet" => {
      let name = list.get(args, 1) ?? "world"
      greet(name)
    },
    "read" => {
      let path = list.get(args, 1) ?? "input.txt"
      let content = fs.read_text(path).map_err((e) => Io(e))!
      println(content)
      ok(())
    },
    other => {
      println("Usage: app <greet|read> [arg]")
      ok(())
    },
  }
}

test "greet succeeds" {
  assert_eq(string.len("hello"), 5)
}
```

<a id="common-mistakes-from-other-languages"></a>

## 他言語に由来するよくある誤り

Rust/Python/JS/Scala で学習した LLM が Almide を生成する際に最もよく起こす誤りです。

<a id="-if-cond-----if-cond-then--else-"></a>

### ✗ `if cond { }` → ✓ `if cond then ... else ...`
```
✗ if x > 0 { "positive" } else { "negative" }
✓ if x > 0 then "positive" else "negative"
```

<a id="-x--expr---x--expr"></a>

### ✗ `x => expr` → ✓ `(x) => expr`
ラムダパラメーターは**必ず**括弧で囲みます。
```
✗ list.map(xs, x => x + 1)
✓ list.map(xs, (x) => x + 1)
```

<a id="-modulefunc---modulefunc"></a>

### ✗ `module::func()` → ✓ `module.func()`
Almide のモジュールアクセスは `::` ではなく `.` です。
```
✗ list::len(xs)
✓ list.len(xs)
```

<a id="-listpushxs-item-for-value---xs--item"></a>

### ✗ 値を得るための `list.push(xs, item)` → ✓ `xs + [item]`
`list.push` は `var` を変更して `Unit` を返します。不変リストの構築には `+` を使います。
```
✗ some(list.push(stack, "("))     // returns Option[Unit]
✓ some(stack + ["("])             // returns Option[List[String]]
```

<a id="-let-x--e-in-body----let-x--e-body-"></a>

### ✗ `let x = e in body` → ✓ `{ let x = e; body }`
ML 風 let-in は非対応です。ブロックを使います。
```
✗ let y = f(x) in y + 1
✓ { let y = f(x); y + 1 }
```

<a id="-foldleft--foldright---listfold"></a>

### ✗ `foldLeft` / `foldRight` → ✓ `list.fold`
```
✗ xs.foldLeft(0, (acc, x) => acc + x)
✓ list.fold(xs, 0, (acc, x) => acc + x)
```

<a id="-char-type---string"></a>

### ✗ `Char` 型 → ✓ `String`
Almide に `Char` 型はありません。単一文字は `String` です：`"a"`、`"("`。

<a id="loop-control-break--continue"></a>

### ループ制御：`break` / `continue`
どちらもループ内で使えます。`break` は直ちに脱出し、`continue` は現在の反復の残りを飛ばします（範囲ループはインデックスを進めます）。
```
while cond {
  if done then break
  if skip then continue
  work()
}
```
再帰ヘルパーを使うのはアルゴリズムをより明確に表せる場合であり、ループ制御がないからではありません。

<a id="-counted-recursion-fn---1---for-_-in-0n"></a>

### ✗ 回数指定の再帰 `f(n - 1)` → ✓ `for _ in 0..<n`
反復回数が事前に分かるなら、範囲 `for` を書いて `var` で状態を引き継ぎます。ループ制御は `break` / `continue` を使います。再帰的データや状態遷移では、アルゴリズムをより明確に表す場合、再帰も引き続き有用です。
```almide
// ✗ correct (tail calls are optimized), but the shape says nothing a `for` would not,
//   reads as a stack risk, and drags `!` onto every iteration in an effect fn
fn fib(a: Int, b: Int, n: Int) -> Int =
  if n == 0 then a else fib(b, a + b, n - 1)
```
```almide check
// ✓ known bound: range for, state in var
fn fib(n: Int) -> Int = {
  var a = 0
  var b = 1
  for _ in 0..<n {
    let next = a + b
    a = b
    b = next
  }
  a
}

fn main() -> Unit = println(int.to_string(fib(10)))
```

<a id="-return-expr---just-expr"></a>

### ✗ `return expr` → ✓ 単に `expr`
ブロックの最後の式が戻り値です。`return` キーワードはありません。
```
✗ fn f(x: Int) -> Int = { return x + 1 }
✓ fn f(x: Int) -> Int = x + 1
```

<a id="-reassigning-function-parameters---var-copy"></a>

### ✗ 関数パラメーターへの再代入 → ✓ `var` コピー
通常の関数パラメーターは不変です。ローカル状態には `var` にコピーします。呼び出し元の `var` 束縛を更新すべき場合にのみ `mut` パラメーターを宣言します（「可変パラメーター」を参照）。
```
✗ fn f(n: Int) -> Int = { n = n - 1; n }
✓ fn f(n: Int) -> Int = { var m = n; m = m - 1; m }
```
