<a id="almide-v1-trust-kernel-coqrocq-proof-spine"></a>

# Almide v1 信任内核（Coq/Rocq 证明主干）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/README.md)的非规范阅读快照，固定提交：35297a612fb29e3f78f6587aba253aa91d51f466。不自动同步。证明、性能、实现及工具鉴定主张均为原文当时的陈述，未在此独立重新验证，也未运行验证命令。以正本为准。本资料不构成工具鉴定或安全认证成立的证据。

证明与认证证据的主干（docs/roadmap/active/v1-mir-architecture.md §5，以及 tier-1 技术栈）。**我们不证明编译器——我们证明检查器。** 庞大且不受信任的编译器输出制品和证书；一个小型检查器 `K` 验证每次构建，而 `K` 的可靠性在这里得到证明：

```
check(certificate, artifact) = accept  ⟹  P(artifact)
```

因此，信任范围从约 100k 行的编译器缩小为数百行的检查器（DO-330 工具鉴定的目标）。编译器与检查器之间的接口是证书格式，而不是证明器。

**证明器选择是 Coq/Rocq（单一 TB），而不是 Lean。** 日常证明（RC 平衡、栈平衡）仍放在 Lean Perceus belt（`crates/almide-perceus-belt/`）中，但航空级主干需要 (a) 安全认证先例，以及 (b) 在同一逻辑中经过验证的编译器，以便将检查器转换为机器码。原文当时只有 Coq/Rocq + CompCert/CertiCoq 同时具备这两点（CompCert 是唯一在安全关键场景中具有经过鉴定的使用实例的已验证编译器）。

<a id="status"></a>

## 状态

- **brick 1（这里）：`OwnershipChecker.v`——已由内核验证。** 针对 MIR 所有权操作序列的 RC 平衡／所有权检查器 `K`，以及其可靠性定理 `check_sound : check ops = true → no_double_free ops ∧ no_leak ops`。这是不可再缩减的单对象核心。通过三种方式验证（Rocq 9.1.1）：
  - 内核检查（`coqc`）、`Qed`、`0` 个 admit；
  - **无额外公理**——`Print Assumptions check_sound` = *Closed under the global context*（没有额外公理；“Print Assumptions ⊆ standard”关卡）；
  - 由 `coqchk` **独立复查**（De Bruijn 准则）。

- **brick 4 起步：`ALS.v`——以 ALS 为规范，以及首次诚实的翻译验证。** 将 `exec` 指定为规范性所有权语义；`eager_copy_refines_safety` 证明只增引用（eager）的轨迹不会发生重复释放，`dec_free_leaks` 则记录这种轨迹不会释放（不产生 release）。此后 wasm renderer 已转向 RC 机制（A1.1b：`Drop → call
  $rc_dec`），以 `balanced_cert_no_memory_fault` 为安全基础，实现单元级无泄漏（`balanced_cert_frees_in_memory`）；eager 定理仍作为双 oracle 基线保留。已通过内核检查、无额外公理、经 coqchk 确认。（完整的 V = 输出的 wasm 字节 ⊒ ALS 需要 Coq 中的 wasm 模型——属于后续 brick。）

- **brick 2：已证明的检查器在真实字节上运行。** `Extract.v` 将内核证明过的 `check` 提取为 OCaml；`driver.ml` + `build-checker.sh` 将其链接为 `checker` 二进制，验证**证书格式 v0**（像 Metamath 一样简单的 token 流：`i`/`I` 表示所有权 +1，`d`/`D` 表示 −1，忽略空白）。`./build-checker.sh` 接受平衡的证书，拒绝重复释放／泄漏。这使携带证明的代码链条实际运转起来：证明不再只是“关于模型的证明”，而成为“带有可靠性证明、能够判定真实输出的运行中二进制”。

- **#4013：`MemorySafe.v`——内存安全的唯一定义。** 对于堆轨迹（alloc / store / load / inc / dec），`MemorySafe` 包含四个条款：SPATIAL（界内）、TEMPORAL（无释放后使用、重复释放或复活）、INITIALIZATION（不读取未写入的字节）和 TYPE（无表示混淆）；`memory_safe_iff_clauses` 证明它恰好就是这些条款。轨迹检查器对它既可靠又完备，每个条款都有一个阴性对照。`rc_check_gives_temporal` 将 brick 1 映射进来：带守卫的 RC 检查给出 TEMPORAL，而且只给出 TEMPORAL。尚无证书携带轨迹，因此这里确定的是 `K(c, a) accepts ⟹ MemorySafe a` 的右侧，而不是证明已发布字节具备该性质。

- **#4005：`SpatialCert.v`——由静态证书得到 SPATIAL 条款。** 用于列表块元素读写的证书语言（read、write、边界检查、重新赋值索引、loop、branch；每个索引槽指明其块），以及一个检查器：只有到达访问的每条路径都经过对同一索引槽的检查，且此后未重新赋值，才接受该访问。`accepts_spatial` 表明：已接受证书的每次运行——任意循环次数、任意分支、写入索引的任意 i64，以及被失败检查提前终止的运行——都会产生 `Spatial`（以及 `WellTyped`）轨迹。该检查就是 emitter 的单次无符号比较；`unsigned_ge_excludes` 表明它也排除负索引。被省略的重复检查（#3345）和外提的检查（#2319）由检查器重新推导，而非直接信任。`rejects_*` 对照表明，失效的旧检查、跨过重新赋值而外提的检查，以及仅一侧分支有检查的情况均被拒绝；`unchecked_run_not_spatial` 则表明被拒绝的形态确实会越界读取。`BoundsCert.v` 将字节格式及其解析器纳入证明；`check_bounds_cert` 被提取出来（driver mode `bounds`），`check_bounds_cert_sound` 说明 ACCEPT 的含义。格式错误的字符串会被拒绝，绝不按前缀读取。structural wasm 路径在每次构建时，为所有可能被 #3345 省略检查的访问输出 `bounds` witness（crates/almide-wasm/src/witness_bounds.rs）；`structural-wall.sh` 用三种方式判定它（提取出的检查器、内核、almide-verify）。与所有权 witness 一样，witness 的事件确实对应字节中包含的访问，这一点仍被信任。

- **#4014：`CoreSafe.v`——类型正确的 core 程序具有 MemorySafe 性质。** 源语言的一个带类型核心，包括整数、布尔值、具有值语义的整数列表、box（只有一个槽的块，可容纳任意值，包括列表和 box）、`let`（新名字）、赋值、元素 store（复制到新块）、`if`、`while`、`make`、带边界检查的索引访问、`len`、`if` 表达式，以及支持递归的一参数和二参数函数（调用在只含参数的新环境中运行函数体；两个参数名互不相同，返回时依次归还各参数）。其大步语义按编译器使用的布局（先头部，再每元素一个槽）输出 MemorySafe.v 的堆轨迹。块使用引用计数并会被释放：读取列表变量会共享其块（`EInc`）；消耗掉的操作数、被覆盖的变量以及正在返回的调用参数会归还引用（`EDec`）。计数为零时释放块，释放 box 时也归还其槽所持有的引用（语义以堆为索引来决定这一点）。不变量是每个块的计数覆盖程序仍持有的引用，包括存活 box 槽中的引用（由 ghost store 记录）。`typed_core_memory_safe`：类型正确的程序，其每次正常结束或被失败检查终止的运行都满足 `MemorySafe`——空间、时间、初始化和类型四个方面。`unchecked_index_not_safe` 表明空间安全依赖于检查；`shared_list_survives_mutation` 让一个别名经历修改和释放；`missing_inc_is_use_after_free` 表明同一次运行若缺少共享读取的 `EInc`，就会从已释放内存加载；`box_frees_and_keeps_content` 释放 box 但保留其内部列表存活；`box_store_without_share_is_use_after_free` 表明缺少 `EInc` 的 box store（#2133 的形态）会访问已释放内存；`count_down_ok` 对自递归函数进行类型判定。尚未包括：闭包、多字段记录、variant、字符串、多参数调用、作用域结束时释放（绑定被覆盖时释放，其余泄漏）、发散运行，以及与编译器 lowering 的任何连接。

- **#4013：`Certified.v`——组合两个证书。** RC 证书给出 TEMPORAL（`rc_check_gives_temporal`）；边界证书使每次访问都位于其块布局内（`run_sound`）。在会发生释放的轨迹上，任意一个单独都不能给出 SPATIAL：访问要求块存在，而只有 RC 一侧说明这一点。`certified_with_bounds_cert` 描述：给定每次分配新块的分配器和符合布局的分配，在同一条交错轨迹上，证书对给出 `Spatial`、`Temporal` 和 `WellTyped`。`access_after_free_not_temporal` 是对照：对已释放块的布局内读取并不安全。`certified_memory_safe` 借助另一个假设 `init_disc` 达到全部四个条款：每次分配之后立即跟随初始化整个区域的 store（构造器纪律）。随后 `InitCert.v` 用已接受的构造位置替代初始化／布局假设；structural emitter 为列表字面量和范围列表记录 `init` witness，而非所有分配位置。`read_before_init_not_initialized` 是其对照。尚未达到：从证书事件到字节的连接。

<a id="verify-the-third-party-make-verify-trust"></a>

## 验证（供第三方使用的 `make verify-trust`）

```
cd proofs
./check.sh           # coqc (+ axiom audit) then coqchk — the proof
./build-checker.sh   # extract → link → run the proven checker on certificates
```

代码注释说明：第一个脚本先执行 coqc（含公理审计），再执行 coqchk，检查证明。第二个脚本依次提取、链接，并对证书运行已证明的检查器。

<a id="known-limitations-the-honest-base--config-management-7"></a>

## 已知限制（诚实的基础——配置管理 §7）

记录受信任基，以便其余部分可以称为“已证明”：
- **内核**（Coq/Rocq）仍是信任根。提取是快速路径，不是关卡判定的唯一根：关卡会独立地在内核中重新求值 witness，并与 `almide-verify` 比较。
- ~~`driver.ml` 中的 tokenizer~~——**已内化到 Coq 中**（`parse` + `check_cert`，由 `check_cert_sound` 证明）：整个*字节 ⟶ accept/reject* 流水线现已由内核检查。受信任的粘合部分缩小到仅剩文件 I/O。
- 所有权、名字、capability、调用模式、边界和初始化都有检查器组件；参见 `Extract.v` 和 `structural-wall.sh`。它们接受的 witness 并不能证明每个已发布程序的完整 `MemorySafe` 谓词：记录、覆盖范围以及轨迹到字节的假设仍然存在。

Rocq/Coq 提供 `coqc`/`coqchk`。当前工具链固定版本、公理台账和精确的模型／实现边界见 [TRUSTED_BASE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)。

<a id="byte-grounding-two-runtimes-one-retired-2753-2761"></a>

## 字节对应：两个运行时，其中一个已退役（#2753、#2761）

两个关卡将已证明的字节列表对应到编译器实际输出的字节。它们针对的是不同运行时：

| 关卡 | 它对应的证明 | 比较的字节对象 |
|---|---|---|
| `check-wasm-bytes.sh`（在 #2761 退役）（+ `check-wasm-exec.sh`） | `WasmEncode.v`、`WasmExec.v`（连同 `WasmRcDec.v`、`WasmDecode.v`、`WasmIsa.v`：FORMAL-CREDIT FC-4） | **原有** WAT renderer 的 `$rc_inc` / `$rc_dec`，从 `render_program` 输出中提取 |
| `check-structural-bytes.sh` | `StructuralDecode.v`（连同 `StructuralRuntime.v`、`StructuralRun.v`、`StructuralAlloc.v`） | **structural** emitter 的 `$inc` / `$dec_flat` / `$free` / `$alloc` 代码体（`almide-wasm` `dump_runtime_bytes`） |

structural emitter 是默认的 `--target wasm` 路径，因此它的运行时字节已经得到对应。原有运行时的字节列表不能重新对应到它：两个运行时的代码不同，而 structural 代码体已经有自己的已证明列表。

因此，`check-wasm-bytes.sh` **与原有 renderer 一同退役**，发生在删除后者的变更中（#2761、#1696 step 5），而不是更早：在此之前，原有 renderer 仍用于发布程序，关卡确实为那些字节提供证据。删除时：

- 删除 `check-wasm-bytes.sh`、`proofs/check.sh` 中对它的调用及其关卡验证行；`check-wasm-exec.sh` 继续针对真实引擎运行 `run_g`，但其 rc 代码体不再是“renderer 的代码体”；
- FC-4 与 C-PROVEN receipt 行改述为关于模型化运行时（已退役的原有运行时）的定理，并将 `check-structural-bytes.sh` 指定为已发布字节的对应依据。

<a id="original-brick-plan-historical-ordering-not-current-pending-status"></a>

## 原始 brick 计划（历史顺序，而非当前待办状态）

1. ✅ 第一个性质（RC 平衡）的检查器与可靠性——本文件。
2. 证书格式 v0（像 Metamath 一样简单的 witness 语言）——接口。
3. 接通编译器以输出 witness；CI 把关“K 接受每次构建”。
4. ALS core（Coq 小步语义）与翻译验证器 `V`（输出的 wasm 字节 ⊒ 已验证模型），每次构建执行。
5. 每次一个性质：栈平衡／名字全域性／capability 上限／类型具体化／终止性——分别以 ALS 定义、证明 K 的可靠性、输出 witness、设 CI 关卡，并加入 receipt。
6. CertiCoq + CompCert：将 `K`/`V` 提取到机器码，关闭 Thompson hole。多样性：≥2 个独立检查器。配置管理：工具链固定版本 + 公理台账 + 已知限制（不可再缩减的基础：Coq 内核、CompCert/CertiCoq TB、硬件，以及 ALS 的有效性）。

---

原文注意事项：原始 brick 计划是历史记录，不是当前完成状态清单。core 定理仅涵盖所述带类型片段及正常结束或被检查终止的运行，不涵盖任意编译器输出；原文既包含一参数、二参数调用，又将多参数调用列为尚未覆盖。这里保留这一表述，不扩展定理。证书到字节的对应以及分配位置覆盖范围仍是明确限制。关于提取链条实际运转和证明器／编译器唯一性的描述均保留为原文当时的主张，并非新的工具鉴定或市场评估。本资料不授权执行示例命令，也未执行它们。
