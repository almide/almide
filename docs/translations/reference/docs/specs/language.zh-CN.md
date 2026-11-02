> 最后更新：2026-10-18

<a id="almide-language-specification"></a>

# Almide 语言规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](language.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](language.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](language.zh-CN.md)

> 非规范性的固定读者译本。以固定原文为准，不自动同步，也不表示已独立执行验证。保留原文的历史日期和主张，不构成新的符合性认证。原文差异保持不变：此处列出可选 module 声明，而 module-system 说无需声明；§7.4 位于 §5.9 下；不同原文页面对 fan 当前/未来时间形式的描述不同。保留内置 + 重载与禁止运算符重载两种表述，不扩大其含义。原文将 Unit 描述为 zero-value type。§4.6 保留顶层 let 通过 const/LazyLock 求值的说明，而 ALS-T7 规定在启动时、main 之前按声明顺序求值。这里不自行解决这项原文间的差异。以下测试和安全边界是原文记述，不是新执行的测试或独立审计。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/language.md)


文件扩展名：`.almd`

---

<a id="1-file-structure"></a>

## 1. 文件结构

Almide 源文件按顺序包含三个部分：

1. **模块声明**（可选）—— `module <path>`
2. **导入**——零个或多个 `import` 声明
3. **声明**——函数、类型、顶层 let、协议、测试

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

### 导入

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

选择性导入使用 `import path.{ Name1, Name2 }` 语法。别名使用 `as`。

测试： `spec/lang/import_test.almd`

<a id="identifiers"></a>

### 标识符

标识符遵循 UAX #31 Default Identifiers：首字符为 XID_Start 或 `_`，随后为 XID_Continue 字符（Rust RFC 2457、Python PEP 3131 与 ECMAScript 的规则）。名称按 NFC 存储，因此 `é` 与 `e` + U+0301 是同一名称。兼容形式不同的字符（全角 `ａ`、`１`、连字、带圈形式）不属于标识符字符，会连同应使用的普通写法一起报告。关键字为 ASCII。

首字符大写形成类型侧名称（类型、variant case、构造器）；小写或无大小写字符（漢字、かな、한글）形成值侧名称。模块用 `type` / `protocol` 或 variant case 声明的无大小写名称，在整个模块内像 `Red` 一样属于类型侧；其他模块中须限定书写（`geo.住所`）。

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

测试： `spec/lang/unicode_identifier_test.almd`,
`spec/integration/caseless_imported_types/`,
`spec/wasm_cross/unicode_identifiers.almd` (C-387),
`tests/unicode_identifier_nfc_test.rs`

---

<a id="2-types"></a>

## 2. 类型

<a id="21-primitive-types"></a>

### 2.1 基本类型

| 类型 | 说明 |
|------|-------------|
| `Int` | 64 位有符号整数 |
| `Float` | 64 位浮点数 |
| `String` | UTF-8 字符串 |
| `Bool` | `true` 或 `false` |
| `Unit` | 零值类型，写作 `()` |
| `Path` | 文件路径 |
| `Bytes` | 字节序列 |

<a id="22-collection-types"></a>

### 2.2 集合类型

| 类型 | 说明 |
|------|-------------|
| `List[T]` | 有序集合 |
| `Map[K, V]` | 键值映射 |
| `Set[T]` | 不重复值的集合 |

<a id="23-error-handling-types"></a>

### 2.3 错误处理类型

| 类型 | 说明 |
|------|-------------|
| `Result[T, E]` | 成功 `ok(v)` 或失败 `err(e)` |
| `Option[T]` | 存在 `some(v)` 或缺失 `none` |
| `T?` | `Option[T]` 的简写，在所有类型位置有效（ADR-0010）。`?` 结合到前一个类型原子，绝不跨越 `->`：`(A) -> B?` 返回 `Option[B]`；可选函数值写作 `((A) -> B)?`；嵌套 Option 写作 `(T?)?`。这是规范写法，`almide fmt` 将 `Option[T]` 规范化为 `T?` |

<a id="24-composite-types"></a>

### 2.4 复合类型

| 类型 | 说明 |
|------|-------------|
| `(A, B, ...)` | 元组 |
| `{ field: Type, ... }` | 记录（匿名） |
| `(A, B) -> C` / `effect (A, B) -> C` | 纯 / 效应函数类型 |

测试： `spec/lang/data_types_test.almd`, `spec/lang/tuple_test.almd`

---

<a id="3-type-annotations"></a>

## 3. 类型标注

<a id="31-simple-types"></a>

### 3.1 简单类型

```almide
let x: Int = 42
let name: String = "alice"
```

<a id="32-generic-types"></a>

### 3.2 泛型类型

Almide 的泛型使用 `[]`，不使用 `<>`。

```almide
let xs: List[Int] = [1, 2, 3]
let m: Map[String, Int] = ["a": 1]
let r: Result[Int, String] = ok(42)
```

<a id="33-function-types"></a>

### 3.3 函数类型

```almide
type Handler = (String) -> String
type Predicate = (Int) -> Bool
type Reducer = (Int, Int) -> Int
type Thunk = () -> Int
```

<a id="34-tuple-types"></a>

### 3.4 元组类型

```almide
type Pair = (Int, String)
type Triple = (Int, Int, Int)
```

<a id="35-record-types-anonymous"></a>

### 3.5 记录类型（匿名）

```almide
type User = { name: String, age: Int }
```

记录字段支持默认值和序列化别名：

```almide
type Config = {
  host: String = "localhost",
  port: Int = 8080,
  name as "display_name": String,
}
```

<a id="36-open-record-types-structural"></a>

### 3.6 开放记录类型（结构化）

```almide
fn get_name[T: { name: String, .. }](obj: T) -> String = obj.name
```

`..` 表示记录可能具有所列字段以外的其他字段。

测试： `spec/lang/type_annotation_test.almd`, `spec/lang/open_record_test.almd`, `spec/lang/generics_test.almd`

---

<a id="4-declarations"></a>

## 4. 声明

<a id="41-functions-fn"></a>

### 4.1 函数（`fn`）

```almide
fn add(x: Int, y: Int) -> Int = x + y
```

本体跟在 `=` 后，是单个表达式。多语句本体使用块：

```almide
fn greet(name: String) -> String = {
  let upper = string.to_upper(name)
  "Hello, ${upper}!"
}
```

也支持无花括号块：本体以 `let`、`var` 或 `guard` 开头时，解析器收集语句直到下一个顶层声明：

```almide
fn process(x: Int) -> Int =
  let doubled = x * 2
  let capped = int.min(doubled, 100)
  capped
```

<a id="generic-functions"></a>

#### 泛型函数

```almide
protocol Show {
  fn show(a: Self) -> String
}
fn identity[T](x: T) -> T = x
fn first[A, B](pair: (A, B)) -> A = pair.0
fn apply[T: Show](x: T) -> String = x.show()
```

泛型参数使用 `[]`。约束用 `:` 指定，以 `+` 组合：

```almide
protocol Show {
  fn show(a: Self) -> String
}
fn same_or_both[T: Show + Eq](x: T, y: T) -> String =
  if x == y then x.show() else x.show() + "/" + y.show()
```

结构性约束使用开放记录类型：

```almide
fn name_of[T: { name: String, .. }](x: T) -> String = x.name
```

<a id="default-parameters"></a>

#### 默认参数

```almide
fn connect(host: String, port: Int = 8080) -> String =
  "${host}:${int.to_string(port)}"
```

第一个有默认值的参数之后，所有参数也必须有默认值。

<a id="named-arguments"></a>

#### 命名实参

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

#### self 参数

函数可将 `self` 作为第一个参数，以实现类似方法的分派：

```almide
type User = { name: String }

fn User.greet(self) -> String = "Hi, ${self.name}"

test "self parameter dispatches via UFCS" {
  assert_eq(User { name: "alice" }.greet(), "Hi, alice")
}
```

<a id="hole-and-todo"></a>

#### Hole 与 Todo

```almide
fn not_yet(x: Int) -> String = _                     // hole: type-checked stub
fn later(x: Int) -> String = todo("implement later")  // todo with message
```

<a id="attributes"></a>

#### 属性

函数声明前可添加一个或多个 `@name` 或 `@name(args)` 属性。解析器接受通用形式：

```almide
@pure
@inline_rust("almide_rt_int_abs({n})")
@schedule(device=gpu, tile=32, unroll=true)
fn decorated(n: Int) -> Int = int.abs(n)
```

语法：

- `@name`——无参数
- `@name(arg, ...)`——位置参数、命名参数（`key=value`）或混合
- 参数值：`"string"`、`42`、`0xff`、`-1`、`true`、`false`，或裸标识符（视为符号标签，不是值引用）

当前具有语义的属性名：

- `@extern(target, "module", "function")`——目标运行时的 FFI 绑定。见 [module-system.md §11](module-system.zh-CN.md#11-extern)。
- `@export(c, "symbol")`——使用 C ABI 导出，与 `--repr-c` 输出配合（见 module-system §10）。
- `@export(wasm, "symbol")`——fn 成为名为 `symbol`（而非自身名称）的 `--target wasm` export，以及宿主可调用的 DCE 根。对链接进产物的所有模块生效：入口文件、兄弟 `import self.x` 模块、依赖包（#3281）。模块中未标注的 `pub fn` 不是 export，入口文件中的则是。产物只有一个 export 命名空间：任意模块中的两个 fn 声明同一符号，或声明的 export 无法 lower，都会拒绝构建（E082，指出 fn）。测试：`tests/wasm_export_attr_test.rs`、`tests/wasm_export_nonroot_test.rs`。
- `@inline_rust("template")`——由 stdlib 及以 Rust 实现 fn 的包编写（aes 与 sha1 包如此，模板背后有 `native/` 模块）。stdlib 之外它是**原生代码**（#4100）：与 `@extern` fn 一样计为所有效应类别，包含它的依赖需根的 `[permissions] native` 授权（[package-system.md §6.4](package-system.zh-CN.md)）。将被标注 fn 的 Rust 目标代码生成路由到内联模板，覆盖基于 TOML 的 `arg_transforms` 分派。`{param_name}` 占位符替换为对应调用实参渲染出的 Rust 表达式。fn 本体不输出为 Rust 函数，模板在每个调用点内联。用于 Stdlib Declarative Unification 工作中的 `stdlib/<module>.almd` 文件。测试：`spec/stdlib/int_bundled_inline_rust_test.almd`。

`@intrinsic("almide_rt_…")` 与 `@wasm_intrinsic` 在权限上**仅限 stdlib**，不只是惯例（#2152）。检查器只在 stdlib 自身源码中接受它们：随附的 stdlib 副本；或 `stdlib/` 下文本与同名内嵌源码完全一致的文件（#4100：看身份，不看布局，复制 `stdlib/` 布局的包不是 stdlib）；或调用者用 `--stdlib-authority <dir>` 指定目录下的任何源码。这是供构建修改版 stdlib 副本的 harness 使用的命令行标志，清单键和环境变量均不能授予权限。其他位置以 E085 拒绝（"intrinsics live in the stdlib; wrap in an effect fn"）。`prim` 模块（stdlib 所基于的原语底层）也受相同权限约束（#3025）：stdlib 外的 `prim.<fn>` 与 `import prim` 为 E085，提示指出包装该底层操作的公共函数。测试：`tests/diagnostics/e085-intrinsic-outside-stdlib/`、`tests/diagnostics/e085-prim-outside-stdlib/`。

`@pure` 会被检查（#3250）：fn 及其所有调用具有空效应集合——无效应类别、无输出、无 `panic` / `assert*`——否则构建在违规调用处以 E092 停止。见 [effect-system.md §2.3](effect-system.zh-CN.md)。

其他属性名（`@schedule`、`@rewrite`）可无错误解析并保留在 AST 中，但尚无语义行为。它们为 Stdlib Declarative Unification 和 MLIR Backend 的后续子阶段预留（见 `docs/roadmap/done/stdlib-declarative-unification.md` 与 `docs/roadmap/on-hold/mlir-backend-adoption.md`）。当前写在用户代码中是合法语法，但编译器会忽略。

测试：`crates/almide-syntax/src/parser/test_attributes.rs`（13 个解析测试）、`crates/almide-tools/src/fmt.rs::attr_tests`（6 个格式往返测试）。

测试： `spec/lang/function_test.almd`, `spec/lang/default_args_test.almd`, `spec/lang/named_args_test.almd`, `spec/lang/generics_test.almd`

<a id="42-effect-functions-effect-fn"></a>

### 4.2 效应函数（`effect fn`）

有副作用（IO、随机性等）的函数使用 `effect fn` 修饰符。用户效应函数暴露 Result 通道，用 `!` 显式传播；不会出错的 stdlib intrinsic 是文档注明的裸值例外。

```almide
import fs

effect fn read_config(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!
  ok(content)
}
```

`effect fn` 返回 `Result[T, E]`，且本体是未以显式 `ok(...)` 结束的块时，成功值的尾部会包装为 `ok(...)`（Unit 尾部变为 `ok(())`）：

```almide
effect fn log(msg: String) -> Result[Unit, String] = {
  println(msg)
  // ok(()) is inserted automatically
}
```

测试： `spec/lang/effect_fn_test.almd`

<a id="43-type-declarations-type"></a>

### 4.3 类型声明（`type`）

<a id="record-types"></a>

#### 记录类型

```almide
type User = { name: String, age: Int }
```

<a id="variant-types-leading-"></a>

#### variant 类型（前导 `|`）

```almide
type Color =
  | Red
  | Green
  | Blue
```

variant case 可以携带数据：

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }
```

variant case 的三种形式：
- **Unit**：`| CaseName`——无 payload
- **元组**：`| CaseName(Type, ...)`——位置字段
- **记录**：`| CaseName{ field: Type, ... }`——命名字段

<a id="inline-variant-no-leading-"></a>

#### 内联 variant（无前导 `|`）

```almide
type Outcome[T, E] = Good(T) | Bad(E)
```

当所有 case 都是不带 payload 的裸大写名称时，解析器将 `A | B | C` 视为 union/enum：

```almide
type Direction = North | South | East | West
```

<a id="type-alias"></a>

#### 类型别名

```almide
type Name = String
type Handler = (String) -> String
```

别名可以接受类型参数。应用实参时，它等于将本体中各参数替换为对应实参的结果，无论写在形参、返回类型、字段、variant payload 还是另一别名的本体中；从其他模块引用时须限定（`geo.Pair[Int]`）：

```almide
type Pair[T] = (T, T)            // Pair[Int] is (Int, Int)
type Step[T] = (T) -> T          // Step[String] is (String) -> String
type Both[T] = Pair[List[T]]     // Both[Int] is (List[Int], List[Int])

fn first(p: Pair[Int]) -> Int = p.0
```

实参按声明顺序替换参数。仅展开别名，泛型记录或 variant 仍是名义类型。实参数量必须等于声明的参数数量（E093），内置类型也一样：`List`、`Option`、`Set` 接受一个，`Map`、`Result` 接受两个，`Int` 等标量不接受参数。

声明顺序无关：别名可引用文件或模块后面声明的类型（`type First = Pair2` 位于 `type Pair2 = (Int,
Int)` 之前）。别名不能回到自身——如 `type A = B` 与 `type B = A`，或经类型实参的 `type Tree = List[Tree]`——因为那样不指向任何类型（E094）；记录或 variant 可以引用自身。

<a id="generic-types"></a>

#### 泛型类型

```almide
type Pair[A, B] = { first: A, second: B }
type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
```

<a id="conventions-deriving"></a>

#### 约定（派生）

```almide
type Color: Eq, Repr = Red | Green | Blue
type Point: Codec = { x: Float, y: Float }
```

内置约定包括 `Eq`、`Repr`、`Ord`、`Hash`、`Codec`、`Encode` 和 `Decode`。

`Codec` 派生 `T.encode(v: T) -> Value` 和 `T.decode(v: Value) ->
Result[T, String]`，并为记录和 variant 派生 `T.decode_strict(v: Value) -> Result[T, String]`。`Encode` / `Decode` 选择单一方向；见 [ALS-D6](als/data-formats.zh-CN.md)。严格解码递归拒绝未知键，包括嵌套容器，并报告路径。`T.decode` 忽略未知键；`T.decode_strict` 首先为文档顺序中第一个不属于被解码字段的键返回 `err("unknown key '<k>'")`，然后按 `T.decode` 的方式解码（C-429）。记录字段可带 `@skip`（既不解码也不编码）或 `@skip_decode`（不解码，仍编码）；两者都需要默认值，否则 E104；输入中此类字段的键视为未知键（C-428）。记录字段只接受 `@builtin`、`@location`、`@skip`、`@skip_decode` 属性，其他属性为 E053（C-430）。

```almide
type User: Codec = {
  name: String,
  @skip_decode is_admin: Bool = false,   // never read from input; still encoded
  @skip cache_hint: Int = 0,             // neither read nor written
}
```

测试： `spec/wasm_cross/codec_field_skip_matrix.almd`, `tests/codec_field_skip_test.rs`

测试： `spec/lang/data_types_test.almd`, `spec/lang/type_alias_test.almd`, `spec/lang/generic_type_alias_test.almd`, `spec/lang/forward_type_alias_test.almd`, `spec/lang/variant_record_test.almd`, `spec/lang/derive_conventions_test.almd`

<a id="44-protocol-declarations"></a>

### 4.4 协议声明

```almide
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
  effect fn load(a: Self) -> Result[Unit, String]
}
```

协议方法可以是 `effect fn`。

测试： `spec/lang/protocol_test.almd`, `spec/lang/protocol_advanced_test.almd`

<a id="45-convention-methods"></a>

### 4.5 Convention method

不存在 `impl` 块（已移除）。满足协议和为类型添加方法，都通过 convention method（`fn Type.method`）完成：

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

调用可写成 `GreetAction.name(a)`，也可用 UFCS 的 `a.name()`。

测试： `spec/lang/trait_impl_test.almd`, `spec/lang/protocol_test.almd`

<a id="46-top-level-let"></a>

### 4.6 顶层 `let`

模块作用域常量：

```almide
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```

在编译时（const）求值，非 const 表达式则通过 `LazyLock` 求值。

测试： `spec/lang/top_let_test.almd`

<a id="47-test-declarations"></a>

### 4.7 测试声明

```almide
test "addition works" {
  assert_eq(1 + 2, 3)
  assert(3 > 0)
  assert_ne(1, 2)
}
```

本体是花括号包围的块表达式。

测试：所有 `spec/lang/*_test.almd` 文件都包含 `test` 块。

---

<a id="5-expressions"></a>

## 5. 表达式

<a id="51-literals"></a>

### 5.1 字面量

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

测试： `spec/lang/expr_test.almd`

<a id="52-string-interpolation"></a>

### 5.2 字符串插值

```almide
test "interpolation" {
  let name = "alice"
  assert_eq("hello ${name}", "hello alice")
  assert_eq("result = ${1 + 2}", "result = 3")
  assert_eq("nested ${string.len(name)}", "nested 5")
}
```

`${}` 内按完整表达式解析。

测试： `spec/lang/string_interp_test.almd`, `spec/lang/interpolation_edge_test.almd`

<a id="53-heredoc-multi-line-strings"></a>

### 5.3 Heredoc（多行字符串）

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

按最小缩进移除前导空白。heredoc 内支持 `${expr}` 插值。

原始 heredoc（不处理转义）：`r"""..."""`

测试： `spec/lang/heredoc_test.almd`

<a id="54-list-literals"></a>

### 5.4 列表字面量

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

测试： `spec/lang/expr_test.almd`

<a id="55-map-literals"></a>

### 5.5 映射字面量

```almide
let m: Map[String, Int] = [:]       // empty map (requires type annotation)
let ab = ["a": 1, "b": 2]           // Map[String, Int]

test "map literals" {
  assert_eq(map.len(m), 0)
  assert_eq(ab["b"], some(2))
}
```

映射使用 `[key: value]` 语法；花括号 `{}` 用于记录/块，方括号 `[]` 用于列表和映射。

测试： `spec/lang/map_literal_test.almd`, `spec/lang/map_edge_test.almd`

<a id="56-record-literals"></a>

### 5.6 记录字面量

匿名记录：

```almide
let alice = { name: "alice", age: 30 }
```

命名记录（带类型构造）：

```almide
type User = { name: String, age: Int }

let alice = User { name: "alice", age: 30 }
```

每个字段都写成 `name: value`，没有字段简写（`{ name, age }` 是语法错误）：

```almide check-fail=syntax
let name = "alice"
let age = 30
let r = { name, age }
```

<a id="spread-records"></a>

#### 记录展开

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

测试： `spec/lang/record_spread_test.almd`, `spec/lang/data_types_test.almd`

<a id="57-tuple-expressions"></a>

### 5.7 元组表达式

```almide
let pair = (1, "hello")        // (Int, String)
let triple = (1, 2, 3)         // (Int, Int, Int)
```

按索引访问元组元素：

```almide
test "tuple index access" {
  let pair = (1, "hello")
  assert_eq(pair.0, 1)
  assert_eq(pair.1, "hello")
}
```

测试： `spec/lang/tuple_test.almd`

<a id="58-if-then-else"></a>

### 5.8 If-Then-Else

`if` 是表达式，要求 `then`。`else` 可选；没有 `else` 时结果为 `Unit`。

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

测试： `spec/lang/control_flow_test.almd`, `spec/lang/expr_test.almd`

<a id="59-match"></a>

### 5.9 Match

穷尽模式匹配：

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

match 分支支持 guard：

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

管道 match 语法：

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

### 7.4 列表模式的穷尽性

match 必须覆盖所有值（E010 指出缺失 case），已被先前分支覆盖的分支为 E014。列表模式按长度计算：`[]` 与 `[p1, p2]` 各覆盖一种长度，`[p1, ..]` 覆盖从一开始的所有长度。列表模式位于何处都一样，包括元组内部（#2600）：

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

移除 `(_, [])` 分支会产生 E010，指出未覆盖的 `([_], [])`。

测试： `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`,
`tests/diagnostics/e010-list-*`,
`tests/list_pattern_tuple_test.rs`

<a id="510-lambda"></a>

### 5.10 Lambda

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

多行 lambda 本体使用块：

```almide
test "block-bodied lambda" {
  let f = (x: Int) => {
    let y = x * 2
    y + 1
  }
  assert_eq(f(3), 7)
}
```

测试： `spec/lang/lambda_test.almd`

<a id="511-block-expressions"></a>

### 5.11 块表达式

块中最后一个表达式就是块的值：

```almide
let result = {
  let x = 1
  let y = 2
  x + y
}
// result = 3
```

测试： `spec/lang/expr_test.almd`, `spec/lang/scope_test.almd`

<a id="512-for-in-loop"></a>

### 5.12 For-In 循环

```almide
fn print_all(xs: List[Int]) -> Unit = {
  for x in xs {
    println(int.to_string(x))
  }
}
```

for 中的元组解构：

```almide
fn print_entries(m: Map[String, Int]) -> Unit = {
  for (k, v) in map.to_list(m) {
    println("${k} = ${v}")
  }
}
```

忽略变量使用下划线：

```almide
fn tick_five() -> Unit = {
  for _ in 0..<5 {
    println("tick")
  }
}
```

测试： `spec/lang/for_test.almd`, `spec/lang/for_tuple_test.almd`

<a id="513-while-loop"></a>

### 5.13 While 循环

```almide
fn count_to_ten() -> Unit = {
  var i = 0
  while i < 10 {
    println(int.to_string(i))
    i = i + 1
  }
}
```

循环内支持 `break` 和 `continue`：

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

测试： `spec/lang/while_test.almd`, `spec/lang/while_loop_test.almd`

<a id="514-range"></a>

### 5.14 范围

```almide
test "range forms" {
  assert_eq(0..<5, [0, 1, 2, 3, 4])     // exclusive end
  assert_eq(1...5, [1, 2, 3, 4, 5])     // inclusive end
}
```

范围可用于 for 循环（经过优化，不分配列表）：

```almide
fn count_to(n: Int) -> Unit = {
  for i in 0..<n {
    println(int.to_string(i))
  }
}
```

测试： `spec/lang/range_test.almd`

<a id="515-pipe-operator"></a>

### 5.15 管道运算符

```almide
test "pipe passes the left side as the FIRST argument" {
  let text = " a,b "
  assert_eq(text |> string.trim |> string.split(","), ["a", "b"])
  let xs = [1, -2, 3]
  assert_eq(xs |> list.filter((x) => x > 0), [1, 3])
}
```

管道运算符 `|>` 将左侧作为右侧的第一个实参传入。没有占位符：调用实参中的 `_` 是无值的 hole，会被拒绝（E046）。

```almide check-fail=E046
fn positives(xs: List[Int]) -> List[Int] = xs |> list.filter(_, (x) => x > 0)
```

测试： `spec/lang/pipe_test.almd`

<a id="516-compose-operator"></a>

### 5.16 组合运算符

```almide
test "compose" {
  let transform = string.trim >> string.to_upper
  assert_eq(transform("  hello  "), "HELLO")
}
```

`>>` 运算符从左向右组合两个函数。

测试： `spec/lang/compose_test.almd`

<a id="517-fan-blocks-concurrent-execution"></a>

### 5.17 Fan 块（并发执行）

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

Fan 块并发执行表达式。块内每个表达式并行运行，块返回结果元组。

Fan 块仅允许表达式，不允许 `let`、`var`、`for`、`while` 语句。

**记录形式。** 每个分支都是 `name: expr` 字段时，块是记录字面量，字段作为并行兄弟运行；值为匿名记录 `{ name: value, … }`，类型与普通字面量完全相同，按名称读取。

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

- 没有字段失败时，移除 `fan` 不改变可观察行为：字段输出按源码顺序出现，值等于普通字面量（C-386）。
- 写作 `e!` 的字段按元组形式规则使其 Err 成为块的 Err：所有字段都会运行，源码顺序中的第一个 Err 胜出（C-199、C-386）。
- 值为 `Result` 的字段（包括效应调用）必须写成 `e!`；未标记时为 E041。兄弟字段在块内不在作用域中。
- 每个分支都必须命名（`fan { a: f(), g() }` 是解析错误），名称只出现一次，没有字段简写：`fan { x }` 是单分支元组形式。

测试： `spec/lang/fan_record_test.almd`, `spec/wasm_cross/fan_record_serial_elision.almd`, `spec/wasm_cross/fan_record_err_source_order.almd`, `tests/diagnostics/e041-fan-record-field-without-bang/`

`fan.*` 块头共享以下语法形式：

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

分支由换行（或 `,`）分隔，不使用 `;`。`fan.race` 在 0.42.0 移除——确定性模型下它只是 `thunks[0]()` 的别名。替代它的确定性块头在 dialect epoch 14 改名为 `fan.min`，并要求时钟参数；同一 epoch 也移除 `fan.any`（`??` 链或 `list.find_map` 表达相同含义）。引用任何旧名称都会产生指出替代形式的墓碑错误 E027。

预算用 `compute.*` 时间构造器建立（封闭单位集合 `ns / us / ms / s / min / h`）；裸 `Int` 或墙钟 `duration.*` 值为类型错误。完整语义见 [docs/SPEC.md §13](../SPEC.zh-CN.md)，规范性时间规则见 [docs/adr/0001-deterministic-time-units.md](../../../../adr/0001-deterministic-time-units.md)。

测试： `spec/lang/fan_test.almd`, `spec/lang/fan_map_test.almd`, `spec/lang/fan_ext_test.almd`, `spec/wasm_cross/fan_*.almd`, `research/spike/charge-probe/fixtures/`

<a id="518-option-and-result-constructors"></a>

### 5.18 Option 与 Result 构造器

```almide
let present: Option[Int] = some(42)            // Option[Int] = some
let absent: Option[Int] = none                 // Option[T] = none

let success: Result[Int, String] = ok(42)      // Result[Int, E] = success
let failure: Result[Int, String] = err("failed")  // Result[T, String] = failure
```

测试： `spec/lang/error_test.almd`, `spec/lang/unwrap_operators_test.almd`

<a id="519-function-calls"></a>

### 5.19 函数调用

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

命名实参：

```almide
fn connect(host: String, port: Int = 80, secure: Bool = false) -> String =
  "${if secure then "https" else "http"}://${host}:${int.to_string(port)}"

test "named arguments" {
  assert_eq(connect("localhost", port: 3000, secure: true), "https://localhost:3000")
}
```

测试： `spec/lang/function_test.almd`, `spec/lang/named_args_test.almd`

<a id="520-member-access-and-index-access"></a>

### 5.20 成员访问与索引访问

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

测试： `spec/lang/expr_test.almd`, `spec/lang/tuple_test.almd`

<a id="521-ufcs-uniform-function-call-syntax"></a>

### 5.21 UFCS（统一函数调用语法）

UFCS 按固定查找顺序解析 `x.f(y)`，先于适用的用户函数考虑 stdlib 和 convention method。当该函数被选中时，可以等价于 `f(x, y)`，但同名本地函数不会覆盖 stdlib 方法。见 `check/calls_ufcs.rs` 与 `spec/lang/user_fn_namespace_collision_test.almd`。

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

## 6. 语句

语句出现在块和函数体内，由换行分隔（无需分号）。

<a id="61-let-binding-immutable"></a>

### 6.1 Let 绑定（不可变）

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

### 6.2 Var 绑定（可变）

```almide
test "var" {
  var count = 0
  count = count + 1                // reassignment (var only)
  assert_eq(count, 1)
}
```

<a id="63-destructuring"></a>

### 6.3 解构

记录解构：

```almide
test "record destructuring" {
  let user = { name: "alice", age: 30 }
  let { name, age } = user
  assert_eq(name, "alice")
  assert_eq(age, 30)
}
```

元组解构：

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

`var` 接受相同模式——元组（可嵌套、带 `_`）和记录简写——每个绑定名都是 `var`（#3149）。值只求值一次，`var (x, y) = p` 的行为与逐个写 `var x = p.0` / `var y = p.1` 完全相同：各名称独立重新赋值，写入一个既不影响其他名称，也不影响 `p`。

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

测试： `spec/lang/variable_test.almd`, `spec/lang/data_types_test.almd`,
`spec/lang/var_destructure_test.almd`

<a id="64-assignment"></a>

### 6.4 赋值

简单重新赋值（仅 var）：

```almide
test "reassignment" {
  var x = 1
  x = x + 1
  assert_eq(x, 2)
}
```

索引赋值（仅 var）：

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

字段赋值（仅 var）：

```almide
test "field assignment" {
  var user = { name: "alice" }
  user.name = "bob"
  assert_eq(user.name, "bob")
}
```

<a id="65-guard-early-return"></a>

### 6.5 Guard（提前返回）

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

使用块本体：

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

`guard` 检查条件；若为假，`else` 分支以兼容外围返回通道的值提前返回，或发散。并不限于返回 Result 的函数。

测试： `spec/lang/guard_test.almd`

<a id="66-expression-statements"></a>

### 6.6 表达式语句

任何表达式都可作为语句。块中最后一个表达式是块的值。

```almide
fn forty_three() -> Int = {
  println("side effect")           // expression statement (Unit)
  let x = 42
  x + 1                           // final expression = block value
}
```

---

<a id="7-pattern-matching"></a>

## 7. 模式匹配

模式出现在 `match` 分支、`let` 解构和 `for` 循环变量中。

<a id="71-pattern-forms"></a>

### 7.1 模式形式

| 模式 | 语法 | 说明 |
|---------|--------|-------------|
| 通配符 | `_` | 匹配任何值，不绑定 |
| 标识符 | `name` | 匹配任何值，绑定到 `name` |
| 字面量 | `42`, `3.14`, `"text"`, `true`, `false` | 匹配确切值 |
| 负字面量 | `-1`, `-3.14` | 匹配负数 |
| 构造器 | `TypeName(p1, p2)` | 匹配带元组 payload 的 variant |
| 记录 | `TypeName{ field1, field2 }` | 匹配带记录 payload 的 variant |
| 记录（嵌套） | `TypeName{ field: pattern }` | 用字段上的嵌套模式匹配 |
| 记录（剩余） | `TypeName{ field, .. }` | 匹配并忽略其他字段 |
| 元组 | `(p1, p2, p3)` | 匹配元组 |
| `some(p)` | `some(inner)` | 匹配 `Option` 的 some case |
| `none` | `none` | 匹配 `Option` 的 none case |
| `ok(p)` | `ok(inner)` | 匹配 `Result` 的 ok case |
| `err(p)` | `err(inner)` | 匹配 `Result` 的 err case |
| 列表 | `[p1, p2]` | 匹配元素数恰好如此的列表 |
| 列表（剩余） | `[p1, ..t]`, `[p1, ..]` | 匹配至少如此多个元素的列表；`t` 绑定尾部 |

<a id="72-examples"></a>

### 7.2 示例

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

### 7.3 嵌套模式

模式可以组合：

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

测试： `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`

---

<a id="8-operators"></a>

## 8. 运算符

<a id="81-precedence-highest-to-lowest"></a>

### 8.1 优先级（从高到低）

| 优先级 | 运算符 | 结合性 |
|------------|-----------|---------------|
| 1 | `.` `()` `[]` `!` `??` `?.` `?` | 后缀族。`??`  链右嵌套（`a ?? b ?? c` = `a ?? (b ?? c)`，含义为“首个成功胜出”） |
| 2 | `not` `-` （一元） | 右（前缀） |
| 3 | `^` | 右 |
| 4 | `*` `/` `%` | 左 |
| 5 | `+` `-` | 左 |
| 6 | `..<` `...` | 非结合 |
| 7 | `==` `!=` `<` `>` `<=` `>=` | 非结合 |
| 8 | `and` | 左 |
| 9 | `or` | 左 |
| 10 | `\|>` `>>` | 左 |

<a id="82-arithmetic-operators"></a>

### 8.2 算术运算符

| 运算符 | 说明 |
|----------|-------------|
| `+` | 加法（Int、Float）；拼接（String、List） |
| `-` | 减法 |
| `*` | 乘法 |
| `/` | 除法 |
| `%` | 取模 |
| `^` | 幂（右结合） |
| `-`（一元） | 取负 |

`+` 被重载：数值相加，字符串和列表拼接。

<a id="83-comparison-operators"></a>

### 8.3 比较运算符

| 运算符 | 说明 |
|----------|-------------|
| `==` | 相等（深度相等） |
| `!=` | 不等 |
| `<` | 小于 |
| `>` | 大于 |
| `<=` | 小于等于 |
| `>=` | 大于等于 |

比较是**非结合**的：`a < b < c` 这样的链式写法是编译错误。使用 `a < b and b < c`。

<a id="84-logical-operators"></a>

### 8.4 逻辑运算符

| 运算符 | 说明 |
|----------|-------------|
| `and` | 逻辑 AND（短路） |
| `or` | 逻辑 OR（短路） |
| `not` | 逻辑 NOT（前缀） |

Almide 使用单词而非符号：`&&` 与 `||` 会被拒绝并给出提示。

<a id="85-unwrap-operators"></a>

### 8.5 解包运算符

| 运算符 | 语法 | 说明 |
|----------|--------|-------------|
| `!` | `expr!` | 在当前 fn/lambda 通道中解包 Result/Option；测试解包并失败，而非传播。Result、Option、带类型错误与不会失败的上下文见[上下文表](../../../../specs/result-option-effect.md#2-演算子)。 |
| `??` | `expr ?? fallback` | 解包或使用回退值。fallback **惰性求值**（仅 none/err 时）。**以最高优先级结合**：`a ?? 1 + 2` = `(a ?? 1) + 2`，与 C 系 `?:`（近乎最低）相反，混用算术时建议加括号。fallback 放在同一行（跨行是 E038，多行用括号 + 后置 `??`）。 |
| `?` | `expr?` | 将 Result 转为 Option（err 变为 none） |
| `?.` | `expr?.field` | 可选链（Option[Record] 转为 Option[FieldType]）——**仅限 Option**。Result 以专用诊断拒绝（组合成 `(r?)?.x`） |

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

测试： `spec/lang/unwrap_operators_test.almd`, `spec/lang/operator_test.almd`

<a id="86-bitwise-operations"></a>

### 8.6 位运算

位运算是函数，不是运算符：

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

### 8.7 字符串拼接

```almide
test "+ concatenates" {
  assert_eq("hello" + " " + "world", "hello world")   // string concatenation with +
  assert_eq([1, 2] + [3, 4], [1, 2, 3, 4])            // list concatenation with +
}
```

测试： `spec/lang/string_test.almd`, `spec/lang/operator_test.almd`

---

<a id="9-visibility"></a>

## 9. 可见性

可见性修饰符位于顶层 `fn`、`type`、`let` 之前。

| 修饰符 | 范围 | Rust 对应 |
|----------|-------|-----------------|
| 无 | public——任何人可访问 | `pub` |
| `mod` | 仅同一项目 | `pub(crate)` |
| `local` | 仅此文件 | private |

```almide
fn public_fn() -> Int = 42              // public (default)
mod fn internal_fn() -> Int = 42        // project-internal
local fn private_fn() -> Int = 42       // file-private
```

修饰符顺序：`[local|mod]? effect? fn`

```almide
local effect fn helper() -> Result[Unit, String] = ok(())
mod fn utility(x: Int) -> Int = x * 2
```

可见性也适用于类型和顶层 let：

```almide
local type InternalState = { count: Int }
mod let CACHE_SIZE = 256
```

测试： `spec/lang/visibility_test.almd`

---

<a id="10-comments"></a>

## 10. 注释

<a id="101-line-comments"></a>

### 10.1 行注释

```almide
// This is a line comment
let x = 42  // inline comment
```

<a id="102-block-comments"></a>

### 10.2 块注释

块注释可以嵌套：

```almide
/* This is a block comment */

/*
  /* Nested block comments work */
  Still inside the outer comment
*/
```

词法分析器完全跳过块注释（不输出为 token）。

测试： `spec/lang/block_comment_raw_string_test.almd`

---

<a id="11-built-in-functions"></a>

## 11. 内置函数

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

字符串表示不是内置函数。除非类型声明 repr，否则每个值按结构打印：记录为 `Name { f: v }`，列表为 `[a, b]`，容器内字符串带引号。定义 `fn T.repr(x: T) -> String` 的类型在所有渲染位置通过它打印：插值最外层（`"${x}"`，ALS-R2），外层记录、列表、option、result、元组、映射或 variant 内部，以及 `assert_eq` / `assert_ne` 失败与 `almide test` 报告。`secret.Secret[A]` 是声明 repr 的 stdlib 类型，在每条路径打印 `Secret(***)`（`docs/stdlib/secret.md`）。

不存在裸 prelude `print`：输出一行用 `println`；不添加换行并显式刷新的输出用导入的 `io.print`。

测试： `spec/lang/prelude_test.almd`

---

<a id="12-entry-point"></a>

## 12. 入口

```almide
import env

effect fn main() -> Unit = {
  let args = env.args()!                   // argv without the program name
  let name = list.get(args, 0) ?? "world"
  println("Hello, ${name}!")
}
```

`main` 不接受参数。命令行参数通过 `env.args()` 访问（不含程序名的 argv）。`effect fn main()` 自动包装为返回 `Result<(), String>`，无需写 `ok(())` 或显式 `Result` 类型。

---

<a id="13-key-design-rules"></a>

## 13. 主要设计规则

- 换行就是语句分隔符（无需分号）
- 泛型用 `[]`，不用 `<>`
- `<` 与 `>` 始终是比较运算符
- 副作用用 `effect fn`
- 无异常，使用 `Result[T, E]`
- 无 null，使用 `Option[T]`
- 无继承，使用组合和协议
- 无宏、无运算符重载、无隐式转换
- 空列表为 `[]`，空映射为 `[:]`（带类型标注）
- `_` 用于 match 通配符、let 丢弃、for 丢弃及 lambda 通配参数
- 使用限定 stdlib 调用（`string.len(s)`）、适用的 UFCS（`s.len()`），或显式选择性导入；没有隐式非限定导入所有函数
- x 为 Int 的 `println(x)` 需要显式转换：`println(int.to_string(x))`

---

<a id="appendix-test-file-index"></a>

## 附录：测试文件索引

所有测试文件位于 `spec/lang/` 下：

| 文件 | 覆盖范围 |
|------|----------|
| `auto_derive_test.almd` | 自动派生 Eq/Hash |
| `bidirectional_type_test.almd` | 类型推断 |
| `block_comment_raw_string_test.almd` | 块注释、原始字符串 |
| `bytes_test.almd` | Bytes 类型 |
| `capture_clone_test.almd` | 闭包捕获语义 |
| `codec_*_test.almd` | Codec 约定（序列化） |
| `compose_test.almd` | `>>` 组合运算符 |
| `control_flow_test.almd` | If/else、match、循环 |
| `data_types_test.almd` | 记录、variant、元组、集合 |
| `default_args_test.almd` | 参数默认值 |
| `default_fields_test.almd` | 记录字段默认值 |
| `derive_conventions_test.almd` | 约定派生（Eq、Repr 等） |
| `edge_cases_test.almd` | 解析器/编译器边界情况 |
| `effect_fn_test.almd` | 效应函数 |
| `eq_protocol_test.almd` | Eq 协议 |
| `equality_test.almd` | 深度相等语义 |
| `error_test.almd` | Result/Option 处理 |
| `escape_analysis_test.almd` | 所有权分析 |
| `expr_test.almd` | 基本表达式 |
| `fan_*_test.almd` | Fan 块（并发） |
| `for_test.almd`, `for_tuple_test.almd` | For-in 循环 |
| `function_test.almd` | 函数声明 |
| `generics_test.almd` | 泛型类型与函数 |
| `guard_test.almd` | Guard 语句 |
| `hash_protocol_test.almd` | Hash 协议 |
| `heredoc_test.almd` | 多行字符串 |
| `trait_impl_test.almd` | Convention method 与协议 |
| `import_test.almd` | 导入声明 |
| `interpolation_edge_test.almd` | 字符串插值边界情况 |
| `lambda_test.almd` | Lambda 表达式 |
| `map_literal_test.almd`, `map_edge_test.almd` | 映射字面量 |
| `match_edge_test.almd` | Match 边界情况 |
| `matrix_test.almd` | Matrix 类型 |
| `named_args_test.almd` | 命名实参 |
| `open_record_test.almd` | 开放记录类型 |
| `operator_test.almd`, `operator_protocol_test.almd` | 运算符 |
| `panic_test.almd` | Panic 行为 |
| `pattern_test.almd` | 模式匹配 |
| `pipe_test.almd` | 管道运算符 |
| `prelude_test.almd` | 内置函数 |
| `protocol_*_test.almd` | 协议系统 |
| `range_test.almd` | 范围表达式 |
| `record_spread_test.almd` | 记录展开 |
| `scope_test.almd` | 变量作用域 |
| `string_interp_test.almd`, `string_test.almd` | 字符串 |
| `tco_test.almd` | 尾调用优化 |
| `top_let_test.almd` | 顶层 let 绑定 |
| `trait_impl_test.almd` | Trait 实现 |
| `tuple_test.almd` | 元组 |
| `type_alias_test.almd` | 类型别名 |
| `type_annotation_test.almd` | 类型标注 |
| `type_system_test.almd` | 类型系统 |
| `unwrap_operators_test.almd` | `!`、`??`、`?`、`?.` 运算符 |
| `value_utils_test.almd` | Value 工具 |
| `variable_test.almd` | 变量绑定 |
| `variant_*_test.almd` | Variant 类型 |
| `visibility_test.almd` | 可见性修饰符 |
| `while_test.almd`, `while_loop_test.almd` | While 循环 |
