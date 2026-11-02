<a id="l2l3-error"></a>

# L2/L3：error

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-error.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-error.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-error.zh-CN.md)

> 非规范固定原文读者快照。未改动原文仍为读取方的权威来源。不自动同步，也未独立执行。历史主张、已采纳提案、实现状态及测量证据保留原范围；本翻译不独立验证。内联代码保持原样，包括原语言字符串。 10月21日的弃用／移除计划、epoch、使用量、probe结果及外部比较是原文时点记录，不是当前API清单。保留chain分隔符语义改变、message无法区分ok与空错误，以及result.context否决与基于证据的重审条件。这些读者文件不移除原API。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md)

> 原metadata非活动注记：stdlib重设计L2/L3——error。退役context、message、chain；它们各为ADR-0004已固定的map_err/match惯用法的第二种写法，从而清空模块。

> 最后更新：2026-10-21
> 前提：[L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)、[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)、[L2-result](../../../../../roadmap/stdlib-redesign/L2-result.md)、ADR-0003 / ADR-0004 / ADR-0012
> 来源：Rust https://doc.rust-lang.org/std/error/trait.Error.html、Go https://pkg.go.dev/errors、Python https://docs.python.org/3/library/exceptions.html、TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Error/cause、Gleam https://gleam-stdlib.hexdocs.pm/gleam/result.html (F)、Elixir https://hexdocs.pm/elixir/Exception.html、OCaml https://ocaml.org/manual/latest/api/Result.html (F)、Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-result/ (F)、Swift https://developer.apple.com/documentation/swift/error

`error` 是含 3 个函数的模块，均处理“err 字符串”。Almide 错误默认为 String（ADR-0002 D2、ADR-0004）；ADR-0004 D2 已将 `result.map_err((e) => "文脈: ${e}")` 确定为添加上下文前缀的规范形式。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 为 err 前置上下文 | `error.context` / `result.map_err` | `map_err`（std）、`context`（anyhow，非标准） | `fmt.Errorf("..: %w", err)` | `raise X from e` | `new Error(msg, { cause })` | `map_error` (F) | — | `map_error` (F) | `recoverCatching` / 异常的 `cause` | — |
| 将 2 个错误“带原因”连接 | `error.chain` | `source()` | `%w` / `errors.Join` | `__cause__` | `cause` | — | — | — | `cause` | — |
| 取出 err 字符串 | `error.message` | `to_string()` | `err.Error()` | `str(e)` | `e.message` | — | `Exception.message` | — | `message` | `localizedDescription` |
| 追溯原因 | — | `source()` | `errors.Unwrap` / `Is` / `As` | `__cause__` | `cause` | — | — | — | `cause` | — |

<a id="完全性の規則"></a>

### 完整性规则

- 前置上下文只有一种写法：`result.map_err((e) => "文脈: ${e}")`（分隔符为 `": "`，原错误放在末尾）。ADR-0004 D2 和 Alternatives 7 将语法糖 `result.context` 作为“同一件事的第二种写法”否决。
- 不提供按错误内容分支的工具（ADR-0004 D4），因此有意不具备“追溯原因”角色（如 Go 的 `errors.Is`）。

<a id="l2-の判定"></a>

### L2 裁定

- 3 项重复：`error.context` 是 `result.map_err` 的固定形式，`error.message` 等于 `match` 或 `result.to_err_option(r) ?? ""`，`error.chain` 等同字符串插值。此外 `error.chain` 的分隔符（`"\ncaused by: "`）与规范形式的 `": "` 不同（在 probe-error/p.almd 中确认）。
- 结论：移除 3 个函数，关闭模块（R-error-01）。L1 表也作出相同裁定：“移除 `error` 的 3 个函数，模块变空。”

<a id="案"></a>

## 提案

<a id="r-error-01-errorcontexterrormessageerrorchain-を撤去しerror-モジュールを閉じる---採用"></a>

### R-error-01 移除 `error.context`、`error.message`、`error.chain`，关闭 `error` 模块——通过

- 内容：
  - `error.context(r, msg)` → `@deprecated(since=15, use="result.map_err")`。替换为 `r |> result.map_err((e) => "${msg}: ${e}")`，行为相同（probe：`error.context(err("boom"), "loading")` 和 `map_err` 版本均为 `err("loading: boom")`）。
  - `error.message(r)` → `@deprecated(since=15, use="result.to_err_option")`。替换为 `result.to_err_option(r) ?? ""`。若需区分 ok 与 `err("")`，则建议 `match`。当前 `error.message` 对 ok 和 `err("")` 均返回 `""`，无法区分（已由 probe 确认）。
  - `error.chain(outer, cause)` → 移除。替换为 `"${outer}: ${cause}"`。若确需 `"\ncaused by: "` 分隔符，直接写该字符串。由于替代项不是函数，在 `use=` 中写入写法本身：`@deprecated(since=15, use="\"${outer}: ${cause}\"")`；移除后通过 `removed_spellings.rs` hint 展示相同形式。`almide fix` 将 `error.chain(a, b)` 改为 `"${a}: ${b}"`（分隔符会改变，因此 fix 列出改写位置）。
  - 3 项都用 `[[deprecation]]` 块（`removal_epoch = 16`），移除后 E002 附 hint。移除后取消 `stdlib/error.almd`、`error_chain.almd`、`error_message.almd`、`runtime/rs/src/error.rs` 和 `STDLIB_MODULES` 注册。
- 依据的 L0 规则：R-L0-01（同一角色只有一种写法；仅调用其他公开函数 1 次的转发也算相同角色）、R-L0-02（以 Result 为主体的函数放在 result）、R-L0-14。
- (a) 9 种语言中，没有把“前置上下文”作为标准专用函数的。Rust `context` 属于外部 anyhow crate，标准是 `map_err`；Go 使用 `fmt.Errorf` 格式，JS 使用 `cause` 参数，均通过通用工具表达。
- (b) 无影响。替代项全是纯计算，两个 target 已有。
- (c) 同时存在 `error.context` 与 `result.map_err`，写作者就要记住两者的上下文分隔符（`": "`）及 ok 时行为。`error.chain` 连分隔符都不同。`error.message` 不区分 ok 和空 err，因此写 `if error.message(r) == "" then ..` 会误判 ok。ADR-0004 D3-(b) 的 lint（漏写 `${e}`，#1105）通过诊断阻止 `map_err` 形式的风险。
- (d) 移除。使用次数（含定义和生成 doc 行）：`error.context` spec 4 / tests 17 / docs 11 / stdlib 0；`error.message` spec 5 / tests 22 / docs 10 / stdlib 1；`error.chain` spec 4 / tests 2 / docs 5 / stdlib 1。CHEATSHEET 和 `llms.txt` 中未出现。`almide fix` 改写 `error.context(r, m)` → `r |> result.map_err((e) => "${m}: ${e}")`，以及 `error.message(r)` → `result.to_err_option(r) ?? ""`。
- gate：移除后 E002 hint 的 fixture（3 项）。R-L0-01 同义语 gate（扩展 #3851 F）也检测仅调用其他公开函数 1 次的函数，因此若移除后同样转发形式回归，就会失败。
- issue：#3909

<a id="r-error-02-errorcontext-を-resultcontext-として残す---不採用"></a>

### R-error-02 将 `error.context` 作为 `result.context` 保留——否决

- 内容：不采纳只修正位置（R-L0-02）、改为 `result.context` 的方案。
- 依据的 L0 规则：R-L0-01。ADR-0004 Alternatives 7 已因此否决 `result.context`（#1104 以否决关闭）。
- (c) 重新考虑条件是 ADR-0004 Falsifier 3（即使有 lint，Dojo 仍测得频繁漏写 `${e}`）。尚无该证据。
- issue：无

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要写法 | 裁定 | 提案 |
|---|---|---|---|---|
| `error.chain` | `chain(outer: String, cause: String) -> String` | Go `fmt.Errorf("%w")` / JS `cause` / Python `from` | 移除 | R-error-01 |
| `error.context` | `context(r: Result[T, String], msg: String) -> Result[T, String]` | Rust map_err（anyhow context 非标准）/ Gleam、OCaml map_error | 移除 | R-error-01、R-error-02 |
| `error.message` | `message(r: Result[T, String]) -> String` | Go `Error()` / JS `message` / Kotlin `message`（都位于异常／错误值一侧） | 移除 | R-error-01 |

（3 个函数，无新增。）

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0（已处理）

- **退回 L1**：L1 表已改为“`error` 的 3 个函数是重复或转发，因此移除，模块变空”，与本页一致。
- 转发的处理：R-L0-01 已加入“仅调用其他公开函数 1 次的函数（转发）也算相同角色”，以 `error.context` 为例。同义语 gate 也有检测仅调用其他公开函数 1 次的规则。
