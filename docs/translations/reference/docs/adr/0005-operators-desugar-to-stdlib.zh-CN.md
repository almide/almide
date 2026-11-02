<a id="adr-0005-value-level-operators-are-desugarings-of-named-stdlib-functions"></a>

# ADR-0005：值级运算符是命名 stdlib 函数的脱糖形式

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0005-operators-desugar-to-stdlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0005-operators-desugar-to-stdlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0005-operators-desugar-to-stdlib.zh-CN.md)

> 固定历史决策的非规范性读者副本，以原文为准，不自动同步。保留记录中的测量、比较和实现主张，但未独立重新验证。代码示例及注释逐字保留，包括原有语言。 实现状态、拟议诊断编号、弃用、测试等价性主张与 MSR 可推翻条件均记录该决策时点的情况，不是新验证的当前行为。有关 LLM 先验及自修复的强主张是原文理由，不是这里的独立测量。本原则明确排除提前 return 的控制流运算符。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md)

- **状态**：Accepted
- **日期**：2026-08-05
- **决策范围**：值级运算符（`??` / `?` / `?.`）与 stdlib 组合子的关系——消除重复的方针、语法糖定义、多余写法的删除、no-op 的处理。
- **相关**：[ADR-0004](../../../../adr/0004-error-branchability-doctrine.md)（“不要为同一件事增加第二种写法”原则——否决 result.context）、REJECTED_PATTERNS.md（`??` 记述矛盾）、BUG 台账（管道 × `??` 错误编译）。
- **经过**：2026-08-05，对错误表面 matrix 中发现的四项问题逐一针对具体事项作出是非裁定：`??` 与 `unwrap_or`/`unwrap_or_else` 的三重写法；`??`“仍写作已否决却已实现”的矛盾；`?.` 已实现却无文档；Option 上 `?` 的静默恒等操作。

<a id="context"></a>

## 背景

fallback 有三种写法，且彼此没有定义关系就并存：

```almide
port_opt ?? 8080                              // 演算子(lazy)
option.unwrap_or_else(port_opt, () => 8080)   // 関数(lazy)— 意味は ?? と同一
option.unwrap_or(port_opt, 8080)              // 関数(eager)— default が常に評価される
```

此外，REJECTED_PATTERNS 仍将 `??` 写作“否决”（其实已实现并使用）；`?.` 能运行甚至有专用诊断，却无文档（诊断代码也未编号）；`o?`（Option 上的 `?`）静默执行恒等映射。

先例的对照：

- **Rust / Haskell**：运算符是函数的语法糖，函数是实体（`x?` → `Try::branch`、`a + b` → `Add::add`；Haskell 中 `(+)` 是可传递的值）。
- **C# `??` / Kotlin `?:`**：没有对应实体函数的孤立运算符。

<a id="decision"></a>

## 决策

**值级运算符定义为到命名 stdlib 函数的脱糖。函数是实体，运算符是表面；两者不是“两种写法”，而是定义关系。没有定义关系的多余写法予以删除。控制流运算符（`!`、auto-`?`）是提前 return，无法拥有实体函数，因此明确位于本原则边界之外。**

<a id="d1-糖衣定義の表contract-テストで等価を固定"></a>

### D1. 语法糖定义表（以 contract 测试固定等价性）

```almide
x ?? d      ≡  option.unwrap_or_else(x, () => d)          // Option operand
r ?? d      ≡  result.unwrap_or_else(r, (_) => d)          // Result operand(E 破棄は (_))
a ?? b ?? c ≡  右結合でネスト
r?          ≡  result.to_option(r)                          // Result → Option
o?.x        ≡  option.map(o, (v) => v.x)                    // Option のフィールド安全アクセス
```

每行设置等价性 contract fixture（native ⇄ wasm，运算符形式与函数形式输出一致）。

<a id="d2--の公式化"></a>

### D2. 将 `?.` 正式化

将已实现、已实测的 `?.` 纳入规范（`some(P{x:5})?.x ?? 0` → 5，`none` → 0）。仅限 Option——对 Result 的误用，为现有专用诊断（目前无代码）**分配 E-code**。从 Result 可组合为 `(r?)?.x`（正是 D1 两个定义的组合）。

<a id="d3-option-への-no-op--は警告"></a>

### D3. 对 Option 上的 no-op `?` 发出警告

`o?`（operand 已为 Option）是定义域外的恒等映射。不再默许，改为警告：

```
warning[W0xx]: この ? は何もしません(operand は既に Option です)
  = help: ? は Result → Option の変換です。取り除いてください
```

重构（返回类型 Result → Option）时编译仍能存活（MSR），同时使无效运算符作为待清理项可见。

<a id="d4-eager-unwrap_or-は削除option--result-両方"></a>

### D4. 删除 eager `unwrap_or`（option / result 两者）

它是没有定义关系的第三种写法。先 deprecated，再删除（遵循 E040 窗口的先例，经过警告期）。删除后的 E002 必须附带自修复 hint：

```
error[E002]: undefined function 'option.unwrap_or'
  hint: fallback は ?? を使ってください: port_opt ?? 8080
        関数形が必要なら option.unwrap_or_else
```

<a id="d5-rejected_patterns-の--記載を訂正"></a>

### D5. 修正 REJECTED_PATTERNS 的 `??` 记述

“否决”→“作为 `unwrap_or_else` 的语法糖采纳（ADR-0005）”。由此关闭标记的矛盾。

<a id="d6-パイプ---miscompile-の修正方針"></a>

### D6. 管道 × `??` 错误编译的修复方针

通过使实现安排在管道转换**之前**执行 D1 脱糖，从结构上消除 BUG 台账中管道内 `??` 的静默错误编译（native≠wasm、错误值、exit 0）。修复 PR 遵循此方针。

<a id="rationale"></a>

## 理由

- **对“没有实体的运算符”的不适感是裁定起点**：删除函数、仅保留运算符的方案（C# 型）会使运算符成为不能用于高阶或管道位置的孤儿。Rust/Haskell 型（定义关系）与 ADR-0004 的“写法只有一种”原则兼容——语法糖不是第二种写法，而是同一个定义的表面。
- **`??` 的惰性与 `unwrap_or_else` 完全一致**，因此不是事后牵强附会的定义。丢弃 E 的 `(_)` 是 ADR-0004 D3-(b) 确定的意图表达写法，直接出现在定义中。
- **`?.` 在家族中具有最强先验**（JS/TS/Swift/Kotlin/C# 都有）。没有理由将已实现的功能置于无文档状态。
- **eager 版本存在的唯一理由是“短”**：若 default 一侧有作用或成本，eager 更差（始终求值）。带 hint 的 E002 让错误写法能自修复。

<a id="alternatives--検討して却下した案"></a>

## 备选方案——已考虑并否决

1. **删除函数，统一为运算符**（C# / Kotlin 型）：`??` 成为孤立运算符，缺乏实体函数的不适感（裁定时提出）仍在，也失去高阶形式。**否决**。
2. **明确并固定三种写法共存**：无定义关系的并列违背 ADR-0004 原则。**否决**。
3. **废除 `?.`**：先验最强且已实现，删除会让 LLM 的反射每次都遇到错误。**否决**。
4. **将默许 `o?` 写入规范**：允许 no-op 堆积。警告可兼顾 MSR（存活）与清理可见性。**否决**。
5. **维持现状**（矛盾记述、无文档、三重写法）：不可接受，**否决**。

<a id="consequences"></a>

## 后果

- fallback 写法收敛为“`??`（表面）+ `unwrap_or_else`（实体）”的定义对。`unwrap_or` 经过警告期后消失（破坏性变更——通过发布说明和 hint 迁移）。
- 规范纳入语法糖定义表（D1），以 contract 固定等价性——通过机械方式消除运算符与函数语义分歧的空间。
- `?.` 文档化、E-code 编号、`o?` 警告、REJECTED_PATTERNS 修正纳入 SPEC / 诊断批次。
- 管道 × `??` bug 修复的设计方针确定（D6）。

<a id="falsifier"></a>

## 可推翻条件

1. **若发现先脱糖的实现与当前 `??` 可观察行为不一致的案例**（优先级、求值顺序的相互作用），则将 D1 从“实现脱糖”退回“文档等价 + contract”，并在规范中明确差异。
2. **若删除 `unwrap_or` 后，即使有 hint，dojo 的 MSR 仍显著恶化**，则撤回 D4，恢复为 deprecated 别名。
3. **若实测发现 `?.` 的嵌套或方法调用形式与 D1 的 option.map 定义不一致**，则修正定义，或在规范中明确限定适用范围为字段访问。

<a id="references"></a>

## 参考资料

- Rust — [The question mark operator](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)、[`Add`](https://doc.rust-lang.org/std/ops/trait.Add.html)（运算符 = trait 函数的语法糖）
- C# — [?? operator](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/operators/null-coalescing-operator)、Kotlin — [Elvis operator](https://kotlinlang.org/docs/null-safety.html#elvis-operator)（无实体函数运算符的对照）
- TypeScript — [Optional chaining](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-3-7.html#optional-chaining)（`?.` 的先验）
- 内部：ADR-0004（统一写法原则、`(_)` 的意图表达）、2026-08-05 matrix（`coalesce/*`、`qop/optional-chaining/*` 单元、管道 × `??` bug）、REJECTED_PATTERNS.md（D5 修正对象）
