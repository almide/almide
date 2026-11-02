原文 description 元数据：通过 dojo MSR 衡量，打造 LLM 最能准确编写的语言的计划
<a id="llm-first-language"></a>

# LLM-first 语言

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](llm-first-language.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](llm-first-language.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](llm-first-language.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md)

> 原文注记：保留2026年4月设计历史，不是当前 benchmark 结果。mission 与模型优越性说法是目标或历史解释，非独立确立排名。有限30题的100%不证明无缺陷、模型能力因果、真值区间或其他任务的 retry 充分性；原文自身也撤回这些推广。UFCS 与 bundled source 纠正优先于旧拒绝/backlog 文本。不能把历史分数/task bank 变更、提议 CI gate 和 llms 生成视作当前已执行。保留原阈值，包括含糊的负阈值 token 及不同历史 UFCS 决策条件。


> **范围更新**：本文 Phase 与 MSR 是2026-04 的设计、测量记录。UFCS 现已实现（见 SPEC method-call syntax），采纳决策不再是待办。bundled stdlib 也已以 `.almd` 为当前 source，不要把旧 TOML routing 障碍当成当前 blocker。剩余 tooling / parser / MSR 改善案需逐项验证。未重新测量外部 Dojo，不能将以下分数、退出目标达成情况沿用到当前版本。

Almide 的 **mission**：“The language LLMs can write most accurately.”（LLM 能最准确编写的语言。）本 roadmap 汇总朝此方向的5轴设计决策和分阶段实施顺序。**设计上以 almide-dojo MSR delta 为决策依据**（不表示全部 PR 实际测过）— 是否添加语言功能以“retry-success 是否提高”判断。

<a id="design-rule"></a>

## 设计规则

> 不采用对人优美、对 LLM 却是陷阱的语法。
>
> 功能价值以 **MSR delta**（dojo 中70b / 8b / Sonnet 的 retry-success 变化）衡量。优美但让 MSR 降低3pt 以上的功能不采用，既有功能也须重审。

<a id="5-軸"></a>

## 5轴

<a id="axis-1--間違える機会を減らす"></a>

### Axis 1 — 减少犯错机会
- **UFCS**（`n.abs()` → `int.abs(n)` 自动解析）：最大押注，自动解决 method-call + list field 的30-40%。作为独立 proposal（见后）。
- **自动导入前30个 stdlib**：扩充当前 Tier 1，减少 E002 类错误。与副作用（import 透明性）权衡。
- **单一规范写法**：文档 / lint 层，不改语言。

<a id="axis-2--パーサが救う"></a>

### Axis 2 — 让 parser 救场
- **错误恢复 parser**：缺一个 `)` 不导致30行级联。类似 Tree-sitter 的 panic-mode。
- **级联抑制**（v0.14.5 已先行实现 ✓）：抑制 parse error 下的 `undefined function`，可向 type error 扩展。

<a id="axis-3--エラーが自己修復可能"></a>

### Axis 3 — 错误可自修复
- **`try:` 代码片段**（Elm 风格）：error 不只附 hint 文本，还附可粘贴修复代码。
- **`almide fix`**：若 try snippet 可机器应用，提供类似 `cargo fix` 的能力，由 compiler 代替 LLM retry。
- **cheat-sheet 单一真相来源**：从 `SPEC.md` 自动生成 `SYSTEM_PROMPT` / `CHEATSHEET.md` / `llms.txt`，让 dojo、本体、外部 tool 看到相同文本。

<a id="axis-4--modification-を安全に-msr-の-m"></a>

### Axis 4 — 让修改安全（MSR 的“M”）
- **Block boundary marker**（类似 MoonBit `///|`）：修改一个函数不破坏其他函数，需要动 parser。
- **逐函数依赖声明**：`fn name(...) uses [list, int]` — 使 LLM 局部修改安全。ROI 不明，后置。
- **用 `#deprecated` + `#alias` 长期迁移**：语言演进不造成灾难。

<a id="axis-5--msr-を-first-class-metric-に"></a>

### Axis 5 — 将 MSR 作为一等指标
- **将 dojo 内置本体 CI**：各 PR 计算 MSR delta，`-2pt 以上`（此处解释为下降2pt或更多；原文 token 有歧义）的回归自动阻断（初期试用 warning-only）。
- **Release notes 中列 MSR 分数**：各 release 对 retry-success 负责。
- **新功能决策轴**：“优美但 MSR -3pt”的功能不采用。

<a id="当初の実装-phase-順2026-04-の履歴現在の-backlog-ではない"></a>

## 原始实施 Phase 顺序（2026-04 历史，不是当前 backlog）

MoonBit 的经验：**只有“LLM-friendly 语言”不够，“LLM-friendly 开发环境”**效果相同或更大。Almide 也先建 tooling，再扩展语言。

<a id="phase-1-完了"></a>

### Phase 1（完成）
- ✅ `try:` snippet + Diagnostic 扩展（f87b7d0b）
- ✅ 明文 DESIGN rule（9a583e72）
- ✅ 整理 roadmap

<a id="phase-2--tooling-moonbit-inspired-実質完了-2026-04-16"></a>

### Phase 2 — Tooling（受 MoonBit 启发，2026-04-16 实质完成）
2-1. ✅ **`almide ide outline <file>`** — 每行列出一个 package pub fn / type / let。消除 grep 和 E002/E003 幻觉。
2-2. ✅ **`almide ide doc <symbol> [--file <f>]`** — 返回 stdlib / user fn 的 signature + docstring。寻找 `string.to_upper` 无需 `grep`。
2-3. **`almide ide peek-def <symbol>`** — 仅返回定义 snippet（有 body）。**暂缓** — dojo 场景没有 peek 既有 body 的需求，对 MSR 贡献小；添加 refactor task bank 后重评。
2-4. **`almide ide find-refs <symbol>`** — 引用列表。**暂缓** — 同上。
2-5. **向 `almide new` 附带 AGENTS.md** — 将 dojo SYSTEM_PROMPT 分发至所有 project。
2-6. **可运行 `*.almd.md` cheat-sheet** — 保证通过 `almide check *.almd.md`，规格不漂移。

<a id="phase-2-実測-try-snippet--hint-改善による-msr-押上げ-2026-04-16"></a>

### Phase 2 实测：try: snippet + hint 改进提升 MSR（2026-04-16）

从 baseline（v0.14.5 = phase2 开始前）出发，在 llm-first-phase2 branch 实现以下内容，持续用 dojo 测量：

**已实现诊断改进**：
- `almide ide outline / doc / stdlib-snapshot` + `@stdlib/<module>` + `--json`
- 给 E002/E003/E004/E009 的6条路径、E001 Unit-leak 的2条路径（fn body / if arm）加 try: snippet
- fn-body Unit-leak 特化：从 AST 提取尾部 `let` 名，提供具体 rewrite
- if-arm Unit-leak 特化：提取返回 Unit 的 arm 的 assign target，引导至 `let new_x = if ...`
- let-in 检测扩展到跨行；Err 后仍保留 partial Let，传递给下游诊断
- 为 int.sqrt / int.gt 等幻觉提供丰富 conversion / operator snippet
- misplaced `test` keyword 的 harness-context hint
- rest/cons pattern（`[h, ...t]` / `head :: tail`）引导至 list.first/drop 的 hint
- 包装泄漏的 rustc 四位 E-code（即使 rustc error 不提 `src/main.rs` 路径，也用 bug banner 包住）

**数字（Δ = phase2 开始 baseline → 最终）**：

| Release / Version | Model | retry-success | 1-shot | Δ baseline |
|---|---|---|---|---|
| v0.14.5 baseline | llama-3.3-70b | 17/30 (57%) | — | — |
| v0.14.5 baseline | llama-3.1-8b | 13/30 (43%) | — | — |
| 0.14.6 (14d1a973) | llama-3.3-70b | **23/30 (77%)** | 12/30 | **+6 (+20pt)** |
| 0.14.6 (14d1a973) | llama-3.1-8b | 10/30 (33%) | 10/30 | -3 (variance) |
| 0.14.6 (14d1a973) | **Sonnet 4.6** | **30/30 (100%)** | 26/30 (87%) | — |

**当时战略判断（以下推广未验证）**：
这30题不能证明整个语言没有缺陷、失败只因模型能力、真值区间或未来 retry budget。保留原判断作为历史。
- Sonnet 30/30 = **Almide 语言设计无结构性缺陷**。70b 剩余7个 fail 纯属 model capability 界限（ADT / state-tracking / complex accumulator）。
- **确定不需要 Path A（UFCS / imperative loop 让步）** — 强模型在当前设计可全部完成，没有污染语言的理由。
- 70b 的1-shot 12/30 vs Sonnet 26/30 差异 = 纯 LLM 能力差，而非 retry 前 prompt 质量。修改 SYSTEM_PROMPT 可缩小的空间有限。
- 8b variance 预计通过 n=3 吸收。`12±2` 为真值区间。

**Phase 2 完成条件**：✅ 70b 至少 baseline +5pt，强模型达到90%+（SOTA 同等），并保持 DESIGN rule。

<a id="phase-3--fix-tooling--stdlib-expansion-mvp-着地-2026-04-16"></a>

### Phase 3 — Fix tooling + stdlib 扩充（MVP 2026-04-16 落地）
3-1. ✅ **`almide fix` MVP** — 自动应用 auto-import（E003 “Add `import json`”等），剩余 `try:` snippet 作为 manual-fix 报告。支持 `--dry-run` 预览。`let-in` → newline chain / `head :: tail` → list.first-drop 等 AST-level 机器 rewrite 在 Phase 3-1.2 扩展。
3-2. ✅ **stdlib primitives**：新增 `list.binary_search(List[Int], Int) -> Option[Int]`、`string.run_length_encode(String) -> List[(String, Int)]` — 将 dojo binary-search / run-length-encoding 任务从算法合成降为 API 调用。其他候选（`list.window`、`list.partition`、`list.chunks`）待 dojo 重测后决定。
3-3. ✅ **llms.txt** — 放在 repo root，以约~5K行汇总 mission/CLI/core idioms/error codes/stdlib pointer，LLM tool 可用1个 URL 获取。手工更新（从 SPEC.md 自动生成需 fmt-idempotent 转换器，故延至 Phase 3-3.2）。

<a id="phase-3-の残タスク"></a>

#### Phase 3 剩余任务
- **3-1.2 `almide fix` 的 AST-level rewrite** — ✅ let-in（source-level），✅ operator-form（`int.gt(a, b)` → `a > b`，AST-level Call → Binary）。只剩 cons pattern（`head :: tail` → list.first/drop），前提是 parser recovery 保留 trailing-body。
- **3-2.2 stdlib bundled-Almide dispatch** — 修改 codegen，使 `stdlib/list.almd` 等 Almide-source 扩展能在 `list.*` module 工作。当前 TOML-module 固定 routing 到 `almide_rt_list_<fn>`，因此在 `stdlib/list.almd` 添加函数不会成为 `E002` 一类错误，而会成为 codegen bug。dogfood 时发现（见 examples/almd-outline README）。
- **3-3.2 llms.txt 自动生成** — 以 SPEC.md / cheatsheet / stdlib-snapshot 为 source-of-truth，利用类似 `almide docs-gen` 的 tooling 再生成 llms.txt。

<a id="phase-4--language-level-changes-tooling-で不足分を見てから"></a>

### Phase 4 — 语言层变化（看 tooling 还有哪些不足之后）
4-1. **UFCS 采纳决策**：根据 Phase 2-3 的 MSR 改善。若 dojo 8b 仍有许多 parse-err 则 GO，假设70b改善已由 tooling 获得。
4-2. **强化错误恢复 parser**：向 type error 扩展级联抑制。
4-3. **强化 `?` operator / Option chain** — 应对70b的9个 type-err。

<a id="phase-5-長期"></a>

### Phase 5（长期）
5-1. **CI 内置 dojo**：自动计算 PR 级 MSR delta。
5-2. **`///|` block boundary**（MoonBit 风格）：通过语法保证局部 refactor。
5-3. **Skill marketplace / playbook**：将 refactor / bug-fix / new-feature 各模式 playbook 插件化。

<a id="当初の-ufcs-proposal当時の保留理由現在は実装済み"></a>

## 最初 UFCS proposal（当时暂缓理由，现已实现）

- **现状（v0.14.3+）**：`n.to_uppercase()` 报 error + hint `Write 'string.to_upper(x)'`。v0.14.4 dojo data 中 retry-success ±0。
- **论点**：error 改进有天花板。UFCS 是“将写错形态当作正确接受”的语言语义改变，属于另一维度 effect。
- **成本**：parser 将 `.method()` 解释为 UFCS sugar，checker 做 module dispatch。预计5-10次迭代。
- **风险**：LLM 训练数据可能分散到3种形式（`int.abs(n)`、`n.abs()`、`n |> int.abs`）而不收敛。放弃 One canonical form 原则。
- **判断标准**：Phase 1-2 MSR 不足+10pt → 用 UFCS 争取余量。超过+10pt → 不采 UFCS，保持 canonical form。

<a id="計測"></a>

## 测量

- **dojo runs**：每次 release 后，用70b / 8b /（可选）Sonnet 对30题以 T=0 运行3次，取中位数。
- **MSR delta table**（列于 Release notes）：
  | Release | Sonnet 4.6 | 70b | 8b | Notes |
  |---|---|---|---|---|
  | v0.14.5 | — | 17/30 | 13/30 | retro — baseline for llm-first roadmap |
  | 0.14.6 | **30/30** | **23/30** | 10/30 | Phase 2 完成（2026-04-16）。Sonnet 30/30 达成语言设计 validation。 |
- **retry budget**：dojo data 确认 `pass-3 = 0 across all runs`。`max_retries=3` 浪费，**2已足够**。剩余 budget 用于将任务跑2轮以去除 variance。

<a id="dojo-data-から当時推測した構造-2026-04-15"></a>

## 当时从 dojo data 推测的结构（2026-04-15）

以下“必然”“保证”“证据”是当时 task bank 的观察/解释，不是对新任务、模型或当前版本的保证。

<a id="両モデル必ず通る-12-タスク--sweet-spot"></a>

### “两模型必过”的12题 = sweet spot
`clamp`、`fizzbuzz`、`factorial`、`flatten-nested`、`repeat-string` 等 — 单函数 / primitive I/O / 浅递归，100% pass。**MSR-first 语言的基准保证线。**

<a id="両モデル必ず落ちる-11-タスク--設計の中心課題"></a>

### “两模型必失败”的11题 = 设计核心课题
集中在3组功能：
- **ADT / sum types**：`anagram-check`、`custom-linked-list`、`mini-json-query`
- **Stateful loops**（low/high 指针等）：`binary-search`、`matrix-ops`
- **字符串算法流水线**：`balanced-parens`、`caesar-cipher`、`roman-numeral`、`run-length-encoding`、`string-reverse`、`result-pipeline`

→ **不要丢弃功能**。在 stdlib 增加“common pattern as a function”，让 LLM 不写算法而做组合：
- `list.binary_search`、`string.run_length_encode`、`list.window`、`list.partition` 等。
- 决定添加新 Phase：**Phase 1.5: stdlib pattern expansion**。

<a id="8b--70b-逆転--one-canonical-form-の証拠"></a>

### 8b > 70b 逆转 = one canonical form 的证据
`max-of-list`、`sum-digits` 上8b通过而70b失败。70b 因更聪明而过度写 `xs.head` method chain / `let rec`。**小模型也能写 = 大模型也不会擅自过度写**。Elm 的“only one way”与 MSR-first 一致。

<a id="parse-error-vs-type-error-の-model-size-依存"></a>

### parse-error vs type-error 的 model-size 依赖
| | parse | type |
|---|---|---|
| 8b | 9 | 3 |
| 70b | 0 | 9 |

→ **面向小模型**：宽容语法（UFCS、auto-import、可选括号？）有效。
→ **面向大模型**：强化类型推断、Option/Result chain（`?` operator）有效。
→ 两者分别在不同 Phase 叠加。

<a id="当時の-ufcs-再考-2026-04-data-を見た後"></a>

## 当时重新考虑 UFCS（看到2026-04数据之后）

- 8b parse-error 9个 vs 70b 0个 = **UFCS 对8b效果强，对70b几乎无关**。
- 70b type-error 9个 = 类型推断 / Option chain 改善有效。
- 结论：为“提升8b下限”，UFCS 很值得采用。打破70b天花板需其他方向（强化类型推断、`?` chain）。
- **决定**：Phase 1-2 后，8b MSR 提升不足+5pt则考虑 UFCS；70b改善另立 type-inference Phase。

<a id="非目標-明示"></a>

## 明确非目标

- 追求 Haskell 级类型系统美（Linear / Dependent / etc.）。
- “Almide is pure FP”式纯粹性品牌。
- 像 Elm 一样严格的“no method syntax”世界观（采用 UFCS 就放弃）。
- “full-stack framework”式锁定。
