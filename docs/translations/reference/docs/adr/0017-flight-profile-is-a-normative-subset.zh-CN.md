<a id="adr-0017-the-bounded-profile-née-flight-profile-is-a-normative-subset-of-the-language-specified-here-before-any-checker-enforces-it"></a>

# ADR-0017：bounded 配置（原名“flight 配置”）是语言的规范性子集，在任何检查器实施约束之前先在此规定

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0017-flight-profile-is-a-normative-subset.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0017-flight-profile-is-a-normative-subset.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0017-flight-profile-is-a-normative-subset.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0017-flight-profile-is-a-normative-subset.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原 ADR 仍是权威且不可变的正本。本快照不自动同步，也不是检查器或生成器输入。下文决定、日期、测量、实现状态和主张均来自固定的历史原文，不代表独立验证或当前版本保证。代码块及其注释保持原样。

> 原文注意事项：此处 requirements-first 的决定、拒绝诊断和逐构建成员资格证明是当时的规范意图，不是所有检查器、cert seat 已实现或获得外部认证／资格认可的证据。仅在两个前提均满足时才能放宽 Float 排除；明确的剩余限制仍保留。

- **状态**：已接受 — 于 2026-08-21 裁决（[#29](https://github.com/almide/als/issues/29)），只有一项修订：**配置以其提供的保证命名，而非以行业命名。** 它是 *bounded 配置* — 静态有界的时间和内存、封闭的调用图、受能力限制的 effect — 标记为 `@bounded`，章节为 `bounded.md`，前缀为 `B`（`ALS-B<n>`）。`@flight` 不出现在语言中；“flight”仍作为动机背景保留在 QUALIFICATION.md 和实现的路线图中。⚖ 3 和 ⚖ 4 按原文批准；⚖ 5 在改名后批准。文件 slug 保留原来的词语（ID 和路径永久不变，先例是 ADR-0015）；下文为修订后的决定，在实现文档仍使用旧名的地方引用原名。
- **日期**：2026-08-21（同日提出并接受）
- **背景**：航空级代码使用语言的子集编写 — MISRA C、Ada Ravenscar、SPARK — 认证计划实际进行资格评定的也是该子集。实现方已设计 Almide 的子集（flight-subset-spec.md、flight ladder 的 G-F1）：仅允许计数循环、循环内不得分配、有界静态分配、无递归（调用图无环）、无闭包／高阶／`any P` 分派、受能力限制的 effect、排除 `break`/`continue`、暂时排除 Float。通过属性逐函数实施约束（该设计写作 `@flight`，此处为 `@bounded` — 参见状态），由每次构建的证书证明其属于子集，而非由评审者检查。目前该设计只存在于实现侧，没有 `ALS-<id>` 节、contract 或 fixture — 因而尚不是语言要求，不能作为语言要求引用。CONTRIBUTING.md 的顺序要求先在这里确立行为；这一配置是路线图中最大的此类行为，也是其来历会被最仔细审阅的行为（QUALIFICATION.md 限制 7）。还有一个事实促成了修订：子集的规则并非关于飞机 — 计数循环、有界分配、无递归和封闭调用图同样适用于嵌入式、实时、沙箱和游戏代码。通用语言若在源码属性中携带行业名称，就让客户渗入了语法。
- **决定**：bounded 配置是本仓库规定的**规范性子集** — 新的 ALS 章节 `bounded.md` 拥有前缀 `B`（在 STANDARD.md 注册）— 先规定，再实施约束：
  1. **是子集，不是方言。** 每个 `@bounded` 程序都是有效的 Almide 程序，无论有无该属性，可观察行为都相同（SPARK ⊂ Ada）。一组 fixture 精确断言这一点：`spec/wasm_cross` 中每个符合配置的程序，在带属性和不带属性时，运行结果逐字节一致。
  2. **成员资格是检查器判定，而该判定本身就是要求。** 子集的每条 OUT 和 RESTRICTED 规则成为一个 `ALS-B<n>` 节，说明可观察事项：执行 X 的 `@bounded` 函数必须以固定诊断代码被拒绝；`tests/diagnostics/<case>/` 保存其 `broken.almd` / `fixed.almd` 对（拒绝的表面是语言的一部分 — README 的“tests/diagnostics”）。
  3. ⚖ **采用 flight-subset-spec.md §2 的 IN/OUT 表作为初始规范内容**，包括已解决的开放问题：循环内 `Dup` 为 IN，`break`/`continue` 为 OUT（精确界限、MISRA 单一出口），通过调用图无环性排除递归，对未知被调用者保守拒绝；嵌套计数循环为 IN，界限取乘积。每行成为一节；标为“未”（尚未实现约束）的行现在也要规定 — 这正是 requirements-first 的意义 — 之后由实现方推进 pin 使其变绿。
  4. ⚖ **在 ADR-0016 的 family 成为规范且 cert seat 拥有 float 运算集之前，Float 不属于 bounded 配置**（目前 MIR 没有 `FloatOp`）。该节以直陈语气规定此事（“执行 Float 算术的 `@bounded` 函数以 E… 被拒绝”）；当两个前提都满足时，应修订该节，而非悄悄放宽。
  5. ⚖ **属性逐函数应用，并以保证命名**：在 `fn` / `effect fn` 上使用 `@bounded`，模块级写法为语法糖；`@bounded` 函数只能调用 `@bounded` 函数以及纯 stdlib 模块的一阶成员（封闭调用图 — 无环性与能力证明的前提）。编译标志不是该机制（它会把导入的无界代码也卷入判定）。`flight` 不是语言的 token。
  6. **flight-subset-spec.md §6 坦诚列出的剩余问题**（有界递归、定点／确定性 Float、静态大小数组、字节级内存界限、`≤ B` 提前退出、可读布局、功能正确性追溯）按 QUALIFICATION.md 的形式记入本章的明确限制清单 — 说明子集尚不能表达什么，避免被误认为已有承诺。
- **理由**：(1) 只针对该子集主张 DO-178C credit（flight-qualification.md §1：“DAL-A for the flight subset”）；不是要求的子集无法承载 credit。(2) 由证书证明成员资格的设计是项目的差异化所在，但证明需要一个待证命题 — 命题就是 `ALS-B` 节。(3) 先加入章节，再建立拒绝屏障，才能使配置的来历体现 `requirements-first`；反过来会让项目中受审查最严格的要求成为 `retroactive`。(4) 诊断语料约束使每条 OUT 规则从第一天起就成为可执行证据，即便还没有检查器 — 在任何实现有门禁之前，判定器已能让接受递归 `@bounded` 函数的实现失败。(5) 名称：配置是对代码的承诺，承诺是有界性；以某个买家命名，对其他买家都不合适，在通用语法中也不合适。
- **替代方案**：(a) 让子集留在实现设计中，等检查器实施约束后再规定 — 拒绝：恰好对评估者最先读取的要求颠倒两个 PR 的顺序。(b) 用编译标志（`--flight` / `--bounded`）而不是属性 — flight-subset-spec.md §4 已拒绝（会卷入导入的 stdlib；约束原子是函数）。(c) 有独立语法的另一种方言 — 拒绝：子集而非方言是 SPARK/MISRA 的经验，也是让两者共用一个判定器的条件。(d) 保留设计的 `@flight` 写法 — 裁决时拒绝：行业不是性质；`@bounded` 命名的是被证明的内容。
- **后果**：新增 `bounded.md` 章节、STANDARD.md 中的新前缀行 `B`、约 15–20 个 `ALS-B` 节及每节一个 contract、每条 OUT 规则一个诊断案例，以及“子集而非方言”定律的跨目标 fixture 组。实现的 G-F1 工作变为“使判定器变绿”，而不是“设计并实施约束”。除非每节连同其审查行一起加入，否则 `proofs/als-validation.toml` 中的 `unvalidated` 上限会按新增节数增加 — PR 必须明确选择。
- **反证条件**：一个 `@bounded` 程序若因属性有无而改变行为，就推翻规则 1；错的是章节，不是程序。认证评估者（#15）若倾向于以整个模块或整个程序为约束单位，则推翻规则 5。ADR-0016 的 family 尚未成为规范时就让 Float 运算集进入 cert seat，是流程违规，不是放宽规则 4 的理由。
- **参考**：almide/almide `docs/roadmap/active/flight-subset-spec.md`（§2 表、§3 已解决问题、§4 约束、§6 剩余项）；`flight-profile.md` §3.5、§7.2（G-F1）；`flight-qualification.md` §1；MISRA C:2012 Rules 17.2、21.3、15.x；Ada Ravenscar profile（ISO/IEC 8652 D.13）；SPARK 子集理由；ADR-0016；本仓库 CONTRIBUTING.md（变更顺序）、QUALIFICATION.md 限制 7、issues #11 和 #9。
