<!-- description: 通过 where 子句定义编译期前置条件和类型不变量 -->
<a id="compile-time-contracts"></a>

# 编译期契约

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](compile-time-contracts.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](compile-time-contracts.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](compile-time-contracts.zh-CN.md)

> 非规范性历史阅读快照；固定原文为权威且不变，不自动同步。未执行引用命令、测量或编译器验证。契约语法、标志及 lowering 是提案而非已交付功能；开头提及后置条件与后续排除、lowering 前后的不同描述均按原文保留。LLM 准确性断言是设计预期，不是新测量结果。linking v2 仍是提案；历史数量、bug 与 host 描述不是当前普查或普遍保证。保留原文开头对 embedded Wasm host 支持的限定。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md)

**优先级：** 2.x — 类型系统稳定之后
**原则：** 降低 LLM 生成“能运行但不正确”代码的概率。用类型检查加额外约束提高 modification survival rate。
**状态：** 通用函数前置条件及类型值不变量仍是提案。下文的 `newtype`／函数 `where` 形式是最初设计草图，不是当前语法。`where` 已存在于测试上下文；已交付的内联 generic protocol bound 是另一功能（见 [type-where-constraints.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md)）。

> “类型保证它是什么，契约保证它落在哪个范围内。”

---

<a id="why"></a>

## 为什么需要

Almide 类型检查器能防止“需要 Int 却传 String”，但不能防止“禁止传 0 却传了 0”。

```almd
fn divide(a: Int, b: Int) -> Int = a / b

// If the LLM writes this, it type-checks. Zero division at runtime
let x = divide(10, 0)
```

契约让函数前置条件、后置条件及类型不变量可以在编译期验证。不使用 SMT solver，仅限编译器可静态求值的谓词；无法求值时退回运行时检查。

<a id="effect-on-modification-survival-rate"></a>

### 对 Modification Survival Rate 的影响

LLM 修改代码时：
1. 类型错误 → 立即检测（**当前状态**）
2. 契约违反 → 编译错误或立即在运行时检测（**本提案**）
3. 逻辑 bug → 无法检测（任何语言都仍有）

**契约填补 1 和 3 之间的空白。** 边界条件（除零、越界访问、传负数）是 LLM 常犯错的模式，与契约可捕获的范围重合。

---

<a id="design"></a>

## 设计

<a id="function-contracts"></a>

### 函数契约

```almd
fn divide(a: Int, b: Int) -> Int
  where b != 0
= a / b

fn clamp(value: Int, lo: Int, hi: Int) -> Int
  where lo <= hi
= if value < lo then lo
  else if value > hi then hi
  else value
```

- `where` 写在函数签名和函数体之间
- 多条件用逗号分隔：`where b != 0, lo <= hi`
- 条件只能引用参数，不能引用函数体局部变量
- 调用带 `where` 的函数时，编译器静态验证条件成立

<a id="proposed-type-contracts-invariants-original-newtype-syntax-sketch"></a>

### 提议的类型契约（不变量；最初的 `newtype` 语法草图）

```almd
type Percentage = newtype Int
  where self >= 0, self <= 100

type NonEmpty[T] = newtype List[T]
  where self.len() > 0

type Port = newtype Int
  where self >= 0, self <= 65535
```

- 与 `newtype` 组合使用
- `self` 引用值本身
- 构造时验证契约

<a id="static-verification-and-dynamic-degradation"></a>

### 静态验证与动态退化

编译器分三个层次处理契约：

| 判断 | 动作 | 示例 |
|----------|--------|---------|
| **静态为真** | 删除检查 | `divide(10, 3)` — 字面量 3 != 0 显然成立 |
| **静态为假** | 编译错误 | `divide(10, 0)` — 字面量 0 != 0 为假 |
| **未知** | 插入运行时检查 | `divide(a, b)` — b 的值到运行时才知道 |

静态验证范围：
- 字面量求值
- 常量折叠（`const` 传播）
- 简单范围分析（if 分支中成立的条件）
- guard 后的条件传播

```almd
// Statically verifiable: after guard, b != 0 is guaranteed
effect fn safe_divide(a: Int, b: Int) -> Result[Int, String] = {
  guard b != 0 else { return err("zero division") }
  ok(divide(a, b))  // ← compiler knows b != 0
}

// Cannot be statically verified: runtime check is inserted
let result = divide(x, y)
// ↓ Compiler-generated code (conceptual)
// if !(y != 0) { panic("contract violation: b != 0 at divide()") }
// let result = x / y
```

**为什么不用 SMT solver：**
- 编译时间不可预测（Z3 最坏情况为指数级）
- 真实代码常得到“未知”，损害使用体验
- LLM 未必写出适合 SMT 的谓词
- 静态验证失败时运行时检查已足够：“立即崩溃”远好于“能运行但不正确”，也更易调试

<a id="cognitive-load-for-llms"></a>

### LLM 的认知负担

`where` 子句：
- 不用也能运行（opt-in）
- 该写在哪里很明确（除零、越界、空列表）
- 语法是函数签名的自然扩展

LLM 学习成本：最小。CHEATSHEET.md 加几行便足以支持生成。
LLM 把 `where` 写错的概率：低。条件表达式与 `if` 语法相同，没有新概念。

---

<a id="proposed-lowering-not-existing-contract-codegen"></a>

## 提议的 lowering（不是已有契约 codegen）

<a id="rust"></a>

### Rust

```rust
fn divide(a: i64, b: i64) -> i64 {
    debug_assert!(b != 0, "contract: b != 0");
    a / b
}

// newtype
struct Percentage(i64);
impl Percentage {
    fn new(value: i64) -> Percentage {
        debug_assert!(value >= 0 && value <= 100, "contract: 0..=100");
        Percentage(value)
    }
}
```

- `debug_assert!` 仅在 debug build 检查，在 release build 删除
- 或通过 `--contracts=always` 标志在 release build 启用

<a id="original-typescript-alternative-historical-only-target-retired"></a>

### 最初的 TypeScript 备选方案（仅历史；目标已退役）

```typescript
function divide(a: number, b: number): number {
    if (!(b !== 0)) throw new Error("contract violation: b != 0");
    return Math.trunc(a / b);
}
```

- 这只是运行时检查策略示例，不是当前编译器目标，也不是针对所有 JavaScript engine 优化的断言。

<a id="wasm"></a>

### WASM

```wasm
(func $divide (param $a i64) (param $b i64) (result i64)
  local.get $b
  i64.eqz
  if
    unreachable  ;; contract violation
  end
  local.get $a
  local.get $b
  i64.div_s
)
```

---

<a id="scope-limitations--what-we-wont-do"></a>

## 范围限制 — 不做什么

| 不做 | 原因 |
|---|---|
| 后置条件（`ensures`） | 多数情况下返回值验证更适合测试，不值得付出语法成本 |
| 量词（`forall`, `exists`） | 需要 SMT，编译时间不可预测 |
| 依赖类型 | 类型级编程降低 LLM 准确性 |
| 带副作用的条件 | `where` 仅允许 pure fn 调用（len、is_empty 等） |
| 循环不变量 | 需要验证器，超出契约范围 |

**`where` 仅限前置条件和类型不变量。** 此限制确保无需 SMT，LLM 也能准确编写。

---

<a id="relationship-with-existing-features"></a>

## 与现有功能的关系

| 现有功能 | 关系 |
|---|---|
| `guard` | 运行时提前 return；契约是 guard 的“编译期提升版” |
| `effect fn` | pure fn 和 effect fn 都能附加契约 |
| Wrapper type | 提议的值不变量需要构造／验证契约；旧 `newtype` 草图不是当前语法 |
| 类型检查器 | 契约验证在类型检查之后、lowering 之前 |
| nanopass | 提议的 `ContractCheckPass`，未确定为已实现 |

<a id="complementary-relationship-with-guard"></a>

### 与 guard 的互补关系

```almd
// guard: checks condition at runtime, early return on failure
effect fn parse_port(s: String) -> Result[Port, String] = {
  let n = int.parse(s)!
  guard n >= 0, n <= 65535 else { return err("invalid port") }
  ok(Port(n))
}

// contract: the Port type itself guarantees 0..65535
type Port = newtype Int
  where self >= 0, self <= 65535

// Construct Port from guard-verified value → contract statically satisfied
```

guard 用于“验证用户输入”，契约用于“把已验证值的性质固化到类型中”。两者正交，组合使用。

---

<a id="implementation-sketch"></a>

## 实现草图

<a id="phase-1-parser--checker"></a>

### Phase 1：Parser + Checker

- 定义不与既有测试上下文 `where` 冲突的函数契约语法
- Parser：实现采纳的函数／类型契约语法；原 `newtype` 拼法不是当前前提条件
- AST：向 FnDecl / TypeDecl 加 `WhereClause { conditions: Vec<Expr> }`
- Checker：验证 where 条件表达式返回 Bool
- Checker：限制 where 条件可引用变量（仅参数或 self）

<a id="phase-2-static-verification"></a>

### Phase 2：静态验证

- lowering 后插入 `ContractCheckPass` nanopass
- 字面量参数常量求值
- guard / if 分支后传播条件（简单数据流）
- 静态为假 → 诊断错误
- 静态未知 → 向 IR 插入运行时检查节点

<a id="phase-3-codegen"></a>

### Phase 3：Codegen

- Rust：生成 `debug_assert!`
- 无 TS 实现目标；原 TS backend 草图仅为历史
- WASM：生成 `if ... unreachable`
- `--contracts=always` / `--contracts=debug` / `--contracts=off` 标志

<a id="phase-4-diagnostic"></a>

### Phase 4：诊断

- 契约违反错误消息：`contract violated: b != 0 at divide()`
- 显示调用方代码位置
- 提示：插入 `guard b != 0 else { ... }`，或更改参数

---

<a id="success-criteria"></a>

## 成功标准

- `divide(10, 0)` 变为编译错误
- 无 guard 时 `divide(10, n)` 带运行时检查编译
- 采纳的受约束类型语法验证 0..100 值的构造（原 `newtype ... where` 拼法仍是草图）
- 所有既有测试通过（没有 `where` 的代码完全不受影响）
- CHEATSHEET.md 新增不超过10行
- 验证 LLM 可使用 `where` 正确生成代码

<a id="why-on-hold"></a>

## 为什么保留 ON HOLD

- 首先需要类型系统和 nanopass pipeline 稳定
- 避免与 Phase 3（Effect System）冲突
- 不在 1.0 范围内，语言规格冻结后再考虑
- 重新研究值不变量如何组合当前 wrapper type 和 generic bound；不假定最初的 `newtype` 设计已经交付
