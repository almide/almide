> **历史资料（#916）。** 下方vs-Rust计分板和全部27项“已完成优化”在v0.25.0针对**#782中退役的v0 emitter**测量，所在文件已不存在；原始log自身注明机器当时有负载。保留为该活动记录；声明baseline后的v1 leg重测是#917。当时所指现行vs-Rust说明在docs/wasm/WASM-OUTPUT.md及memory ledger的“Rust-wasm打倒アーク”（v0.34.1，adversarial-Rust 4-0）。

<!-- description: WASM优化roadmap及针对Rust+LLVM的持续计分板 -->
<a id="wasm-optimization-roadmap"></a>

# WASM优化路线图

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-optimization-roadmap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-optimization-roadmap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-optimization-roadmap.zh-CN.md)

> 非规范性历史读者资料。固定原本为权威且不变，不自动同步，未独立执行benchmark/gate。保留有负载机器旧v0 emitter成绩、旧的当前措辞、后续v1 wasm-opt记录及提案目标的时点。与显示时间不符的比率格照录，不擅自修正。全胜和语言专用优化普遍优势属于原文活动主张，不是新的比较证据或保证；差分gate不是对外部wasm-opt变换的形式验证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md)

> Almide WASM emitter：hello world 336B。**当时状态：对Rust+LLVM 3胜/2平/6负（v0.25.0对rustc 1.95，2026-06-07，对手也在变化，见Baseline Update）**。v0.23.4时为5W/2T/4L。本roadmap目标是在**全部11项**benchmark中胜过当时的stable rustc。

<a id="precision-benchmark-1m-scale-v0234"></a>

## 精密benchmark（1M规模，v0.23.4）

| Benchmark          | Almide  | Rust   | 比率      | 状态     |
|--------------------|---------|--------|------------|------------|
| fib38              | 163ms   | 164ms  | ~1.0x      | ≈ 平      |
| sort_1M            |   6.1ms |  6.7ms | **0.91x**  | ✓ 胜      |
| list_map_1M        |   3.9ms |  1.6ms | 3.9x       | ✗ 负   |
| list_filter_1M     |   2.6ms |  2.2ms | 1.5x       | ✗ 负   |
| list_fold_1M       |   1.1ms |  0.4ms | 2.8x       | ✗ 负   |
| str_concat_1M      |   2.6ms |  1.4ms | 2.2x       | ✗ 负   |
| map_insert_100k    |   3.8ms |  4.2ms | **0.90x**  | ≈ 平      |
| map_get_100k       |   5.0ms |  6.3ms | **0.79x**  | ✓ 胜      |
| int_parse_1M       |  57ms   | 66ms   | **0.86x**  | ✓ 胜      |
| int_tostring_1M    |  35ms   | 59ms   | **0.59x**  | ✓ 胜      |
| math_sqrt_1M       |   0.4ms |  1.5ms | **0.27x**  | ✓ 胜      |

**成绩：5胜、2平、4负**

---

<a id="baseline-update-2026-06-07--v0250-vs-rustc-195-相手が動いた"></a>

## Baseline更新2026-06-07 — v0.25.0对rustc 1.95（对手进步了）

| Benchmark          | Almide 0.25.0 | Rust 1.95 | 比率     | 状态  | 对比v0.23.4 |
|--------------------|---------------|-----------|-----------|---------|-----------|
| fib38              | 145.2ms       | 142.7ms   | 1.02x     | ≈ 平   | 保持平 |
| sort_1M            |   5.8ms       |  4.8ms    | 1.21x     | ✗ 负 | **胜→负逆转** |
| list_map_1M        |   3.3ms       |  0.7ms    | 4.7x      | ✗ 负 | Almide改善，比率恶化 |
| list_filter_1M     |   2.6ms       |  1.7ms    | 1.53x     | ✗ 负 | 持平 |
| list_fold_1M       |   1.1ms       |  0.4ms    | 2.75x     | ✗ 负 | 持平 |
| str_concat_1M      |   3.0ms       |  1.4ms    | 2.14x     | ✗ 负 | 持平 |
| map_insert_100k    |   4.2ms       |  3.7ms    | 1.14x     | ✗ 负 | **平→负逆转** |
| map_get_100k       |   4.8ms       |  5.4ms    | **0.89x** | ✓ 胜   | 保持胜 |
| int_parse_1M       |  55.6ms       | 56.0ms    | 0.99x     | ≈ 平   | 胜→平 |
| int_tostring_1M    |  18.6ms       | 50.8ms    | **0.37x** | ✓ 胜   | **35→18.6ms改善** |
| math_sqrt_1M       |   0.4ms       |  1.4ms    | **0.29x** | ✓ 胜   | 保持胜 |

**成绩：3胜/2平/6负。** Almide未退步、反而改善，但rustc 1.95 + wasmtime 42将map从1.6→0.7ms、sort从6.7→4.8ms，令两项逆转。原始数据与条件：`research/benchmark/stdlib/results/v0.25.0-vs-rust1.95.txt`（有负载机器，min-of-5；sort / map_insert逆转需在安静环境重验）。二进制大小Almide 5.9KB对Rust 109KB（18.6x）。

<a id="教訓--タスク-ratio-ratchet-gate"></a>

### 教训 → 任务：ratio-ratchet gate

针对固定对手的目标值会悄然过时。下方Fix Plan Expected值针对旧rustc导出，达成后map仍约2.9x。

- [ ] **perf-ratchet gate**：每次用**current stable rustc重建**Rust双胞胎（`rust_wasm_compare/src/precise_all.rs`），对min-of-5 ratio设ratchet，恶化则CI红。结果JSON兼任`make verify` dossier性能项目。
- [ ] **基于rustc 1.95重新推导Fix Plan Expected**（预期fold ≈TIE、concat ≈TIE；map hoist后仍约2.9x。Rust 0.7ms ≒ 23GB/s受带宽限制，进一步逼近才是11/11核心）。
- [ ] **调查sort_1M / map_insert_100k逆转**：安静环境重测，若确有差异再做根因分析。
- [ ] 顺序：fold（小）→ str_concat（小）→ map base-pointer hoist（中）→ filter（大）。

---

<a id="remaining-losses--root-cause--fix-plan"></a>

## 剩余落后项 — 根因与修复计划

四项落后都是“我们的emit不如LLVM高效”，不是WASM限制。Almide和Rust都输出WASM，在同一wasmtime JIT运行；LLVM也使用相同v128指令，只是生成更紧凑的loop。

<a id="1-list_fold-28x--simd-accumulator-not-implemented"></a>

### 1. list_fold（2.8x）— 未实现SIMD accumulator

**根因**：我们emit scalar loop，LLVM emit i64x2.add accumulator。

```
Almide (scalar):
  loop: i64.load → i64.add acc → ptr += 8

LLVM (SIMD):
  loop: v128.load → i64x2.add acc_v128 → ptr += 16
  end:  extract_lane 0 + extract_lane 1 → final sum
```

**修复**：检测Int元素的`fold(init, (a, x) => a + x)`，emit i64x2 accumulator并在末尾horizontal add，4倍unroll。

**工作量**：小。对fold lambda body做pattern match，与map SIMD同法。
**预期**：1.1ms → 约0.4ms，与Rust持平。

<a id="2-list_map-39x--simd-loop-overhead"></a>

### 2. list_map（3.9x）— SIMD loop overhead

**根因**：已有SIMD，但每个v128 op都带冗余`local.get dst_ptr` / `local.get src_ptr`。LLVM用单一base pointer相对load/store `offset`避免它们。

```
Almide:
  local.get dst_ptr          ;; redundant per-op
  local.get src_ptr          ;; redundant per-op
  v128.load offset=0
  ...
  v128.store offset=0

LLVM:
  v128.load offset=0 (base=src)   ;; no extra local.get
  ...
  v128.store offset=0 (base=dst)
```

**修复**：把`local.get src_ptr`和`local.get dst_ptr`提到unroll block之外；用`local.tee`将base留在stack。每个展开op仅用`v128.load offset=N`，N=0,16,32,48。

**工作量**：中。将SIMD emit重构为stack-based addressing。
**预期**：3.9ms → 约2ms。

<a id="3-str_concat-22x--initial-cap0-causes-20-grows"></a>

### 3. str_concat（2.2x）— 初始cap=0造成20次grow

**根因**：`var s = ""`创建cap=0 string；每次grow调用`__string_append`，有函数call开销。1M append产生20次grow。Rust也从0增长，但LLVM完全内联grow路径。

**修复**：将`var s = ""`以初始cap=16或64 emit。前16次append仅为inline byte store，无函数call。并完全内联grow路径，避免string_append call。

**工作量**：小。空string emit加入capacity。
**预期**：2.6ms → 约1.5ms。

<a id="4-list_filter-15x--scalar-predicate"></a>

### 4. list_filter（1.5x）— scalar谓词

**根因**：我们的branchless filter不错，但仍scalar；LLVM可能通过SIMD批量检查谓词。

**修复**：对`x % 2 == 0`等简单谓词（当时已由ModInt peephole变为`x & 1 == 0`），emit SIMD批量检查：`v128.load` → `v128.and(mask)` → `i64x2.eq(zero)` → 条件copy。WASM SIMD缺少compress-store，故有难度。

**工作量**：大。SIMD filter需要创造性方法。
**预期**：2.6ms → 约2.0ms，改善有限，SIMD filter很难。

---

<a id="completed-optimizations-v0233--v0234"></a>

## 已完成优化（v0.23.3 + v0.23.4）

<a id="session-1-v0233-14-optimizations"></a>

### Session 1（v0.23.3）：14项优化

| # | 优化 | 效果 | 文件 |
|---|---|---|---|
| 1 | hash table map（开放寻址） | map快1000倍 | calls_map.rs, list_layout.rs |
| 2 | sort run检测（升/降序） | sort快5倍 | calls_list_helpers.rs |
| 3 | lambda内联（无capture） | map/filter/fold ~2x | calls_list_closure2.rs, pass_closure_conversion.rs |
| 4 | 二分递归变换 | fib快2倍 | pass_tco.rs, target.rs |
| 5 | stream融合（map→filter→fold） | pipeline快4倍 | calls_list_closure2.rs |
| 6 | 无分支filter | filter快1.5倍 | calls_list_closure2.rs |
| 7 | 基于pointer迭代 | 减少loop开销 | calls_list_closure2.rs, calls_list_helpers.rs |
| 8 | WASM pipeline中的TCO | 尾递归 → loop | target.rs |
| 9 | 自适应scratch local | fib local数量88→8 | functions.rs |
| 10 | string布局迁移（data@8） | 修复10多个文件 | runtime.rs, rt_*.rs, calls_*.rs |
| 11 | Swiss Table布局（1-byte tag） | 更适合cache的probe | calls_map.rs, expressions.rs |
| 12 | map.get??default → get_or融合 | 消除Option heap alloc | pass_peephole.rs |
| 13 | sort的1-pass逆序copy | sort与Rust持平 | calls_list_helpers.rs |
| 14 | 基于pointer的list.map | 消除idx乘法 | calls_list_closure2.rs |

<a id="session-2-v0234-13-optimizations"></a>

### Session 2（v0.23.4）：13项优化

| # | 优化 | 效果 | 文件 |
|---|---|---|---|
| 15 | SIMD v128 list.map（4倍展开） | map SIMD路径 | calls_list_closure2.rs, wasm_macro.rs |
| 16 | 内联单字符string append | 跳过函数call | statements.rs |
| 17 | string_append memory.copy | 消除3个byte loop | runtime.rs |
| 18 | 基于pointer的filter/fold | 消除idx乘法 | calls_list_closure2.rs |
| 19 | ModInt peephole（x%n==0 → x&(n-1)==0） | 避免i64.rem_s | expressions.rs |
| 20 | map增长倍数4倍 | resize次数13→7 | calls_map.rs |
| 21 | 指数增长allocator | 摊销O(1)增长 | runtime.rs |
| 22 | 初始memory 128KB | 最小占用 | mod.rs |
| 23 | 条件式fs初始化 | 非fs跳过preopen | mod.rs, functions.rs, dce.rs |
| 24 | 积极函数DCE | 清理element table | mod.rs |
| 25 | 消除dead data | 剥离未用string | dce.rs |
| 26 | offset 4096的string pool | 安全data DCE | mod.rs |
| 27 | binary大小：25KB → 336B | hello world减少98.7% | 上述全部 |

<a id="tier-2--medium-term"></a>

## Tier 2 — 中期

<a id="21-escape-analysis-for-optionresult"></a>

### 2.1 Option/Result逃逸分析

检测不逃逸的Option/Result，以WASM multi-value `(i32, i64)`返回，避免heap分配wrapper。

<a id="22-arena-allocator"></a>

### 2.2 Arena allocator

对不逃逸分配进行region-based memory管理，与逃逸分析配合。

<a id="23-partial-evaluation"></a>

### 2.3 部分求值

在已知call site对高阶函数做专门化。

<a id="24-wasm-opt-integration-optional--done-2026-07-23"></a>

### 2.4 wasm-opt集成（可选）— 完成（2026-07-23）

以`almide build --target wasm --wasm-opt`交付。上文“额外不足1%”估计针对旧v0 emitter内建DCE测量（#782退役前），已不成立。v1 renderer内reachability DCE（本session落地：裁剪preamble/import/function/data-segment，并将name-section裁至仅function names）把Hello World默认已验证module从8,713 B减至770 B；但`wasm-opt -Oz`在此基础上仍额外节省28–43%（2026-07-23测量）：770→548 B（Hello）、11,965→6,868 B（Variant）。Reachability DCE只去掉整块未触达unit，不能做wasm-opt的指令级工作（local coalescing、inlining、dead-store elimination）；两者互补，而非冗余。

严格保持opt-in、默认off：trust-spine交付自身带cert rendering过程生成的精确bytes，而`wasm-opt`是针对输出的外部未验证变换。`--wasm-opt`自身安全主张——不改变Almide生成wasm的可观测行为——并非空言，而由专门差分gate `tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`在CI（`.github/workflows/ci.yml`）执行支持。完整说明：[docs/wasm/WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md)。

<a id="25-memory-layout-type-safety"></a>

### 2.5 Memory layout类型安全

用newtype wrapper（MemOffset、StringPoolOffset、HeapPtr）替换magic constant（SCRATCH_ITOA、NEWLINE_OFFSET），防止整数常量与data section offset冲突这一类bug。

<a id="tier-3--long-term"></a>

## Tier 3 — 长期

<a id="31-wasm-gc-proposal"></a>

### 3.1 WASM GC提案
<a id="32-wasm-component-model"></a>

### 3.2 WASM Component Model
<a id="33-profile-guided-optimization"></a>

### 3.3 Profile-Guided Optimization

---

<a id="principles"></a>

## 原则

1. **相同WASM，相同runtime。** Almide和Rust都为同一wasmtime JIT生成WASM bytecode；性能差属于emit质量，而非platform限制。
2. **语言知识 > 通用优化。** 每项Almide特有变换（get_or fusion、itoa、sqrt）均优于LLVM通用对应项。
3. **测量真正瓶颈。** map.get问题在Option heap alloc，而非hash table layout。优化前总要profile。
4. **1M规模揭示真相。** 100k benchmark受noise支配。总应在1M以上规模验证。

代码图注释译文：Almide scalar loop加载i64，加到acc，pointer前进8。LLVM SIMD loop加载v128，加到i64x2 accumulator，pointer前进16，最后将lane 0与1相加。map图中Almide每op取pointer是冗余的；LLVM例子无需额外local.get，使用base相对offset。代码及其中注释保留原文。
