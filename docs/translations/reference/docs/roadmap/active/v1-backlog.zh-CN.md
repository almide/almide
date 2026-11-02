<a id="v1-backlog--trust-spine-の残件一覧優先度付き"></a>

# V1 待办——trust spine 剩余事项（含优先级）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-backlog.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-backlog.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-backlog.zh-CN.md)

> 非规范、固定原文的历史读者快照。原路径仍为权威来源，为其读取方保持不变。不自动同步，也未独立执行。已采纳提案、完成勾选及剩余工作均保留原文时点状态，不是新核实的当前实现状态。 7月4日摘要说 Tier 2 已关闭，但下文仍列出有意推迟的 Tier 2 事项；保留两种记述。“医疗级”和 byte-identity 主张属于原文措辞，不代表医疗认证或独立安全证明。不声称旧 v0-native 阻碍和交接指示仍然有效，也不在此执行指示。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-backlog.md)

> 原 metadata 摘要（非活动读者注记）：v1 trust-spine 待办——健全性、drop 完整性、self-host 表面、拒绝。

> 更新于 2026-07-04。Tier 1（健全性）、Tier 2（drop 完整性）、Tier 3 的轻量 self-host 已关闭。另捕获并修正了 2 个既有健全性 bug（cert 双重 move／guard early-exit 静默错误编译）。剩余重型事项（fast-exp、spec 拒绝分类、依赖 v0-native 的契约）在下方保留，并给出准确恢复点。

<a id="-クローズ済み2026-07-04"></a>

## ✅ 已关闭（2026-07-04）

- **T1-1 mir>ir breach**（wav.almd::find_chunk_at）——原先把 opt-tuple fold 的 body 按各分量 lower 3 次，改为每 iteration 1 次。BUG gate 0。
- **T1-2 anon-record drop 的 `f0` 类型错误**——以结构类型（`List[{…}]`）绑定 List-element drop field。无类型注解的匿名 record 通过 render。
- **T2-3 Matrix record-field 泄漏**——为 `record_drop_field_frees` 添加 Matrix / List[Matrix] 分支（生成 `__drop_matrix` / `__drop_list_matrix`，对行进行 rc_dec 扫描）。
- **T3-5 轻量 self-host**——list.enumerate/zip（scalar + flat-heap rc）、map.entries/from_list（skv repr）、list.get_or_hshare/take_hshare、fs.read_bytes、bytes.read_length_prefixed_strings_le。nn unlinked 位置 29→12。
- **T4-10 variant statement match 的 List-field 提取**——将 ctor pattern 扩展至 List[…] 等所有 heap field，加上 record-ctor pattern（`Data { seq, .. }`）、arm-tail 的 for/while 循环执行，以及 deferred-subject match 的拒绝。也纳入 record-ctor 值的 tagged construction（修正误认为 plain record 的错误编译）。spec 拒绝 6→2。
- **（捕获）cert 双重 move**——在 `let base=…; if c then …+base else base` 形式中，`EndIf` val-move 对 else-arm 已 Dup 的 value 重复计数，导致 ownership REJECT（`iammd`）。runtime 原本就健全（`Consume` 是 move 标记，不做 rc_dec）。cert 侧从 val-move 排除 consumed-value 后修正。T4-10 扩大 in-profile 范围后暴露此问题。
- **（捕获）guard early-exit 静默错误编译**——曾把 `guard cond else E; …` 按 always-continue defer，导致 `!cond` 路径错误（`guard len>0 else err();
  ok(x)` 在空输入下返回 ok）。由于 v1 没有 early-return 控制，改为明确拒绝。parity baseline 回归为零（byte-verified 集合中没有 guard 函数）。

<a id="tier-2-残--drop-完全性"></a>

## Tier 2 剩余——drop 完整性

1. **Option[Matrix]（有意推迟）**。defunc find 有意拒绝 matrix-shaped payload（两层 option drop 尚未完备）。无实际使用。若开放，应先在生成侧加入相当于 `__drop_opt_matrix` 的处理，再移除 gate。
2. （记录）**List[flat variant] ctor field** 在构造侧被拒绝，生成侧也不 emit free——当前一致（从不构建）。开放时两侧同时处理。

<a id="tier-3-残--実行面の拡張重量級"></a>

## Tier 3 剩余——执行表面扩展（重型）

3. **fast-exp 家族 7 个函数（重型、需精度验证）**：`matrix.softmax_rows` / `gelu` / `swiglu_gate` / `multi_head_attention` / `masked_multi_head_attention` / `rope_rotate` / `from_q1_0_bytes`。v0 对 almide-kernel SIMD fast-exp **按 lane 顺序求和**，因此逐位精确转写需要忠实再现 exp_pd_{wasm,neon} 多项式（Horner 6 项）、2-lane 部分和、奇数 tail 的 libm exp（参考实现为 crates/almide-kernel/src/silu.rs::exp_pd_neon，遵循 math_exp.almd 先例）。**因属医疗级工作，不可仓促弄错 ULP**。这是 nn 在 v1 上端到端运行的前提。当前 8 个位置明确拒绝。
4. **list.enumerate_h / zip_h（rich 元素）**：对 `Complex=(Float,Float)` 元素 enumerate，会接入由 filter→map 从 `(Int, tuple)` 提取 `e.1` 的 heap-tuple 管道（fft 的 4 个辅助位置，明确拒绝）。ROI 低，非推断路径。

<a id="tier-4-残--spec-壁バケット既存-roadmap-路線"></a>

## Tier 4 剩余——spec 拒绝分类（现有 roadmap 路线）

5. **guard-else early return 建模**（39 个位置，当前明确拒绝）：向 v1 引入 early-return 控制，将 `guard cond else E; rest` 结构化为 `if cond then rest else E` 即可恢复 in-profile。回收本次从错误编译改成拒绝的部分。
6. `fan.*` capability 系约 32 项 → effectful-27-blueprint。
7. ResultOk/Err/OptionSome 参数物化约 15 项。**注意**：`Result[String,String]` 的 `ok(payload)` tail ctor 很容易，但含 guard 的函数（大多数）须先完成 T4-5 early-return。无 guard 的 if/else 形式已经可以 lower。
8. effectful stdlib（`datetime.parse_iso` / `env.os` / `env.temp_dir` / `fs.stat` / `http.*` / `net.*` / `process.*`）——capability 声明的基础。像 fs.read_bytes 一样，从能通过 admitted-effectful 列表及对应 prim 开通的部分开始。
9. 其他物化约 10 项：EmptyMap/MapLiteral/List 参数、call-arg 位置的 capturing lambda、opaque fn-value 的 HOF。

<a id="tier-5--契約計測"></a>

## Tier 5——契约与测量

10. **matrix.mul / from_bytes OOB 契约台账（因依赖 v0-native 受阻）**：v1 已符合 native 契约（mul 按 k 递增，OOB 为全零矩阵），但由于 `almide run`（native）损坏，matrix fixture 无法通过 output-parity gate 进行 byte-verify（成为 v0fail）。**前提是修复 v0-native matrix codegen（见下方交接）。**
11. **覆盖率**：最近 66.30%。最大未覆盖面为 control_p5（defunc 引擎），其次是 tail.rs / binds_p2 / binds_p4。使用 `bash proofs/coverage.sh` 测量。

<a id="v1-レーン外引き継ぎ事項--ここでは直さない"></a>

## v1 通道之外（交接事项——此处不修复）

- **v0-native matrix codegen 损坏（develop-v1 既有）**：生成 Rust 的 `bridge::AlmideMatrix` 已改为 enum，而 vendored glue（transpose 等）仍使用旧 `Vec<Vec<f64>>` API，产生不一致，使 `almide run` 的 matrix 代码报“codegen produced invalid Rust”。看起来是 flat ABI / burn 重构（并行会话负责区域）的过渡状态，因此**交给负责会话**。v1 matrix parity oracle 暂用 `almide run --target wasm`（byte-identical 保证）。Tier 5-10 契约 fixture 等待此问题解决。
