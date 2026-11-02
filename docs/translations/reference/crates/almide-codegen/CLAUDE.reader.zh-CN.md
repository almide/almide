# almide-codegen

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/CLAUDE.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范、不激活的阅读快照。这是历史参考资料，不是有效的代理指令或用于生成的项目模板，不授予运行命令的权限，也不自动同步。原文主张不是独立的当前测量；未执行命令或调用模型。

通过 nanopass 流水线与模板 walker 将 IR 转换为 Rust 源码，另有功能受限的 `@gpu` WGSL 源码输出器。实际发布的 Wasm 二进制路径不在这里：`src/wasm_route.rs` 将它路由到 `crates/almide-wasm` 的 structural emitter。旧的 `emit_wasm/` emitter 和 MIR Wasm renderer 已退役；MIR 仍服务于 native verified-renderer 子集。

> **理想形态重构路线图**：[docs/roadmap/done/codegen-ideal-form.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)
> 在加入新的 codegen 修正前，先检查其中列出的“应避免临时补丁式修复的要点”。尤其是：函数解析应置于独立 pass，stdlib emit 应采用声明驱动，避免通过 `emit_stub_call` 产生运行时 trap。

<a id="three-layer-architecture"></a>

## 三层架构

<a id="1-nanopass-pipeline-pass_rs"></a>

### 1. Nanopass 流水线（`pass_*.rs`）

25 个以上语义变换 pass，每个只做一件事，在 `target.rs::build_pipeline` 中按目标组合。Rust 流水线依次为：UnifyVarTables →
ListPatternLowering → LambdaTypeResolve → ConcretizeTypes → MeteredRegionDelegate → MeterCharge →
PatternLiteralGuard → ResolveCalls → RegionWindow → BoxDeref → LICM → EggSaturation →
MatrixShapeSpec → ConstFold → IntrinsicLowering → BorrowInsertion →
OwnedSourceHoist → TailCallOpt → CaptureClone → CloneInsertion → MatchSubject → EffectInference →
StdlibLowering → StreamFusion → ResultPropagation → BuiltinLowering →
DecodeSlotHint → DecodeErrFrame → Peephole → RustLowering → FanLowering → NormalizeRuntimeCalls →
IrLinkFlatten → SharedCellBorrow → VarStorage → RangeCountingVars → TopLetStorage → BorrowLowering →
SliceBinders → StrMapKey → ThreadShareMarks → BoxTakeLowering → StrViewRebind

- 每个 pass：`impl NanoPass { fn run(&self, program, target) -> PassResult }`

<a id="2-toml-templates-templatestoml"></a>

### 2. TOML 模板（`templates/*.toml`）

各目标的语法模式。walker 替换 `{var}` 占位符。目标差异放在这里，而不是 walker 中。

<a id="3-walker-walker"></a>

### 3. Walker（`walker/`）

与目标无关的 IR renderer。**没有任何 `if target == Rust` 检查。** 所有目标相关决策都由 pass 与模板完成。

<a id="rules"></a>

## 规则

- **walker 必须保持与目标无关。** 需要目标专属行为时，添加 nanopass 或模板 guard。
- **各 nanopass 相互独立。** 每个 pass 读取并改写 IrProgram。pass 只能通过其声明假定顺序：`depends_on` / `run_before` 边，或表示边界（`UnifyVarTables`、`ConcretizeTypes`、`IrLinkFlatten`）的 `barrier()`，以及 `Postcondition`。后者具有单调性：一旦某个 pass 建立了条件，在所有 profile 中（包括 release；遍历每个文件约耗时 8 ms），每个后续 pass 之后都会重新验证该条件。因此，破坏先前 pass 不变量的 pass 会与先前 pass 一同被指出。`ALMIDE_IR_FAULT=<pass>` 在指定 pass 之后注入违例，是 release 二进制的阴性对照。还会检查声明的边是否完整反映事实：`scripts/check-pass-shuffle.sh` 按边允许的带种子随机顺序（`ALMIDE_SHUFFLE_PASSES=<seed>`）运行流水线，并对生成的 Rust 做逐字节 diff；出现分歧意味着缺少声明，`scripts/pass-shuffle-bisect.py` 会指出相应的一对 pass。新增 pass 必须在同一变更中声明其边。
- **这里没有 `Target::Wasm`。** 该 variant、它的 26-pass 流水线、wasm 专用 pass（包括完整的第二套 Perceus）以及 `Verified`/`Canonical` 证书链已在 #930 删除，也包括 tombstone 时期的 `unreachable!` 分支。Wasm 请求在到达本 crate 之前就会路由到 `almide-wasm`；不要在这里添加 wasm 输出逻辑。

<a id="history-note-the-rt-oracle-registry"></a>

## 历史说明：rt-oracle registry

`rt-oracle-registry.toml` 加上差分测试关卡，曾防止已退役的 v0 `emit_wasm/` 运行时偏离 native 运行时。`scripts/check-rt-oracle-registry.sh` 现在会带着 RETIRED 提示提前退出：v1 自举标准库实现改由 `spec/wasm_cross` fixture 与 almide-interp oracle 把关。

---

流水线清单及历史性能／实现主张是固定原文的描述，不是重新检查当前构建的结果。本资料不恢复已退役的 Wasm emitter。
