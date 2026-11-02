原文 description 元数据：让 stdlib/&lt;module&gt;.almd 扩展 TOML module（修复 codegen dispatch）
原文 done 元数据：2026-04-16
<a id="bundled-almide-dispatch-for-stdlib-modules"></a>

# 标准库 module 的捆绑 Almide 调度

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bundled-almide-dispatch.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bundled-almide-dispatch.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bundled-almide-dispatch.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-dispatch.md)

> 原文注记：done 日期及末尾解决记录取代较早的方案 3 建议与待批准阻塞。方案 3 未被采用，解决记录也纠正了先前 option/result 的说明。工时、效果及 target 描述是历史原文主张，不是新测量或保证；本文不证明后续 AST API 或重写规则已实现。


动机：dojo run #10 表明，`almide fix` 重试循环的集成能放大效果。如果自动重写规则、辅助原语及其他 stdlib 扩展用 Almide 编写（而非 Rust runtime + TOML template），贡献者就能更快推进。但当时通过捆绑的 `.almd` 文件扩展 `list.*` / `string.*` 等并不奏效：codegen 不论来源，都为每次 stdlib 调用输出 `almide_rt_<module>_<fn>`，因此捆绑的 `fn binary_search` 最终试图调用不存在的 Rust runtime 函数。

<a id="current-behavior-the-bug"></a>

## 当前行为（缺陷）

```
stdlib/list.almd: fn bundled_probe() = 42     # Almide source
↓ parser loads file (BUNDLED_MODULES opt-in)
↓ canonicalize: bundled fn is NOT merged into the `list` namespace
↓ lowering: no IrFunction named `bundled_probe` exists in the program
↓ codegen: user code calls `list.bundled_probe()` → rt_list_bundled_probe
↓ rustc: error[E0425]: cannot find function `almide_rt_list_bundled_probe`
```

图示说明：parser 载入 Almide probe 后，它没有并入 list 命名空间，IR 中也没有函数，却输出了 runtime 函数调用，最终触发 Rust 的未定义函数错误。

**2026-04-16 调查**（“理想形态”梳理期间）：把 `list` 加入 `BUNDLED_MODULES` + `AUTO_IMPORT_BUNDLED`，并提供含 probe 函数的 `stdlib/list.almd` 后发现，问题不*只是* codegen。当 module 名与 TOML 支持的 module 重叠时，捆绑函数根本不会进入 IR 的 `functions` 列表。当时的解释是，`option` / `result` 捆绑 source 能工作，是因为 Tier-1 module 的 frontend 解析走不同于 Tier-2 / TOML stdlib 的路径，而 `list` 位于 TOML 一侧。

因此修复有两层：

1. **Frontend 侧**：当捆绑 `.almd` 文件与 TOML 注册的 stdlib module 同名时，把其函数合入 module 的公开接口（env.fn 注册），让调用处解析为真正的 `IrFunction`。
2. **Codegen 侧**：当 `list.<func>` 调用解析为用户 IrFunction（即函数已在 IR 中定义）时，绕过 `almide_rt_<module>_<func>` 输出，使用普通用户函数调用路径。

单独任何一步都不够。roadmap 条目原先只限于 codegen，现在涵盖两层。

<a id="proposed-fix"></a>

## 建议修复

让 pass 区分“这个函数有 TOML template”和“这个函数是捆绑 Almide”。仅由捆绑 source 提供的函数跳过 `rt_` 输出，转入非 stdlib 的 `Module` 调用路径；walker 随后把它输出为普通函数引用，解析到捆绑 IR 函数。

实现草图：

```rust
// pass_stdlib_lowering.rs
let is_stdlib = is_stdlib_module(&module);
let is_toml_fn = is_stdlib && toml_module_has(&module, &func);
if !is_toml_fn {
    // Either a non-stdlib module, or a stdlib module whose `func`
    // comes from bundled Almide source. Leave as Module call.
    return leave_as_module_call(...);
}
// rt_ emission (unchanged)
```

代码注释说明：非 stdlib module，或函数来自捆绑 Almide source 的 stdlib module，保留 Module 调用；rt_ 输出保持不变。

codegen crate 必须能使用 `toml_module_has(module, func)` 查询。当前 `module_functions()` 位于 `almide-frontend::stdlib`，由 frontend 的 build script 从 TOML 构建，codegen 无法访问。

<a id="plumbing-options-pick-one"></a>

### 连接方案（选一项）

1. **将 TOML 函数清单移入 `almide-types`（或 `almide-lang`）**，让 frontend 和 codegen 都能读取。需要迁移 build-script 生成步骤。长期看最整洁。
2. **把清单作为 codegen 输入传下去。** 在 `CodegenCtx`（或 `Target` 层配置）增加 field，用 frontend 的 `stdlib::module_functions` 初始化。改动较小，但所有启动 codegen 的入口都要传递它。
3. **通过 IR 直接检查**：捆绑 Almide 文件 lowering 到 IR 后，各捆绑 `IrFunction` 可携带 `source: FunctionSource::{Bundled, User, Extern}` tag。pass 随后检查“是否有名为 `<module>.<func>`、带 `Bundled` tag 的 `IrFunction`”，以跳过 rt_ 输出。语义上最整洁：事实来源是 IR 自身。需要扩展 IR struct。

建议：**方案 3**。它使 IR 自描述（codegen 不需要外部数据），lowering 时的检查也很简单（查询 `program.functions` 的 HashMap）。

<a id="what-this-unlocks"></a>

## 它能解锁什么

捆绑 Almide 能扩展 stdlib module 后：

- **用 Almide 编写自动重写规则**：`almide fix` 修复函数目前是 `src/cli/fix.rs` 中的 Rust code。捆绑调度可用后，重写规则可写成读取并返回 AST 的 Almide 函数（须先公开 AST API，属于后续规范）。短期内，若 HELPER 原语（`list.binary_search`、`string.run_length_encode` 等）能用 Almide 而非 TOML + Rust + WASM 提供，Rust 实现的重写也能受益。
- **加深自用**：examples/ 中的 `almd-outline` demo 已显示 `ide outline` formatter 可以用 Almide 编写。下一层：formatter 的 stdlib-outline 子程序可使用捆绑 Almide list 原语，不再绕经 compiler。这是自我应用。
- **Stdlib 的 WASM 支持不再需要额外实现**：当前 stdlib 采用 TOML + Rust + WASM 形式，三处都要编辑，很容易漏一处。捆绑 Almide 是一个文件，同一 source 可编译到 Rust 和 WASM 两个 target。

<a id="non-goals"></a>

## 非目标

- 替换现有 TOML template。快速路径 intrinsic（`try_inline_intrinsic`、`try_lower_to_iter_chain`）是 Rust 专用优化，保留。
- 删除 `almide_rt_*` runtime。所有现有函数继续使用它；此次变更只影响通过捆绑 Almide 加入 stdlib module 的新函数。

<a id="testing"></a>

## 测试

修复后：

1. 添加含 `fn binary_search_v2(xs: List[Int], target: Int) -> Option[Int] = ...` 的 `stdlib/list.almd`（简单的 dispatch test，不必与现有 TOML 版相同）。
2. 把 `list` 加入 `BUNDLED_MODULES` 与 `AUTO_IMPORT_BUNDLED`（此前失败尝试已加入，可撤销或重用）。
3. 用户 code `list.binary_search_v2([1, 3, 5], 3)` 必须：
   - 通过类型检查。
   - 由 `almide run` 正确编译并执行。
   - 由 `almide build --target wasm` 生成有效 WASM。
4. 现有 TOML 支持的 `list.binary_search` 继续正常工作，无需改变（回归保护）。

<a id="estimated-scope-revised-post-investigation"></a>

## 预计范围（调查后修订）

- **Frontend 合并**：1-2 小时。查明捆绑 .almd 函数目前未能并入 TOML module 命名空间的位置，可能在 `canonicalize::registration` 或 `import_table`。现有 `option` / `result` 捆绑方式可作参照；追踪它为何有效即可找到合并点。
- **Codegen 绕行**：1 小时。IR 中已有捆绑函数后，在 `pass_stdlib_lowering` 入口检查 `program.functions` 中的 `<module>.<func>` 形式名称；存在则跳过 rt_ 输出。
- **测试**：1 小时。回归保护：向 `stdlib/list.almd` 加入新捆绑函数，验证它能通过类型检查、运行（Rust target）、编译（WASM target），且不破坏现有 TOML 支持的 `list.*` 调用方。

总计：一次专门工作时段内 3-4 小时。这是 `diagnostic-snippet-externalization` 以及整体“用 Almide 写 stdlib 并自用”路线的架构阻碍之一。

落地时预计 dojo 测量值不变（这是基础设施，不是诊断）；依赖它的下游工作才会改变指标。

<a id="when-to-implement"></a>

## 何时实现

现在（依据 dojo run #10 的建议提高优先级：“`almide fix` 效果显著，因此让自动重写规则能用 Almide 编写的价值已经确定”）。阻塞项：用户批准选择方案 3 还是其他方案。

<a id="resolution-2026-04-16"></a>

## 解决结果（2026-04-16）

已在 `llm-first-phase2` 实现。实际设计在两个重要方面偏离规范，二者都来自实现调查：

1. **没有采用方案 3（IR `FunctionSource` tag）。** 在 `pass_stdlib_lowering` 入口预扫描 `program.modules`，检查 `is_bundled_module(m.name)`，可以用更小差异获得相同自描述性（不改 IR struct，也不用逐函数传递 lowering tag）。它存于作用域为该 pass 执行的 `thread_local!` cell。如果第二个 consumer（其他 pass 或 target）需要相同查询，再考虑方案 3。
2. **规范误述了 option/result 的基准状态。** 调查声称，option/result 捆绑函数“当前可用，是因为 Tier-1 module 的 frontend 解析走不同于 Tier-2 / TOML stdlib 的路径”。修复到位后重新检查：option/result `.almd` source 从不产生 **codegen 输出**（每个函数都与 TOML entry 冲突并被剪除），但它们确实供类型检查器使用，且**覆盖 TOML signature**。具体而言，TOML 使用 `Fn[Unit] -> X`，捆绑 `.almd` 使用 `fn() -> X`，所有实际调用方都写后者。删除捆绑 `.almd` 会立即破坏 `coverage_misc_test`。两部分意外地相互依赖：捆绑 = signature，TOML = runtime dispatch。解开这种关系的路线见 `roadmap/active/option-result-bundled-cleanup.md`。

<a id="surfaced-gaps-not-in-the-original-spec"></a>

### 原规范未包含、后来暴露的缺口

- **Verifier**（`almide-ir::verify`）把 `known_module_functions` 当作权威。把捆绑 list lowering 到 IR 后，`result.collect` 对 `list.is_empty` 的调用无法通过 verify（verifier 看不到 TOML 函数）。修复：在 registry 中完全跳过捆绑 stdlib module。
- **需要剪除 TOML 重复项。** 捆绑 `.almd` 与 TOML 定义都会声明例如 `option.map`。两者都 lowering 会产生两个 `almide_rt_option_map` 定义。`src/main.rs` 的 lowering 循环会剪除名称与生成的 stdlib_sigs 中 `module_functions(name)` 重叠的 IR 函数。

<a id="files-touched"></a>

### 改动文件

- `crates/almide-types/src/stdlib_info.rs` — `list` ∈ BUNDLED + AUTO_IMPORT
- `crates/almide-frontend/src/stdlib.rs` — `get_bundled_source("list")`
- `stdlib/list.almd` — smoke 函数
- `src/main.rs` — 捆绑 lowering 放行 + TOML 剪除
- `crates/almide-ir/src/verify.rs` — 跳过捆绑 module
- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — `BUNDLED_FNS`
- `spec/stdlib/list_bundled_test.almd` — 回归保护

<a id="estimated-3-4h-actual-3h"></a>

### 预计 3-4h，实际约3h。
