<a id="adr-0009-function-types-complete-the-capability-quadrant-fn-params-are-transparent-by-default"></a>

# ADR-0009：函数类型补齐能力四象限；fn 参数默认透明

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0009-fn-type-quadrant-transparency.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0009-fn-type-quadrant-transparency.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0009-fn-type-quadrant-transparency.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[日英混合正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md)的非规范性阅读译本，不会自动同步。原本仍具权威性，规范性陈述属于原本而非本阅读版。历史实现、兼容性、健全性及测量主张均原样保留，未经独立执行或验证。代码块及注释保留原文。

- **状态**：已采纳
- **日期**：2026-08-06
- **决策范围**：fn 类型（参数位置的函数类型）中 effect / 失败可能性的表示 — 引入四象限、普通 fn 类型参数的默认语义、显式槽位的用途，以及如何堵住 effect 洗白漏洞
- **相关**：[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（声明侧的四象限 — 本 ADR 将其镜像到类型侧）、[ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)（`!` 位的一位多态 — 本 ADR 将其补全为两位）、[ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)（完全显式 — 确定 lambda 内 `!` 的含义）、#1055（本 ADR 为其提供设计）、#1051 / #489（当前的上下文资格模型）
- **背景**：2026-08-06，深入分析错误表面矩阵中标记为 #1055 的 15 个单元格。其中 10 个已由 ADR-0006/0008 解决；通过是非裁定解决剩余 5 个（包括 effect 洗白的健全性缺陷）。

<a id="context"></a>

## 背景

当前模型（由 #1051 固定）：**lambda 继承外层 fn 的 effect 资格**（上下文资格）。由于 fn 类型不携带资格，已在边界处实测到两个缺陷：

```almide
// 1. 健全性バグ: pure fn からコンビネータ経由で効果が実行できる(ロンダリング)
effect fn g(x: Int) -> Result[Int, String] = ...   // fs を触る
fn main() -> Unit = println("${list.try_map([1, 9], g)}")
// 実測: コンパイル通過・効果実行・exit 0。purity.rs の最適化前提を破る

// 2. 非対称な拒否: 同じ effect fn を auto-lift 形(-> Int)で渡すと
//    今度は E005 で拒否され、しかもエラーが lift 前の型を表示して意味不明
```

此外，无法用类型表达像 http.serve 这样希望将“handler 的资格与运行时契约（Err→500）一起固定”的 API（#1055 的原始议题）。

15 个单元格的剩余部分（6 个 auto-? 类单元格已由 ADR-0008 解决，4 个禁止 lambda 内 `!` 的单元格已由 ADR-0006 解决）。

<a id="decision"></a>

## 决策

<a id="d1-fn-型は4象限を持つadr-0002-の宣言文法の鏡映"></a>

### D1. fn 类型具有四象限（镜像 ADR-0002 的声明语法）

```almide
(A) -> B            // pure ・総
(A) -> B!           // pure ・可謬(ADR-0006)
effect (A) -> B     // effect・総 — 2026-09-24 撤回(ADR-0002 §D6): effect は可謬を含意し、下の行と同義
effect (A) -> B!    // effect・可謬
```

<a id="d2-素の-fn-型パラメータは透過-引数のビットが呼び出しへ流れる"></a>

### D2. 普通 fn 类型参数“透明” — 实参的位流向调用

写作 `f: (A) -> B` 的参数接受四象限中的任意一种，**所传实参的位流向 HOF 调用表达式**（将 ADR-0006 的一位规则推广为两位；各位独立）：

```almide
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)

apply((n) => n * 2, 5)               // pure 総 → apply(...) も pure 総
apply((n) => int.parse(s)! + n, 5)   // 可謬   → apply(...)! が要る
apply((n) => count_files(n)!, 5)     // effect → apply(...) は effect 呼び出し
```

这样就在**正确的位置**堵住洗白漏洞：`try_map([1,9], g)` 接收 g 的 effect 位，成为 effect 调用，因此在 pure main 内，E006 出现在**调用方**。effect fn 中带 effect 的 map lambda（今天合法的代码）无需修改仍然合法 — **迁移破坏为零**。

<a id="d3-明示スロットは資格の要求固定"></a>

### D3. 显式槽位“要求并固定资格”

按显式声明的 `effect (A) -> B!` 槽位检查的 lambda，获得该资格对应的函数体使用便利（允许 effect 调用及使用 `!`）。具有运行时契约的 API 采用这种声明：

```almide
effect fn serve(port: Int, f: effect (HttpRequest) -> HttpResponse!) -> Unit
// handler の err はランタイム契約(almide_http_serve)により 500 応答へ
```

<a id="d4-象限の上方包摂は暗黙無損失"></a>

### D4. 象限向上包含是隐式的（无损）

把 pure lambda 传给 `effect` 槽位、把全函数 lambda 传给 `!` 槽位，均隐式合法（使用比要求更少的资格总是安全的 — 与 Swift 的非 throwing 闭包可进入 throws 槽位同构）。这是 ADR-0003 无损原则的应用。

<a id="d5-実装の段階は-1108-に接続"></a>

### D5. 实现阶段衔接 #1108

`!` 位 = #1108（Phase 2）。effect 位作为同一机制的 Phase 3 叠加。切换到 Phase 3 时，#1051 的上下文资格规则被 D2 的类型规则替换（D2 不破坏可观察行为）。保持 #489 的不变量（传播不跨越闭包边界）— lambda 内的 `!` 落入 lambda 自身的通道（ADR-0006）。

<a id="rationale"></a>

## 依据

- **补全两轴**：0002 在声明中建立两轴，0006 将其中一位放入类型。没有理由不放入另一位；缺少它的上下文资格模型已被实测发现会漏过健全性缺陷。
- **透明默认值沿用 Swift rethrows 已有实践的形式**：“隐式传递位”是已成立 10 年的设计。不要求任何注解，现有代码全部存续。
- **封闭两位的位置**：介于 Swift（一位，effect 不受约束）与 Koka / OCaml 5（行多态，强大但推断沉重、先验分布也稀薄）之间。这是唯一适合以可预测性为先的 almide 的位置。
- **健全性**：purity.rs 信任的“pure fn 不执行 effect”由类型保证。

<a id="alternatives--検討して却下した案"></a>

## 替代方案 — 考虑后否决的方案

1. **严格默认**（普通 fn 类型仅接受 pure 全函数）：现有带 effect 的 HOF lambda 全部失效，迁移时必须为每个 HOF 写注解。**否决**。
2. **维持上下文资格模型并逐个修补漏洞**：洗白与不对称拒绝是该模型的结构性产物，修补后仍会不断出现边角问题。**否决**。
3. **行多态（Koka 式）**：S 成本最大，推断错误处于分布之外。**否决**。
4. **不把 effect 位放在 fn 类型中，仅供具名 fn 使用**：无法用类型表达 http.serve 的 handler，#1055 的原始议题仍未解决。**否决**。

<a id="consequences"></a>

## 后果

- 类型语法增加带 `effect` 前缀与 `!` 后缀的 fn 类型（parser、checker、mono、native ⇄ wasm）。泛型 HOF 最多有四种单态化状态。
- #1051 的上下文资格规则在 Phase 3 被类型规则吸收（行为兼容）。
- 洗白漏洞（#1055 单元格）在 Phase 3 成为 check 错误 — 此前作为已知健全性缺陷记录在 #1055 中。
- 可以声明 http.serve 等运行时契约 API 的 handler 类型。

<a id="falsifier"></a>

## 证伪条件

1. **如果透明性导致无法在泛型 HOF 内假定 callback 的纯粹性，破坏 walled-real 基线的性能 ratchet** — 退回显式选择启用的多态（rethrows 关键字式）。
2. **如果四态单态化显著膨胀二进制大小预算（minigit 418KB 系列的测量）** — 使实现转向共享状态（Result 形式的单一实现）。
3. **如果在 dojo 中区分透明槽位与显式槽位成为显著的错误来源** — 将显式槽位的用途限制在 stdlib 内部（运行时契约 API）。

<a id="references"></a>

## 参考资料

- Swift — [rethrows](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/declarations/#Rethrowing-Functions-and-Methods)、SE-0413（非 throwing → throws 槽位的包含）
- Koka — [Effect types](https://koka-lang.github.io/koka/doc/book.html#sec-effect-types)、OCaml 5 — effect handlers（行多态一侧的对比）
- 内部：#1055（原始议题草案）、#1051 / #489（当前模型的固定点）、2026-08-05 矩阵的 `purity/*`、`effectctx/*` 单元格（已保存洗白探针）、ADR-0002 / 0003 / 0006 / 0008

<a id="implementation-note--the-effect-bit-through-values-4151-2026-10-25"></a>

## 实现注记 — effect 位随值传播（#4151，2026-10-25）

对于作为值使用的具名 effect fn（`list.map(xs, fs.read_text)`、用户 HOF、记录字段、经 pure helper 向下传递的闭包），D2 的洗白漏洞此前仍然存在。现在按如下方式堵住；本 ADR 未作规定的地方采用以下保守选择。

1. **以 effect fn 名称作为实参就是 effect 调用**（checker 中的 D2 规则）：从 pure fn 使用裸名称原本就会触发 E006；现在模块路径（`fs.read_text`）也如此。显式 `effect` 槽位（D3）豁免。
2. **该位随值而非 `Ty::Fn` 传播。** D5 原计划将其作为类型位。实际由已携带 ADR-0026 category 集合的值流分析（`crates/almide-codegen/src/effect_flow`）携带，作为同一掩码中的另一位：`effect fn` 自身的节点及每个未经扫描的 stdlib `effect fn` 调用都会设置它。它随值经过记录字段、列表、option 和闭包（ADR-0026 D1：存储后的值保留其集合），无需新的表面语法，也不增加单态化的倍数。
3. **E109。** 若 pure fn 求解后的集合具体包含该位 — 经由其读取的值，而非其自身的 fn 类型参数（后者属于 D2 的透明性，在其调用点计入）— 则拒绝并给出路径。普通 fn 类型的记录字段*不*透明：拒绝的是运行它的 pure fn，而不是存入它的 effect fn。
4. **健全性优先于精度。** 离开跟踪位置的值按 fn 签名汇入池中，调用池中的值时计入整个池；如果任何地方曾将某个签名相同、会运行 effect fn 的 fn 放入列表，则从列表读取 fn 的 pure fn 会被拒绝。仓库实测（spec/、stdlib/、examples/、tests/：3,459 个文件，build 路径；全部 5,102 个 `.almd` 文件，`check`）：没有合法程序被拒绝；两个从 pure `main` 传递 `process.exec` 的 fixture 被改为 `effect`。
5. **Abort 不是 effect。** `process.exit`（无函数体、`-> Never`）不携带该位 — 它属于 ADR-0022 的 abort，降低后的 `assert` 分支会调用它。根据声明读取，而非名称列表。
6. **未改变：** 把 `effect (A) -> B` 值传给普通 `(A) -> B` 参数仍会通过 check（unify 忽略 `is_effect`），随后因两种返回表示在 rustc 处失败（E0271）。这是独立的类型缺口，不是洗白：分析会在调用点计入。

## 原本注意事项

最初的四象限描述与零迁移破坏主张属于该决策的历史阶段。保留的代码注释指出，D1 的 total-effect 象限已于 2026-09-24 撤回（ADR-0002 §D6）。2026-10-25 的 #4151 补充以值流跟踪取代 D5 计划的类型位，不增加单态化状态。池化分析是保守的；未拒绝合法程序的报告仅限实测仓库语料，不表示普遍不存在误报。E0271 类型缺口仍明确未解决；Abort 不是 Effect。本阅读版不将这些阶段整合为更强的当前保证。

### 保留的日文代码注释译文

- 背景示例：健全性缺陷允许 pure fn 通过组合子执行 effect；g 会访问文件系统。实测探针通过编译、执行 effect 并以 0 退出，破坏 purity.rs 的优化前提。以自动提升形式（返回 Int）传入同一个 effect fn 时，却被 E005 拒绝；错误显示提升前的类型，导致含义不清。
- D1 列出 pure 全函数、pure 可失败函数（ADR-0006）、effect 全函数和 effect 可失败函数。effect 全函数一行已于 2026-09-24 按 ADR-0002 §D6 撤回：effect 蕴含可失败性，因此与下一行同义。
- D2 示例注释说明：pure 全函数 callback 使 apply 仍为 pure 全函数；可失败 callback 要求在 apply 调用上使用感叹号；带 effect 的 callback 使 apply 成为 effect 调用。
- D3 的 handler 错误按 almide_http_serve 运行时契约变为 500 响应。
