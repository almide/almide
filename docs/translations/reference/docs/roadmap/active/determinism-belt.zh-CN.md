<!-- description: 历史 determinism belt 设计、退役 type-state 边界及当前证据限制 -->
<a id="the-determinism--purity-belt"></a>

# 确定性／纯度 Belt

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](determinism-belt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](determinism-belt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](determinism-belt.zh-CN.md)

> 非规范性历史读者快照；固定原本不变且为权威。不自动同步，未新运行证明、runtime、测量或 CI。进度层及过时实现说明保留为历史。cert／模型级证明及限定范围实验检查不是普遍成果物安全保证或航空认证。确定性 belt 原文已声明 v0 type-state 边界退役；较强旧引语及稳定排序／输入顺序主张保留为历史断言，而非新确立保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md)

<a id="reading-this-historical-belt-design"></a>

## 如何阅读这份历史 belt 设计

下述 `Verified → Canonical → emit_wasm` type-state chain 属于退役 v0 emitter，**不是当前发射边界**：[codegen/lib.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/lib.rs) 记录其在 #782/#930 退役；当前 Wasm 路线为 [wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)。impurity lint 与差分 determinism gate 是覆盖范围内的证据，不是 browser compiler 永不崩溃或所有 host 均生成相同 bytes 的定理。原实现笔记及其限制如下。

> 用于 compiler **输出确定性＋target 可移植性**的 Perceus 类比。AlmidePerceusBelt 将“若生成，RC 就平衡”变为定理；此设计通过构造强制“若生成，输出就是 (IR, target) 的纯函数”，从而主张 wasm32（browser playground）compile 不会崩溃，也不会偏离 x86-64。

<a id="why"></a>

## 为什么

compiler 在 browser playground 中编译为 **wasm32-unknown-unknown** 运行。曾发布两个被所有 verifier（stack-effect、Perceus、spec suite）漏过的 bug，同属一类：codegen 读取非确定／impure／不可移植来源，而非成为 `(IR, target)` 的纯函数。

1. 无条件调用 `std::time::Instant::now()`（ALMIDE_PROFILE 计时）→ 在无 clock 的 wasm32-unknown-unknown 上 panic → 每次 browser 内 compile 崩溃。（v0.23.13 修复调用；Belt 的意图是令其无法写出。）
2. `HashMap`/`HashSet` 迭代分配 WASM function/table/index/offset 顺序；hashbrown 迭代依赖 host pointer 宽度（h2 = `hash >> (usize_bits-7)`），且 `Sym` key map 依赖 intern 顺序（process 历史）。→ wasm32 生成不同／会 trap 的 module。

spec suite 只在 x86-64 运行 compiler；首次 determinism gate 用拥有 WASI time 的 wasm32-wasip1，因此漏过 bug #1。Belt 是结构性回答。

<a id="the-four-layers-strong-inner-gates-carry-the-load-broad-outer-gates-catch-what-types-cant"></a>

## 四层（强内层 gate 承担主要工作；广外层 gate 捕捉类型无法捕捉的部分）

| 层 | 内容 | 强度 | 状态 |
|---|---|---|---|
| **L1 Purity wall** | compile-path crate 不能*命名* clock/RNG/thread/atomic-counter 来源（grep gate `scripts/check-forbidden-impurities.sh`＋计划中的 `clippy.toml` `disallowed-methods`）；仅经 `almide_base::profile::ProfileTimer` 计时（cfg-gated、wasm-safe） | lint/deny（文本方式，易被 alias 影响，**不是** Perceus 级 type-state） | **完成（Phase 0/1）** |
| **L2 `DetMap`/`DetSet` prelude** | codegen-path crate 唯一可*迭代*的 collection 按 content 排序（BTreeMap 支撑）；普通 `HashMap` 只保留为经审计 `// DET-ESCAPE` 后的 lookup-only cache | type-state | roadmap |
| **L3 `Canonical<IrProgram>`** | 终端 `CanonicalizePass` 将 emit 顺序 function Vec 排成 content 派生顺序；`emit` 仅接受 `Canonical`（`Verified` 的兄弟），`emit_wasm::emit` 现为 `pub(crate)` 防止绕过 gate。即使上游 pass 按 hash 顺序构建 Vec，也重新正规化 | type-state（真正 Perceus spine） | **完成** |
| **L4 Runtime tripwire** | `check-host-determinism.sh`（wasm32-wasip1）＋`check-browser-determinism.sh`（经 node 的 wasm32-unknown-unknown）将 compiler 输出与 native 作 byte 比较 | test（开放世界后备，永久） | **完成** |
| L5 Lean proof | `canonicalize` 幂等且不依赖输入顺序 | proof | 可选 |

<a id="done-so-far-v02313--this-work"></a>

## 已完成（v0.23.13＋本工作）

- **L1**：`almide_base::profile::ProfileTimer`（唯一许可、wasm-safe clock）；三个 codegen clock 位置经它路由。`scripts/check-forbidden-impurities.sh` 在 `crates/{almide-codegen,almide-frontend,almide-optimize,almide-ir}/src` 出现原始 `std::time|Instant|SystemTime|thread::spawn|thread_rng|fastrand|AtomicU64|fetch_add` 时让 CI 失败（排除 `/generated/` `/runtime/`，它们是输出至用户 native program 的 runtime code）。接入 CI `checks` job。
- **已确认实际存在的数据顺序泄漏**：`record_fields`/`variant_info` → `BTreeMap`；`closures.rs`/variant-eq/name-section 排序；**`almide-optimize/src/mono/mod.rs` 的 `all_instances`/`new` → `BTreeMap`**（特殊化函数 append 顺序 → WASM function index，原按 HashMap 顺序，依赖 host 宽度＋intern 顺序）。
- **L4**：两个 gate 均在 CI；`spec/wasm_cross/` fixture 包括 `playground_default.almd`、`generics_mono.almd`（单态化）、`anon_records_and_fusion.almd`（匿名 record 命名＋egg-fused pipeline）。

<a id="l3-shipped-this-pr--the-type-state-spine"></a>

## L3 已交付（本 PR）— type-state spine

基于实际 codebase 审计（emit 边界＋Sym 迭代面＋egg 是否实际生效＋每个按 emit 排序的 collection）推动实现，并**纠正两个前提**：

- **`CanonicalizePass`**（`pass_canonicalize.rs`）：终端 WASM pass，按 `(is_test, name)` 稳定排序 `program.functions` 及每个 `module.functions`。安全，因为函数在 emit 时按名解析（排序改变 WASM func index，但保持语义）。刻意**不**重排 `program.modules`/`top_lets`（top-level init 顺序执行，后一个 `let` 可能观察前一个，顺序有语义），也不重排 `type_decls`（variant-tag 顺序）。幂等。
- **`Canonical<'a>` type-state**（`lib.rs`，`Verified` 兄弟）：`Canonical::certify` 消耗 `Verified`（先决条件为 RC 已验证），断言 `is_canonical`。WASM emit 仅接受 `Canonical`，**`emit_wasm::emit` 降为 `pub(crate)`**，关闭此前任意 caller 均可直接调用 `emit` 的绕过路径。codegen 路线现为 `codegen → Verified → Canonical → emit`。
- **egg counter — 曾是实际泄漏，现已修复**：`EGG_FRESH_COUNTER`（`AtomicU64`）令长寿命 process 中多次 compile 的 `__egg_v{n}` 漂移（Rust target 已确认；在仅命名函数的 WASM name section 中潜伏）。替换为 `vt.len()`（等于即将发生的 `alloc` 分配的 `VarId`），唯一、逐 compile 确定、无 global state。*也移除一个原始 atomic，否则当 egg-lab 纳入范围时 L1 grep 会报告它。*
- **匿名 record 命名 — “雷区”已解决**：`__anon_record_{record_fields.len()}`（遍历顺序 counter）→ `__anon_record_{FNV1a64(field-shape)}`，即 content 纯函数（用 FNV 而非 `DefaultHasher` 以避免 `RandomState`）。解决“无全域 content key”担忧：code 已计算的 dedup key 就是全域 key。
- **`default_fields`**（Rust struct-literal emit）：`HashMap.keys()` 顺序 → 排序。最后一个 RandomState 类 emit-order 位置（Rust target）。

已验证：debug assert 启用下完整 `spec/` suite **240/240**（`Canonical`／后置条件检查从未触发）；多次 compile 间 Rust＋WASM 输出**逐字节相同**。

<a id="deferred-honest-limits"></a>

## 延期事项（诚实的限制）

- **L1 易受 alias 影响，可用 `#[allow]` 绕过**；提供 coverage＋CI 失败，而非 Perceus 级不可能性。真正 spine 是 L3（当时已交付）。
- **`Sym` interner 风险真实存在，但目前潜伏，未实际生效。** `Sym: Hash` 是 process-global `ThreadedRodeo` 上按 intern 顺序的 `Spur` id，所以 `HashMap<Sym,_>` 迭代依赖 process 历史。但审计发现 compile 路线中每个真正以 `Sym` 为 key 的 map 都只用于 lookup、diagnostic，或分配永不到达输出 bytes 的 `DefId`，当时均未由迭代顺序泄漏。仍是潜在误用点（下一次将 `for (sym, _) in map` 馈入输出即会生效）；L2 应禁止迭代 `Sym` key map，防御性地将 `lower/mod.rs` 的 `env.top_lets` 按 `Sym::as_str()` 排序是低成本保险。尚未完成。
- **L3 仅限 WASM**（与 `Verified` 同范围）。Rust target 展平 module，由 egg＋`default_fields` 修复覆盖，而非 type-state gate。将 `Canonical` 扩展至 Rust emit 属未来工作。
- **L3 sort key 的全域性**：通过对 `(is_test, name)` **稳定**排序处理。真正重名回退到已确定的上游顺序，因此原文称 canonicalize 保持幂等且输入顺序无关。雷区为 anon-record counter，现改为 content key。
- **没有任何层认证成果物本身**：确定性是 provenance 的性质，拿到 bytes 时已丢失。构造层约束*过程*；emitter 忠实性＋未知第 N+1 个来源轴仅由 L4 经验性闭合。对 runtime gate 的依赖是永久而非过渡。

<a id="next-highest-leverage-step"></a>

## 下一步最高杠杆工作

L3 关闭结构缺口，所以剩余工作是强化外层：（1）`clippy.toml` `disallowed-methods` deny（Instant::now、SystemTime::now、thread_rng、fetch_add）＋`#![deny(clippy::disallowed_methods)]`，将 L1 从 grep 升级为 compiler 检查；（2）L2 `DetMap`/`DetSet`，令潜伏 `Sym` 迭代风险无法写出；（3）可选 L5 Lean proof，证明 `canonicalize` 幂等且输入顺序无关。
