<a id="adr-0010-t-is-general-option-shorthand--every-type-position-atom-tight-binding-fmt-normalized"></a>

# ADR-0010：T? 是通用 Option 简写——所有类型位置、紧绑定于原子、由 fmt 规范化

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0010-option-type-shorthand.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0010-option-type-shorthand.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0010-option-type-shorthand.zh-CN.md)

> 固定历史决策的非规范读者副本，不是新的权威版本，也不会自动同步。历史测量和实现主张未在此独立复核。原文称 `Option[T]` 为“19 个字符”；这里保留这一疑似计数不一致之处，不擅自修正。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0010-option-type-shorthand.md)

- **状态**：Accepted
- **日期**：2026-08-06
- **决策范围**：类型记法 `T?` 的采用、有效位置、绑定规则和标准形式（fmt 规范化方向）。
  表达式运算符 `?`（Result→Option）/`??`/`?.` 不在范围内（已由 ADR-0005 确定）。
- **相关**：[ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md)（`-> T!` 的先例；其 Alternatives 5 将 `T?` 列为范围外、另行讨论是否采用，本记录处理的就是该问题），
  [ADR-0009](../../../../adr/0009-fn-type-quadrant-transparency.md)（fn 类型槽中的 `!`——统一绑定方向）。
- **经过**：2026-08-06，v0.54.0 发布后，通过三个是非裁定问题（Q1 范围 / Q2 绑定 / Q3 规范化）确定。

<a id="context"></a>

## 背景

`-> T!`（ADR-0002）把可失败性缩成了一个字符，但缺失（Option）仍是 19 个字符的 `Option[T]`。语料实测（2026-08-06）：

| Option 注解位置 | 数量 |
|---|---|
| 返回位置 `-> Option[T]` | 387 |
| 参数、字段位置 `: Option[T]` | 331 |
| 泛型参数位置 `[Option[T]]` | 147 |
| 嵌套 `Option[Option[T]]` | 45 |
| fn 类型槽的返回 `(A) -> Option[B]` | 52 |
| 包裹 fn 类型本身 `Option[(A) -> B]` | **0** |

`!` 表示“调用函数的可失败性”，是箭头的属性，因此限制在返回位置是正确的。
`?` 表示“值的缺失”，是类型的属性；实测也有过半（478/865）位于返回位置之外。

<a id="decision"></a>

## 决定

**D1（Q1 是）：采用 `T?` ≡ `Option[T]`，作为适用于所有类型位置的通用类型语法糖。**
不限定在返回位置——依赖位置的记法会重演 v0.51.0 已消除的表面规则违例：编写者概括出的规则换个位置就失效。指称规则固定为一行：`T?` 与 `Option[T]` 指称相同（类似 `16` 与 `0x10`）。

**D2（Q2 是）：`?` 最紧密地绑定紧前方的类型原子，不跨越 `->`。**
类型原子是名称（可带泛型及模块限定）或由括号封闭的类型。
整个 fn 类型和嵌套 Option 使用括号明确表示：

```almide
f: (Int) -> Int?            // = (Int) -> Option[Int]   実在 52 件の形
on_tick: ((Int) -> Unit)?   // = Option[(Int) -> Unit]  実在 0 件の形に括弧税
pair: (String, Int)?        // = Option[(String, Int)]  タプルは括弧済みアトム
nested: (Int?)?             // = Option[Option[Int]]    `Int??` は ?? にレクスされるため不可
fn parse_opt(s: String) -> Int?!   // = Result[Option[Int], String]  ? が先、! は戻りマーカー
```

**D3（Q3 是）：fmt 将 `Option[T]` → `T?` 规范化（所有类型位置）。**
标准形式是短形式。混用直接影响 LLM 可编写性（语料一致性 = 生成一致性），因此沿 v0.53.5 的 one-name-one-meaning 路线收敛为一种形式。嵌套、fn 类型和记录类型的内部按可重新解析的要求加括号输出。

**D4（2026-08-07 更新：永久拒绝）**：**永久不进行** `Result[T, String] → T!` 的 fmt 规范化——从暂缓（等待证明）改为拒绝。实测证据：#1108 的 1-bit 规则将 callback bit 放在**声明标记**上，因此即使函数体相同，`list.map(xs, f)` 的语义也会因 f 的写法分叉（长形式 = map-of-results `[ok(1), err(…)]` / `T!` = 遇到首个 err 即停止）。机械替换会把所有 map-of-results 变成 first-err。这种分叉是规范的一部分，由 spec/lang/fallible_hof_test.almd 中“the marker, not the type, carries the callback bit”固定。因此从 0.55 起，这不是“一种类型、两种记法”，而是**编写者选择策略的两种声明**。T? 没有 HOF bit，因此不会产生同类问题；T? 规范化的健全性不变。

<a id="rationale"></a>

## 理由

- 短形式覆盖全部实测分布（返回 387 / 参数 331 / 泛型 147），只有出现次数为 0 的形式承担括号成本。
- `!`（ADR-0009：可失败性属于箭头的返回）与 `?` 的绑定方向一致。
- 实现与 ADR-0002 Phase 1a 的机制完全同构：伪泛型 `Generic { name: "?" }` → resolver 中的 `Ty::option(T)`，fmt 重新输出表面写法。下游（checker/IR/codegen/mir）只看到 Option——跨目标行为没有变化。

<a id="alternatives--検討して却下した案"></a>

## 备选方案——考虑后拒绝

1. **仅从返回位置开始**（与 `T!` 对齐）：新增依赖位置的规则，排除 478 个实测用例，还要教“在 `->` 后可写、在 `:` 后不可写”。**拒绝**。
2. **`?` 附着整个 fn 类型**（`(A) -> B?` = Option[fn]）：把 0 个实测用例的形式设为无括号，却让 52 个实际用例承担括号成本，分配颠倒。**拒绝**。
3. **不做 fmt 规范化**（两种写法共存）：迁移差异为零，但混用持续，模型所读语料分裂成两种形式。**拒绝**。
4. **对 `T??` 特殊词法分析**（在类型上下文拆开 `??`）：给 lexer 引入上下文依赖。嵌套 Option 实测 45 例，括号 `(T?)?` 足够。**拒绝**。

<a id="consequences"></a>

## 后果

**收益**：224 个文件中的 1044 处 Option 注解机械缩短。`?`（缺失）/`!`（可失败）/`?.`/`??` 这一组符号在类型与表达式中对应相同的语义轴。

**代价**：
- 增加语法表面（类型原子后的 `?`）及其教学成本。
- 一次性规范化差异（spec/ + examples/，即 fmt gate 的范围）。
- **stdlib 也已迁移（2026-08-07 追加）**：`almide fmt --no-import-edit` 解决了原先保留不动所依据的 splice-context 风险（自动插入 import），因此对 stdlib 的 258 个文件进行批量规范化，机械确认 import 行没有增减。原先保留长写法作为边界见证的职责，转交给 spec 的合一测试（fallible_marker / option_marker）。

<a id="falsifier"></a>

## 可证伪条件

- 若 Dojo 的 MSR 测量中，fn 类型槽和嵌套场景显著出现对 `?` 绑定的实际误读，则重新裁定 D2 的括号规则。
- 若规范化后手写代码大量退回 `Option[` 写法，则重新考虑 D3 的方向。

<a id="references"></a>

## 参考

- 裁定日志：2026-08-06 会话（Q1/Q2/Q3 均为是）。
- 实测：上表（通过 rg 统计语料，2026-08-06）。
