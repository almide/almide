> 最終更新：2026-10-18

<a id="almide-language-specification"></a>

# Almide 言語仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](language.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](language.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](language.zh-CN.md)

> 非規範の固定読者用翻訳。固定原文を正本とし、自動同期や独立した実行検証を意味しない。原文の歴史的な日付・主張を保持するもので、新たな適合性認証ではない。原文の差異も保持する。ここでは任意の module 宣言を示す一方、module-system は宣言不要とする。§7.4 は §5.9 内に置かれ、fan の現在・将来の時間形式の説明は原文ページ間で異なる。組み込み + の多重定義と演算子オーバーロード禁止の両記述を、どちらも一般化せず保持する。原文は Unit を zero-value type と記述する。§4.6 のトップレベル let の const/LazyLock による評価の説明も保持する一方、ALS-T7 は main より前の起動時に宣言順で評価すると規定する。この原文間の差異はここでは解消しない。以下のテストや安全境界は原文の記述であり、新たに実行したテストや独立監査ではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/language.md)


拡張子：`.almd`

---

<a id="1-file-structure"></a>

## 1. ファイル構造

Almide のソースファイルは次の三つの部分をこの順で持つ：

1. **モジュール宣言**（任意）— `module <path>`
2. **インポート** — 0 個以上の `import` 宣言
3. **宣言** — 関数、型、トップレベル let、プロトコル、テスト

```almide project
// file: mylib/src/core.almd
type Parser = { pos: Int }
type Lexer = { src: String }
fn new_parser() -> Parser = Parser { pos: 0 }
// file: main.almd
module myapp.utils

import fs
import json
import mylib.core.{ Parser, new_parser }

type Config = { port: Int, host: String }

fn default_config() -> Config = Config { port: 8080, host: "localhost" }
fn fresh() -> Parser = new_parser()

test "default config" {
  let c = default_config()
  assert_eq(c.port, 8080)
  assert_eq(fresh().pos, 0)
}
```

<a id="imports"></a>

### インポート

```almide project
// file: mylib/src/core.almd
type Lexer = { src: String }
fn lex(s: String) -> Lexer = Lexer { src: s }
fn version() -> String = "1"
// file: main.almd
import fs                           // simple module import
import mylib.core                   // dotted path → core.lex(...)
import mylib.core.{ lex, Lexer }    // selective import → lex(...)
import mylib.core as mc             // aliased import → mc.version()

test "every import form resolves" {
  assert_eq(core.lex("a").src, "a")
  let l: Lexer = lex("b")
  assert_eq(l.src, "b")
  assert_eq(mc.version(), "1")
  assert_eq(fs.exists("/nonexistent/almide-doctest"), false)
}
```

選択的インポートは `import path.{ Name1, Name2 }` 構文を使う。別名には `as` を使う。

テスト： `spec/lang/import_test.almd`

<a id="identifiers"></a>

### 識別子

識別子は UAX #31 Default Identifiers に従う。最初は XID_Start 文字または `_`、以降は XID_Continue 文字である（Rust RFC 2457、Python PEP 3131、ECMAScript の規則）。名前は NFC で保存するため、`é` と `e` + U+0301 は同じ名前になる。互換形が異なる文字（全角 `ａ`、`１`、合字、丸囲み文字）は識別子文字ではなく、代わりに書く通常表記とともに報告する。キーワードは ASCII。

先頭が大文字なら型側の名前（型、バリアントケース、コンストラクタ）、小文字または大小のない文字（漢字、かな、한글）なら値側の名前となる。モジュールが `type` / `protocol` またはバリアントケースとして宣言する大小のない名前は、`Red` と同じくモジュール全体で型側となり、別モジュールからは修飾して書く（`geo.住所`）。

```almide
type 住所 = { 都市: String, 番地: Int }

type 色 = | 赤 | 緑

fn 色名(c: 色) -> String = match c {
  赤 => "red",
  緑 => "green",
}

effect fn main() -> Unit = {
  let 場所 = 住所 { 都市: "東京", 番地: 3 }
  println("${場所} ${色名(緑)}")
}
```

テスト： `spec/lang/unicode_identifier_test.almd`,
`spec/integration/caseless_imported_types/`,
`spec/wasm_cross/unicode_identifiers.almd` (C-387),
`tests/unicode_identifier_nfc_test.rs`

---

<a id="2-types"></a>

## 2. 型

<a id="21-primitive-types"></a>

### 2.1 プリミティブ型

| 型 | 説明 |
|------|-------------|
| `Int` | 64 ビット符号付き整数 |
| `Float` | 64 ビット浮動小数点 |
| `String` | UTF-8 文字列 |
| `Bool` | `true` または `false` |
| `Unit` | ゼロ値型、表記は `()` |
| `Path` | ファイルパス |
| `Bytes` | バイト列 |

<a id="22-collection-types"></a>

### 2.2 コレクション型

| 型 | 説明 |
|------|-------------|
| `List[T]` | 順序付きコレクション |
| `Map[K, V]` | キーと値の対応 |
| `Set[T]` | 重複のない値の集合 |

<a id="23-error-handling-types"></a>

### 2.3 エラー処理型

| 型 | 説明 |
|------|-------------|
| `Result[T, E]` | 成功 `ok(v)` または失敗 `err(e)` |
| `Option[T]` | 存在 `some(v)` または不在 `none` |
| `T?` | `Option[T]` の省略形で、すべての型位置で有効（ADR-0010）。`?` は直前の型アトムに結合し、`->` をまたがない。`(A) -> B?` は `Option[B]` を返す。オプショナルな関数値は `((A) -> B)?`、入れ子の Option は `(T?)?` と書く。正準表記であり、`almide fmt` は `Option[T]` を `T?` に正規化する |

<a id="24-composite-types"></a>

### 2.4 複合型

| 型 | 説明 |
|------|-------------|
| `(A, B, ...)` | タプル |
| `{ field: Type, ... }` | レコード（匿名） |
| `(A, B) -> C` / `effect (A, B) -> C` | 純粋 / エフェクト関数型 |

テスト： `spec/lang/data_types_test.almd`, `spec/lang/tuple_test.almd`

---

<a id="3-type-annotations"></a>

## 3. 型注釈

<a id="31-simple-types"></a>

### 3.1 単純型

```almide
let x: Int = 42
let name: String = "alice"
```

<a id="32-generic-types"></a>

### 3.2 ジェネリック型

Almide のジェネリクスは `[]` を使い、`<>` は使わない。

```almide
let xs: List[Int] = [1, 2, 3]
let m: Map[String, Int] = ["a": 1]
let r: Result[Int, String] = ok(42)
```

<a id="33-function-types"></a>

### 3.3 関数型

```almide
type Handler = (String) -> String
type Predicate = (Int) -> Bool
type Reducer = (Int, Int) -> Int
type Thunk = () -> Int
```

<a id="34-tuple-types"></a>

### 3.4 タプル型

```almide
type Pair = (Int, String)
type Triple = (Int, Int, Int)
```

<a id="35-record-types-anonymous"></a>

### 3.5 レコード型（匿名）

```almide
type User = { name: String, age: Int }
```

レコードフィールドは既定値とシリアライズ用別名をサポートする：

```almide
type Config = {
  host: String = "localhost",
  port: Int = 8080,
  name as "display_name": String,
}
```

<a id="36-open-record-types-structural"></a>

### 3.6 オープンレコード型（構造的）

```almide
fn get_name[T: { name: String, .. }](obj: T) -> String = obj.name
```

`..` は、列挙したもの以外のフィールドをレコードが持ちうることを示す。

テスト： `spec/lang/type_annotation_test.almd`, `spec/lang/open_record_test.almd`, `spec/lang/generics_test.almd`

---

<a id="4-declarations"></a>

## 4. 宣言

<a id="41-functions-fn"></a>

### 4.1 関数（`fn`）

```almide
fn add(x: Int, y: Int) -> Int = x + y
```

本体は `=` に続く単一の式である。複数の文を持つ本体にはブロックを使う：

```almide
fn greet(name: String) -> String = {
  let upper = string.to_upper(name)
  "Hello, ${upper}!"
}
```

波括弧なしのブロックもサポートする。本体が `let`、`var`、`guard` で始まると、パーサーは次のトップレベル宣言まで文を集める：

```almide
fn process(x: Int) -> Int =
  let doubled = x * 2
  let capped = int.min(doubled, 100)
  capped
```

<a id="generic-functions"></a>

#### ジェネリック関数

```almide
protocol Show {
  fn show(a: Self) -> String
}
fn identity[T](x: T) -> T = x
fn first[A, B](pair: (A, B)) -> A = pair.0
fn apply[T: Show](x: T) -> String = x.show()
```

ジェネリック仮引数は `[]` を使う。境界は `:` で指定し、`+` で組み合わせる：

```almide
protocol Show {
  fn show(a: Self) -> String
}
fn same_or_both[T: Show + Eq](x: T, y: T) -> String =
  if x == y then x.show() else x.show() + "/" + y.show()
```

構造的境界はオープンレコード型を使う：

```almide
fn name_of[T: { name: String, .. }](x: T) -> String = x.name
```

<a id="default-parameters"></a>

#### 既定引数

```almide
fn connect(host: String, port: Int = 8080) -> String =
  "${host}:${int.to_string(port)}"
```

最初の既定引数より後のすべての仮引数にも既定値が必要。

<a id="named-arguments"></a>

#### 名前付き引数

```almide
type User = { name: String, age: Int, active: Bool }

fn create(name: String, age: Int, active: Bool = true) -> User =
  User { name: name, age: age, active: active }

let u = create("alice", 30, active: false)

test "named argument" {
  assert_eq(u.active, false)
}
```

<a id="self-parameter"></a>

#### self 仮引数

関数はメソッドのようなディスパッチのため、第一仮引数に `self` を取れる：

```almide
type User = { name: String }

fn User.greet(self) -> String = "Hi, ${self.name}"

test "self parameter dispatches via UFCS" {
  assert_eq(User { name: "alice" }.greet(), "Hi, alice")
}
```

<a id="hole-and-todo"></a>

#### Hole と Todo

```almide
fn not_yet(x: Int) -> String = _                     // hole: type-checked stub
fn later(x: Int) -> String = todo("implement later")  // todo with message
```

<a id="attributes"></a>

#### 属性

関数宣言の前に一つ以上の `@name` または `@name(args)` 属性を付けられる。パーサーは一般的な形を受け付ける：

```almide
@pure
@inline_rust("almide_rt_int_abs({n})")
@schedule(device=gpu, tile=32, unroll=true)
fn decorated(n: Int) -> Int = int.abs(n)
```

文法：

- `@name` — 引数なし
- `@name(arg, ...)` — 位置引数、名前付き引数（`key=value`）、または混在
- 引数値：`"string"`、`42`、`0xff`、`-1`、`true`、`false`、または裸の識別子（値参照ではなくシンボリックタグとして扱う）

現在意味を持つ属性名：

- `@extern(target, "module", "function")` — 対象ランタイムの FFI 束縛。[module-system.md §11](module-system.ja.md#11-extern) 参照。
- `@export(c, "symbol")` — C ABI による export。`--repr-c` 出力と組み合わせる（module-system §10）。
- `@export(wasm, "symbol")` — fn を自身の名前ではなく `symbol` という名前の `--target wasm` export とし、ホストが呼べる DCE ルートにする。成果物にリンクするすべてのモジュール（エントリーファイル、兄弟の `import self.x` モジュール、依存パッケージ）で有効（#3281）。モジュールの属性なし `pub fn` は export ではないが、エントリーファイルのものは export となる。成果物の export 名前空間は一つ。どのモジュールであれ二つの fn が同じシンボルを要求する場合、または宣言した export が lower されない場合、ビルドを拒否する（E082、fn 名を示す）。テスト：`tests/wasm_export_attr_test.rs`、`tests/wasm_export_nonroot_test.rs`。
- `@inline_rust("template")` — stdlib、および fn を Rust で実装するパッケージが使う（aes と sha1 パッケージではテンプレートの後ろに `native/` モジュールがある）。stdlib 外では**ネイティブコード**である（#4100）。`@extern` fn と同じくすべてのエフェクトカテゴリを持ち、これを含む依存先にはルートの `[permissions] native` 許可が必要（[package-system.md §6.4](package-system.ja.md)）。注釈付き fn の Rust ターゲットのコード生成をインラインテンプレートに振り分け、TOML ベースの `arg_transforms` ディスパッチを上書きする。`{param_name}` プレースホルダーは、対応する呼び出し引数を描画した Rust 式に置き換える。fn 本体は Rust 関数として出力せず、テンプレートを各呼び出し箇所へインライン展開する。Stdlib Declarative Unification の作業で `stdlib/<module>.almd` ファイルが使用する。テスト：`spec/stdlib/int_bundled_inline_rust_test.almd`。

`@intrinsic("almide_rt_…")` と `@wasm_intrinsic` は、慣習だけでなく権限上も **stdlib 専用**（#2152）。検査器が受理するのは stdlib 自身のソース、つまり同梱 stdlib コピー、または `stdlib/` 内で本文が同名の埋め込みソースと完全一致するファイル（#4100：配置ではなく同一性。`stdlib/` の配置を模倣するパッケージは stdlib ではない）、あるいは呼び出し元が `--stdlib-authority <dir>` で指定したディレクトリ内の任意のソースである。このフラグは変更した stdlib コピーをビルドするハーネス用で、マニフェストのキーや環境変数では権限を付与できない。それ以外では E085（"intrinsics live in the stdlib; wrap in an effect fn"）で拒否する。`prim` モジュール（stdlib の基盤となるプリミティブ層）も同じ権限に従う（#3025）。stdlib 外の `prim.<fn>` と `import prim` は E085 となり、ヒントはその基盤操作を包む公開関数を示す。テスト：`tests/diagnostics/e085-intrinsic-outside-stdlib/`、`tests/diagnostics/e085-prim-outside-stdlib/`。

`@pure` は検査される（#3250）。fn とその呼び出し先すべてのエフェクト集合が空、つまりエフェクトカテゴリ、出力、`panic` / `assert*` がないことを要求し、違反があれば原因の呼び出し箇所で E092 によりビルドを停止する。[effect-system.md §2.3](effect-system.ja.md) 参照。

ほかの属性名（`@schedule`、`@rewrite`）はエラーなく解析され AST に保持するが、意味的な動作はまだない。Stdlib Declarative Unification と MLIR Backend の後続段階向けに予約している（`docs/roadmap/done/stdlib-declarative-unification.md`、`docs/roadmap/on-hold/mlir-backend-adoption.md` 参照）。現在のユーザーコードに書くことは構文上合法だが、コンパイラは無視する。

テスト：`crates/almide-syntax/src/parser/test_attributes.rs`（13 解析テスト）、`crates/almide-tools/src/fmt.rs::attr_tests`（6 書式往復テスト）。

テスト： `spec/lang/function_test.almd`, `spec/lang/default_args_test.almd`, `spec/lang/named_args_test.almd`, `spec/lang/generics_test.almd`

<a id="42-effect-functions-effect-fn"></a>

### 4.2 エフェクト関数（`effect fn`）

副作用（IO、乱数など）を持つ関数は `effect fn` 修飾子を使う。ユーザーのエフェクト関数は Result チャネルを公開し、`!` で明示的に伝播する。エラーを返さない stdlib intrinsic は、文書化された裸の値の例外である。

```almide
import fs

effect fn read_config(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!
  ok(content)
}
```

`effect fn` が `Result[T, E]` を返し、本体が明示的な `ok(...)` なしで終わるブロックの場合、成功値の末尾を `ok(...)` で包む（Unit の末尾は `ok(())` になる）：

```almide
effect fn log(msg: String) -> Result[Unit, String] = {
  println(msg)
  // ok(()) is inserted automatically
}
```

テスト： `spec/lang/effect_fn_test.almd`

<a id="43-type-declarations-type"></a>

### 4.3 型宣言（`type`）

<a id="record-types"></a>

#### レコード型

```almide
type User = { name: String, age: Int }
```

<a id="variant-types-leading-"></a>

#### バリアント型（先頭の `|`）

```almide
type Color =
  | Red
  | Green
  | Blue
```

バリアントケースはデータを持てる：

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }
```

バリアントケースの三つの形式：
- **Unit**：`| CaseName` — payload なし
- **タプル**：`| CaseName(Type, ...)` — 位置フィールド
- **レコード**：`| CaseName{ field: Type, ... }` — 名前付きフィールド

<a id="inline-variant-no-leading-"></a>

#### インラインバリアント（先頭の `|` なし）

```almide
type Outcome[T, E] = Good(T) | Bad(E)
```

すべてのケースが payload のない裸の大文字名なら、パーサーは `A | B | C` を union/enum として扱う：

```almide
type Direction = North | South | East | West
```

<a id="type-alias"></a>

#### 型エイリアス

```almide
type Name = String
type Handler = (String) -> String
```

エイリアスは型仮引数を取れる。実引数を適用すると、仮引数をそれぞれ実引数で置き換えた本体になる。仮引数、戻り値型、フィールド、バリアント payload、別エイリアスの本体など、どこに書いても同じで、別モジュールからは修飾する（`geo.Pair[Int]`）：

```almide
type Pair[T] = (T, T)            // Pair[Int] is (Int, Int)
type Step[T] = (T) -> T          // Step[String] is (String) -> String
type Both[T] = Pair[List[T]]     // Both[Int] is (List[Int], List[Int])

fn first(p: Pair[Int]) -> Int = p.0
```

実引数は宣言順に仮引数を置き換える。展開するのはエイリアスだけで、ジェネリックなレコードやバリアントは nominal 型のまま。引数数は宣言した仮引数数と一致しなければならない（E093）。組み込みでも同じで、`List`、`Option`、`Set` は一つ、`Map`、`Result` は二つ、`Int` のようなスカラーはゼロである。

宣言順は関係しない。エイリアスはファイルやモジュールの後方で宣言した型を参照できる（`type First = Pair2` が `type Pair2 = (Int,
Int)` より前にある場合）。エイリアスが自分へ戻ることはできない。`type A = B` と `type B = A`、あるいは型引数を通る `type Tree = List[Tree]` は型を指さないため E094 となる。レコードやバリアントは自己参照できる。

<a id="generic-types"></a>

#### ジェネリック型

```almide
type Pair[A, B] = { first: A, second: B }
type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
```

<a id="conventions-deriving"></a>

#### 規約（導出）

```almide
type Color: Eq, Repr = Red | Green | Blue
type Point: Codec = { x: Float, y: Float }
```

組み込み規約は `Eq`、`Repr`、`Ord`、`Hash`、`Codec`、`Encode`、`Decode` を含む。

`Codec` は `T.encode(v: T) -> Value` と `T.decode(v: Value) ->
Result[T, String]` を導出し、レコードとバリアントには `T.decode_strict(v: Value) -> Result[T, String]` も導出する。`Encode` / `Decode` は片方向だけを選ぶ。[ALS-D6](als/data-formats.ja.md) 参照。厳格デコードは入れ子のコンテナも含め未知のキーを再帰的に拒否し、そのパスを報告する。`T.decode` は未知のキーを無視する。`T.decode_strict` はまず、デコード対象フィールドのキーではない最初のキー（文書順）に `err("unknown key '<k>'")` を返し、その後は `T.decode` と同様にデコードする（C-429）。レコードフィールドは `@skip`（デコードもエンコードもしない）または `@skip_decode`（デコードせずエンコードはする）を持てる。どちらも既定値が必要で、なければ E104。そのようなフィールドの入力キーは未知のキーとみなす（C-428）。レコードフィールドが受け付ける属性は `@builtin`、`@location`、`@skip`、`@skip_decode` のみで、ほかは E053（C-430）。

```almide
type User: Codec = {
  name: String,
  @skip_decode is_admin: Bool = false,   // never read from input; still encoded
  @skip cache_hint: Int = 0,             // neither read nor written
}
```

テスト： `spec/wasm_cross/codec_field_skip_matrix.almd`, `tests/codec_field_skip_test.rs`

テスト： `spec/lang/data_types_test.almd`, `spec/lang/type_alias_test.almd`, `spec/lang/generic_type_alias_test.almd`, `spec/lang/forward_type_alias_test.almd`, `spec/lang/variant_record_test.almd`, `spec/lang/derive_conventions_test.almd`

<a id="44-protocol-declarations"></a>

### 4.4 プロトコル宣言

```almide
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
  effect fn load(a: Self) -> Result[Unit, String]
}
```

プロトコルのメソッドは `effect fn` にできる。

テスト： `spec/lang/protocol_test.almd`, `spec/lang/protocol_advanced_test.almd`

<a id="45-convention-methods"></a>

### 4.5 Convention method

`impl` ブロックは存在しない（削除済み）。プロトコルの充足も型へのメソッド追加も、
convention method（`fn Type.method`）で行う:

```almide
type Context = { user: String }
type GreetAction = { greeting: String }

fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok("${a.greeting}, ${ctx.user}")

test "convention methods: qualified and UFCS" {
  let a = GreetAction { greeting: "hi" }
  assert_eq(GreetAction.name(a), "greet")
  assert_eq(a.name(), "greet")
  assert_eq(a.execute(Context { user: "bob" }), ok("hi, bob"))
}
```

呼び出しは `GreetAction.name(a)` でも UFCS の `a.name()` でも可。

テスト： `spec/lang/trait_impl_test.almd`, `spec/lang/protocol_test.almd`

<a id="46-top-level-let"></a>

### 4.6 トップレベル `let`

モジュールスコープの定数：

```almide
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```

コンパイル時（const）に評価するか、非 const 式では `LazyLock` 経由で評価する。

テスト： `spec/lang/top_let_test.almd`

<a id="47-test-declarations"></a>

### 4.7 テスト宣言

```almide
test "addition works" {
  assert_eq(1 + 2, 3)
  assert(3 > 0)
  assert_ne(1, 2)
}
```

本体は波括弧で囲んだブロック式。

テスト：すべての `spec/lang/*_test.almd` ファイルが `test` ブロックを含む。

---

<a id="5-expressions"></a>

## 5. 式

<a id="51-literals"></a>

### 5.1 リテラル

```almide
let a = 42                     // Int
let b = 0xFF                   // Int (hex)
let c = 1_000_000              // Int (underscores for readability)
let d = 3.14                   // Float
let e = "hello"                // String
let f = true                   // Bool
let g = false                  // Bool
let h = ()                     // Unit

test "literal values" {
  assert_eq(b, 255)
  assert_eq(c, 1000000)
}
```

テスト： `spec/lang/expr_test.almd`

<a id="52-string-interpolation"></a>

### 5.2 文字列補間

```almide
test "interpolation" {
  let name = "alice"
  assert_eq("hello ${name}", "hello alice")
  assert_eq("result = ${1 + 2}", "result = 3")
  assert_eq("nested ${string.len(name)}", "nested 5")
}
```

`${}` 内の式は完全な式として解析する。

テスト： `spec/lang/string_interp_test.almd`, `spec/lang/interpolation_edge_test.almd`

<a id="53-heredoc-multi-line-strings"></a>

### 5.3 Heredoc（複数行文字列）

```almide
let id = 7
let sql = """
  SELECT *
  FROM users
  WHERE id = ${id}
"""

test "heredoc strips the common indent" {
  assert_eq(sql, "SELECT *\nFROM users\nWHERE id = 7")
}
```

最小インデントに基づいて行頭の空白を除去する。heredoc 内でも `${expr}` による補間が働く。

raw heredoc（エスケープ処理なし）：`r"""..."""`

テスト： `spec/lang/heredoc_test.almd`

<a id="54-list-literals"></a>

### 5.4 リストリテラル

```almide
let empty: List[Int] = []      // empty list (needs the annotation)
let ints = [1, 2, 3]           // List[Int]
let strs = ["a", "b", "c"]     // List[String]
let trailing = [1, 2, 3,]      // trailing comma allowed

test "list literals" {
  assert_eq(list.len(empty), 0)
  assert_eq(ints, trailing)
}
```

テスト： `spec/lang/expr_test.almd`

<a id="55-map-literals"></a>

### 5.5 マップリテラル

```almide
let m: Map[String, Int] = [:]       // empty map (requires type annotation)
let ab = ["a": 1, "b": 2]           // Map[String, Int]

test "map literals" {
  assert_eq(map.len(m), 0)
  assert_eq(ab["b"], some(2))
}
```

マップは `[key: value]` 構文を使う。波括弧 `{}` はレコード・ブロック、角括弧 `[]` はリスト・マップ用。

テスト： `spec/lang/map_literal_test.almd`, `spec/lang/map_edge_test.almd`

<a id="56-record-literals"></a>

### 5.6 レコードリテラル

匿名レコード：

```almide
let alice = { name: "alice", age: 30 }
```

名前付きレコード（型付き構築）：

```almide
type User = { name: String, age: Int }

let alice = User { name: "alice", age: 30 }
```

すべてのフィールドを `name: value` と書く。フィールドの省略表記はない（`{ name, age }` は構文エラー）：

```almide check-fail=syntax
let name = "alice"
let age = 30
let r = { name, age }
```

<a id="spread-records"></a>

#### レコードのスプレッド

```almide
type User = { name: String, age: Int }

let base = { name: "alice", age: 30 }
let bob = { ...base, name: "bob" }
let typed = User { ...base, name: "bob" }

test "spread copies the unnamed fields" {
  assert_eq(bob.age, 30)
  assert_eq(typed.age, 30)
}
```

テスト： `spec/lang/record_spread_test.almd`, `spec/lang/data_types_test.almd`

<a id="57-tuple-expressions"></a>

### 5.7 タプル式

```almide
let pair = (1, "hello")        // (Int, String)
let triple = (1, 2, 3)         // (Int, Int, Int)
```

タプル要素にはインデックスでアクセスする：

```almide
test "tuple index access" {
  let pair = (1, "hello")
  assert_eq(pair.0, 1)
  assert_eq(pair.1, "hello")
}
```

テスト： `spec/lang/tuple_test.almd`

<a id="58-if-then-else"></a>

### 5.8 If-Then-Else

`if` は式であり `then` が必要。`else` は任意で、`else` を省略すると結果は `Unit`。

```almide
fn sign(x: Int) -> String = if x > 0 then "positive" else "non-positive"
fn pick(a: Bool, b: Bool, x: Int, y: Int, z: Int) -> Int =
  if a then x else if b then y else z

test "if is an expression" {
  assert_eq(sign(-1), "non-positive")
  assert_eq(pick(false, true, 1, 2, 3), 2)
}
```

```almide
let count = 1
let label = if count == 1 then "item" else "items"
```

テスト： `spec/lang/control_flow_test.almd`, `spec/lang/expr_test.almd`

<a id="59-match"></a>

### 5.9 Match

網羅的パターンマッチ：

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }

fn area(shape: Shape) -> Float = match shape {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
  Named{ name, sides } => float.from(sides),
}

test "every case is handled" {
  assert_eq(area(Rect(2.0, 3.0)), 6.0)
  assert_eq(area(Named{ name: "tri", sides: 3 }), 3.0)
}
```

match の腕は guard をサポートする：

```almide
fn classify(n: Int) -> String = match n {
  x if x > 0 => "positive",
  0 => "zero",
  _ => "negative",
}

test "guards are tried in order" {
  assert_eq(classify(5), "positive")
  assert_eq(classify(0), "zero")
  assert_eq(classify(-2), "negative")
}
```

パイプ match 構文：

```almide
fn or_zero(value: Option[Int]) -> Int = value |> match {
  some(x) => x,
  none => 0,
}

test "pipe-match" {
  assert_eq(or_zero(some(4)), 4)
  assert_eq(or_zero(none), 0)
}
```

<a id="74-exhaustiveness-of-list-patterns"></a>

### 7.4 リストパターンの網羅性

match はすべての値を網羅しなければならず（E010 が不足するケースを示す）、先行する腕が既に網羅する腕は E014 となる。リストパターンは長さで数える。`[]` と `[p1, p2]` はそれぞれ一つの長さ、`[p1, ..]` は長さ一以上のすべてを網羅する。タプル内など、リストパターンの位置にかかわらず同じ（#2600）：

```almide
fn f(xs: List[Int], ys: List[Int]) -> Int =
  match (xs, ys) {
    ([x, ..xt], [y, ..yt]) => x + y,
    ([], []) => 0,       // reachable: both lists empty
    ([], _) => 1,
    (_, []) => 2,
  }

test "list patterns under a tuple" {
  assert_eq(f([1], [2]), 3)
  assert_eq(f([], []), 0)
}
```

`(_, [])` の腕を外すと E010 となり、未網羅の `([_], [])` を示す。

テスト： `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`,
`tests/diagnostics/e010-list-*`,
`tests/list_pattern_tuple_test.rs`

<a id="510-lambda"></a>

### 5.10 ラムダ

```almide
test "lambda forms" {
  let inc = (x) => x + 1
  let add = (x, y) => x + y
  let dbl = (x: Int) => x * 2                    // with type annotation
  let k = (_) => 42                              // wildcard parameter
  let sum = ((a, b)) => a + b                    // tuple destructuring in parameter
  assert_eq(inc(1), 2)
  assert_eq(add(1, 2), 3)
  assert_eq(dbl(2), 4)
  assert_eq(k("anything"), 42)
  assert_eq(sum((1, 2)), 3)
}
```

複数行のラムダ本体はブロックを使う：

```almide
test "block-bodied lambda" {
  let f = (x: Int) => {
    let y = x * 2
    y + 1
  }
  assert_eq(f(3), 7)
}
```

テスト： `spec/lang/lambda_test.almd`

<a id="511-block-expressions"></a>

### 5.11 ブロック式

ブロックの最後の式がブロックの値になる：

```almide
let result = {
  let x = 1
  let y = 2
  x + y
}
// result = 3
```

テスト： `spec/lang/expr_test.almd`, `spec/lang/scope_test.almd`

<a id="512-for-in-loop"></a>

### 5.12 For-In ループ

```almide
fn print_all(xs: List[Int]) -> Unit = {
  for x in xs {
    println(int.to_string(x))
  }
}
```

for 内のタプル分解：

```almide
fn print_entries(m: Map[String, Int]) -> Unit = {
  for (k, v) in map.to_list(m) {
    println("${k} = ${v}")
  }
}
```

無視する変数にはアンダースコア：

```almide
fn tick_five() -> Unit = {
  for _ in 0..<5 {
    println("tick")
  }
}
```

テスト： `spec/lang/for_test.almd`, `spec/lang/for_tuple_test.almd`

<a id="513-while-loop"></a>

### 5.13 While ループ

```almide
fn count_to_ten() -> Unit = {
  var i = 0
  while i < 10 {
    println(int.to_string(i))
    i = i + 1
  }
}
```

ループ内では `break` と `continue` をサポートする：

```almide
fn odd_numbers() -> Unit = {
  var i = 0
  while true {
    if i >= 10 then break
    i = i + 1
    if i % 2 == 0 then continue
    println(int.to_string(i))
  }
}
```

テスト： `spec/lang/while_test.almd`, `spec/lang/while_loop_test.almd`

<a id="514-range"></a>

### 5.14 範囲

```almide
test "range forms" {
  assert_eq(0..<5, [0, 1, 2, 3, 4])     // exclusive end
  assert_eq(1...5, [1, 2, 3, 4, 5])     // inclusive end
}
```

範囲は for ループで使える（最適化され、リストを割り当てない）：

```almide
fn count_to(n: Int) -> Unit = {
  for i in 0..<n {
    println(int.to_string(i))
  }
}
```

テスト： `spec/lang/range_test.almd`

<a id="515-pipe-operator"></a>

### 5.15 パイプ演算子

```almide
test "pipe passes the left side as the FIRST argument" {
  let text = " a,b "
  assert_eq(text |> string.trim |> string.split(","), ["a", "b"])
  let xs = [1, -2, 3]
  assert_eq(xs |> list.filter((x) => x > 0), [1, 3])
}
```

パイプ演算子 `|>` は左辺を右辺の第一引数として渡す。プレースホルダーはない。呼び出し引数内の `_` は値を持たない hole なので拒否する（E046）。

```almide check-fail=E046
fn positives(xs: List[Int]) -> List[Int] = xs |> list.filter(_, (x) => x > 0)
```

テスト： `spec/lang/pipe_test.almd`

<a id="516-compose-operator"></a>

### 5.16 合成演算子

```almide
test "compose" {
  let transform = string.trim >> string.to_upper
  assert_eq(transform("  hello  "), "HELLO")
}
```

`>>` 演算子は二つの関数を左から右へ合成する。

テスト： `spec/lang/compose_test.almd`

<a id="517-fan-blocks-concurrent-execution"></a>

### 5.17 Fan ブロック（並行実行）

```almide
effect fn fetch_users() -> Result[Int, String] = ok(3)
effect fn fetch_orders() -> Result[Int, String] = ok(7)
effect fn fetch_config() -> Result[String, String] = ok("prod")

test "fan returns a tuple of the unwrapped results" {
  let (a, b, c) = fan {
    fetch_users()
    fetch_orders()
    fetch_config()
  }
  assert_eq(a + b, 10)
  assert_eq(c, "prod")
}
```

Fan ブロックは式を並行実行する。ブロック内の各式は並列に動き、ブロックは結果のタプルを返す。

Fan ブロックが許可するのは式だけで、`let`、`var`、`for`、`while` 文は許可しない。

**レコード形式。** すべての腕が `name: expr` フィールドなら、ブロックはフィールドを並列の兄弟として実行するレコードリテラルとなる。値は匿名レコード `{ name: value, … }` で、通常のリテラルとまったく同じ型になり、名前で読む。

```almide
effect fn add(a: Int, b: Int) -> Result[Int, String] = ok(a + b)
effect fn mul(a: Int, b: Int) -> Result[Int, String] = ok(a * b)

test "fan record form" {
  let r = fan {
    sum: add(3, 4)!
    product: mul(3, 4)!
  }
  assert_eq(r.sum + r.product, 19)
  assert_eq(r, { sum: 7, product: 12 })
}
```

- どのフィールドも失敗しなければ、`fan` を外しても観測可能な変化はない。フィールドの出力はソース順に現れ、値は通常のリテラルと等しい（C-386）。
- `e!` と書いたフィールドは、タプル形式の規則でその Err をブロックの Err にする。すべてのフィールドを実行し、ソース順で最初の Err が選ばれる（C-199、C-386）。
- 値が `Result` のフィールド（エフェクト呼び出しも含む）は `e!` と書かなければならず、マーカーがなければ E041。兄弟フィールドはブロック内のスコープにない。
- すべての腕に名前が必要（`fan { a: f(), g() }` は構文エラー）。名前は一度だけ現れ、省略表記はない。`fan { x }` は一腕のタプル形式。

テスト： `spec/lang/fan_record_test.almd`, `spec/wasm_cross/fan_record_serial_elision.almd`, `spec/wasm_cross/fan_record_err_source_order.almd`, `tests/diagnostics/e041-fan-record-field-without-bang/`

`fan.*` ブロックのヘッドは共通の表面構文を持つ：

```almide
effect fn a() -> Result[Int, String] = err("a failed")
effect fn b() -> Result[Int, String] = ok(2)
fn work(x: Int) -> Int = x * 2

test "fan.* heads" {
  let first = a() ?? b() ?? -1          // first Ok in source order: a `??` chain, not a fan form
  let (ra, rb) = fan.settle {           // a TUPLE of per-arm Results
    a()
    b()
  }
  let r = fan.bounded(compute.ms(100)) { work(21) } ?? -1  // deterministic budget
  let w = fan.min(compute.ms(5)) { work(1), work(2) } ?? -1  // least compute within the budget
  assert_eq(first, 2)
  assert_eq(ra, err("a failed"))
  assert_eq(rb, ok(2))
  assert_eq(r, 42)
  assert_eq(w, 2)
}
```

腕は改行（または `,`）で区切り、`;` は使わない。`fan.race` は 0.42.0 で削除された。決定的モデルでは別名の `thunks[0]()` だったためである。その後継の決定的ヘッドは dialect epoch 14 で `fan.min` に改名され、時計引数が必須になった。同じ epoch で `fan.any` も削除された（`??` チェーンまたは `list.find_map` が同じ意味を表す）。どちらの旧名の参照も、置き換え先を示すトンボストーンエラー E027 となる。

予算は `compute.*` の時間コンストラクタで作る（閉じた単位集合 `ns / us / ms / s / min / h`）。裸の `Int` や壁時計の `duration.*` 値は型エラー。完全な意味論は [docs/SPEC.md §13](../SPEC.ja.md)、規範的な時間規則は [docs/adr/0001-deterministic-time-units.md](../../../../adr/0001-deterministic-time-units.md) を参照。

テスト： `spec/lang/fan_test.almd`, `spec/lang/fan_map_test.almd`, `spec/lang/fan_ext_test.almd`, `spec/wasm_cross/fan_*.almd`, `research/spike/charge-probe/fixtures/`

<a id="518-option-and-result-constructors"></a>

### 5.18 Option と Result のコンストラクタ

```almide
let present: Option[Int] = some(42)            // Option[Int] = some
let absent: Option[Int] = none                 // Option[T] = none

let success: Result[Int, String] = ok(42)      // Result[Int, E] = success
let failure: Result[Int, String] = err("failed")  // Result[T, String] = failure
```

テスト： `spec/lang/error_test.almd`, `spec/lang/unwrap_operators_test.almd`

<a id="519-function-calls"></a>

### 5.19 関数呼び出し

```almide
fn add(x: Int, y: Int) -> Int = x + y
fn f[T](x: T) -> T = x

test "call forms" {
  let text = "a,b"
  let xs = [1, 2]
  assert_eq(add(1, 2), 3)                           // positional args
  assert_eq(string.split(text, ","), ["a", "b"])    // module function call
  assert_eq(list.map(xs, (x) => x + 1), [2, 3])     // higher-order
  assert_eq(f[Int](42), 42)                         // explicit type arguments
}
```

名前付き引数：

```almide
fn connect(host: String, port: Int = 80, secure: Bool = false) -> String =
  "${if secure then "https" else "http"}://${host}:${int.to_string(port)}"

test "named arguments" {
  assert_eq(connect("localhost", port: 3000, secure: true), "https://localhost:3000")
}
```

テスト： `spec/lang/function_test.almd`, `spec/lang/named_args_test.almd`

<a id="520-member-access-and-index-access"></a>

### 5.20 メンバーアクセスとインデックスアクセス

```almide
test "member and index access" {
  let user = { name: "alice" }
  let pair = (1, "x")
  let xs = [10, 20]
  let m = ["key": 5]
  assert_eq(user.name, "alice")      // field access
  assert_eq(pair.0, 1)               // tuple index access
  assert_eq(xs[0], 10)               // list index
  assert_eq(m["key"], some(5))       // map index (returns Option[V])
}
```

テスト： `spec/lang/expr_test.almd`, `spec/lang/tuple_test.almd`

<a id="521-ufcs-uniform-function-call-syntax"></a>

### 5.21 UFCS（統一関数呼び出し構文）

UFCS は固定された検索順序で `x.f(y)` を解決し、適用可能なユーザー関数より先に stdlib と convention method を調べる。その関数が選ばれた場合は `f(x, y)` と等価になりうるが、同名のローカル関数が stdlib メソッドを上書きするわけではない。`check/calls_ufcs.rs` と `spec/lang/user_fn_namespace_collision_test.almd` 参照。

```almide
test "UFCS" {
  let text = " x "
  let trimmed = string.trim(text)
  let same = text.trim()           // equivalent via UFCS
  assert_eq(trimmed, same)
}
```

---

<a id="6-statements"></a>

## 6. 文

文はブロックと関数本体に現れる。改行で文を区切る（セミコロン不要）。

<a id="61-let-binding-immutable"></a>

### 6.1 Let 束縛（不変）

```almide
fn some_fn() -> Result[Int, String] = ok(1)

test "let forms" {
  let x = 1
  let y: Int = 1                   // with type annotation
  let _ = some_fn()                // discard result
  assert_eq(x, y)
}
```

<a id="62-var-binding-mutable"></a>

### 6.2 Var 束縛（可変）

```almide
test "var" {
  var count = 0
  count = count + 1                // reassignment (var only)
  assert_eq(count, 1)
}
```

<a id="63-destructuring"></a>

### 6.3 分解

レコード分解：

```almide
test "record destructuring" {
  let user = { name: "alice", age: 30 }
  let { name, age } = user
  assert_eq(name, "alice")
  assert_eq(age, 30)
}
```

タプル分解：

```almide
test "tuple destructuring" {
  let point = (1, 2)
  let triple = (1, 2, 3)
  let (x, y) = point
  let (first, _, third) = triple   // wildcard for unused
  assert_eq(x + y, 3)
  assert_eq(first + third, 4)
}
```

`var` も同じパターン、すなわちタプル（入れ子や `_` を含む）とレコードの省略形を取り、束縛されるすべての名前が `var` となる（#3149）。値は一度だけ評価し、`var (x, y) = p` は `var x = p.0` / `var y = p.1` と一つずつ書く場合とまったく同じように振る舞う。各名前は独立して再代入され、一つへの書き込みはほかの名前にも `p` にも届かない。

```almide
test "var destructuring" {
  let point = (1, 2)
  var (x, y) = point
  x = x + 10
  y = y * 3
  assert_eq(x + y, 17)
  assert_eq(point.0, 1)
}
```

テスト： `spec/lang/variable_test.almd`, `spec/lang/data_types_test.almd`,
`spec/lang/var_destructure_test.almd`

<a id="64-assignment"></a>

### 6.4 代入

単純な再代入（var のみ）：

```almide
test "reassignment" {
  var x = 1
  x = x + 1
  assert_eq(x, 2)
}
```

インデックス代入（var のみ）：

```almide
test "index assignment" {
  var xs = [1, 2]
  var m: Map[String, Int] = [:]
  let value = 5
  xs[0] = 99
  m["key"] = value
  assert_eq(xs, [99, 2])
  assert_eq(m["key"], some(5))
}
```

フィールド代入（var のみ）：

```almide
test "field assignment" {
  var user = { name: "alice" }
  user.name = "bob"
  assert_eq(user.name, "bob")
}
```

<a id="65-guard-early-return"></a>

### 6.5 Guard（早期 return）

```almide
import fs

type AppError = NotFound(String) | Invalid(String)

fn positive(x: Int) -> Result[Int, String] = {
  guard x > 0 else err("must be positive")
  ok(x)
}

effect fn require(path: String) -> Result[String, AppError] = {
  guard fs.exists(path) else err(NotFound(path))
  ok(path)
}

test "guard returns the else value early" {
  assert_eq(positive(-1), err("must be positive"))
  assert_eq(require("/nonexistent/almide-doctest"), err(NotFound("/nonexistent/almide-doctest")))
}
```

ブロック本体を使う場合：

```almide
import fs

effect fn create_once(path: String) -> Result[Unit, String] = {
  guard not fs.exists(path) else {
    println("already exists")
    ok(())
  }
  fs.write_text(path, "")!
  ok(())
}
```

`guard` は条件を検査する。偽なら `else` 分岐が周囲の戻り値チャネルと互換の値で早期 return するか、発散する。Result を返す関数だけに限定されない。

テスト： `spec/lang/guard_test.almd`

<a id="66-expression-statements"></a>

### 6.6 式文

どの式も文として書ける。ブロックの最後の式がブロックの値になる。

```almide
fn forty_three() -> Int = {
  println("side effect")           // expression statement (Unit)
  let x = 42
  x + 1                           // final expression = block value
}
```

---

<a id="7-pattern-matching"></a>

## 7. パターンマッチ

パターンは `match` の腕、`let` の分解、`for` ループ変数に現れる。

<a id="71-pattern-forms"></a>

### 7.1 パターンの形式

| パターン | 構文 | 説明 |
|---------|--------|-------------|
| ワイルドカード | `_` | 何にでもマッチし、束縛しない |
| 識別子 | `name` | 何にでもマッチし、`name` に束縛 |
| リテラル | `42`, `3.14`, `"text"`, `true`, `false` | 正確な値にマッチ |
| 負のリテラル | `-1`, `-3.14` | 負数にマッチ |
| コンストラクタ | `TypeName(p1, p2)` | タプル payload のバリアントにマッチ |
| レコード | `TypeName{ field1, field2 }` | レコード payload のバリアントにマッチ |
| レコード（入れ子） | `TypeName{ field: pattern }` | フィールドの入れ子パターンでマッチ |
| レコード（残り） | `TypeName{ field, .. }` | 追加フィールドを無視してマッチ |
| タプル | `(p1, p2, p3)` | タプルにマッチ |
| `some(p)` | `some(inner)` | `Option` の some ケースにマッチ |
| `none` | `none` | `Option` の none ケースにマッチ |
| `ok(p)` | `ok(inner)` | `Result` の ok ケースにマッチ |
| `err(p)` | `err(inner)` | `Result` の err ケースにマッチ |
| リスト | `[p1, p2]` | 要素数がちょうどその数のリストにマッチ |
| リスト（残り） | `[p1, ..t]`, `[p1, ..]` | 少なくともその数の要素を持つリストにマッチ。`t` は末尾を束縛 |

<a id="72-examples"></a>

### 7.2 例

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }

fn describe(value: Int) -> String = match value {
  0 => "zero",
  n if n > 0 => "positive: ${int.to_string(n)}",
  _ => "negative",
}

fn report(result: Result[String, String]) -> Unit = match result {
  ok(value) => println(value),
  err(msg) => eprintln(msg),
}

fn doubled(option: Option[Int]) -> Int = match option {
  some(x) => x * 2,
  none => 0,
}

fn area(shape: Shape) -> Float = match shape {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
  Named{ name, .. } => {
    println(name)
    0.0
  },
}

test "pattern forms" {
  assert_eq(describe(3), "positive: 3")
  assert_eq(doubled(some(2)), 4)
  assert_eq(area(Circle(1.0)), 3.14)
}
```

<a id="73-nested-patterns"></a>

### 7.3 入れ子パターン

パターンは組み合わせられる：

```almide
fn add_both(pair: (Result[Int, String], Result[Int, String])) -> Result[Int, String] =
  match pair {
    (ok(x), ok(y)) => ok(x + y),
    (err(e), _) => err(e),
    (_, err(e)) => err(e),
  }

test "nested patterns" {
  assert_eq(add_both((ok(1), ok(2))), ok(3))
  assert_eq(add_both((ok(1), err("b"))), err("b"))
}
```

テスト： `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`

---

<a id="8-operators"></a>

## 8. 演算子

<a id="81-precedence-highest-to-lowest"></a>

### 8.1 優先順位（高い順）

| 優先順位 | 演算子 | 結合性 |
|------------|-----------|---------------|
| 1 | `.` `()` `[]` `!` `??` `?.` `?` | 後置族。`??` のチェーンは右入れ子(`a ?? b ?? c` = `a ?? (b ?? c)`、意味は「最初の成功が勝つ」) |
| 2 | `not` `-` （単項） | 右（前置） |
| 3 | `^` | 右 |
| 4 | `*` `/` `%` | 左 |
| 5 | `+` `-` | 左 |
| 6 | `..<` `...` | 非結合 |
| 7 | `==` `!=` `<` `>` `<=` `>=` | 非結合 |
| 8 | `and` | 左 |
| 9 | `or` | 左 |
| 10 | `\|>` `>>` | 左 |

<a id="82-arithmetic-operators"></a>

### 8.2 算術演算子

| 演算子 | 説明 |
|----------|-------------|
| `+` | 加算（Int、Float）、連結（String、List） |
| `-` | 減算 |
| `*` | 乗算 |
| `/` | 除算 |
| `%` | 剰余 |
| `^` | べき乗（右結合） |
| `-`（単項） | 符号反転 |

`+` は多重定義され、数値には加算、文字列とリストには連結となる。

<a id="83-comparison-operators"></a>

### 8.3 比較演算子

| 演算子 | 説明 |
|----------|-------------|
| `==` | 等しい（深い等値性） |
| `!=` | 等しくない |
| `<` | より小さい |
| `>` | より大きい |
| `<=` | 以下 |
| `>=` | 以上 |

比較は**非結合**であり、`a < b < c` のような連鎖はコンパイルエラー。`a < b and b < c` を使う。

<a id="84-logical-operators"></a>

### 8.4 論理演算子

| 演算子 | 説明 |
|----------|-------------|
| `and` | 論理 AND（短絡） |
| `or` | 論理 OR（短絡） |
| `not` | 論理 NOT（前置） |

Almide は記号ではなく単語を使い、`&&` と `||` はヒント付きで拒否する。

<a id="85-unwrap-operators"></a>

### 8.5 取り出し演算子

| 演算子 | 構文 | 説明 |
|----------|--------|-------------|
| `!` | `expr!` | 現在の fn/lambda チャネルで Result/Option を取り出す。テストは伝播ではなく取り出して失敗する。Result、Option、型付きエラー、失敗しないコンテキストは[コンテキスト表](result-option-effect.ja.md#2-演算子)参照。 |
| `??` | `expr ?? fallback` | 取り出すか代替値を使う。fallback は**遅延評価**（none/err のときだけ）。**最高位に結合**：`a ?? 1 + 2` = `(a ?? 1) + 2`。C 系の `?:`（ほぼ最下位）と逆なので、算術と混ぜるときは括弧を推奨。fallback は同じ行に置く（行またぎは E038、複数行は括弧 + `??` 後置）。 |
| `?` | `expr?` | Result を Option に変換（err は none になる） |
| `?.` | `expr?.field` | オプショナルチェーン（Option[Record] から Option[FieldType]）— **Option 専用**。Result は専用診断で拒否（`(r?)?.x` と合成する） |

```almide
import fs

type User = { name: String }
fn risky_fn() -> Result[Int, String] = ok(1)

effect fn demo(m: Map[String, String], path: String, user: Option[User]) -> Result[String, String] = {
  let value = map.get(m, "key") ?? "default"
  let content = fs.read_text(path)!
  let name = user?.name
  let opt = risky_fn()?
  ok("${value} ${content} ${name ?? "?"} ${opt ?? 0}")
}
```

テスト： `spec/lang/unwrap_operators_test.almd`, `spec/lang/operator_test.almd`

<a id="86-bitwise-operations"></a>

### 8.6 ビット演算

ビット演算は演算子ではなく関数：

```almide
test "bitwise functions" {
  assert_eq(int.band(6, 3), 2)      // AND
  assert_eq(int.bor(6, 3), 7)       // OR
  assert_eq(int.bxor(6, 3), 5)      // XOR
  assert_eq(int.bnot(0), -1)        // NOT
  assert_eq(int.bshl(1, 3), 8)      // shift left
  assert_eq(int.bshr(8, 3), 1)      // shift right
}
```

<a id="87-string-concatenation"></a>

### 8.7 文字列連結

```almide
test "+ concatenates" {
  assert_eq("hello" + " " + "world", "hello world")   // string concatenation with +
  assert_eq([1, 2] + [3, 4], [1, 2, 3, 4])            // list concatenation with +
}
```

テスト： `spec/lang/string_test.almd`, `spec/lang/operator_test.almd`

---

<a id="9-visibility"></a>

## 9. 可視性

可視性修飾子はトップレベルの `fn`、`type`、`let` より前に置く。

| 修飾子 | スコープ | Rust 相当 |
|----------|-------|-----------------|
| なし | public — 誰でもアクセス可 | `pub` |
| `mod` | 同一プロジェクトのみ | `pub(crate)` |
| `local` | このファイルのみ | private |

```almide
fn public_fn() -> Int = 42              // public (default)
mod fn internal_fn() -> Int = 42        // project-internal
local fn private_fn() -> Int = 42       // file-private
```

修飾子の順序：`[local|mod]? effect? fn`

```almide
local effect fn helper() -> Result[Unit, String] = ok(())
mod fn utility(x: Int) -> Int = x * 2
```

可視性は型とトップレベル let にも適用する：

```almide
local type InternalState = { count: Int }
mod let CACHE_SIZE = 256
```

テスト： `spec/lang/visibility_test.almd`

---

<a id="10-comments"></a>

## 10. コメント

<a id="101-line-comments"></a>

### 10.1 行コメント

```almide
// This is a line comment
let x = 42  // inline comment
```

<a id="102-block-comments"></a>

### 10.2 ブロックコメント

ブロックコメントは入れ子にできる：

```almide
/* This is a block comment */

/*
  /* Nested block comments work */
  Still inside the outer comment
*/
```

ブロックコメントは lexer が完全に読み飛ばす（トークンとして出力しない）。

テスト： `spec/lang/block_comment_raw_string_test.almd`

---

<a id="11-built-in-functions"></a>

## 11. 組み込み関数

```almide
test "built-ins" {
  let s = "hi"
  println(s)              // print line to stdout
  eprintln(s)             // print line to stderr (debug only)
  assert_eq(1, 1)         // assert equal (also available outside tests)
  assert_ne(1, 2)         // assert not equal (also available outside tests)
  assert(true)            // assert true (also available outside tests)
}
```

文字列表現は組み込み関数ではない。型が repr を宣言しない限り、すべての値は構造的に出力する。レコードは `Name { f: v }`、リストは `[a, b]`、コンテナ内の文字列は引用符付きになる。`fn T.repr(x: T) -> String` を定義する型は、描画するすべての場所でそれを通じて出力する。補間の最上位（`"${x}"`、ALS-R2）、外側のレコード、リスト、option、result、タプル、マップ、バリアントの内部、および `assert_eq` / `assert_ne` の失敗や `almide test` レポートも含む。`secret.Secret[A]` はこれを宣言する stdlib 型で、すべての経路で `Secret(***)` を出力する（`docs/stdlib/secret.md`）。

裸の prelude `print` は存在しない。一行には `println`、改行を追加せず明示的にフラッシュする出力には import 済みの `io.print` を使う。

テスト： `spec/lang/prelude_test.almd`

---

<a id="12-entry-point"></a>

## 12. エントリーポイント

```almide
import env

effect fn main() -> Unit = {
  let args = env.args()!                   // argv without the program name
  let name = list.get(args, 0) ?? "world"
  println("Hello, ${name}!")
}
```

`main` は仮引数を取らない。コマンドライン引数には `env.args()` でアクセスする（プログラム名を除いた argv）。`effect fn main()` は `Result<(), String>` を返すよう自動で包むため、`ok(())` や明示的な `Result` 型を書く必要はない。

---

<a id="13-key-design-rules"></a>

## 13. 主要な設計規則

- 改行は文の区切り（セミコロン不要）
- ジェネリクスは `[]`、`<>` は使わない
- `<` と `>` は常に比較演算子
- 副作用には `effect fn`
- 例外なし。`Result[T, E]` を使う
- null なし。`Option[T]` を使う
- 継承なし。合成とプロトコルを使う
- マクロ、演算子オーバーロード、暗黙変換なし
- 空リストは `[]`、空マップは `[:]`（型注釈付き）
- `_` は match のワイルドカード、let の破棄、for の破棄、ラムダのワイルドカード仮引数に使う
- 修飾した stdlib 呼び出し（`string.len(s)`）、適用可能な UFCS（`s.len()`）、または明示的な選択 import を使う。全関数の暗黙的な非修飾 import はない
- x が Int の `println(x)` には明示変換が必要：`println(int.to_string(x))`

---

<a id="appendix-test-file-index"></a>

## 付録：テストファイル一覧

すべてのテストファイルは `spec/lang/` 配下にある：

| ファイル | 対象 |
|------|----------|
| `auto_derive_test.almd` | Eq/Hash の自動導出 |
| `bidirectional_type_test.almd` | 型推論 |
| `block_comment_raw_string_test.almd` | ブロックコメント、raw 文字列 |
| `bytes_test.almd` | Bytes 型 |
| `capture_clone_test.almd` | クロージャーキャプチャの意味論 |
| `codec_*_test.almd` | Codec 規約（シリアライズ） |
| `compose_test.almd` | `>>` 合成演算子 |
| `control_flow_test.almd` | If/else、match、ループ |
| `data_types_test.almd` | レコード、バリアント、タプル、コレクション |
| `default_args_test.almd` | 仮引数の既定値 |
| `default_fields_test.almd` | レコードフィールドの既定値 |
| `derive_conventions_test.almd` | 規約の導出（Eq、Repr など） |
| `edge_cases_test.almd` | パーサー・コンパイラの境界ケース |
| `effect_fn_test.almd` | エフェクト関数 |
| `eq_protocol_test.almd` | Eq プロトコル |
| `equality_test.almd` | 深い等値性の意味論 |
| `error_test.almd` | Result/Option 処理 |
| `escape_analysis_test.almd` | 所有権解析 |
| `expr_test.almd` | 基本式 |
| `fan_*_test.almd` | Fan ブロック（並行処理） |
| `for_test.almd`, `for_tuple_test.almd` | For-in ループ |
| `function_test.almd` | 関数宣言 |
| `generics_test.almd` | ジェネリック型と関数 |
| `guard_test.almd` | Guard 文 |
| `hash_protocol_test.almd` | Hash プロトコル |
| `heredoc_test.almd` | 複数行文字列 |
| `trait_impl_test.almd` | Convention method とプロトコル |
| `import_test.almd` | インポート宣言 |
| `interpolation_edge_test.almd` | 文字列補間の境界ケース |
| `lambda_test.almd` | ラムダ式 |
| `map_literal_test.almd`, `map_edge_test.almd` | マップリテラル |
| `match_edge_test.almd` | Match の境界ケース |
| `matrix_test.almd` | Matrix 型 |
| `named_args_test.almd` | 名前付き引数 |
| `open_record_test.almd` | オープンレコード型 |
| `operator_test.almd`, `operator_protocol_test.almd` | 演算子 |
| `panic_test.almd` | Panic の振る舞い |
| `pattern_test.almd` | パターンマッチ |
| `pipe_test.almd` | パイプ演算子 |
| `prelude_test.almd` | 組み込み関数 |
| `protocol_*_test.almd` | プロトコルシステム |
| `range_test.almd` | 範囲式 |
| `record_spread_test.almd` | レコードスプレッド |
| `scope_test.almd` | 変数のスコープ |
| `string_interp_test.almd`, `string_test.almd` | 文字列 |
| `tco_test.almd` | 末尾呼び出し最適化 |
| `top_let_test.almd` | トップレベル let 束縛 |
| `trait_impl_test.almd` | トレイト実装 |
| `tuple_test.almd` | タプル |
| `type_alias_test.almd` | 型エイリアス |
| `type_annotation_test.almd` | 型注釈 |
| `type_system_test.almd` | 型システム |
| `unwrap_operators_test.almd` | `!`、`??`、`?`、`?.` 演算子 |
| `value_utils_test.almd` | Value ユーティリティ |
| `variable_test.almd` | 変数束縛 |
| `variant_*_test.almd` | バリアント型 |
| `visibility_test.almd` | 可視性修飾子 |
| `while_test.almd`, `while_loop_test.almd` | While ループ |
