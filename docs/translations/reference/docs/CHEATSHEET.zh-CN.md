<a id="almide-quick-reference-for-ai-code-generation"></a>

# Almide 速查表（用于 AI 代码生成）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CHEATSHEET.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CHEATSHEET.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CHEATSHEET.zh-CN.md)



> 原文：[CHEATSHEET.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md)。本译文是固定提交版本、面向读者的非规范性快照，以具有规范性的英文原文为准，不替代规范、契约、检查器或生成流程的输入。不会自动生成或自动同步。代码示例及注释、行内代码均保持原样。本次翻译未独立执行或重新验证代码、行为、跨目标等价性、安全性、时间或证明相关主张。

> 原文不一致说明：动态 json.* API 为“29 个函数”的表述，与同一固定版本中 json 模块索引列出 3 个函数、value.* 说明已迁移操作的内容并不一致。“没有同义词”的建议，也与生成索引中 list.length 等已弃用别名并存。正文保留原文，未擅自更新或统一。历史名称、移除时间、目标限制和证明范围均为原文时点的主张。

文件扩展名：`.almd`

<a id="file-structure"></a>

## 文件结构
```
@dialect(3)          // optional: the language dialect this file was verified against
import <module>
// declarations...
```

函数迁移或移除时，标记旧函数，告诉调用方如何修复。编译器会比较两个签名，只有整个修改仅需替换名称时，才向 `almide fix` 提供该编辑：

```almide
@deprecated(since = 3, use = "string.trim_start")   // renamed
@deprecated(since = 3, note = "unsound on surrogate pairs")  // removed, no successor
```

`@dialect(N)` 是可选的，放在所有内容之前。它记录文件上次检查通过时的语言方言，而非发布版本号：只有语言表面变化可能破坏已有代码时，epoch 才会推进（`proofs/dialect-epochs.toml` 列出每次破坏了什么）。标记比编译器旧时不提示；比编译器新时为 `E051`，因为这里尚未验证该文件。使用 `almide check <file> --stamp` 写入或推进标记，它只会向前更新。

<a id="project-layout-multi-file"></a>

## 项目布局（多文件）
项目超过 1 个文件时，创建一个包：
```
mypkg/
  almide.toml          # [package] name = "mypkg"
  src/
    main.almd          # entry: imports siblings via `import self.<name>`
    classifier.almd    # sibling module: bare fn / let / type
    bindings/
      python.almd      # nested namespace → mypkg.bindings.python
```
导入同级模块：
```
import self.classifier                          // → classifier.fn(), classifier.LET
import self.classifier.{classify, NUMBERS}      // selective: bare names
import self.classifier as cls                   // alias
```
**不要**写 `import x from "./x.almd"`，不支持文件路径。始终使用 `import self.<sibling>`。

<a id="types"></a>

## 类型
```
type Name = { field: Type, ... }                     // record
type Name = | Case1(Type) | Case2 | Case3{f: Type}  // variant (leading |)
type Name[A, B] = { first: A, second: B }            // generic (use [] not <>)
type Name = Type                                     // type alias (transparent)
type Name = Case1(Type) | Case2(Type)                // inline variant (no leading |)
type Handler = (String) -> String                    // function type alias
```

<a id="conventions"></a>

### 约定
```almide check
type Color: Eq, Repr = Red | Green | Blue   // convention after type name with :
```

**变体序列化——推荐模式。** 变体类型跨越序列化边界（JSON / 文件 IO / dojo 测试夹具）时，声明 `: Codec`，自动生成 `Type.encode` / `Type.decode`，而非手写 `match` 分支链。派生 Codec 使用外部标签：构造器名为唯一键；记录载荷保留字段名，元组载荷为按位置排列的数组，unit 分支为 `null`：
```almide check
type Event: Codec = | Click(Int, Int) | Scroll{dy: Int} | Quit
// Event.encode(Click(10, 20))    → {"Click": [10, 20]}
// Event.encode(Scroll { dy: 5 }) → {"Scroll": {"dy": 5}}
// Event.encode(Quit)             → {"Quit": null}
```
仅对永不序列化的变体省略 `: Codec`（程序从不写出的内部枚举）。其他变体都应启用：LLM 手写往返转换代码容易出错。

<a id="names-any-script"></a>

### 名称（任意文字体系）
标识符使用 Unicode（UAX #31，以 NFC 保存）；关键字仍为 ASCII。全角 `ａ`/`１` 被拒绝，不会折叠。大写开头属于类型侧，小写或无大小写区分的文字（漢字/かな/한글）属于值侧；模块声明为类型或分支的无大小写名称，在该模块中属于类型侧；从另一模块使用时写限定名（`geo.住所`）。
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

### 内置类型
- 基本类型：`Int`、`Float`、`String`、`Bool`、`Unit`、`Path`
- 集合：`List[T]`、`Map[K, V]`、`Set[T]`
- 错误：`Result[T, E]`（`ok(v)` / `err(e)`）、`Option[T]`（`some(v)` / `none`）
- Option 简写：所有类型位置均有 `T?` ≡ `Option[T]`（规范写法，`almide fmt` 会归一化为此形式）

<a id="integer-literals"></a>

### 整数字面量
```
255  1_000_000       // decimal, `_` separators anywhere between digits
0xFF  0x1_00         // hex     (0x / 0X)
0b1010  0b1111_0000  // binary  (0b / 0B)
0o17                 // octal   (0o / 0O)
```
所有进制都是普通 `Int` 值，算术相同，两个目标一致。只有前缀（如 `0x` 后无数字）是编译错误，绝不会静默变为 `0`。

<a id="functions"></a>

## 函数
```
fn name(x: Type, y: Type) -> RetType = expr
fn name(x: Type) -> Int!                             // pure-fallible: sugar for Result[Int, String]
effect fn name(x: Type) -> T = expr                  // touches the world; a call is written name(x)! — see Effects
```

`=` 后的函数体是一个表达式，即使下面各行缩进也是如此。多语句函数体需要花括号：`fn main() -> Unit = { ... }`（见“块”）。

<a id="option-shorthand-t-adr-0010"></a>

### Option 简写 `T?`（ADR-0010）

`T?` ≡ `Option[T]`，在每个类型位置都有效（不同于标记箭头的 `!`）。`?` 绑定到紧邻前方的类型原子，绝不跨越 `->`：

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

`T?` 是规范写法：`almide fmt` 将 `Option[T]` 改写为它。

<a id="visibility-optional-prefix-before-fntype"></a>

### 可见性（fn/type 前的可选前缀）
- `fn f()` — 公开（默认）
- `mod fn f()` — 仅同一项目（Rust 中为 `pub(crate)`）
- `local fn f()` — 仅此文件（私有）
- 类型使用相同前缀（`mod type T = …`、`local type T = …`）；不带前缀的 `type` 公开。没有 `pub type`（E097：删除 `pub`）

<a id="modifiers-order-matters-localmod-effect-fn"></a>

### 修饰符（顺序重要）：`[local|mod]? effect? fn`

<a id="predicate-fn-emptyxs-listt---bool-bool-return-only"></a>

### 谓词：`fn empty(xs: List[T]) -> Bool`（仅返回 Bool）

<a id="hole--todo"></a>

### 空洞 / Todo
```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```
两者都接受上下文要求的类型，因此程序其余部分仍可通过类型检查；执行到它们时都会 PANIC（native：退出码 101，`not yet implemented: hole at line N` / `not yet implemented: <your message>`）。这是仅限 native 的工作流：结构化 wasm 发射器不支持这些节点，仍含任一节点的程序会被拒绝（`almide run --target wasm` 以 1 退出，诊断指出不支持的形态）。为 wasm 构建之前先填好空洞。

**调用实参**中的 `_` 是另一回事，会被拒绝（E046）：
```
let v = add(_, 10)          // ✗ E046 — not partial application, no `_` sections
let add10 = (x) => add(x, 10)   // ✓ name the missing value with a lambda
```

<a id="mutable-parameters"></a>

### 可变参数
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
调用者必须传入 `var` 绑定（`let`、非 `mut` 参数或临时值为 E032）。`mut` 可用于任意位置的任意参数。所有标准库原地操作都使用此机制（`list.push`、`list.pop`、`list.clear`、`map.clear`、`string.push`、`string.clear`，以及整个 `bytes` 写入家族：`push`、`append_*`、`put_*`、`fill`、`clear`、`copy_from`）：接收者为 `mut` 参数，因此局部缓冲区是 `var`，写入调用者缓冲区的辅助函数声明 `mut b: Bytes`。除此之外保持值语义：`let c = a` 复制值，被调函数无法通过普通参数访问调用者的绑定。带 `mut` 参数的函数只能调用，不能作为值使用：`let f = incr` 或 `list.map(xs, incr)` 是 E096，因为函数类型无法承载写回。对 `var` 调用它（`incr(n)`，或作为值使用 `() => incr(n)`）；在回调槽中，复制到局部 `var` 后返回：`list.map(xs, (item) => { var copy = item; incr(copy); copy })`。

<a id="effects-failure-and-"></a>

## 效果、失败与 `!`

<a id="effect-fn-and-the--at-its-call-sites-adr-0002-d6-adr-0008"></a>

### `effect fn` 及其调用处的 `!`（ADR-0002 §D6、ADR-0008）

`effect fn f() -> T` 会接触外部世界（fs、http、process、random、io 等）。用户声明的 effect fn **始终可失败**：调用产生 `Result[T, String]`，即便函数体不可能失败。`-> T` 是规范写法，因为 `effect` 已说明它可以失败。

纯 `fn` 仍可调用 `println`、`eprintln`、`panic` 和 `assert` / `assert_eq` / `assert_ne`：它们只写出或中止，从不读取，不使函数成为 `effect`（ADR-0022）。从纯函数执行其他效果（`fs`、`env`、`io.*`、`http`、`fan` 等）是 E006。标准库读取器也是 effect fn：对 `io.stdin()!` 调用的 `io.read` / `io.read_exact` / `io.read_to_end`、`env.args` 和 `args` 读取器（`args.has_flag`、`args.option`、`args.positional`）读取 stdin 或 argv，因此像 `io.read_line` 一样需要 `effect fn` 调用者（dialect epoch 8；纯函数调用它们为 E006）。返回 `String?` 的那个（`args.option`）在默认值前需要 `!`：`args.option("out")! ?? "out.txt"`；`!` 传播错误，`??` 为 none 提供默认值（#3522）。

把 effect 调用上的 `!` 像 Swift 的 `await` 一样阅读：它标出接触外部世界的位置，不论这次调用能否失败（Almide 没有 `await`，这里只是阅读类比）。它是 Swift 的 `try`，**不是** `try!`：向外层函数传播错误，从不陷阱退出。

| 调用 | 应写形式 |
|---|---|
| 用户声明的 effect fn，或可失败的标准库函数（`fs.read_text` 等） | 用 `f()!` 传播，用 `let _ = f()` 明确丢弃，或对 `ok`/`err` 做 `match`。单独的 `let x = f()` 为 E041，裸语句 `f()` 为 E042 |
| 不会失败的标准库 effect fn（`random.int`、`io.read_line`、`fs.exists` 等） | 返回普通值；其上的 `!` 是无害的空操作 |
| 用 Almide 编写、返回普通值的标准库 effect fn（`io.stdin()`、`io.stdout()`、`uuid.v4()`、`process.child_pid(c)` 等） | 与用户声明的一样，调用产生 `Result`：写 `!`，例如 `io.write(io.stdout()!, data)!` |

始终正确的一条规则：**每个 effect 调用都写 `!`。** 加上它不需要知道标准库 effect fn 是否会失败。

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

### 纯且可失败的标记 `-> T!`（ADR-0002 Phase 1）

`-> T!` 声明可失败的纯函数，返回类型就是 `Result[T, String]`。内部的 `!` 会传播（无需 effect fn），末尾的值自动提升为 `ok(...)`。可写载荷，也可显式写 Result，两者均可：

```almide check
fn parse_port(s: String) -> Int! = int.parse(s)      // pass-through (already a Result)
fn double_port(s: String) -> Int! = int.parse(s)! * 2  // value tail — lifts into ok(...)
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! propagates in a T! body
  guard n > 0 else err("must be positive")
  n                                                  // value tail lifts (ok(n) also fine)
}
```

函数体使用 `!` 的 lambda 会成为可失败闭包 `(A) -> Result[T, E]`（第一类值；作为任意 list / map / set / option 高阶函数的回调，例如 `list.all(xs, (x) => p(x)!)!`、`map.fold`、`set.map`、`option.map` 等，会使高阶函数使用首个 err 短路的可失败形式；函数类型槽写为 `(A) -> B!` 或 `(A) -> B!E`）。其 E 来自上下文：带类型槽 `(A) -> B!E`、带类型的 `let`、结果传播进入的函数；否则来自其 `!` 操作数的合并：共享一个 E 则保留，不一致则退为 String（ADR-0021）。test 块中的 lambda `!` 保持普通 unwrap。作为返回标记的 `!` 只允许出现在函数声明的返回位置和函数类型槽中。

标记也承载带类型的错误（ADR-0012 D2）：对具名错误类型 E，`-> T!E` ≡ `Result[T, E]`，而 `T!` 仍是 `T!String` 的简写。`?` 先结合：`-> B?!E` 是 `Result[Option[B], E]`。传播不变，`!` 从不转换 E（ADR-0003/0004：按结构而非消息文本分支）；E 不匹配仍是检查错误。

```almide check
type ConfigError = | Missing(String) | BadValue(String)
fn load(path: String) -> String!ConfigError = {
  guard path != "" else err(Missing(path))
  "text"                          // value tail lifts into ok(...) with E=ConfigError
}
```

<a id="unwrap-operators"></a>

### 解包运算符
```
expr!              // unwrap Result/Option, propagate the failure (effect fn, or a pure fn returning Result/Option)
expr ?? fallback   // unwrap or use fallback value
expr?              // Result → Option (err → none)
expr?.field        // optional chaining (Option[Record] → Option[FieldType])
o?.x ?? default    // the idiom: read a field if present, else default (ADR-0005; `?.` is Option-only — on a Result write `(r?)?.x`)
```

每个值级运算符都是某个具名标准库函数的语法糖（ADR-0005）：`x ?? d` ≡ `option.unwrap_or_else(x, () => d)` / `result.unwrap_or_else(r, (_) => d)`（回退惰性求值），`r?` ≡ `result.to_option(r)`，`o?.x` ≡ `option.map(o, (v) => v.x)`。对已经是 Option 的值使用 `o?` 是空操作并产生警告（E056）。

<a id="reading-a-file-that-may-not-exist-adr-0004-d4"></a>

### 读取可能不存在的文件（ADR-0004 D4）

不存在是值，不是错误；绝不要按错误文本分支：

```almide
let cfg = fs.read_text_if_exists(path)! ?? "default"
//  ok(none) = absent (missing parents too) / err = permission, IO — real failures
// family: read_text / read_bytes / read_lines + _if_exists
```

<a id="error-handling-doctrine-adr-0004"></a>

### 错误处理原则（ADR-0004）

**选择 `E` 是分层决策，而非个人喜好。** `E = String` 是**擦除**层，即供人和模型阅读的*报告*通道，也是默认值。变体 `E` 是**细化**层，即程序根据内容改变行为的*分支*通道；它只在**封闭领域**内值得付出成本，也就是自行处理该错误的模块或包。跨出该领域时，变体被**降级**回 String，且降级始终显式表现为 `map_err`（没有转换钩子，无法静默发生）。完整规则：[docs/specs/result-option-effect.md §8.0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/result-option-effect.md)。

**绝不按错误消息的文本分支**（`string.contains(e, …)`、`e == "No such file"`）；文本是报告而非 API，E035 会警告。调用者必须按失败*种类*分支时，就该定义变体错误类型并匹配结构：

```almide
type LoadError = | NotFound(String) | BadValue(String)

match load(p) {
  ok(v)               => v,
  err(NotFound(_))    => default_value,     // branch on structure
  err(BadValue(msg))  => process.exit(1),
}
```

对于与种类无关的回退，完全不读取错误：`load(p) ?? default`。

**为错误添加上下文**——规范写法（不要发明变体；保留 `": "` 作为分隔符，`${e}` 放在末尾）：

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

## 内置协议
Eq 和 Hash 是自动的（编译器按类型结构派生），无需注解。
```
// Eq: all value types support == (except Fn)
let same = color_a == color_b  // just works
```
<a id="protocols-user-defined-conventions"></a>

### 协议（用户定义约定）
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
内置约定（Eq、Repr、Ord、Hash、Codec）也是协议。

自有类型也可用相同方式，通过普通方法名（绝不使用 `__` 前缀）满足内置句柄与消息协议：

| 协议 | 类型需编写的方法 |
|---|---|
| `Read` | `effect fn T.read(self, max: Int) -> Result[Bytes, String]` |
| `Write` | `effect fn T.write(self, data: Bytes) -> Result[Unit, String]` |
| `Close` | `effect fn T.close(self) -> Result[Unit, String]` |
| `HttpMessage` | `header`、`header_values`、`headers`、`body`、`body_bytes`、`set_header`、`set_body`、`set_body_bytes` |
| `Body` | `fn T.media_type(self) -> String`（字符串字面量）、`fn T.to_text(self) -> String` |

随后 `io.read` / `io.write` / `io.close`、`http.header` / `http.body` 和 `http.response` 就能接受你的类型。媒体类型可运行脚本（HTML、SVG、XML）的 `Body` 还需在 `media_type` 上声明 `@media_owns(...)`，拥有此类型的依赖需要应用授予 `[trust] media` 权限（E118）。

首个参数可显式命名并标注类型（`a: GreetAction`），也可写为裸 `self`（`self: Self` 的语法糖）；在约定方法中，两者都解析为声明类型，与 `protocol { ... }` 声明内部一样。可变接收者为 `mut self`（`mut self: Self` 的语法糖）。

<a id="generic-protocols-explicit-conformance"></a>

### 泛型协议（显式符合）
协议可以接受类型参数。一个类型只符合在声明处指明的一个实例化；泛型函数由应用后的协议约束。调用运行该类型自己的方法，不搜索其他内容。
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
- 在符合声明和约束中，每个参数恰好给一个类型：`Repository[Int, User]`，绝不写裸 `Repository`。
- 每个类型对每个协议只符合一次；第二个实例化需包装类型。
- 实现必须与协议方法的 `effect` 和 `mut` 完全匹配。
- 其他模块的协议像类型一样带模块名：`[R: ports.Repository[K, V]]`、`type Mem: ports.Repository[Id, User]`。
- 协议是约束，不是类型：`List[Repository[Int, User]]` 是错误。

<a id="expressions"></a>

## 表达式

<a id="if-an-omitted-else-returns-unit"></a>

### If（省略 `else` 时返回 Unit）
```
if cond then expr else expr
if a then x else if b then y else z
```
**没有 `else` 的 `if` 返回 Unit。** 提前返回请改用 `guard`。

<a id="match-exhaustive-supports-guards"></a>

### Match（穷尽，支持守卫）
```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

<a id="patterns"></a>

### 模式
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
**`_` 可出现在 match 模式、`let _ = x`（丢弃）、`for _ in xs` 和 lambda 参数 `(_ ) => expr` 中。**

**模式中不支持：** 没有范围模式（`1..5`）；rest 只能位于列表最后一项（`[..t, x]` 被拒绝）。支持在分支顶层使用或模式（`"a" | "an" | "the" => …`），但不能绑定变量；函数体需要载荷时请分别写分支。支持列表 rest（`[h, ..t]`），这是惯用的头/尾拆分：

```almide check
fn sum(xs: List[Int]) -> Int = match xs {
  [] => 0,
  [h, ..t] => h + sum(t),
}

fn main() -> Unit = println(int.to_string(sum([1, 2, 3])))
```

<a id="lambda"></a>

### Lambda（匿名函数）
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

**Lambda 与效果**：lambda 继承外层函数的效果能力，这是所有高阶被调函数（`list.map` 等）的一致规则。`effect fn` 内的 lambda 可以调用 effect fn，但结果仍为**显式 `Result` 值**（自动 `?` 绝不跨越闭包边界）：用 `?? fallback` 或 `match` 处理，或用 `!` 传播到 **lambda 自己**的失败通道，绝非外层函数。普通 lambda 中的 `!` 会推断出可失败 lambda，并将末尾值提升为 `ok(...)`；不要求显式 effect 类型槽。test 块 lambda 则保留测试的 unwrap 语义。纯函数内的 lambda 调用 effect fn 是错误，但允许纯可失败 lambda。例外是计量区域（`fan.bounded` / `fan.min` 的函数体）：它们按设计是纯的，因此即便在 effect fn 中也拒绝效果调用。

**effect 函数类型槽**（`effect (A) -> B`）：高阶函数可声明其回调运行效果，如 `effect fn serve(port: Int, handler: effect
(HttpRequest) -> HttpResponse) -> Unit`。对 `effect (…) -> …` 槽检查的 lambda 获得完整的 effect fn 函数体便利性：允许效果调用，`!` 传播到处理器自己的失败通道（失败的 `http.serve` 处理器变成 500 响应）。两种写法都统一接受：

```
http.serve(8080, (req) => {
  let body = fs.read_text("index.html")!   // ← `!` works: the slot is effect-typed
  http.response(200, body)
})!
```

调用 effect 函数类型的值本身也是效果调用（高阶函数内部的 `h(x)!`）；从纯函数执行则为 E006。将 effect fn 作为参数传入，如 `list.map(xs, fs.read_text)`、`apply(fs.read_text, p)`，也会使该调用成为效果调用（普通函数类型参数是透明的），因此在纯函数中也是 E006。纯函数运行经值获得的 effect fn——记录字段、列表元素、effect fn 构建的闭包——是 E109：效果位随值携带，所以应将函数标为 `effect fn`（#4151）。普通 `(A) -> B` 槽仍拒绝可失败 lambda（E005）；把槽声明为 `(A) -> B!` 或 `effect (A) -> B`。裸箭头形式为规范写法；`almide fmt` 将旧式 `fn(A) -> B` 归一化为它。

具名函数与 lambda 一样可作槽值（#1148）：以值引用的 effect fn 是 `(A) -> Result[B, String]` 闭包；可以绑定（`let h: effect (String) -> Int = parse_pos`，注解可选）、通过管道调用（`(s |> h)!`），或直接传入 effect 槽。纯具名函数也可填入 effect 槽（结果会包为 ok）。UFCS 方法语法不适用于函数值（`x.h()` 是 E002，应写 `h(x)` 或 `x |> h`）。

<a id="block-last-expression-is-the-value"></a>

### 块（最后一个表达式为值）
```
{
  let x = 1
  let y = 2
  x + y
}
```

<a id="forin-loop"></a>

### For...in 循环
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

### While 循环
```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

<a id="range"></a>

### 范围
```
0..<5           // [0, 1, 2, 3, 4]  (exclusive end)
1...5           // [1, 2, 3, 4, 5]  (inclusive end)
for i in 0..<n { ... }   // optimized: no list allocation
let xs = list.map(0..<10, (i) => i * i)  // range as List[Int]
```
只要预先知道迭代次数，就优先写 `for i in 0..<n`（用 `var` 传递状态）；循环支持 `break` / `continue`。递归数据或状态转换在递归表达算法更清楚时使用递归，参见“✗ 计数递归”。

<a id="pipe"></a>

### 管道
```
text |> string.trim |> string.split(",")   // = string.split(string.trim(text), ",")
xs |> list.filter((x) => x > 0)            // the left side is always argument 1
```
没有管道占位符：`xs |> list.filter(_, f)` 是 E046，应去掉 `_`。其他实参位置则通过 lambda：`x |> (v) => f(a, v)`。

<a id="record--spread"></a>

### 记录与展开
```
{ name: "alice", age: 30 }
{ ...base, name: "bob" }
User { name: "alice" }     // named record construction — canonical form
User(name: "alice")        // alias: named-argument call syntax, normalized to the brace form
User("alice")              // WRONG: records take named fields, not positional (E021)
```
括号别名也能匹配普通记录类型（`User(name) => ...`）。它**不能**用于变体的记录载荷分支：`Circle { radius }`（`type Shape = Circle { radius: Float } | ...` 的分支）只能以 `Circle { radius }` 匹配，不能写 `Circle(radius)`（E021）。

<a id="list"></a>

### 列表
```
[1, 2, 3]
[]                         // empty list (there is NO list.new())
xs[0]                      // index read
xs[i] = value              // index write (var only)
xs |> list.find_map(f)     // first some(..) that f gives, or none — f is not called past the hit
```

<a id="map"></a>

### Map
```
["a": 1, "b": 2]          // map literal
[:]                        // empty map (requires type annotation)
let m: Map[String, Int] = [:]
m["key"]                   // index read (returns Option[V])
m["key"] = value           // index write (var only)
```

<a id="string-interpolation"></a>

### 字符串插值
```
"hello ${name}, result=${1 + 1}"
```
数值、`Bool` 和 `String` 打印其值；`List`、`Map`、`Set`、`Option`、`Result`、元组、记录和变体打印 Almide 字面量形式（`[1, 2]`、`["a": 1]`、`some(3)`、`P { n: 1 }`）。没有字符串形式的值——`Bytes`、`Unit`、`Matrix`、句柄、函数值或含此类值的容器——会触发 [E089](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/diagnostics/E089.md)：应插值真正想表达的内容（`${bytes.to_list(b)}`、`${matrix.to_lists(m)}`、`${f(x)}`）。声明了 `fn T.repr` 的类型，无论内部持有什么，都通过该方法打印，所以带函数字段的记录一旦声明它也可插值。

<a id="string-escapes"></a>

### 字符串转义
```
"\n \t \r \\ \" \$"   // newline, tab, return, backslash, quote, dollar
"\x1b"                // \xNN  — two hex digits, codepoint 0x00..0xFF (ESC here)
"\u{1F600}"           // \u{…} — 1..6 hex digits, any Unicode scalar (😀)
```
格式错误的数字转义（如 `\xzz`、`\u{}`）保留字面形式。

<a id="heredoc-multi-line-strings"></a>

### Heredoc（多行字符串）
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

## 语句

<a id="top-level-let-module-scope-constant"></a>

### 顶层 let（模块作用域常量）
```almide check
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```
顶层 `let` 在编译时求值（const），或通过 `LazyLock` 求值（String 等非 const 表达式）。

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

### 解构
```
let { name, age } = user    // record destructure (1 level only)
var (x, y) = pair           // var takes the let patterns; every name is a var
```

<a id="processing-a-file-line-by-line-large-files"></a>

### 逐行处理文件（大文件）

`fs.read_lines` 物化整个文件，小文件无妨，大文件则遇到内存瓶颈。大文件聚合使用 `fs.fold_lines`（内存 O(最长行)，行语义与 read_lines 相同）：

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

分区遍历器（`fs.fold_lines_range` / `fs.fold_lines_chunked`）没有可失败形式，因为分区遍历没有明确定义的首个 err；所以可能出错的分块体需自行处理错误。

`result.collect` / `collect_map` 已移除（ADR-0007：名称承诺 Rust 的首错短路，却做了相反的事）。用 partition 收集每个错误：

```almide
let (oks, errs) = result.partition(results)
if list.is_empty(errs) then ok(oks) else err(errs)   // Result[List[T], List[E]]
```

<a id="guard-early-return--loop-break"></a>

### Guard（提前返回 / 跳出循环）
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

## 并发与确定性时间（fan）

所有 `fan.*` 形式都需要 `effect fn` 上下文。Almide 没有 `async`/`await`。

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

### fan 记录形式——并行字段，按名称读取

`fan { name: expr, … }` 将字段作为并行兄弟运行，产生匿名记录 `{ name: value, … }`。移除 `fan` 会得到相同输出和值（字段按源码顺序打印）。可失败字段必须写 `!`：字段的 Err 是整个块的 Err；每个字段仍会运行，按源码顺序第一个 Err 胜出。没有 `!` 的 Result 字段是 E041。兄弟字段在块内部不在作用域中（不能在块内相互引用）。

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

### 时间构造器（封闭集合）

两种时钟类型，每种六个单位：`ns / us / ms / s / min / h`。

```
compute.ms(100)     // Compute — deterministic compute-time (fan.bounded budgets, fan.min's deterministic clock)
duration.ms(5000)   // Duration — wall-clock time (fan.timeout deadlines; fan.min's real-time clock, #3788)
```

- 裸 `Int` 绝不是时间：`fan.bounded(5000) {...}` 是类型错误，应写 `compute.ms(5000)`。
- `Compute` 与 `Duration` 不混用：`fan.bounded(duration.ms(5)) {...}` 是类型错误。
- 没有字面量后缀：`100ms` 无法解析，应写 `compute.ms(100)`。
- 负参数在运行时中止（`Error: negative time: ...`）；构造溢出则饱和到最大值。
- `fan.min(compute…)` / `fan.bounded` 结果是确定性的：同一程序 + 同一输入 = 在每个目标和机器上相同的胜者/判定，限于同一计量器版本内（计费表及其校准有版本，变化随版本公布）。
- 时钟类型决定层级：`compute.*` 表示纯函数体，各处答案相同；`duration.*` 表示预言机层级，判定是宿主输入，与 ω 一起记录和重放。
<a id="scoped--a-declared-reclamation-boundary"></a>

## scoped——声明的回收边界

`scoped { ... }` 声明块所分配存储的终点；`scoped fn` 将“可在区域内运行”作为 worker 签名的一部分。二者都是受检查的义务，而非提示；块的值就是其块体的值，允许片段之外的形态在两个目标上都会于检查时拒绝。

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

- 内部允许：标量（`Int`/`Float`/`Bool`/`Unit`）、同文件声明的标量变体类型、标量记录及这些类型的元组和 `Option`；对其他 `scoped fn`、构造器及 `int`/`float`/`math`/`bool` 成员的调用。
- 只有标量跨越边界，内部构建的值是 **E086**。
- 被调函数不是 `scoped fn`、全局量、`println`、lambda、捕获的堆值、`!` / `?` / `guard`，均为 **E087**。
- 单个非尾部自调用或相互递归为 **E088**（携带累加器；每个分支有两次自调用的树遍历允许）。
- `scoped` 是上下文相关的：名为 `scoped` 的变量仍可用，`match scoped { ... }` 仍匹配它。

<a id="test"></a>

## 测试
```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

<a id="testing-effect-fn-error-cases"></a>

### 测试 effect fn 错误情况

test 块中，`effect fn` 调用返回 `Result[T, String]`，不会自动解包。用 `!` 取值，或直接对 `ok`/`err` 断言：

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

### 测试代码中止

`testing.assert_aborts(expected, () => …)` 只在子进程（native）或新实例（wasm）中运行该闭包；若以包含 `expected` 的文本中止则通过，包括 `panic`、索引越界、除零、断言失败。之后测试继续。闭包必须是在调用内书写、不捕获测试内容的 lambda（E098），调用直接位于 `test` 块内（E099）。没有 `assert_throws`（dialect epoch 14 已移除）。

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

## 内置函数
```
println(s)                 // print line to stdout
eprintln(s)                // print line to stderr
assert_eq(a, b)            // assert equal
assert_ne(a, b)            // assert not equal
assert(cond)               // assert true
panic(msg)                 // abort: message on stderr, exit 1
```
**没有 `print` 函数。** 所有输出（包括面向用户的错误消息）使用 `println`。
六个函数都能从纯 `fn` 调用，输出与中止不需要 `effect`。
`eprintln` 仅用于调试/内部错误，面向用户的消息必须使用 `println`。

<a id="stdin--parsing"></a>

### 标准输入与解析
```almide check
import io                                     // io is NOT auto-imported
effect fn main() -> Unit = {
  let line = io.read_line() ?? ""             // String? — none at end of input; it cannot fail, so NOT a Result
  let n = int.parse(string.trim(line)) ?? 0   // int.parse(s) -> Result[Int, String]
  println(int.to_string(n))
}
```
`io.read_line()` 读取一行 stdin，去掉末尾换行，返回 `String?`：输入结束为 `none`，空行为 `some("")`。调用者必须为 `effect fn`。字节通过句柄读取：`io.read(io.stdin()!, n)!`、`io.read_to_end(io.stdin()!)!`。`int.parse` / `float.parse` 返回 `Result`；用 `??`、可失败上下文中的 `!` 或 `?` 解包。没有 `int.from_string`。

<a id="entry-point"></a>

## 入口点
```
effect fn main() -> Unit = {
  let args = env.args()              // command-line args, without the program name
  let name = list.get(args, 0) ?? "world"
  let content = fs.read_text("config.txt")!   // propagate error with !
  println("Hello, ${name}: ${content}")
}
```
`effect fn main()` 自动包装为返回 `Result<(), String>`。无需写 `ok(())` 或 `-> Result[...]`。
仅当 `main` 执行实际效果（读取、文件、网络、`fan`、`!`）时才写 `effect fn main()`；只计算和打印的 `main` 应为 `fn main() -> Unit`。

**临时文件**：使用 `fs.create_temp_file(prefix)` / `fs.create_temp_dir(prefix)` 创建，绝不在 `fs.temp_dir()` 下使用固定名称。临时目录是共享的，其他用户可先创建固定名称（作为指向你的文件的符号链接）。创建函数选择随机名称，独占创建（绝不复用或跟随已有条目），并设为仅你可访问；它们返回路径。`fs.temp_dir()` 是位置，不是安全名称。也绝不硬编码 `/tmp`，Windows 上不存在它。

```almide
let path = fs.create_temp_file("report")!
fs.write_text(path, data)!
```
通过 `env.args()` 访问命令行参数（不是 main 参数）；列表不含程序名 argv[0]。

<a id="operators-precedence-highlow"></a>

## 运算符（优先级高→低）
`. () [] ! ? ?. ??`（后缀）> `not -`（前缀）> `>>` > `^`（幂）> `* / %` > `+ -` > `..< ...` > `|>` > `== != < > <= >=`（非结合）> `and` > `or`

`^` 为幂运算（右结合，也接受 `**`）。`+` 用于字符串和列表连接（与加法重载）。XOR 使用 `int.bxor(a, b)`。

`|>` 不对称：右侧是单一调用/组合链，因此后续运算符作用于管道结果；`xs |> list.map(f) + ys` 是 `(xs |> list.map(f)) + ys`，而 `xs |> f >> g` 传入组合 `f >> g`。

<a id="ufcs"></a>

## UFCS
方法语法将接收者作为第 1 个实参，例如 `xs.map(f)` 解析为 `list.map(xs, f)`，普通用户函数 `fn twice(n: Int) -> Int` 无需属性即可调用为 `n.twice()`。这不表示每个 `f(x, y)` 与 `x.f(y)` 都无条件等价：按函数值记录字段、接收者的标准库模块、约定/协议、普通用户函数、类型定义模块的顺序解析。参见 [UFCS 解析](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-frontend/src/check/calls_ufcs.rs)。

<a id="standard-library-modules"></a>

## 标准库模块

当前函数签名与数量：[生成的标准库参考](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)。下表是导航指南，不是第二份 API 清单。

| 模块 | 描述 | 导入 |
| --- | --- | --- |
| [string](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/string.md) | 字符串操作 | 自动导入 |
| [list](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/list.md) | 列表操作 | 自动导入 |
| [map](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/map.md) | Map（字典）操作 | 自动导入 |
| [set](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/set.md) | 集合操作 | 自动导入 |
| [deque](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/deque.md) | 双端队列，两端 push/pop | 自动导入 |
| [heap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/heap.md) | 优先队列，最小优先 | 自动导入 |
| [int](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int.md) | 整数算术与位运算 | 自动导入 |
| [float](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float.md) | 浮点操作 | 自动导入 |
| [bool](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bool.md) | 解析、比较和打印布尔值 | 自动导入 |
| [value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/value.md) | 动态值操作 | 自动导入 |
| [result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md) | Result 类型操作 | 自动导入 |
| [option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/option.md) | Option 类型操作 | 自动导入 |
| [json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/json.md) | JSON 解析与查询 | `import json` |
| [math](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/math.md) | 数学函数 | 自动导入 |
| [regex](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/regex.md) | 编译后的 `Regex` 上的正则表达式（`regex.new(pat)` / `regex.compile(pat)`） | `import regex` |
| [datetime](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/datetime.md) | 日期与时间 | 自动导入 |
| [compute](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/compute.md) | 确定性计算时间构造器（fan 预算） | 自动（检查器表面） |
| [duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/duration.md) | 墙钟时间构造器 | 自动（检查器表面） |
| [bytes](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bytes.md) | 二进制数据 | 自动导入 |
| [matrix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/matrix.md) | 二维矩阵操作 | 自动导入 |
| [testing](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/testing.md) | 测试断言 | `import testing` |
| [fs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/fs.md) | 文件系统 | `import fs` |
| [env](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/env.md) | 环境与系统 | `import env` |
| [process](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md) | 子进程（`process.ProcessCommand` + `output` / `run` / `start`）、退出、信号 | `import process` |
| [io](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md) | 标准流，任意句柄上的 `read` / `write` / `close` | `import io` |
| [net](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/net.md) | TCP、UDP、名称查找、IP 地址 | `import net` |
| [unix](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/unix.md) | Unix 域套接字、描述符传递、共享内存 | `import unix` |
| [uuid](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uuid.md) | RFC 9562 UUID：v4、v7、解析、比较 | `import uuid` |
| [http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/http.md) | HTTP 客户端与服务器；`HttpRequest` 和 `HttpResponse` 共用 `header` / `headers` / `body` / `set_header` / `set_body` | `import http` |
| [random](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/random.md) | 随机数生成；秘密使用 `random.bytes` | `import random` |

<a id="regex-compile-once-pass-the-regex-first"></a>

### 正则表达式：编译一次，先传 `Regex`

每个 `regex` 操作都接受 `Regex`，而非模式字符串。`regex.new` 接受字面量模式（由 `almide check` 检查，E102；被拒绝时中止）；`regex.compile` 接受运行时构建的模式并返回 `err(reason)`。未参与匹配的捕获组为 `none`。

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

## JSON 与传输格式

带类型的 Codec 是所有 JSON 工作的默认路径。动态 `json.*` API（29 个函数）仅用于探索和无模式透传；若知道形态，就声明类型。

**规则 1——传输类型使用传输名称。** 描述外部 JSON 格式的类型逐字复现其字段名，即便违反 Almide 的 snake_case 约定。类型就是传输格式的文档，没有可被记错的重命名层：

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
需要 Almide 风格名称的领域逻辑定义自己的类型，并用普通代码映射（构造器调用，检查器验证每个字段）。字段别名 `name as "wire": T` 仅用于不能成为 Almide 标识符的键（`@type`、`$ref`、`user-id`）。

**规则 2——none 省略键。** 编码完全丢弃 `none` Option 字段（不写 `null`）；解码将缺失键或显式 `null` 还原为 `none`。始终有 `decode(encode(x)) == x`。带 `= default` 值的字段仍输出。默认值支持标量（`Int`/`Float`/`Bool`/`String`）、`Option[T]` 和 `List[<scalar>]` 字段；其他类型的默认值目前在 native 构建时拒绝（#1520 扩展了列表家族）。这与 proto3 JSON 一致，所以 protobuf 定义的 API（OTLP 等）可直接使用，proto3 `oneof` 也只是全部为 Option 的镜像类型：

```almide
type AnyValue: Codec = {
  stringValue: Option[String] = none,
  intValue: Option[String] = none,    // proto3 JSON: int64 rides as string
  boolValue: Option[Bool] = none,
}
AnyValue { stringValue: some("hi") }  // → {"stringValue":"hi"} — exactly one key
```

**规则 3——`Value` 是逃生通道。** `Value` 字段在双向均原样透传（嵌套文档、显式 null）。`Option[Value]` 是缺失和 null 不同的唯一位置：缺失 → `none`，显式 `null` → `some(value.null())`；用于 RFC-7386 风格补丁语义。

**规则 4——外部变体形态使用手写 codec。** 派生变体形式采用外部标签（`{"Click": {...}}`）。对于内部标签 API（`{"type": "click", ...}`），每个分支提供自己的 Codec 记录，只手写分派；约定方法会自动与所有派生 Codec 组合：

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
手写 codec 始终配套往返测试：`test "Event roundtrips" { assert_eq(Event.decode(Event.encode(C(Click{x:1,y:2}))), ok(C(Click{x:1,y:2}))) }`

**规则 5——请求解码到输入类型（`: Decode`），绝不解码到同时用于发送或存储的类型（`: Codec`）。** `Codec` 是 `Encode + Decode`：`: Decode` 仅派生 `T.decode` 和 `T.decode_strict`，`: Encode` 仅派生 `T.encode`。仅 `: Decode` 的类型是输入类型，每个字段都是输入，因而精确列出客户端允许设置的内容。`http.decode_json[T](req)` 只接受输入类型，记录或变体均可（`Codec` 类型为 E110，因此 `{"is_admin": true}` 永远无法进入 `User`）；它严格解码（任意深度的 `T` 之外的键都是 `400 Bad Request: unknown key '<k>'`，带路径如 `…, at address`），处理器在代码中构建领域值。输入类型中嵌套的记录也是输入类型。其他地方 `T.decode` 仍宽松（忽略未知键），`T.decode_strict` 则在每一层拒绝它们：

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
`@skip` 同时从 `decode` 和 `encode` 排除字段。字段属性严格限定为 `@skip`、`@skip_decode`（以及 WGSL 的 `@builtin`/`@location`）；serde 或 pydantic 风格写法（`@serde(skip)`、`@json_ignore`、`@redact`）是错误 E053，绝不静默忽略。

**规则 6——响应的 Content-Type 就是正文类型。** `http.response(status, body)` 接受 `Body`：String 为 `text/plain; charset=utf-8`，Bytes 为 `application/octet-stream`，`Value` 为 `application/json`，`html.*` 构建 `text/html`。HTML 使用 `html.escape`（文本）、`html.attr` / `html.url_attr`（属性）和 `html.script_json`（`<script>` 内数据）构建；`html.raw(s)` 仅用于可信标记。其他媒体类型使用 `http.typed("text/csv", bytes)`，类型以字面量书写（计算类型或 HTML / XML / SVG 为 E117；SVG 来自 almide/svg 等包，应用需在 `almide.toml` 中通过 `[trust]
media = { "almide/svg" = ["image/svg+xml"] }` 授权，否则 E118）。Content-Type 绝不按名称写入：`set_header` / `with_headers` 拒绝它（E116），`set_body` / `http.with_body(resp, body)` 同时替换正文与类型。获取的响应保留到达时的类型：

```almide check
import html
import http

fn greet(req: HttpRequest) -> HttpResponse = {
  let name = http.param(req, "name") ?? "guest"
  http.response(200, html.escape("Hello, ${name}"))
}

fn report(rows: String) -> HttpResponse = http.with_headers(200, http.typed("text/csv", bytes.from_string(rows)), [("X-Rows", "1")])
```

Codec 字段类型包括 `String`/`Int`/`Float`/`Bool`/`Value`、其他 Codec 类型，以及在其上任意嵌套的 `Option`/`List`（`List[Option[Int]]` 保留元素 null；`Option[List[T]]` 在 none 时省略）。声明时拒绝：`Option[Option[T]]`（线上无法区分）、非字段位置的 `Option[Value]`，以及非传输叶子类型（Map、Bytes、元组、定长整数、函数类型、Result）；把它们包装为具名 Codec 类型或在边界转换。

**秘密：`Secret[A]`（`import secret`）。** 除非类型声明 repr，每个值都结构化打印，所以普通 `String` 字段中的密码会出现在 `"${user}"`、`assert_eq` 失败和 `almide test` 报告中。将它存为 `Secret[String]`：每个渲染器都打印 `Secret(***)`，`secret.reveal(s)` 是唯一出口，`Secret[String]` / `Secret[Bytes]` 的 `==` 为恒定时间（其他 `Secret` 无 `==`，E103），带 Secret 字段的记录只读不写：声明为 `: Decode`（`: Codec` 也仅派生 `decode`，`T.encode` 为 E023），并使用 `secret.reveal` 手动构造发出的 Value。环境中的令牌生来就是 Secret：`env.secret(name)` 返回 `Option[Secret[String]]`。

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

## 验证 webhook 签名（HMAC）

使用 `import hash` 和 `import secret`。密钥和标签均为 `Secret[Bytes]`；通过返回 `Option[Secret[String]]` 的 `env.secret(name)!` 读取密钥。在将揭示的字符串转换为密钥字节前处理缺失。使用 `is_valid_` 项验证，密钥在前：它计算 HMAC-SHA256 并恒定时间比较。标签自己的 `==` 也是恒定时间（它是 `Secret[Bytes]`），所以绝不能对两个标签 `secret.reveal` 后比较，也不要用 `hash.sha256(key + body)`（可被长度扩展伪造）。头部的十六进制经 `hex.decode`；错误十六进制变为空标签，验证为 `false`，绝不中止。签名时，头值为 `hex.encode(secret.reveal(hash.hmac_sha256(key, body)))`。

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

自行比较两个令牌时，将两者持有为 `Secret[String]`（`env.secret`、`secret.new`），通过恒定时间的 `==` 比较（C-426）。标准库没有 `hash.md5` / `hash.sha1`，它们在 dialect epoch 15 已移除；使用 `hash.sha256`。

<a id="key-rules"></a>

## 关键规则
- 换行 = 语句分隔（无需分号）
- 泛型使用 `[]`，不是 `<>`
- `<` `>` 始终是比较运算符
- 副作用使用 `effect fn`，不是 `fn name!()`
- 每个 effect 调用写为 `f()!`，像 Swift `await` 那样阅读；它是 Swift `try`，绝非 `try!`
- 谓词函数返回 `Bool`（无特殊后缀）
- 无异常，所有地方使用 `Result[T, E]`
- 无 null，使用 `Option[T]`
- 无继承，使用组合
- 无宏或用户定义运算符声明；内置运算符分派由类型/协议驱动。没有隐式数值转换
- 空列表 = `[]`，空 map = `[:]`（带类型注解）
- `_` 是模式和被忽略绑定/参数中的通配符或丢弃，也可为表达式位置的带类型空洞；不是普通变量名
- 上方模块表只供导航；完整公开 API 见[生成的标准库参考](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)
- 迭代使用 `for x in xs { ... }`
- **没有嵌套函数。** 所有 `fn` 必须在顶层。局部辅助函数用 lambda
- **没有 `let mut`。** 可变绑定用 `var`。`mut` 本身是真实关键字，但仅作参数修饰符（`fn f(mut x: Int)`），见“函数”
- Almide 不是 Rust。没有 `&`、`trait`（用 `protocol`）或 `impl`（用约定方法：`fn Type.method(...)`）。可见性用 `local` / `mod`；`pub` 是默认公开函数和绑定的可选显式同义词，不适用于类型

<a id="naming-conventions-across-stdlib"></a>

## 标准库命名约定

- `bytes`：带类型的字节 IO 是一个矩阵：`get_<t>(b, pos) -> T?` 按位置读取（越界为 none）；`next_<t>(b, pos) -> (T?, next_pos)` 读取并推进（未读到内容则位置不变）；`put_<t>(b, pos, x)` 原地覆写（窗口超出缓冲区则中止）；`append_<t>(b, x)` 扩展；批量使用 `get_<t>_array(b, pos, count) -> List[T]?`。每个 2+ 字节类型都带 `_le` / `_be`；`<t>` ∈ `u8|i8|u16|i16|u32|i32|i64|f16|f32|f64|bool`（无 u64，`get_i64_*` 读取相同位）。`bytes.new()` 为空，`bytes.zeros(n)` 是 n 个零字节。
- `matrix` 包含通用稠密操作；神经网络内核（`gelu`、`softmax_rows`、attention、RoPE、norm、Q1_0/Q8_0、Qwen3 块）自 dialect epoch 14 起属于 `almide-graphics/nn` 包的 `nn.kernels` 模块，OpenAI / Anthropic 流式客户端属于 `almide/llm` 包（`import nn.kernels`、`kernels.gelu(m)`）。
- `matrix`：行形态操作用 `_rows` 后缀（`slice_rows`、`gather_rows`）；消耗/产生*一行*的操作用单数 `_row`（`dot_row`、`broadcast_add_row`）；列操作用 `_cols`（`split_cols_even`、`concat_cols`）。
- `from_X` 从其他表示构建，`to_X` 是其逆操作（例如 `from_bytes_f64_le` ↔ `to_bytes_f64_le`）。
- 数值转换以构建出的类型命名（`int`、`float`、`int8`…`int64`、`uint8`…`uint64`、`float32`、`float64`）：`T.from(x)` 接受任意数值 `x`，值不属于 `T` 时中止（Float 先向零截断）；`T.from_checked(x)` → `Option`（除非 `T` 精确容纳 x，否则为 `none`）；`T.from_saturating(x)` 钳制（NaN 为 0）；`T.from_wrapping(x)` 保留整数低位（二补码）。例如 `float.from(n)`、`int.from_saturating(f)`、`uint8.from_wrapping(n)`。每种整数承载类型都有 `min_value()`/`max_value()`。

<a id="common-mistakes-do-not"></a>

## 常见错误（不要这样做）
- `list[1, 2, 3]` → **错误**。写 `[1, 2, 3]`。`list` 是模块，不是类型构造器
- `map(xs, f)` → **错误**。写 `list.map(xs, f)`。所有标准库函数都需要模块前缀
- `list.each(xs, f)` / `list.for_each(xs, f)` → **错误**。没有普通 `each`；逐元素执行副作用用 `for x in xs { ... }`
- 将 `map[K, V]` 作为值 → **错误**。用带类型注解的 `[:]` 创建空 map
- `List.new()` → **错误**。写 `[]`。List 没有 `new()`
- 将 `{"a": 1}` 作为 map → **错误**。写 `["a": 1]`。花括号 `{}` 用于记录/块，方括号 `[]` 用于列表和 map
- `string.length(s)` → **错误**。写 `string.len(s)`。没有同义词
- `list.length(xs)` → **错误**。写 `list.len(xs)`。没有同义词
- `matrix.row_dot(m, r, v)` / `matrix.concat_cols_many(ms)` → **错误**。写 `matrix.dot_row(m, r, v)` / `matrix.concat_cols(ms)`。没有同义词
- `string.to_lowercase(s)` → **错误**。写 `string.to_lower(s)`。没有同义词
- `string.to_uppercase(s)` → **错误**。写 `string.to_upper(s)`。没有同义词
- `string.substring(s, i, j)` → **错误**。写 `string.slice(s, i, j)`。没有同义词
- x 为 Int 时 `println(x)` → **错误**。写 `println(int.to_string(x))`。没有隐式转换
- `1 :: 2 :: []` → **错误**。写 `[1, 2]`。没有 cons 运算符 `::`
- `fn foo<T>(x: T)` → **错误**。写 `fn foo[T](x: T)`。泛型用 `[]`，不是 `<>`
- `let mut x = 1` → **错误**。写 `var x = 1`。`mut` 仅是参数修饰符（`fn f(mut x: Int)`），不是绑定修饰符
- `let err = f()` → **错误**。`ok`/`err`/`some`/`none`（及首字母大写形式）是内置 Result/Option 构造器，属于保留字。重命名绑定（`let msg = f()`），或用反引号转义保留名称（`let `err` = f()`）
- 函数内部嵌套 `fn` → **错误**。所有 `fn` 必须位于顶层。局部函数用 `let helper = (x) => ...`
- 含 `...` 的 `match x { ... pattern => expr }` → **错误**。模式中没有展开
- 在循环中用 `random.int(0, 255)` 生成令牌、密钥或密码 → **错误**。`random.int` / `float` / `choice` / `shuffle` 不用于秘密。秘密使用 OS 密码学随机源 `random.bytes(n)`
- effect 调用写 `try f()` / `try! f()` / `f()?` → **错误**。写 `f()!`。Almide 的 `!` 是 Swift `try`（传播），绝非 `try!`（陷阱）；`?` 将 Result 转为 Option
- `async fn` / `await` → **错误**。Almide 没有 async/await。使用 `fan { }` / `fan.map` / `fan.settle` / `fan.min(clock)` / `fan.bounded` / `fan.timeout`
- `fan.race { … }` → **错误**（dialect epoch 14 改名，E027）。写 `fan.min(compute.ms(5)) { … }`，时钟实参必需，说明“最少”按什么衡量
- `fan.any { a(), b() }` / `fan.any(xs, f)` / `fan.first_ok` → **错误**（dialect epoch 14 移除，E027）。按顺序取第一个 Ok 是顺序执行，不是 fan；写 `a() ?? b() ?? fallback`，列表则用 `list.find_map(xs, (x) => f(x)?) ?? fallback`
- `fan.min { a(), b() }` / `fan.min(xs, f)` → **错误**。时钟实参必需：`fan.min(compute.ms(5)) { a(), b() }` / `fan.min(compute.ms(5), xs, f)`
- `fan.settle([...])` → **错误**。thunk 列表形式已移除，写 `fan.settle { a(), b() }`
- `fan.bounded(100) {...}` / `fan.min(5000) {...}` → **错误**。裸 Int 不是时间，写 `compute.ms(100)`
- `compute.msec(5)` / `compute.sec(5)` / `compute.m(5)` → **错误**。单位集合封闭：`ns / us / ms / s / min / h`
- 将 `100ms` / `5s` 作为字面量 → **错误**。没有时间字面量，写 `compute.ms(100)` / `duration.s(5)`
- `let list = [1, 2]` 后写 `list.len(list)` → **错误**。局部绑定在成员位置同样遮蔽同名模块，因此这里解析为局部值的 `len` 方法，接收者为第一个实参，你写的为第二个（E004）。不要用仍需使用的模块名命名绑定，容易误写的有 `list`、`string`、`map`、`set`。两种修复都接受：重命名绑定（`let xs = [1, 2]`，然后 `list.len(xs)`），或去掉实参，调用局部值的方法。`list.len()` 打印 `2`，因为被遮蔽的名称就是局部值。自动导入模块被静默遮蔽：没有 import 语句可报告为未使用，所以没有警告
- `io.read_line_opt()` → **错误**（已移除）。`io.read_line()` 本身返回 `String?`，输入结束为 `none`
- `io.read_byte()` / `io.read_n_bytes(n)` / `io.write_bytes(xs)` / `io.write(data)` → **错误**（已移除）。字节走句柄：`io.read(io.stdin()!, 1)!`、`io.read_exact(io.stdin()!, n)!`、`io.write(io.stdout()!, data)!`（`data: Bytes`）
- `net.tcp_read(s, n)` / `net.tcp_read_exact` / `net.tcp_write` / `net.tcp_close` / `net.tcp_close_listener` → **错误**（已移除）。`net.TcpStream` 是 io 句柄：`io.read(s, n)!`、`io.read_exact(s, n)!`、`io.write(s, data)!`、`io.close(s)!`；除 `import net` 外还需 `import io`
- `net.unix_connect` / `net.unix_send` / `net.shm_create` / … → **错误**（已迁移）。使用 `import unix`：`unix.connect`、`unix.write_fds`、`unix.select`（原 `unix_wait`）、`unix.shm_create`；字节通过 `unix.Fd` 上的 `io.read` / `io.write` / `io.close`
- `process.exec_status` / `exec_status_timeout` / `exec_in` / `exec_with_stdin` → **错误**（已移除）。统一为一个记录：`process.output(process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo", stdin: "", timeout_ms: 5000 })!` → `{code, stdout, stderr}`；写出类型名即可省略 `dir` / `stdin` / `timeout_ms`
- `process.run(cmd, args)` / `process.run_in(dir, cmd, args)` / `process.exec_attached` → **错误**。`process.run(process.ProcessCommand { cmd: "cargo", args: ["build"] })!` 返回退出码
- 后台子进程：`let c = process.start(process.ProcessCommand { cmd, args })!`，然后 `process.poll(c)!`（运行中为 `none`）、`process.wait(c)!`、`process.stop(c, 500)!`、`process.child_pid(c)!`
- 用 `process.exec("sh", ["-c", "git status | head"])` 获取 shell → 避免。程序接受 argv 列表：`process.exec("git", ["status"])!`，管道在 Almide 中完成

<a id="complete-example"></a>

## 完整示例
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

## 来自其他语言的常见错误

以下是基于 Rust/Python/JS/Scala 训练的 LLM 生成 Almide 时最常见的错误。

<a id="-if-cond-----if-cond-then--else-"></a>

### ✗ `if cond { }` → ✓ `if cond then ... else ...`
```
✗ if x > 0 { "positive" } else { "negative" }
✓ if x > 0 then "positive" else "negative"
```

<a id="-x--expr---x--expr"></a>

### ✗ `x => expr` → ✓ `(x) => expr`
Lambda 参数**必须**放在圆括号中。
```
✗ list.map(xs, x => x + 1)
✓ list.map(xs, (x) => x + 1)
```

<a id="-modulefunc---modulefunc"></a>

### ✗ `module::func()` → ✓ `module.func()`
Almide 使用 `.` 访问模块，不是 `::`。
```
✗ list::len(xs)
✓ list.len(xs)
```

<a id="-listpushxs-item-for-value---xs--item"></a>

### ✗ 用 `list.push(xs, item)` 取得值 → ✓ `xs + [item]`
`list.push` 修改 `var` 并返回 `Unit`。不可变列表构建使用 `+`。
```
✗ some(list.push(stack, "("))     // returns Option[Unit]
✓ some(stack + ["("])             // returns Option[List[String]]
```

<a id="-let-x--e-in-body----let-x--e-body-"></a>

### ✗ `let x = e in body` → ✓ `{ let x = e; body }`
不支持 ML 风格 let-in，使用块。
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

### ✗ `Char` 类型 → ✓ `String`
Almide 没有 `Char` 类型，单个字符为 `String`：`"a"`、`"("`。

<a id="loop-control-break--continue"></a>

### 循环控制：`break` / `continue`
循环内均支持这两者。`break` 立即退出；`continue` 跳过当前迭代的剩余部分（范围循环仍推进索引）。
```
while cond {
  if done then break
  if skip then continue
  work()
}
```
使用递归辅助函数应因为它更清晰地表达算法，而非因为循环控制不可用。

<a id="-counted-recursion-fn---1---for-_-in-0n"></a>

### ✗ 计数递归 `f(n - 1)` → ✓ `for _ in 0..<n`
预先知道迭代次数时，写范围 `for`，用 `var` 传递状态。循环控制用 `break` / `continue`；对于递归数据或状态转换，若能更清晰地表达算法，递归仍有用。
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

### ✗ `return expr` → ✓ 直接 `expr`
块中最后一个表达式就是返回值，没有 `return` 关键字。
```
✗ fn f(x: Int) -> Int = { return x + 1 }
✓ fn f(x: Int) -> Int = x + 1
```

<a id="-reassigning-function-parameters---var-copy"></a>

### ✗ 重新赋值函数参数 → ✓ `var` 副本
普通函数参数不可变。局部状态复制到 `var`；仅当需要更新调用者的 `var` 绑定时才声明 `mut` 参数（见“可变参数”）。
```
✗ fn f(n: Int) -> Int = { n = n - 1; n }
✓ fn f(n: Int) -> Int = { var m = n; m = m - 1; m }
```
