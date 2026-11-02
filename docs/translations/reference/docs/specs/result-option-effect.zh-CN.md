# Result、Option、Effect——完整规格

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](result-option-effect.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](result-option-effect.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](result-option-effect.zh-CN.md)



> 权威原文：[docs/specs/result-option-effect.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/result-option-effect.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是非规范性的读者快照译文，不自动同步。原规格及其声明的规范地位保持不变。模型证明、实现主张、历史状态和证据边界均保留原文范围；本译文不提供独立执行、测量、符合性或安全性证明。代码块内代码及注释逐字保留。

> 最后更新：2026-10-31
> 基于 2026-08-05 实测的 287 单元矩阵（#1122）全面修订。设计决策来源为 ADR-0002～0009。全文描述“可运行代码”，各节注明验证测试。
> 2026-08-13：将 §8.0（双层模型）及 §9（非目标）从 ADR-0012 D1/D4 提升到正文。
> 2026-09-27：L3 改为 ADR-0021（lambda 失败通道的 E），撤回 §9 的非目标 4。

> 原文差异说明：§2 表格将 Option 的传播概括写为 `err("none")`；另见[effect 调用规格](../../../../specs/effect-fn-call-semantics.md)，它区分外围为 Option 通道时传播 `none`。下表保留原文范围和措辞，不擅自调和两份原文。上文的实测矩阵及工作代码描述属于历史原文主张，并非本译文重新执行的结果。

## 1. 类型

### Result[T, E]
```text
ok(v)   : Result[T, E]   // 成功値
err(e)  : Result[T, E]   // エラー値
```

### Option[T]
```text
some(v) : Option[T]       // 値あり
none    : Option[T]       // 値なし
```

### Option 语法糖 `T?`（ADR-0010）

`T?` ≡ `Option[T]`。在**所有类型位置**有效（`!` 是返回位置标记，`?` 则是值的属性）。`?` 最紧密地结合前面的类型原子（名称+泛型，或括号包围的类型），不跨越 `->`：

```almide
fn f(v: Int?) -> Int? = v                     // 引数・戻りどちらも可
type Slots = {
  f: (Int) -> Int?,                           // fn 型 slot: Option[Int] を返す fn
  on_tick: ((Int) -> Unit)?,                  // fn 値そのものが Option — 括弧必須
  pair: (String, Int)?,                       // Option[タプル]
  nested: (Int?)?,                            // 入れ子(`Int??` は ?? にレクスされ不可)
}
fn g(s: String) -> Int?! = ok(int.parse(s)?)  // Result[Option[Int], String](? が先、! は戻りマーカー)

test "T? in every type position" {
  assert_eq(f(some(1)), some(1))
  assert_eq(g("7"), ok(some(7)))
  assert_eq(g("x"), ok(none))
}
```

规范形式为 `T?`：`almide fmt` 将 `Option[T]` 规范化为 `T?`（D3）。stdlib 也已用 `fmt --no-import-edit` 规范化（2026-08-07，spec/lang/option_marker_test.almd 是统一的见证）。

### Never（底类型）
```text
process.exit(n) : Never   // 戻らない関数の戻り値型
```
Never 可赋给任何类型，可在 guard else、if then、match arm 中使用。

### 可失败标记 `-> T!` / `-> T!E`

在 fn 声明及函数类型的返回位置，`T!` 是 `Result[T, String]` 的语法糖，`T!E` 是 `Result[T, E]` 的语法糖。可失败性（fallibility）与效应（effect）是正交的两轴，四个象限均可书写：

```almide
fn        total()      -> Int  = 1          // pure ・総
fn        fallible()   -> Int! = ok(1)      // pure ・可謬(= Result[Int, String])
effect fn e_total()    -> Int  = 1          // effect・総(暗黙 lift、§3)
effect fn e_fallible() -> Int! = ok(1)      // effect・可謬

test "the four quadrants" {
  assert_eq(total(), 1)
  assert_eq(fallible(), ok(1))
  assert_eq(e_total(), ok(1))       // 呼び出し型は lift 後の Result(§3)
  assert_eq(e_fallible(), ok(1))
}
```

不带 E 的 `T!` 默认错误类型为 `String`。自定义错误类型写为 `T!MyError` 或显式 `Result[T, MyError]`（ADR-0012 D2）。函数类型 `(A) -> B!` / `(A) -> B!E` 也合法（ADR-0009 D1）。不可在函数返回位置以外使用，如普通值类型注解 `let x: Int! = ...`。
`!` 运算符在函数体内传播，**值尾部自动提升为 ok(...)**（Phase 1b，与 effect fn 提升具有相同易用性；Result 类型的出口不变）：

```almide
fn parse_port(s: String) -> Int! = int.parse(s)     // パススルー(既に Result)
fn double_port(s: String) -> Int! = int.parse(s)! * 2  // 値 tail → ok(...) に lift
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! が T! 本体で伝搬
  guard n > 0 else err("must be positive")
  n                                                  // 値 tail lift(ok(n) でも可)
}
```

测试：`spec/lang/fallible_marker_test.almd`、`spec/lang/fallible_typed_error_test.almd`、`spec/lang/fallible_lambda_test.almd`。

## 2. 运算符

所有值级运算符都定义为对具名 stdlib 函数的脱糖（ADR-0005）。定义表：

```text
x ?? d      ≡  option.unwrap_or_else(x, () => d)   // Option operand
r ?? d      ≡  result.unwrap_or_else(r, (_) => d)   // Result operand
r?          ≡  result.to_option(r)
o?.x        ≡  option.map(o, (v) => v.x)
```

`!` 是控制流（提前 return），没有函数实体（ADR-0005 明示的边界）。

### `expr!`——解包并传播

| 输入类型 | 输出 | 失败时 |
|---|---|---|
| `Result[T, E]` | `T` | 将 err 传播到包围函数的失败通道 |
| `Option[T]` | `T` | 转换为 `err("none")` 并传播（无损嵌入——ADR-0003 D3） |
| test 内 | `T` | 解包（panic 导致测试失败） |

`!` 合法的上下文（**并非 effect fn 专用**——C-211）：

1. effect fn 的函数体。
2. test 块。
3. **返回类型解析为 Result / Option / `T!` 的 pure fn**。

其他上下文为 `E022`。非 Option/Result 操作数（`5!`）为 `E034`。传播点不转换 E，不匹配即检查错误（ADR-0003，实现 #1103）。E 相同则自定义错误类型保留结构传播，可进行 match（ADR-0003 D1）。

```almide
fn twice(s: String) -> Result[Int, String] = ok(int.parse(s)! * 2)   // pure + !
```

测试：`spec/lang/unwrap_operators_test.almd`、`spec/lang/fallible_marker_test.almd`、`spec/wasm_cross/pure_bang_propagation.almd`。

### `expr?`——Result → Option 转换

| 输入类型 | 输出类型 |
|---|---|
| `Result[T, E]` | `Option[T]`（err → none，丢弃 E） |
| `Option[T]` | `Option[T]`（恒等，无诊断） |

仅转换**最外一层**：`Result[Result[T, E], E]?` 是 `Option[Result[T, E]]`。不做 flatten（嵌套扁平化由 `result.flatten` / `option.flatten` 负责）。非 Option/Result 操作数为 `E034`。

### `expr ?? fallback`——解包并回退

| 输入类型 | 输出类型 |
|---|---|
| `Result[T, E]` | `T`（err 时 fallback，丢弃 E） |
| `Option[T]` | `T`（none 时 fallback） |

- **惰性求值**：仅在 none / err 时求值 fallback（`o ?? expensive()` 在 o 为 some 时不调用 expensive()）。这是定义 `≡ unwrap_or_else`（ADR-0005）的结果。
- **fallback 类型在检查时与解包后的 T 统一**（不匹配为 E001；`n ?? "hello"`、`n ?? some(1)` 是检查错误）。
- **优先级最高（后缀家族）**：`a ?? 1 + 2` 是 `(a ?? 1) + 2`。这**与 C 系 `?:`（近乎最低）相反**，混合算术时建议加括号。
- **链向右嵌套**：`a ?? b ?? c` 是 `a ?? (b ?? c)`（AST 实测），含义是“首次成功者胜出”。
- **禁止跨行（E038）**：语句级 fallback 须与 `??` 在同一行。`??` 本身和中缀运算符一样，可置于续行开头（#3155）：`xs\n  |> list.first\n  ?? 0`。要将 fallback 放到下一行，使用括号 + 后缀 `??`：
  ```almide
  fn parse_or_minus_five(s: String) -> Int = {
    let v = (int.parse(s) ??
      -5)
    v
  }
  ```
- 非 Option/Result 操作数为 `E034`。

测试：`spec/lang/unwrap_operators_test.almd`、`tests/diagnostics/e038-qq-line-crossing/`、`tests/diagnostics/e034-off-type-bang/`。

### `expr?.field`——可选链（仅 Option）

```almide
type P = { x: Int }
fn getx(o: Option[P]) -> Int = o?.x ?? 0    // some(P{x:5}) → 5、none → 0

test "optional chaining" {
  assert_eq(getx(some(P { x: 5 })), 5)
  assert_eq(getx(none), 0)
}
```

安全访问 `Option[レコード]`（Option[记录]）的字段，返回 `Option[フィールド型]`（Option[字段类型]）。**仅 Option**——用于 Result 时以专门诊断拒绝（从 Result 可组合成 `(r?)?.x`）。

测试：`spec/lang/unwrap_operators_test.almd`。

## 3. effect fn

### 声明与提升

```almide
import fs

effect fn read_file(path: String) -> String = fs.read_text(path)!
```

| Almide | 生成的 Rust |
|---|---|
| `effect fn f() -> T` | `fn f() -> Result<T, String>`（隐式提升） |
| `effect fn f() -> Result[T, E]` | `fn f() -> Result<T, E>`（不双重包装） |
| `fn f() -> T` | `fn f() -> T`（无转换） |

### 传播全部显式（ADR-0008——auto-`?` 已删除）

过去 effect fn 体内有依赖位置的隐式传播（auto-`?`），在 5 个位置有效、另 5 个位置无效，行为随注解或模式形状反转。**0.55 已将整套机制删除**：可失败调用在**任何位置都产生 Result 值**，需要传播时写 `!`。

| 写法 | 含义 |
|---|---|
| `let v = int.parse(s)!` | 传播（v 为 Int，err 时提前 return） |
| `let r: Result[Int, String] = int.parse(s)` | 保留 Result 值 |
| `let _ = fail()` | **有意丢弃**——err 不传播（C-217） |
| `let v = int.parse(s)` | **E041**（原隐式位置全部报错，提示插入 `!`） |
| `fail()`（语句位置） | **E042**（must-use，提示 `!` 或 `let _ =` 两种写法） |
| `match get() { ok(v) => …, err(e) => … }` | Result 是普通值，普通 match |
| `match get()! { 42 => … }` | 在值层 match（`!` 明示层次） |

`e()?` / `e() ?? fb` 是 ADR-0005 普通值操作（无特例规则）：`e()?` ≡ Result→Option 转换，`e() ?? fb` ≡ unwrap_or_else。

回归测试：`spec/lang/explicit_propagation_test.almd`（两类危险消除 + `let _` 的 A/B）、`spec/wasm_cross/let_wildcard_discard.almd`（C-217）、`spec/wasm_cross/effect_option_explicit_bang.almd`（C-216）。

### lambda 的可失败性（ADR-0006 D1 / ADR-0009——#1108 Phase 2b）

lambda 是“迷你可失败 fn”：体内的 `!` 进入 **lambda 自身的失败通道**（`Result[T, E]`，E 按下述 L3 决定），不跨闭包边界（保持 #489 不变量）。可失败性由使用驱动推断（L1～L9，2026-08-07 批准）：

```almide
fn halve(n: Int) -> Int! = if n % 2 == 0 then ok(n / 2) else err("odd")
fn retry(op: (Int) -> Int!) -> Int! = op(1)!          // fn 型 slot の `!`(L7/L8)

fn demo(xs: List[String]) -> Int! = {
  let g = (x) => int.parse(x)! * 2      // g: (String) -> Result[Int, String] — 第一級
  let doubled = g("21") ?? -1           // 呼べば Result 値、普通に消費
  let halves = list.map(xs, (s) => halve(int.parse(s)!)!)!   // 複合可謬 callback → first-err 形(L6)
  ok(doubled + list.sum(halves))
}

test "fallible lambdas (in fn bodies — test bodies keep unwrap semantics, L9)" {
  assert_eq(demo(["4", "6"]), ok(47))
  assert_eq(demo(["4", "7"]), err("odd"))
  assert_eq(retry((n) => ok(n + 1)), ok(2))
}
```

- 通道 E（ε）由 ADR-0021 决定（L3）：先由 lambda 流入的上下文（带类型 slot `(A) -> B!E`、带类型 `let`、传播结果的 fn、`err(e) => err(e)` 等消费）决定；若上下文不决定，则对体内 `!` 操作数（及 guard / 值 join 中 `err(..)`）的 E 求 join：全相同则取该 E，不同则 String。String 是顶（任何 E 都可经 repr 字符串，即打印 `"${e}"`，进入 String，#2725），故 join 总有答案。当 ε 确定为带类型 E 时，其他 E 的 `!` 在该位置产生 E022。Option 操作数的 none 变为 err("none")（L4），值尾部提升为 ok(...)（L5）。
- **test 块内例外**：lambda 的 `!` 仍是解包，HOF dispatch 也仍为全函数形式（L9，test 世界完整保留 #1108 之前语义）。
- 对普通 `(A) -> B` slot 的 bit 透传（用户 HOF）尚未实现；E005 带有指明两种解决写法的定向提示（Phase 2b-iii）。

测试：`spec/lang/fallible_lambda_test.almd`（固定 L1～L9）、`spec/lang/typed_error_callback_channel_test.almd`（L3，#2601 全写法、带类型 slot、带类型 `let`）、`tests/diagnostics/e022-callback-bangs-disagree/`、`tests/diagnostics/e022-typed-slot-callback-string-bang/`。

## 4. fn main

| Almide | Rust codegen | 备注 |
|---|---|---|
| `fn main() = ...` | `fn main()` | `fn` 的效应规则。输出、assert、panic 内建例外见 ADR-0022 / [effect-system.md](../../../../specs/effect-system.md) §2.1 |
| `effect fn main() -> Unit = ...` | `fn main() -> Result<(), String>` | 自动提升 |

- **main 不接收参数。** 命令行参数来自 `env.args()`（不含程序名的 argv）。
- 未处理 Err 到达 main 时，**两个目标均 exit 1 + stderr `Error: <msg>`**（Display，无引号）。由 `!` 对 Option none 产生的 `err("none")` 走相同路径，双目标一致。旧文所述“wasm 以 exit 0 静默忽略的残留差异”已解决，实测确认两目标都 exit 1 + `Error:`。

测试：`tests/wasm_runtime_test_parts/p3.rs::unhandled_main_error_terminates_consistently`、`spec/lang/result_option_matrix_test.almd`。

## 5. test 块

```almide
fn f() -> Int = 42

test "name" {
  assert_eq(f(), 42)
}
```

| 属性 | 值 |
|---|---|
| `is_effect` | `true`（可调用 effect fn） |
| Result 包装 | 无 |
| effect 调用结果 | **显式 Result 值**（auto-`?` 在 test 中不起作用） |
| `!` 行为 | 解包（panic 导致测试失败） |

test 中调用 effect fn，结果仍作为 Result 返回；用 `!` 解包或 `??` / match 处理（旧文“test 中无需 !”是错误的）。

`assert_eq` / `assert_ne` 的失败输出基于 Almide repr。操作数使用 ALS-R2 显示形式并按 C-418 规则转义，在 native / wasm 的执行与测试路径中共用显示（不是 Rust `Debug`）。回归测试：`tests/assert_operand_escape_lanes_test.rs`。

测试：`spec/lang/expr_test.almd` 及其他所有 `*_test.almd`。

## 6. guard else

```almide
import process
import env

fn positive(n: Int) -> Int! = {
  guard n > 0 else err("must be positive")    // guard condition else diverge_expr — else は戻りチャネルへ
  ok(n)
}

effect fn main() -> Unit = {
  let n = list.len(env.args()!)
  guard n > 0 else process.exit(3)             // else: Never(process.exit)
  println(int.to_string(n))
}

test "guard else returns the else value" {
  assert_eq(positive(0), err("must be positive"))
  assert_eq(positive(2), ok(2))
}
```

- 条件约束为 Bool。
- `else_expr` 类型在**检查时检查**（#1118）：
  1. 与函数返回通道统一（pure 为声明类型，effect fn 也可为提升后的 Result）。
  2. `Never`（process.exit / panic 系）。
  3. 循环内的 `break` / `continue`。
  类型不匹配（`-> Int` fn 中的 `else "nope"` / `else { println(..) }`）为 E001。
- 即使条件为常量，else 的效应也不丢失（`guard false else process.exit(3)` 是 exit 3；#1117 修复误编译，`tests/guard_exit_gate_test.rs` 以绝对结果固定）。

测试：`spec/lang/fallible_marker_test.almd`（guard else err）、`tests/guard_exit_gate_test.rs`。

## 7. match 穷尽性

包含 Result / Option 在内，match 缺少分支（无 err / none 分支）是**硬错误 E010**，不是警告，并提示添加分支。

测试：`tests/diagnostics/`（E010 系 fixture）。

## 8. 错误值原则（ADR-0004）

### 8.0 双层模型——置于哪一层（ADR-0012 D1）

选择错误类型 `E` 不是偏好，而是**分配层次**。此段为标准规则，以前须阅读 ADR-0002（通道）/ 0003（转换）/ 0004（原则）/ 0012（终态）四篇才能重建。

| 层 | `E` | 通道用途 | 何时选择 |
|---|---|---|---|
| **擦除层（默认）** | `String` | **报告**——供人和 LLM 阅读 | 默认，调用方不按内容分支时均适用 |
| **精化层（显式选择）** | variant | **分支**——程序按内容改变行为 | 希望按内容分支时（ADR-0004 D1），且适用领域**封闭** |

- **擦除层**添加上下文前缀的规范形式为 `map_err`，分隔符是 `": "`（ADR-0004 D2）。
- **精化层**的“封闭领域”指**能够完全处理该错误的模块或包**内部（与 SE-0413 三条件采用相同范围）。离开领域时，保留 variant 的理由消失。
- **降级**（variant `E` → String）在边界上**必须可见为 `map_err`**（ADR-0003）。没有隐式转换钩子，因此降级发生位置保留在代码中。

这被表述为**终态**。推翻需要 ADR-0004 / ADR-0012 的 Falsifiers 指定的**实测证据**，仅有新意见不够。

### 8.1 规则

- 错误值收敛到 String（不引入标签或 error set）。
- **不要按错误消息正文分支**；使用 `string.contains(e, ...)` / 相等比较的分支条件产生 E035 警告。需要分支则定义 variant E，并按结构 match。
- `map_err` lambda 不使用错误参数会产生 E036 警告（有意丢弃写作 `(_) =>`）。
- 上下文前缀的规范形式：`r |> result.map_err((e) => "context: ${e}")`（分隔符 `": "`）。
- 收集所有错误使用 `result.partition`。`result.collect` / `collect_map` 已移除（ADR-0007）。
- fs 的“不存在”是值：`fs.read_text_if_exists(p)! ?? default`（C-215）。

测试：`tests/diagnostics/e035-*/`、`e036-*/`、`spec/stdlib/fs_if_exists_test.almd`。

## 9. 非目标——考虑过但不采用（ADR-0012 D4）

以下不是“尚未做”，而是已决定**不采用**的内容，均基于 2026 年其他语言实测及实践报告再次确认（来源见 ADR-0012 References）。列出它们是因为相同提案周期性返回；再次提出时，需要反驳以下理由。

1. **stdlib 的 `E` 保持 String**。MoonBit 最新推荐的 parse API 本身使用普通 `raise` + 消息字符串，是否结构化在其仓库仍未决定（core PR #3997）。Unison 生态收敛于综合 `Failure`。
2. **不引入转换钩子（`From` 等价物），也不引入 union / row / error set**。Koka 的 row 推断把内部类型变量名泄漏到用户错误消息（2025 年实践报告）；Roc 的 open/closed union 是其最难的推断概念；Zig error set 依赖闭世界编译；Rust provider API 被 RFC 3885（2026-02）替换并废弃。
3. **传播保持显式 `!`**（ADR-0008）。MoonBit 让传播隐式，再由 IDE 下划线补偿——“IDE 自动给可能 throw 的函数加*下划线*”。此补偿**在原始文本中不存在**，而原始文本是 LLM 和 diff 评审者唯一拥有的接口。
4. ~~lambda 失败通道保持 String~~——**由 ADR-0021 撤回**（见 L3）。按上下文 → 操作数 join → String 的两阶段顺序决定，不使用 row 变量或泛化。
5. **`main` 的 `E` 保持 String**。边界降级的 `map_err` 是可见点，去掉它就看不见降级。

测试：`spec/lang/fallible_marker_test.almd`、`tests/diagnostics/e035-*/`、`e036-*/`、`spec/lang/fallible_typed_error_test.almd`（`T!E` 已实现规格见 §1）。
