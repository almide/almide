<a id="almide-language-specification"></a>

# Almide 言語仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](SPEC.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](SPEC.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](SPEC.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md)の非規範的な読者向け翻訳です。規範的効力は英語原本にあり、本訳にはありません。自動同期されない固定スナップショットで、実装・コード例・安全性・性能を独立に実行検証していません。

> 原文上の注意：§9.8 の find_map 説明は Err を飛ばす記述と「後の要素は実行されない」が併記され、§13.2 は成功候補より後を実行しないと説明します。また、可変引数の E007 表記、旧 fan 形式、別名禁止の一般則と許容する別名などの文脈差も原文どおり保持しています。これらを翻訳で新しい規則に統合していません。「本番用」などの状態や予定は原文時点の主張であり、現在の保証ではありません。

**「自由に書くための言語ではなく、正しく収束するための言語。」**

> **規範的な位置づけ（2026-08-20 宣言、#530 / A0-1）：** この仕様の原本と `docs/specs/` は規範的です。**実装が ALS と食い違う場合、実装が誤っています。** インタープリターは ALS の実行可能な伴走実装（3方向オラクルの第3の判定役）で、仕様に従うものであり、それ自体が仕様ではありません。凍結された公開面と破壊的変更の方針は [docs/STABILITY.md](../../../STABILITY.md) にあります。既知の文章上の矛盾は #530 の未解消項目であり、それぞれ ALS 節を引用するフィクスチャとともに解消します。

---

<a id="0-design-philosophy"></a>

## 0. 設計思想

<a id="core-thesis"></a>

### 中心的な主張

LLM 向け言語設計の本質は表現力の最大化ではなく、**生成の各段階で有効な候補の集合を最小化すること**です。

<a id="four-pillars"></a>

### 4つの柱

| 原則 | 定義 |
|---|---|
| **予測可能** | 各生成段階で「次に有効な構文、API、意味」を厳密に絞り込める |
| **局所的** | ある場所の理解や変更に必要な情報をできる限り近くに置く |
| **修復可能** | エラー時にコンパイラが少ない手順でほぼ一意の修正候補を返す |
| **簡潔** | 構文上の雑音が少なく、意味の密度が高い。厳密かつ簡潔 |

<a id="7-design-principles"></a>

### 7つの設計原則

1. **正準性** — 同じ意味を表す主要な方法は原則1つにする
2. **表面の意味論** — 副作用、失敗可能性、省略可能性、可変性を構文または型に表す
3. **局所的推論** — 関数や式の意味を、おおむね近くの構文だけで理解できるようにする
4. **段階的完成** — 未完成コードを合法とし、型付きホールを埋めて進められるようにする
5. **修復優先** — コンパイラは拒否する道具ではなく修復する道具とし、診断を構造化する
6. **語彙の節約** — 標準ライブラリは同義語のない一貫した語彙を持つ
7. **魔法をなくす** — 実行時に意味を変える仕組み、文脈依存 DSL、暗黙の型変換は禁止する

<a id="trade-offs-intentionally-sacrificed"></a>

### トレードオフ（意図的に捨てるもの）

- 熟練者の記述の自由
- 文化的な「言語らしさ」と使い勝手を重視した DSL
- メタプログラミング能力（マクロ、リフレクションなし）
- 独自演算子構文と暗黙変換（既存演算子は宣言されたプロトコルを通してディスパッチ可能）
- 複数の戻り方やラムダ表記
- 暗黙のインスタンス解決によるアドホック多相

**目標：高い簡潔さ + 少ない自由度**

---

<a id="1-lexical-specification"></a>

## 1. 字句仕様

<a id="11-identifiers"></a>

### 1.1 識別子

```
Identifier ::= (XID_Start | "_") XID_Continue*
```

識別子は Unicode UAX #31 に従い、NFC で保存します。互換正規化した形が正準形と異なる文字（全角ラテン文字や数字など）は、変換せず拒否します。単独の `_` はプレースホルダーで、束縛名ではありません。キーワードとモジュール名は ASCII のままです。

テスト：`spec/lang/unicode_identifier_test.almd`、`tests/unicode_identifier_nfc_test.rs`。

識別子末尾の `?` は削除されました。述語は `Bool` を返す通常の識別子です（`is_` 接頭辞の規約：`is_empty`、`is_digit`）。

<a id="12-type-names"></a>

### 1.2 型名

型名は同じ Unicode 識別子文字を使います。先頭の大文字は型側の名前を示します。大小文字を持たない名前（CJK、かな、ハングルなど）は、モジュールが型または variant のケースとして宣言しない限り値側です。別モジュールからは修飾形を使います（例：`geo.住所`）。

テスト：`spec/lang/unicode_identifier_test.almd`、`spec/wasm_cross/unicode_identifiers.almd`。

<a id="13-literals"></a>

### 1.3 リテラル

```
IntLiteral       ::= [0-9]+ | "0" [xX] [0-9a-fA-F]+
                   | "0" [bB] [01]+ | "0" [oO] [0-7]+  // separators described below
FloatLiteral     ::= [0-9]+ "." [0-9]+ ([eE] [+-]? [0-9]+)?
StringLiteral    ::= '"' (char | "\n" | "\t" | "\r" | "\\" | "\"" | "\$")* '"'
InterpolatedStr  ::= '"' (char | "${" Expr "}")* '"'
SingleQuoteStr   ::= "'" (char | "\'" | "\\" | "\n" | "\t" | "\r")* "'"   // escapes only, no interpolation
HeredocStr       ::= '"""' ... '"""'                   // multiline, indent-stripped
RawHeredocStr    ::= 'r"""' ... '"""'                  // no escapes, no interpolation
RawStr           ::= 'r"' ... '"'                      // no escapes, no interpolation
BoolLiteral      ::= "true" | "false"
```

null リテラルは**ありません**。不在は `none`（`Option[T]` のコンストラクター）で表します。

二重引用符の文字列は `${expr}` による補間とバックスラッシュエスケープをサポートします。単一引用符の文字列はエスケープ列（`\'`、`\\`、`\n`、`\t`、`\r`）をサポートしますが、**補間はありません**。raw 文字列と raw heredoc は補間もエスケープもサポートしません。

heredoc 文字列は、空でない行の最小インデントに基づいて先頭の空白を取り除きます。最初の行（`"""` の後が空の場合）と最後の行（`"""` の前が空白だけの場合）は除去します。

数値リテラルは視覚的な区切りとして `_` を使えます（例：`1_000_000`）。

<a id="14-comments"></a>

### 1.4 コメント

```
// line comment (to end of line)
/* block comment (nestable) */
```

ブロックコメントは入れ子にできます。`/* outer /* inner */ still outer */` は有効です。

<a id="15-keywords-34"></a>

### 1.5 キーワード（34）

```
module  import  type    protocol for     in      fn      let
var     mut     if      then     else    match   ok      err
some    none    todo    true     false   not     and     or
pub     effect  test    guard    break   continue while
local   mod     fan
```

`strict` は削除されました。`strict types` / `strict effects` 宣言は何の検査も有効にしていなかったため、この語は再び通常の識別子です。

<a id="16-operators-and-delimiters"></a>

### 1.6 演算子と区切り記号

```
Operators:   +  -  *  /  %  ^  ==  !=  <  <=  >  >=  |>  >>  ..<  ...
Postfix:     !  ?  ?.  ??
Unary:       -  not
Logical:     and  or
Assignment:  =
Arrows:      ->  =>
Delimiters:  (  )  {  }  [  ]  ,  .  :  ;  |  _  @  ...
```

- `^` は累乗（右結合）。`**` を別名として受け入れます
- `+` はオーバーロードされ、数値は加算、文字列とリストは連結です
- `..<` は終端を含まない範囲、`...` は含む範囲です（Rust 形式の `..`/`..=` は E031 診断となり、機械的な fix-it が付きます）
- `...` はスプレッドです（レコード内）
- `_` はパターン束縛を破棄するか、式中の型付きホールを表します。パイプ／呼び出し引数のプレースホルダーではありません（E046、§9.10）
- `@` は extern 注釈などの属性を導入します

---

<a id="2-statement-separators"></a>

## 2. 文の区切り

**改行が文を区切ります。** セミコロンは複数の文を1行に置く場合にのみ使います。

```
let x = 1
let y = 2
let a = 1; let b = 2
```

<a id="21-line-continuation-rules"></a>

### 2.1 行継続の規則

次の場合、改行を無視して文が続きます。

**行末が以下の場合：**
- 二項演算子：`+`、`-`、`*`、`/`、`%`、`==`、`!=`、`<=`、`>=`、`<`、`>`、`and`、`or`、`|>`
- 区切り：`,`、`.`、`:`
- 開き括弧：`(`、`{`、`[`
- 矢印：`->`、`=>`
- 代入：`=`
- キーワード：`if`、`then`、`else`、`match`、`not`、`|`

**次の行が以下で始まる場合：**
- 名前が続く `.`（メソッドチェーン）
- 中置演算子：`|>`、`>>`、`+`、`-`、`*`、`/`、`%`、`^`、`==`、`!=`、`<`、`<=`、`>`、`>=`、`..<`、`and`、`or`
- `??`（フォールバックは `??` の行に置きます — E038）

行頭でオペランドに密着する `-`（`-1`）は新しい文を始め、行頭の `...` はスプレッドなので、どちらも継続しません。

```
let result = items
  .filter((x) => x > 0)
  .map((x) => x * 2)

text
  |> string.trim
  |> string.split(",")

list.first(xs)
  |> option.map((x) => x + 1)
  ?? 0
```

---

<a id="3-syntactic-categories"></a>

## 3. 構文カテゴリ

これは概要であり、2つ目の網羅的な文法ではありません。具体的な構文と例は、個別の[言語仕様](../../../specs/language.md)を参照してください。

```
Program   ::= ImportDecl* TopDecl*

TopDecl   ::= TypeDecl | ProtocolDecl | FnDecl
            | TopLetDecl | TestDecl

Stmt      ::= LetStmt | VarStmt | AssignStmt | GuardStmt | BreakStmt | ContinueStmt | Expr

Expr      ::= Literal | Name | InterpolatedStr
            | RecordExpr | SpreadExpr | ListExpr | MapExpr
            | CallExpr | MemberExpr | IndexExpr
            | PipeExpr | BinaryExpr | UnaryExpr
            | IfExpr | MatchExpr
            | ForInExpr | WhileExpr | FanExpr
            | BlockExpr | LambdaExpr
            | HoleExpr | TodoExpr
            | RangeExpr | TupleExpr
            | "(" Expr ")"
```

---

<a id="4-modules-and-imports"></a>

## 4. モジュールとインポート

パッケージの識別情報は `almide.toml` で宣言します。ソースファイルに `module` 宣言はありません。

<a id="41-import-declaration"></a>

### 4.1 インポート宣言

```
ImportDecl ::= "import" ModulePath
             | "import" ModulePath "as" Ident
             | "import" ModulePath "." "{" NameList "}"

ModulePath ::= Ident ( "." Ident )*
```

例：
```
import fs                       // stdlib module
import json                     // stdlib module (not auto-imported)
import mylib                    // user package
import mylib.parser             // sub-module import
import mylib as m               // alias: m.hello()
import self as app              // self-alias for the current package
import mylib.{Parser, Lexer}    // selective import
```

**禁止：ワイルドカードインポート。** `import fs.*` はコンパイルエラーです。

<a id="42-auto-imported-modules-prelude"></a>

### 4.2 自動インポートされるモジュール（Prelude）

以下の公開モジュールは `import` なしで利用できます。

```
string  list  int  float  bool  math  map  set  deque  heap
result  option  value  datetime  bytes  matrix
int8  int16  int32  int64  uint8  uint16  uint32  uint64  float32  float64
```

ほかの公開 stdlib モジュールには明示的な `import` が必要です。現在の一覧は[標準ライブラリリファレンス](../../../stdlib/README.md)を参照してください。正本は [`stdlib_info.rs`](../../../../crates/almide-types/src/stdlib_info.rs) の `TIER1_ALWAYS_ACCESSIBLE` と `AUTO_IMPORT_BUNDLED` です。`prim` は stdlib 内部実装モジュールで、ユーザー向け公開面ではありません。ユーザーソースからの参照は E085 で拒否します。廃止された `error`、`log`、`mem` モジュールは公開 stdlib に含まれません。[compute](../../../stdlib/compute.md) と [duration](../../../stdlib/duration.md) の時計の公開面は自動利用可能なチェッカー組み込みであり、インポート可能なモジュールではありません。

<a id="43-prelude-types"></a>

### 4.3 Prelude の型

暗黙にインポートされる型とコンストラクター：
- プリミティブ：`Int`、`Float`、それらの幅付き数値系列、`Bool`、`String`、`Unit`、`Bytes`。`Path` は `String` の別名
- コレクション：`List[T]`、`Map[K, V]`、`Set[T]`
- エラー処理：`Option[T]`、`Result[T, E]`
- コンストラクター：`some(x)`、`none`、`ok(x)`、`err(x)`
- 真偽値：`true`、`false`
- 境界：`Value`（`json.parse` は `Value` を返し、別の組み込み `Json` 型ではありません）

<a id="44-visibility-modifiers"></a>

### 4.4 可視性修飾子

```
fn pub_fn() -> String = ...       // public (default)
mod fn internal() -> String = ... // same project only (pub(crate) in Rust)
local fn helper() -> String = ... // same file only (private)
```

可視性は `fn`、`type`、`let` 宣言に適用します。
```
local type Internal = { data: String }
mod let THRESHOLD = 100
```

---

<a id="5-type-declarations"></a>

## 5. 型宣言

<a id="51-generics-----notation"></a>

### 5.1 ジェネリクス — `[]` 表記

**型引数は `[]` を使います。** `<>` は比較演算子専用です。

```
GenericParams ::= "[" TypeParam ( "," TypeParam )* "]"
TypeParam     ::= TypeName
```

```
Result[List[Map[String, Int]], Error]
fn transform[T, U](xs: List[T], f: Fn(T) -> U) -> List[U] = list.map(xs, f)
```

<a id="52-record-types"></a>

### 5.2 レコード型

```
type User = {
  id: Int,
  name: String,
}

type Pair[A, B] = {
  first: A,
  second: B,
}
```

<a id="53-variant-types"></a>

### 5.3 Variant 型

variant ケースには unit、タプル形式、レコード形式の3つがあります。

```
type Token =
  | Word(String)
  | Number(Int)
  | Eof

type Shape =
  | Circle(Float)
  | Rect{ width: Float, height: Float }
  | Point
```

インライン variant（先頭の `|` なし）：

```
type Direction = North | South | East | West
```

<a id="54-conventions"></a>

### 5.4 規約

```
ConventionClause ::= ":" TypeName ("," TypeName)*
```

規約名は型名の後、`=` の前に `:` で指定します。

```
type Color: Eq, Repr =
  | Red | Green | Blue
```

組み込みプロトコルには `Eq`、`Repr`、`Ord`、`Hash`、`Codec`、`Encode`、`Decode`、`Numeric` があります。要件と導出規則は[型システムリファレンス](../../../specs/type-system.md)を参照してください。プロトコルの宣言は、その要件の代わりにはなりません。

<a id="55-protocols"></a>

### 5.5 プロトコル

```
ProtocolDecl ::= "protocol" TypeName "{" ProtocolMethod* "}"
ProtocolMethod ::= ["effect"] "fn" Ident "(" Params ")" "->" TypeExpr
```

プロトコルは必要なメソッドの集合を定義します。型は `: ProtocolName` で充足を宣言します。メソッド自体は `Type.method` という名前の通常のトップレベル関数で、第1引数の型を `Type` と明示します。`impl Protocol for Type { ... }` ブロックはありません。規約メソッドは平坦なトップレベル関数のままです。入れ子も追加インデントもなく、型にメソッドを付ける方法は1つです。

```
protocol Serializable {
  fn serialize(a: Self) -> String
  fn deserialize(raw: String) -> Result[Self, String]
}

type Config: Serializable = { key: String, value: String }
fn Config.serialize(c: Config) -> String = c.key + "=" + c.value
fn Config.deserialize(raw: String) -> Result[Config, String] = ok(Config { key: raw, value: raw })
```

チェッカーは宣言された各規約メソッドをプロトコルの署名（引数数と引数／戻り値の型。`Self` は宣言する型に置換）と照合し、不一致ならそのメソッドに診断を付けます。不一致のメソッドを持つ `Type: Protocol` 注釈は `almide
check` で拒否され、後のネイティブコード生成で失敗するまで放置しません。現時点ではジェネリック型の署名検査を省略します（`Self` が型自身の型引数を持つ必要がありますが、まだそれを伝達していません）。

第1引数は名前と型を明示（`a: Config`）しても、単独の `self` と書いてもかまいません。後者は `self: Self` の糖衣構文で、`protocol { ... }` 宣言内と同様、規約メソッドの宣言型に解決します。

`Self` は実装する型のプレースホルダーです。組み込み規約はプロトコルであり、構造的導出と明示メソッドは[型システムリファレンス](../../../specs/type-system.md)に従います。

プロトコルはジェネリック境界として使えます。

```
fn show[T: Serializable](item: T) -> String = item.serialize()
```

動的ディスパッチはありません。プロトコル境界付きジェネリクスはすべてコンパイル時に単相化します。

<a id="56-type-alias"></a>

### 5.6 型別名

```
type Score = Int
type Handler = (String) -> String
```

- 透過的な別名：`Score` と `Int` は交換可能
- 関数型の別名は `(Params) -> Return` 構文を使用

<a id="57-type-application"></a>

### 5.7 型適用

```
List[String]
Result[User, ParseError]
Map[String, List[Int]]
```

<a id="58-tuple-types"></a>

### 5.8 タプル型

```
(Int, String)              // tuple type
(Int, String, Bool)        // 3-tuple
```

`.0`、`.1` などでアクセスします。

<a id="59-function-types"></a>

### 5.9 関数型

```
Fn(Int) -> String
Fn(Int, Int) -> Bool
```

括弧だけの形式（5.6参照）も等価で、どこでも使えます：`fn apply(f: (Int) -> Int) -> Int = f(3)`、`type Handler = (String) -> String`。

---

<a id="6-basic-type-environment"></a>

## 6. 基本的な型環境

<a id="primitives"></a>

### プリミティブ

```
Int  Float  Bool  String  Path  Unit
```

`Bytes` は独立した型で、`List[Int]` と交換可能ではありません。変換には [bytes API](../../../stdlib/bytes.md) を使います。`Path` は `String` の別名です。幅付き整数と浮動小数点の型は[型システムリファレンス](../../../specs/type-system.md)で説明します。

<a id="collections"></a>

### コレクション

```
List[T]  Map[K, V]  Set[T]
```

<a id="error-handling"></a>

### エラー処理

```
Option[T]   -- some(x) | none
Result[T, E] -- ok(x) | err(e)
```

<a id="boundary-types"></a>

### 境界型

```
Value
```

外部入力の受け手として使います。ドメインロジックで使う前に明示的な変換が必要です。

---

<a id="7-function-declarations"></a>

## 7. 関数宣言

```
FnDecl ::= Visibility? "effect"? "fn" Name GenericParams?
            "(" ParamList? ")" "->" TypeExpr "=" Expr

Visibility ::= "local" | "mod"        // default is public
ParamList  ::= Param ( "," Param )*
Param      ::= "mut"? Identifier ":" TypeExpr ( "=" Expr )?   // default value optional
```

修飾子の順序：`[local|mod]? effect? fn`

<a id="71-principles"></a>

### 7.1 原則

- 引数型は必須
- 戻り値型は必須
- 本体は単一の式（`=` の後）
- `effect fn` は副作用のある関数を示す
- `fan { }` はスコープ付き実行（`effect fn` 内のみ）。スケジューリングはコンパイラと選択したホスト経路による（§13）

```
fn add(x: Int, y: Int) -> Int = x + y

effect fn greet(name: String) -> Result[Unit, String] = {
  println("Hello, ${name}!")
  ok(())
}
```

<a id="72-default-arguments"></a>

### 7.2 既定引数

パラメーターには既定値を持たせられます。最初の既定値以降のすべてのパラメーターにも既定値が必要です。

```
fn connect(host: String, port: Int = 8080, secure: Bool = false) -> Connection = _
```

<a id="73-named-arguments"></a>

### 7.3 名前付き引数

呼び出し位置で引数に名前を付けられます。

```
connect("localhost")
connect("localhost", port: 443, secure: true)
connect(host: "localhost", secure: true)
```

位置引数の後に名前付き引数を置けます。名前付き引数の後に位置引数は置けません。

すべて名前付き引数による**レコード型**（またはレコードペイロードを持つ variant ケース）の呼び出しはレコード構築です。`Cfg(name: "x")` は別名であり、正準の `Cfg { name: "x" }` に正規化して同じ検証を行います（未知／重複／既定値のない欠落フィールドは E021）。レコードの位置引数とタプルコンストラクターの名前付き引数は拒否します（E021）。

括弧の別名はパターン内の通常のレコード型にも一致します（`Cfg(name) =>
...`）。variant のレコードペイロードケースには**一致しません**。そこでは波括弧形式だけを使います（`Circle { radius } => ...`。`Circle(radius) => ...` は不可。E021: "use a record pattern"）。

<a id="74-predicate-functions"></a>

### 7.4 述語関数

述語は `Bool` を返す通常の識別子です（`?` 接尾辞は削除済み）。

```
fn empty(xs: List[Int]) -> Bool = list.len(xs) == 0
fn tracked(index: Index, path: Path) -> Bool = _
```

<a id="75-effect-fn----explicit-side-effects"></a>

### 7.5 `effect fn` — 明示的な副作用

非 `effect` 関数から `effect fn` を呼ぶと、警告ではなく**コンパイルエラー**です。

```
fn pure_fn() -> String =
  fs.read_text("file.txt")    // Compile error: cannot call effect fn from non-effect fn
```

effect ビットは数学的な純粋性やケイパビリティカテゴリがゼロという主張ではありません。出力／中断の組み込みは通常の `fn` でも許容されます（§16.4）。透過的な関数パラメーターを介した呼び出しは、[エフェクトシステムリファレンス](../../../specs/effect-system.md)のコールバック規則に従います。

<a id="76-top-level-let----module-scope-constants"></a>

### 7.6 トップレベル let — モジュールスコープの定数

```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello, world"
```

可能な場合はコンパイル時に評価します。数値と単純な式は `const` になり、String と複雑な式は Rust コード生成で `LazyLock<T>` を使います。

<a id="77-extern-annotations"></a>

### 7.7 Extern 注釈

ターゲット固有の関数に対する FFI 束縛：

```
@extern(rust, "std::fs", "read_to_string")
effect fn read_text(path: String) -> Result[String, String] = _
```

この例はネイティブ Rust 束縛であり、移植可能な実装ではありません。以前の TypeScript バックエンドは廃止済みです。外部／ホスト束縛は、選択したターゲットとその権限／信頼境界を満たす必要があります。

<a id="78-mutable-parameters"></a>

### 7.8 可変パラメーター

パラメーターの `mut` は可変参照渡しを意味します。関数内のパラメーターへの代入は、呼び出しが戻った後、呼び出し元にその場の変更として見えます。新しい状態を持ち帰る戻り値はありません。

```
fn incr(mut x: Int) -> Unit = { x = x + 1 }

fn main() -> Unit = {
  var n = 5
  incr(n)
  println(int.to_string(n))   // 6
}
```

呼び出し元は `var` 束縛を渡す必要があります。`let` 束縛や一時式は、変更を書き戻す先がないため拒否します（E007）。`mut` は第1引数だけでなく、どのパラメーターにも付けられます。stdlib のその場での操作（`list.push`、`list.pop`、`list.clear` など）はこの仕組みで書きます。隠れたレシーバー規約ではなく、`mut` を付けた通常のパラメーターです。

---

<a id="8-statements"></a>

## 8. 文

<a id="81-let--var"></a>

### 8.1 let / var

```
let x = 1                   // immutable
let x: Int = 1              // with type annotation
var y = 2                   // mutable
y = y + 1                   // reassign (var only)
```

<a id="82-destructuring"></a>

### 8.2 分解

```
let { name, age } = user    // record destructure (one level only)
var (x, y) = pair           // var takes the let patterns; x and y are both vars
```

- `var` は `let` と同じパターンを取り、束縛された名前はすべて可変です（#3149）
- タプル分解は入れ子や位置の破棄が可能です：`var (x, (y, _)) = value`
- レコード省略形の分解は、汎用の入れ子 match パターンではありません
- 改名は許可しません

<a id="83-guard"></a>

### 8.3 guard

```
guard cond else expr
```

早期終了を伴う前提条件検査です。条件が偽なら else 式を返します。

```
effect fn validate(x: Int) -> Result[Int, String] = {
  guard x > 0 else err("must be positive")
  guard x < 1000 else err("too large")
  ok(x * 2)
}
```

<a id="84-reassignment"></a>

### 8.4 再代入

`var` で束縛された識別子だけに許可します。

```
var count = 0
count = count + 1
xs[0] = "updated"           // index write (var only)
m["key"] = value             // map write (var only)
```

---

<a id="9-expressions"></a>

## 9. 式

<a id="91-if-expression"></a>

### 9.1 if 式

```
if cond then expr else expr
```

- `else` を省略すると Unit 値の条件式になります。値を生成する2分岐式ではありません
- 条件は `Bool` でなければなりません。truthiness はありません
- 連鎖：`if a then x else if b then y else z`

<a id="92-match-expression"></a>

### 9.2 match 式

```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

**match は網羅的でなければなりません**（型チェッカーが強制）。

ガードは `if` の後に `Bool` 式を取ります。ガードが偽なら次のアームを試します。

<a id="93-patterns"></a>

### 9.3 パターン

```
Pattern ::= "_"                                      // wildcard
           | Identifier                               // bind
           | Literal                                  // int, float, string, bool
           | "some" "(" Pattern ")"                   // Option
           | "none"
           | "ok" "(" Pattern ")"                     // Result
           | "err" "(" Pattern ")"
           | TypeName                                 // unit constructor
           | TypeName "(" Pattern ("," Pattern)* ")"  // tuple constructor
           | TypeName "{" FieldPattern* ".."? "}"     // record constructor
           | "(" Pattern "," Pattern+ ")"             // tuple
```

`FieldPattern ::= Ident (":" Pattern)?`

リスト／末尾、or、as パターンもあります。束縛と網羅性の規則は[言語リファレンス](../../../specs/language.md)にあります。上記は網羅的なパターン一覧ではありません。

レコードパターンは部分一致に `..` を使えます。

```
match shape {
  Circle(r) => 3.14 * r * r,
  Rect{ width, height } => width * height,
  Rect{ width, .. } => width,
  Point => 0.0,
}
```

<a id="94-lambdas"></a>

### 9.4 ラムダ

```
(x) => expr
(x, y) => expr
(x: Int) => x + 1
items.map((x) => x * 2)
```

本体は任意の式で、複数行ラムダ用のブロック式も含みます。

```
let f = (x) => {
  let y = x * 2
  y + 1
}
```

形式は1つだけです。ラムダパラメーターには任意で型注釈を付けられます。

<a id="95-block-expression"></a>

### 9.5 ブロック式

ブロックの最後の式がその値です。

```
{
  let x = 1
  let y = 2
  x + y
}
```

<a id="96-forin"></a>

### 9.6 for...in

```
for item in items {
  println(item)
}

for (k, v) in map.to_list(config) {
  println(k + " = " + v)
}

for i in 0..<10 {
  println(int.to_string(i))
}
```

リストを反復します。ループ本体は `Unit` 型です。コレクションの反復には `for...in` を使います。

<a id="97-while"></a>

### 9.7 while

```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

条件が真である間ループします。本体は `Unit` 型です。`break` と `continue` はループ内の早期終了と次の反復への制御を提供し、外側の関数から値を返すものではありません。

<a id="98-fan-deterministic-data-parallelism"></a>

### 9.8 fan（決定的なデータ並列性）

```
fan { expr1, expr2, expr3 }
```

スコープ内のアーム式を join し、結果をタプルとして返します。`effect fn` 内でのみ有効です。アームはセミコロンではなくカンマまたは改行で区切ります。アームとして使うブロック式には通常の文を含められます。

`fan` は意味ではなく**スケジューリング**の構文です。並列実行できても、プログラムの観測可能な動作はリスト順の逐次評価と厳密に同じと定義します。[並行性の立場](https://github.com/almide/almide/blob/develop/docs/roadmap/active/concurrency-stance.md)を参照してください。

規則：
- すべての兄弟アームを join し、キャンセルはありません。どれかが `Err` を返すと、ブロック値は**リスト順で最初の** `Err`（実時間で最初に失敗したものではない）です。これにより結果はスケジューリングに依存しません。終了しない兄弟アームがあると、逐次評価と同様、プログラムは終了しません
- `fan` ブロック内に `let`/`var`/`for`/`while` はなく、式だけです
- 外側のスコープからの `var` 捕捉はありません（データ競合を防止）

ライブラリ形式：
- `fan.map(xs, f)` — fan 公開面でコレクションを map（全要素を実行し、結果はリスト順）
- `fan.settle(xs, f)` — 全要素の Result をリスト順に返す
- `fan.min(clock, xs, f)` — 時計が選ぶ1要素（§13.5）

結果が `Ok` になるリスト順の最初の要素は fan 形式では**ありません**。逐次的なので `list.find_map(xs, (x) => f(x)?) ?? fallback` です（`?` は要素の `Err` を `none` に変え、その要素は飛ばされ、後の要素は実行されません）。ブロック表記は `??` の連鎖、`a() ?? b() ?? fallback` です。

（サンクリストの呼び出し形式 `fan.race(thunks)` / `fan.any(thunks)` / `fan.settle(thunks)` は、検査時に移行ヒントを出す `E027` の廃止記録です。`fan.race { }`（方言エポック14で `fan.min` に改名）と `fan.any { }` / `fan.any(xs, f)`（同エポックで削除。`??` の連鎖と `list.find_map` が同じことを表す）も同様です。リスト順の意味論では、古い `fan.race(thunks)` は厳密に `thunks[0]()` でした。）

<a id="910-pipe"></a>

### 9.10 パイプ

```
text |> string.trim |> string.split(",")
```

`x |> f` は `f(x)` と等価です。

複数引数の関数では、パイプされた値は**第1**引数になります。

```
xs |> list.filter((x) => x > 0)
text |> string.split(",")
```

プレースホルダーはありません。呼び出し引数の `_` は拒否され（E046）、`text |> string.split(_, ",")` では `_` を除く修正を示します。別の位置へ渡すにはラムダへパイプします：`x |> (v) => f(a, v)`。

**match へのパイプ：**

```
value |> match {
  some(x) => x,
  none => "default",
}
```

<a id="911-ufcs-uniform-function-call-syntax"></a>

### 9.11 UFCS（統一関数呼び出し構文）

UFCS 解決が `f` を選ぶと、`x.f(y)` は `x` を第1引数として渡します。これはスコープ内の任意の `f` と構文解析時に無条件で等価になるという意味ではありません。

```
string.trim(text)       // canonical form
text.trim()             // UFCS form -- equivalent

string.split(text, ",")
text.split(",")         // equivalent
```

解決はレシーバーの型と字句スコープに依存します。呼び出し可能なレコードフィールド、組み込みモジュール関数、規約／プロトコルメソッド、ユーザー／インポート関数には異なる解決経路があります。ローカル束縛はモジュール名を隠せます。`crates/almide-frontend/src/check/calls_ufcs.rs` を参照してください。

<a id="912-range"></a>

### 9.12 範囲

```
0..<5           // [0, 1, 2, 3, 4]    exclusive end
1...5           // [1, 2, 3, 4, 5]    inclusive end
for i in 0..<n { ... }   // no list allocation (optimized)
```

<a id="913-record-and-spread"></a>

### 9.13 レコードとスプレッド

```
let alice = { name: "alice", age: 30 }
let bob = { ...alice, name: "bob" }       // age inherited from alice
{ name }                                   // shorthand: { name: name }
```

<a id="914-list"></a>

### 9.14 リスト

```
[1, 2, 3]
[]                        // empty list (type inferred from context)
xs[0]                     // index access
xs[i] = value             // index write (var only)
```

<a id="915-map"></a>

### 9.15 Map

```
["a": 1, "b": 2]         // map literal
[:]                       // empty map (requires type annotation)
let m: Map[String, Int] = [:]
m["key"]                  // index access (returns Option[V])
m["key"] = value          // index write (var only)
```

<a id="916-tuple"></a>

### 9.16 タプル

```
(1, "hello")              // tuple literal
let pair = (1, "hello")
pair.0                    // index access: 1
pair.1                    // "hello"
```

<a id="917-string-interpolation"></a>

### 9.17 文字列補間

```
let name = "world"
let msg = "hello ${name}, 1+1=${1 + 1}"
```

二重引用符の文字列と非 raw heredoc で動作します。単一引用符文字列、raw 文字列、raw heredoc は補間しません（§1.3）。

<a id="918-hole--todo"></a>

### 9.18 ホール / Todo

```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```

`_`（ホール）と `todo(msg)` は任意の期待型を受け入れます。コンパイラは期待型、利用可能な変数、提案を報告します。

---

<a id="10-operators"></a>

## 10. 演算子

<a id="precedence-lowest-to-highest"></a>

### 優先順位（低い順）

| レベル | 演算子 | 結合性 |
|-------|-----------|---------------|
| 1 | `or` | 左 |
| 2 | `and` | 左 |
| 3 | `==` `!=` `<` `<=` `>` `>=` | 非結合（連鎖はエラー） |
| 4 | `\|>`（パイプ） | 左、非対称（下記参照） |
| 5 | `..<` `...`（範囲） | なし |
| 6 | `+` `-`（加減） | 左 |
| 7 | `*` `/` `%`（乗除） | 左 |
| 8 | `^`（累乗。`**` は別名） | 右 |
| 9 | `>>`（合成） | 左（二項で最も強い） |
| 10 | `-` `not`（単項） | 接頭 |
| 11 | `.` `()` `[]` `!` `?` `?.` `??`（後置） | 左 |

- **`\|>` は非対称**です。右辺は1つの呼び出し／合成の連鎖です。その後のほかの演算子はパイプ結果に適用します。`xs \|> list.map(f) + ys` は `(xs \|> list.map(f)) + ys` で、`xs \|> f >> g` は合成 `f >> g` へパイプします
- 代入（`=`）は文であり、演算子ではありません
- 演算子構文は固定です。既存演算子は宣言されたプロトコル要件（`Eq`、`Numeric` など）を通してディスパッチできます。任意の新しい演算子記号はサポートしません
- `&&` と `||` は `and`/`or` を使うヒント付きで拒否し、`++` は `+` を使うヒント付きで拒否します

<a id="operator-semantics"></a>

### 演算子の意味

| 演算子 | 意味 |
|----------|---------|
| `+` | 加算（Int、Float）または連結（String、List） |
| `-` `*` `/` `%` | 算術 |
| `^`（`**`） | 累乗（ビット XOR は `int.bxor`） |
| `==` `!=` | 深い等価性（Fn 以外の全型） |
| `<` `<=` `>` `>=` | 比較 |
| `and` `or` `not` | 真偽値の論理 |
| `\|>` | パイプ |
| `>>` | 関数合成 |
| `..<` `...` | 範囲（終端を含まない／含む） |
| `!` `?` `?.` `??`（後置） | アンラップ／Option 化／オプショナルチェーン／フォールバック |

等価性は型に従います。Secret／規約の等価性はターゲットコード生成前に関数呼び出しへ変換される場合があります。通常の Rust の等価性は `almide_eq!` を使います。Wasm は値の型に応じて等価性を実装し、サポートする複合値の構造比較を含みます。汎用的な生メモリのバイト等価性ではありません。フロントエンドの `lower/expressions_access.rs::lower_eq_protocol` と `crates/almide-wasm/src/equality*.rs` を参照してください。

---

<a id="11-error-model"></a>

## 11. エラーモデル

<a id="111-three-layer-strategy"></a>

### 11.1 3層戦略

| 層 | 仕組み | 用途 |
|---|---|---|
| **通常の失敗** | `Result[T, E]` | 解析、検証、I/O、検索 |
| **プログラマーの誤り** | `panic` | 到達不能、不変条件違反 |
| **テスト** | `assert_eq`、`assert` | テストのアサーション |

例外は**存在しません**。`throw`/`catch` はありません。

<a id="112-unwrap-operators"></a>

### 11.2 アンラップ演算子

`Result[T, E]` と `Option[T]` の明示的エラー処理のための3つの後置演算子：

```
expr!          // context-dependent unwrap/propagation; see the context table below
expr ?? val    // unwrap or — use fallback on failure
expr?          // to option — convert Result to Option (err → none)
```

```
effect fn load_json(path: String) -> Result[Value, String] = {
  let text = fs.read_text(path)!       // err → propagates
  let config = json.parse(text)!       // err → propagates
  ok(config)
}

let port = int.parse(input) ?? 8080    // err → use 8080
let name = map.get(config, "key") ?? "default"  // none → use "default"
```

`!` は純粋で失敗可能な関数とラムダでも使えます。テストブロックの例外を含め、動作は戻り値の文脈に依存します。すべての effect 呼び出しでの自動伝播の同義語ではありません。正確な規則は [Result/Option の文脈表](../../../specs/result-option-effect.md)を参照してください。

<a id="113-typed-error-variants"></a>

### 11.3 型付きエラー variant

エラーの種類で分岐するには、エラーパラメーターに variant 型を使います。

```
type LoadError = | NotFound(String) | ParseFailed(String)

effect fn load_json(path: String) -> Result[Value, LoadError] = {
  let text = fs.read_text(path)
    |> result.map_err((e) => NotFound(e))!
  let config = json.parse(text)
    |> result.map_err((e) => ParseFailed(e))!
  ok(config)
}

match load_json("app.json") {
  ok(config) => use(config),
  err(NotFound(path)) => create_default(),
  err(ParseFailed(msg)) => println("Bad config: ${msg}"),
}
```

---

<a id="12-holes-and-incomplete-code"></a>

## 12. ホールと未完成コード

**言語の中核機能**です。段階的な開発を可能にします。

```
fn parse(text: String) -> Ast = _
fn optimize(ast: Ast) -> Ast = todo("implement constant folding")
```

<a id="compiler-obligation"></a>

### コンパイラの義務

ホールを見つけるとコンパイラは以下を返します。
- 期待型 `T`
- スコープ内の利用可能な変数と型
- 期待型を返せる関数候補
- 式の提案

---

<a id="13-concurrency-fan"></a>

## 13. 並行性：`fan`

<a id="131-fan-block"></a>

### 13.1 fan ブロック

`fan { }` は式を1つのスコープ付き単位として実行します。ネイティブはスコープ付きスレッドを使え、対応する埋め込み Wasm 経路はインスタンスワーカーを使えます。順序付きアームと未対応の並列形式は逐次実行します。観測可能な契約はアーム順の評価であり、全アームの並列実行や全 Wasm ホストの同じスケジューラーを約束するものではありません。`effect fn` 内でのみ有効です。

```
effect fn main() -> Result[Unit, String] = {
  let (a, b) = fan {
    fetch_user(1)
    fetch_posts(1)
  }
  println("${a}, ${b}")
  ok(())
}
```

結果をタプルとして返します。すべての兄弟アームを join し、キャンセルしません。どれかの式が `Err` を返すと、ブロック値はリスト順で最初の `Err` です（最小添字。どのアームが時間的に先に失敗したとしても）。

<a id="132-fanmap-and-the-block-head-fansettle"></a>

### 13.2 fan.map とブロック先頭の fan.settle

```
let results = fan.map(urls, (url) => fetch(url))    // Result[List[B], String], list order
let all     = fan.settle { job_a(), job_b() }       // tuple of per-arm Results, arm order
let first   = fetch_a() ?? fetch_b() ?? fallback    // NOT a fan form: first Ok in SOURCE order
```

- `fan.map(xs, f)` は全要素を実行し、結果はリスト順です。リスト順で最初の `Err` が map 全体の `Err` になります
- `fan.settle { ... }` — 全アームが全ターゲットでアーム順に完了まで実行され（タプルに変換）、結果は各アームの Result のタプルです
- ソース順で最初の `Ok` は `??` の連鎖で表します（左側が失敗したときだけ右側を評価するので、成功候補より後は実行しません）。リストでは `list.find_map(xs, (x) => f(x)?) ?? fallback` です。選択は定義上逐次的なので、どちらも fan 形式ではありません。`fan.any { }` / `fan.any(xs, f)` は方言エポック14で削除されました（E027 は両方の代替を示します。ADR-0028）
- サンクリストの表記 `fan.any([...])` / `fan.settle([...])` は、それ以前に削除されました（E027 が移行を示します）

<a id="133-deterministic-time-compute-and-duration"></a>

### 13.3 決定的時間：`Compute` と `Duration`

時間量は閉じた単位集合（`ns / us / ms / s / min / h`）のモジュール修飾コンストラクターで構築し、名義的な時計型を持ちます。

```
compute.ms(100)     // Compute  — deterministic compute time (budgets)
duration.ms(5000)   // Duration — wall-clock time (oracle-tier surfaces)
```

- 時間が期待される場所の単独 `Int` は型エラーです。2種類の時計は混ざりません（防壁はチェッカー内だけにあり、変換時には両方ともナノ秒の `Int` に消去します）
- 負のコンストラクター引数は実行時に決定的に中断します（stderr に `Error: negative time: compute.us(-5)`、終了1。両ターゲットで同一）。構築時のオーバーフローは折り返さず最大値に飽和します
- 決定的時間は固定した抽象機械の課金単位で計測します（関数入口。ただしループも再帰もない呼び出し先は免除。ループ先頭、`+` 文字列連結。補間は対象外。#3778 以降は全ターゲットで同じ表）。版付きの較正定数（CM-1）によって宣言ナノ秒から変換します。契約は**比率のみ**です。宣言時間は宣言された範囲内で実時間を近似します（常設 CI ゲートが測定）が、**計量自体**は正確でマシン非依存です。完全な規範規則：`docs/adr/0001-deterministic-time-units.md`。

<a id="134-fanbounded--deterministic-compute-budget"></a>

### 13.4 fan.bounded — 決定的な計算予算

```
let r = fan.bounded(compute.ms(100)) { work(input) } ?? fallback
```

- 決定的な計算予算の下で本体を実行します。計量した消費が予算を超えれば `Err`、それ以外は `Ok(body value)` です。型：`Result[T, String]`
- 判定はプログラムと入力**だけ**の関数です。同じプログラムは、すべてのターゲットとマシンで同じ宣言ナノ秒において `Ok` から `Err` に切り替わります
- 予算は外側の領域との最小値を上限にします（内側は残額を超えて消費できません）。現時点ではソースで領域を入れ子にできません。本体は純粋なので、領域本体内の `fan.*` 形式は E007 です
- 本体は**純粋**な文脈で検査し、通常の（非 `Result`）値を返すブロックです

<a id="135-fanminclock--one-arm-selected-by-the-clock-you-pass"></a>

### 13.5 fan.min(clock) — 渡した時計で1つのアームを選ぶ

```
let b = fan.min(compute.us(50)) { a(), b() } ?? fallback              // least deterministic compute within the budget
let n = fan.min(compute.us(50), xs, (x) => ok(solve(x))) ?? fallback  // mapper form, per-element budget
```

- 時計引数は**必須**です。その型が「最小」を何で測るかを示し、値が予算です。`fan.min { … }` と `fan.min(xs, f)` は構文解析エラー（"fan.min requires a clock argument"）です。時計なしでは `fan.min { 3, 5 }` が最小の値と読めてしまいます
- `Compute` 時計（決定的な層）：各アームを決定的時計で計量し、`(spend, index)` が**辞書順で最小**のアームが勝ちます。つまり最も低コストのアームで、同値はソース順で解決します。予算超過のアームは除外し、認められたアームがなければ `err("fan.min: no branch succeeded within budget")` です。型：`Result[T, String]`。すべてのアームをソース順に1つずつ実行します。アームは純粋です（E006）。`Err` を返したアームは自ら失格になります
- **マッパー形式**は1引数の純粋ラムダを動的リスト上で競わせます。アーム `i` は `f(xs[i])`、添字順はリスト順、予算は要素ごとで、ブロック形式と同じ法則です。マッパーは `Result` を返す必要があります（マッパー形式の契約）。`ok(v)` は競争し、`err(_)` は要素を失格にします。空リストは、認められたアームがない `Err` です
- `Duration` 時計（オラクル層）— 決定済み（ADR-0028、2026-10-18）ですが後続作業（#3788）で導入予定で、現時点では型エラーです。予定する規則：アームは同時に開始し、最初に `Ok` で**完了**したものが勝ちます。アームは I/O を行えます。入口から1つの期限を共有します（アームごとではない）。敗者は次の課金地点または I/O 境界でキャンセルされ、`fan.min` はその完了を待ってから戻ります（Haskell の `race`、Python の `TaskGroup`、Kotlin、Swift のような構造化方式で、JS の動かし続ける方式ではない）。敗者がすでに行った副作用は残り、stdout は破棄し、勝者の値だけを返します。ネイティブは並列、wasm は非同期対応まで順に実行します（アーム0が成功すれば勝つのも合法なスケジュール）。ω の記録／再生で勝者添字を固定します
- 履歴：以前の仕様は「全ターゲットで結果が同じ」（§13.1）と「最初に完了したアームが勝つ」（当時の§13.2）を同時に約束していました。後者は実時間のスケジューリングを観測可能にするため、両立しません。実時間の race は0.42.0で削除され、決定的な先頭構文は `fan.race` として復活し、方言エポック14で `fan.min` に改名されました（ほかのエコシステムで `race` は「最初に完了」を意味するため）。`Compute` 時計では速いアームが勝ち、全ターゲットが勝者に合意します（T9）。`Duration` 時計はオラクル層で、勝者はホスト入力です

<a id="136-fantimeout--the-oracle-tier-wall-clock-deadline"></a>

### 13.6 fan.timeout — オラクル層の実時間期限

```
let t = fan.timeout(duration.ms(5000)) { work(input) } ?? fallback
```

- 期限は `Duration`（実時間）です。課金地点で**協調的に**検査し、操作の途中では検査しません（Go context のキャンセルモデル）。入れ子の期限は絶対期限の最小値を上限にします
- 判定は **ω 相対**です。どの検査で期限が発火するかはホスト入力です（ADR-0001 S8）。記録した ω はバイト単位で同一に再生します（ネイティブでの記録は `ALMIDE_OMEGA_RECORD=1`、再生は `ALMIDE_OMEGA=<n>`。再生は時計ではなく記録した中断点を使います）。ネイティブと埋め込み Wasm の再生は `tests/fan_timeout_replay_test.rs` で検証し、異なる再生序数でも両経路の観測可能な進行が一致します（#3781）。`ALMIDE_OMEGA` はコンパイラ／実行の呼び出しに設定します。中断点はビルド成果物に焼き込まれ、既存バイナリがホストから動的に読みません。これはタイムアウト中断点の再生であり、任意の I/O やすべての非同期バグの記録ではありません。両端は絶対的です。作業より大きい期限では必ず完了し、発散する本体はどの有限期限でも必ず中断します

<a id="137-rules"></a>

### 13.7 規則

- すべての `fan` 形式は `effect fn` 内だけです。純粋関数は fork できません（`fan.bounded` / `fan.timeout` 本体と `fan.min(compute…)` アーム自体は純粋として検査します）
- `var` の捕捉なし。外側の `let` 束縛だけです（データ競合を防止）
- 非構造的な `spawn` なし。並行性はすべてスコープ付きです
- アームが `@ordered` stdlib 関数に到達する `fan { }`（`env.set`、`fs` 書き込み、stdin 読み取り、`process` spawn など、兄弟アームが読めるプロセス内状態）は全ターゲットでアーム順に実行します（ADR-0024 D3）。どの経路でも観測結果は逐次的です
- 設計上、兄弟アームのキャンセルはありません。キャンセルされたアームの副作用はキャンセル時点に依存し、それこそモデルが排除するスケジューリング依存だからです。ブロックは全兄弟を join し、リスト順で最初の `Err` を報告します。（中断するのは課金地点での純粋本体だけです。`fan.bounded`、`fan.timeout`、`fan.min(compute…)` アームが該当します。予定される `fan.min` の `Duration` 時計（#3788）には別の敗者キャンセル契約があり、現時点では未実装です。`fan.timeout` は1つの純粋本体で、敗者アームを競わせません）
- 予算は `Compute` であり、`Duration` や単独の `Int` ではありません。実時間制限は決定的層ではなくオラクル層（`fan.timeout`）に属します

---

<a id="14-testing"></a>

## 14. テスト

<a id="141-test-declaration"></a>

### 14.1 test 宣言

```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

テストは関数と同じファイルのトップレベル宣言です。別のテストファイルは必須ではありません（専用テストファイルの規約は `*_test.almd`）。

<a id="142-assertion-functions"></a>

### 14.2 アサーション関数

```
assert(cond: Bool)                    // fails if false
assert_eq(actual: T, expected: T)     // fails if actual != expected
assert_ne(actual: T, expected: T)     // fails if actual == expected
```

<a id="143-running-tests"></a>

### 14.3 テスト実行

```bash
almide test                      # all .almd with test blocks (recursive)
almide test spec/lang/           # directory
almide test spec/lang/expr_test.almd  # single file
almide test --run "pattern"      # filter by name
```

---

<a id="15-naming-conventions"></a>

## 15. 命名規約

<a id="predicates"></a>

### 述語

述語は通常の識別子 `is_empty`、`exists`、`contains` を使います。戻り値型は `Bool` であり、名前末尾の `?` はありません。

stdlib の述語は `is_` 接頭辞の規約を使います：`string.is_empty(s)`、`string.is_digit(s)`。

<a id="stdlib-naming-rules"></a>

### Stdlib の命名規則

| 規約 | 規則 | 例 |
|---|---|---|
| モジュール接頭辞 | 正準形は `module.function()` | `string.len(s)`、`list.get(xs, i)` |
| 述語 | `Bool` を返す通常の名前。多くは `is_` 接頭辞 | `string.is_empty(s)`、`fs.exists(path)` |
| 同義語なし | 操作ごとに名前は1つ | `length`/`size`/`count` ではなく `len` |
| 対称な組 | 逆操作には対応する名前 | `split`/`join`、`to_string`/`to_int` |
| 戻り値型の一貫性 | 失敗可能な検索は `Option`、I/O は `Result` | `list.get() -> Option[T]` |

---

<a id="16-standard-library"></a>

## 16. 標準ライブラリ

[標準ライブラリリファレンス](../../../stdlib/README.md)が現在の公開 API 一覧です。各モジュールページは署名、文書、例、`@since` 情報を含み、署名索引は `make stdlib-docs` によってコンパイラのモジュールインターフェースから生成します。この仕様で一覧を重複させたり、別の関数件数を維持したりはしません。

<a id="161-auto-imported-modules"></a>

### 16.1 自動インポートモジュール

インポート規則とコンパイラ管理のレジストリは [§4.2](#42-auto-imported-modules-prelude) を参照してください。現在の関数と型は[リファレンス](../../../stdlib/README.md)の各モジュールページに従います。

<a id="162-import-required-modules"></a>

### 16.2 インポート必須モジュール

自動インポート集合外の公開モジュールには `import module` が必要です。署名にどの呼び出しが effectful かを示します。インポートが必要なこと自体はエフェクト分類ではありません。

<a id="163-bundled-modules-pure-almide-import-required"></a>

<a id="163-bundled-modules"></a>

### 16.3 同梱モジュール

同梱は実装を示し、インポート方針ではありません。セルフホストの `.almd` モジュールには、自動インポートと明示的インポートの両方の API が含まれます。実装の配置は [`stdlib/README.md`](../../../../stdlib/README.md)、ユーザー向けインポート規則は [§4.2](#42-auto-imported-modules-prelude) を参照してください。

<a id="164-built-in-functions"></a>

### 16.4 組み込み関数

どこでもインポートなしで利用できます。

```
println(s: String)              // print line to stdout
eprintln(s: String)             // print line to stderr
assert_eq(a: T, b: T)          // assert equal
assert_ne(a: T, b: T)          // assert not equal
assert(cond: Bool)              // assert true
```

`print` 関数はありません（改行なしの出力は `io.print`）。`println` は `String` を要求し、暗黙変換しません。

`println`、`eprintln`、`panic`、`assert`、`assert_eq`、`assert_ne` は通常の `fn` で許容されます。書き込みまたは中断だけで読み取りはせず、呼び出し元を effectful にしません。ほかの I/O は引き続き effect の制限を受けます。正確な範囲とテストは [effect-system.md §2.1](../../../specs/effect-system.md#21-output-and-abort-builtins-are-admissible-in-a-pure-fn) を参照してください。

---

<a id="17-entry-point"></a>

## 17. エントリーポイント

```almide
import env

effect fn main() -> Unit = {
  let args = env.args()
  println(list.get(args, 0) ?? "no argument")
}
```

エントリー関数は引数を取らず、ランタイムが `main()` を呼びます。引数には `env.args()` を使います。プログラム名は除かれ、要素0が最初のユーザー引数です。失敗可能な戻り値経路が必要なら、エントリー関数は代わりに明示的な `Result[Unit, E]` を返せます。

---

<a id="18-codegen-architecture"></a>

## 18. コード生成アーキテクチャ

<a id="181-multi-target"></a>

### 18.1 複数ターゲット

Almide は複数のターゲットへコンパイルします。

| ターゲット | 状態 | 方式 |
|--------|--------|----------|
| **Rust** | 本番用 | 完全な所有権解析、借用／clone パス |
| **WASM** | 本番用 | 直接出力（線形メモリ、WASI） |

<a id="182-codegen-v3-pipeline"></a>

### 18.2 Codegen v3 パイプライン

ネイティブ Rust 経路は型付き IR を意味／所有権パスで変換し、ウォーカー／テンプレートで Rust を出力します。これは Wasm の出力経路とは異なります。Wasm は直接の構造的エミッターを使い、ホストの公開面に応じて core／component のパッケージ形式を選びます。維持されているパイプラインの説明は [codegen](../../../specs/codegen.md) と[アーキテクチャ](../../../ARCHITECTURE.md)を参照してください。

伝播の変換はソースの `!` を戻り値の文脈に従って処理し、すべての effect 呼び出しに伝播を挿入するわけではありません。Fan の変換は、該当する順序契約を保ちながら対応する実行基盤を選びます。

<a id="183-cross-target-semantics"></a>

### 18.3 ターゲット間の意味論

| 機能 | Rust | WASM |
|---------|------|------|
| `Result[T, E]` | 宣言／推論したエラー型 `E` を保つ Result の入れ物 | 対応するペイロード配置を持つタグ付き値 |
| `Option[T]` | `Option<T>` | `T` のペイロード配置を持つタグ付き値 |
| `effect fn` | effect ビットと戻り値経路は別。`!` は文脈を使用 | 対応する effect／戻り値の意味論。無条件の String エラーではない |
| `==` / `!=` | プロトコル呼び出しまたは通常の構造的等価性 | 型に従う等価性。汎用バイト比較ではない |
| String / List の `+` | ランタイム／ヘルパー、または最適化されたネイティブ操作 | 型固有の構造的／ランタイム操作 |
| `fan { }` | 必要に応じスコープ付き並列または順序付き実行 | 逐次、または対応する埋め込みホストのワーカー経路 |

---

<a id="19-prohibitions"></a>

## 19. 禁止事項

| # | 禁止事項 | 理由 |
|---|---|---|
| 1 | 暗黙の型変換 | LLM が型を混同する |
| 2 | truthiness | 条件は `Bool` のみ |
| 3 | 独自演算子構文 | 演算子語彙は固定。型に従う組み込み／プロトコルのディスパッチは対応を継続 |
| 4 | 例外（`throw`/`catch`） | 制御フローが見えない |
| 5 | 複数のラムダ表記 | 生成の分布が分散する |
| 6 | 内部 DSL | 文脈依存が強い |
| 7 | ワイルドカードインポート | 名前の出所が不明 |
| 8 | `null` | `Option[T]` に統一 |
| 9 | API 別名 | 語彙が増えるほど幻覚が増える |
| 10 | `<>` ジェネリクス | 比較演算子と曖昧になる |
| 11 | モンキーパッチ／オープンクラス | 実行時の意味変更が見えない |
| 12 | マクロとリフレクション | 言語の公開面は固定する必要がある |
| 13 | `return` キーワード | 最後の式が常に値 |
| 14 | `class` キーワード | レコードと variant には `type` を使う |

---

<a id="20-compiler-diagnostics"></a>

## 20. コンパイラ診断

<a id="201-one-error-one-root-cause"></a>

### 20.1 1つのエラー、1つの根本原因

連鎖する派生エラーを抑制し、1つの根本原因を提示します。

<a id="202-structured-error-output"></a>

### 20.2 構造化されたエラー出力

以下は診断の説明例であり、CLI JSON の通信スキーマではありません。利用側は文書化された [CLI](../../../specs/cli.md) の出力と、そのスキーマ／バージョン契約を使ってください。

```json
{
  "kind": "type_mismatch",
  "location": { "file": "app.almd", "line": 12, "col": 5 },
  "expected": "Result[Config, IoError]",
  "actual": "String",
  "suggestions": ["ok(text)", "parse_config(text)"]
}
```

<a id="203-rejected-syntax-hints"></a>

### 20.3 拒否する構文へのヒント

コンパイラはほかの言語でよく使われる構文を認識し、実行可能な修正ヒントを出します。

```
Prefix '!x' is not boolean negation in Almide
  Hint: Use 'not x' for boolean negation, not '!x'.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.

'||' is not valid in Almide at line 8:10
  Hint: Use 'or' for logical OR.

'&&' is not valid in Almide at line 8:10
  Hint: Use 'and' for logical AND.
```

<a id="204-auto-fix-candidates"></a>

### 20.4 自動修正候補

- インポートの欠落
- 型変換候補
- match アームの欠落
- `effect` 修飾子の欠落
- 純粋関数が effectful なコードを呼ぶ場合の `effect fn` 提案

<a id="205-official-formatter"></a>

### 20.5 公式フォーマッター

```bash
almide fmt app.almd
```

- 1つの AST から厳密に1つの整形結果を出す
- インポートはアルファベット順
- 末尾のカンマを含める
- 固定の改行規則

---

<a id="21-typing-rules"></a>

## 21. 型付け規則

<a id="variables"></a>

### 変数

```
G(x) = T
-----------
G |- x : T
```

<a id="let"></a>

### let

```
G |- e : T
--------------------
G, x:T |- let x = e
```

<a id="if"></a>

### if

```
G |- c : Bool    G |- t : T    G |- e : T
-------------------------------------------
G |- if c then t else e : T
```

<a id="functions"></a>

### 関数

```
G, x1:T1, ..., xn:Tn |- body : R
------------------------------------------
G |- fn f(x1:T1,...,xn:Tn) -> R = body
```

<a id="option--result-constructors"></a>

### Option / Result コンストラクター

```
G |- e : T                          G |- e : T
--------------------                --------------------
G |- some(e) : Option[T]           G |- ok(e) : Result[T, E]

G |- e : E
--------------------
G |- err(e) : Result[T, E]
```

<a id="match"></a>

### match

```
G |- e : T
G,p1 |- e1 : R  ...  G,pn |- en : R
exhaustive(p1...pn, T)
---------------------------------------
G |- match e { p1=>e1, ..., pn=>en } : R
```

<a id="pipe"></a>

### pipe

```
G |- x : A    G |- f : A -> B
-------------------------------
G |- x |> f : B
```

<a id="spread"></a>

### spread

```
G |- base : { f1:T1, ..., fn:Tn }
G |- ei : Ti  (for overridden fields)
---------------------------------------
G |- { ...base, fi: ei, ... } : { f1:T1, ..., fn:Tn }
```

<a id="fan"></a>

### fan

```
G |- e1 : Result[T1, E]    G |- e2 : Result[T2, E]    enclosing function is effect
------------------------------------------------------------------------------------
G |- fan { e1, e2 } : Result[(T1, T2), E]
```

<a id="guard"></a>

### guard

```
G |- cond : Bool    G |- else_ : R
return_type = R
---------------------------------------
G |- guard cond else else_
```

<a id="hole--todo"></a>

### hole / todo

```
expected_type = T               expected_type = T
-------------------             -------------------
G |- _ : T                      G |- todo(msg) : T
```

---

<a id="22-complete-example"></a>

## 22. 完全な例

```almide
import env
import fs
import json

type Config = {
  root: String,
  bare: Bool,
  description: String,
}

type ConfigError =
  | Io(String)
  | Parse(String)

fn default_config(root: String) -> Config =
  { root: root, bare: false, description: "" }

fn with_description(config: Config, desc: String) -> Config =
  { ...config, description: desc }

fn summary(config: Config) -> String =
  "root=${config.root}, bare=${config.bare}"

fn decode_config(text: String) -> Result[Config, ConfigError] = {
  let raw = json.parse(text) |> result.map_err((e) => Parse(e))!
  let root = value.get_string(raw, "root") |> option.to_result(Parse("missing root"))!
  ok(default_config(root))
}

effect fn load(path: String) -> Result[Config, ConfigError] = {
  let text = fs.read_text(path) |> result.map_err((e) => Io(e))!
  decode_config(text)
}

effect fn main() -> Result[Unit, ConfigError] = {
  let args = env.args()
  let path = list.get(args, 0) ?? "config.json"
  let config = load(path)!
  println(summary(config))
  ok(())
}

test "default config" {
  let cfg = default_config("/repo")
  assert_eq(cfg.bare, false)
  assert_eq(cfg.description, "")
}

test "with_description updates correctly" {
  let cfg = default_config("/repo")
  let updated = with_description(cfg, "my repo")
  assert_eq(updated.description, "my repo")
  assert_eq(updated.root, cfg.root)
}

test "decode config validates the boundary" {
  assert_eq(decode_config('{"root":"/repo"}')!, default_config("/repo"))
  assert_eq(decode_config('{}'), err(Parse("missing root")))
}

```

---

<a id="23-evaluation-metrics"></a>

## 23. 評価指標

| 指標 | 定義 |
|---|---|
| **Pass@1** | 最初の生成でコンパイルとテストに合格する割合 |
| **Repair Turns** | 最初の失敗から最終成功までの修正反復数 |
| **Token Cost** | 成功までの入出力トークン総数 |
| **API Hallucination Rate** | 存在しない API または誤った署名の割合 |
| **Edit Breakage Rate** | コード変更時に無関係な動作を壊す割合 |
| **Diagnostic Utilization Gain** | 構造化診断の有無による性能差 |
