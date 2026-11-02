<a id="issue-fixes-research-and-evidence-2026-09-12"></a>

# Issue 修复：研究与证据（2026-09-12）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bugfix-research-2026-09-12.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bugfix-research-2026-09-12.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bugfix-research-2026-09-12.zh-CN.md)

> 固定原文的非规范性历史读者快照。原本不变，不自动同步，未新运行所述测试、测量、CI、命令或外部参考核验。最初本地限定／未解决描述与后续集成及修复并存；历史结果不认证当前软件。末段称三类 dialect、两类缺陷，但表中列两行 dialect、三行缺陷（其中一行属同一位置）；保留该计数差异，不静默调和。预留、隔离及证明主张保留原文排除范围。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/bugfix-research-2026-09-12.md)

基线：`edbd7b751e4902e7ea3daff622757d35c71ded13`。
实现修改仍在本地；没有关闭 issue 或发布 release。
前置 C-214 judge 修订已单独提交为 [almide/als#65](https://github.com/almide/als/pull/65)。

<a id="changes"></a>

## 修改

| Issue | 修改 | 回归证据 |
| --- | --- | --- |
| #2114 | 通过既有 owned-string append helper 增长 stdin 行；在 LF/EOF 停止，保留共享 cursor，并匹配 native 的 CR 去除行为。 | `tests/io_read_line_boundary_test.rs`：native、core WASI、direct component；长度 4094–4097、8193、65537；跨旧边界 UTF-8；CRLF、EOF、后续 line/byte/drain read。 |
| #2112 | 两个 Value collection constructor 都接受 borrowed slice；复制到 owned Value payload。 | `spec/wasm_cross/value_borrowed_constructors.almd`，挂接既有 C-103：参数、空输入、nested object、snapshot 独立性；native/wasm/interpreter 一致。 |
| #2107 | named-record lookahead 忽略 comment，并接受无尾随 comma 的 record pattern 结束前 newline/comment trivia。 | `tests/record_comment_parse_test.rs`。 |
| #2106 | 将 case comment 保存为非语义 variant metadata；用既有 ExprId table 保存 expression-member comment。含 comment 的 container 按物理行打印。named 与 anonymous record 共享 field parser。 | `tests/fmt_container_comments_test.rs`：variant 写法、record、spread record、call、tuple、守恒与幂等性；既有 formatter/parser suite。 |
| #2113（部分） | E015 要求精确名称，排除 host/effect 候选，明确未比较函数体，不给替换 snippet。outline 识别 bundled `args`。direct component build 以 E081 拒绝不可用的生成 host operation，在写输出前指出 operation 和 p2/p3 target。 | `tests/reimpl_lint_best_match_test.rs`、更新 E015 diagnostic fixture、`tests/component_availability_test.rs`（p2/p3 argv 拒绝、保留已有输出、preview1 argv 接受）。 |
| #2108 | 在 CHEATSHEET 中将退役 `0..n` 替换为 `0..<n`。 | 对照周围 range 示例作 source review。 |
| #2109 | 用 E031 和 grammar 专属指导诊断被拒绝的 range-pattern、range-type 及 prefix `..` / `..=` 形式，不机械改写；保留有效 rest marker。 | `tests/retired_range_context_test.rs`：issue 复现、operator span、没有不安全 fix-it、合法 list/record rest 及当前 range 写法。 |
| #2110 | 添加带仅初始化 `_start` 的 structural library emission；要求每个 public export 及可达 callee 都能 lowering。无 main build 走该路线，普通 program emission 仍要求 main。 | `tests/wasm_library_route_test.rs`：完整 board-parser 复现、默认／强制 structural routing、export 存在、Wasmtime 调用、严格拒绝不可 lowering 的 public 函数体。既有 export DCE 回归通过。 |
| #2111 | 用 stock/embedded/component 区分及函数级 availability 正本替换旧 HTTP stub 提案。 | `proofs/target-availability.toml` 及既有 component/embedded 实现。 |

修改前后复现：旧 release binary 对 4097 ASCII bytes 返回 **4096**；修改后返回 **4097**。Value fixture 在旧 binary 上因 E0308 失败，在修改后的实现上通过。

<a id="design-references"></a>

## 设计参考

从要求的 `../almide-references` checkout 读取：

- Koka `3ac4f001ab7277b484d661fdbada1aaf8d01ecbf`、`kklib/src/os.c`、`kk_os_read_line`：反复追加固定 scratch buffer，直到 newline/EOF；buffer size 不是行长限制。Almide 保留自己的 EOF/CR 约定和既有几何增长 append allocator，而不采用 Koka error surface 或 C-string 假设。
- Go `e51216de8e26247ee0f3d2cfa576233b0d29f542`、`src/go/ast/commentmap.go`：comment 按 source 顺序关联 node。Almide 同样不把 trivia 放进语义 serialization，并区分同行 trailing comment 与 leading comment。其 `src/go/parser/parser.go` 的 `errorExpected` 也把拒绝语法落实到当前 token。E031 扩展使用外围 Almide grammar 及真实 token span；不支持形式不会获得 expression operator 的机器可应用 rewrite。
- Rust `0d31508599a7814a7044e9a7a871e3dc5f037753`、`compiler/rustc_mir_build/src/builder/expr/as_place.rs`：把 borrowed projection 转为 owned storage 使用 `to_vec()`。Value 边界遵循此所有权区别，不要求 borrowed Vec。
- Zig `lib/std/os/wasi.zig` 明确把 argv/environment 绑定到 `wasi_snapshot_preview1`；不能推断 component world 具备这些 service。component audit 使用各 direct shim 实现的 operation，保留 p3 filesystem 和 HTTP capability。`lib/std/wasm.zig` 将 function/memory export 与 host entry point 独立建模。structural library mode 遵循 `almide-mir/src/pipeline.rs` 记录的 Almide 既有 #881 ABI：`_start` 初始化 global，public function 保持为 host 可调用的 export root。
- Rust `library/std/src/thread/scoped.rs` 通过 scope exit 前 join 每个 child，将 borrowed capture 绑定到有界 lifetime。这说明消除 capture-clone 必须确立不逃逸／lifetime 边界；只观察 read-only access 对返回 closure 并不足够。

<a id="als-and-quality"></a>

## ALS 与质量

已阅读 judge 的 CONTRIBUTING 指令及既有 pin `52af321db00a452aa41d591a3ec4591354c8dbd8` 下 ledger。未修改 contract statement、contract ID 或 ALS pin。C-103 增加实现证据；双向 fixture link 和生成文档已刷新。contract gate、pinned-ID gate、syntax-element coverage gate 通过。

Codopsy **2.2.0** 使用既有配置：commissioned wasm crate 保持 **A / 91**。syntax、frontend 和 tools 仍为 A。不要将这些分数解释为全仓库 A：隔离 HEAD archive 已测得 `crates` 为 **B / 82**、`src` 为 **B / 86**；codegen cleanup 前汇总值未变；最新 `crates` 汇总为 B / 83。没有放松阈值或排除项。codegen crate 起点为 B / 89（隔离基线确认）。将不完整 variable-reference traversal 改为共同 free-variable analysis，并传播 runtime-registry I/O error 后，得分 A / 91。缺失 include file 和已声明 kernel module 现在以 source path 报错，不再无上下文 panic 或静默省略。`tests/runtime_registry_io_test.rs` 覆盖两种缺 source 情况。全仓库汇总 A 要求仍未完成。cleanup 后，全部 13 个 codegen snapshot、native fold/index-block 回归、两个 registry I/O 回归及全部 438 Almide file 通过。

<a id="validation"></a>

## 验证

- 最终 release build：`almide test` 通过全部 **438** file，**425** 经 wasm，**13** 经 native fallback。loopback TCP test 需要在 filesystem/network sandbox 外执行，并在那里通过。
- formatter/parser 回归 suite：**156** 项 Rust test 通过，包括格式化既有 single-file corpus 的副本并检查输出。
- 六项 diagnostic-harness check 通过（先更新变化后的 hint 期望，再重跑失败检查）；E015/outline 有五项 focused test。
- stdin 边界 matrix 在 native、core WASI、direct component 上通过。新 Value fixture 通过 cross-target 与 interpreter-oracle 两类 gate。
- contract ledger、ALS pin、ALS syntax coverage、wasm file discipline 和 `git diff --check` 通过；这不是运行了每个 repository CI workflow。
- #2109 后：parser/formatter/context suite 156 项通过，两个既有 range-spelling CLI check 通过，context test 加上精确 diagnostic-span assertion 也通过。重建 release 再通过全部 438 Almide test file。syntax Codopsy 保持 A / 95。
- #2113 component build audit 后：新拒绝／输出保留 test 通过，全部 12 个既有 p2/p3 component test 实际执行并通过，重建 release 又通过全部 438 Almide file。改动的 wasm-run crate 在 Codopsy 2.2.0 下为 A / 96。

<a id="remaining-work"></a>

## 剩余工作

<a id="wasi-version-verification"></a>

### WASI 版本验证

2026-09-12，官方 [WASI release](https://github.com/WebAssembly/WASI/releases/tag/v0.3.1) 为 **0.3.1**，发布于 2026-08-11。除 0.3.0 async 特性外，还要求 Component Model maps、implements 及 external-id 支持。已签入的 `crates/almide-wasm-run/wit/p3/world.wit` 明确针对 **0.3.0**。版本字符串本身既不能证明也不能否定与更新 host 兼容；完整 0.3.1 一致性尚未实证。

既有 p3 实现可运行：`cargo test --test
component_p3_test -- --nocapture` 在 Wasmtime **47.0.3** 上通过全部 **11** test，无执行 skip。覆盖 stdout/stderr/exit 一致、stdin、deterministic fan、filesystem read/write、并发 prefetch 与 cancellation、structural routing、HTTP 及 framed HTTP、transport error 后恢复。test 使用 `ALMIDE_COMPONENT_P3=1` 和 `--component`。默认 direct component 仍选 p2。声称 p3 filesystem write 待完成的旧 comment 已纠正。

<a id="open-issues"></a>

### 未解决 issue

- #2103：旧 judge 的 C-214 明确承诺 `exec failed:`。规范修订为 [almide/als#65](https://github.com/almide/als/pull/65)，commit `2490704`，在 `/tmp/als-issue2103` 准备。ALS-R5、C-214 和 missing-command fixture 一并修改，精确 timeout-fire message 保留。所有 local judge gate 通过，包括 reference independence/kernel/totality 与 runner self-test。author/verifier 记录明确声明没有独立性。required CI 通过，修订合并为 `4fa62a2211d71823b198dbd4076f881503bf4729`。单独 commit `9e29ef1af` 推进实现 pin；runtime status twins 现在实现修订后标识 command 的拼写。新进 develop 变更已实现 C-348 检查过的 byte slice。
- #2098：closure 构造 capture copy 和 enumerate 物化仍需更广优化。scalar enumerate/fold 在 tuple 不能逃逸时移除 tuple list 和逐元素 tuple 分配，用 scalar snapshot 保持 eager-source semantics。`research/benchmark/perf/spectralnorm/compare.py` 现以输出一致、warmup、九次 round-robin sample 和可选强制 ratio budget 比较两 target 的三种写法。n=1200 时，相比已签入命令式 corpus 基线，native indexed fold 为 2.36×，wasm enumerate fold 为 3.21×。中位数及限制见 README；不是优化完成主张。首轮优化后，wasm enumerate 为 129.72 ms / 命令式基线的 1.17×，此前为 365.81 ms / 3.21×。empty/scalar 及 escaping-tuple/heap-element fallback 在 `tests/enumerate_fold_test.rs` 中与 native 对照测试。全部 438 Almide file 通过；wasm Codopsy 保持 A / 91。native 同步 scalar fold 现在经已证明的即时 iterator call 借用 read-only capture；逃逸及共享可变 capture 保留既有所有权。iterator collector closure 为 Rust iterator ABI 取消装箱。indexed benchmark 在 native 现测得 58.28 ms / 1.42×，此前 97.39 ms / 2.36×。`tests/fold_capture_borrow_test.rs` 检查输出、逃逸／可变行为、空输入及没有生成 capture-copy binding。三个既有 capture/borrow suite 的六项额外 test 通过；13 个 codegen snapshot 全通过（direct-fold snapshot 已审查更新）；四项 native differential check 全通过；438 Almide file 再次全通过。更广 capture borrowing 仍未完成。
- #2113：正常 direct component build 现在以具名 diagnostic 拒绝不支持 host op。直接使用 low-level transform 的 caller 仍有 generic runtime refusal，其 stderr/exit 行为需要单独 runtime 工作。E015 与 outline 已修复。
- #2110 的 board parser 现可无 main 检查和构建（手动复现为 31,517-byte stock-WASI artifact）。export inventory 及测试 RateLimited/BoardNotFound 的调用均在 Wasmtime 通过。

<a id="integration-with-develop-at-dc2f934df"></a>

### 与 dc2f934df 的 develop 集成

初始修复和单独 ALS pin commit 先发布于 `fix/issue-regressions-20260912`，再集成新进 develop 变更。保留其检查过的 byte slice、所有权修复、process/fs diagnostic 和 stream fusion。把完整 free-variable check 移到新抽取的 statement-lowering module。capture-free fold 保留在已有 fusion 路线；过早提升会在 map/filter chain 与 fold 之间插入 collection，被 pipe-chain snapshot 捕获。

集成后，全部 440 Almide test file 通过（4,081 tests；428 WASM，12 native fallback）。36 项 focused Rust integration/snapshot test 通过。contract traceability 覆盖 348 contract 和 704 symmetric fixture；ALS pin、pass roster、file discipline、layout check 通过。Codopsy v2.2.0 在阈值不变下测得 codegen A/91、wasm A/90。这些 module 分数不认证整个 repository 为 rank A。

<a id="process-status-diagnostics-2103"></a>

### Process status diagnostic（#2103）

随已合并 ALS-R5/C-214 修订和单独 pin 推进，两个 status function 都标识调用及双引号 command，转义嵌入 quote/backslash/control character，省略参数列表。timeout twin 包含给定 millisecond bound。host error text 原样保留；deadline error 保持 `exec timed out after <ms>ms`。local Rust 参考（`library/std/src/sys/process/unix/unix.rs`，spawn 位于64–68及148–150行）保留不同 InvalidInput 和 host errno error。本实现装饰该 error，而不重构或分类。普通 process function 保留既有 quoting contract；此修订规定 status twins。

process matrix 现包括两个 status function。编译 fixture 检查 quote、backslash、CR/LF/tab、Unicode、省略 private argument 及真实 host error suffix。七项 focused test 通过，包括 orphaned-pipe deadline 回归。全部 440 Almide file 通过（4,082 tests）。仅更新修改后的 timeout fixture AST hash。此前 Value constructor fixture 也补上缺失 AST/check/run manifest entry，从其生成 AST、成功 check 和 WASM 输出测量。

首次 PR CI run 暴露 14 个既有 fixture 中过时 E015 diagnostic hash。全部保留 exit status 0。十个含修订后的 signature-only warning；四个不再 fuzzy-name 匹配（`c_sqrt`/`c_floor`/`c_ceil`、`parse1`、UFCS helper 名和 `bo`）。已审查 source 声明及此前 substring/edit-distance rule。只刷新这些测量行，不放松 diagnostic 排除项或 comparison floor。

<a id="second-pass-the-ci-runs-own-findings-same-day"></a>

## 第二轮：CI run 自身的发现（同日）

最初两次 PR run 的五个红 gate 都是真缺陷，不是 flake。

| Gate | 缺陷 | 修改 |
| --- | --- | --- |
| Commissioned wasm gates | 新 `value_borrowed_constructors` fixture 在 spine/AST manifest 中，却不在 size ledger 或 allocation ledger 中 | 添加测量所得三行，其他全部不动（全量再生成会在同一 commit 主张无关 drift） |
| Test Rust shard 3 | `pass_capture_borrow.rs` 的 `Reads` visitor 用 `_ => {}` 匹配 `expr.kind`；traversal totality lint 读形态，不读后续 `walk_expr` | 改写为 guard，语义不变 |
| Test Rust shard 0 | #2106 只要 member 带任何 leading comment 就强制物理行，破坏 #1404 中 inline `/* */` 留在 node 同行的规则 | trigger 改为“不能共处一行的 comment”（`//` 或跨行 block comment）；`fmt_expr_sans_leading` 打印 inline comment |
| Coq / TCB | 抽取 checker 在 Rocq 9.1.1 测得1,348行，在 `develop` 缓存 opam root 的旧 Rocq 下为1,196行 | 用复现 CI 数值的本地抽取（`proofs/build-checker.sh`，Rocq 9.1.1）重新生成 block |
| Emit & Format | op-31 shim 离开 transform 后，Hello, world 减少7 bytes | `scripts/gen-readme-stats.sh --measure` |

<a id="2116--ioread_all-was-bounded-by-the-park-not-the-heap"></a>

## #2116 — `io.read_all` 受 park 限制，而非 heap

变更前测量：native 可读任意大小；stock WASI 在 **257,024 B** 以 `Error: host op unsupported in the WASI build` 拒绝；p2 component 在 **326,657 B** 终止，stdout、stderr 皆空，exit 1。这是 #2113 从 Cloudflare Worker 报告的症状，在普通 `wasmtime run` 下复现。

`io.read_all` 现在从共享 cursor 按4096-byte chunk 获取 stream（op 35，由全部三个 shim 服务），追加到 owned-string accumulator，方式与 #2114 lowering `io.read_line` 相同。三个 leg 在所有测量大小一致：`tests/io_read_all_capacity_test.rs` 到1 MB，手动到100 MB。**host op 31 已退役**，从 guest、p1/p2/p3 shim、embedded host 和 `P1_SERVED_OPS` 移除。连同它移除的还有共享 unsupported-op message 的三处中的两处及 `g_pcap` global。第三处 env.set overlay 拒绝现在报出自身名称。

阅读的参考：Zig `e4cbd752c`，`lib/std/heap/arena_allocator.zig` 的 `resize`，仅在 buffer 末端与 arena frontier 一致时原地扩展，否则拒绝。此先例属于 #2117 剩余一半而非本次变更，已记录在 issue。

<a id="2117--the-accumulator-above-the-largest-size-class"></a>

## #2117 — 超过最大 size class 的 accumulator

`$alloc` 仅在低于 `16 << 15` 时向 class 取整，`$free` 放弃总量 ≥ 2^20 的 block，因此 string 超过512 KB 后每次 append 都重新分配，并泄漏每份已不足大的 copy：5.5 MB accumulator 耗尽地址空间，触发 C-197。`$str_append` 现于该 ceiling 以上请求双倍 capacity，采用 `$list_push` 和 `$map_reserve` 自身策略；以下所有内容保持 byte-identical。`io.read_all` 在0.08 s 达100 MB；`acc = acc + s` 在0.01 s 构造8 MB。`tests/string_accumulator_growth_test.rs` 要求两 leg 在32 MiB `--heap-cap` 下完成；仅回退 allocator 变更，就使 wasm leg 再次变成 `Error: out of memory`。

size ledger 在单独 commit 重新申报：197个 emitted module 支付几何 append 成本（汇总 +0.90 %）；509个 shipped module 移除 op-31 shim（每个 −149 B，codec fixture 增长后汇总 +0.59 %）。

<a id="2098--the-spelling-tax-is-now-gated"></a>

## #2098 — 写法代价现在受 gate 约束

`scripts/check-spelling-ratio.sh` 在一次 round-robin run 中比较同一 leg 上同一 program 的写法，因此比率像 native/Rust ratio gate 一样抵消 runner 影响。n=1200、五个 sample、六个 artifact stdout 逐字节相同：native `1.00 / 1.39 / 1.70`，wasm `1.00 / 0.89 /
0.72`。issue 开始时4.25×现为 **0.72×**；文档写法在 structural leg 的三种写法中最快。gate 在 perf-ratchet job 以2.5×运行，高于 native 数字并留噪声余量。

<a id="filed-not-fixed"></a>

## 已提报，未修复

同族三项缺陷经测量、复现并提报，而非仓促塞进此变更：#2118（op 32 将 `b_len` bytes 无界写入 park；当前 source 不可达，全部 stdlib caller 传8）；#2119（direct component 的 `cabi_realloc` 在 grow 失败时静默退出，违反 C-197 形式）；#2120（超过 park 的261,120-byte staging room，`env.get` 回答 `none`、`env.args` 回答 `[]`，native 却返回值；静默错误答案，也是同族最糟者）。

<a id="2120--the-park-stops-being-where-a-result-has-to-fit"></a>

## #2120 — park 不再是结果必须装得下的地方

当 environ block 或 argv 超过 park data page（精确到字节测量为261,120 bytes），native 回答值，`env.get` 却回答 `none`、`env.args` 回答 `[]`。两者都像普通有效答案，因此 program 在 stderr 为空、exit 0 时走 `??` fallback 或无参数路径。

shim 的 `host_read` 现从 `g_ppos` global 而非固定地址复制，即 p2 shim 已有的间接层。三个结果：

- 大于 page 的结果在新增长 memory 的顶端暂存。`stage_for` 增长结果所需 page 的两倍，取顶部 `need` bytes，8对齐（检查 alignment 的 host 直接拒绝奇数 pointer array；测得 `Pointer not aligned to 4`）。`fs_call` 与 `host_read` 之间 guest 自己的分配从 bump head 开始，位于旧 memory end 或以下，最多 `need` bytes，因此到不了暂存 bytes。
- overlay hit 与 environ hit 原地回答：值已在 memory 中，因此不再复制进 page，`env.args` 将整个 frame payload 下移到 `park + DATA` 的操作也消失。
- machine 拒绝 grow 时采用 C-197 形式（`Error: out of memory`，exit 1），而非错误答案。

同类实现从反方向回答同一问题。Zig（`../almide-references/zig` @ `e4cbd752c8c0`，`lib/std/process.zig`，`getEnvMap` 的 WASI arm）调用 `environ_sizes_get`，从 caller allocator 同时分配 pointer array 和 buffer，离开时释放；wasi-libc 用 `malloc` 做同样的事。每种情况目标均为 guest memory，差别在所有者。shim 不能走该路线：guest 的 `host_read` 在 `fs_call` 返回后运行，所以 shim 分配的 block 会超出 call 生存期，再无他者可释放。增长 memory 并在 frontier 上方暂存是同样答案，只把所有权留在 shim 能履行的位置：bytes 按构造是 transient，bump allocator 后续复用 page。

`tests/env_staging_capacity_test.rs` 将两 service 的 native 与 stock WASI 对照固定在16、260,000（旧 page 内）、262,000（刚越边界）及900,000 bytes，并检查三个小参数的顺序和 framing。

间接层仅随能使用它的 service 交付：697个 corpus module 中688个 byte-identical，触达 env/args 的九个增加96–101 bytes。`env.set` 的64 KiB overlay 保留自身界限——自 #2116 起是明确具名拒绝而非静默拒绝——也是最后一个 park-sized 限制。

<a id="2118--the-one-writer-into-the-span-with-no-bound"></a>

## #2118 — 唯一无界写入 span 的 writer

op 32 在 staging address 写入 `b_len` bytes，不检查任何内容，长度超过 span 就会到达 env.set overlay，继而到达活 guest block。source 无法触达它：stdlib 每个 `prim.random_get` call site 都请求8。正因如此，界限应放在 emitter 而非每个 artifact shim：`prim.rs` 拒绝非 `ENTROPY_MAX` 内 literal 的长度，所以检查不增加字节，也无 runtime branch；拒绝发生于选择 target 前，p2/p3 arm 也继承。`entropy_length_ok` 在界限两侧及计算长度形态上做单元测试；corpus lowering 保持 byte-identical。

<a id="2119--the-canonical-abis-mute-corner"></a>

## #2119 — canonical ABI 的无声角落

issue 询问能否从 `cabi_realloc` 写出 C-197 行，并把在那里写列为首选。答案是否定的，证据在 host 而非 spec 文字：wasmtime 47.0.4 的 `src/runtime/component/func.rs` 用 `set_may_leave(false)` 包住每次 lowering，注释为“此处运行期间 component 禁止调用 import”；`func/host.rs` 在 `lower_result_and_exit_call`（为 host 产生值调用 realloc 的窗口）周围也如此；`wasmtime-internal-cranelift` 的 `compiler/component.rs` 对 `Trampoline::LowerImport` 用 `trapz may_leave, TRAP_CANNOT_LEAVE_COMPONENT` 强制。因此 realloc 中 host call 不只是未证明，而是会 trap。

shim 正在这么做。对带 `--heap-cap 1048576` 的 p2 component 排空1.2 MB stdin 的测量：

```
2: wasm trap: cannot leave component instance
EXIT=134
```

所以缺陷不是 issue 所说的静默，而是 T6 禁止形式的 raw trap，把内存不足归咎于 component 结构。host 自身 source 排除了 issue option 1；option 3——将静默登记 ledger——则会登记错误事实。

**预留纪律（option 2，对 shim 服务的 op 全域化）。** 每次 landing 提前一步在允许 abort 的普通 guest code 中预留：

- `$reserve(need)` 增长 memory；若拒绝 grow，则打印与 `$alloc` 相同的 `Error: out of memory` 行并 exit 1。
- 结果经 `cabi_realloc` landing 的 import 前，shim 预留它自己为该 call 选择的界限：op 35 blocking-read 的 `a_len`、op 32 entropy 的 `b_len`。host 不能回答多于请求量，因此预留精确，realloc 自己 grow 不再可达。
- guest 侧分配（p3 read-to-end buffer、fan slot table、http body buffer）经 `$alloc`，即 `$reserve` 后同一 bump；每个 call site 只改一个 index，而非复制 size 表达式。
- `cabi_realloc` 的 grow 失败分支现为裸 `unreachable`，记录为没有 channel 的角落。返回 memory 无法支撑的 pointer 会静默损坏 guest，比终止更糟。

同类实现也以同样方式结束。wit-bindgen 的 `cabi_realloc` 调用 Rust `handle_alloc_error`，后者 abort；wasi-libc 也 abort。ecosystem 没有 guest 从该位置报告，因为没有谁能这样做。

证明外剩下一处仅由 host 选择大小的 landing：p3 preopen table，按声明的64 KiB（`PREOPEN_RESERVE`）预留。heap cap 下 preopen path 超过它的 host 仍以 trap 结束；残余已在 C-197 声明，而非留待发现。

`tests/component_realloc_oom_test.rs` 固定两方向：必须出现该行及 exit 1，而 `cannot leave component instance` 和 `wasm trap` 不得出现；已证明移除 op-35 预留会使测试失败。

<a id="2098--two-measurements-one-of-them-of-the-build-profile"></a>

## #2098 — 两项测量，其中一项测的是构建配置

issue 中的 native 数值（早先修复后，索引2.36倍、enumerate 1.68倍）来自不带 `--release` 的 `almide build`，即项目默认配置：`opt-level = 1`，无 LTO。此级别是刻意且不可缺少的：LLVM 尾调用优化在更低级别不运行，没有它，`spec/wasm_cross/mutual_tail_recursion.almd` 会使 native stack 溢出（`src/cli/cargo_build.rs` 记录发现之日）。但 LLVM 也在此级别拒绝展开迭代器链。

同一树、同一机器、同一轮转框架的测量：

| 写法 | 默认配置 | `--release` |
|---|---:|---:|
| 索引 fold | 1.42x | **1.01x** |
| enumerate fold（之前） | 1.68x | 1.37x |

因此索引写法剩余差距不是编译器生成内容造成的：opt-level 3 下，同样的生成源码与命令式形式完全匹配。enumerate 的差距经过优化仍存在，所以才是真正的发现。

下一步看似自然的推论——`almide build` 应默认用 `--release`——经不起自身测量。两种配置轮转测量时，`--release` 对 opt-level 1 已处理良好的两种写法更慢（命令式47.95对41.93 ms，enumerate 48.14对41.77 ms），仅索引更快（48.23对59.65）。opt-level 3 下三者都约48 ms；opt-level 1 下两者约42、一者约60。值得保持的性质是不同写法成本相同，而非某个配置胜出。

**enumerate 额外成本实际所在。** 手动编辑生成代码、重新构建来定位（n=1200、九个样本、轮转、输出一致）：

| 生成代码 | 比率 |
|---|---:|
| 原始生成 | 1.38x |
| 融合 `.enumerate()`，capture 仍克隆 | 1.14x |
| 融合且借用 source | **1.01x** |

两个机制都逐行发生，因此每次运行48,000次：`almide_rt_list_enumerate` 构建 `Vec<(i64, f64)>`，供 fold 遍历一次后丢弃；以及为其输入而完整复制捕获的1,200元素向量。

**修复。** SOURCE 位置的 `list.enumerate` 不是 source，而是其上的 ADAPTER。`IterStep::Enumerate` 不携带程序文本，位置来自迭代器，因此纯、全定义且保序，不为合并规则的纯度与 abort 计数增加阶段。适配后 source 前的 clone 按构造已多余：CloneInsertion 为避免消耗型 runtime call 取走调用者的值而放置它，融合消去调用也移除了消耗方。删除 clone 时需保留的例外是会修改同一变量的 callback，这一点会检查而非假定；实践中这种变量已经是读取时生成 snapshot 的共享单元，mutation fixture 在两条路径上锁定此行为。

结果：默认配置下 native enumerate 从1.68倍降为 **1.01倍**，wasm 路径不变（`IterChain` 仅用于 Rust）。`tests/enumerate_source_fusion_test.rs` 锁定借用形态、snapshot 形态、四个链位置及 native/wasm 一致性；禁用 source-adapter 规则后，两项形态断言都被证明会失败。

perf 语料现在记录两种配置并说明各自回答什么问题，CI 预算由2.5倍降至2.0倍。

<a id="2128--a-boundary-that-is-a-check-because-a-rule-alone-already-failed-once"></a>

## #2128 — 以检查实现边界，因为仅靠规则已经失败过

路线图决定：读取结构化数据（TOML、markdown、台账、生成输出）的 gate 用 Almide 编写，仅 grep 并退出的 gate 保留 bash。原因是成本不对称而非偏好：六十多个 `check-*.sh` 各自重新编写 grep/awk/sed，而 Almide 侧已付出类型化 TOML 和 markdown reader 的成本，下一个结构化 gate 可免费使用。

仅有规则不能维持边界，项目的测量已经证明：此前“不提交 `.sh`”的目标附带“数量增加则 gate 使 CI 失败”的完成标准，但未实现。六周内已提交 shell 从50变成124个文件，新增81个中50个为 `scripts/check-*.sh`。没有测量，因此无人察觉。

所以把边界加入已有台账。每个 `[[gate]]` 行现在带第二轴 `reads = structured | scalar | UNCLASSIFIED`；声明 `structured` 但路径为 `.sh` 的行会检查失败。`UNCLASSIFIED` 与台账的 `UNVERIFIED` 一样如实表示债务，双向只许减少，因此无需等现有80行全部分类才落地机制。

**Almide gate 在台账中完全不存在。** 枚举范围为 `scripts/check-*.sh` 加两个指定脚本，所以 Almide gate 根本无法拥有验证类别。现在也从 `tools/almide-gates/src/main.almd` 读取子命令分派，以 `tools/almide-gates:<subcommand>` 寻址。直接从 dispatch 读取，使两者无法漂移，并设下限以在解析失明时失败。

可见性付出的代价是上限：`output-parity` 与 `fuzz-track-record` 进入视野时尚无相对原 `.sh` 的验收检查，因此 `unverified_ceiling` 从0升到2。债务不是新的，#2130 正为此开放；台账看不见的 gate 比记录为未验证更差。

落地前演示三种失败方向：`.sh` 路径的 `structured` 行、没有台账行的 Almide gate、缺少 `reads` 字段的行。也演示棘轮双向生效：分类一行使数量低于上限后，gate 要求下调头部上限。

检查自身也在80行之中。它用 bash 读取 TOML 台账，因此按自身边界应归入 Almide；诚实债务分类正是用来如实说明这一点的。
<a id="2134--one-mistake-two-codes-and-only-the-indirect-spelling-readable"></a>

## #2134 — 一个错误、两个代码，只有间接写法可读

`none()` 报告：

```
error[E001]: type mismatch in function call: expected Option[?0] but got fn() -> Option[Int]
  hint: Fix the expression type or change the expected type
```

同一错误经绑定书写时，已经给出有用诊断：

```
error[E002]: `f` is not a function — it has type Option[?0]
  hint: `f` is a value; only functions and closures can be called
```

所以诊断已存在，只是直接写法无法到达。`none` 是 `ExprKind::None`，进入 call dispatch 的计算型 callee 分支，而该分支仅做 `constrain(callee_ty, fn(args) -> ?ret)`。读者看到的是这次合一的两边；当期待类型为推断槽时，未解析的 `?ret` 又产生一个误导性的 E025，要求添加类型注解。一个笔误、两个错误，第二个还指离修复方向。

callee 位置的值现在无论出现在哪里都为 E002，`none` 获得专属修复行及 `try:`（整个错误就是括号；恰因 `some` 接受参数，对称写法才显得合理）。E025 连锁随之消失：没有约束，就没有未固定的内容留下。

反方向是同一形态的镜像，名称距离建议对两者都无用，因为没有任何名称拼错：

| 实际 | 期待 | 发生了什么 |
|---|---|---|
| `fn(A) -> T` | `T` | 从未提供实参（以 `some` 代替 `some(x)`） |
| `fn() -> T` | `T` | 从未进行调用 |

两者仅凭类型即可决定：实际类型是函数，其结果类型头与期待类型头匹配。因此 `arity_shape_hint` 加入 E001 提示链，与既有 fallible-callback shape hint 并列。

C-237 自身陈述写了旧代码（`none()` 是 E001），括号中的 `(none is a value, not a function)` 却描述 E002 消息而非 E001；台账在同一变更中纠正。

<a id="2130-step-1--the-diff-that-was-specified-and-never-built-and-what-it-found"></a>

## #2130 step 1——规定了却从未构建的 diff，以及它的发现

Unit 0.46 将七个关卡移植到 `tools/almide-gates/`，明定验收检查是与原 `.sh` 逐字节一致。CI 只运行 `.sh` 一侧；对应实现由 `almide test tools/almide-gates/` 运行，却从来没有比较过两者。工具自身的注释点明代价：“新鲜度关卡比较 bash 与已提交文档，从不将移植版与其中任何一方比较”。

**diff 第一次运行就发现漂移**，这正是正式启用任何移植版之前应先构建它的全部理由。`stamp` 的 FATAL 路径——PATH 中二进制与 workspace build 不一致——有两处偏离：

- bash 已能说明原因（上次 `make install` 后，`cargo test` 重新链接了 `target/release`）及修复方式；移植版仍是最初的一行；
- bash 在此返回，因此该路径不打印 toolchain 行和末尾分隔线；移植版却继续打印所有内容。

任何一处都会在对应实现正式启用当天交付不同的证据文本，而 FATAL 路径恰是开发者真正会遇到的。移植版现在在两条路径（不一致分支与普通列表）均逐字节匹配；为此需用结构表达提前返回，而不能用 `return`——Almide 没有它。

`check-contracts` 原本就逐字节一致，包括 exit code。

仍有两个对应实现未纳入比较，明确列出而不伪装完成：`output-parity` 扫描整个语料，`fuzz-track-record` 查询 GitHub API，因此各自都需专门 fixture 设计，diff 才有意义。没有正式启用任何移植版，也没有删除 `.sh`——那是稳定观察之后的 step 3。
<a id="2133--not-an-oom-a-case-payload-that-never-took-ownership"></a>

## #2133——不是 OOM，而是从未取得所有权的 case payload

报告为 273 字节输入触发 `Error: out of memory`。develop 上已经变成 `wasm trap: out of bounds memory access`，故障地址为 `0x30384238`——四个可打印 ASCII 字节，正是字符串 payload 被当成块 handle 送入 `$rc_inc` 时的模样。两种表现都不是缺陷本身，而是它的下游结果。

**缩减**通过删除报告者树中的内容完成，而非猜测：`report.render` → `match j.verdict` → `Supported` 分支 → 一个 `probe_rows(matched)` → 内部 `list.map` → 最终完全离开那棵树，缩至十三行：

```almide
type Probe = { kind: String }
type Verdict = | Supported{ matched: List[Probe] } | Nothing

fn of(xs: List[String]) -> List[Probe] = list.map(xs, (f) => Probe { kind: f })

fn verdict_of(xs: List[String]) -> Verdict = {
  let checked = of(xs)
  Supported { matched: checked }
}

fn tags(v: Verdict) -> List[String] =
  match v { Supported{ matched } => list.map(matched, (p) => p.kind), Nothing => [] }

effect fn main() -> Unit = println(list.join(tags(verdict_of(["a", "b"])), ","))
```

Native 打印 `a,b`。wasm 路径打印已释放内存的字节，**两者均 exit 0**——因此用户遇到的 trap 形态再缩小一步，就是静默错误答案。差分语料正为此类问题而设，却没有对应单元。

**输出直接说明了问题。** `verdict_of` 渲染为：

```wat
local.get 7
local.get 1            ;; checked
i32.store offset=20    ;; the payload store — no rc_inc anywhere near it
local.get 7
local.get 1
call 48                ;; the epilogue releases checked
```

代码注释译文：local.get 1 是 checked；payload store 附近完全没有 rc_inc；epilogue 释放 checked。

**原因**是 `lower_named_record` 的 variant 分支少了一次调用：下方十行的 record 分支通过 `self.rc_share_guard(fexpr, fty)` 存储每个字段，record 形状的 CASE 分支却没有——显式字段和声明的默认值都没有。因此，`let` 绑定的堆值移入 payload 时未存入共同拥有的 `+1`，而 frame 仍拥有该绑定；epilogue 便释放了 case 持有的块。

只有 `let` 写法有此问题。`Supported { matched: of(xs) }` 将 producer 内联，新值带着自身 credit 移入，因此始终正确；TUPLE 形状构造器（`lower_variant_ctor`）从编写之初就带 guard。藏在写法而非特性背后的缺陷，按特性组织的 fixture 无法发现，因此新 corpus fixture 覆盖 `let` 形式、default-field 形式，以及所有 producer frame 返回后再读出该形态的情况。

**证据是差分测试，而非 corpus fixture，原因在于关卡。** 该 fixture 需要返回 heap variant case 的函数，而原有 brick 根本无法 lowering：不论 exported 还是 `local`，有无 `match` 都不行。因此放入 `spec/wasm_cross/` 会同时触动三个只能缩减的棘轮：host 与 browser determinism 的 wall 上限（各 28 -> 29），以及 `proofs/walled-real-baseline.txt` 的两个新条目；后者自身关卡说明新增条目是“需要审查的退步，而非修复”。为获得 `tests/variant_case_payload_ownership_test.rs` 已提供的覆盖——构建、运行并比较两条路径——而放宽三个棘轮，是不划算的交换，关卡拒绝是正确的。fixture 被撤回，上限恢复。
<a id="2129--the-three-mis-reads-were-the-bash-regexes-and-two-real-ones-were-elsewhere"></a>

## #2129——三次误读来自 BASH 正则，另外两处才是真实缺陷

issue 引用的注释——

> 写成 scanner 而非正则，因为在这个 Unit 中，正则已经三次静默误读输入。

——位于移植 bash 关卡时编写的 `tools/almide-gates/src/ledger_coverage.almd`。Unit 0.46 日志列出了所指三例，全部是移植版必须复现的 bash pattern，不是 `stdlib/regex` 调用：用空格敏感 pattern 匹配 `spec = "..."` key，导致空格不同的行静默丢失；`grep -qE "^## $sec"` 没有边界选择项，使 `ALS-T1` 匹配到 `## ALS-T14`，无效 key 得以通过；以及剥离后以行顺序作为 rank，将注释行算成 class。

因此，issue 按原文写出的 step 1 没有目标。它说对的是这个缺口：`regex` 是这里无人信任的模块，却没有测量来判断这种不信任是否有依据。于是先进行测量。

**与外部引擎比较 5,922 个案例**（Python 的 `re`，使用特意选取、令各参考引擎相互一致的子集），发现 **5 类共 114 个分歧**。三类是方言，两类是缺陷：

| 类别 | 判定 |
|---|---|
| `split` 返回 capture group | 方言——Python 包含它们，Rust/Go/Almide 不包含 |
| 空选择分支（`\|ab`） | 方言——Python 3.7+ 会在空匹配位置重新尝试非空匹配；**已对照 ripgrep 验证 Rust 与 Almide 一致** |
| `[]]` 从不匹配 | **缺陷** |
| 在可匹配空字符串的 pattern 上执行 `split` | **缺陷** |
| `split` 丢弃末尾空字段 | **缺陷**（同一位置） |

**`[]]`。** class 首位的 `]` 是字面量成员——POSIX，以及 PCRE、Python、Rust、Go、JS 均遵循此规则。若把它当成终止符，pattern 就变成空 class 加多余的 `]`：无错误、无匹配，从外部看不见异常。这正是注释不信任的形态。

**零宽匹配上的 `split`。** 循环用同一个 cursor 表示两件不同的事——下一个字段从哪里开始、下一次搜索从哪里开始——于是零宽匹配吃掉一个字符：`split("^", "abc")` 返回 `["a", "bc"]`，把 `a` 移出了所属字段。Python、Rust、Go 都返回 `["", "abc"]`。分离 cursor 并始终输出末尾字段（`split("$", "a")` = `["a", ""]`）后，114 个分歧降为 6 个，全部属于那一个已确认的方言类别。

两个引擎都包含修复——`runtime/rs/src/regex.rs` 与自举对应实现 `stdlib/regex_engine.almd`——因为 C-032 要求两路径逐字节一致；只修 native 一侧会立即使该 fuzz 失败，说明关卡在发挥作用。


<a id="the-exclusion-that-removed-more-than-it-meant-to"></a>

### 移除了超出本意内容的排除

把两个 regex fixture 排除在 run-parity manifest 外看似正确——固定 oracle 记录了修复前答案，不能裁判它们——但这是只有关卡才看出的错误。run manifest 也是 exercised-surface、allocation、size 扫描的语料列表，因此删一行会从全部扫描中移除该 fixture：`surface_matrix` 失败，点名 wasm 路径实际覆盖 surface 中消失的八个构造——`regex.captures`、`find`、`find_all`、`full_match`、`is_match`、`replace`、`replace_first`、`split`——因为只有这两个 fixture 在那里运行该模块。排除它们，就会在交付修复的同时，静默撤掉所修对象的覆盖。

`scripts/lib/run-oracle-stale.txt` 是现有 register 未提供的精确机制：行仍留在 manifest，其他每项扫描仍遍历 fixture，仅跳过对 oracle 答案的比较。这种比较有三个而非一个——`run_parity`、`backend_parity` 和 WASI 关卡都比较同一 hash，第一个修好后，第二、第三个依次失败。reader 位于 `almide_corpus`，使第四种比较不能悄悄忘记它；仅将 manifest 用作语料列表的四项扫描（exercised surface、allocation、size、witness floor）按设计不受影响。它双向只能缩减：某条登记重新与 oracle 一致时会失败；用一条伪造行指向本来就一致的 fixture，在全部三种比较中验证了这一点。它们的分配 watermark 下降（从 88,144 到 87,824；从 291,248 到 290,608——split 修复分配更少字段），并在这次变更中确认。

**常设关卡是持久的那一半。** C-032 的 oracle 是我们自己的 native 引擎：它确认两路径一致，却看不到双方以相同方式误读同一规则，两个缺陷正因此存活。新的 `scripts/check-regex-reference.sh` 在 0.15 s 内向两路径提出外部引擎已回答的 6,624 个问题，子集及刻意排除项在 `proofs/regex/README.md` 明示。在登记前已证明，伪造参照行会使两路径都失败。
