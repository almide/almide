# DDD gauntlet

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[spec/gauntlet/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/gauntlet/README.md)。这是供读者参考的非规范性快照译文，不会自动同步。未对测量值及主张进行独立复核，以原文为准。
>
> 原文注意：这是多个时间点的记录。“5 个未立项”等标题及旧数量与表中已关闭状态不一致，“五个 issue”后实际列有七个 ID。译文保留原文，未作更正；请勿将其视为当前未解决数量或状态。

把 2026-08-22 至 2026-08-26 期间破坏既有实现的每种形态汇成一个可运行的矩阵。它来自一次练习：**编写普通的 ports-and-adapters 程序**——值对象、聚合根、repository protocol、两个 adapter、application service，在 composition root 中连接——然后观察编译器如何处理。

```sh
./run.sh /path/to/almide      # check / native / wasm for every cell
```

这次练习追踪了 24 个缺陷（#1539、#1547、#1549–#1583）；既有实现已关闭 22 个，尚有 2 个开放。价值不在修复本身，而在于**重放一致性语料无法触达其中任何一个**，而且有几个只是同一种错误换了外衣。

## 1. 完成目标移动了

`NON-CARRYOVER.md` 将完成定义为 "the burn-up replays the whole 599-fixture corpus"。练习开始时语料确为 599。现在是 **610 个 `spec/wasm_cross` 样例加 4 个新的 `tests/*.rs` native pin**，每个新增行都是本次练习发现的缺陷：

```
spec/integration/modules/{convmut,genlib,layports}/…      spec/lang/protocol_fallible_return_test.almd
spec/integration/modules/cross_module_generic_chain_test  spec/lang/mut_param_whole_assign_test.almd
spec/integration/modules/cross_module_layered_ports_test  spec/lang/mut_param_protocol_bound_test.almd
spec/integration/modules/cross_module_mut_receiver_test   spec/lang/effect_option_qq_test.almd
spec/integration/modules/cross_module_receiver_mode_test  spec/wasm_cross/bang_in_record_literal_field.almd
spec/wasm_cross/fallible_tuple_value_tail.almd            spec/wasm_cross/record_pair_result_return.almd
spec/wasm_cross/result_option_call_payload_return.almd    spec/wasm_cross/result_tuple_variant_payload.almd
spec/wasm_cross/result_tuple_list_slots.almd              spec/wasm_cross/unwrap_or_option_payload.almd
spec/wasm_cross/variant_pair_result_return.almd           spec/wasm_cross/qq_record_fallback.almd
spec/wasm_cross/mut_param_effect_never_err.almd           tests/native_mut_param_pins_test.rs
spec/wasm_cross/variant_record_literal_equality.almd      tests/generic_letter_type_name_test.rs
spec/wasm_cross/fuel_cut_in_arm_loop.almd                 tests/nested_package_import_test.rs
```

**教训不是“提高数字”。** 语料是对已裁定事项的封闭枚举，无法包含尚无人写程序触发的缺陷。依托它声称完成时，应说明这一点。Greenfield 版门禁应携带第二个维度：*针对语言编写、而非从 tracker 中收集的程序形态验收套件*。本目录是一个起点。

## 2. 缺陷类别及消除各类缺陷的不变量

### A. `check` 接受，但 backend 无法输出——占主导的类别

这是数量最多、也最关乎使命的类别：编译器告诉作者程序正确，随后却交给作者关于他们从未写过的生成代码的 rustc 错误。

| 单元 | 原问题 |
|---|---|
| `c1_cross_module_method` | convention method 的 receiver borrow mode 没有跨越模块边界，调用方按值传入，而定义要求引用 |
| `c2_mut_param_whole_assign` | 对 `mut` param 执行 `p = Record { … }` 时，针对 `&mut` 绑定输出了 `p = …` |
| `c3_mut_param_protocol_bound` | 在 protocol bound 下单态化 `mut` param 时丢失引用传递约定 |
| `c4_derived_variant_repr` | 对 derived-`Repr` variant 调用 `.repr()`，却调用了 derive 从未生成的函数 |
| `c5_generic_to_generic` | generic fn 调用另一模块的 generic fn 时，从未单态化被调用方 |
| `c6_borrowed_param_field_store` | 将借用的 `String` param 存入 field/slot 时，在需要拥有值之处输出 `&str` |
| `c7_generic_letter_type_name` | 名为 `T`（或 `A B C E F K U V`）的用户类型跨模块边界后被当成类型变量 |
| `c8_generic_applied_at_letter` | 将 `go[Q]` 应用于名为 `Q` 的类型时单态化失败 |

**不变量**：良类型程序必须可输出。若 backend 无法 lower 某种形态，应在 *check 阶段以源代码层诊断拒绝*，而不是先接受再在下游失败。以上每行都是 frontend 与 backend 对语言定义不一致的表现。Greenfield 应能说明在哪里强制一致，并设置 fuzz 不一致的门禁，而不是等待用户遇到。

### B. 从错误的命名空间解析名称——反复出现的系列

除上面的 `c7`/`c8` 外，还有 `#1558`（用户 fn 与任一已链接 stdlib fn 共享裸名称时，会被静默排除在重写之外：`fn replace` 遇到 wall，`fn bump` 能运行），以及既有实现更早的 `#1087`。每次都是同一种形态：**在另一个模块也会写入的表中查找裸名称。** 受影响集合不可见（取决于链接了哪些模块），也不稳定（增加 `import` 会改变无关函数）。

**不变量**：解析必须以已解析身份为键，而非裸拼写。如果 pass 必须按名称索引，键必须携带作用域。单字母类型名也是普通名称。

### C. 静默错误值——恰好一个

`w1_qq_record_fallback`：对 `Result[record, String]` 使用 `??`，并以 record literal 为 fallback 时，读到了错误值。这是集合中唯一产生错误答案而非拒绝的行；它由*扩大样例*而非 fuzzer 发现。

**不变量**：不能处理某类 payload 的路径应拒绝处理。router 的失败模式应是 wall，而非误读。

### D. 作者会踩到的表层缺口——2 个已修复，5 个仍未立项

| 单元 | 状态 |
|---|---|
| `s1_protocol_fallible_return` | 已修复（protocol method 中 `-> T!` / `-> T!E` 曾产生 parse error） |
| `s2_multiline_tuple` | 已修复（tuple literal 无法跨行，但 list/record/map 可以） |
| `s3_module_qualified_protocol` | **已关闭（#1589 stage 1）**——`ports.Store` 可在 bound 和 conformance list 中解析，限定符须像类型一样指向声明模块。裸的跨模块名称仍可解析，但会产生 deprecation warning，其 machine fix 为限定拼写 |
| `s4_bare_mut_self` | **已关闭（#1589 stage 1）**——凡写 receiver 的地方，`mut self` 都是 `mut self: Self` 的简写 |
| `s5_match_self` | **已关闭（#1590）**——`self` 曾位于 parser 的拒绝标识符表中，而作为标识符使用的前瞻缺少 `{` 分支；现在 `self` 按其本来的普通标识符解析，此单元 check 通过 |
| `s6_generic_protocol` | **已关闭（#1589 stage 1）**——`type UserRepo: Repository[User]` 和 `[R: Repository[User]]` 通过精确的实参替换被接受，调用运行唯一声明的实现（C-365，`spec/integration/generic_protocol_ports/`） |
| `s7_protocol_as_type` | **如实诊断（#1590），表达能力仍开放（#1589）**——E029 现在说明 "'Policy' is a protocol, not a type"，并抑制派生的 `E002`s；`List[Policy]` 形态仍先输出一个元素不匹配 `E001`（见 #1590），existential dispatch 仍属 #1589 |

**greenfield 的不变量**：这些是设计决策，而非 bug，应有意作出决定。无论 `self` 意味着什么，在每个位置都应相同；protocol 也应一致（只是 bound，还是也可作类型）。无论哪种拼写是规范形式，另一种能解析的地方，规范形式都应能解析。

### E. 跨目标一致性是一个矩阵，却曾被逐单元关闭

`x01`–`x16` 是 native 已运行、wasm 路径需要追赶的形态。值得学习的是*燃尽的形状*：五个独立 issue（`#1547 → #1564 → #1578 → #1579 → #1580 → #1581 → #1583`）分别关闭了最终证明属于同一矩阵的一个单元——payload 种类 × producer × 拼写 × 位置。每次关闭提交的措辞（"completing the matrix cells"）都泛化到了实际覆盖之外，而下一个 probe 又找到相邻单元。

**不变量**：一个系列应通过*枚举单元的可执行矩阵门禁*关闭，而非仅修复报告的实例。既有实现已在 `CLAUDE.md` 中为 stdlib API 写明此规则（"API families are extended by matrix, never point-wise… a gated matrix cannot drift"），只是未用于 codegen 系列。Greenfield 应使其成为结构性规则：lowering 路径声明覆盖的单元，由门禁断言声明。

## 3. 验证流程的教训——最可迁移的部分

发现这些问题的代价比任何单个 bug 都高。

1. **在错误路径运行的回归测试固定不了任何东西。** 本批次四个样例中的两个在*修复前*编译器上就通过，因为形态适合时 `almide test` 会在 wasm 路径运行文件，而 bug 位于 native codegen。仓库早在 `tests/effect_tail_generic_bound_test.rs` 写过规则：*"a corpus file would run on the wasm leg and assert nothing"*，三天后还是违反了。**信任每个 pin 前，都必须对照损坏的编译器做 A/B。**

2. **没有 pin 的修复再次损坏也仍为 green。** 某批次四项中的一项合入时没有样例，也没有 `proofs/` 条目；还得另开 issue 才提交标本。

3. **只活在提交信息里的标本不是标本。** 对该项从文字进行了五次重建，都没能重现 wall；真实触发条件比描述更窄。如果不能从记录重建 repro，记录就不是证据。

4. **局部 revert A/B 优于整个二进制 A/B。** 回退一个 hunk 并观察样例 wall 能证明单一因果；在父提交重建整个编译器，只能证明该提交中的*某件事*有影响。应将前者作为“此样例固定此修复”的证据标准。

5. **wall 信息必须指出原因，而非机制。** `#1558` 的 wall 说 "mut params are not in this brick"，但真正的修复是*重命名函数*。作者无法从该信息推导出这个操作。

## 4. 当前快照（既有实现 @ `4e5a15696`）

```
clean rows: 25    rows with a check/native/wasm gap: 9
```

9 项为：五个未立项的表层缺口（`s3`–`s7`），一个有意的 wasm decline（`x09`，`List[record]` 元素），`x13`（在 record-literal field 内，嵌套在 call/operator/interpolation 下的 `!`——**#1583，开放**），以及两个 layered package。两者分别被 `x13`（functional port）与 `#1576`（mut port：`mut`-param 的 err-path 语义需获正式裁定——callee 修改后返回 `err` 时，caller 的实参保留什么？）阻塞。`#1576` 已在结构化路径（0.61.2）获裁定并关闭：err 在任何 write-back 前传播，caller slot 因而保留调用前绑定，`pkg/mut_port` 在下方 greenfield gate 中与 native 输出逐字节相同。#1871 于 2026-09-30 作出相反裁定（(B)，write visible，#2917）：err arm 也携带 buffer，因此 err 前的写入会像 native 一样到达 caller；`cells/x18` 固定了这一点。

## 5. 如何使用这里的内容

- greenfield 一旦能承载这些表层形式，就对其运行 `./run.sh`。每行都来自真实程序，而非合成边界情况。
- 把 §2 的不变量当作设计评审问题，而非 bug 列表。若设计保持 frontend 与 backend 对同一语言达成一致、以身份解析名称、按矩阵关闭系列，这些类别中的多数将无法表示。
- 五个未立项表层缺口（`s3`–`s7`）是既有实现尚未回答的语言问题。Greenfield 可有意识地一次作答，而不继承当前偶然行为。

## 6. greenfield gate（2026-08-26）

本目录在 greenfield 各路径上受门禁约束：`crates/almide-wasm/tests/gauntlet.rs` 经 spine-front → emit → host 重放每个单元，并在 `crates/almide-wasm/tests/golden/gauntlet-manifest.txt` 固定各判定（`run` 使用逐字节精确的 stdout hash，`wall` 使用拒绝原因，`reject` 使用 front 的首行）。任何漂移，无论退步还是改进，在正式裁定前都为 red：

```sh
ALMIDE_UPDATE_GAUNTLET=1 cargo test --release -p almide-wasm --test gauntlet
```

`RUN_FLOOR`（只增不减、burn-up 风格）使降级一个可运行单元需要修改两处。`run.sh` 仍是既有实现侧的比较 runner。门禁首日抓住两个真实 greenfield 缺陷：mono clone 丢失 `mutated_params`（c3，静默错误值），以及 `io.print` 完全没有 lowering；两者当天修复并固定。
