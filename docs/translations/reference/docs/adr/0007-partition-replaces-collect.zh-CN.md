<a id="adr-0007-resultcollect-is-removed-partition-is-the-substance-for-all-errors-collection"></a>

# ADR-0007：移除 result.collect；partition 是收集全部错误的底层操作

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0007-partition-replaces-collect.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0007-partition-replaces-collect.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0007-partition-replaces-collect.zh-CN.md)

> 固定历史决策的非规范读者副本。原文仍是权威版本，本副本不会自动同步。保留记录中的测量、比较和实现主张，但未独立复核。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0007-partition-replaces-collect.md)

- **状态**：Accepted
- **日期**：2026-08-05
- **决策范围**：收集全部错误（all-errs）类 API 的写法——`result.collect` / `result.collect_map` 的存废及其与 `partition` 的关系。
- **相关**：[ADR-0004](../../../../adr/0004-error-branchability-doctrine.md)（习惯用法文档化与证据门槛的先例——拒绝 result.context）、[ADR-0005](../../../../adr/0005-operators-desugar-to-stdlib.md)（底层操作与语法糖的定义关系）、[ADR-0006](../../../../adr/0006-fallibility-polymorphic-hofs.md)（first-err 侧融入 `list.map(f)!`）。
- **经过**：源于 2026-08-05 拆解 try_ 的调查中发现的先验预期冲突。通过是非裁定，决定“现在执行 C（删除），B（validate）设证据门槛”。

<a id="context"></a>

## 背景

`result.collect` 的含义与 **Rust 最强的先验预期恰好相反**：

```almide
// Rust の collect(std docs 実引用済み): 最初の Err で打ち切り、E は E のまま
//   [Ok(2), Ok(4), Err("err!"), Ok(8)].collect() == Err("err!")

// almide の result.collect: 全走査・全エラー収集、E は List[E] に変わる
result.collect(rs)   // List[Result[T, E]] -> Result[List[T], List[E]]
```

`option.collect` 与 Rust 的行为一致，形成只有 result 侧违背预期的不对称。此外，ADR-0006 之后 first-err 的标准形式成为 `list.map(f)!`，因此 Almide 永远不需要以“collect”之名提供 first-err。

底层操作已经存在：`result.partition`（intrinsic）。collect 只封装 partition 和一个判断：

```almide
let (oks, errs) = result.partition(rs)
if list.is_empty(errs) then ok(oks) else err(errs)   // ← これが collect の全意味
```

用量实测（2026-08-05）：仅自身测试、2～3 个 fixture 和文档。迁移成本低。

<a id="decision"></a>

## 决定

<a id="d1-resultcollect--resultcollect_map-は-deprecated--削除"></a>

### D1. `result.collect` / `result.collect_map` 先弃用，再删除

删除后的 E002 必须附带指向 partition 习惯用法的自我修复 hint：

```
error[E002]: undefined function 'result.collect'
  hint: 全エラー収集は partition で書きます:
        let (oks, errs) = result.partition(rs)
        if list.is_empty(errs) then ok(oks) else err(errs)
```

<a id="d2-partition-が唯一の実体2-行イディオムを-cheatsheet-に正準形として明記"></a>

### D2. `partition` 是唯一的底层操作；在 CHEATSHEET 中把两行习惯用法明确为标准形式

与 map_err 添加上下文前缀的习惯用法（ADR-0004 D2）同等对待。冲突词“collect”从 result 模块消失。

<a id="d3-validate--validate_map-は証拠ゲート付きで予約今は追加しない"></a>

### D3. `validate` / `validate_map` 设证据门槛后保留候选（现在不添加）

以用途命名的语法糖（Scala cats Validated / Kotlin Arrow mapOrAccumulate 一系），与拒绝的 result.context（ADR-0004 Alternatives 7）同属“为习惯用法命名”，遵循相同规则：现在只写入文档，出现实际损害证据后再考虑（可证伪条件 1）。已经确认，判断条件反转（颠倒 `is_empty` 的 if 分支）不会被类型系统捕获——这可能成为未来证据。

<a id="d42026-08-07-追記-option-側は現状維持を正式決定"></a>

### D4（2026-08-07 追加）：正式决定 option 侧保持现状

移除 `result.collect` 的理由是“语义与 Rust 最强先验预期**恰好相反**”，而不是“collect”这个词本身。实测 option 侧没有违背预期，不能以同一理由移除。因此正式决定以下事项，并关闭 #1135 集群 4：

- **保留 `option.collect` / `option.collect_map`**。`[some(1), none, some(3)]` 遇到 first-none 就停止并返回 `none`，与 Rust 的 `Option` collect 一致。名称与含义相符的写法，没有仅为对称而删除的理由（D1 的依据是“名称说谎”，而非“名称相同”）。
- **`option.to_result` 的 E 仍固定为 String**。不引入通用 `ok_or`（E 多态），这是对 ADR-0002 D2（E 固定为 String）和 ADR-0004（自定义 E 显式写成 `Result[T, MyError]`）的一致应用。需要自定义 E 时写 `match o { some(v) => ok(v), none => err(MyErr(..)) }`。
- 最终 first-err/all-errs 的“命名约定”以**不创建约定**收束：每个函数有忠实表达自身含义的名称，`result` 侧 all-errs 由 `partition`（底层操作）负责，first-err 由可失败性多态 `list.map(xs, (x) => f(x)!)!`（ADR-0006）负责。

可证伪条件：若 Dojo MSR 测得把 option.collect 的 first-none 停止误读成 all-none 收集的实例，或通用 `ok_or` 缺失无法通过显式 match 绕过的实例，则重新考虑。

<a id="rationale"></a>

## 理由

- **与先验预期相反的名称每次都会收取成本**：即使 check 能凭类型差异（List[E] 与 E）捕获误用，每次纠正 Rust 惯性误读仍是永久成本。
- **一致应用写法最小化**：已有底层操作（partition），却存在未文档化其定义关系的语法糖（collect），按 ADR-0005 原则也应整理。
- **与 context 先例一致**：validate 与 context 都有“给习惯用法起用途名称”的诱惑。同一天删一个、加另一个，需要区分原则，但这条界线只能任意划定，因此采用相同证据门槛。

<a id="alternatives--検討して却下した案"></a>

## 备选方案——考虑后拒绝

1. **改名为 `collect_all`**：冲突根源“collect”仍在，误读风险最多减半。**拒绝**。
2. **立即改名为 `validate`**：无法说明与拒绝的 result.context 有何区别。**拒绝**（D3 将其列为带证据门槛的候选）。
3. **保留名称并在文档注释**：写“与 Rust 不同”也无法消除惯性。**拒绝**。

<a id="consequences"></a>

## 后果

- result 模块的表面减少两个函数，all-errs 统一到 partition。
- 迁移对象：spec 测试、wasm_cross fixture（同一 PR 中重新关联契约）、monkey 场景和文档。intrinsic `almide_rt_result_collect_map` 也删除。
- option 侧保持现状（`option.collect` 与 Rust 先验一致，没有冲突；但 ADR-0006 落地后，要在 Phase 2 设计中确认与多态形式 `list.map(f)?`… 的关系）。

<a id="falsifier"></a>

## 可证伪条件

1. **若 dojo 中验证用途频繁出现，同时实测到 partition 习惯用法的编写错误（条件反转等）**，则打开 D3 门槛，提出将 `validate` / `validate_map` 添加为命名函数的 ADR。
2. **若迁移到 partition 后，dojo 相应任务的 MSR 相比旧 collect 显著下降**，则恢复直接返回 Result 的单一函数（与 D3 一并决定名称）。

<a id="references"></a>

## 参考

- Rust — [std::result: Collecting into Result](https://doc.rust-lang.org/std/result/index.html)（已取得 first-err 的实际引用）。
- Scala cats — [Validated](https://typelevel.org/cats/datatypes/validated.html)、Kotlin Arrow — [mapOrAccumulate](https://arrow-kt.io/learn/typed-errors/working-with-typed-errors/)（D3 的谱系）。
- 内部：ADR-0004（context 先例）/ 0005 / 0006，本文用量实测，以及 `stdlib/result.almd` 的 partition intrinsic。
