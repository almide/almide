<a id="adr-0015-the-reference-evaluator-is-a-fresh-source-level-judge-owned-evaluator-behind-a-black-box-protocol--seeded-by-λ_almd-ratcheted-by-an-abstain-ledger-amended-rust"></a>

# ADR-0015：参考求值器是黑盒协议背后全新、源码级、由裁判方拥有的求值器 — 以 λ_almd 为种子，通过 abstain 台账防止倒退（修订：Rust）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0015-reference-evaluator-is-fresh-source-level-python.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0015-reference-evaluator-is-fresh-source-level-python.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0015-reference-evaluator-is-fresh-source-level-python.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md)的非规范性阅读译本，不会自动同步，包括规范性陈述在内的原本仍具权威性。已采纳设计、历史测量、工具鉴定、证明及实现主张未经独立执行、认证或现状验证而保留。采纳不确立当前实现或保证。包括多行路径在内的 inline code 完全保留。

- **状态**：已采纳（2026-08-21 批准为附带航空质量条款的 Python；**同日修订为 Rust** — 见修订段；文件名保留原始 slug，ADR ID 和路径永久不变）
- **日期**：2026-08-20
- **背景**：QUALIFICATION.md 限制 2 — cross 执行分支判定的是一致，而非真相；两个目标以相同方式出错不可见。[#10](https://github.com/almide/als/issues/10) 指明填补这一缺口的工具：由本仓库拥有的参考求值器，以及 runner 中判定为 *legs == ref* 的 `ref` 分支。当前已存在内容的实测情况：
  - 该分支必须判定的语料：`spec/wasm_cross` 的 592 个 fixture、`spec/programs`、`spec/wasm_cross_pkg`；它们调用 **478 个不同 stdlib 函数**（`module.fn`，主要模块为 int/list/string/bytes/value/float/regex/map/json/fs/set/fan）。
  - 唯一机械化语义是 λ_almd 内核（C-280、edit-locality belt、Lean 4.29、1,583 行、无 Mathlib）：**12 种表达式形式**（int/string 字面量、var、let、单参数 call、ok/err、对 Result 的 match、`!`、`??`、print、seq），相对于关系已证明健全的 fuel-indexed 求值器，以及**48 个轨迹由求值器固定的程序**构成的生成语料（实现仓库中的 `proofs/kernel-conformance/`），另有表面 fixture `spec/wasm_cross/kernel_conformance.almd`。
  - 实现中的第三个 oracle `crates/almide-interp` 是 IR 解释器（10.8k 行），与编译器共享 parser、checker 和 lowering — 前半部分的缺陷会使三个分支以相同方式变化。
  - 裁判方工具链为 Python 3 + bash；`gates` 没有编译器、cargo 或 Lean。`tree-sitter-almide` 是手工维护的编译器 parser 镜像（1,050 行 grammar.js，仅 Rust binding，实现在自身一侧每周以门禁检查漂移）。
  - ADR-0014 已确定输出形状：固定期望由工具导出；`spec/wasm_cross` 的 Roc 形式轨迹只作为参考求值器输出出现。
- **决策**：
  1. **从规范文本全新编写。** 求值器依据 ALS 各章（`docs/specs/als/`，110 节）、语言文法（实现以 `docs/GRAMMAR.md` 发布的 EBNF，在此作为裁判方 parser 规范保留）及 stdlib 规范编写，绝不移植 `almide-interp` 的求值规则。移植会复制实现的语义，保留本来要捕捉的共同漂移；重新阅读会发现规范缺口，正如 doctest 清理过程所做的那样。
  2. **源码级、独立前端、抽象值。** 使用自己的递归下降 parser 解析 `.almd` 源码（无编译器 crate，也不构建 tree-sitter：第三套文法会增加自身漂移，并给门禁使用 Python 的仓库增加原生构建步骤）。值是抽象的（Int 按 ALS 先无界再 wrap，另有 String、List、Map、Set、Tuple、Record、Variant、Option、Result、Fn）— 不使用字节堆：裁判方建模的是程序观察到*什么*，而非实现*如何*布局。
  3. **Rust（stable、固定版本），独立 crate，置于黑盒协议之后。** 求值器是本仓库中的 crate `ref/`：仅 stable Rust（`rust-toolchain.toml` 固定 channel；无 nightly feature，因此经鉴定的工具链 — Ferrocene，[#18](https://github.com/almide/als/issues/18) — 可原样重新构建），不依赖任何 `almide-*` crate（门禁检查），依赖保持为零或经审查的少数。runner 绝不链接它，而是调用 REF PROTOCOL — `<ref> run <file.almd> --json` 输出 `{"exit": n, "stdout": s, "stderr": s}` 或 `{"abstain": {"class": c, "reason": r}}` — 因此任意语言的求值器（在其片段上的已编译 Lean 求值器、未来移植版本）均可插入，裁判方的接缝是协议而非实现。（修订 — 首次批准选的是 Python；为何在一行代码都未写前改变，见下文。）
  4. **以 λ_almd 为种子，从第一天起 100%。** 48 个内核程序和 `kernel_conformance.almd` 是首个 oracle：求值器在判定任何其他内容前，必须逐字节重现这些由求值器固定的轨迹；这一致性是 CI 永久保持的 1.0 下限（#10 的“λ_almd 一致比例”，以 fragment gate 为种子，只能随内核增长而增长）。
  5. **通过弃权保持诚实。** 求值器未实现的任何内容均为带 class 的 ABSTAIN（`stdlib:<module.fn>`、`syntax:<form>`、`runtime:<capability>`），逐 fixture 记录在 `proofs/ref-abstain.toml` 中，分类管理且只允许缩减 — 弃权既不是通过，也不是失败。禁止静默 fallback。
  6. **执行分支的判定为 *legs == ref*。** 对求值器实际求值的 fixture，native 和 wasm 必须分别等于参考可观察值（exit、stdout、stderr，按 `cross` 的方式 trim）；两个目标互相一致不再足够。对弃权的 fixture，保留既有一致性判定并统计弃权。每个契约增加 reference-backed 属性（其所有 fixture 均已求值），由 README generator 作为只增不减的比例输出。
- **依据**：参考的价值在于独立性，而此处独立性恰好只能一次在一个维度上获得：独立前端（从文法编写第二个 parser）、独立语义（对 ALS 文本的第二次解读）、独立 stdlib（从 stdlib 规范编写第二套实现）。每个被否决方案都为方便而放弃其中一项。Python 是小程序 tree-walker 最廉价的宿主，让裁判方仅用 `python3` 就能安装；协议让语言选择可以改变，其代价是移植而非重新设计。从内核语料开始，意味着求值器首先必须正确处理的 49 个程序，其预期轨迹是*已证明*的，而不是仅达成一致 — 这是当前两个仓库中唯一能得到真相而非一致的地方。
- **替代方案**：
  (a) *在 Lean 中扩展 λ_almd belt 直至覆盖整个表面* — 作为可执行分支被否决：整个表面的机械化是研究级工作（#10）；belt 保留为机械化种子，由一致性 ratchet 关联两者。(b) *在新 parser 后移植 `almide-interp` 规则* — 否决：复制了被测语义。(c) *以 tree-sitter-almide 为前端* — 否决：第三套手工维护、有自身漂移的文法，在 Python 门禁仓库中引入原生构建，且 parser 会接受编译器拒绝的内容。(d) *先用 Python* — 原始批准方案；同日由下方修订取代：切换成本为零时，上限更高的方案胜出。(e) *只判定手选子集* — 否决：abstain 台账判定全部内容，并说明跳过了什么。
- **航空质量条款**（随决策批准；每项都是门禁而非指南 — 修订针对 Rust 重新表述）：
  1. *确定性。* 求值器不依赖任何宿主顺序：每次 Map/Set 迭代均按 ALS 规定的顺序，而非宿主顺序。`HashMap`/`HashSet`（每进程随机迭代顺序）是禁用类型（`clippy.toml` 的 `disallowed-types`）；仅用有序结构。门禁运行语料两次，要求轨迹和判定逐字节相同。
  2. *全域性或弃权。* 语法全域性由构造保证 — 每种 AST 节点都是 variant，对 AST 的每个 `match` 都穷尽（编译器拒绝遗漏的形式）；未实现形式是带 class 的显式 `Abstain` 分支，绝非通配分支。stdlib 全域性通过测量确定：被判定语料调用的每个 `module.fn` 要么已实现，要么位于 `proofs/ref-abstain.toml`；运行时未处理的调用是求值器失败（协议错误），绝非静默通过 — 门禁枚举两张表，发现任何缺口即失败。
  3. *对求值器自身做 mutation testing。* 在 crate 上运行 `cargo-mutants`，由 kernel + cross 语料判定；kill rate 是不允许向坏方向退步的 ratchet，并列出幸存者（edit-locality 调查在 12 条法则中将 mutation 排在第一）。
  4. *固定、可鉴定的工具链 — 跟踪 Ferrocene。* `rust-toolchain.toml` 固定当前 Ferrocene 版本构建所依据的 upstream stable 版本（批准时：Ferrocene 26.05.0 == Rust 1.95.0；订阅本身是 #18 的资源项），因此从普通 rustc 切换为通过 `criticalup` 安装的 Ferrocene，只需改变 CI 步骤，无需改变代码或版本；不使用 nightly feature；在符合性声明中将工具链版本与候选二进制及平台一同记录。固定版本仅随 Ferrocene 发布一起变动。
  5. *不得将已规定语义委托给宿主（宿主多样性条款）。* native 目标在 Rust `std` 上运行；若参考实现将 `string.split` 写为 `str::split`，就与实现共享宿主语义，一致性测量的将是共享宿主，而非 ALS 文本。因此，ALS 自行规定行为的 std 操作（字符串分割/trim/case、浮点格式化与解析、整数解析、排序/顺序、集合迭代顺序、hashing）属于**禁用方法**（`clippy.toml` 的 `disallowed-methods`）— 求值器依据 ALS 文本，用 char/byte primitive 编写。wasm 目标（自托管 Almide）仍是实质不同的宿主，因此三方投票保留一个无关联分支。
  6. *独立于实现。* crate 的 `cargo tree` 不含 `almide-*` crate，也没有通向实现仓库的 path/git 依赖（门禁检查）。
- **修订（2026-08-21，在任何求值器代码出现之前）**：首次批准选 Python，理由是 DO-330 不要求对 TQL-4/5 验证工具的编译器进行鉴定（确实不要求 — 该主张依赖工具自身的验证，即条款 1–4），且 Python 宿主增加 N-version 多样性。对照主张阶梯（[#9](https://github.com/almide/als/issues/9)、[#18](https://github.com/almide/als/issues/18)）重新判定：可信度上限不同 — 若裁判方的参考求值器是可用经鉴定工具链重建的 stable Rust crate，对未来审查者的主张比 CPython 脚本更强；Rust 通过构造提供语法全域性（Python 唯一只能测量的条款），而宿主多样性的损失可通过结构化门禁（条款 5）弥补，不必依赖纪律。Python 的优势是编写速度与无需工具链 — 确实存在，但只能通过重写来改变。尚未写任何代码，切换成本为零，因此选择上限更高的方案。不变的是：从规范文本全新编写、独立前端、抽象值、黑盒协议、1.0 的 λ_almd 种子、abstain 台账和判定规则。`gates` 增加 crate 的缓存 cargo 步骤（其 header 不再写“没有编译器”）。
- **证伪条件**：(1) 求值器在语料上的墙钟时间超过 cross 分支自身 — 错的是设计而非语言；(2) abstain 台账连续两个版本不再缩减 — 则阻碍是 stdlib 规范缺口而非求值器，作为章节债务进入 #10；(3) 内核一致性回退 — 按定义是求值器错误（内核已证明），绝非反过来。
- **后果**：`ref/`（独立 crate：lexer、parser、求值器、stdlib 以及协议 CLI `ref run <file> --json`）；`scripts/check-ref-*.sh` 门禁（内核一致性 1.0、独立性、lint、全域性、abstain 台账、mutation ratchet）；带 `--ref` 命令的 `scripts/conformance.py --legs ref`；覆盖各 ref 判定 class 的 `selftest-conformance.py` 场景（ref pass、native ≠ ref、wasm ≠ ref、两者相互一致却均 ≠ ref — 共同漂移 class —、统计 abstain、格式错误的协议输出标红）；`proofs/gate-verification.toml` 中的一行；`proofs/ref-abstain.toml` 只缩不增；在此以 `proofs/kernel-
  conformance/` 保留内核语料并记录 generator commit（只重新生成，绝不编辑，ADR-0014）。仅当某 fixture 的 abstain 集合为空后，`spec/wasm_cross` 的 Roc 形式 `.expected` 轨迹才成为可能 — ADR-0014 的条件，如今有了具名工具。

## 原本注意事项

原始文件名、以 Python 为依据的论述和“没有编译器”的背景仍作为历史记录保留，而同日修订选择 Rust 并增加缓存 cargo 步骤。本译本保留这些内容，不暗中调和。48 个内核程序加一个表面 fixture、1.0 一致性下限、抽象值模型及显式弃权界定了所述范围；跨分支一致或本阅读版都不确立整个语言的真相或整个产物的安全。航空质量条款是该已采纳设计中提议的强制门禁，并非工具鉴定或认证已完成的证据。Ferrocene 版本、语料规模和仓库/工具链描述是当时原本的主张。“本仓库”与裁判方路径保持原本的裁判方/实现区分；本翻译不会创建求值器或门禁。固定原本没有独立附录；所有条款、修订、证伪条件和后果均已收录。
