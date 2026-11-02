<a id="adr-0021-a-lambdas-failure-channel-carries-the-error-type-its-s-agree-on--contextual-type-first-then-the-join-string-as-the-top"></a>

# ADR-0021：Lambda 失败通道携带各 `!` 一致的错误类型 — 优先上下文类型，其次 Join，String 为顶

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0021-lambda-failure-channel-is-the-join-of-its-bangs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0021-lambda-failure-channel-is-the-join-of-its-bangs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0021-lambda-failure-channel-is-the-join-of-its-bangs.zh-CN.md)

> 非规范性历史读者快照，未修改固定原本为权威来源。不自动同步，未独立扫描语料、重跑基准或证明当前保证。开头尚未实现及分阶段计划，与带日期#2725实现注记并存。D2/D6输出不变受该注记限制：native repr对Option、Float、控制字符转义会改变。既有语料零例是限定范围测量，不代表所有程序；出现次数行可跨同一attempt重叠。join全域性只针对通道合成，不代表无所有类型错误。保留ADR-0003文本与实际E→String实现差异。跨语言runtime与成功主张属原文当时内容，此处未重验。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md)

- **状态**：已接受（设计裁定，实现尚未开始，阶段见 D7）。
- **日期**：2026-09-27
- **决定范围**：如何确定 lambda 自身失败通道的 E，即函数体 `!` 所落入的通道。包括类型规则、默认、诊断/fix-it、与 `__fallible_*` 规范形式改写的关系、迁移影响及实现阶段。
- **相关**：[ADR-0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md)（**修订 D1 的“E 固定 String”和 D4**）、[ADR-0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md)（D5 的 #489 不变量不变）、[ADR-0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md)（**取代 D4-4**）、[ADR-0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md)（不变，传播点规则直接用于 lambda 通道）、`docs/specs/result-option-effect.md` L3（“E 固定 String”，本 ADR 修订）、#2601（起点）、PR #2714（E022 指明 callback）。
- **经过**：2026-09-27裁定 #2601。实现方提出 A（现状 + 好的 E022）/ B（规范形式扩展至 tail 叶）/ C（推断通道 E）。以八种语言实跑 probe（Rust、Swift、Go、TypeScript、OCaml、Zig、Lean、MoonBit）、第一手资料、Almide 全语料扫描及 dojo bank pilot 生成物测量后决定。调查笔记为 `almide-references/RESEARCH-typed-errors-in-closures.md`。

<a id="context"></a>

## 背景

Lambda 是“迷你可失败 fn”，本体 `!` 落入其自身失败通道（ADR-0006 D1、#489，不跨 closure 边界）。当时通道**固定为 `Result[_, String]`**（`infer_expr_g3_lambda` 创建 `Ty::result(fresh, Ty::String)`）。variant E 的 `!` operand 静默变为 Debug 字符串落入其中。

唯一例外是：传给 `list.*` / `fs.*` 可失败 HOF 的 callback，**整个**函数体为 `call(..)!` 时，`normalize_fallible_hof_callback` 剥离标记并转给 `__fallible_*`，call 自身的 `Result[_, E]` 原样成为 callback 类型。只有这种写法保留 E。

<a id="実測-現状の型は綴りで変わるalmide-0640almide-check"></a>

### 实测：当时类型随写法改变（Almide 0.64.0，`almide check`）

`step: (Int, Int) -> Int!E`，外层 fn 为 `-> Int!E`，`xs |> list.fold(0, CB)!`：

| CB 写法 | 结果 |
|---|---|
| `(acc, x) => step(acc, x)!` | ok（保留 E） |
| `(acc, x) => { step(acc, x)! }`（只加花括号） | **E022** |
| `(acc, x) => x \|> step(acc)!`（只改为 pipe） | **E022** |
| `(acc, x) => if x == 100 then acc else step(acc, x)!` | **E022** |
| `(acc, x) => { let v = step(acc, x)!; v + 1 }` | **E022** |
| 将 `(n) => step(n)!` 传给用户 HOF 类型化 slot `f: (Int) -> Int!E` | **E005**（规范形式也不行） |
| `let f: (Int) -> Int!E = (n) => step(n)!` | **E001** |

不改变含义的编辑（花括号、pipe、一行 `let`）把 HOF 调用类型从 `Result[_, E]` 改成 `Result[_, String]`，错误却出现在远处 `)!`。ADR-0012 D2 允许 fn 类型 slot 中用 `T!E`，却没有一个带 `!` 的 lambda 能放进去。这正是 MSR 缺陷的形态。

还测出两项实际危害：

- **仍有 check 通过而 rustc 失败的缺口（0.64.0）**。map 结果不用 `)!` 而用 `match` 消费时，`(x) => match x { 0 => 0, _ => step(0, x)! }` 通过 `almide check`，随后 rustc 报 E0277 + E0308，wasm 报 E082（`ty-mismatch:Scalar(Str)-vs-Named(0)`）。#2660后只有 #2601 的 `)!` 形式变为 E022。
- **只有 native 接受擦除**。在 `-> Int!` fn 中将 typed `!` 放入 lambda，native 打印 `err Neg(-2)` 并通过；wasm 两条 leg 却以 `unwrap-err-ty-mismatch` 拒绝（E082）。

<a id="実測-llm-はこの形を書き全部落ちる"></a>

### 实测：LLM 会写这种形态，且全部失败

汇总 dojo bank pilot（run 36083949429、2026-09-25、compiler 0.63.1）。typed-error 任务有66个 attempt 给 variant E call 加 `!` 放进 callback（全为 llama-3.3-70b）：

| 位置 | 出现 | 失败 |
|---|---:|---:|
| 规范形式（整个本体为 `f(..)!`） | 36 | 30 |
| block 中途的 `let v = f(x)!` | 26 | 26 |
| if/match tail 叶 | 16 | 16 |

**至少含一个非规范 typed `!` 的 attempt 有37个，全部失败。** 其中14个由模型自行引入（label-orders 6、ignore-expired-coupons 4、restore-isbn 3、retired-signing-key 1）。典型编辑是为修改而将 baseline 规范形式 `(cart, code) => apply_coupon(book, cart, code, today)!` 展开为 `{ let c = find(book, code)!; ... }`。这正是 MSR 测量的“因编辑而坏掉”路径。bank 的人工 baseline 也有3个包含此形，0.64.0无法编译（`early-recovery/{drop-exact-repeats-of-a-reading, reorder-events-with-small-clock-skew,
merge-overlapping-shifts-in-the-timesheet}`）。#2601 是编写 bank 题目时发现的。

<a id="実測-既存コードへの露出almide---emit-ast--解析器"></a>

### 实测：既有代码暴露范围（`almide --emit-ast` + 分析器）

统计 test block 外，lambda 自身本体（不含嵌套 lambda）带 `!` 的数量：

| 语料 | 数量 | 规范形式 | 仅 tail 叶（B 覆盖） | block 中途（仅 C） | variant E 的 `!` |
|---|---:|---:|---:|---:|---:|
| `spec/` | 109 | 79 | 11 | 19 | 0 |
| `stdlib/` | 2 | 0 | 2 | 0 | 0 |
| `tests/` + `examples/` | 18 | 14 | 0 | 4 | 0 |
| `research/benchmark/exercises/` | 0 | – | – | – | – |
| almide-dojo(main: tasks/msr/src/bank) | 6 | 6 | 0 | 0 | 0 |
| almai / comide / golemide / gramide-cli | 0 | – | – | – | – |
| **合计** | **135** | **99** | **13** | **23** | **0** |

11个 operand 单靠语法无法确定类型，人工追踪确认全属 String（`Handler` / `HttpHandler` 类型 closure 参数、`Order.total -> Money!`）。36个非规范形式中，仅13个（36%）只需 tail 叶，23个（64%）位于 block 中途。bank 生成物42个非规范出现中，26个（62%）也在 block 中途。

<a id="survey--他言語は-closure-内の伝搬演算子の-e-をどう型付けるか"></a>

## 调查 — 其他语言如何为 Closure 内传播运算符的 E 定型

基于实跑 probe（Rust 1.96.1 / Swift 6.3.3 / Go 1.27rc2 / TypeScript 5.9.3）及第一手资料。详情、完整 probe 和原始输出在调查笔记。

| 语言 | closure 内 E | 无约束时 | 何时需注解 | 失败诊断（原文） |
|---|---|---|---|---|
| Swift 6.3.3 | 是否抛错可推断，**类型只取注解或 context** | `any Error`（顶） | 无 context 而需具体 E 时（每次） | `thrown expression type 'any Error' cannot be converted to error type 'E'`（外层 `try`，probe） |
| Rust 1.96 | 从 context 推断（`?` 经 `From`） | 无（E0282） | 无返回类型 context 的 `let` closure | `error[E0282]: type annotations needed … cannot infer type of the type parameter `E`` + `Ok::<i64, E>` 建议（probe） |
| MoonBit | arrow `x => …` **从本体精确推断**（`raise Bad`），`fn` literal 推断已 deprecated | 裸 `raise` = `Error` | arrow 不需要 | `[4122] Function with error can only be used inside a function with error types…`（probe） |
| OCaml 5.5 | `let*` 以 HM 推断（仅同 E），异常/effect 不在类型里 | 类型变量 | 不需要 | `The constant 42 has type int but an expression was expected of type string`（probe） |
| Gleam | `use` 匿名 fn 以 HM 推断（仅同 E） | 类型变量 | 不需要 | `Type mismatch … Expected type: Result(Int, Bool)` |
| Lean 4.29 | lambda monad 来自期望类型或本体 action | 无（metavariable） | 只有 `throw` 且无 context | `typeclass instance problem is stuck MonadExcept String (?m.18 s)`（probe） |
| Koka / Roc / jacquard | open row / open tag union 推断、组合 | open row / union | 几乎不需要 | `effects do not match … inferred effect: <…\|_e>`（Koka） |
| Zig 0.16 | 无 closure，nested fn error set 推断，fn pointer 类型显式 set | 推断 set | fn pointer 实参总需要 | `function type cannot have an inferred error set`（probe） |
| Go 1.27 / TypeScript 5.9 | 类型无 error 通道（`error` / `unknown` 固定） | 顶 | – | `TS2322: Type 'unknown' is not assignable to type 'Bad'`（catch侧，probe） |
| aver / vera / vibe / wado | aver、sui 无 lambda；vera 必须注解 effects；vibe、wado 从本体推断 | – | vera 总需要 | – |

Swift SE-0413 的 Future directions 正写着本 ADR 规则：“closure 的 thrown error type 推断为未捕获 throw site 的 `errorUnion(E1, …, EN)`”。但既有代码依赖 closure `any Error`，所以为 source 兼容放在 upcoming feature flag（`FullTypedThrows`）之后。Revision 6 记录“Closure type inference did not get implemented in Swift 6.0”；编译器测试 `test/expr/closure/typed_throws.swift` 写着“不从本体推断 thrown error type，因为会破坏既有代码”。**Swift 不交付推断不是规则缺陷，而是既有代码类型会改变。** Almide 实测此类既有代码为0（背景暴露表）。

可得四点：

1. **HM/effect 系（OCaml / Haskell / Koka / Lean / Roc / Gleam）从本体推断 closure E**，无需注解。ADR-0012 D4-4 担心的推断错误难题实为 Koka row 变量与 Roc open/closed union，并非 E 推断本身。
2. **从 context 决定 E 的语言（Rust）在无 context 时失败**。Rust E0282 因 `?` 经 `From`，无法从 operand 决定 E。Almide 无 `From`（ADR-0003 D4），故 operand E 直接成为候选，结构上不产生该失败类别。
3. **Swift 处于与 Almide 当时同形的位置，也自认未完成**。无注解时 closure thrown type 降到 `any Error`（顶）。将 typed `try` 放进 closure，从 `throws(E)` fn 调 `try xs.map { ... }`，会在**外层 `try`**报 `thrown expression type 'any Error' cannot be converted to error type 'E'`。与 PR #2714 前 Almide E022 位置、结构相同。2026年的6.3.3本体推断 FullTypedThrows 仍 experimental，production compiler 无法启用。Swift 可用 closure `throws(E)` 注解逃离；Almide 没有 lambda 返回注解，没有退路。
4. **最新 LLM 时代语言倾向本体推断**。MoonBit 精确推断 arrow lambda E（`(String) -> Int raise Bad`），并 deprecated 不推断的 `fn` literal。vibe 和 wado 也从本体推断 closure effect。

<a id="decision"></a>

## 决定

**Lambda 失败通道 E 从固定 String 改为“能确定的类型”：有上下文期望类型先使用它，否则用本体 `!` operand E 的 join。默认及顶为 String。`__fallible_*` 规范形式剥离不再负责定型，降为 lowering 优化。**

<a id="d1-型付け規則l3-の改訂"></a>

### D1. 类型规则（修订 L3）

令 lambda λ 的失败通道为 `Result[T, ε]`，依序决定 ε：

1. **检查模式（期望类型带 E）**：λ 对 `(A) -> B!E` / `(A) -> Result[B, E]` 检查时，ε := E。期望类型有三个来源：用户 HOF 类型化 slot、`let f: (A) -> B!E = ...` 注解、返回 `-> (A) -> B!E` 的 fn 的值 tail。本体每个 `!` 按与 fn 本体完全相同规则（`bang_error_channel`、ADR-0003 D1/D2）匹配 ε。一致则不转换传播；若是 String、Option、非 Result effect call 或另一 E'，在该 `!` 报 E022。
2. **合成模式（期望类型不带 E，内置 `list.*` / `fs.*` HOF、无注解 `let`）**：ε := ⊔ { err(o) | o 是 λ 自身本体的 `!` operand }。err(`Result[_, E]`) = E，err(`Option`) = String（none → err("none")，ADR-0003 D3），err（不返回 Result 的 effect call）= String。⊔ 为现行隐式转换顺序“任意 E ≤ String”的最小上界，**全 operand 同 E 则为 E，任一不一致则为 String**。
3. **默认**：无 `!` operand 的通道（如 effect slot lambda），及离开 λ 时含 E 未解决 operand 的通道，均为 String。两者都属当时行为。

`!` 落入最内层 lambda 通道（#489 / ADR-0009 D5不变）。test block 内 `!` 仍为 unwrap，无通道（L9不变）。

三项性质消除 ADR-0012 D4-4 顾虑：

- **⊔ 全域**。顺序有顶 String，合成模式定型不会失败。无 row 变量或泛化，推断器不增加任何错误消息种类。
- **保留接受范围**。当时通过 check 的 lambda 全部仍通过；仅当所有 `!` 同 E 时 ε 才从 String 变 E。
- **不依赖写法**。花括号、pipe、增加 `let`、移入 if/match 不改变 operand 集合，也不改变 ε。

<a id="d2-観測可能な変化は-1-種類だけ"></a>

### D2. 仅一种可观测变化

传入 ε 从 String 变 E 的 lambda 后，HOF 调用类型由 `Result[_, String]` 变 `Result[_, E]`。后续消费三种：

- `-> T!E` fn 中 `)!`：**接受**（#2601所需修复）。
- `-> T!` fn 中 `)!`：经 fn 层 E→String Debug 化接受，**打印与当时逐字节不变**。只是同 E 值的 Debug 从内层 `!` 移至外层 `!`。
  （2026-09-27实现注记，#2725：转换字符串统一为 repr，即 `"${e}"` 打印，而非 Rust `Debug`。在 `Neg(-2)` / `Bad("x")` / record / 嵌套 variant 中字节相同，但 Option（`Some(2.0)` 对 `some(2)`）、Float（`1e20` 对 `100000000000000000000`）及控制字符转义不同；wasm/interp 只能复现 repr。native 仅这三种打印改变。）
- 值位置消费（如 `match r { err(e) => string.len(e) }`）：仅此改变消费类型。**全部既有语料中0例**，因为背景表中含 variant E `!` 的 lambda 为0。

<a id="d3-診断と-fix-it"></a>

### D3. 诊断和 Fix-It

ε 本身推断不失败（D1），但在四种情况诊断：

1. **检查模式 operand 与 ε 不符**：在该 `!` 报 E022。复用 fn 本体文本/hint（`bang_error_channel`），主语为“callback 错误类型是 `E`，来自 slot `f: (Int) -> Int!E`”。fix-it 用 ADR-0003 D2 按方向的规范形式；若只有一个携带 String 的 ctor，则填入名字，如 `int.parse(s) |> result.map_err((e) => BadValue(e))!`。当时 E005/E001（“传入 fn(Int) -> Result[Int, String]”）不再出现。
2. **合成 ⊔ 降为 String，在 `-> T!E` fn 的 `)!` 触发 E022**：扩展 PR #2714 的 E022，**同时指出两个不一致 operand**：
   ```
   error[E022]: operator '!' cannot propagate this error: the fn's error type is `ShiftErr`,
     but the callback's `!`s fail with different types — `ShiftErr` (line 43, col 20)
     and `String` (line 44, col 31) — so its channel is `String`
     hint: convert the odd one at its `!` so every `!` in the callback fails with `ShiftErr`:
       int.parse(raw) |> result.map_err((e) => BadNumber(e))!
   ```
3. **未解决 operand 导致默认 String**：hint 建议将 callback 绑定到类型化 `let`（`let step_cb: (Acc, Line) -> Acc!ShiftErr = (s, l) => ...`），进入检查模式。不加 lambda 返回类型注解新语法；既有 fn 类型注解承担 Swift `throws(E)` 角色。
4. **check 通过而 rustc 失败的缺口（背景）**：D1 后 ε 与 HOF 结果类型一致，结构上关闭。在 D7 step 1（#2722）先以 E022封堵。

<a id="d4-__fallible_-と正準形剥がしの格下げ"></a>

### D4. 降级 `__fallible_*` 与规范形式剥离

`normalize_fallible_hof_callback` 当时做两件事：(a) 将 `list.map` 等转给 `__fallible_map` 等；(b) 剥离规范标记保留 E。D1 后 (b) 不再需用于定型，因为合成 ε 给出相同 E，`__fallible_*` carrier 已 E-generic。规范 typed fold 已在 native/wasm 同时输出 `ok 3` / `neg -2`，即证据。保留 (a)；对 (b) 用 alloc 台账、size ratchet、perf 做 A/B，无差异就删除，有差异则保留为 lowering 优化。类型上两者相同。

<a id="d5-他-adr-との関係明示の改訂"></a>

### D5. 与其他 ADR 的关系（明确修订）

- **取代 ADR-0012 D4-4。** 撤回“lambda 失败通道保持 String（ADR-0009 L3），使用驱动 E 推断引入 Koka 式错误难题”。依据为 D1 ⊔ 全域且无 row 变量（调查1、2），以及“显式注解 Result”替代路径不存在（背景表末两行）。所引“ADR-0009 L3”不在 ADR-0009；L3 是 `docs/specs/result-option-effect.md` 的 L1–L9 一项（2026-08-07批准），修订的是那里。
- **修订 ADR-0006 D1“E 按 ADR-0002 D2 固定 String”，撤回 D4。** HOF E 从 callback ε 流动，与可失败位同样透明。D4 不支持 E-generic traverse 已被规范路径事实打破。Falsifier 3（自定义 E traverse 实需≥3处）也已触发：3个人工 dojo baseline、14个模型自引 attempt、#2601本体及评论两形。
- **ADR-0009不变。** D5 的 lambda `!` 落入自身通道保留，只决定其 E。D2透明性以同样形态扩展到 E。
- **ADR-0003不变。** lambda 各 `!` 按 fn 本体同样“`!` 不转换 E”规则匹配 ε。但实现有一注：ADR-0003 D2 写 CustomE→String 是 check 错误，实际 #2635（`bang_error_channel.rs` 开头 doc comment）在 fn 层 Debug 化并接受。D1 ⊔ 定义于此**已实现**顺序 E≤String。将来若按字面执行 D2，顺序失去顶，合成不一致会改为 D3-2 E022 + map_err fix-it 拒绝；框架“contextual → 一致 → 默认”仍不变。

<a id="d6-移行影響"></a>

### D6. 迁移影响

- 接受范围缩小：**0**（D1保留）。
- 类型改变的 HOF 调用：全语料 spec / stdlib / tests / examples / exercises / dojo main / almai / comide / golemide / gramide-cli 为 **0**。
- 新接受：#2601四形及评论两形、3个人工 bank baseline、37个失败非规范 typed attempt 的类型错误部分（其他错误另算）、类型化 slot / 类型化 `let` 中带 `!` 的 lambda。
- 契约台账：native 可观测输出不变（D2）；wasm 当时 E082拒绝的形态可运行，因此新建一个 cross-target 契约及 fixture 固定（C-NNN 先在 almide/als 编号，遵循 CLAUDE.md 顺序）。

<a id="d7-実装段階1-段階-1-pr各段階に-issue--すべて-2601-配下"></a>

### D7. 实现阶段（每阶段一个 PR、一个 Issue，均属 #2601）

1. **先封堵缺口**（#2722）：用 `match` 消费 map（check通过→rustc E0277），按当前规则报 E022。让 #2714 `lambda_err_erasures` 不只覆盖 `)!`，也覆盖 HOF 值消费。bug fix 不等 D1。
2. **Checker 合成 ε**（#2723）：`lambda_ret` E 为 fresh，记录 operand E，在 λ 出口取 ⊔（默认 String）；检查模式填期望 E。消除 typed-slot E005/E001拒绝；包括 spec L3修订及 `spec/lang/fallible_lambda_test.almd` pins。
3. **Rust lowering**（#2724）：ε≠String lambda emit 为 `Result<_, E>` closure，`?` 不转换。⊔降到 String 的 lambda 保留当时 Debug `map_err`（`pass_lambda_type_resolve.rs` / `walker/expressions_control.rs`）。验证生成 Rust 无警告编译。
4. **Wasm 两条 leg 与 interp**（#2725）：structural leg `lower_try_unwrap` 的 `unwrap-err-ty-mismatch` wall 通过 ε匹配路径；incumbent与almide-interp也读 ε。本阶段增加3-way oracle fixture及新 C-NNN（als先行）。
5. **诊断**（#2726）：D3-2指出两个不一致 operand，D3-3类型化 `let` hint，`tests/diagnostics/` broken/fixed对。
6. **规范形式剥离 A/B 与降级**（#2727）：用alloc台账、size ratchet、perf比较D4(b)，决定删除或保留为lowering优化。
7. **文档**（#2728）：修订 `docs/specs/result-option-effect.md`（L3和非目标4）；CHEATSHEET / llms.txt callback示例须实现交付后以 `almide check` fence添加（#1483规则）。

<a id="rationale"></a>

## 理由

- **直连 MSR 定义**：当时规则使花括号、pipe、`let`、if等不改含义的编辑改变类型。bank非规范typed 37/37失败，说明模型确实走此编辑路径。D1 ε只依operand集合，不依写法。
- **B不够**：扩展tail叶仅覆盖既有非规范36%、bank非规范typed38%（16/42）。其余block中途形态是模型为修改展开规范形式时**必经**路径（ignore-expired-coupons例）。B只把语法规范扩大一层，写法决定类型的缺陷仍在。
- **C成本低于ADR-0012估计**：担心的推断困难是row/union性质，不存在于两层顺序⊔。迁移影响0例。
- **Swift是反面教材**：同形的无closure推断/默认顶设计，Swift增加注解退路，推断仍experimental；Almide无注解退路，停留同处就无法表达。Swift不交付原因是既有closure类型变化造成source break（调查）；Almide实测0例。D1-2 ⊔是SE-0413 Future directions `errorUnion`在Almide两层顺序E≤String的实例。

<a id="alternatives--検討して却下した案"></a>

## 替代方案 — 考虑后否决

1. **A：现状 + #2714 E022**：诊断改善，但背景7形中6形仍无法写，类型化slot/类型化 `let` 的 `!` lambda连规范形式也不行，ADR-0012 D2承诺落空。bank37/37失败不变。**否决**。
2. **B：规范形式扩展到tail叶**：局部、无需推断，但只覆盖36–38%，仅把写法决定类型后推一层；block中途 `let` 边界仍在最常编辑位置。**否决**。
3. **C-strict：operand不一致总报错，不隐式⊔到String**：符合ADR-0003 D2字面，但fn实现接受E→String时，仅lambda严格会增加 `!` 规则不对称，也缩小接受范围。按D5注记，D2施行时一同迁移。**暂缓**。
4. **新增lambda返回注解语法（Swift `throws(E)`式）**：已有fn类型注解（`let` / slot）足以进入检查模式，新语法一义两写。**否决**。
5. **仅HOF E-generic化，lambda保持String**：carrier已E-generic，缺的只有lambda侧。**不成立**。

<a id="consequences"></a>

## 后果

**收益**：不依写法的callback类型；typed slot可用（完成ADR-0012 D2）；结构性关闭check通过/rustc失败缺口；bank typed-error人工baseline原样可编译；native与wasm接受一致。

**代价**：checker（fresh E、operand记录、λ出口⊔）、Rust closure类型、wasm两leg、interp四处，以及D7七阶段。既有类型改变0例，无手工迁移。

<a id="falsifier"></a>

## 可证伪条件

1. **D7 step2后，spec/dojo出现至少3例“ε变E导致值消费类型错误”**：撤回合成模式，只留检查模式，合成恢复默认String。
2. **λ出口未解决operand降为String，成为dojo typed-error失败语料E022主因**：将⊔延迟到fn级解决之后（改实现，不改规则）。
3. **ADR-0003 D2（CustomE→String check错误）施行**：D1-2“不一致→String”改为“不一致→E022 + map_err fix-it”（转C-strict）。

<a id="references"></a>

## 参考资料

- 调查笔记：`almide-references/RESEARCH-typed-errors-in-closures.md`（完整probe、原始输出、分析器、bank pilot汇总）。
- Swift SE-0413 — [Typed throws](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)
- Rust — [E0282](https://doc.rust-lang.org/error_codes/E0282.html)、[`?` 与 `From`](https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator)
- 内部 — #2601、PR #2714、#2635（`bang_error_channel.rs`）、#2660、`crates/almide-frontend/src/check/infer_calls_closures.rs`（`infer_expr_g3_lambda` / `accept_lambda_channel_prop` / `normalize_fallible_hof_callback`）、`crates/almide-wasm/src/data.rs`（`unwrap-err-ty-mismatch`）、`docs/specs/result-option-effect.md`（L1–L9）、almide-dojo bank pilot run36083949429。
