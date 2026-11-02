<a id="als--実行時規範runtime"></a>

# ALS — 运行时规范（Runtime）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](runtime.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](runtime.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](runtime.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/runtime.md)的非规范读者快照。下文的规范地位和要求属于原文，不属于本副本。不自动同步；本次翻译未独立复核历史实现、符合性及安全主张。代码与规则标识符保持不变。

> Last updated: 2026-10-26

程序执行的观测规范（错误终止、字符串插值的显示形式、并发组合器）。引用方式与 [strings.md](../../../../../specs/als/strings.md) 开头相同。

<a id="als-r1-effect-main-のエラー終了形"></a>

## ALS-R1 effect-main 的错误终止形式

`effect fn main` 以 Err 结束时，向 stderr 输出一行 `Error: <メッセージ>`（メッセージ意为消息），以 **exit code 1** 退出。Ok 结束为 exit 0。panic 级异常（包括 ALS-T6 的 abort）也统一采用相同的 `Error:` 前缀和 exit 1。
Contracts: C-035。

> **适用范围补充（非规范）**：不要将上述约定扩展到任意进程故障。显式 `panic` 参见 C-219，hole 等参见 ALS-E30，子进程 raw panic / trap 的识别参见 ALS-T30。各项分别记录自己的标题及退出状态。

<a id="als-r2-補間の表示形"></a>

## ALS-R2 插值显示形式

字符串插值 `"${v}"` 的显示形式按类型规定：

- **容器**（List/Map/Set/元组/Option/Result）：Almide 字面量形式（`[1, 2]`、`("a", 1)`、`some(3)` 等）。嵌套也递归采用相同形式。
- **记录／变体**：`TypeName { field: v, … }`，字段按**声明顺序**。匿名记录按字段名字典序。递归及泛型 ADT 的每次实例化均遵循同一规则。
- **裸 Float**：Display 去掉整数值的 `.0`（`3`），而 `float.to_string` 保留它（`3.0`）。区分这两种形式是规范的一部分。
Contracts: C-008, C-009, C-010, C-011。

<a id="als-r3-fan-並行コンビネータの決定性"></a>

## ALS-R3 fan 并发组合器的确定性

**接受形式**（语法元素方向：`ExprKind::Fan` / `FanBounded` / `FanRace` / `FanRaceMap` / `FanSettle` / `FanTimeout`）：列表形式 `fan.map(xs, (x) =>
…)`，块形式 `fan.settle { … }` / `fan.min(clock) { … }`。所有形式都**要求 effect fn 上下文**。`fan.map` 的 mapper **必须返回 Result**（裸值在检查时拒绝，并引导改为 `(x) => ok(…)`；any/settle 的 thunk 自动包装，map 的 mapper 不会）。`fan.settle { a; b }` 返回 **Result 元组**，元素数等于 thunk 数（ALS-DT4）。

`fan.map`、`fan.settle` 的结果**按列表顺序确定**（不是最先完成者，而是从参数列表开头求值所得的第一个适用结果）。错误以 ALS-R1 的统一 abort 形式呈现。

`fan.map` 和 `fan.settle` 的观测（stdout、stderr、退出码、返回值）必须与**按列表顺序逐一求值全部元素的顺序执行**相同。执行基础设施（顺序、线程、异步 subtask）的选择不得改变这些观测（C-004）。`fan.map` 在某元素返回 Err 后仍**求值全部剩余元素**；若有多个 Err，则选择**最小 index 的 Err**，与块形式 `fan { }` 相同（C-005、C-199、`spec/wasm_cross/fan_map_err_runs_every_element.almd`）。元素 k 的 trap 表现为：元素 0..k-1 完成且输出按列表顺序出现，随后在元素 k 到 trap 为止的输出之后 abort；k 后面元素的输出不出现（C-200、`spec/wasm_cross/fan_trap_waits_for_elements_below.almd`）。

旧名 `fan.race` 和 `fan.timeout` 曾在 0.42.0 / 0.29.0 删除，随后**在 0.47.0 获得确定性语义后恢复**：race 按 (spend, index) 字典序最小者选胜者（ALS-DT3、C-205，包括 mapper 形式 `fan.min(clock, xs, f)`）；timeout 是在 charge site 协作检查的墙钟期限（ALS-DT5、C-208）。`fan.bounded(c) { … }` 是确定性预算（ALS-DT2、C-204），没有 mapper 形式。旧 tombstone 裁定（C-004/C-006）由此次恢复所取代（SUPERSEDED）。当前最小 spend 的写法是 `fan.min`（dialect epoch 14）。`fan.race` 是上述恢复时的名称，不是当前调用名称。
Contracts: C-004, C-005, C-006。

<a id="als-r4-非有限浮動小数の定数表示"></a>

## ALS-R4 非有限浮点数的常量显示

常量折叠产生的非有限值（inf / -inf / NaN）显示为具名常量（`inf`、`-inf`、`NaN`）。位模式或 `1e999` 形式不符合规范。
Contracts: C-012。

<a id="als-r5-プロセス環境"></a>

## ALS-R5 进程环境

`env.args` 返回**不含** argv[0]（程序名）的参数序列，两目标一致（0.59.1 实测：两个参数时长度为 2）。`env.get(name)` 观察宿主进程环境变量，存在时返回 some(值)，不存在时返回 none，两目标逐字节相同（wasm 使用 WASI environ + 运行器继承环境）。`random.int(a, b)` 即使使用 WASI entropy，也始终位于 [a, b]。

`env.secret(name)` 用 `Secret[String]` 返回与 `env.get(name)` 相同的值：仅当 `env.get(name)` 为 some(v) 时返回 some(secret.new(v))，为 none 时返回 none。经 `secret.reveal` 取出的值与 `env.get` 逐字节相同，空值也一样。所有渲染均将该值显示为 `Secret(***)`（C-425），`==` 对取出的字符串进行恒定时间比较（C-426）。两目标一致。
测试：`spec/wasm_cross/env_secret.almd`。Contracts: C-438。

`env.os()` 和 `fs.temp_dir()` 是等价性规则的**唯二例外**。两者报告运行中的宿主，因此 native 返回实际 OS 名和实际临时目录，而 wasm 路径返回 WASI 沙箱（`wasi` / `/tmp`）。强使两目标一致反而是缺陷；程序询问“当前平台是什么”时必须得到真实回答。例外并非无约束：两路径都要证明相同的确定性不变量（os 是封闭集合 {macos, linux, windows, wasi} 的成员；temp_dir 非空，在 posix 宿主上为绝对路径）。目前例外为两个函数（env.os、fs.temp_dir，C-189）；增加第三个必须修改 C-189 的 statement 及其夹具。

`process.output` 和 `process.start` 的启动失败返回包含操作名和命令名的 err。前缀为 `process.output(<quoted cmd>): `（`process.start` 同形）；指定工作目录 `dir` 时为 `process.output(<quoted dir>, <quoted cmd>): `，之后附上宿主错误说明。命令名以双引号包围，转义内部引号、反斜杠、控制字符，不包含参数序列。可执行文件不存在属于启动失败，不得报告为超时。期限触发时的 err 保持为 `exec timed out after <ms>ms`。
测试：`spec/stdlib/process_timeout_test.almd`

wasm 目标上的嵌入式宿主（`almide run --target wasm` 和 `almide test` 的 wasm 路径）以与 native 相同的观测提供子进程族（`process.exec`、`output`、`run`、`kill`、`pid`），以及 `process.start` 的子进程和其 `poll`、`wait`、`stop`、`read_new_stdout`、`read_new_stderr`、`child_pid`。捕获的 stdout/stderr、每次增量读取返回的字符串、退出码（由信号终止的子进程为 -1）、期限 err 及其他 err 字符串逐字节一致。pid 是宿主值，不在路径间比较。独立产物（`almide build --target wasm`）仅当程序含子进程操作时，才导入私有接口 `almide:process/spawn` 的 `call`；不含这些操作的产物没有该导入。未定义该导入的运行时在执行 `_start` 前的加载阶段拒绝模块，因此子进程调用不会在运行时返回错误答案。将此类程序构建为组件（`--component`）以 E081 拒绝。
测试：`spec/embedded_cross/process_spawn_family.almd`、
`spec/embedded_cross/process_start_family.almd`

`http.start(http.new_request(method, url, body, headers), limits)` 开始请求后立即返回调用句柄（`HttpCall`）。每次调用通过 `limits = { total_ms,
idle_ms }` 传入上限（毫秒，0 表示无限制）。`total_ms` 是从 `start` 起计的墙钟时间，包含连接、等待首字节及整个正文。`idle_ms` 限制字节到达间隔，也包括等待首字节。触发的上限以指明自身名称的 err `request timeout: total_ms <n> exceeded` 或 `request timeout: idle_ms <n> exceeded` 终止调用。`ALMIDE_HTTP_TIMEOUT_SECS` 是不接受上限参数的调用的默认值，不影响带上限的调用。`poll` 和 `read_new` 不等待；`wait` 等待调用结束。`cancel` 以及丢弃句柄最后一份副本，都会以 err `request cancelled` 结束运行中的调用并关闭连接。此后的 `read_new` 返回空串，服务器会观察到连接关闭。上限是否触发是宿主性质（与 C-214 同一纪律），固定的是触发时 err 的形式。
wasm 目标上的嵌入式宿主（`almide run --target wasm`）以与 native 相同的观测提供 `start`、`poll`、`read_new`、`wait`、`cancel`；err 字符串、`cancel` 关闭连接、丢弃最后一份句柄副本取消调用也相同。独立产物（`almide build --target wasm`）不能具有该族，`almide check --target wasm` 以 E081 拒绝。OpenAI、Anthropic 流式客户端在 dialect epoch 14 移至 almide/llm 包（C-270）。
测试：`spec/stdlib/http_call_test.almd`、`spec/embedded_cross/http_call_handle_errs.almd`

`http.serve(port, handler)` 在嵌入式 wasm 路径（`almide run --target wasm`）也按 native 相同语义运行。一次运行的全部请求由一个实例按接受顺序逐一处理。main 只运行一次，与 native 一样调用 `http.serve`。宿主绑定 `0.0.0.0:<port>`，逐个将解析后的请求交给 guest。main 中 `serve` 之前的效应只发生一次。传给 `serve` 的应用封闭于实例（不读取 main 的局部变量，almide/almide#2698）。因此不规定 main 在 `serve` 之前计算的值，求值应用的实例数量不可观测。
两路径用相同代码读取请求：请求行的方法和目标、按首个冒号分开并去掉两边空白的头行（到达顺序），以及 Content-Length 正文（UTF-8，无效字节替换为替换字符）。两路径对每个请求都以相同 status 码、头集合和正文应答。头集合将响应头字段视为（名称，值）对，名称按 ASCII 大小写不敏感比较。不同名称字段不要求顺序，同名字段保留相对顺序（RFC 9110 §5.3）。排除宿主管理的 `date`、`connection`、`keep-alive`、`transfer-encoding`、`content-length`（它们属于 framing，正文去掉 framing 后比较）。理由短语不属于规范。HTTP/2、HTTP/3 没有理由短语（RFC 9113 §8.3.2、RFC 9114 §4.3.2），宿主 HTTP 库写自己的理由短语（native 核心为 `418 OK`，hyper 为 `418 I'm a teapot`，在 almide/almide#2659 原型中测量）。
`method`、`path`、`body`、`header`（首个匹配，忽略 ASCII 大小写）、`query_params` 返回相同值。query_params 将首个 `?` 后的内容按 `&` 分割，每对按首个 `=` 分开，丢弃没有 `=` 的项，解码 `+` 和 `%XX`，后面的键获胜。处理器的 `err(m)` 变为 `500`，正文为 `Internal error: <m>`，带 `Content-Type: text/plain`。在 export 宿主（运行 C-375 产物的 `wasmtime serve`）上，处理器 trap 或中断由宿主自身的 `500` 回应，实例被丢弃。
在套接字宿主（native、嵌入式路径）上，bind 失败无论调用处于何位置都会中断执行，向 stderr 写入 `Error: bind failed: <os message>`，以退出码 1 结束。`http.serve` 的类型不返回 err，因此调用方的 `!` 不起作用（本规范以前，native 运行时返回的 err 只在调用位于函数末尾时呈现；其他位置会在没有服务器的情况下继续执行）。
套接字宿主的服务器运行期间也保持 native 流式规则。stderr 不缓冲，所以写入 stderr 的每行在两路径都会到达流，两份记录包含相同行（跨请求的行顺序不属规范；不包含 `wasmtime serve` 的 `stdout [req_id] :: ` 等宿主日志装饰）。stdout 是终端时每次写入都 flush，否则每次结束一行的写入都 flush（C-162）。
在套接字宿主上，信号停止服务器而不丢失输出（almide/almide#2692）。`http.serve` 运行时收到第一个 SIGTERM 或 SIGINT（Windows 为 Ctrl-C 或 Ctrl-Break），宿主停止接受连接（尚未接受的连接不回应而直接关闭），完成并回应正在处理的请求，flush stdout，然后从 `http.serve` 返回。因此后续语句继续执行，退出码取自 main。第二个信号（清理期间或 `serve` 返回后均可），或超出请求超时（30 秒）后清理仍在等待，会 flush stdout，以退出码 1 结束，不回应正在处理的请求。此处 serve 停止不以 128+信号编号退出。一般 `process.exit` 范围及 preview-1 限制见 ALS-R9 / C-350。
范围外：HTTP framing、连接复用（宿主决定），以及其他信号。native Windows 的强制停止不 flush stdout，以退出码 1 结束；只有 serve 线程访问 stdout 缓冲区，在 stdout 成为全进程单一缓冲区（almide/almide ADR-0020 §5.5）前如此。
标准产物（`almide build --target wasm`）没有监听套接字。serve 形程序成为导出 `wasi:http/handler@0.3.0` 的组件，其形式见下段（C-375）。其他可到达 `http.serve` 的程序，在标准产物的 check 和构建阶段都拒绝（E081，两者均为 almide/almide#2659）。
测试：`spec/serve_cross/http_serve_replay.almd`（不终止的服务器夹具，通用运行器不执行；实现侧驱动启动两路径，重放相同请求序列，比较各响应 status 码、头集合、去 framing 正文，以及按行多重集合比较 stderr 记录，并在已占用端口上启动以比较中断）；`spec/serve_cross/http_serve_shutdown.almd`（同一驱动将 stdout 重定向到文件、启动两路径的不终止服务器夹具。在处理器睡眠的请求期间发一次 SIGTERM，会回应请求，保留直到 main 在 `http.serve` 之后所写行的全部行，以退出码 0 结束；在停滞请求期间发两次 SIGTERM，保留停止前写出的全部行，以退出码 1 结束）。

serve 形程序（`main` 的主体恰好是一次 `http.serve(port, app)` 调用，之前只能放仅由 `port` 读取的 `let`）经 `almide build --target wasm` 构建，会成为导出 `wasi:http/handler@0.3.0` 的组件，`wasmtime serve` 无需标志就能加载并提供服务。组件只导入服务 world 的接口，不含 `wasi:filesystem`。`main` 不运行，`port` 不求值（地址由宿主决定）。应用和顶层 `let` 按实例求值。一个实例同时只运行一个处理器（背压），宿主通过增加实例处理并发请求。guest 应用 native 上限：超过 1 MiB 的请求正文返回 `413`，超过 8 KiB 的请求行返回 `414`，均不调用处理器。处理器 trap 或中断由宿主的 `500` 回应，宿主丢弃该实例。其余接受的请求以与 native 相同的 status 码、头集合、去 framing 正文回应。比较采用 C-367 的 HTTP 语义；理由短语、`date`、framing 属于宿主，不逐字节比较。
范围外：宿主地址、接受行为、并发度、实例复用、超时、停止、日志装饰（均为宿主配置）。
测试：`spec/serve_cross/http_serve_export.almd`（不终止的服务器夹具，通用运行器不执行；实现侧驱动构建标准 wasm 版本并通过 `wasmtime serve` 提供服务，同时运行 native，重放相同请求序列并比较 status 码、头集合、去 framing 正文，向产物发送超限正文和过长请求行，最后请求 `/trap`）。

Contracts: C-096, C-112, C-118, C-133, C-189, C-214, C-366, C-367, C-375。

<a id="als-r6-ファイルシステムのパス解決"></a>

## ALS-R6 文件系统路径解析

wasm 的 fs 运行时在启动时建立 WASI preopen 目录表，将绝对路径解析为最长匹配 preopen + 相对剩余路径（包括 `./` 规范化）。向同一路径写入后读回，会访问与 native std::fs 相同的宿主文件，不依赖 CWD。open 错误措辞遵循 ALS-T6 系的 native 措辞规范。

`fs.list_dir(path)` 返回目录的**全部**条目。条目数无上限，宿主缓冲区边界不会体现在观测中（wasm 的 `fd_readdir` 是可恢复 API，单次遍历只返回能放入调用方缓冲区的部分）。两目标均排除 `.` 与 `..`，顺序规范化为**字节字典序**（native 用 `names.sort()`，wasm 用相同字节比较的插入排序）。文件系统的 readdir 顺序不可观测。读取失败返回 err，而非截短列表。
测试：`spec/wasm_cross/fs_list_dir_multipass.almd`
Contracts: C-042, C-272。

<a id="als-r7-ストリーミング行走査の可謬コールバック"></a>

## ALS-R7 流式逐行遍历的可失败回调

ADR-0006 的一位可失败性多态也适用于 fs 流式逐行遍历。`fs.fold_lines` / `fs.for_each_line` 的回调体使用 `!` 时，检查器将被调用方改写为内部载体 `fs.__fallible_fold_lines` / `fs.__fallible_for_each_line`，整个调用成为可失败调用（与 `list.map` → `list.__fallible_map` 同构）。载体不是源码写法，源码直接点名会得到 E043。

规范观测量是**回调调用序列**。回调一直调用到首个返回 err 的行，此后的行**一次也不调用**（first-err 截止）。回调的 err 消息原样进入调用方 err 通道。这两项在 native ⇄ wasm 间逐字节相同。

native 路径还在失败行停止**读取本身**（`almide_rt_fs_fold_lines_effect` 从 BufReader 循环 return）。wasm 自托管与 C-220 一样先读整个文件，因此“读取器停在何处”不是 wasm 观测量；像 RSS 一样，这是 native 限定性质，不包含在可观测承诺中。

分区遍历（`fold_lines_range` / `fold_lines_chunked`）**没有可失败形式**。分割遍历的“首个 err”会成为线程执行顺序的观测量，因此不存在可定义的截止点；`tests/fs_streaming_family_gate_test.rs` 的矩阵将此固定为有意省略。

测试：`spec/wasm_cross/fs_fallible_stream_callback.almd`（两路径）、
`spec/stdlib/fs_streaming_test.almd`（for_each_line 单元，仅 native）、
`tests/fs_streaming_family_gate_test.rs`（矩阵门禁）。
Contracts: C-274。

<a id="als-r8-http-レスポンスヘッダの規範"></a>

## ALS-R8 HTTP 响应头规范

`http` 响应构造族（`response` / `json` / `redirect` / `with_headers` / `set_status` / `body` / `set_header` / `header`）是**不访问网络的纯数据操作**，在两目标运行。头名称有三条规范：

- **字段名不区分大小写**（RFC 9110 §5.1）。折叠**仅限 ASCII**（字段名为 `token`，与 v0 的 `eq_ignore_ascii_case` 一致，不得用 Unicode 的 `string.to_lower`）。`header` 返回首个匹配，没有则返回 none。
- **每个字段名至多一个条目。** 写入（`set_header`、`with_headers`）忽略大小写，**原地覆盖已有条目的值**（保留首次存储的名称拼法和位置），不存在则追加到末尾。因此只要 `k` 与 `k'` 仅 ASCII 大小写不同，`header(set_header(r, k, v), k') == some(v)` 就始终成立。
- **`with_headers` 按上述覆盖规则构造传入列表**（C-275）。没有重名时保持列表顺序，重名时在最初位置及拼法上放置最后的值。

  不擅自**预置** `Content-Type`。只有没有其他途径指定该名称的 `response`（`text/plain`）和 `json`（`application/json`）带默认 Content-Type。`redirect` 只有 `Location`。

测试：`spec/wasm_cross/http_response_headers.almd`、
`spec/stdlib/http_response_test.almd`。

处理器接受 `HttpRequest` 并返回 `HttpResponse`（`HttpHandler = effect (HttpRequest) -> HttpResponse`）；加上路由器、中间件的应用也仍是处理器。`http.new_request(method,
target, body, headers)`（`headers` 为 `List[(String, String)]`）无须套接字即可构造请求，读取族（`method` / `path` / `body` / `header` / `query_params` / `param`）对其在两目标返回相同值。`path` 返回包含查询字符串的 target。路由在两目标给出相同结果：

- 路由为 `"METHOD /path"`（省略方法表示全部方法）；`{name}` 绑定一个片段，末尾 `{name...}` 绑定剩余路径，允许为空。绑定值经百分号解码，`+` 仍为 `+`。匹配使用去掉查询字符串的路径，不计算空片段。
- 匹配路由中**最具体者**回应，注册顺序不影响结果。若 A 匹配的全部路径也被 B 匹配，则 A 至少与 B 一样具体；带方法的路由比同路径无方法路由更具体。
- `http.router(routes)` 对以下表返回点名全部问题的 `err` 并拒绝：能匹配同一请求但双方均不比对方更具体的路由对（包括重复相同模式）、非末尾 `{name...}`、重复绑定名称、不是大写英文字母的方法、不以 `/` 开头的路径。被拒绝的表不回应请求。
- 没有路由匹配路径时返回 `404 Not Found`；路径仅匹配其他方法时，返回 `405 Method Not Allowed` 及 `Allow` 头（方法排序后以 `, ` 连接，有 GET 时包含 HEAD）。HEAD 落到 GET 路由，返回空正文。target 不以 `/` 开头，或路径中的 `%` 后不是两位十六进制数字时，返回 `400 Bad Request`。
- `http.mount(prefix, sub)` 将 prefix 之后的路径和查询交给 `sub`，prefix 绑定的名称也能从 `sub` 读取。`http.wrap(h, [a, b])` 等于 `a(b(h))`，列表第一项在最外层。
- `http.decode_json[T](req)` 将正文读作 JSON，经 `T.decode_strict` 转为输入类型 `T`（只声明 `: Decode` 的 record 或 variant，C-435）。失败的 `err` 是正文带失败原因的 `400 Bad Request` 响应。任意深度上不属于 `T` 的键（包括嵌套输入记录及 variant 载荷内部的键）产生 `400 Bad Request: unknown key '<k>'`，并附该 record 路径（`, at address`）。若 `T` 也属于 `Encode`（即 `Codec`），或其字段／variant 载荷含 `Codec` record，检查时以 E110 拒绝（C-436）。处理器的 `err` 仍作为 `err` 返回调用方。

测试：`spec/stdlib/http_router_test.almd`。
Contracts: C-275, C-368, C-436。

<a id="als-r9-プロセス終了コードの値域"></a>

## ALS-R9 进程退出码范围

`process.exit(code)` 接受的 code 为 POSIX 退出状态 **0..=255**。该范围的值在 native 和嵌入式宿主（`almide run --target wasm`）上都**原值退出**。这一范围的目的，是能编写原样返回子进程 126、127、128+n 的包装器（#2780）。Rust 的 `std::process::exit`、Go 的 `os.Exit`、Python 的 `sys.exit`、Node 的 `process.exit` 也传递相同值。

计算得到的参数超出 0..=255 时，产生已定义的值域错误：向 stderr 输出一行 `Error: exit code must be in 0..=255`，以 **exit 1** 结束（全部目标逐字节一致）。上述语言将低 8 bit 交给 OS，所以 `exit(256)` 报告成功（0），`exit(-1)` 报告 255。调用方无法检测此截断，因此 Almide 在这里区别处理。

| 层 | 可传递的值 | 越界结果 |
|---|---|---|
| POSIX `exit(status)` | 仅低 8 bit | 父进程将 `256` 观察为 `0`（静默截断） |
| WASI preview-1 `proc_exit`（stock 运行时） | `[0, 126)` | 宿主 trap。wasmtime 47 输出 `exit with invalid exit status outside of [0..126)`，以 exit 1 结束，作为退出请求的信息不再保留 |
| component model `wasi:cli/exit#exit-with-code(u8)` | 0..255 | 不 trap |

**WASI preview-1 构建无法传递 126..=255。** `almide build --target
wasm` 输出的 preview-1 产物在 stock 运行时执行时，该范围值的 `proc_exit` 会 trap（#2303）。所以这种构建自行拒绝该区间：向 stderr 输出一行 `Error: a WASI preview-1 build cannot exit with a code in
126..=255`，以 **exit 1** 结束。这是 preview-1 lowering 的带诊断限制（与 wasm 上 `env.cwd` 同类），不是无说明的 1，也不是原因不明的宿主 trap。0..=125 在任何路径都以原值退出。

这是 preview-1 lowering 的性质，不是 wasm 自身性质。如果能传递完整一字节退出码的产物（component model 的 `exit-with-code(u8)`）成为默认形式，就可移除该限制；移除是向后兼容的扩展。

直接调用 `process.exit` 时，若参数是越界整数字面量，checker 以指向参数的 **E084** 拒绝。包括括号、单目负号、各进制字面量；通过模块别名、选择性 import、管道调用也用相同规则。`-0` 视为 0，双重单目负号视为正值。

```almide check-fail=E084
import process
effect fn main() -> Unit = process.exit(256)
```

变量、算术表达式等计算参数不被此字面量诊断拒绝。存有函数值的变量调用也不由该诊断跟踪。运行时范围检查仍然适用。同名用户函数和局部函数值不是标准库的 `process.exit`，不在该诊断范围内。

测试：`spec/wasm_cross/exit_code_out_of_range.almd`、
`spec/wasm_cross/exit_code_upper_bound.almd`、
`spec/wasm_cross/exit_code_passthrough.almd`、
`spec/embedded_cross/exit_status_passthrough.almd`、
`tests/diagnostics/e084-exit-code-domain/broken.almd`、
`tests/diagnostics/e084-exit-code-domain/fixed.almd`、
`tests/exit_literal_check_test.rs`。
Contracts: C-350, C-351。

> 原文背景：带日期的正文保留历史 fan 写法，并区分嵌入式宿主、独立 preview-1、服务组件路径。ALS-R5 的“唯二例外”与另述的宿主相关 PID、期限及 HTTP 排除范围一并保留，不应扩展为任意宿主上的行为完全一致。ALS-R3 的一般错误措辞不取消另行规定的 settle Result 元组行为。ALS-R8 的默认 Content-Type 陈述保留，不在此与后续类型化响应规则合并。这些内容不构成对当前运行时或安全性的独立验证保证。
