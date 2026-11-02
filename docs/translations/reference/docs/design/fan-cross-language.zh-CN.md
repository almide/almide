<a id="fan-against-js-rust-python-and-go-what-each-one-selects-waits-for-and-stops"></a>

# Fan 对比 JS、Rust、Python 和 Go：各自选择、等待和停止什么

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-cross-language.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-cross-language.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-cross-language.zh-CN.md)

> 固定原文的非规范性历史读者快照，不是当前实现指南或独立跨语言比较。未修改原本仍为权威来源；不自动同步，未新运行测试、基准或源码验证。审计自身证据固定于0746eb689（10月17日），带10月18日状态追加，与下方读者版原文固定点不同。已修复 issue、余下发现及已决定但落地中的 Duration 取消均保留各自时间语境。原文 pure 分类仍允许 println，不保证没有可观测输出。他语言主张、“唯一”主张、select_ok／丢弃说明及绝对确定性描述均为历史原文断言，未在此独立重新确立。代码、标识符及参考保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md)

> 最后更新：2026-10-18（Almide 列反映已接受的 ADR-0028：K1 为 `??` / `list.find_map`，K4 为 `fan.min(compute…)`，K2 为 `fan.min(duration…)`，已决定并在 #3788 落地中）。审计证据固定在0746eb689（每个 Almide 单元格的依据见 [fan-semantics-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-semantics-audit.md)）。其他语言的语义来自参考文档（链接在末尾），而不是名称。

本文不按名称匹配操作：Almide `race` 不是 JS `race`，Almide `any` 也不是 JS `any`。每个操作用三个问题描述：

- **选择**：返回哪个结果。
- **等待**：返回前必须完成什么。
- **停止**：阻止什么运行，以及如何阻止。

两种停止不能混为一谈。

- **丢弃**：运行时停止驱动其拥有的计算。例如不再被 poll 的 Rust future、未被取用的 Rayon split、从未启动的 Almide 元素。
- **取消外部工作**：通知运行时之外已开始的工作停止。例如已发送的 HTTP 请求、子进程、已发出的写入。下文所有语言中这都是*协作式*：Go `context`、JS `AbortSignal`、Python `CancelledError` 只会到达检查它们的代码。丢弃 future 永远不能撤回已发送请求。

当前 Almide 完全没有外部取消。它只会“从不启动某元素”或“在计费点截断 pure 本体”（fan.bounded / fan.timeout / fan.min(compute…) 预算）。这些本体经过 pure 检查（E006），所以被截断的本体不能有外部 effect。已决定的 `fan.min(duration…)`（#3788）加入对落败分支的协作式取消：在下一计费点或 I/O 边界取消，并在该形式返回前等待它们结束——属于 Haskell `race` 和 Python `TaskGroup` 那样的结构化方式。

<a id="the-five-selection-kinds"></a>

## 五种选择方式

| # | 类型 | 确定性？ |
|---|---|---|
| K1 | **输入顺序**中的首个成功 | 是 |
| K2 | **真实时间**中的首个成功 | 否 |
| K3 | 真实时间中的首个**完成**，成功或失败均可 | 否 |
| K4 | **逻辑成本最小**的成功（相同则按输入顺序） | 给定成本模型时，是 |
| K5 | **全部收集**（所有结果，或全成功／错误） | 对值而言是；错误选择因语言而异 |

<a id="mapping"></a>

## 映射

| 类型 | Almide | JS | Rust | Python asyncio | Go |
|---|---|---|---|---|---|
| K1 | `list.find_map(xs, (x) => f(x)?)`、`a() ?? b()`——顺序执行，不是 fan 形式 | — | `Iterator::find_map`；Rayon `find_first`（pure 谓词） | — | — |
| K2 | `fan.min(duration.ms(n)) { }` / `(duration…, xs, f)`——已决定，#3788 | `Promise.any` | `futures::future::select_ok`；Rayon `find_any` | `as_completed` 循环 / 带成功检查的 `wait(FIRST_COMPLETED)` | 手写：channel 加 `context` cancel |
| K3 | —（失败分支会跳过，绝不选中；见 K2） | `Promise.race` | `futures::select!` / `future::select` / `select_all` | `asyncio.wait(return_when=FIRST_COMPLETED)` | 从共享 channel 首次接收 |
| K4 | `fan.min(compute.us(n)) { }`、`fan.min(compute.us(n), xs, f)`——必须提供 clock 参数 | — | — | — | — |
| K5 全成功或错误 | `fan.map(xs, f)`、`fan { }` | `Promise.all` | 对 Result 的 `join_all` / `try_join_all`；Rayon `collect::<Result<_,_>>()` | `gather()`、`TaskGroup` | `errgroup.Group` |
| K5 所有结果 | `fan.settle(xs, f)`、`fan.settle { }` | `Promise.allSettled` | `join_all` | `gather(return_exceptions=True)` | 带逐索引错误 slot 的 `errgroup` |

冲突的两个名称正是陷阱。**JS `race` 是 K3，而 Almide `race` 曾是 K4。JS `any` 是 K2，而 Almide `any` 曾是 K1。** 携带 JS 含义的读者，会在墙钟顺序不同于列表顺序或成本顺序的每个输入上得到不同结果。dialect epoch 14 解决了它：`fan.race` 改为 `fan.min(clock)`，删除 `fan.any`，改用上述顺序写法（ADR-0028）。

<a id="selects--waits-for--stops-per-operation"></a>

## 各操作选择、等待、停止什么

<a id="k1-first-success-in-input-order"></a>

### K1：输入顺序中的首个成功

| | 选择 | 等待 | 停止 |
|---|---|---|---|
| Almide `list.find_map(xs, (x) => f(x)?)` / `a() ?? b()`（原 `fan.any`） | 最低索引的 Ok；若全失败则用 `??` fallback | 直到胜者确定，这要求其*之前*所有元素失败 | **胜者之后的元素一律不启动**（审计§4，测试 `any_runs_nothing_after_the_winner…`）。胜者之前的元素全部已运行，保留其 effect。旧 `fan.any` fs-prefetch 例外不属于替代它的 `list.find_map` 循环。 |
| Rayon `find_first` | 最左侧满足谓词的元素 | 整棵 split tree 结束 | 找到匹配后，“尽快”跳过其右侧 split。其他线程已运行的工作会完成，所以谓词可能在答案之后的元素上运行。pure 谓词只是约定，没有检查器。 |

Almide K1 在可观测层面是顺序的：运行元素集合恰好是直到胜者的前缀。Rayon K1 返回同一答案，但已运行集合是依赖时序的前缀超集。

<a id="k2-first-success-in-real-time"></a>

### K2：真实时间中的首个成功

| | 选择 | 等待 | 停止 |
|---|---|---|---|
| JS `Promise.any` | 首个 fulfil 的 promise；若全 reject 则为包含所有原因的 `AggregateError` | 首个 fulfilment，或全部 rejection | **什么也不停止**：promise 无法取消，其他 promise 继续运行。必须手动接入 `AbortController`。 |
| `futures::future::select_ok` | 首个完成的 `Ok` future；若全失败则最后一个错误 | 首个 `Ok`，或全部失败 | **丢弃**其余 future，使其不再被 poll。已开始的外部工作（已发请求、已启动任务）不会撤销。 |
| Rayon `find_any` | 任意顺序中的任意匹配元素 | 直到找到一个 | 跳过剩余 split，运行中的会完成 |
| Python `wait(FIRST_COMPLETED)` 循环 | 结果通过检查的首个任务 | 每轮首次完成 | 自身不做任何停止：调用方必须对其他任务 `task.cancel()`，在下一次 `await` 抛出 `CancelledError` |
| Go | 手写代码所选结果 | 首次接收 | 对 worker 必须检查的 `context` 调用 `cancel()` |

<a id="k3-first-completion"></a>

### K3：首次完成

| | 选择 | 等待 | 停止 |
|---|---|---|---|
| JS `Promise.race` | 首个 settle 的结果，fulfil **或 reject** 均可 | 首次 settlement | 无（见 `Promise.any`） |
| `futures::select!` / `select_all` | 首个完成分支，Ok 或 Err | 首次完成 | 丢弃其他分支（`select!`），或交回剩余 future（`select_all`） |
| Python `wait(FIRST_COMPLETED)` | 已完成集合 | 首次完成 | 无；把 pending 集合返回调用方 |
| Go | 首次接收 | 首次接收 | context cancel，协作式 |

<a id="k4-least-logical-cost"></a>

### K4：最小逻辑成本

只有 Almide 提供这一项。`fan.min` 在确定性计费计量器下逐个运行所有分支，选择消耗最少的 Ok；相同则按源码／列表顺序（审计§5）。提前找到便宜分支并不节省工作：每个分支都运行至完成或达到预算。必需的 Compute clock 参数提供每分支预算；在计费点被截断的分支被排除。分支是 pure，因此没有开始任何外部工作。每台机器的答案相同；自 #3778 起，每个目标也相同：两个计量器读取同一计费表（无循环 wrapper 入口免计费，插值免费）。

最接近的其他方案是对自行计算的成本使用 Rayon `min_by_key`（它会把所有计算运行至完成，因此不节省工作），以及带显式预算的搜索框架。四个生态均没有标准组合子实现这一项。

<a id="k5-collect-all"></a>

### K5：全部收集

| | 选择（出错时） | 等待 | 停止 |
|---|---|---|---|
| Almide `fan.map` / `fan { }` | **最低索引**的 Err | **每个**元素，包括失败之后的 | 无：每个元素均运行（ADR-0024 D1）。按索引而非时间选错误。 |
| Almide `fan.settle` | 每个 Result | 每个元素 | 无 |
| JS `Promise.all` | **时间上首次 rejection** | 首次 rejection 或全部 fulfilment | 无：返回的 promise reject 后，其他仍继续运行 |
| JS `Promise.allSettled` | 每个 outcome | 每个 promise | 无 |
| `futures::future::join_all` | 每个输出 | 每个 future | 无 |
| `futures::future::try_join_all` | **时间上首次 Err** | 首个 Err 或全部 Ok | 首个 Err 时**丢弃**其他 |
| Rayon `collect::<Result<Vec<_>,_>>` | *某个* Err，未规定是哪一个 | 每个运行中 split | 见到 Err 后停止接取新 split |
| Python `gather()` | 时间上首个抛出的异常 | 首个异常 | 无：“其他 awaitable 不会取消，将继续运行” |
| Python `gather(return_exceptions=True)` | 输入顺序的每个结果或异常 | 每个 awaitable | 无 |
| Python `TaskGroup` | 包含**全部**失败的 `ExceptionGroup` | 每个任务，包括已取消任务 | 首次失败时**取消**其余任务，在下一次 `await` 送达 `CancelledError` |
| Go `errgroup.Group.Wait` | **时间上首个非 nil 错误** | 每个 `Go` 调用返回 | 自身不停止。`WithContext` 在首个错误时取消派生 context，只有检查 `ctx` 的 worker 会停。 |
| Go `errgroup.SetLimit(n)` | （同上） | （同上） | 限制运行 goroutine 数：`Go` 阻塞直到有空 slot |

只有 Almide 按输入位置选择错误，并仍然运行其余元素。这使 `fan.map` 在顺序、并行与 wasm 执行下逐字节一致。代价是长列表前部失败也不节省工作。cheatsheet 告诉读者，如需首次失败即停止，应使用带 `!` 的 `for` 循环。

<a id="what-a-translator-must-change"></a>

## 移植者必须改变什么

| 来源 | 写法 | Almide 含义 | 正确 Almide |
|---|---|---|---|
| JS `Promise.any` | `fan.any` | E027 | #3788落地后用 `fan.min(duration.ms(n), mirrors, f)`（真实时间首个成功，取消并等待落败者）；此前若能接受列表中最早者，可用 `list.find_map(mirrors, (m) => f(m)?)` |
| JS `Promise.race` | `fan.race` | E027 | pure 分支中最小 compute 用 `fan.min(compute.ms(n)) { … }`；首个完成用 `fan.min(duration.ms(n)) { … }`（#3788），但失败分支跳过而非选中 |
| JS `Promise.all` | `fan.map` | 全运行、最低索引错误 | `fan.map`（成功值相同） |
| Python `TaskGroup` | `fan.map` | 不取消兄弟任务 | `fan.map`；若需停止则用 `for … !` 循环 |
| Go `errgroup.WithContext` | `fan.map` | 无 context；最早索引错误 | `fan.map` |
| Rust `try_join_all` | `fan.map` | 不丢弃任何项 | `fan.map` |
| Rayon `find_first` / `Iterator::find_map` | `list.find_map(xs, (x) => f(x)?)` | 同一答案；胜者后元素不启动 | `list.find_map` |
| I/O timeout（`asyncio.wait_for`、`context.WithTimeout`） | `fan.timeout { http.get(u) }` | 当前拒绝：本体必须 pure（E006） | 已决定（ADR-0028）：`Duration` clock 接受 I/O 本体，在下一计费点或 I/O 边界取消并等待——#3788 |

<a id="references"></a>

## 参考资料

- JS: MDN, [Promise.all](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/all), [allSettled](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled), [any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any), [race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race), [AbortController](https://developer.mozilla.org/en-US/docs/Web/API/AbortController)
- Rust futures 0.3: [join_all](https://docs.rs/futures/latest/futures/future/fn.join_all.html), [try_join_all](https://docs.rs/futures/latest/futures/future/fn.try_join_all.html), [select](https://docs.rs/futures/latest/futures/future/fn.select.html), [select_all](https://docs.rs/futures/latest/futures/future/fn.select_all.html), [select_ok](https://docs.rs/futures/latest/futures/future/fn.select_ok.html)
- Rayon 1.10: [find_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html#method.find_first), [find_any](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html#method.find_any), [with_min_len](https://docs.rs/rayon/1.10.0/rayon/iter/trait.IndexedParallelIterator.html#method.with_min_len)
- Python 3.12: [asyncio.gather](https://docs.python.org/3/library/asyncio-task.html#asyncio.gather), [TaskGroup](https://docs.python.org/3/library/asyncio-task.html#task-groups), [asyncio.wait](https://docs.python.org/3/library/asyncio-task.html#asyncio.wait), [Task.cancel](https://docs.python.org/3/library/asyncio-task.html#asyncio.Task.cancel)
- Go: [golang.org/x/sync/errgroup](https://pkg.go.dev/golang.org/x/sync/errgroup) (`Wait`, `WithContext`, `SetLimit`)
