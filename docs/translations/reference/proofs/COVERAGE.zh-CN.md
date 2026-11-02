<a id="compiler-structural-coverage--2026-07-04-proofscoveragesh"></a>

# 编译器结构覆盖率 — 2026-07-04（proofs/coverage.sh）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](COVERAGE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](COVERAGE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](COVERAGE.zh-CN.md)

> 非规范性、固定原文的历史读者快照。原本保持未修改，仍为其使用方的权威来源；未自动同步、重新测量、独立验证证明、修改检查或执行所录命令。日期、计数、表格、限制及后续修订保留原范围；这不是当前覆盖率报告，也不保证内存安全或认证。7月快照仍包含已退役渲染器文件。按原文保留历史叙述：9月29日“删除只可能提高TOTAL”的预测之后，9月30日记载工作负载覆盖丢失。10月2日声称的逐crate增量与表格差值也保持不变，不静默调和。更新下限或运行mutation的指示是历史资料，不构成执行授权。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/COVERAGE.md)

工作负载：almide-mir + almide-codegen 测试套件、针对全部可运行 spec 的 render_program（v1 路径）、`almide test spec/`（v0 生产路径）。

> **自 #2753（2026-09-29）起**，render_program 全量扫描已移除：检查让每个可运行 spec 程序通过发布版 `almide build --target wasm`（强制结构路径）及 `almide verify --emit` 见证生成器，并从报告中排除旧 WAT 渲染器文件（#2761 删除它们）。下面的快照早于这一变化，仍列出这些文件。

```
crates/almide-frontend/src/lower/mod_p2.rs                                                      735               111    84.90%          21                 4    80.95%         314                41    86.94%           0                 0         -
crates/almide-frontend/src/lower/mod_p3.rs                                                      398               127    68.09%          17                 4    76.47%         157                43    72.61%           0                 0         -
crates/almide-frontend/src/lower/statements.rs                                                  665                90    86.47%          23                 3    86.96%         273                44    83.88%           0                 0         -
crates/almide-frontend/src/lower/types.rs                                                       127                 6    95.28%          13                 1    92.31%          58                 4    93.10%           0                 0         -
crates/almide-frontend/src/stdlib.rs                                                            310               270    12.90%          12                 6    50.00%         148               128    13.51%           0                 0         -
crates/almide-frontend/src/type_env.rs                                                          376                79    78.99%          38                 7    81.58%         219                41    81.28%           0                 0         -
crates/almide-mir/src/certificate.rs                                                           1237               321    74.05%          48                 3    93.75%         688               171    75.15%           0                 0         -
crates/almide-mir/src/certificate_p2.rs                                                         824                 5    99.39%          32                 1    96.88%         347                 3    99.14%           0                 0         -
crates/almide-mir/src/coown_names.rs                                                             39                 0   100.00%           2                 0   100.00%          17                 0   100.00%           0                 0         -
crates/almide-mir/src/lib.rs                                                                    373                61    83.65%          13                 2    84.62%         192                42    78.12%           0                 0         -
crates/almide-mir/src/lib_p2.rs                                                                 186                 0   100.00%          20                 0   100.00%         114                 0   100.00%           0                 0         -
crates/almide-mir/src/lower/binds.rs                                                            515                57    88.93%           5                 0   100.00%         208                17    91.83%           0                 0         -
crates/almide-mir/src/lower/binds_p2.rs                                                        1143               376    67.10%           8                 3    62.50%         569               207    63.62%           0                 0         -
crates/almide-mir/src/lower/binds_p3.rs                                                         998                55    94.49%          26                 1    96.15%         419                21    94.99%           0                 0         -
crates/almide-mir/src/lower/binds_p4.rs                                                        1223               484    60.43%          17                 3    82.35%         502               186    62.95%           0                 0         -
crates/almide-mir/src/lower/calls.rs                                                            575               143    75.13%          18                 9    50.00%         307                64    79.15%           0                 0         -
crates/almide-mir/src/lower/calls_p2.rs                                                         817               212    74.05%          11                 2    81.82%         431               149    65.43%           0                 0         -
crates/almide-mir/src/lower/calls_p3.rs                                                         576               207    64.06%          16                 2    87.50%         273                96    64.84%           0                 0         -
crates/almide-mir/src/lower/calls_p4.rs                                                        1953               583    70.15%          28                16    42.86%         883               285    67.72%           0                 0         -
crates/almide-mir/src/lower/control.rs                                                         1129               352    68.82%          31                 3    90.32%         673               218    67.61%           0                 0         -
crates/almide-mir/src/lower/control_p2.rs                                                      1258               191    84.82%          33                 3    90.91%         635               108    82.99%           0                 0         -
crates/almide-mir/src/lower/control_p3.rs                                                      1108               349    68.50%          25                 5    80.00%         535               162    69.72%           0                 0         -
crates/almide-mir/src/lower/control_p4.rs                                                      2321               551    76.26%          39                 6    84.62%         918               174    81.05%           0                 0         -
crates/almide-mir/src/lower/control_p5.rs                                                      3463              2062    40.46%          50                28    44.00%        1483               838    43.49%           0                 0         -
crates/almide-mir/src/lower/layout.rs                                                            16                 0   100.00%           4                 0   100.00%          12                 0   100.00%           0                 0         -
crates/almide-mir/src/lower/mod.rs                                                             1281               223    82.59%          80                11    86.25%         832               122    85.34%           0                 0         -
crates/almide-mir/src/lower/mod_p2.rs                                                          1101               172    84.38%          77                 3    96.10%         569                69    87.87%           0                 0         -
crates/almide-mir/src/lower/mod_p3.rs                                                           904               201    77.77%          23                 4    82.61%         472               119    74.79%           0                 0         -
crates/almide-mir/src/lower/mod_p4.rs                                                          1189               351    70.48%          39                12    69.23%         629               202    67.89%           0                 0         -
crates/almide-mir/src/lower/mod_p5.rs                                                          1193               138    88.43%          55                 3    94.55%         695                81    88.35%           0                 0         -
crates/almide-mir/src/lower/mod_p6.rs                                                          3111               676    78.27%         104                17    83.65%        1941               464    76.09%           0                 0         -
crates/almide-mir/src/lower/tail.rs                                                            1037               408    60.66%          12                 4    66.67%         530               207    60.94%           0                 0         -
crates/almide-mir/src/purity.rs                                                                  81                 0   100.00%           5                 0   100.00%          46                 0   100.00%           0                 0         -
crates/almide-mir/src/render_rust.rs                                                            457               106    76.81%          20                 2    90.00%         268                57    78.73%           0                 0         -
crates/almide-mir/src/render_wasm.rs                                                            769                54    92.98%          43                 0   100.00%         433                28    93.53%           0                 0         -
crates/almide-mir/src/render_wasm/registry.rs                                                     3                 0   100.00%           1                 0   100.00%         648                 0   100.00%           0                 0         -
crates/almide-mir/src/render_wasm_p2.rs                                                         585               109    81.37%          10                 0   100.00%         304                57    81.25%           0                 0         -
crates/almide-mir/src/render_wasm_p3.rs                                                           3                 0   100.00%           1                 0   100.00%           3                 0   100.00%           0                 0         -
crates/almide-mir/src/translation_validation.rs                                                 256                84    67.19%          15                 0   100.00%         145                41    71.72%           0                 0         -
TOTAL                                                                                        199403             70872    64.46%        6394              2316    63.78%       98363             33553    65.89%           0                 0         -
```

低覆盖率台账（下一批编写测试的目标，优先影响输出的部分）：
- lower/control_p5.rs 43%——defunc HOF 引擎（最新、最大的未测试范围）。
- lower/tail.rs 61%、binds_p4 63%、binds_p2 64%——bind/tail  lowering 分支。
- lower/calls_p3 65%、mod_p4 68%——路由及调用参数实体化。

<a id="re-measurement--2026-07-04-after-the-crush-pass"></a>

## 重新测量 — 2026-07-04（crush pass 后）

TOTAL 行覆盖率在分母增长后仍为 **65.89%**（98,363 行增至 98,573 行——该 pass 添加 zip-fusion / scalar-tuple-fold / beta-reduce / eager-init 机制及其固定单元测试，因此范围扩大时比例仍保持）。五个已晋级 probe（tests_part5）直接覆盖最新 defunc 分支。下一目标不变：control_p5 defunc 引擎、tail.rs、binds_p2/p4。

<a id="re-measurement--2026-07-04-crush-pass-close-nn-walls-0"></a>

## 重新测量 — 2026-07-04（crush pass 收尾：nn 拒绝点为 0）

TOTAL 行覆盖率为 **66.30%**（99,602 行，33,565 未覆盖）——分母增长约 1,240 行（matrix_core 路由、一般不忠实闭包拒绝、defunc `list.find`、跨模块记录名称规范化），但未覆盖只增加 12 行，从 65.89% 上升。七个新增已晋级固定测试（tests_part5：adt 元组构造器、构造器列表字段递归、matrix 基础/拒绝/归一化+bytes、defunc find、load_weights 记录返回）几乎完全覆盖新增范围。render_wasm/registry.rs 已覆盖行从 648 → 679（matrix_core 自托管）。下一目标不变：control_p5 defunc 引擎（最大未测试范围）、tail.rs、binds_p2/p4。

<a id="rung-2-instruments--2026-08-26-566"></a>

## 第 2 阶段仪器 — 2026-08-26（#566）

先测量约束：rustc 的 MC/DC 插桩已不存在（2024 年加入，2025 年 8 月删除——rust-lang/rust#144999；实测 1.96.1 接受 `block | branch | condition`），因此 MC/DC 级证据是逐决策论证，而非编译器标志：

- **proofs/mcdc-ledger.sh / .toml**——十个安全性文件（perceus_verified、certificate 系列、translation_validation、effect/capability 检查）中的每个布尔运算符位置都有一行：`vectors` 指定独立性对单元测试（每项测试只翻转一个条件，判定随之翻转），或在只允许缩减的上限下标为 `pending`。落地时状态：51 个位置，其中 6 个由 16 项测试解决（perceus_verified 87/207/213、calls.rs 402/449），45 个待处理。
- **安全集合逐文件行覆盖率下限**——proofs/coverage-safety-baseline.txt，由 coverage.sh 在 TOTAL 棘轮后强制执行：即使 TOTAL 保持，也不允许安全性文件退化。以完整本地运行（1.96.1/macOS）减去 5 个百分点跨平台余量为初值（CI 1.89.0/linux 有几个百分点差异：同一晚 translation_validation 本地 70.09 对 CI 68.18；TOTAL 本地 65.50 对 CI 67.59）。
- **条件级后备防线**——`ALMIDE_COVERAGE_CONDITION=1` 加上使用日期固定 nightly（nightly-2026-08-01）的夜间 `coverage-condition` 作业，第一步探测标志：逐条件分支记录包含惰性运算符右侧，使用相同下限、相同棘轮。
- **推送时棘轮**——ci.yml 的 `coverage-ratchet` 作业在相关性检查后，于 push/PR 时运行同一检查（CI 实测 11–14 分钟；只在夜间运行曾使回归最多延后 24 小时发现）。

扫描器自身的诚实性也由首次运行调整：闭包头（`.or_else(|| …)`）按前置 token 规则排除；文件第一个 `#[cfg(test)]` 之后的位置属于测试本身，不纳入台账。

<a id="vector-truth-check--2026-08-27-566-mutation"></a>

## vector 真实性检查 — 2026-08-27（#566，mutation）

`vectors` 台账行以前只检查具名测试是否存在——空洞测试也能使检查保持绿色。`proofs/mcdc-mutation.sh` 关闭这一缺口：对每个已解决位置，在其精确字符偏移应用运算符交换 mutant（&& <-> ||）（所有退出路径均恢复源码并验证哈希），要求指定 vectors 在未变异时为绿、mutant 下为红。v1 mutant 类别是运算符交换；条件级固定值 mutant 需要操作数范围（AST），是下一项棘轮。每晚运行（作业“mcdc-mutation”）；绝不与其并行运行 cargo。

<a id="condition-floor-re-anchored--2026-09-29-2941"></a>

## 重新锚定条件下限 — 2026-09-29（#2941）

夜间逐条件作业在 v0.65.1-rc4 变红：TOTAL 55.14%，低于 56.55% 下限（fuzz-nightly run 36537776113）。最后一次绿色夜间运行是 9d80eb004 上的 36457122588，值为 56.94%。#2922 将该检查工作负载从旧 `render_program` 移至结构 wasm 构建和见证生成器。整个下降可由此次移动解释，分为两部分，均属结构变化。

从两个夜间运行的表逐文件测量（分支 / 未覆盖）：

| 步骤 | 分支 | 未覆盖 | TOTAL |
|---|---|---|---|
| 最后绿色夜间运行（9d80eb004） | 37886 | 16313 | 56.94% |
| 同一夜排除 INCUMBENT_ONLY 文件 | 36480 | 15888 | 56.45% |
| rc4（371de8ff5） | 36820 | 16518 | 55.14% |
| #2930 后 develop（54349cd6c，probe run 36550517708） | 36745 | 16429 | **55.29%** |

1. **分母。** #2922 将旧实现专用文件（`render_wasm*`、`region_*`、`pipeline_{b,c,link,global_slots,test_runner}` 等）移出范围。这就是 #2935 删除的 1406 个分支，其中 425 个未覆盖（70% 覆盖）。排除它们后，同一绿色夜间运行从 56.94% 变为 56.45%，已低于下限。该下限于 2026-09-04 在这些文件仍纳入范围且被执行时设定。
2. **仅能通过已删除流水线到达的 lowering 过程。** `almide-mir/src/lower/` 在总数约 11270 不变的情况下，未覆盖分支从 3720 增至 4315（+595）。旧流水线 lowering 每个程序生成的 `__drop_*` / `__repr_*` 源码，并运行 `pipeline_b` 的程序 pass（`populate_abi_registries`、`rewrap_never_err_into_result_targets` 等）。两个产品路径都不再调用这些代码：
   - 见证生成器仅 lowering 程序自身函数；
   - 原生路径也仅 lowering 程序自身函数。

   在 #2935 的 head 上，死代码 probe（将 `lower` 设为私有）发现 172 个条目、约 7,400 行，其剩余调用者仅为自身单元测试及保留的 `emit_cert*` / `classify_corpus*` 示例。#2950 将删除它们。在此之前，没有工作负载能到达它们，增加工作负载也不会测到产品运行的内容。

剩下的是双向噪声：其他报告 crate 的未覆盖减少 40，新结构文件的 140 个分支中 46 个未覆盖。

因此将下限重新锚定到 #2930 后 develop 的 CI 测量值 **55.29%**（`proofs/coverage-baseline-condition.txt` 5655 → 5529）。#2935 和 #2950 只移除没有任何工作负载到达的代码，因此各自只可能提高 TOTAL。每项落地后的第一个绿色夜间运行执行 `--update`，将下限再次向上锁定。

<a id="condition-floor-raised--2026-09-30-batch-073-2950"></a>

## 提高条件下限 — 2026-09-30（batch-073，#2950）

develop 降至 **53.46%**（e5b570e19 上的一次性 probe run 36583088728：36691 分支，17076 未覆盖），低于 55.29% 下限。相对下限自身测量（probe run 36550517708：36745 / 16429），总数近似不变而未覆盖分支增加 647。旧实现删除移除了经旧渲染路径驱动 MIR  lowering 的测试与工作负载，使该 lowering 代码不再执行。大部分是 #2950 删除的死 lowering 代码（`repr_sources*`、`drop_sources*`、`usage_scan`、`variant_drop_field_frees`、`mod_p2_rewrap` 等）。单独 #2950 测得 55.06%（run 36578464452）。

batch-073（包含 #2950）测得 **55.35%**（6da40736b 上 run 36595422938：35957 / 16054），因此下限升至该值（`coverage-baseline-condition.txt` 5529 → 5535）。

TOTAL 恢复，但仍存活文件在分支总数不变时丢失了已覆盖分支。主要为：`lower/defunc_tuple_fold_b.rs` 在 78 中 34 → 0，`lower/calls_p4_b_tail.rs` 在 61 中 44 → 21，`lower/defunc_tuple_fold.rs` 在 70 中 35 → 16，`lower/result_ctors.rs` 在 116 中 74 → 62，`lower/mod_p5_b.rs` 在 146 中 119 → 108，`lower/calls_p4_c.rs` 在 96 中 55 → 45。#3007 跟踪通过产品路径实际运行的工作负载恢复它们，而非只重新锚定。

<a id="condition-floor-raised--2026-09-30-3007"></a>

## 提高条件下限 — 2026-09-30（#3007）

`crates/almide-mir/tests/lower_witness_live_shapes.rs` 通过见证生成器（`almide verify` 的 `program_witnesses`，唯一仍可到达这些位置的产品路径）驱动 #3007 列出的仍存活 lowering 过程。在基于 batch-074（7b0e8a93f）的两个一次性分支上，完全按照夜间条件作业测量：

| 运行 | 分支 | 未覆盖 | TOTAL |
|---|---|---|---|
| batch-074（probe run 36642157804） | 36255 | 16217 | 55.27% |
| batch-074 + 测试（probe run 36642433391） | 36255 | 16123 | **55.53%** |

下限升至该值（`coverage-baseline-condition.txt` → 5553）。逐文件已覆盖分支，前 → 后：`defunc_tuple_fold_b.rs` 在 78 中 0 → 34，`defunc_tuple_fold.rs` 在 70 中 16 → 35，`result_ctors.rs` 在 116 中 62 → 73，`mod_p5_b.rs` 在 146 中 108 → 113，`defunc_hof_inner.rs` 在 128 中 89 → 93。`calls_p4_b_tail.rs`（61 中 21）、`calls_p4_c.rs`（96 中 45）、`calls.rs`（120 中 89）未变化：未执行分支是 `prim.*` 底层分支，仅旧实现 lowering 自托管标准库函数体时能到达。测试还是删除它们，取决于 `prim` 是否属于用户公开面（#3025）。

此下限包含随 batch-074 进入 develop 的 #2950 删除；在未包含它的树上测量，TOTAL 约低两个百分点。

<a id="condition-floor-restored-without-moving-it--2026-10-02-3195"></a>

## 不移动下限而恢复条件覆盖率 — 2026-10-02（#3195）

夜间逐条件作业在 v0.66.0 窗口每晚都红（52.08–52.30%，下限 55.53%）。源码树并未失去测试，而是工作负载失去原生路径。`almide test spec/` 先在 wasm 路径运行每个文件，仅对该路径无法通过的文件做原生编译。当 wasm 路径需要覆盖率运行器未安装的 wasmtime CLI 时，每个文件都回退到原生，扫描因而偶然测量了 Rust 代码生成。7fb199704 把 wasm 路径移至嵌入式宿主，回退清空，almide-codegen 一夜丢失约 1,500 个已覆盖分支（develop 595d60459，51.86%）：`pass_peephole.rs` 在 248 中 148 → 32，`pass_region_window.rs` 在 188 中 154 → 46，`pass_list_pattern.rs` 在 86 中 59 → 1，`pass_shared_cell_borrow.rs` 在 44 中 41 → 1，并遍及全部原生 pass 和 walker。

`coverage.sh` 现在还运行 `almide test spec/ --target rust`（CONDITION 模式开启；夜间行模式作业以 `ALMIDE_COVERAGE_NATIVE_LEG=1` 选择开启；推送时作业不含此路径，保留 45 分钟预算）。在修复分支上按夜间条件作业测量（run 36972952531）：

| 运行 | 分支 | 未覆盖 | TOTAL |
|---|---|---|---|
| v0.66.0-rc5 夜间（36966772832） | 38303 | 18270 | 52.30% |
| 修复分支（36972952531） | 38303 | 16618 | **56.61%** |

almide-codegen 增加 1,579 个已覆盖分支，almide-frontend 增加 8，其他 crate 未变。下限保持 55.53%。红色 #566 作业现在创建或评论带 `coverage-ratchet` 标签的 issue（`scripts/coverage-night-route.sh`、`coverage-route` 作业），使失败夜间结果无需打开运行记录也能送达人。
