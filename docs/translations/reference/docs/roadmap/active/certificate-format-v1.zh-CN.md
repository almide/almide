历史描述：逐函数所有权证书格式 — i/a/d/m 字母表、调用/分支/闭包扩展，以及已交付的构件。
<a id="certificate-format-v1--design"></a>

# 证书格式 v1 — 设计

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](certificate-format-v1.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](certificate-format-v1.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](certificate-format-v1.zh-CN.md)

> 非规范性固定原文历史设计及实现台账读者版。原本不变，不自动同步，未独立检查证明、运行corpus或benchmark。开篇OCaml提取架构、退役MODE方案、早期subset唯一性方案、带守卫reuse精化及7月修订作为不同历史层保留，不改写成单一当前设计。brick完成不证明任意生成函数；原文仍保留renderer、生成drop、assembler/engine及kernel信任边界。kernel-as-oracle验证witness性质，不填平整个函数的byte对应。定理/测试数与性能是各阶段当时报告。证书及资格鉴定对象不表示第三方认证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)

> 状态：**设计已接受；brick 1–2 和 5 已交付**（所有权字母表 i/a/d/m；调用 2a/2b/2c — effect 调用、逐调用点 cap、param-mode 签名 + manifest 声明 cap；full mode 5a/5b/5c — 分支一致 `{…|…}`、`b` 字母、闭包 dispatch 签名 + 捕获闭包）。3a/3b/3c + 4a/4b 已在各自 brick 中交付（3c = 树内 raw-byte ⟶ ISA decoder，WasmDecode.v）。**brick 6 已交付**（kernel-as-oracle 门槛 — extraction 不再是 verdict 的信任根）。**v1 brick 阶梯已完成。** Heap-capture 闭包 env 于 2026-07-10 交付（见 5c）；其借用 env load 由 call-mode/capture 记账覆盖，而非 `b`。剩余棘轮（已记录，不是 brick）：rc 片段以外的 A2（整个函数体仍属于 renderer 契约），toolchain 对齐后在 fast path 采用 verified-extraction/CertiRocq。取代隐式“每对象 i/d”格式。基于先例 + 对抗式设计审查（Perceus/Koka reuse、linear/foundational PCC、WasmCert-Coq byte 语义、v0 所有权分类）。

<a id="why-this-exists"></a>

## 为什么需要它

v1 信任论点：**证明 CHECKER，不是编译器**。不受信任的编译器输出逐构建 *witness*；经 kernel 证明的小型 Rocq checker 提取为 OCaml 后重新验证 — `accept(witness, artifact) ⟹ P(artifact)`。核心卖点是资格鉴定对象（checker）只有审计者能读完的**几百行**。

v0 witness（`ownership_certificate`：每对象一个 `i`/`d` 流，由 `check_all` 检查平衡）只能说“括号平衡”。前方三个 gap 要求更多：

- **G1 witness ⟹ wasm bytes** — 将 witness 绑定实际输出 bytes 和 runtime heap machine（`__alloc`/`__rc_dec`/free-list）。
- **G2 frees（real-RC）renderer** — 表达 SHARING（fresh / alias / move）、reuse 和 leak-freedom；i/d 流太粗。
- **G3 slice → 完整语言** — 调用、控制流、闭包、嵌套/递归 heap。

<a id="the-central-tension-and-the-principle-that-resolves-it"></a>

## 核心矛盾及解决原则

witness 若丰富到能表达三者，朴素 checker 也随之膨胀，几百行 core 就会瓦解。解决方法：

> **保持 checker 小巧，不是限制 witness 能表达什么，而是限制 checker 决定什么。** 将 witness“细化至可检查”：每项全局决定（所有权推断、分支协调、reuse 安全性、byte layout）都由不受信任的编译器预先解决为局部具体事实，由 checker 按固定规则检查。checker 从不推断、从不遍历 CFG、从不打开 callee。

checker 大小于是随**规则数量**（事件字母 + subset 实例 + op→wasm 模式表条目）增长，而不随程序大小、语言复杂度或编译器复杂度增长。

<a id="the-format-flat-stream--side-table-ground-facts-only"></a>

## 格式：flat-stream + side-table，仅具体事实

逐构建 witness 是程序级 bundle，由换行/`|` 分隔的平坦文本节组成 — 全是具体事实（自然数、`|` 列表、固定字母表）；**没有节嵌入 Coq 项、高阶结构或“推断 X”指令**。各节由 Rocq 中已证明的两种 parser 形状解析（`parse_go` 逐行 fold；`parse_pair` 按 `|` 分割）。checker 是各节 verdict 的 AND，再加少量跨节*一致性*检查。

<a id="1-ownership-stream--the-v0-core-with-a-richer-alphabet"></a>

### 1. 所有权流 — 字母表更丰富的 v0 core

每个引用计数 OBJECT 一行，每个事件一个字符；字符带有现有 fold 处理的有符号 delta，而*字母本身*是编译器已决定的具体事实：

| 字母 | δ | 具体事实 | gap |
|---|---|---|---|
| `i` | +1 | FRESH 获取（Alloc / fresh Dup / owned Call-result） | — |
| `a` | +1 | ALIAS 获取（inc 已有 SHARED ref）— share-vs-move | G2.1 |
| `d` | −1 | 普通释放（Drop） | — |
| `m` | −1 | MOVE-OUT（Consume → container / return / consuming callee） | move≠drop |
| `r` | −1 | 可 REUSE 的释放（已证明唯一性的 drop） | G2.2 |
| `b` | 0 | 活的所有权引用借用（不改计数；要求 `rc > 0`） | G2.3 |

checker 的逐行规则是 `proofs/OwnershipChecker.v` 中带守卫的 fold：`i|a ⇒ +1`；`d|m ⇒ (if rc≤0 reject else −1)`；`r ⇒ (if rc=1 then 0 else reject)`；`b ⇒ (if rc≤0 reject else rc)`。从 `0` 开始，仅接受无 fault 且以 `0` 结束的运行。Alias 和 MoveOut 复用增减平衡规则；Reuse 额外检查唯一性（`check_reuse_sound`），Borrow 检查存活性。借用参数/env load 通过 call mode 记账，而非此事件。两者都不是不受限制的 delta-only 事件。**v0 的 `i`/`d` 是退化情况**。

~~在 bundle 前加 `MODE` byte（`eager` | `perceus` | `full`），选择合法字母~~ — **已退役（brick 5b 决定，2026-07-08）**。字母表本身可以自守卫：`r` 的义务由 fold 自身解除（当且仅当 rc = 1 有效 — `check_reuse_sound`），`b` 自带存活性守卫（rc = 0 时 fault）。所以 witness 在结构上不可能“在义务存在前声称 reuse”；义务逐事件强制，而非逐 bundle。MODE 前缀会成为 checker 无法重新推导的编译器断言（违反具体事实原则）；配备级字母政策（“该运营者禁止 perceus mode”）属于运营者关切，应在 manifest 层（`[permissions]` 风格），而非 witness bytes 中。

<a id="2-side-table-sections--all-the-one-subset-law-subsetv"></a>

### 2. Side-table 节 — 全部复用同一个 subset 法则（Subset.v）

`subset_check sup sub = forallb (fun x => mem x sup) sub` 以多种方式实例化 — **一个证明（`subset_check_sound`）覆盖全部；添加属性只是添加命名，不是添加证明**：

- `used ⊆ defined` — 名称全域性。
- `used-caps ⊆ allowed` — capability 上限。
- **`actual-call-modes ⊆ declared-param-modes`** — 调用点与签名（下文）。
- `r-objects ⊆ proven-unique-at-drop` — Perceus reuse 健全性（G2.2）。
- `used-field-offsets ⊆ declared-heap-offsets` — byte layout（G1）。

<a id="3-call-signatures--compositionality-the-whole-language-lever"></a>

### 3. 调用签名 — 可组合性（通往完整语言的杠杆）

各函数有一行 SIGNATURE：逐参数 mode（move/borrow）、return mode（move-out/borrow）、声明 effects/caps。在调用点 `call fn_id : actuals`，checker 查找 `fn_id` 签名，用 subset 规则检查 `actuals` — **从不打开 callee**。检查 N 次调用是 O(N) membership，与 callee 大小无关。闭包是签名中包含捕获 env 所有权的值。这让 call / closure / recursion 以“一条规则 + 一种签名”进入，保持 checker 平坦。→ G3.1/G3.3。

<a id="4-byte-binding--table-now-semantics-once-g1"></a>

### 4. Byte-binding — 现在用表，语义只证明一次（G1）

逐 op：固定 `op → wasm-instruction-pattern` 转换表的 tag，加上 layout 常数节（`rc_offset`、`free_list_offset`、`heap_ptr_global`）。逐构建检查是输出 bytes 对表的**语法**匹配（便宜，在小型 checker 内）。**语义**半边 — runtime 函数（`__alloc`/`__rc_dec`/free-list）精化抽象 ±1 op — 针对 wasm memory-model library 证明一次（重但可摊销，需要 WasmCert-Coq）。`R(M,w)` = “M 的 bytes 与 w 的 op 对应表匹配”；`V'` 是逐构建表匹配器。→ 关闭 G1.2 的“Dec 是抽象 token”，不用每次构建重新证明 runtime。

<a id="the-checker-size-invariant--and-the-tripwire-to-guard"></a>

## Checker 大小不变量 — 必须守住的警戒线

> **不变量。** 每条 checker 规则只能是：(a) 一行有符号 delta 的 left-fold；(b) 对具体事实 `|` 列表的 `mem`/`subset` 查询；(c) 与固定转换表做语法 pattern-match。大小 ∝ 字母数 + subset 实例数 + 表条目数。

> **警戒线（core 瓦解的时刻）。** checker 禁止打开 callee、遍历 CFG / 跟随控制流、求解 meta-variable / 执行推断。某规则一旦需要这些操作，就是全局决定泄漏进 trusted base — 将其推回编译器输出的具体事实。

这就是“整理成可检查形式的竞赛”的精确形式：通过增加节购买表达力（各节有自己的内化 parser 和复用共享 Subset/Balance 法则的健全性引理），而不是让规则更聪明。

<a id="migration-from-id"></a>

## 从 i/d 迁移

`i`/`d` 是 v1 字母表的**退化实例**。`OwnershipChecker.v` 保留平衡检查架构，以新事件扩展 `exec` 和解析。Alias/MoveOut 复用旧 delta 语义；Reuse/Borrow 添加唯一性/存活性守卫，并需相应证明分支（见下方 brick 4、5b）。原 `i`/`d` 流在扩展字母表下仍有效。

<a id="build-order-each-step-gate-green-dual-oracle-ratchet-holds"></a>

## 构建顺序（每步 gate-green，维持 dual-oracle 棘轮）

1. **所有权字母表（eager: i/a/d/m）。** ✅ **已交付。** `a`（alias）、`m`（move-out）是具体事实；`exec` 是四分支 fold；`eager_copy_refines_safety` 从“Dec-free”推广到“increment-only”（MoveOut 也是 −1）。`return_list.almd` witness 从 `id → im`。proof spine + gate + 31 项测试 + CI green。
2. **调用。**
   - **2a：effect 调用。** ✅ **已交付。** `lower.rs` 将 `println(s)` lower 为借用活字符串的 `Op::Call{PrintStr}`。真实打印程序穿过 PCC 链（所有权 `id` ACCEPT），且 **capability witness 来自真实源码**（`used=[Stdout]`）— 因未声明，被 cap bound REJECT（`|0`）：sandbox 承诺捕获真实 host effect。
   - **2b：逐调用点 capability subset 规则。** ✅ **已交付。** `lower.rs` 将用户调用 `beep()` lower 为 `Op::CallFn`；编译器把各 callee 可达 cap fold 进 caller（`reachable_caps`，在调用图上传递），已证明的 `check_caps_cert` 再验证 `reachable ⊆ declared`。因此没有直接效果的 `main` 也会因仅经 `beep` 到达的 Stdout 被 REJECT（`tcaps` witness `|0`）。checker 从不打开 callee，只做 subset（Subset.v 杠杆，零新 Coq）。关闭 direct-only caps gap。诚实范围：编译器 reachability fold 仍是逐构建信任项（逐 edge 验证、未知 callee 保守 reject 是加固措施）。
   - **2c：所有权 param-mode + manifest 声明 cap。** ✅ **已交付。** (i) `proofs/CallModes.v` — SIGNATURE 节（§3）：逐函数声明 heap-param mode（borrow | move，以自然数表示）+ 逐调用点 actual mode；`check_modes_cert` 逐 site 验证位置一致（未知 callee 保守 reject，每 site 一次自然数列表相等检查，checker 大小不变量保持）。合成法则 `check_fill_sound` 使其健全：已接受的 caller 流（call site 缩约为声明 mode marker：borrow = 无事件，move = `m`），在任何满足声明 mode 的 callee 流内联下（`param_stream_ok`、Reuse-free + shift-invariant），都仍无 double-free/leak。因此逐函数 cert 可组合，而 checker 无需打开 callee。非空洞：caller 认为 borrow/callee 认为 move 的组合（单独均平衡，内联则 double-free）正是一致规则禁止的情形（`disagreement_double_frees`）。Emitter：`call_modes_witness`（certificate.rs；带点 self-host callee 遵循 renderer 契约并省略，与 caps graph 的已知类别政策相同）；gate 行：手工 agree/mismatch + 真实源码 `two_functions` / `heap_arg_call`（实际传递 heap 参数）。两定理均 axiom-clean、经 coqchk、已入台账。当时 lowering 仅 borrow，因此输出 mode 全为 `0`；将来消耗型 move 约定落地时，签名与 site 必须同时切换，否则构建拒绝 — 这种一致现已机器检查。(ii) Manifest 声明 cap — ACCEPT 情形：`apply_manifest_caps` 将 `effect fn` 声明上限精化为运营者 `almide.toml [permissions].allow`（由 `manifest_caps` 投影到 Capability registry；pure fn 保留 ∅，manifest 永远不能授予 effect system 拒绝的能力）。gate：同一个真实打印程序在 `allow=["IO"]` 下 ACCEPT，在 `allow=["Rand"]` 下 REJECT — 首个非空洞 cap bound（此前 all-caps effect 默认值永远无法拒绝 effect fn）。2c 之外的后续精化：签名中的 return-mode 字母（当时 owned-heap-result 约定统一）、闭包签名（brick 5）、细粒度 `FS.read` 风格 manifest 词汇（effect-system-capability Phase 1）。
3. **Byte-binding：op→wasm 模式表。**
   - **3a：表 + 逐构建匹配器。** ✅ **已交付。** `proofs/Translation.v` 将 op→wasm-instruction 表形式化为 Coq 对象（形式 byte-binding `R(M,w)` — 关闭 G1.1/G1.3/G1.4），并证明 eager-mode 安全性实例（复用 ALS）；`translation_validation.rs::validate_translation` 在真实 WAT 上逐构建再检查每个 op 的 pattern 均存在且没有 `rc_dec` — 比单纯 Dec-free 扫描更严格（能抓住丢弃 op 的 renderer）。诚实范围：存在性检查，不是精确逐 op byte-window 双射。语义实现（runtime memory machine：`call $rc_dec` 如抽象 −1 那样改变 free-list）是 runtime-memory-model + WasmCert-Coq library — **G1.2，最难的单块**（3b）。
   - **3b：runtime memory model（抽象 memory 半边）。** ✅ **已交付。** `proofs/RuntimeModel.v` 将 runtime heap 建模为线性内存状态机 — 对象 refcount 位于 `base + RC_OFFSET` 的 CELL，`rt_inc`/`rt_dec` 是具体内存写入 — 并证明 `mrun_tracks_exec`：cell 与抽象 refcount（`OwnershipChecker.exec`）完全相同地演进、精确同步 fault。推论 `balanced_cert_no_memory_fault`：已接受证书（从 rc 0 平衡）由内存中永不 double-free 的机器实现。抽象 Dec 不再是悬浮 token，而是绑定具体内存操作（两定理 axiom-clean、经 coqchk 验证）。
   - **3c：raw-byte ⟶ ISA 绑定（G1.2 最后一段，树内）。** ✅ **已交付（2026-07-09，`proofs/WasmDecode.v`）。** rc opcode 子集的已证明 DECODER `decode : bytes → option (list instr)`（未知 opcode / malformed memarg / 非零 global index / 缺少 `end` ⇒ None — 保守 reject），将真实 assembler bytes 绑定 WasmIsa 关系式 small-step 语义：`decode rc_inc_bytes` 和 `decode rc_dec_real_bytes`（两者均由 check-wasm-bytes.sh 逐构建接地到 wat2wasm，包含 renderer 实际形状和 `p + 0` 加法）给出这样的程序：每次 `irun` 归约都带有已证明效果 — `rc_inc_bytes_isa_effect`（cell +1）、`rc_dec_bytes_isa_traps`（double-free 在 byte 层规格中 STUCK）、`rc_dec_bytes_isa_frees`（唯一引用释放将 cell 归零并接入 $freelist）。链闭合：真实 bytes ⟹（已证明 decode）ISA program ⟹（已证明 irun）memory effect。诚实残余（已记录，与 WasmCert-Coq 自身同类）：wasm ENGINE 忠实于 ISA 规格（check-wasm-exec.sh 中由 wasmtime 逐构建交叉检查）、assembler 接地、kernel；rc primitive 以外的整个函数体仍是 §3 renderer 契约。完整 WasmCert-Coq IMPORT 在树内仍不可行（针对旧 Coq，不是 Rocq 9）— 树内 ISA + decoder 是已接受架构（WasmIsa.v header）。
4. **perceus mode（`r`）+ leak-freedom。**
   - **4a：`r` 事件 + 内存级 leak-freedom。** ✅ **已交付。** `OwnershipChecker.v` 增加 `Reuse`（`r`）— 可复用释放（perceus mode），如 `−1` 一样 fold，因此逐字复用 `check_sound`（零新证明，与 `a`/`m` 相同）；`RuntimeModel.v` 证明 `balanced_cert_frees_in_memory`：已接受证书使 runtime cell 留在 **0 — 已释放，而非泄漏**。因此 leak-freedom（`check` 的“以 0 结束”部分，witness 层已由 `check_sound` 的 `no_leak` 证明）现在绑定真正释放的 MEMORY — eager-copy renderer 做不到（不输出 release），输出 release 的 renderer 则实现它。提取 checker 端到端处理 `r`（`build-checker.sh` perceus demo）。**完成（4b）reuse 健全性 — `check_reuse_sound`**：不用 subset 节（否则信任编译器断言的“proven-unique”集合，是 checker 无法重推的推断），由 FOLD 解除唯一性义务。`exec` 的 `Reuse` 分支收紧为当且仅当 `rc = 1` 有效，因此 SHARED 对象（rc > 1）的 Reuse 会 FAULT。checker 从自己的计数导出唯一性，更简单且严格健全。`RuntimeModel.step_mem`/`Termination.fuel_exec` 保持 lockstep（`rt_reuse`）。关闭的漏洞 `iard`（复用 shared 对象但平衡的 cert）现于 Coq（`cert_shared_reuse_rejects`）和提取 checker（`build-checker.sh shared_reuse.cert`）两层 REJECT。**完成（A1.2 证明基础）— `FreeList.alloc_not_live`**：建模 free-list allocator（bump + free-set + ghost live-set）并证明 REUSE-SAFE — 有效分配（fresh frontier 或 free-list block）从不返回当前 LIVE block（no reuse-after-free）；alloc/free 的 INV 保留将其提升到完整运行。这将 A1.2 分叉解决为 PROVE：输出物理 free-list 的 renderer slice 精化此模型，而非增加 trusted runtime。**完成（A2 首个 slice）— `WasmRcDec.rc_dec_prog_realizes_rt_dec`**：renderer 输出的精确 `$rc_dec` 指令树，以 RuntimeModel `Mem` 上的小 op 语义（load/add/sub/store/trap）建模为数据，可证明计算 `rt_dec` — 同一 trap、同一递减。因此 release 语义已在指令树级证明（剩余 A2 gap 仅 raw-byte ENCODING — assembler / full WasmCert-Coq ISA）。proof spine = **25 个 axiom-clean 定理**。**完成（A1.3 cow 安全性）— `CowSafety.make_unique_yields_unique`**：clone-on-shared 纪律经建模、证明产生唯一所有的 block，因此原地修改不破坏任何 alias（aliased-mutation 类无法发生）；sharing renderer 的 cow 精化它。proof spine = **27 个 axiom-clean 定理** — A1 的安全性类别（leak / double-free / reuse-soundness / reuse-after-free / aliased-mutation）现全部已证明。**完成（A1.2-render）— 物理回收已实现**：rc→0 时 `$rc_dec` 将 block 归还 free-list，`$alloc` 复用 exact-size head，精化 `FreeList`；double-free sentinel 保留（link 放在 dead len field，不是 rc cell），经 wasmtime 验证（reuse `p1==p2` + double-free trap + value-semantics byte 不变）。**完成（A1.3-render）— SHARING + cow 已实现**：`Dup` 通过 `rc_inc` 共享（不复制），`MakeUnique` 是 cow（clone-on-shared，先 `rc_dec` 以免 temp），精化 `CowSafety`；rc cell 实际追踪抽象 refcount（1→2→1→0），运用已证明 rc machine，输出 byte 不变。**A1 实现 real-RC sharing、cow、free-list 回收及 sentinel。** 已建模 RC routine 有 refinement proof，但这并不消除对生成 drop routine、整个函数 lowering 或 renderer 的信任。当前 C-PROVEN/C-FAITHFUL 边界见 `proofs/receipt.sh`。剩余（性能/encoding，不是安全性）：A1.2 size-class/遍历（当时 exact-size head-match）；A2 raw-byte encoding。`rc_dec`/`rc_inc` 已完成（A1.1b / A1.3-render）。
5. **full mode：分支一致 + `b` + 闭包签名。** ✅ **已交付（2026-07-08）。**
   - **5a：分支资源状态一致（`CBranch`，format v4）。** `OwnershipChecker.v`：`CertItem` 增加 `CBranch thenb elseb` — 当且仅当两个平坦 arm 本体都从入口计数无 fault 地到达相同结果才接受（一致，而非保留：净 +1 arm 是 heap-result 分支）；`UL_branch` 展开为 runtime 实际选择的一个 arm，核心 `check_line_unroll_sound` 陈述不变。parser `parse_bc`（format v4，`{ then | else }`）是 `parse_clc` 的超集 — 每个 flat/CLoop/CCondLoop cert 解析 byte-identical。Emitter：逐 `IfThen`/`Else`/`EndIf` region 缓冲 arm 事件；各自平衡的 arm 输出 FLAT（byte-identical，所有既有 cert 零改动），不平衡则分组 `{then|else}`。因此 CROSS-ARM COMPENSATION（一 arm 的 `i` 由另一 arm 的 `d`“平衡”：路径依赖 leak/double-free，以前 flat-ACCEPT）现在结构性拒绝，lowering 的逐 arm 平衡承诺不再是受信任惯例。`verify_ownership` 增加同样分支 JOIN（每 arm 入口状态 snapshot + 每对象一致 + `BranchDisagreement`），退役平坦的双 arm fold。**分组在首次接触就发现真实 leak**：effect-fn 尾部 `if err then err(msg) else ok(total)` 在 error path 将错误累积器 move 进 Err block，却从不在 ok path 释放（`{m|}`，每次 happy-path 调用泄漏一个空字符串 block；flat cert 无条件计入 `m`）。根修复：`lower_heap_result_if_inner` 强制 RELEASE PARITY — 一 arm move-out 的外层 handle，在 sibling arm 加补偿 `Drop`（`{m|d}`，两 arm 一致）。spec 280/280，corpus 26,976 objects ACCEPT。
   - **5b：`b`（borrow，+0）字母。** `exec` 增加 `Borrow` — 无 delta，存活性守卫（rc ≤ 0 时 fault）；owned-object use-after-free 现在可由 witness 表达（`idb` REJECT；此前 `Op::Borrow`/`MakeUnique` 对 cert 不可见）。扩展每个逐 op 引理（`exec_app`、`reuses_unique`、Termination fuel interpreter、ALS increments-only、RuntimeModel memory machine 的 `rt_borrow` — live-read 与抽象语义一起 fault — 及 CallModes 的 `exec_shift`/`exec_fill`：`b` 是 shift-safe，故可跨调用边界组合）。Emitter：对 stream 持有所有权（有 +1）的对象，`Op::Borrow`/`Op::MakeUnique` 输出 `b`；直接使用借用 param 仍无事件，其存活性是 CALLER 的义务，由 mode 一致解除（`bare_borrow_body_needs_own_ref` 记录理由：零初值 param stream 上的 `b` 会 fault）。500-seed differential floor 现真实覆盖 `b`。MODE-byte 问题已解决：退役（见 §1 — 字母表自守卫，政策属于 manifest 层）。
   - **5c：闭包签名（G3.1/G3.3，可检查 slice）。** `call_modes_witness` 不再跳过 `Op::CallIndirect`：funcref 值只能来自 `Op::FuncRef`，因此 site 的 POSSIBLE-CALLEE 集合 = 程序 FuncRef 表中 param 形状匹配 dispatch 的 target（arity + 逐位置 heapness；其他 target 在 `call_indirect` 类型 gate trap，fail-stop）。witness 每个 possible callee 输出一行一致性，因此已证明 `forallb site_ok`（`check_modes_cert_sound`）就是该集合上的 Forall 提升 — 零新 Coq 表面。空/不可见集合（从未 lower 的 FuncRef target）→ 越界 sentinel → 保守 REJECT。真实源码 gate 行：`funcref_call.almd`（返回的 lifted funcref 通过 CallIndirect dispatch；lifted `__lambda_*` 辅助函数纳入 witness program）。**后续已交付（2026-07-09）：捕获闭包。** 第一类函数值现统一为 CLOSURE BLOCK — heap 上的 `[rc][len][cap][fnidx][captured…]` DynList；call 从 slot 0 load fnidx，并把 block 作为首个借用 env 参数传入，lifted lambda prologue 读回 capture（`fn adder(n) =
     (x) => x + n` 返回/绑定/dispatch，v0-byte-identical — closure_capturing_wasm.almd；corpus in-profile 4,693 → 4,745，walls 317 → 306）。零新 op / render 规则 / checker 规则：block 是普通 owned heap 对象（`i`/`d`/`m` cert），env 是普通借用 heap param（CallModes 一致覆盖闭包边界 — possible-callee 展开现在匹配 env+args 形状）。**CLOSURE ENV FULL MODE 于 2026-07-10 交付**：capture 包括 HEAP 值（精确一层种类 — String / List[Int] / List[Float]）及 Fn 值（闭包捕获闭包 — `compose`）。heap capture 是共同所有：创建时 `Dup` + move-in（caller 对象的 cert `a`+`m`；CowSafety 使 share 具有值语义安全性），prologue 以借用 handle 读回（`LoadHandle`，param 纪律）。block 自描述 — slot 0 fnidx，slot 1 = n_heap | n_closure<<16。因此统一的 GENERATED-ALMIDE `$__drop_closure`（render pipeline 注入，与所有生成 `$__drop_*` 一样在 witness 表面外）可在任意 drop site 释放任意闭包：捕获闭包通过自递归，heap capture 各一次 rc_dec，绝不触碰 fnidx slot。heap-result 闭包 call 现在也在 call 参数位置 lower（`println(hi("world"))`）。v0-byte-identical（closure_capturing_heap_wasm）；gate 行 `closure_heap_capture.almd`（greeter `im|am` + 2-heap-arg dispatch mode 行），kernel 一致。**`b` 消费者问题已解决**：lambda 内 env read 是通过零初值借用 stream 的 load，按 5b param 纪律无事件。因此 `b` 真正承重的 cert 消费者是 OWNED stream 的 `Op::MakeUnique`/`Op::Borrow`（5b 已交付），早期“closure-env borrow letter”说法退役：env 边界由 CallModes 一致 + 共同所有 capture 记账承担，不是 `b`。剩余棘轮：嵌套 heap capture（List[String]/Value/variant env，需要有类型的递归 slot free）和 Float capture（f64↔i64 reinterpret prim）。
   - Gate：build-checker byte demo ×6（borrow live/uaf/nothing、branch agree/disagree/cross-arm）、gate.sh 29 行（新增 8：branch A/R、borrow A/R、closure A/R、真实 `heap_result_if.almd` + `funcref_call.almd`）、`check_bc_unroll_sound` axiom-clean + coqchk + 入台账，corpus-wall 覆盖不变（4,693 in-profile / 317 walled — 未悄悄丢弃任何项），almide-mir 579，workspace + spec suites 全绿。
6. **退役提取信任：KERNEL-AS-ORACLE gate。** ✅ **已交付（2026-07-09）。** 逐构建 gate 的 VERDICT 不再依赖未验证 OCaml extraction + `ocamlopt`（TRUSTED_BASE 项目 2，Thompson hole）：`gate.sh` 的 `kernel_verify` 将各 witness bytes 原样内联到生成断言文件并用 `coqc` 检查 — Rocq KERNEL 自身逐行在精确 bytes 上求值已证明 checker（`vm_compute`）（全部 31 行 + 两个 manifest 行），binary/kernel 乖离使构建失败。`corpus-wall.sh` 对整个 corpus witness 集合运行 batch oracle（对 27k-object ownership cert 做一次 `check_bc` fold，对 4.7k names / 3.9k caps / 281 tcaps witness 做 `forallb`）— `vm_compute` 实测约 4 分钟，无条件接入（不采样）。每次构建执行篡改演练：(i) binary 和 kernel 都拒绝损坏 witness；(ii) 捕获模拟 verdict 分歧（证明 oracle 真能约束）。提取 binary 仍是 fast path — 交叉检查，但绝不是 verdict 根。SCOUT 记录（6a）：`rocq-verified-extraction`（MetaRocq，存在 Rocq 9.1 package）及 CertiRocq（CertiCoq → Rocq 9.1 port，2026）均已在 upstream 存在 — 采用延期（local 是无 opam 的 source build 9.1.1，CI 是 opam 9.2；且两者都在 fast path 基础中保留 compiler：Malfunction→ocamlopt / C→CompCert），记录为 fast-path 棘轮。verdict 由 kernel 承担，所以两者都不是 verdict 必需项。诚实残余：`vm_compute` 的 bytecode evaluator 是 kernel 自身 TCB 的一部分（一如既往），oracle 认证的是 WITNESS 层性质（rc primitive 之外的 wasm-byte 链仍是 §3 renderer 契约）。

<a id="open-risks-honest"></a>

## 未解决风险（诚实陈述）

- Witness SIZE 增长（每构建更多节）— 是吞吐问题，不是信任问题；checker 仍小。注意不要拖慢 `gate.sh`。
- Brick 3b/3c 已提供所记录 runtime-model/ISA 和 decoder slice，不是尚未构建的层。未解决边界是已证明 RC routine 之外整个函数的 witness-to-byte 对应。原 WasmCert-Coq 集成是提议路线，不是任意生成代码现已获得语义验证的证据。structural backend 当前测量与证明范围见 `proofs/receipt.sh`。
- ~~分支资源状态一致（brick 5）最容易诱使人遍历 CFG~~ — 已在警戒线内交付：`CBranch` 规则是两个平坦 arm fold + 一次相等检查；无 CFG 遍历、无路径枚举（emitter 把逐对象 arm 事件组织为具体事实；嵌套 region arm 回退到始终拒绝的 poison `{i|}`，而非更聪明的规则）。
