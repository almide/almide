说明：stdlib 重设计 L2/L3 — option 在共同窗口撤去 eager unwrap_or，保留 collect，以一个矩阵门禁覆盖 option/result 家族。
<a id="l2l3-option"></a>

# L2/L3: option

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-option.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-option.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-option.zh-CN.md)

> 非规范性带日期重设计参照版，原本权威且不变。不自动同步、无新probe、未核实现行API。撤去窗口和使用数是历史资料。eager unwrap_or转lazy ??是迁移，不保证默认表达式求值相同。区分保留Option collect与撤去Result collect，以及当时仅Result适用的可谬map。旧ADR E=String与probe多态E差异不暗中统一。14函数表包含拟撤去函数。跨语言分类为原文当时主张，非新比较。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)、共同判定 [L2-result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md)。
> 来源：Rust https://doc.rust-lang.org/std/option/enum.Option.html (F)；Go —（无 Option 类型，comma-ok https://go.dev/ref/spec#Index_expressions）；Python —（只有 `None` 与类型注释，https://docs.python.org/3/library/typing.html#typing.Optional）；TS/JS —（`?.` 与 `??`，https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Nullish_coalescing）；Gleam https://gleam-stdlib.hexdocs.pm/gleam/option.html (F)；Elixir —（`nil` 与 `||`，https://hexdocs.pm/elixir/Kernel.html）；OCaml https://ocaml.org/manual/latest/api/Option.html (F)；Kotlin https://kotlinlang.org/docs/null-safety.html；Swift https://developer.apple.com/documentation/swift/optional。

读法：`??`（空值合并）在左侧 none 时使用右侧；`?.`（可选链）只在 some 时进入内部值；`!`（传播）在 effect fn 中原样返回失败。ADR-0005 已决定，它们都脱糖（改写成另一种写法）为 stdlib 函数。

9 种语言中 5 种把 Option 作为类型（Rust、Gleam、OCaml、Kotlin 的 `T?`、Swift 的 `T?`）。Go、Python、JS、Elixir 用 `nil`/`None`/`undefined` 表示无值，并用运算符或语法而非函数处理。所以本表“至少 5/9”基本意味着“具备 Option 的 5 种语言全部”。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 转换内部值 | `option.map` / `?.` | `map` (F) | — | — | `?.` | `map` (F) | — | `map` (F) | `?.let {}` | `map` |
| 串联转换 | `option.flat_map` | `and_then` (F) | — | — | `?.` | `then` (F) | — | `bind` (F) | `?.let {}` | `flatMap` |
| 展平嵌套 | `option.flatten` | `flatten` (F) | — | — | — | `flatten` (F) | — | `join` (F) | — | — |
| 默认值（提前求值） | `option.unwrap_or`（已弃用） | `unwrap_or` (F) | — | `x if x is not None else d` | `??` | `unwrap` (F) | `\|\|` | `value ~default` (F) | `?:` | `??` |
| 默认值（按需求值） | `??` = `option.unwrap_or_else` | `unwrap_or_else` (F) | — | — | `??` | `lazy_unwrap` (F) | `\|\|` | — | `?:` | `??` |
| 判断 some/none | `option.is_some` / `is_none` | `is_some` / `is_none` (F) | — | `is not None` | `!== undefined` | `is_some` / `is_none` (F) | `!= nil` | `is_some` / `is_none` (F) | `!= null` | `!= nil` |
| 转换为 Result | `option.to_result` | `ok_or` (F) | — | — | — | `to_result` (F) | — | `to_result ~none` (F) | — | — |
| 按条件筛选 | `option.filter` | `filter` (F) | — | — | — | — | — | — | `takeIf {}` | — |
| 两值配对 | `option.zip` | `zip` (F) | — | — | — | — | — | — | — | — |
| none 时用另一 Option | `option.or_else` | `or_else` (F) | — | — | — | `lazy_or` (F) | — | — | `?:` | `??` |
| 转换为列表 | `option.to_list` | `iter` | — | — | — | — | — | `to_list` (F) | `listOfNotNull` | — |
| List[Option] → Option[List]（首个 none 时停止） | `option.collect` | `collect` | — | — | — | `all` (F) | — | — | — | — |
| 转换同时执行上项 | `option.collect_map` | `map(..).collect()` | — | — | — | — | — | — | — | — |
| 取内部值，none 时停止 | `?? panic(msg)` | `expect` / `unwrap` (F) | — | — | — | `let assert` | — | `get` (F) | `!!` | `!` |
| 从 List[Option] 仅收集 some | `list.filter_map((x) => x)` | `flatten` | — | — | — | `values` (F) | — | `List.filter_map Fun.id` | `filterNotNull` | `compactMap` |
| 查看内部值（副作用） | — | `inspect` (F) | — | — | — | — | — | `iter` (F) | `also` | — |
| none 时用另一 Option（提前求值） | — | `or` (F) | — | — | — | `or` (F) | — | — | `?:` | `??` |
| 拆开配对 | — | `unzip` (F) | — | — | — | — | — | — | — | — |
| 条件判断 | — | `is_some_and` (F) | — | — | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### 完整性规则

- option 与 result 作为一个 family（同形函数的家族）。两边都需要：`map`、`flat_map`、`flatten`、`unwrap_or_else`、两个 `is_<状態>`、`to_<相手の型>`、`filter`、`zip`、`or_else`、`to_list`。占位词分别指状态与对方类型。
- 有意只在一边存在：
  - `collect` / `collect_map` 仅 option。result 侧由 ADR-0007 D1 撤去；全错误收集交给 `result.partition`，遇首错停止交给 `list.map(xs, (x) => f(x)!)!`（ADR-0006）。option 与 Rust `collect` 语义一致，所以 ADR-0007 D4 决定保留。
  - `map_err`、`to_err_option`、`partition` 仅 result（none 不含内部值，无对应项）。
- 默认值仅用 `??`（R-L0-06），不保留提前求值的 `unwrap_or`。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`option.unwrap_or` 与 `??` 同角色。在 #1107 窗口撤去（R-option-01）。
- 缺失且达到 5/9 的角色：“取出，none 时停止”（5 种）与“从 List[Option] 只收集 some”（5 种）。已有写法 `?? panic(msg)`、`list.filter_map((x) => x)`，所以不增加函数（R-option-03）。
- 家族空缺：无。把 option/result 对应列成表并以门禁守护（R-option-04）。
- 位置：`option.collect` / `collect_map` 第一参数是 List，仅按 R-L0-02 主语规则应放 list；但符合该规则的例外：“含义取决于容器元素类型的函数，放在元素类型模块”，所以放 option 正合规则（R-option-02）。

<a id="案"></a>

## 提案

<a id="r-option-01-optionunwrap_or-を-epoch-16-で撤去する---採用"></a>

### R-option-01 在 epoch 16 撤去 `option.unwrap_or` — ○ 采纳

- 内容：`option.unwrap_or(o, d)` 仍标记 `@deprecated(since=3, use="??")`，无撤去日期。在 `proofs/dialect-epochs.toml` 加入 `[[deprecation]]` 块（`since_epoch = 3`、`removal_epoch = 16`），与 `list.get_or`、`map.get_or`、`result.unwrap_or` 同窗口撤去。撤去后 E002 提示“写 `o ?? d`；需要函数形式时用 `option.unwrap_or_else`”（ADR-0005 D4 文案）。
- L0 依据：R-L0-06、R-L0-14。
- (a) Rust、Gleam、OCaml 有提前求值默认值函数。但 Almide 中 `??` 是唯一写法，仅在需要时求值，与 Rust `unwrap_or_else`、Swift/TS `??` 相同。
- (b) 无影响。`??` 已在两 target 脱糖到 `unwrap_or_else`（C-291）。
- (c) 仅剩 `??` 后，编写者无需关心是否提前求值。
- (d) 撤去。`almide fix` 把 `option.unwrap_or(o, d)` 改为 `o ?? d`（PR #1902 已实现）。使用数：spec 100 / tests 15 / docs 30 / stdlib 2（含定义）、research 22。#1107 记录 `.almd` 内 128 处。
- 门禁：撤去后的 E002 提示 fixture（`tests/diagnostics/`）。用 `check-interface-diff.sh --allow-breaking` 作为已声明 breaking 通过。
- issue：#1107（已有）。

<a id="r-option-02-optioncollect-と-optioncollect_map-を残し置き場所の例外として登録する---採用据え置き"></a>

### R-option-02 保留 `option.collect` 和 `option.collect_map`，登记位置例外 — ○ 采纳（不变）

- 内容：名称与位置不变。`option.collect`、`option.collect_map`、`result.partition` 符合 R-L0-02 例外：“含义取决于容器元素类型的函数，放在元素类型模块”（L0 本文以 `option.collect`、`result.partition` 为例）。`collect_map` 由回调返回的 `Option` 决定结果类型，也属同一例外。
- L0 依据：R-L0-02、R-L0-00。
- (a) Rust 用 `collect` 收集至 Option，Gleam 把 `option.all` 放 option 模块；没有语言放在 List 侧。
- (b) 纯计算，两 target 相同。
- (c) 移至 `list.collect` 会把 Option 专用函数混入 `list`；还在另一位置恢复 ADR-0007 已删 `result.collect` 的名称，增加与 Rust `collect` 混写的机会。
- (d) 不变。确认：`option.collect_map` 替代 `list.map((x) => f(x)!)` 的 Option 版。当时可谬多态的 `list.map` 仅适用于 Result（probe-option/p.almd 确认 E005）。
- 门禁：R-L0-G 位置检查除第一参数类型外，也接受“第一参数元素类型，或返回类型外层类型”为模块主语类型的情况，不用名称允许列表。
- issue：#3949。

<a id="r-option-03-抜けている役割の関数を足さない---不採用"></a>

### R-option-03 不增加缺失角色函数 — × 不采纳

- 内容：不增加 Rust/Kotlin 等的 `expect`、`inspect`、提前求值 `or`、`unzip`、`is_some_and`、`values`。
- L0 依据：R-L0-00、R-L0-01、R-L0-06。
- (a) 达到 5/9 的仅“取出，none 时停止”和“只收集 some”。前者用 `o ?? panic("msg")`（probe-option/q.almd 确认 `panic` 可按类型放在 `??` 右侧），后者用 `list.filter_map((x) => x)`。`inspect`、`or`、`unzip`、`is_some_and` 不超过 4 种语言。
- (c) 增加 `or` 会同时出现提前求值 `or` 与按需求值 `or_else`，重造 ADR-0005 通过 `unwrap_or` 消除的双写法。`?? some(9)` 是类型错误 E001，会引导到 `option.or_else`。
- (d) 无。
- 门禁：不加函数，改为在 CHEATSHEET “Unwrap operators” 加一个 `?? panic(msg)` 示例，放入 ```` ```almide check ```` fence，用 `scripts/check-llm-surface.sh` 检查。
- issue：#3905（仅 CHEATSHEET 一行）。

<a id="r-option-04-option-と-result-の対応を行列のゲートにする---採用"></a>

### R-option-04 用矩阵门禁检查 option/result 对应 — ○ 采纳

- 内容：把上述完整性规则做成 `tests/stdlib_surface_conventions_test.rs` 一条规则。检查 option 家族单元在 result 也存在，反向亦然；有意单侧单元（option：`collect`、`collect_map`；result：`map_err`、`to_err_option`、`partition`）放入附理由的允许列表。
- L0 依据：R-L0-01、R-L0-G、CLAUDE.md“按矩阵扩展 API 家族”。
- (a) Rust、Gleam、OCaml 的 Option/Result 具有名称基本相同的函数组。
- (b) 无影响。
- (c) 编写者从一侧猜另一侧名称（有 `option.zip` 就应有 `result.zip`）。当时一致，但无门禁会失衡。
- (d) 不变，仅增加门禁。
- issue：#3949。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当时，显示 mut） | 其他语言主要名称 | 判定 | 提案 |
|---|---|---|---|---|
| `option.map` | `map(o: Option[A], f: (A) -> B) -> Option[B]` | Rust map / Gleam map / OCaml map / Swift map / Kotlin `?.let` | 保持 | — |
| `option.flat_map` | `flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]` | Rust and_then / Gleam then / OCaml bind / Swift flatMap | 保持 | R-L0-10（`flat_map` 的名称由规则固定） |
| `option.flatten` | `flatten(o: Option[Option[A]]) -> Option[A]` | Rust flatten / Gleam flatten / OCaml join | 保持 | — |
| `option.unwrap_or` | `unwrap_or(o: Option[A], default: A) -> A`（deprecated） | Rust unwrap_or / Gleam unwrap / OCaml value / 多数： `??` 运算符 | 撤去 | R-option-01 |
| `option.unwrap_or_else` | `unwrap_or_else(o: Option[A], f: () -> A) -> A` | Rust unwrap_or_else / Gleam lazy_unwrap / Swift・TS `??` | 保持 | `??` 的脱糖目标（ADR-0005 D1） |
| `option.is_some` | `is_some(o: Option[A]) -> Bool` | Rust・Gleam・OCaml is_some | 保持 | — |
| `option.is_none` | `is_none(o: Option[A]) -> Bool` | Rust・Gleam・OCaml is_none | 保持 | — |
| `option.to_result` | `to_result(o: Option[A], e: E) -> Result[A, E]` | Rust ok_or / Gleam to_result / OCaml to_result | 保持 | 注：E 为多态（probe-error/p.almd 确认自定义 variant E）。ADR-0007 D4 的“E 固定为 String”是过时表述 |
| `option.filter` | `filter(o: Option[A], f: (A) -> Bool) -> Option[A]` | Rust filter / Kotlin takeIf | 保持 | R-option-04 |
| `option.zip` | `zip(a: Option[A], b: Option[B]) -> Option[(A, B)]` | Rust zip | 保持 | R-option-04 |
| `option.or_else` | `or_else(o: Option[A], f: () -> Option[A]) -> Option[A]` | Rust or_else / Gleam lazy_or / Kotlin・Swift 运算符 | 保持 | R-option-03 |
| `option.to_list` | `to_list(o: Option[A]) -> List[A]` | OCaml to_list / Kotlin listOfNotNull / Rust iter | 保持 | — |
| `option.collect` | `collect(xs: List[Option[T]]) -> Option[List[T]]` | Rust collect / Gleam all | 保持 | R-option-02 |
| `option.collect_map` | `collect_map(xs: List[T], f: (T) -> Option[U]) -> Option[List[U]]` | Rust map().collect() | 保持 | R-option-02 |

（14 个函数，无新增。）

与 R-L0-05 的关系：像 `option.map(o, (x) => println(x))` 那样为副作用把 `map` 写成语句，会丢弃 `Option[Unit]`，在 R-L0-05 后成为 E042。应改写成 `match o { some(x) => println(x), none => () }`。

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0（已处理）

- R-L0-02 例外的类型：此前提议加入但书，第一参数是 `List[Option[T]]` 或 `List[Result[T, E]]`，含义由元素类型决定的函数（`option.collect`、`option.collect_map`、`result.partition`）放在元素类型模块。
  - 处理：已作为 R-L0-02 但书加入（“含义取决于容器元素类型的函数，放在元素类型模块”）。R-option-02 以此为依据，不用允许列表。
