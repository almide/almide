<a id="als-e--expression-semantics-normative"></a>

# ALS §E — 表达式语义（规范性）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](expressions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](expressions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](expressions.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/expressions.md)的非规范性读者译本。下文的规范性地位与实现的符合义务属于正本原文，不属于本译文。这是固定提交的快照，不是生成流程输入，也不会自动同步。原文与机器使用方保持不变。示例、测量、跨目标一致性、符合性与证明主张未经独立执行或重新验证。

> 最后更新：2026-10-24

> **状态**：规范性。这些节是独立于实现的**规范**，v0（native）和 v1（MIR/wasm）两个实现都有义务符合这些节。符合性证据为 `spec/wasm_cross/` 中对应的测试夹具（三项观察：stdout、stderr、exit code）。本文件是**语法元素维度**（proofs/als-element-coverage.toml）的首组章节，各节采用“接受形式的语法”“值的规范”“裁定”“测试夹具”四部分模板。所有主张在制定时均已在两个目标上实测（以推测制定规范违反 F1）。

<a id="als-e1-整数リテラルexprkindint"></a>

## ALS-E1 整数字面量（`ExprKind::Int`）

**接受形式**：

```ebnf
int     := dec | hex | oct | bin
dec     := digit (digit | "_")*
hex     := "0x" hexdigit (hexdigit | "_")*
oct     := "0o" octdigit (octdigit | "_")*
bin     := "0b" bindigit (bindigit | "_")*
```

任何进制均接受下划线作为数字分隔符，不影响值。

**值的规范**：整数字面量的类型为 `Int` = **64 位有符号二进制补码整数**（i64）。可表示范围为 −9223372036854775808 至 9223372036854775807。

**范围裁定**：超出范围的字面量产生**检查期错误 E024**（`integer literal
'…' is out of range for Int`）；静默回绕或饱和均不符合规范。**紧接一元负号的字面量**连同符号一起检查范围，因此最小值 `-9223372036854775808` 可直接写成字面量（负号与数字之间允许空白）。没有此裁定，i64::MIN 就只能写成 `-9223372036854775807 - 1`，转抄常量表容易引入错误。

**显示往返**：`int.to_string` 在包括最小值的整个范围内返回十进制表示（包括 i64::MIN 的数字串 `-9223372036854775808`；v1 wasm 的 `$itoa` 已通过执行固定测试验证此边界）。

测试：`spec/wasm_cross/literal_int_forms.almd`（契约 C-231）。

<a id="als-e2-真偽リテラルexprkindbool"></a>

## ALS-E2 布尔字面量（`ExprKind::Bool`）

**接受形式**：关键字 `true` / `false`（保留字，不能用作标识符）。

**值的规范**：类型为 `Bool`。在字符串插值、`==` 的操作数等所有观察点，显示形式均为小写拼写 `true` / `false`。

测试：`spec/wasm_cross/literal_int_forms.almd`（契约 C-232）。

<a id="als-e4-ユニットリテラルexprkindunit"></a>

## ALS-E4 单元字面量（`ExprKind::Unit`）

**接受形式**：`()`（一对空圆括号）。

**值的规范**：类型为 `Unit`。`Unit` 恰好只有一个值，`()` 是其唯一的字面量表示。可用于表达式位置、`let` 绑定位置及 `-> Unit` 函数体；作为块的尾表达式时成为函数返回值。

**相等性裁定**：`Unit` 上的 `==` **自反为真**（`() == ()` → `true`，`!=` → `false`）。只有一个成员的类型不存在运行时读取，因此 v1 降低处理**仅对不含调用的操作数对**将比较折叠为常量 Bool。含调用的操作数（`f() == ()`）不折叠：消除调用效果属于错误编译，故会触及现有拒绝边界（wall）而被拒绝（诚实地不支持 > 静默给出错误答案）。此前 Eq-over-Unit 在 v1 的所有位置均有 wall，相等性证据仅限 native；现已在两个目标上有执行固定测试。

测试：`spec/wasm_cross/literal_unit.almd`（契约 C-233）、`spec/lang/unit_literal_test.almd`（assert 形式，在 wasm 分支执行）。

> **ALS-E3（浮点字面量）的旧“尚未撰写”备注（后文已有部分章节）**：测试夹具无法通过 `almide fmt` 的 spec 门禁（fmt 经由 f64 值规范化指数及下划线拼写 — #1261）。在作出裁定且 fmt 保留拼写之前，proofs/als-element-coverage.toml 的 `ExprKind::Float` 行仍维持 UNWRITTEN（F1：没有测试夹具就不制定规范）。

<a id="als-e6-括弧式exprkindparen"></a>

## ALS-E6 圆括号表达式（`ExprKind::Paren`）

**接受形式**：`( expr )`。

**值的规范**：圆括号表达式的值、类型和效果与内部表达式相同；圆括号除覆盖结合优先级外没有其他含义。`(1 + 2) * 3` 为 `9`，`1 + 2 * 3` 为 `7`（两个目标均已实测）。fmt **保留**冗余圆括号（不擅自剥除）。

**与单元素元组的区分裁定**：`(e)` 是圆括号表达式，`(e,)` 是**单元素元组**；末尾逗号承载语义。fmt 在往返中保留单元素元组的末尾逗号（#1265 修复；回归测试 `fmt_one_tuple_keeps_trailing_comma`）。已知限制：native 后端在字符串插值与单元素元组共存时无法降低 `.0`（#1267，明确拒绝，不产生错误值）。

测试：`spec/wasm_cross/grouping_unary.almd`、`spec/wasm_cross/tuple_single.almd`（均为契约 C-234）。

<a id="als-e7-単項演算子exprkindunary"></a>

## ALS-E7 一元运算符（`ExprKind::Unary`）

**接受形式**：逻辑否定为关键字 `not`（`not b`，可重复使用 `not not b`）。算术取负为前缀 `-`（适用于字面量、变量、圆括号表达式和浮点数）。

**`!` 的裁定**：**不接受**前缀 `!`；编译错误以 `Use 'not' for boolean negation` 引导用户（后缀 `!` 是 unwrap，ADR-0008）。表达式语句位置与插值 `${...}` 内的词法路径不同，但给出相同引导（`tests/unary_not_diag_test.rs` 固定两个位置）。

**值的规范**：`not true` → `false`，`not not b` ≡ `b`。字面量前紧邻的 `-` 连同符号纳入范围检查（ALS-E1 的裁定）。`-x`（变量）、`-(3 + 4)`（圆括号表达式）→ `-7`，`-(1.5)` → `-1.5`；两个目标均相同。

测试：`spec/wasm_cross/grouping_unary.almd`（契约 C-235）、`tests/unary_not_diag_test.rs`（反例）。

<a id="als-e8-タプルexprkindtuple--exprkindtupleindex"></a>

## ALS-E8 元组（`ExprKind::Tuple` / `ExprKind::TupleIndex`）

**接受形式**：字面量 `(e1, e2, …)`（元素类型可不同）。单元素元组必须有末尾逗号 `(e,)`（ALS-E6 的区分）。按位置读取为 `.k`（k 是从 0 开始的十进制数字串）。解构绑定为 `let (x, y) = t`。`(A, B)` 可作为函数参数类型和返回类型。

**链式索引裁定**：**不接受** `n.0.1`；词法分析将 `0.1` 读为浮点字面量。0.59.1 用专用诊断说明此事（“`.0.1` 被词法分析为浮点字面量 '0.1'，而不是两次投影”；提示要求按 `(x.0).1` 加括号）。嵌套读取应写成 `(n.0).1`（实测固定测试）。

**值的规范**：元组是每个位置各有类型的定长值。`==` 为逐元素结构相等。无论通过解构绑定、`.k`、传参还是返回值，两个目标的观察值均相同。

**诊断更新**：`.k` 越界或对象不是元组时，产生检查期 E045。旧 #1266 中“仅在 build 阶段拒绝”的说明不适用于现状（`tests/tupleindex_diag_test.rs`、`crates/almide-frontend/src/check/infer_members.rs` 中的元组索引检查）。以下 native 单元素元组限制是历史记录，尚未重新验证是否仍为当前限制：native 后端在与字符串插值共存时无法降低单元素元组的 `.0`（#1267）。

测试：`spec/wasm_cross/tuple_ops.almd`（契约 C-236）。

<a id="als-e9-optionresult-コンストラクタexprkindsome--exprkindnone--exprkindok--exprkinderr"></a>

## ALS-E9 Option/Result 构造器（`ExprKind::Some` / `ExprKind::None` / `ExprKind::Ok` / `ExprKind::Err`）

**接受形式**：`some(e)`、`none`（**裸值**，调用 `none()` 产生检查期 E001）、`ok(e)`、`err(e)`。均为小写拼写。

**值的规范**：`some(e)` 构造 `Option[T]`，元素类型从 `e` 推断。`none` 的类型来自注解（`let n: Int? = none`）或消费位置（`n ?? 7`）。`ok(e)` / `err(e)` 在期待 Result 类型的上下文（带注解的绑定、返回位置、直接消费）构造 Result。

**ok/err 上下文裁定**：在 effect fn 中，**把 `ok(e)` / `err(e)` 绑定到无注解的 `let` 会在检查期被拒绝**（E041，ADR-0008 的显式传播规则；提示引导至 `!`、`??`、`?`、match）。不存在让 Result 隐式直接通过的路径。反例 `tests/ctor_diag_test.rs` 固定这两项裁定。

**消费**：`??` 在 Ok/Some 侧返回内部值，在 Err/None 侧返回后备值（`ok(3) ?? 0` → `3`，`err("boom") ?? -9` → `-9`，两个目标均已实测）。完整的传播与分支规范见 R 系列（错误层面）。

**嵌套规范**：`some(none)` / `some(some(v))` 是普通 Option 值，可用带 Option 类型默认值的 `??` 逐层剥离（`sn ?? none` → 内层 Option；实测 none 侧 → 9、some 侧 → 5，两个目标相同；#1270 关闭了 v1 的 wall）。

测试：`spec/wasm_cross/option_result_ctors.almd`（契约 C-237）、`tests/ctor_diag_test.rs`（反例）。

<a id="als-e10-レンジ式exprkindrange"></a>

## ALS-E10 范围表达式（`ExprKind::Range`）

**接受形式**：不含终点 `start ..< end`、包含终点 `start ... end`。边界是 Int 标量表达式（字面量、变量、调用）。

**退役拼写裁定**：`..` 与 `..=` **已退役**；检查期 E031 引导至现行拼写，`almide fix` 进行机械迁移。历史上 `..` 不含终点，`..=` 包含终点，静默改读无论哪个方向都会产生差一错误；只有拒绝才安全（反例 `tests/range_spelling_diag_test.rs` 固定两种拼写）。

**值的规范**：范围是一等值；可先绑定 `let r = 0..<3`，再通过 `for i in r` 迭代，产生与内联头部 `for i in 0..<3` 相同的序列。仅用作 for-in 头部的绑定**不会实体化**（#1400、C-238，由 `range_bind_no_materialize.almd` / `range_bind_huge.almd`（2^32 次迭代，不实体化 32 GB）/ `range_materialize_oom.almd` 固定；#1272 的延迟空值、0 次迭代是此前的回归固定测试）。其他用法会实体化为实际列表。不含终点的 `0..<3` → 0,1,2，包含终点的 `0...5` → 0–5；非零起点和调用边界在两个目标上也相同（测试夹具固定总和 3 / 15 / 9 / 6）。

测试：`spec/wasm_cross/range_first_class.almd`（契约 C-238）、`tests/range_spelling_diag_test.rs`（反例）。

<a id="als-e11-リストリテラルと索引exprkindlist--exprkindindexaccess"></a>

## ALS-E11 列表字面量与索引（`ExprKind::List` / `ExprKind::IndexAccess`）

**接受形式**：字面量 `[e1, e2, …]`、空列表 `[]`（元素类型来自注解或上下文），可嵌套。读取为 `xs[i]`（从 0 开始）。拼接为 `+`（ALS 全局的运算符重载：字符串与列表的 `+` 均为拼接）。

**值的规范**：`xs[i]` 直接返回元素类型的值（不是 Option）。**越界会在运行时中止**；两个目标都输出统一消息 `Error: index out of bounds` + exit 1，三个观察点（stdout、stderr、退出码）一致（与写入侧现有契约 C-067 的裁定相同；静默补 0 或忽略均不符合规范）。`+` 返回将左右元素依次排列的新列表。

测试：`spec/wasm_cross/collection_literals.almd`（契约 C-239）；越界中止为 `spec/wasm_cross/index_bounds_write_heap.almd`（C-067）。

<a id="als-e12-マップリテラルexprkindmapliteral--exprkindemptymap"></a>

## ALS-E12 映射字面量（`ExprKind::MapLiteral` / `ExprKind::EmptyMap`）

**接受形式**：`["k1": v1, "k2": v2]`，即**方括号 + 冒号**。空映射为 `[:]`（类型来自注解）。**不接受**花括号 `{"k": v}`（解析错误；从 JSON 或其他语言转抄的错误在检查时被阻止）。

**值的规范**：`m[k]` 返回 `Option[V]`（与列表的直接读取相对），用 `??` 提供默认值。键不存在时为 `none`（实测 `m["zz"] ?? -1` → `-1`）。插入顺序确定地保留（AlmideMap 的规范；迭代顺序即插入顺序）。

测试：`spec/wasm_cross/collection_literals.almd`（契约 C-240）。

<a id="als-st1-束縛文stmtlet--stmtvar--stmtassign"></a>

## ALS-ST1 绑定语句（`Stmt::Let` / `Stmt::Var` / `Stmt::Assign`）

**接受形式**：不可变绑定 `let x = e`（可选类型注解 `let x: T = e`）、可变绑定 `var x = e`、重新赋值 `x = e`（仅限 `var`）。

**遮蔽裁定**：**接受** `let` 同名重新绑定；新绑定的初始化表达式可见旧绑定（`let x = 1` 之后的 `let x = x + 10` 使 `x` 为 11）。这既不是错误也不是警告（逐步精化值是惯用法）。

**不可变性裁定**：给 `let` 赋值产生**检查期 E009**，提示引导至 `var` 声明。`var` 的重新赋值固定为声明时的类型，赋入其他类型产生 **E001**（无静默拓宽）。反例由 `tests/binding_diag_test.rs` 固定。

**值的规范**：对 `var` 的赋值可观察地跨越控制流（循环体、分支）生效（测试夹具：连续两次赋值 → 10，循环累加 → 6）。`let` 的初始化表达式也可为条件表达式（`let y = if … then … else …`）。

测试：`spec/wasm_cross/binding_stmts.almd`（契约 C-241）、`tests/binding_diag_test.rs`（反例）。

<a id="als-e13-条件式exprkindif"></a>

## ALS-E13 条件表达式（`ExprKind::If`）

**接受形式**：`if cond then arm`（语句位置可省略 else）、`if cond then arm
else arm`；分支为单个表达式或花括号块 `then { … } else { … }`；可连用 `else if`。条件为 `Bool`。

**`then` 的裁定**：`then` **必需**；从花括号语言转抄的 `if cond { … }` 是解析错误，以 `if requires 'then'` 引导（反例由 `tests/if_then_diag_test.rs` 固定）。

**值的规范**：条件表达式有值（`let g = if … then "low" else if … then
"mid" else "high"`）。省略 else 的形式仅允许在语句位置，值为 `Unit`。只有选中的分支触发效果（若观察到未选分支的效果，即不符合规范；v1 降低处理针对双分支线性化的 wall 保护此规范）。

测试：`spec/wasm_cross/if_block_forms.almd`（契约 C-242）、`tests/if_then_diag_test.rs`（反例）。

<a id="als-e14-ブロック式exprkindblock"></a>

## ALS-E14 块表达式（`ExprKind::Block`）

**接受形式**：`{ stmt* tail? }`，即语句序列加可选尾表达式。

**值的规范**：块的值为**尾表达式的值**（没有则为 `Unit`）。块是表达式，可用于 `let` 初始化、`if` 分支或函数体。块内绑定具有块作用域（测试夹具：`let b = { let t = a * 2
t + 1 }` → 11，if 分支内的块 → 9，两个目标相同）。

测试：`spec/wasm_cross/if_block_forms.almd`（契约 C-243）。

<a id="als-e15-while-文exprkindwhile"></a>

## ALS-E15 while 语句（`ExprKind::While`）

**接受形式**：`while cond { body }`，条件为 `Bool`，主体为花括号块。先判断条件（若最初即为假，主体执行 0 次）。

**值的规范**：这是语句，值为 `Unit`。对可变状态（`var`）的赋值可观察地跨迭代累积（测试夹具：计数器总和 10 / 终值 5；数据依赖的迭代次数 — Collatz(16) → 4 步；两个目标相同）。每次迭代开始时重新求值条件。

**break/continue**：语句位置的守卫形式在两个目标上执行（ALS-E24，#1306 中该实现单元已落地）。跨堆帧仍保留 wall。

测试：`spec/wasm_cross/while_loops.almd`（契约 C-244）。

<a id="als-e16-文字列補間exprkindinterpolatedstring"></a>

## ALS-E16 字符串插值（`ExprKind::InterpolatedString`）

**接受形式**：`"text ${expr} text"`，片段数量任意，`expr` 为表达式（变量引用及算术表达式已实测固定）。字面量 `${` 用 `\${` 转义（实测 `"escaped \${not_interp}"` → `escaped ${not_interp}`）。

**值的规范**：各片段的值以其类型的**规范显示形式**嵌入；String 原样、Int 十进制（ALS-E1）、Bool 小写（ALS-E2）、Float 使用规范形式（`1.5` 已实测固定；全面浮点显示规范须待 ALS-E3/T2 裁定）。整个插值的类型为 `String`。

**声明的 repr**：所有值均以结构形式显示。不过，若类型声明了 `fn T.repr`，该值在任意深度（记录、variant、列表、Option、Result、元组、Map 内）均以该 repr 字符串显示。相同规则也适用于 `assert_eq` / `assert_ne` 失败块（ALS-T18）的 `expected:` / `found:`。`import secret` 的 `Secret[A]` 无论 `A` 为何，均显示为 `Secret(***)`，只有 `secret.reveal(s)` 会暴露内容。

**`!` 的裁定**：插值内的前缀 `!` 与表达式语句位置一样被拒绝，并引导使用 `not`（ALS-E7、`tests/unary_not_diag_test.rs` 中的插值用例）。

测试：`spec/wasm_cross/string_interpolation.almd`（契约 C-245）、`spec/wasm_cross/secret_never_rendered.almd`（契约 C-425）。

<a id="als-e17-識別子exprkindident"></a>

## ALS-E17 标识符（`ExprKind::Ident`）

**接受形式**：已绑定的名称。解析选择**最近的绑定**（遮蔽规范见 ALS-ST1）。

**裁定**：未解析的名称产生**检查期 E003**；诊断包含解析失败的名称本身（仅凭诊断即可确定哪个标识符有误，由 `tests/ident_diag_test.rs` 固定）。不存在运行时名称解析失败（通过检查的名称必定解析到绑定；v1 中属于 NameTotality 的证明对象）。

**词法**（#1456）：标识符采用 UAX #31 默认标识符规则：首字符为 XID_Start 或 `_`，后续为 XID_Continue（与 Rust RFC 2457、Python PEP 3131、ECMAScript 相同）。兼容分解不同于规范分解的字符（全角字母数字、连字、带圈字符等，仅在 NFKC 下折叠）不是标识符字符，在名称位置产生语法错误，不会静默折叠（`ａｂｃ` 不会作为与 `abc` 不同的名称通过）。名称以 NFC 保存，规范等价的拼写（`é` 与 `e` + U+0301）视为同一名称。关键字仅限 ASCII。

**类型侧与值侧**：大写字母开头的名称属于类型侧（类型、分支、构造器）；小写或无大小写之分的字符（汉字、假名、韩文等）开头的名称属于值侧。但即使名称不分大小写，只要模块在 `type` / `protocol` 之后直接声明它，或将它声明为 variant 分支或别名目标（声明的 `=` 或 `|` 之后、圆括号之外，且后面不紧接 `.`），它在该模块全局均属于类型侧（`type 色 = | 赤 | 緑` 之后，`赤` 与 ASCII 的 `Red` 一样不能作为绑定名称）。模块名（`module` / `import` 路径的每个区段及 `import … as` 的别名）为 ASCII，非 ASCII 名称产生语法错误，因此限定符不会成为无大小写之分的名称。模块限定名称的最后一段若不分大小写（`geo.住所`），则在类型位置、模式位置及命名记录的 `{` 前按类型侧读取。使用非 ASCII 的 let、fn、参数、记录字段、类型和分支名称的程序，在两个目标上字节一致，包括记录和 variant 显示中拼出的声明名称。

测试：`spec/wasm_cross/string_interpolation.almd`（契约 C-246）、`tests/ident_diag_test.rs`（反例）、`spec/wasm_cross/unicode_identifiers.almd`（契约 C-387）。

<a id="als-e18-match-式exprkindmatch"></a>

## ALS-E18 match 表达式（`ExprKind::Match`）

**接受形式**：`match subject { pattern [if guard] => expr, … }`。模式包括字面量（字符串、整数）、通配符 `_`、绑定名 `x`、带守卫的绑定名 `x if cond`、Option 的 `some(x)` / `none`、Result 的 `ok(x)` / `err(e)`、列表模式 `[]` / `[a, b]`（元素数相同，加上逐元素绑定或字面量匹配）。其 **rest 形式** `[h, ..t]` / `[h, ..]`（#1461）将长度判断放宽为 `>=`，前缀元素之后的全部内容由 `..name` 绑定为**与匹配对象相同的列表类型**（`..` 不绑定）。rest **只能是列表模式的最后一个元素**（后接元素产生解析错误“Rest pattern must be last”）。
**as 模式** `name @ pattern`（#1461）绑定该位置的**整个值**，同时由内部模式分解、匹配同一个值（`c @ Circle(r)`）。`@` 的结合强度高于分支级别的 `|`；or 选项内的 as 与其他绑定名一样产生 E080。
**or 模式** `a | b | c`（#1461）在任一选项匹配时选中该分支。选项**不能包含绑定名**（E080，因为分支体不能依赖究竟哪个选项匹配；需要负载时应拆分分支或使用 `_`）。穷尽性检查将该分支视为覆盖各选项的**并集**。降低处理将其脱糖为每个选项一个 IR 分支，因此两个后端及 interp 只看到单模式分支（跨目标一致性由构造保证）。分支体是表达式（包括插值）。

**选择裁定**：**从头按顺序选取第一个匹配的分支**（测试夹具：`n = 7` 越过 `x if x < 0` 和 `x if x % 2 == 0`，落入 `_`；实测固定 first-match-wins）。只求值选中的分支。

**穷尽性裁定**：非穷尽的 match 产生**检查期 E010**；诊断**点名**缺失的情况（`missing none`），并附加添加分支的提示（`tests/match_diag_test.rs` 固定）。通过类型检查的 match 在运行时不存在“没有任何分支匹配”。

测试：`spec/wasm_cross/match_forms.almd`（契约 C-247）、`spec/wasm_cross/list_rest_pattern.almd`（契约 C-332，rest 形式的所有执行分支一致：递归 sum、通过覆盖阶梯终止化的自然形式、带守卫的 rest、字符串 tail）、`spec/wasm_cross/as_pattern.almd`（契约 C-333，as 形式：variant 整体转发、as×rest 组合、option 承载值）、`tests/match_diag_test.rs`（反例）。

<a id="als-e19-for-in-文exprkindforin"></a>

## ALS-E19 for-in 语句（`ExprKind::ForIn`）

**接受形式**：`for pat in iterable { body }`，迭代对象为列表、范围（ALS-E10）、映射（ALS-E12 的 `(k, v)` 迭代）。`pat` 是绑定名，或针对元组元素列表的解构 `(i, v)`。

**值的规范**：这是语句，值为 `Unit`。元素从头依次各绑定一次，循环体对可变状态的赋值跨迭代累积（测试夹具：List[Int] 累加 → 8，List[String] 元素输出，List[(Int, Int)] 解构头部 → 53）。迭代对象仅在循环开始前求值一次。

带守卫的 `break` / `continue` 已在两个目标上落地（C-256 / #1306；`loop_break_continue.almd` 固定 for-range break，`spec/lang/loop_break_continue_test.almd` 固定 for-list continue；接受形式见 ALS-E24）。

测试：`spec/wasm_cross/for_in_forms.almd`（契约 C-248）；范围头部为 `range_first_class.almd`（C-238），映射迭代为 `collection_literals.almd`（C-240）。

<a id="als-st2-分解束縛stmtletdestructure"></a>

## ALS-ST2 解构绑定（`Stmt::LetDestructure`）

**接受形式**：`let (a, b, …) = e`，右侧为元组。各分量不可变地绑定到对应位置的值。

**值的规范**：各分量类型为右侧元组的对应位置类型（测试夹具：`let (p, q) =
(1, 2)` 之后 `p + q` 参与加法，构成实测 53 的一部分）。与函数参数位置的元组解构（`((idx, case)) =>`）及 for-in 头部解构（ALS-E19）共享相同规范。

测试：`spec/wasm_cross/for_in_forms.almd`（契约 C-249）、`spec/wasm_cross/tuple_ops.almd`（C-236 的 `let (x, y) = t`）。

<a id="als-e20-パイプと合成exprkindpipe--exprkindcompose"></a>

## ALS-E20 管道与组合（`ExprKind::Pipe` / `ExprKind::Compose`）

**接受形式**：`x |> f`（单段、链式 `x |> f |> g`、传入高阶函数 `xs |> list.map((x) => …) |> list.len`）。组合为 `f >> g`，返回可调用的值，`(f >> g)(x)` 即 `g(f(x))`。

**值的规范**：`x |> f` ≡ `f(x)`（观察等价；测试夹具：`5 |> double` → 10、`5 |> double |> inc` → 11、高阶函数链 → 3）。`>>` 从左到右应用（向 `double >> inc` 传入 4 → `inc(double(4))` = 9，两个目标相同）。

测试：`spec/wasm_cross/pipe_compose_forms.almd`（契约 C-250）。

<a id="als-e21-if-letexprkindiflet"></a>

## ALS-E21 if let（`ExprKind::IfLet`）

**接受形式**：`if let x = o { … } else { … }`，绑定名是**裸标识符**。`if let some(x) = o` 是解析错误（绑定名位置不能写模式；设计采用 Swift 式隐式 unwrap）。

**值的规范**：检查对象为 Option。some 时内部值绑定到 `x` 并执行 then 分支，none 时执行 else 分支（测试夹具：some(5) → 10、none → "empty"，两个目标相同）。

测试：`spec/wasm_cross/if_let_forms.almd`（契约 C-251）。

<a id="als-st3-式文とコメントstmtexpr--stmtcomment"></a>

## ALS-ST3 表达式语句与注释（`Stmt::Expr` / `Stmt::Comment`）

**接受形式**：语句位置的表达式。返回 `Unit` 的调用（如 `println(…)`）直接构成语句。有意丢弃有值的纯表达式使用 `let _ = e`。

**must-use 裁定**：把返回 Result 的调用裸放在语句位置产生**检查期 E042**（ADR-0008），不存在静默吞错。只有两条出路，提示明确写出两者：`expr!`（传播）、`let _ = expr`（显式丢弃，err 不传播 — C-217）。反例由 `tests/expr_stmt_diag_test.rs` 固定。

**接受形式（注释）**：行注释 `// …`（行首或行尾）、块注释 `/* … */`（可多行、位于声明之前或语句之间）。

**值的规范**：注释**在语义上不可见**，任何位置的任何形式均不影响可观察行为（测试夹具：插入所有形式后的输出在两个目标上都与无注释版本相同）。fmt 保留注释（comment_map / doc_map 以声明为单位承载）。

测试：`spec/wasm_cross/expr_stmt_comment.almd`（契约 C-252）、`tests/expr_stmt_diag_test.rs`（反例）。

<a id="als-st4-場所代入stmtindexassign--stmtfieldassign"></a>

## ALS-ST4 位置赋值（`Stmt::IndexAssign` / `Stmt::FieldAssign`）

**接受形式**：列表元素 `xs[i] = v`、记录字段 `p.x = v`、映射键 `m[k] = v`（已有键覆盖，不存在则插入，即 upsert）。目标为 `var` 绑定（对 `let` 的位置赋值违反不可变性，属于 ALS-ST1 的 E009 系列）。

**值的规范**：赋值后读取可观察到新值（测试夹具：21 / 12 / 6 / 100，两个目标相同）。值语义：赋值不会通过共享泄漏（COW）。**越界索引写入在运行时中止**（统一消息 + exit 1，C-067 的裁定）。

测试：`spec/wasm_cross/place_assign_ascription.almd`（契约 C-253）；越界为 `spec/wasm_cross/index_bounds_write_heap.almd`（C-067）。

<a id="als-e22-型注釈式exprkindtypeascription"></a>

## ALS-E22 类型注解表达式（`ExprKind::TypeAscription`）

**接受形式**：`(e: T)`，即表达式位置的类型注解。

**值的规范**：值仍为 `e`，`T` 提供类型检查的期待类型（测试夹具：`(7: Int)` → 7）。不影响运行时表示。

测试：`spec/wasm_cross/place_assign_ascription.almd`（契约 C-254，与 C-253 共享测试夹具）。

<a id="als-e23-レコードexprkindrecord--exprkindspreadrecord--exprkindmember"></a>

## ALS-E23 记录（`ExprKind::Record` / `ExprKind::SpreadRecord` / `ExprKind::Member`）

**接受形式**：字面量 `{ x: 1, y: 2, tag: "a" }`（类型来自声明 `type Pt = { … }` 或上下文）、字段读取 `p.x`、展开更新 `{ ...p, y: 20 }`（继承未列出的字段）。

**值的规范**：字段读取返回声明类型的值。展开创建**新值**，**原值不变**（测试夹具：`q = { ...p, y: 20 }` 后 `p.y` 仍为 2；值语义，不会观察到通过共享的变更）。仅可对 `var` 绑定进行字段赋值（ALS-ST4）。

测试：`spec/wasm_cross/record_forms.almd`（契约 C-255）。

<a id="als-e24-break-と-continueexprkindbreak--exprkindcontinue"></a>

## ALS-E24 break 与 continue（`ExprKind::Break` / `ExprKind::Continue`）

**接受形式**：循环体（`while` / `for-in`）内语句位置的 `break` / `continue`，可裸用，或采用 `if cond then break` / `if cond then continue` 守卫形式。

**值的规范**：`continue` 跳过当前迭代剩余部分，进入下一次迭代（**for-range 必定执行 step**；跳过 step 导致无限循环的陷阱由降低处理从结构上排除）。`break` 立即退出循环。**循环体中途的 break 在该位置立即生效**（测试夹具：`if k > 3 then break; last = k` → 3；延迟为标志而错误产生 4 的问题在 #1306 修复，v0、interp、v1 native、v1 wasm 四者一致）。

**3-way 裁定**：此测试夹具有意放在 spec/wasm_cross。#1306 的发现（两个 v1 分支以相同错误值达成一致，对 2-way 门禁不可见）属于只有让 interp 参与投票的 3-way 才能发现的类别。跨堆帧的 break 会跳过每次迭代的 Drop，因此仍明确触及 wall。

测试：`spec/wasm_cross/loop_break_continue.almd`（契约 C-256）、`spec/lang/loop_break_continue_test.almd`（wasm 分支）。

<a id="als-e25-エラー演算子exprkindunwrap--exprkindtooption--exprkindunwrapor--exprkindtry"></a>

## ALS-E25 错误运算符（`ExprKind::Unwrap` / `ExprKind::ToOption` / `ExprKind::UnwrapOr` / `ExprKind::Try`）

**接受形式**：后缀 `e!`（按上下文 unwrap 或传播，不限于 effect fn，也包括可失败 fn 和可失败 lambda。ADR-0008、[按上下文的说明](../../../../../specs/result-option-effect.md)）、后缀 `e?`（Result → Option 转换）、`e ?? d`（Ok/Some 的值，Err/None 时为 `d`）。不接受跨换行的后缀 `?` / `?.`（行首 `?` 是另一个表达式）。

**值的规范**：`half(10)!` → 5（Ok 路径；Err 则传播给调用方，进入 ALS-R1 的 abort 形式）。`err(_)?` → `none`，`ok(4)?` → `some(4)`（测试夹具：-1 / 4）。`??` 的消费与 ALS-E9 相同。传播规范详情由 ADR-0006/0008 的契约组（C-216、C-217、C-222）固定。

**Try 裁定**：`ExprKind::Try` 是**解析器不会生成的死词汇**；表层后缀 `?` 构造 `ToOption`。节点仍留在 AST，但没有对应语法（删除属于后续词汇清理，与 #1300 的 CallArg::Label 同类）。

**`?.` 已由 ALS-E28 规范化**（标量类由 C-264 固定）。以记录为主体的 wasm 侧 wall 记载于 E28 的限制节。

测试：`spec/wasm_cross/error_operators.almd`（契约 C-257）。

<a id="als-e26-呼び出しとラムダexprkindcall--exprkindlambda"></a>

## ALS-E26 调用与 lambda（`ExprKind::Call` / `ExprKind::Lambda`）

**接受形式**：命名调用 `f(a, b)`（位置参数）、lambda `(x: Int) => e`（类型注解可选）、`(x) => e`、零参数 `() => e`，以及高阶函数参数位置的内联 lambda。effect 调用的消费规范为 ALS-ST3/E25（E041/E042）。

**值的规范**：参数从头依次各求值一次，并按值传递（值语义）。lambda 是函数值；绑定后调用与作为参数传递两条路径的观察相同（测试夹具：5 / 10 / 42 / 12，两个目标相同）。参数数量不匹配产生检查期 E004。

测试：`spec/wasm_cross/call_lambda_ctor.almd`（契约 C-258）。

<a id="als-e27-コンストラクタ参照exprkindtypename"></a>

## ALS-E27 构造器引用（`ExprKind::TypeName`）

**接受形式**：类型声明 `type Color = Red | Green | Rgb(Int, Int, Int)` 的分支名称；带负载时为调用形式 `Rgb(1, 2, 3)`，无参数分支为裸引用 `Green`。

**值的规范**：构造的 variant 值通过 match 的分支模式消费（测试夹具：`Rgb(r, _, b) => r + b` → 4、`Green` → 20，两个目标相同）。match 的穷尽性见 ALS-E18（E010）。

**构造器是函数值**（#1111、2026-08-05 是/否批准）：带负载的分支名称的裸引用是函数值，单参数分支为 `(A) -> T`，多参数分支为 `(A, B, …) -> T`（无部分应用；需要柯里化时写成 lambda）。无参数分支的裸引用就是值本身（不是函数值）。内置构造器 `some` / `ok` / `err` 同等对待：裸 `some` 是等价于 `(x) => some(x)` 的函数值（解析器合成 eta 展开；所有执行分支经 lambda 路径处理，故两个目标的一致性由构造保证）。`[1, 2] |> list.map(some)` == `[some(1), some(2)]`，`list.map(xs, Circle)` == `list.map(xs, (x) =>
Circle(x))`。

测试：`spec/wasm_cross/call_lambda_ctor.almd`（契约 C-259）、`spec/wasm_cross/ctor_fn_value.almd`（契约 C-322）、`spec/lang/ctor_fn_value_test.almd`。or 模式：`spec/wasm_cross/or_pattern.almd`（契约 C-323）、`spec/lang/or_pattern_test.almd`、`tests/diagnostics/e080-or-pattern-binder/`。

<a id="als-dl1-宣言declmodule--declimport--decltype--declfn--decltoplet--declprotocol--decltest--decltestwheredef"></a>

## ALS-DL1 声明（`Decl::Module` / `Decl::Import` / `Decl::Type` / `Decl::Fn` / `Decl::TopLet` / `Decl::Protocol` / `Decl::Test` / `Decl::TestWhereDef`）

**接受形式**：`module name`（开头，可选）/ `import mod`（非自动导入的标准库模块和外部包必需；ALS 的 import 规范见 module-system 规格）/ `type T = Case | Case(payload)` 与 `type T = { fields }` / `fn f(…) -> T =
…`、`effect fn …` / 顶层 `let NAME = value`（可从 fn 体引用）/ `protocol P { fn sig }` 与 `fn T.method(…)` 实现 / `test "name" { … }` 块与 `local test where` / `mod test where` 定义。

**值的规范**：一个测试夹具贯穿 import（json 的 parse→stringify 往返）、variant 类型、顶层 let（Int/String）、fn/effect fn，两个目标相同（300 / hi / careful / [1,2]）。夹具开头是在注释块正下方的 `module` 头部，即曾被 #1323（fmt(valid)→invalid 类）导致无法格式化的形式，因此这里也固定了带注释的 module 头部能够通过 fmt。多模块的执行证据仍为 `spec/integration/modules/` 组（每次 CI 执行）。协议解析规范由现有契约 C-094/C-126（`protocol_ufcs_inferred_lambda.almd`）固定。test 块持续在 `almide test` 的 wasm 分支执行（整个 spec/lang 都是证据；`test where` 为 `spec/lang/test_where_test.almd`）。

**Strict 已移除**：`strict <mode>` 曾是仅被接受、没有任何使用方的声明（#1321）。2026-08-13 的裁定将其**从语法中删除**；语言不保留承诺无效检查模式的形式。此后 `strict` 是普通标识符，写作声明会产生解析错误。

**记录字段属性**：字段只接受 `@builtin`、`@location`、`@skip`、`@skip_decode` 四种属性。字段上的其他属性以检查错误 E053（`unknown attribute `@<name>` on record field '<field>'`）拒绝（方言 epoch 15）。fn 属性词汇之外的属性仍与以前一样产生 E053 警告。

测试：`spec/wasm_cross/declaration_forms.almd`（契约 C-260）、`tests/diagnostics/e053-unknown-field-attribute-serde-skip/`、`tests/diagnostics/e053-unknown-field-attribute-redact/`（契约 C-430）。

<a id="als-e3-浮動小数点リテラルexprkindfloat-部分節"></a>

## ALS-E3 浮点字面量（`ExprKind::Float`）— 部分章节

**接受形式（已确定部分）**：十进制小数 `1.5`、`0.5`（小数点两侧必须有数字；`.5`/`1.` 为解析错误，已实测）、负零 `-0.0`。指数形式（`1e10`/`1E10`/`1.5e-3`）和下划线分隔（`1_000.25`）**被接受**，但 fmt 经由值重新打印拼写，无法用测试夹具固定；因此**形式的规范化仍为 OPEN（等待 #1261 裁定）**。超范围字面量（`1e999`）的处理也是同一 issue 的分支（目前饱和为 inf，fmt 输出不可解析的 `inf.0`）。

**值的规范（已确定部分）**：类型为 binary64（ALS-T2）。规范显示使用 `float.to_string`；测试夹具固定 `1.5` / `0.5` / **`-0.0`（保留符号）** / 运算结果 `1.75` 在两个目标上相同。

测试：`spec/wasm_cross/string_float_stable.almd`（契约 C-261）。

<a id="als-e5-文字列リテラルexprkindstring-部分節"></a>

## ALS-E5 字符串字面量（`ExprKind::String`）— 部分章节

**接受形式（已确定部分）**：双引号字面量、空字符串 `""`、转义 `\t`、`\n`、`\\`（拼写经 fmt 稳定，值由测试夹具固定）。`\u{…}`（替换为值）、heredoc `"""…"""`（压为单行）、引号选择（自动切换）均被接受，但 fmt 不保留形式，因此**形式的规范化仍为 OPEN（#1263）**。未知转义（`\q`）在检查期以 **E047**（unknown escape sequence）拒绝（#1264 已落地，0.59.1 实测）。

**值的规范（已确定部分）**：`\t` → 制表符，`\n` → 换行，`\\` → 一个反斜杠字符（测试夹具用可见界定符固定）。空字符串是长度为 0 的值。

测试：`spec/wasm_cross/string_float_stable.almd`（契约 C-262）。
<a id="als-dl2-回復ノードexprkinderror--stmterror"></a>

## ALS-DL2 恢复节点（`ExprKind::Error` / `Stmt::Error`）

**裁定**：两个节点均为**恢复词汇**，只用于占据解析错误的位置，使后续诊断收集得以继续，**绝不会出现在被接受的程序中**。没有对应的语法形式（与 ALS-E25 的 `Try` 同样处理，但这两个节点确实会在错误路径生成）。

**验证**：`tests/parser_recovery_test.rs` 固定两个方向：（1）整个 spec/wasm_cross 语料库（已接受、已在两个目标执行的程序）的 AST 中恢复节点为**零**；（2）损坏文件报告错误时，**解析继续越过错误位置**（后续声明仍可见）。

测试：`tests/parser_recovery_test.rs`（结构验证）、`spec/wasm_cross/declaration_forms.almd`（接受侧，契约 C-263）。

<a id="als-e28-オプショナルチェーンexprkindoptionalchain"></a>

## ALS-E28 可选链（`ExprKind::OptionalChain`）

**接受形式**：`expr?.field`，后缀，不跨换行（行首 `?` 是另一个表达式）。主体为 `Option[レコード]`（记录的 Option）。

**值的规范**：主体为 `some(v)` 时返回 `some(v.field)`，为 `none` 时返回 `none`（测试夹具：`some({x:3,y:4})?.x ?? 0` → 3、`none?.x ?? -9` → -9，两个目标相同）。整条链的类型为 `Option[フィールド型]`（字段类型的 Option）；也就是说，`?.` 将“存在则读取”包装在 Option 中返回（剥离值是 `??`/match 的工作）。

**C-264 的范围与旧路径记录**：C-264 针对标量字段。以下堆负载 wall 是该契约成立时的诊断记录，并不表示目前所有 String 字段都被拒绝。另见 `spec/lang/unwrap_operators_test.almd` 中 `name: String` 的读取。标量字段的 v1 wall 在 #1329 解除（脱糖移至共享 optimize 切点，合成辅助函数改为 Named 调用，因此所有位置均可降低）。**堆负载（String 字段）仍沿用既有路径**；已实测，用户手写的同形辅助函数本身也会触及 `heap-result match は空の遅延ヒープ値を move out する`（heap-result match 将空的延迟堆值移出） 实现单元，这并非 OptionalChain 特有。解除须等待该实现单元。

测试：`spec/wasm_cross/optional_chain_scalar.almd`（契约 C-264）、`spec/lang/optional_chain_test.almd`（wasm 分支 4 个用例）。

<a id="als-st5-guard-文stmtguard"></a>

## ALS-ST5 guard 语句（`Stmt::Guard`）

**接受形式**：`guard cond else raise-expr`，用于语句位置。`raise-expr` 为 `err(e)` 或 `err(e)!`（两种拼写被规范化为同样形式，#1336）。它不是省略 else 的 `if`，而是**提前退出**；后续语句仅属于 cond 为真的路径。

**值的规范**：cond 为真则直接通过并执行后续语句（测试夹具：`checked(5)!` → 10）。为假则 else 中的 err 传播给调用方，由调用侧的 `!` 或 match 接收（测试夹具：match 捕获 `checked(-3)` 的 err，得到 "must be positive"）。两个目标相同。

**标量/堆边界**：标量值 guard 在 #1336 已于两个执行分支启用。`guard let x = e else …`（`Stmt::GuardLet`）是**另一类别**，在前端脱糖为 variant match，曾触及尾位置 variant match 实现单元；ALS-ST6（#1340 / C-266）落地后，guard let 已单独成节，Stmt::GuardLet 的覆盖行指向 ALS-ST6。else 分支除 raise 外也接受 `continue`（`spec/lang/guard_test.almd` 固定）。

测试：`spec/wasm_cross/guard_statement.almd`（契约 C-265）、`spec/lang/guard_test.almd`（wasm 分支）。

<a id="als-st6-guard-let-文stmtguardlet"></a>

## ALS-ST6 guard let 语句（`Stmt::GuardLet`）

**接受形式**：`guard let x = scrut else else-expr`，用于语句位置。`scrut` 为 Result / Option，`x` 绑定 Ok / Some 的负载，在所有后续语句中可见。else 分支除 raise（`err(e)` / `err(e)!`）外也可采用**值形式**，该值从函数返回（`guard let head = list.first(xs) else { d }`，由 `spec/lang/guard_let_test.almd` 固定为接受形式）。

**值的规范**：成功极性时绑定并直接通过（测试夹具：`int.parse("21")` → v=21 → 42）。失败极性时 else 的 err 传播给调用方（测试夹具：match 捕获 `doubled("x")` 的 err，得到 "not a number"）。两个目标相同。连用的 guard let 折叠为单一承载值绑定（#1340）。

**块嵌套裁定（#1340 的发现）**：guard let 在前端脱糖为**嵌套在块中的 variant match**。裸尾部 match 早已在两个执行分支启用，但嵌套一层块就触及 wall，因为 `auto_wrap_abi_body` **只将函数体根节点**重新定型为 Result 承载值。因此原因不是极性或通配符分支，而是**位置**；修复方式是让“规范化为已证明 bind 形式的识别器”认识 variant match（与 ALS-ST5 相同机制的另一面）。

测试：`spec/wasm_cross/guard_let_statement.almd`（契约 C-266）、`spec/lang/guard_let_test.almd`（wasm 分支）。

<a id="als-e29-二項演算子exprkindbinary"></a>

## ALS-E29 二元运算符（`ExprKind::Binary`）

**接受形式**：算术 `+ - * / % **`（幂的 `^` 拼写也同义；引用的测试夹具固定 `2 ^ 10`，与 `2 ** 10` 同值 1024）、比较 `< <= > >=`、相等 `== !=`、逻辑 `and` / `or`（关键字；没有 `&&`/`||`；否定为 `not`，ALS-E7）。

**Int 除法与余数裁定**：`/` **向零截断**（测试夹具：`-7 / 3` → **-2**，`7 / 3` → 2）；`%` 取**被除数的符号**（`-7 % 3` → **-1**）。与 C/Rust 约定相同，不是向下取整除法。除以零在运行时中止（C-001 的裁定）。

**`+` 的重载**：除数值加法外，还用于 **String 与 List 拼接**（`"cat" + "dog"` → `catdog`，`[1,2] + [3]` 长度为 3）。混合类型的 `+` 为检查期类型错误。

**`**` 的裁定**：幂没有硬件指令，因此脱糖为对标准库内部幂函数的调用（Float 转写 libm，Int 使用平方求幂法）；运算符节点在 caps 台账中计为 1 次调用（mir==ir）。

**值的规范**：所有形式在两个目标上字节相同（测试夹具固定 14 行观察点）。相等语义依类型而定；标量采用原始比较，String / List / Value 采用深比较（ALS-E11、`list.eq_*` 系列）。

测试：`spec/wasm_cross/binary_operators.almd`（契约 C-267）。

<a id="als-e30-ホールと未実装マーカーexprkindhole--todo--placeholder"></a>

## ALS-E30 孔洞与未实现标记（`ExprKind::Hole` / `Todo` / `Placeholder`）

**接受形式**：表达式位置的 `_`（`ExprKind::Hole`）、`todo("メッセージ")`（`ExprKind::Todo`），以及**调用参数位置的 `_`**（`ExprKind::Placeholder`）。前两者是**有类型的孔洞**，通过类型检查，类型从上下文推断（`let y: Int = _` 为 Int）。第三种**不被接受**（见下方裁定）。

**值的规范（native）**：孔洞和 `todo` 都会**在求值到达时中止**；以 Rust panic 向 stderr 输出 `not yet implemented: <メッセージ>`，**退出码为 101**。孔洞消息标明 Almide 侧行号（`hole at line 4`）。`todo` 原样携带自己的字符串（即使包含引号，也不会因转义而损坏，#1346）。中止前已产生的输出保留（测试夹具：`io.print("before")` 后接 `todo` → stdout 有 `before`，然后 101）。

**裁定 1：此项没有跨目标契约**。wasm 渲染器**在构建期拒绝**包含孔洞/`todo` 的函数（明确的 wall）。而且拒绝**不依赖可达性**；实测在永不求值的 match 分支中含 `_ => todo(...)` 的程序，native 正常退出（exit 0），wasm 则将整个函数视为子集之外而拒绝。因此原则上无法编写 `spec/wasm_cross/` 测试夹具，本节**不主张两个目标行为相同**。孔洞是仅限 native 的草拟工作流，不应留在发布程序中。契约台账仅覆盖下方裁定 2 的部分。

**裁定 2：调用参数中的 `_` 是检查期错误 E046**（契约 C-268）。`add(_, 10)` **不是**部分应用；绑定后会得到返回值类型，在管道位置则成为额外参数并产生 E004，也就是曾让“没有值的孔洞”直接流入 codegen 的形式（#1266 类的非法 Rust）。E046 标明位置（第几个参数），并引导至 lambda（`(x) => add(x, 10)`）。类型检查先于目标选择，因此该错误在 native 和 wasm 上相同（与 `fan.timeout` 的 E027 墓碑 C-006 结构相同）。

**裁定 3：interp 弃权**。native 的中止是 panic（exit 101），而 interp 的 `Flow::Abort` 模拟的 R1 中止是 `Error: <msg>` + exit 1，属于**不同的观察**。第三裁判弃权优于投出错误票（almide-interp 的规则），所以 `Hole` / `Todo` 返回带原因的 `Flow::Unsupported`。

测试：`tests/diagnostics/e046-call-arg-placeholder/`（E046，契约 C-268）、`spec/lang/typed_hole_test.almd` 与 `tests/typed_hole_diag_test.rs`（native 中止形式及行号、`todo` 的消息保留）。

<a id="als-e31-スコープ付き領域exprkindscoped--scoped-fn"></a>

## ALS-E31 有作用域的区域（`ExprKind::Scoped` / `scoped fn`）

**接受形式**：

```ebnf
scoped_block := "scoped" block
fn_decl      := [visibility] ["scoped"] ["effect"] "fn" name …
```

`scoped` 是**上下文关键字**。在表达式位置，仅当 `scoped` 后紧接同一行的 `{` 时按块修饰符读取；在声明位置，仅当后面紧接 `fn` / `effect` 时按修饰符读取。`match` / `while` / `for … in` 的**头部**是例外，因为随后的 `{` 打开的是语法自身的主体，`scoped` 仍为标识符（`match scoped { … }` 保持 #1997 之前的含义）。因此，包含名为 `scoped` 的标识符的现有程序语义不变，方言 epoch 不增加。

**值的规范**：`scoped { body }` 的值就是 `body` 的值，有无 `scoped` **不改变观察（stdout、stderr、退出码）**。`scoped fn` 在区域外调用时是普通函数，结果与在区域内调用相同。也就是说，该修饰符声明的是**内存回收边界**，不是值的含义（契约 C-362、C-363）。

**回收规范**：为块分配的内存在边界回收。各执行分支可以采用不同实现：结构化 wasm 分支在块入口保存分配器状态，在出口回退（`crates/almide-wasm/src/region.rs`）；native 分支在 arena 中运行区域克隆（`__rgn_` 对应实现），无法生成对应实现的闭包则在同一闭合点释放所有权。**回收本身不可观察**；本节对两个目标施加的仅为上述值规范和下述接受判定。不主张峰值内存上界、终止性，也不主张到达边界前分配量的上界。

**接受规范（stage 1 片段）**：允许**标量**（`Int` / `Float` / `Bool` / `Unit`）、**受管理的代数数据**（在同一文件声明的非泛型 variant 类型，各分支仅为 unit 或位置字段，字段为标量或符合相同条件的 variant 类型；仅含标量字段的记录；这些类型构成的元组及 `Option`），以及**普通工作函数**和**直接尾递归工作函数**。只能调用其他 `scoped fn`、`int` / `float` / `math` / `bool` 的标量标准库调用，以及构造器。

**拒绝规范**：片段之外的形式**在检查期**被拒绝。检查先于目标选择，因此 native 与 wasm 的判定相同（契约 C-364）：

- **E086**：区域的值越过边界。`{name} would outlive its scoped
  allocation` 附带 `= allocated at <file:line:col>` 和 `= scope ends at
  <file:line:col>` 两行，以及 `help:`。
- **E087**：区域内不允许的操作，包括调用非 `scoped` 函数、全局量、宿主资源（输出、`effect fn`）、未知间接调用、捕获外部堆值、片段外的类型。`<構文> is not permitted in a scoped
  {function,block}` 附带说明可能保留什么的一行，以及指明提出要求的声明的一行。
- **E088**：保留后续计算的递归形式。`recursive call retains the current scoped
  activation` 附带说明调用之后还剩什么的一行。单次非尾自递归和相互递归属于此类。树遍历形式（一个分支内有两个或更多自调用）因无法重写为累加器而作为普通工作函数被接受。

`scoped effect fn` 在语法上被接受，但在 stage 1 以 E087 拒绝；效果是宿主资源，而此片段没有在边界运行的终结处理。

测试：`spec/wasm_cross/scoped_region_value.almd`（契约 C-362）、`spec/wasm_cross/scoped_worker_in_and_out.almd`（契约 C-363）、`tests/diagnostics/e086-scope-escape-tail/` 及其他 E086 / E087 / E088 反例测试夹具（契约 C-364）。


## 关于原文限制的读者注记

原文中的 ALS-E3 旧 UNWRITTEN 备注与后面的部分章节并存，拼写方面的 OPEN 问题未在此解决。ALS-E6 将 native 单元素元组与插值问题称为已知限制，而 ALS-E8 明确将其标记为历史记录，尚未重新验证是否仍为当前限制。本译文保留两种说明，不自行调和。ALS-E28 限定标量字段契约的范围，并明确堆负载 wall 是历史诊断记录，不表示当前所有 String 字段都被拒绝。ALS-E30 区分 native panic（exit 101）、不依赖可达性的 wasm 构建拒绝及解释器弃权，不提出跨目标行为契约。ALS-E31 保留 stage 1 片段，也不主张峰值内存上界、终止性或到达边界前分配量的上界。日期、特定版本的实测、实现历史、证明及测试夹具引用都是原文主张，并非本译文的新验证。
