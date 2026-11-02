描述：夜间差分模糊测试的历史问题分诊，保留发现、修复及已撤回的假设
<a id="fuzz-findings-triage-re-green-the-nightly-differential-gate"></a>

# 模糊测试发现分诊：让夜间差分门禁重新通过

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fuzz-findings-triage.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fuzz-findings-triage.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fuzz-findings-triage.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fuzz-findings-triage.md)

> 原文历史注记：2026年7至8月调查记录，并非当前 CI 状态、运行 campaign 的指令或独立验证的安全保证。保留 fixed/live/closed/剩余项的历史顺序，包括明确撤回的容量阈值和泄漏假说。R3 的早期部分进展与后续关闭并存，不将后文第十二个 cell 未完成调查推断为已解决。程序数、binary revision、有限 churn loop、输出比较及门禁均为原文报告的观察，非新测量或普遍证明。错误的探针成功标准与同一 binary/command 纠正也是证据历史。区分资源上限跳过、honest wall、abort 契约、观测泄漏及错误输出。未执行引用命令、CI 变更或 bug 修复。


<a id="historical-triage-log-not-live-ci-status"></a>

## 历史分诊日志，不是实时 CI 状态

本页保留 7–8 月的测试轮次报告，包括后来被探针明确撤回的假设。开头的夜间失败报告指的是 2026 年 7 月和提交 `25585249`；最初的十二项发现记录为在 7 月 18 日全部消除。后续各波有各自的日期和结果。这些条目都不能说明今天的夜间运行是通过还是失败。实时 CI 状态应查看实际工作流运行；不要把本日志中已被取代的假设重新当成新发现。

Fuzz (nightly) 自 2026-07-15 起一直失败。生成式差分模糊测试器（`tools/xtarget-fuzz`）正在发现真实的 native ⇄ wasm 可观察差异——正是跨目标契约台账声称不会悄然发生的那类问题。每多失败一夜，“字节完全一致”的声明就与现实偏离得更远。

<a id="reproduction"></a>

## 复现

```bash
cargo build --release --bin almide          # the oracle binary
cd tools/xtarget-fuzz && cargo build --release && cd ../..
tools/xtarget-fuzz/target/release/xtarget-fuzz replay --seed 1784352208133210990 --index <i>
tools/xtarget-fuzz/target/release/xtarget-fuzz gen    --seed 1784352208133210990 --index <i>  # source
```

代码注释译文：the oracle binary 指判定器二进制；source 指源代码。

2026-07-18 运行的全部 12 项发现，在当前 develop（25585249）上均可复现——没有一项因 #782 的阻断清理而碰巧修复。

注意：同一工作流的 coverage-ratchet 作业（#566）也在失败，需要单独调查（通过 `gh api /repos/almide/almide/actions/jobs/<id>/logs` 获取作业日志；`gh run view --log` 对此仓库返回空）。

<a id="findings-seed--1784352208133210990"></a>

## 发现（seed = 1784352208133210990）

| 类别 | 索引 | 症状 | 状态 |
|---|---|---|---|
| C. String 函数返回 `""` | 323, 768, 904 | `ok(float.to_fixed(…))` → wasm `ok("")`；heap-Ok Result 上的 `result.map_err` | **已修复（2026-07-18）** — 两个根因：(1) ok/err 构造器中标准库调用的载荷落入延迟 Opaque（binds_p4 Module 调用的 String 分支，C-138）；(2) result 值组合器在以 cap 为标签的 heap-Ok 块上链接了以 len 为标签的标量实现（result_call_name 中的 `_h` 配套版本 + `_x` 阻断，C-139） |
| D. Unicode 谓词反转 | 191 | `none` 与 `some("Ǆ")`（标题大小写） | **已修复** — 与 C 同根因（值流经 Result/Option 构造器载荷） |
| E. i32 边界元组 | 609 | `(true, -2147483648)` 与 `(false, 2)` | **已修复** — 与 C 同根因 |
| A. 负零显示 | 67, 655 | native `-0` / wasm `0` | **已修复（2026-07-18）** — 不是显示问题：v1 自托管的 `float.round` 按 `x >= 0.0` 分支（IEEE 下对 -0.0 为 TRUE），丢失了符号；copysign 保留符号（C-140） |
| G. 构建/运行失败 | 65（wasm 运行失败）, 96（wasm 构建失败） | 失败行为不一致 | **已修复** — 65：`list.zip_with` 为每种实例化都链接 Int 类型实现，String zip 因标量闭包表类型而陷阱 → 按元素表示分流 + `_str` 配套版本（C-141）。96：v0 发射器的 `result.unwrap_or_else` 内联缺少 F64 情况 → 无效模块；已参照 option 配套版本补齐（C-142） |
| F. Option 反转 | 858 | `some("5")` 与 `none` | **已修复（2026-07-18）** — `some/ok/err(<heap if>)` 载荷落入延迟 Opaque，读到 `none`；构造器部件匹配现在将 If/Match 的 String 载荷交给 heap-result-if 机制（C-143） |
| B. List 坍缩为 `[]` | 198, 659 | `[1000000, 7, 256]` → `[]`, `[true,true,false,true]` → `[]` | **已修复** — 四个机制：(1) 非字面量的标量列表绑定现在明确阻断，而非延迟到悄然给出 `[]` 的 Opaque（C-144）；(2) 带单态化后缀的标准库名称（`or_else__Int_String_String`）按基础名称分流（C-145）；(3) 允许在闭包环境中捕获 String-err Result（与 `__drop_list_str` 精确对应的布局族），因此带捕获的 or_else 链可在 v1 验证执行；(4) v0 提升后的闭包返回捕获别名时，现在给出共同所有的 +1 引用（C-146） |

**种子 1784352208133210990：12/12 项发现全部消除（2026-07-18）。**

<a id="wave-2-seed-20260718-1000-program-campaign-88-resolved"></a>

## 第 2 波（种子 20260718，1000 个程序的测试）：8/8 已解决

| 索引 | 类别 | 解决方式 |
|---|---|---|
| 0 | NativeBuildFailure | 生成的运行项目通过空 `[workspace]` 表自我隔离——在任何 cargo 工作区目录（模糊测试器的 .scratch）内运行 almide，都会使 cargo 解析父工作区并拒绝构建 |
| 9 | RunFailureDivergence | `list.unique_by` 按键的表示分流：String 键使用 `_sk` 配套版本（prim 按字节比较内容是否相等），否则用 `_x`（C-147） |
| 198 | 挂起 | 测试框架修复：只有 wasm 正常成功时，native 挂起才算发现；wasm 在 4GB 上限处发生 OOM 陷阱不构成终止证据（`pos + 0` 变异会让两端都挂起）。ladder.rs 中的纯分类器 + 单元测试 |
| 248 | ok("") | 构造器的 Var 载荷改用 Dup 而非移动——`let a = ok(r0); let b = ok(r0)` 两者都是真实值，r0 保持存活（C-150） |
| 259 | WasmBuildFailure | v0 `list.scan` 的累加器存储固定为 i64 → 按 valtype 分为三支；v1 `list.scan_str` 配套版本 + 按累加器表示分流（C-148） |
| 590 | 浮点垃圾值 | v0 result.unwrap_or_else 的 heap-Ok 载荷释放后使用 → 共享 +1（C-149） |
| 647 | some(garbage) | `result.map/map_err/flat_map` 的结果为 heap-Ok（输入为标量）→ 确定性的 `_x` 阻断（C-151） |
| 888 | err→ok(0) | 对未获准的堆调用载荷进行构造时明确阻断（绑定防线扩展，C-152；允许嵌套 Result 的 drop = F2 后续事项） |

顺手修复测试框架/发射器：v0 emit_result_call 获得真正的 `or_else`/`flatten` 分支（命名分派回退在缺少低层运行时函数的管道上触发 ICE；由宿主确定性门禁发现，该门禁现为 262/262 字节完全一致）。walled-real 基线增加 DESIGNED-PROBE 节（result_wall_escalations::main 固定 C-152 的阻断）。v1 扫描基线：PASS 288 / WALL 12（9 个永久项 + 3 个按设计存在的 fixture 探针）/ INVALID·TRAP 0。

<a id="wave-3-seed-20260718-re-campaign-after-the-classifier-fixes-7-unique--1000"></a>

## 第 3 波（分类器修复后以种子 20260718 重测）：1000 个程序中 7 个唯一问题

预期中止分类器的修复（编译失败仍为发现；运行时中止进入三点比较）揭露了旧规则“任何 native 非零 = NativeBuildFailure”所掩盖的中止形式差异：

| 索引 | 形态 | 类别 |
|---|---|---|
| 10 | 语料变异：main 中的 `assert_eq(sql, "hello")` | **已修复（2026-07-18）** — ALS-T18：非测试的 assert 失败现在仅在前端降低一次（if + eprintln + process.exit(1)），让全部四个消费者继承 `Error: assertion failed…` + 退出码 1，操作数只求值一次（C-153，3 个 fixture）。顺手修复：裸 `eprintln` 内建在 v1 未链接，在 v0 触发 ICE；现在注册为自托管实现（fd-2 的 print_str 配套版本），并有共享的参数化 v0 运行时函数 |
| 49 | C-138 fixture 变异 | native 101 与 wasm 134——两端都泄露原始中止形式 |
| 119 | C-062 RawPtr fixture 变异 | native 1 与 wasm 134（陷阱）——unsafe 桥接的越界形式需要裁定 |
| 5 | `int.clamp(4, 3, 1)`（min > max） | **已修复** — ALS-T6 裁定：四个消费者均输出 `Error: clamp requires min <= max` + 退出码 1；float 的 `!(lo <= hi)` 将 NaN 边界也归入同一行（C-154，2 个 fixture） |
| 145 | `or_else(ok(..), (a) => ok(..))` | **已修复** — E025 不可判定槽验证器现在能看见中间调用结果（推断时将每个调用结果类型入队；求解后的检查仅对确实未固定的槽触发）。两目标现在都在检查阶段拒绝，带位置范围和提示；恢复接受一致性，模糊测试器分类为 GeneratorReject |
| 149 | `Result[Option[Float], ..]` 上的 `ok(result.unwrap_or(.., none))` | **已修复** — option/result `unwrap_or` 的堆分支在两个分支都给出共同所有的 (+1) 引用（保留的载荷 + Var 默认值），即 #727 共享族的 unwrap_or 版本（扩展 C-149，新增 fixture） |
| 98/92 | C-002 Int8 溢出变异（`neg_one_i8(128)`） | **已修复** — E024 的调用参数版本：流入固定位宽参数的裸整数字面量在检查时做范围验证（上下文记录钩子现在会在固定位宽上下文中为 i64 范围内字面量创建站点，并接入调用参数推断点）。check 现在拒绝 native rustc 原本会拒绝的输入，弥合检查与构建的落差；模糊测试器分类为 GeneratorReject |

还顺手修复了 **coverage-ratchet 作业失败**（另一个 #795 作业）：原因是 `find -perm +111`——GNU find 拒绝的 BSD 语法；现改用可移植的 `/111`（proofs/coverage.sh）。第 3 波尾声（针对 batch-5 二进制测试 1000 个程序）：索引 92（检查与构建差异）、995（JSON nbsp/emsp 键的显示差异，新形态）、998（native 退出 1 与 wasm 陷阱 134，中止形式泄露）。Interp 新增 Flow::Exit（process.exit），使 assert 脱糖能在三方判定器中求值。

第 3 波与第 1–2 波（实例降低错误）的主线不同：主要是中止形式规范化（ALS-T6 承诺 `Error: …` + 退出码 1 的位置，泄露原始 101 panic 和 134 trap），以及接受一致性（未解类型变量进入代码生成）。clamp/RawPtr 边界需要先在 ALS 中做规范裁定，再修复。

持续测试直到无发现的状态：第 3 波分诊尚未结束。剩余完成条件：1000 次运行无发现 + coverage-ratchet 作业诊断 + 连续两晚夜间测试通过。

反馈给 #777/F3 的教训：C 类两个根因都是“延迟/错误链接的值未被阻断便进入可观察输出”。(1) 延迟 Opaque 构造器载荷打印为 `ok("")`；(2) 按名称索引的注册表链接忽略了类型所蕴含的布局。F3 门禁应将两者都结构化：流入显示/相等/可观察操作的 Opaque 必须阻断该函数；自托管链接必须携带表示兼容性检查（机械执行 `_h`/`_x` 后缀规约）。

<a id="wave-4-2026-07-2830-nightly-findings--the-first-post-revival-campaigns-unit-042"></a>

## 第 4 波（2026-07-28..30 夜间发现——恢复后的首轮测试，Unit 0.42）

0.41 的检测工具恢复让测试每晚都能完成；以下是这些已完成夜间运行记录的发现。2026-07-31 在 develop e610331d 上重放并分类（macOS；检查/构建阶段的判定与宿主无关）。

| # | 夜间 / 种子 | 类型 | 症状 | 状态 |
|---|---|---|---|---|
| 861 | 7/28, 1785217538023450905 | OutputDivergence | native `r15 = 100`，wasm `r15 = 0` — `result.unwrap_or_else(err, closure returning captured Float)` | **已解决 — 已归因（B2）**：`020021df`（“通过读取 is_heap_ty 让闭包捕获任意标量，使 Float 和每种固定位宽整数不再阻断提升”）。该发现的闭包 `(s14) => r12` 捕获 FLOAT；提交前提升被阻断，HOF 在缺失闭包的情况下运行，产生全零结果（wasm `r15 = 0`）；提交后捕获可提升，两目标都读到 100。机制完全吻合，撤回此前对 7021f11f 的怀疑 |
| 535 | 7/28，同一种子 | NativeBuildFailure | check 接受 `let m: Int8 = --9223372036854775808`，随后 rustc 拒绝 | **已解决 — 已归因（B2）**：`6ac44503`（“穿过完整括号与一元负号链找到整数字面量，使窄类型注解能检查范围，并由净符号决定”）。C-173（18f96604）已在 7/28 当晚的编译器中，但无法看穿双重一元负号；带净符号的链遍历恰好让 E024 拒绝 `--9223372036854775808`。机制完全吻合 |
| 57 | 7/29, 1785304212462799529 | RunFailureDivergence | C-044 fixture 变异：`factorial(n - -2147483648)` — native 终止（LLVM 的累加器尾递归消除将约 40 亿层的非尾递归变成循环，回绕至 ≤ 1 后展开）；wasm 忠实递归，在约 16k 帧时以 `call stack exhausted` 陷阱终止 | **已纳入契约（2026-07-31，M2 #1017 选项 1）** — C-196：调用栈耗尽是可观察行为承诺之外的资源限制。收敛边界 fixture `recursion_depth_within_limits.almd`（深度 1000 时字节一致）；判定器 `one_sided_stack_exhaustion` 规则（单元测试 ×4）仿照双端规则将此类归为跳过；重放现在显示 SKIPPED，并指出 C-196。深度保护规范化（两目标在相同深度给出相同 T6 中止）已记录为强化后续项 |
| 12 | 7/29，同一种子 | WasmBuildFailure | 阻断信息指出最外层症状（“带调用分支的 Bool If”）；最小化（m1–m9）证明根因在别处：嵌套 map 字面量 `Map[String, Map[String, Bool]]` 从未实化。msv 路径（`map_heap_val_nested_route`）只允许内部 `Map[String, String]`，而且理由充分：`$__drop_map_msv` 把内部全部 2m 个槽当 String 清扫，因此内部标量值 map 会被当成垃圾句柄释放（接受但不安全）。其上的 if 条件链随后一路拒绝，直到语句阻断 | **已修复（2026-07-31）** — 设计好的 msb 扩展已落地：`map_msv.almd` 中 `__drop_map_msb` / `__drop_list_str_msb` 的键清扫 drop；两个 drop 注册点上的 `is_map_msb_ty` 类型分流 + pairs 字面量 `StrMapSkv` 分类分支；msv from_list/get_or 门禁扩展至内部标量值 map（实现对句柄通用，只有 drop 按类型区分）。重放无问题；反复分配释放的 spec 测试在 wasm 端执行 drop。已知且诚实的残留：外层嵌套 map 的 `len`/`contains`/`set` 仍阻断（本发现不需要） |
| 29 | 7/30, 1785389912282950207 | WasmBuildFailure | 未链接的标准库调用 `map.get_or_str_wall` — String 值的 `map.get_or` 版本不在自托管注册表中 | **已修复（2026-07-31）** — `stdlib/map_str.almd` 中增加 `map_get_or_str` 配套版本（命中分支遵守 str 族的复制规约深复制，未命中分支按 hval 分支形状返回默认值）、注册表条目，并在 `mod_p4_e.rs` 的 `_str` 分流列表中允许 `get_or`；重放无问题，新增 spec 测试 |

<a id="wave-4-additions-the-loop-until-dry-tail"></a>

### 第 4 波补充（持续测试直到无发现的尾声）

第 2 轮本地测试（2026-07-31，1,283 个程序 / 1,093 个无问题 / 165 条原始发现事件去重为 2 个唯一类别）：

| # | 种子 | 类型 | 症状 | 状态 |
|---|---|---|---|---|
| L2 | 1785460667454423000 idx 5 | WasmBuildFailure | 阻断消息（“无法解析的条件上的 if（Bool 调用）”）是误报——#904“分支拒绝被命名为条件问题”陷阱的第二例。差分调试（p1–p8、g5a–g5f）证明存在阈值相互作用：完整程序阻断，但每一种单独简化都通过——移除 r6、简化 r1 的嵌套 if 上 unwrap 链、用 r1 替换 r4 别名、缩短 5 元素列表，或把 1e-300 次正规默认值替换为 0.5。没有单个构造是罪魁；某种降低预算/相互作用（怀疑是被省略调用/caps 类，即 #848 路线）只在组合形态上拒绝 | **已修复（2026-07-31）** — 两个新检测工具（条件侧跟踪、被丢弃模块调用原因跟踪）标明拒绝链，根因竟是缺失的匹配分支：`str_list_literal_elems_lowerable` 没有 `If` 元素分支，所以 `["a", (if r2 then "b" else "d")]` 落入 `_ => false` → 整个字面量拒绝 → Opaque → 诚实拒绝。“阈值”谜团其实是常量折叠：每个简化版的 `if` 都在优化时折叠，只有完整程序能走到缺失分支。修复：在预检查和元素构建器中都允许 String 结果字面量分支的 `if` 元素（cond = 已跟踪标量 Var 或 Bool 字面量，这是已证明的 `try_lower_heap_result_if` 总能降低的形态，遵守构建中途不得拒绝的保护）。重放无问题；wasm 端的 `spec/lang/list_literal_if_element_test.almd` 使用运行时条件，避免折叠掩盖路径；mir 601/0 |
| L3 | 1785460667454423000 idx 54 | 相对参考解释器的 OutputDivergence | C-182 fixture 变异（Float32/Float64 负数字面量的上下文类型）：native 与 wasm 完全一致（7 行，`-2.5 … 123456792.0 -1.5`），但两者都与 almide-interp 不同——要么是共享降低错误（第三裁判正是为检测此类而存在），要么是 interp 的 Float32 模型缺口。重放不打印 interp 的预期输出，模糊测试报告有缺口，需要顺手修复 | **已修复（2026-07-31）** — 错的是解释器裁判：`let p: Float32 = 123456789.12345679` 保留了 f64 表示，因为 interp 从不在 Float32 字面量诞生时缩窄它（加宽载体约定在 float32.* 桥接处缩窄，但字面量从不经过它）。两个后端都正确折叠到 f32（`123456792.0`）。在 eval.rs 的字面量求值中修复（`ty == Float32` 时用 `as f32 as f64`）；顺手修复报告缺口：发现摘要现在指出首个不同的行（`first_line_diff`，单元测试 ×2）；在 C-182 下新增 fixture `float32_literal_excess_precision.almd`，让三方门禁永久覆盖此类。重放无问题，interp 测试 56/0 |

| # | 来源 / 种子 | 类型 | 症状 | 状态 |
|---|---|---|---|---|
| L0 | 2026-07-31 本地 B4 测试（707 个程序，607 个无问题），种子 1785458401504935000 索引 0 | WasmBuildFailure | 阻断：“带无法提升/闭包列表高阶参数的 map.fold 在本实现块中无法忠实执行（阻断，而非错误值）”。最小化（t3）：使用堆 map 累加器、闭包返回 map 字面量的 `map.fold` | **已修复（2026-07-31）** — 关键洞察：Option[String] 的 `Some(s)` 就是 1 元素 DynListStr（`materialize_opt_str_some`），所以 Option[String] 值与 skv 内层 map 遵守相同的“len@4 计数的 String 槽”规约，msb drop 可原样适用。修复纯粹是扩宽类型门禁：`is_map_msb_ty`、`StrMapSkv` pairs 分类器和两条 msv 路径现在允许内层 `Option[String]`。fold 无需改动，`map.fold_skv_hacc`（skv 对象上的任意堆累加器）早已存在；字面量本体一旦可降低，闭包就能提升。重放无问题；wasm 端 spec 测试覆盖 get_or some/none/miss、fold-into-mso-acc、反复分配释放。已知残留单元：mso `get_or` 结果上的 `??` 缺少读取形态播种（诚实回退，已记录，L0 不需要） |
| L1 | 最小化 L0 时发现（探针 t1） | 检查器漏洞（两目标均 ICE） | `map.fold` 对标量累加器的闭包参数检查不足：类型错误的本体（因 (k,v,acc) 顺序错误形成 `Int + String`）通过 `almide check`，却死于 IR 验证。list.fold 和具体堆累加器的 map.fold 都能正确拒绝 | **已提交问题** — [#1018](https://github.com/almide/almide/issues/1018)，分配给 0.52 漏洞搜寻行（显式报错而非静默；不构成夜间失败风险，因为生成器只生成类型正确的闭包） |

第 3 轮测试（2026-07-31，1,304 个程序 / 1,108 个无问题 / 163 条原始事件 → 1 个唯一问题）：

| # | 种子 | 类型 | 症状 | 状态 |
|---|---|---|---|---|
| L4 | 1785464182375979000 idx 0 | WasmBuildFailure | `Map[String, Result[Int, String]]` 值（get_or / remove）流入 `result.unwrap_or` — STRICT 模式的“值子集之外的标量绑定”拒绝 | **已修复（2026-07-31）** — 记录的编辑列表原样落地：族内层集合集中为 `is_msv_family_inner`（内层 String 键 map、Option[String]、T 为标量或 String 的 Result[T,String]；即所有“len@4 计数的低 32 位 String 句柄”块，在 `$rc_dec (param i32)` 包装下标签安全），三条路径（get_or / remove / from_list）统一使用它，扩宽 `is_map_msb_ty` + pairs 分类器，并注册新的 `map_remove_msv`（遵循 set_copy 规约，但跳过一个槽）。从问题扫描台账再次确认一个额外陷阱：使用 rc_inc 的辅助函数还必须加入 `coown_names.rs` 白名单，否则渲染为未链接。重放无问题；wasm 端 spec 测试覆盖 ok/err get_or、remove、反复分配释放；mir 601/0 |

第 4 轮测试（2026-07-31，1,271 个程序 / 1,072 个无问题 / 3 个唯一问题）：

| # | 种子 1785466162321453000 | 类型 | 症状 | 状态 |
|---|---|---|---|---|
| L5 | idx 985 | RunFailureDivergence | C-045 变异：`for i in 0..<4294967295 { list.push(ys, i*2) }`（约 34 GB 的 push）。native 在 64 位地址空间中完成；wasm32 撞上线性内存上限，恰在内存大小边界（0x80010000）以越界内存错误终止——分配器增长失败后仍越过末端运行，而非干净中止 | **已修复 + 已纳入契约（2026-07-31，M2 #1019 选项 1 完成）** — 三层：(1) `memory.grow` 被拒绝时触发 `$oom`；(2) 第 2 层根因是 **i32 分配边界溢出**：单次约 2.1 GB 请求使 `$bump = p + n` 越过 2^32 回绕，回绕后的 bump 跳过增长检查，块写入恰在边界越过内存末端（观察到的 0x80010000 错误）。已在 `$alloc`/`$alloc8` 中加入无符号回绕保护（`new < old → $oom`），并在 `$list_new` 中加入 cap×8 乘法回绕保护；探针现在输出规定的 `Error: out of memory` + 退出码 1；(3) C-197 落地（197 active / 0 flagged，收敛 fixture `allocation_within_limits.almd` 字节一致），判定器新增 `one_sided_memory_exhaustion` 跳过规则（单元测试 ×3，fuzzer 26/0），对应 C-196 的栈规则 |
| L6 | idx 33 | WasmBuildFailure | 同一“List 参数无法忠实实化”类别——获准集合之外的下一个列表元素单元（元素类型待从源码确定） | **仍在处理 — 已分类**：`List[Option[Map[String, Bool]]]` 字面量（`[some(["k0": true, …]), some(n1), …]`），链接到以 Option-map 为默认值的 `list.get_or`。组合破坏了“len 计数的 String 槽”规约：`some(map)` 块的槽 0 是 map 句柄，因此平坦 rc_dec 会在最后引用时泄漏 map 的键；drop 需要按载荷分流。已有基础：`materialize_opt_aggregate_some` + `optrec:<drop_fn>` 路径（`Op::DropWrapperRec`）已对 `Option[record]` 做同样的事。修复应是 Option-of-map 配套分支（载荷 drop_fn = msb/skv map 清扫）+ 允许列表元素 + 该元素类型上的 `list.get_or`。实现设计（q1/q2 探针：阻断位于 binds_p2_b.rs 的 `try_lower_bind_heap_fresh_scalar_list` 中显式嵌套所有权门禁）：(1) map_msv.almd 中静态三层 drop — `__drop_list_omb` / `__drop_list_omb_loop`：逐元素检查，若 option 块（1 槽 DynListStr）是最后引用且 `len@4 == 1`，则将槽 0 的 map 载荷交给 `__drop_msb_inner`，再 rc_dec 该块；(2) 构造：新增 `ListElemDrop::OptMapSkv` 分类分支（Tuple→Option[Map[String,scalar]] 元素），以及经 opt-aggregate 块实化 `some(<map literal|tracked var>)` 的元素降低（map 句柄移入/Dup 到槽 0），将 `none` 实化为长度 0 的块，注册 drop `"list_omb"`；(3) 允许该元素类型的 `list.get_or`（heapelem hshare 路径）；(4) 已格式化的 spec 测试，包括反复分配释放。细化（第 2 遍）：`List[Option[String]]` / `List[Option[Int]]` 字面量早已能构建，机制是 CtorFlat/CtorLenLoop 路径，由 `lenlist_elem_class`（repr_sources_d.rs）控制，只允许“精确一层”载荷（标量 → Flat；String / 平坦标量列表 → 经共享生成的 `$__drop_list_lenlist` 使用 LenLoop）。map 载荷正因需要内部清扫而被排除；msb 洞察可以组合：len-loop 规约是递归的（option 块 len 槽 → map len 槽 → String），静态 `__drop_list_omb` 正是表达这一点。不要原地扩宽 `lenlist_elem_class`/`$__drop_list_lenlist`（会改变已有用户共享的 drop）；添加独立的 `OptMapSkv` 类 → drop `"list_omb"`，在 binds_p3_b 分类器约第 243 行的 lenlist 分支之前决定。元素载荷实化（some(map-literal)）必须将 map 经 from_list 机制处理；验证 `try_lower_option_ctor` 的聚合载荷部件能到达该路径，否则扩展部件。**已修复（2026-07-31，第 4 遍）** — 实现块比设计更小，因为两个部件已存在：map 字面量脱糖为 `map.from_list` 调用，且 opt 构造器的计算 map 部件分支（`is_str_int_map_ty` = 任意堆键/标量值 map）已能实化它们（q6 探针）。落地内容：(1) 静态三层 `__drop_list_omb`（列表槽 → option len 槽 → msb 键清扫，len-loop 规约可组合）；(2) 在 lenlist 分支前决定 `ListElemDrop::OptMapSkv`，以 drop `"list_omb"` 进入构造器元素路径；(3) 发现并修复新的链接陷阱：静态 drop 来自 DropVariant 操作，按需链接器的 CallFn 扫描看不见；程序若对 map_msv.almd 的唯一需求是 drop，就会留下悬空 `$__drop_list_omb`。现在只要存在任意 msv 族静态 drop 操作，链接器就强制纳入模块（参照 value_core 先例）。i33 重放无问题；wasm 端 spec 测试覆盖字面量 + get_or + 反复分配释放；mir 601/0。[#1020](https://github.com/almide/almide/issues/1020)（独立 `let o = some(map_var)` 绑定的暴露问题）当天被证伪并关闭：C-197 的 OOM 中止让泄漏可观察，三个反复分配释放变体——包括假设最坏的 drop 顺序（`keep = some(n1)`，option 比 map 变量活得更久）——使用 32 字节键运行 1.5–2M 次迭代，从未中止。`some(<live heap var>)` **会 Dup**，所以 option 共同拥有 +1，无论哪方先 drop，平坦 rc_dec 都能平衡。记录的教训：读代码提出的假设不等于发现；提交问题前先测量，今天正是自己的检测工具作出了裁决 | 

2026-07-31 夜间运行（run 30608326062，05:59 UTC 的 head，包含直到 B2 归因为止的全部修复）：测试完成（完整预算连续记录 4/14），记录了 2 项发现 —
| # | 种子 1785477905242041784 | 类型 | 状态 |
|---|---|---|---|
| N1 | idx 93 | NativeBuildFailure：`literal out of range for i32` | **已修复（2026-07-31）** — 现在在调用参数循环中，将已声明的元组变体载荷类型固定到字面量构造器参数上（构造器调用不携带 call_sig，因此此前没有钩子看见该位置）。回归测试固定了一个陷阱：大写开头的构造器被调用者解析为 TypeName，而非 Ident；首个修复只匹配 Ident，悄然从未触发。两个再现例现在都给出 E024；范围内载荷仍接受；frontend + literal-domain（8/8）+ spec/lang（159）通过 |
| N2 | idx 9 | WasmBuildFailure（又是条件阻断误报形态） | **已修复（2026-07-31）** — 先前“规约不成立”的判断错误：`try_lower_tuple_construct` 以 `DynList {{ len = slot count }}` 构建元组，堆槽存储为句柄，因此全 String 元组原样符合 len 计数规约（len=2，两个槽都是 String 句柄），`__drop_msb_inner` 可准确释放。N2 化简为纯门禁扩宽：`is_msv_family_inner` + `is_map_msb_ty` + pairs 分类器允许全 String 元组值。重放无问题，两目标探针一致，wasm 端 spec 测试覆盖 get_or 命中/未命中 + 反复分配释放，mir 601/0。**第 4 波待处理数：0** |

策略说明（第 1–4 轮）：每轮约 1,300 个程序都会揭露 1–3 个更深类型组合的新单元（map 值 → option 值 → result 值 → option-of-map 元素）。手工列举的族/drop 注册表每项发现增加一格；机械化终态是“按构造保证完整性”的路线（由类型推导 drop 分流）。在此之前，逐项推进本就是设计的一部分，基于连续通过记录的完成标准将其纳入。
| L7 | idx 757 | NativeBuildFailure | 检查器接受了 rustc 拒绝的 Float32 字面量：`error: literal out of range for f32` — 535 整数字面量定义域类的浮点对应问题（C-173 只覆盖整数） | **已修复（2026-07-31）** — E024 的浮点版本：`FloatOverflowSite` 队列（预筛为 f64 有限但 f32 无穷的字面量）+ 带注解绑定的上下文固定（`float_literal_chain` 穿过括号/一元运算，参照整数钩子；裸字面量自身的求解类型仍是 `Float`，所以 Float32 上下文位于绑定处）+ 求解后验证器。重放现在以 E024 给出 GENERATOR REJECT；回归测试 `float32_range_is_checked_and_precision_is_not`（正负两种越界均拒绝，超额精度及普通 Float 接受）；frontend + literal-domain + spec/lang 全通过。颇有意思的来源：模糊测试器通过变异本次会话新增的 C-182 fixture 找到了它 |

第 5 轮测试（2026-07-31，1,385 个程序 / 1,198 个无问题，已包含第 4 波及夜间运行的全部修复）：2 个唯一问题，均为新类别（没有已修单元退行）。

| # | 种子 1785481464003870000 | 类型 | 症状 | 状态 |
|---|---|---|---|---|
| P1 | idx 961 | **OutputDivergence — 静默错误值类别** | `bytes.read_f32_le(verts, dst + 9223372036854775807)`（i64::MAX 偏移使下标运算溢出）：native 读到 `z=0.0`，wasm 读到 `z=2.0`——即越界字节读取没有按构造保证跨目标相等。两项中优先级最高：`bytes.read_*` 的边界规约是内存安全接口；native 返回 0.0 时 wasm 返回相邻值，意味着一端未检查读取，或检查方式不同 | **已修复（2026-07-31）** — 无需契约决策：文档规定的“越界 → +0.0”已经是规范，只是两端都未在溢出偏移处落实。根因是保护 `pos + k > n`：当 `pos = i64::MAX` 时，加法回绕为负，检查通过，继续读取（wasm 返回相邻值；native 的地址运算碰巧落到零）。普通越界偏移始终正确，只有溢出区间泄漏。遵守 API 完整性规则，修复整个族：`bytes_core.almd` 中全部 **14** 处改为防溢出的 `pos > n - k`（长度非负且 ≤ 2^31，因此 `n - k` 不可能溢出）。重放无问题；回归测试 `bytes reads are +0.0 at an overflowing offset` 覆盖 f32/f64/u32/u16/i32，以及不变的范围内和普通越界答案；spec/stdlib 112 个文件通过 |
| P2 | idx 27 | WasmBuildFailure（标量元素列表字面量，元素在子集之外） | 元素为 `map.get_or(list.get_or([<map literals>], 1, ["k0": true]), "ΑΒΓ", false)`，即送入 `list.get_or` 的 `List[Map[String, Bool]]` 字面量。map 值矩阵已很广，但元素为 map 的列表是下一个组合边界（L6 形态向外移一层：option-of-map → 裸 map 元素） | **已修复（2026-07-31）** — 裸 map 列表元素类别已落地：`ListElemDrop::MapSkv`（`binds_p3_b.rs` 中的分类分支，位于 lenlist 分支前），`stdlib/map_msv.almd` 中静态两层 `$__drop_list_mb`，按需链接器 `MSV_STATIC_DROPS` 中的 `list_mb`，以及三个 spec 测试（字面量/索引/get_or、反复分配释放循环、计算元素）。p27 和所有 b3–b15 探针现在都能构建，且 native ⇄ wasm 一致；spec/lang 159 + spec/stdlib 112 文件通过，almide-mir 601 个单元测试通过。下方保留调查历史，因为走错的路耗费了一整个会话。**我的第一次分类错误，已由探针纠正**：这不是缺失的 map 元素单元。七个探针（w1–w7）全部能构建，包括看似完全相同的部件——绑定的 `List[Map[String,Bool]]` 字面量、参数位置的内联字面量、以 `map.map` 高阶函数结果为元素，以及键来自 fold 生成 String 的 map——因此没有缺失任何单一构造。拒绝链真实存在且已跟踪（`[defunc] fold route declined (rolled back)` → `[scalar-call] map.get_or declined: "List argument cannot be faithfully materialized"`），意味着 fold 的脱函数化回滚污染了后续列表参数实化，是类似 L2 的状态/阈值相互作用，而非类型单元缺口。二分定位（b1–b5）发现阻断需要三个条件同时成立，去掉任意一个都能构建：(a) `list.fold` 生成的 String（b2：内联为字面量 → 可构建）；(b) 内层列表中的 `map.map` 高阶函数元素（b4：换成普通 map 字面量 → 可构建）；(c) 内层列表有**四个**元素（b3：两个 → 可构建；b5：三个 → 可构建）。带高阶函数元素且上游有 fold 时，3→4 元素的断崖是预算/状态特征，不是类型单元缺口；与 L2 同类，但阈值可计数，因此是本会话最清晰的线索。纠正（b6）：断崖不是元素数——4 个元素、后两个各为 1 键 map 时可构建。按内层列表键值对总数排序：b3 = 4 对 ✅，b6 = 6 ✅，b5 = 8 ✅，p27 = 12 ❌。所以阈值是 8 到 12 对之间的大小预算（随总条目数增长的操作数 / 值 ID / 指令数），只有 fold 和高阶函数元素也贡献时才到达；因此出现了此前的三条件解读。已夹定（b7 = 10 对 ✅，b8 = 11 ✅，p27 = 12 ❌）：断崖恰在第 12 项，只差一个键。共享键集假设已测试并证伪（b9：两个元素键集相同、仅 6 对时可正常构建），因此是数量，而非驻留化。每个探针都符合：当 fold 和高阶函数元素也参与时，列表参数实化路径在 11→12 个 map 条目附近存在硬容量限制。剩余工作是机械性的，需要新鲜上下文：给路径加检测（对比 b8 与 p27 的操作数/值 ID 数，它们只差一个键，差异只有少量操作，跨越阈值的计数器应很明显），然后提高或移除限制，并以旧边界 2× 的 spec 测试固定。给接手者的注意：这是容量阻断，因此是诚实阻断（没有错误字节），损失的是覆盖率，不是正确性。**再次纠正——这次探针集可在同一个二进制上复现（2026-07-31）。** 上面夹定中的每一个“可构建”都无效：在同一个二进制上用 `almide build <f> --target wasm` 重跑 b3/b5/b6/b7/b8/b9/p27，全部阻断，包括 b3（2 个元素，4 对）。不存在断崖、大小预算或第 12 项阈值；那些读数来自未实际执行 wasm 路径的成功判据，本不该记录为证据。教训在流程：只有一次运行中，每个单元都用同一二进制和同一命令测量，夹定才算证据。
缩减为三行再现例，恢复了最初分类：`let xs: List[Map[String, Bool]] = [["k0": true]]` 后接任何使用，都会以 `non-empty List[heap] literal with nested-ownership elements` 阻断。同一位置的 `List[Int]` 字面量可以构建（m9 对照）。所以 P2 正是最初看起来的样子：**缺失单元——元素为 map 的列表字面量**，是已关闭的 L6 option-of-map 情况的裸 map 对应项。不是容量阻断，也不是 fold/高阶函数相互作用。修复设计（已完成，待实现；L6 option-of-map 路径是精确模板，少一层间接）：
(1) `stdlib/map_msv.almd`：在 `__drop_list_omb_loop` 旁加入两层 drop。`__drop_msb_inner(ih)` 已能清扫 map 的 String 键并 rc_dec map 块，所以循环体只需一个调用，无需 omb 的 option 块 len 槽处理：
`fn __drop_list_mb_loop(h, n, i) = if i >= n then () else { __drop_msb_inner(prim.load64(h + 12 + i * 8)); __drop_list_mb_loop(h, n, i + 1) }` 和 `fn __drop_list_mb(xs: List[Map[String, Bool]]) = { let h = prim.handle(xs); if prim.load32(h + 0) == 1 then __drop_list_mb_loop(h, prim.load32(h + 4), 0) else (); prim.rc_dec(h) }`。
(2) `crates/almide-mir/src/lower/binds_p3.rs`：`ListElemDrop::MapSkv`。
(3) `binds_p3_b.rs`：`Map[String, Bool|Int|Float]` 的分类分支，与 `OptMapSkv` 分支放在一起，并位于 `lenlist_elem_class` 调用之前，理由相同（lenlist 对 map 载荷返回 None，元素会落入嵌套所有权阻断）；在 drop 句柄匹配中注册 `list_mb`。
(4) `pipeline_c.rs`：把 `"list_mb"` 加入 `MSV_STATIC_DROPS`，让按需链接器强制纳入 `map_msv`。
(5) 在 `spec/stdlib/` 中现有“option-map 列表”测试旁添加 spec 测试；若此形态具有跨目标可观察性，再加 `spec/wasm_cross` fixture。
元素实化应已可用：`OptMapSkv` 今天已能在 option 块内构建 map 字面量，裸 map 就是去掉包装的版本；编写 drop 前用 m5 再现例验证。仍然成立的已记录死路：先将列表绑定到变量也无法绕过（b12），它会提前一条消息在字面量本身阻断，反而是更干净的再现例 |

<a id="definition-of-done"></a>

## 完成标准

1. 每项发现都最小化（`gen` → delta）、定位根因，并修复（按台账规则附 `spec/wasm_cross/` fixture + 契约条目），或转为诚实阻断（绝不悄然给出错误值）。
2. 结构性补上 C 类阻断覆盖漏洞，将机制反馈至 #777 跟踪集合/阻断一致性门禁设计。
3. coverage-ratchet 作业失败已诊断并修复（或按 #566 规约，在独立提交中重新论证下限）。
4. 本地 `xtarget-fuzz run --count 1000` 测试无发现，然后 Fuzz (nightly) 连续两晚通过。

<a id="ownership-boundary"></a>

## 职责边界

模糊测试器本身（生成器、判定阶梯、差分调试器）位于 `tools/xtarget-fuzz`，不是本工作流的对象；这里只处理它的发现。分诊时发现的模糊测试器错误（例如判定误分类）顺手修复，并附独立测试。

<a id="wave-5--round-6-local-campaign-2026-07-31"></a>

## 第 5 波 — 第 6 轮本地测试（2026-07-31）

在修复 P2 的二进制上运行 3,990 个程序 / 30 分钟 / `--jobs 6`（0.41.0 安装，整个运行使用同一个二进制）。**2 个唯一发现**，因此未满足 B4 的“0 发现”标准。

| # | 种子 / 索引 | 类型 | 形态 | 状态 |
|---|---|---|---|---|
| R1 | 1785489842024900000 / 5 | WasmBuildFailure（诚实阻断） | `List[String]` 字面量的元素是 `if`，而其条件是调用 | **已修复** — 准入谓词比它保护的构建器更严格。`try_lower_heap_result_if` 早已能实化一般条件（纯标量，或带堆参数、返回 Bool/Int 的纯调用；在每条件独立帧内释放），无法处理时整体回滚；谓词却重述了更窄的规则（“cond 必须是已跟踪 Var 或 Bool 字面量”），阻断了构建器本可处理的形态。修复方式：对分支递归应用谓词，把条件留给构建器处理；即 `binds_p3`/`binds_b.rs` 中的 `str_list_literal_elem_lowerable`。3 个 spec 测试 |
| R2 | 1785489842024900000 / 3132 | **仅 native 挂起** | `matrix.masked_multi_head_attention(q, k, v, n_heads = -2147483648)`。native 挂起，wasm 打印形状并以 0 退出 | **已修复** — C-198：整个头数族采用同一定义域规则，两目标均按规定中止，并有 4 测试矩阵门禁。见下文 |

<a id="r2-root-cause--one-function-two-divergences-and-a-family-that-guards-inconsistently"></a>

### R2 根因 — 一个函数，两种差异，以及保护不一致的函数族

`almide_rt_matrix_mha_core`（runtime/rs/src/matrix.rs:480）以 `let n_heads = n_heads as usize;` 开头。对 `n_heads = i64::from(i32::MIN)`，这会变成 18446744071562067968，`n_heads == 0` 的提前退出不触发，`for h in 0..n_heads` 运行约 1.8e19 次迭代，造成挂起。

自托管的 `__mha_impl`（stdlib/matrix_activations.almd:285）完全没有头数保护：`let dh = dm / n_heads`。因此同一函数出现两种差异：

| `n_heads` | native | wasm |
|---|---|---|
| `0` | 返回空矩阵（显式提前退出） | **除零陷阱** |
| `< 0` | `as usize` → 约 1.8e19 次迭代循环 → **挂起** | 约零次迭代 → 返回全零 (sq, d) 矩阵 |

两者都不是孤立缺陷。`almide_rt_matrix_rms_norm_heads`（matrix_p2.rs:450）已经写了 `n_heads.max(1) as usize`。同样的危险，一个族成员有保护，另一个没有，这正是 CLAUDE.md 的 API 族规则要防止的逐点生长。

**函数族**：所有接收头数的 `matrix` 函数——`multi_head_attention`、`masked_multi_head_attention`、`rope_rotate`、`rope_rotate_at`、`rope_rotate_neox_at`、`rms_norm_heads`，以及接收 `n_q_heads` / `n_kv_heads` 的 GQA 入口。
**完整性规则**：头数 `< 1` 对每个成员都在定义域外，每个成员在两目标上都以同样方式报告。现有三种行为（挂起 / 陷阱 / 静默截断为 1）统一为一种。门禁是函数族 × {i32::MIN, -1, 0} 的矩阵测试，防止下一个成员无保护地发布。

**第 5 波两项发现当天全部关闭。** 共同教训正是 CLAUDE.md 的 API 族规则所说的：R2 对同一个问题给了三个逐点答案；R1 将保护条件重述得比被保护对象更窄。两者都不是缺少功能，而是逐点长出来的接口表面。

<a id="wave-6--round-7-local-campaign-2026-07-31"></a>

## 第 6 波 — 第 7 轮本地测试（2026-07-31）

在修复第 5 波的二进制上运行 3,424 个程序 / 25 分钟 / `--jobs 6`。**1 个唯一发现**（比第 6 轮的 2 个减少），因此仍未满足 B4 的“0 发现”标准。

| # | 种子 / 索引 | 类型 | 状态 |
|---|---|---|---|
| R3 | 1785492509375906000 / 17 | WasmBuildFailure — `OptionSome argument cannot be faithfully materialized (a heap payload outside the executable subset)` | **已关闭（2026-08-01）** — 全部 4 个单元可构建，native ⇄ wasm 一致；C-201 + `spec/wasm_cross/option_tuple_payload_matrix.almd` 是矩阵门禁。native 渲染残留 → #1037 |

<a id="r3--the-tuple-payload-family-is-keyed-three-different-ways"></a>

### R3 — 元组载荷族使用三种不同索引方式

最小再现例（2 行）：

```almide
let s0: Option[(String, Int)] = none
let r: (String, Int) = option.unwrap_or(some(option.unwrap_or(s0, ("a", 1))), ("b", 2))
```

载荷是调用结果，而非元组字面量。一次运行、同一个二进制的实测扫描：

| `Some` 载荷类型 | 准入键（`binds_p4_b.rs::try_lower_opt_tuple_and_variant_payloads`） | 调用结果载荷 |
|---|---|---|
| `(Int, String)` | `_ if Self::is_int_str_tuple(&expr.ty)` — **仅类型** | ✅ 可构建 |
| `(String, Int)` | `IrExprKind::Tuple { .. } if is_str_int_tuple(…)` — **形态 + 类型** | ❌ 阻断 |
| `(String, String)` | `IrExprKind::Tuple { .. } if is_str_str_tuple(…)` | ❌ 阻断 |
| `(Int, Int)`（全标量） | `IrExprKind::Tuple { .. } if is_all_scalar_tuple(…)` | ❌ 阻断 |

四个单元中一个按类型索引，三个按形态索引。元组字面量载荷在四者都能工作（探针 s1）；先将调用结果绑定到 `let` 也全部能工作（探针 s8），只有内联调用结果载荷让它们分裂。这是今天同类问题的第三例，此前是 R2（一个头数问题的三个答案）和 R1（准入谓词重述得比受保护构建器更窄）。

<a id="fix-design-and-why-it-was-not-implemented-in-the-same-pass"></a>

### 修复设计，以及为何没有同一遍实现

不对称存在于构建器，而不只是匹配分支：

* `try_opt_int_str_tuple_payload` → `self.lower_owned_heap_field(expr)`：从任意表达式实化一个拥有所有权的堆值，因此该单元按类型索引。
* `try_opt_str_int_tuple_payload` / `_str_str_` / `_scalar_`：解构 `IrExprKind::Tuple { elements }`，调用 `try_lower_tuple_construct(&elements)` / `try_lower_scalar_tuple_construct(&elements)`，因此按构造只支持字面量。

所以修复是让三个按形态索引的构建器拥有 int_str 早已有的相同回退：载荷不是字面量 `Tuple` 时，经过 `lower_owned_heap_field`，再使用现有 `materialize_opt_str_some` + 现有 `variant_drop_handles` 条目（`opt_str_int` / `opt_str_str`）。

**需要单独一遍处理的风险**：每个单元注册的 drop 句柄假设了部件形态，而 `lower_owned_heap_field` 可能产生不同于 `try_lower_tuple_construct` 的形态。若不同，回退会造成二重释放或泄漏，属于内存安全缺陷，不是阻断。验证需对照 `opt_str_int` / `opt_str_str` 的 drop 本体阅读两种部件构造，再运行能暴露引用计数错误的反复分配释放测试。这是细致工作，因此刻意没有在漫长会话的末尾启动；以上诊断已足够完整，后续实现是谨慎的机械工作，而非探索。

探针文件位于会话草稿区 `s_s1`..`s_s11`；最有区分力的三例是 s9（`(Int, String)` 调用结果 → 可构建）、s5（`(String, Int)` → 阻断）、s11（`(Int, Int)` → 阻断）。

<a id="r3-outcome-2026-07-31"></a>

### R3 结果（2026-07-31）

此前拖住工作的 drop 规约风险**没有发生，原因可以核查**：`materialize_opt_int_str_some` 与 `materialize_opt_str_some` 除了将结果登记在哪个所有权集合外，逐字节完全相同。两者都用 `Op::Consume` + `live_heap_handles.retain(…)` 接收部件，因此部件的构造路径不会影响释放方式，drop 由载荷类型选择，属于布局属性。这让回退是安全的，而不仅是看似合理。

三个按形态索引的单元现在都改按类型索引，并采用 `(Int, String)` 单元一直拥有的 `lower_owned_heap_field` 回退。一次运行、同一个二进制验证：s5 `(String, Int)`、s6（lambda 形式）、s9 `(Int, String)`、s10 `(String, String)` 均可构建，且 native ⇄ wasm 一致。312 个 spec 文件通过（lang 159 / stdlib 112 / integration 41），另有 5 个新测试，包括反复分配释放循环；若所有权推理有误，它正能暴露引用计数错误。

**明确命名的残留——全标量单元。** `(Int, Int)`（探针 s11）仍阻断，因为 `lower_owned_heap_field` 不会实化全标量元组调用结果。其元组分支覆盖堆对（`is_flat_heap_pair_ty`、`is_str_closure_pair_ty`、`is_flat_heap_scalar_pair_ty`），全标量元组一个也不匹配。因此该族是 3/4 对称，而非 4/4；写明此行是让最后一个单元成为有名的缺口，而不是重新发现。补齐它意味着教 `lower_owned_heap_field` 处理全标量元组调用结果，而非再次修改 Option 单元。**进一步缩小（2026-07-31）**：元组字面量已处理——`owned_heap_field.rs` 的 `IrExprKind::Record { .. } | IrExprKind::Tuple { .. }` 分支经 `try_lower_scalar_tuple_construct` 允许仅含标量的聚合。返回 `(Int, Int)` 的调用走的是完全不同的分支（聚合组的 `CallTarget::Module` → `lower_pure_module_value_call`，`calls.rs:89`），并在那里拒绝。所以修复位于调用路径，而非元组路径：让 `lower_pure_module_value_call`（或它上面的分支）实化全标量聚合结果。**再次缩小（2026-07-31），比“调用路径”更小**：用户函数调用结果对两种元组形态都有效；探针 `s_s12`（`fn pair() -> (Int, Int)`）和 `s_s13`（`fn pair() -> (String, Int)`）都能通过 `some(pair())` 构建。因此全标量元组单元并非普遍损坏，回退已覆盖它。

仍阻断的特定形态是 `some(option.unwrap_or(s0, (1, 2)))`，即模块调用产生的全标量元组（探针 `s_s11`）。同一个模块调用产生 `(String, Int)` 却能构建（探针 `s_s5`），尽管两种形态都通过同一回退到达 `lower_owned_heap_field`。两者只在选择它们的分支上不同：`is_all_scalar_tuple`（先检查，接收 `(Int, Int)`）与 `is_str_int_tuple`。因此剩余问题是为何相同回退在一个分支下成功、另一个却失败。可能是 `repr_of(Option[(Int,Int)])`，或全标量元组结果不像带 String 的结果那样在 `live_heap_handles` 中追踪，导致 `materialize_opt_str_some` 的 `Consume` + `retain` 处理不同。

下一步：在 `lower_owned_heap_field` 返回处，对比 `s_s5` 与 `s_s11` 两条路径；输入仅元组元素类型不同，一条返回 `Some(id)`，另一条返回 `None`。两个探针均为两行，位于会话草稿区。

**原始 R3 程序现在晚一步阻断**，报 `List argument cannot be faithfully
materialized (would borrow an empty deferred heap value)`，即空列表上的 `list.fold(list.sort(tmp3), …)`。仍是诚实阻断（退出码 1，无输出），不是错误字节；是不同单元，不是退行。

<a id="dogfood-findings-unit-046-2026-07-31"></a>

## 自用发现（Unit 0.46，2026-07-31）

不是模糊测试发现，而是编写真实程序（`tools/almide-gates`）时遇到的；记录在此，因为当前缺陷台账位于这里。

| # | 形态 | 严重性 |
|---|---|---|
| [#1029](https://github.com/almide/almide/issues/1029) | 在 `for` 循环体中调用 `effect fn` 不会自动解包：native 输出无效 Rust，wasm 把堆地址当作值打印 | **已通过 #1030 修复** |
| [#1030](https://github.com/almide/almide/issues/1030) | **#1029 的根因** — 列表连接不约束右操作数的元素类型 | **已修复** — `infer_plus_op_concat` 现在使用 `constrain` 而非 `unify_infer` |

**#1030 是迄今发现最小的“检查器接受、降低时重新解释”类实例**：无 effect、无并发、无泛型，只有三行。对比也十分精确：`[1, "a"]`（字面量）被 E001 正确拒绝，而 `[1] + ["a"]`（连接）不会。同一属性，一条路径检查，另一条不检查。

**这说明了什么覆盖问题。** 真实程序的 180 行发现了 324 个 spec 文件和七轮模糊测试未发现的问题。spec 语料由了解编译器的人编写；为完成任务而写的程序会使用没人想到测试的形态。一天内出现三个此类实例（#1027、#1029、#1030），全靠偶然而非检测器发现，这正是阶梯第 0.52 行漏洞搜寻视角（#912）的理由。

<a id="1030-fix-2026-07-31"></a>

### #1030 修复（2026-07-31）

在 `check/infer_statements.rs::infer_plus_op_concat` 中改了一个调用：

```rust
- self.unify_infer(le, re);
+ self.constrain(le.clone(), re.clone(), "list concatenation element");
```

`unify_infer` 绑定推断变量，当两边都是具体但不同的类型时保持沉默。`constrain` 仍会统一，因此该规则原本针对的推断变量情况（`List[?0] + List[Int]`）不受影响；但无法统一时也会报告。

两个再现例现在在两目标上都于检查阶段报错：

- `[1] + ["a"]` → `error[E001]: type mismatch in list concatenation element: expected Int but got String`
- `out = out + [one(p)]`（#1029）→ `… expected Int but got Result[Int, String]`

第二项值得注意：连接变得诚实后，#1029 的消息准确指出了问题——effect 调用是 `Result`，列表需要 `Int`——从而引导用户使用 `!`，而非面对“Almide bug” panic。一个修复，两份报告，诊断甚至比我单独针对 #1029 会写出的更好。

回归：335 个文件通过（spec 324 + examples 11），诊断测试框架通过，另有 `tests/diagnostics/e001-concat-element-mismatch`。

<a id="r3-remainder--one-hypothesis-refuted-2026-08-01"></a>

### R3 剩余问题——一个假设被反驳（2026-08-01）

已测试并排除：**“全 scalar tuple 不是 heap，因此走不同路径。”**
它是 heap。`crates/almide-types/src/types/heap.rs:62` 无条件将 `Ty::Tuple(..)` 分类为 heap。
match 没有 wildcard 且穷尽，113行的单元测试将 `Ty::Tuple(vec![Ty::Int, Ty::Bool])` 固定在 heap 一侧。

因此，在 OptionSome 载荷路径中，区分 `(Int, Int)` 与 `(String, Int)` 的不是是否为 heap。
两者经同一 fallback 到达 `lower_owned_heap_field`，且均为 heap；不同的只是选择它们的分支
（优先检查的 `is_all_scalar_tuple`，与 `is_str_int_tuple`）。

不删除这个猜测，而记录其被反驳，是因为这是最自然的第一猜测，否则别人会再花同样二十分钟。
剩下要插桩的是：`lower_pure_module_value_call`（`calls.rs:89`）的哪个提前 return 会对
`option.unwrap_or(s0, (1, 2))` 触发，却不对 `option.unwrap_or(s0, ("a", 1))` 触发。
两个 probe 是会话 scratchpad 中的 `s_s11` 和 `s_s5`，各两行。

**第二个假设也排除**：`repr_of`（`lower/mod.rs`）不拒绝 tuple；提前 `Err` 只针对 `Ty::Unknown`，
下方 scalar 宽度表是分类而非门禁。因此拒绝也不在 `repr_of`。

**有意在这里停止追查。** 连续排除三个假设而未关闭单元，说明下一步需要插桩
（打印各 probe 在 `lower_pure_module_value_call` 走的分支），而不是继续阅读。
继续猜测正是本轮前面 P2 bracket 最终撤回的原因；这里也值得付出遵守这项纪律的成本。

不存疑的事实：这是诚实的 wall（exit 1、无输出、无错误字节），损失的是覆盖而非正确性。
区分它的两个两行 probe 是会话 scratchpad 的 `s_s5`（构建）和 `s_s11`（wall 拒绝）。

<a id="r3-remainder--the-decline-is-localised-to-one-call-with-a-named-next-hypothesis-2026-08-01"></a>

### R3 剩余问题——拒绝定位到一个调用，并明确下个假设（2026-08-01）

经阅读追踪、两个 probe 确认：

- `s_s5` `Option[(String, Int)]` → `try_opt_str_int_tuple_payload` → `lower_owned_heap_field(expr)` → **成功**
- `s_s11` `Option[(Int, Int)]` → `try_opt_scalar_tuple_payload` → `lower_owned_heap_field(expr)` → **失败**

两个分支却以同一表达式形状作同一调用
（`option.unwrap_or(s0, <tuple literal>)`，一个 `CallTarget::Module`）。唯一区别是结果类型。
因此拒绝位于 `lower_pure_module_value_call`（`calls.rs:89`）内部，依赖类型。

**接下来测试的假设**（比“插桩提前返回”更具体）：stdlib 的 `option.unwrap_or` 按载荷类型路由或单态化，
有携带 String 的 tuple 单元，却没有全 scalar 单元。检查 `self_host_registry.rs` /
`stdlib_info.rs` 中 `option.unwrap_or` 特化如何注册，以及全 scalar tuple 实例化是否存在。

如果如此，修复是 registry 项而非 lowering 变更，比早先假设所暗示的所有权分析变更小且安全得多。

<a id="r3-remainder--a-wrong-root-cause-retracted-2026-08-01"></a>

### R3 剩余问题——错误根因，现已撤回（2026-08-01）

上述假设正确，缺口明确。`unwrap_or` 按载荷类型路由，两模块的单元集合不对称：

| payload | `result` | `option` |
|---|---|---|
| String | `result.str_unwrap_or` | `option.unwrap_or_str` |
| Value | `result.value_unwrap_or` | `option.value_unwrap_or` |
| List[Value] | `result.list_value_unwrap_or` | `option.listvalue_unwrap_or` |
| List[Int] / List[String] | （经 flat） | `option.listint_unwrap_or` / `option.liststr_unwrap_or` |
| **flat scalar block**（全 scalar TUPLE、`List[<scalar>]`、`Bytes`） | **`result.flat_unwrap_or`** | **— 缺失 —** |
| heap fallback | `result.unwrap_or_hx` | `option.unwrap_or_hx` |

`Option[(Int, Int)]` 不匹配任何单元，于是路由返回 `None`，调用方 wall 拒绝。
`Option[(String, Int)]` 不是全 scalar，走 heap 路径并构建。这就是 probe `s_s11` 与 `s_s5` 的全部区别。

**这与本轮其他发现同类**：一个族逐点增长。
`result.zip` 的 `(Int, Int)` 需要时，`result` 获得 flat 单元；直到 fuzz 程序提出需求之前，
没人需要，因此没人添加 `option` 的对应单元。

**具体修复：**

1. 在 `stdlib/value_core.almd` 添加 `option_flat_unwrap_or`，以 `result_flat_unwrap_or`（705行）为模型，
   但使用 OPTION layout。Option 是 len-as-tag，不是 Result 的 cap-as-tag-at-@16，所以不能复制本体：
   读取 len slot，`0` = none 时返回 fallback，否则加载 payload handle，让裸 Var 尾部自动获取。
   同文件相邻的 `?? d`-over-`Option[List[String]]` helper 是更接近的模板，注释甚至写着“同一个 len-as-tag prim”。
2. 在 `self_host_registry.rs` 中的 `("result_flat_unwrap_or",
   "result.flat_unwrap_or")` 旁注册。
3. 向 `unwrap_or_call_name_option`（`lower/mod_p4_f.rs`）添加路由分支，照应 `result` 分支的谓词
   （`Ty::Tuple(ts)` 全 scalar，或非 heap 元素的 `List`）。
4. **同一 PR 落地矩阵门禁**——族规则是 `option` 与 `result` 拥有相同的 payload 单元。
   断言单元集合相等的测试才能阻止下次分歧。这个缺口存在，正是因为从未检查这一点。

Probe：会话 scratchpad 的 `s_s11`（wall）和 `s_s5`（构建），各两行。

**撤回——以上错误。** 我 grep 输出名称（`Some("option.*"` 对比 `Some("result.*"`）比较模块，
得出 `option` 没有 flat 单元的结论。但它有：`unwrap_or_call_name_option_flat_scalar`（`lower/mod_p4_f.rs`）
匹配 `Ty::Tuple(ts) if all non-heap`，并路由至 `option.listint_unwrap_or`。
option 一侧复用此名称覆盖整个 flat 族，而不是另起 `option.flat_unwrap_or`。
因此按名称比较会看见并不存在的缺口。

**读名称而非谓词，产生了自信的错误答案**——与已撤回 P2 bracket 同样的失败模式，只是换了伪装。
能抓住它的规则是：比较两个族时，比较它们匹配什么，而不是叫什么。

这一结论导致的变更（`stdlib/value_core.almd` 的 `option_flat_unwrap_or` 及其注册）已回退。
回退本身错了两次，值得记录：删除逻辑向前跨过注释块，把相邻的 `option_liststr_unwrap_or` 一并删掉；
同时 `almide fmt` 已重排整个1100行文件，diff 不再单独显示我的编辑。
从 HEAD 恢复文件才是正确做法；对照 HEAD 的函数清单差异抓住了丢失的邻居，重新运行 `spec/stdlib`
又发现清单检查会漏掉的第二症状（native fallback 为43，正常为6）。

**所以 R3 原因仍未知。** 已排除 heap 分类（`Ty::Tuple` 确实为 heap）、`repr_of`（只拒绝 `Ty::Unknown`），
以及路由单元缺失（单元存在，谓词覆盖此例）。仍成立的是：`s_s5` 构建、`s_s11` wall，
两者仅元素类型不同，且经同一调用到达 `lower_owned_heap_field`。
下一步仍是插桩，打印实际分支，而不是继续阅读。

<a id="r3--narrowed-again-the-decline-is-before-the-call-not-inside-it-2026-08-01"></a>

### R3——再次缩小：拒绝在调用之前，而非内部（2026-08-01）

读完 `lower_pure_module_value_call`（`calls.rs:227+`）：在 `repr_of` 后选择名称，
推入 `Op::CallFn`，返回 `Ok`。尾部没有依类型失败；对合法 `option.unwrap_or`，两种载荷类型都成功。

所以此前定位（“拒绝在 `lower_pure_module_value_call` 内，且依类型”）后半错误。
分歧不在调用中，**其中一个根本不到达它。**

这指向 `lower_owned_heap_field` 的 group dispatch。其 `_leaf` group 返回
`Option<Option<ValueId>>`，两种否定不同且有文档：
`None` =“不是我的 group，试下一个”；`Some(None)` =“是我的 group，但拒绝此 field”。
若 `_leaf` 分支匹配全 scalar tuple 并返回 `Some(None)`，router 就此停止，
不再尝试包含 `CallTarget::Module` 分支的 `_aggregate`。

**下个 probe**（便宜且可定论）：检查 `owned_heap_field.rs` 的 `_leaf` 是否有分支可匹配类型为全 scalar tuple 的 `Call` 表达式。
若存在且拒绝，那就是 bug，修复在分支 guard，而完全不在调用路径。

已排除：heap 分类、`repr_of`、路由单元缺失（已撤回，单元存在），现在又排除
`lower_pure_module_value_call` 内依类型失败。每次排除都记录而非删除，这是能逐渐缩小而非绕圈的唯一原因。

<a id="r3--fifth-hypothesis-eliminated-and-a-note-on-when-to-stop-reading-2026-08-01"></a>

### R3——第五个假设排除，以及何时停止阅读（2026-08-01）

`_leaf` group 完全没有 `Call` 分支。其分支是 `LitStr`、`BinOp::ConcatStr`、
`BinOp::ConcatList`、`StringInterp`、`Lambda`、`Var`、`Member`、`TupleIndex`。
因此 `Call` 表达式无论载荷类型都穿过它到 `_aggregate`（那里有 `CallTarget::Module` 分支）。
“`_leaf` 分支拒绝全 scalar tuple”的假设错误。

**通过阅读排除五个假设，没有一个确认。** 此比例就是信号：阅读已不再划算，
每次继续猜测都有撤回风险。本轮已经发生过“缺少路由单元”的撤回，回退时还损坏了 stdlib 文件。

**下次会话应插桩而非阅读。** 具体：在 `lower_owned_heap_field` 和 `try_opt_scalar_tuple_payload`
各 `return`/`?` 位置加临时 `eprintln!`，构建，运行 `s_s5` 与 `s_s11`，比较 trace，再移除打印。
两次构建加一个 diff，一轮就能指出五遍细读未能找到的位置。

已排除且记录，避免别人重复：
1. 全 scalar tuple 不是 heap → 是（`heap.rs:62`，有单元测试）
2. `repr_of` 拒绝它 → 只拒绝 `Ty::Unknown`
3. `option` 缺 flat 路由单元 → 存在，谓词覆盖此例（已撤回）
4. `lower_pure_module_value_call` 依类型失败 → 尾部总到达 `Op::CallFn`
5. `_leaf` 分支拒绝 Call → `_leaf` 没有 `Call` 分支

始终不变：`s_s5` 构建，`s_s11` wall，仅元素类型不同。
wall 诚实（exit 1、无输出、无错误字节），损失覆盖而非正确性。

<a id="r3--instrumented-and-the-answer-arrived-in-one-pass-2026-08-01"></a>

### R3——插桩后，一轮便得到答案（2026-08-01）

五次阅读无结果，两次带临时 `eprintln!` 的构建立即找到。
同时记录答案与方法，因为方法才可迁移。

**表达式不是 `Call`，而是 `Match`。**

```
[R3] NO group claimed it
[R3] scalar_tuple: lower_owned_heap_field -> false for kind
     Match { subject: IrExpr { kind: Var { id: VarId(0) },
     ty: Applied(Option, [Tuple([Int, Int …
```

`option.unwrap_or(s0, (1, 2))` 在到达 `lower_owned_heap_field` 前脱糖为 `match`。
**五个已排除假设全都针对 `Call` 路径——此表达式从不走的路径。**
细读不断错过，是因为推理正确而前提错误，只有 trace 能展示这个前提。

**实际拒绝点。** `owned_heap_field.rs:476` 确实有 `Match` 分支：

```rust
IrExprKind::Match { subject, arms } => {
    let if_expr = self.desugar_match_to_if(subject, arms, &expr.ty)?;
    let IrExprKind::If { cond, then, else_ } = &if_expr.kind else { return Some(None) };
    let obj = self.try_lower_heap_result_if(cond, then, else_, &expr.ty)?;
    …
}
```

所以拒绝在 `desugar_match_to_if` 或 `try_lower_heap_result_if` 中。
`(String, Int)` probe 到同一分支10次且构建成功，故分支能工作，是这两个函数之一拒绝全 scalar tuple。
最可能候选：脱糖后的 `else` 分支为 tuple 字面量 `(1, 2)`，`try_lower_heap_result_if` 必须将其具体化为分支值。
（不是 `try_lower_heap_result_if` 自身的 `is_heap_ty(result_ty)` guard——`Ty::Tuple` 是 heap。）

**下一步**：同样 trace 再向下一层，在 `try_lower_heap_result_if` / `lower_heap_result_if_inner`
各 `?`/`return` 内打印，运行 `s_s11`，读取拒绝的分支。一次构建即可。

**方法一般化。** 五遍细读比两次插桩构建更贵，还错了。
信号是比例——排除五个，确认零个。值得保留的规则是：*排除多于确认且毫无收敛时，前提大概错了；只有 trace 能指出是哪一个前提。*

<a id="r3--closed-and-the-trace-beat-five-readings-by-two-builds-2026-08-01"></a>

### R3——已关闭，trace 用两次构建胜过五遍阅读（2026-08-01）

**一句话原因。** `lower_owned_heap_field` 的 `Match` 分支只懂字面量分支链，
因此直接拒绝 `option.unwrap_or` 脱糖得到的 `some`/`none` 配对。

上一条的猜测是错的，值得保留可见：它把 `try_lower_heap_result_if` 列为最可能拒绝者。
在该调用处打印 `inner -> {is_some}` 的 trace，整个编译一次也未打印 `false`，此函数从不拒绝。
拒绝在前一步 `desugar_match_to_if`，第二个 trace 打印到达的形状：

```
[R3m] subj_ty=Applied(Option, [Tuple([Int, Int])]) arms=2
[R3m]   pat=Some { inner: Bind { var: VarId(2), ty: Tuple([Int, Int]) } } guard=false
[R3m]   pat=None                                                        guard=false
```

`desugar_match_to_if` 处理 Bool 双分支形式，否则落至 `build_match_chain`，
后者需要 int 字面量分支加 catch-all。variant 模式配对两者都不符合。

**修复不是新机制，而是其他六处已有的调用。**
`try_lower_variant_value_match` 正是“将以 Option/Result 为对象的 match 降低为值”；
拒绝时回滚自身 `ops`/`lifted`/`live_heap_handles` mark，
`binds_p2`、`calls_p4`、`control_p3`、`tail_b`、`heap_result_ctrl_arms` 都先尝试它。
`owned_heap_field` 是唯一直接跳到字面量脱糖的 match-lowering 位置。
在现有分支之前加一分支，以 `is_variant_ty(&subject.ty)` 为 gate。

**这个族现在是矩阵，不是样本。** 同一 binary、同一次运行测得的四种元素类型组合
——`(Int, Int)`、`(String, Int)`、`(Int, String)`、`(String, String)`——都构建并在 native ⇄ wasm 一致。
`spec/wasm_cross/option_tuple_payload_matrix.almd`（C-201）是可执行门禁，
因此未来按元素类型重新分割此族的改动会在这里失败，而不用等 fuzzer 再发现。

**实际测量而非略过的剩余问题。** 修复后，NATIVE verified render 对此族以
`ownership verification failed` wall 拒绝并 fallback 到标准 codegen。
这像是修复破坏了东西，于是将新分支放在临时 env guard 后，在同一 binary 上 A/B：

| program | 无该分支 | 有该分支 |
|---|---|---|
| 将 `option.unwrap_or(s0, (1,2))` 绑定至 `let`（从不到达该分支） | ownership verification failed | ownership verification failed |
| 矩阵 fixture | outside the MIR-lowering subset | ownership verification failed |

第一行足以定论：ownership wall 存在于该分支碰不到的程序。
修复改变的是第二程序的 wall 理由，从“subset”变为“ownership”——两者皆 wall，后者信息严格更多。
已登记 #1037，并在 C-201 陈述内指明，因此契约承诺 stdout+exit 相等，不声称并不存在的 verified native render。

**再次总结方法。** 两次插桩构建找到五遍细读漏掉的东西，第三次 trace 又纠正第二次产生的猜测。
上一条规则——*排除多于确认、毫无收敛时，前提大概错了*——成立。
本轮补充推论：**确认一个假设的 trace，仍值得在行动前指向下一个假设。**
`try_lower_heap_result_if` 是拒绝者的猜测写作“最可能候选”，但完全错误；
返回点一个 `eprintln!` 比产生猜测的阅读便宜。

<a id="a-cheap-rule-the-r3-fixture-bought-2026-08-01"></a>

### R3 fixture 换来的廉价规则（2026-08-01）

新 fixture 通过所有本地门禁——`spec/lang`、`spec/stdlib`、`spec/integration`、`spec/wasm_cross`、
完整 `cargo test`、跨 target 字节比较——却使 CI 的 `Emit & Format` 变红。
因为 `almide test` 不检查格式，而 fmt gate 覆盖 `spec/` 与 `examples/`。

**添加或编辑 `spec/` 下任何内容后，运行 `almide fmt --check spec/ examples/`。**
只需几秒，是测试命令不隐含覆盖的唯一 gate。
（formatter 还移除语句之间空行，因此按段落空行书写的 fixture 一定会重写，最好在 diff 让你吃惊前知道。）

注意范围：gate 仅为 `spec/ examples/`。`tools/` 有意排除；
格式化 dogfood 会把多行签名和管道链压为长单行，因此正确做法是保持不动，而非“修复”。

<a id="q1_0-sign-of-zero-1358-fuzz-nightly-run-31667874852"></a>

## Q1_0 零符号（#1358，fuzz-nightly run 31667874852）

Seed 506685997636 / index 703——`spec/wasm_cross/matrix_select_rows_oob.almd`（C-229）的变异。
全零18-byte block 上的 `matrix.select_rows_q1_0`：native `-0`，wasm `0`。
它们比较相等，因此下游不察觉，这正是其存活过一次发布的原因。

**根因：一种写法，两种答案。** native 逆量化器的负分支写作 `let neg_scale = -scale;`，
self-host wasm decoder 写作 `0.0 - scale`。每个非零 scale 下两者逐位一致，
但 `scale = +0.0` 时不同：`-(+0.0)` 是 `-0.0`，`0.0 - 0.0` 是 `+0.0`。

**裁定：幅值为零的逆量化元素是 `+0.0`。** 量化元素是*幅值*（block 的 fp16 scale）×*方向*
（Q1_0 符号 bit / Q8_0 int8 quant）。幅值为零时 bytes 不编码方向，
IEEE-754 返回的符号取决于算术的写法而非数据。
`+0.0` 也是其余 API 已产生的零（OOB→zeros 行、非整倍列数 guard、`vec![0.0; _]`）。
非零 weight 保持精确符号及位。float decoder `from_bytes_f16_le` / `_f32_le` / `_f64_le`
明确不适用此规则：那里存储的 `-0.0` 就是数据。

**全面排查在同一 decoder 又发现两个分歧，都更严重。** Q1_0 与 Q8_0 共用2-byte fp16 block scale。
探测十个 raw scale 后，native `fp16_bits_to_f32` 与 wasm `__q10_scale` 在三类答案上不同：

| fp16 输入 | native（修复前） | wasm（修复前） |
|---|---|---|
| `0x0000` 全零 block，符号 bit 0 | `-0.0` | `0.0` |
| `0x0001` … `0x03FF`（次正规数） | `5.96e-8` … `6.0975e-5` | **`0.0`——冲为零** |
| `0x7C00` / `0xFC00` / `0x7C01` | `inf` / `-inf` / `NaN` | **`65536` / `-65536` / `65600`** |

次正规数一行是真实 weight 上静默的幅值错误，不只是符号：
wasm 对每个 `exp == 0` 模式只返回符号 bit。`exp == 31` 更糟，wasm 没有 Inf/NaN 分支，落入正规数公式。
让 `__fp16_bits` 忠实对应 native decoder，便关闭了两项问题（以递归正规化尾数、`0xFF` 指数分支）。

**族排查——检查了什么，以及哪些本已正常。** 每个具有 wasm 路径的量化 loader 和 byte loader 的零 block 行为：

| loader | 修复前 | 判定 |
|---|---|---|
| `select_rows_q1_0` | native `-0.0` / wasm `+0.0` | **分歧**——报告的 bug |
| `from_q1_0_bytes` | native `-0.0` / wasm `+0.0` | **分歧**——同根的对应 decoder，未在报告中 |
| `select_rows_q8_0_dq` | 两条路径均为 `-0.0`（zero scale × negative quant；±0 scale × 任一种 quant） | 一致，因此 fuzzer 找不到，但违反同一规则，已修复 |
| `select_rows_f32` | 相同，且存储的 `-0.0` 保留 | 正常 |
| `from_bytes_f32_le` / `_f16_le` / `_f64_le` | 相同，且存储的 `-0.0` 保留 | 正常，且理应如此 |
| `linear_q1_0_row_no_bias` / `linear_q8_0_row_no_bias` | 仅 native（wasm wall 拒绝） | 无跨 target 声明；它们公开点积而非元素 |

`runtime/rs/burn/matrix_burn_p4.rs` 还有使用同样 `-scale` 的第三份 q1_0 decoder，
但该 backend 不在 `[lib]` 中，也没有东西构建它，因此有意不动。

**教训与 gate。** C-229 fixture 已覆盖此输入且通过，
因为它断言幅值（对 list 呈现中两边都读作 `0` 的值使用 `${x}`），而分歧是符号 bit。
声称 bit pattern 的 fixture 必须观察 bit pattern：现在用能打印 `-0.0` 的 `float.to_string` 表示零。
新的 C-269 fixture 经两个 q1_0 入口固定整个 fp16 scale 定义域，使对应 decoder 不再漂移。
A/B 验证：修复前 binary 上两个 fixture 均红。

<a id="1375--one-source-shape-two-independent-wrong-value-mechanisms-2026-08-13"></a>

## #1375——一种源码形状，两个独立的错误值机制（2026-08-13）

`json.parse("hi") ?? json.parse("[1,2]")!` 在 wasm 打印 `true`，在 native 打印 `[1,2]`。
输出 WAT 一读便明确机制：`??` 降低到正确骨架（tag 在 `+16`，payload 在 `+12`，
`tag != 0` 选择 fallback），但 fallback 分支结尾为
`(local.set $v13 (call $json.parse …)) (local.get $v13)`——整个 `Result` block 交给 merge，既不读 tag 也不传播。
heap-arm lowering 应用 `e! == e`，它在 RESULT 类型分支是恒等，在 payload 类型分支却不是。

本条增加的规则：**缩小到机制后，再运行整个邻域，因为同一表面形状可能有不止一个机制。**
把 `a ?? f(..)!` 改写为 `(match a { ok($p) =>
ok($p), err(_) => f(..) })!` 修复了十一个单元；第十二个——同一分支写作字面量 `match`——仍打印 `true`。
输出代码说明原因：该写法到达 `branch_lift`，将分支提升为合成的非 effect `fn … -> <payload>`。
提升 helper 内的 `!` 没有向外通道，因此 native 在非 `Result` fn 发出 `?`（rustc E0277，显式失败）；
wasm 则总是包裹 helper 本体，调用点将 wrapper 绑定为裸 payload（静默错误）。
两个机制、一个形状；第二个可见，是因为第一修复后再测邻域，而非只在开头测量。

也值得保留让修复可执行的 ANF。`(match a { … })!` 会 wall 拒绝，
因为 `desugar_let_unwrap` 构造传播 match，其对象又会是 match。
先绑定（`let $u = match a { … }; let v = $u!`）会在对象位置放入已验证形状 `Var`。
只多一条语句，仍是单次求值，调用次数不变。
提升 fallback 本身才是错误 ANF：`??` 绝不能在 ok 路径求值 fallback，fixture 用未使用 fallback 会失败的单元固定这一点。
