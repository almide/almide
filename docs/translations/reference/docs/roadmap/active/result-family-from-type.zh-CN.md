<!-- description: Derive call-result layout from the type, not from module/func name tables -->
<a id="result-family-from-type--layout-is-a-function-of-the-type-names-only-choose-code"></a>

# result-family-from-type — layout 是类型的函数，名称只选择代码

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](result-family-from-type.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](result-family-from-type.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](result-family-from-type.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/result-family-from-type.md)
> 历史active roadmap，区分Phase 0–2落地、Phase 3部分实现与未完成的registry推导／Stage C。原文称8个编译器／8/8却只列7项；保留此差异及repo外调查位置，未独立解决。结构上不可表达的主张限于所述family/routing bug类别，不保证所有编译器故障均不可能。

**状态**：active（phase 0 于 2026-08-14 落地；本文是该工作线的基本准则）
**动因**：#1406 及其四个已记录的同类问题 — 一类 bug，五起事件。
**调查证据**：`../almide-references/RESEARCH.md`（8 个编译器的调查，file:line 引用；保存在 repo 外）。

<a id="the-disease"></a>

## 症状

MIR lowering 从 `(module, func)` 名称表决定调用结果的物理 layout（“family”）：

- `is_self_host_result_module_fn`（scalar 的 len-as-tag family，约 27 行）
- `is_self_host_result_str_module_fn`（heap-Ok 的 cap-as-tag family，约 37 行）

同类的五起事件均已记录在 tree 内：

1. **#1406-D7** — 分类位置看到 routing 前的名称（`fan.any_map` 覆盖九种配对；`fan_any_call_name` 在 emit 时加后缀），所以 routed 形式的名称行从不触发；String 输出配对分错 family → wall。
2. **C-145** — 带 mono 后缀的 `or_else__…` 在 `base_stdlib_fn_name` 出现前，漏过所有按名称做出的判断。
3. **#1144** — 以 `__` 开头的 carrier 名称被切成空字符串；所有按名称做出的判断悄悄漏掉，整个 fn 被 wall 拒绝。
4. **fs.copy/append 分错 family** — 曾一度列在 str family；每个 err 都读成 ok（静默走错分支；mod_p5.rs:34-49 有记录）。
5. **永久的逐点维护** — 每新增 stdlib 配对都需添加表行；这些表的存在本身就在结构上违反 CLAUDE.md 的“按 matrix 扩展，而不是逐点扩展”规则。

<a id="the-root-cause"></a>

## 根本原因

layout 不是类型的函数。当时，同一个类型 `Result[Unit, String]` 有两种物理 layout：

- **len-as-tag**（@4 = 0/1，payload @12）：通过 `materialize_result_ok` 由 ctor 构建 — fs.copy、fs.append、fs.remove、fs.write_bytes、fs.write_bytes_raw。
- **cap-as-tag**（tag @16，payload @12，Ok 约定 len@4=0）：由 prim-render 构建（$write_text_file family）— fs.write、fs.mkdir_p、fs.rename、fs.remove_all。

因为分歧取决于 producer 身份，只有名称能恢复它 — 这就是名称表存在的原因。其他 Result 类型已经一致：scalar-Ok（`Int`/`Float`）总是 len-as-tag；heap-Ok（String/List/Value/…）总是 cap-as-tag。

<a id="what-the-surveyed-compilers-do-one-line-each"></a>

## 被调查编译器的做法（各一行）

- rustc：返回 ABI = `arg_of(sig.output())`；intrinsic 名称表（436 arms）选择指令，并通过 debug assert 对照类型导出的 layout。
- Swift：每个类型 key 缓存一个 `TypeLowering*`（没有 decl/name 字段）；唯一按名称导出的规则（ObjC selector families）在 import 时并入函数类型。
- Zig：`firstParamSRet(cc, return_type, target)`；compiler_rt 名称从类型构建 — 名称是类型决策的输出，绝不是输入。
- Roc：每个 LIR local 上保存 intern 后的 layout；builtin 名称 registry 携带 symbol + RC 契约，明确不携带 layout。
- Lean 4：类型 → 封闭的 14-variant IRType；唯一按名称判断结果的例外（3 个 Array fn）在 3 个 pass 中重复，并配有未强制执行的“Keep in sync”注释 — N=3 时的同一症状。Almide 为 N=64。
- Koka：`cType :: Type -> CType`；唯一的名称表处理参数 borrow；结果始终 owned +1（ABI 常量，无需跟踪）。
- Grain：统一 tagged word + 1-bit Managed flag；释放时读取 runtime tag。

**规律（8/8）：名称选择代码，类型选择 layout。**

<a id="the-cure-in-phases"></a>

## 分阶段解决

<a id="phase-0--tactical-1406-landed-with-this-doc"></a>

### Phase 0 — 战术性 #1406（随本文落地）
`returns_foreign_result` 的 can-err seed + 分类位置的 `is_fan_any_map`+`is_heap_ok_result` 类型分流 + C-004 fixture。在最严重的对象上验证类型分流形状。该特例在 Phase 2 融入通则。

<a id="phase-1--make-family--fty-true-unify-resultunit-string"></a>

### Phase 1 — 让 `family = f(Ty)` 成立：统一 `Result[Unit, String]`
让 `Result[Unit, String]` 的 ok(())/err(m) ctor 路径通过 cap-as-tag materialization（fs.write 的 prim family 已用此 layout：Ok = len@4=0、@12=0、tag@16=0；Err = len@4=1、@12=msg、tag@16=1）。把五个 ctor 构建的名称从 scalar 表移到 str 表。drop routing 不变（平坦 `DropListStr` 对该 layout 的两个 arm 都精确适用 — fs.write 的先例）。该阶段后，类型→family 映射为全函数：

```
result_family(Result[T, E]) = HeapOk (cap-as-tag @16)  if T is heap or Unit
                              Scalar (len-as-tag @4)   otherwise (Int/Float/Bool)
```

gate：完整 corpus（418 个 wasm_cross fixture，3-way）、ABI probe diff、各 suite。

<a id="phase-2--one-family-function-name-tables-carry-no-family"></a>

### Phase 2 — 一个 family 函数，名称表不携带 family
在 `is_heap_ok_result` 旁引入 `result_family(ty: &Ty) -> ResultFamily`，作为唯一决策点。把两个名称表合成一个集合，仅表示“此调用的结果已 materialize”（两表的并集）。将分类位置（bind：binds_p2_c.rs；subject：tracked_calls.rs + control.rs；control_p2_b.rs:30/73 的 heap-ok refinement 操作）改写为 `materialized(name) && family(ty)`。删除 `is_fan_any_map` — 通则已覆盖。fs.copy 事件变得不可表达：行不再携带 family，因此无法用某行把类型放入错误 family。

<a id="phase-3--derive-materialized-from-the-registry-kill-the-hand-table"></a>

### Phase 3 — 从 registry 导出“materialized”（消除手工表）
剩余名称集合重复了 self_host_registry + prim floor 已有的知识。改为推导：Module 调用 materialize 其 Result，当且仅当 routed 名称（`list_heap_call_name` — 唯一 router）在 `self_host_registry` 中链接（ctor 路径的 layout 由构造保证 canonical），或是 prim floor。落地一个可执行 gate，遍历每个由 registry 链接且返回 Result 的 fn，断言分类位置与 `result_family` 一致 — matrix-gate 纪律：受 gate 约束的 matrix 不能漂移。

<a id="phase-4--seed-once-stage-c-direction--close"></a>

### Phase 4 — 只 seed 一次（Stage-C 方向）并收尾
逐值 repr 设计（Roc 的 `Local { layout_idx }` — 使“未跟踪”不可表达），以及作为第一步的 seed-once 入口，已起票 #1414。这是自然延续，不属本工作线：它统一的集合插入机制属于 drop-routing 代码，即便不改变行为的重构也有实际风险；而本工作线要集中化的决策逻辑（`result_family` + 合并集合）已经是单一点。更新 ARCHITECTURE.md，将本文连同实测差异移到 done/。

<a id="landed-state-2026-08-14"></a>

## 落地状态（2026-08-14）

- Phase 0：cd0d01820..f837fa0a5（can-err seed、类型分流、C-004 fixture、本文）。
- Phase 1：`materialize_result_err_str`（ctor Result-err block 上 @16 tag — 超集兼容，len reader 不变），五个 ctor 构建的 `Result[Unit, String]` fn 移入 str family；在 `??` scalar 路径准入 `is_result_unit_str_ty`。`Result[Unit, String]` 现只有一种 layout。
- Phase 2：`result_family(ty)` + `is_self_host_materialized_result_fn`（合并集合）；tracked_calls.rs / binds_p2_c.rs / control_p2_b.rs 查询它们；#1406 的 `is_fan_any_map` 特例及 control_p2_b 的 heap-ok 逃生通道融入通则。
- Phase 3（首部分）：crates/almide-mir/src/lower/tests_result_family.rs 中三个 gate test 固定 family 函数的全域性、九配对一名称分流，以及名称 mangling 事件类别（C-145、#1144）。从 registry 完整导出合并集合仍是本工作线未完尾项 — 等 registry 增加声明返回类型列再做；此前，合并集合是没有 family 含义的两张存储表。
- Stage C：#1414。
- `??` 路径乱象（上移一层的同类症状 — 逐点准入清单背后按 payload 类别分五条路径）：2026-08-15 已遏制。末端 scalar 路径现在用类型 guard（准入但无法路由的 operand 退到诚实 wall — 静默走错分支这一类问题不可表达）；准入清单是具名的单一来源谓词（`unwrap_or_heap_ok_route_exists` — 路径能力，不是 family）；最终 desugar（`?? → match`，一次改写删除乱象）已在 `ALMIDE_QQ_DESUGAR=all` 后实现，实测剩余 13 个 fixture — 接受 matrix 见 #1418。途中补齐两处覆盖缺口：`?? ()`（Unit-is-0 fallback arm）和用户 fn 的 `Result[Unit, String] ??`。

<a id="option-family--surveyed-2026-08-14-no-family-disease"></a>

## Option family — 2026-08-14 调查，没有 family 症状

已按同类 bug 审计 `is_self_host_option_module_fn`（mod_p4.rs），结果无问题：名称表只表示“materialized”（正是 Result 表在 Phase 2 达到的形式）。payload routing 在分类位置已由类型驱动（`is_heap_elem_list_ty`、`opt_str_int` tuple 路径等），checked 数值转换 family 按形状准入并有 drift gate（`checked_conversion_family_is_admitted`），比此前 Results 的状态更先进。唯一共有的债务是下述 registry 推导尾项。

<a id="non-goals"></a>

## 非目标

- 将 len-as-tag 与 cap-as-tag 在物理上统一为一种 block 形状（Phase 1 只消除那一个冲突；scalar-Ok 保留 len-as-tag — 该分流可由类型计算，所以无害）。
- Option / 非 Result variant（同样模式，在此之后另开工作线）。
- v0 分支修改（v0 使用自己的 lowering；由 3-way oracle 守护）。
