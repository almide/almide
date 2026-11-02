<a id="almide-language-specification"></a>

# Almide 语言规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](SPEC.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](SPEC.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](SPEC.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md)的非规范读者译文。规范效力属于英文原文，而非本译文。本页是不自动同步的固定快照，未独立运行验证实现、示例、安全性或性能。

> 原文注意：§9.8 的 find_map 说明同时写有跳过 Err 和“后续元素不再执行”，而§13.2 说明不执行胜者之后的候选。可变参数的 E007 写法、旧 fan 形式、禁止别名的一般规则与允许的别名等上下文差异也按原文保留；未通过翻译合并成新规则。“生产用”等状态及计划属于原文时点的主张，不是当前保证。

**“不是为了自由书写，而是为了正确收敛的语言。”**

> **规范地位（2026-08-20 声明，#530 / A0-1）：** 本规范原文和 `docs/specs/` 具有规范性——**当实现与 ALS 不一致时，错误在实现。** 解释器是 ALS 的可执行配套实现（三方判定机制的第三个裁判），受规范约束，从不自身充当规范。冻结表面和破坏性变更政策见 [docs/STABILITY.md](../../../STABILITY.md)；已知文字矛盾是 #530 尚待消除的项目，每项都以引用对应 ALS 节的夹具完成关闭。

---

<a id="0-design-philosophy"></a>

## 0. 设计理念

<a id="core-thesis"></a>

### 核心论点

面向 LLM 的语言设计，其本质不是最大化表达能力，而是**最小化每个生成步骤的有效候选集合**。

<a id="four-pillars"></a>

### 四大支柱

| 原则 | 定义 |
|---|---|
| **可预测** | 每个生成步骤都能严格缩小“下一个有效语法、API 和语义”的范围 |
| **局部性** | 理解或修改某处所需的信息尽量放在附近 |
| **可修复** | 出错时，编译器在少量步骤内返回近乎唯一的修复候选 |
| **紧凑** | 语义密度高、语法噪声低，严格而简洁 |

<a id="7-design-principles"></a>

### 7项设计原则

1. **规范唯一性**——原则上，相同含义应只有一种主要表达方式
2. **表面语义**——副作用、可失败性、可选性和可变性必须体现在语法或类型中
3. **局部推理**——函数或表达式的含义应大体能仅凭附近语法理解
4. **逐步完成**——未完成代码合法，可通过填充有类型的空洞推进
5. **修复优先**——编译器应是修复工具而非拒绝工具；诊断结构化
6. **词汇经济性**——标准库词汇一致，没有同义词
7. **无魔法**——禁止在运行时改变含义的机制、依赖上下文的 DSL 和隐式类型转换

<a id="trade-offs-intentionally-sacrificed"></a>

### 取舍（有意放弃）

- 专家的书写自由
- 文化性的“语言手感”和符合人体工学的 DSL
- 元编程能力（无宏、无反射）
- 自定义运算符语法和隐式转换（现有运算符可通过声明的协议分派）
- 多种返回风格和 lambda 表示法
- 通过隐式实例解析实现的特设多态

**目标：高度简洁 + 低自由度**

---

<a id="1-lexical-specification"></a>

## 1. 词法规范

<a id="11-identifiers"></a>

### 1.1 标识符

```
Identifier ::= (XID_Start | "_") XID_Continue*
```

标识符遵循 Unicode UAX #31，并以 NFC 存储。兼容规范化形式与规范形式不同的字符（例如全角拉丁字母和数字）会被拒绝，而非折叠转换。单独的 `_` 是占位符，不是绑定名称。关键字和模块名保持 ASCII。

测试：`spec/lang/unicode_identifier_test.almd`、`tests/unicode_identifier_nfc_test.rs`。

标识符末尾的 `?` 已移除——谓词是返回 `Bool` 的普通标识符（`is_` 前缀约定：`is_empty`、`is_digit`）。

<a id="12-type-names"></a>

### 1.2 类型名

类型名使用相同的 Unicode 标识符字符。首字母大写表示类型侧名称。无大小写的名称（例如 CJK、假名或韩文）属于值侧，除非其模块将它声明为类型或 variant 分支；从其他模块使用时应采用限定写法（如 `geo.住所`）。

测试：`spec/lang/unicode_identifier_test.almd`、`spec/wasm_cross/unicode_identifiers.almd`。

<a id="13-literals"></a>

### 1.3 字面量

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

**没有** null 字面量。缺失用 `none`（`Option[T]` 的构造器）表示。

双引号字符串支持 `${expr}` 插值和反斜杠转义。单引号字符串支持转义序列（`\'`、`\\`、`\n`、`\t`、`\r`），但**不支持插值**。原始字符串和原始 heredoc 既不支持插值也不支持转义。

Heredoc 字符串根据非空行的最小缩进去掉前导空白。首行（`"""` 后为空时）和末行（`"""` 前仅含空白时）会被去掉。

数值字面量支持 `_` 作为视觉分隔符（如 `1_000_000`）。

<a id="14-comments"></a>

### 1.4 注释

```
// line comment (to end of line)
/* block comment (nestable) */
```

块注释可以嵌套：`/* outer /* inner */ still outer */` 有效。

<a id="15-keywords-34"></a>

### 1.5 关键字（34）

```
module  import  type    protocol for     in      fn      let
var     mut     if      then     else    match   ok      err
some    none    todo    true     false   not     and     or
pub     effect  test    guard    break   continue while
local   mod     fan
```

`strict` 已移除——`strict types` / `strict effects` 声明没有启用任何检查，因此该词重新成为普通标识符。

<a id="16-operators-and-delimiters"></a>

### 1.6 运算符和分隔符

```
Operators:   +  -  *  /  %  ^  ==  !=  <  <=  >  >=  |>  >>  ..<  ...
Postfix:     !  ?  ?.  ??
Unary:       -  not
Logical:     and  or
Assignment:  =
Arrows:      ->  =>
Delimiters:  (  )  {  }  [  ]  ,  .  :  ;  |  _  @  ...
```

- `^` 是幂运算（右结合），`**` 可作为别名
- `+` 被重载：数值加法，字符串和列表拼接
- `..<` 是不含末端的范围，`...` 是含末端的范围（Rust 风格的 `..`/`..=` 会产生 E031 诊断及机械 fix-it）
- `...` 是展开（用于记录）
- `_` 丢弃模式绑定，或表示表达式中的有类型空洞；不是管道／调用参数占位符（E046，§9.10）
- `@` 引入属性，包括 extern 注解

---

<a id="2-statement-separators"></a>

## 2. 语句分隔

**换行分隔语句。** 分号仅用于在同一行放置多条语句。

```
let x = 1
let y = 2
let a = 1; let b = 2
```

<a id="21-line-continuation-rules"></a>

### 2.1 续行规则

以下情况忽略换行并继续语句：

**行末为：**
- 二元运算符：`+`、`-`、`*`、`/`、`%`、`==`、`!=`、`<=`、`>=`、`<`、`>`、`and`、`or`、`|>`
- 分隔符：`,`、`.`、`:`
- 左括号：`(`、`{`、`[`
- 箭头：`->`、`=>`
- 赋值：`=`
- 关键字：`if`、`then`、`else`、`match`、`not`、`|`

**下一行开头为：**
- 后接名称的 `.`（方法链）
- 任意中缀运算符：`|>`、`>>`、`+`、`-`、`*`、`/`、`%`、`^`、`==`、`!=`、`<`、`<=`、`>`、`>=`、`..<`、`and`、`or`
- `??`（其回退表达式留在 `??` 所在行——E038）

行首紧贴操作数的 `-`（`-1`）开始新语句，行首 `...` 是展开，因此两者都不续行。

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

## 3. 语法类别

这是概览，不是第二份详尽文法。具体语法和示例见专题[语言规范](../../../specs/language.md)。

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

## 4. 模块和导入

包身份在 `almide.toml` 中声明。源码文件中没有 `module` 声明。

<a id="41-import-declaration"></a>

### 4.1 导入声明

```
ImportDecl ::= "import" ModulePath
             | "import" ModulePath "as" Ident
             | "import" ModulePath "." "{" NameList "}"

ModulePath ::= Ident ( "." Ident )*
```

示例：
```
import fs                       // stdlib module
import json                     // stdlib module (not auto-imported)
import mylib                    // user package
import mylib.parser             // sub-module import
import mylib as m               // alias: m.hello()
import self as app              // self-alias for the current package
import mylib.{Parser, Lexer}    // selective import
```

**禁止通配符导入。** `import fs.*` 是编译错误。

<a id="42-auto-imported-modules-prelude"></a>

### 4.2 自动导入模块（Prelude）

以下公开模块无需 `import` 即可使用：

```
string  list  int  float  bool  math  map  set  deque  heap
result  option  value  datetime  bytes  matrix
int8  int16  int32  int64  uint8  uint16  uint32  uint64  float32  float64
```

其他公开 stdlib 模块要求显式 `import`；当前清单见[标准库参考](../../../stdlib/README.md)。权威依据是 [`stdlib_info.rs`](../../../../crates/almide-types/src/stdlib_info.rs) 中的 `TIER1_ALWAYS_ACCESSIBLE` 和 `AUTO_IMPORT_BUNDLED`。`prim` 是 stdlib 内部实现模块，不是用户表面：用户源码引用它会以 E085 拒绝。已退役的 `error`、`log` 和 `mem` 模块不属于公开 stdlib。[compute](../../../stdlib/compute.md) 和 [duration](../../../stdlib/duration.md) 时钟表面是自动可用的检查器内置项，不是可导入模块。

<a id="43-prelude-types"></a>

### 4.3 Prelude 类型

隐式导入的类型和构造器：
- 基本类型包括 `Int`、`Float`、其定宽数值系列、`Bool`、`String`、`Unit`、`Bytes`；`Path` 是 `String` 的别名
- 集合：`List[T]`、`Map[K, V]`、`Set[T]`
- 错误处理：`Option[T]`、`Result[T, E]`
- 构造器：`some(x)`、`none`、`ok(x)`、`err(x)`
- 布尔值：`true`、`false`
- 边界：`Value`（`json.parse` 返回 `Value`，不是独立内置的 `Json` 类型）

<a id="44-visibility-modifiers"></a>

### 4.4 可见性修饰符

```
fn pub_fn() -> String = ...       // public (default)
mod fn internal() -> String = ... // same project only (pub(crate) in Rust)
local fn helper() -> String = ... // same file only (private)
```

可见性适用于 `fn`、`type` 和 `let` 声明：
```
local type Internal = { data: String }
mod let THRESHOLD = 100
```

---

<a id="5-type-declarations"></a>

## 5. 类型声明

<a id="51-generics-----notation"></a>

### 5.1 泛型——`[]` 表示法

**类型参数使用 `[]`。** `<>` 仅保留给比较运算符。

```
GenericParams ::= "[" TypeParam ( "," TypeParam )* "]"
TypeParam     ::= TypeName
```

```
Result[List[Map[String, Int]], Error]
fn transform[T, U](xs: List[T], f: Fn(T) -> U) -> List[U] = list.map(xs, f)
```

<a id="52-record-types"></a>

### 5.2 记录类型

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

### 5.3 Variant 类型

variant 分支有三种形式：unit、元组式和记录式。

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

内联 variant（无前导 `|`）：

```
type Direction = North | South | East | West
```

<a id="54-conventions"></a>

### 5.4 约定

```
ConventionClause ::= ":" TypeName ("," TypeName)*
```

约定名称在类型名之后、`=` 之前用 `:` 指定：

```
type Color: Eq, Repr =
  | Red | Green | Blue
```

内置协议包括 `Eq`、`Repr`、`Ord`、`Hash`、`Codec`、`Encode`、`Decode` 和 `Numeric`。其要求和派生规则见[类型系统参考](../../../specs/type-system.md)；声明协议不能替代这些要求。

<a id="55-protocols"></a>

### 5.5 协议

```
ProtocolDecl ::= "protocol" TypeName "{" ProtocolMethod* "}"
ProtocolMethod ::= ["effect"] "fn" Ident "(" Params ")" "->" TypeExpr
```

协议定义所需方法的集合。类型通过 `: ProtocolName` 声明满足协议；方法自身是名为 `Type.method` 的普通顶层函数，第一个参数显式标为 `Type`——没有 `impl Protocol for Type { ... }` 块。约定方法保持扁平的顶层函数形式：无嵌套、无额外缩进层级，只有一种将方法附加到类型的方式。

```
protocol Serializable {
  fn serialize(a: Self) -> String
  fn deserialize(raw: String) -> Result[Self, String]
}

type Config: Serializable = { key: String, value: String }
fn Config.serialize(c: Config) -> String = c.key + "=" + c.value
fn Config.deserialize(raw: String) -> Result[Config, String] = ok(Config { key: raw, value: raw })
```

检查器根据协议签名验证每个已声明约定方法（参数数量及参数／返回类型，`Self` 替换为声明类型），不匹配时把诊断定位到方法。带不匹配方法的 `Type: Protocol` 注解会在 `almide
check` 时被拒绝，不会拖到原生代码生成时才失败。目前跳过泛型类型的签名检查（`Self` 需要携带该类型自己的类型参数，但尚无机制传递它们）。

首参数可以显式命名并标注类型（`a: Config`），也可写作单独的 `self`。后者是 `self: Self` 的语法糖，在约定方法中解析为声明类型，与 `protocol { ... }` 声明内部一样。

`Self` 是实现类型的占位符。内置约定是协议；结构派生和显式方法遵循[类型系统参考](../../../specs/type-system.md)。

协议可用作泛型约束：

```
fn show[T: Serializable](item: T) -> String = item.serialize()
```

没有动态分派——所有带协议约束的泛型都在编译时单态化。

<a id="56-type-alias"></a>

### 5.6 类型别名

```
type Score = Int
type Handler = (String) -> String
```

- 透明别名——`Score` 和 `Int` 可互换
- 函数类型别名使用 `(Params) -> Return` 语法

<a id="57-type-application"></a>

### 5.7 类型应用

```
List[String]
Result[User, ParseError]
Map[String, List[Int]]
```

<a id="58-tuple-types"></a>

### 5.8 元组类型

```
(Int, String)              // tuple type
(Int, String, Bool)        // 3-tuple
```

通过 `.0`、`.1` 等访问。

<a id="59-function-types"></a>

### 5.9 函数类型

```
Fn(Int) -> String
Fn(Int, Int) -> Bool
```

仅用括号的形式（见5.6）等价，也能在所有位置使用：`fn apply(f: (Int) -> Int) -> Int = f(3)`、`type Handler = (String) -> String`。

---

<a id="6-basic-type-environment"></a>

## 6. 基本类型环境

<a id="primitives"></a>

### 基本类型

```
Int  Float  Bool  String  Path  Unit
```

`Bytes` 是独立类型，不能与 `List[Int]` 互换；转换使用 [bytes API](../../../stdlib/bytes.md)。`Path` 是 `String` 的别名。定宽整数和浮点类型见[类型系统参考](../../../specs/type-system.md)。

<a id="collections"></a>

### 集合

```
List[T]  Map[K, V]  Set[T]
```

<a id="error-handling"></a>

### 错误处理

```
Option[T]   -- some(x) | none
Result[T, E] -- ok(x) | err(e)
```

<a id="boundary-types"></a>

### 边界类型

```
Value
```

用于接收外部输入。在领域逻辑中使用前要求显式转换。

---

<a id="7-function-declarations"></a>

## 7. 函数声明

```
FnDecl ::= Visibility? "effect"? "fn" Name GenericParams?
            "(" ParamList? ")" "->" TypeExpr "=" Expr

Visibility ::= "local" | "mod"        // default is public
ParamList  ::= Param ( "," Param )*
Param      ::= "mut"? Identifier ":" TypeExpr ( "=" Expr )?   // default value optional
```

修饰符顺序：`[local|mod]? effect? fn`

<a id="71-principles"></a>

### 7.1 原则

- 参数类型必需
- 返回类型必需
- 函数体是单个表达式（位于 `=` 后）
- `effect fn` 标记有副作用的函数
- `fan { }` 用于有作用域执行（仅限 `effect fn` 内）；调度取决于编译器和选择的宿主路径（§13）

```
fn add(x: Int, y: Int) -> Int = x + y

effect fn greet(name: String) -> Result[Unit, String] = {
  println("Hello, ${name}!")
  ok(())
}
```

<a id="72-default-arguments"></a>

### 7.2 默认参数

参数可以有默认值。第一个有默认值的参数之后，所有参数也必须有默认值。

```
fn connect(host: String, port: Int = 8080, secure: Bool = false) -> Connection = _
```

<a id="73-named-arguments"></a>

### 7.3 命名参数

调用处可给参数命名：

```
connect("localhost")
connect("localhost", port: 443, secure: true)
connect(host: "localhost", secure: true)
```

允许在位置参数后使用命名参数。不允许在命名参数后使用位置参数。

全命名参数调用**记录类型**（或带记录载荷的 variant 分支）就是记录构造：`Cfg(name: "x")` 是别名，规范化为标准的 `Cfg { name: "x" }`，并接受相同验证（未知／重复／无默认值而缺失的字段为 E021）。记录上的位置参数和元组构造器上的命名参数会被拒绝（E021）。

括号别名也可在模式中匹配普通记录类型（`Cfg(name) =>
...`）。它**不**匹配 variant 的记录载荷分支——那里仅允许花括号形式（`Circle { radius } => ...`，绝非 `Circle(radius) => ...`，E021: "use a record pattern"）。

<a id="74-predicate-functions"></a>

### 7.4 谓词函数

谓词是返回 `Bool` 的普通标识符（无 `?` 后缀，已移除）：

```
fn empty(xs: List[Int]) -> Bool = list.len(xs) == 0
fn tracked(index: Index, path: Path) -> Bool = _
```

<a id="75-effect-fn----explicit-side-effects"></a>

### 7.5 `effect fn`——显式副作用

从非 `effect` 函数调用 `effect fn` 是**编译错误**，不是警告。

```
fn pure_fn() -> String =
  fs.read_text("file.txt")    // Compile error: cannot call effect fn from non-effect fn
```

effect 位不声称数学纯性或能力类别为零。输出／中止内置项可用于普通 `fn`（§16.4），通过透明函数参数的调用遵循[效应系统参考](../../../specs/effect-system.md)中的回调规则。

<a id="76-top-level-let----module-scope-constants"></a>

### 7.6 顶层 let——模块作用域常量

```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello, world"
```

尽可能在编译时求值。数值和简单表达式成为 `const`；String 和复杂表达式在 Rust 代码生成中使用 `LazyLock<T>`。

<a id="77-extern-annotations"></a>

### 7.7 Extern 注解

用于绑定目标特定函数的 FFI：

```
@extern(rust, "std::fs", "read_to_string")
effect fn read_text(path: String) -> Result[String, String] = _
```

此例是原生 Rust 绑定，不是可移植实现。旧 TypeScript 后端已经退役；外部／宿主绑定须满足所选目标及其权限／信任边界。

<a id="78-mutable-parameters"></a>

### 7.8 可变参数

参数上的 `mut` 以可变引用传递：函数内部对参数的赋值在调用返回后原地对调用方可见，而不是用返回值携带新状态。

```
fn incr(mut x: Int) -> Unit = { x = x + 1 }

fn main() -> Unit = {
  var n = 5
  incr(n)
  println(int.to_string(n))   // 6
}
```

调用处必须传入 `var` 绑定——`let` 绑定或临时表达式会被拒绝（E007），因为没有可写回修改的位置。`mut` 可出现在任何参数上，不仅是第一个。原地 stdlib 操作（`list.push`、`list.pop`、`list.clear` 等）就这样编写——不是隐藏的接收者约定，而是标记为 `mut` 的普通参数。

---

<a id="8-statements"></a>

## 8. 语句

<a id="81-let--var"></a>

### 8.1 let / var

```
let x = 1                   // immutable
let x: Int = 1              // with type annotation
var y = 2                   // mutable
y = y + 1                   // reassign (var only)
```

<a id="82-destructuring"></a>

### 8.2 解构

```
let { name, age } = user    // record destructure (one level only)
var (x, y) = pair           // var takes the let patterns; x and y are both vars
```

- `var` 接受与 `let` 相同的模式，每个绑定名称都可变（#3149）
- 元组解构可以嵌套和丢弃位置：`var (x, (y, _)) = value`
- 记录简写解构不是通用的嵌套 match 模式
- 不允许重命名

<a id="83-guard"></a>

### 8.3 guard

```
guard cond else expr
```

带提前退出的前置条件检查。条件为假时，返回 else 表达式。

```
effect fn validate(x: Int) -> Result[Int, String] = {
  guard x > 0 else err("must be positive")
  guard x < 1000 else err("too large")
  ok(x * 2)
}
```

<a id="84-reassignment"></a>

### 8.4 重新赋值

仅允许对用 `var` 绑定的标识符赋值：

```
var count = 0
count = count + 1
xs[0] = "updated"           // index write (var only)
m["key"] = value             // map write (var only)
```

---

<a id="9-expressions"></a>

## 9. 表达式

<a id="91-if-expression"></a>

### 9.1 if 表达式

```
if cond then expr else expr
```

- 省略 `else` 得到值为 Unit 的条件式，不是产生值的双分支表达式
- 条件必须是 `Bool`，没有 truthiness
- 串联：`if a then x else if b then y else z`

<a id="92-match-expression"></a>

### 9.2 match 表达式

```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

**match 必须穷尽**（由类型检查器强制）。

守卫在 `if` 后接 `Bool` 表达式。守卫为假时尝试下一个分支。

<a id="93-patterns"></a>

### 9.3 模式

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

语言还支持列表／尾部、or 和 as 模式。其绑定和穷尽性规则见[语言参考](../../../specs/language.md)；上述形式并非完整模式清单。

记录模式支持用 `..` 部分匹配：

```
match shape {
  Circle(r) => 3.14 * r * r,
  Rect{ width, height } => width * height,
  Rect{ width, .. } => width,
  Point => 0.0,
}
```

<a id="94-lambdas"></a>

### 9.4 Lambda

```
(x) => expr
(x, y) => expr
(x: Int) => x + 1
items.map((x) => x * 2)
```

函数体可以是任意表达式，包括用于多行 lambda 的块表达式：

```
let f = (x) => {
  let y = x * 2
  y + 1
}
```

只有一种形式。Lambda 参数可选择添加类型注解。

<a id="95-block-expression"></a>

### 9.5 块表达式

块中最后一个表达式就是它的值：

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

遍历列表。循环体类型为 `Unit`。集合遍历使用 `for...in`。

<a id="97-while"></a>

### 9.7 while

```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

条件为真时循环。循环体类型为 `Unit`。`break` 和 `continue` 提供循环内部的提前退出和下一次迭代控制，不从外围函数返回值。

<a id="98-fan-deterministic-data-parallelism"></a>

### 9.8 fan（确定性数据并行）

```
fan { expr1, expr2, expr3 }
```

汇合有作用域的分支表达式，并以元组返回其结果。仅在 `effect fn` 内有效。分支用逗号或换行分隔，不用分号；作为分支的块表达式可包含普通语句。

`fan` 是**调度**构造，而非语义构造：执行可以并行，但程序的可观察行为被定义为与按列表顺序依次求值完全一致。见[并发立场](https://github.com/almide/almide/blob/develop/docs/roadmap/active/concurrency-stance.md)。

规则：
- 汇合所有兄弟分支，不取消。若任何分支返回 `Err`，块值是**列表顺序中的第一个** `Err`（不是实时时钟上最早失败的），因此结果与调度无关。某兄弟分支不终止会使程序不终止，和顺序求值一样
- `fan` 块内没有 `let`/`var`/`for`/`while`，只有表达式
- 不捕获外层作用域的 `var`（防止数据竞争）

库形式：
- `fan.map(xs, f)`——通过 fan 表面对集合做 map（每个元素都执行，结果按列表顺序）
- `fan.settle(xs, f)`——每个元素的 Result，按列表顺序
- `fan.min(clock, xs, f)`——由时钟选择一个元素（§13.5）

按列表顺序第一个结果为 `Ok` 的元素**不是** fan 形式：它是顺序性的，因此写作 `list.find_map(xs, (x) => f(x)?) ?? fallback`（`?` 将元素的 `Err` 变为 `none`，该元素被跳过，后续元素不再执行）。块形式为 `??` 链：`a() ?? b() ?? fallback`。

（thunk 列表调用形式 `fan.race(thunks)` / `fan.any(thunks)` / `fan.settle(thunks)` 已是检查时带迁移提示的 `E027` 退役项；`fan.race { }`（在方言纪元14更名为 `fan.min`）和 `fan.any { }` / `fan.any(xs, f)`（同一纪元移除；`??` 链和 `list.find_map` 表达同一意思）亦如此。在列表顺序语义下，旧 `fan.race(thunks)` 恰好就是 `thunks[0]()`。）

<a id="910-pipe"></a>

### 9.10 管道

```
text |> string.trim |> string.split(",")
```

`x |> f` 等价于 `f(x)`。

对于多参数函数，管道值作为**第一个**参数：

```
xs |> list.filter((x) => x > 0)
text |> string.split(",")
```

没有占位符：调用参数中的 `_` 会被拒绝（E046），`text |> string.split(_, ",")` 会指出修复方式——去掉 `_`。要传到其他位置，请管道传给 lambda：`x |> (v) => f(a, v)`。

**管道传入 match：**

```
value |> match {
  some(x) => x,
  none => "default",
}
```

<a id="911-ufcs-uniform-function-call-syntax"></a>

### 9.11 UFCS（统一函数调用语法）

UFCS 解析选中 `f` 时，`x.f(y)` 将 `x` 作为其第一个参数。这不是解析阶段与作用域内任意 `f` 无条件等价。

```
string.trim(text)       // canonical form
text.trim()             // UFCS form -- equivalent

string.split(text, ",")
text.split(",")         // equivalent
```

解析取决于接收者类型和词法作用域：可调用记录字段、内置模块函数、约定／协议方法，以及用户／导入函数各有不同解析路径。局部绑定可遮蔽模块名。见 `crates/almide-frontend/src/check/calls_ufcs.rs`。

<a id="912-range"></a>

### 9.12 范围

```
0..<5           // [0, 1, 2, 3, 4]    exclusive end
1...5           // [1, 2, 3, 4, 5]    inclusive end
for i in 0..<n { ... }   // no list allocation (optimized)
```

<a id="913-record-and-spread"></a>

### 9.13 记录和展开

```
let alice = { name: "alice", age: 30 }
let bob = { ...alice, name: "bob" }       // age inherited from alice
{ name }                                   // shorthand: { name: name }
```

<a id="914-list"></a>

### 9.14 列表

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

### 9.16 元组

```
(1, "hello")              // tuple literal
let pair = (1, "hello")
pair.0                    // index access: 1
pair.1                    // "hello"
```

<a id="917-string-interpolation"></a>

### 9.17 字符串插值

```
let name = "world"
let msg = "hello ${name}, 1+1=${1 + 1}"
```

适用于双引号字符串和非原始 heredoc。单引号字符串、原始字符串和原始 heredoc 不做插值（§1.3）。

<a id="918-hole--todo"></a>

### 9.18 空洞 / Todo

```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```

`_`（空洞）和 `todo(msg)` 接受任意期望类型。编译器报告期望类型、可用变量和建议。

---

<a id="10-operators"></a>

## 10. 运算符

<a id="precedence-lowest-to-highest"></a>

### 优先级（从低到高）

| 级别 | 运算符 | 结合性 |
|-------|-----------|---------------|
| 1 | `or` | 左 |
| 2 | `and` | 左 |
| 3 | `==` `!=` `<` `<=` `>` `>=` | 不结合（链式使用是错误） |
| 4 | `\|>`（管道） | 左，不对称（见下文） |
| 5 | `..<` `...`（范围） | 无 |
| 6 | `+` `-`（加减） | 左 |
| 7 | `*` `/` `%`（乘除） | 左 |
| 8 | `^`（幂；`**` 是别名） | 右 |
| 9 | `>>`（组合） | 左（结合最紧的二元运算） |
| 10 | `-` `not`（一元） | 前缀 |
| 11 | `.` `()` `[]` `!` `?` `?.` `??`（后缀） | 左 |

- **`\|>` 不对称**：其右侧是单个调用／组合链。其后的其他运算符作用于管道结果——`xs \|> list.map(f) + ys` 是 `(xs \|> list.map(f)) + ys`，而 `xs \|> f >> g` 管道传入组合 `f >> g`
- 赋值（`=`）是语句，不是运算符
- 运算符语法固定。现有运算符可通过声明的协议要求（例如 `Eq` 和 `Numeric`）分派；不支持任意新运算符符号
- `&&` 和 `||` 被拒绝，并提示改用 `and`/`or`；`++` 被拒绝并提示使用 `+`

<a id="operator-semantics"></a>

### 运算符语义

| 运算符 | 含义 |
|----------|---------|
| `+` | 加法（Int、Float）或拼接（String、List） |
| `-` `*` `/` `%` | 算术 |
| `^`（`**`） | 幂（按位 XOR 为 `int.bxor`） |
| `==` `!=` | 深度相等（除 Fn 外的所有类型） |
| `<` `<=` `>` `>=` | 比较 |
| `and` `or` `not` | 布尔逻辑 |
| `\|>` | 管道 |
| `>>` | 函数组合 |
| `..<` `...` | 范围（不含／包含末端） |
| `!` `?` `?.` `??`（后缀） | 解包／转 Option／可选链／回退 |

相等性由类型引导。Secret／约定相等性可在目标代码生成前降级为函数调用；普通 Rust 相等性使用 `almide_eq!`。Wasm 按值类型实现相等性，包括对所支持复合值的结构比较，而不是通用原始内存字节相等。见前端的 `lower/expressions_access.rs::lower_eq_protocol` 及 `crates/almide-wasm/src/equality*.rs`。

---

<a id="11-error-model"></a>

## 11. 错误模型

<a id="111-three-layer-strategy"></a>

### 11.1 三层策略

| 层 | 机制 | 用途 |
|---|---|---|
| **正常失败** | `Result[T, E]` | 解析、验证、I/O、查找 |
| **程序员错误** | `panic` | 不可达、不变量违规 |
| **测试** | `assert_eq`、`assert` | 测试断言 |

**不存在**异常。没有 `throw`/`catch`。

<a id="112-unwrap-operators"></a>

### 11.2 解包运算符

用于显式处理 `Result[T, E]` 和 `Option[T]` 错误的三个后缀运算符：

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

`!` 也可用于纯的可失败函数和 lambda。其行为取决于返回上下文，包括测试块的例外规则；它不等同于在每个 effect 调用处自动传播。精确规则见 [Result/Option 上下文表](../../../specs/result-option-effect.md)。

<a id="113-typed-error-variants"></a>

### 11.3 有类型的错误 variant

要按错误种类分支，请用 variant 类型作为错误参数：

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

## 12. 空洞与未完成代码

**语言的核心功能。** 支持逐步开发。

```
fn parse(text: String) -> Ast = _
fn optimize(ast: Ast) -> Ast = todo("implement constant folding")
```

<a id="compiler-obligation"></a>

### 编译器义务

发现空洞时，编译器返回：
- 期望类型 `T`
- 作用域内可用的变量及类型
- 能返回期望类型的函数候选
- 建议表达式

---

<a id="13-concurrency-fan"></a>

## 13. 并发：`fan`

<a id="131-fan-block"></a>

### 13.1 fan 块

`fan { }` 将表达式作为一个有作用域单元运行。原生可使用有作用域线程；受支持的嵌入式 Wasm 路径可使用实例 worker。有序分支和不支持的并行形态采用顺序执行。可观察契约是分支顺序求值，不保证每个分支都并行或每个 Wasm 宿主拥有相同调度器。仅在 `effect fn` 内有效。

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

结果以元组返回。所有兄弟分支都会汇合，没有取消。若任意表达式返回 `Err`，块值为列表顺序中的第一个 `Err`（最小索引，无论哪个分支在时间上先失败）。

<a id="132-fanmap-and-the-block-head-fansettle"></a>

### 13.2 fan.map 与块头 fan.settle

```
let results = fan.map(urls, (url) => fetch(url))    // Result[List[B], String], list order
let all     = fan.settle { job_a(), job_b() }       // tuple of per-arm Results, arm order
let first   = fetch_a() ?? fetch_b() ?? fallback    // NOT a fan form: first Ok in SOURCE order
```

- `fan.map(xs, f)` 执行每个元素；结果按列表顺序，列表顺序中第一个 `Err` 是整个 map 的 `Err`
- `fan.settle { ... }`——每个分支都在每个目标上按分支顺序运行至完成（降级为元组）；结果是各分支 Result 的元组
- 源码顺序中的首个 `Ok` 用 `??` 链表示（只有左侧失败才计算右侧，因此胜者之后的候选不运行）；列表上写作 `list.find_map(xs, (x) => f(x)?) ?? fallback`。两者都不是 fan 形式，因为选择按定义就是顺序性的。`fan.any { }` / `fan.any(xs, f)` 在方言纪元14移除（E027 指出两种替代方式；ADR-0028）
- thunk 列表写法 `fan.any([...])` / `fan.settle([...])` 更早已移除（E027 指出迁移方式）

<a id="133-deterministic-time-compute-and-duration"></a>

### 13.3 确定性时间：`Compute` 与 `Duration`

时间量通过带模块限定的构造器，从封闭单位集（`ns / us / ms / s / min / h`）构造，并携带名义时钟类型：

```
compute.ms(100)     // Compute  — deterministic compute time (budgets)
duration.ms(5000)   // Duration — wall-clock time (oracle-tier surfaces)
```

- 在期望时间处使用普通 `Int` 是类型错误；两种时钟永不混用（防火墙仅在检查器中，降级时两者都擦除为以纳秒计的 `Int`）
- 负构造参数在运行时确定性中止（stderr 上为 `Error: negative time: compute.us(-5)`，退出1，两目标相同）；构造溢出会饱和到最大值而非回绕
- 确定性时间按冻结抽象机器的计费单位计量（函数入口——无循环且非递归的被调函数豁免——循环头及 `+` 字符串拼接，从不计插值；自 #3778 起所有目标采用同一张表），并通过带版本的校准常量（CM-1）从声明纳秒转换。契约**仅限比率**：声明时间在声明区间内近似墙钟（常设 CI 关卡测量它），但**计量本身**精确且与机器无关。完整规范规则见 `docs/adr/0001-deterministic-time-units.md`

<a id="134-fanbounded--deterministic-compute-budget"></a>

### 13.4 fan.bounded——确定性计算预算

```
let r = fan.bounded(compute.ms(100)) { work(input) } ?? fallback
```

- 在确定性计算预算下运行函数体。计量消耗超过预算时值为 `Err`，否则为 `Ok(body value)`。类型：`Result[T, String]`
- 判定**仅**取决于程序和输入：同一程序在每个目标和机器上，都在相同的声明纳秒从 `Ok` 转为 `Err`
- 预算与外围区域取最小上限（内层绝不能花费超过剩余额度）。目前源码不能嵌套区域：区域体内的 `fan.*` 形式为 E007，因为其体是纯的
- 函数体在**纯**上下文中检查；它是返回普通（非 `Result`）值的块

<a id="135-fanminclock--one-arm-selected-by-the-clock-you-pass"></a>

### 13.5 fan.min(clock)——由传入时钟选择一个分支

```
let b = fan.min(compute.us(50)) { a(), b() } ?? fallback              // least deterministic compute within the budget
let n = fan.min(compute.us(50), xs, (x) => ok(solve(x))) ?? fallback  // mapper form, per-element budget
```

- 时钟参数**必需**。其类型说明按什么测量“最小”，其值就是预算。`fan.min { … }` 和 `fan.min(xs, f)` 是解析错误（"fan.min requires a clock argument"）；没有时钟时，`fan.min { 3, 5 }` 会被读作最小值
- `Compute` 时钟（确定性层）：每个分支都按确定性时钟计量；胜者为 `(spend, index)` **字典序最小**的分支——成本最低，同成本按源码顺序解决。超预算分支被排除；无合格分支则为 `err("fan.min: no branch succeeded within budget")`。类型：`Result[T, String]`。每个分支都按源码顺序依次运行。分支为纯（E006）。返回 `Err` 的分支自行失格
- **映射器形式**在动态列表上竞争一个纯的单参数 lambda：分支 `i` 为 `f(xs[i])`，索引顺序就是列表顺序，预算按元素计算——与块形式同一法则。映射器必须返回 `Result`（映射器形式的契约）：`ok(v)` 参与竞争，`err(_)` 使元素自行失格。空列表得到无合格分支的 `Err`
- `Duration` 时钟（判定输入层）——已决定（ADR-0028，2026-10-18），在后续工作（#3788）落地；目前是类型错误。将采用的规则：分支共同启动，首个以 `Ok` **完成**的获胜；分支可做 I/O；从入口共享一个期限（不是每分支各自期限）；败者在下一计费点或 I/O 边界被取消，`fan.min` 等待它们后才返回（结构化，如 Haskell `race`、Python `TaskGroup`、Kotlin 和 Swift，而非 JS 的继续运行方式）；败者已经执行的效应保留，stdout 丢弃，仅返回胜者值。原生并行运行分支；wasm 在具备 async 前按顺序运行（分支0成功即获胜，这是合法调度）；ω 记录／重放固定胜者索引
- 历史：早期规范同时承诺“所有目标结果相同”（§13.1）和“首个完成的分支获胜”（当时§13.2），两者不兼容，因为后者使墙钟调度可观察。墙钟竞争在0.42.0移除；确定性块头以 `fan.race` 回归，并在方言纪元14改名为 `fan.min`（其他生态中 `race` 都表示“首个完成”）。`Compute` 时钟上快的分支获胜，所有目标一致认定胜者（T9）；`Duration` 时钟是判定输入层，胜者属于宿主输入

<a id="136-fantimeout--the-oracle-tier-wall-clock-deadline"></a>

### 13.6 fan.timeout——判定输入层的墙钟期限

```
let t = fan.timeout(duration.ms(5000)) { work(input) } ?? fallback
```

- 期限是 `Duration`（墙钟）；在计费点**协作式**检查，从不在操作中途检查（Go context 取消模型）；嵌套期限取绝对期限的最小上限
- 判定是 **ω 相对**的：在哪次检查触发期限属于宿主输入（ADR-0001 S8）。记录的 ω 可逐字节一致地重放（原生记录用 `ALMIDE_OMEGA_RECORD=1`，重放用 `ALMIDE_OMEGA=<n>`——使用记录的切断点而非时钟）。`tests/fan_timeout_replay_test.rs` 覆盖原生和嵌入式 Wasm 重放：不同重放序号在两条路径产生匹配的可观察进度（#3781）。在编译器／运行调用上设置 `ALMIDE_OMEGA`：切断点烘焙进构建产物，不是现有二进制动态读取宿主变量。这是超时切断重放，不是任意 I/O 或所有异步缺陷的记录。两端是绝对的：期限大于工作量时总会完成；发散的函数体在任何有限期限下都会被切断

<a id="137-rules"></a>

### 13.7 规则

- 所有 `fan` 形式仅限 `effect fn` 内——纯函数不能 fork（`fan.bounded` / `fan.timeout` 的体和 `fan.min(compute…)` 的分支本身按纯上下文检查）
- 不捕获 `var`，只捕获外层 `let` 绑定（防止数据竞争）
- 没有非结构化 `spawn`，所有并发都有作用域
- 分支到达 `@ordered` stdlib 函数的 `fan { }`（`env.set`、`fs` 写入、stdin 读取、`process` spawn——兄弟分支可能读取的进程内状态）在每个目标上按分支顺序执行（ADR-0024 D3）；无论哪条路径，观察都是顺序的
- 按设计不取消兄弟分支：被取消分支的副作用会取决于取消发生的时间，这正是模型排除的调度依赖。块汇合每个兄弟分支，并报告列表顺序中的首个 `Err`。（唯一切断是在计费点切断纯体：`fan.bounded`、`fan.timeout` 和 `fan.min(compute…)` 分支。计划中的 `fan.min` 的 `Duration` 时钟（#3788）有独立的败者取消契约，目前未实现。`fan.timeout` 只有一个纯体，不是竞争的败者分支）
- 预算是 `Compute`，绝非 `Duration` 或普通 `Int`——墙钟限制属于判定输入层（`fan.timeout`），不是确定性层

---

<a id="14-testing"></a>

## 14. 测试

<a id="141-test-declaration"></a>

### 14.1 test 声明

```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

测试是与函数同文件的顶层声明。无需单独测试文件（专用测试文件约定为 `*_test.almd`）。

<a id="142-assertion-functions"></a>

### 14.2 断言函数

```
assert(cond: Bool)                    // fails if false
assert_eq(actual: T, expected: T)     // fails if actual != expected
assert_ne(actual: T, expected: T)     // fails if actual == expected
```

<a id="143-running-tests"></a>

### 14.3 运行测试

```bash
almide test                      # all .almd with test blocks (recursive)
almide test spec/lang/           # directory
almide test spec/lang/expr_test.almd  # single file
almide test --run "pattern"      # filter by name
```

---

<a id="15-naming-conventions"></a>

## 15. 命名约定

<a id="predicates"></a>

### 谓词

谓词使用普通标识符：`is_empty`、`exists`、`contains`。返回类型为 `Bool`；名称末尾没有 `?`。

stdlib 谓词使用 `is_` 前缀约定：`string.is_empty(s)`、`string.is_digit(s)`。

<a id="stdlib-naming-rules"></a>

### Stdlib 命名规则

| 约定 | 规则 | 示例 |
|---|---|---|
| 模块前缀 | 标准形式为 `module.function()` | `string.len(s)`、`list.get(xs, i)` |
| 谓词 | 返回 `Bool` 的普通名称，通常带 `is_` 前缀 | `string.is_empty(s)`、`fs.exists(path)` |
| 无同义词 | 每种操作一个名称 | `len`，不是 `length`/`size`/`count` |
| 对称配对 | 逆操作采用匹配名称 | `split`/`join`、`to_string`/`to_int` |
| 返回类型一致性 | 可失败查找返回 `Option`，I/O 返回 `Result` | `list.get() -> Option[T]` |

---

<a id="16-standard-library"></a>

## 16. 标准库

[标准库参考](../../../stdlib/README.md)是当前公开 API 清单。模块页面包含签名、文档、示例和 `@since` 信息；签名索引由 `make stdlib-docs` 从编译器模块接口生成。本规范不复制这些列表，也不维护第二份函数计数。

<a id="161-auto-imported-modules"></a>

### 16.1 自动导入模块

导入规则及编译器拥有的注册表见 [§4.2](#42-auto-imported-modules-prelude)。当前函数和类型请遵循[参考](../../../stdlib/README.md)中的各模块页面。

<a id="162-import-required-modules"></a>

### 16.2 需要导入的模块

自动导入集合之外的公开模块要求 `import module`。其签名说明哪些调用有效应；导入要求本身不是效应分类。

<a id="163-bundled-modules-pure-almide-import-required"></a>

<a id="163-bundled-modules"></a>

### 16.3 随附模块

随附描述实现，而非导入政策：自托管 `.almd` 模块既包含自动导入 API，也包含显式导入 API。实现布局见 [`stdlib/README.md`](../../../../stdlib/README.md)，面向用户的导入规则见 [§4.2](#42-auto-imported-modules-prelude)。

<a id="164-built-in-functions"></a>

### 16.4 内置函数

无需导入，在各处均可使用：

```
println(s: String)              // print line to stdout
eprintln(s: String)             // print line to stderr
assert_eq(a: T, b: T)          // assert equal
assert_ne(a: T, b: T)          // assert not equal
assert(cond: Bool)              // assert true
```

没有 `print` 函数（不换行输出使用 `io.print`）。`println` 要求 `String`，没有隐式转换。

`println`、`eprintln`、`panic`、`assert`、`assert_eq` 和 `assert_ne` 可用于普通 `fn`：它们写入或中止，从不读取，也不使调用方带有效应。其他 I/O 仍受 effect 限制。准确范围和测试见 [effect-system.md §2.1](../../../specs/effect-system.md#21-output-and-abort-builtins-are-admissible-in-a-pure-fn)。

---

<a id="17-entry-point"></a>

## 17. 入口点

```almide
import env

effect fn main() -> Unit = {
  let args = env.args()
  println(list.get(args, 0) ?? "no argument")
}
```

入口函数不接收参数；运行时调用 `main()`。参数使用 `env.args()`：程序名被排除，因此元素0是首个用户参数。需要可失败的结果通道时，入口函数也可返回显式 `Result[Unit, E]`。

---

<a id="18-codegen-architecture"></a>

## 18. 代码生成架构

<a id="181-multi-target"></a>

### 18.1 多目标

Almide 编译到多个目标：

| 目标 | 状态 | 方法 |
|--------|--------|----------|
| **Rust** | 生产用 | 完整所有权分析，借用／clone 遍 |
| **WASM** | 生产用 | 直接输出（线性内存、WASI） |

<a id="182-codegen-v3-pipeline"></a>

### 18.2 Codegen v3 流水线

原生 Rust 路径通过语义／所有权处理遍降级带类型 IR，再由 walker／模板渲染 Rust。这不是 Wasm 输出路径：Wasm 使用直接结构化发射器，按宿主表面选择 core／component 包装。维护中的流水线说明见 [codegen](../../../specs/codegen.md) 和[架构](../../../ARCHITECTURE.md)。

传播降级按返回上下文处理源码 `!`，并非在每个 effect 调用处插入传播。Fan 降级在保留适用顺序契约的同时选择受支持的底层执行机制。

<a id="183-cross-target-semantics"></a>

### 18.3 跨目标语义

| 功能 | Rust | WASM |
|---------|------|------|
| `Result[T, E]` | 保留声明／推断错误类型 `E` 的 Result 载体 | 带对应载荷布局的带标签值 |
| `Option[T]` | `Option<T>` | 带 `T` 载荷布局的带标签值 |
| `effect fn` | effect 位与返回通道分开；`!` 使用其上下文 | 对应的 effect／返回语义，不是无条件 String 错误 |
| `==` / `!=` | 协议调用或普通结构相等 | 类型引导的相等性，而非通用字节比较 |
| String / List 上的 `+` | 运行时／辅助函数或优化后的原生操作 | 类型特定的结构／运行时操作 |
| `fan { }` | 按适用情况采用有作用域并行或有序执行 | 顺序或受支持的嵌入式宿主 worker 路径 |

---

<a id="19-prohibitions"></a>

## 19. 禁止事项

| # | 禁止项 | 原因 |
|---|---|---|
| 1 | 隐式类型转换 | LLM 会混淆类型 |
| 2 | truthiness | 条件只能是 `Bool` |
| 3 | 自定义运算符语法 | 运算符词汇固定；仍支持类型引导的内置及协议分派 |
| 4 | 异常（`throw`/`catch`） | 控制流不可见 |
| 5 | 多种 lambda 表示法 | 生成分布分散 |
| 6 | 内部 DSL | 强上下文依赖 |
| 7 | 通配符导入 | 名称来源不明 |
| 8 | `null` | 统一为 `Option[T]` |
| 9 | API 别名 | 词汇越多，幻觉越多 |
| 10 | `<>` 泛型 | 与比较运算符有歧义 |
| 11 | 猴子补丁／开放类 | 运行时语义改变不可见 |
| 12 | 宏和反射 | 语言表面必须固定 |
| 13 | `return` 关键字 | 最后一个表达式始终是值 |
| 14 | `class` 关键字 | 记录和 variant 使用 `type` |

---

<a id="20-compiler-diagnostics"></a>

## 20. 编译器诊断

<a id="201-one-error-one-root-cause"></a>

### 20.1 一个错误，一个根因

抑制级联派生错误，只呈现单个根因。

<a id="202-structured-error-output"></a>

### 20.2 结构化错误输出

下面是示意诊断，不是 CLI JSON 传输模式。使用方应采用文档规定的 [CLI](../../../specs/cli.md) 输出及其模式／版本契约。

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

### 20.3 拒绝语法的提示

编译器识别其他语言常见语法，并提供可执行的提示：

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

### 20.4 自动修复候选

- 缺少导入
- 类型转换候选
- 缺失 match 分支
- 缺少 `effect` 修饰符
- 纯函数调用带效应代码时建议 `effect fn`

<a id="205-official-formatter"></a>

### 20.5 官方格式化器

```bash
almide fmt app.almd
```

- 一个 AST 恰好生成一种格式化输出
- 导入按字母顺序排列
- 包含尾随逗号
- 固定换行规则

---

<a id="21-typing-rules"></a>

## 21. 类型规则

<a id="variables"></a>

### 变量

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

### 函数

```
G, x1:T1, ..., xn:Tn |- body : R
------------------------------------------
G |- fn f(x1:T1,...,xn:Tn) -> R = body
```

<a id="option--result-constructors"></a>

### Option / Result 构造器

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

## 22. 完整示例

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

## 23. 评估指标

| 指标 | 定义 |
|---|---|
| **Pass@1** | 首次生成即通过编译和测试的比例 |
| **Repair Turns** | 从首次失败到最终成功的修复迭代数 |
| **Token Cost** | 成功前输入／输出 token 总数 |
| **API Hallucination Rate** | 不存在 API 或错误签名的比例 |
| **Edit Breakage Rate** | 修改代码时破坏无关行为的比例 |
| **Diagnostic Utilization Gain** | 有无结构化诊断时的性能差异 |
