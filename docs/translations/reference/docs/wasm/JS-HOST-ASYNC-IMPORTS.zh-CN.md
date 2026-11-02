<a id="async-js-imports-on---host-js-jspi-3353-3371-3383"></a>

# `--host js` 上的异步 JS 导入（JSPI，#3353、#3371、#3383）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](JS-HOST-ASYNC-IMPORTS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](JS-HOST-ASYNC-IMPORTS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](JS-HOST-ASYNC-IMPORTS.zh-CN.md)

> 固定原文的非规范读者快照，以原文为准，不自动同步。未独立验证当前实现、运行时兼容性、性能或内存行为。历史版本与测量均为原文主张，不是当前的普遍保证。此处不执行命令或 hook。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/JS-HOST-ASYNC-IMPORTS.md)


状态：仅在 JS host 上实现（`src/cli/js_host_async.rs`、`src/cli/js_host_fan.rs`）。模块字节不变，除非是每个元素各调用一次异步 hook 的 `fan`（选择 7，#3383）：这种 fan 会导入重叠执行协议。其余情况仅改变生成的 `<mod>.js` / `<mod>.d.ts`。

<a id="the-problem"></a>

## 问题

`@extern(wasm, "js", NAME)` hook 必须同步返回值。Workers KV、`fetch` 和 Cloud Storage 仅以 Promise 形式提供，所以 JS host 原先无法从 Almide 代码进行存储 I/O。JSPI（`WebAssembly.Suspending` / `WebAssembly.promising`）解决此问题：

- Suspending import 挂起 wasm 栈，直到 hook 的 promise 敲定结果。
- 经 `promising` 进入的 export 返回 Promise。

报告者的原型仅修改 glue，在 Node 24、workerd 及 Cloudflare edge 上正确运行。

<a id="choices"></a>

## 选择

**1. 标记放在 extern 声明上**（#3371）。

```almide
@extern(wasm, "js", "kv_get", returns: promise)
fn kv_get(key: String) -> String
```

- 标记说明宿主绑定的事实：该 JS hook 以 Promise 响应。它位于绑定 hook 的地方，如同 wasm-bindgen 的 `suspending`、Emscripten 的 `JSPI_IMPORTS` 和 WIT 的 `async func`。
- 它是命名参数，而非 `async`。ADR-0024 N1 将 `async`、`await`、`Future` 排除在语言表面之外；裸 `async` 会诱导使用语言中不存在的 `async fn`。effect 轴仍是“无颜色”的 `fn` / `effect fn`。
- 类型、effect 和调用方不变。Almide 声明仍是 `fn kv_get(key: String) -> String`。调用方看到普通同步调用，原生目标忽略该标记。
- 解析器仅在 `@extern(wasm, "js", ...)` 的三个位置参数之后接受 `returns: promise`（或 `returns = promise`）。其他命名参数或值、重复出现，或将标记用在其他目标／模块上，均为解析错误；提示会指出可接受写法。`almide fmt` 输出 `returns: promise`。
- 不使用 `--host js` 时，标记不改变模块，两种情况的模块字节相同。使用 `--host js` 时，仅当 `fan` 重叠调用该 hook（选择 7）才改变模块；其余地方仅生成的 glue 读取标记。
- 标记引入前，develop 曾带有临时构建标志 `--async-import NAME`。它在 v0.67.0-rc1 之前移除，从未在发布版中交付。若将来程序必须在一个 host 同步绑定同一 hook、另一个 host 异步绑定，可重新提供构建时覆盖；属性仍为默认方式。

**2. effect。** 异步 import 不获得新的 effect 类别。

- 根据 ADR-0026，extern 主体属于 foreign，已推断为 ⊤（所有类别；`pass_effect_inference.rs`），其调用者已携带这一信息。
- 挂起是 host 运行 import 的方式，不是 import 对程序所做的事，因此类型和类别都不变。

**3. 哪些 export 变为异步。** 恰好是调用图能到达异步 import 的 export；调用图从交付字节读取（在可选 `--wasm-opt` 之后）。

- 在 Node 24.21 上测得：从未经 `promising` 进入的 export 调用 Suspending import，会抛出 `SuspendError`；即使 hook 返回普通值也会发生。因此“仅让可能挂起的 export 异步化”必须是保证。
- 每个 `call_indirect` / `call_ref` 都假定可以到达表中的所有函数。过度近似只会使 export 不必要地异步，绝不会让可能挂起的 export 保持同步。
- 只要有异步 import 就把所有 export 异步化，也正确；但这会迫使所有调用方对纯 helper（`width`）也 await，因此改用可达性。
- `run()` 经 `_start` 遵循同一规则。
- 异步 export 在 `.d.ts` 中返回 `Promise<T>`；有标记 hook 的类型变为 `T | Promise<T>`。
- 作者不为 export 添加标记，因此不存在未经 `promising` 进入却可到达挂起 import 的 export，也就无法到达 `SuspendError`。

**4. 每个实例一次只运行一个调用。**

- JSPI 允许一个调用挂起时第二个 export 调用进入。structural runtime 保存逐实例状态，这些状态必须在调用期间遵循栈纪律：
  - region window 关闭时回退的 bump／heap head。
  - 行缓冲区构建游标。
  - 确定性计量器的 region globals。
- 在这些状态上交错两个挂起栈，正是 ADR-0023（重入一节）对 p3 handler 拒绝的情况；它通过背压将它们串行化。
- glue 采用相同串行化方式：
  - 异步 export 经单一 FIFO 队列（`serial`）运行。每个调用连同其挂起过程都完成后，下一个才进入。
  - 异步调用挂起期间调用同步 export（或 `run()`），会抛出 `almide: <f> was called while an async call is suspended …`，而不是进入。
- 可观察结果等同于按调用顺序逐个执行（ADR-0024 的“as if sequential”）。fixture 断言任何时刻最多一个 hook 待完成。

**5. 缺少 JSPI 时**，`init()` 在实例化之前抛错。错误列出异步 import、缺少的功能（`WebAssembly.Suspending /
WebAssembly.promising`），并指向 Node ≥ 24.20、Chrome ≥ 137 或 workerd。在 Node 上还会指出运行版本：24.0 至 24.19 共用默认关闭 JSPI 的 V8，直到 24.20 才开启（#3362）。托管“Node 24”运行时可能滞后；2026-10-04，Google Cloud Run functions 的 `nodejs24` 是 24.19.0。在这些版本上，`--experimental-wasm-jspi` 仅在 `node` 命令行有效：Node 拒绝在 `NODE_OPTIONS` 中使用它，运行时的 `v8.setFlagsFromString` 也不会安装 API。不尝试半途执行。没有带标记 extern 的模块不需要 JSPI，仍可在原先支持的环境运行。

**5a. 忘记标记会被拒绝**（#3371）。每个未标记 hook 的返回值都经过一次检查：`typeof r?.then === "function"`。成立时调用抛出：

```
almide: hooks.js.kv_get returned a Promise; mark its @extern with returns: promise
```

诊断译文：hooks.js.kv_get 返回了 Promise；应为其 @extern 添加 returns: promise。

- 没有检查时，Promise 原先会静默作为 `"[object Promise]"` 或 `0` 通过。
- throw 像任何 hook throw 一样展开模块栈帧，因此实例被放弃，直到重新运行 `init()`。
- 对 fallible extern，这一拒绝也不是 hook 的 err；把 throw 转成 err 的 catch 会让该拒绝继续向外传递。
- 被丢弃的 promise 会加上无操作拒绝处理器，避免其自身失败再次被报为未处理 rejection。
- 带标记 hook 会 await，不携带此检查。

**6. 挂起前后的 marshalling。**

- 异步 import 的参数在 hook 运行前（调用表达式内部）解码；结果在 promise 敲定后编码。报告者确认可从恢复执行的 hook 调用 `__alloc`。
- export 参数在它自己的排队轮次内构建，位于前面的调用都敲定之后。
- 所有权和布局与同步 wrapper 使用相同的已记录 export ABI（#3352、#3354）。fixture 的 1,000 轮异步调用中，`memoryBytes()` 保持不变。

**7. Fan 重叠执行**（#3383，2026-10-05 的 ADR-0024 修订）。

- `returns: promise` hook 是发往进程外系统的请求。该系统接收请求的顺序是环境的 ω（ADR-0024 D2/D3），因此这些 hook 上的 `fan` 可以重叠等待。JS host 上由 JSPI 提供底层支持（D8）。
- 若把每个元素的一次调用 lowering 为独立 Suspending 调用，会逐元素等待。glue 改为对某个 fan 可到达的每个异步 hook，提供模块 `almide:fan` 的三个 import：
  - `start:NAME(args) -> slot` 为普通 import；解码参数、调用 hook，并将其 Promise 保存在槽内。
  - `wait()` 是唯一 Suspending import；等待所有已启动槽，分别将其敲定为值或拒绝结果。
  - `take:NAME(slot) -> value` 为普通 import；通过与 hook 自身返回相同的 helper 编码并返回已敲定值。拒绝在 fallible extern 中成为 err，在 infallible extern 中放弃实例，与直接调用完全相同（#3356）。
- 模块按分支顺序启动所有元素，挂起一次，再按分支顺序取值。每个元素都运行（D1），按分支顺序的第一个 err 为结果（D6），所以观察结果与逐次 lowering 一致。（自 dialect epoch 14 起，选取第一个 Ok 使用 `list.find_map` / `??` 链；定义上就是逐次执行，因此不走重叠路径。）
- emitter（`crates/almide-wasm/src/fan_js_async.rs`）仅在模块用 `--host js` 构建，并且每个元素都是一次 `returns: promise` extern 调用、参数为变量或字面量、值为标量或 fallible extern 的标量 Result 时启用。覆盖三种形态：fallible hook 的 `fan.map(xs, (x) => hook(x))`，infallible hook 的 `(x) => ok(hook(x))`，以及每个分支都是此类调用的 `fan { hook(a), other(b) }` 块。
- 其他情况仍逐次执行，每个元素一次 Suspending 调用：计算参数的元素、调用同步 hook 的元素，或连续执行多次调用的元素。多次调用要求元素在调用之间挂起（ADR-0024 D8 本身的要求）。
- export 推断不变。调用图可到达 `wait` 的 export 经 `promising` 进入，像其他异步 export 一样串行化。
- **模块字节。** 具有此类 fan 的程序在 `--host js` 下模块字节不同：多三个 import，不直接调用 hook。其余程序以及所有不带 `--host js` 的构建均与原先字节相同。`ALMIDE_FAN_SEQUENTIAL=1`（ADR-0024 D7）构建逐次 lowering。

<a id="not-covered"></a>

## 未覆盖的情况

- **回调模块的 hook。** 从异步 hook 内调用同步 export，会被 busy guard 拒绝；调用异步 export 则会排在正在等待它的调用之后，永远无法完成。不要这样做。
- **拒绝的 hook** 按抛错同步 hook 处理（#3356，docs/wasm/WASM-OUTPUT.md 的“JS host”）：fallible extern（`effect fn` 或 `Result[T, String]`）以 err 形式收到拒绝；infallible extern 放弃实例直到再次运行 `init()`。
- **Flow／流**（`docs/roadmap/on-hold/flow-design.md`，“JSPI での Flow 実装”）。本文只覆盖单值 import。

<a id="gate"></a>

## 门禁

`spec/wasm_host_js/async_imports.almd` 由 `scripts/check-js-host.sh` 运行，其异步 hook 标为 `returns: promise`。fixture 检查：

- 哪些 export 异步，哪些保持同步。
- 重叠调用按顺序、一次一个地获得正确响应。
- 通过闭包到达的异步 import，以及 effect fn 的 err。
- 拒绝的 infallible hook（放弃实例，`init()` 恢复）。
- 内存保持不变。
- 无 JSPI 时拒绝。

`hook_errors_async.almd` 是 `hook_errors.almd` 的拒绝对应版。`fan_async_overlap.almd` 覆盖 fan 重叠（7）。门禁构建两次：一次重叠，一次带 `ALMIDE_FAN_SEQUENTIAL=1`；两次运行必须打印同一 `.expected`：分支顺序的值、中间分支的拒绝、`list.find_map` 第一个 Ok（逐次）、fan 块、内存不变，以及会放弃实例的 infallible hook。host 断言重叠运行时所有 hook 同时在途，耗时约为最慢 hook；逐次运行时仅一个在途，耗时为总和。在 Node 24.21 上，八个耗时 10–80 ms 的 hook，重叠用时 82 ms，逐次用时 369 ms。`unmarked_promise.almd` 针对 infallible、fallible 和 `Unit` hook 覆盖 5a 的拒绝；它不使用任何标记，因此不需要 JSPI。

带标记 extern 的 fixture 需要有 JSPI 的 node，因此 CI 的 JS host 作业运行固定 Node 24.22.1（`.github/workflows/ci.yml`）并进行 JSPI 功能探测。本地没有 JSPI 的 node 会发出警告并跳过这些 fixture。

解析器接受与拒绝案例位于 `crates/almide-syntax/src/parser/test_attributes.rs` 及诊断 fixture `tests/diagnostics/plain-extern-returns-unknown`。

> 范围注记：选择 4 中最多一个待完成 hook 的断言描述串行化 export 调用；选择 7 另行允许合格 fan 内的 hook 重叠。无 SuspendError 的陈述保留在生成 wrapper 的可达性规则范围内，并不保证绕过 wrapper 的使用。fixture 内存不变不是所有程序均无泄漏的证明。
