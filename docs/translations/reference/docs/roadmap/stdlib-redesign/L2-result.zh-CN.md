<!-- description: Stdlib重设计L2/L3 — result：与option一同撤去eager unwrap_or，保留filter/to_err_option/partition，不新增运算符已覆盖的功能 -->
<a id="l2l3-result"></a>

# L2/L3: result

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-result.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-result.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-result.zh-CN.md)

> 非规范性带日期重设计快照。固定原文为权威且不变，不自动同步、无新使用量调查或当前实现核实。保留eager unwrap_or撤去及lazy ??替换的原迁移提案，不宣称默认值求值不变。跨语言F标记属于原调查。保持函数却标不采纳的标题照录，本资料不实际撤去或改名API。历史effect fn传播描述不重新定义现行语言整体规则。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)、对应页[L2-option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)。
> 来源：Rust https://doc.rust-lang.org/std/result/enum.Result.html (F)；Go — 双值`(T, error)` https://pkg.go.dev/errors ；Python — 异常 https://docs.python.org/3/tutorial/errors.html ；TS/JS — 异常和Promise https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/try...catch ；Gleam https://gleam-stdlib.hexdocs.pm/gleam/result.html (F)；Elixir — `{:ok, v}` / `{:error, e}`及`with` https://hexdocs.pm/elixir/Kernel.SpecialForms.html#with/1 ；OCaml https://ocaml.org/manual/latest/api/Result.html (F)；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-result/ (F)；Swift https://developer.apple.com/documentation/swift/result

读法：`??`，空值合并，err时用右侧；`?`，Option化，将Result转Option；`!`，传播，在effect fn内原样返回err，ADR-0008规定仅显式传播。三者依ADR-0005脱糖为stdlib函数；日语“脱糖”读作dattō。

五种语言有Result类型：Rust、Gleam、OCaml、Kotlin、Swift。Go用双值，Python/JS用异常，Elixir用tuple。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 转换ok内容 | `result.map` | `map` (F) | — | — | `then` | `map` (F) | `with` | `map` (F) | `map` (F) | `map` |
| 转换err内容 | `result.map_err` | `map_err` (F) | `fmt.Errorf("%w")` | `raise .. from` | — | `map_error` (F) | — | `map_error` (F) | `recover` (F) | `mapError` |
| 串联转换 | `result.flat_map` | `and_then` (F) | — | — | `then` | `try` (F) | `with` | `bind` (F) | — | `flatMap` |
| 消除嵌套 | `result.flatten` | `flatten` (F, 1.89) | — | — | — | `flatten` (F) | — | `join` (F) | — | — |
| 默认值，提前求值 | `result.unwrap_or`（不推荐） | `unwrap_or` (F) | — | — | — | `unwrap` (F) | — | `value ~default` (F) | `getOrDefault` (F) | — |
| 默认值，仅需要时求值 | `??` = `result.unwrap_or_else` | `unwrap_or_else` (F) | — | — | `catch` | `lazy_unwrap` (F) | — | — | `getOrElse` (F) | — |
| 判断ok/err | `result.is_ok` / `is_err` | `is_ok` / `is_err` (F) | `err == nil` | — | — | `is_ok` / `is_error` (F) | — | `is_ok` / `is_error` (F) | `isSuccess` / `isFailure` (F) | — |
| 转Option，ok侧 | `result.to_option` / `?` | `ok` (F) | — | — | — | — | — | `to_option` (F) | `getOrNull` (F) | `try?` |
| 转Option，err侧 | `result.to_err_option` | `err` (F) | — | — | — | — | — | `get_error`（异常版） | `exceptionOrNull` (F) | — |
| 将List[Result]分为ok和err | `result.partition` | `Iterator::partition` | — | — | `Promise.allSettled` | `partition` (F) | `Enum.split_with` | — | — | — |
| 转list | `result.to_list` | `iter` | — | — | — | — | — | `to_list` (F) | — | — |
| 两个值配对 | `result.zip` | — | — | — | `Promise.all` | — | — | `product` (F) | — | — |
| err时用另一Result | `result.or_else` | `or_else` (F) | — | `except` | `catch` | `try_recover` (F) | — | — | `recoverCatching` (F) | `flatMapError` |
| 按条件将ok变err | `result.filter` | — | — | — | — | — | — | — | — | — |
| 两侧折叠为单值 | `match` | `map_or_else` (F) | — | — | — | — | — | `fold` (F) | `fold` (F) | — |
| 取内容，err则停止 | `?? panic(msg)` | `expect` / `unwrap` (F) | — | — | — | `let assert` | — | `get_ok`（异常版，F） | `getOrThrow` (F) | `get()` |
| 取err内容，ok则停止 | `match` | `unwrap_err` / `expect_err` (F) | — | — | — | `unwrap_error` (F) | — | `get_error` (F) | `exceptionOrNull` | — |
| 查看内容，副作用 | — | `inspect` / `inspect_err` (F) | — | — | — | — | — | `iter` / `iter_error` (F) | `onSuccess` / `onFailure` (F) | — |

<a id="完全性の規則"></a>

### 完整性规则

- option/result对应关系由[L2-option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)完整性规则的同一表维持，R-option-04。仅result有`map_err`、`to_err_option`、`partition`；result没有`collect` / `collect_map`，已由ADR-0007 D1撤去。收集所有错误用`partition`，首错即停用`list.map(xs, (x) => f(x)!)!`。
- 错误类型E默认String，ADR-0002 D2；函数类型保持对E多态，不造仅String函数如前置上下文，ADR-0004 D2，见[L2-error](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md)。
- 默认值只用`??`，R-L0-06。

<a id="l2-の判定"></a>

### L2判断

- 重复：`result.unwrap_or`与`??`同角色，在#1107窗口撤去，R-result-01。
- 缺失角色达5/9以上：五语言的“err则停止并取出”可写`r ?? panic(msg)`。“折叠两侧”“取err内容”“窥看”有三到四语言，可用`match`，不加函数，R-result-04。
- Almide独有形态：三参数`result.filter`、`result.to_err_option`名称，均保留，R-result-02/03。
- 位置：`result.partition`首参数是List，但符合R-L0-02但书：容器元素类型有意义的函数放在元素类型模块。因此放result符合规则，R-result-05，与R-option-02相同。

<a id="案"></a>

## 提案

<a id="r-result-01-resultunwrap_or-を-epoch-16-で撤去する---採用"></a>

### R-result-01 epoch 16撤去`result.unwrap_or` — ○ 采纳

- 内容：同R-option-01。给`@deprecated(since=3, use="??")`增加`[[deprecation]]`块，`since_epoch = 3`、`removal_epoch = 16`，在epoch 16 `breaks`撤去。撤后E002提示：“写`r ?? d`；需err内容时用`result.unwrap_or_else(r, (e) => ...)`”。
- L0依据：R-L0-06、R-L0-14。
- (a)～(c)同R-option-01。
- (d) 撤去，`almide fix`将`result.unwrap_or(r, d)`改为`r ?? d`。使用数spec 27 / tests 6 / docs 16 / stdlib 2（含定义）/ research 1。
- Gate：同R-option-01。
- Issue：#1107，既有。

<a id="r-result-02-resultfilter-の-3-引数を据え置く---不採用撤去も改名もしない"></a>

### R-result-02 保留三参数`result.filter` — × 不采纳，不撤去也不改名

- 内容：保留`result.filter(r, pred, err_val)`。
- L0依据：R-L0-00、R-L0-14第4项，不重新赋予含义。
- (a) 没有语言在Result上提供filter；已(F)确认Rust Result也无。
- (c) 从option类推写两参数`result.filter(r, pred)`会产生参数数量类型错误，不会静默通过。无err值无法造err，故三参数由含义决定。撤去只使写法变长为`result.flat_map(r, (x) => if pred(x) then ok(x) else err(e))`，并不解决写法冲突。
- (d) 无。使用数spec 11 / tests 2 / docs 12 / stdlib 1。
- Issue：无。

<a id="r-result-03-resultto_err_option-を改名しない---不採用"></a>

### R-result-03 不改名`result.to_err_option` — × 不采纳

- 内容：不向Rust `err()`或Kotlin `exceptionOrNull`靠拢改名。
- L0依据：R-L0-00、R-L0-07。
- (a) 写法分散：Rust `err`、Kotlin `exceptionOrNull`、OCaml仅异常版`get_error`，无多数派。
- (c) ok侧`to_option`与err侧`to_err_option`仍符合R-L0-07 `to_<先の型>`，即转到目标类型的形态，可成对阅读。
- (d) 无。使用数spec 9 / tests 4 / docs 7 / stdlib 1。
- Issue：无。

<a id="r-result-04-抜けている役割の関数を足さない---不採用"></a>

### R-result-04 不新增缺失角色函数 — × 不采纳

- 内容：不加`expect`、`unwrap_err`、`map_or_else` / `fold`、`inspect` / `inspect_err`、提前求值的`and` / `or`。
- L0依据：R-L0-00、R-L0-01、R-L0-06。
- (a) 仅“err则停止并取出”达5/9，可写`r ?? panic("msg")`，已在probe-option/q.almd确认。OCaml/Kotlin的`fold`及Rust `map_or_else`共三语言，`inspect`也三语言。
- (c) 接收两函数的`fold`在不同语言参数顺序不同，ok先或err先，易写错；`match r { ok(v) => .., err(e) => .. }`以名称标明分支。
- (d) 无。
- Issue：无。

<a id="r-result-05-resultpartition-を-result-に残す---採用据え置き"></a>

### R-result-05 `result.partition`留在result — ○ 采纳，保留

- 内容：不移动位置，符合R-L0-02元素类型模块但书，其正文也以`result.partition`为例。不放名称允许清单，而让R-L0-G位置检查读取元素类型，与R-option-02相同。
- L0依据：R-L0-02、R-L0-01。
- (a) Gleam是`result.partition` (F)。
- (c) 已有`list.partition(xs, f)`按谓词分组。将List[Result]版移到`list`会同名双义，违反R-L0-01；ADR-0007 D2指定`result.partition`为收集全错误的唯一实现。
- (d) 无。使用数spec 11 / tests 7 / docs 17 / stdlib 2。
- Issue：#3949。

<a id="r-result-06-resultcollect-を戻す---不採用"></a>

### R-result-06 恢复`result.collect` — × 不采纳

- 理由：已由ADR-0007 D1撤去。可失败元素序列只有一种写法：在`list.map` callback里写`!`，首err即停，ADR-0006。恢复`collect`会给同角色第二写法，R-L0-01；也与Rust构造容器的通用`collect`含义不同。
- Issue：无。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名，当时，显示mut | 其他语言主要写法 | 判断 | 提案 |
|---|---|---|---|---|
| `result.map` | `map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` | Rust・Gleam・OCaml・Kotlin・Swift map | 保留 | — |
| `result.map_err` | `map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` | Rust map_err / Gleam・OCaml map_error / Swift mapError | 保留 | ADR-0004 D2规范形式 |
| `result.flat_map` | `flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` | Rust and_then / Gleam try / OCaml bind / Swift flatMap | 保留 | R-L0-10 |
| `result.unwrap_or` | `unwrap_or(r: Result[A, E], default: A) -> A`（deprecated） | Rust unwrap_or / Gleam unwrap / Kotlin getOrDefault | 撤去 | R-result-01 |
| `result.unwrap_or_else` | `unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` | Rust unwrap_or_else / Gleam lazy_unwrap / Kotlin getOrElse | 保留 | `??`的脱糖目标，ADR-0005 D1 |
| `result.is_ok` | `is_ok(r: Result[A, E]) -> Bool` | Rust・Gleam・OCaml is_ok / Kotlin isSuccess | 保留 | — |
| `result.is_err` | `is_err(r: Result[A, E]) -> Bool` | Rust is_err / Gleam・OCaml is_error / Kotlin isFailure | 保留 | — |
| `result.to_option` | `to_option(r: Result[A, E]) -> Option[A]` | Rust ok / OCaml to_option / Kotlin getOrNull | 保留 | `?`的脱糖目标 |
| `result.to_err_option` | `to_err_option(r: Result[A, E]) -> Option[E]` | Rust err / Kotlin exceptionOrNull | 保留 | R-result-03 |
| `result.partition` | `partition(rs: List[Result[T, E]]) -> (List[T], List[E])` | Gleam partition | 保留 | R-result-05 |
| `result.flatten` | `flatten(r: Result[Result[A, E], E]) -> Result[A, E]` | Rust・Gleam flatten / OCaml join | 保留 | R-option-04 |
| `result.to_list` | `to_list(r: Result[A, E]) -> List[A]` | OCaml to_list / Rust iter | 保留 | R-option-04 |
| `result.zip` | `zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` | OCaml product | 保留 | R-option-04 |
| `result.or_else` | `or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` | Rust or_else / Gleam try_recover / Swift flatMapError | 保留 | — |
| `result.filter` | `filter(r: Result[A, E], pred: (A) -> Bool, err_val: E) -> Result[A, E]` | （其他语言无） | 保留 | R-result-02 |

共15函数，无新增。

与R-L0-05关系：为副作用而作语句写`result.map(r, (x) => println(x))`，R-L0-05后会是E042；当时E042已阻止在语句中丢弃Result。改用`match`。

<a id="l0-への差し戻し処理済み"></a>

## 返回L0处理，已解决

- R-L0-02的`result.partition`例外与[L2-option](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)返回项相同；已作为但书纳入R-L0-02，因此R-result-05据此决定。
