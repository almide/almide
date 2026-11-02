原文 description 元数据：飞行 gate G-F4 — 端到端通过 `make verify` 的参考应用（在 counted sim 循环上运行的定点 Q16.16 PID 控制律 kernel）、7阶段验证管线（已存在 vs 依赖基石）、产出 receipt（C-SAFE/C-PROVEN 绿，C-WCET/C-FAITHFUL 待做），以及降低风险顺序（Slice 0 scalar-no-print 当前绿 → Slice 1 print=G-F0 frontier → Slice 2 基石あ解锁 C-WCET → Slice 3 基石い解锁 Ferrocene）。
<a id="flight-reference-app--pid-制御則カーネル--make-verify--receiptg-f4"></a>

# 飞行参考应用 — PID 控制律 kernel + make verify + receipt（G-F4）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-reference-app.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-reference-app.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-reference-app.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md)

> 原文注记：保留历史应用计划及后续状态纠正，不是可飞行控制器或已获受理资格化。原代码 sketch 有已知语法/状态更新错误，刻意不修复、不执行。后续入口优先于旧 print/renderer/app receipt 待办；历史 receipt 是示例，不是新生成证据。分配上限标签 C-WCET 不确立物理最坏执行时间，证书安全不证明控制律稳定性、gain 或功能需求。仅 Stdout 的 capability 不覆盖全部 host effect。初始仅 kernel 的表述及 clean compile=object 信任口号不消除抽取、renderer、host、toolchain 信任。Ferrocene lane 在等待访问时使用 stock rustc，不实证已用资格化 toolchain 或获得监管 credit。数字、投票、界限是原文时点记录；未重跑实现、证明、测量或 receipt 命令。

> **状态 2026-08-12 — 应用落地（PR: flight-pid-reference）**：`spec/wasm_cross/flight_pid_control.almd` 以 C-230 固定契约。实测 native == wasm **字节一致**（16步 trace），**interp 也投票**（3-way 一致，voting 254/374），corpus wall **TOTAL**（6319 fn，walled 0 — 全 fn 附证书）。本应用阶段1、2a-c、3、4、5 green（2a-c 的 Coq 复验在 CI trust-spine）。与设计 sketch 的差异：`err` 是 Result ctor 保留字 → `e`/`prev_e`，`..` → `..<`，本地 clamp → 委托 `int.clamp`（E015），**sketch 忘了更新 integral/prev_err** — 修正为 driver 的 scalar `var` 更新（依 §1 约束注记）。剩余：阶段2d（基石あ）、6-7（基石い）+ receipt 的 app-scoped 块（此行是2026-08-12时点待办记录）。

<a id="current-evidence-entrypoints"></a>

## 当前证据入口

- `spec/wasm_cross/flight_pid_control.almd` 是已实现应用；下方原 sketch 含已记录的状态更新/语法错误，保留作设计历史，不是可复制的当前示例。
- `proofs/receipt.sh` 现在产出 app-scoped G-F4 块。描述所检查成果物的是其实际 verdict 与 scope，而不是下方原 PENDING 清单。
- `crates/almide-mir/src/render_native.rs` 与 `src/cli/emit.rs` 提供 native renderer/trace-map 路径，不只是未连接的368行 demo。
- `.github/workflows/ferrocene-lane.yml` 明示等待 Ferrocene 访问权限期间使用 stock rustc。该 lane 通过不证明使用已资格化 toolchain、完整 Almide→Rust 忠实性或监管受理。
- 每循环 witness/物理 WCET 提案仍独立；不可仅因参考应用或 bounded profile 存在就标为完成。

> **目标**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) 阶梯 **G-F4** 的具体设计 — 用飞行 profile 编写、具备验证/receipt 路径的**参考应用**（将幻灯片化为成果物的“弹药”）。运行 → oracle 字节一致 → 每次构建证书 → 可读 Rust → Ferrocene。须落在飞行子集（[flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)）内。
> **相关**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 / [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) / [flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)。

---

<a id="1-リファレンスアプリ--採用fixed-point-pid-制御則カーネル"></a>

## 1. 参考应用 — 采用定点 PID 控制律 kernel

`pid_control.almd` — 用整数定点（Q16.16）实现单轴 PID 控制器，在**有界 counted 仿真循环**（N步）中运行，每步打印饱和执行器指令。正是阶梯为 G-F4 举例的“控制律 kernel”（[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2）。

**采用理由（按标准）**：
- **(a) 看起来安全关键**：PID + 饱和 + anti-windup 是航空/汽车控制最普遍形态（FADEC、舵面执行器、ECU 扭矩）。cert 工程师立刻把“控制律”映射到 DO-178C/ISO 26262 心智模型；比 CRC（看似 utility）或 ring buffer（底层管道）更有力。
- **(b) 符合飞行子集**：全 scalar Int（定点），hot path 无动态分配，循环体无递归，driver 为 counted range `for t in 0..N`，恰是 `try_lower_scalar_for_range`（`crates/almide-mir/src/lower/control.rs:766`）lower 的形状。PID `step` 是 pure 算术 `fn`（无 heap/caps），仅 driver 的 `println(int.to_string(cmd))` 到达 Stdout。
- **(c) 小却真实**：约~60-80行。Q16.16 乘法（用 `/ 65536` 重标度）、饱和（`math.max`/`math.min`，已自托管、pure）、积分 anti-windup（accumulator clamp）。控制工程师能认出每项。
- **(d) 端到端运用证书**：触及全部已证明性质 — **ownership**（每个 `int.to_string` 产生1个 `Alloc` `i`，move-out 至 `println`，drop）、**name totality**（driver→`pid_step`→`clamp` 调用图）、**capability bound**（driver **声明** Stdout 并到达 ⇒ ACCEPT，积分/微分 math 为 pure ∅）、**counted loop** = WCET 基石的首个真实 witness 对象。

**结构 sketch**（基于 `docs/CHEATSHEET.md` + 自托管表面）：

```almide
// Q16.16: 1.0 == 65536。全スカラ Int ── hot path に heap なし。
let SCALE: Int = 65536
let OUT_MIN: Int = 0 - 6553600
let OUT_MAX: Int = 6553600
let I_MIN: Int = 0 - 3276800
let I_MAX: Int = 3276800

fn fmul(a: Int, b: Int) -> Int = (a * b) / SCALE          // (a*b)>>16
fn clamp(x: Int, lo: Int, hi: Int) -> Int = math.max(lo, math.min(hi, x))

// 1 PID ステップ。PURE: capability 宣言なし(∅)。
fn pid_step(kp: Int, ki: Int, kd: Int, setpoint: Int, measured: Int,
            integral: Int, prev_err: Int) -> Int = {
  let err   = setpoint - measured
  let integ = clamp(integral + err, I_MIN, I_MAX)         // anti-windup
  let deriv = err - prev_err
  let raw   = fmul(kp, err) + fmul(ki, integ) + fmul(kd, deriv)
  clamp(raw, OUT_MIN, OUT_MAX)                            // アクチュエータ飽和
}

// COUNTED シミュレーション driver。effect fn({Stdout} 宣言)。counted range。
effect fn main() -> Unit = {
  var integral = 0
  var prev_err = 0
  var measured = 0
  let setpoint = 65536                                    // 1.0 ステップ入力
  for t in 0..16 {                                       // counted、リテラル bound = WCET witness
    let cmd  = pid_step(13107, 6553, 3276, setpoint, measured, integral, prev_err)
    measured = measured + (cmd / 256)                    // 簡易プラント
    println(int.to_string(cmd))                          // 唯一の Stdout 出口
  }
}
```

保留 sketch 的注释全文说明：Q16.16：1.0 == 65536；全 scalar Int，hot path 无 heap。乘法对应 (a*b)>>16。1个 PID 步骤是 PURE，无 capability 声明（∅）；积分 clamp 做 anti-windup，最后 clamp 做执行器饱和。counted 仿真 driver 为 effect fn（声明 {Stdout}），使用 counted range。setpoint 是1.0阶跃输入；循环字面量 bound 是 WCET witness。更新 measured 是简化 plant，打印是唯一 Stdout 出口。保留原代码及已知状态更新遗漏，不在这里修复。

*（counted scalar 循环内 scalar `var` 再赋值为 `scalar_loop_depth` 允许形态（`control.rs:816`）。每次迭代 heap-tuple 共返回 `(cmd, integral)` 属 DEFERRED 子集，故 integral 保持 scalar `var` — 见 §5。）*

**次选：CRC-16/CCITT**（`crc16.almd`）— 对固定字节表做 counted 循环，将 CRC 寄存器 shift/XOR fold。**更显然在子集内**（与 `list.sum` 迭代形完全相同），是**最小切片种子**（§4），但 cert 工程师会觉得像“utility”。保留作**最小先行**，demo 由 PID 引领。

---

<a id="2-原設計の検証パイプライン当時の-existgated-記録"></a>

## 2. 原设计验证管线（当时 exist/gated 记录）

CI Trust Spine 为 `make verify-trust`→`make receipt`（`Makefile:77-84`）。本应用各阶段：

| # | 阶段 | 命令/工具 | 产出 | "pass" | 状态 |
|---|---|---|---|---|---|
| 1 | source→IR→MIR | `render_program` example（`examples/render_program.rs:28-50`） | per-fn MIR | 全 fn `Ok`（in-subset），walled 零 | **EXISTS** |
| 2a | ownership 证书 + ACCEPT | `gate.sh`: `emit_cert_from_source`→`checker ownership` | `ownership.cert` | `check_all_sound` 0 | **EXISTS** |
| 2b | name-totality + ACCEPT | `checker names` | `names.cert` | `check_names_cert_sound` 0 | **EXISTS** |
| 2c | capability + ACCEPT | `checker caps`/`tcaps`。`main`={Stdout}、`pid_step`=∅ | `caps.cert` | reachable⊆declared：main 已声明 Stdout ⇒ **ACCEPT** | **EXISTS**（应用新的正向案例：*已声明* effect 获 ACCEPT） |
| 2d | alloc-bound/counted-loop witness | `checker allocbound`（[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)） | `<B>\|L16{…}` | `lrun` 重导 `total≤B`，body alloc-free ⇒ 与 trip 无关 | **GATED 基石（あ）**（`CountedLoopStart` 尚无，`lib.rs:254` 仅 generic `LoopStart`） |
| 3 | corpus wall 成员资格 | `corpus-wall.sh` | wall report | `lower_function` 全域，in-profile witness 全 ACCEPT | **EXISTS** |
| 4 | wasmtime 执行 | `render_program`→`.wat`→wasmtime | 指令列 stdout | 运行，exit 0 | **PARTIAL GATED**：scalar+heap 执行已有，实际 `println`→`$print_str` 触及纪律测试 `handwritten_wasm_runtime_does_not_grow`（baseline 11）=“v0 trap”，G-F0 frontier |
| 5 | oracle 字节一致（dual oracle vs v0） | v0（`almide run --target wasm`）与 `diff`（C-REPRO “until v1 parity”） | 字节一致 | v1 stdout == v0 stdout | **gate 机制 EXISTS**，阶段4 print path 落地后覆盖本应用 |
| 6 | 可读 Rust render | `render_rust_program`（[flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)） | review-grade `.rs` | 经 `cargo fmt` 后通过人工 review | **GATED 基石（い）**（当前只有368行 `Vec<i64>` demo） |
| 7 | Ferrocene compile | `ferrocene` rustc | 用资格化 toolchain 编译 | clean compile = object-code 信任（绕开 Gap-1） | **GATED 基石（い）** + Ferrocene access |

**exist/gated 总结**：阶段 **1、2a-c、3** 在值语义+scalar-counted-loop 子集中**当前 green**。阶段 **2d**（WCET 主张）依赖基石（あ），阶段 **6-7**（Ferrocene leg）依赖基石（い）。阶段 **4-5** = 活跃的 **G-F0 frontier**（“scalar/heap 可运行”与“循环 print 并与 v0 字节一致”之间*唯一*的东西 = `$print_str` 自托管）。

---

<a id="3-原設計の-receipt-形現行出力は-receiptsh-を参照"></a>

## 3. 原设计 receipt 形状（当前输出见 receipt.sh）

`make receipt`（`receipt.sh`）把 checked facts 纳入 C-SAFE/C-REPRO/C-FAITHFUL/C-PROVEN 包。用于 `pid_control.almd`（以 app-scoped 块扩展总体 receipt）：

```
# Receipt — Almide v1 flight reference app: pid_control.almd
# 再現: make verify-trust && make receipt
# 信頼基底と限界: proofs/TRUSTED_BASE.md

App: pid_control.almd — 単軸 Q16.16 PID 制御則、16 ステップ counted sim。
Subset: scalar fixed-point + counted range + 1 宣言 {Stdout} effect。

| claim       | meaning(this app)                                  | status  | evidence(段)                                              |
|-------------|----------------------------------------------------|---------|------------------------------------------------------------|
| C-PROVEN    | checker soundness は Coq kernel のみに依存          | PASS    | 45 定理・Print Assumptions closed・coqchk(段 2)           |
| C-SAFE/own  | pid_control に double-free/UAF なし                 | PASS    | 全 fn ownership witness ACCEPT(段 2a/3)                   |
| C-SAFE/name | call graph に dangling MIR 参照なし                 | PASS    | checker names ACCEPT(段 2b)                               |
| C-SAFE/cap  | main は宣言した {Stdout} のみ到達、PID 数学は pure  | PASS    | reachable⊆declared ACCEPT(段 2c)                          |
| C-WCET      | 総確保が静的有界、ループ alloc-free                 | PENDING | allocbound witness <B>|L16{…} を lrun ≤ B(段 2d) ── キーストーン(あ) |
| C-REPRO     | v1 stdout == v0 stdout、byte 一致                   | PARTIAL | dual-oracle diff(段 5)、$print_str discipline trap で blocked(段 4) |
| C-FAITHFUL  | render した Rust が MIR を refine、Ferrocene compile| PENDING | render_rust + rust_pattern 忠実 + Ferrocene(段 6-7) ── キーストーン(い) |

Falsification(否定空間、receipt-logic §5):
  C-SAFE  : render 成果物の UAF PoC 1 つ ⟹ false
  C-REPRO : v1≠v0 stdout の host 1 つ ⟹ false
  C-WCET  : B を超えて確保する実行 1 つ ⟹ false
  C-PROVEN: sorry / build 失敗 / 反例プログラム
```

保留 receipt 示例的全文说明：标题为 Almide v1 飞行参考应用 pid_control.almd；复现命令 make verify-trust && make receipt；信任基和限制见 proofs/TRUSTED_BASE.md。App：单轴 Q16.16 PID 控制律，16步 counted 仿真。Subset：scalar 定点 + counted range + 1个声明的 {Stdout} effect。

- C-PROVEN：checker soundness 仅依赖 Coq kernel；PASS；45个定理、Print Assumptions closed、coqchk（阶段2）。
- C-SAFE/own：pid_control 无 double-free/UAF；PASS；全部 fn ownership witness ACCEPT（阶段2a/3）。
- C-SAFE/name：调用图无悬空 MIR 引用；PASS；checker names ACCEPT（阶段2b）。
- C-SAFE/cap：main 只到达已声明 {Stdout}，PID 数学 pure；PASS；reachable⊆declared ACCEPT（阶段2c）。
- C-WCET：总分配静态有界，循环 alloc-free；PENDING；allocbound witness &lt;B&gt;|L16{…} 经 lrun ≤ B（阶段2d），基石（あ）。
- C-REPRO：v1 stdout == v0 stdout，字节一致；PARTIAL；dual-oracle diff（阶段5），因 $print_str discipline trap blocked（阶段4）。
- C-FAITHFUL：render 的 Rust 精化 MIR，Ferrocene compile；PENDING；render_rust + 忠实 rust_pattern + Ferrocene（阶段6-7），基石（い）。

反证（否定空间，receipt-logic §5）：C-SAFE 在 render 成果物中有1个 UAF PoC 即为 false；C-REPRO 有1个 host 的 v1≠v0 stdout 即为 false；C-WCET 有1次执行分配超出 B 即为 false。C-PROVEN 的反证为 sorry / build 失败 / 反例程序。这是历史 receipt 翻译，不是新的 verdict。

**实际展示物**：60行 `.almd`（1分钟可读完并认出 PID 控制律）→ 1条命令 `make verify-trust && make receipt` **在客户机器上**（CI 在信任基外）重导上表 → 4条绿色 C-SAFE/C-PROVEN 行 + 诚实的 C-WCET/C-FAITHFUL PENDING。**不隐藏 PENDING 的诚实就是可信性。**

---

<a id="4-当初の-de-risking-順最小先行"></a>

## 4. 原始降低风险顺序（最小先行）

- **Slice 0 — pure scalar counted-loop 算术，无 print（当前 green）**。`pid_step` + 累积 driver（`fn sim() -> Int = { var acc=0; for t in 0..16 { acc = acc + pid_step(...) }; acc }`）。当前全量 lower，ownership+names+caps（∅）pass、corpus wall pass、**scalar wasmtime 执行与 v0 一致**。green 阶段：1、2a-c、3、4、5。CRC-16 次选的自然位置。**除 print 和2个基石外端到端贯通的最小可信飞行形态产物。**
- **Slice 1 — 添加 `println(int.to_string(cmd))` 循环（G-F0 frontier）**。引入自托管 `int.to_string`（`Alloc` `i`、ownership 验证）+ `$print_str`，解锁 C-REPRO（阶段5），但先跨过 `handwritten_wasm_runtime_does_not_grow` discipline trap（活跃 G-F0）。**关键路径解锁**：print 自托管的一刻，全 PID 应用运行并与 v0 字节一致，C-SAFE+C-REPRO 全 green。
- **Slice 2 — 基石（あ）解锁 C-WCET**。`CountedLoopStart{bound}`/`End`（`lib.rs:248`），`try_lower_scalar_for_range` 以 literal bound + alloc-free-body wall emit，`alloc_bound_witness_string`、`CountedLoop.v`/`NoAllocInLoop.v`/`AllocBound.v`（Brick 0→1）。receipt C-WCET 从 PENDING→PASS，“WCET-by-construction”成为实际 PID 循环上的*已证明*主张。**把“memory-safe”变成“*flight*-shaped”的行。**
- **Slice 3 — 基石（い）解锁 C-FAITHFUL + Ferrocene**。生产级 `render_rust_program`，移植忠实 `rust_pattern` 层（证明便宜，`eager_copy_refines_safety` 已证明），将 PID render 为 review-grade Rust，Ferrocene compile。C-FAITHFUL PENDING→PASS，receipt 端到端完成。

---

<a id="5-当初のギャップ一覧printrendererreceipt-の後続状況は冒頭参照"></a>

## 5. 原始缺口清单（print/renderer/receipt 后续状态见开头）

1. **Ferrocene compile + 可读 Rust（阶段6-7）**需生产级 MIR→Rust renderer（基石（い））。当前为368行 `Vec<i64>` demo，C-FAITHFUL 诚实地 PENDING。证明成本低，工程（≈5× render_wasm）才是真正工作。
2. **alloc-bound/WCET 主张（C-WCET）**需基石（あ）。`CountedLoopStart` 尚无，checker 当前对循环 flat fold，不证明任何 trip count。当前应用*循环内 memory-safe*，但 WCET/分配上限是*未证明主张*。
3. **print path（C-REPRO 端到端执行）**为活跃 G-F0 frontier。实际 `println` 运行且与 v0 一致，但遇到 `handwritten_wasm_runtime_does_not_grow`（baseline 11）trap。直到 `$print_str` 干净自托管前阶段4/5 blocked（无 print 的 Slice 0 green）。
4. **功能正确性是应用责任，不是证书责任**。cert 证明 PID kernel memory-safe/name-total/cap-bounded/（基石（あ）后）WCET-bounded，**完全不证明控制律是否正确（gain/响应是否稳定）**。支柱③（需求追溯 + MC/DC）在范围外。receipt 不得暗示 C-SAFE =“控制器正确”。
5. **capability 词汇仅 Stdout**。`eprintln`/abort/fs/net 是未命名 host effect。PID 在 Stdout 内，所以不阻碍本应用，但 receipt 须将 C-SAFE/cap 诚实限定为“无声明外的 **Stdout** effect”。
6. **子集形状约束应用**。每迭代 heap-tuple 再赋值 DEFERRED，`i64::MIN` 是 `int.to_string` 未处理 edge。采用应用将 integral 保持 scalar `var` 以维持 in-subset — 应尊重而非掩盖的设计约束。
