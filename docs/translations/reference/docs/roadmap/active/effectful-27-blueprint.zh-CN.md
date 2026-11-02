历史描述：effect/raw-pointer 蓝图；字节测试的 tier 不是普遍语义证明。
<a id="blueprint--making-the-27-effectful--raw-pointer-stdlib-fns-functional-in-v1"></a>

# 蓝图 — 使约27个 effectful / raw-pointer stdlib 函数在 v1 中达到 FUNCTIONAL

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effectful-27-blueprint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effectful-27-blueprint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](effectful-27-blueprint.zh-CN.md)

> 未改动固定原本的非规范性历史读者快照。不自动同步，未独立重跑测试、证明检查或实现。6月17日蓝图与6月25日附录各自保留 host/API/capability 前提；SAFE-NOW、PROVEN、已验证标签不授权实施，也不宣称当前支持。有限字节 golden 回归测试和指针 round-trip 不是普遍语义或内存安全证明。多声明安全的措辞针对建模 capability 性质，不消除另列的所有权/RC、scratch memory、runner、host、provenance 义务。预测与旧计数保留原地位。原文称评审修正在§7，但列表在§8；保留此引用，不暗中修改。历史构建命令、memory 引用及优先队列不是执行指令或授权。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effectful-27-blueprint.md)

<a id="evidence-terminology-in-this-historical-blueprint"></a>

## 本历史蓝图中的证据术语

6月17日设计使用 **PROVEN** 标记 tier-1 有限 byte-golden 回归测试。
此标签必须读为**在引用案例上做过字节测试**，不是每次执行的数学证明。
建模 capability 定理是另一类证据，保留其所述前提；指针 round-trip 测试也不证明任意内存安全。

下文旧 host 可用性、raw-pointer API 及预计 543/621 计数是历史设计输入，不是当前支持或安全指导。
当前权威及 raw-memory 边界见 [capability 概览](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/capability-system.md) 和
[ADR-0031](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md)。任何历史 SAFE-NOW 标签都不授权恢复已移除的 raw-address API。

仅设计（计划，不是构建）。由 6-agent 设计分工 + 对抗评审产生，依据实树（2026-06-17）。
**判定：SOUND-with-fixes——可安全地以它为基础构建，不可照搬。** §7 评审修正属于规格的一部分。

> **诚实规则（关键）：** 约27项不是一个问题，绝不能用一个“完成”数字表示。
> 三个 tier，三个验证上限。只有 tier-1 可称为“proven”。
> 参见 [[../../../../.claude .../feedback_done_means_three_tiers]]（memory）。

<a id="1-three-mechanisms-each-its-own-ceiling"></a>

## 1. 三种机制（各有上限）

| | 新增 | 离开 sandbox | 上限 |
|---|---|---|---|
| **(A) WASI host-import 基础层** | 新 `PrimKind` render 为 `(call $wasi_fn)`，并在 `preamble()` 新增 `(import "wasi_snapshot_preview1" …)`，仿照唯一已有 `fd_write`（`render_wasm.rs:890`）；effectful 主体随后在其上用 Almide self-host | YES | **declared-caps-safe**（运行 + caps ACCEPT；输出本质上不可证明） |
| **(B) Capability 词汇** | 扩展 `enum Capability`（`lib.rs:471`，当时只有 `Stdout=0`）+ `Capability::id` + Coq ids 至 `{Stdout,Fs,Net,Clock,Random,Env,Process}`；让 `cap_witness`+`declared_caps` 诚实归属 | 不适用（它为 A 记账） | 证明基础 |
| **(C) raw-pointer / mem** | 一个 `repr_of` 分支（`Ty::RawPtr => Word`）；`bytes.*` ptr 函数 = `prim.handle(b)+12`；`$bump` 上非 cap 的 `prim.heap_mark/heap_reset` | NO（linear-mem 地址算术） | **offset-functional**（round-trip；不适用字节相等） |

**实测锚点 (A)：** v0 的 `emit_wasm/runtime.rs:14-145` 已在与 v1 测试相同的 wasmtime 下注册并运行全部
preview1 集（`fd_write, clock_time_get, random_get, proc_exit, fd_read, path_open, …`）。
因此采用它们是*把已验证调用提升到 prim 基础层*，而非新发现。

<a id="2-reachability-reality-the-hard-honest-limit"></a>

## 2. 可达性的现实（严格、诚实的边界）

v1 runner 是裸 `wasmtime run <file>`——默认 WASI ctx，无 `--dir`、无 `--env`、继承 stdio
（`tests_part3.rs:525/:547`）。所以：

- **现在可达：** stdout（完成）、`clock_time_get`、`random_get`、`proc_exit`、`fd_read`（stdin）。
- **仅修改 runner 后可达**（`--dir .` / `--env`）：全部 `fs.*`、`env.get/cwd`、`process.env`。
  阻碍在*调用方式*，不在 wasm（v0 `calls_fs.rs` 已验证机制）。
- **preview1（当时目标）下 BLOCKED——第4指标列，不是“未实现”：** 全部 `http.*`（24）、
  `net.*` 原始 TCP（12）、SSE/LLM streaming（无 `sock_connect`/出站 socket）、
  `env.set`（`environ` 只读）。约36函数。preview1 下 native-only。**并非永久：**
  [`wasm-platform-frontier.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-platform-frontier.md) 计划 WASI 0.2/0.3 + Component Model 迁移
  （`wasi-http` 有 wasmtime host 实现；`wasi:sockets` = capability 范围网络，
  直接关联本蓝图 §1(B) capability 词汇，见 [`effect-system-capability.md`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md)）。
  在那个目标下，它们成为 **declared-caps-safe**（tier-2）。所以是“preview1 受阻，frontier 路线图解锁”，
  **必须作为单独指标列展示（§5），不能并入未实现数**，否则约36项需迁移 target，
  “剩78”却被误读为“78项可做”。这缩小了 preview1 “27→functional”的现实范围：
  clock/random/exit/stdin 现在可做，fs/env 要 runner flag，http/net 只在 WASI 0.2/p2 后。

<a id="3-build-order-safe-now-vs-needs-fresh"></a>

## 3. 构建顺序（SAFE-NOW 对 NEEDS-FRESH）

`NEEDS-FRESH` = 涉及 Coq registry / `cap_witness` / `declared_caps` / `is_known_free` /
`repr_of` / `$bump` 不变量 → 需要全注意力 + 对抗评审。

**Phase 0 — 脚手架与诚实指标配线**
- **B0** tier manifest `docs/stdlib/tier-manifest.toml` + `check-tier-manifest.sh` + `stdlib-tier-report.sh`（3列，绝不混合）。**SAFE-NOW**
- **B1** 按函数直接/传递 `Effect` 集填充 `effect_map`（`frontend/lower/mod.rs:412` 当时丢弃它）。**SAFE-NOW**
- **B2** 拆分 `Effect::IO` 为 Stdout 和 Fs。**NEEDS-FRESH**（review #3：declared_caps 的关键；与 B6 原子绑定。填充但未拆分的 effect map → 诚实 declared_caps = 少声明 = accept-but-unsafe）。
- **B3** 起草（未链接）self-host 主体：`datetime.now/monotonic_ns`、`random.*`、`io.read_*`，以及 **pure** `http` builders（`response/json/with_headers/req_*/query_params`——纯数据、无 cap、可立即 self-host）。**SAFE-NOW**

**Phase 1 — capability 基础（前提 gate、原子）**
- **B4** Task #35：`Capability::id` 单一来源 + 生成/断言 Coq id block + CI drift gate（类似 `check-stdlib-purity-registry.sh`）。必须早于任何 id 扩展（id 编号错误 = 静默 accept-but-unsafe）。**NEEDS-FRESH。** *（review #9：这是也保护 PROVEN 列的基础卫生，应与强证明队列并行，而非排后。）*
- **B5** capability 词汇 + caps-witness 诚实性——**原子单位、NEEDS-FRESH：**
  - 扩展 `lib.rs:471` 的 `enum Capability` + `Capability::id` 分支。
  - `proofs/CapabilityBound.v` 枚举新 id + 对抗 `Example`。**不改定理**——`subset_check_sound`（`Subset.v`）对 `nat` 普遍成立。
  - `certificate.rs:167` 的 `CallIndirect` taint：硬编码 `Stdout` → **`Capability::ALL`**（review #2：从唯一 `ALL` 常量导出；B4 drift gate 断言 `CallIndirect taint == Capability::ALL`，否则遗漏第8个 cap = 无 witness 的 closure）。
  - `classify_corpus.rs:465` 的 `is_known_free`：停止一概信任 `n.contains('.')`；带点 ⇒ known-free **仅当 `purity::is_pure(module,func)`**。**（review #1：必须先于 B7 落地。一旦出现带 capability 的带点 op，`contains('.')` 就把 `datetime.now` 当 caps-clean 放行 = accept-but-unsafe。收窄 `is_known_free` 与首个 capability op 是一个原子单位。两个 fold closure——`reaches_capability_or_unknown` 和 `reachable_caps_or_tainted`——都适用。）**
  - 原子地把 `TRUSTED_BASE.md` + `corpus-wall.sh` + 注释（#34 claim-drift gate）中的“没有未声明 **Stdout** effect”改成完整集合。
- **B6** 诚实 `declared_caps`：以 `transitive Effect → Capability` map 替换 `lower/mod.rs:128` 的 `if is_effect { [Stdout] }`。依赖 B1+B2。少声明 = unsafe；多声明 = 破坏 parity。**NEEDS-FRESH。**

**Phase 2 — 现在可达 host prim**（各自遵循 `fd_write` 链：`prim.almd` 声明 → `lower_prim_call` 分支 → `PrimKind` variant → render 分支 → `preamble` import → `cap_witness` 分支）。全部 **NEEDS-FRESH**（各带 capability）：
- **B7** `ClockTimeGet{clock_id}` → `Capability::Clock`——`datetime.now/monotonic_ns`、`env.unix_timestamp/millis`。Tier-2。
- **B8** `RandomGet` → `Capability::Random`——`random.int/float/choice/shuffle`。Tier-2。
- **B9** `ProcExit` → `Capability::Process`——`process.exit -> Never`。Tier-2。
- **B10** `FdRead{fd}` → `Capability::Stdin`——`io.read_*`、`process.stdin_lines`。**review #6：必须先声明不重叠 scratch-region 常量表**（交错 `println(int.to_string(read_line()))` 时 read 输出 buffer 不能碰到 print iovec@8 / itoa scratch，否则静默错输出，比 trap 更糟）。Tier-2。

**Phase 3 — raw-pointer / mem**
- **B11** `bytes.data_ptr(b)->Int = prim.handle(b)+12`——**SAFE-NOW**（无 cert/cap/所有权事件；惯用法已在 `bytes_core.almd` 出货；返回 Int 而非 RawPtr，所以不需 B12）。**但属 tier-3 OFFSET，不是免费增加 proven 数的成果**（review #4：硬编码 `LIST_HEADER=12` ≠ v0 的 +4 → 无法字节匹配；round-trip 测试只证明 v1 自身布局）。
- **B12** `repr_of` 的 `Ty::RawPtr => ScalarWidth::Word`。**NEEDS-FRESH**（此子集首个新分支；对抗评审：RawPtr Scalar 绝不能在 `verify_ownership` 冒充 heap 值）。
- **B13** `as_ptr/as_mut_ptr -> RawPtr`（与 B11 同主体，返回 RawPtr → 需 B12）。Tier-3。
- **B14** `from_raw_ptr`/`copy_to_ptr`——首条 RawPtr 参数路径（`bind_params` 放入 scalar 槽，绝非 heap borrow）。Tier-3 UNSAFE-contract。
- **B15** `heap_mark/heap_reset` + `mem.save/restore` + `bytes.heap_save/restore`——**困难；review #5：当前证明下不可成为 stdlib 可达 prim。** mark 以下的 `heap_reset` 绕过 `$rc_dec` 并破坏 `$freelist`；所有权证明对此盲目（`restore` 后活 ref 仍存在 = 同一 binary 内 accept-but-unsafe use-after-free）。**默认：完全延期，直到有 arena-escape-freedom 证明**（不是“gated UNSAFE-floor-only”——那仍把 accept-but-unsafe prim 放入验证表面）。

**Phase 4 — runner 解锁 / blocked**
- **B16** runner `wasmtime run --dir . --env …`——**NEEDS-FRESH（刻意）：** 静默改变整个证明所针对的 sandbox；单项最大解锁（启用全部 `fs.*` + `env.get/cwd` + `process.env`）。没有它，fs prim compile+validate 后运行时 trap `ENOTCAPABLE` = 与 v0 native 的静默 parity 破坏。
- **B-BLOCKED：** `http.*`/`net.*`/`env.set`——记录为 wasm-unsupported / native-only（§2）。

<a id="4-soundness-story-why-vocab-expansion-keeps-no-undeclared-effect"></a>

## 4. 健全性叙述（词汇扩展为什么仍保留“无未声明 effect”）

第4性质 `within_bound = subset_prop allowed used` 在 `Subset.v` 的 `nat` 列表上定义，
**全称量化，不枚举 capability**。所以增加 id **免费**推广证明；`subset_check_sound` 字节不变，
唯一 Coq 编辑是 registry 注释和非关键 `Example`。
**全部负担转向不受信 emission 侧：`cap_witness` 必须过近似实际 host effect**
（每次到达 host 都向 `used` 贡献 id）。由这些满足：穷尽的 `RtFn::capability` match
（漏 variant = compile error）+ 各 host-prim 的 `cap_witness` 分支 + `Capability::ALL` 的
`CallIndirect` taint + 对每条 effectful `CallFn` 路径拒绝或发出 cap。
唯一破坏是 host op 到达 cap 却未贡献 id → 该 id 空真（accept-but-unsafe），全在 emission 侧。
所以 B4+B5+B6 原子地 NEEDS-FRESH，没有配套 cap + witness 的 sandbox-exit prim 被禁止。
C 不携带 capability → 完全不能削弱此性质（它的风险是所有权/RC 证明 B15，另一个不变量）。

**★ FRESH-BUILD 对抗问题 Q1（固定——本计划每个 brick 唯一健全性关键问题）：**
*“emitter 是否在任何地方少声明 capability，即有没有接触 host effect（fs/net/clock/random/…）
却不向 `used` 贡献 id 的路径？”* 因为健全性现在完全依赖 `cap_witness` 的过近似，
**多声明（保守）总是安全；少声明是唯一 accept-but-unsafe 漏洞。**
所以每次 fresh 对抗评审从这里开始，必须反驳的具体少声明路径恰为：
(1) 新 host-prim 没有 `cap_witness` 分支；(2) 带点 effectful `CallFn` 被 `is_known_free` 放行（B5）；
(3) `CallIndirect` taint 窄于 `Capability::ALL`（B5/B4）；
(4) `declared_caps` 把 effect 映射为少于其到达数量的 cap（B6/B2）；
(5) 遗漏 `RtFn::capability` variant（compile-error，因此结构上已闭合）。
若都没有少声明，该 brick 健全；其他性质（ownership/names）不受 capability 工作影响，
B15 除外（独立不变量）。

<a id="5-honest-3-tier-metric-resolves-100-needs-redefining"></a>

## 5. 诚实的 3-tier 指标（解决“100%需重新定义”）

```
STDLIB FUNCTIONAL COVERAGE  543 / 621
  ├─ PROVEN (byte-matches v0)               : 543  ← the ONLY column any claim may call "proven"
  ├─ DECLARED-CAPS-SAFE (runs, caps⊇used)   :   0
  ├─ OFFSET-FUNCTIONAL (ptr round-trip)     :   0
  ├─ BLOCKED-IN-WASM-preview1 (native-only) : ~36  ← http/net/env.set; needs WASI 0.2/p2 (frontier)
  └─ unimplemented (buildable on this target):  ~42
```
**第4列对诚实至关重要：** 约36项不是“此目标上待做”，而需 WASI-0.2 迁移
（`wasm-platform-frontier.md`）。藏在 `unimplemented` 会把“剩78”读成“78项可做”，
正是3列分离禁止的过度主张；WALL 也应用物理分离纪律。
`check-tier-manifest.sh`（B0）从 `blocked_target = "preview1"` 字段计算此列，令边界是数字而非散文。
- **Tier-1 PROVEN**——render→wasm→wasmtime 字节匹配 `#[test]`（`assert_eq!(out, <v0 golden>)`）。
  **review #7：`check-tier-manifest.sh` 必须验证引用测试含 v0-golden 字节断言
  （grep `assert_eq!`+golden marker），不只是存在同名 `#[test]`**；否则误标 `proven` 的 smoke test
  会犯“把能工作报告为已证明”的大错。
- **Tier-2 DECLARED-CAPS-SAFE**——caps ACCEPT（`reachable ⊆ declared`，复用 gate）+
  无 trap 运行的 smoke（exit-0，**不对 stdout 字节做任何断言**）。主张是“仅做所声明的事”，不是“产生 X”。
- **Tier-3 OFFSET-FUNCTIONAL**——布局无关功能 law（`load8(store8(a,v))==v`），绝非字面字节相等。

**重定义目标：** **“三 tier 总计 621/621 FUNCTIONAL，其中约594 PROVEN。”**
标题数字是 PROVEN 子计数。约594是*预测*（tier-report script 自我修正），不是已达成主张。
**任何 API 或文档都不可输出混合的“621 proven”。**

<a id="6-smallest-genuinely-safe-first-step"></a>

## 6. 真正安全的最小第一步

**B11**（`bytes.data_ptr`）——唯一不碰 proof/gate/cap/foundation 的项目。
但标 **OFFSET**（tier-3），不是免费的 proven。其他均为强证明队列之后的 NEEDS-FRESH（优先级节）。

<a id="7-priority--this-is-the-plan-not-a-queue-jump"></a>

## 7. 优先级 — 这是计划，不是插队

此工作上限是 tier-2/3（弱证明）。**强证明要项优先**——各以 PROVEN 落地并增加标题数：
`string.split`（List[String] 构建）、`list.map/filter/fold`（高阶 self-host）、
**#60 StringInterp**（核心语言表面）。§3 安排其后。
例外：**B11** 机会性进行；**B4**（review #9）并行（也保护 proven 列），不延后。

<a id="8-review-must-fixes-folded-into-25-above"></a>

## 8. 评审必须修复项（已纳入上文§2–§5）

1. 收窄 `is_known_free` 必须先于 B7，覆盖两个 fold closure（否则带点 cap call = accept-but-unsafe）。2. `CallIndirect` taint = `Capability::ALL`（派生）+ B4 断言覆盖。3. B2 改标 NEEDS-FRESH，与 B6 原子绑定。4. B11 是 tier-3 OFFSET，不是“安全且完成”。5. B15 = 延期（非 UNSAFE-floor-only）。6. B10 先需静态 scratch-region map。7. tier 检查验证字节断言，不只测试存在。8. `process.args` 在 test-runner 与 `almide run --` 中不同（生产 parity）。9. B4 提升为与 strong-proof 并行。

<a id="envargs-v1--validated-design--the-entanglement-2026-06-25"></a>

## env.args v1 — 已验证设计与纠缠（2026-06-25）

env.args 是 v1 WASI 基础层首个 I/O capability。它仿照已验证 random.int 机制（当时唯一允许的 effectful call）：self-host stdlib 函数 → 携带 Capability 的 `prim.*` op → 传递 cap_witness → `used ⊆ declared` cert 验证 → render 发出 WASI call。具体部分：
1. `Capability::CliArgs`（lib.rs:599 enum；lib.rs:614 的 Coq registry 映射须保持单射+稳定：Stdout=0、Entropy=1、CliArgs=2）——形式 trust-spine 新增。
2. `PrimKind::ArgsSizesGet` + `PrimKind::ArgsGet`（lib.rs:443 区域）携带 `Capability::CliArgs`（certificate.rs:178 的 cert 记账，仿 random_get→Entropy）。
3. `stdlib/env_args.almd`——self-host `env_args() -> List[String]`：`prim.args_sizes_get(argc_p, bufsize_p)` → 分配 argv+buf → `prim.args_get` → 循环把 argc 个 null 结尾字符串放进 `List[String]`（buffer 解析比 random.int 单次 i64 读取复杂）。
4. 两个 prim 的 render_wasm WASI runtime（render_wasm 已有 WASI 基础层，random.int 的 random_get 在 v1 可用）。
5. calls.rs:187 允许 `env.args`（扩展 `is_admitted_effectful` 谓词）。

🚨 纠缠（为何比单纯 capability 新增更大）：BYTE-MATCH ORACLE 坏了。
`almide run --target wasm -- file.csv` 不把 `--` 参数传给 wasm 执行
（env.args() 返回 EMPTY → csv-to-json 输出“usage”而非 JSON），连完整 emit backend 的参数配线都坏了。
wasm_cross harness 只跑 pure fixture（不支持 program args）。所以 env.args 获得 cert-byte 验证之前，
须修复参数配线（CLI runner 转发 + harness 给 native 与 wasmtime 相同 argv）。
这才是“分阶段”的真正第一步，是独立于 trust-spine capability 工作的 CLI/test-infra brick。

顺序：(a) 修复参数配线 + wasm_cross harness 参数支持（字节匹配 oracle），
然后 (b) 仿照 random.int 的 env.args capability（步骤1–5）。csv-to-json 还需 fs.read_text
（path_open+fd_read self-host + Capability::FsRead），以及 almide-grep + fs.walk，各是独立 capability brick。
这两个应用的全部 WASI 文件 I/O 基础层约3–4个 capability brick + infra。
建议作为专注的 fresh 工作（每个 capability 委派+gate）。

保留英文指标图说明：STDLIB FUNCTIONAL COVERAGE 为543/621。PROVEN（与v0字节匹配）543，是原图唯一可称proven的列。DECLARED-CAPS-SAFE（运行且caps⊇used）0，OFFSET-FUNCTIONAL（指针round-trip）0；BLOCKED-IN-WASM-preview1（仅native）约36，指http/net/env.set、需WASI 0.2/p2迁移；此target上可构建但未实现约42。旧标签受开头有限测试边界限制。
