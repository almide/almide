
> 本文是固定提交的读者参考译文，原文仍为权威规范；不会自动同步。Accepted、实现状态及测量数字均属于记录当时。翻译时没有重新运行测量或 GCE。“所有输入”“模型代码不会触及预算”“无需操作系统沙箱”及与其他编译器的比较，均是本 ADR 的主张和设计范围，不是译者独立作出的安全保证或部署建议。保留原文主张不代表已经复核；实际使用需另查当前规范、实现与威胁模型。

<a id="adr-0030-compile-cost-is-a-contract--every-phase-has-a-counted-budget-the-source-cannot-raise-it-and-exceeding-it-is-a-defined-diagnostic"></a>

# ADR-0030：编译成本是一项契约——每个阶段都有计数预算，源码不能提高预算，超限产生定义明确的诊断

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0030-compile-cost-is-a-contract.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0030-compile-cost-is-a-contract.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0030-compile-cost-is-a-contract.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0030-compile-cost-is-a-contract.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。



- **状态**：Accepted（2026-10-28，所有者的 Q5 裁定：实现分支的 GCE 轮次通过后采纳）。方向来自所有者于 2026-10-24 的裁定（○）：“把编译器视为处理不可信输入的程序，并以契约承诺其资源使用”。
- **日期**：2026-10-24
- **范围**：`almide check` / `build` / `fmt` / `lsp` / `ide` 对其输入（源文件、`almide.toml`、`almide.lock`）的预算、超限诊断，以及维持这些承诺的门禁。不包括编译后程序的运行时间；该事项归 `docs/roadmap/active/security-by-construction.md` 第 9 行负责。
- **实现**：#4136。
- **相关**：
  - #4114：推断类型以 2^n 增长；22 行就使 `almide check` 占用 3.7 GB。
  - 表达式嵌套上限（500）。
  - `docs/roadmap/active/security-by-construction.md` 第 9、25 行。
  - #4128：LSP 处理不可信项目。
  - #4129：环境开关。
  - ADR-0017：flight profile 是规范性子集。

<a id="context"></a>

## 背景

Almide 的运行时一侧承诺，不可信输入会在文档所述边界内，以值、定义明确的错误或定义明确的中止结束。编译器没有同样的承诺，却一直在处理不可信输入：

- 拉取请求上的 CI；
- 文件刚打开时的 LSP；
- 评分器和模型评估框架（Dojo）；
- 在线试验环境。

2026-10-24 红队轮次的测量结果：

| 形式 | 是否有界 | 测量结果 |
|---|---|---|
| 表达式 / 列表嵌套 `((((…))))` | 是，“nesting too deep (max 500)” | 深度 50,000：0.06 s，48 MB |
| `let t(i) = (t(i-1), t(i-1))` | **否** | 深度 22：3.7 GB；深度 39：在 5.4 GB 停止（#4114） |
| 通过泛型 `fn dup[T](x: T) -> (T, T)` 进行相同的翻倍 | **否** | 深度 18：295 MB，每行 ×2 |
| 用于穷尽性检查的宽元组 `match`，宽度 24 | 是，保持平稳 | 0.02 s，23 MB |

其他编译器在事后逐例为这些形式设限。以下仅为背景，本 ADR 不依赖它们的数字：

- Rust 的 `recursion_limit` 和 `type_length_limit`，**源码**可通过 `#![...]` 提高。
- TypeScript 的“type instantiation is excessively deep”。
- Swift 基于时间的“expression too complex”。
- GHC 的 `-freduction-depth`。

原文称，它们都没有同时提供以下三个性质：

- 每个阶段都有界；
- 源码不能改变界限；
- 判定不依赖机器。

<a id="decision"></a>

## 决定

**D1. 一张预算表。** 每个随输入增长的阶段都在同一个注册表中拥有命名预算，并列出达到预算时的诊断。该注册表与 `ALMIDE_*` 开关注册表（#2205）并列，并渲染进 `docs/specs/cli.md`。初始条目包括：

- 语法嵌套；
- 推断类型大小（节点数）；
- 每个程序的泛型实例化次数；
- 穷尽性检查工作量；
- 清单和锁文件大小；
- 依赖数量与深度。

**D2. 计数，不计时。** 预算统计节点、实例化、字节或步骤，绝不统计秒数或空闲内存。同一输入得到同一判定：

- 在每台机器上；
- 在原生编译器和运行于 wasm 的编译器中；
- 在 CLI 与 LSP 中。

（参见“判定随空闲内存变化”的教训。）

**D3. 源码不能提高预算。** 没有能修改预算的属性、清单键或锁文件条目。运行编译器的人可以使用 `--budget NAME=N` 修改。`--profile critical` 拒绝该标志。环境变量也不能改变预算；这遵循 #4129。

**D4. 超限是定义明确的诊断。** 它是带有源码位置、测量值、预算以及面向修复模型的提示的 E-code，例如“type too large (N nodes, max M): name the intermediate type or use a record”。不能是崩溃、内存耗尽后的强制终止或卡死。

**D5. 先解决结构，最后才靠预算。** 如果一种表示方式能使爆炸性增长不可能发生，就使用它。类型通过 hash-cons 或 arena-intern 共享，因此 `(t, t)` 的成本为 O(1)；诊断中的类型打印省略重复结构。预算是对结构无法防止之事的最后防线。

**D6. 根据测量设置预算。** 每个数值约为在 `spec/`、`stdlib/`、练习语料和 Dojo 任务库中观察到的最大值的 10×。测量脚本与输出一并提交。原文的设计主张是：模型编写的代码不会触及预算，只有对抗性输入才会。

**D7. 门禁。** CI 针对每一种已知的爆炸性增长形式运行生成器（#4114 的翻倍及其泛型形式、嵌套、宽 match、长插值链、大量 import），并断言两点：

- 每一种都以其预算对应的诊断结束；
- 在每个生成器的取值范围内，`almide check` 的计数工作量相对源码大小最多线性增长。

负面对照：禁用类型共享和类型大小预算，会使 #4114 生成器超出预算，门禁失败。

<a id="consequences"></a>

## 后果

- 结合第 9 行的运行时边界，将“无论向 Almide 提供什么输入，都在文档所述边界内以定义明确的结果结束”的主张从程序扩展到工具链。
- 预算是兼容性界面。提高预算是增量变更；降低预算可能拒绝此前能够编译的代码，因此需要在 `proofs/dialect-epochs.toml` 中加入条目。
- 其提出的后果是：LSP 与评分器可处理不可信源码，而无需为资源耗尽问题设置操作系统级沙箱。副作用隔离是另一回事（#4128）。

<a id="rulings-2026-10-27"></a>

## 裁定（2026-10-27）

1. D6 的数值由测量取得：`scripts/measure-compile-budgets.sh` 写入 `proofs/compile-budgets.toml`，每个新默认值约为语料最大值的十倍。早于本 ADR 的两个预算保留原值：`nesting` 仍为 500（语料最大值 33 的 15×），`manifest_bytes` 仍为 1 MiB（#4194）。
2. 预算表是单一规范表（`almide budgets --md`，嵌入 `docs/specs/cli.md`），对应单一契约 C-440：“表中每项预算均成立”。
3. `almide fmt` 共享该表。它只做解析，因此适用的预算是 `nesting`。
4. 从 D1 中删去“每个函数的生成代码大小”。生成代码相对 IR 线性增长，而 `nesting` 和 `type_size` 已为 IR 设限。
5. D5 与预算一起落地，而不是替代预算。复合类型共享参数列表（`almide_types::TyVec`）：克隆只复制指针，每个列表缓存节点数及是否包含推断变量。类型解析过程按变量和共享列表进行记忆化。在解除类型大小预算限制后，#4114 的链以相对其长度线性的时间和内存完成检查（`tests/compile_budget_test.rs`）。负面对照 `ALMIDE_CONTROL_NO_TYPE_SHARING` 使每次克隆重新复制整棵树。
