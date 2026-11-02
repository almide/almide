<a id="almide-compiler-crates"></a>

# Almide 编译器 Crate

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 固定原文代理指示的非活动、非规范读者快照。原文仍为权威且不变。CLAUDE.reader 不是自动加载的指示文件名。引用规则不授权执行、修改代码、提交、推送或发布。不自动同步，也未独立重新验证实现、安全或性能。2026-08-13 数字保留该树、release、五次最佳的范围，不是普遍性能保证。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/CLAUDE.md)


> **Codegen 理想形态（已完成的设计记录）**：[docs/roadmap/done/codegen-ideal-form.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)——新 pass 或 emit 修正的设计决策应与该记录一致。

<a id="pipeline"></a>

## 流水线

```
Source (.almd)
  → almide-syntax    Lex + parse → AST
  → almide-frontend  Type check + lower → IR
  → almide-optimize  Monomorphize + DCE → IR
  → almide-codegen   Nanopass + emit → Rust
  → almide-mir       MIR lowering → v1 native Rust render + certificate witnesses
  → almide-wasm      structural leg → WASM bytes (the only wasm renderer)
```

<a id="dependency-graph"></a>

## 依赖图

```
almide-base           Interned strings (Sym), spans, diagnostics
  ↕
almide-types          Ty enum, unification, stdlib info
almide-syntax         Lexer, parser, AST nodes
  ↕
almide-lang           Re-export facade (types + syntax)
  ↕
almide-ir             Typed IR, VarTable, visitors
  ↕
almide-frontend       Type checker, constraint solver, AST→IR lowering
almide-optimize       Monomorphization, DCE, constant propagation
almide-codegen        Nanopass pipeline, TOML templates, walker（WGSL は @gpu 関数の限定的なソース出力）
almide-mir            v1 Middle IR: ownership/layout SoT, MIR lowering + native renderers +
                      certificate witnesses (proofs/gate.sh; `almide verify` judges the
                      structural leg's since #4011); no WASM/WAT
                      renderer since #2761
almide-interp         Pre-codegen IR tree-walker — 3rd cross-target oracle / executable spec
almide-tools          Formatter, module interface (.almdi). NOT the LSP — that
                      lives in src/cli/lsp*.rs at the workspace root, and
                      lsp-types is a root Cargo.toml dependency, so a
                      crates/-scoped survey will not find it
almide-layout         THE single source for heap block layout — every consumer
                      (almide-wasm, almide-interp's arena) derives from it
almide-wasm           Commissioned structural wasm emitter: typed IR → wasm
                      bytes via wasm-encoder (no WAT text). The only
                      `--target wasm` leg (routing: `almide::wasm_route` in
                      the root lib — the CLI and the playground call the
                      same `route_wasm`, #2554)
almide-wasi           The `to_wasi` transform that makes build artifacts
                      stock-runtime, and the p1 host-op set it serves — pure
                      wasm-encoder/wasmparser, builds for wasm32 (#2554);
                      re-exported as `almide_wasm_run::wasi`
almide-wasm-run       The embedded almide.* host (wasmtime) + the p2/p3
                      component transforms
almide-anchor-check   The byte-level translation check of #4011 (d): decodes a
                      shipped module itself (wasmparser only, no compiler
                      code) and holds every located bounds / init witness
                      event to the instruction it names; run per artifact by
                      proofs/structural-wall.sh
almide-wasm-vm        The qualification-scoped wasm interpreter (#865): runs a
                      shipped `to_wasi` artifact with an instruction set pinned
                      to the emitter's, fixed buffers and fuel; std only.
                      Requirements: almide-wasm-vm/REQUIREMENTS.md
almide-spine          Salsa-cached front queries + the parity gates; its s5
                      driver re-exports `almide::wasm_leg` (root lib), so the
                      product leg and the gates judge ONE implementation
almide-corpus         Corpus path resolution (in-tree, or the als/ judge mount
                      in the greenfield form)
almide-str            ADR-0013's shared string buffer (AlmideStr: one block
                      [rc: u32, spare: u32, len | bytes], a 16-byte header,
                      edited through a unique view). Raw allocation, so it
                      does not take the workspace's `forbid(unsafe_code)`;
                      CI runs its tests under Miri (`miri-almide-str`).
                      Every generated native program splices it in as its
                      Almide String (`AlmideString = AlmideStr`;
                      docs/roadmap/active/adr-0013-string-sharing.md)
```

`almide-interp` 是已链接 IR 的*同级消费者*，不属于编译流水线：它执行的 IR 切点在 `lower → optimize →
mono → ir_link` **之后**，但在 `almide-codegen` 的任何目标 lowering pass **之前**，因此不与任一后端共享 codegen pass。参见 [almide-interp/CLAUDE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/CLAUDE.md)。

<a id="core-design-principles"></a>

## 核心设计原则

1. **类型检查器是权威来源。** 所有表达式类型都来自 almide-frontend 填充的 `TypeMap`。lowering 和 codegen 信任它，不重新推断类型。

2. **IR 携带完整类型信息。** 每个 `IrExpr` 有 `ty: Ty` 字段。codegen 在 emit 时绝不应需要查询类型检查器。

3. **VarId 消除遮蔽。** lowering 时为所有变量分配唯一 `VarId(u32)`。IR 和 codegen 中不做基于字符串的变量查找。

4. **仅在 lowering 脱糖一次。** pipe（`|>`）、UFCS（`x.method()`）、字符串插值（`"${expr}"`）由 almide-frontend 的 lowering pass 脱糖。codegen 不会看到这些形式。

5. **Nanopass 隔离。** 每个 codegen pass 只做一个语义变换。walker 与目标无关，从不检查 `if target == Rust`。目标差异编码在 pass 选择和 TOML 模板中。

6. **到处使用字符串驻留。** 所有标识符、类型名、字段名均为 `Sym`（已驻留、`Copy`）。用 `==` 比较，不做字符串匹配。用 `almide_base::intern::sym()` 驻留，用 `.as_str()` 读取。

<a id="layout-discipline-new-long-lived-structures"></a>

## 布局纪律（新的长寿命结构）

**规则（#1316）。** 编译流水线中每个**新**长寿命数据结构——模块缓存格式、MIR→CLIF 结构、寿命超过一次编译步骤的任何产物——都使用**平坦数组 + `u32` ID + 驻留原子**。不用充满 `Box`/`Rc` 的树、不用 `String` 字段，也不用加载时必须逐项重建的 map。现有 AST/IR 明确不在范围内：这是零追溯改造政策，只约束尚不存在的代码。

由 `scripts/check-layout-discipline.sh` 对照 `proofs/layout-ledger.toml` 设置门禁。cache/CLIF 模块中的类型，或冻结既有文件之外任何派生 `Serialize`/`Deserialize` 的类型，都需一行台账：`FLAT`（每次从源码重新导出，不能由该行说谎），或 `DEVIATION` 加一段说明其成本及消除条件。`DEVIATION` 数量为只减不增的上限。**门禁无法判断的内容——SoA 与 AoS、函数体、类型别名——列在脚本开头；这部分靠审查，理由写在 deviation 注记中。** 以下测量：2026-08-13、本树、`--release`、五次取最佳。

1. **驻留原子，而非拥有型文本。** 在长寿命*行*中遵循上方原则6。`Option<Sym>` 为4 B（niche packing），`Option<String>` 为24 B；仅此字段就使 `VarInfo` 从120 B变为96 B（−20%）。`VarInfo.module_origin:
   Option<String>` 保存模块*名*，而IR中其他地方均为 `Sym`；因为没有检查要求，它才混入。克隆填充该字段的100k行表，拥有型耗时1.95 ms，驻留型0.57 ms（3.4倍）。

2. **整数ID，而非嵌入树**——让相等性检查成为整数比较。`Applied(UserDefined(String), [Int])` 的 `Ty` 相等比较实测4.33 ns/cmp（`TypeConstructorId::UserDefined` 载荷为拥有型 `String`，同一用户类型用 `strcmp` 比较），三层嵌套 `Ty` 为7.28 ns，而 `u32` 为0.57 ns：分别7.6倍与13倍。`DefInfo` 有四个驻留字段和一个 `ty: Ty`，正是后者使 `DefTable` 无法直接按字节加载。

3. **平坦数组，让加载就是读取。** `spec/stdlib/stdlib-test.almd` 的IR是1.93 MB JSON；`serde_json::from_str::<IrProgram>` 与读取同样字节的 `fs::read` 相比，一次为4.269 ms对0.059 ms，另一次为6.648 ms对0.088 ms——72倍和76倍。指针密集缓存每次构建、每个模块都付此成本，平坦缓存只付读取成本。

4. **pass 只遍历一列时采用SoA。** 对200k行的 `use_count` 求和，`VarInfo` 的120 B stride耗时0.234 ms，`Vec<u32>` 列耗时0.090 ms（2.6倍）。这是门禁*无法*强制的一条——平坦行的 `Vec<Row>` 能通过，但它是结构体数组。由审查决定并记录。

可仿照的形态已经存在：`almide-ir` 中的 `VarId`/`VarTable`、`DefId`/`DefTable`（`u32` newtype + `Vec<Row>` + `get(id)`），以及 `almide-base` 中的 `Sym`。

<a id="when-adding-a-new-feature"></a>

## 添加新功能时

- **新语法** → almide-syntax（parser）→ almide-frontend（checker + lowering）→ almide-codegen（passes + templates）。
- **新stdlib函数** → `stdlib/<module>[_<part>].almd` 中的纯Almide实现；在函数体的 `fn` 上方用 `@implements("module.fn")` 为WASM + interp注册（build生成行；`self_host_registry.rs` 是只缩减的legacy表）。仅在必要时使用 `runtime/rs/src/<module>.rs` 中的native intrinsic加 `@intrinsic` 声明。解释器在支持的基础范围上自动求值已注册的自托管函数体。只有该范围无法服务函数时才加手工bridge glue，并验证三路oracle仍覆盖它而非弃权。参见 [almide-interp/CLAUDE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/CLAUDE.md#coverage-model--does-a-new-stdlib-fn-get-covered-automatically)。
- **新类型** → almide-types（Ty variant）→ almide-frontend（推断规则）→ almide-ir（IR nodes）→ almide-codegen（发射）。
- **新codegen目标** → almide-codegen（pass流水线 + TOML模板 + 目标入口）。
- **多个stdlib函数共享的性质**（消耗源值、原地修改、可复用即将结束生命周期的源值等）→ 在函数本身的 `.almd` 声明中作为属性声明（`@consume`、`@borrow_ref` 等），让每条执行路径读取该声明。绝不以名称列表回答（`matches!(f, "set" | "swap")`、`&["almide_rt_…", …]`）：列表是性质的第二份副本，后加函数会静默遗漏。由 `tests/name_keyed_sites_test.rs` 对照 `proofs/name-keyed-sites.toml` 设门禁——既有名称列表是只缩减的 `legacy` 行，新增列表被拒绝。

## 原文图中说明的译文

流水线：.almd 源码经 syntax 词法／解析成为 AST，经 frontend 类型检查与 lowering 成为 IR，经 optimize 单态化与 DCE 成为 IR，经 codegen 的 Nanopass 与 emit 成为 Rust，经 mir 的 MIR lowering 成为 v1 native Rust render 与证书 witness，最后由 wasm 结构路径生成 WASM 字节。最后一项被记为唯一 wasm renderer。

依赖图按顺序说明：

- almide-base：驻留字符串 Sym、span、诊断。
- almide-types：Ty 枚举、类型合一、stdlib 信息。almide-syntax：lexer、parser、AST 节点。
- almide-lang：重新导出 types + syntax 的 facade。
- almide-ir：带类型 IR、VarTable、visitor。
- almide-frontend：类型检查器、约束 solver、AST→IR lowering。almide-optimize：单态化、DCE、常量传播。
- almide-codegen：Nanopass 流水线、TOML 模板、walker。WGSL 是仅限 @gpu 函数的源码输出。
- almide-mir：v1 Middle IR，所有权／layout 权威来源，MIR lowering + native renderer + 证书 witness。使用 proofs/gate.sh；从 #4011 起 almide verify 判断结构路径的证书。从 #2761 起不再有 WASM/WAT renderer。
- almide-interp：codegen 前的 IR tree-walker，第三个跨目标 oracle／可执行规范。
- almide-tools：格式化器、.almdi 模块 interface，不是 LSP。后者位于 workspace root 的 src/cli/lsp*.rs，lsp-types 是 root Cargo.toml 依赖，因此仅调查 crates/ 会漏掉它。
- almide-layout：堆块 layout 的唯一权威来源；包括 almide-wasm、almide-interp arena 在内的所有消费者都从它导出。
- almide-wasm：已启用的结构式 wasm emitter，通过 wasm-encoder 将带类型 IR 变为 wasm 字节，不使用 WAT 文本。唯一 --target wasm 路径；root lib 中 almide::wasm_route 路由，CLI 和 playground 调同一个 route_wasm（#2554）。
- almide-wasi：使构建产物适配 stock runtime 的 to_wasi 变换，及其服务的 p1 host-op 集合。纯 wasm-encoder/wasmparser，可为 wasm32 构建（#2554）；以 almide_wasm_run::wasi 重新导出。
- almide-wasm-run：嵌入式 almide.* host（wasmtime），及 p2/p3 component 变换。
- almide-anchor-check：#4011(d) 的字节级翻译检查。仅用 wasmparser、不用编译器代码，解码出货模块本身，将每个已定位 bounds/init witness event 与它指明的指令核对。由 proofs/structural-wall.sh 按产物运行。
- almide-wasm-vm：限定适格性评估范围的 wasm 解释器（#865）。使用固定为 emitter 对应集合的指令、固定缓冲区和 fuel，运行出货的 to_wasi 产物；仅依赖 std。要求位于 almide-wasm-vm/REQUIREMENTS.md。
- almide-spine：Salsa 缓存的前端 query 和 parity gate。其 s5 driver 重新导出 root lib 的 almide::wasm_leg，因此产品路径与门禁判断同一实现。
- almide-corpus：解析语料路径，位于树内或 greenfield 形式的 als/ judge mount。
- almide-str：ADR-0013 的共享字符串缓冲区。AlmideStr 为图示 rc、spare、len、bytes 单块，16字节 header，经唯一视图编辑。采用 raw allocation，因而不采用 workspace 的 forbid(unsafe_code)。CI 以 miri-almide-str 在 Miri 下运行其测试。每个生成的 native 程序都将它拼入作为 Almide String（AlmideString = AlmideStr）；参见 docs/roadmap/active/adr-0013-string-sharing.md。
