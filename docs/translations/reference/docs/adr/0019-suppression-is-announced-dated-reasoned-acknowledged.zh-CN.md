<a id="adr-0019-suppression-is-announced-dated-reasoned-and-acknowledged--its-shape-is-fixed-before-the-construct-exists"></a>

# ADR-0019：抑制必须公告、有期限、有理由并获确认 — 在构造存在前固定其形式

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0019-suppression-is-announced-dated-reasoned-acknowledged.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0019-suppression-is-announced-dated-reasoned-acknowledged.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0019-suppression-is-announced-dated-reasoned-acknowledged.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md)
> Accepted 固定的是未来构造方案；原文明示此处没有实现，列出门禁也均不存在。当前缺失及未来引入是固定原文时点陈述，并非新审计。未签名确认 trailer 是声明，不验证身份或签名。


- **状态**：Accepted — 2026-09-22 决定，未另开 ○× 轮次，依维护者常设指示“需要 ○× 时采用推荐项”。下述每项选择（写法、范围、期限、trailer 名、退出行为）均直接采用推荐选项。此处没有任何内容已实现。本记录约束未来引入该构造的 PR（#2152 项 4；arena-breakthroughs T8）。
- **日期**：2026-09-22
- **背景**：Almide 没有抑制构造，没有 `@allow`、注释 pragma、清单豁免或命令行标志。每次编译发现的每条警告都会在每次编译中输出（唯一隐藏警告的开关是 REPL 内部的 `SUPPRESS_WARNINGS`，任何源文本都无法触及）。需求压力将来自已列入路线图的三个方向：dialect epoch 把警告升级为错误（E053 自身模块文档正有此计划）、lint 出现误报、bounded profile 增加规则。语言针对的编写者是模型，而模型会反射性复现 `#[allow(dead_code)]` 和 `// eslint-disable-next-line`，倾向于静默且永久的豁免。`attr_vocab.rs` 已将“接受一切、不报告任何内容的构造”称为对这类编写者最糟的表面。调查（arena-breakthroughs §2.3）找到一个相邻例子，具有非静默、有期限且在提交历史确认的抑制：aver 的 `[[check.suppress]]` 要求理由，仅适用于警告，输出各文件豁免消除了多少警告，并报告未匹配任何内容的规则。
- **决定**：Almide 获得抑制构造时，必须采用以下形式，不得更宽松：
  1. **写法与范围。** 在一个声明（`fn`、`effect fn`、`type`、顶层 `let`、`test`）上放置 `@allow(code = "E053", until = "2027-01-31",
     reason = "…")`，仅豁免该声明 span 内的这一个代码。一个属性豁免一个代码，两个代码需两个属性，各有自己的日期和理由。不提供模块级、文件级、注释、清单或命令行形式。
  2. **仅限警告。** 只能豁免编译器以 warning 级别输出的代码；指名 error 级别代码本身就是错误。dialect epoch 将警告升级为错误时，所有对应豁免在该 epoch 变成错误，豁免不能延期 epoch。
  3. **非静默 — 每次编译重新公告。** 每个有效豁免在 stderr 输出一行 `note[E053]: waived at <file>:<line> until
     <date> (<n> occurrence(s)): <reason>`；本次编译未抑制任何内容的豁免报告为未使用。`check`、`build`、`run`、`test` 均输出，`--json` 携带 `waived` 记录；任何标志、环境变量或 profile 都不能关闭。已豁免警告不计入 `--deny-warnings`；公告本身不是警告，也不能被豁免。
  4. **有期限 — 过期即错误。** `until` 必填，为距编译日期最多 **180 天**的 ISO-8601 日历日期。`until` 之后首次编译时，该豁免成为 ERROR，指出豁免、理由及过期日期。缺失、格式错误或超出期限的日期也报错。编译日期取系统时钟，除非设置了 `SOURCE_DATE_EPOCH`，以便旧 tag 重建固定到其自身日期；日期只决定判定，不进入输出字节。
  5. **有理由。** `reason` 必填，修剪空白后不能为空。
  6. **通过提交 trailer 确认。** 差分新增 `@allow(`，或修改现有属性的 `code`、`until`、`reason` 的提交，须携带 **`Waiver-Acked-by: Name <email>`** trailer。删除豁免无需 trailer。
- **理由**：(1) *非静默*：`#[allow]` 失败在写一次后再无人阅读。每次构建重新公告每个豁免，将债务放在能偿还者面前；对模型，就是放进后续每次构建的上下文窗口。aver 的移除警告计数和未用规则报告出于同样想法，逐个打印豁免则更进一步。(2) *有期限*：有理由却无终点的豁免仍是永久的；日期将其变为借款。180 天足够等待实际 upstream 修复，又足够短，防止用 `until = "9999-12-31"` 把借款变成赠与。(3) *过期即错误*：“过期豁免”若是 WARNING，本身又可被豁免和忽略，正是该构造要阻止的倒退。(4) *有理由*：无人解释的豁免，就是无人能够撤销的豁免（aver 对同一规则的表述）。(5) *获确认*：agent 不能静默借款，历史记录谁同意。(6) *`@allow` 写法*：熟悉 Rust 的模型会写没有日期和理由的 `@allow(x)`。以 `@allow` 为真实属性，这种尝试得到精确错误，fix-it 指出缺失字段，捕获并修正先验。换成任何其他名字，`@allow(x)` 会成为未知属性，即 E053 警告，再次走向静默路径。(7) *声明范围*：编辑局部性，豁免随代码移动，出现在代码编辑者的 diff 中。(8) *仅限警告*：错误是语言规则，豁免规则就成了私有方言。
- **替代方案**：
  (a) *永不添加抑制* — 虽是当前状态，但作为常设决定被拒绝。期限压力下第一个误报 lint 或 epoch 升级会产生临时逃生口（关闭检查、把正确代码改得更差、固定旧编译器）。现在固定形式，才能阻止压力下快速加入静默 `@allow`。
  (b) *Rust 的 `#[allow(lint)]`* — 静默、永久、无理由；RFC 2383 的 `reason =` 可选，`#[expect]` 仅在期望不再成立时发声。四项属性中不满足三项。
  (c) *清单表（aver 的 `[[check.suppress]]`）* — 有理由、有报告、集中管理；因局部性被拒绝：从函数编辑者不看的文件进行 glob 匹配，重命名时静默扩张或孤立，不出现在函数 diff 中。保留其未用规则报告（决定 3）。
  (d) *命令行标志（`--allow E053`）* — 位于构建脚本而非源码，CI 与本地运行不一致，没有理由、没有日期。
  (e) *注释 pragma（`// almide: allow E053`）* — 注释不是 AST，格式化器、LSP、检查器看不到；拼错 pragma 无效果，再现 E053 失败。
  (f) *过期作为警告* — 因理由 (3) 拒绝。
  (g) *用必填跟踪 issue URL 替代理由* — URL 离线失效，模型无法读取；若有 issue，理由仍可附其编号。
  (h) *签名确认*（签名提交或签名 trailer）— 签名政策是仓库设置而非语言规则，且并非每位贡献者签名；trailer 是记录的声明（见后果）。
- **构造落地后的强制执行**（引入 PR 须携带每一行，当前均不存在）：

  | 属性 | 门禁 |
  |---|---|
  | 非静默 | `tests/diagnostics/` 中断言公告的 broken/fixed 对；CLI 测试在同一豁免文件上运行 `check`、`build`、`run`、`test`、`--json`、`--deny-warnings`，每次断言 note，并检查未用豁免报告 |
  | 有期限 | 对缺失、格式错误、超期上限及已过期 `until` 各设 E-code，各带 `tests/diagnostics/` 对；过期夹具固定 `SOURCE_DATE_EPOCH` 保持确定性。代码由引入 PR 分配，此处不预留 |
  | 有理由 | 缺失或空 `reason` 的 E-code 及其测试对 |
  | 仅限警告 | `@allow` 指向 error 级代码的 E-code，以及验证 epoch 升级把现有豁免变为该错误的测试 |
  | 获确认 | `scripts/check-waiver-acks.sh` 由 CI 的 `checks` job 对 PR 提交范围运行，并由 lefthook `commit-msg` hook 运行；在 `.almd` 文件新增或修改 `@allow(` 的提交必须携带 `Waiver-Acked-by:`（用 `git interpret-trailers --parse` 读取）；`proofs/gate-verification.toml` 中一行及其负向测试 |
  | 清单 | 仓库自身除诊断夹具外的有效豁免数量为带记录、只能缩减的上限，初始为 0 |

- **后果**：每个抑制在每次构建可见、会过期、解释自己，并在历史中有具名确认者；写熟悉形式的模型会被修正为完整形式。成本是依赖日期的构建判定（由 `SOURCE_DATE_EPOCH` 及日期不进入输出字节来约束）、含豁免代码的 stderr 噪声（有意为之）、贡献者须学习的 trailer（hook 会提示）。确认是声明，不是证明：git trailer 是未签名文本，门禁强制存在和形式，不验证身份；伪造确认是在永久历史中可见于审查的虚假声明，仅主张这种强度。今天没有变化，这是审查引入抑制构造 PR 的标准形式。
- **证伪条件**：(1) 构造存在后，连续两个发布中，仅推迟 `until` 的提交多于删除豁免的提交，则借款模型失败，重审决定（续期限制或必需跟踪 issue）。(2) 若 Dojo 测量显示模型补丁为某代码插入 `@allow` 比修复更频繁，则熟悉写法论证错误，应重审名称。(3) 若 upstream 修复周期更长（依赖发布周期），180 天上限迫使续期，则重审期限长度，但不重审到期机制。
- **参考**：#2152（项 4）；docs/roadmap/active/arena-breakthroughs.md（T8 项 4、§2.3）；crates/almide-frontend/src/attr_vocab.rs（E053）；`../almide-references/aver/docs/cli.md`、`[[check.suppress]]`；Rust RFC 2383（lint 理由）、https://rust-lang.github.io/rfcs/2383-lint-reasons.html ；`git interpret-trailers`、https://git-scm.com/docs/git-interpret-trailers ；Linux kernel 补丁流程中的 `Acked-by:`、https://docs.kernel.org/process/submitting-patches.html ；`SOURCE_DATE_EPOCH`、https://reproducible-builds.org/docs/source-date-epoch/ 。编号：0018 被进行中的分支（value-strings 记录）占用，因此本记录使用 0019 以免冲突。
