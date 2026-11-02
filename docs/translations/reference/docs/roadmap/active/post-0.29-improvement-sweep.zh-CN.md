<a id="post-029-improvement-sweep"></a>

# 0.29 之后的改进盘点

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](post-0.29-improvement-sweep.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](post-0.29-improvement-sweep.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](post-0.29-improvement-sweep.zh-CN.md)

> 非规范、固定原文的历史读者快照。原路径仍为权威来源，为其读取方保持不变。不自动同步，也未独立执行。已采纳提案、完成勾选及剩余工作均保留原文时点状态，不是新核实的当前实现状态。 这是0.29.0之后的台账，不是当前 issue 跟踪。保留原有混合状态：#785 未勾选，但 #782 称其完成；ceangal 标题为3/6，后续行却为4/6。已勾选行也可能仍写全量 gate 运行中。发布、CI 和 gate 指示是历史文本，不是在此执行的行动。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/post-0.29-improvement-sweep.md)

> 原 metadata 摘要（非活动读者注记）：0.29.0之后的改进盘点——所有未解决缺口建成 issue 并排序，从展示修复到 v0 wasm 退役。

0.29.0（verified-default 化）交付后的全方位盘点。将所有改进候选建成 issue，按优先级逐项处理。各项细节与验收标准以 issue 为准；本文是顺序及当前位置台账。

<a id="すぐ直す顔の修繕衛生"></a>

## 立即修复（展示与卫生）

- [x] [#778](https://github.com/almide/almide/issues/778) README Project Status 表过时且自相矛盾（Tests 240/232→285、MSR 23/25 vs 30/30、未写 verified-default）。
- [x] [#779](https://github.com/almide/almide/issues/779) `almide.lock` 的裁定（commit 或 gitignore，在 org 内统一）。
- [x] [#780](https://github.com/almide/almide/issues/780) 将 org-trust-status headline 切换为 resolved-strict sweep——诚实的首次测量：13/28 repos clean、24 modules wall、11 frontend-rejected（#783 类）。porta/almai/toml 的新 real wall 是下一批 lowering 目标。
- [x] [#741](https://github.com/almide/almide/issues/741) 添加 `math.tanh` / `math.atan`——在 native/v0-wasm/v1-self-host 共 3 条路径实现 vendored libm 转写，C-134 + 3000 点差分 fuzz。也发现并修复 `0.0 - t` 丢失 IEEE 负零的陷阱（符号位 bxor）。

<a id="正しさnative-側の非対称を塞ぐ"></a>

## 正确性（消除 native 一侧的不对称）

- [x] [#757](https://github.com/almide/almide/issues/757) nested variant-tag panic——根治：#610 box 改写中的 `matches!` guard 将非 boxed 的 inner tag／字面量抹为 `_`。通过 guard_shape 递归保留所有 refutable 约束。扩展 C-070 + `nested_variant_tag_box.almd`（全量 gate 正在运行）。
- [x] [#753](https://github.com/almide/almide/issues/753) debug-profile ANF trap——在当前 develop 无法复现（0.29 周期的 lowering 修复已解决）。用 debug 二进制验证两个 fixture 后关闭。仅 debug 执行 postcondition 符合设计（已在 pass.rs 文档化）。
- [x] [#783](https://github.com/almide/almide/issues/783) name-pinning 再发——根治：repair 基于 `map_children`，跳过了 ForIn/While body（`Vec<IrStmt>`）内的 Bind ty。改为 canonical `IrMutVisitor`，统一到与 checker 相同的遍历家族（消除整类 enumeration drift）。还为 gate 的 where_ 添加位置粒度。
- [x] [#784](https://github.com/almide/almide/issues/784) 匿名 record 字段的 Unknown——真正原因是**无注解负字面量 module 常量**的 seed 为 Unknown（`infer_literal_type` 缺少 Unary 分支）。修复后 ceangal suite 编译通过（剩余为 #433 类 cell 与个别测试失败）。回归：向 cross_module_let_test 添加 2 项。
- [ ] [#785](https://github.com/almide/almide/issues/785) 调用初始化的 module 常量也有 Unknown leak——refresh 路径（check_decl 的 top_lets 重新注册）未能救到读取端。这里才是核心契约。

<a id="戦略級次の大玉"></a>

## 战略级（下一批大项）

- [ ] [#782](https://github.com/almide/almide/issues/782) **Phase 3：退役 v0 wasm emitter**——前提大幅推进：frontend bug #783/#784/#785 已完成；**wasm:skip stale sweep 完成**（23→15 markers，其中 8 个的 wasm 路径已追上：fn_ref／递归 generics／closure-capture／borrow_hoist／mut params×2／random(C-112)／url_decode）。真实剩余事项：[#786](https://github.com/almide/almide/issues/786) cross-module Unit effect fn 生成无效 wasm、结构化 native 的 7 个 markers、oracle 后继决策。
- [~] [#764](https://github.com/almide/almide/issues/764) native trust-spine——**Float slab 已交付**（真实 f64、3 点 byte 一致、differential 4 rows）。剩余 records（发现 DynList 令设计大幅缩小——重用 ListLit + 1 个 FieldGetScalar op，台账已固定 6 个 emit 位置）/variants/closures → rung 6 → default flip。
- [ ] [#617](https://github.com/almide/almide/issues/617) Matrix/Bytes 的 RcCow 化（保持值语义，消除深拷贝成本）。

<a id="品質基盤コツコツ級"></a>

## 质量基础（持续推进）

- [~] [#566](https://github.com/almide/almide/issues/566) coverage ratchet——**机制已交付**：`proofs/coverage.sh` 中加入 baseline 比较（TOTAL line % 下降即 FAIL，`--update` 仅允许提高），加上 nightly CI job（fuzz-nightly.yml 的 coverage-ratchet）。剩余：初始化 baseline（全量 gate 后运行 coverage.sh）和 MC/DC（flight 范围的 #566 本体）。
- [~] [#781](https://github.com/almide/almide/issues/781) cog>100 burndown——**最差的第 1 项已完成**：将 `list_heap_call_name`（cog 324）拆为 per-module router + 7 个 helpers，通过 wall-list byte 一致验证。剩余 14 项继续按台账从最差开始处理。

<a id="ceangal-suite-burndown783784ufcsrecord-order-で-36-まで前進"></a>

## Ceangal suite burndown（#783→#784→UFCS→record-order 推进至 3/6）

- [x] cross-module UFCS 无法解析 qualified 类型名（`count.get()` E001）——checker/lowering 两侧的 defining-module 推导均以 bare 名为前提做 suffix 扫描。qualified 名改用 `rsplit_once('.')` 直接推导。
- [x] 匿名 record 字段顺序导致 E005——`Ty::compatible` 的 Record×Record 按位置 zip 比较（solver 按名称，形成不对称）。统一为 by-name 集合比较，同时消除 `mix({ g: .., r: .. })` 的单文件缺口。
- [x] [#787](https://github.com/almide/almide/issues/787) ScratchAllocator overflow——通过 minimal tier 8/4/4 解决（two-pass hwm emit 仍作为最终目标）。
- [x] [#788](https://github.com/almide/almide/issues/788) 修复 mono 的 2 层 bug——①自由变量（module global）的 alpha-rename；②qualified/bare Named unify 失败（`cell.get` 未特化——正是已记录的 #433 类 cell.get 问题）。`ALMIDE_MONO_DEBUG` 计量在 tree 内，添加回归 cell。**ceangal 4/6**（从早晨 0/6 提升）。
- diff_bench 的 `view.box` E002 是 ceangal 侧过时 bench（未跟进 view API 改名），不是编译器 bug。

<a id="発信ユーザー手番"></a>

## 对外发布（用户负责）

- 将确定的发布文案（#773 已关闭，全文在 `docs/roadmap/done/claim-wording-perceus-byte-identity.md`）随 0.29.0 公告一同发布。

<a id="関連"></a>

## 相关

- flight ladder 另设：[#586](https://github.com/almide/almide/issues/586)（#776 参考应用／#777 缩小 lowering 信任基底／#569 WCET 已更新）。
- [v1-release-path](../../../../../roadmap/active/v1-release-path.md) / [v1-org-byte-verification](../../../../../roadmap/active/v1-org-byte-verification.md) / [code-health-codopsy](../../../../../roadmap/done/code-health-codopsy.md)
