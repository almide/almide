<a id="rejected-patterns"></a>

# 已拒绝的模式

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REJECTED_PATTERNS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REJECTED_PATTERNS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](REJECTED_PATTERNS.zh-CN.md)



> 原文：[docs/design/REJECTED_PATTERNS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/REJECTED_PATTERNS.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，日语原文）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 设计理由与历史比较（包括 LLM 能力、零开销、最优执行的说法）按原文主张保留，不是独立确立的保证。明确撤回或当前允许的条目仍保留该状态；本译文不将过去拒绝改成当前禁令。

> Matz：“我本应更仔细地考虑从 Perl 借来的那些功能。”
> TypeScript：为 enum 和 namespace 后悔了 10 年。
>
> 本列表中的功能是**有意不采用的**。再次提出前请先阅读这里。

---

<a id="構文"></a>

## 语法

<a id="-nullish-coalescing-演算子--撤回済み-adr-0005"></a>

### ~~`??`（nullish coalescing）运算符~~——**已撤回拒绝（ADR-0005）**

最初以“`unwrap_or` 提供相同功能，因此违反 Canonicity”为由拒绝。**ADR-0005 逆转了这一决定**：`??` 是规范形式，其脱糖定义（`a ?? b ≡ unwrap_or_else`）见 [docs/specs/result-option-effect.md §“运算符的定义”](../../../../specs/result-option-effect.md)。相反，计划删除的是 `unwrap_or` 函数（ADR-0005 D4，almide#1107）。保留此条作为拒绝历史——“三种写法”问题通过删除函数而非运算符解决。

<a id="return-キーワード"></a>

### `return` 关键字

**拒绝理由**：隐式的最后一个表达式就是函数返回值。不需要 `return`。
- `guard ... else <expr>` 以结构化方式表达提前退出。
- 多条 return 路径用 `if/then/else` 或 `match` 表达。

<a id="si-文字列インデックス"></a>

### `s[i]`（字符串索引）

**当前 API**：使用 `string.get(s, i)` 获取字符串中的字符。这并不意味着连现有运算符的 protocol dispatch 也禁止。

<a id="match-arm-の--省略--現行構文では許可"></a>

### 省略 match arm 的 `{}`——当前语法允许

旧的拒绝说明与当前 parser 不一致。arm body 是表达式，因此可以写成 `some(x) => x`。多语句块使用 `{ ... }`（`crates/almide-syntax/src/parser/compounds.rs::parse_match_arm`）。

<a id="ternary---演算子"></a>

### 三元 `? :` 运算符

**拒绝理由**：`if/then/else` 是表达式。不需要三元运算符，否则 LLM 必须记住两种条件表达式语法。

---

<a id="型システム"></a>

## 类型系统

### null / nil / undefined

**拒绝理由**：`Option[T]` 是唯一的“值可能不存在”表示。null 造成 Go 的 nil panic、JavaScript 的 undefined 困境。

<a id="implicit-type-conversion"></a>

### 隐式类型转换

**拒绝理由**：通过 `int.to_string(n)`、`int.from_saturating(x)` 显式转换。隐式转换会造成类似 Python 2→3 的 `/` 运算符（int→float）那样无提示的行为变化。

<a id="custom-operator-syntax--protocol-dispatch-とは区別"></a>

### 自定义运算符语法——区别于 protocol dispatch

没有让用户定义新运算符符号或语法的功能。不过，现有运算符可以按声明的 protocol 分派到 requirement method。例如，`Eq` 的 `==` 对应 `Type.eq`，`Numeric` 的 `+` 对应 `add`。旧的“全面禁止运算符重载”说明与当前实现不一致（`canonicalize/protocols.rs`、`spec/lang/operator_protocol_test.almd`、`spec/stdlib/bigint_operators_test.almd`）。

<a id="algebraic-effects-拡張-effect-system"></a>

### 代数效应（扩展 effect system）

**拒绝理由**：`effect fn` 仅作为 I/O/副作用标记。正如 Gleam 在没有 effect system 的情况下取得成功，简单二元分类（pure / effect）已足够。未采用表层语法 `effect[fs]` / `effect[http]`。另一方面，已实现经函数值传播的 capability category set 推断及 `[permissions].allow`（[capability system](../../../../wasm/capability-system.md)）。

<a id="higher-kinded-types-hkt"></a>

### 高阶种类类型（HKT）

**拒绝理由**：这是 Swift 的 PAT（Protocol with Associated Types）花了 7 年才改善易用性的复杂性。没有证据表明 LLM 能正确使用 HKT。

<a id="type-classes--implicit-instances"></a>

### 类型类 / 隐式实例

**拒绝理由**：Haskell 的 orphan instances、Scala 的 implicit resolution——需要全局搜索，不适合 LLM 的局部推理。

<a id="trait--interface-キーワード"></a>

### `trait` / `interface` 关键字

**拒绝理由**：Almide 采用 `protocol` 关键字。原因：
- `trait` 是 Rust 特有词汇。让人联想到 associated types、trait objects、orphan rules，使 LLM 带入 Rust 模式。
- `interface` 在 Go/Java/TS 中广泛使用，但其含义分裂为 Go 的隐式满足与 Java 的显式 `implements`。
- `protocol` 是 Swift/Python（PEP 544）的词汇。Almide 现有的 `: Convention` 语法（`type Dog: Eq`）与 Swift 的 `: Protocol` 语法一致。
- 是 2024–2026 年新语言倾向选择的词汇。

<a id="impl-ブロック"></a>

### `impl` 块

**拒绝理由**：Almide 通过 convention methods（`fn Type.method()`）实现协议。不需要 `impl Protocol for Type { ... }` 块：
- convention methods 是顶层函数——扁平、无嵌套。
- LLM 更易准确生成（不增加缩进深度）。
- 与现有 convention method 系统完全一致。

<a id="暗黙的プロトコル満足-go-式-structural-interfaces"></a>

### 隐式协议满足（Go 式结构化接口）

**拒绝理由**：Go 只要存在方法就隐式满足接口。Almide 要求显式声明 `type Foo: Protocol`：
- LLM 只阅读代码就能看出实现关系。
- 编译器能准确报告缺少的方法。
- 与现有 `: Eq, Repr` 语法一致。

<a id="dynamic-dispatch--protocol-objects"></a>

### 动态分派 / 协议对象

**拒绝理由**：Almide 的协议通过单态化解析。不需要 vtable 或动态分派：
- 编译时全部展开为具体类型。
- 运行时开销为零。
- 不需要类似 Go interface 值的装箱。

---

<a id="ランタイム並行処理"></a>

## 运行时与并发

<a id="mutable-by-default"></a>

### 默认可变

**拒绝理由**：`let` 不可变，`var` 可变。Ruby 的默认可变造成与 Ractor 不兼容。禁止在 `fan` 块内捕获 `var`。

<a id="async--await-キーワード"></a>

### async / await 关键字

**拒绝理由**：用 `effect fn` + `fan` 表达并发。async/await 带来：
- Python：函数着色问题导致生态分裂。
- Rust：Pin/Unpin、Send + 'static、运行时选择的复杂性。
- Swift：延迟 7 年才改装加入。

Almide 的 `fan` 只写“并发的意图”。编译器为各目标生成最优实现。

### goroutine / green thread / actor

**拒绝理由**：`fan` 的结构化并发已足够。避免 goroutine leak（Go）和 Ractor 不兼容（Ruby）。Supervision/Actor 暂缓，留待将来考虑。

<a id="futuret--promise-型の露出"></a>

### 暴露 `Future[T]` / `Promise` 类型

**拒绝理由**：不向用户暴露。`effect fn foo() -> Int` 的返回类型是 `Int`，不是 `Future[Int]`。避免 LLM 被 `Future[Future[Int]]` 搞混。

---

<a id="メタプログラミング"></a>

## 元编程

<a id="macros"></a>

### 宏

**拒绝理由**：Zig 的 comptime、Rust 的 proc macros——虽然强大，但 LLM 无法正确生成。编译器内部生成与用户编写的 macro 语法是不同的事。标准库当前定义在 `stdlib/*.almd` 中，而非旧的 TOML stdlib 定义。

### monkey patching / open classes

**拒绝理由**：Ruby 的 monkey patching 带来“developer happiness”，但对 LLM 而言，“方法在定义之外的地方被修改”是无法分析的行为。

<a id="reflection--runtime-type-inspection"></a>

### 反射 / 运行时类型检查

**拒绝理由**：通过静态类型在编译时全部解析。不需要运行时类型信息。

---

<a id="パッケージ設定"></a>

## 包与配置

<a id="複数の設定ファイル形式"></a>

### 多种配置文件格式

**拒绝理由**：`almide.toml` 是唯一配置。Python 从 `setup.py` → `setup.cfg` → `pyproject.toml` 花了 23 年。

<a id="実行可能な-build-script"></a>

### 可执行构建脚本

**拒绝理由**：`almide.toml` 仅采用声明式 TOML。像 Python `setup.py` 那样的可执行配置会破坏可复现性。

---

<a id="このリストの運用"></a>

## 本列表的维护

- **添加**：考虑某功能并拒绝时，将其连同理由加入这里。
- **删除**：仅在语言方向发生根本改变时（edition 变更级别）。
- **引用**：PR 或 issue 提出“想要此功能”时，指向这里。
