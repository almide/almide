<a id="adr-0024-fan-runs-effect-callbacks-concurrently-on-every-target-as-if-sequential--fanmap-runs-every-element-the-compiler-picks-the-substrate-and-no-concurrency-concept-reaches-the-writer"></a>

# ADR-0024：`fan` 在每个目标上并发运行 effect 回调，保持如同顺序执行的观测 — `fan.map` 运行每个元素，由编译器选择执行基础，不把并发概念交给编写者

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0024-fan-effect-callbacks-run-concurrently-as-if-sequential.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0024-fan-effect-callbacks-run-concurrently-as-if-sequential.zh-CN.md)

> 非规范性历史读者快照，未改动的固定原本为权威。不自动同步，未独立运行基准，不保证当前实现。区分接受的设计与带日期的部分实现；审计仍明确D4和effect回调线程化未实现。观测指stdout/stderr/exit/值，不代表全部外界effect相同；保留trap残余与环境请求顺序的限定。他语言比较及耗时为原文当时主张，不是新测量。后续fan选择决定可能替代旧取消/any规则，保留历史而不暗中统一。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md)

- **状态**：已接受。维护者于2026-09-30接受两项待决决定：(1) 外部系统接收程序请求的顺序属于环境回答（ω），不属于程序观测；(2) `fan.map` 运行每个元素，结果是列表顺序中的第一个 `Err`。接受时均未实现。截至2026-10-17（docs/design/fan-semantics-audit.md，固定0746eb689），D1、D5、D6已实现。截至2026-10-18（#3780），D3已用于 `fan { }` 块：stdlib以 `@ordered` 标记有序集合（`env.set` 加入最初列表，与 `kv` 一样是进程局部状态），若某分支的可执行闭包可达其中操作，该块在native上也按分支顺序运行（`mark_inline_fans`，spec/wasm_cross/fan_ordered_arms.almd）。D4（顺序执行note）尚未实现。同日2026-10-18（#3779、#3784），native并行版本（`fan.map`，fan内的 `list.map` / `filter` / `any` / `all`）为分配的一块元素而非每个元素保留一条输出时间线；块内元素在同一线程上按列表顺序运行，因此每块只注册一次，就能在元素粒度保持D5顺序与D6最小索引trap（spec/wasm_cross/fan_list_map_trap_order.almd）。先前每元素注册使轻量map慢于顺序执行。step 4在线程上运行effect `fan.map` / `fan.settle` 也尚未实现：除非唯一effect是嵌套fan，effect回调仍顺序执行（#2594）。2026-10-05修订（#3383）：`--host js` 上async JS hook的D3、D8（见参考资料前的修订节）。
- **日期**：2026-09-30
- **范围**：native与wasm的 `fan { … }`、`fan.settle { … }`、`fan.map(xs, f)`、`fan.settle(xs, f)` 执行基础；元素失败后的 `fan.map` 含义；fan顺序运行时给编写者的提示；不把并发带入表层的不变量。`fan.any` 不在范围内（§8）。
- **完成其设计**：#2594（native的 `fan.map` 顺序运行effect回调，文档却另有说法）。
- **基于**：[ADR-0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md)（执行基础是自由变量；其D1输出事务成为这里的step 1）、[ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §3（concurrent slot与可执行闭包）、§5.5（native线程、RNG重新播种）、[ADR-0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)（纯fn可以打印，所以纯回调也可被观测）、[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)（wasm HTTP是p3 component上的 `wasi:http@0.3`）。
- **章程**：[concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)（#1000，确定性数据并行）、[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md) §3（head × form表）和pillar 3（取消是优化，不是语义）、[execution-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/execution-inception.md)（as-if规则）。
- **涉及契约**：C-004、C-005、C-200（§9）。契约变更先落入almide/als。

<a id="1-context"></a>

## 1. 背景

<a id="11-what-2594-found"></a>

### 1.1 #2594的发现

文档承诺一件事，编译器却做另一件事。

- `docs/CHEATSHEET.md` 用 `fan.map(urls, (u) => http.get(u))!` 展示fan-out习惯用法，并称纯回调“在native上并行”。
- `runtime/rs/src/fan.rs` 把所有**effect**回调送往 `almide_rt_fan_map`，即对 `Rc<dyn Fn>` 的顺序循环。只有作用于 `Send` 标量的纯lambda字面量会到达 `almide_rt_fan_map_par`（#2044）。那里的注释指出原因：`Rc<dyn Fn>` 既不是 `Send` 也不是 `Sync`。

这个原因属于闭包表示，不属于语言。块形式证明了这一点。2026-09-29，一个试验程序有三个 `fan { … }` 分支，各自把 `String` 和 `List[String]` 传给sleep 500 ms的effect fn，**505 ms**即完成。生成Rust是内联的 `std::thread::scope`，分支是带所有权捕获的 `move` 闭包。传给 `fan.map` 的lambda字面量也可以用相同方式lowering。

同一测量还显示ADR-0011记录的缺口。三个分支的 `println` 输出为 `arm 1, arm 3, arm 2`，而且多次运行顺序不同。ADR-0011 D1（每分支输出事务）尚未落地。

<a id="12-the-substrate-today-per-head-and-target"></a>

### 1.2 当时各head与目标的执行基础

| 表层 | native | wasm（p3 component） | interp |
|---|---|---|---|
| `fan { a; b }` | 每分支一线程，全部运行，分支顺序的第一个 `Err` | 顺序 | 顺序 |
| `fan.settle { a; b }` | 顺序（三个1 s分支耗时3015 ms，#2594评论） | 顺序 | 顺序 |
| `fan.map(xs, f)`，纯标量lambda | 分块线程（#2044） | 顺序 | 顺序 |
| `fan.map(xs, f)`，effect回调 | 顺序 | 顺序；主体恰为一个 `fs.read_text` 时，作为async subtask预取并按列表顺序等待（b973a1dd4） | 顺序 |
| `fan.settle(xs, f)` | 顺序 | 顺序 | 顺序 |

没人选择过这个矩阵。各格只是表示方式使哪种实现容易，恰是ADR-0011所谓“执行基础住在章程的空白处”。

<a id="13-the-block-and-mapper-forms-disagree-after-an-err"></a>

### 1.3 `Err` 后块形式与mapper形式不一致

章程表对无标记head的两种形式给出同一规则：“all — 全部；列表顺序中的第一个 `Err`”。实现却分裂了：

- `fan { a; b }` 运行**每个**分支，随后报告第一个 `Err`（`crates/almide-wasm/src/fan.rs` 头部：“全部分支运行后，第一个err导致abort”；native `thread::scope` join每个分支）。
- `fan.map` 在第一个 `Err` 处**停止**（`stdlib/fan_map.almd`：“第一个err处短路”；native `collect()` 也停止）。

短路是并发不能在不投机的情况下复现的规则。元素 `k` 失败时，并发map已开始元素 `k+1`。打印输出可以丢弃，但POST已发送。

<a id="14-what-the-peers-do"></a>

### 1.4 其他语言和库的做法

| | 结果顺序 | 失败时 | 失败后的兄弟 | 并发上限 | 收集全部的形式 |
|---|---|---|---|---|---|
| JS `Promise.all` | 输入顺序 | **时间上**第一个失败 | 继续运行 | 无内置 | `Promise.allSettled` |
| Go `errgroup` | 自行收集 | 时间上第一个失败 | context被取消（协作式） | `SetLimit(n)` | 自行编写 |
| Python `asyncio.gather` / `TaskGroup` | 输入顺序 | 第一个异常 | gather：继续 / TaskGroup：取消 | 无内置 | `return_exceptions=True` |
| Kotlin `coroutineScope` + `awaitAll` | 输入顺序 | 第一个异常 | 取消 | `Semaphore` | `supervisorScope` |
| Swift `TaskGroup`、Trio nursery、Java `StructuredTaskScope` | 完成顺序 | 第一个异常 | 取消 | 无内置 | 依库而定 |
| Haskell `mapConcurrently` | 输入顺序 | 第一个异常 | 取消 | 无内置 | — |
| Rust `try_join_all` / `buffered(n)` | 输入顺序 | 第一个 `Err` | 通过drop取消 | `buffered(n)` | `join_all` |

它们都承诺结果顺序，却不承诺effect顺序。在每个例子中，哪个失败胜出取决于时序。改变并行度会改变程序打印内容。

各生态还付出了Almide不能付出的代价（§3）：

- **Rust。** `async fn` 与 `fn` 是两种颜色，stdlib与生态随之分裂。`Send`、`'static`、`Pin` 约束抵达编写者，常表现为远离编辑行的类型错误（“future is not `Send`”）。取消是在 `.await` 处 `drop`，取消安全成为编写者的工作。executor由库选择，生态因而分叉（tokio、async-std）。
- **Go。** goroutine让并发易写，也让数据竞争易于交付。除非编写者另行安排，goroutine会比调用者活得更久。
- **JS/TS。** 没人await的promise会静默失败。浏览器是单线程。
- **Swift 6。** 严格的 `Sendable` 检查让数据竞争成为编译错误，而迁移变成一堵关于类型、而非编写者想改的代码的错误墙。
- **C#。** `async`/`await` 易读，但竞争仍然能编译。

<a id="2-decision"></a>

## 2. 决定

**`fan` 在每个目标上并发运行，其观测恰与按列表顺序逐个求值相同。`fan.map` 运行每个元素，报告列表顺序中的第一个 `Err`。编译器选择执行基础；编写者永远看不到并发概念、相关错误或调节旋钮。**

<a id="d1-the-observation-is-the-sequential-run-every-element-evaluation"></a>

### D1. 观测是顺序、运行每个元素的求值

对 `fan { … }`、`fan.settle { … }`、`fan.map(xs, f)`、`fan.settle(xs, f)`，参考含义是按列表顺序逐个求值，**每一个都执行**，然后：

- `fan { … }` / `fan.map`：若某元素返回 `Err`，结果是最低索引的 `Err`；否则为按列表顺序排列的结果。
- `fan.settle`：每个元素的 `Result`，按列表顺序排列。

这改变了 `fan.map`：不再在第一个 `Err` 停止（§1.3，维护者决定2）。块与mapper形式现在有同一规则，正如章程表一直所述。

“观测”是#1000的定义：stdout、stderr、退出码与返回值。

<a id="d2-the-environments-order-is-ω"></a>

### D2. 环境的顺序是ω

两个元素向外部系统发送请求时，该系统接收请求的顺序属于环境回答ω。这是async-inception的oracle层（B1）已作为声明输入处理的类别（维护者决定1）。程序看到的响应，是与其他I/O一样的输入。

<a id="d3-the-compiler-chooses-the-substrate-per-fan-by-one-question"></a>

### D3. 编译器对每个fan用一个问题选择执行基础

除非其可执行闭包可达**有序操作**，fan就**并发**运行。若可达，则**顺序**运行。没有第三类。

- 可执行闭包直接复用ADR-0020 §3.2的 `X(E)`。四种表层的每个函数值参数都已是concurrent slot（ADR-0020 §3.1）。
- **有序操作**是这样一种stdlib函数：其对进程局部状态的effect可能被兄弟元素读到，使兄弟结果依赖顺序。stdlib以无参数属性标记（暂名 `@ordered`，拼写由实现PR决定，与ADR-0020留下 `@concurrent` 相同）。初始集合：
  - 所有创建、写入、追加、复制、重命名、删除的 `fs` 函数（`fs.write_*`、`fs.append_*`、`fs.copy`、`fs.rename`、`fs.remove*`、`fs.mkdir_p`、`fs.create_temp_*`）；
  - 所有 `kv` 函数（ADR-0020 §4；读可观测兄弟的写）；
  - 所有从stdin读取的 `io` 操作（哪个元素得到哪行依赖顺序）；
  - `process` 创建子进程（子进程可执行上述任意操作）。
- 非有序：输出（`print`、`println`、`eprintln`，由D5覆盖）、`fs` 读取、`http` 客户端调用（ω，D2）、读时钟（已属环境）、`random`（ADR-0020 §5.5：随机性不承诺可复现，所以只要求每线程不同seed）。

只有某元素写入时，读写才会竞争。只要有写入，整个fan就顺序执行。规则是每fan一位，各目标相同。

<a id="d4-sequential-is-a-note-never-an-error"></a>

### D4. 顺序执行给note，绝不给错误

选择顺序执行基础改变速度，不改变观测。因此编译器不因它拒绝程序。它以编写者术语给出带见证路径的**note**（不是warning，因为没有问题）：

```
note: this fan runs its elements one at a time
  --> src/sync.almd:12:3
   |
12 |   fan.map(items, (it) => save(it))
   |   ^^^^^^^ `save` writes a file (fs.write_text, src/store.almd:8),
   |           and another element could read it
```

编译器无法移到其他线程的任何东西，如列表中的闭包值，也同样处理。note指出值，不指出 `Send`、`Sync`、线程或生命周期。

<a id="d5-output-is-transactional-per-element-adr-0011-d1-promoted-to-a-precondition"></a>

### D5. 输出是每元素事务（ADR-0011 D1升级为前提）

每个元素的stdout、stderr进入该元素的一条时间线。先前元素结束后按列表顺序flush。元素0没有前驱，直接流式输出。嵌套fan的元素flush到外层元素时间线。stdout与stderr保持在每元素同一时间线中，flush时拆入两个fd（ADR-0011 D1）。也覆盖可以打印的纯回调（ADR-0022）。

在此落地前，不得为任何表层开启并发执行基础（ADR-0011：“D2被拒后Rung 1”）。

<a id="d6-failures-err-waits-for-everyone-a-trap-waits-for-everyone-below-it"></a>

### D6. 失败：`Err` 等所有元素，trap等所有更低索引元素

- **`Err`。** D1规定每个元素都运行，所以并发执行基础join所有元素并选择最低索引的 `Err`。没有投机。
- **trap**（除零、溢出、越界、`panic`、失败的 `assert`）。顺序执行时元素 `k` trap意味着 `0..k` 已运行，`k` 之后没有运行。并发执行基础停止启动元素，等待 `k` 以下的所有元素，让最低索引trap胜出。它按顺序flush该元素以下的时间线，再flush陷入trap元素的部分时间线，然后走统一main-error路径abort（C-005 / C-200形状不变）。
- **残余。** 胜出trap以上已启动的元素，可能已发送顺序求值绝不会发送的请求。输出会被丢弃，所以观测一致；对外界的effect却不一致。trap属于bug类退出，要消除此差异，就必须在 `k` 完成前绝不启动 `k+1`，那就是顺序执行。此残余在这里和C-200明确记录，不予隐藏。

<a id="d7-no-concurrency-knob-in-the-surface"></a>

### D7. 表层没有并发调节旋钮

没有 `limit:` 参数。async-inception的拒绝表已说“`fan.map(xs, limit: n, f)` — 需求得到证明前不添加”。runtime自行限制进行中的元素。初始上限为ADR-0020的 `max_in_flight` 默认64，让进程只有一个数值。`ALMIDE_FAN_SEQUENTIAL=1` 保留为消融开关，是测量环境变量而非语言特性。
（2026-10-17审计注记：开关仅覆盖 `fan.map` 与 `fan { list.map }` 的并行版本。native `fan { }` 块仍使用线程。）

若动机程序搜索工具遇到64无法承受的provider限流，那就是该表要求的需求证明，由新ADR决定拼写。

<a id="d8-wasm-gets-its-concurrency-from-wasi-03-async-below-the-ir"></a>

### D8. Wasm从IR以下的WASI 0.3 async获得并发

在p3 component上，元素的host call经async lowering成为由waitable set驱动的subtask；这是b973a1dd4用于 `fs.read_text` 预取、ADR-0023用于 `wasi:http@0.3` 的机制。连续进行多个host call的元素需要在调用间挂起自身。编译器以状态机或stack switch提供。**此机制位于IR以下，绝不出现在类型、签名、诊断或文档里**（N1）。

一般情形落地前，wasm顺序运行fan。D1使观测相同，故这是速度差距而非行为差距。不用共享内存、也不用atomics；ADR-0011 D3与C-210的 `FORBIDDEN` 保持有效。

<a id="3-invariants-what-almide-does-not-repeat"></a>

## 3. 不变量：Almide不重蹈的覆辙

这些是规范性的。未来打破任何一条，须有新ADR取代本节。

| # | 不变量 | 排除的失败 |
|---|---|---|
| N1 | 表层没有 `async`、`await`、`Future`、task handle、`Pin` 或executor。并发就是 `fan` 表达式。effect轴保持 `fn` / `effect fn`，不新增颜色。 | Rust的两色和双份生态；状态机泄漏进类型 |
| N2 | 元素能否在其他线程运行由编译器决定，绝不成为编写者面对的错误。诊断不出现 `Send`、`Sync`、`Sendable`、`'static`、“thread”。 | Rust的“future is not `Send`”、Swift 6的 `Sendable` 迁移错误墙 |
| N3 | 取消是优化而非语义（async-inception pillar 3）。没有表层会取消已开始的元素。`race` 与 `select` 保持移除（E027）。 | Rust在 `.await` 处的取消安全、`select!` 丢弃半完成工作 |
| N4 | 一个runtime，由编译器按目标选择。编写者不选executor。 | tokio / async-std分叉 |
| N5 | `fan` 表达式产生值时所有元素均已完成。没有detach，没有spawn。 | Go泄漏的goroutine，JS未await的promise |
| N6 | 元素不可达任何 `var`（ADR-0020 §3，E008）。 | Go与C#的数据竞争 |
| N7 | 改执行基础绝不改观测，并由gate检查（ADR-0011 D5），反复运行并强制执行基础（ADR-0011 F5）。 | 各其他例子：输出是调度的函数 |
| N8 | 没有表层并发度旋钮（D7），除非需求被证明，之后也只有一种拼写。 | `buffered(n)` / `SetLimit` / p-limit，各生态各自一套 |

<a id="4-rationale"></a>

## 4. 理由

1. **其他例子的失败有同一原因：调度成为编写者必须推理的东西。** Rust使其可由类型表达，Go使其廉价，JS使其直到失败才可见。Almide章程已把观测固定为顺序求值（#1000），调度无处泄漏。缺少的是承诺的另一半：编译器因此可以自由调度，而且确实会这样做。
2. **运行每个元素使D3只需一位。** 短路时并发需要投机，只有可撤销effect才能安全投机。于是回调按调用图推断分为读取、不可逆写入、共享状态。深层helper的一次修改就会悄悄改变fan类别。这是Rust的auto-trait推断，只是把错误换成性能悬崖。没有短路，剩下的问题只是“兄弟能否观测这个元素对进程状态的effect？”
3. **块形式已经运行每个分支。** D1让 `fan.map` 与 `fan { }` 及章程表一致，不引入新含义。
4. **执行基础是自由变量（ADR-0011）。** 因而应给note而非错误：编译器无法并发时程序仍正确，只是更慢。拒绝它会重现Swift 6的结局，却不增加正确性。
5. **符合动机程序。** 搜索聚合器用 `fan.settle(providers, (p) => query(p, q))` 调用多个provider。`http` 调用非有序，因此在native和p3 component上并发运行。即使provider用POST查询也如此，因为没有元素被投机启动。

<a id="5-alternatives"></a>

## 5. 替代方案

| 选项 | 裁决 | 原因 |
|---|---|---|
| 只承诺结果顺序，允许effect顺序变化（本ADR讨论的首稿） | **拒绝** | 违背#1000与ADR-0011 D1：观测会依赖调度，正是ADR-0011要堵住的确定性缺口 |
| 保留短路，只投机“可撤销”回调（读、GET、输出） | **拒绝** | 经调用图推断三类，一次修改无形中翻转fan类别（§4.2） |
| 拒绝编译器无法并发运行的回调（先warning后编译错误） | **拒绝** | 程序正确。错误会成为Rust的 `Send` 错误与Swift 6迁移墙（N2） |
| 静默回退顺序执行 | **拒绝** | 正确但不可诊断。D4 note不阻塞而保留可见原因 |
| 现在添加 `fan.map(xs, f, limit: n)` | **推迟** | 章程要求已证明的需求（D7） |
| 为I/O密集并发添加 `async fn` / `await` | **拒绝** | N1。`fan` 已表达哪些工作重叠 |
| 让每个值 `Send`（Rc→Arc），使任意回调可移动 | **拒绝** | ADR-0020 §5.5已因整个runtime的成本拒绝它。所有权捕获与有序操作满足需求 |
| 保持effect回调的 `fan.map` 顺序执行并修正文档 | **拒绝** | 动机程序仍无并发，执行基础仍由偶然决定（§1.2） |

<a id="6-consequences"></a>

## 6. 后果

<a id="gains"></a>

### 收益

- `fan.map(urls, (u) => http.get(u))` 按cheatsheet已有说法在native上并发，并在D8落地后于p3 component并发。
- `fan { }` 输出不再交错（D5），堵住ADR-0011缺口，移除C-004的EXCEPTION条款。
- `Err` 后 `fan { }` 与 `fan.map` 只有一个规则。
- 写下N1–N8不变量，之后“添加 `await`”或“添加limit旋钮”必须论证为何应违背它们。

<a id="costs"></a>

### 成本

- **行为变化。** `fan.map` 元素失败的程序现在也运行后面的元素。它们的输出按顺序flush，在abort前出现，effect也发生。这是含义改变，所以是dialect epoch（§9）。
- **`Err` 后浪费工作。** 对1 000个URL做map，首个失败后仍获取其余999个。想在首个失败停止，编写者用带 `!` 的 `for` 循环明确表达。
- **缓冲。** 一个元素的输出要保留到所有前驱完成。最坏为除首个元素之外全部元素的输出总量（ADR-0011 D1）。
- **trap残余**（D6）。
- D8一般情形落地前，**wasm速度落后**。

<a id="7-falsifiers"></a>

## 7. 可证伪条件

- **F1。** 存在不同执行基础观测不同、D5/D6无法弥合的程序。则从受影响表层移除该执行基础（ADR-0011 F1：放弃基础，绝不放弃观测）。
- **F2。** 运行全元素的成本出现在真实程序而非假想中。例如搜索工具或dojo corpus遇到问题，而 `for` 循环不是自然写法。则重新讨论短路，但设计不得恢复§4.2的三类。
- **F3。** D3有序集合错误：集合外stdlib函数使兄弟观测到顺序（漏掉类别），或集合内函数无端强迫真实程序顺序执行。修正集合并向D5式gate添加案例。
- **F4。** 用户需要编写者可见的limit（D7）。
- **F5。** D8机制无法保持在IR以下，比如component model迫使用户函数具有async签名。则受影响形状的wasm保持顺序执行，不放宽N1。

<a id="8-out-of-scope"></a>

## 8. 范围之外

- **`fan.any`。** 提前截断就是其含义（“最低索引的成功，顺序回退”）。它的并发需要§4.2为 `fan.map` 所拒绝的投机。保留顺序基础。p3 component现有只读预取保留，因为只放弃读取。另一个决定可重新讨论它。
- `fan.race` / `fan.bounded` / `fan.timeout`：由async-inception管辖。
- `http.serve` 并发：ADR-0020 §5.5。

<a id="9-contracts-epoch-plan"></a>

## 9. 契约、epoch、计划

**契约变更**（先almide/als）：

- **C-004。** “`fan.map` 按列表顺序顺序运行元素fn”改为“`fan.map` 与 `fan.settle` 无论执行基础如何，都表现为按列表顺序逐一求值全部元素”。在落地D5的PR删除EXCEPTION条款（ADR-0011 D6）。
- **C-005。** 限定“第一个返回Err的元素fn”：所有元素均运行，显现的是最低索引 `Err`。
- **C-200。** “不等待进行中的兄弟”改为D6：等待trap元素以下的兄弟，并说明残余。

**Dialect epoch。** 台账计数“构造含义改变”（`proofs/dialect-epochs.toml`）。D1属于此类，因此使用ADR-0020 epoch 6的下一个epoch。`breaks` 行为：“即使一个元素返回 `Err`，`fan.map` 也运行每个元素；若要在首次失败停止，使用带 `!` 的 `for` 循环”。对 `spec/`、`examples/`、dojo corpus的迁移影响（`fan.map` 中失败元素后跟打印或写入元素的程序）在step 3测量，早于确定epoch。

**计划。**

| Step | 工作 | 依赖 |
|---|---|---|
| 0 | 在almide/als修改C-004 / C-005 / C-200文案 | — |
| 1 | native的 `fan { }`、`fan.settle { }`、`fan_map_par` 实现D5；删除C-004 EXCEPTION | 0 |
| 2 | D3集合的 `@ordered` 属性；从ADR-0020 `X(E)` 计算基础位（`fan { }` 已于2026-10-18 #3780完成）；D4 note（未决） | ADR-0020 step 1（#2697） |
| 3 | 所有路径的D1：`stdlib/fan_map.almd`、wasm emitter、native、`almide-interp`、参考求值器（ADR-0015）；影响测量；epoch | 0 |
| 4 | native：把 `fan.map` / `fan.settle` 的lambda字面量和命名fn回调lowering为类似块分支的拥有捕获的 `move` 闭包；`fan.settle { }` 线程化；进行中上限（D7）；每线程RNG seed（ADR-0020 §5.5） | 1, 2, 3 |
| 5 | ADR-0011 D5 gate扩至四种表层，反复运行并强制执行基础 | 4 |
| 6 | wasm p3：单调用元素的host call作为async subtask（把b973a1dd4推广至 `http.*`），然后多调用元素（D8） | 1, 3, ADR-0023 |
| 7 | 文档：CHEATSHEET fan节、`docs/specs/als/runtime.md` ALS-R3、面向LLM文档 | 4 |

<a id="amendment-2026-10-05-async-js-hooks-on---host-js-3383"></a>

## 2026-10-05修订：`--host js` 的async JS hook（#3383）

维护者在#3383裁定接受（仅首个slice）。#3371将“此JS hook以Promise回答”放在extern上（`@extern(wasm, "js", NAME, returns: promise)`）。本修订规定JS host上此类hook的 `fan` 含义。

**D3修订。** 标记 `returns: promise` 的extern是向进程外系统（Workers KV、`fetch`、Cloud Storage）发请求。该系统接收请求的顺序属于环境ω（D2），正是D3已赋予 `http` 客户端调用的类别。所以 `returns: promise` extern**不是**有序操作，不强迫fan顺序运行。同步extern维持原状：绝不重叠。

**D8修订。** `--host js` 以JSPI（`WebAssembly.Suspending` / `WebAssembly.promising`）为基础，正如p3 component的WASI 0.3 async。如果fan的每个元素都用已拥有的值执行**一个**async-hook调用，则像p3预取那样lowering（`fan.rs`，host op 40 start / 41 await / 42 abandon），glue为fan可达的每个hook生成三个import：

- `start:NAME(args) -> slot`：普通import。调用hook，把Promise保存在slot中。
- `wait()`：**唯一的**Suspending import。等待所有已开始slot，使每个结算为值或拒绝。
- `take:NAME(slot) -> value`：普通import。返回结算值，按hook自身返回类型封送。拒绝在fallible extern中成为err，infallible extern则放弃instance，与直接调用相同（#3356）。

module按分支顺序启动所有元素，挂起一次，再按分支顺序取值。每个元素都运行（D1），结果是分支顺序第一个 `Err`（D6），因此观测与顺序lowering相同（N7）。`fan.any` 取到首个ok为止。未取的slot也已由 `wait` 等待（N5），然后未读即drop。该slice为 `fan.map`、`fan.any`、`fan { }` 块。JS host其他fan仍顺序执行并给D4 note：元素计算自己的参数、调用同步hook、或连续多次调用。多次调用需要元素在其间挂起，也就是D8本体的状态机或stack switch。

任何内容都不进入类型、签名、诊断、面向LLM文档（N1）。可达 `wait` 的export经 `promising` 进入，与直接async调用相同（#3353）。只有含此类fan且用 `--host js` 构建的程序，module字节才与顺序lowering不同；其他构建均字节一致。`ALMIDE_FAN_SEQUENTIAL=1`（D7）强制顺序lowering。gate `spec/wasm_host_js/fan_async_overlap` 构建并运行两种lowering，要求输出相同，且断言重叠版本的所有hook同时进行中（issue测得十次50 ms读取从510 ms → 50 ms）。

<a id="references"></a>

## 参考资料

- #1000 / [concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)、[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md) §3与pillar 3、[execution-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/execution-inception.md)
- 本仓库：`runtime/rs/src/fan.rs`、`stdlib/fan_map.almd`、`crates/almide-wasm/src/fan.rs`、commit b973a1dd4、`docs/CHEATSHEET.md` fan节、`docs/contracts/contracts.toml` C-004 / C-005 / C-200、`proofs/dialect-epochs.toml`；2026-09-29试验测量（带heap参数的三分支 `fan { }` 耗时505 ms；分支间 `println` 交错）
- [MDN: Promise.all](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/all)、[Promise.allSettled](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled)
- [Go: errgroup](https://pkg.go.dev/golang.org/x/sync/errgroup)、[Go: race detector](https://go.dev/doc/articles/race_detector)
- [Python: asyncio task groups and gather](https://docs.python.org/3/library/asyncio-task.html)
- [Kotlin: composing suspending functions / structured concurrency](https://kotlinlang.org/docs/composing-suspending-functions.html)
- [Swift: TaskGroup](https://developer.apple.com/documentation/swift/taskgroup)、[Swift 6 migration guide](https://www.swift.org/migration/documentation/migrationguide/)
- [Haskell: Control.Concurrent.Async](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html)
- [Rust: tokio::select! and cancellation safety](https://docs.rs/tokio/latest/tokio/macro.select.html)
- [N. J. Smith, Notes on structured concurrency](https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/)
- [B. Nystrom, What color is your function?](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/)
- [WASI 0.3](https://wasi.dev/releases/wasi-p3)
