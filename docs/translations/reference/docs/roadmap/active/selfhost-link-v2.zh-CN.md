<!-- description: Self-host linking v2 — 链接 renderer 已生成的单态化 generic 函数体，退役 shim registry 的逐类型 twin -->
<a id="self-host-linking-v2--link-the-mono-instances-retire-the-twin-matrix"></a>

# Self-host linking v2 — 链接 mono instance，退役 twin matrix

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](selfhost-link-v2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](selfhost-link-v2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](selfhost-link-v2.zh-CN.md)

> 非规范性历史阅读快照；固定原文为权威且不变，不自动同步。未执行引用命令、测量或编译器验证。契约语法、标志及 lowering 是提案而非已交付功能；开头提及后置条件与后续排除、lowering 前后的不同描述均按原文保留。LLM 准确性断言是设计预期，不是新测量结果。linking v2 仍是提案；历史数量、bug 与 host 描述不是当前普查或普遍保证。保留原文开头对 embedded Wasm host 支持的限定。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/selfhost-link-v2.md)

<a id="proposal-scope-and-historical-measurements"></a>

## 提案范围与历史测量

这仍是链接统一提案，不是完成报告。#1154 观测及下方 callee／call 数量是促成本提案的快照，并未声称重新普查整个 corpus。host effect 并非全都 native-only：embedded Wasm host 提供其支持的 fs/process/http 操作，逐 operation 的 wall 见 [target availability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml)。这不消除提案确实存在的 registry／twin 及单态化函数体链接问题。旧 machinery 叙事见 [v1-selfhost-machinery.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-machinery.md)。

这是 [[v1-selfhost-machinery]] 记录的 shim 时代的后继设计。当时通过在 prim floor 上手写 splice-context `.almd` shim，再由 `self_host_registry.rs` 按 NAME 链接，把 stdlib self-host 到 wasm 分支。它奏效了——约380个函数逐字节一致地执行——也积累了本文要退役的债务形态。

<a id="the-evidence-that-the-successor-is-already-half-built"></a>

## 后继方案已完成一半的证据

两个观测，一个动态，一个结构性。

**动态 — #1154 wall-escape trace。** 对 `result.zip(ok(1.0), ok("abc"))`，生成的 module 同时包含：

- `$almide_rt_result_zip__Float_String_String` — **真实 generic `stdlib/result.almd` 函数体的正确类型单态化 instance**，由普通 user-fn pipeline render，字节正确，却是 **dead**；
- `$result.zip` — `result_core.almd` 的手写 scalar shim，由 registry 链接，把两个参数都按 len-as-tag 读取，对 heap-Ok 参数**错误**（两个 ok 输入却返回 `err("abc")`）。

**结构性 — sibling-linking 路径已经存在且有效。** `linkable_module_fns`（`pipeline.rs:99`）把每个**没有 registry entry 服务的** bundled pure-Almide fn 作为普通 user sibling 提供；`resolve_user_module_calls`（`pipeline.rs:150`）把其 Module call 改写为 Named call；`monomorphize_module_fns` 为每个 TYPE 参数 tuple 特化一个函数体（`mono/mod.rs:44`，按类型变量加 suffix — `__fallible_map__Int_String_String`）。`list.split_at`、`list.iterate` 和七个 `__fallible_*` carrier 在当时已这样到达 wasm 分支。registry 的 `registry_served_names()`（`pipeline.rs:79`）恰是让 shim 留存的排除列表：dotted name 采用 shim 不是因为必须，而是因为在列表里。未注册的走正确 machinery，已注册的被手写 shim 截获——这种倒置就是全部论据。

<a id="the-debt-shape-being-retired"></a>

## 要退役的债务形态

1. **每个 consumer 都重新推导两种 tag layout。** scalar-Ok Result 的 tag 位于 `@4`（len-as-tag）；heap-Ok Result 是 1-slot cap-as-tag block（payload 低位 `@12`，tag `@16` — `result_materialize.rs`）。两种 layout 都不是问题，问题是每个 router arm 都重新推导适用哪一个。
2. **手动路由 twin matrix。** `result_call_name`（`mod_p4_c.rs`）及相关函数手动映射 combinator × payload-kind × twin（`_h` / `_x` / `_s2h`）。其 comment block 是 bug 台账：silent `ok("")` class（fuzz index 904，C-139）、seed-20260718 map misroute（C-151）、#1154 的 zip；每个都是同一 matrix 中漏掉或过窄的 arm。String-suffix twin 是手工单态化，而手工单态化会漂移。
3. **unlinkable class。** 29个 dotted callee 的74个 call site 作为 class-b（“unlinkable stdlib call”）被 wall 阻挡，因为没人手写对应 cell 的 shim：`zlib.*`、`process.*`、`fs.fold_lines*`、`string.split_once`、`map.upsert_skv_wall`、`list.split_at` 等。每个都是 v2 要删除的表中缺失的一行。

<a id="the-v2-claim"></a>

## v2 的主张

> callee 有 pure `.almd` 函数体的 dotted stdlib call，经过已有 sibling 路径——`linkable_module_fns` 加逐类型 tuple 单态化——链接；`registry_served_names()` 缩到 PRIM FLOOR，即 host intrinsic 和直接读内存的 splice-context 函数体。v2 不是新 machinery，而是每次验证一个 family，然后从列表删除名字。

按价值顺序列出的结果：

- 删除 twin matrix 及每个 combinator 的 routing arm。layout 选择收敛到 mono instance 自身 lowering 已使用的唯一 repr oracle（`repr_of`）。
- #1154 bug class 不再可表示，因为再无 name-level routing 步骤去选错函数体。
- class-b unlinkable 集合缩向真实 host 边界（fs/process/http effect、allocator、prim floor），而非“没人手写过的 cell”。
- `list.iterate` 及 `__fallible_*` carrier 不再特殊：都是按 instantiation 链接的 generic 函数体。这是 #1134 umbrella 的最终形式：按 shape 消除 wall 使函数体能 lowering，v2 使链接统一。

<a id="what-stays-a-shim-on-purpose"></a>

## 有意保留为 shim 的部分

- **prim floor**（`prim.load*/store*/alloc*`、handle arithmetic）及所有基于它的 splice-context 函数体：它们就是 memory model。
- **host-effect intrinsic**（fs、process、http、env、random 的 host seed）：跨 callback 与 errno 分类的表面，C-215/C-220 已定义为 native-first。
- 有**已证明**性能理由的手写 shim 可以保留，但每个都需针对其 mono instance 的 parity gate；没有 parity gate 的 shim 就是等待再次发生的 #1154 形态。

<a id="staged-plan-each-stage-falsifiable"></a>

## 可逐阶段证伪的计划

1. **先建 parity harness（证伪手段）。** 对 dotted name 同时有 pure generic `.almd` 函数体的每个 registry entry，在 corpus 使用的 instantiation 下同时 render shim 和 mono instance；在生成输入上逐字节比较执行（把 3-way oracle 的能力指向内部）。差异意味着当时就存在 #1154 类 bug，值得迁移前知道。该 harness 也是后续每次切换的 soak gate。
2. **Family 1：`result.*`** — 两次出事的 matrix。逐 instantiation 将 call 解析切换为 mono instance，每切一个 combinator 就删除 `_h`/`_x` arm；被 wall 阻挡的 `_x` cell（zip-mixed 等）变为可执行代码。ratchet：routing-arm 数和 class-b callee 数都单调下降，每个 commit 记入台账（C1 纪律）。
3. **Family 2..n：`option.* / list.* / map.*`** — 同样逐 family 切换，先按 bug 历史、再按 unlinkable-cell 数排序。
4. **registry 终局。** `self_host_registry.rs` 缩到 prim floor 加 host intrinsic，删除 twin suffix namespace（`_h`、`_x`、`_s2h`）；`v1-selfhost-machinery.md` 作为时代记录移至 `done/`。

<a id="costs-and-open-decisions"></a>

## 成本与未决问题

- **module 大小。** 每个 instantiation 一个 mono instance，对比一个共享 shim。测量而非猜测：minigit/onebrc size 统计是 scoreboard；DCE 已移除未使用 instance（dead zip instance 证明会生成，问题仅是变为 LIVE 后的边际增长）。若真增长，instantiation-collapsing（同 repr 的 instantiation 共用 instance）是已知手段。
- **性能。** shim 是手调 prim-floor code，mono instance 是 lowering 后的 generic 函数体。perf ratchet（`check-perf-ratio.sh`）与其他更改一样 gate 切换 commit。
- **layout 统一**（未决，v2 不要求）：把 scalar-Ok result 合并为 cap-as-tag 可彻底删除双 layout 差别，但 hot scalar path 付出 boxing 成本。v2 刻意不依赖它：无论如何 repr oracle 都吸收双重性。在 family 1 测量后决定。

<a id="what-would-falsify-this"></a>

## 什么会证伪它

- **parity harness 找到超出个别 bug 的系统性 shim↔mono 差异**——说明 mono instance 并非可信侧，修复顺序应反转，先修 generic 函数体。
- **#1134 shape burn-down 后 fallible/HOF family 的 mono instance 仍被 wall 阻挡**——链接切换跑在 lowerability 前面，必须按 family 跟在 burn-down 后。
- **instantiation-collapsing 也无法挽回的不可接受 size／perf 回退**——hot family 继续保留 twin matrix，但放在 parity harness 后，再不允许无 gate。

<a id="relations--three-adjacent-recreates-three-altitudes"></a>

## 关系 — 三个相邻重建、三个层次

- **[Closure Architecture v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)**（已完成）负责 closure 表示层：一个 identity、一个 capture-set、`lift_lambda` 的 decline class。#1134 family-1 wall（fallible_hof/fallible_lambda baseline entry）在那里：bundled generic 函数体在 lambda 参数可 lift 时，当时就能链接；lambda 无法 lift 时才触发 wall。本文不碰该边界，只删除决定 dotted call 的函数体来自哪里的层。
- 依托：#1134 按 shape 消除 wall（使函数体和 lambda 可 lowering）及 #1108 2b-iii（user HOF 的2-mode单态化；v2 切换沿用同一 mono machinery）。
- 正交于：identity 层重建（#908 / QualifiedRef）。它退役 NAME-keyed identity，本文退役 NAME-keyed linking；两者可组合但互不要求。
- 完成后替代：[[v1-selfhost-machinery]] 的 linking 部分，prim-floor 部分仍在那里保留。
