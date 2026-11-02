<a id="adr-0020-a-served-handler-cannot-reach-a-var-state-lives-in-a-host-independent-kv-store-the-app-is-a-value-and-every-host-is-a-thin-shell"></a>

# ADR-0020：被serve的handler不可达 `var`；状态位于host无关KV store；app是值，每个host是薄shell

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0020-http-serving-and-handler-state.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0020-http-serving-and-handler-state.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0020-http-serving-and-handler-state.zh-CN.md)

> 非规范性历史读者快照，未改动的固定原本为权威。不自动同步，未独立运行benchmark，不保证当前host/安全性，也不授权执行。保留日期层次：最初“均未实现”与后加step 2 E095已实现并存；epoch6为推广var规则，epoch11为served-app instance闭合拒绝。旧P2 export正文保留，但由开头ADR0023修订明确取代，不当作当前P3要求。10月23日连接I/O池不代表已实现并发handler、worker app factory或每worker RNG；该注记仍将其列未完。stock KV可用性、host比较、零影响数字是当时证据，非本次复测。HTTP语义一致排除host管理framing、timing、request间log顺序；KV准入仍需conformance。原文诊断、示例及代码注释逐字节保留。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md)

- **状态**：已接受。维护者于2026-09-27确定方向（下列四支柱）。本记录明确方向、提供证据及实现顺序。目前均未实现。证据迫使的细化在出现处标**[refined]**，并汇总于§9。
- **2026-09-28修订**：§5.3“哪种export形状”（P2 `incoming-handler@0.2`）及step 7/9顺序由[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)取代：stock server导出 `wasi:http/handler@0.3.0`。P3需要 `-W component-model-more-async-builtins` 的P2前提来自probe的同步stream builtin。async-builtin P3 handler在wasmtime 47无需flag即可serve（ADR-0023 §2.1、§3.3）。
- **step 2已实现（#2698）**：§5.2的instance闭合规则是**E095**（`docs/diagnostics/E095.md`），在 `crates/almide-frontend/src/concurrent_reach.rs` 的concurrent-slot发现过程中检查，所以经过wrapper参数传递的app在wrapper调用方被检查（§3.1）。hint指出top-level `let` 与handler-local值；§5.2的kv条款随 `kv` module加入（step 3）。`spec/serve_cross/http_serve_replay.almd` 抽取 `/boot`，在handler内读时钟。拒绝属于dialect epoch 11（`served-app-is-instance-closed`）。
- **日期**：2026-09-27
- **范围**：`http.serve`、`http` router family、`fan` body、新 `kv` stdlib module、server的 `almide build --target wasm` 形状及对应契约台账行（C-367与新行）。
- **相关**：#2665（native serve顺序运行）、#2659（server的stock wasm artifact）、#2664（structural路径effect fn值，前提PR #2693）、#2692（SIGTERM丢stdout）、#2588（router，已关闭；#2665/#2659由其拆出）、#2594（`fan.map` effect回调顺序运行）、#1696（退役旧wasm emitter）、[ADR-0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md)（执行基础是自由变量）、[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) §D6（effect是fallible）。
- **证据基础**：`../almide-references/RESEARCH-http-serve-concurrency.md`（2026-09-27测量23配置M1–M23，阅读18系统源码S1–S18；probe和原始输出在 `../almide-references/research-2026/http-serve-concurrency/`）、§4引用的wasi:keyvalue、wasmtime、Spin、Workers KV、Durable Objects一手资料阅读、#2659原型（branch `stock-wasm-serve-2659`、`research/spike/p3-http-handler-probe/`）、§6迁移测量（`research/spike/concurrent-var-reach/reach.py`）。

<a id="1-context"></a>

## 1. 背景

<a id="11-one-handler-four-answers"></a>

### 1.1 一个handler，四种回答

probe handler在语言存放可变状态的位置保存counter并递增，`/slow` 时sleep 2 s，然后再次读counter。driver先发 `/warm`，再t=0发 `/slow`，t=0.3 s发 `/fast`，之后顺序发三次 `/seq`。

| 可观测回答 | `/slow` n→after | `/fast` latency | 测量位置 |
|---|---|---|---|
| 共享、顺序 | 2→2 | **1.70 s** | Almide 0.64.0 native，captured及top-level `var`（M1、M2）；embedded wasm（M3）；Node busy-wait（M5）；Python `HTTPServer`（M9）；asyncio + `time.sleep`（M12）；Rust std无线程（M17） |
| 共享、并发 | 2→**6** | 0.00 s | Node `await`（M4）；Go `net/http`（M6，`-race` 在runtime报告 `DATA RACE`，M7）；Python threading（M8）；asyncio（M11）；Rust `Arc<Mutex>`（M16）；axum（M19、M20） |
| 隔离 | **1→1**，每request都见 `n=1` | 0.00 s | Python `ForkingMixIn`（M10）；Rust `thread_local!` + 每连接一线程（M14，恰是Almide top-level `var` lowering）；每request新instance的 `wasmtime serve` P2、Spin P2（S12、S13） |
| 按worker拆分 | 1, 1, 2, 2，取决于运行request的worker | 0.00 s | Rust `thread_local!` + 4线程池（M15）；wado `serve`（S9）；Actix（S16）；Workers global（S11） |

Almide自身host已覆盖三行：

- native与embedded（`almide run --target wasm`，C-367）顺序运行；
- P2 component的 `wasmtime serve` 隔离；
- P3 component的 `wasmtime serve` 复用一个instance，最多128 request、同时16个（S12；#2659原型实测wasmtime 47.0.2的guest counter为1, 2, 3）；
- Workers“包括并发request”复用isolate，无routing保证（S11）。

**只有不能观测可变状态的handler在全部host给出同一回答。** `var` 中counter在Almide自身target之间也不可移植。

<a id="12-what-the-close-peers-do"></a>

### 1.2 相近语言的做法

41系统没有一个依据handler body触及什么来改变serve模式（研究§4.3）。与Almide相似的语言要么无法表达共享可变状态，要么将其作为编译错误：

- **不可表达**：Gleam（S1）、Erlang/Elixir（S2）、Roc（S4：“持久可变状态应放SQLite或外部service”）、aver（S7：“设计上无共享可变状态”）、Koka（S5：local `var` 不可逃逸）；
- **编译错误**：Rust（将 `Rc<Cell>` 放入线程产生E0277，M13；axum `State<Rc<Cell>>` 也如此，M18）、Swift 6（`mutation of captured var 'counter'
  in concurrently-executing code`，M21；`main actor-isolated var ... can not be
  mutated from a nonisolated context`，M22）。

Rust的 `Send` 检查**抓不到**Almide top-level `var` 形状（`thread_local!`，M14/M15）：它可编译，却静默拆分状态。

<a id="13-what-almide-does-today"></a>

### 1.3 当时Almide的做法

- `fan { … }` 用E008拒绝captured local `var`（研究probe `raw/almide-fan-var.txt`）。E008不达top-level var或经被调用fn到达的var。
- `fan.map` 回调若可达 `var`（captured、top-level、通过被调用 `effect fn`），**被接受并顺序运行**。lowering只对带Send-safe标量capture的pure lambda literal并行（`runtime/rs/src/fan.rs:23-40`，#2044）。这是内容驱动切换，只因两版本同结果才安全。#2594就是代价：effect回调总顺序运行。
- native `http.serve` 一次回应一个连接（`runtime/rs/src/http.rs:736-743`），handler是非 `Send` 的 `Rc<dyn Fn>`。

Almide的使命指标是modification survival。最坏结局是编辑时静默行为变化。暂定计划“并发，除非handler达 `var`，则warning并顺序”就是这种变化：加一条 `counter = counter + 1` 即翻转server latency，唯一信号是warning。其他例子不这样做。

<a id="2-decision"></a>

## 2. 决定

四支柱。

1. **传给并发host的函数不可达 `var`，由checker强制。** 覆盖 `http.serve` / `http.route` / `http.mount` / `http.wrap` handler/middleware及每个 `fan` body。captured、top-level、传递到达的var全部拒绝（§3）。规则推广E008并保留其code。effect（IO、random、clock、http client）继续允许。堵住 `fan.map` 静默顺序执行缺口。
2. **跨request状态只在显式host无关KV store中**（§4）：`kv` module对pure `f` 提供atomic `update(key, f)`，每key线性化。native用进程内带锁map，embedded用host map。**[refined]** stock wasm host仅通过conformance fixture才可作为backing。当时没有，因此直到出现一个，`almide build --target wasm` 的 `kv` 都是E081（§4.4）。
3. **app是值，entry point是薄host shell**（§5）。`let app = http.router([...])` 是top-level值，`main` 的 `http.serve(port, app!)` 是native/embedded shell。`almide build --target wasm` 将同app导出为 `wasi:http/incoming-handler`，取代#2659 p3-sockets方案。**[refined]** served表达式须对top-level item闭合，不可capture `main` local（§5.2）。export host上 `main` 不是entry point（§5.4）。测app是普通调用：`let h = app!` 后 `h(http.new_request(...))!`。
4. **契约是HTTP语义，而非byte级**（§7）。C-367从“response bytes相同”变为“status、header集合、body相同”。log行atomic，并发request之间顺序未规定。native serve并发：worker pool、每worker一个app instance、每worker RNG重新播种（§5.5）。SIGTERM排空进行中request并flush输出（§5.6）。limit以record提供（§5.7）。

<a id="3-the-checker-rule-pillar-1"></a>

## 3. checker规则（pillar 1）

<a id="31-sites"></a>

### 3.1 site

**concurrent slot**是参数，其实参可与自身或兄弟的其他调用同时运行。stdlib声明：

| 表层 | concurrent slot |
|---|---|
| `http.serve(port, f)` 和 `http.serve_with_limits(port, f, limits)` | `f` |
| `http.route(pattern, handler)` | `handler` |
| `http.mount(prefix, sub)` | `sub` |
| `http.wrap(handler, middleware)` | `handler`，`middleware` 每个元素 |
| `http.router(routes)` | 自己没有；handler经已是site的 `route` / `mount` 进入 |
| `fan { a, b, … }` | 每个分支 |
| `fan.map` / `fan.settle` / `fan.any` / `fan.any_map` / `fan.race` / `fan.timeout` / `fan.bounded` | 每个函数值参数（callback或thunk list） |

声明为参数上的stdlib属性（拼写由实现PR决定，暂名 `@concurrent`），不按函数名称硬编码。**site**是concurrent slot中的一个实参表达式 `E`。

**穿过user wrapper的slot推断。** 沿 `let` 跟踪后，若 `E` 是外层fn参数 `p`，则 `p` 成为该fn的concurrent slot。检查移到该fn每个call site，反复至固定点。例如 `fn serve_on(port: Int, h: HttpHandler) = http.serve(port, h)` 使 `serve_on` 的 `h` 成为concurrent slot。推断slot属于fn interface（`almide compile`），所以跨package边界。

<a id="32-the-executable-closure-of-a-site"></a>

### 3.2 site的可执行闭包

**body**是fn body、lambda body或 `fan` 分支。

若lambda是stdlib函数中*非concurrent slot*回调参数的直接实参，则**折叠**入语法上包含它的body。该函数在返回前调用callback且不保存：`list.map`、`list.fold`、`option.map`、`map.fold` 等。折叠lambda在外层body调用内部运行，其他lambda都有独立body。

**可执行闭包** `X(E)` 是满足以下条件的最小body集合：

1. `E` 内语法上出现的每个未折叠lambda（若 `E` 是 `fan` 分支，还含 `E` 自身）均在 `X`；
2. `X` 中body提及top-level fn `g`，无论调用 `g` 还是用作值，`g` body均在 `X`；
3. `X` 中body或 `E` 自身提及top-level `let` `x`，则 `x` initializer中的未折叠lambda及initializer提及的top-level fn body进入 `X`。由此可达record/list内closure：`let app = http.router([http.route("GET /", h)])` 经 `app` 把 `h` body放入 `X`；
4. 对在提及它的body外绑定的local `let` 同样适用；
5. `X` 内body中语法出现的所有未折叠lambda均在 `X`。它可能被调用，故规则保守。

<a id="33-violation"></a>

### 3.3 违反

若 `X(E)` 中某body `B` 以读或赋值提及 `var` `v`，且 `v` 在 `B` 外声明，则 `E` **可达** `v`。

单一条款覆盖所有形状：

| 形状 | 为何在 `B` 外 |
|---|---|
| `main` 中 `var c` 被handler lambda捕获 | `c` 声明在 `main` body，lambda有自己的body |
| top-level `var` | 在所有body外声明 |
| handler调用的 `effect fn bump()` 内写top-level `var` | `bump` body在 `X`（规则2），var在其外 |
| factory `effect fn mk() -> HttpHandler = { var c = 0; (req) => { c = c + 1 … } }` | 返回lambda未折叠（规则5），有独立body，`c` 在外 |
| `fan` 分支读外层 `var` | 分支有独立body |

不构成违反：

- handler内或它所调用fn内声明的 `var` 按每次调用存在；
- 该fn内部折叠lambda用的 `var`，如 `var c = 0; xs |> list.map((x) => x + c)`，也按调用存在；
- site外用 `let` 拷贝var值，如 `let scale = scale0`，是snapshot而非reach。这正是现有E008 fix-it（`tests/diagnostics/e008-lambda-callback-capture/fixed.almd`）。

effect正交：site可做IO、抽随机数、读时钟、调用http client、使用 `kv`。

规则是带interface summary的package内分析。每top-level fn在compiled interface携两事实：“可达top-level var”（带witness path）及推断concurrent slot。所以暴露触及自package top-level `var` 的effect fn，会让调用它的下游handler失败，witness指明该fn。

<a id="34-diagnostic"></a>

### 3.4 诊断

保留**E008**。title从“fan block captures mutable variable”推广为“并发运行body可达 `var`”。三变体均指出site与witness path：

```
error[E008]: this handler can reach `hits`, a `var` — handlers run concurrently
  --> app.almd:14:3
  in the handler passed to http.route (reached through fn `bump` at app.almd:4)
  hint: a handler cannot share mutable state. Keep state that must outlive a
        request in a kv store: `kv.incr(counts, "hits", 1)!`. For a value fixed
        before serving, use a top-level `let` instead of `var`.
```

```
error[E008]: this fan.map callback can reach `total`, a `var` — fan bodies run concurrently
  hint: return each element's contribution and combine the results after the fan,
        e.g. `fan.map(xs, f)! |> list.sum`.
```

`fan { … }` block措辞保持当时原样：

```
error[E008]: cannot capture mutable variable 'count' inside fan block
  hint: Use a `let` binding instead of `var` for values shared across fan expressions
```

fix-it判定：

- 可达var在程序任何位置都未被赋值时，**mechanical**：改声明为 `let`；
- 否则**conditional**：site前绑定 `let` copy（只读使用），或把状态移入 `kv`。

实现PR重写 `docs/diagnostics/E008.md`。

**与已有code关系。** E011（pure函数closure内修改mutable变量）保留；它使上述factory必须为 `effect fn`，并先于E008执行。新规则涵盖所有当前E008案例：analyzer检出九个 `tests/diagnostics/e008-*` fixture的全部12 broken site，不检出fixed（§6）。所以E008检查从inferrer的 `fan` 分支（`crates/almide-frontend/src/check/infer_calls_closures.rs:215-234`）移至推断后统一site pass。

<a id="35-staging"></a>

### 3.5 阶段

规则拒绝目前可编译的程序。dialect台账标准“先前接受程序被拒”（`proofs/dialect-epochs.toml:13-15`）使其成为epoch。**决定：新dialect epoch 6直接作为错误，不设warning release。** 理由：

1. 测得影响为零个手写程序（§6）。
2. epoch 4/5已有先例：两者在0.63.0各自epoch直接成为error。
3. warning窗口至少会交付一个release，其中并发native serve（§8 step 4）可静默给出M14/M15回答。并发serve**之前**规则必须成为error，因而固定计划顺序。
4. 研究本身判定：warning是LLM编辑循环中最易丢失的信号（研究§6）。

epoch条目的 `breaks` 行写：“http handler或 `fan` body可达的 `var`（captured、top-level、经调用fn）是E008；跨request状态放 `kv`，只读使用绑定 `let` copy”。

<a id="4-the-store-pillar-2"></a>

## 4. store（pillar 2）

<a id="41-what-the-backends-actually-offer-primary-sources"></a>

### 4.1 backend实际提供什么（一手资料）

| Backend | API | 声明保证 | 能承载每key线性化的 `update` 吗 |
|---|---|---|---|
| **wasi:keyvalue**，Phase 2（README.md:7）；main为 `@0.2.0-draft2`，唯一tag是 `v0.2.0-draft` | `store.open(identifier) -> result<bucket, error>`；`bucket` 有 `get`、`set`、`delete`、`exists`、`list-keys(cursor)`。draft2的 `atomics` 加 `resource cas { new(bucket, key); current() }`、`swap(cas, value) -> result<_, cas-error>`、`increment(bucket, key, delta: s64) -> result<s64, error>`。带tag的draft仅 `increment(…, delta: u64) -> u64`。`batch`：get/set/delete-many | store.wit:1“最终一致的KV操作”；store.wit:14-25“足够一致以保证读自己的写…仅适用于同一client”；atomic.wit:3-5“成功完成或什么也没做”；batch.wit:10-11“不保证原子性”。开放PR #56要将store保证弱化为仅最终一致；开放PR #59称同key的 `increment` 与其他操作混用“依实现而定” | 只能经draft2 `cas` loop，且提案从未称“linearizable” |
| **`wasmtime serve` 47**（`-S keyvalue`） | `wasi:keyvalue@0.2.0-draft`（wasmtime `crates/wasi-keyvalue/wit/deps/keyvalue/world.wit:1`），内存backend，仅identifier `""`（lib.rs:164-171） | `open` 返回 `Bucket { in_memory_data: self.ctx.in_memory_data.clone() }`（lib.rs:166-167）：每bucket是预设数据的**私有副本**。`serve` 每instance新建context（`src/commands/serve.rs:443-457`），P2 reuse默认1 | **不能。** 写入下一request不可见，甚至同request第二次 `open` 也不可见，无CAS |
| **Spin 3.x / 4.1** | `wasi:keyvalue@0.2.0-draft2`（`wit/deps/keyvalue-2024-10-17/`）、`spin:key-value@3.0.0` store（无atomics）、旧 `fermyon:spin/key-value`；`"default"` store为SQLite（runtime-config lib.rs:459-503） | 文档未声明一致性或原子性（kv-store-api-guide）。SQLite `increment` 在 `Mutex<Connection>` 下为transactional，存储**8-byte i64 LE**；其他长度key令host panic（`crates/key-value-spin/src/store.rs:326-359`）。CAS `swap` 按值比较（l.376-457），但 `current()` 看到无key时做**无条件upsert**（l.436-446） | 已有key可以。**首次insert不行**：两个竞争creator都“成功” |
| **Cloudflare Workers KV** | `get` / `put` / `delete` / `list` | “KV通过最终一致达到高性能”；修改“可能需60秒或更久才在其他全球网络位置可见”；“不适合需要原子操作的应用”；“同key每秒最多1次写”（developers.cloudflare.com/kv/concepts/how-kv-works/、/kv/api/write-key-value-pairs/） | **不能。** 无increment、CAS，最后写获胜 |
| **Cloudflare Durable Objects** | `ctx.storage.kv.get/put/delete/list`、`transaction`、`transactionSync`、`sql.exec`；用 `idFromName(name)` 路由 | “持久、事务性、强一致storage”；“单线程、协作式多任务”；“一串读接一串写，中间无其他I/O时自动原子”（developers.cloudflare.com/durable-objects/api/sqlite-storage-api/、/concepts/what-are-durable-objects/） | **可以**，但只能经JS Durable Object class，不是wasm host import。Workers也无官方wasi:http component支持（“WASI support is experimental…仅部分syscall实现”，/workers/runtime-apis/webassembly/） |
| **Deno KV**（仅设计参考） | 带versionstamp的 `atomic().check(...).set(...).commit()`；`KvU64` 上的 `sum` | “只有指定versionstamp匹配才commit”；写“总是强一致”（docs.deno.com/deploy/kv/transactions/） | 可以。`versionstamp: null` 检查把“不存在”也当版本，避免Spin首次insert缺口 |

两事实决定设计：

- `wasmtime serve` 根本不能经wasi:keyvalue保留handler状态。
- 两个stock wasi:keyvalue host实现**不同package版本**（`@0.2.0-draft` 对 `@0.2.0-draft2`），import其中一个的component不能在另一个link。

<a id="42-the-almide-surface"></a>

### 4.2 Almide表层

```almide
import kv

/// A handle naming a store. Pure: no IO, so it can be a top-level `let` and
/// captured by any handler.
fn store(name: String) -> KvStore

effect fn get(s: KvStore, key: String) -> String?
effect fn set(s: KvStore, key: String, value: String) -> Unit
effect fn delete(s: KvStore, key: String) -> Unit
/// Atomic read-modify-write, linearizable per key. `f` gets the current value
/// (none if absent) and returns the new one (none deletes the key). The result
/// is the value now stored. `f` is PURE (a plain arrow type), so a backend that
/// retries may call it more than once, and nobody can tell.
effect fn update(s: KvStore, key: String, f: (String?) -> String?) -> String?
/// `update` over the decimal text of an Int, defined in Almide over `update`
/// and never mapped to a host `increment` (the encodings differ between hosts,
/// §4.1). A missing key counts as 0; a non-Int value is err and is left
/// unchanged; the arithmetic wraps like `+`.
effect fn incr(s: KvStore, key: String, delta: Int) -> Int
```

每操作均effect且fallible（ADR-0002 §D6），调用者写 `!`。错误把wasi:keyvalue的 `error`（`no-such-store`、`access-denied`、`other(string)`）映射为 `err` 字符串。

**[refined] 命名。** module为 `kv`，handle类型 `KvStore`，不用 `store` / `Store`，有两项测得理由：

- 同名local会静默遮蔽module（`import json` 下 `let json = "x"` 仅E060即可过 `almide check`），而 `store` 是常见local名；
- `http` 已避免与user类型冲突的裸nominal名（`stdlib/http.almd:4-6`：“不命名为 `Request`/`Response`”）。

合规stock host须提供如下wasi:keyvalue draft2子集WIT形状。Almide shell将 `update` 映射为其上的CAS loop。

```wit
// imported from wasi:keyvalue@0.2.0-draft2
store.open: func(identifier: string) -> result<bucket, error>;
bucket.get: func(key: string) -> result<option<list<u8>>, error>;
bucket.set: func(key: string, value: list<u8>) -> result<_, error>;
bucket.delete: func(key: string) -> result<_, error>;
atomics.cas.new: static func(bucket: borrow<bucket>, key: string) -> result<cas, error>;
atomics.cas.current: func() -> result<option<list<u8>>, error>;
atomics.swap: func(cas: cas, value: list<u8>) -> result<_, cas-error>;
```

有意排除两项：

- `increment`：编码由host定义（wasmtime十进文本，Spin 8-byte LE），与 `get` 混用依实现（PR #59）；
- `list-keys`、batch、TTL：正确counter/session/cache均不需要（下文）。`list-keys` “MAY展示过时列表”（store.wit），batch“不保证原子性”，TTL仅上述一个backend有。添加任一都另立记录。

<a id="43-why-this-is-the-minimal-set"></a>

### 4.3 为何这是最小集合

- **counter**：`kv.incr(hits, "total", 1)!`。线性化，所以N并发request最终恰为N。
- **session**：login用 `kv.set(sessions, id, data)!`，各request用 `kv.get(sessions, id)!`，logout用 `kv.delete(sessions, id)!`。每key同时单writer，不需要update。
- **cache**：在 `f` 外计算值（effect留在外部），然后 `kv.update(cache, url, (cur) => some(cur ?? fetched))!`。首个writer胜出，竞争request都返回存储值。

完整program及test：

```almide
import http
import kv

let counts = kv.store("counts")

effect fn hit(req: HttpRequest) -> HttpResponse = {
  let n = kv.incr(counts, "hits", 1)!
  http.response(200, "n=${n}")
}

let app = http.router([http.route("GET /hit", hit)])

effect fn main() -> Unit = http.serve(8080, app!)!

test "two hits count to two" {
  let h = app!
  let _ = h(http.new_request("GET", "/hit", "", map.new()))!
  let r = h(http.new_request("GET", "/hit", "", map.new()))!
  assert_eq(http.body(r), "n=2")
}
```

<a id="44-per-backing-semantics"></a>

### 4.4 各backing语义

| 程序运行位置 | Backing | 一致性 | 持久性 | 可见性 |
|---|---|---|---|---|
| native（`almide run`、构建binary） | process-global分片map，每shard一个 `Mutex`；`f` 在其key shard锁下恰运行一次 | 所有操作每key线性化 | process生命周期，exit即消失 | 所有worker thread立即可见 |
| embedded（`almide run --target wasm`） | almide host process内map，经新host op访问 | 同上 | 同上 | 同上 |
| 两路径的 `almide test` | **每个 `test` block**新建空map，所有name从空开始 | 同上 | block | 仅该block；block内用 `http.new_request` 调用的app看见该block map |
| stock wasm（`almide build --target wasm`） | **[refined]** 通过conformance fixture的host之wasi:keyvalue draft2（并发首次write竞争、CAS retry竞争、跨instance持久） | 合规host上每key线性化 | host决定（Spin默认SQLite文件持久） | host决定 |

**当时stock可用性：没有通过者。**

- wasmtime serve 47不跨request或 `open` 保留状态，也无CAS。
- Spin有首次insert缺口。
- Workers无wasi:keyvalue，其KV不原子。

所以 `proofs/target-availability.toml` 中 `kv.*` 在**stock-p1和component路径为E081**。理由指明本记录与缺失合规性。stateless app到处可export；stateful app在native/embedded运行，某host通过后才可export。这是“每host正确”的“正确或拒绝”解读。其他解读（`wasmtime serve` counter每request静默重置，Workers KV丢update）是pillar 1要排除的M10/M15失败模式。

<a id="5-the-app-the-hosts-and-main-pillar-3"></a>

## 5. app、host和 `main`（pillar 3）

<a id="51-one-program-every-host"></a>

### 5.1 一个程序，所有host

```almide
let app = http.router([ http.route("GET /users/{id}", get_user), … ])   // Result[HttpHandler, String]
effect fn main() -> Unit = http.serve(8080, app!)!                        // the socket hosts' shell
```

top-level `let` initializer不能调用effect fn（实测 `let port = env.millis()` 产生E006），因此 `app` 是程序文本的pure函数，每worker一次或每instance一次求值不可观测。Rust lowering已将持 `Rc` 值的top-level `let` 变为**每thread lazy slot**。`let app = http.router(...)` 生成代码是 `__AlmideTl_APP` 后的 `thread_local! { static SLOT … = Box::leak(Box::new(almide_rt_http_router(...))) }`；`String` 的 `let` 则为共享 `LazyLock`（0.64.0用 `--target rust` 实测）。所以每worker app instance不新增codegen成本。

`router` 返回 `Result`，top-level initializer不许 `!`（实测E022）。因此shell写 `app!`：

- native/embedded在bind前求值一次，错误route表如当时一样在listen前exit 1；
- export host在instance初始化求值，`err` 令初始化失败，message到stderr。

<a id="52-refined-the-served-expression-is-closed-over-top-level-items"></a>

### 5.2 **[refined]** served表达式对top-level item闭合

`http.serve` / `http.serve_with_limits` 的实参须**instance-closed**：其可执行闭包（§3.2）读取而其中任何body都不绑定的name，均须top-level item（fn、`let`、type、constructor）。export host无main local（§5.4），native worker thread也构建独立instance（§5.5）。实现PR分配的新code（E095，#2698）为：

```
error[E095]: the app passed to http.serve captures `boot`, a local of main
  hint: the app runs in several instances — one per worker natively, one per
        request on a wasi:http host — and main's locals do not exist there.
        Make `boot` a top-level `let`, compute it inside the handler, or keep it
        in a kv store.
```

这退役了instance host都无法满足的C-367承诺：“main在 `serve` 前计算的值（随机抽样、读时钟）在整个run的每request相同”。研究将其称为未决P2 divergence（研究§6，“Out of scope for F”）。fixture（`spec/serve_cross/http_serve_replay.almd:52-56`、`boot`）迁移为handler-local值，或 `kv.update(meta, "boot", (b) => some(b ?? drawn))!`。

<a id="53-hosts-and-what-the-program-can-observe"></a>

### 5.3 host与程序可观测内容

| Host | instance model | 程序可观测（pillar 1–2成立） |
|---|---|---|
| native（`almide run`、binary） | 一个process，worker thread池，每个独立app instance，同时最多 `max_in_flight`（§5.5） | response、`kv`、request间未规定顺序的完整log行、timing |
| embedded（`almide run --target wasm`） | 一个instance，按request完整到达顺序串行；host用native池同时读最多64连接（C-367、C-389、#4021） | 相同回答；慢handler延迟下一request（仅timing），慢client不延迟 |
| `wasmtime serve` 47，P2 component（`wasi:http/incoming-handler@0.2.x`，wasmtime 47带 `wasi:http@0.2.12`） | 每request新instance，从不并发（P2 `--max-instance-reuse-count` 默认1，“WASIp2 component不可并发调用，所以设大于1无效”，`serve --help`，serve.rs:141-155） | 相同回答；`kv` 为E081（§4.4）；hyper写head（§7） |
| `wasmtime serve` 47，P3 component（`wasi:http/handler@0.3.0`） | 最多复用128次，每instance最多16并发，idle 1 s后drop；需 `-W component-model-more-async-builtins -S p3 -S http`（#2659原型实测无 `-W` 不load） | 若guest shell串行化instance内handler，则相同回答（下文） |
| Spin 3.x / 4.x | P2每request新instance；3.6 P3“并发或串行”复用（spinframework.dev/blog/announcing-spin-3-6） | 相同回答，此处未测量 |
| Cloudflare Workers | 无官方component/wasi:http支持，只有未验证的非官方路线（`jco transpile` + JS fetch handler） | 不属本记录artifact范围 |

**export形状。** stock artifact导出P2的 **`wasi:http/incoming-handler@0.2`**，不是P3 handler。

- P2在固定wasmtime无experimental flag即可load。
- P2是guest从不交错的instance model。
- P2是方向指定的interface名称。

P3（`wasi:http/handler@0.3.0`）留待后续（§8 step 9），等wasmtime无需 `-W` 即提供。P3下每当handler在stream builtin阻塞，最多16handler会在同一linear memory及RC heap交错（#2659原型）。pillar 1让*程序*状态不可共享，但*runtime* allocator、lazy-`let` slot、scratch buffer尚未证明可跨挂起点重入。所以P3 shell**串行化instance内handler**（带backpressure的guest queue），直到gate证明每挂起点可重入。两选项均未测量；选串行shell因它在每host正确，而 `--max-instance-concurrent-reuse-count 1` 只是其他host不遵守的wasmtime flag。

<a id="54-what-main-means"></a>

### 5.4 `main` 的含义

- **socket host**（native、embedded）：`main` 如当时运行一次。`http.serve` bind并serve至shutdown signal（§5.6），随后**返回** `()`，继续后续语句，exit code由main决定。main中 `serve` 前语句运行一次，但因§5.2不能影响app。
- **export host**：`main` **不是**entry point。component导出 `incoming-handler.handle`，按instance惰性求值app，把 `incoming-request` 转成 `HttpRequest`，调用app、写response。
- **export build形状规则。** `almide build --target wasm` 导出的程序必须有 `main`，body恰为一个call `http.serve[_with_limits](port, APP[, limits])`，可在前面放仅用于 `port` / `limits` 的 `let`。export host不求这些参数：host拥有socket和自己的limit。任何其他 `main` 语句在 `--target wasm` 都是check-time error（实现PR分配code）。fix-it：“此代码在native运行但wasi:http host不运行；setup移入top-level `let` 或handler，删除readiness行（host自己打印）”。这是新表层，不拒绝当时可build程序：此前stock build的 `http.serve` 是E081。

未选择“main的serve前effect在每instance init运行并必须幂等”。幂等无法检查，instance数由host选择，该prologue的 `kv` write在P2会每request可观测地重复。

<a id="55-native-concurrency-closes-2665"></a>

### 5.5 native并发（关闭#2665）

pillar 1–3成立后最便宜的正确模型：

- **accept thread与worker thread池。** worker按需启动至 `max_in_flight`，每个接受的 `Send` 类型 `TcpStream` 交给空闲worker。全忙则停accept，以kernel backlog背压。
- **每worker构建自身app instance。** compiler把instance-closed served表达式（§5.2）lowering为普通 `fn() -> Rc<dyn Fn(…)>` **factory**，fn pointer所以 `Send`。worker调用factory一次，经现有每thread slot读top-level let。main先自身求值一次，`err` 则bind前exit。
- **`kv` process-global**（§4.4）。值为拥有所有权且 `Send` 的 `String`；无Almide `Rc` 跨thread。
- **stdout变为process-global。** 当时 `ALMIDE_STDOUT_BUF` 是 `thread_local!` 64 KiB `BufWriter`（`crates/almide-codegen/src/lib.rs:346-359`），exit只flush main thread副本，会丢worker输出。改为 `Mutex` 后一个buffer，每次 `print` / `println` 加锁追加字节，故**一行原子**，terminal-flush及64 KiB规则保留。stderr仍无缓冲，每 `eprintln` 是Rust stderr锁下一次 `write`。
- **每worker RNG重新播种。** 当时xorshift状态为 `thread_local!`，seed来自 `SystemTime` 纳秒（`runtime/rs/src/random.rs:6-10`），同tick启动的两个worker会抽同一流。每worker seed混OS entropy（有 `getrandom` 则用，否则clock）、worker index、thread id。随机性不承诺可复现，只规定不同：两个活worker不共享seed。
- **拒绝fork**，虽无需 `Send`：进程内 `kv` map会变成每child一份，未flush父stdout复制到每child，RNG fork后仍同流（研究§4.2）。
- **拒绝 `Send` 值表示（全面Rc→Arc）**，因整个runtime成本（研究§5选项C）。

embedded保持顺序，答案同native，只有新C-367不承诺的timing不同。

<a id="56-shutdown-closes-2692"></a>

### 5.6 shutdown（关闭#2692）

首次SIGTERM或SIGINT，socket host：

1. 停止accept；
2. 在 `request_timeout_ms` 上限内让进行中request完成；
3. flush stdout；
4. 从 `http.serve` 返回。

drain中第二信号跳过等待，host flush并exit code 1。embedded在host同样处理，guest serve loop返回。exit code仍在C-350 `0..=125` 内，无128+signal code。

`almide run` 将这些信号恰送程序一次（#2809）。Unix native launcher `exec` 编译程序，supervisor持有的pid即program pid；仅向它SIGTERM就启动drain，terminal group的Ctrl-C不会因转发父进程重复而强停。`almide run` 的退出status为程序自己的：exit code或报告为signal致死。launcher不自行加128+n，C-350将其保留给 `process.exit`。Windows launcher等待，Ctrl-C到console每process，因此不转发，自身忽略事件直到程序退出。

<a id="57-limits"></a>

### 5.7 limit

```almide
type ServeLimits = { max_in_flight: Int, request_timeout_ms: Int, max_body_bytes: Int }
effect fn serve_with_limits(port: Int, f: effect (HttpRequest) -> HttpResponse, limits: ServeLimits) -> Unit
```

名称沿 `*_with_limits` family（`request_stream_with_limits`、`HttpLimits`）。`http.serve(port, f)` 默认64进行中、30 s request timeout、1 MiB body。

| Field | native | embedded | export host |
|---|---|---|---|
| `max_body_bytes` | 共享server core强制：超body为 `413 Payload Too Large`，不调用handler | 同上 | 同上，guest shell读body stream时强制 |
| `request_timeout_ms` | 强制：超后 `503`、连接关闭、handler结果丢弃 | 同上 | host自己的 |
| `max_in_flight` | 强制为pool大小 | 路径顺序，所以为1 | host自己的 |

export host的limit属host配置（如 `wasmtime serve
--max-concurrent-requests`），是timing和准入，不是已接受request的答案。

**状态（2026-09-28，#2823）。** record出现前，共享server core（`crates/almide-rt-core/src/http_server_core.rs`）已强制默认值，使native/embedded均拒绝#2823攻击：无论 `Content-Length` 声明还是chunked到达，超1 MiB body都不按声明大小allocate而回 `413`，不调handler。未达 `Content-Length` 即截断body为 `400`。accept后30 s未读完request为 `503`，晚于此的handler response被 `503` 替换。core也限制request/header行（8 KiB：`414` / `431`）、header数（100：`431`），decode chunked request body，对其他transfer coding回 `501`。`ServeLimits` 与 `serve_with_limits` 随worker pool（#2665）落地，把值传至 `http_server_next_with`。

**状态（2026-10-23，#4021，C-389）。** 连接I/O在共享core有界池（`http_server_pool`）运行：accept thread接最多64连接，64时停accept（kernel backlog背压）；各连接worker读request、写response。request head须accept后10 s完整到达，否则 `408`（完全未发送client无回答关闭）；body和handler等待仍受30 s request timeout及其 `503` 约束。handler仍在调用 `serve` 的thread上按request完整到达顺序逐个运行；§5.5每worker app factory、process-global stdout、每worker RNG seed仍属#2665。所以慢client不再拖停任一socket host，慢handler仍会。

**监听地址（#2826、#2829）。** `http.serve` 保持bind `0.0.0.0:<port>`。C-367声明该地址，部署container/VM的server需从host外访问。静默改loopback会让server无报错地不可达。地址成为显式选项：`ServeLimits` 的 `host: String` field，默认 `"0.0.0.0"`，与 `serve_with_limits`（#2829）一起加入。

<a id="6-migration-measured-impact"></a>

## 6. 迁移：测得影响

`research/spike/concurrent-var-reach/reach.py` 在 `almide --emit-ast` 上实现§3.1–3.3（含escape和let-snapshot条款），2026-09-27用almide 0.64.0运行。

先验证分析器：

- 检出全部var可达研究probe：4个 `serve` probe（captured、top-level，native/wasm版本）及5个 `fan` probe（block、captured、只读captured、top-level、经 `bump`）；
- 检出factory probe（`mk()` 返回捕获自身 `var` 的closure）；
- 放行带fn-local var的 `list.map` probe；
- 对 `tests/diagnostics/e008-*` 恰检出12 broken site，不检出fixed。

| Corpus（commit） | `.almd` 文件 | 有site文件 | site | 可达 `var` 的site |
|---|---|---|---|---|
| `spec/`（develop b0d48b31b） | 1,446 | 58 | 282（`fan` 分支55、`fan.map` 122、`fan.any` 56、`fan.settle` 18、`http.serve` 2、`http.route` 21、`http.mount` 2、`http.wrap` 6） | **0** |
| `stdlib/` | 317 | 6 | 0；stdlib**没有top-level `var`**，所以无stdlib fn携传递reach | **0** |
| `research/benchmark/exercises/` | 25 | 0 | 0 | 0 |
| almai（316db10） | 50 | 0 | 0 | 0 |
| comide（fb9d654） | 20 | 0 | 0 | 0 |
| golemide（1f7c7f7） | 26 | 0 | 0 | 0 |
| gramide-cli（5f67679） | 1 | 1 | 调依赖package的8个 `fan` 分支；7个gramide依赖无top-level `var`（`^var ` 仅匹配Go/JS源码的heredoc test string内部） | **0** |
| almide-dojo bank：`tasks/`、`msr/`、`src/`（f58aaa2） | 123 | 1 | 0 | 0 |
| 其他almide-org repo（playground 123、porta 36、bindgen 24、docs 16、web 6、lander 4、sqlite 3、js 1） | 213 | 0 | 0 | 0 |
| `tests/diagnostics/`（negative fixture） | — | 99 | 69 | 12，均在已有 `e008-*` broken fixture，当时已是error |

**var规则破坏零个手写程序。**

**instance闭合规则（§5.2）**破坏两个 `http.serve` site之一：`spec/serve_cross/http_serve_replay.almd` 的 `boot` capture正是退役C-367承诺。docs全部serve示例（`docs/stdlib/http.md` ×7、`docs/CHEATSHEET.md:351`）已经闭合。

**export形状规则**不破坏任何程序，因为当时stock build拒绝 `http.serve`。

所以按§3.5，epoch 6直接error，不设warning窗口。

<a id="7-contract-changes-pillar-4"></a>

## 7. 契约变更（pillar 4）

以下是提案，每项先落入almide/als，再推进pin，然后实现（CLAUDE.md，Behavior Contracts）。ID在那里分配。

<a id="71-c-367-rewritten-http-semantic-identity-across-every-serving-host"></a>

### 7.1 C-367重写：所有serving host的HTTP语义一致

> `http.serve(port, app)` 对每个已接受request，在native、embedded wasm（`almide run --target wasm`）以及 `almide build --target wasm` 接受程序的 `wasi:http/incoming-handler` host上，给出相同**status code、header集合、body**。**header集合**为response的(name, value)对，name按ASCII大小写无关比较。不同name field无序，同name保留相对顺序（RFC 9110 §5.3）。排除host管理field：`date`、`connection`、`keep-alive`、`transfer-encoding`、`content-length`（framing；body字节去framing后比较）。**reason phrase非契约**：HTTP/2、HTTP/3无此项（RFC 9113 §8.3.2、RFC 9114 §4.3.2）；hyper写canonical phrase，native core用自己的表（#2659原型测得native `418 OK`，`wasmtime serve` 为 `418 I'm a teapot`）。request reader（`req_method`、含query的 `req_path`、`req_body`、`req_header`（首匹配、ASCII大小写无关）、`query_params`、`param`）在每host给同值。handler `err(m)` 为 `500`，body `Internal error: <m>`，`Content-Type:
> text/plain`。socket host bind失败以 `Error: bind failed:
> <os message>` 和exit 1 abort。app每instance求值（native每worker thread，export host每 `wasi:http` instance）。因instance-closed且不可达 `var`，instance数不可观测。不承诺timing及并发request被serve顺序。不覆盖：HTTP framing、连接复用（host拥有），以及stock host的 `kv` store（见store契约）。

fixture保留replay script，去掉 `boot`，按header集合规则比较三路径；§8 step 7后加入 `wasmtime serve`。

<a id="72-new-rows-ids-allocated-in-als"></a>

### 7.2 新行（ID在als分配）

- **serve并发与log。** “native `http.serve` 并发serve最多 `max_in_flight` request，所以sleep handler不延迟期间到达的request。各 `print` / `println` / `eprintln` call字节连续到达stream。不同进行中request的行顺序未规定，同request保留程序顺序。两个worker thread不共享RNG seed。”证据：2 s `/slow` 及0.3 s后发送 `/fast` 的loopback fixture（`/fast` 先于 `/slow` 回答），多writer行完整性fixture。（ADR-0011 D1要求 `fan` 分支输出等同顺序求值，其程序定义参考顺序为分支顺序。并发request无此顺序，到达顺序由网络作为输入选择，所以serve log诚实地未规定，而非transactional。）
- **kv store。** “`kv.update(s, k, f)` 在native/embedded每key线性化：N并发 `kv.incr(s, k, 1)` 后为 `N`。任意worker完成 `set` / `update` 后的 `get` 返回该值或更晚值。store存续于process，每个 `test` block从全部store为空开始。在stock `--target wasm` build上，host通过conformance fixture前，`kv` 在check时被拒。”证据：64writer increment fixture（native/embedded）、每test隔离fixture、E081可用性行。
- **shutdown。** “SIGTERM/SIGINT时socket host停止accept，在 `request_timeout_ms` 内完成进行中request，flush stdout，从 `http.serve` 返回。第二信号flush并exit 1。”证据：fixture打印、接收request，在stdout重定向时收到SIGTERM并确认全部行（#2692复现）。
- **limit。** “超 `max_body_bytes` 的body为 `413`，不调handler；超 `request_timeout_ms` 的request为 `503`。”证据：native/embedded两个fixture。
- **export artifact。** “main为单serve call的serve程序经 `almide build --target wasm` 输出导出 `wasi:http/incoming-handler@0.2` 的component；不运行 `main`；app每instance求值。”证据：固定 `wasmtime serve` 下C-367 replay。

<a id="8-implementation-plan"></a>

## 8. 实现计划

每step一个带gate的PR，按下列顺序。step 1/2须早于4（§3.5）；step 0是已发现前提。

各step issue：0a #2664、0b #2696、1 #2697、2 #2698、3 #2699、4 #2665、5 #2692、6 #2700、7 #2659、8 #2701、9 #2702。跟踪issue #2705。

| # | Step | Gate | 关闭/相关 |
|---|---|---|---|
| 0a | structural wasm经声明fn类型提供effect fn值 | `proofs/target-availability.toml` router行去掉structural wall | #2664（进行中PR #2693） |
| 0b | 让 `http.router` 加handler call在wasm运行：当时即使无 `serve` 也**两路径**E082（旧：“match over an UNTRACKED subject …”；structural：`ty-mismatch:Fn`，#2659原型 `call:__parts`、`stdlib/http.almd:410`）。仅修**structural**路径 | `spec/stdlib/http_router_test.almd` 中 `h(http.new_request(...))!`（用 `let h = app!`）在native与 `--target wasm` 字节一致运行 | #2696，7及wasm“调用app即可测试”的前提 |
| 1 | 推广E008（§3）：concurrent-slot属性、slot推断、可执行闭包pass、escape/snapshot条款、witness path、interface summary；`CURRENT_DIALECT = 6` 的epoch 6 | §3.3各形状一组 `tests/diagnostics/` broken/fixed（captured、top-level、经fn、经local `let`、经top-level `let`、经record/list、factory escape、推断wrapper slot、`fan.map`、`fan.settle`），折叠 `list.map` 的fixed对，`scripts/check-dialect-epochs.sh`，`spec/` reach.py维持0 | 堵住 `fan.map` 静默顺序缺口，解除#2594障碍（effect回调可并行，另行决定） |
| 2 | instance闭合规则（§5.2）及code；`spec/serve_cross/http_serve_replay.almd` 移除 `boot`。**此前**：新C-367措辞（§7.1）的als PR与pin推进 | diagnostics对；两路径header集合比较的replay green | 4/7前提 |
| 3 | `kv` module（§4.2）、native backing、每test新store、interp bridge（3-way oracle）、embedded host op、stock E081行。**此前**：als store契约 | native/embedded64writer fixture、每test隔离fixture、availability ratchet | pillar 2 |
| 4 | native并发serve（§5.5）：accept thread/worker pool、serve参数factory lowering、global stdout、每worker RNG seed、`serve_with_limits`、`ServeLimits`（§5.7）。**此前**：als并发/limit契约 | slow/fast loopback（#2665完成条件）、行完整性、413/503 fixture、非serve bench性能比不变 | **关闭#2665** |
| 5 | native/embedded shutdown（§5.6）。**此前**：als shutdown契约 | 两路径#2692复现fixture | **关闭#2692** |
| 6 | `tests/http_serve_cross_test.rs` 的HTTP语义比较（header集合、去framing） | 新C-367下native ⇄ embedded replay | 7前提 |
| 7 | `almide build --target wasm` 对serve程序导出 `wasi:http/incoming-handler@0.2`（§5.3–5.4）：export形状规则及code；component shell（request/response转换、`max_body_bytes`、每instance app init）。基于**structural**及component路径（`crates/almide-wasm-run/src/wasi_p2.rs`），绝不用旧emitter。**此前**：als export契约 | 固定 `wasmtime serve` 下C-367 replay（P2，无 `-W`）、形状诊断对fixture、`http.serve` stock行从E081翻转 | **关闭#2659**（取代p3-sockets方案） |
| 8 | 文档表层：CHEATSHEET / llms.txt / `docs/stdlib/http.md` / `docs/diagnostics/E008.md`，§4.3程序带 `almide check` fence | `scripts/check-llm-surface.sh` | 仅1–7之后，本记录不改该表层 |
| 9 | （以后）wasmtime无 `-W` 可serve时，P3 `wasi:http/handler@0.3.0` export及guest每instance串行化（§5.3）；在首个合规host用wasi:keyvalue draft2绑定stock `kv` | P3 replay、conformance fixture | #2659后续 |

**与#1696（退役旧wasm emitter）的关系。** 不添加旧code：

- step 0b/7明确在structural与component路径；
- embedded `kv` host op（step 3）在 `almide-wasm-run` host侧，加structural lowering；
- 当前旧router服务只是暂时措施，step 0b使其不再必要，减少保留旧emitter理由。

`proofs/target-availability.toml` 所有新行都指明structural路径。

**附带发现，登记为bug #2703/#2704**（0.64.0、2026-09-27验证）：

- **embedded时钟。** 调用 `http.serve` 的**任何**程序中读时钟（`datetime.now()` / `env.millis()`），在 `almide run --target wasm` 都为E082。不限handler，`main` 内 `serve` 之前同样拒绝。structural阻挡 `call:datetime.now` / `call:env.millis`，旧emitter阻挡 `http.serve`。无 `serve` 时同一clock read能运行。这细化了研究将位置定为handler的旁证。
- **effect-slot lambda中intrinsic effect fn的 `!`。** 对按 `effect (…) -> …` slot检查的lambda（`http.serve` handler或 `let f: effect (Int) -> Int = …`），内部 `random.int(0, 999)!` / `env.millis()!` 是**E022**。同位置 `fs.read_text(p)!`（返回 `Result`）及user `effect fn … -> Int` 可接受，effect fn body里的 `random.int(0, 9)!` 也接受。违背 `stdlib/http.almd:19-25`（“effect-fn body ergonomics（`!` 传播…）”）及CHEATSHEET:342-354。

<a id="9-refinements-the-evidence-forced"></a>

## 9. 证据迫使的细化

1. **“stock wasm用host KV”变为“合规host KV，当前无合规者”。**
   - `wasmtime serve` 47每次 `open` clone bucket，每instance新context（§4.1）。
   - Spin CAS首次insert无条件upsert。
   - Workers KV最终一致，无atomics。
   - 因而stock build的 `kv` 为E081，直到有host通过fixture。
2. **served app必须instance-closed。** handler不能捕获main任何local，不只是var。export host不运行 `main`，native worker构建独立instance。退役C-367“main在 `serve` 前计算的值在全部request相同”，即其自身fixture的 `boot`。
3. **export host的 `main` 非entry point。** export build接受单serve call的 `main`。拒绝幂等prologue，因为幂等不可检查且 `kv` 写入在P2每request重复。
4. **stock export是P2 `incoming-handler@0.2`，非P3。** 固定wasmtime P3需 `-W component-model-more-async-builtins`，一个heap最多交错16handler。P3 shell到来时将每instance串行化。
5. **规则需要escape与snapshot条款。**
   - 返回捕获自身 `var` 的closure的factory fn共享该var；E011使factory必须为 `effect fn`，却不阻止它。
   - var值的 `let` copy不是reach。无此条款analyzer会报E008自身 `fixed.almd`。
6. **native模型是每thread app instance的worker pool，不是fork。**
   - top-level `Rc` let现有每thread lazy slot使instance几乎免费。
   - fork拆分 `kv` 并复制未flush stdout。
   - thread-local stdout必须global化，否则丢worker输出。
7. **名称。** module `kv`，类型 `KvStore`（§4.2）。limit entry point `serve_with_limits` 遵从已有 `*_with_limits` family。
8. **export artifact新增两前提**：除#2664，router call当时在两wasm路径都被wall（0b）。

<a id="10-alternatives"></a>

## 10. 替代方案

- **handler达 `var` 则warning并顺序serve**（研究方案E），暂定计划。
  - 测量或源码系统无一按handler内容切serve模式（§1.2）。
  - Almide唯一先例 `fan.map` 仅因双版本同结果才安全；`serve` 切换改变latency，正是#2665：1.70 s对0.00 s（M1对M6/M16/M19）。
  - trigger是LLM“只加request counter”时一行，唯一信号warning。
- **每request隔离**（fork/deep copy，研究B）。
  - counter永远静默回答1（M10、M14），embedded无法匹配（2..6对1）。
  - fork后child继续父RNG流（§5.5）。
  - fork拆分进程内 `kv`。
- **锁下共享heap**（研究C或global lock）。
  - 无锁race静默；Go只有runtime `-race` 报告（M6/M7）。
  - global lock重回顺序serve（M1/M5/M9/M12/M17的1.70 s）。
  - 每值改 `Arc` 是整个runtime成本。
  - embedded无法产生 `after=6`，P2/Workers根本不能共享memory。
- **stock artifact用p3 `wasi:sockets`**（#2659方案1）。
  - 可行：#2659原型commit e63493268本地通过三路径byte同一replay。
  - 仍拒绝。`wasmtime run -S p3 -S inherit-network` 是CLI runner而非serving platform；serving platform（`wasmtime
    serve`、Spin、Workers）说incoming-handler。
  - 它把request解析/response写入搬入guest，使byte一致变成转写。
  - 唯一保留的native单instance在pillar 1–2成立后不可观测。
- **opt-in并发**（`http.serve` 仍顺序，另设 `serve_concurrent` 适用规则，研究G）。model最先写的默认仍阻塞（#2665）。
- **显式state fold**（`http.serve_state(port, init, (s, req) => (s2,
  resp))`，研究H）。
  - fold结构上顺序，fast request又需等待。
  - 状态在instance memory，P2每request重置，P3/Workers拆分。
  - 每key store保有同样显式性而无两问题。
- **`kv.incr` 映射host `increment`。** 编码不同（wasmtime十进文本、Spin 8-byte LE），与 `get` 混用依实现（wasi-keyvalue PR #59）。

<a id="11-consequences"></a>

## 11. 后果

收益：

- 一个程序在Almide所有serve host意味着一组答案；编辑不能静默翻转serve模式或counter语义。
- 2 s `/slow` 后的 `/fast` 在native立即回答（#2665）。
- stock artifact成为普通 `wasi:http` component（#2659）。
- `fan.map` 静默顺序缺口关闭，清除#2594语义障碍。
- 0b落地后，两路径均可直接调用app测试。

代价：

- 新dialect epoch。
- 新stdlib module。
- 每次print加锁的共享runtime stdout buffer。
- 每worker求值top-level `Rc` let，产生每worker memory/startup成本。
- export host失去main计算的每process值。
- 合规host出现前stateful app不能export到stock（§4.4）。

<a id="12-falsifier"></a>

## 12. 可证伪条件

1. 若Dojo连续两个release测得model编写handler/`fan` task命中E008后，产生*错误*程序多于正确 `kv` / `let` 改写，则诊断/fix-it有错，应重审。除非找到合规替代，规则自身保留。
2. 若出现正确行为需要并发body可达 `var` 且不能用 `kv` 表达的手写程序（如每连接streaming状态机），重审规则的到达范围而非其存在。
3. 若每worker app instance让单client的spec serve fixture比当时顺序loop慢，则重审每worker instance模型（如通过pure值的 `Send` 表示共享immutable app）。
4. 若wasi:keyvalue稳定版没有CAS或保证弱于每key（开放PR #56方向），按标准实际atomic primitive重设计stock `kv` binding，保持Almide表层。

<a id="13-references"></a>

## 13. 参考资料

- 研究note：`../almide-references/RESEARCH-http-serve-concurrency.md`（M1–M23、S1–S18、§4–§6），probe/raw输出在 `../almide-references/research-2026/http-serve-concurrency/`。
- wasi-keyvalue：https://github.com/WebAssembly/wasi-keyvalue （README.md:7、55-57、82-86；`aa972c86e` 的wit/store.wit、wit/atomic.wit、wit/batch.wit；tag `v0.2.0-draft`；PR #56/#59/#60）。
- wasmtime 47：tag v47.0.3的 `src/commands/serve.rs`（44-68、141-155、380-460、535-545行）和 `crates/wasi-keyvalue/src/lib.rs`（100-171、226-246行），https://github.com/bytecodealliance/wasmtime/tree/v47.0.3 ；`wasmtime serve --help` / `-S help`（47.0.2）；`wasmtime-wasi-http-47.0.3/wit/deps/http.wit` 中 `wasi:http@0.2.12` WIT（`interface incoming-handler`，636行）。
- Spin：https://github.com/spinframework/spin （`wit/deps/keyvalue-2024-10-17/`、`crates/key-value-spin/src/store.rs:326-457`、`crates/factor-key-value/src/host.rs:706-745`）；https://spinframework.dev/v4/dynamic-configuration ；https://spinframework.dev/blog/announcing-spin-3-6 。
- Cloudflare：https://developers.cloudflare.com/kv/concepts/how-kv-works/ 、https://developers.cloudflare.com/kv/api/write-key-value-pairs/ 、https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/ 、https://developers.cloudflare.com/durable-objects/concepts/what-are-durable-objects/ 、https://developers.cloudflare.com/durable-objects/api/namespace/ 、https://developers.cloudflare.com/workers/reference/how-workers-works/ 、https://developers.cloudflare.com/workers/runtime-apis/webassembly/ 。
- Deno KV：https://docs.deno.com/deploy/kv/transactions/ 。
- RFC 9110 §5.3（field顺序），https://www.rfc-editor.org/rfc/rfc9110#section-5.3 ；RFC 9113 §8.3.2，https://www.rfc-editor.org/rfc/rfc9113#section-8.3.2 ；RFC 9114 §4.3.2，https://www.rfc-editor.org/rfc/rfc9114#section-4.3.2 。
- tree内：`runtime/rs/src/http.rs:723-743`、`runtime/rs/src/fan.rs:3-40`、`runtime/rs/src/random.rs:6-10`、`crates/almide-codegen/src/lib.rs:346-359`、`crates/almide-frontend/src/check/infer_calls_closures.rs:215-234`、`stdlib/http.almd:19-30, 355-626`、`proofs/dialect-epochs.toml`、`docs/contracts/contracts.toml`（C-367、C-368）、C-350。
- 编号：0018由进行中branch（value-strings记录）占用，所以本记录取0020。
