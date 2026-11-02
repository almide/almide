<a id="the-common-issue-header--area-label--spec-for-rewriting-almidealmide-issues"></a>

# 通用 issue 标头与 area 标签 — 改写 almide/almide issue 的规范

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](issue-header-spec.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](issue-header-spec.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](issue-header-spec.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-inventory-2026-09-21/issue-header-spec.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 原文元数据：本文件未指定原文日期。

> 这是历史流程，不是本翻译任务的可执行指令：命令、issue 编辑、恢复及长期规则引用仅为阅读复现。生成此快照未获取或修改 issue、未应用标签、未运行构建或基准。保留固定日文字段和精确模板，但不构成新的指令或机器输入。

每个未关闭的 issue 都获得 (A) 一个置于正文最前面、所有现有内容上方的区块，以及 (B) 表明所属 crate 的 area 标签。原正文不删除、不改写、不重排。该区块使独自打开此 issue、没有其他上下文的读者知道该做什么，以及怎样知道工作已完成。

已发布的完整示例：**#2397** — 先阅读（`gh issue view 2397`），并遵循其形状。

<a id="a-the-block-verbatim-in-shape"></a>

## (A) 形状原样采用的区块

```markdown
> **状態** — <one of: BLOCKER / 実装 / 計測器・ゲート / 設計判断 / 追跡 / 調査>・<one of: 着手可 / 前提待ち: #NNNN / 判断待ち>
> **一行で** — <what is wrong or missing, in one sentence, no background>
>
> **やること**
> 1. <concrete first action a fresh reader can start today>
> 2. <…>
>
> **完了条件** — <a verifiable condition: a gate that goes green, a measured number, a fixture that exists, a diagnostic that fires. Never "it feels done">
> **根拠** — <the measurement, file:line, commit or PR that makes this real; or `未測定` if the issue rests on a claim nobody has checked>
> **触る場所** — <paths, 2-4 of them, the ones a fix would actually edit>

---
```

状态含义（选择最贴近的一种）：

| 状态 | 适用情况 |
|---|---|
| `BLOCKER` | 带有 `I-unsound` / `I-miscompile` / `I-divergence` / `regression` — 未关闭期间拒绝最终发布标签 |
| `実装` | 目标形态已知的 compiler/stdlib/runtime 代码变更 |
| `計測器・ゲート` | 工作是门禁、ratchet、manifest、测量框架，或修复其中给出虚假结果的部分 |
| `設計判断` | 编码之前需要裁决（通常标有 `mob`） |
| `追跡` | 由其他 issue 组成的 umbrella，或由 bot 维护的台账 |
| `調査` | 对象尚未明确；第一步是测量，而非编辑 |

<a id="rules-for-filling-it"></a>

## 填写规则

1. **推导，不编造。** 每一行来自 issue 自己的正文、评论或代码树。如果 issue 没说怎样算完成，就说明需要什么才能作出判断 — 这才是诚实的完成条件。
2. **验证代价低时，验证状态。** 例如运行 issue 自带的 3 行复现、对其提及的 PR 使用 `gh pr view`、对它声称缺失的路径用 `ls`、对它声称不存在的符号用 `grep`。本任务不得启动构建、基准或测试套件。如果状态自起票以来已变化，在“一行で”中说明，保留原文字以显示原先情况。
3. **`根拠` 指向可核查的东西** — `crates/…/foo.rs:120`、PR 编号、从 issue 引用的一对实测数字、门禁脚本。如果无从核查，就写 `未測定`，不要写貌似合理的句子。这个字段里未经验证的主张比空白更糟。如果依据是 issue 自己的数字，说明归属：`本文の実測(2026-09-13)`。
4. **不新增主张。** 不添加分析，不提议 issue 中尚未包含的设计，不估计工作量或时长。这是包装，不是改写。
5. **标签使用日语，内容使用 issue 原有语言。** 六个字段名固定为日语，便于读者扫读。
6. **`前提待ち: #NNNN`** 仅用于该项落地之前 issue 无法开始的情形。*相关*不等于前置条件。
7. **任何地方都不出现邻近项目名** — 不得在 issue 中写出比较对象语言／工具链的名称。使用“a neighbouring toolchain”／“近隣実装”这一写法。（用户的长期指示。）

<a id="b-the-area-label"></a>

## (B) area 标签

每个 issue 恰好有一个 `A-*` 标签（工作确实跨两个 crate 时才可有两个），从已有标签选择，不创建新标签：

| 标签 | crate |
|---|---|
| `A-frontend` | `crates/almide-frontend`、`almide-syntax` — parse、check、diagnostics、import table |
| `A-ir` | `crates/almide-ir`、`almide-mir`、`almide-optimize`、`almide-spine` |
| `A-codegen` | `crates/almide-codegen` — nanopass、Rust lowering、native borrow/RC inference |
| `A-wasm` | `crates/almide-wasm`、`almide-wasm-run`、`almide-wasm-vm` |
| `A-runtime` | `runtime/rs`、`crates/almide-rt-core` — native intrinsic |
| `A-interp` | `crates/almide-interp` — 第三个 oracle |
| `A-stdlib` | `stdlib/*.almd`、`crates/almide-types` 的 stdlib registry |
| `A-driver` | `crates/almide-driver`、`src/` — CLI: run/build/test/check/fmt/bench |
| `A-perf` | `research/benchmark/perf`、`scripts/check-perf-ratio.sh` |
| `A-ci` | `.github/workflows`、`scripts/*.sh` 门禁、台账和 ratchet |
| `A-modules` | 模块与包系统 — 解析、子模块、MVS、dialect epoch |

- 标签必须与“触る場所”一致。无法选择时说明 issue 的路径不明确 — 在报告中写明，不添加 `A-*` 标签。
- 如果既无 `bug` 也无 `enhancement`，且正文分类明确，则添加（错误行为 = `bug`，新增表面 = `enhancement`）。
- **不得添加、删除或修改** `I-*`、`regression`、`P-*` 或 `mob`。这些属于严重程度／调度／mob 决策，不应由我们在格式整理时决定（`docs/project/ISSUE-TAXONOMY.md`）。
- 用 `gh issue edit <n> --add-label A-xxx` 应用（重复 `--add-label`）。

<a id="mechanics-this-part-is-not-optional"></a>

## 操作步骤（此部分不可省略）

- 先获取：`gh issue view <n> --json body --jq .body > $SCRATCH/i<n>.md`
- 在该文件前面添加区块，构建新正文（`cat hdr i<n>.md >
  i<n>.new.md`）；不得传入由获取文件之外的内容构建的 `--body`。
- 写入：`gh issue edit <n> --body-file $SCRATCH/i<n>.new.md`
- 重读并用不会误判的方法检查往返：再次获取已发布正文，断言原正文的前 200 个字符仍存在其中（用 Python 比较，不靠目测）。否则从取得的副本恢复（`gh issue edit <n> --body-file $SCRATCH/i<n>.md`），并将该 issue 报告为跳过。
- 幂等性：若取得的正文已经以 `> **状態** —` 开头，则该 issue 已处理 — 不再添加第二个区块。检查标签后继续。
- 正文为空或获取失败时，跳过并报告，不猜测。正文是其他 issue 复选框清单的追踪 issue 仍需添加区块，并保留全部复选框原样。

<a id="report-back"></a>

## 报告

每个 issue 一行：`#NNNN <状態> <A-label> ok`；另外对跳过的、验证与正文矛盾的、无法选择 area 标签的 issue 附简短说明。

> 保留模板的说明译文：状態（状态）从列出的分类及可开始／等待前提／等待决定中选择；一行で（用一句话）不加背景地说明问题或缺失；やること（待办）从新读者今天即可开始的具体第一步列起；完了条件（完成条件）须可验证，例如门禁通过、实测数值、fixture 存在或诊断触发，而非“感觉完成”；根拠（依据）为测量、file:line、commit、PR，未经检查的主张则填未測定（未测量）；触る場所（修改位置）为修复实际会编辑的 2–4 个路径。
