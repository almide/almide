# Selmite Language Specification v0.6

**"自由に書くための言語ではなく、正しく収束するための言語"**

---

## 0. 設計哲学

### 核心命題

LLM向け言語設計の本質は、表現力の最大化ではなく、**各生成ステップにおける有効候補集合の最小化**にある。

### 4本柱

| 原則 | 定義 |
|---|---|
| **Predictable** | コードの続きを生成するとき「次に来る正しい構文・API・意味」が狭く絞れる |
| **Local** | ある箇所を理解・修正するために必要な情報が、できるだけ近くにある |
| **Repairable** | 誤りが起きても、コンパイラ・ランタイム・型系が少ない手数で一意に近い修正候補を返せる |
| **Compact** | 意味密度が高く、記法のノイズが少ない。厳格でも長くならない |

### 設計憲法 7条

1. **正準性** -- 同じ意味を表す主要な書き方は、原則1つにする
2. **表面意味** -- 副作用、失敗可能性、欠損可能性、可変性は、構文か型に現れなければならない
3. **局所推論** -- 関数や式の意味は、近くの構文だけで大半が分かるべきである
4. **段階的完成** -- 未完成コードは合法であり、型付きの穴を埋めながら前進できるべきである
5. **修復優先** -- コンパイラは拒絶器ではなく修復器であるべきで、診断は構造化される
6. **語彙節約** -- 標準ライブラリは小さく、一貫した語彙だけを持つ
7. **魔法の禁止** -- 実行時に意味が変わる機構、文脈依存DSL、暗黙型変換を原則禁止する

### トレードオフ（意図的に犠牲にするもの）

- 熟練者の記述自由度
- 文化的な"言語らしさ"
- DSLの気持ちよさ
- メタプログラミングの爆発力
- 極端な型表現力
- 短さのための省略美

**狙い: 高い簡潔さ + 低い自由度**

---

## 1. 字句仕様

### 1.1 識別子

```
Identifier ::= [a-z_][a-zA-Z0-9_]*
```

末尾に `?` を1つだけ許す:

```
Name ::= Identifier | Identifier "?"
```

意味規則（static rule で強制）:
- `name?` -- **Bool predicate 専用**（戻り値型は必ず Bool）

### 1.2 型名

```
TypeName ::= [A-Z][a-zA-Z0-9]*
TypeConstructor ::= TypeName
```

### 1.3 リテラル

```
IntLiteral       ::= [0-9]+
FloatLiteral     ::= [0-9]+ "." [0-9]+
StringLiteral    ::= '"' ... '"'
InterpolatedStr  ::= '"' ( char | "${" Expr "}" )* '"'
BoolLiteral      ::= "true" | "false"
```

欠損値リテラル `null` は**存在しない**。欠損は `none`（`Option[T]` の構築子）で表現する。

### 1.4 予約語

```
module import type trait impl for fn let var
if then else match
ok err some none
try do
todo unsafe effect deriving test
async await guard newtype
```

将来予約: `strict`, `where`

---

## 2. 文の区切り

**改行が文の区切り。** セミコロンは1行に複数文を書く場合のみ使用。

```
let x = 1
let y = 2
let z = x + y    // 改行で区切り

let a = 1; let b = 2   // 1行に複数文はセミコロン
```

### 2.1 行継続ルール

以下の場合、改行は無視され次行に継続する:

**行末が以下のトークンの場合:**
- 二項演算子: `+`, `-`, `*`, `/`, `%`, `++`, `==`, `!=`, `<=`, `>=`, `<`, `>`, `and`, `or`, `|>`
- 区切り: `,`, `.`, `:`
- 開き括弧: `(`, `{`, `[`
- 矢印: `->`, `=>`
- 代入: `=`
- キーワード: `if`, `then`, `else`, `match`, `try`, `do`, `not`, `|`

**次行が以下のトークンで始まる場合:**
- `.` (メソッドチェーン)
- `|>` (パイプ)

```
let result = items
  .filter(fn(x) => x > 0)
  .map(fn(x) => x * 2)
  .fold(0, fn(acc, x) => acc + x)

text
  |> string.trim
  |> string.split(",")
```

---

## 3. 構文カテゴリ

```
Program   ::= ModuleDecl ImportDecl* TopDecl*

TopDecl   ::= TypeDecl | TraitDecl | ImplDecl | FnDecl | TestDecl

Stmt      ::= LetStmt | VarStmt | AssignStmt | Expr

Expr      ::= Literal
            | Name
            | InterpolatedStr
            | RecordExpr
            | SpreadExpr
            | ListExpr
            | CallExpr
            | MemberExpr
            | PipeExpr
            | IfExpr
            | MatchExpr
            | BlockExpr
            | DoExpr
            | LambdaExpr
            | HoleExpr
            | TodoExpr
            | TryExpr
            | BinaryExpr
            | UnaryExpr
            | "(" Expr ")"
```

---

## 4. モジュールと import

### 4.1 モジュール宣言

```
ModuleDecl ::= "module" ModulePath
ModulePath ::= Identifier ( "." Identifier )*
```

### 4.2 import 宣言

```
ImportDecl ::= "import" ImportPath
             | "import" ImportPath "." "{" NameList "}"

NameList ::= Name ( "," Name )*
```

例:
```
import fs
import json
import collections.{List, Map}
```

**禁止: wildcard import。** `import fs.*` はコンパイルエラー。

選択的 import は許可する。「何を持ち込んだか」がコード上に見えるため、LLMの名前解決を助ける。

### 4.3 prelude は極小

暗黙 import は真に基本的な型だけ:
- `Int`, `Float`, `Bool`, `String`, `Unit`
- `Option`, `Result`, `List`
- `some`, `none`, `ok`, `err`
- `true`, `false`

`map`, `filter` などの関数はコレクション型のメソッドとしてのみ提供し、グローバル関数として浮遊させない。

---

## 5. 型宣言

### 5.1 ジェネリクス — `[]` 記法

**型引数には `[]` を使用する。** `<>` は比較演算子専用。

```
GenericParams ::= "[" TypeParam ( "," TypeParam )* "]"
TypeParam     ::= TypeName ( ":" TraitBound )?
TraitBound    ::= TypeName ( "+" TypeName )*
```

根拠: `<>` は比較演算子と構文的に衝突し、パーサーが文脈依存の曖昧性解決を必要とする。`[]` は常にジェネリクスを意味し、曖昧性がゼロ。`>>` の分割問題も発生しない。

```
// 曖昧性なし
Result[List[Map[String, Int]], Error]
fn map[U](self, f: fn(T) -> U) -> List[U]
```

### 5.2 レコード型

```
TypeDecl   ::= "type" TypeName GenericParams? "=" TypeExpr DerivingClause?

RecordType ::= "{" FieldTypeList? "}"
FieldTypeList ::= FieldType ( "," FieldType )*
FieldType  ::= Identifier ":" TypeExpr
```

例:
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

### 5.3 バリアント型

```
VariantType  ::= VariantCase ( "|" VariantCase )*
VariantCase  ::= TypeConstructor
               | TypeConstructor "(" TypeExprList ")"
               | TypeConstructor "{" FieldTypeList "}"

TypeExprList ::= TypeExpr ( "," TypeExpr )*
```

例:
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

バリアントは**0引数、タプル形式（位置引数）、レコード形式（名前付き）**の3形態を許す。

### 5.4 deriving

```
DerivingClause ::= "deriving" TypeName ( "," TypeName )*
```

バリアント型の `From` trait 実装を自動導出する。`Name(Type)` 形式のケースから `From[Type]` を機械的に生成。

```
type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From

// 上記は以下と等価:
// impl From[IoError] for ConfigError { fn from(e: IoError) -> ConfigError = Io(e) }
// impl From[ParseError] for ConfigError { fn from(e: ParseError) -> ConfigError = Parse(e) }
// impl From[DecodeError] for ConfigError { fn from(e: DecodeError) -> ConfigError = Decode(e) }
```

根拠: `impl From` の手書きはコピペエラーの温床。LLMが3つの微妙に異なるブロックを正確に生成するのは無駄なリスク。

### 5.5 newtype

```
TypeExpr ::= ... | "newtype" TypeExpr
```

同じ構造だが型的に区別される新しい型を作る:

```
type UserId = newtype Int
type Email = newtype String
```

- `UserId` と `Int` は暗黙に変換されない
- ラップ: `UserId(42)` / アンラップ: `id.value`
- ランタイムコストはゼロ（コンパイル時のみの区別）
- IDや単位の取り違えを型で防止する

### 5.6 型適用

```
SimpleType ::= TypeName
             | TypeName "[" TypeExprList "]"
```

例:
```
List[String]
Result[User, ParseError]
Map[String, List[Int]]
```

---

## 6. Trait（最小の抽象化機構）

```
TraitDecl ::= "trait" TypeName GenericParams? "{" TraitMethodList "}"
TraitMethodList ::= ( TraitMethod )*
TraitMethod ::= "effect"? "fn" Name GenericParams? "(" ParamList ")" "->" TypeExpr
```

例:
```
trait Iterable[T] {
  fn map[U](self, f: fn(T) -> U) -> Self[U]
  fn filter(self, f: fn(T) -> Bool) -> Self[T]
  fn fold[U](self, init: U, f: fn(U, T) -> U) -> U
  fn any(self, f: fn(T) -> Bool) -> Bool
  fn all(self, f: fn(T) -> Bool) -> Bool
  fn len(self) -> Int
}

trait Storage[T] {
  effect fn save(self, item: T) -> Result[Unit, IoError]
  effect fn load(self, id: String) -> Result[T, IoError]
}
```

### impl

```
ImplDecl ::= "impl" TypeName GenericParams? "for" TypeName "{" FnDecl* "}"
```

例:
```
impl Iterable[T] for List[T] {
  fn map[U](self, f: fn(T) -> U) -> List[U] = _  // builtin
  fn filter(self, f: fn(T) -> Bool) -> List[T] = _
}
```

### 制約

- trait にはメソッドシグネチャのみ（デフォルト実装なし、v0.1では）
- trait 継承なし（v0.1では）
- 孤児ルール: 自分のクレート内でしか impl を書けない

---

## 7. 基本型環境

### プリミティブ

```
Int, Float, Bool, String, Bytes, Path, Unit
```

### コレクション

```
List[T], Map[K, V], Set[T]
```

### エフェクト表現

```
Option[T], Result[T, E]
```

### 境界型

```
Json, Value
```

- 外部入力の受け口としては使用可
- domain logic に持ち込む前に `decode[T]` を要求
- core domain 型として `Json` を公開すると linter warning

### 構築子

```
some(x)  : Option[T]
none     : Option[T]    // 型は文脈から推論
ok(x)    : Result[T, E]
err(x)   : Result[T, E]
```

---

## 8. 関数宣言

```
FnDecl ::= "pub"? "async"? "effect"? "fn" Name GenericParams? "(" ParamList? ")" "->" TypeExpr "=" Expr

ParamList ::= Param ( "," Param )*
Param     ::= Identifier ":" TypeExpr
```

修飾子の順序: `pub? async? effect? fn`

原則:
- **引数型は必須**
- **戻り値型は必須**
- 本体は式
- **副作用を持つ関数は `effect fn` で宣言する**
- **非同期関数は `async fn` で宣言する（`effect` を暗黙に含む）**

### 8.1 `effect fn` — 副作用の明示

`effect` キーワードが関数宣言の前に付くと、その関数は副作用を持つことを示す。

```
fn tracked?(index: Index, path: Path) -> Bool =
  index.entries.any(fn(entry) => entry.path == path)

effect fn add(index: Index, file: Path) -> Result[Index, IoError] =
  if tracked?(index, file) then ok(index)
  else do {
    let bytes = try read(file)
    let id = hash(bytes)
    ok(index.insert(file, id))
  }
```

根拠: v0.2 の `!` サフィックス（`read_text!`）は関数名にメタ情報を埋め込むため、宣言側と呼び出し側の両方で管理が必要だった。`effect fn` にすることで:
- 関数名は純粋な識別子になる（lexer が単純化）
- 副作用は宣言のキーワードで表現（型情報に近い位置）
- 呼び出し側は普通に呼ぶだけ
- コンパイラが `effect fn` から非 `effect fn` の呼び出しを検出すればよい

---

## 9. 文

### 9.1 let / var

```
LetStmt ::= "let" Identifier TypeAnnotation? "=" Expr
           | "let" "{" Identifier ("," Identifier)* "}" "=" Expr
VarStmt ::= "var" Identifier TypeAnnotation? "=" Expr
TypeAnnotation ::= ":" TypeExpr
```

- `let` は**不変**
- `var` は**可変**
- ローカル変数は型注釈省略可。公開API・モジュール境界・フィールドは明示。

#### 分割束縛

レコードからフィールドを取り出す:

```
let { name, age } = user
```

等価コード:
```
let name = user.name
let age = user.age
```

- `var` 版は提供しない（不変束縛のみ）
- ネストした分割は不可（1レベルのみ）
- リネームは不可（フィールド名がそのまま変数名になる）

### 9.2 guard

```
GuardStmt ::= "guard" Expr "else" Expr
```

前提条件のチェックと早期脱出。条件が偽のとき、else節の式が返される。

```
fn f(x: Int) -> Result[Int, Error] = {
  guard x > 0 else err("must be positive")
  ok(x * 2)
}
```

- ブロック内でのみ使用可能
- else 節は通常 `err(...)` で早期リターン
- if-else のネストを平坦化し、前提条件を先に書ける

### 9.3 再代入

```
AssignStmt ::= Identifier "=" Expr
```

`var` で束縛された識別子にしか許されない（static rule）。

### 9.4 ブロック

```
BlockExpr ::= "{" StmtList "}"
StmtList  ::= ( Stmt NEWLINE )* Stmt?
```

最後の文が式なら、その値がブロック値になる。

```
{
  let x = 1
  let y = 2
  x + y
}
```

---

## 10. 式

### 10.1 if 式

```
IfExpr ::= "if" Expr "then" Expr "else" Expr
```

- **条件式は必ず `Bool`。truthiness は禁止。**

```
let msg = if x > 0 then "positive" else "non-positive"

// コンパイルエラー:
if x then ...         // Int は Bool でない
if list then ...      // List は Bool でない
```

### 10.2 match 式

```
MatchExpr    ::= "match" Expr "{" MatchArmList "}"
MatchArmList ::= MatchArm ( "," MatchArm )*
MatchArm     ::= Pattern Guard? "=>" Expr
Guard        ::= "if" Expr
```

**match は網羅的でなければならない**（exhaustive check は typechecker の責務）。

ガード付きの例:
```
match value {
  ok(n) if n > 100 => "big",
  ok(n) => "small",
  err(e) => e,
}
```

ガードは `if` の後に Bool 式を取る。ガード条件が false の場合、次のアームに進む。ネストした if/match を平坦化でき、LLM が生成するコードの構造を単純に保つ。

### 10.3 パターン

```
Pattern ::= "_"
          | Identifier
          | Literal
          | "some" "(" Pattern ")"
          | "none"
          | "ok" "(" Pattern ")"
          | "err" "(" Pattern ")"
          | TypeConstructor
          | TypeConstructor "(" PatternList ")"
          | TypeConstructor "{" FieldPatternList "}"

PatternList      ::= Pattern ( "," Pattern )*
FieldPatternList ::= FieldPattern ( "," FieldPattern )*
FieldPattern     ::= Identifier ":" Pattern | Identifier
```

例:
```
match shape {
  Circle(r) => 3.14 * r * r,
  Rect{ width, height } => width * height,
  Point => 0.0,
}

match result {
  ok(value) => value,
  err(e) => handle(e),
}
```

### 10.4 ラムダ

```
LambdaExpr ::= "fn" "(" LambdaParamList? ")" "=>" Expr
```

**1種類だけ。短縮記法は禁止。**

```
fn(x) => x + 1
fn(x: Int, y: Int) => x + y
items.map(fn(x) => x * 2)
```

### 10.5 名前付き引数

```
CallExpr ::= Expr "(" CallArgList ")"
CallArg  ::= Expr | Identifier ":" Expr
```

呼び出し側で引数に名前を付けられる。宣言側の変更は不要。

```
// 位置引数（従来通り）
create_user("alice", 30, true)

// 名前付き引数（順序自由、自己文書化）
create_user(name: "alice", age: 30, active: true)

// 混在OK（位置引数の後に名前付き）
create_user("alice", age: 30, active: true)
```

根拠: LLMが最も間違えるのは「同じ型の引数が3つ以上あるとき」。`f(true, false, true)` のような呼び出しは、名前なしでは意味が不明。名前付きなら位置を間違えても名前で正しく対応する。

### 10.6 文字列補間

```
let name = "world"
let msg = "hello ${name}, 1+1=${1 + 1}"
```

LLMが最も頻繁に書くのはメッセージの組み立て。正準形を1つ与えることで `+` 結合や `format` 関数のブレを排除する。

### 10.7 レコード式とスプレッド

```
RecordExpr ::= "{" FieldInitList "}"
             | "{" "..." Expr "," FieldInitList "}"

FieldInit ::= Identifier ":" Expr
            | Identifier                 // 短縮: { name } は { name: name } と同等
```

例:
```
let alice = { name: "alice", age: 30 }
let bob = { ...alice, name: "bob" }      // age は alice から引き継ぐ
```

### 10.8 リスト式

```
ListExpr ::= "[" ExprList? "]"
```

### 10.9 パイプ

```
PipeExpr ::= Expr "|>" Expr
```

例:
```
text
  |> string.trim
  |> string.split(",")
  |> list.map(fn(s) => string.trim(s))
```

メソッドチェーンと関数呼び出しの2つの書き方が混在する問題を、パイプで正準化する。`x |> f` は `f(x)` と同等。

#### プレースホルダー `_`

パイプの右辺で多引数関数を使う場合、`_` で左辺の値を挿入する位置を指定できる:

```
text |> split(_, ",")           // split(text, ",")
xs |> filter(_, fn(x) => x > 0)  // filter(xs, fn(x) => x > 0)
```

- `_` は呼び出し引数内でのみプレースホルダーとして機能
- 1つの呼び出しに複数の `_` は不可（コンパイルエラー）
- `_` がない場合は従来通り `x |> f` → `f(x)`

### 10.10 UFCS（Uniform Function Call Syntax）

**`f(x, y)` と `x.f(y)` は等価。** コンパイラが自動的に解決する。

```
// 以下は全て同じ意味
string.trim(text)
text.trim()

// 以下も同じ
string.split(text, ",")
text.split(",")
```

#### 解決ルール

`x.f(args...)` が呼ばれたとき:

1. `x` の型にメソッド `f` があればそれを呼ぶ
2. なければ、スコープ内の関数 `f(x, args...)` を探す
3. どちらも見つからなければコンパイルエラー

#### 根拠

LLMが最も頻繁に迷う判断の一つが「これはメソッド呼び出しか関数呼び出しか」。UFCSにより:
- `string.trim(text)` と `text.trim()` のどちらで書いても正しい
- パイプ `text |> string.trim` も引き続き有効
- 正準形の選択肢が増えるように見えるが、**どれを書いても同じ意味になる**ため、間違いが存在しなくなる
- trait メソッドと自由関数の境界が消え、「この関数はどこに定義されている？」を気にせず書ける

### 10.11 do ブロック（Result/Option の自動 try 伝播）

```
DoExpr ::= "do" BlockExpr
```

`do` ブロック内では、`Result[T, E]` や `Option[T]` を返す式に対して自動的に `try` が適用される。

```
effect fn load(path: Path) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)        // 自動 try: Result[String, IoError]
    let raw = json.parse(text)           // 自動 try: Result[Json, ParseError]
    decode[Config](raw)                  // 自動 try: Result[Config, DecodeError]
  }
```

`do` の型推論ルール:
- ブロックの戻り型が `Result[T, E]` のとき、ブロック内の式が `Result[U, E]` なら自動的に unwrap して `U` を束縛
- エラー型が異なる場合、`From` trait による変換を試み、変換できなければコンパイルエラー

これが **Result 冗長問題の解決策**。`try` を手書きするか `do` で自動化するかの二択を与え、正準形は2つだが意味は明確に分かれる:
- `try`: 1つの式だけ unwrap したい
- `do`: ブロック全体を Result 文脈で書きたい

### 10.12 hole / todo / try

```
HoleExpr ::= "_"
TodoExpr ::= "todo" "(" StringLiteral ")"
TryExpr  ::= "try" Expr
```

**この3つはこの言語の中核。**

---

## 11. 演算子

```
UnaryOp  ::= "-" | "not"
BinaryOp ::= "+" | "-" | "*" | "/" | "%" | "++"
            | "==" | "!=" | "<" | "<=" | ">" | ">="
            | "and" | "or"
            | "|>"
```

`++` は**リスト/文字列の結合専用**。`+` のオーバーロードで文字列結合をするとLLMが混乱するため分離。

### 優先順位

1. unary (`-`, `not`)
2. `*` `/` `%`
3. `+` `-` `++`
4. 比較 (`==`, `!=`, `<`, `<=`, `>`, `>=`)
5. `and`
6. `or`
7. `|>`

- 代入は演算子ではなく文
- operator overloading は原則禁止（組み込み型のみ）
- 迷ったら括弧を書く

---

## 12. エラーモデル

### 12.1 三層のエラー戦略

| 層 | 機構 | 用途 |
|---|---|---|
| **通常失敗** | `Result[T, E]` | parse, validate, I/O, lookup |
| **プログラマエラー** | `panic` | 到達不能、不変条件違反 |
| **テスト用** | `expect` | テスト内の簡易 unwrap |

例外は**存在しない**。`throw` / `catch` はない。

### 12.2 try の typing rule

#### Result に対する try

```
Γ ⊢ e : Result[T, E]
current_return_type = Result[R, E]
-----------------------------------
Γ ⊢ try e : T
```

#### Option に対する try

```
Γ ⊢ e : Option[T]
current_return_type = Option[R]
-----------------------------------
Γ ⊢ try e : T
```

#### 混用禁止

`Result` を返す関数内で `Option` に `try` を使う自動変換はしない。明示変換を書く:

```
let value = try opt.ok_or(MyError("missing"))
```

### 12.3 エラー変換

エラー型の変換は明示的に行う。ただし `do` ブロック + `From` trait + `deriving` で軽量化:

```
trait From[T] {
  fn from(value: T) -> Self
}

type AppError =
  | Io(IoError)
  | Parse(ParseError)
  deriving From

// do ブロック内でエラー型が異なる場合、From が実装されていれば自動変換
effect fn load(path: Path) -> Result[Config, AppError] =
  do {
    let text = fs.read_text(path)    // IoError -> AppError via From
    let raw = json.parse(text)       // ParseError -> AppError via From
    decode[Config](raw)
  }
```

---

## 13. Hole と未完成コード

**この言語の核心機能。**

### 13.1 Hole

```
fn parse(text: String) -> Ast = _
```

### 13.2 todo

```
fn optimize(ast: Ast) -> Ast = todo("implement constant folding")
```

### 13.3 typing rule

```
expected_type = T
-------------------
Γ ⊢ _ : T          // hole: 型検査は通すが最終成果物ではエラー

expected_type = T
-------------------
Γ ⊢ todo(msg) : T  // todo: 同上、ただしメッセージを保持
```

### 13.4 コンパイラの義務

hole を見つけたら:
- 期待型 T
- スコープ内の利用可能な変数とその型
- 期待型を返せる関数候補
- 候補式のテンプレート

を構造化して返す。

```json
{
  "error": "hole",
  "location": { "file": "main.lang", "line": 12, "col": 5 },
  "expected_type": "Result[Commit, ParseError]",
  "available_names": [
    { "name": "text", "type": "String" },
    { "name": "parse_header", "type": "(String) -> Result[Header, ParseError]" }
  ],
  "suggestions": [
    "parse_header(text)",
    "todo(\"return commit\")"
  ]
}
```

---

## 14. エフェクト設計

### 14.1 `effect fn` — コンパイルエラーで強制

非 `effect` 関数から `effect fn` を呼ぶと **warning ではなくエラー**。

```
fn pure_fn(x: Int) -> Int =
  read(some_path)    // コンパイルエラー: effect fn を non-effect fn から呼び出せない
```

warning だと無視されてエフェクト境界が形骸化する。エラーにすることで、副作用の境界が言語レベルで保証される。

### 14.2 unsafe ブロック

本当にエフェクト境界を無視したいときは明示的に:

```
fn technically_pure(x: Int) -> Int =
  unsafe { read(cache_path) }    // 明示的に安全性を破る
```

`unsafe` の存在が「ここは危険」を表面化する。

### 14.3 標準ライブラリの規約

```
effect fn now() -> Timestamp
effect fn getenv(key: String) -> Option[String]
effect fn read_text(path: Path) -> Result[String, IoError]
effect fn write(path: Path, data: String) -> Result[Unit, IoError]
effect fn random_int(min: Int, max: Int) -> Int
```

I/O, clock, env, net, randomness は全部 `effect fn`。

---

## 15. Async/Await

### 15.1 async fn

`async fn` は非同期関数を宣言する。**`async` は `effect` を暗黙に含む**（全ての非同期操作はI/Oを伴うため）。

```
async fn fetch(url: String) -> Result[String, HttpError] = _
async fn fetch_json[T](url: String) -> Result[T, HttpError] = _
```

`async fn` の戻り値型は内側の型を書く。実際のランタイム戻り値は `Async[Result[String, HttpError]]` だが、型注釈では `Result[String, HttpError]` と書く。

### 15.2 await

```
AwaitExpr ::= "await" Expr
```

`await` は `Async[T]` を解除して `T` を取り出す。`try` と同様のプレフィクス演算子で、`async fn` 内でのみ使用可能。

```
async fn load(url: String) -> Result[Config, AppError] = {
  let text = await fetch(url)      // fetch: Async[Result[String, HttpError]]
                                    // await: Result[String, HttpError]
  let config = try parse(text)     // try: Config
  ok(config)
}
```

### 15.3 do ブロックとの組み合わせ

`do` ブロック内で `await` と暗黙 `try` を組み合わせる:

```
async fn load(url: String) -> Result[Config, AppError] =
  do {
    let text = await fetch(url)     // await で Async 解除, do で Result 自動 try
    let config = parse(text)        // do で Result 自動 try
    config
  }
```

**`await` は明示、`try` は `do` が暗黙化。** この分離が重要:
- どの行が非同期かは `await` で見える（局所推論）
- エラー処理は `do` が一括で担う（ノイズ削減）

### 15.4 構造化並行性

非構造化な `spawn` / `join` は**禁止**。並行実行は組み込みのコンビネータのみ:

```
// 全タスクを並列実行して全結果を待つ
async fn parallel[T](tasks: List[Async[T]]) -> List[T]

// 最初に完了したタスクの結果を返す
async fn race[T](tasks: List[Async[T]]) -> T

// タイムアウト付き実行
async fn timeout[T](ms: Int, task: Async[T]) -> Result[T, TimeoutError]

// スリープ
async fn sleep(ms: Int) -> Unit
```

例:
```
async fn load_all(urls: List[String]) -> Result[List[String], HttpError] =
  do {
    await parallel(urls.map(fn(url) => fetch(url)))
  }

async fn fetch_fastest(urls: List[String]) -> Result[String, HttpError] =
  do {
    await race(urls.map(fn(url) => fetch(url)))
  }

async fn fetch_with_timeout(url: String) -> Result[String, AppError] =
  do {
    await timeout(5000, fetch(url))
  }
```

### 15.5 Typing Rules

```
Γ ⊢ e : Async[T]
current_fn is async
----------------------------
Γ ⊢ await e : T
```

`await` を非 `async fn` 内で使用するとコンパイルエラー。`async fn` を非 `async fn` / 非 `effect fn` から呼ぶとコンパイルエラー。

### 15.6 根拠

- `async` が `effect` を含むため、修飾子は最大2種類（`async fn` or `effect fn`）。「`async effect fn` と書くべきか？」の迷いは `async` が `effect` を含むことで解消
- 構造化並行性により、リソースリークやデッドロックのリスクを言語レベルで排除
- `do` + `await` の組み合わせにより、非同期エラー処理コードが同期コードとほぼ同形になる

---

## 16. テスト

### 16.1 test 宣言

```
TestDecl ::= "test" StringLiteral BlockExpr
```

テストは**トップレベル宣言**として関数と同じファイルに書く。テスト専用ファイルに分離する必要はない。

```
fn add(x: Int, y: Int) -> Int = x + y

test "addition" {
  assert_eq(add(1, 2), 3)
  assert_eq(add(0, 0), 0)
}

test "negative addition" {
  assert_eq(add(-1, 1), 0)
}
```

### 16.2 アサーション関数

テストブロック内で使用可能な組み込み関数:

```
assert(cond: Bool)                    // cond が false なら失敗
assert_eq(actual: T, expected: T)     // actual != expected なら失敗
assert_ne(actual: T, expected: T)     // actual == expected なら失敗
```

### 16.3 根拠

- LLMが最も頻繁に生成するのはテストコード。書き方が1つに定まることで、生成分布が収束する
- テストが関数の隣にあることで、LLMが関数の意図を理解しやすい（局所推論）
- `test "name" { ... }` は構造が単純で、LLMがテンプレートとして学習しやすい

---

## 17. 命名規則

### ? は Bool predicate 専用

```
fn empty?(xs: List[Int]) -> Bool = xs.len == 0
fn tracked?(index: Index, path: Path) -> Bool = ...
fn exists?(path: Path) -> Bool = ...
```

`?` が付いた関数の戻り値が `Bool` でなければコンパイルエラー。

### 破壊更新

| 非破壊（新しい値を返す） | 破壊（in-place、`effect fn`） |
|---|---|
| `fn push(list, item) -> List[T]` | `effect fn push(list, item) -> Unit` |
| `fn sort(list) -> List[T]` | `effect fn sort(list) -> Unit` |

---

## 18. 標準ライブラリ

### 18.1 コレクション API（trait ベース・命名固定・別名禁止）

全コレクション型で統一:

| 操作 | シグネチャ | 備考 |
|---|---|---|
| `map` | `fn[U](self, fn(T) -> U) -> Self[U]` | 変換 |
| `filter` | `fn(self, fn(T) -> Bool) -> Self[T]` | 絞り込み |
| `fold` | `fn[U](self, U, fn(U, T) -> U) -> U` | 累積 |
| `any` | `fn(self, fn(T) -> Bool) -> Bool` | 任意要素条件 |
| `all` | `fn(self, fn(T) -> Bool) -> Bool` | 全要素条件 |
| `len` | `fn(self) -> Int` | 長さ |
| `contains` | `fn(self, T) -> Bool` | 存在判定 |
| `find` | `fn(self, fn(T) -> Bool) -> Option[T]` | 検索 |
| `get` | `fn(self, key) -> Option[T]` | キー取得 |
| `first` | `fn(self) -> Option[T]` | 先頭 |
| `last` | `fn(self) -> Option[T]` | 末尾 |

**`collect`, `select`, `inject`, `pluck` 等は存在しない。**

### 18.2 Result / Option のメソッド

```
// Result[T, E]
fn map[U](self, fn(T) -> U) -> Result[U, E]
fn map_err[F](self, fn(E) -> F) -> Result[T, F]
fn and_then[U](self, fn(T) -> Result[U, E]) -> Result[U, E]
fn unwrap_or(self, default: T) -> T
fn is_ok?(self) -> Bool
fn is_err?(self) -> Bool

// Option[T]
fn map[U](self, fn(T) -> U) -> Option[U]
fn and_then[U](self, fn(T) -> Option[U]) -> Option[U]
fn unwrap_or(self, default: T) -> T
fn ok_or[E](self, err: E) -> Result[T, E]
fn is_some?(self) -> Bool
fn is_none?(self) -> Bool
```

### 18.3 文字列操作

```
// string モジュール
fn trim(s: String) -> String
fn split(s: String, sep: String) -> List[String]
fn join(parts: List[String], sep: String) -> String
fn starts_with?(s: String, prefix: String) -> Bool
fn ends_with?(s: String, suffix: String) -> Bool
fn contains?(s: String, sub: String) -> Bool
fn replace(s: String, from: String, to: String) -> String
fn len(s: String) -> Int
fn to_int(s: String) -> Option[Int]
fn to_float(s: String) -> Option[Float]
```

### 18.4 中核モジュール

| モジュール | 対象 |
|---|---|
| `string` | 文字列操作 |
| `path` | パス操作 |
| `fs` | ファイル I/O（全て `effect fn`） |
| `json` | JSON パース・生成 |
| `http` | HTTP 通信（全て `effect fn`） |
| `time` | 時刻（`now` 等は `effect fn`） |
| `env` | 環境変数（全て `effect fn`） |

---

## 19. 禁止事項

| # | 禁止 | 理由 |
|---|---|---|
| 1 | 暗黙型変換 | LLMが型を混ぜる |
| 2 | truthiness | 条件式は Bool のみ |
| 3 | monkey patch / open class | 実行時の意味変更はLLMに不可視 |
| 4 | operator overloading | 演算子の意味が型で変わると読めない |
| 5 | 例外 (throw/catch) | フローが不可視 |
| 6 | 複数ラムダ記法 | 生成分布が散る |
| 7 | 内部DSL | 文脈依存が強い |
| 8 | wildcard import | 名前の出所が不明 |
| 9 | null | `Option[T]` に統一 |
| 10 | API 別名 | 語彙増加 = 幻覚増加 |
| 11 | `<>` ジェネリクス | 比較演算子と曖昧。`[]` を使う |

---

## 20. コンパイラの責務

### 20.1 1エラー1本質

派生エラーの連鎖を抑制。根本原因を1つ示す。

### 20.2 構造化エラー出力

```json
{
  "kind": "type_mismatch",
  "location": { "file": "main.lang", "line": 12, "col": 5 },
  "expected": "Result[Config, IoError]",
  "actual": "String",
  "suggestions": ["ok(text)", "try parse_config(text)"]
}
```

### 20.3 自動修正候補

- 不足 import の候補提示
- 型変換候補
- match 網羅漏れの自動生成
- `effect` 漏れの指摘
- `do` ブロック提案（`try` が3つ以上連続する場合）

### 20.4 公式 formatter（言語に組み込み）

- 1 AST に対して整形結果は1つ
- import 順序はアルファベット固定
- trailing comma あり
- 長い call chain / pipe chain の改行規則は固定

これは LLM の diff stability に直結する。

---

## 21. Linter

style 警察ではなく**生成安定化装置**。

| ルール | 内容 |
|---|---|
| effect-leak | 非 `effect fn` から `effect fn` 呼び出し（エラー） |
| unused-result | `Result` を無視 |
| unsafe-unwrap | `Option` を unsafe に潰す |
| long-chain | chain が5段以上 |
| ambiguous-name | 1文字変数（ラムダ引数以外） |
| missing-annotation | 公開関数の型省略 |
| json-in-core | core domain で `Json` 型を使用 |

---

## 22. 段階的厳格化

```
module repo.index
strict types      // 全ての型注釈を必須にする
strict effects    // effect の伝播を完全にチェック
```

プロジェクト設定で:
```
[strictness]
core = "all"      // 全 strict
app = "medium"    // types のみ strict
script = "light"  // strict なし
```

---

## 23. escape hatch

危険な機能は `unsafe` ブロックに隔離:

```
unsafe {
  // ここでは effect ルールを無視できる
  // ここでは型検査を一部スキップできる
}
```

`unsafe` の存在が「ここは通常のルールを破っている」を表面化する。

---

## 24. Typing Rules

### 変数

```
Γ(x) = T
-----------
Γ ⊢ x : T
```

### let

```
Γ ⊢ e : T
--------------------
Γ, x:T ⊢ let x = e
```

### if

```
Γ ⊢ c : Bool    Γ ⊢ t : T    Γ ⊢ e : T
-----------------------------------------
Γ ⊢ if c then t else e : T
```

### 関数

```
Γ, x1:T1, ..., xn:Tn ⊢ body : R
-----------------------------------------
Γ ⊢ fn f(x1:T1,...,xn:Tn) -> R = body
```

### Option / Result 構築子

```
Γ ⊢ e : T                         Γ ⊢ e : T
-------------------                -------------------
Γ ⊢ some(e) : Option[T]           Γ ⊢ ok(e) : Result[T, E]

Γ ⊢ e : E
-------------------
Γ ⊢ err(e) : Result[T, E]
```

### match

```
Γ ⊢ e : T
Γ,p1 ⊢ e1 : R  ...  Γ,pn ⊢ en : R
exhaustive(p1...pn, T)
--------------------------------------
Γ ⊢ match e { p1=>e1, ..., pn=>en } : R
```

### try (Result)

```
Γ ⊢ e : Result[T, E]
return_type = Result[R, E]
----------------------------
Γ ⊢ try e : T
```

### try (Option)

```
Γ ⊢ e : Option[T]
return_type = Option[R]
----------------------------
Γ ⊢ try e : T
```

### do ブロック

```
return_type = Result[R, E]
Γ ⊢ block : R   (with implicit try on Result[_, E] expressions)
----------------------------
Γ ⊢ do { block } : Result[R, E]
```

`do` ブロック内で `Result[T, E]` 型の式は暗黙に unwrap され `T` として束縛される。エラー型 `E` が異なる場合、`From` trait による変換を試み、変換できなければコンパイルエラー。

### hole / todo

```
expected_type = T               expected_type = T
-------------------             -------------------
Γ ⊢ _ : T                      Γ ⊢ todo(msg) : T
```

### pipe

```
Γ ⊢ x : A    Γ ⊢ f : A -> B
------------------------------
Γ ⊢ x |> f : B
```

### spread

```
Γ ⊢ base : { f1:T1, ..., fn:Tn }
Γ ⊢ ei : Ti  (for overridden fields)
--------------------------------------
Γ ⊢ { ...base, fi: ei, ... } : { f1:T1, ..., fn:Tn }
```

### await

```
Γ ⊢ e : Async[T]    enclosing function is async
-------------------------------------------------
Γ ⊢ await e : T
```

### destructure

```
Γ ⊢ e : { f1:T1, ..., fn:Tn }
--------------------------------------
Γ, f1:T1, ..., fn:Tn ⊢ let { f1, ..., fn } = e
```

### guard

```
Γ ⊢ cond : Bool    Γ ⊢ else_ : R
return_type = R
--------------------------------------
Γ ⊢ guard cond else else_
```

---

## 25. 完全サンプル

```
module repo.config

import fs
import json

type Config = {
  root: Path,
  bare: Bool,
  description: String,
}

type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From

fn exists?(path: Path) -> Bool =
  fs.exists?(path)

effect fn load(path: Path) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)
    let raw = json.parse(text)
    decode[Config](raw)
  }

fn with_description(config: Config, desc: String) -> Config =
  { ...config, description: desc }

fn default_config(root: Path) -> Config =
  { root: root, bare: false, description: "" }

fn summary(config: Config) -> String =
  "root=${config.root}, bare=${config.bare}"

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
```

ここに現れる性質:
- 失敗は `Result`、例外なし
- 副作用は `effect fn` で可視化、コンパイラが強制
- `do` ブロックで Result のノイズを最小化
- `deriving From` で型安全なエラー変換をボイラープレートなしで実現
- `...` スプレッドで immutable レコード更新
- 文字列補間で正準的なメッセージ組み立て
- `?` は Bool predicate のみ、意味が一意
- `[]` ジェネリクスで構文的曖昧性ゼロ
- 改行区切りで自然な見た目
- 型境界が全て見える
- テストが関数のすぐ隣にある（局所推論）
- match guard でパターンマッチが平坦に書ける
- UFCS でメソッド/関数の区別が不要

---

## 26. 変更履歴

### v0.5 → v0.6

| 変更 | 理由 |
|---|---|
| 分割束縛 (`let { name, age } = user`) | レコードフィールドの取り出しを簡潔に。`user.name` の繰り返しを排除 |
| `newtype` (`type UserId = newtype Int`) | 同構造だが型的に区別。IDや単位の取り違えを防止 |
| パイプのプレースホルダー (`x \|> f(_, y)`) | 多引数関数をパイプで使用可能に。ラムダ不要 |
| `guard` 文 (`guard cond else expr`) | 前提条件チェックの平坦化。if-else のネスト削減 |

### v0.4 → v0.5

| 変更 | 理由 |
|---|---|
| 名前付き引数 (`f(name: "alice")`) | 位置引数の入れ違いを排除。自己文書化 |
| `async fn` / `await` | 非同期処理を `effect fn` と一貫した形で導入 |
| 構造化並行性 (`parallel`, `race`, `timeout`) | `spawn`/`join` を禁止し、安全な並行パターンのみ提供 |

### v0.3 → v0.4

| 変更 | 理由 |
|---|---|
| Match guard (`pattern if cond => expr`) | ネストした if/match を平坦化。パターンマッチの表現力向上 |
| UFCS (`f(x, y)` ≡ `x.f(y)`) | メソッド vs 関数の判断を排除。どちらで書いても正しい |
| `test "name" { ... }` 構文 | テストの書き方を一意に。関数の隣に書ける局所性 |

### v0.2 → v0.3

| 変更 | 理由 |
|---|---|
| `<>` → `[]` ジェネリクス | 比較演算子との曖昧性排除。`>>` 分割問題の消滅 |
| `fn name!()` → `effect fn name()` | 関数名からメタ情報を分離。lexer/parser の単純化 |
| `deriving` 追加 | `impl From` ボイラープレートの排除。コピペエラー防止 |
| 行継続ルール明文化 | `.` `\|>` 開始行の暗黙ルールを仕様化 |

---

## 27. v0.7 への検討事項

- ジェネリクスの variance 規則
- trait のデフォルト実装
- stream の基本型
- モジュールの可視性制御（`pub fn` / `fn`）

---

## 28. 評価指標

| 指標 | 定義 |
|---|---|
| **Pass@1** | 1回の生成でコンパイル + テストを通る率 |
| **Repair Turns** | 最初の失敗から最終成功までの修正回数 |
| **Token Cost** | 成功までの総入出力トークン数 |
| **API Hallucination Rate** | 存在しないAPI・誤ったシグネチャの出現率 |
| **Edit Breakage Rate** | 既存コード修正で要求外の振る舞いを壊した率 |
| **Diagnostic Utilization Gain** | 構造化診断あり/なしでの修復性能差 |

比較対象:
- Python / Ruby / TypeScript / Go baseline
- Python strict profile / Ruby canonical profile / TypeScript reduced profile
- この言語
