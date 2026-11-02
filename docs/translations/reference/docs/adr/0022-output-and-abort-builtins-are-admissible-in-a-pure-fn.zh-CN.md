<a id="adr-0022-the-output-and-abort-builtins-are-admissible-in-a-pure-fn--writes-and-aborts-never-reads-and-they-do-not-make-the-caller-effectful"></a>

# ADR-0022：输出与中止内建函数可用于 pure fn — 只写入或中止，从不读取，也不会使调用者变为 effectful

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。以正本为准，本资料不自动同步。以下 Accepted 状态、测量与实现主张均为原文记录，并非独立的当前验证结果。未执行所述命令。

- **状态**：Accepted（记录现有行为；不改变检查器或代码生成）
- **日期**：2026-09-27
- **范围**：`println`、`eprintln`、`panic`、`assert`、`assert_eq` 和 `assert_ne` 是否可以在普通 `fn` 中调用；这种调用对调用者的 effect、传播、`fan`、服务处理函数、参考求值器以及 native 与 wasm 输出缓冲意味着什么。
- **相关**：[effect-system.md §2.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-system.md)（规范规则）、[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（effect 轴）、[ADR-0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)（仅通过 `!` 传播）、[ADR-0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md) D1（fan 分支输出）、[ADR-0015](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md)（参考求值器）、[ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §5.5 / §7.2（并发服务与日志行）、契约 C-004、C-153、C-162、C-367。
- **历史**：2026-09-27。这项规则从未写下来，因此文档同时教授 `main` 的两种写法（`effect fn main() -> Unit = println(…)` 和 `fn main() -> Unit = println(…)`），读者无法判断哪种是必需的。向用户提供了三种选择：A（允许 pure fn 使用输出与中止内建函数，并写明规则）、B（要求它们使用 `effect`）、C（只允许中止）。用户选择了 **A**。

<a id="context"></a>

## 背景

<a id="what-the-checker-does-today-almide-0640-measured"></a>

### 检查器目前的行为（almide 0.64.0，已测量）

- `check_named_call_with_type_args` 在查找签名前先解析内建函数（`crates/almide-frontend/src/check/calls.rs:437-440`）。内建函数分支（`check_builtin_output`，`crates/almide-frontend/src/check/builtin_calls.rs:62`）约束参数类型，并返回 `Unit`（`println`、`eprintln`、assert 系列）或 `Never`（`panic`）。它从不读取 `can_call_effect`，所以这些名称不会到达 `calls.rs:514` 的 E006 检查（`sig.is_effect && !self.env.can_call_effect`）。
- 因而 `fn show(p: Int) -> Unit = println("p=${p}")` 可以通过检查并运行，`fn main() -> Unit = println(…)` 也一样。在同一位置使用 `fs.read_text` 会触发 E006，`io.print` 也是如此：`io.print` 是标准库的 *effect fn*，而非内建函数。
- 语料库已经依赖这一点。`spec/`、`stdlib/` 和 `research/benchmark/exercises/` 中有 535 个文件带有 pure 的 `fn main`，386 个带有 `effect fn main`。在 `spec/wasm_cross/` 的 792 个 fixture 中，有 486 个使用 pure 的 `fn main`。
- `crates/almide-frontend/src/stdlib.rs:305` 仍有一个 `builtin_effect_fns()` 函数，返回 `["println", "eprintln", "panic"]`。其文档注释写着“内建 effect 函数”。没有任何地方调用它，该名称与本 ADR 矛盾。

<a id="survey-how-peers-treat-print-and-abort-in-pure-code"></a>

### 调查：其他语言如何处理纯代码中的输出和中止

来源均为一手资料：`almide-references/` 下的本地克隆（Koka、Roc、Gleam、Swift、Rust）以及官方语言文档（Go、Haskell）。

| 语言 | 是否在类型中跟踪 effect？ | 纯代码中的实际输出 | 调试输出例外 | 纯代码中的中止 | 一手来源 |
|---|---|---|---|---|---|
| Rust | 否 | 是：任何位置都可 `println!` | `dbg!`（stderr，返回其值） | 是：`panic!` | `library/std/src/macros.rs`（`println` :143、`eprintln` :221、`dbg` :358；:296 的 `dbg!` 文档示例是普通 `fn factorial`） |
| Go | 否 | 是：任何位置都可 `fmt.Println` 或内建 `println` | 内建 `print`/`println` 用于自举：“不保证一直保留在语言中” | 是：`panic` | go.dev/ref/spec §Bootstrapping |
| Haskell | 是（`IO`） | 否：`putStrLn :: String -> IO ()` | `Debug.Trace.trace :: String -> a -> a`，纯类型，“*不应*用于生产代码” | 是：`error :: HasCallStack => [Char] -> a` 是纯 bottom | hackage `base` Prelude、`Debug.Trace` |
| Koka | 是（effect row） | 否：`println(s) : console ()` | `trace(message) : ()` 是 total（通过 `unsafe-total`）；`notrace()` 可将其关闭 | 跟踪 `throw : exn a`。`assert` 和 `impossible` 被赋予 total 类型（通过 `unsafe-abort`） | `lib/std/core/console.kk:78`、`debug.kk:58-60, 81, 84`、`exn.kk:49`、`core.kk:55, 79` |
| Gleam | 否 | 是：任何位置都可 `io.println(string) -> Nil` | `echo`（1.9）将模块和行号一起输出到 stderr；`gleam publish` 拒绝仍含有 `echo` 的包 | 是：`panic`、`todo`、`assert` | `changelog/v1.9.md:32, 99`、`compiler-cli/src/publish.rs:489-508`、标准库 `gleam/io.gleam:42` |
| Roc | 是（`->` 为 pure，`=>` 为 effectful，通过推断） | 否：“effectful 函数只能由其他 effectful 函数调用” | pure 函数中允许 `dbg` 和 `expect`：“不是程序语义的一部分，因此允许在 effectful 函数以外产生该副作用” | 是：`crash`；pure 函数“不保证是 *total*” | `docs/langref/functions.md:25-42`、`expressions.md:113-117`、`statements.md:138-151` |
| Swift | 否（仅 `throws`/`async`） | 是：任何位置都可 `print(_:separator:terminator:)` | 没有单独的例外（`print`、`debugPrint`） | 是：`fatalError(…) -> Never`、`precondition`、`assert` | `stdlib/public/core/Print.swift:54`、`Assert.swift:275-278` |

表格说明：

1. **每种语言都允许纯代码中止。** 跟踪 effect 的三种语言也如此：Haskell 的 `error` 是纯 bottom，Roc 的 `crash` 可以出现在纯代码中，Koka 将 `assert` 和 `impossible` 赋予 total 类型。Koka 只跟踪可恢复的 `throw`（`exn`）。中止终结程序的观测，而不增加新的观测，所以这些语言都不将它视为 I/O。
2. **每种跟踪 effect 的语言都豁免调试输出**（`trace`、`dbg`），理由相同：输出面向程序员，不属于程序语义。这些豁免仅允许写入，没有任何一种允许纯代码*读取*任何东西。
3. **不跟踪 effect 的语言不限制输出。**

Almide 处于这两类之间。`println` 是真正的程序输出，而非跟踪通道，因此 Almide 的豁免比 Haskell 的 `trace` 或 Roc 的 `dbg` 更宽。但读取（`fs`、`env`、`io.read_line`、`http`、时钟、`random`）仍受 effect 限制，不跟踪 effect 的语言则不这样做。

<a id="decision"></a>

## 决定

**`println`、`eprintln`、`panic`、`assert`、`assert_eq` 和 `assert_ne` 可用于 pure 的 `fn`。它们写入 stdout 或 stderr，或中止程序，从不读取。调用它们不会使调用者变为 effectful，也不会产生需要传播的内容。** 规范文本是 [effect-system.md §2.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-system.md)。本 ADR 记录原因。

<a id="d1-the-exempt-set-is-closed-and-names-builtins-not-modules"></a>

### D1. 豁免集合是封闭的，指的是内建函数而非模块

以上六个名称就是完整集合。`io.print`、`io.write`、`io.write_bytes` 和 `log` 模块属于标准库 effect fn，在 pure fn 中仍触发 E006。向该集合添加名称必须修订本 ADR。候选必须是只写入或中止、返回 `Unit` 或 `Never` 的内建函数。

<a id="d2-no-effect-no-propagation"></a>

### D2. 无 effect，也无传播

内建函数返回 `Unit`（`println`、`eprintln`、assert 系列）或 `Never`（`panic`），从不返回 `Result`。没有可供 `!` 解包的内容：`println(s)!` 在任何上下文中都是 E034。调用它们的 pure fn 在签名、模块接口和所有调用点都保持 pure。effect fn 将其作为普通值调用，不带 `!`。

<a id="d3-reads-remain-the-boundary"></a>

### D3. 读取仍是边界

pure fn 仍不能观测外界。被豁免的内建函数都不会返回程序外的信息。因此 pure fn 的*结果*仍是其参数的函数。这项豁免增加的是*观测*：stdout 或 stderr 上的字节，以及可能终止的进程。

<a id="d4-interactions-each-is-stated-normatively-in-effect-systemmd-21"></a>

### D4. 相互作用（均在 effect-system.md §2.1 中以规范形式陈述）

- **服务处理函数（ADR-0020）。** 通过 pure 辅助函数输出的处理函数遵循与直接输出相同的规则：每次调用的字节连续到达对应流（一行是原子的），同一请求的行保持程序顺序，不同处理中请求的行顺序未规定。
- **`fan` 函数体。** 从 `fan` 分支调用的 pure fn 作为该分支的一部分输出。这项豁免不增加超出 fan 契约的顺序承诺（C-004；native 上 `fan.settle` 的 thunk 副作用交错不在契约范围内）。`fan` 自身仍要求 effect 上下文（E007）；ADR-0011 D1（按分支顺序输出）一旦实现，将像其他字节一样覆盖这些字节。
- **参考求值器。** 两个求值器对这些名称都没有 effect 上下文。进程内解释器（`almide-interp`）按名称分派（`crates/almide-interp/src/dispatch.rs:393`）。judge 的参考求值器（ADR-0015）将它们列在 `PRELUDE` 中（almide/als `ref/src/eval.rs:217-224`），该列表正好是 D1 中的六个名称。所以三方 oracle 对 pure fn 与 effect fn 的输出采用相同方式比较。只有检查器强制区分 pure/effect。
- **缓冲。** pure fn 的输出经过与 effect fn 相同的流。native stdout 只有一个缓冲区：stdout 是终端时每次写入都刷新，否则使用 64 KiB 缓冲；stderr 不缓冲（C-162，以及 C-367 的陈述）。契约按流分别成立：stdout 字节、stderr 字节和退出码在 native 与 wasm 之间逐字节一致。两个流都写入同一文件时如何交错不属于契约（测量结果：将 `println; eprintln; println` 用 `2>&1` 合并，native 得到 `o1 e1 o2`，wasm 得到 `e1 o1 o2`）。中止先刷新此前写入的 stdout，再写入自己的 stderr 区块（C-153）。
- **`scoped`。** 不变。`scoped fn` 比 pure fn 更严格，其内部的 `println` 仍是 E087。`@bounded` 允许输出内建函数，正如 C-316 已有的规定。

<a id="d5-which-main-the-docs-teach"></a>

### D5. 文档教授哪种 `main`

只要 `main` 不执行实际 effect，普通的 `fn main() -> Unit` 就是正确的。只有当 `main` 读取、写文件、使用网络、使用 `fan` 或通过 `!` 传播时，才写 `effect fn main()`。面向 LLM 的文档（`docs/CHEATSHEET.md`、`llms.txt`）采用这种教法，并用一句话说明。

<a id="rationale"></a>

## 理由

- **语言本来就如此。** 方案 A 不改变任何行为，语料库中 535 个文件依赖这一行为。B 会将它们全部变成 E006，并给除输出外不执行 effect 的程序增加一个 `effect` 关键字。增加不携带信息的关键字会使程序更难修改，不利于 modification survival。
- **中止在所有语言中都被豁免（调查第 1 点）。** 在 pure fn 中拒绝 `assert` 会让 Almide 成为表格中唯一这样做的语言，还会阻碍“在 pure 辅助函数中检查不变量”这一惯用模式。
- **纯代码给调用者的保证与输入有关，而非输出。** 在 A 下，“pure fn 的结果是其参数的函数”仍成立（D3）。检查器的真正价值在于限制读取与宿主访问，所有可触达宿主的模块仍受限制。
- **纯代码中的调试输出具有普遍性（调查第 2 点）。** 表格中每种跟踪 effect 的语言都不得不为此增加逃生口。Almide 不需要第二个只用于跟踪的内建函数：`eprintln` 已经写入 stderr，不会混入 stdout。

<a id="alternatives-rejected"></a>

## 被否决的替代方案

- **B：输出要求 `effect`**（Haskell 和 Koka 的 `console`）。纸面上更严格，但会破坏 535 个语料文件以及每个带 pure `main` 的文档示例。选择这一方式的 effect 跟踪语言仍必须添加跟踪豁免，结果相当于方案 A 再加一个内建函数。
- **C：仅豁免中止；输出要求 `effect`。** 对输出而言成本与 B 相同，还移除了调试 pure 辅助函数最常用的方式。
- **用只用于跟踪的内建函数（`dbg`/`trace`），而不是允许 `println`。** 这会增加第二种输出写法供 LLM 选择，而且所有既有的 pure `main` 仍需要 `effect`。Gleam 仅在发布时强制限制调试输出，说明此类规则属于工具层面，而非类型系统。

<a id="consequences"></a>

## 后果

- 规则现已写明并固定。检查测试为 `tests/checker_test.rs`（`pure_fn_admits_output_and_abort_builtins`、`pure_fn_still_rejects_a_real_read_with_e006`）。运行 fixture 为 `spec/lang/pure_fn_output_builtins_test.almd` 和 `spec/wasm_cross/pure_fn_output_and_assert_abort.almd`；后者是 C-153 的证据，必须在 native 与 wasm 上逐字节一致。
- 文档只教授一条 `main` 规则（D5）。
- 成本：pure fn 的调用次数和调用顺序可以通过输出观测。因此，优化器不能对可能输出或中止的 pure 调用进行去重、提升或重排。pure fn 因而不能任意记忆化。这本来就成立，因为 `panic` 可以在任何 pure fn 中运行。

<a id="falsifier"></a>

## 反证条件

如果观察到以下任一情况，则撤回本决定（改用 B 或只用于跟踪的内建函数）：

1. 在性能语料上测得，pure/effect 分离原本应允许的优化（pure 调用的 CSE、提升或记忆化）实际上被输出内建函数阻碍。
2. Dojo 测量表明，LLM 编写的程序在 pure 辅助函数中进行输出，其方式破坏了 modification survival（例如 pure 调用移动时输出顺序发生变化）。仅仅“更严格会更符合原则”并不是反证条件。

<a id="references"></a>

## 参考资料

- Rust：`library/std/src/macros.rs`（克隆位于 `almide-references/rust`）
- Go：<https://go.dev/ref/spec#Bootstrapping>
- Haskell：<https://hackage.haskell.org/package/base/docs/Debug-Trace.html>、<https://hackage.haskell.org/package/base/docs/Prelude.html>
- Koka：`lib/std/core/{console,debug,exn,core}.kk`（克隆位于 `almide-references/koka`）
- Gleam：`changelog/v1.9.md`、`compiler-cli/src/publish.rs`（克隆位于 `almide-references/gleam`）
- Roc：`docs/langref/{functions,expressions,statements}.md`（克隆位于 `almide-references/roc`）
- Swift：`stdlib/public/core/{Print,Assert}.swift`（克隆位于 `almide-references/swift`）

---

保留历史上的语言比较与已记录语料数量，未独立重新确认。标题不承诺没有可观测作用或可任意记忆化；原文明示允许输出／中止并限制优化。参考求值器描述及分支顺序的将来表述按原文保留。
