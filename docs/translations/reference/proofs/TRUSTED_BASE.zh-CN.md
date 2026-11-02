<a id="trusted-base-ledger-config-management--tier-1-layer-7"></a>

# 受信任基础台账（配置管理——tier-1 第 7 层）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](TRUSTED_BASE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](TRUSTED_BASE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](TRUSTED_BASE.zh-CN.md)

> 固定原文的非规范性阅读快照；权威台账保持不变。日期、测量、证明与 CI 声明均为原文历史陈述，不是新复现的验证、外部认证或普遍的沙箱／内存安全保证。新旧进展层并存（CI 版本、高阶处理／控制流准入、println／字节绑定范围），原样保留而未调和。模型保证、产物字节声明及剩余信任假设须按各自范围阅读。不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/TRUSTED_BASE.md)

航空级纪律：**准确说明信任什么，才使其余部分可以称为“已证明”。** 审阅者通过本页了解边界。它是 receipt 的 `C-PROVEN` 声明所对应的诚实说明。

<a id="toolchain-pin"></a>

## 工具链固定版本

| 组件 | 版本 | 作用 |
|---|---|---|
| Rocq / Coq | **9.1.1** | 内核 + `coqchk` 独立复查（权威版本，本地源码构建） |
| OCaml | **5.4.1** | 抽取目标（直到 CertiCoq） |

复现每项声明：`make verify-trust`（或 `proofs/check.sh` + `proofs/gate.sh`）。

**CI 跨版本说明（如实披露）。** `Trust Spine` GitHub Actions workflow 在 opam 的 Rocq **9.3** 上重新推导整个 spine——rocq-core 固定为 9.3.0（`ROCQ_CORE_VERSION`，也是缓存键），配合 rocq-stdlib 9.2.0（opam 没有 9.1.1；上述权威固定版是本地源码构建）。证明在 **9.1.1、9.2、9.3 上均经内核检查且无额外公理**——这是跨版本重新推导，而非单一版本产物（是优势，不是缺口）。9.3 将 `is` 保留，因此证明不再绑定它（#3368）。Rocq 9.2+ 仅提供 `rocq` 驱动，因此 CI 将 `coqc`/`coqchk` 提供为 `rocq compile` / `rocq check` 的薄包装（后者就是 Rocq Proof Checker——独立 De Bruijn 复查确实存在）。

<a id="build-and-run-toolchain-4010"></a>

## 构建与运行工具链（#4010）

下述证明涉及 Almide 的检查器及模型。实际运行的字节由这里的证明未覆盖的工具生成并执行，因此逐一按名称和版本信任它们。`scripts/check-toolchain-pins.sh` 保持此列表与 workflow 和 `Cargo.toml` 实际使用内容一致（`scripts/toolchain-pins.txt` 为机器可读副本）。使用却未列出的版本会失败，已列出却不再使用的版本也会失败。

| 组件 | 版本 | 做什么 | 独立检查它的手段 |
|---|---|---|---|
| rustc | **1.97.0**（`RUST_TOOLCHAIN`） | 构建编译器，并编译原生路径生成的 Rust | 没有任何内容证明它。原生路径在契约语料上与 wasm 路径逐字节比较 |
| LLVM | 该 rustc 内附版本（1.97 为 22.1 系列；`rustc -vV` 打印它） | 原生代码生成 | 同 rustc |
| rustc（Ferrocene） | **1.95.0** | 使用记录的 Ferrocene upstream pin（#573）上的普通 rustc，构建 run-manifest 语料生成的 Rust | 仅为兼容性证据；此路径没有使用通过资格鉴定的 Ferrocene 二进制 |
| rustc（stable，perf） | **1.99.1**（`RUST_STABLE_TOOLCHAIN`） | 构建 `perf-ratchet` 和 `perf-profile-arm` 性能比中的手写 Rust 一侧；没有发布产物以它构建 | 比率检查本身；固定版本使其基线不变 |
| rustc nightly | **nightly-2026-08-01**、**nightly-2025-06-01** | 覆盖率插桩（分支覆盖、MC/DC）及对 `almide-str` 运行 Miri；没有发布产物以它们构建 | — |
| wasmtime（嵌入 crate） | **50.0.1** | wasm 上的 `almide run` / `almide test`，fan worker | 下述普通 CLI 运行 |
| wasmtime（CI 中的普通 CLI） | **50.0.1** | 在 CI 检查中运行已构建 `.wasm` 产物，所有位置统一版本 | 运行时 `tests/wasm_vm_parity_test.rs`（REQ-VM-9）将 `almide-wasm-vm` 与它比较 |
| almide-wasm-vm | 本源码树 | 面向 Critical profile 范围的第二个独立 wasm 解释器（仅标准库，无 `unsafe`） | REQ-VM-9：处于其五个 WASI import 范围内的每个 `spec/wasm_cross` fixture，与普通运行时执行结果一致；通过 `--profile critical` 时也与原生一致 |
| OS、libc、CPU | CI runner 与用户各自使用的版本 | 一切 | 接受，不检查 |

**CI 下载的内容（安全行 16）。** workflow 或 CI 脚本下载的每个二进制或归档，都对照 `.github/pinned-downloads.txt` 所列 sha256 检查（`scripts/fetch-pinned.sh`）。涵盖 wasmtime、cargo-nextest、wasm-pack、elan、Lean 工具链和 Node。第一方 `latest` Almide release 则改为验证其 release attestation。每次 `git clone` 都 checkout 一个 40 位十六进制提交（codopsy）；opam 仓库固定到一个提交，其 opam 文件携带各源码归档的校验和；每个第三方 action 也在该处声明其获取内容。`scripts/check-action-pins.sh` 拒绝其他情况。以下四个渠道在这里依赖信任而非固定：
- **rustup。** `dtolnay/rust-toolchain` 为精确工具链运行它。rustup 对照 static.rust-lang.org 提供的该发布 channel manifest 中的 sha256 检查每个组件。与 rustc 本身一样，信任 Rust 发布基础设施。
- **apt 和 Homebrew 包**（binaryen、musl-tools、opam、wabt、valgrind、Vulkan 驱动）。它们对照发行版签名包索引检查，而非此处声明的摘要。
- **`actions/cache` 条目。** 只有本仓库自身运行会写入；PR 缓存仅限其自身 ref。缓存的 release tarball 在使用前由 `fetch-pinned.sh` 重新哈希；已解包的树（`~/.elan` 下 Lean 工具链、以固定提交为键的 codopsy 二进制、opam root）不会。
- **标为 `# supply: head-tracked` 的 clone。** 移动的 head 本身就是测量对象。下游 canary 测试每个项目默认分支 HEAD：仅 dispatch 触发，token 只读，运行路径不含秘密。语法偏差检查将 `almide-grammar` 当作数据读取，绝不构建或运行它。

**两个引擎。** wasm 执行不只依赖 wasmtime。`almide-wasm-vm` 不与 wasmtime 共享代码，REQ-VM-9 在整个 `spec/wasm_cross` 语料上与普通运行时做差分。范围是 Critical profile 的公开面：导入超过其五个 WASI 调用（fs、env、args）的产物在加载时拒绝，因此对这些产物，wasmtime 仍是唯一引擎。

**嵌入式宿主不是沙箱。** core-module 路径的文件系统操作遵守 `[permissions] fs` 授权；没有此键时，保留对宿主文件系统的访问。p3 component 路径（`run_component`、`crates/almide-wasm-run/src/component_host.rs`）预打开获授权的根，缺少键时预打开当前工作目录和临时目录，并继承环境。Wasm 约束 guest 的内存访问，宿主约束 guest 可使宿主分配的量：每次运行所有实例共用一个线性内存预算，宿主读取前对每个 guest span 检查内存边界（#4135）。它不限制 guest 可要求宿主执行的行为。`docs/wasm/README.md`（“What the embedded host grants”）有逐路径表。文件系统范围限制（#4022）已实现，但不是针对全部宿主能力的通用沙箱。

**浮动步骤（技术债）。** 剩余未指定版本的 stable 步骤是 `fuzz-nightly.yml:coverage-ratchet`，列于 `scripts/toolchain-pins.txt`。固定它需要重新测定基线并删除该台账条目。两个 perf 作业使用 `RUST_STABLE_TOOLCHAIN`；见上表。

**升级工具链时。** 提升 `RUST_TOOLCHAIN` 或 wasmtime 版本会改变受信任组件，因此落地前：

1. 阅读两版本之间修复或引入的 upstream `I-unsound` issue（rust-lang/rust）及 wasmtime 安全公告。在 PR 写明生成的 Rust 或 wasm 可到达哪些问题。生成 Rust 将 `unsafe` 限于 `runtime/rs` 和两个编译器生成的 prelude（region arena、SIGPIPE 设置，#4007），因此 safe Rust 检查中的健全性缺陷是最重要的情况。
2. 在新版本上运行跨目标语料及 REQ-VM-9。差异是发现的问题，不是噪声。
3. 在同一 PR 更新 `scripts/toolchain-pins.txt` 与此表，否则检查拒绝 PR。

**用户的 rustc（#4110）。** CI 固定版本不能说明用户机器工具链的状况；std 安全修复只有在该工具链包含它时才会进入原生 Almide 二进制（CVE-2024-24576：Windows 上 `Command` 执行 `.bat` / `.cmd` 时，在 1.77.2 之前将未转义参数传给 cmd.exe，会经 `process.exec` 重新开放 shell 注入）。因此每次原生构建（`almide run` / `build` / `test` / `bench`、cdylib 和 install 路径）都在 rustc 或 cargo 运行前读取 `rustc --version`，拒绝低于 rustc 1.97.0 的版本——唯一写明该值的位置是 `src/cli/rustc_floor.rs` 中的 `MIN_RUSTC`：max(1.77.2, 验证生成代码所用的 `RUST_TOOLCHAIN`)。当 Almide 依赖的 std 修复落地时提高它；常量注释列出每次提升覆盖的公告。`tests/rustc_floor_test.rs` 运行报告 1.77.1 及比下限低一个补丁版本的假 rustc（拒绝），以及下限版本（构建并运行），并对照常量检查本段和 docs/specs/cli.md。其覆盖不到的范围：Ferrocene 路径直接用自身 rustc 编译生成 Rust，以及已经在构建缓存中的二进制会不经新检查复用。

<a id="the-irreducible-base-cannot-be-discharged-by-proof--消えない底"></a>

## 不可消除的基础（无法以证明解除——“无法消除的底层”）

这四项必须信任；其余所有内容均相对于它们证明：

1. **Coq/Rocq 内核。** 数十年对抗性审查；`coqchk` 独立复查每个 `.vo`（De Bruijn 准则）。新逻辑没有积累的审查，因此借用而非发明。
2. **OCaml 抽取及 OCaml 编译器——已从门禁判定的信任根退役（brick 6b，2026-07-09）。** 已证明检查器仍抽取为 OCaml（`ocamlopt`）作为快速路径，但每行 `gate.sh` 判定及整个语料见证集合，都由 Rocq 内核自身独立重新验证：生成的断言文件逐字嵌入见证字节，`vm_compute` 在经内核检查的逻辑中求值已证明检查器（gate.sh 的 `kernel_verify`；corpus-wall.sh 的 batch oracle，完整 27k 对象语料实测约 4 分钟）。二进制与内核分歧使构建失败；每次构建均运行篡改演练（损坏见证 + 模拟分歧判定，均被捕获），使 Thompson 漏洞不再影响判定：判定由内核（第 1 项）承担。如实披露剩余信任：抽取流水线仅就便利二进制作为快速路径的正确性受信任（有交叉检查）；`vm_compute` 字节码求值器始终是内核自身可信计算基础的一部分。upstream 已存在验证抽取（MetaRocq、Rocq 9.1）及 CertiRocq（CertiCoq → Rocq 9.1 移植，2026）——记录为本地（源码构建 9.1.1，无 opam）与 CI（opam 9.2）工具链对齐后的未来快速路径棘轮；判定不需要其中任一项。
3. **硬件。** CPU 忠实执行机器码。
4. **ALS 有效性**——形式语义捕获预期含义。这是唯一以经验（interp + dojo + 使用）检查、绝不证明的项目。

<a id="axiom-ledger-the-print-assumptions--standard-gate"></a>

## 公理台账（“Print Assumptions ⊆ standard”检查）

每个定理都**仅依赖内核**——`Print Assumptions` 对所有定理报告 *Closed under the global context*（无 `Admitted`，无额外公理）。由 `proofs/check.sh` 验证。表本身也有**主张偏差检查**（`check.sh` 的 claim-drift 检查，#34 / 指标⑤）：机械确认下列每个定理都是内核复查器（`coqchk`）实际验证过的常量，使本台账的公开主张不会超出已证明内容（虚构行将使检查失败）。此表是 spine 的代表性样本：

| 定理 | 文件 | 假设 |
|---|---|---|
| `check_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_all_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_cert_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_reuse_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_clc_unroll_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `eager_copy_refines_safety` | ALS.v | 在全局上下文下闭合 |
| `mrun_tracks_exec` | RuntimeModel.v | 在全局上下文下闭合 |
| `alloc_not_live` | FreeList.v | 在全局上下文下闭合 |
| `region_window_reuse_safe` | FreeList.v | 在全局上下文下闭合 |
| `region_reset_leaves_no_free_into_the_region` | FreeList.v | 在全局上下文下闭合 |
| `pinned_stays_immortal_forever` | FreeList.v | 在全局上下文下闭合 |
| `p_region_reset_preserves_PINV` | FreeList.v | 在全局上下文下闭合 |
| `reuse_restores_rc_1` | FreeListRc.v | 在全局上下文下闭合 |
| `reuse_hands_back_a_zero_count_block` | FreeListRc.v | 在全局上下文下闭合 |
| `double_release_traps` | FreeListRc.v | 在全局上下文下闭合 |
| `r_region_reset_preserves_RINV` | FreeListRc.v | 在全局上下文下闭合 |
| `rc_dec_prog_realizes_rt_dec` | WasmRcDec.v | 在全局上下文下闭合 |
| `rc_inc_bytes_encode_the_instruction_tree` | WasmEncode.v | 在全局上下文下闭合 |
| `rc_inc_bytes_execute_to_rt_inc` | WasmExec.v | 在全局上下文下闭合 |
| `rc_dec_bytes_trap_on_zero` | WasmExec.v | 在全局上下文下闭合 |
| `rc_dec_bytes_frees_when_one` | WasmExec.v | 在全局上下文下闭合 |
| `make_unique_yields_unique` | CowSafety.v | 在全局上下文下闭合 |
| `check_fill_sound` | CallModes.v | 在全局上下文下闭合 |
| `check_modes_cert_sound` | CallModes.v | 在全局上下文下闭合 |
| `check_bc_unroll_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_xc_unroll_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_line_abort_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_xc_abort_sound` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_xc_no_resurrection` | OwnershipChecker.v | 在全局上下文下闭合 |
| `check_line_prefix_safe` | OwnershipChecker.v | 在全局上下文下闭合 |
| `merge_subset_sound` | Subset.v | 在全局上下文下闭合 |
| `subset_check_fast_sound` | Subset.v | 在全局上下文下闭合 |
| `rc_inc_bytes_isa_effect` | WasmDecode.v | 在全局上下文下闭合 |
| `rc_dec_bytes_isa_traps` | WasmDecode.v | 在全局上下文下闭合 |
| `rc_dec_bytes_isa_frees` | WasmDecode.v | 在全局上下文下闭合 |
| `memory_safe_iff_clauses` | MemorySafe.v | 在全局上下文下闭合 |
| `check_trace_sound` | MemorySafe.v | 在全局上下文下闭合 |
| `check_trace_complete` | MemorySafe.v | 在全局上下文下闭合 |
| `rc_check_gives_temporal` | MemorySafe.v | 在全局上下文下闭合 |
| `unguarded_rc_check_misses_resurrection` | MemorySafe.v | 在全局上下文下闭合 |
| `unsigned_ge_excludes` | SpatialCert.v | 在全局上下文下闭合 |
| `run_sound` | SpatialCert.v | 在全局上下文下闭合 |
| `accepts_spatial` | SpatialCert.v | 在全局上下文下闭合 |
| `unchecked_run_not_spatial` | SpatialCert.v | 在全局上下文下闭合 |
| `check_bounds_cert_sound` | BoundsCert.v | 在全局上下文下闭合 |
| `typed_core_memory_safe` | CoreSafe.v | 在全局上下文下闭合 |
| `unchecked_index_not_safe` | CoreSafe.v | 在全局上下文下闭合 |
| `certified_spatial_temporal` | Certified.v | 在全局上下文下闭合 |
| `certified_with_bounds_cert` | Certified.v | 在全局上下文下闭合 |
| `certified_memory_safe` | Certified.v | 在全局上下文下闭合 |
| `shared_list_survives_mutation` | CoreSafe.v | 在全局上下文下闭合 |
| `missing_inc_is_use_after_free` | CoreSafe.v | 在全局上下文下闭合 |
| `box_frees_and_keeps_content` | CoreSafe.v | 在全局上下文下闭合 |
| `box_store_without_share_is_use_after_free` | CoreSafe.v | 在全局上下文下闭合 |
| `site_window` | InitCert.v | 在全局上下文下闭合 |
| `sited_init` | InitCert.v | 在全局上下文下闭合 |
| `sited_alloc_ok` | InitCert.v | 在全局上下文下闭合 |
| `certified_sited_memory_safe` | InitCert.v | 在全局上下文下闭合 |
| `reorder_memory_safe` | InitCert.v | 在全局上下文下闭合 |
| `check_init_cert_sound` | InitCert.v | 在全局上下文下闭合 |
| `fill_window` | InitCert.v | 在全局上下文下闭合 |
| `rsite_window` | FieldCert.v | 在全局上下文下闭合 |
| `ftrace_safe` | FieldCert.v | 在全局上下文下闭合 |
| `fields_memory_safe` | FieldCert.v | 在全局上下文下闭合 |
| `check_fields_cert_sound` | FieldCert.v | 在全局上下文下闭合 |

<a id="known-limitations-what-is-not-yet-proven--recorded-not-hidden"></a>

## 已知限制（尚未证明的内容——记录而不隐瞒）

**这里“内存安全”的含义（#4013）。** 唯一定义在 `MemorySafe.v`：在堆轨迹上（alloc / store / load / inc / dec，各自指定一个块），每个 load/store 都位于其存活块内部（SPATIAL）；没有操作触及已释放块，也没有分配发放仍存活的块（TEMPORAL）；每个被读取字节都已在分配后写入（INITIALIZATION）；每次访问都使用该块的表示（TYPE）。一个可判定的轨迹检查器对此既健全又完备（`check_trace_sound` / `check_trace_complete`）。四项中，spine 早期定理仅达到 TEMPORAL：`rc_check_gives_temporal` 从带守卫的 RC 检查及分配器的新鲜性（`FreeList.alloc_not_live`）推导它。SPATIAL、INITIALIZATION 和 TYPE 在其他文件中没有对应证明，唯一例外是 `SpatialCert.v`（#4005）证明静态边界检查证书的检查器对定长列表块的元素读写具有 SPATIAL（及 TYPE）健全性：`accepts_spatial`——已接受证书的每次运行，在任何循环次数、分支及重新赋值的 i64 索引下都是 `Spatial`；生成器的一次无符号比较排除负索引（`unsigned_ge_excludes`）。`BoundsCert.v` 将证书字节解析器纳入证明并提取 `check_bounds_cert`（driver mode `bounds`，`check_bounds_cert_sound`）。structural wasm 路径每次构建都为每个索引运算符访问输出 `bounds` witness（crates/almide-wasm/src/witness_bounds.rs）：列表的每次 `xs[i]` 读取和存储、Bytes 的每次 `b[i]` 读取和存储；#3345 的 fact 候选在其区间内，其余每次访问各自使用新的专属 slot。检查记录在比较生成处，访问记录在 load/store 生成处；almide-verify 和 proofs/structural-wall.sh 用提取检查器与内核判定；scripts/witness-reinject.sh bounds 向生成器重新注入三个丢失检查的 bug，要求各 witness 被拒绝。certified corpus 覆盖（trust-spine run 40085282713，2026-10-24）：954/954 个 bounds witness 获接受，93 个非空 witness 中记录 365 次读取、82 次写入，对应 434 个生成检查；在此全访问记录之前，只有 5 个 witness 非空。自 #4011 (d) 起，每个 bounds/init event 也携带指向发布模块的 ANCHOR；`almide-anchor-check`（crates/almide-anchor-check——使用 wasmparser decoder/validator，不含编译器代码；自身受信任，有 18 个接受／拒绝单元模块和三项字节级演练 `scripts/witness-reinject.sh anchors`）要求每个 structural-wall 产物中已定位 event 都确为其所指命令：check 是开启会中止的 `if` 的无符号比较，access 是 load/store，site 对应分配大小及各 store 的宽度、offset、地址 local，且没有跳出的分支或未记录 store；fill 对应精确倒计数。自 sec-rows-2-3-4（2026-10-27）起，也检查比较的操作数（crates/almide-anchor-check/src/operands.rs）：依据发布指令，将每个已定位访问对应到其 slot 的检查——同一索引、所寻址块按自身 slot 大小计算的 `len / S`、位于一个 slot 内、检查支配访问、其间索引和列表不变（写入只能由列表自身的 copy-on-write judge 完成，其调用由 anchor 文件的 `judges` 行定位；没有将列表交给其他调用；没有对其 count word 的 store）。这包括提升后的 count 和 payload pointer，也检查了 kills 所代表的内容；995-fixture corpus 中 578 个已定位访问全部覆盖，并有 `scripts/witness-reinject.sh anchors` 的三项操作数演练。信任项：已定位 judge 调用确为 copy-on-write judge（返回与参数 count 相同的块）；只有接收到块的调用才会原地缩小它；表示方式；anchor 文件的 `alloc` 索引确为 `$alloc`；以及索引运算符和下述字符串读取（#4281）以外所有未被 witness 记录的堆访问。

在源码层面，`CoreSafe.v`（#4014）为 Almide 的类型化核心赋予输出堆轨迹的大步语义：核心包含 int、bool、具有值语义的 int 列表、`let`、赋值、元素 store、`if`、`while`、自带边界检查的索引、`if` 表达式及递归的一／二参数函数，两参数不同，调用返回时依次归还各参数。语义包含引用计数：共享读取递增，已消费操作数、被覆盖变量、返回调用的参数递减；计数到零的块被释放；释放 box（其唯一 slot 可持有任意值的块）会归还 slot 所持有的内容。它证明 `typed_core_memory_safe`：对任何函数体良类型的函数表，良类型核心程序每次结束或因检查而中止的运行均为 `MemorySafe`，满足全部四项。这只是核心（没有闭包、记录、字符串、多参数调用；绑定在被覆盖时释放，而非作用域结束时，因此其余会泄漏；仅涵盖终止及中止运行），没有机制将它绑定到编译器 lowering。`Certified.v` 在一条轨迹上组合两份逐构建证书：`certified_with_bounds_cert` 在带守卫 RC 检查接受每个块的 stream、分配器保持新鲜、分配符合 layout、轨迹访问是已接受 bounds 证书的运行这些前提下，给出 `Spatial`、`Temporal` 和 `WellTyped`；`certified_memory_safe` 再增加一个构造纪律假设（每次分配后立即接着初始化整个范围的 store），从而加入 INITIALIZATION。

`InitCert.v`（#4013）用逐构建证书替代这一假设及 layout 假设（`alloc_ok`）：construction site 包含分配的 header、slot 宽度、slot 数量、表示，接着是它存储的 slot；当每个 slot 均被存储且没有越过末端的存储时接受。`site_window` 证明已接受 site 的 window 为 MemorySafe，并使该块全范围初始化；`sited_init` 和 `sited_alloc_ok` 为由已接受 site 构成的轨迹中每次分配提供 INITIALIZATION 与 layout；`certified_sited_memory_safe` 将 RC 证书、bounds 证书及 site 组合为 `MemorySafe` 的全部四项，不再留下初始化或 layout 假设。`reorder_memory_safe` 证明交换不同块上相邻 event 不改变 MemorySafe，因此 site window 可以与其元素自身的 event 交错；`check_init_cert_sound` 将字节解析器纳入证明（driver mode `init`）；`fill_window` 将其扩展到 LOOP FILL——按运行时 count 定长、由倒计数循环填充的块。当循环倒计数使用的 local 与定长所用 local 相同，且 cursor 每次前进一个 slot 时接受，此时对任意 count 都 MemorySafe 且完成初始化。structural 路径每次构建为每个列表字面量和 range 列表（loop fill）输出 `init` witness（crates/almide-wasm/src/witness_init.rs），proofs/structural-wall.sh 用提取检查器、内核及 almide-verify 判定（certified corpus，trust-spine run 40094443633：554 个非空 witness 中 2973 个 site，全部接受）。其他分配 site（记录、variant、字符串、运行时 routine）尚无记录，因此 `sited`——每次分配均为记录过的 site——仅在该次运行全部分配均为列表字面量或 range 时成立。信任项：记录纪律，以及 site 的块直到最后一个 store 都保持私有。尚无证书携带轨迹，所以该定义所针对的定理——对于发布产物 `a`，`K(c, a) accepts ⟹ MemorySafe a`——仍未解决（#4005、#4006、#1696）。在证明它之前，“内存安全”是关于轨迹模型的主张，不是关于字节的主张。

**记录、元组与 variant 字段（#4013 TYPE 条款，#4279）。** 字段在固定 offset 读写，没有检查；因此，当且仅当块按访问所假定的 layout 构造时，访问才在界内且已初始化。`FieldCert.v` 将 layout 类型化 construction site 与字段访问加入为第三种逐构建证书 `fields`：layout 表按 kind（记录、元组或 variant 的一个 case）列出 payload 大小及各字段 offset/宽度，variant case 的 field 0 是 tag word；site 必须存储其 kind 的所有字段，访问必须指定已声明字段，tag read 必须是每个 case 的 field 0。`rsite_window` 证明已接受 site 的 window 为 MemorySafe，并使该 layout 块的所有字段初始化；`ftrace_safe` 证明，在这类块的分配是已接受 site、其 load/store 指定所属 kind 已声明字段的轨迹中，给定 TEMPORAL，则记录 kind 的块上每个 event 都 MemorySafe；`fields_memory_safe` 将其与其他证书针对所有其他块的条款组合为 `MemorySafe`；`check_fields_cert_sound` 将字节解析器纳入证明（driver mode `fields`）。structural 路径每次构建对记录、元组、variant 字面量与构造器、成员及位置读取、dying read 的 slot take、pattern test/bind、spread 覆写输出 witness（crates/almide-wasm/src/witness_fields.rs）。`almide-anchor-check` 使用同一 layout 表将每个已定位 event 对应到发布字节：分配 payload 大小，每次 store/access 宽度与 offset，variant site 存储自身 case 的 tag，以及每个 variant case 字段的读取被自身地址对应 case 的 tag test 保护（外围 `if` 的条件蕴含 `tag == C`；地址是同一变量，或同一字段重新加载，期间无可能写内存的操作；只有 callee 本体无写入才允许调用，通过模块上的不动点求得）。1000-fixture certified corpus（GCE，ba31c7646）：1000/1000 个 `fields` witness 获全部三种判定接受，每个产物均 FIELDS OK——3998 个 site／8636 个 store，5390 个字段访问（552 个 variant case 字段受其 tag test 约束），614 个 tag read。信任项：块的静态类型就是其 layout（表示方式，bounds/init witness 已信任这一点；IR 无 cast，拒绝 FieldAssign），以及 #4279 新增的一行（所有者裁定，2026-10-30）：

> **信任项（#4279）：construction window 后，没有操作改写 variant 块的 tag word。**
> 范围：仅限 variant case-field 读取所依赖的 case。两端均对照字节检查：construction site 将 tag store 记为 field 0，`almide-anchor-check` 要求它存储 site 自身的 case 常量；每次记录的 case-field read 必须位于其自身地址针对该 case 的 tag test 下（`operands.rs` 的 `case_guarded`）。所信任的仅是两者之间的区间——期间没有指令存储 tag word——因为堆模型（`MemorySafe.v`）不携带值。tag read 本身不需要这行信任项：field 0 在每个 case 中都是 tag word（`ITag`）。演练 `case-field-unguarded`（scripts/witness-reinject.sh anchors）把 case 字段读取移到测试之前，必须得到 ANCHORS REJECT。

运行时自身对这些块的 routine（typed drop、copy、相等、比较、display 本体）尚无记录（#4278）；无参 case 是分配模型之外的静态 pool 块。

**字符串字节（#4281）。** structural 路径字符串 API 在用户代码中读取的每个字节（`get`、`first` / `last`、`capitalize`、`to_codepoint`、`to_list`、`reverse`、`lines`、`pad_*`、`starts_with` / `ends_with`、`strip_prefix` / `strip_suffix`、`io.read_line`）都记录于 `bounds` witness，slot 为一个字节，附带该 API 已经生成的 test——不为记录新增测试；1030 个 corpus/perf 产物的发布字节与 merge base 相同或更小（1021 个相同，4 个合计缩小 27 B，5 个仅 local index 不同）。没有新 event，也没有新 Rocq 条款：每个 test 都是 `BoundsCert.v` 中的 `c` 或 `e`，`almide-anchor-check`（operands.rs 的 `covers`）在字节上检查其覆盖对应读取的依据，每个 fact 对应一条规则：
- API 自身的 `idx >=u len(s)` / `len(s) == 0` 开启 `if`，读取（`s + idx` / byte 0）位于其 `else` 中；`c` 锚定于 `if`。
- byte loop 的退出 test `k >=u n; br_if` 针对 `n = len(s)`，或针对由外围测试限制到它之内的 `n`（`n >u len(s)` 或 `len(s) != n`，loop 位于其 `else`）：`k < n <= len(s)`。
- 同样两个 fact 下的 `(len(s) - n) + k` 后缀读取：`len(s) - n + k < len(s)`。
- 在退出 test 及令 `k` 为正的 test（`k >u x` 的 then 分支）下，读取 cursor 前一字节 `s + k - 1`：`0 <= k - 1 < len(s)`。
- 向下移动的 cursor，退出为 `p == 0; br_if`（对 loop 归纳）：入口 `p = len(s)`，或在 `len(s) == 0` 的 `else` 中为 `len(s) - 1`；loop 中每次写入 `p` 都是退出 test 后的 `p - 1`；读取为 `s + p` 或 `s + p - 1`，两个 offset 之和至少为一。
- 新块自身的大小：`i32.const N; call $alloc`（`c` 锚定于调用），读取位于其 N 字节内（存活块的分配大小从不变化；其存活性由所有权 witness 负责）。
各规则都要求从每次 `len` 读取到访问期间，`s` 的 count 不变（既有 count 规则），且 loop 保护的 read 必须在其 loop 内。1001-fixture certified corpus（GCE，e23e4ea28）：1001/1001 个 bounds witness 获全部三种判定接受，记录 1083 个 loop 退出 test（此前 429 个）及 850 个生成检查；每个产物均 ANCHORS OK，1899 个已定位访问受其 guard 约束（此前 1018 个）。演练 `string-test-one-byte-loose`、`string-loop-exit-loosened` 和 `string-prefix-length-test-loosened`（`scripts/witness-reinject.sh
anchors`）各自得到 ANCHORS REJECT；`crates/almide-anchor-check/tests/string_bytes.rs` 中有 33 个接受／拒绝模块。未记录的部分：字符串 routine 自身本体，以及留待 #4278 逐 helper 证书处理的运行时 routine——emitter helper 中 25 次 byte read（runtime.rs 6、runtime_str.rs 6、utf8_helpers.rs 6、kmp.rs 4、value_helpers.rs、json_helpers.rs、map_index.rs 各 1），以及 24 个 self-hosted `stdlib/string_*.almd` 本体中的 120 个 `prim.load8`。

**native 路径（#4006）。** 对 verified native renderer 所渲染的程序（`proofs/native-verified-floor.txt`，958 个 `spec/wasm_cross` 程序中的 106 个，只增不减），`proofs/native-wall.sh` 每次构建判定每个已渲染函数的所有权证书——取自 renderer 实际渲染的精确 MIR（`try_render_rust_source_certified`），因此 probe charge、result-window 改写及 make-unique 省略均在证书覆盖内——使用提取检查器、Rocq 内核和 almide-verify，并固定所渲染 Rust 源码的 SHA-256。其 poison 路径拒绝这样一个 render 的 witness：在绕过 Rust 镜像检查的情况下丢失了一个 release（`scripts/witness-reinject.sh native`）。信任项：rustc 将所渲染源码编译为保持该平衡的二进制；以及 floor 以外的全部程序，它们由标准 codegen 无证书地发布。

<a id="historical-implementation-progression-retained-evidence"></a>

## 实现的历史进展（保留的证据）

以下逐步积累的叙述记录早期 MIR／旧 renderer 工作。其中语料数量、待做工作及对已移除 `check-wasm-bytes.sh` 的引用均属历史，不是当前发布保证。旧 renderer 于 #2761 退役，其 `Wasm*` 定理仍是模型定理。当前 structural-runtime 字节落地核对采用 `check-structural-bytes.sh`，artifact/witness 检查采用 `structural-wall.sh`，native 子集由 `native-wall.sh` 检查。当前轨迹模型限制已在上文说明；精确的 certified 集合与信任边界请查阅 receipt。

早期进展曾如下描述其范围：

- **航空级性质集合在值语义子集上已完整**：RC 平衡（`MemorySafe` 的 TEMPORAL 条款，见下文）、名字全域性、能力边界（含传递性）、类型具体化、内存模型无泄漏、复用健全性（`Reuse` 仅作用于唯一拥有的对象，不会对存在别名的对象原地复用）、空闲链表复用安全性（有效分配绝不返回当前存活的块，不会发生释放后复用，`FreeList.alloc_not_live`）、区域重置安全性（`RegionSave`/`RegionRestore` 窗口在任意主体执行期间保持分配器不变量，因此前沿重置不会留下指向已回收区域的空闲链表条目，复用安全性在重置后仍成立——`FreeList.region_window_reuse_safe` / `FreeList.region_reset_leaves_no_free_into_the_region`）、PINNED_RC 不灭性（由 `$alloc8` / `__alloc_pinned` 分配的块，即所有由宿主写入的 WASI 缓冲区和 preopen 表，在任意运行过程中绝不被释放、不进入空闲链表、不由 `$alloc` 返回，并在区域重置后存活：`FreeList.pinned_stays_immortal_forever` / `FreeList.p_region_reset_preserves_PINV`；这是 C-042 所违反的不变量）、复用的计数方面（空闲链表交还的块没有陈旧引用计数，交还时恰为 0，`FreeListRc.reuse_hands_back_a_zero_count_block`；构造器 store 后恰为 `RC_INITIAL` = 1，`FreeListRc.reuse_restores_rc_1`；在任意运行及区域重置过程中，空闲链表只含零计数块，存活集合只含正计数块，因此可证明重复 release 会在 `$rc_dec` sentinel 处 TRAP——`FreeListRc.double_release_traps` / `r_region_reset_preserves_RINV`）、写时复制的别名安全性（`MakeUnique` 得到唯一拥有的块，不会原地修改存在别名的对象，`CowSafety.make_unique_yields_unique`）、字节绑定表及实现 `rt_dec` / `rt_inc` 的 `$rc_dec` / `$rc_inc` 指令树（`WasmRcDec`）、rc_inc 指令树编码为真实 wasm 字节（`WasmEncode`，与 wat2wasm 对应）、这些真实字节在 wasm 栈机上执行为 `rt_inc`，以及通过通用解释器（`WasmExec`）在 renderer 真实字节上执行的完整 `$rc_dec` 字节的安全性——无重复释放且无泄漏——还有操作数栈平衡与无循环片段的终止性。全部经内核检查且无额外公理（137 条已审计定理）。剩下的是深度（字节绑定 ISA 层，以及实现空闲链表／`rc_inc` 的 RENDERER；其安全模型现已证明，因此该部分是在细化证明，而非增加受信任运行时）和广度（子集之外的 lowering：控制流、闭包、标准库），而不是子集上的新性质。
- **MIR lowering 的 WALL 已在整个 v0 语料上得到实证验证**（`proofs/corpus-wall.sh`，位于 `make verify-trust` → CI 中）。整个 v0 规范语料（465 个 `.almd` 文件、4195 个到达 MIR 的函数）经过真实 frontend → `lower_function`，在真实源码而不只是手工 MIR 上断言两项健全性不变量：(1) **wall**——`lower_function` 对整个语料是全域的：每个函数要么为 `Ok`（在 profile 内），要么明确为 `Unsupported`（被 wall 阻挡）；**零 panic、零未检测拒绝**（值语义子集之外的程序会附带原因被拒绝，绝不静默错误 lowering）；(2) **accept ⟹ 对全部三项已证明性质均安全**——内核证明的检查器重新验证每个 profile 内函数的 witness，对所有权（无重复释放／泄漏，`check_sound`）、名字全域性（无悬空 MIR 引用，`check_names_cert_sound`）及能力边界（无未声明宿主 effect，`check_caps_cert_sound`）均 ACCEPT。不同性质的 witness 粒度不同，门禁遵循这种差别：所有权检查器对堆对象进行折叠（对整个流作一次 fold），名字／能力检查器则解析单个 `<superset>|<subset>` witness，因此逐函数重新验证（批量文件会错误地把所有函数的 id 折叠到同一 superset；这一问题在构建此门禁时被发现并纳入处理）。这是第 4 步“持续语料验证 = parity 的定义”的诚实初版：它尚不主张满足完成定义（已证明 profile 接受整个语料），而是建立衡量进展的机制，证明边界是墙而不是漏洞。**当前诚实的覆盖率：所有权＋名字方面，4083/4195 个函数在 profile 内（97%）；caps-VERIFIED 较低，为约束 parity 的 3528，见 caps 注记**。范围为值语义子集，加上 **`Range` 值、CLOSURE 值以及无法解析的 `Method`/`Computed` 目标调用**（`f(0..n)`、`var g = (x) => …`、`obj.method()`、`(g)()`）。`Range` 与 CLOSURE 值（新的堆环境）是新值；无法解析的被调用者（此处未知的分派／闭包值）被建模为延后处理的新值（堆 `Alloc{Opaque}` / 标量 `Const`）。其 receiver／参数中的调用被捕获，但 method／computed 调用自身被省略，因此 `ir_calls > mir_calls` 门禁将函数标记为 caps-unverified；被调用者能力未知，这样才诚实。还包括**错误运算符**（`e!`/`e?`/`e ?? d`/`e?.field` 产生新值，即解包／默认／映射后的结果，像所有 Opaque 一样延后处理；操作数调用被捕获）。Almide 没有 `try`/`catch`：`e ?? d`（解包或取默认值）、作为 `ToOption` 的 `e?`（`Result → Option`）以及 `e?.f`（可选链）都是不含控制流的全域值映射，始终可以安全延后。**`e!`（`Unwrap`）以及 effect-fn 自动 `?`（`Try`）会提前返回 `Err`**，而该提前返回曾是 v0 WASM codegen 的泄漏：Err 路径是裸 `return_`，越过函数作用域结束时的堆释放（Perceus rc_dec 仅位于终端 `Ret`），因此提前返回时仍存活的堆局部值泄漏（Rust 始终无泄漏：`e!`→`?`→退出作用域的 `Drop`）。延后并继续的证书是平衡的（检查器证明其 no_leak），因此认可这种形态会导致 **accept-but-unsafe**。**已修复**（修的是底层 v0 错误，不只是用 wall 拒绝）：wasm emitter 现在会在每个 `Try`/`Unwrap`/`Fan` Err 路径 `return_` 传播错误之前，释放当时存活的堆局部值。`emit_early_return_decs` 读取 Perceus 自身的 rc_dec 活性（动态拥有堆集合，在 heap Bind 时加入、RcDec 时移除，排除 env-borrow 和仅 donate 的 `__*` 临时值；返回的 Err 指针是 scratch 临时值，绝不释放）。已实证验证无泄漏（100k 次 ×100KB 的 err-loop 完成而不 OOM）且无重复释放（全部 260 文件 wasm 语料及跨目标字节门禁仍成功）。因此提前返回形态现在在两目标上均忠实 lowering；move/consume 模型从不在没有 Dec 的情况下 move 用户堆局部值（经验证的 RcInc 别名方式），所以动态集合恰为存活集合。参见 docs/roadmap/active/v0-unwrap-early-return-leak.md。还包括**解构模式**（`match` 分支的 `Some(x)`/`Ok(v)`/`Foo(a,b)`，以及 `let Foo{..}=`/`let (a,b)=` 以容器粒度绑定 payload；堆绑定别名整个 subject，使用 `Op::Dup` 并复用已证明的 `a` 事件；元素／payload 精确身份需要布局 brick，像 Opaque 一样延后；标量绑定为 `Const`；记录简写字段被 wall 阻挡）。
  还包括**高阶纯组合器**（带闭包的 `list.map`/`filter`/`fold`/…，位于值位置或 EFFECT／语句位置均可）。纯组合器在调用期间执行闭包后将其丢弃，因此闭包不会逃逸。闭包参数按其能力处理（`Lambda` 主体中的调用，`ClosureCreate`/`FnRef` 被调用者 → effect 标记；闭包 caps 到达 witness，`mir<=ir` 门禁会污染嵌套高阶／不可分析主体），其值延后处理，捕获项被借用。OPAQUE 函数值（caps 无法分析）被 wall 阻挡；值结果为新拥有值，Unit／标量 effect 结果不带所有权，被丢弃的堆结果会分配后 drop。
  还包括 **`for`/`while` 循环**（每次迭代的作用域帧使一次被建模迭代内部平衡 ⟹ 对任意 N，N 次运行时迭代均无泄漏；没有 loop op）。堆 iterable 被借用或经 `lower_call_args` 实体化；循环变量逐迭代别名容器（`Op::Dup`，容器粒度），或为标量 `Const`。只含标量的帧上的 `break`/`continue`（零参数、无值、无标签；Almide 没有 `break x`/`return`）被允许为无操作（帧不含堆 handle，因此真实提前退出不会跳过 Drop，两个目标均无泄漏）。但在堆帧（堆循环变量的 `Op::Dup`、堆主体局部值）上会被 wall 阻挡——v0 wasm 后端在 break 分支目标之后才释放，实际提前退出会泄漏逐迭代堆 handle（这是被对抗验证捕获、未交付的 accept-but-unsafe 情况）。堆重新赋值，即循环累加器 `acc = acc + [x]`，被延后而非重新绑定；`acc` 在各迭代之间保留仍存活的循环前 handle，因此某次迭代不会 drop 后续迭代要读的 handle（内存安全；累加本身像 Opaque 一样延后）。它也不是帧 handle，所以标量循环＋堆累加器＋break 被允许；标量重新赋值 `i = i+1` 也被允许。还包括 **`if`/`match` 控制流**（语句／标量／Unit／堆尾部以及堆绑定位置）；分支携带逐分支作用域帧，被线性化进平坦 op 流，没有 branch op。自 brick 5a 起，逐分支纪律是受检查而非受信任的：各分支必须对每个对象的退出计数达成一致（按分支分组的证书 `{…|…}` 加 `verify_ownership` 的分支合流；一个分支将外层 handle move 出去时，release parity 会在兄弟分支插入补偿 Drop）。结果是调用方输出的单个合流槽：丢弃（Unit／语句）、`Const`（标量）、或新的 `Alloc{Opaque}`（堆，按构造内存安全，其值内容像 Opaque 一样延后）。堆 `match` subject 会实体化（新值进入作用域结束时 drop 的拥有临时值，已跟踪变量则借用）；绑定 payload 的模式采用容器粒度（见下文）；分支 guard 被 wall 阻挡；重新赋值堆变量的分支被延后，变量保留分支前 handle，因此分支后读取不会解引用该分支已 drop 的 handle（无路径相关 UAF）。
  还包括**元组解构**（`let (a,b) = (x,y)` 逐元素处理，或 `(a,b) = t` 每个元素都别名容器）、**字段／元素赋值**（xs[i]=v / r.field=v → MakeUnique 写时复制）、**任意堆参数的 println**（`println("x")`/`f()`/`a++b` 实体化＋借用，因到达 Stdout 而诚实地标记为 caps-unverified），以及**重新赋值**：`x = v` 重新绑定 `x`；旧绑定延续到作用域结束并且恰好 drop 一次（保守地延长生命周期，绝不重复释放）；在 `v` 中读取旧 `x` 会借用仍存活的旧 handle（在重新绑定之前 lowering），绝不会 UAF。还包括**字段／元素提取**——`xs[i]` / `r.field` / `t.0` / `m[k]`：标量结果是明确的复制 → `Const`；堆结果通过现有 `Op::Dup`（v1 容器粒度字段访问）别名容器，提取值是整个容器上的第二个 handle，使容器在该值的整个生命期内存活（保守地延长生命周期，绝不 UAF）。它复用已证明的 `a`/alias 事件，因此 Coq 检查器与 backing 门禁不变，诚实地将字段精确别名（值自身的对象）延后到布局 brick（LayoutId 当前是占位符）；嵌套容器提取（`a.b.c`，没有单个已跟踪 `src`）仍被 wall 阻挡。还包括表达式主体函数、直接返回堆字面量、直接返回具名调用结果、接受**借用堆参数**的函数、**一阶纯 stdlib `Module` 调用**、**嵌套 CALL 参数**——`f(g(x))` / `assert_eq(g(x), …)` 实体化到拥有临时值，借给外层调用，在作用域结束时 drop——**字面量 CALL 参数**——`f("x")` / `f([1,2,3])` / `f(3.14)`，堆字面量经 `Alloc`，标量字面量为 `Const`——**Option·Result 构造器**——`Some(x)` / `Ok(e)` / `None` / `Err(e)`，堆 variant 像容器字面量一样实体化——以及 **BinOp / UnOp**——`a+b` / `s1++s2` / `-n`，新计算值（堆拼接经 `Alloc`，标量算术／逻辑为 `Const`；操作数具有自身所有权，采用值语义所以结果绝非别名）。其余形态被 wall 阻挡，按特性统计的 `Unsupported` 直方图指出下一个待允许的领域（当前最大的分类指明具体 stdlib 函数：带闭包参数的 `list.map`/`filter`/`fold`——高阶 brick——以及调用作为参数时的实体化）。
  **stdlib `Module` 调用只允许纯调用**：只有一阶（没有闭包参数）且被调用者可证明为纯（不触及任何宿主能力）时，`<module>.<func>` 调用才 lowering 为 `Op::CallFn`。这是能力健全性的要求：已证明检查器仅从 `Op::Call` 的带类型 `RtFn` 导出 `used` 能力，因此调用 effectful stdlib 名称的 `CallFn` 会静默地从 `used` 中遗漏其能力，导致 accept-but-unsafe。纯被调用者到达空能力集合，空 `used` witness 忠实表示它；effectful（经 `effect fn` 关键字的 `fs`/`http`/`net`/`io`/`env`/`process`/`random`/`zlib`）、普通但不纯（`datetime`/`args`/`mem`/`testing`，不带关键字也到达宿主）以及高阶调用均被 wall 阻挡，绝不 lowering。纯度注册表（`crates/almide-mir/src/purity.rs::PURE_MODULES`）完全位于不受信任的 emitter 中；**漂移门禁**（`corpus-wall.sh` 中的 `proofs/check-stdlib-purity-registry.sh`）从 `stdlib/*.almd` 重新导出 effectful 集合；只要被允许模块新增 `effect fn`，或任一 stdlib 模块未分类，就失败，因此 pure→effectful 漂移不能静默交付。**能力性质按传递性检查，并诚实地限定于 Stdout。** 只有保守 fold（`certificate::reaches_capability_or_unknown`）证明沿每条 `Op::CallFn` 边均不达 Stdout 时，才输出函数的空能力 witness（主张 caps-safe）。profile 内被调用者会被 fold；纯 stdlib `Module` 调用、variant 构造器、已知不达 Stdout 的内建函数（`assert*`/`eprintln`/`panic`/`to_string`——这些到达 stderr／abort，而非 Stdout）没有问题；其他任何未知被调用者（被 wall 阻挡或跨文件的用户函数）均会污染，因此函数被报告为 `caps-unverified`（3528/4083 已验证，555 未验证），而非被错误接受。**门禁验证的是真正的能力边界性质 `reachable ⊆ declared`**（正是 `proofs/CapabilityBound.v` 所证明的），不是退化的“完全不触及任何能力”。`lower_function` 将每个函数的 effect 签名 lowering 成 `declared_caps` 边界：`effect fn` 声明 `{Stdout}`（唯一建模的 cap），纯 `fn` 声明 ∅；classifier 折叠传递可达 cap 集合（`certificate::reachable_caps_or_tainted`，有任何污染就返回 `None`，仅当每条边都可分析才返回 `Some(set)`），随后输出 `<declared>|<reachable>`，供已证明的 `check_caps_cert` 重新验证 `reachable ⊆ declared`。因此 effectful 函数根据自身声明的边界接受验证（打印的 `effect fn` 因已声明所用 Stdout 而被接受），并非仅因触及能力就被排除。到达未声明能力的函数——例如会打印的非 `effect fn`，因为 frontend 的 `is_effect` 标志并不覆盖所有 Stdout 到达——不满足 `reachable ⊆ declared`，被保守标记为 caps-unverified，绝不错误接受。
  **Opaque lowering 省略的调用**（列表元素、ctor payload、BinOp 操作数或标量值，其子表达式未被 lowering）是 fold 看不到的第二个 caps 盲点。`lower::record_elided_calls` 将每个此类调用显化为裸 EFFECT MARKER `Op::CallFn{dst:None, args:[], result:None}`：现有 handler 将无结果、无 dst 的调用视为仅有 EFFECT、没有值的调用，不产生所有权事件、不引用任何值（所以所有权／名字 witness 与 `+1` backing 门禁不变），但 caps fold 会按名字匹配并传递折叠被调用者。仅显化健全建模的调用——一阶 `Named` 调用（fold 展开或诚实污染它）及一阶纯 `Module` 调用；高阶／effectful 的 `Module`／`Method`／`Computed` 调用保持省略，让门禁继续污染该函数（不会错误清除污染）。健全性安全网：marker 只记录在被整体省略的位置，因此 MIR 调用数最多只能向 IR 数增长；语料门禁断言 `mir_calls <= ir_calls`，使重复计数（marker 掩盖真实省略并错误清除污染的唯一方式）成为 WALL BREACH，结构上无法交付。源码仍比 MIR 有更多调用节点的函数（无法实体化的省略调用），及其所有传递调用者，都保持保守污染，因此 caps-verified 数量诚实，绝不夸大。这关闭了直接 witness 漏洞（`reachable_caps` 的诚实范围：未知被调用者原先贡献 ∅）。诚实范围：仅建模 `Capability::Stdout`，所以性质是“没有未声明的 **Stdout** effect”；stderr、abort、fs、net 是真实宿主 effect，但尚未命名（更广的 `Capability` 集合＋经 frontend lowering 的 `declared_caps` 属于后续 brick）。
  **堆参数采用默认借用调用约定**（调用者拥有引用；参数不向证书贡献拥有的 `+1`）。这是当前运行时下唯一健全的约定：拥有参数的 `+1` 会是合成的，没有真实运行时 `Alloc`/`rc_inc` 支撑，属于门禁盲区中的释放后使用类别。借用参数若未先获取自身引用（`Dup`）就返回或释放，会被明确 wall 阻挡（`returning a borrowed param directly`），其证书将是在 rc 0 时释放，已证明检查器会判错。**防复发 backing 门禁**（位于 `corpus-wall.sh`，并有对应单元测试）断言证书每个 `+1` 都由真实 `Alloc`/`Dup`/heap-result 支撑，因此结构上无法交付重新引入合成参数所有权的变更。注意：此 brick 不改变已证明 Coq 检查器（仅证书生成移除了无支撑的参数 `i`），所以检查器大小不变量仍成立。覆盖率只报告，绝不用脆弱的精确计数作为门禁；硬性约束只有健全性不变量，另加防崩塌下限（至少 1 个 profile 内 witness 必须到达检查器，因此覆盖率静默降为零会使门禁失败）。
- **索引边界内存安全——已发现并用 wall 封堵的漏洞。** 所有权检查器证明 RC 性质，但不检查列表索引边界；renderer 的 `$list_set`/`$list_get`/`$elem_addr` 也未做边界检查，因此越界索引会计算出块外地址，向那里 store 会破坏内存——这是接受却不安全的漏洞（与 RC 不同的内存安全维度）。现已用 wall 封堵（不是静默处理）：`$elem_addr` 在 `idx < 0 ∨ idx ≥ cap` 时 trap（`unreachable`），因此每次元素访问均检查边界，OOB 是受控停止，绝不会导致破坏（已在 wasmtime 验证：OOB 会 trap，界内不会；值语义输出逐字节不变——所有实际访问均在界内）。同一次审计封堵了第二个无界写入缺口：`$print_list` 原先在固定缓冲区 `[SCRATCH_ADDR, HEAP_BASE)` 中无上限地构建输出行，极长列表会使行溢出到堆；现在会在追加将越过 `HEAP_BASE` 的元素之前 trap（打印缓冲区 wall）。更深层修复——由检查器静态拒绝 OOB（检查时的 wall，而非运行时）——属于后续 brick；当前 wall 是运行时 trap。
- **wasm renderer 已采用 RC 机制（A1.1b）：每个 drop 输出一次 release。** `Drop` 现在渲染为 `call $rc_dec`，将引用计数单元（A1.1a 重新布局后位于堆偏移 0 = `RuntimeModel.RC_OFFSET`）递减到 0，因此二进制确实在单元层面释放。安全基础也随之迁移：不再是 `eager_copy_refines_safety`（制品已不是 Dec-free），而是 `RuntimeModel.balanced_cert_no_memory_fault`——接受的（平衡的）证书在内存机中不会重复释放——加上 `balanced_cert_frees_in_memory`——其单元最终为已释放状态（rc 0）。二者均已由内核证明且无额外公理（本片段纯粹复用证明，没有改动 `.v`）。每次构建的 V，即 `validate_translation_perceus`，将每个 witness drop 绑定到一个 `call $rc_dec` 字节表示（每个 drop 一次 release，不能更少），因此证明传递到真实字节；`$rc_dec` 运行时 sentinel 会在运行时对重复释放 trap（单元已为 0 时触发 `unreachable`——已在 wasmtime 验证触发）。因此，现在可以对输出制品而非仅模型主张 `C-SAFE` 的无重复释放和单元级无泄漏。物理回收现已实现（A1.2-render）：rc→0 时 `$rc_dec` 将块归还 free-list，`$alloc` 复用大小完全匹配的表头，细化已证明的 `FreeList.v` 模型（`alloc_not_live`：有效分配绝不会返回当前存活的块——无 reuse-after-free）。重复释放 sentinel 得以保留：free-list 链接放在已死亡的 LEN 字段，而非 rc 单元，因此再次 release 已释放块仍会 trap（已验证：重复释放 trap 测试和 alloc/free/realloc 中 `p1==p2` 的复用测试均在 wasmtime 通过；值语义输出逐字节不变）。自 #909 的第三个 sentinel 起，该陈述已获证明，而非仅经验证：`FreeListRc.v` 在 allocator 旁携带引用计数（作为 `RuntimeModel.read_rc`，即字节定理写入的同一单元），证明 `double_release_traps`——free-list 块读出 0，因此 sentinel 触发——以及 alloc/store 对 `reuse_restores_rc_1`。仍需信任的范围比以前更窄，且现在通过可执行关卡检查，不再只是断言：emitter 在每个由 rc 管理的 `call $alloc` 位置写入 `RC_INITIAL` store，由 `almide-mir` 的 `every_rc_managed_alloc_site_initializes_the_rc_cell` 检查。它枚举 renderer 的全部 16 个 `$alloc` 位置，要求每处初始化 rc 单元或属于明确命名的 raw-scratch 位置（已证明的 `alloc_raw` 类别），并从 `.v` 文件读取 `RC_INITIAL`，避免常量漂移。关于共享的诚实范围现在也已实现（A1.3-render）：`Dup` 通过 `rc_inc` 共享（不复制）；`MakeUnique` 是 copy-on-write（共享时 clone——先 `rc_dec`，这样别名保持原内容存活，无需临时变量，再执行 `list_copy`），细化 `CowSafety.make_unique_yields_unique`。rc 单元现在确实跟踪抽象引用计数（1→2→1→0），运行已证明的 rc 机（`WasmRcDec`/`RuntimeModel`），同时值语义输出逐字节不变。因此 A1 的 renderer 现在完全采用真实 RC——共享／cow／free-list／重复释放 sentinel——每部分均细化一个证明，新增受信任运行时为零。尚未完成（属于性能／编码，而非安全）：free-list 仅匹配大小完全相同的表头（大小不符的已释放块尚不复用——只是错过复用，绝不导致不安全；大小分类／遍历属于后续片段）；以及指令树的原始字节编码（A2 推迟的较重一半）。
- **字节绑定是部分的。** op→wasm 指令表是形式化的 Coq 对象（`Translation.v`），运行时堆被建模为内存状态机，其 rc 单元被证明跟踪抽象引用计数（`RuntimeModel.mrun_tracks_exec`）；`validate_translation` 每次构建重新检查每个 op 的模式均被输出（drop 的模式为 `call $rc_dec`），`validate_translation_perceus` 检查每个 drop 输出一次 release。模型的 `RC_OFFSET = 0` 现在与 renderer 的物理 rc 单元偏移一致（A1.1a 重新布局），`call $rc_dec` 写入该单元。**A2 首个片段已完成（指令树层面），`WasmRcDec.rc_dec_prog_realizes_rt_dec`**：renderer 输出的确切 `$rc_dec` 指令树（建模为数据，并为 load/add/sub/store/trap 片段提供小型操作语义）被证明计算 `RuntimeModel.rt_dec`——相同的 trap（单元 0），相同的递减。因此，无泄漏／无重复释放证明使用的抽象 release，正是输出指令计算的内容，而不只是一个 token。字节编码现在也被绑定（A2 字节片段，`WasmEncode.rc_inc_bytes_encode_the_instruction_tree`）：Coq wasm 二进制编码器产生的字节，与 `wat2wasm` 为 renderer 的 `$rc_inc` 输出的字节完全一致，由 `proofs/check-wasm-bytes.sh` 在每次构建中建立对应。自 #932 起，该关卡成为没有手抄常量的闭合三角：它汇编的 WAT 从 renderer 自身渲染输出中提取，比较所用字节列表从 `.v` 源码解析，因此偏离证明的 renderer 修改会使关卡失败，而非让定理失去关联（既非循环论证，又锚定 renderer）。与 `rc_inc_prog_realizes_rt_inc` 组合后，输出字节编码了计算 `rt_inc` 的指令树。执行现在也已绑定（`WasmExec.rc_inc_bytes_execute_to_rt_inc`）：一个最小 wasm 栈机运行真实 rc_inc 字节，内存效果恰好是 `rt_inc`。因此，字节经执行后计算抽象 acquire，而不只是编码一条理应如此计算的指令。由此，`rc_inc` 实现端到端绑定：指令树 ↔ 真实字节 ↔ 执行 ↔ rt_inc，信任链抵达实际 wasm 字节。解释器还执行重复释放 trap 的控制流（`WasmExec.trap_bytes_trap_on_zero` / `_pass_on_nonzero`：`(if (i32.eqz cell) (then unreachable))` 的字节当且仅当单元为 0 时 trap——真实、已建立对应的字节上的 sentinel），因此它既处理直线代码，也处理安全关键 trap。一般结构化控制流的基础也已建成（`WasmExec.skip_block` + `imm_len`）：一个识别立即数的结构查找器，可定位块对应的 `end`，不会被与操作码冲突的立即数迷惑（`i32.const 4` = `41 04`，其中 0x04 是 `if`；`i32.const 11` = `41 0b`，其中 0x0b 是 `end`）——已针对这些确切冲突案例证明。这表明一般控制流只需一个小型的按操作码列出的立即数长度表，而非完整 WasmCert-Coq 解析器。它现已接入一般 `if` 执行器（`WasmExec.run_g` + `split_block`）：带 fuel 上限的解释器运行一般结构化 `if … end`，条件非零时执行 then 主体，否则跳过（已对 `if (cond) (then store 0:=42)` 证明主体执行／跳过），超越固定 trap 模式；另有 `global.get`/`set`（将 `$freelist` 全局量作为保留单元，已证明往返）和带索引的 locals + `local.set`（用于 `$rc` 临时值的 locals 环境）。借此，完整 `$rc_dec` 字节——包括 free-list 及 `global.get`/`set`——与 wat2wasm 建立了对应（check-wasm-bytes.sh），而 rc_dec 的两个安全性质均由一般解释器在这些真实字节上通过执行得到证明：`rc_dec_bytes_trap_on_zero`（无重复释放——release 已为 0 的单元会 trap）和 `rc_dec_bytes_frees_when_one`（无泄漏——有效 release 使 rc 单元为 0，块被释放），运行的是 renderer 的实际 `$rc_dec` 字节序列。根据信任模型（“保护的是安全，而非功能正确性”），这两个才是重要的 rc_dec 字节绑定；free-list push 的组织方式（释放块加入哪个列表）属于功能，而非安全性质。解释器执行现在双向建立对应：字节对应 wat2wasm，执行对应生产引擎——wasmtime 差分测试（`rc_cell_values_match_the_interpreter_on_-
  wasmtime`）确认真实引擎计算出的 rc 单元值与 run_g 预测相同（rc_inc 1→2，rc_dec 1→0）。因此残余从“信任 run_g = wasm 规范”缩小为“wat2wasm/wasmtime = 规范”——生产工具与工具链其余部分处于同一信任级别。尚未完成：功能性列表操作运行时（bootstrap-runtime 债务，将通过已证明路径在 Almide 中自举，#30，而非手工绑定）；以及完全位于 Coq 中的完整 WasmCert-Coq ISA，以彻底消除解释器↔规范残余，而非将其对应到 wat2wasm/wasmtime。
- **一个真实 `.almd` 现在端到端流通**（`proofs/fixtures/return_list.almd` → 实际 frontend → MIR → 已证明检查器，针对所有权与名字——周指标 ① 0→1）。lowering 仅覆盖值语义子集（堆字面量、别名、索引赋值 copy-on-write、标量／堆 move-out 返回——尚无调用或控制流，#29），因此更广泛的拒绝案例与 capability witness 仍是代表性 MIR 形态（emit_cert.rs）。
- **提取是经过交叉检查的快速路径，而非判定根**（上文第 2 项，brick 6b）：内核 oracle 重新验证每个关卡及语料的判定；已验证提取／采用 CertiRocq 是为快速路径本身记录的未来棘轮目标。
- **只有一个独立检查器。** 多样性（≥2 个独立检查器）属于 brick 6。

<a id="proven-vs-trusted-boundary-map-flight-evidence-gaps-f3-1"></a>

## 已证明与受信任部分的边界图（flight-evidence-gaps F3-1）

用一页回答审计者的第一个问题：**究竟哪些已经证明，哪些只是受信任的工程实现？**（2026-07-03——实地检查发现五个破坏输出的 bug 后编写，全部位于受信任区域。）

```
                         ┌──────────────────────────────────────────────┐
  .almd source ──parse──▶│ FRONTEND (trusted Rust)                      │
                         │  check / lower / optimize / mono / ir_link   │
                         └──────────────┬───────────────────────────────┘
                                        │ IR
                         ┌──────────────▼───────────────────────────────┐
                         │ MIR LOWERING (trusted Rust, WALLED)          │  ← the five 2026-07-03
                         │  lower_function + pre-desugars               │    bugs lived HERE
                         │  emits per-function WITNESSES                │
                         └──────────────┬───────────────────────────────┘
                                        │ MIR + certificates
                         ┌──────────────▼───────────────────────────────┐
                         │ CHECKER (Coq-PROVEN kernel)                  │  ← accept ⟹ ownership ∧
                         │  check_all_sound / names / caps-transitive   │    names ∧ caps (37 thms)
                         └──────────────┬───────────────────────────────┘
                                        │ accepted MIR
                         ┌──────────────▼───────────────────────────────┐
                         │ RENDERER render_wasm (trusted Rust;          │
                         │  rc_dec/rc_inc byte trees PROVEN — WasmExec) │
                         └──────────────┬───────────────────────────────┘
                                        │ wat
                              wasmtime (UNQUALIFIED tool)
```

图中说明译文：.almd 源码经过解析，进入 frontend（受信任的 Rust），执行 check / lower / optimize / mono / ir_link，输出 IR。MIR lowering（受信任的 Rust，有 wall）执行 lower_function 和预脱糖，输出逐函数 witness；2026-07-03 的五个 bug 位于这里。MIR 与证书进入检查器（经 Coq 证明的内核）；check_all_sound / names / caps-transitive 表示接受蕴含所有权、名字和 capability 性质（37 个定理）。已接受 MIR 进入 renderer render_wasm（受信任的 Rust；rc_dec/rc_inc 字节树由 WasmExec 证明），输出 wat；最后由 wasmtime（未经工具鉴定的工具）处理。

证明给出的保证：已接受函数不会重复释放、泄漏、留下悬空名字，或到达未声明的 capability。证明不给出的保证：lowering 选择了正确语义——由证书判定为健全的函数仍可能打印错误字符串。该缺口仅由差分证据覆盖（输出一致性 baseline + org 测试集），其覆盖范围由 `proofs/coverage.sh` 测量。

<a id="ffi-an-externs-declared-purity-is-the-authors-claim"></a>

### FFI：`@extern` 声明的纯度是作者的主张

`@extern(wasm, ..)` import 是程序外部代码，capability 证书无法查看其内部。每次调用都会记录 FOREIGN（`cert_project::cap::FOREIGN`），因此报告始终会显示外部代码可达。该代码是否有宿主 effect，由 extern 声明说明：普通 `fn` extern 仅记录 FOREIGN，普通 `fn` 的界限允许它，检查器也如此；`effect fn` extern 还记录 FOREIGN_EFFECT，只有 `effect fn` 才允许。普通 `fn` extern 的纯度受到信任，而非经过检查。这是作者的声明，与每个 FFI 边界所接受的信任相同。由 `witness_decls::tests::a_plain_caller_of_an_effect_extern_is_refused` 及 `tests/verify_shim_test.rs::a_plain_fn_calling_a_pure_extern_builds_under_the_verifier_and_reports_foreign` 固定。

<a id="the-five-2026-07-03-trusted-zone-bug-classes-and-their-regression-pins"></a>

### 2026-07-03 受信任区域的五类 bug 及其回归约束

| 类别 | 出现的问题 | 约束位置（关卡或 fixture） |
|---|---|---|
| match 线性化运行了两个有 effect 的分支 | wall 通过但输出错误 | 含调用分支的 WALL guard（`lower_branch`）+ parity baseline 中的 `mutual_recursive_types` / grammar dispatch fixture |
| never-err strip 与真实 Result 块不匹配 | 从 Result handle 读取记录字段 | parity baseline 中的 `effect_assign_unwrap`（全部五条路径）+ lifted/self strip 关卡 |
| 标量模块全局量被 lowering 为 Const-0 | 每次使用顶层 `let` 都读出 0 | const-init 实体化 + 对含调用初始化的 WALL；baseline 中的 `top_let_test` |
| 提升的 lambda 丢失 variant/global registry | `filter` 丢掉每个元素 | sub-ctx 继承（binds.rs）；baseline 中的 `closures_and_variants` |
| `$_start` 将显式 Result main 留在栈上 | 每个 Result-main CLI 都产生无效 wasm | `$_start` tag-read/drop；grammar CLI matrix 经字节验证 |

| `prim.handle(<literal>)` 解析为延迟 Const **0** | 生成的 case table 读取地址 0——每次 lookup 都静默未命中（合入时于 2026-07-03 发现） | prim 参数循环中的 literal 实体化（calls_p4）+ parity baseline 中的 `string_case_unicode` |

六例的共同模式：受信任区域内部，生产者与消费者之间的 REGISTRY 或约定（跟踪集合、布局表、调用约定、标量延迟 Const fallback）发生漂移。延迟 Const fallback（子集之外的标量尾部 lowering 为 `Op::Const` 零，调用被省略）是这一模式尚存的系统性实例——它有意以输出正确性换取 caps 覆盖率；当标量函数的输出从不被观测时，这样是可靠的，现在则不可靠。移除它（在值被观测的路径上使用 wall 而非 Const）已记录于 flight-evidence-gaps F2。结构性修复方向是 certificate-format-v1 / value-rc-cert（缩小受信任区域）；战术方向是 wall 纪律（所有消费未跟踪／未知形态的地方都必须 wall，绝不猜测），目前已在 match 线性化与 Map 表示路由位置强制执行。

<a id="use-relativized-completeness"></a>

## 相对于用途的完备性

完备性按模型与用途声明，而非绝对声明。历史 eager-copy 结果涉及其所有权片段，不是已发布 structural runtime 当前的完备性边界。轨迹模型定理、运行时细化与逐制品证书检查具有不同假设和覆盖范围。没有一项证明完整的源码到制品语义，也没有一项证明每个已发布程序都满足 `MemorySafe`。receipt 会列出其证书覆盖制品的主张及剩余受信任假设。
