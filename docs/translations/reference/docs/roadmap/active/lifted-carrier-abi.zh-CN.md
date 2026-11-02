<a id="1437-the-lifted-carriers-abi--declaration-driven-one-readable-layout"></a>

# #1437：提升后 carrier 的 ABI——声明驱动、单一可读布局

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](lifted-carrier-abi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](lifted-carrier-abi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](lifted-carrier-abi.zh-CN.md)

> 非规范固定读者快照。未改变的原文仍为权威来源，不替换机器输入。不自动同步，也未独立执行。区分保留历史测量、提案及原文时点的实现更新，不构成当前正确性或性能的独立证明。代码与注释保持原样。 L1 明确已实现，但 L2、L3 仍待完成。8月 probe 步骤、拒绝数和 gate 提案不得被解读为纯声明 ABI 或默认切换已完成。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/lifted-carrier-abi.md)

> 原 metadata 非活动摘要：#1437——用声明驱动的单一 carrier 布局取代生成端漂移。

<a id="l1-implemented-later-stages-remain-separate"></a>

## L1 已实现；后续阶段仍独立

#1437 的 **L1 仅 probe 下的 wrapping 与 heap-family 接纳已实现**，见 [mod_c_tail.rs](../../../../../../crates/almide-mir/src/lower/mod_c_tail.rs) 和 [binds_p2_bang.rs](../../../../../../crates/almide-mir/src/lower/binds_p2_bang.rs)。下方 L1 步骤和拒绝数量是 8 月调查快照。这不表示 L2 的纯声明 ABI 或 L3 的默认切换已经成立：`bang_return_probe` 和以 probe 为条件的位置脱糖仍存在。后续里程碑应保持待完成，不能把整个工作方向视为完成。

状态：DESIGN（调查于 2026-08-24；方向遵循 issue 中记录的候选——通过构造保证生成端一致）。解除 return-op R2 矩阵中 fn-family 类别的阻碍（40 个 fn 级拒绝，约占剩余 fallback 文件的 30 个）。上级工作：return-op-eradication R2/R3、result-family-from-type Phase 3、rot-eradication R5。

<a id="the-disease-measured-precisely"></a>

## 精确测得的问题

#1437 是两个纠缠的缺陷，调查厘清了“lift 物化 len-as-tag”究竟是什么：

1. **是 hybrid ABI（#841），不是 tag offset 的选择。** 对可返回错误、已提升的 heap-ret fn，`auto_wrap_abi_body` 将根函数体改为 `Result[T, String]`，但 `wrap_return_positions_in_ok` 受 `opt_ret || !has_propagation_site(body)` 控制（mod_c.rs:599）。含 `!` 的函数体保留 RAW ok 尾部，而 err 路径物化真正的 wrapped block。任何读取端都无法区分 raw String block 和 carrier；记录的 crash（fs_fold_lines_range、`0x2f2f`）是把 raw payload 字节当成 tag 读取。probe OFF 时，位置脱糖随后将函数体改写成显式 ok()/err() 分支，掩盖 hybrid；在 R2（脱糖关闭）下，没有 wrapping，因此此类在 `decl_ret_family = None` 处被拒绝（mod_c.rs:552——遏制措施）。
2. **ABI 决策是试探 lowering 的不动点。** `AUTO_WRAP_ABI_FNS` 的成员资格 = 函数体谓词 + 相互递归试探内联（`lowers = |f| lower_function_all_with_types(f, ..).is_ok()`，mod_p2_b.rs:264），与 never-err strips 一同迭代到 snapshot 不动点（pipeline_b.rs:406-447，#485）。接纳改变函数体，函数体改变成员资格：哪些 fn 有 Result ABI，取决于当前 lowering 接纳什么。frontend 自身的 lift 已由签名驱动（`should_lift_effect_fn_ret`、pass_result_propagation.rs:78）；MIR registry 是第二套更狭窄的、按名称索引的判断（rot-eradication R5 的残余类别）。

<a id="the-measured-fact-that-shrinks-the-fix"></a>

## 缩小修复范围的测量事实

一旦每个出口都真正 wrapped，@16 tag slot 就已经对 lifted-heap 类能产生的所有 block 有效：

- err 路径：`materialize_result_err_str` 写入 SUPERSET block（len@4 = 1 且 tag@16 = 1；result_materialize.rs:59-93）。
- heap ok 路径：heap `x` 的 `ok(x)` 路由至 `materialize_result_str` / `_aggregate`，本来就采用 cap-as-tag（Ok 时 @16 = 0）。
- scalar materializer block 按构造将 @16 置零（`Init::ResOkScalar` 的 8 字节 payload store；lib.rs:196）。

因此，对 `result_family` 归为 HeapOk 的 `Result[<heap T>, String]`，**hybrid 一消失**，生成端与类型就按构造在 @16 一致。无需新布局、stdlib reader 变更或生成 drop 变更。（完整 Scalar-family 的 @16 收敛——RESEARCH Stage A、约 70 个文件、14 个 stdlib reader、5 个生成 drop——明确不是本工作所需，仍属独立 roadmap。）

<a id="stages"></a>

## 阶段

<a id="l1--kill-the-hybrid-under-the-probe-lift-the-containment-the-r2-unblocker"></a>

### L1——在 probe 下消除 hybrid，解除遏制（解除 R2 阻碍）

- `auto_wrap_abi_body`：在 `bang_return_probe()` 下无条件 wrap return 位置（曾使跳过合理的位置脱糖在这里不运行）。probe OFF 行为逐字节相同。
- `decl_ret_family` 的 arm 2 扩展至 heap 返回：`Some(result_family(Ty::result(T, String)))`——2026-08-16 实验的一行修改，现在有依据：probe 下每个出口都是 wrapped block，其 @16 读取由上述生成端清单保证正确。（该实验因缺少可区分的复现而失败，正是因为遏制使此类被拒绝；R2 下，可区分程序恰是被拒绝的矩阵文件——如今拒绝，之后运行，三路字节相等。）
- 值位置的 `e!` reader（calls_p4.rs:463-500）硬编码 tag@4 / payload@12：若 probe 下可经 heap callee 到达，则按 family 选择 offset（generic hoist 应先将其移到 bind 位置；先测量，再修复或记录不可达原因）。
- **Gate**：红→绿 probe 家族（可出错的 lifted heap-ret producer + `!` consumer；String / List[String] / record payload；ok 与 err 路径）；census fn-family 收缩（40 → 残余）；`spec/wasm_cross/effect_tco` 在 probe ON 下保持绿（TCO 例外范围——wrapping 改写 if spine 类型后，循环转换仍须触发；若回归，则 wrap 保留尾部自递归例外并写入文档）；coverage ratchet 与 probe OFF battery 不变；heap-cap churn；因果 A/B。

<a id="l2--the-abi-decision-becomes-a-declaration-fact-kills-the-fixpoint"></a>

### L2——ABI 决策成为声明事实（消除不动点）

- 在任何改写之前，从原始声明和语法上的 can-err（compute_can_err 不依赖接纳）只计算一次成员资格；删除 populate→rewrite 不动点（pipeline_b.rs:406-447）及试探 lowering 对成员资格的影响。从相反方向关闭 #485 drift 类：改写不得改变 fn 的 ABI；若 strip 去除最后一个传播 `!`，仍保留 wrapped ABI，wrap 自然物化。
- 保留 never-err raw-T ABI（支撑 yaml/parser TCO 集群）。NEVER_ERR ∧ ¬can-err 本身就是声明级事实。
- 对齐目标：frontend 的 `should_lift_effect_fn_ret`——一个决策、一个位置，在边界纳入签名（原样采用 rot-eradication R5 的解决方案）。
- **Gate**：`ALMIDE_ABI_PROBE` snapshot diff（前后成员资格必须能逐 fn 解释）；全语料三路比较；census；#485/#786 回归 fixture。

<a id="l3--r3-folds-the-probe-default-the-hybrid-dies-everywhere"></a>

### L3——R3 将 probe 设为默认，hybrid 全面消失

当 R2 矩阵为空且 probe 切为默认（return-op R3），L1 wrap 成为唯一路径，`has_propagation_site` 与 hybrid 随位置脱糖一同删除。无需另立工作；在此记录是为避免将 L1 误认为终态。

<a id="non-goals"></a>

## 非目标

- 不改变 Scalar-family 布局（Result[scalar, String] 保持 len-as-tag；其生成端和读取端目前一致）。
- 不改变 stdlib .almd reader，不改变生成 drop（原因见上方 @16 清单）。
- L2+L3 之前不解除 probe OFF 下的遏制：probe OFF 的 hybrid 消费者是位置脱糖，目前由其处理。

<a id="exit-criteria"></a>

## 退出条件

1. R2 census：fn-family 拒绝减至 void-fn 残余；约 30 个文件的类别在 wasm 分支 probe ON 下运行，三路字节相等。
2. `AUTO_WRAP_ABI_FNS` 的填充不读取 lowering verdict（L2）。
3. 将可区分的 fixture 家族按契约提交后关闭 #1437 issue；L1 前编译器拒绝（因果 A/B），之后运行。
