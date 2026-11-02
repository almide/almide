<a id="adr-0006-one-bit-fallibility-polymorphism-for-hofs-the-try_-family-is-dissolved"></a>

# ADR-0006：HOF 的一位可失败性多态；解散 try_* 家族

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0006-fallibility-polymorphic-hofs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0006-fallibility-polymorphic-hofs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0006-fallibility-polymorphic-hofs.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)
> 这是含后续修订的已采纳 ADR 历史。2026-09-27 修订替换固定 String 的 E 并撤回 D4；2026-10-01 修订扩大可失败 HOF 覆盖，保留列明的 TOTAL_ONLY 例外。早先冻结／删除计划、使用量测量及 MSR 预期仍是历史主张，不是新执行结果或当前兼容性保证。


- **状态**：Accepted
- **日期**：2026-08-05
- **决定范围**：高阶函数（HOF）的 callback 可失败时，如何显露可失败性：专用 `try_*` 函数（现状），还是可失败性多态（callback 的 `!` 流向 HOF）
- **相关**：[ADR-0002](../../../../adr/0002-fallibility-effect-orthogonal.md)（`-> T!`、E=String；本 ADR 是其对 HOF 的推论）、[ADR-0004](../../../../adr/0004-error-branchability-doctrine.md)（String 终点；D4 有意省略的依据）、[ADR-0005](../../../../adr/0005-operators-desugar-to-stdlib.md)（最少写法）、#1103（Phase 1）、#1055（capability 轴；本 ADR 先提取其子集）
- **经过**：2026-08-05 深入探究“不喜欢 `try_` 前缀”的不适感。调查发现 try_* 家族是“用函数填补 rethrows 等价语言功能缺失的临时设施”，经语言比较与使用量实测后决定拆除。

<a id="context"></a>

## 背景

v0.53.6 交付的 `list.try_*` 家族（`try_map` / `try_filter` / `try_flat_map` / `try_filter_map` / `try_fold` / `try_find` / `try_each` 共 7 个函数）是专用单态函数组：“callback 返回 Result、首个 err 即停止、E 不变”。

有三个问题：

1. **`try` 是 Almide 词汇中不存在的借词**，出借方的含义还不一致（Rust `try_fold` = 提前停止；Swift/Zig `try` = 传播 = Almide 的 `!`）。接触 Swift/Zig 的 LLM 可能误解 `try_map`。
2. **表面按策略 × HOF 乘法膨胀**：每加一个 HOF 就要问是否需要 try_ 姐妹函数。
3. Almide 列表是 eager，因此 Rust 式惰性组合（`map(f).collect::<Result<_,_>>()`，collect 在首个 err 停止）**原则上不可用**。try_map 是此约束下产生的融合函数。

<a id="他言語比較一次情報"></a>

### 其他语言比较（第一手来源）

| 语言 | 可失败 map | 机制 |
|---|---|---|
| Swift 6 (SE-0413) | `try xs.map(f)` — 只有一个 map | **可失败性多态**：`map<U, E: Error>(body: (Element) throws(E) -> U) throws(E) -> [U]`。total callback 推断 E=Never，使 map 也为 total |
| Rust | `map(f).collect::<Result<_,_>>()` | 惰性迭代器＋FromIterator（“返回首个 Err”，std 文档明确说明）。闭包不能具有 `?` 多态，因此同时存在 `try_fold` 家族 |
| Zig | 循环＋`try f(x)` | 错误集合透明地跨调用推断 |
| Haskell / Scala | `traverse` | 策略位于类型（Either=停止／Validation=全部收集） |
| Gleam | `list.try_map` | **专用单态函数——缺少类型类和效果条件下的妥协**（与 Almide 现状相同） |
| OCaml 5 / Koka | `List.map` 不变 | 效果系统透明传递 |

只有 Gleam 选择 P1（名称含策略），这是缺少工具的妥协。Almide 已在 ADR-0002 决定构建 Swift 的 throws 模型（工具）。

<a id="使用量実測2026-08-05"></a>

### 使用量实测（2026-08-05）

排除家族自身定义、测试与文档后，真实使用处为**整个仓库一行**：`tools/almide-gates/src/main.almd:46`（经 effect fn，E=String）。编译器 `.rs` 中的 `try_fold` 是 Rust 自己的 `Iterator::try_fold`，无关。E 泛型（非 String）使用为**零**。

<a id="decision"></a>

## 决定

**HOF 可失败性从 callback 类型流入（一位可失败性多态）。立即冻结 `try_*` 家族，多态 HOF 落地时 deprecated，下一 minor 删除。**

<a id="d1-1-ビット可謬性多相"></a>

### D1. 一位可失败性多态

fn 类型参数可带 `!`（`(A) -> B!`）。generic HOF 在 callback 可失败时自身也可失败，唯一规则是“**callback 为 `!`，HOF 就为 `!`**”。E 按 ADR-0002 D2 固定为 String（比 Swift 的 E 泛型简单一级）。提前停止是传播的自然结果，无需专用实现：

```almide
nums  |> list.map((n) => n * 2)              // 総 callback → List[Int]
files |> list.map((f) => read_meta(f)!)!     // 可謬 callback → List[Meta]!(first-err 打ち切り)
```

<a id="d2-try_-family-は即時凍結"></a>

### D2. 立即冻结 try_* 家族

不新增姐妹函数。现有家族门禁（`tests/list_try_family_gate_test.rs`）改作“固定为 7”的守卫。CLAUDE.md 推荐的 try_map 惯用法**在多态 HOF 落地前仍有效**（先删会使 var+for 复活，删除顺序至关重要）。

<a id="d3-着地時に-deprecated--次-minor-で削除"></a>

### D3. 落地时 deprecated → 下一 minor 删除

deprecation hint 直接给出机械改写：

```
list.try_map(xs, f)      →  list.map(xs, (x) => f(x)!)!
list.try_filter(xs, p)   →  list.filter(xs, (x) => p(x)!)!
list.try_fold(xs, z, f)  →  list.fold(xs, z, (a, x) => f(a, x)!)!
// find / each / flat_map / filter_map も同型
```

迁移成本实测：一行。

<a id="d4-e-ジェネリックな-traverse-は意図的に非サポート"></a>

### D4. 有意不支持 E 泛型 traverse

自定义 E 的提前停止遍历用显式递归（或边界转换为 String）编写。这是符合 ADR-0004（String 终点、variant E 留在自建领域内）的有意省略。若实测到需求，按证伪条件 3 重审。

<a id="rationale"></a>

## 理由

- **ADR-0002 的推论**：既将可失败性设为 fn 属性轴，它在 fn **类型**（参数位置）出现就是轴的补全，不是新概念。
- **停止表面增殖**：策略 × HOF 的乘法消失，以后无论添加何种 HOF 都只有一个名字。
- **MSR**：编写者不必回忆存在哪些 try_ 姐妹，只需写通常的 `map` 并放置 `!`。可失败性过多或不足由 check 指出，借 hint 自我修复。
- **现在成本最低**：使用一行、首次发布。采用越广，拆除成本越高。

<a id="alternatives--検討して却下した案"></a>

## 替代方案 — 已考虑并拒绝

1. **只改名**（`traverse` / `map_try` 等）：若未来 D1 使家族消解，就会**改名两次**，作为最差选择**拒绝**。
2. **永久保留 try_***（Gleam 式）：决定制造工具的语言，无理由持续保留工具缺失造成的妥协。**拒绝**。
3. **等待完整 #1055（效果多态）一起实施**：一位可失败性多态仅固定 E、传播 bool，规模小且可分离，没有理由绑架它。**拒绝**（先提取）。
4. **引入惰性迭代器以支持 Rust 式组合**（P3）：改变整个集合模型，影响范围相差数量级。**拒绝**。

<a id="consequences"></a>

## 后果

- Phase 2 实现（#1108）：fn 类型的 `!`、lambda 可失败性推断（`(x) => f(x)!` 为 `(A) -> B!`）、generic HOF 的 total／fallible 两态单态化、native ⇄ wasm 等价。
- 双 `!` 写法 `((x) => f(x)!)!` 成为标准形式；它与 B 路线（auto-`?` 位置）的设计相互作用，因此 Phase 2 设计时必须交叉参照。
- try_* 分 3 阶段退场（冻结 → deprecated → 删除），各阶段写入发布说明。

<a id="falsifier"></a>

## 证伪条件

1. **一位传播推断变得不可预测**（如有无注解改变行为，或 Phase 2 无法解决 B 路线式不规则）：撤回 D1，永久保留 try_*。
2. **dojo A/B 表明 `((x) => f(x)!)!` 相比 try_map 显著降低 MSR**：停止拆除，改为多态 HOF 与 try_* 共存（try_* 为语法糖，ADR-0005 式定义关系）。
3. **实测到自定义 E 的 traverse 需求**（≥3 处真实使用）：用新 ADR 讨论 E 泛型多态（Swift 完整形式）。

<a id="references"></a>

## 参考

- Swift Evolution SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)（已获取 `map` 从 rethrows 泛化为 `throws(E)` 的前后引文）
- Rust — [std::result: Collecting into Result](https://doc.rust-lang.org/std/result/index.html)（已获取“返回首个 Err”的引文）、[Iterator::try_fold](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.try_fold)
- Zig — [Errors](https://ziglang.org/documentation/master/#Errors)
- Gleam — [gleam/list.try_map](https://hexdocs.pm/gleam_stdlib/gleam/list.html#try_map)
- 内部：ADR-0002 / 0004 / 0005、使用量实测（正文）、#1103（Phase 1）、`tests/list_try_family_gate_test.rs`（冻结守卫）

<a id="amendment-2026-09-27-e-は-callback-の失敗チャネルから流れる--adr-0021"></a>

## 修订（2026-09-27）：E 从 callback 失败通道流入 — ADR-0021

D1 的“E 按 ADR-0002 D2 固定为 String”由 [ADR-0021](../../../../adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) 修订。HOF 的 E 来自 callback 失败通道 ε（期望类型 → `!` operand 的 join → 默认 String）。撤回 D4（有意不支持 E 泛型 traverse）。`__fallible_*` carrier 在规范形式路径中已经以 E 泛型工作。证伪条件 3（真实需求 ≥3）由 dojo bank 的 3 个人工 baseline 及 bank pilot 产物的 37 次 attempt 触发（#2601）。

<a id="amendment-2026-10-01-d1-は全-hof-に及ぶ--3163"></a>

## 修订（2026-10-01）：D1 覆盖全部 HOF — #3163

D1 的“callback 为 `!` 则 HOF 也为 `!`”规则，原实现只适用于 list 的 7 个函数（map / filter / flat_map / filter_map / fold / find / each）。E005 hint 却称“core list HOFs 接受可失败 callback”，导致 `list.all(xs, (x) => p(x)!)` 在声称接受的提示下被拒绝（#3163）。撤回旧的“有意省略”（any/all/count 属于 find 形式领域，sort_by 的可失败键无顺序）：谓词查询在与 total 形式相同的停止点停止；键提取函数按元素顺序各算一次键后再排序，所以首个 err 明确。

现在 list / map / set / option 所有接收 callback 的函数都有可失败形式。唯一表位于 `almide_types::fallible_hofs`，checker 改写、dead-carrier 守卫、hint、interp HOF allowlist 都读取它。result 的 HOF 与 `option.collect_map` 以 TOTAL_ONLY 附理由排除（result 容器自身就是失败通道）。完整性规则由 `tests/list_fallible_family_gate_test.rs` 作为可执行矩阵检查。
