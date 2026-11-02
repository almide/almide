<a id="type-system-specification"></a>

# 类型系统规格

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](type-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](type-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](type-system.zh-CN.md)



> 权威原文：[docs/specs/type-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是非规范性的读者快照译文，不自动同步。原规格及其声明的规范地位保持不变。模型证明、实现主张、历史状态和证据边界均保留原文范围；本译文不提供独立执行、测量、符合性或安全性证明。代码块内代码及注释逐字保留。

> 最后更新：2026-10-31

Almide 使用基于约束的类型系统，包含双向推断、记录的结构化类型，以及用于特设多态的 protocol 系统。所有类型在编译期解析；没有运行时类型检查。

> 原文范围说明：“没有运行时类型检查”不能扩展为没有任何运行时检查，或整体安全性已获证明。§3 的“自动解包”保留原文措辞，但其描述带有显式 `!`，不意味着引入隐式传播。

---

<a id="1-primitive-types"></a>

## 1. 基本类型

以下核心内建类型的 kind 为 `*`（具体类型，零个类型参数）。定宽数值类型（`Int8` / `Int16` / `Int32` / `Int64`、`UInt8` / `UInt16` / `UInt32` / `UInt64`、`Float32` / `Float64`）也为内建；参见 `spec/lang/sized_numeric_types_test.almd` 和对应的[标准库参考](../../../../stdlib/README.md)。

| 类型 | 描述 | Eq | Hash | Ord |
|---|---|---|---|---|
| `Int` | 64 位有符号整数 | 是 | 是 | 是 |
| `Float` | 64 位 IEEE 754 | 是 | 否 | 是 |
| `String` | UTF-8 字符串 | 是 | 是 | 是 |
| `Bool` | `true` / `false` | 是 | 是 | 是 |
| `Unit` | 零值类型 `()` | 是 | 是 | 是 |
| `Bytes` | 字节缓冲区 | 是 | 是 | 否 |
| `Matrix` | 数值矩阵 | 是 | 否 | 否 |

`Float` 不可哈希（不能作 `Map` 的键或 `Set` 的元素）。函数类型（`Fn`）既不具有 Eq，也不可哈希。

```almide
let x = 42          // Int
let y = 3.14        // Float
let s = "hello"     // String
let b = true        // Bool
let u = ()          // Unit
```

测试：`spec/lang/expr_test.almd`、`spec/lang/bytes_test.almd`、`spec/lang/matrix_test.almd`

---

<a id="2-collection-types"></a>

## 2. 集合类型

### List[T]

同质的有序序列。kind：`* -> *`。

```almide
let xs = [1, 2, 3]              // List[Int]
let ys: List[String] = []       // empty list requires annotation
let zs = xs + [4, 5]            // + concatenates lists
let first = xs[0]               // Int (index access)
```

### Map[K, V]

键值字典。kind：`* -> * -> *`。键必须可哈希（不能使用 `Float`、`Fn` 或 `Map` 作键）。

```almide
let m = ["a": 1, "b": 2]       // Map[String, Int]
let empty: Map[String, Int] = [:]
let v = m["a"]                  // Option[Int]
```

### Set[T]

元素唯一的集合。kind：`* -> *`。元素必须可哈希。

```almide
let s = set.from_list([1, 2, 3])  // Set[Int]
```

### Tuple

固定长度的异质积类型。元数可变。

```almide
test "tuples" {
  let t = (1, "hello", true)     // (Int, String, Bool)
  let x = t.0                    // Int — positional access
  let (a, b) = (1, 2)            // destructuring
  assert_eq(x + a + b, 4)
}
```

测试：`spec/lang/data_types_test.almd`、`spec/lang/tuple_test.almd`、`spec/lang/map_literal_test.almd`

---

<a id="3-optiont-and-resultt-e"></a>

## 3. Option[T] 与 Result[T, E]

用于可缺失值及错误处理的内建参数化类型。两者均为具有专用构造器的 `Applied` 类型。

### Option[T]

kind：`* -> *`。构造器：`some(v)`、`none`。

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

kind：`* -> * -> *`。构造器：`ok(v)`、`err(e)`。

```almide
let x: Result[Int, String] = ok(42)
let y: Result[Int, String] = err("fail")
```

在 `effect fn` 的函数体内，`Result` 通过 `!` 自动解包：

```almide
import fs

effect fn read(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!   // propagates err
  ok(content)
}
```

测试：`spec/lang/data_types_test.almd`、`spec/lang/error_test.almd`、`spec/lang/unwrap_operators_test.almd`

---

<a id="4-record-types"></a>

## 4. 记录类型

<a id="named-records"></a>

### 具名记录

使用 `type` 声明。按名称访问字段。

```almide
type Point = { x: Float, y: Float }

let p: Point = { x: 1.0, y: 2.0 }
let px = p.x                          // Float
let p2 = { ...p, y: 5.0 }            // spread update
```

<a id="anonymous-records"></a>

### 匿名记录

无类型名的记录字面量按结构赋型。

```almide
let user = { name: "alice", age: 30 }   // { name: String, age: Int }
let n = user.name                        // String
```

<a id="open-records-row-polymorphism"></a>

### 开放记录（行多态）

类型为 `{ field: Type, .. }` 的参数接受至少拥有所需字段的任何记录。允许并保留额外字段。

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

开放记录可用作类型别名（形状别名）：

```almide
type Named = { name: String, .. }
fn greet_named(who: Named) -> String = "Hi, ${who.name}!"
```

支持嵌套开放记录：

```almide
fn get_port(app: { config: { port: Int, .. }, .. }) -> Int = app.config.port
```

封闭记录（不带 `..` 的 `{ name: String }`）要求字段精确匹配。

测试：`spec/lang/open_record_test.almd`、`spec/lang/record_spread_test.almd`

---

<a id="5-variant-types"></a>

## 5. Variant 类型

代数数据类型（带标签联合）。通过 `|` 分隔的分支声明。

<a id="unit-payload-enum-like"></a>

### Unit 载荷（类似枚举）

```almide
type Direction = | North | South | East | West

fn to_str(d: Direction) -> String = match d {
  North => "N",
  South => "S",
  East => "E",
  West => "W",
}
```

<a id="tuple-payload"></a>

### 元组载荷

```almide
type Shape = | Circle(Float) | Rect(Float, Float)

fn area(s: Shape) -> Float = match s {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
}
```

<a id="record-payload"></a>

### 记录载荷

variant 分支可以携带具名字段：

```almide
type Pat =
  | Match { scope: String, regex: String }
  | BeginEnd { scope: String, begin: String, end_pat: String, patterns: List[Pat] }
  | Include(String)
  | Empty
```

记录 variant 的构造与模式匹配：

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

<a id="recursive-variants"></a>

### 递归 Variant

variant 类型可以引用自身。类型检查器通过环检测防止 Eq／Hash 检查无限循环。

```almide
type Tree[T] = | Leaf(T) | Node(T, List[Tree[T]])
```

<a id="inline-variants-no-leading-pipe"></a>

### 行内 Variant（无前导竖线）

省略前导 `|` 时，直接从第一个分支开始：

```almide
type AppError = NotFound(String) | Io(String)
```

测试：`spec/lang/data_types_test.almd`、`spec/lang/type_system_test.almd`、`spec/lang/variant_record_test.almd`

---

<a id="6-function-types"></a>

## 6. 函数类型

函数是一等值。类型语法使用 `fn(Params) -> Ret`。

```almide
fn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)

fn make_adder(n: Int) -> fn(Int) -> Int = (x) => x + n

test "function values" {
  let add5 = make_adder(5)
  assert_eq(apply(add5, 10), 15)
}
```

内部表示为 `Ty::Fn { params: Vec<Ty>, ret: Box<Ty>, is_effect: bool }`。

函数类型永远不具有 Eq，也永远不可哈希。

<a id="effect-functions"></a>

### Effect 函数

`effect fn` 标记执行副作用的函数。类型检查器强制纯函数不得调用 effect 函数（错误 E006）。`FnSig` 上的 `is_effect` 标志跟踪这一属性。

```almide
import fs

effect fn read_file(path: String) -> Result[String, String] = fs.read_text(path)
```

<a id="type-aliases-for-function-types"></a>

### 函数类型的类型别名

```almide
type Handler = (String) -> String
```

测试：`spec/lang/type_system_test.almd`、`spec/lang/function_test.almd`、`spec/lang/lambda_test.almd`、`spec/lang/effect_fn_test.almd`

---

<a id="7-user-defined-generics"></a>

## 7. 用户定义的泛型

泛型类型参数使用 `[]` 语法（不是 `<>`）。

<a id="generic-functions"></a>

### 泛型函数

```almide
fn id[T](x: T) -> T = x
fn pair[A, B](a: A, b: B) -> (A, B) = (a, b)
```

类型实参可以推断，也可以显式给出：

```almide
fn id[T](x: T) -> T = x

test "type arguments" {
  assert_eq(id(42), 42)              // T inferred as Int
  assert_eq(id[String]("hi"), "hi")  // T explicitly String
}
```

<a id="generic-record-types"></a>

### 泛型记录类型

```almide
type Box[T] = { value: T, label: String }

fn unbox[T](b: Box[T]) -> T = b.value
```

<a id="generic-variant-types"></a>

### 泛型 Variant 类型

```almide
type Either[A, B] = | Left(A) | Right(B)

fn map_right[A, B, C](e: Either[A, B], f: fn(B) -> C) -> Either[A, C] = match e {
  Left(a) => Left(a),
  Right(b) => Right(f(b)),
}
```

<a id="structural-bounds-on-generics"></a>

### 泛型的结构约束

泛型参数可以要求特定记录字段：

```almide
fn describe[T: { name: String, .. }](x: T) -> String = "name: ${x.name}"

fn set_name[T: { name: String, .. }](x: T, n: String) -> T = { ...x, name: n }
```

约束 `T: { name: String, .. }` 是 `OpenRecord` 约束。检查器将它存储于 `FnSig.structural_bounds`，并在每个调用点验证。

<a id="protocol-bounds-on-generics"></a>

### 泛型的 Protocol 约束

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

多个约束以 `+` 连接。检查器验证每个调用点的具体类型已声明符合所有所需 protocol。

测试：`spec/lang/generics_test.almd`、`spec/lang/type_system_test.almd`、`spec/lang/open_record_test.almd`、`spec/lang/protocol_generics_test.almd`

---

<a id="8-type-inference"></a>

## 8. 类型推断

Almide 使用基于约束的类型推断，配合 Union-Find 数据结构。

<a id="three-pass-architecture"></a>

### 三遍架构

1. **推断**（`infer.rs`）：遍历 AST，为未知类型分配新类型变量（`?0`、`?1`、……），收集类型之间的等式约束。
2. **求解**（`solving.rs`）：通过合一处理约束。Union-Find 合并等价类型变量并将其绑定到具体类型。
3. **替换**（`mod.rs`）：将 `expr_types` 中所有推断变量替换为已解析的具体类型。

<a id="inference-variables"></a>

### 推断变量

新类型变量命名为 `?N`（例如 `?0`、`?1`）。它们不同于用户声明的 `TypeVar`（`T`、`U`）。`UnionFind` 通过按秩合并与路径压缩管理等价类。

<a id="bidirectional-inference"></a>

### 双向推断

类型既向前流动（从参数到返回值），也向后流动（从期望类型到表达式）：

```almide
test "inference from annotation and from use" {
  let xs: List[Int] = []         // [] gets type List[Int] from annotation
  let f = (x) => x + 1           // x inferred as Int from + operator
  assert_eq(list.len(xs), 0)
  assert_eq(f(1), 2)
}
```

<a id="let-polymorphism"></a>

### Let 多态

泛型函数在每个调用点使用新的推断变量实例化：

```almide
fn id[T](x: T) -> T = x

test "instantiated per call" {
  assert_eq(id(42), 42)             // T = Int at this call
  assert_eq(id("hello"), "hello")   // T = String at this call
}
```

<a id="lambda-parameter-inference"></a>

### Lambda 参数推断

lambda 参数从其使用方式推断：

```almide
test "lambda parameter inferred from the list" {
  assert_eq(list.map([1, 2, 3], (x) => x * 2), [2, 4, 6])   // x: Int inferred from List[Int]
}
```

<a id="constraint-solving"></a>

### 约束求解

约束是 `(expected, actual, context)` 三元组。求解器对每一对进行合一：

- **推断变量 + 具体类型**：把变量绑定到具体类型。
- **推断变量 + 推断变量**：合并两个变量。
- **具体类型 + 具体类型**：按结构递归（例如 `List[?0]` 与 `List[Int]` 合一出 `?0 = Int`）。
- **Unknown**：与一切合一（错误恢复哨兵）。

**出现检查（occurs check）**防止无限类型（如 `T = List[T]`）。

测试：`spec/lang/bidirectional_type_test.almd`、`spec/lang/type_annotation_test.almd`、`spec/lang/lambda_test.almd`

---

<a id="9-structural-typing-and-open-records"></a>

## 9. 结构化类型与开放记录

Almide 通过开放记录类型支持记录的结构化子类型关系。

<a id="compatibility-rules"></a>

### 兼容性规则

| 形参类型 | 实参类型 | 结果 |
|---|---|---|
| `{ a: Int }` | `{ a: Int }` | 成功 |
| `{ a: Int }` | `{ a: Int, b: String }` | 错误（封闭，有额外字段） |
| `{ a: Int, .. }` | `{ a: Int, b: String }` | 成功（开放，允许额外字段） |
| `{ a: Int, .. }` | 含字段 `a: Int` 的 `Named` | 成功（解析具名类型） |
| `{ a: Int, .. }` | `{ a: Int }` | 成功（也允许精确匹配） |

<a id="unification-with-open-records"></a>

### 与开放记录合一

开放记录按与顺序无关的方式匹配字段。对于 `{ a: Int, .. }` 与 `{ a: Int, b: String }`，若实际类型中每个所需字段都有按名称对应的字段，则合一成功。

<a id="chain-calling"></a>

### 链式调用

开放记录参数可以组合：接受 `{ name: String, breed: String, .. }` 的函数，可以把其参数传给接受 `{ name: String, .. }` 的函数。

```almide
fn chain_b(x: { name: String, .. }) -> String = x.name
fn chain_a(x: { name: String, breed: String, .. }) -> String = chain_b(x)
```

测试：`spec/lang/open_record_test.almd`

---

<a id="10-protocol-system"></a>

## 10. Protocol 系统

protocol 定义符合类型必须实现的一组方法。它们发挥与 Rust 的 trait 或 Haskell 的类型类相同的作用。

<a id="defining-a-protocol"></a>

### 定义 Protocol

```almide
protocol Showable {
  fn show(a: Self) -> String
}
```

方法签名中的 `Self` 是 `TypeVar("Self")`，在每个符合性声明位置替换为具体类型。

<a id="declaring-conformance"></a>

### 声明符合性

在类型上声明符合性，并提供所需的约定方法（对于可派生的内建 protocol，也可以使用派生）：

**约定方法**（唯一机制，`impl` 块已移除）：

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

约定方法在函数环境中注册为 `Type.method`。

<a id="built-in-protocols"></a>

### 内建 Protocol

| Protocol | 方法 | 说明 |
|---|---|---|
| `Eq` | `fn eq(a: Self, b: Self) -> Bool` | 除 `Fn` 外，所有值类型均具有 Eq。自动派生。 |
| `Repr` | `fn repr(v: Self) -> String` | 字符串表示。 |
| `Ord` | `fn cmp(a: Self, b: Self) -> Int` | 排序（-1、0、1）。字段满足排序要求时，可通过 `type T: Ord = ...` 派生；见 `spec/wasm_cross/ord_recursive.almd`。约定方法也可实现该 protocol。排序组合子参见 [ALS-C9](../../../../specs/als/collections.md#als-c9-順序系コンビネータの全域性)。 |
| `Hash` | `fn hash(v: Self) -> Int` | 哈希值。不允许 `Float`、`Fn` 或 `Map`。 |
| `Codec` | `fn encode(v: Self) -> Value`、`fn decode(v: Value) -> Result[Self, String]` | 序列化。记录类型还派生 `fn decode_strict(v: Value) -> Result[Self, String]`，拒绝不属于其解码字段的键（C-429）；字段可带 `@skip` / `@skip_decode`（C-428）。 |

<a id="protocol-validation"></a>

### Protocol 验证

注册全部声明后，检查器验证：

1. 所有所需方法均已定义（作为约定方法）。
2. 方法签名匹配 protocol 定义（参数类型、返回类型、元数）。
3. `Self` 已正确替换为具体类型。

<a id="using-protocols-as-generic-bounds"></a>

### 将 Protocol 用作泛型约束

```almide
fn display[T: Showable](item: T) -> String = item.show()
```

每个调用点都会检查 `T` 的具体类型是否已声明符合 `Showable`。

<a id="multiple-protocols"></a>

### 多个 Protocol

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

<a id="generic-protocols-explicit-conformance"></a>

### 泛型 Protocol（显式符合性）

protocol 可以声明类型参数。一个类型符合声明时指定的该 protocol 的**一个**实例化；泛型函数由**已应用的** protocol 约束：

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

检查器强制执行的规则：

1. **精确实参。** 符合性声明和约束为每个 protocol 参数给出一个类型（`Repository[Int, User]`）。引用泛型 protocol 时省略实参或实参数量错误，都会在引用位置报错。
2. **每个 protocol 只有一次符合性。** `type X: Repository[Int, User],
   Repository[String, User]` 存在歧义（`fn X.find` 的定义是同一个实现），因而被拒绝；约束两次指定同一个 protocol 也会被拒绝。
3. **确定性实现。** 经 `[R: Repository[Int,
   User]]` 的调用执行实参类型自身的 `Type.method`。类型声明的实参必须与约束实参相等（按名义比较：字段相同的两个记录也是不同实参）；不搜索其他实现。仅由约束提及的字母（上文的 `K`、`V`）在调用点和单态化中均由符合性声明固定。
4. **Effect 与所有权属于方法签名。** 泛型 protocol 方法的实现，当且仅当 protocol 方法为 `effect` 时为 `effect`，并且恰好对相同参数采用 `mut`（`mut self` 是 `mut self: Self` 的简写）。
5. **限定名称。** 另一模块声明的 protocol 与类型一样，用该模块限定：`[R: ports.Repository[K, V]]`、`type MemUsers: ports.Repository[domain.UserId, domain.User]`。限定符必须指定声明该 protocol 的模块。裸名称仍能解析（以前 protocol 名是全局的），但产生弃用警告，其机器可应用修复是使用限定写法。

通过约束的调用由单态化 lowering，因此经约束到达的 effect 方法，与普通 effect 调用一样用 `!` 传播。protocol 不是类型（`List[Repository[Int, User]]` 会产生 E029）；将 protocol 当作类型是另一个后续阶段（#1589）。

测试：`spec/wasm_cross/generic_protocol_conformance.almd`（C-365）、`spec/integration/generic_protocol_ports/`（跨模块的 ports / adapters / service）、`tests/diagnostics/protocol-*`、`tests/protocol_qualification_test.rs`

<a id="marker-protocols"></a>

### 标记 Protocol

没有方法的 protocol 用作标记：

```almide
protocol Serializable {}
type Marker: Serializable = { tag: String }
```

测试：`spec/lang/protocol_test.almd`、`spec/lang/trait_impl_test.almd`、`spec/lang/protocol_generics_test.almd`、`spec/lang/protocol_advanced_test.almd`、`spec/lang/derive_conventions_test.almd`

---

<a id="11-type-aliases"></a>

## 11. 类型别名

`type Name = ExistingType` 创建透明别名。别名可以与底层类型互换。

```almide
type Score = Int
type Label = String

let s: Score = 100
let total = s + 50      // works: Score is Int
```

类型别名由 `TypeEnv.resolve_named` 解析，它在 `env.types` 中查找名称并返回底层类型定义。

测试：`spec/lang/type_alias_test.almd`

---

<a id="12-tagged-variants-and-internal-union-types"></a>

## 12. 带标签 Variant 与内部 Union 类型

若公开类型要携带整数或字符串，请声明显式携带载荷的 variant 分支：

```almide
type StringOrInt = IntValue(Int) | StringValue(String)

fn render(value: StringOrInt) -> String = match value {
  IntValue(n) => int.to_string(n),
  StringValue(s) => s,
}
```

`type StringOrInt = Int | String` **不是**声明无标签的整数或字符串载荷类型：当所有备选项都是大写开头的简单名称时，解析器将其解释为具有 `Int` 与 `String` 两个 **unit 分支**的 variant。载荷请使用上面的显式构造器。

另外，编译器内部的 `Ty::Union(Vec<Ty>)` 将成员排序并去重存储。`Ty::union()` 构造器会展平嵌套 union 并去重：

- `Ty::union([Int, String, Int])` 产生 `Ty::Union([Int, String])`。
- `Ty::union([Int])` 产生 `Ty::Int`（单成员时解包）。

与 union 合一时，使用绑定快照逐个尝试成员，提交第一个成功结果。

---

<a id="13-how-types-flow-through-the-compiler"></a>

## 13. 类型如何流经编译器

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

<a id="key-data-structures"></a>

### 关键数据结构

- **`Ty` 枚举**（`crates/almide-types/src/types/mod.rs`）：内部类型表示；该枚举是权威清单，包括定宽数值类型、函数 effect 标志和编译期常量参数。
- **`TypeEnv`**（`crates/almide-frontend/src/type_env.rs`）：类型环境，保存类型声明、函数签名、作用域、protocol 定义和符合性跟踪。
- **`FnSig`**（`crates/almide-types/src/types/mod.rs`）：包含参数、返回类型、泛型、结构约束及 protocol 约束的函数签名。
- **`TypeConstructorId`**（`crates/almide-types/src/types/constructor.rs`）：标识类型构造器（List、Option、Result、Map、Set、用户定义）。由 `Ty` 的 `Applied` 分支使用。
- **`Kind`**（`crates/almide-types/src/types/constructor.rs`）：“类型构造器的类型”，即 `*`、`* -> *`、`* -> * -> *`。
- **`UnionFind`**（`crates/almide-frontend/src/check/types.rs`）：用于推断变量等价关系的并查集结构，按秩合并并进行路径压缩。
- **`Checker`**（`crates/almide-frontend/src/check/mod.rs`）：统筹三遍类型检查管线。

<a id="unification-cratesalmide-typessrctypesunifyrs"></a>

### 合一（`crates/almide-types/src/types/unify.rs`）

`unify` 函数将签名类型与具体类型匹配，收集 `TypeVar` 绑定：

1. 签名侧为 `TypeVar`：绑定到实际类型（带出现检查）。
2. 实际侧为 `TypeVar`：接受（多态兼容性）。
3. `Applied` 类型：匹配构造器 ID，递归合一实参。
4. `Union` 类型：使用绑定快照逐个尝试成员。
5. 其他情况：委托给 `Ty::compatible`。

`substitute` 将类型中已绑定的 `TypeVar` 替换成其绑定值。

<a id="type-constructor-registry-cratesalmide-typessrctypesconstructorrs"></a>

### 类型构造器注册表（`crates/almide-types/src/types/constructor.rs`）

每个类型构造器都连同其 kind 和代数律一起注册。注册表使所有容器类型能够采用统一操作，并支持流融合优化：

| 构造器 | Kind | 代数律 |
|---|---|---|
| `List` | `* -> *` | FunctorComposition, FunctorIdentity, FilterComposition, MapFoldFusion, MapFilterFusion |
| `Option` | `* -> *` | FunctorComposition, FunctorIdentity, MonadAssociativity |
| `Result` | `* -> * -> *` | FunctorComposition, FunctorIdentity |
| `Set` | `* -> *` | （无） |
| `Map` | `* -> * -> *` | （无） |

用户定义类型通过 `register_user_type` 按其元数注册。
