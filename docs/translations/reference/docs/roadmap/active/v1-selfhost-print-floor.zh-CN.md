原文说明：自托管 print 底座，支撑其余自托管工作的可观测性基石
<a id="v1-self-host-print-floor--the-③-observability-keystone"></a>

# v1 自托管 print 底座 — ③ 可观测性基石

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-selfhost-print-floor.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-selfhost-print-floor.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-selfhost-print-floor.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-print-floor.md)

> 原文注记：这是6月14日设计/进展记录，不是当前 runtime 保证或本次执行测试。最初2元素 gather 设计在后段 DONE 改为两次单 iovec write；wasmtime 说法按历史观察保留，不推广到所有版本。早期立即数 offset load/store、PrimHostCall 草图与后来的 handle/address、Op::Prim 设计并存。print 缺失、linkage 待定、基础计划之后有带日期的 print 完成记录；控制流仍为 NEXT。preamble 函数数不变本身不证明安全或无可信代码；封闭底座单列、raw-memory prim 限制、checker 前提、capability 计数和每 arm 均衡仍重要。未编译草图；数值格式化示例省略负数等情况。未重跑实现、test、proof 或 benchmark。


**状态**：设计（2026-06-14）。综合自5 agent 设计流程（先前工作＋代码库盘点＋2个候选底座；裁决步骤被中断，所以这是人工综合的决定）。不替代任何内容，只为第一个自托管切片*具体化* `v1-mir-architecture.md` §4。

<a id="why-print-is-the-keystone"></a>

## 为什么 print 是基石

v1 执行路径（lower→MIR→render_wasm→wasm→wasmtime）如今对 heap-value 和 scalar-call 程序与 v0 字节一致（commits 915fe2ee, 8cb5a093）。但 v0 oracle 比较程序 **stdout/exit**，v1 **没有 print**，所以没有任何计算可端到端观测：scalar 值、控制流、数据都在“盲跑”（仅可单元测试）。**Print 是观测通道**，让其他每项功能都可与 v0 作端到端验证；真实程序也需要输出。因此 `println` 是通向“全部 v0 功能通过 v1 运行”的单项最高杠杆解阻。

连接已存在：`println(s)` lowering 为 `Op::Call{RtFn::PrintStr}`（lower/calls.rs），render 发出 `(call $print_str ...)`，但 `$print_str` **没有函数体**。纪律测试 `handwritten_wasm_runtime_does_not_grow`（render_wasm.rs，baseline 11）禁止向手写 preamble 添加 `$print_str`（那是 v0 陷阱）。所以 print_str 必须**用 Almide 自托管**，通过 v1 编译为 `(func $print_str ...)`。这样 preamble 函数数量不变，纪律测试 PASS。

<a id="the-decision-the-thin-primitive-floor-candidate-a-scoped-to-print_str"></a>

## 决定：限定于 print_str 的薄 primitive 底座（候选 A）

设计了两个候选：

- **A — 薄底座**：新增 MIR primitive op `Load{width}` / `Store{width}` / `HostCall{cap}`（原始内存＋fd_write host import），inline render；print_str（最终全部 stdlib）用 Almide 在其上编写。这是 §4 的理想：最小可信/不可证明表面，“约20 op 的可证明底座”。
- **B — 厚 `host.write_line(s)` primitive**：单 op 内嵌 byte-copy＋iovec＋newline＋fd_write。一个 brick 就能打印，但**扩大手写 WAT 本体**（只是在玩 `(func $` *计数*，可信 WAT *表面*仍增加），把 stdlib 捆入底座 — v0 陷阱，“暂存以后再消解”。拒绝：“现在交付陷阱，以后再削薄”正是项目禁止的拼补。

**让 A 对 print_str 可行的决定性洞察（反驳“A 需要多 brick 控制流”的担忧）**：`print_str` **无需 copy loop、itoa、Div/Rem/比较或一般控制流**。`fd_write` 接受 **iovec 数组**，因此用 **2元素 iovec** 零复制打印：

```
iovec[0] = { ptr = s + LIST_HEADER, len = load32(s, LIST_LEN_OFFSET) }  // the string bytes, in place
iovec[1] = { ptr = NEWLINE_ADDR,    len = 1 }                            // "\n"
fd_write(STDOUT_FD, IOVEC_ADDR, /*count*/ 2, NWRITTEN_ADDR)
```

保留的 iovec 示例注释：首元素是原地字符串字节，第二元素是换行，count 为2。

OS 负责 gather，我们不循环。控制流＋itoa 的顾虑适用于 `print_INT`（数字格式化），**不是** `print_STR`。因此 print_str 的薄底座只需带立即数 offset 的 `load`/`store`＋`fd_write`＋一次指针加法 `s + LIST_HEADER`，即 **scalar 值流＋3个底座 op**，没有循环。

<a id="floor-design-the-trusted-proof-bounded-set"></a>

## 底座设计（可信、证明范围封闭的集合）

新 MIR op 是封闭集合，由全域 match inline render。新增 preamble `(func $...)` 为**零**，所以不碰 open-stdlib baseline-11 ratchet；像 `RC_PRIMITIVE_FNS` 一样作为底座单独计数。它们小、全域、不作决策，因此可证明忠实于 wasm spec，这是 §4.1 目标。

| MIR op | wasm | 不可再还原的原因 |
|---|---|---|
| `PrimLoad{width, signed}` | `i32.load8_u`/`i32.load`/`i64.load` | 读取计算 offset 处的原始内存低于所有高层 op |
| `PrimStore{width}` | `i32.store8`/`i32.store`/`i64.store` | 对偶操作；打包 iovec 所需 |
| `PrimHostCall{cap}` | `(call $fd_write ...)`（现有 WASI import） | wasm 没有 syscall；host import 是唯一 sandbox 出口，由 host 定义，无法用语言内部表达。携带 `Capability`，故仍纳入穷尽 caps 计数 |

源码机制：编译器保留的 **`prim` module**，包含编译器识别的 intrinsic，在 MIR lowering 按**名称** dispatch（与 `println`/`RtFn::PrintStr` 已用的接缝完全相同），而不是 v0 `@wasm_intrinsic`/`RuntimeCall` 路径；后者仅供 v0 emitter，v1 lowering 将 `RuntimeCall` defer 为 `Opaque`/`Const`。checker 用固定 signature 注册 `prim`；`prim.load32(ptr, off)` lowering 为 `Op::PrimLoad{..}`。只允许 runtime source 文件用 `prim.*`（像 AssemblyScript `load<T>`/`store<T>` 的 unsafe 子语言）。

```almide
// print_str.almd (the runtime author writes this; compiled through v1 like any fn)
fn print_str(s: String) -> Unit = {
  let len = prim.load32(s, LIST_LEN_OFFSET)        // read header len
  prim.store32(IOVEC_ADDR, 0, prim.add(s, LIST_HEADER))  // iovec[0].ptr = s+12
  prim.store32(IOVEC_ADDR, 4, len)                  // iovec[0].len
  prim.store8(NEWLINE_ADDR, 0, 10)                  // '\n'
  prim.store32(IOVEC_ADDR, 8, NEWLINE_ADDR)         // iovec[1].ptr
  prim.store32(IOVEC_ADDR, 12, 1)                   // iovec[1].len
  prim.fd_write(STDOUT_FD, IOVEC_ADDR, 2, NWRITTEN_ADDR)
}
```

代码说明：runtime 作者写 print_str.almd，经 v1 如普通函数一样编译。读取 header 长度，将原地字符串 bytes 和换行放入 iovec，调用 fd_write。保留原注释和草图。

<a id="settled-implementation-design-2026-06-14"></a>

## 已确定的实现设计（2026-06-14）

**基础完成**（commits 74cc1fff `Op::ConstInt`、7d82f628 `IntBinOp` emission）：整数 literal 实体化（`fn f() -> Int = 42` → `(i64.const 42)`），整数运算执行（`a + b` → `i64.add`）。`lower_scalar_value`（lower/calls.rs）递归 lowering Var/LitInt/Int-Add·Sub·Mul，子集外 rollback→defer。corpus-wall cert-neutral，因为 scalar 无 ownership。这恰好满足 print_str 的 `h + 4` / `h + 12` 地址计算与 literal 地址所需。

**SOURCE 机制＝PATH A**：选择 bundled stdlib module `prim`，不复用 `@intrinsic`，后者 lowering 为 v1 defer 到 Opaque 的 `RuntimeCall`。Stdlib module 是 bundled `.almd`，由 `bundled_sigs.rs` 提取 signature。`prim.X(..)` 调用通过类型检查，lowering 为 `IrExprKind::Call{Module{"prim", X}}`，原样到达 v1 lowering。步骤：(a) 加 `stdlib/prim.almd` 声明 prim 函数（本体 `= _`）；(b) 在 bundled-module list（almide-lang `stdlib_info`）注册 `prim`；(c) 在 `lower/calls.rs` 截获 `module == "prim"`（于 `lower_pure_module_value_call` 或兄弟函数的 purity gate 前），把每个 `func` 映射为 MIR prim op。

**MIR PRIM OPS**：新增、inline render → preamble `(func $` 计数不变 → 纪律 PASS。封闭集合像 RC_PRIMITIVE_FNS 一样计入底座。MIR 统一 i64；i32 wasm memory 边界由 op wrap/extend：

| prim fn（Almide） | MIR op | wasm render |
|---|---|---|
| `prim.handle(s: String) -> Int` | `PrimHandle{dst,src}` | `(i64.extend_i32_u (local.get $src))` — heap handle（i32）→ i64 地址 |
| `prim.load32(addr: Int) -> Int` | `PrimLoad{dst,addr,width:4}` | `(i64.extend_i32_u (i32.load (i32.wrap_i64 (local.get $addr))))` |
| `prim.store32(addr: Int, val: Int)` | `PrimStore{addr,val,width:4}` | `(i32.store (i32.wrap_i64 …addr) (i32.wrap_i64 …val))` |
| `prim.store8(addr: Int, val: Int)` | `PrimStore{…,width:1}` | `(i32.store8 …)` |
| `prim.fd_write(fd,iov,count,nw: Int) -> Int` | `PrimFdWrite{dst,…}` | `(i64.extend_i32_u (call $fd_write (i32.wrap_i64 …)×4))` — 携带 `Capability::Stdout` |

Cert：PrimHandle/Load/Store 是 ownership no-op（scalar）；为 name witness 定义 `dst` 并使用 operand。**PrimFdWrite 携带 `Capability::Stdout`**，caps fold 必须像 `RtFn::PrintStr` 一样映射到 Stdout，正确要求 print_str 及其 caller 声明 Stdout。op 全程使用 i64 ValueId（scalar model）；`prim.handle` 是 String→Int 桥，令所有地址计算成为 `Int` 的 `IntBinOp`（没有 String+Int 类型错误）。

**print_str.almd**：使用基础 `h + 4` / `h + 12`＋literal 地址；地址与现有 preamble layout 一致：NWRITTEN=0、IOVEC=8、SCRATCH=512。
```almide
fn print_str(s: String) -> Unit = {
  let h = prim.handle(s)            // i64 address of the [rc][len][cap][bytes] block
  let len = prim.load32(h + 4)      // header len
  let data = h + 12                 // byte start
  prim.store32(8, data)             // iovec[0] = { data, len }
  prim.store32(12, len)
  prim.store8(512, 10)              // "\n" at scratch
  prim.store32(16, 512)             // iovec[1] = { 512, 1 }
  prim.store32(20, 1)
  prim.fd_write(1, 8, 2, 0)         // fd_write(stdout, iovec@8, count 2, nwritten@0)
}
```

代码说明：h 是 [rc][len][cap][bytes] block 的 i64 地址；h+4 为 header 长，h+12 是 byte 起点。iovec[0] 是 data 和 len，在 scratch 的512写换行，iovec[1] 是512和1。fd_write 参数是 stdout、iovec@8、count 2、nwritten@0。

**LINKAGE**：本 slice 内待定细节。render_program lowering 单 source 的 `ir.functions`；bundled `prim`/print_str module 自动导入以作类型检查，但本体未必 lowering 进 `ir.functions`。必须包含 print_str 本体：把 `stdlib/print_str.almd` 拼到编译源码，或 linker 在到达 `PrintStr` 时包含 bundled print_str fn 本体。PrintStr CallFn → `(call $print_str)` 连接已存在。

**SUB-SLICES**：(1) MIR prim op＋render＋手构造 MIR `print_str` 测试，真正在 wasmtime 打印 "hi"（独立证明 op＋render，无 frontend）；(2) `prim` module＋lower/calls.rs 截获（source→op）；(3) print_str.almd＋linkage → `println("hello")` 与 v0 字节一致；纪律测试 PASS（自托管，无 preamble 增长）；corpus-wall/gate/cargo test 绿。

**目标（2026-06-14 设置）：“v1 が v0 と同じように動くまで”（直到 v1 与 v0 一样运行）** — 底座＋print 是基石；之后 scalar/控制流/数据都可作 e2e v0 验证，再通过同一底座自托管 stdlib。

<a id="decomposition-campaign-order-toward-all-v0-features"></a>

## 分解（通向“全部 v0 功能”的 campaign 顺序）

1. **Scalar 值计算**（基础、前提条件、defer 的 (a') track）：`Op::Const` 携带 literal 值（`Const{dst, value: Option<i64>}`，`None`=defer/0）。lowering 对 scalar 运算发出 `IntBinOp`，不 defer 到 `Const`；scalar 值贯通（call/prim 结果是真实值而非 `Const(0)`）。现在可用 function-export+call 或 PrintInt fixture 单元测试，print 落地后可 e2e 观测。侵入性：`Op::Const` 在约12处被 match（cert/V/render/lower）；全部维持 ownership no-op（scalar 不携带所有权），因此 ownership cert 字节不变。
2. **`prim` module＋3个底座 op**（`PrimLoad`/`PrimStore`/`PrimHostCall`）：checker module＋按名称 lowering＋inline render＋纪律测试底座计数（像 RC_PRIMITIVE_FNS 一样单列）。
3. 在1+2之上的 **print_str.almd** → v1 编译为 `(func $print_str)` → 现有 `PrintStr` CallFn 解析 → **`println("hello")` 运行且与 v0 字节一致**；纪律测试 PASS（preamble 不变），corpus-wall/gate/cargo test 绿。
4. **解锁可观测性** → scalar 值、控制流等可 e2e v0 验证。接着 `print_int`（itoa → Div/Rem/比较＋控制流 track），再在相同底座自托管 `string`/`list`/`map`/`json`，趋向 §4 的“零手写 wasm runtime”。

<a id="invariants-守る系-must-hold-throughout"></a>

## 不变量（守る系，全程必须保持）

- 底座 op inline render → preamble `(func $` 数不变 → 纪律测试 PASS；底座单独计数（封闭、证明范围有界），不计入 open-stdlib baseline。
- `PrimHostCall` 携带 `Capability` → sandbox/caps 计数仍穷尽，host effect 不会隐形。
- Scalar 值无 ownership → scalar 值＋prim 工作不改变 ownership cert（i/a/d/m）字节；corpus-wall ownership/names/caps ACCEPT 数除经验证恢复外不得移动。
- 自托管 print_str 是普通 `MirFunction`，不新增手写 WAT。

<a id="done-so-far-2026-06-14"></a>

## 迄今完成（2026-06-14）

基石已实现：普通 `println("…")`（v0 相同源码）通过 prim 底座上 Almide 编写并自动链接的 `print_str`，与 v0 字节一致（commits 915fe2ee → b2896c87）。Scalar 值基础完成：`ConstInt`、`IntBinOp` Add/Sub/Mul/Div/Mod。prim 底座：`Op::Prim{Handle/Load/Store/FdWrite}`。print_str 用两次单 iovec fd_write 写 string+"\n"（wasmtime 不 gather 2元素 iovec）。

<a id="next-control-flow-execution--print_int-the-campaigns-next-architectural-slice"></a>

## 下一步：控制流执行 → print_int（campaign 下一架构切片）

这对健全性至关重要（触及 verify_ownership/cert），corpus 不覆盖执行（只有 runtime 会用），所以要专用敌对验证（像 scalar-call slice），最好另作专注工作。

**原因**：`if/match` 目前线性化（两 arm lowering 进 flat op list，值 defer 为 `Const`/`Opaque`）；对 cert 健全但无法正确执行（两 arm 的 effect 都会运行）。`print_int`（递归 itoa）需要真正分支，只执行被选 arm。

**设计**（先 scalar；heap-result if 后续）：
- `Op::If { cond: ValueId, then_body: Vec<Op>, then_val: Option<ValueId>, else_body:
  Vec<Op>, else_val: Option<ValueId>, dst: Option<ValueId> }` — 嵌套 op 本体。
- cond 的比较 `IntOp`：`Lt/Le/Gt/Ge/Eq/Ne`，以 `left.ty == Ty::Int` 为 gate，render 为 `(i64.extend_i32_u (i64.lt_s …))`（i64 比较→i32 0/1→i64）。
- lowering：调整 `lower_branch`，发出每 arm 均衡的 `Op::If`＋arm 值，保持原线性化的每 arm drop 纪律；这是“只执行一个 arm”健全性的关键不变量。
- cert：`verify_ownership` / `name_witness` / `cap_witness` 顺序递归两 arm；这是 corpus 对线性化已证明的同一逻辑，现在用于结构化 arm（复用带来健全性，但 lowering 的每 arm 均衡是不容缺少的前提，须敌对验证）。
- render：`(local.set $dst (if (result i64) (i32.wrap_i64? — cond is i64 0/1, use
  (i32.eqz (i32.eqz (i32.wrap_i64 cond))) or (i64.ne cond 0)) (then <then_body>
  (local.get $then_val)) (else <else_body> (local.get $else_val))))`。
- `defined_value`/`value_reprs`：`Op::If` dst → 它的 repr。V 递归或跳过。
- 然后 `stdlib/print_int.almd`：递归 `put_int(n, pos)` — `if n < 10 then {store8;
  pos+1} else { let p = put_int(n/10, pos); store8(p, '0'+n%10); p+1 }` — 建于 prim＋Div/Mod［完成］＋If［本次］＋递归［完成］。连接 `int.to_string`/PrintInt。

print_int 之后：控制流＋数字可观测 → 将整个 scalar/branch 表面与 v0 作 e2e 验证；再用同一底座自托管 string/list/map/json。
