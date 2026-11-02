<a id="reimpl-lint-signature-match-detection-of-stdlib-reimplementations"></a>

# 重实现 lint：通过签名匹配检测 stdlib 重实现

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](reimpl-lint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](reimpl-lint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](reimpl-lint.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/reimpl-lint.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史元数据：检测签名匹配 stdlib fn 的用户 fn，建议委托；完成日期 — 2026-04-20。

> 原文注意事项：完成节记录 Warning 和 stdlib 侧 TypeVar 的单向通配，保留的旧计划却说 Info、描述不同的泛型匹配并排除双向泛型情况。保留这些历史差异，不悄悄调和。接近零误报的主张及 +1/+2 测量假设是原文理由，不是独立证明的语义等价或新的基准结果。委托 shim 仍需主动接受。

<a id="completion-status-2026-04-20"></a>

## 完成状态（2026-04-20）

已作为 `E015 Possible stdlib reimplementation` 落地 — 从 `check_reimpl_lint`（`crates/almide-frontend/src/check/mod.rs`）触发的 Warning 级诊断。

检测规则：

1. 顶层 `fn`（不是 test / derive / 已单态化函数 / convention method）。
2. 短名称与某个 stdlib 短名称的 Levenshtein 距离 ≤ 2（不区分大小写；已有 `almide_base::diagnostic::levenshtein` helper 现为 `pub`，供跨 crate 使用）。
3. 参数个数、类型以及返回类型结构匹配（`check/mod.rs` 中的 `ty_reimpl_eq`）。stdlib 侧的 `TypeVar` 为通配符（单态用户 fn 匹配泛型 stdlib fn）；反方向不匹配。

三项必须同时成立；任一不满足即抑制建议，使误报率保持接近零。

输出为警告及委托 `try:` shim（`fn <user_name>(args) ->
T = <module>.<stdlib_fn>(args)`）。文档位于 `docs/diagnostics/E015.md`。shim 不自动应用 — 用户或 LLM 重试必须主动接受。

等级：保持 `Warning`，没有向 `Level` enum 添加 `Info`。这符合路线图允许的“info/warning-level”，避免跨 crate 的 variant 级联修改。若 dojo 测量显示警告过于嘈杂，再重新考虑。

<a id="original-plan-retained-for-history"></a>

## 最初计划（保留历史）

触发条件：应用 SYSTEM_PROMPT 的“Prefer stdlib”节后仍无改善，且 dojo 测量表明 `list.binary_search` / `string.run_length_encode` / 其他 stdlib 原语被从头重实现时，再实现此项。

<a id="goal"></a>

## 目标

当用户定义的顶层 `fn` 与名称非常相似的 stdlib 函数**签名完全匹配**时，发出 info/warning 级诊断。诊断指向 stdlib 函数并建议委托，使 LLM 重试能收敛到更短、更惯用的形式。

<a id="scope-discipline"></a>

## 范围纪律

此 lint **不得**误报。用户有自己的理由编写 `list.first` 的特殊版本时，不应被反复打扰。触发条件严格：

- 名称相似度：用户 fn 名与 stdlib fn 短名之间 Levenshtein 距离 ≤ 2，且
- 参数签名：类型逐元素结构相同，且
- 返回类型：结构相同。

三条件都必须成立。签名精确匹配使误报率基本为零：若用户编写 `fn binary_search(xs: List[Int], target: Int) -> Option[Int]`，其意图与 `list.binary_search` *不同*的概率极小。

<a id="example-binary-search-case"></a>

## 示例（二分查找情况）

```almide
// User writes (~30 lines of algorithm):
fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
    let low = 0
    let high = list.len(xs) - 1
    // ... loop implementation ...
```

发出的诊断：

```
info: fn 'binary_search' has the same signature as stdlib `list.binary_search`
  --> binary_search.almd:1:4
  |
1 | fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
  |    ^^^^^^^^^^^^^^^^^
  hint: if this is the standard algorithm, delegate to stdlib:
  try:
      fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
        list.binary_search(xs, target)
```

`try:` 片段是**委托 shim**，不是改写用户函数体。用户（或 LLM 重试）选择保留现有函数体，还是换成单行版本。

<a id="non-triggers-explicitly-must-not-fire"></a>

## 不触发情形（明确不得触发）

- 参数顺序不同（`fn lookup(target: Int, xs: List[Int])`）
- 参数类型不同（`fn binary_search(xs: List[Float], target: Float)`）
- 返回形态不同（未找到时返回 -1 的 `fn binary_search(...) -> Int`）
- 名称距离 > 2（`fn find_by_binary_search`）
- 用户定义了 stdlib 版本没有的泛型（反之亦然）

有疑问就不触发。这是可选优化，不是正确性检查。

<a id="implementation-sketch"></a>

## 实现草图

1. **建表（每个 checker 实例一次）**：收集作用域中每个可见 stdlib 函数的 `(module, fn_name, param_tys, ret_ty, generics)`。来源：`almide_frontend::stdlib::module_functions` + `lookup_sig`。按短名称（模块前缀之后的部分）分组。

2. **逐个用户 fn**：推断后遍历顶层 `Decl::Fn`：
   - 从签名计算 `user_key = (param_tys, ret_ty)`。
   - 查找短名称距用户 fn 名编辑距离 ≤ 2 的候选 stdlib fn。
   - 对每个候选，结构比较 `(param_tys, ret_ty)` 与 `user_key`（见下文）。匹配则发出 info 诊断。

3. **类型的结构相等**：
   - `Ty::Int == Ty::Int`，等等。
   - `Ty::Applied(id_a, args_a) == Ty::Applied(id_b, args_b)` 当且仅当 `id_a == id_b && args elementwise ==`。
   - `Ty::TypeVar("A")` 被视为等于同一位置的另一 `TypeVar`（单态签名应匹配用户侧多态签名 — 不假装进行了泛化）。
   - 忽略 protocol／结构约束（stdlib fn 很少有此类约束；若它有而用户没有，则跳过）。

4. **诊断**：附加作为委托 shim 的 `try_snippet`。严重级别为 `Info`（不是 `Warning`），使其不阻断编译。

<a id="what-this-lint-does-not-do"></a>

## 此 lint 不做什么

- 不按算法形状检测（while + mid + low + high → 二分查找）。那属于另一工具（也许在 Phase 5 作为真正静态分析器实现）；对 Phase 3 太模糊。
- 不改写函数体。
- 不经 `almide fix` 应用 — 用户（或 LLM 重试）必须主动接受 shim。

<a id="measurement-hypothesis"></a>

## 测量假设

若实现并启用：

- 70b：binary-search +1（直接促成本项的案例）。
- 70b：run-length-encoding +1（若签名匹配；目前 stdlib 返回 `List[(String, Int)]`，因此用户侧返回 `String` 的 RLE 不会匹配 — 这是有意设计，参见不触发情形）。
- 8b：效果极小（8b 很少写出与 stdlib 精确匹配的签名，通常会选错类型）。

预计合计：**70b 上 +1 至 +2**。风险较低，因为诊断为 `Info` 级 — 不阻止编译，只引导重试。

<a id="when-to-implement"></a>

## 何时实现

- dojo run #10（集成 `almide fix` 重试循环及 SYSTEM_PROMPT 的“Prefer stdlib”节）相对当前 baseline 的结果 ≤ +2pt。
- 或者：日志直接表明，框架改善后 binary-search／类似任务仍在重实现 stdlib。

否则暂缓 — 测量窗口成本低，实现约为 50 行 checker 代码加测试。

> 保留示例的注释说明用户编写约 30 行算法，并省略循环实现。旧诊断示例指出 binary_search 与标准库 list.binary_search 的签名相同，并给出有条件提示：“如果这是标准算法，就委托给标准库”。
