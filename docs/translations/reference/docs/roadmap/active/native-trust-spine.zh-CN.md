原文说明：让 almide build --target rust 经与 wasm 相同的 v1 Perceus MIR，使单一内存模型服务两条路径
<a id="native-trust-spine--perceus-as-the-single-memory-model-764"></a>

# Native Trust Spine — Perceus 作为单一内存模型（#764）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-trust-spine.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](native-trust-spine.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](native-trust-spine.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-trust-spine.md)

> 原文注记：7月14～16日阶梯记录阶段实现与测量子集，不是当前支持清单、普遍安全结果或工具鉴定。已交付勾选 slab 与未勾选父 rung、过时原 recipe 并存；record access 旧案 FieldGetScalar，而已交付 slab 用 ListGetScalar。rung6 的18/18仅指18/249可 render fixture 内，org整文件0/61，不是全 corpus 支持。scalar capture/signature 不代表 heap 或 Float closure 覆盖。旧 fallback 和验证 opt-out 已被末尾明确拒绝/structural-only 注记替代，不授权绕过验证。shim 底座及 renderer 对应仍为限定信任义务，平衡 checker 不等于任意输出正确性。未重跑 binary、差分 corpus、proof 或 benchmark。


> 目标：`almide build --target rust` 经过与 wasm 完全相同的 v1 Perceus MIR，render 为符合 ownership 惯例的 Rust。`Dup` → `.clone()`，`Drop` 消去为 Rust scope 结束 drop，runtime 边界映射到封闭 native shim 底座。`verify_ownership`（与 Lean 对应的 checker）检验并确认 render 消去的同一组 op 的 Perceus 平衡，即每次构建检查 Drop 消去 rendering 定理。交付模式仿照 wasm：opt-in→ladder→default。

<a id="architecture-findings-2026-07-14-rung-1"></a>

## 架构发现（2026-07-14，rung1）

- 用户函数 lowering 为 target 中立 MIR op（`IntBinOp`/`CallFn`/控制流/`Alloc`/`Dup`/`Drop`）；wasm 特性集中在 (a) 基于 prim/linear-memory 的自托管 runtime fn 和 (b) heap handle repr。
- 所以 native 路径直接 render 用户函数，将 runtime 边界名（`int.to_string`、`print_str`、`__chk_div` 等）映射到封闭 native shim 底座。与 v0 `runtime/rs` 同一纪律，不在 inline 中重实现。
- 明确拒绝 native render 基于 prim 的自托管本体：那会在 Rust 中发出 linear-memory emulator，虽正确却不惯用、慢。

<a id="ladder"></a>

## 阶梯

- [x] **Rung1**（已交付，`--target rust` 上以 `--verified` opt-in）：i64 scalar、String literal＋`int.to_string`、`println`、完整 scalar 控制流（if-as-value、loop）、scalar 用户 fn、C-001/C-002 abort shim。
  gate：`tests/native_v1_differential_test.rs`，corpus 比较 v1/v0 字节（stdout/stderr/exit），wall case assert `Err`。其余处全部诚实 wall（首个观测 wall 是 `__str_concat`）。
- [x] **Rung2**（已交付）：dynamic String op shim（`__str_concat`、`string.eq`、`string.len`，后者数 codepoint）；用户 fn 的 String param/return 按 MIR call-mode signature 使用 borrowed `&str` / owned `String`；String 值 if-join 在首 arm yield 时 patch decl。pipeline 设精确 wall：heap `Repr::Ptr` param/result 只有声明 `Ty` 指明 String 才 render 为 string；{Int, Bool, String, Unit-ret} 外 signature 在 lowering 前 wall。
- [x] **Rung3**（已交付）：String 底座扩大到当前可达的完整边界表面：`string.contains` / `starts_with` / `ends_with` / `to_upper` / `to_lower` / `trim` / `repeat` / `cmp`。每个 shim 都是精确 v0 oracle 表达式，延续 C-016/C-019/C-020 纪律。另接通 `almide run --verified` native（`compile_to_binary_with`）。
- [x] **Rung4 — SCALAR LIST，经共享 MIR op 交付（2026-07-15，ceangal/module-var 工作线按计划构建）**：三个 target 中立 op `Op::ListLit { dst, elems }` / `ListGetScalar { dst, list, idx }` / `ListSetScalar { list, idx, val }`，在三个 producer（scalar literal builder、`lower_scalar_index_access`、含 mutable-global 路径的 IndexAssign store）逐一替换 inline `Alloc{DynList}`＋`Handle`/`ElemAddr`/`Load|Store` prim 序列。render_wasm 展开为精确旧 WAT，行为相同；RATCHET0＋KERNEL 重验全 corpus，cert stream 不变：ListLit 属 alloc-class `i`，get/set 是中立 borrow，Coq 词汇不动。native 用 pipeline 从声明类型构造的 SIG-KIND 表将它们定为 `Vec<i64>`/`&[i64]`，单凭 heap `Repr::Ptr` 无法区分 String/List。bounds shim 与 `$elem_addr_chk` 字节相同地 abort（"Error: index out of bounds"，exit1）。差分 corpus：list_param / list_index_math / list_set。范围 `List[Int]`/`List[Bool]` signature；Float list 采用 rung5 f64 约定。`list.len` 仍为自托管 CallFn（已 target 中立，4b 决定是否 op 化）。
- [ ] Rung4 残余（保留原设计记录）：v1 lower 将 list literal 实体化为 `Alloc{DynList}`＋inline `Prim` store，允许对实体化 list 作直接 `xs[i]` prim load。list 世界按设计低于 prim 底座，故 boundary-name mapping 无法让 native 到达。拒绝 native renderer 匹配 prim idiom（猜测，而非 op 忠实性）。路径是 target 中立 list op，如 `Op::ListLit`/`ListGet`/`ListLen`/`ListSet`，render_wasm lowering 为精确当前 prim 序列（既有 gate 保护字节一致），native 映为 `Vec` op。这触及 ceangal/module-var 工作线正在编辑的相同 `lower/binds*` brick，应与它共同工作，不要另行并列。
- [ ] Rung5：record/variant（native struct/enum）、closure。
  **Records slab 已交付（2026-07-15 晚）**：scalar-record literal 经 `Op::ListLit`（声明顺序 slot，默认补零；`try_lower_scalar_record_construct` 维护 `materialized_aggregates`）。scalar field 读取经带静态 slot index 的 `Op::ListGetScalar`（`lower_scalar_field_access`，含 tuple 路径；pre-Handle block resolver 拆成 `resolve_aggregate_container_block`）。pipeline 经 `try_render_rust_source` 传递 record/variant layout，将全 scalar `Named` record 接纳为 `NativeSigKind::ListI64`。native 把 record block render 为 `Vec<i64>`/`&[i64]`；mask 驱动 `DropListStr` record drop 消去到 scope 结束（全 scalar ⇒ 空 mask）。三路字节相同；差分行 record_field / record_out_of_order / record_return / record_float_field；classify wall-list 字节相同。
  **Variants slab 已交付（2026-07-15 夜）**：flat-variant ctor 经 `Op::ListLit`（slot0 tag＋field＋补零至类型宽度；heap-field ctor 保持 prim 路径，tracking 精确镜像 prim：needs_rec→variant_drop_handles、materialized_aggregates）。match 读取：slot0 tag 及所有 scalar payload 在 subject block 上经 `Op::ListGetScalar`（value-match 非 unwrap 路径、unit-match、bind_variant_arm）；heap payload bind 保持基于 h 的 LoadHandle。native 加 DEAD-PURE-HANDLE 消除：scalar-only match 留下未使用 `Prim{Handle}`；Handle 纯，未使用者 render 为 no-op，使用者仍诚实 wall。三路字节相同（variant_match / variant_nullary / variant_multi_payload），classify wall-list 字节相同。首次 gate REJECT（kernel checker）来自 `loop_carried_slots` 的 ListLit-feeder gap；certificate.rs 现在也将其视为 alloc-class（record 重赋值的 `idd`＋`i` 拆分）。
  **Closures slab 已交付（2026-07-16 夜）**：scalar-capture env block 经 `Op::ListLit`（[fnidx, drop-header=0, captures…]，lift_lambda fast path）；heap/closure capture 保持 prim Dup/Consume 路径。lambda prologue scalar-capture load 与 call-site fnidx（slot0）经 `Op::ListGetScalar`。native render：`Op::FuncRef` 为 lambda 名称排序 index（`let vN: i64 = K`）；`Op::CallIndirect` 为生成的 `__almd_ci_<arity>(idx, env, args…)` dispatch 表，每 arity 一份，def/call 从相同 BTreeMap 顺序得到 index 空间，按构造一致；仅 i64-ret lambda 有 arm，heap-ret CallIndirect 在 dispatch 前 wall。Vec 上 `DropVariant("closure")` 消去到 scope 结束（drop header0，无递归释放）。signature 接纳：`(Int|Bool…) -> Int|Bool` Fn 类型→ListI64（env block 以 `Vec<i64>` 流动）；lifted lambda 由 MirParam repr 注册 [ListI64, I64…] param kind（heap param 使整个程序 wall，因 dispatch arm 无法类型化）。三路字节相同（closure_capture / closure_two_envs / closure_multi_arg）。下一边界：heap-capture env block（prim 路径）、heap-param/-ret lambda（typed dispatch 表）、Float capture。
  **Rung6 已测量＋默认已切换（2026-07-16）**：v1 native renderer 成为 default（`--no-verified` 退出两条路径；诊断在 `ALMIDE_VERIFIED_DEBUG` 后）。切换证据：12个差分行三路字节相同（v0 oracle 现固定 `--no-verified`，切换后裸 `almide run` 就是 v1-first），加 spec/wasm_cross native 扫描：**18/249 fixture 可 render，18/18字节相同**。扫描抓到一个真实偏离：`rt_chk_mod` 说 "modulo by zero"，v0/wasm C-002 oracle 为 "division by zero"，切换前修复。ORG native 列：**0/61 org source 文件可 render**（yaml/sha1/toml/svg/rsa/base64/bigint/aes/ceangal/almai/homullus/porta）。subset（scalar core＋rung5四个 slab）尚不到 org 级代码；probe 的首批 blocker 是 `Bytes` param（sha1/base64/aes）、multi-module＋top-let（ceangal/porta/homullus）、Map/String 密集 stdlib。其他处诚实 wall→v0，所以 verified subset 外切换造成零行为变化。下一测量工具：逐函数 native classifier（wasm classify_corpus 类似物），使 org 列按 fn 覆盖而非整个 program 全有或全无计数。
  **原 variants recipe 注记（保留）**：ctor block 是相同 DynList（slot0=tag，slots1+=payload；`try_lower_variant_ctor` 的 Alloc+stores→前置 tag const 的 ListLit）；match 解构＝tag Load(slot0)＋Eq dispatch＋payload load（binds_p4:451/538 解构点→ListGetScalar）；signature 接纳＝scalar-payload variant→ListI64；drop＝native `DropVariant` 消去 arm（scalar payload ⇒ block free）。
  **Record/variant 侦察（2026-07-15，真实 lower 路径、已传 layout 的 probe_native --mir）**：scalar record lowering 为与 list 相同 DynList block（12-byte header＋8-byte slot；`Init::DynList`）；field 读取是 raw `Handle + Add(12 + 8*slot) + Load` prim 序列，是 rung4 对 list 已解的低于 prim 底座类别。因此 record slab 比担忧的小：scalar-record literal 可原样复用 `Op::ListLit`（同 block、同 cert `i`），field 读只需一个新 op `Op::FieldGetScalar { rec, slot }`（静态 offset，无 bounds check，wasm render 与当前 prim 序列字节相同，native `rec[slot]`）。多个 lower 点发出 prim 序列（binds.rs:672、calls_p2.rs:1270、binds_p4.rs:451/538、defunc_tuple_fold.rs×2），对 classify wall-list 字节 gate 逐点 op 化。variant ctor 是到合成函数的 `CallFn "<Ctor>"`（assembler 侧），本体为 prim store；同样 op 化解锁，再审计 match 解构 tag-dispatch lowering。注意：首次 probe 使用无 layout 的 `lower_function_all`，误把 record 当 lowering gap；始终经 `lower_function_all_with_globals` dump（修复后 `debug_dump_mir` 如此）。
  **Float slab 已交付（2026-07-15）**：`NTy::F64`/`NativeSigKind::F64`；float literal 保持 bits-typed `i64` local，在每个 float-op 边界转换（`f64::from_bits`，bit exact）。FloatBin Add/Sub/Mul/Div、FloatUn Neg/Abs/Sqrt/Floor/Ceil、FloatCmp（均 IEEE hardware 跨 target 相同）；Min/Max/CopySign wall（Rust/wasm NaN semantics 不同，只从不 native render 的 self-host 本体可达）。`float.to_string` shim 为精确 v0 oracle 表达式。差分行 float_print / float_arith / float_branch / float_fn_param，加三路 probe（v0-native==v1-native==wasm）。新增 `debug_dump_mir`＋examples/probe_native.rs 为 rung 开发工具。
  **原设计注记（留档）**：MIR 把 Float 放在持 f64 bits 的 i64 local 中（`PrimKind::FloatUn/FloatBin` 每 op 前后重解释，render_wasm_p2 831-851）。native“真正 f64”需要：(1) `NTy::F64`＋`NativeSigKind::F64`（sig 表已在 `Repr` 无法区分处辨明声明 Ty）；(2) 内部 local 类型：当且仅当来自 FloatBin/FloatUn 或 Float-kind param/call-result 才为 F64，与 `Str`/`Vec` local 同样 op 驱动推断；(3) `FBits`/`FFromBits` prim 成为 `f64::to_bits`/`from_bits`（或 consumer 恰为对应 reinterpret 时 no-op）；(4) shim：`float.to_string` → 与 v0 相同 `almide_rt_float_to_string`（Dragon4，字节相同）；literal 上 `prim.ffrombits(<const>)` fold 到精确 f64 常量。差分行：float literal print、算术链、compare/branch、fn param/return、`float.to_string` round-trip。gate：corpus 增加，wall 以当前 `out_of_subset_walls_honestly` 中 assert 的 `float` 行缩小。
- [x] Rung6（2026-07-16）：诚实测量 org byte-verify native 列，0/61文件（Bytes param、多 module＋top-let、Map/String stdlib 为首批 wall；下一工具逐 fn native classifier）。
- [x] 默认切换（2026-07-16，develop 0ff5ad51）：v1-first native（`--no-verified` 退出两条路径），18/18 wasm_cross native render 与 v0 字节相同作为切换证据，关闭 #764。
- [x] Walled-real ratchet 枚举形式（2026-07-16）：v1 test-runner 切换让每个 `test` block 成为一等 corpus fn（约1.9k新 fn），暴露扩张前 `wall=0` ENDGAME 从未测到的217 wall；一轮扫描削至37（sized-numeric compare、单一递归 typed-eq engine、widening identity、nested-list eq self-host、map/bytes index desugar、nested-unwrap ANF-lift、call-result container）。gate 现为相对 `proofs/walled-real-baseline.txt` 的严格集合 diff（新 wall→失败、过期 entry→失败、集合只缩，目标为空），余项 #791 跟踪。

<a id="intended-invariants-and-their-evidence-boundary"></a>

## 期望不变量及证据边界

- subset 外拒绝是期望的接纳边界，不是所有接纳程序正确 render 的证明。ownership 检查及差分测试覆盖声明性质/fixture，不是任意输出正确性。
- 上述 v0 fallback 与 `--no-verified` 描述是历史 rung 记录。当前 `--no-verified` 被拒绝（`src/main.rs`），Wasm 路径 structural-only（`src/wasm_route.rs`）；旧 option 不是绕过验证的指示。
- `verify_ownership` 在 render 前对每个 lowered fn 执行。
- 差分 gate corpus 只增长，rung 连同 corpus 行交付。
- shim 底座是封闭 map（`render_native.rs::shim`）；新增属于 trusted-floor 变更，需要同 PR 的差分行。
