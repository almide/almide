# 型システム仕様

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](type-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](type-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](type-system.zh-CN.md)



> 正本：[docs/specs/type-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`。非規範的な読者向けスナップショット訳で、自動同期しない。原仕様とその明示する規範上の位置付けは変更しない。モデル証明・実装の主張・歴史的状態・証拠の範囲は原文の限定を保ち、この翻訳による独立の実行・測定・適合性／安全性証明はない。コードブロック内のコードとコメントは原文のまま。

> 最終更新：2026-10-31

Almide は、双方向推論、レコードの構造的型付け、アドホック多相のための protocol システムを備えた制約ベースの型システムを使う。すべての型はコンパイル時に解決され、実行時の型検査はない。

> 原文の範囲に関する注記：「実行時の型検査はない」を、実行時検査が一切ない、または安全性全般が証明済みという主張に広げない。§3 の「自動アンラップ」は原文の表現を保ったもので、明示的な `!` を伴う記述であり、暗黙の伝播を追加する意味ではない。

---

## 1. プリミティブ型

以下の中核組み込み型の kind は `*`（具体型、型引数ゼロ）である。サイズ付き数値型（`Int8` / `Int16` / `Int32` / `Int64`、`UInt8` / `UInt16` / `UInt32` / `UInt64`、`Float32` / `Float64`）も組み込みである。`spec/lang/sized_numeric_types_test.almd` と対応する[標準ライブラリリファレンス](../../../../stdlib/README.md)を参照。

| 型 | 説明 | Eq | Hash | Ord |
|---|---|---|---|---|
| `Int` | 64 ビット符号付き整数 | はい | はい | はい |
| `Float` | 64 ビット IEEE 754 | はい | いいえ | はい |
| `String` | UTF-8 文字列 | はい | はい | はい |
| `Bool` | `true` / `false` | はい | はい | はい |
| `Unit` | ゼロ値型 `()` | はい | はい | はい |
| `Bytes` | バイトバッファー | はい | はい | いいえ |
| `Matrix` | 数値行列 | はい | いいえ | いいえ |

`Float` はハッシュ化できない（`Map` のキーにも `Set` の要素にもできない）。関数型（`Fn`）は Eq でもハッシュ化可能でもない。

```almide
let x = 42          // Int
let y = 3.14        // Float
let s = "hello"     // String
let b = true        // Bool
let u = ()          // Unit
```

テスト：`spec/lang/expr_test.almd`、`spec/lang/bytes_test.almd`、`spec/lang/matrix_test.almd`

---

## 2. コレクション型

### List[T]

同種の要素からなる順序付き列。kind：`* -> *`。

```almide
let xs = [1, 2, 3]              // List[Int]
let ys: List[String] = []       // empty list requires annotation
let zs = xs + [4, 5]            // + concatenates lists
let first = xs[0]               // Int (index access)
```

### Map[K, V]

キーと値の辞書。kind：`* -> * -> *`。キーはハッシュ化可能でなければならない（`Float`、`Fn`、`Map` はキーにできない）。

```almide
let m = ["a": 1, "b": 2]       // Map[String, Int]
let empty: Map[String, Int] = [:]
let v = m["a"]                  // Option[Int]
```

### Set[T]

要素が重複しないコレクション。kind：`* -> *`。要素はハッシュ化可能でなければならない。

```almide
let s = set.from_list([1, 2, 3])  // Set[Int]
```

### Tuple

固定長で異種の要素を持つ直積型。要素数は可変。

```almide
test "tuples" {
  let t = (1, "hello", true)     // (Int, String, Bool)
  let x = t.0                    // Int — positional access
  let (a, b) = (1, 2)            // destructuring
  assert_eq(x + a + b, 4)
}
```

テスト：`spec/lang/data_types_test.almd`、`spec/lang/tuple_test.almd`、`spec/lang/map_literal_test.almd`

---

## 3. Option[T] と Result[T, E]

値の欠如とエラー処理のための組み込みパラメーター付き型。どちらも専用コンストラクターを持つ `Applied` 型である。

### Option[T]

kind：`* -> *`。コンストラクター：`some(v)`、`none`。

```almide
let x: Option[Int] = some(42)
let y: Option[Int] = none

fn or_zero(o: Option[Int]) -> Int = match o {
  some(v) => v,
  none => 0,
}

test "both constructors" {
  assert_eq(or_zero(x), 42)
  assert_eq(or_zero(y), 0)
}
```

### Result[T, E]

kind：`* -> * -> *`。コンストラクター：`ok(v)`、`err(e)`。

```almide
let x: Result[Int, String] = ok(42)
let y: Result[Int, String] = err("fail")
```

`effect fn` 本体では、`Result` は `!` で自動アンラップされる：

```almide
import fs

effect fn read(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!   // propagates err
  ok(content)
}
```

テスト：`spec/lang/data_types_test.almd`、`spec/lang/error_test.almd`、`spec/lang/unwrap_operators_test.almd`

---

## 4. レコード型

### 名前付きレコード

`type` で宣言する。フィールドは名前でアクセスする。

```almide
type Point = { x: Float, y: Float }

let p: Point = { x: 1.0, y: 2.0 }
let px = p.x                          // Float
let p2 = { ...p, y: 5.0 }            // spread update
```

### 無名レコード

型名のないレコードリテラルは構造的に型付けされる。

```almide
let user = { name: "alice", age: 30 }   // { name: String, age: Int }
let n = user.name                        // String
```

### 開いたレコード（行多相）

`{ field: Type, .. }` 型の引数は、少なくとも必要なフィールドを持つ任意のレコードを受け入れる。追加フィールドは許可され、保持される。

```almide
fn greet(who: { name: String, .. }) -> String = "Hello, ${who.name}!"

type Dog = { name: String, breed: String }
type Person = { name: String, age: Int, email: String }

test "any record carrying the field is accepted" {
  assert_eq(greet(Dog { name: "Rex", breed: "Lab" }), "Hello, Rex!")                    // ok
  assert_eq(greet(Person { name: "Alice", age: 30, email: "a@b" }), "Hello, Alice!")    // ok
  assert_eq(greet({ name: "Bob" }), "Hello, Bob!")                                      // ok — exact match
}
```

開いたレコードは型エイリアス（形状エイリアス）にも使える：

```almide
type Named = { name: String, .. }
fn greet_named(who: Named) -> String = "Hi, ${who.name}!"
```

入れ子の開いたレコードもサポートする：

```almide
fn get_port(app: { config: { port: Int, .. }, .. }) -> Int = app.config.port
```

閉じたレコード（`..` のない `{ name: String }`）は、フィールドの完全一致を要求する。

テスト：`spec/lang/open_record_test.almd`、`spec/lang/record_spread_test.almd`

---

## 5. Variant 型

代数的データ型（タグ付き共用体）。`|` で区切ったケースとして宣言する。

### Unit ペイロード（列挙型風）

```almide
type Direction = | North | South | East | West

fn to_str(d: Direction) -> String = match d {
  North => "N",
  South => "S",
  East => "E",
  West => "W",
}
```

### タプルペイロード

```almide
type Shape = | Circle(Float) | Rect(Float, Float)

fn area(s: Shape) -> Float = match s {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
}
```

### レコードペイロード

variant のケースは名前付きフィールドを持てる：

```almide
type Pat =
  | Match { scope: String, regex: String }
  | BeginEnd { scope: String, begin: String, end_pat: String, patterns: List[Pat] }
  | Include(String)
  | Empty
```

レコード variant の構築とパターンマッチ：

```almide
type Pat =
  | Match { scope: String, regex: String }
  | BeginEnd { scope: String, begin: String, end_pat: String, patterns: List[Pat] }
  | Include(String)
  | Empty

fn describe(p: Pat) -> String = match p {
  Match { scope, regex } => scope + " " + regex,
  BeginEnd { scope, .. } => scope,
  _ => "other",
}

test "record-payload construction and matching" {
  let p = Match { scope: "keyword", regex: "\\bfn\\b" }
  assert_eq(describe(p), "keyword \\bfn\\b")
  assert_eq(describe(Empty), "other")
}
```

### 再帰的 Variant

variant 型は自分自身を参照できる。型検査器は Eq／Hash 検査の無限ループを防ぐため、循環を検出する。

```almide
type Tree[T] = | Leaf(T) | Node(T, List[Tree[T]])
```

### インライン Variant（先頭のパイプなし）

先頭の `|` を省略すると、最初のケースから直接始める：

```almide
type AppError = NotFound(String) | Io(String)
```

テスト：`spec/lang/data_types_test.almd`、`spec/lang/type_system_test.almd`、`spec/lang/variant_record_test.almd`

---

## 6. 関数型

関数は第一級の値である。型構文は `fn(Params) -> Ret` を使う。

```almide
fn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)

fn make_adder(n: Int) -> fn(Int) -> Int = (x) => x + n

test "function values" {
  let add5 = make_adder(5)
  assert_eq(apply(add5, 10), 15)
}
```

内部表現は `Ty::Fn { params: Vec<Ty>, ret: Box<Ty>, is_effect: bool }`。

関数型は決して Eq ではなく、ハッシュ化もできない。

### Effect 関数

`effect fn` は副作用を行う関数を示す。型検査器は、純粋関数が effect 関数を呼べないことを強制する（エラー E006）。`FnSig` の `is_effect` フラグがこれを追跡する。

```almide
import fs

effect fn read_file(path: String) -> Result[String, String] = fs.read_text(path)
```

### 関数型の型エイリアス

```almide
type Handler = (String) -> String
```

テスト：`spec/lang/type_system_test.almd`、`spec/lang/function_test.almd`、`spec/lang/lambda_test.almd`、`spec/lang/effect_fn_test.almd`

---

## 7. ユーザー定義ジェネリクス

ジェネリック型パラメーターは `[]` 構文を使う（`<>` ではない）。

### ジェネリック関数

```almide
fn id[T](x: T) -> T = x
fn pair[A, B](a: A, b: B) -> (A, B) = (a, b)
```

型引数は推論させても明示してもよい：

```almide
fn id[T](x: T) -> T = x

test "type arguments" {
  assert_eq(id(42), 42)              // T inferred as Int
  assert_eq(id[String]("hi"), "hi")  // T explicitly String
}
```

### ジェネリックレコード型

```almide
type Box[T] = { value: T, label: String }

fn unbox[T](b: Box[T]) -> T = b.value
```

### ジェネリック Variant 型

```almide
type Either[A, B] = | Left(A) | Right(B)

fn map_right[A, B, C](e: Either[A, B], f: fn(B) -> C) -> Either[A, C] = match e {
  Left(a) => Left(a),
  Right(b) => Right(f(b)),
}
```

### ジェネリクスの構造的境界

ジェネリックパラメーターに、特定のレコードフィールドを要求できる：

```almide
fn describe[T: { name: String, .. }](x: T) -> String = "name: ${x.name}"

fn set_name[T: { name: String, .. }](x: T, n: String) -> T = { ...x, name: n }
```

境界 `T: { name: String, .. }` は `OpenRecord` 制約である。検査器はこれを `FnSig.structural_bounds` に保存し、各呼び出し位置で検証する。

### ジェネリクスの Protocol 境界

```almide
protocol Showable {
  fn show(a: Self) -> String
}
protocol Nameable {
  fn get_name(a: Self) -> String
}

fn display[T: Showable](item: T) -> String = item.show()
fn show_named[T: Showable + Nameable](item: T) -> String = item.get_name() + ": " + item.show()
```

複数の境界は `+` で結ぶ。検査器は、各呼び出し位置の具体型が、必要なすべての protocol への適合を宣言していることを検証する。

テスト：`spec/lang/generics_test.almd`、`spec/lang/type_system_test.almd`、`spec/lang/open_record_test.almd`、`spec/lang/protocol_generics_test.almd`

---

## 8. 型推論

Almide は Union-Find データ構造を用いた制約ベースの型推論を行う。

### 3 パス構成

1. **推論**（`infer.rs`）：AST を走査し、不明な型に新しい型変数（`?0`、`?1`、…）を割り当て、型間の等値制約を集める。
2. **解決**（`solving.rs`）：単一化によって制約を処理する。Union-Find が同値な型変数を統合し、具体型に束縛する。
3. **置換**（`mod.rs`）：`expr_types` 内のすべての推論変数を、解決済みの具体型へ置き換える。

### 推論変数

新しい型変数は `?N`（例：`?0`、`?1`）と命名する。これらはユーザーが宣言した `TypeVar`（`T`、`U`）と別物である。`UnionFind` 構造は、ランクによる併合と経路圧縮で同値類を管理する。

### 双方向推論

型は前向き（引数から戻り値へ）にも後ろ向き（期待される型から式へ）にも流れる：

```almide
test "inference from annotation and from use" {
  let xs: List[Int] = []         // [] gets type List[Int] from annotation
  let f = (x) => x + 1           // x inferred as Int from + operator
  assert_eq(list.len(xs), 0)
  assert_eq(f(1), 2)
}
```

### Let 多相

ジェネリック関数は、各呼び出し位置で新しい推論変数を使ってインスタンス化される：

```almide
fn id[T](x: T) -> T = x

test "instantiated per call" {
  assert_eq(id(42), 42)             // T = Int at this call
  assert_eq(id("hello"), "hello")   // T = String at this call
}
```

### ラムダ引数の推論

ラムダ引数はその使われ方から推論される：

```almide
test "lambda parameter inferred from the list" {
  assert_eq(list.map([1, 2, 3], (x) => x * 2), [2, 4, 6])   // x: Int inferred from List[Int]
}
```

### 制約解決

制約は `(expected, actual, context)` の三つ組である。ソルバーは各対を単一化する：

- **推論変数 + 具体型**：変数を具体型へ束縛する。
- **推論変数 + 推論変数**：二つの変数を併合する。
- **具体型 + 具体型**：構造的に再帰する（例：`List[?0]` と `List[Int]` は `?0 = Int` を単一化する）。
- **Unknown**：すべてと単一化する（エラー回復用の番兵）。

**出現検査（occurs check）**が無限型を防ぐ（例：`T = List[T]`）。

テスト：`spec/lang/bidirectional_type_test.almd`、`spec/lang/type_annotation_test.almd`、`spec/lang/lambda_test.almd`

---

## 9. 構造的型付けと開いたレコード

Almide は開いたレコード型を通じて、レコードの構造的部分型付けをサポートする。

### 互換性の規則

| 仮引数の型 | 実引数の型 | 結果 |
|---|---|---|
| `{ a: Int }` | `{ a: Int }` | 成功 |
| `{ a: Int }` | `{ a: Int, b: String }` | エラー（閉じており、追加フィールドがある） |
| `{ a: Int, .. }` | `{ a: Int, b: String }` | 成功（開いており、追加を許可） |
| `{ a: Int, .. }` | フィールド `a: Int` を持つ `Named` | 成功（名前付き型を解決） |
| `{ a: Int, .. }` | `{ a: Int }` | 成功（完全一致も許可） |

### 開いたレコードとの単一化

開いたレコードは順序に依存しないフィールド照合を使う。`{ a: Int, .. }` と `{ a: Int, b: String }` の場合、必要な各フィールドに対して、実際の型に同名の対応フィールドがあれば単一化が成功する。

### 連鎖呼び出し

開いたレコード引数は合成できる。`{ name: String, breed: String, .. }` を受け取る関数は、その引数を `{ name: String, .. }` を受け取る関数に渡せる。

```almide
fn chain_b(x: { name: String, .. }) -> String = x.name
fn chain_a(x: { name: String, breed: String, .. }) -> String = chain_b(x)
```

テスト：`spec/lang/open_record_test.almd`

---

## 10. Protocol システム

protocol は、適合する型が実装しなければならないメソッド群を定める。Rust の trait や Haskell の型クラスと同じ役割を果たす。

### Protocol の定義

```almide
protocol Showable {
  fn show(a: Self) -> String
}
```

メソッドシグネチャ内の `Self` は `TypeVar("Self")` であり、適合する各箇所で具体型に置換される。

### 適合の宣言

型で適合を宣言し、必要な規約メソッドを用意する（導出可能な組み込み protocol なら導出を使ってもよい）：

**規約メソッド**（唯一の仕組み。`impl` ブロックは削除された）：

```almide
protocol Showable {
  fn show(a: Self) -> String
}

type Dog: Showable = { name: String }
fn Dog.show(d: Dog) -> String = "Dog: " + d.name

test "conformance by convention method" {
  assert_eq(Dog { name: "Rex" }.show(), "Dog: Rex")
}
```

規約メソッドは、関数環境へ `Type.method` として登録される。

### 組み込み Protocol

| Protocol | メソッド | 注記 |
|---|---|---|
| `Eq` | `fn eq(a: Self, b: Self) -> Bool` | `Fn` 以外のすべての値型が Eq。自動導出。 |
| `Repr` | `fn repr(v: Self) -> String` | 文字列表現。 |
| `Ord` | `fn cmp(a: Self, b: Self) -> Int` | 順序（-1、0、1）。フィールドが順序要件を満たせば `type T: Ord = ...` で導出可能。`spec/wasm_cross/ord_recursive.almd` 参照。規約メソッドでも実装できる。順序コンビネーターは [ALS-C9](../../../../specs/als/collections.md#als-c9-順序系コンビネータの全域性) を参照。 |
| `Hash` | `fn hash(v: Self) -> Int` | ハッシュ値。`Float`、`Fn`、`Map` は不可。 |
| `Codec` | `fn encode(v: Self) -> Value`、`fn decode(v: Value) -> Result[Self, String]` | シリアライズ。レコード型は `fn decode_strict(v: Value) -> Result[Self, String]` も導出し、デコード対象のフィールドにないキーを拒否する（C-429）。フィールドに `@skip` / `@skip_decode` を付けられる（C-428）。 |

### Protocol の検証

すべての宣言を登録した後、検査器は以下を検証する：

1. 必要なメソッドがすべて（規約メソッドとして）定義されている。
2. メソッドシグネチャが protocol 定義と一致する（引数型、戻り値型、引数の数）。
3. `Self` が具体型へ正しく置換されている。

### Protocol をジェネリック境界として使う

```almide
fn display[T: Showable](item: T) -> String = item.show()
```

各呼び出し位置で、検査器は `T` の具体型が `Showable` への適合を宣言しているか確認する。

### 複数の Protocol

```almide
protocol Showable {
  fn show(a: Self) -> String
}
protocol Nameable {
  fn get_name(a: Self) -> String
}

type Widget: Showable, Nameable = { id: Int, name: String }
fn Widget.show(w: Widget) -> String = "#${int.to_string(w.id)}"
fn Widget.get_name(w: Widget) -> String = w.name

fn show_named[T: Showable + Nameable](item: T) -> String = item.get_name() + ": " + item.show()

test "multiple bounds" {
  assert_eq(show_named(Widget { id: 1, name: "knob" }), "knob: #1")
}
```

### ジェネリック Protocol（明示的適合）

protocol は型パラメーターを宣言できる。型は、宣言時に指定したその protocol の**一つ**のインスタンス化に適合する。ジェネリック関数の境界は**適用済み** protocol である：

```almide
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

test "generic protocol through an applied bound" {
  var users = Users { rows: [] }
  rename(users, 1, "ada")
  assert_eq(lookup(users, 1).map((u) => u.name) ?? "-", "ada")
}
```

検査器が強制する規則：

1. **正確な引数。** 適合宣言と境界は、protocol の各パラメーターに一つずつ型を与える（`Repository[Int, User]`）。ジェネリック protocol を引数なし、または誤った引数個数で指定すると、参照位置でエラーとなる。
2. **Protocol ごとに一つの適合。** `type X: Repository[Int, User],
   Repository[String, User]` は曖昧（`fn X.find` の定義群は一つの実装を表す）であり拒否される。同じ protocol を二回指定する境界も同様。
3. **決定的な実装。** `[R: Repository[Int,
   User]]` を通じた呼び出しは、実引数型自身の `Type.method` を実行する。型で宣言した引数は境界の引数と一致しなければならない（名目的に比較し、フィールドが同じ二つのレコードでも別の引数とする）。他の実装は探索しない。境界だけが指定する文字（上の `K`、`V`）は、呼び出し位置でも単相化でも適合宣言によって固定される。
4. **Effect と所有権はメソッドシグネチャの一部。** ジェネリック protocol のメソッド実装と protocol メソッドの `effect` 指定は必要十分の関係で一致し、`mut` 引数も厳密に同じ引数に付ける（`mut self` は `mut self: Self` の略記）。
5. **修飾名。** 別モジュールで宣言された protocol は、型と同様にそのモジュールを付けて指定する：`[R: ports.Repository[K, V]]`、`type MemUsers: ports.Repository[domain.UserId, domain.User]`。修飾子は protocol を宣言したモジュールを指定しなければならない。非修飾名もまだ解決される（以前は protocol 名がグローバルだった）が、非推奨警告となり、機械適用可能な修正は修飾した綴りになる。

境界を通じた呼び出しは単相化によって lowering されるので、境界経由の effect メソッドも通常の effect 呼び出しと同じように `!` で伝播する。protocol は型ではない（`List[Repository[Int, User]]` は E029）。protocol の型としての利用は、別の後続段階である（#1589）。

テスト：`spec/wasm_cross/generic_protocol_conformance.almd`（C-365）、`spec/integration/generic_protocol_ports/`（モジュールをまたぐ ports / adapters / service）、`tests/diagnostics/protocol-*`、`tests/protocol_qualification_test.rs`

### マーカー Protocol

メソッドのない protocol はマーカーとして働く：

```almide
protocol Serializable {}
type Marker: Serializable = { tag: String }
```

テスト：`spec/lang/protocol_test.almd`、`spec/lang/trait_impl_test.almd`、`spec/lang/protocol_generics_test.almd`、`spec/lang/protocol_advanced_test.almd`、`spec/lang/derive_conventions_test.almd`

---

## 11. 型エイリアス

`type Name = ExistingType` は透明なエイリアスを作る。エイリアスは元の型と交換可能である。

```almide
type Score = Int
type Label = String

let s: Score = 100
let total = s + 50      // works: Score is Int
```

型エイリアスは `TypeEnv.resolve_named` が解決する。これは `env.types` で名前を検索し、元となる型定義を返す。

テスト：`spec/lang/type_alias_test.almd`

---

## 12. タグ付き Variant と内部 Union 型

整数または文字列のペイロードを持つ公開型では、明示的なペイロード付き variant ケースを宣言する：

```almide
type StringOrInt = IntValue(Int) | StringValue(String)

fn render(value: StringOrInt) -> String = match value {
  IntValue(n) => int.to_string(n),
  StringValue(s) => s,
}
```

`type StringOrInt = Int | String` は、タグなしの整数または文字列のペイロード型を宣言するものでは**ない**。選択肢がすべて大文字で始まる単純名の場合、パーサーは宣言を `Int` と `String` という二つの **unit ケース**を持つ variant と解釈する。ペイロードには上記の明示的なコンストラクターを使う。

これとは別に、コンパイラ内部の `Ty::Union(Vec<Ty>)` は要素をソートし重複を除いて格納する。`Ty::union()` コンストラクターは入れ子の union を平坦化し、重複を除く：

- `Ty::union([Int, String, Int])` は `Ty::Union([Int, String])` を生成する。
- `Ty::union([Int])` は `Ty::Int` を生成する（要素が一つならラップを外す）。

union との単一化は、束縛のスナップショットを使って各要素を試し、最初に成功したものを確定する。

---

## 13. コンパイラ内での型の流れ

```text
Source (.almd)
    │
    ▼
  Parse → AST (untyped)
    │
    ▼
  Check → expr_types: HashMap<ExprId, Ty>
    │        ├── registration.rs  — register FnSig, type decls, protocols
    │        ├── infer.rs         — walk AST, assign ?N vars, collect constraints
    │        ├── solving.rs       — Union-Find unification
    │        └── resolve          — substitute ?N → concrete Ty in expr_types
    │
    ▼
  Lower → Typed IR (IrProgram)
    │        ├── trusts expr_types — no type guessing
    │        ├── desugars pipe, UFCS, interpolation, operators
    │        └── assigns VarId to every variable reference
    │
    ▼
  Codegen → target source (Rust / WASM)
             ├── reads IR types for dispatch decisions
             └── emits target-specific type representations
```

### 主要データ構造

- **`Ty` 列挙型**（`crates/almide-types/src/types/mod.rs`）：内部の型表現。この列挙型が、サイズ付き数値型、関数の effect フラグ、コンパイル時定数パラメーターを含む正本の一覧である。
- **`TypeEnv`**（`crates/almide-frontend/src/type_env.rs`）：型環境。型宣言、関数シグネチャ、スコープ、protocol 定義、適合の追跡を保持する。
- **`FnSig`**（`crates/almide-types/src/types/mod.rs`）：引数、戻り値型、ジェネリクス、構造的境界、protocol 境界を持つ関数シグネチャ。
- **`TypeConstructorId`**（`crates/almide-types/src/types/constructor.rs`）：型コンストラクター（List、Option、Result、Map、Set、ユーザー定義）を識別する。`Ty` の `Applied` variant が使う。
- **`Kind`**（`crates/almide-types/src/types/constructor.rs`）：「型コンストラクターの型」。`*`、`* -> *`、`* -> * -> *`。
- **`UnionFind`**（`crates/almide-frontend/src/check/types.rs`）：推論変数の同値関係のための素集合構造。ランクによる併合と経路圧縮を使う。
- **`Checker`**（`crates/almide-frontend/src/check/mod.rs`）：3 パスの型検査パイプラインを統括する。

### 単一化（`crates/almide-types/src/types/unify.rs`）

`unify` 関数はシグネチャの型と具体型を照合し、`TypeVar` の束縛を集める：

1. シグネチャ側が `TypeVar`：実際の型へ束縛する（出現検査あり）。
2. 実際の型側が `TypeVar`：受け入れる（多相的互換性）。
3. `Applied` 型：コンストラクター ID を照合し、引数を再帰的に単一化する。
4. `Union` 型：束縛のスナップショットを使って各要素を試す。
5. その他：`Ty::compatible` に委譲する。

`substitute` は、型中の束縛済み `TypeVar` を、その束縛先に置換する。

### 型コンストラクターレジストリ（`crates/almide-types/src/types/constructor.rs`）

各型コンストラクターは kind と代数法則を伴って登録される。このレジストリはすべてのコンテナー型に対する統一的な操作を可能にし、ストリーム融合の最適化を支える：

| コンストラクター | Kind | 代数法則 |
|---|---|---|
| `List` | `* -> *` | FunctorComposition, FunctorIdentity, FilterComposition, MapFoldFusion, MapFilterFusion |
| `Option` | `* -> *` | FunctorComposition, FunctorIdentity, MonadAssociativity |
| `Result` | `* -> * -> *` | FunctorComposition, FunctorIdentity |
| `Set` | `* -> *` | （なし） |
| `Map` | `* -> * -> *` | （なし） |

ユーザー定義型は、引数の数を指定して `register_user_type` で登録する。
