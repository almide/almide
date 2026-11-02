说明：从类型正确源码到正确二进制的链中，仍缺少机械或数学证明的层
<a id="correctness-guarantee-gaps"></a>

# 正确性保证的缺口

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](correctness-guarantee-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](correctness-guarantee-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](correctness-guarantee-gaps.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md)

> 原文注记：退役backend清单及5–6月日志为历史，不是当前漏洞或待实现清单。开头现行边界优先于旧release仅警告及gate缺失记载；6月4日更新6月3日仅stdout覆盖。algorithm RC证明、runtime行为、翻译符合、byte一致分别对待。46bug主张与A–H合计41、A-case未计数不推测整合。旧fan胜者提案与Option传播残项不代表今日状态。未执行命令、gate修改或测试。


> “well-typed source -> correct binary”链：缺乏机械／数学证明的层

状态：**历史缺口分析及后续订正日志**。下述原 Perceus／`emit_wasm` 架构已退役，不要把任务表当成当前结构化 backend 清单。当前路线和证明边界见 `src/wasm_route.rs`、`proofs/receipt.sh`、`TRUSTED_BASE.md`。

<a id="evidence-boundary"></a>

## 证据边界

原标题“heap alloc 恰好释放一次”夸大了算法定理所能确立的范围。对建模 RC 事件的证明，本身并不证明 Rust 翻译、生成的 drop routine 或任意执行程序。下方历史分析本身就记录了模型到实现的缺口。

当前 IR postcondition（`crates/almide-codegen/src/pass.rs`）在 release 和 debug build 都会失败终止；后续历史 milestone 提议的变更并非当前缺失 gate。结构化 runtime 证明和剩余 trusted seam 应从当前 receipt 阅读，而不是旧的固定 23 定理／Perceus／StackBalance 清单。

<a id="original-gap-inventory-historical-architecture"></a>

## 原缺口清单（历史架构）

<a id="1-wasm-emitter-emit_wasmrs--largest-gap"></a>

### 1. WASM emitter（`emit_wasm/*.rs`）— 最大缺口

手写 WASM 指令输出。数百次 `wasm!()` macro call 的 stack effect 仅靠人工 review 验证。

- `rt_string.rs`、`rt_list.rs` 等：手写指令序列。
- `expressions.rs`：BinOp、Match、Record 的 branch stack effect 靠目视。
- layout offset：`LayoutRegistry` 集中管理，但正确使用仍靠人工。

**当时状态**：`emit_wasm/engine/` 已有 WasmBuilder + WasmIR + LayoutRegistry 基础设施。部分迁移完成（list_layout.rs 使用 WasmBuilder）。大部分 `expressions.rs` 和 `rt_*.rs` 仍用 raw `wasm!()`。

<a id="2-anf-pass--heap-alloc-visibility-unproven"></a>

### 2. ANF pass — heap alloc 可见性未证明

ANF 必须把每个 heap intermediate lift 到 VDecl，Perceus 才能追踪。`needs_lift()` 覆盖已知 case，但没有覆盖全部 case 的证明。遗漏＝heap leak。

**当时状态**：`needs_lift()` 覆盖 Call、RuntimeCall、BinOp、If、Match、Block。**已知缺口**：Fan、List/MapLiteral/Record/Tuple literal、StringInterp、IterChain、ForIn、While、UnwrapOr、OptionalChain、Member、IndexAccess、ClosureCreate 未匹配。无专门 ANF test。

<a id="3-closure-conversion--env-layout-correctness"></a>

### 3. Closure conversion — env layout 正确性

`ClosureConversionPass` 将 capture 变量打包进 env struct，通过计算 offset 的 `EnvLoad` 读取。offset 正确性仅经测试验证。

**当时状态**：offset 公式 `index * 8`，capture 按 VarId 排序。输出使用类型感知 load（I32/I64/F64）。offset 边界或 env size 一致性的 `debug_assert` 为零。测试覆盖：`closure_nested_capture_test.almd` + `monkey06_closures_test.almd`。

<a id="4-type-inference---ir-lowering-fidelity"></a>

### 4. 类型推导 → IR lowering 忠实性

`almide-frontend` 推导的类型必须忠实反映在 IR `expr.ty`。已有 `ConcretizeTypes` postcondition 检查，但全部 pattern 覆盖未证明。

**当时状态**：`resolve_node_ty` 明确处理 17 个 variant。24+ variant 返回 None（MapLiteral、Record、SpreadRecord、Range、MapAccess、StringInterp、Try、UnwrapOr、ToOption、OptionalChain、Clone、Deref、Borrow、BoxNew、RcWrap、ClosureCreate、FnRef、ForIn、While、Fan 等）。postcondition audit（`audit_remaining_unresolved`）捕获未解析类型，却不保证访问了全部 variant。设计故意为 best-effort。

<a id="5-perceus-incdec-insertion--lean-to-rust-fidelity"></a>

### 5. Perceus Inc/Dec 插入 — Lean 到 Rust 的忠实性

Lean 4 定理证明算法正确，但 Rust 实现是手工翻译，不是机械提取。通过与 Lean spec 人工比较验证符合性。

**当时状态**：所有缺口中依据最强。`perceus_verified.rs` 在 Rust 中映射 Lean 证明。proptest 验证 is_freed/has_dec。PerceusVerifyPass 在每次 WASM build 运行。`perceus_monkey_test.almd` 含对抗 case。Lean 证明：23 定理、0 sorry。

<a id="risk-ranking"></a>

## 风险排序

1. **WASM emitter** — 风险最高、范围最广、无静态检查。
2. **ANF `needs_lift()`** — 静默失败（leak 而非 crash）。
3. **Closure env offset** — 错误 offset＝内存破坏。
4. **类型 lowering 忠实性** — postcondition 检查缓解。
5. **Perceus Lean 符合性** — 广泛测试缓解。

---

<a id="original-milestone-steps-retain-with-the-dated-follow-up-logs"></a>

## 原 milestone 步骤（与带日期后续日志一起保留）

<a id="gap-1-wasm-emitter---wasmir-migration"></a>

### Gap 1：WASM emitter → WasmIR 迁移

跟踪：[wasm-engine-redesign.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md)

| 步骤 | 描述 | 交付物 | 依赖 |
|------|-------------|-------------|------------|
| 1a | 审计剩余 raw `wasm!()` call site | 尚未用 WasmBuilder 的 file/function 清单 | — |
| 1b | `rt_string.rs` 迁至 WasmBuilder | rt_string 中 raw `wasm!()` 零 | — |
| 1c | `rt_list.rs` 迁至 WasmBuilder | rt_list 中 raw `wasm!()` 零 | — |
| 1d | 迁移 `expressions.rs`（BinOp、Match、Record） | expressions 中 raw `wasm!()` 零 | — |
| 1e | WasmIR Op enum 加 stack-effect 类型注释 | 每个 Op 声明 `(pops, pushes)` | 1b-1d |
| 1f | 实现 WasmIR stack-effect verifier | 拒绝净 effect 不等于预期的指令序列 | 1e |
| 1g | 删除旧 raw emission path | `wasm!()` macro 删除或成为 dead | 1f |

**Gate**：1f 后，编码前静态验证每个 WASM function 指令流的 stack balance。

<a id="gap-2-anf-needs_lift-completeness"></a>

### Gap 2：ANF `needs_lift()` 完整性

| 步骤 | 描述 | 交付物 | 依赖 |
|------|-------------|-------------|------------|
| 2a | 为 `needs_lift()` 添加缺失 IrExprKind variant | Fan、List、MapLiteral、Record、Tuple、StringInterp、IterChain、ForIn、While、UnwrapOr、OptionalChain、Member、IndexAccess、ClosureCreate | — |
| 2b | 加 postcondition assert：ANF 后遍历全部 Call/RuntimeCall/BinOp 参数，断言每个 heap 型参数为 `IrExprKind::Var` | 违规时 debug-mode panic | 2a |
| 2c | 加 ANF 专用 spec test：每位置的 nested heap expression | `spec/lang/anf_lift_test.almd` | 2a |

**Gate**：2b 后，遗漏 case 触发 debug-mode panic，而非静默 leak。

<a id="gap-3-closure-env-offset-verification"></a>

### Gap 3：Closure env offset 验证

| 步骤 | 描述 | 交付物 | 依赖 |
|------|-------------|-------------|------------|
| 3a | EnvLoad emission 加 `debug_assert!(index < captures.len())` | 越界 index panic | — |
| 3b | 加 ClosureVerifyPass（ClosureConversionPass 的 postcondition）：每个 lifted fn 断言全部 EnvLoad index < param env_size / 8 | 注册为 postcondition | — |
| 3c | 加 closure capture fuzzer：随机 capture pattern（0–20 变量、混合类型、nested） | `tests/closure_env_test.rs` 中 proptest | 3b |

**Gate**：3b 后，编译时捕获 offset 不匹配（debug build）。

<a id="gap-4-concretizetypes-postcondition-coverage"></a>

### Gap 4：ConcretizeTypes postcondition 覆盖

| 步骤 | 描述 | 交付物 | 依赖 |
|------|-------------|-------------|------------|
| 4a | 加简单 `resolve_node_ty` case：StringInterp→String、Clone→expr.ty、Deref→inner、Range→List[Int] | 返回 None 的 variant 从 24 减至约 15 | — |
| 4b | postcondition audit 加每 variant 访问 counter | CI log 显示未访问 variant（覆盖盲点） | — |
| 4c | audit 对非白名单 Unknown 类型改为 hard error | 非白名单 variant 中 Unknown＝编译错误，非静默 fallthrough | 4a, 4b |

**Gate**：4c 后，产生未列入白名单 Unknown 的新 IR variant 使 build 失败。

<a id="gap-5-perceus-lean-rust-conformance"></a>

### Gap 5：Perceus Lean→Rust 符合性

| 步骤 | 描述 | 交付物 | 依赖 |
|------|-------------|-------------|------------|
| 5a | 差分 test：IR 序列化 JSON，运行 Lean `perceusTransform` 和 Rust `perceus_fnbody`，比较 Inc/Dec 位置 | `tests/perceus_differential_test.rs` | — |
| 5b | 扩大 proptest 覆盖：closure capture、mutable 再赋值、nested match | `perceus_verified.rs` 中增加 proptest strategy | — |

**Gate**：5a 提供机械符合性证据。完整提取（Lean→Rust codegen）是长期愿景，不是近期步骤。

---

<a id="priority-order"></a>

## 优先顺序

先做低成本高杠杆的事：

1. **2a + 2b** — ANF needs_lift() 修复 + assert。以天而非周计，封堵静默 leak 风险。
2. **3a + 3b** — Closure offset assertion，容易添加。
3. **4a** — ConcretizeTypes 简单 case，直接。
4. **5b** — 扩大 Perceus proptest，低成本增量。
5. **1b-1g** — WASM emitter 迁移，工作量最大，单独跟踪。

<a id="11-status-snapshot-2026-05-30--closure--where--fold-inference-round"></a>

  # 11. 状态快照（2026-05-30）— closure / where / fold 推导轮次

  本轮关闭的缺口：
  - **WASM closure-table 回归缺陷**：ANF deny-list 反转（5cae928d）把 `Ty::Fn` lambda 纳入 lift，遗漏 `pre_scan_closures` func-table 注册，使 `call_indirect` 引用未声明 table 0 → 无效 WASM。在 `needs_lift` deny-list 加 Lambda/ClosureCreate 后恢复（802bb944）。
  - **§1（Inference↔Codegen contract）两个实例在 checker 侧根治**：
    1. `test where greet = (name) => ...` 的 override lambda param 变 Unknown → checker 将 override 值与被遮蔽 fn signature unify（88993d58 / 25a6f341）。
    2. 对未解析类型变量作 subject 的 `match`，`some(stack)` 等内层 pattern 变量变 Unknown（fold accumulator `some([])`）→ `bind_pattern` 在 subject 为 `Ty::TypeVar` 时，先与 `Option/Result/List[?inner]` unify，再绑定（e8d44dcb）。
  - **移除 anti-pattern**：删除 lowering 侧 `patch_lambda_params_from_checker` / `patch_lambda_from_fn_sig`（930bd486）。checker 成为 source of truth，推进“Lowering never runs inference”。剩余：`lower_where_call_response` 同类 patch 仍在（确认 CallResponse 在 checker TypeMap 反映后可移除）。
  - spec POSTCONDITION：5 → 0（wasm/native 均是）。

  剩余完整性缺口（本轮未开始，需单独深入处理）：
  - **将 §5 提升为保证**：postcondition 在 release hard-fail。目前 `hard_fail = cfg!(debug_assertions)`（pass.rs:248），release 仅打印后放行。注意直接改 hard error 会破坏兼容：如下 under-constrained 等“目前 warning 放行的程序”全成 compile error。先测全部 spec/ 受影响数量。
  - **明确拒绝 under-constrained**：用 `ok([])` 作 fold init、从未出现 err 值时，err 类型 `E` 原理上无约束；当前触发 POSTCONDITION 但仅 warning。要“歧义即类型错误”，需设计 constraint solving 后的可达性分析边界：该 TypeVar 真无法决定，还是以后会决定。应与 §5 提升同时进行。

<a id="12-status-snapshot-2026-06-03--fan-cross-target-codegen--semantics"></a>

  # 12. 状态快照（2026-06-03）— fan.* 跨 target codegen 与语义

  本轮关闭缺口（build-correctness：well-typed fan source → 正确 native binary）：
  - **native fan thunk codegen 的 E0308 / E0277**：向 race/any/settle 传多个不同 capture thunk，生成 Rust 把不同 closure 类型放进 `Vec<impl Fn>`，触发 E0308（“两个 closure 不会同型”）。向 fan.map 传 closure VALUE（uniform repr 的 `Rc<dyn Fn>`，!Send/!Sync）触发 E0277。WASM table-dispatch 两者都正确，value-blind WASM-only spec 隐藏了 native build 失败。修复（20739311）：
    - box path 不再 unbox fan thunk，race/any/settle 改用 `Box<dyn Fn() -> _ + Send + Sync>` box。`Box<dyn Fn + Send + Sync>` 自身为 `Fn + Send + Sync`，因此 runtime `Vec<impl Fn + Send + Sync>` sig 不变，异质 capture thunk 统一为单个元素类型。IR `RcWrap` 加 `wrap: FnBox`（`Rc` | `BoxSendSync`）。
    - fan.map 接收 `Rc<dyn Fn>`，逐次执行，不建 thread。closure VALUE 由此通过（失去并行，但结果相同）。
  - 新增 capturing-thunk 值验证 spec（baac55c3）。CI `almide test spec/ --target rust` 永久覆盖 native 编译路径，复发可检测。

  剩余完整性缺口（本轮未开始，不是 build 而是跨 target **语义**差异）：
  - **fan.race / fan.any 胜者跨 target 不等价**：native thread 取 wall-clock 最快（非确定）；WASM 按 list 顺序（race 仅运行 fns[0]，any 取 list 顺序首 OK）。同 well-typed source 可返回不同胜者。目前仅以 `assert(a or b)` 允许回归测试非确定性。正确方向是统一两 target 的确定语义（如保留并行执行，按 list 顺序取首 OK）。
  - ~~**fan.timeout 在 WASM 为 no-op**~~：**已解决（0.29.0）**，从语言删除 fan.timeout（C-006 flip、E027 tombstone）。deadline 在 host 边界施加。
  - **fan.map 的 err 行为不同**：native `.unwrap()` panic，WASM 包装 err 并传播到 enclosing fn 外（return）。返回 err 的 thunk 造成观察差异。
  均可 build、可定型，却造成“同源码跨 target 不同行为”的等价性缺口。适合放在 [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) 框架处理。

<a id="13-status-snapshot-2026-06-03--cross-target-program-termination-on-unhandled-error"></a>

  # 13. 状态快照（2026-06-03）— 未处理错误时的跨 target 程序终止

  effect / main / Option / Result“类型成立，但结束行为棘手”。**未处理错误**抵达 `main` 时，在两 target 实测 8 例（`almide run` 对比 `build --target wasm` + wasmtime）：

  | # | 场景 | native | wasm |
  |---|---|---|---|
  | 1 | main：effect err auto-`?` 传播 | exit 1, stderr `Error: "kaboom"` | **exit 0，无输出** ❌ |
  | 2 | main：`boom()!` 解包 `Err` | exit 1, stderr `Error: "kaboom"` | **exit 0，无输出** ❌ |
  | 6 | main：`list.first([])!`（解包 `None`） | exit 1, stderr `Error: "none"` | **exit 0，无输出** ❌ |
  | 7 | 嵌套 effect err 传播 | exit 1, stderr `Error: "deep"` | **exit 0，无输出** ❌ |
  | 3 | `boom() ?? 99`（处理 `Err`） | got 99 | got 99 ✅ |
  | 5 | `list.first([]) ?? 42`（处理 `None`） | got 42 | got 42 ✅ |
  | 4 | effect ok | got 7 | got 7 ✅ |
  | 8 | 非 effect `fn main` | plain main | plain main ✅ |

  **应统一语义（目标）**：**未处理错误**（auto-`?` 传播／`!` 解包的 `Err`、`None`／嵌套 effect 传播）到达 `main` 时，**两 target 都**“**exit code ≠ 0** + **stderr 错误信息**”。`??` / `match` 已处理错误在两 target 值一致（如实测，此处健全）。

  **根因**：native `effect fn main` → `Result<(),String>` 由 Rust `Termination` trait 处理（err → stderr + exit 1）。wasm `_start = __main_runner` **丢弃** `main` 的 `Result`，吞掉 err，**失败看似成功（exit 0，无输出）**，是最危险差异。

  **✅ 已修复（Result 错误路径）**：决定遵循 WASI/POSIX（成功 exit0／失败非零+stderr），格式统一用 Display，去掉 Rust Debug 的 `"引用符"` 毛病（引号）：
  1. **native**：`effect fn main` 重命名为 `__almide_main`，生成 `fn main` wrapper，用 `eprintln!("Error: {}", e)` + `exit(1)` 输出 `Err`（`walker/mod.rs`）。输出 `Error: "<msg>"` → `Error: <msg>`（去引号，方向正确）。
  2. **wasm**：`__main_runner` 检查 effect main 的 `Result` tag（`[tag:i32@0][payload@4]`，tag≠0=`Err`），若 `Err`，向 fd 2 输出 `Error: <msg>\n` + `proc_exit(1)`（`emit_wasm/mod.rs`）。非 effect `fn main`（`Unit`）无 tag，故**仅 effect**（`is_effect`），否则误把 ok 当 err，所有 wasm main 都坏。
  - 验证：`tests/wasm_runtime_test_parts/p3.rs::{unhandled_main_error_terminates_consistently,
    successful_main_exits_zero_both_targets}`。8 例实测中 7 例 native==wasm（exit/stderr 一致）。

  **残留差异（另一 bug）**：以 `!` 解包 **Option `None`**（`list.first([])!`，例 6），wasm 仍不能将 `Err` 传播至 main `Result`（变成 `Ok`），exit 0 静默忽略。这不是 `__main_runner`，而是 **`!`/Option-`None` 的 wasm 传播 lowering 上游 bug**。Result `Err` 传播已按上述统一。

  与 §12（fan race/timeout/map-err）同为“build 成功、观察不等价”。广泛检测的正途是两 target 对观察输出（stdout/stderr/exit）作 byte 比较的差分 gate，是 cross-target equivalence 的 rank 1 force-multiplier。`tests/wasm_runtime_cross_target.rs` 中 spec/wasm_cross stdout harness 为部分实现（仅 stdout；本节两测试先行比较 stderr/exit）。

<a id="14-cross-target-divergence-burndown-map-2026-06-04--面的ハントの結果"></a>

  # 14. 跨 target 差异清理图（2026-06-04）— 广泛搜寻结果

  **Gate 已存在**（#352）。`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` 在 native+wasm 运行 `spec/wasm_cross/*.almd`，**byte 比较 (exit, stdout, stderr)**，由 `// @xt-allow: <理由>` 跟踪差异（当前 1 项＝float 最短往返；理由即原因）。native 为 oracle。因此核心不是建 gate，而是 **清理 `@xt-allow` 债务**（与 traversal-totality LEGACY_DEBT 同类 ratchet）。

  **广泛搜寻（61 个并行 agent、169 probe、逐项独立复现）发现 46 个 native↔WASM 真实 bug，不只 fan。** 去重为 **8 个根因 cluster**：

  | # | cluster（根因） | 数量 | 优先级 | locus |
  |---|---|---|---|---|
  | A | WASM 字符串用**字节 index**，native 用**码点**（chars/slice/reverse/take/codepoint/pad…，reverse 生成无效 UTF-8） | 9 | 高 | `emit_wasm/rt_string*` |
  | B | WASM `int/float.parse` 宽松 + **未检查溢出**（返回 `ok(ゴミ)`，即垃圾） | 8 | 高（静默错误值） | wasm parse runtime |
  | C | WASM `float.to_string` 在 `\|x\|≥2^63` **trap**，丢 `-0.0` 符号，to_fixed 舍入不同 | 4 | 中高 | wasm float fmt |
  | D | 复合元素相等性 WASM 为**指针同一性**，native 为结构相等（map/set 复合 key、nested list.contains/dedup） | 4 | 高 | wasm element-eq |
  | E | WASM list **不检查边界 → OOB heap 读取／破坏**（slice/insert/remove_at/swap） | 5 | **高（安全）** | wasm list runtime |
  | F | map.map/set.map 改类型 closure **trap**（indirect call 类型不匹配） | 2 | 中 | wasm closure dispatch |
  | G | **fan.\***＝§12。WASM 仅跑 `fns[0]` 的逐次 stub，native thread::scope。副作用集合、胜者、全失败（panic101/trap134/propagate1）不同 | 6 | **先定 contract** | `emit_wasm/calls.rs:1478-1561` + `runtime/rs/src/fan.rs` |
  | H | div/mod by zero（native panic101 vs wasm trap134）、常量折叠 div0（native 编译错误）、Map Display | 3 | 中 | wasm arith/display |
  | A-case | to_upper/to_lower/capitalize：WASM **仅 ASCII**（±32 逐字节），native 全 Unicode（é→É、ß→SS 伸长、Greek/Cyrillic）。A 的对抗验证发现 | — | 中（需 Unicode case 表，重） | `emit_wasm/calls_string.rs` emit_str_case_convert |

  §13（termination）和 §12（fan）均属这 8（+A-case）cluster 的部分。

  **进度（burndown，2026-06-04）**：✅ **E**（#363 合并，OOB 边界检查），✅ **B-int**（#364 合并，parse 溢出／符号／trim + 4 错误字符串），✅ **A**（字符串码点化：4 UTF-8 helper + 全 op 转换，负 n 钳为 i32::MAX，2-arg slice 的 i64::MAX 默认以 i64 钳制修复）。剩余 D / C+B-float / H / F / G(fan) / A-case。

  **fan(G) 须先决定 contract**（§12 深挖）。native/wasm 均偏离规范（native race 只取 Ok，实为 any；wasm 仅 fns[0]）。推荐 **确定的 list-order-first**：两 target 全 thunk 执行 → list 顺序首 OK 获胜 → 全失败统一 Err 到 main-error 终止路径（§13）。并发非确定性消失，native==wasm 显然成立，可进差分 gate。另一方案“true Promise.race（首完成、不论 Ok/Err）”依赖 wall-clock，非确定，无法进 gate。

  **清理程序**（同 LEGACY_DEBT 的 grandfather→drain）：每 cluster 的确定性 repro 加到 `spec/wasm_cross/` → 根因修复 → 去 `@xt-allow`。fan 胜者非确定，raw race 不适合 byte gate；以全失败／副作用集合／顺序有保证的 fan.settle 构造。
  **推荐顺序（价值×易着手）**：**E（安全）→ B（parse）→ A（字符串）→ D（相等）→ C（float）→ H（div/mod）→ F（closure）→ G（fan，决定 contract 后）**。每 cluster 一个 PR。连接 [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) 框架。
