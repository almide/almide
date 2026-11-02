<a id="closure-codegen-cross-target-gaps"></a>

# 闭包 codegen 的跨目标差异

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](closure-codegen-cross-target-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](closure-codegen-cross-target-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](closure-codegen-cross-target-gaps.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-codegen-cross-target-gaps.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史元数据：描述 — 对抗性差分扫描发现的跨目标（native 与 wasm）闭包 codegen 差异 — 全部 8 项已修复；完成日期 — 2026-06-02。

> 原文注意事项：“全部 8 项已修复”和测试总数描述当时的扫描，最后一节仍明确保留架构问题，包括 cell 内对象泄漏。“良性”是原文区分泄漏与崩溃的说法，不是一般安全保证。PR 摘要称五项修复，首表将其归为四行；保留数量与措辞，不虚构一行来调和。Lean 证明的表述仅限所述 cell 不变量。

> 在 [Closure Architecture v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)（捕获 *cell*）之外，第二轮对抗性跨目标差分扫描（重跑保存的 `p6-cross-target-sweep` workflow：机器生成多样闭包程序，对比 `almide run` 与 `wasmtime`，进行完整性批评）发现了一批闭包**类型检查／解析／存储**方式上的差异 — 与捕获 cell 机制正交。方法是：即便源代码相同，只要一个程序在某目标上正确编译并运行，在另一目标上却报错／trap／静默产生差异，就是 bug。任何捕获／codegen 变更都应复用这项扫描。

状态：**完成** — 全部 8 项已修复并进入 `develop`。5 项在 PR #334；3 个深层问题（map 中存储的闭包、匿名 record 的闭包字段、经闭包对非 Copy 元素作 IndexAssign）在后续修复。各次之后 spec 240/240、cargo 791/0 和扫描重跑均无问题。

<a id="fixed-pr-334-develop"></a>

## 已修复（PR #334、`develop`）

| 差异 | 症状 | 修复 |
|-----|---------|-----|
| WASM `string.push` / `string.clear` | 无 WASM dispatch 分支 → emitter ICE（native 正常） | 在 `emit_wasm/calls_string.rs` 增加分支（push 使用 `string_append` + `emit_mutator_writeback`；clear 将 len 设为 0） |
| 元组中的闭包 → `let (g,_)=…` | native `E0562`（绑定类型中出现 impl-Trait） | 将 let 类型中嵌套的 `Fn` 子树擦除为 `_`（`erase_fn_types`、walker），让 rustc 推断具体闭包类型 |
| `fs[0]()`（调用从列表索引取得的闭包） | 被解析为 const-generic `fs::<0>()` → native `E0425` / wasm trap | `parser/helpers.rs peek_type_args_call` 现在要求方括号内含 `TypeName` token — `[0]` 是索引，`[Int]` 是类型参数 |
| capture-clone 包装闭包上的带类型参数 `(k: String)=>` | 参数注解丢失（`move |k|`）→ `E0282` | walker Bind 穿过 `{ let __cap=…; move|k|… }` 块为尾部 lambda 的参数加注解（HOF 参数 lambda 仍无注解 — 由 `render_iter_chain` 内联拼入） |

已验证：spec 240/240、cargo 784/0、+5 个跨目标回归测试、parser 无回归。

<a id="also-fixed-the-3-deep-ones"></a>

## 另已修复（3 个深层问题）

<a id="closures-stored-in-a-map--e0308"></a>

### 存在 Map 中的闭包 → `E0308`

```almide
var m: Map[String, () -> Unit] = map.new()
map.insert(m, "a", () => { list.push(acc, 1) })
let g = map.get_or(m, "a", () => {})   // was: error[E0308] mismatched types
```

修复：`pass_rust_lowering` 现在对 `Map[K, Fn]` 的 `map.insert` / `map.get_or` 闭包参数应用 `RcWrap`。map 的值类型仍为擦除后的 `_`，随后从装箱参数推断出统一的 `Rc<dyn Fn>` — 与 `List[Fn]` 字面量完全相同（让 `_` 从装箱元素推断的技巧），因此无需更改类型输出。元组保留 `_` 擦除路径（槽位彼此独立）。

<a id="closure-in-an-anonymous-record--e0277"></a>

### 匿名 record 中的闭包 → `E0277`

```almide
let r = { run: () => { list.push(acc, 1) } }
r.run()                                  // was: error[E0277] (closure isn't Debug/PartialEq)
```

修复：泛型匿名 record 的 struct 要求 `T: Clone + Debug + PartialEq`，闭包不满足后两项。带闭包字段的匿名 record 现在只 derive `Clone`，移除那些泛型约束（derive(Clone) 会重新添加 `T: Clone`）— 与 `type` 声明的 record 已有的 `has_fn_fields` 放宽规则一致。在 `collect_anon_records` 期间追踪 Fn 字段键。（`type` 声明的 record 一直可用。）

<a id="indexassign-of-a-non-copy-element-through-a-closure--wasm-trap"></a>

### 经闭包对非 Copy 元素作 IndexAssign → WASM trap

```almide
var xs: List[String] = ["a", "b"]
let f = () => { xs[0] = xs[0] + "!" }    // was: wasm trap (native "a!!")
```

根因：捕获的共享 cell 在 cell ptr 上通过普通 `rc_inc` 执行 `rc_inc` 操作，但其 `RcDec` 执行了带类型的 rc_dec，把 cell ptr 当作列表遍历 — 将 `cell[0]`（对象 ptr）读成元素数量，对垃圾地址执行 decref。`List[Int]`（Copy 元素，无元素 drop 循环）幸存，`List[String]` 则 trap。修复：`mutable_capture` 的 `RcDec` 现在对 cell 使用普通 `rc_dec`，与 `rc_inc` 匹配（`inc == dec` — 经 Lean 证明的 cell 不变量）。

<a id="fragility--elimination-targets-surfaced-while-fixing-the-above"></a>

## 脆弱点／消除目标（修复上述问题时发现）

1. `emit_wasm/statements.rs` 从值的类型（`byte_size(value.ty)`）推导列表元素宽度，而非列表声明的元素类型 — 容易偏离。未解决。
2. cell 解引用按操作分别重做（Var read / IndexAccess / IndexAssign / 每个 mutator / RcInc / RcDec），而非统一“解引用一次 → 普通 ptr”的抽象。未解决。
3. ~~parser 的 `[X]()` 歧义~~ — 已消除（#334）。
4. ~~闭包装箱只覆盖 `List[Fn]` 字面量~~ — 扩展到 `Map[K, Fn]`（insert/get_or）和匿名 record 的闭包字段。`Set[Fn]` 与一般容器元素位置仍依赖 `_` 推断技巧；单一的“把进入同质容器的任何闭包装箱”pass 可以涵盖全部情况。近似未解决。
5. ~~非 Copy 元素的 `IndexAssign` 未协调 cell 解引用与 drop~~ — 通过上述 `RcInc`/`RcDec` 的 cell 一致性修复。更深的问题仍在：cell 的 RC-drop 释放 cell，却不释放内部对象（所有 cell 类型共有的泄漏，包括 `List[Int]`）— cell 需要带类型的析构函数。未解决（良性：泄漏，而非崩溃）。

> 保留的代码注释含义：Map 示例此前产生 E0308 类型不匹配；匿名 record 示例因闭包不具备 Debug/PartialEq 而产生 E0277；索引赋值示例此前 wasm 会 trap，而 native 为 "a!!"。
