<a id="ci-critical-path-measured-baseline-peer-survey-and-the-ranked-plan-2026-09-21"></a>

# CI 关键路径：实测基线、同类仓库调查与排序计划（2026-09-21）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ci-critical-path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ci-critical-path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ci-critical-path.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-critical-path.md)

> 原文历史注记：带日期的只读CI研究，不代表当前GitHub价格、仓库权限、必需check、plan可见性或新测量。#2444后的初始下限是估计，随后才有66→43.4→25.1分钟实测及笔记本权重的后续纠正。同类调查的概括和例外均保留，“仅rust/lean4”不抹去hexa或100% self-hosted Zig行。后加的C step删除（#4023）与旧A+C算术并存，不暗中重算。历史门禁、权限、付费runner、workflow提案不是执行授权。未执行命令、修改远端设置、产生费用或修改CI。


#2381 背后的研究记录（PR #2440、#2444 及后续）。只读调查 ../almide-references，并逐 job 清点 ci.yml；所有节省量均由第0节指定的实测 job 时间计算。每一步落地后的实测结果得到后追加在第4节之后。

只读研究。基线是分支 `ci-2381-solo-giants`（worktree `.claude/worktrees/ci-2381-critical-path`），包含 PR #2440（ledger 移出串行 nextest group，同族每 shard 一个）和 PR #2444（五个巨大 target 分为六个 `Test Rust (solo …)` job）。下列数字均来自指定 run 的 GitHub Jobs API、该 worktree 文件，或明确标记的估计。

<a id="0-measured-baseline-what-the-plan-is-arithmetic-against"></a>

## 0. 实测基线（计划计算的依据）

Develop run 35569545602（今晚前最后一次 green develop，端到端66分钟，06:41:34→07:46:59），逐 job（开始→完成）：

| job | 墙钟时间 | 注释（≥20 s 的 step 时间） |
|---|---|---|
| Test Rust (shard 0/4) | 60.0 min | `Cargo tests` 3531 s = interp_abstain_ledger 1666 s 后接 interp_bridge_fallback_ledger 1586 s，串行 |
| Test Rust (shard 2/4) | 41.1 min | interp_oracle 1223 + opt_parity 1075 串行（family group） |
| Test Rust (shard 3/4) | 27.9 min | |
| Commissioned wasm gates | 27.8 min | carried gates `cargo test --release` 679 s → release-only root tests 436 s → double-free sweep 76 s → target-availability 426 s → ratio 25 s（1668 s 中1642 s，严格串行） |
| Test Rust (shard 1/4) | 25.6 min | |
| Almide gates | 15.4 min | reachability ratchet 485 s，parity goldens 140 s，spec tests 135 s，WAT audit 50 s，其余每项<30 s |
| WASM browser-ABI determinism | 11.4 min | 单个654 s step，编译受限（harness crate 的 wasm-pack build，`actions/cache` 以 emitter 源码为 key） |
| WASM host-arch determinism | 8.6 min | 单个450 s step，同型（native + wasm32-wasip1 harness builds） |
| Emit & Format | 7.6 min | pass-order shuffle 183 s，stability closure 93 s，约40个不足一分钟的门禁 |
| Commissioned aviation quality | 7.4 min | codopsy build（缓存）+ score |
| Test WASM (Windows host) | 6.0 min | |
| Perf ratchet | 6.0 min | |
| Build (Linux) | 5.4 min | cargo build 138 s，nextest archive 86 s，rustc ratchet 27 s，clippy ratchet 37 s，+ checkout/toolchain/cache/upload |
| Test shards cover every target | 2.4 min | |
| Lean / Test WASM / Test ARM / ratchet-separation / coverage / changes | 每项 ≤ 0.7 min | |

表中 shard 名使用该 run 从0开始的编号。2026-09-22 改名为 `Test Rust (shard 1/4)` … `(shard 4/4)`，因此此处 `shard 0/4` 就是今天的 `shard 1/4`。

PR #2440 run 35581361256：09:05:52 创建 run，09:34:44 首个 job `Change class` 才启动，即任何工作之前有 **29分钟 runner 容量等待**。`build` 09:40:51 结束，shard 09:42:38 被领取（负载下每次 `needs` 跳转又增加约2分钟排队）。shard 运行33–44分钟，各巨型 target 比独跑慢1.4–1.9倍（争用）。

巨型 target 单跑时间（计划采用的数字）：interp_abstain_ledger 1666 s，interp_bridge_fallback_ledger 1586 s，wasm_cross_target_spec 1377 s，run_parity 1310 s，interp_cross_target_spec 1223 s，wasm_opt_parity_spec 1075 s。其余合计约6.5 ks（`scripts/ci-test-weights.txt` 含巨型项共15,964 s；次大 `diagnostic_harness_test` 为1501 s，是7-test fixture loop，见§2.5）。

**#2444 后预计下限（尚未测量，分支没有 run）**：build 5.5 + solo 准备约2 + 1666 s（27.8分钟）≈ **35–36分钟**，Commissioned wasm gates 紧随其后于5.5+27.8≈33分钟结束。注意 ledger 的1666 s来自有争用的 shard；真正单跑时间要等#2444运行才知道（interp侧与oracle在其1223 s总时长内计算的函数相同，因此隔离后的ledger应明显少于1666 s）。

今天 develop 的 required checks（API）：Build (Linux)、Test WASM、Test ARM、Emit & Format、WASM host-arch determinism、Lean Proofs、Test Rust (shard 1–4/4)、Test shards cover every target、Almide gates、WASM browser-ABI determinism、acceptance-ring 名称。**非必需：Commissioned wasm gates、Commissioned aviation quality、Perf ratchet、Test WASM (Windows host)、Coverage ratchet、Ratchet-commit separation、Test Rust (solo …)**（solo仅经coverage job代理step覆盖）。develop merge queue ruleset：`REBASE`、`ALLGREEN`，最多5 entries build/merge，check响应超时120分钟。develop保护无PR审查要求、无push限制、不强制admin，技术上可直接push。

<a id="1-peer-survey-what-the-reference-repos-actually-do"></a>

## 1. 同类仓库调查（参考仓库实际如何做）

来源：`almide-references/` 下每个仓库的 `.github/workflows/*.yml`。`RESEARCH-neighbours.md` 除与wado的一行拓扑比较（60行）外无CI内容。swift和moonbit-compiler无CI配置；zig CI在 `.forgejo/workflows/ci.yaml`。

| repo | 语言／套件 | runner规模 | 缓存 | 测试并行 | PR与merge/nightly | merge_group | 时长／超时 |
|---|---|---|---|---|---|---|---|
| **aver** | Rust；15集成target实测约2,066 s；33 cert lanes | ubuntu-latest ×11 + windows1 | 每job rust-cache，binaryen/wasm-tools/elan/prelude用 `actions/cache`，无archive交接 | 按实测秒数LPT装箱（`tools/ci_test_shard.py i 4`），再在**两个分割target内用 `cargo nextest --partition slice:i/4`**；cert lanes由JSON的 `-E 'test(/…/)' --partition slice:` 产生，`decide` job证明分区不相交且穷尽 | PR=ci+5 smoke cert lanes（约5分钟）；全33 lanes每6小时+release/**；fuzz/rust-codegen仅nightly；proptest PR256 cases、nightly2000 | 无 | “330分钟内核工作分30lanes，每PR排队约40分钟”→移出PR；ci.yml无超时 |
| **vera** | Python，12,290 pytest tests | ubuntu + macos-15/26 + windows + arm（13 cells） | 无 | pytest-xdist `-n auto` | stress suite nightly + path-filtered PR，只有PR cancel-in-progress | 无 | suite3–4分钟；coverage cell8→3分钟 |
| **jacquard** | OCaml/dune | ubuntu-latest | 无 | `cc: [clang, gcc]` | PR+main全跑，main push从不取消 | 无 | `docs/ci-cd.md` 记录60/90/30/240分钟超时 |
| **hexa** | 自托管语言，约32 bash gate workflows | hosted混合 + **self-hosted pool**，hosted fallback action | source hash为key的ccache | 跨workflow fan-out，xargs逐文件上限 | PR严格 `paths:` filters，main不筛选，只取消PR | 无（aggregator `selfhost-gates-summary`） | darwin self-hosted队列**2.7分钟slot等待中位168.8分钟（p90 230.7）** |
| **sui** | Python | 3×3 matrix | 无 | 无 | PR+main | 无 | 无 |
| **vibe** | selfhost compiler，495-file unit battery | ubuntu-latest ×29 | 约25个 `actions/cache` key；**compiler只构建一次，以16 MB artifact交给14jobs** | **8 shards，LPT权重（`scripts/unit_test_weights.tsv`），`VIBE_UNIT_TEST_SHARD=i/8`** | coverage仅main；无concurrency block | 无（aggregator `ci-required`） | 预算“CI必须小于5分钟”；cold battery5m20s→4m11s；“单次构建将job时间7879→4971 s，却损失墙钟时间”；job开始中位365 s对3 s |
| **wado** | Rust，4,671 fixtures | ubuntu-latest ×20 | rust-cache `shared-key: ci, save-if: false`；lockfile改变时**独立 `cargo-cache.yml` 发布cache** | `level: [O0..Os]` matrix + 3 sibling jobs；EMI nightly `WADO_EMI_SHARD=i/8` | `changes` docs-only skip job（不用 `paths-ignore`，避免必需check卡住）；EMI仅nightly | 无（aggregator `test`，`if: always()`） | 每job `timeout-minutes: 60` |
| **koka** | Haskell，约718 `.kk` cases | 5-OS matrix | stack-action、vcpkg | 无 | PR/dev相同，cancel-in-progress | 无 | 无 |
| **lean4** | C++/Lean，通过ctest约4.4k test files | ubuntu-latest + **self-hosted `chonk`，带 `runner-fallback-action` → nscloud 8×16 / 32×64 / macOS 6×14 / Windows 4×16** | ccache + sha为key的stage1 olean cache，外部Lake cache | ctest `-j$NPROC`；regex `-E/-R` 子集；primary与 `matrix-secondary` | **标签驱动check-level**：PR仅Linux，macOS/Windows/sanitizers仅merge_group/nightly/tags | **有**（aggregator `all-done`） | 未明示；cache网络step超时20分钟 |
| **roc** | Zig，定制MiniCI | hosted；benchmark self-hosted；arm shards禁用（网络掉线） | 只有Zig deps cache有效（`.zig-cache` 超2 GiB）；**`minici-build.tar` 一次构建供7shards** | **命名边界range分片**（`--minici-from/-to/-after/-before`） | **PR每OS minici，nightly全套件**，通过时fast-forward `nightly` 分支 | 无（aggregator `finish`） | shard超时120/180 |
| **rust** | Rust+LLVM | ubuntu4c免费；**hosted4/8/16-core**，arm8-core，windows8-core；**AWS CodeBuild8c/36c、EC2 c8a.8xlarge** | sccache→S3；ghcr Docker cache | 约90-job matrix；手维护编号shard（`x86_64-gnu-llvm-22-{1,2,3}`） | **bors：PR12jobs约40分钟，auto约90jobs约2小时** | bors（非merge_group） | PR约40分钟、auto约2小时、约10 merges/day、rollups |
| **gleam** | Rust | hosted | rust-cache `key: v1-<target>`；binary artifact→e2e job | 7-way OS matrix | PR/main相同；docs `paths-ignore` | 无 | 30/10 |
| **grain** | OCaml | hosted | esy cache用 `lookup-only` warming job（避免一次cache build阻塞所有target） | 3 OS × {native, js} | PR/main/queue相同 | **有** | 无 |
| **wasmi** | Rust | hosted | rust-cache；固定nightly保持cache温热 | 仅Miri下nextest | 每PR全跑；Miri spec nightly | 无 | 无 |
| **spacewasm** | Rust | hosted含arm | rust-cache **`save-if: main`** | matrix | Miri integration仅daily | 无 | 无 |
| **plumbum / xonsh / zod / sh** | Python / TS | hosted | uv cache／无 | matrix | PR/main相同 | 无 | 无；xonsh每test `--timeout=600` |
| **zig** | Zig | **100% self-hosted**特殊target | 故意不用 | 16并行jobs | riscv64仅push；只取消PR | 无 | 120–600分钟超时 |

对Almide重要的共同事实：

- **只有一个同类项目aver使用nextest `--partition`**，且仅用于单个test *case* 占主导的场景，正如Almide巨型项。除Almide自身外，没人用 `test-groups`、`slow-timeout` 或archive。
- **“构建一次，artifact分发”是共同模式**（vibe、roc、gleam、grain、Almide的 `almide-linux` + nextest archive）。vibe测到了陷阱：job时间减少37%，墙钟时间却增加，因为每consumer承担artifact下载及runner排队下的 `needs` 跳转（job开始中位365 s）。
- **长套件同类项目真正采用的主杠杆是分层门禁**：aver（cert330分钟→PR5-lane smoke）、lean4（PR check-level0）、roc（PR minici、nightly全套件）、rust（12PR jobs对90auto）。全部接受某类失败只在PR后（queue/nightly/bors）发现。suite2–5分钟的vera/vibe/zod/sui随处全跑，其方式不能迁移。
- **大型／self-hosted runner**只由rust和lean4使用，且均带hosted fallback；hexa darwin pool是警示（2.7分钟job等待中位168分钟）。
- **必需aggregator job**（vibe `ci-required`、wado `test`、hexa `selfhost-gates-summary`、roc `finish`、lean4 `all-done`）是应对“matrix改名悄然取消必需check”的近乎通用解。
- **每job设超时**（wado全部60，jacquard逐job文档，roc逐shard）。Almide ci.yml只在Windows/cross/coverage用 `timeout-minutes`；挂起的巨型项耗掉默认360分钟及一个runner slot。
- 除rust（PR约40分钟、auto约2小时）和vibe（5分钟预算）外，同类项目未记录CI时长。rust显示编译器40分钟PR lane被视为正常；vibe显示每fixture编译1–6 s的495-test逐test编译套件的成本，与Almide corpus的fixture单价类似。

<a id="2-almides-ciyml--job-inventory-duplicates-fan-out-candidates-shardability"></a>

## 2. Almide ci.yml — job清单、重复工作、fan-out候选与可分片性

<a id="21-jobs-name--needs--what-it-does"></a>

### 2.1 Job（名称→needs→工作）

| job | needs | 工作 |
|---|---|---|
| changes | — | docs-only分类（0.1 min） |
| ratchet-separation | — | 逐提交ratchet/impl分离（0.2） |
| build | — | `cargo build --release`，nextest archive（`ci-test` profile），rustc+clippy棘轮，上传 `almide-linux` + `almide-tests-archive` |
| shard-coverage | changes, build, test-rust-solo | 证明shards ∪ solos == 全targets，ledger侧分割其binary；代理“全部solo通过” |
| test-rust ×4 | changes, build | 对本shard的binary id重放archive（packer在 `scripts/ci-test-shard.sh`，weights文件） |
| test-rust-solo ×6 | changes, build | 每个巨型项一个，相同准备（toolchain、nextest、archive、wasmtime v47、apt binaryen） |
| almide-gates | changes, build | registry再生成（`cargo build -p almide-codegen`）、parity goldens、`almide test spec/ --target rust`、examples、fallback ratchet、wall corpus、reachability ratchet（485 s）、WAT audit（`cargo build --example`）、dogfood、README block、frees churn |
| test-arm | changes | macos-14 NEON test（0.3） |
| test-wasm | changes, build | `almide test spec/ --target wasm`（0.5） |
| test-wasm-windows | changes | Windows parity子集（6） |
| wasm-host-determinism | changes | wasm32 lib `cargo check` + harness native和wasm32-wasip1构建 + fixture loop（8.6） |
| wasm-browser-determinism | changes | wasm-pack harness + node fixture loop（11.4） |
| lean-proofs | — | sorry/axiom grep、3次 `lake build`、conformance再生成（0.7，缓存） |
| coverage-ratchet | changes, build | 除非crates/src/runtime/spec/stdlib改变，否则按相关性跳过；否则约14分钟插桩重建 |
| checks (Emit & Format) | build | binary和repo上约45 shell gates（7.6） |
| perf-ratchet | changes, build | valgrind + wasmtime；三个ratio gates（6） |
| build-cross / corpus-cross / test-rust-cross | 仅面向main的PR | macOS/Windows |
| commissioned-wasm-gates | changes, build | 两次 `cargo test --release` 编译 + 三个script gate，串行（27.8） |
| commissioned-aviation-quality | changes | codopsy A + 800行上限（7.4） |
| commissioned-mutation-gate | changes | 仅PR：增量scope，function-overlap mutant优先、已记录killer suite优先，28分钟预算并列明延期余项（#2814）；scope无mutant时0.5。全扫为develop的 `mutation-sweep.yml` |
| trigger-playground | 仅main push | |

完整run的runner-slot数：4shards+6solos+其他15jobs≈ **25 jobs**。queue `max_entries_to_build: 5` 意味最多5并行queue runs，加PR runs及develop push run，即150+slots面对标准runner并发上限（Free org20、Team60，本token无法读取org plan）。29–37分钟等待源于此，而非任何单个job。

<a id="22-work-done-more-than-once"></a>

### 2.2 重复执行的工作

| 重复工作 | 位置 | 成本 |
|---|---|---|
| **每次queue merge还触发develop完整 `push` run**（今日10:06的run 4a3d9dd4b即例） | `on: push: branches: [main, develop]` | 每merge PR多一个25-job完整run，是runner-slot压力最大单一来源。queue的 `merge_group` 已测完全相同tree（REBASE，group合入时tree==develop tip）。注意§4限制 |
| **每run构建corpus table 3次**：`corpus.rs::build_corpus` **无条件**构建全部722fixture的native+wasm+wasm-opt+interp；`wasm_runtime_cross_target`、`_opt_parity`、`_interp_oracle` 各造一份。opt-parity不读native/interp，cross-target不读wasm-opt/interp，oracle不读wasm-opt | `tests/wasm_runtime_test_parts/corpus.rs:51-125` | 约3×1100–1400 s runner时间，约一半为读取gate弃置的侧（未逐侧测量；native每fixture一次rustc编译，较昂贵） |
| **interp侧又被计算3次**：两ledger tests（`interp_abstain_ledger`、`interp_bridge_fallback_ledger`）对全corpus调用 `run_interp_capture(_with_fallbacks)`，oracle也在table内算同侧 | `tests/wasm_runtime_interp_ledger.rs:76-84, 273-284` | 同一次sweep的两个consumer耗1666+1586 s |
| wasmtime install | 14jobs（v47.0.3缓存tarball）+2jobs用**不同pin v27.0.0**（almide-gates、commissioned） | 每个约10 s；pin分裂更是正确性隐患而非成本 |
| `apt-get update && apt-get install binaryen` | 4shards+6solos+almide-gates+checks=12jobs | 每个约30 s、约6 runner-min/run，flaky半边是 `apt-get update` |
| 从 `get.nexte.st/latest` 安装 `cargo-nextest`（**未固定**） | build、4shards、6solos、shard-coverage=12jobs | 每个约5 s；run中途nextest发布可使 `build` 与consumer的archive格式不符 |
| `almide-tests-archive` 下载 | 11jobs | 每个约35 s |
| rust toolchain安装 | 12jobs（archive consumer只为 `cargo nextest` shim所需） | 每个约20 s |
| release-profile workspace编译 | commissioned编译6crate tests（含运行679 s），再编root的3个release-only tests（含运行436 s）；coverage-ratchet插桩编译；determinism编译harness（cache key用emitter hash，MIR PR次次miss） | 只有commissioned编译在关键路径 |

<a id="23-jobs-that-could-run-on-merge_group-only"></a>

### 2.3 可仅在 `merge_group` 运行的job

按“PR作者在queue之前也无法对结果采取行动”选候选：`commissioned-wasm-gates`（27.8）、`wasm-browser-determinism`（11.4）、`wasm-host-determinism`（8.6）、`perf-ratchet`（6.0）、`test-wasm-windows`（6.0）、`commissioned-aviation-quality`（7.4）。但见§4第6项：GitHub不能要求一个queue check却不要求相同PR check，因此queue-only lane需aggregator。repo自身记录的偏好（commit eb48f630b：“今天各耗一次CI轮次的两个门禁”）是在PR lane增加而非删除gate。为完整性列出，排序较低。

<a id="24-independent-steps-inside-commissioned-wasm-gates"></a>

### 2.4 `Commissioned wasm gates` 内独立step

五步只共享release target目录：

- A. `cargo test --release -p almide-wasm -p almide-wasm-run -p almide-spine -p almide-syntax -p almide-corpus -p almide-layout` — 679 s。
- B. `cargo test --release --test embedded_cross_test --test module_type_repr_test --test wasm_vm_parity_test` — 436 s（root crate release-test编译+运行）。
- C.（#4023后删除）double-free trap sweep原在trap开启下重跑 `backend_parity` 和 `gauntlet`，76 s。现在每个wasm build都含trap，所以A在其下运行。
- D. `scripts/check-target-availability.sh` — 426 s，只需下载binary，无需cargo。
- E. `scripts/check-wasm-runtime-ratio.sh` — 25 s，只需binary。

D+E无编译依赖，可立刻移出（独立job或移入已有binary/wasmtime的 `almide-gates`）。A+C与B可拆两job，各付release编译成本（rust-cache恢复deps，workspace crate重编）。wall：27.8→max(A+C≈13.5,B≈8.5,D+E≈8.5)≈ **13.5分钟**；每run runner成本增加约8分钟。

<a id="25-are-the-six-giants-fixture-range-shardable"></a>

### 2.5 六个巨型项可按fixture范围分片吗

六项枚举corpus方式相同：`read_dir(spec/wasm_cross)`（722 `.almd`，`spec/wasm_fail` 4），`sort_by_key(path)`，一个 `for` loop，结果积入 `Vec` 后末尾assert。无一spawn线程。当前partition hooks：

- `corpus.rs:78` 遵从 `ALMIDE_CORPUS_FILTER=<substring>`（按stem保留），开发循环，明确“绝不在CI设置”。
- `wasm_runtime_interp_ledger.rs` 与 `crates/almide-spine/tests/run_parity.rs` 完全无filter。

range/modulo分区（sort后应用 `ALMIDE_CORPUS_SHARD=k/N`：`entries.retain(|(i, _)| i % N == k)`）是在三处约10行修改（corpus.rs、两个ledger循环、run_parity manifest循环）。**部分corpus逐gate健全性**：

| gate | 逐fixture断言？ | 分区会破坏的全corpus断言 |
|---|---|---|
| wasm_cross_target_spec | 是（逐fixture equal/allowed/stale/failed） | 无 |
| wasm_opt_parity_spec | 是 | “全部 `wasm_opt.is_none()` → skip”守卫，逐shard也可 |
| interp_cross_target_spec | 是 | 无 |
| run_parity | 是 | **`n_unsupported <= ceiling`、`n_fuel <= ceiling`**（全manifest shrink-only计数）及 `manifest.len() > 550`；须变为“各shard总和≤ceiling”（count artifact输出、coverage job合计），否则对N份部分count各检查一次ceiling，构成静默放宽 |
| interp_abstain_ledger | observed↔ledger集合相等，**双向** | **stale检测**：ledger里的fixture不在本shard便被视为“不再弃权”，产生误红（corpus.rs:74-77已有针对filter的警告）。修复：仅比较ledger中属于本shard fixture集合的部分。新弃权检测逐shard仍健全 |
| interp_bridge_fallback_ledger | *名称*（`module.func`）集合相等，不是fixture | 只被其他shard fixture到达的名称会被视为stale而误红，本shard无法判断。须先跨shard聚合观测名集合再比较（artifact+combine job），或保持不分片（1586 s成为新下限），或以§4第2项加速 |

覆盖证明（repo规则：丢fixture的partition必须红，而不是快）：`--list-corpus k/N` mode打印shard stems，coverage job断言 `∪ shards == ls spec/wasm_cross` 且无重复，与 `Test shards cover every target` 同型。无此步骤的range分片正是“测得更少、绿得更快”的失败，不得交付。

**逐job估计**（准备S≈120 s，实测自shard job开始至 `Cargo
tests` 开始；native warm-up W≈60–120 s为*估计*，fresh `/tmp/almide-run` 的首次 `almide
build` 编译runtime deps；ledger和run_parity无W）：

| 巨型项 | solo (s) | N=2每job | N=4每job |
|---|---|---|---|
| interp_abstain_ledger | 1666 | 833+120≈ **16 min** | 417+120≈ **9 min** |
| interp_bridge_fallback_ledger | 1586 | 793+120≈15 min（仅有聚合比较时） | 397+120≈8.6 min |
| wasm_cross_target_spec | 1377 | 689+210≈15 min | 344+210≈9.2 min |
| run_parity | 1310 | 655+120≈13 min | 328+120≈7.5 min |
| interp_cross_target_spec | 1223 | 612+210≈13.7 min | 306+210≈8.6 min |
| wasm_opt_parity_spec | 1075 | 538+210≈12.5 min | 269+210≈8 min |

巨型项关键路径5.5（build）+约16（N=2）≈ **21.5分钟**，或5.5+约9.2（N=4）≈ **15分钟**；但必须同时处理 `Commissioned wasm gates`（33分钟结束）和小项shards，否则它们成下限。solo job数6→12（N=2）或24（N=4）；N=4时每run约43jobs，若未先落地§4项5/7，会恶化§0容量等待。N=2是合理第一步。

**六项之后的下一个巨型项**：`diagnostic_harness_test`（合计1501 s；7个 `#[test]`，其中2个遍历 `collect_cases()` fixture）。weights文件没有逐test分解；若一test约1000 s，移走六项后该shard本身也需约20分钟wall。先测量，再假定小项shards约16分钟。

<a id="3-github-side-levers"></a>

## 3. GitHub侧杠杆

- **更大型hosted runner**：Linux4核$0.016/min、8核$0.032、16核$0.064（公共repo也收费，此处标准runner免费）。巨型项是一个process每次跑一个fixture，16核不会缩短1666 s，#2444已移除争用。小项shard受 `BuildDirLock` 限制（一台机同一时刻一个 `almide` native compile）。大型runner唯一买到的是**独立并发池**，迁入job的29分钟容量等待消失，代价约25jobs×8min×$0.016≈4核大型runner每run $3–4。更便宜：减少job数（项5、7）。
- **Self-hosted**：hexa darwin池（排队中位168分钟）和roc禁用arm shard，是反对小池的同类证据；lean4仅配hosted fallback action才用。不推荐。
- **`concurrency`**：已为 `${{ github.workflow }}-${{ github.ref }}` 并cancel-in-progress，对PR push正确。但下一queue merge也会取消develop push run（35565762632于06:41被下一push取消），只有queue run已审查tree才可接受。vera/jacquard/hexa/wado四者故意不取消main分支run，以保留逐commit记录。Almide的 `save-if: develop` rust-cache写入也随取消而消失，dep cache更新比表面更少。
- **仅 `merge_group` 的heavy lanes**：只能通过aggregator实现（见项6）。
- **required-check重设计**：以一个aggregator job替换13个命名context（`if: always()`，`needs:` 包括所有gate，任一非success即失败；docs-only run只接受由 `changes` guard的job为 `skipped`）。消除ci.yml header警告的matrix改名风险、`shard-coverage` 的solo代理step，以及目前七个实质gate非必需的状态。这是增强而非加速，但任何lane拆分都以此为前提。
- **nextest**：对整个archive用 `--partition hash:k/N` 会在*test*层分区（将 `diagnostic_harness_test` 的7tests分散），按构造完整（仍可通过 `nextest list --partition` union证明）；但按数量而非权重平衡，不能拆单test巨型项，只替代小项packer。`.config/nextest.toml` 的 `slow-timeout = { period = "…", terminate-after = N }` 加各job `timeout-minutes` 可关闭当前占slot长达6h的4h20m hang类（#1008）。archive与test-groups已使用，逐job `--extract-to` 已是最便宜形式。

<a id="4-ranked-plan"></a>

## 4. 排序计划

节省量基于#2444后关键路径（约36分钟+容量等待）。“风险”特指变更如何可能削弱green的含义。

| # | 项目 | 机制 | 关键路径节省 | 风险 | 工作量 |
|---|---|---|---|---|---|
| 1 | **先测#2444，再以corpus覆盖门禁将巨型项N=2范围分片** | corpus.rs/ledger/run_parity排序后 `ALMIDE_CORPUS_SHARD=k/N`；matrix `leg × shard`；`--list-corpus` mode；coverage job断言∪== `ls spec/wasm_cross` 且无重复；run_parity输出counts，coverage合计后对ceiling；abstain比较ledger ∩ shard；bridge-fallback **保持不分片或聚合名称** | 巨型job27.8→约16分钟，**约12分钟**（36→约24），须与#3一起实现 | shard静默漏看fixture正是失败；同PR必须含覆盖step；两ratchet/ledger gate需聚合形式，否则放宽 | M（3test文件+shard脚本+coverage job） |
| 2 | **进程内并行interp sweep**（ledger双gate与oracle interp侧），对排序fixture列表用scoped线程池，按index收结果 | 无backend、无spawn、无flock、无共享fixture；`Interpreter` 基于 `Rc` 但线程封闭；唯一static为 `OnceLock`/`Mutex`/`RwLock`（`stdlib_pool.rs:66`、`bundled_sigs.rs:47,107,230`），目前编译为 `Sync`。两个ledger同调一函数，改一次sweep | 4vCPU不拆job时1666→约1666/3≈9–10分钟，与#1 N=2合用每job约5分钟；两ledger一次sweep使runner分钟数下降 | `parse_cached`/`bundled_sigs` 的锁可能串行化、限制提速（可见而非静默）；非确定顺序不能改变ledger文本（按index收集，writer已排序） | S–M |
| 3 | **`Commissioned wasm gates` fan-out为A+C/B/D+E**（命令原样） | 三jobs，`needs: [changes, build]` | 27.8→约13.5分钟；配#1将下限33→约24 | 若命令逐字节复制且三者全部必需，门禁强度无风险（当前整个job非必需） | S |
| 4 | **只构建每gate实际读取的corpus侧** | binary级const `NEEDED_LEGS`，`build_corpus` 跳过其余（opt_parity不需native/interp；cross_target不需wasm-opt/interp；oracle不需wasm-opt） | 逐侧未测量；native逐fixture rustc编译，故opt_parity可能减半（1075→约500 s），cross_target去掉interp约1/4；每个#1 shard更便宜 | “不再读取”的侧必须确实不比较，三test正文是证明（cross_target不用 `l.wasm_opt` 等）；未使用field加compile-time `let _ = ` 不够，需保留断言每binary leg集合的test | S |
| 5 | **停止queue已跑job的重复develop `push` run** | heavy jobs加 `if: github.event_name != 'push' || github.ref == 'refs/heads/main'`；push保留 `build`（rust-cache `save-if: develop` writer）、`changes`、`checks`、lean | 单run关键路径0；**每merge PR减少约25 job-slots**，29–37分钟容量等待的最大来源 | develop无push限制（API `restrictions: null`、`enforce_admins: false`），直接push将未测试；必须与强制queue的push限制或ruleset同时落地。`github.event.before` docs-only路径也失去意义 | S（yml）+repo设置 |
| 6 | **必需aggregator check**（`CI required`、`if: always()`），纳入solo、Commissioned×2、perf、Windows、coverage | vibe/wado/lean4模式 | 0分钟；#1 matrix扩张及lane拆分前提，修正七个非必需gate | 接受 `skipped` 必须精准白名单仅限 `changes` guard jobs；漏 `needs` 会静默丢gate，保留列出所有job名的 `scripts/check-workflow-parse` 式负例控制 | S |
| 7 | **超时+固定工具+一个setup composite** | 每job `timeout-minutes`（wado全60；巨型45，其余20）；nextest `slow-timeout`/`terminate-after`；固定 `cargo-nextest`；缓存binaryen tarball替代12jobs的 `apt-get update`；toolchain+nextest+archive+wasmtime+binaryen共用action；统一wasmtime pin（v27/v47） | 每job准备约省0.5–1分钟；hang45而非360分钟 | 无；固定版本增强可复现性 | S |
| 8 | **将 `check-target-availability`（426 s）与 `reachability ratchet`（485 s）移至独立job**，待#1–#3把下限降至约20分钟后 | 仅binary脚本 | Almide gates15.4→约7.5，只在其成为下限时重要 | 原样搬移则无 | S |
| 9 | **仅merge-group的heavy lane**（Commissioned、determinism×2、perf、Windows、aviation） | jobs `if: github.event_name != 'pull_request'`，#6 aggregator在 `event_name == merge_group` 时遇 `skipped` 即失败 | #1–#3后PR lane减少0分钟（不再是下限），但每PR push少6slots；queue不变 | PR隐藏的失败要在queue一轮约25分钟后才发现并将PR移出队列，正是eb48f630b选择预付的成本；只有抱怨容量而非wall时才值得 | S，但属政策变更（○×） |
| 10 | **大型hosted runner** | 巨型项用 `runs-on: ubuntu-4-core`/8-core | 每巨型约0分钟（单process）；只有迁入其池才避免约29分钟容量等待，$3–4/run | 无 | S，持续费用，先做#5 |
| 11 | **每shard构建一次corpus table供三gate共享**（binary sha+fixture hash key的artifact，gate读取并断言key） | 每shard 3build改1 | #4后已去除弃置侧，剩余收益是native+wasm每shard 2次改1次（约5分钟runner时间；若table job不在关键路径则关键路径0） | stale/不匹配table静默审旧输出，key断言是整个gate；repo历史#2090“可怕的wasm破坏是静默的”支持同process保留leg/gate | M，排最后 |

**1–4+7后端到端预测**（算术、未实测）：queue领取→changes0.1→build5.5→max(巨型shard约9–16分钟、#2使ledger约5–9、Commissioned A约13.5、Almide gates15.4、小项shard约16–20取决于 `diagnostic_harness_test`)≈ **5.5+约16–20=22–26分钟**，对比今晚66、#2444后约36。再往下，`Almide gates` 与含 `diagnostic_harness_test` 的小项shard成为下限，需#8和逐test测量。突发下29–37分钟容量等待独立于这些，只能靠#5少run、#7/#9每run少job或#10另一池解决。

**无法借用的同类做法**：5分钟套件的vibe/vera/zod因工作小，优化cache温热及job开始延迟；Almide是真执行13 ks。aver `--partition slice:` 只适合多test binary（ledger binary2tests，其余1）。rust bors/auto分层与lean4标签分层是PR/queue拆分仅有先例，均接受更晚发现，这是本repo迄今拒绝的取舍。

<a id="5-measured-after-the-wave-landed-2026-09-21-develop-run-35633436273-on-83f922680"></a>

## 5. 变更波次落地后实测（2026-09-21，develop run 35633436273，83f922680）

新布局下首次未取消的develop push run（batch #2451带入#2440/#2444/#2445/#2446/#2449）。一次run、一组数字，不是预测：

| | 旧develop（35569545602） | 本次 |
|---|---|---|
| wall，push→最后必需job green | 66 min | **43.4 min**（17:39:49→18:23:11） |
| 关键路径 | shard0（59分钟，两ledger tests串行） | build4.5→`Test Rust (solo run_parity)`35→coverage gate3 |
| 其他最长job | shard1–3≈44/41/35 | coverage ratchet21、browser-ABI determinism15、solo opt_parity/cross_target/interp_ledger各14、shard0 13 |
| shards（只含小项） | 44 / 41 / 35 / 33 | **13 / 11 / 7 / 9** |
| Commissioned wasm gates | 27.8（一个job） | 13 / 12 / 8（三jobs，#2445） |
| interp ledger | 1666 s + 1586 s串行 | 一个solo侧14分钟（pool上一次sweep，#2446） |

同tree的PR run（35625736101）耗45.5分钟，尾部为非必需mutation gate（44分钟）；必需green在38分钟。因此“PR多久能入队”的引用值为**约38–43分钟**，尾部现在是runner独跑的单test `run_parity`。#2454（一次sweep interp侧的fixture-range N=2）将该侧减半（本地实测303/799 s对未分片1107 s），算术约4.5+18+3≈ **26分钟**，后续下限是coverage ratchet21和browser-ABI determinism15。

当天还测得一件比任何job更昂贵的事：merge-queue REBUILD让被取代ref的run继续存活（15:25Z有18个，105jobs排队、44运行）；queue头必需job内不重试的 `curl` 遇GitHub504，会重启后方所有entry。#2449在workflow内取消旧run并重试nextest/wasmtime/elan下载；当天清空积压的方法是一个batch PR审一次。

<a id="51-after-2454-develop-run-35645124173-on-6d38f0164-2026-09-21-19291954z"></a>

### 5.1 #2454之后（develop run 35645124173，6d38f0164，2026-09-21 19:29→19:54Z）

| | 本次 |
|---|---|
| wall，push→最后必需job green | **25.1 min** |
| 关键路径 | build4.5→`Test Rust (solo run_parity 2/2)`16→coverage gate2 |
| 巨型项逐半（1/2 · 2/2） | run_parity6 · 16；interp_oracle4 · 12；interp_ledger3 · 11；cross_target3 · 3；opt_parity<2 · 2 |
| 下一个下限 | `Almide gates`15、browser-ABI determinism14、structural侧13 |

一天、三种布局、三个数值：66→43.4→25.1。分片使每巨型项成本减半，但余数类slice不均（run_parity6对16），墙钟未减半；#2457携逐半表接手。#2381基于此测量关闭。

<a id="52-why-the-halves-were-uneven-and-the-balanced-slice-2457"></a>

### 5.2 两半不均的原因与平衡slice（#2457）

所有corpus fixture逐fixture墙钟时间，本地测一次（Darwin/arm64，14cpus，`ALMIDE_CORPUS_WEIGHTS_DIR`，未分片；各gate在该开关下记录的wall用于生成 `proofs/corpus-weights.txt`）：

| leg | fixtures | 合计 | 中位 | 最大 | 前20占比 | modulo两半（1/2 · 2/2） | LPT两半 |
|---|---|---|---|---|---|---|---|
| interp sweep（ledger、oracle） | 727 | 460 s | 2 ms | 65.6 s | **94.6 %** | 110 · **349 s** | 230 · 230 s |
| run_parity（串行遍历） | 729 | 186 s | 25 ms | 56.9 s | 85.0 % | 98 · 88 s | 93 · 93 s |
| builds（cross_target、opt_parity） | 727 | 281 s | 375 ms | 1.4 s | 4.8 % | 142 · 139 s | 141 · 141 s |

所以是重尾，不是余数偏差：interp侧95%时间在20fixture（前七个 `string_position_large_offset`、`regex_repetition_depth`、`float_parse`、`matrix_q1_full_loader_cols_beyond_buffer`、`string_interp_large_hole`、`bytes_temp_receiver`、`float_subnormal_preserved`，全部排序索引为奇数，即全在2/2，两半差3.2倍，就是CI实测3对11分钟）。仅build侧平坦，故cross_target/opt_parity按数量能均分。run_parity尾部相同fixture，但本地均衡98/88 s，CI却6/16分钟：重fixture与其他的比率属于runner而非程序（2/2巨型项分配受限，1/2巨型项CPU受限）。因此提交表由CI runner自身测量生成（每solo shard将wall记录在 `corpus-shard-*` artifact，下载后运行 `scripts/gen-corpus-weights.sh --render`）。

slice现在按记录列做LPT分区（均一权重精确重现modulo slice；未记录stem取中位，表不能排除fixture；coverage仍证明union）。表预测interp每半230/230 s、run_parity93/93 s。落地后的develop run记录在此。

<a id="53-the-prediction-was-a-laptops-and-the-runner-did-not-honour-it-2502"></a>

### 5.3 预测来自笔记本，runner并不遵循（#2502）

上表是本地测量，落地后的develop仍保持旧形：35692820397中 `Test Rust (solo run_parity 1/2)`479 s对 `(2/2)`1096 s，35694867589为841/1168 s；预测1:1，实际偏1.4–2.3倍。记录比率根本不是同一机器。逐fixture runner对Mac：

| fixture | Mac | ubuntu-latest | × |
|---|---|---|---|
| `string_position_large_offset` | 38.9 s | 520.5 s | 13.4 |
| `regex_repetition_depth` | 56.9 s | 426.7 s | 7.5 |
| `float_parse` | 6.0 s | 126.5 s | 21.1 |
| `bytes_temp_receiver` | 2.4 s | 57.0 s | 24.1 |
| 整个run_parity列 | 186 s | 2008 s | 10.8 |

不是恒定倍数：7 GB runner（#2387）上分配受限fixture多13–24倍、CPU受限多7倍，这正是LPT分区所依赖的比率。因此提交表现在来自CI run自身记录——所有solo job已在 `ALMIDE_CORPUS_WEIGHTS_DIR` 下记入 `corpus-shard-*` artifact（位于 `weights/` 子目录；平铺时run_parity gate的 `run_parity` 列与自身 `fixtures` / `counts` partial名称冲突，从CI下载生成时曾混入743行垃圾）：

```
bash scripts/gen-corpus-weights.sh --from-ci [run-id]   # default: latest green develop
```

保留代码的注释译文：默认使用最新green develop。

表的 `# measured-on:` 指明来源run。由35694867589生成时，相同wall重新分片为run_parity **1004/1004 s**；对*另一次run*实测wall重放此表则844/730 s（1.16倍）。同runner的run之间比率可迁移，这是Mac表不具备的性质。

每周ratchet（`shard-balance.yml`、`gen-corpus-weights.sh --check`）从十个solo job自身log测量并在倾斜时失败，与test-shard侧完全一样。按slice能控制的东西判断各leg：

- **串行leg**（`run_parity`、`cross_target`、`opt_parity`）：列均分即wall均分，故max/min≤1.4。
- **线程池leg**（`interp_ledger`、`interp_oracle`）：sweep使用 `available_parallelism`，wall是完成所有工作的总跨度；一个fixture（`string_position_large_offset`，该leg1657 s中的492 s）形成任何分区都无法移动的下限。LPT已分开两个巨型fixture，两半仍约1.8倍（预测492/272 s，实测593/286 s）。在那里设比率目标会因重切片无法修复的原因永久红，故改为max wall≤1.6×该下限；它捕获不再隐藏巨型项的slice，并指出杠杆是降低该fixture成本。

而且 `# measured-on:` 不写 `ci` 时根本拒绝判断：只意味着“表来自笔记本”的红灯无人能处理，故打印偏差与刷新命令，保持green直至提交CI实测表。
