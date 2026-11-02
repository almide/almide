<a id="adr-0023-wasm-http-is-wasihttp03--a-program-that-uses-http-builds-as-a-p3-component-the-embedded-host-serves-the-same-imports-and-no-host-serves-a-custom-http-op"></a>

# ADR-0023：Wasm HTTP采用 `wasi:http@0.3` — 使用HTTP的程序构建为p3 component，embedded host提供相同import，任何host都不再提供自定义HTTP op

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0023-wasm-http-over-wasi-http.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0023-wasm-http-over-wasi-http.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0023-wasm-http-over-wasi-http.zh-CN.md)

> 非规范性历史读者快照，未改动的固定原本为权威。不自动同步，未独立执行，不保证当前host兼容性或安全性。已接受方向、原型及计划gate不表示迁移已实现，保留原文当时尚未实现的说明。测量、生态与版本主张属于原文日期证据，未在本次复核。213对166 package与新增64个名称保留为各自原文叙述，不用减法改写。stock host的自由文本internal error不在错误一致保证内，proxy/trust root由各host管理。HTTP component默认化、host op退役及契约变更仍是记录的计划；这里的命令不授权执行。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)

- **状态**：已接受。维护者于2026-09-28确定目标为WASI 0.3（`wasi:http@0.3`），不设p2阶段。本记录以测量和先例检验提议方向，并给出实现顺序。目前均未实现。证据改变提案处标记**[refined]**，列表见§9。
- **日期**：2026-09-28
- **范围**：`--target wasm` 的所有 `http` 客户端函数、`--target wasm` 的 `http.serve`、embedded host（`almide run --target wasm`、`crates/almide-wasm-run`）、p3 shim（`crates/almide-wasm-run/src/wasi_p3*.rs`）、`almide build --target wasm` 的默认形状，以及契约行C-328、C-366、C-367。
- **取代**：[ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §5.3“选择哪种export形状”（P2的 `wasi:http/incoming-handler@0.2` 选择）及其step 7、9的顺序。ADR-0020其余内容保留（下文§3.3解释为何其支柱使P3 instance model安全）。
- **相关**：#1710（wasi:http@0.3客户端路径）、#1628（component model阶段）、#2659（server的stock artifact）、#2702（ADR-0020 step 9）、#2633（call handle）、#2819–#2826（HTTP审计）、#865（wasm VM）、#1696（退役旧emitter）、`proofs/wasi-pin-policy.toml`、`proofs/target-availability.toml`。
- **证据基础**：§2测量于2026-09-28运行，使用almide 0.64.0（5555cb251）、`wasmtime` CLI 47.0.2，阅读源码的crate为wasmtime 47.0.4 / wasmtime-wasi 47.0.4 / wasmtime-wasi-http 47.0.3 / wasmparser 0.259.0；保存在tree外的两个原型（以branch `stock-wasm-serve-2659` 的#2659 probe为基础，用wasm-encoder + wit-component构建的 `wasi:http/handler@0.3.0` component；及embedding host，§2.4）；`../almide-references/RESEARCH-component-model.md` 和 `RESEARCH-http-serve-concurrency.md`；§2.6的一手资料。

<a id="1-context"></a>

## 1. 背景

<a id="11-three-lanes-three-http-mechanisms"></a>

### 1.1 三条路径，三种HTTP机制

编译器生成一个core module，其host表层为 `almide.*` import（带op编号的 `fs_call`、`host_read`、`print`、`exit` 等）。接下来取决于路径：

| 路径 | 运行什么 | 如何提供HTTP |
|---|---|---|
| native | Rust binary | 拼入runtime的 `crates/almide-rt-core`（client/server core） |
| embedded（`almide run --target wasm`） | wasmtime上的core module，`crates/almide-wasm-run/src/host.rs` 提供 `almide.*` | **host**调用相同 `almide-rt-core` 函数，提供 `fs_call` op 43..=50（client）、53..=59（call handle）、70..=72（serve） |
| stock p1（默认 `almide build --target wasm`） | `to_wasi` 将 `almide.*` 重写为WASI preview-1 shim | 无：所有HTTP函数在构建时E081 |
| p3（`ALMIDE_COMPONENT_P3=1 … --component`） | `to_p3` 将 `almide.*` 重写为WASI 0.3上的**guest侧**shim，并用wit-component包装 | shim通过 `wasi:http/client@0.3.0` 提供op 43..=50 |

因此“自定义op”指两种不同事物：

- 生成代码与shim边界的**op编号**。在stock artifact内它们从不到达host：shim是guest代码，正如Rust p2 component的preview-1 adapter也是guest代码。
- embedded host在**host侧提供这些编号的服务**。正是它使embedded路径成为与所有stock runtime不同的ABI。

<a id="12-what-is-missing"></a>

### 1.2 缺失的部分

来自 `proofs/target-availability.toml`（develop f4d5c063b）：

- `get_response` / `post_response` / `put_response` / `patch_response` / `delete_response` / `request_response`（6个）及 `request_stream` 在structural、stock-p1**和**embedded路径均被wall阻挡；
- `anthropic_streaming_call`、`openai_streaming_call` 及其 `_with_limits` 形式（4个）在所有路径被wall阻挡；它们是native intrinsic（`@intrinsic("almide_rt_sse_*")`、`stdlib/http.almd:252`）；
- call handle（`start` / `poll` / `read_new` / `wait` / `cancel`、`request_stream_with_limits`）与 `http.serve` 仅由embedded host提供；
- p3 shim只提供五个verb和framed family（op 43..=50），不提供其他 `http`。还拒绝 `env.get`（host op 26），测量见§2.3。

<a id="13-the-question"></a>

### 1.3 问题

如何让wasm Almide程序成为HTTP client和server，使native与wasm给出相同回答？原提议方向是：(1) 不再用自定义host op，在guest上通过 `wasi:http` 补齐函数；(2) embedded host实现 `wasi:http` 而非自定义op，让同一artifact在embedded与stock runtime运行；(3) 使用HTTP的程序默认构建成component；(4) server导出handler；(5) 迁移后退役自定义op。owner将版本固定为WASI 0.3。

<a id="2-measured-evidence"></a>

## 2. 测量证据

所有命令在scratch目录运行，配有本地echo server（`python3 echo.py 18777`，一个以content-length回应 `"<METHOD> <path> len=<body length>"` 的 `ThreadingHTTPServer`）。

<a id="21-what-wasmtime-47-needs-for-p3-and-why"></a>

### 2.1 wasmtime 47运行p3需要什么及原因

`wasmtime 47.0.2`、`wasmtime run -S help` / `-W help`，随后运行：

| Artifact | 命令 | 结果 |
|---|---|---|
| `hello_p3.wasm`（Almide，仅 `println`，p3） | `wasmtime run hello_p3.wasm` | 拒绝：`synchronous 'stream.write' requires the component model more async builtins feature` |
| 同上 | `wasmtime run -W component-model-more-async-builtins=y hello_p3.wasm` | `hello`（无需 `-S p3`，wasmtime 46起p3默认开启） |
| `get_p3.wasm`（Almide，`http.get`，p3） | `wasmtime run -W component-model-more-async-builtins=y get_p3.wasm` | 拒绝：`component imports instance 'wasi:http/types@0.3.0', but a matching implementation was not found` |
| 同上 | `… -S http get_p3.wasm` | `GET /hello len=0` |
| `h_sync.wasm`（#2659 handler probe：同步 `stream.write` / `future.write`） | `wasmtime serve -S p3 -S http h_sync.wasm` | 拒绝：`synchronous 'stream.write' requires …` |
| `h_async.wasm`（同probe，将两个write改为 `[async-lower]`，write回答BLOCKED时用 `waitable-set.wait`） | `wasmtime serve -S p3 -S http h_async.wasm` | `HTTP/1.1 418 I'm a teapot`、`x-count: 1`，body `hello from p3 #1`，下一request为 `#2` |
| 同上 | `wasmtime serve h_async.wasm`（**完全无flag**） | 相同回答 |

原因见wasmparser 0.259.0的 `src/validator/component.rs`：唯一受 `cm_more_async_builtins`（🚝）门控的canonical builtin是**同步** `stream.read` / `stream.write` / `future.read` / `future.write`，以及**async** `stream.cancel-*` / `future.cancel-*`。`waitable-set.new/wait/poll/drop`、`waitable.join`、`subtask.cancel/drop`、`backpressure.inc/dec`、`task.return` 和async stream/future op只需 `cm_async`；wasmtime 47默认打开 `cm_async`（`wasmtime-47.0.4/src/config.rs:2526-2531`）。

**发现1。** 固定wasmtime上p3 component需要 `-W`，只是因为Almide shim用了🚝同步builtin。用baseline async builtin构建的shim可运行于 `wasmtime serve app.wasm` 和 `wasmtime run -S http app.wasm`，其中 `-S http` 如 `--dir` 一样是capability授权。**不需要升级wasmtime。** 无flag host会接受的也是相同形状：🚝不是WASI 0.3 baseline的一部分（RESEARCH-component-model.md，“Built-ins under the baseline gate”）。

ADR-0020 §5.3选择P2是因为“固定wasmtime上的P3需要 `-W
component-model-more-async-builtins`”。该前提来自#2659 probe的同步write，而非P3本身。

<a id="22-the-embedded-host-can-serve-the-same-imports-prototype"></a>

### 2.2 embedded host可提供相同import（原型）

`wasmtime-wasi-http 47.0.3` 有 `p3` feature（`src/p3/mod.rs`，标记“experimental, unstable”；仅p3的bugfix不发patch release）。其出站transport是trait method：`WasiHttpHooks::send_request(request,
options, fut)`。不启用 `default-send-request` feature时，embedder必须提供该方法，而该crate不会引入rustls / webpki-roots / tokio-rustls。

原型host（约80行）构建带 `wasm_component_model_async(true)` 的 `Engine`，链接 `wasmtime_wasi::p3`、`wasmtime_wasi_http::p3`；`send_request` 收集request body，在blocking thread调用 `almide_rt_core::http_client_core::request_response`，即当时native与embedded host调用的同一函数。首次就针对固定crate编译成功（`cargo build --release -j2 --offline`：冷构建3 min 41 s），并原样运行Almide构建的p3 component：

```
$ MORE=1 embedhost post_p3.wasm        # http.post, then http.get to a refused port
POST /p len=6
err: http request failed (p3 transport)
```

| 同一 `post.almd` 运行于 | stdout |
|---|---|
| native（`almide run`） | `POST /p len=6` / `err: connection failed: Connection refused (os error 61)` |
| embedded、host op（`almide run --target wasm`） | 与native相同 |
| `wasmtime run … -S http post_p3.wasm` | `POST /p len=6` / `err: http request failed (p3 transport)` |
| 原型host（wasi-http p3 + rt-core hook） | 与wasmtime CLI行相同 |

**发现2。** 在固定版本嵌入可行，同一artifact在embedded host和stock CLI上给出相同回答。成功通信已在四条路径一致。错误文字不一致，原因在**guest**而非host：原型hook返回 `internal-error(some("connection failed: Connection refused (os
error 61)"))`，shim却将所有 `error-code` 合并成一条静态文字（C-328已记录不承诺p3措辞）。§4.2决定如何让文字一致。

嵌入的依赖成本：原型依赖树有213个package，当时 `almide-wasm-run` 为166；新增的64个名称是 `wasmtime-wasi`、`wasmtime-wasi-http`、`wasmtime-wasi-io`、`wiggle`、`tokio`、`hyper`、`h2`、`http`、`http-body(-util)`、`cap-std` 及传递依赖crate。原型binary为25 MB，安装的 `almide` binary为50 MB。

<a id="23-the-p3-lane-today-refuses-what-an-http-program-usually-needs"></a>

### 2.3 当时p3路径拒绝HTTP程序通常需要的功能

```
$ ALMIDE_COMPONENT_P3=1 almide build envget.almd --target wasm --component
error[E081]: env.get (host op 26) is unavailable in the direct WASI 0.3 component.
$ ALMIDE_COMPONENT_P3=1 almide build resp.almd --target wasm --component
error[E081]: `http.get_response` is not available on --target wasm
```

p3的 `component_availability::check` 提供op 1..=9、13..=16、30..=32、34..=35、40..=50、60、73。LLM client用 `env.get` 读key，当时无法在p3构建。

<a id="24-cost-size"></a>

### 2.4 成本：大小

相同程序的 `almide build`（structural路径，不用wasm-opt）：

| 程序 | p1 core module | p2 component（`--component`） | p3 component |
|---|---|---|---|
| `println("hello")` | 1,322 B | 6,107 B | 18,421 B（core module 7,521 B） |
| `http.get` + `println` | E081 | E081 | 30,278 B（core module 12,735 B） |

p3 component大部分大小是component类型信息（`wasi:http/types` interface），而非代码。差异为kilobyte量级，且仅HTTP程序付出（§3.3）。

<a id="25-cost-time"></a>

### 2.5 成本：时间

`bench.py`：向loopback echo顺序执行N次 `http.get`，2次预热后9次运行取中位数，N = 1和N = 500。斜率为每request成本。

| 路径 | N=1（ms） | N=500（ms） | 每request（µs） |
|---|---|---|---|
| native binary | 2.4 | 52.3 | 100 |
| embedded、host op（`almide run --target wasm`，包含compile） | 102.2 | 153.2 | 102 |
| `wasmtime run -W … -S p3 -S http`（hyper transport） | 4.8 | 66.3 | 123 |
| 原型host（wasi-http p3 + rt-core transport，未缓存compile：8 ms） | 11.8 | 77.5 | 132 |

`drive.py`：对 `http.response(200, …)` server发送500次顺序request，每次一条连接：

| Server | 中位数 | p90 |
|---|---|---|
| native `http.serve` | 88 µs | 135 µs |
| embedded `http.serve`（host op 70..=72） | 69 µs | 83 µs |
| `wasmtime serve` p3 handler（默认instance复用） | 93 µs | 113 µs |
| `wasmtime serve --max-instance-reuse-count 1`（每request新instance） | 107 µs | 129 µs |

**发现3。** 相比host op，component路径每loopback request多20–30 µs，未缓存嵌入还多一次约10 ms的compile。相对真实网络往返，两者都小，均不是保留第二个ABI的理由。

<a id="26-how-others-do-it-2026"></a>

### 2.6 其他实现的做法（2026）

面向stock runtime的实现没有为HTTP交付私有host ABI。各toolchain都lowering至 `wasi:http`，transport由host拥有：

- **Rust**：`wasm32-wasip2`（Tier 2）生成component；`wasip3` crate及wit-bindgen async mode面向 `wasi:http@0.3`；`wasm32-wasip3` 为Tier 3。`wstd` / `waki` / `wasi-fetch` 是这些binding上的薄client/handler库。
- **Go**：componentize-go把goroutine映射到callback ABI（golang/go#77141是提案）；TinyGo `wasip2` 使用 `wasi:http`。
- **Python**（componentize-py、asyncio）和**JS**（jco / ComponentizeJS、StarlingMonkey通过 `wasi:http` 的 `fetch`）：已交付；jco支持全部WASI 0.3。
- **Host**：wasmtime 46+默认启用WASI 0.3与component-model async；Spin 3.6“并发和串行”复用WASIp3 instance；wasmCloud 2.5运行在wasmtime 46上。
- MoonBit的p3 async为“计划中”；Grain没有。

WASI 0.3.0于2026-06-11获批准，0.3.1于2026-08-11获批准（`proofs/wasi-pin-policy.toml` [sources]）。来源：[WASI 0.3 launch](https://bytecodealliance.org/articles/WASI-0.3)、[wasi.dev WASI 0.3](https://wasi.dev/releases/wasi-p3)、[Spin 3.6](https://spinframework.dev/blog/announcing-spin-3-6)、[wasmCloud on WASI P3](https://wasmcloud.com/blog/wasi-p3-on-wasmcloud/)、[wasip3 crate](https://crates.io/crates/wasip3)、[rustc wasm32-wasip3](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip3.html)，以及 `../almide-references/RESEARCH-component-model.md` §“Reference-compiler wasm backends”。

<a id="27-what-a-stock-host-does-with-the-audits-issues-source-reading"></a>

### 2.7 stock host如何处理审计问题（源码阅读）

`wasmtime-wasi-http 47.0.3` 默认transport（`src/p3/request.rs`）：

- 直接连接（`TcpStream::connect(&authority)`）；不支持proxy（#2819）；
- 仅信任 `webpki_roots::TLS_SERVER_ROOTS`（#2820）；
- connect、first-byte、between-bytes timeout来自guest的 `request-options`，各自默认600 s（#2825）；
- `fields.append` 用 `http::HeaderValue::from_bytes` 验证，因此值中的CR/LF会成为 `header-error.invalid-syntax`（#2822）；
- 禁止九个header名（`DEFAULT_FORBIDDEN_HEADERS`、`src/lib.rs:142`）：`connection`、`keep-alive`、`proxy-authenticate`、`proxy-authorization`、`proxy-connection`、`transfer-encoding`、`upgrade`、`host`、`http2-settings`。native当时全部接受；
- hyper解析response，因此处理1xx临时响应和chunk extension（#2824）；
- header名以小写到达guest（#2659 probe：`x-Count` → `x-count`）。

<a id="3-decision"></a>

## 3. 决定

<a id="31-the-five-points"></a>

### 3.1 五个要点

| # | 提案 | 裁决 |
|---|---|---|
| 1 | 不再用自定义host op，缺失client函数由guest通过 `wasi:http` 实现 | **确认并细化。** host不再提供新op。op编号只允许用在生成代码与p3 shim之间的guest内部调用，shim通过 `wasi:http/client@0.3.0` 提供（adapter pattern，§1.1）。6个 `*_response`、`request_stream` 和call handle进入shim；4个streaming LLM函数**通过call handle用Almide自托管**，完全不需shim或host代码。 |
| 2 | embedded host实现 `wasi:http`（wasmtime-wasi-http）而非自定义op | **确认并细化**（§2.2）。host以固定47链接 `wasmtime_wasi::p3`、`wasmtime_wasi_http::p3`，不用 `default-send-request`，`send_request` hook为 `almide-rt-core`。transport保留共享native代码，从构造上维持C-328。embedded **server** loop保留 `http_server_core` 用于accept、parse、limit，调用guest的 `wasi:http/handler.handle` export（§3.4）。 |
| 3 | 使用HTTP的程序默认构建为component | **确认p3，但有两个前提。** 调用任何 `http` 函数的程序默认成为WASI 0.3 component。首先shim仅用baseline async builtin（发现1），artifact无 `-W` 可运行；其次shim提供HTTP程序使用的op：env/args（`wasi:cli/environment@0.3.0`）、sleep及§7 step 3其余功能。**[refined]** **不**使用HTTP的程序保持preview-1 core module：1.3 KB而非18 KB，仍运行于wasmtime LTS 36（无默认p3）和#865 VM。默认按每程序op集合决定，如 `wants_http` 已用来路由 `to_p3`。 |
| 4 | server导出handler | **确认，重新定位至 `wasi:http/handler@0.3.0`。** 取代ADR-0020 §5.3的 `incoming-handler@0.2`（§3.3）。 |
| 5 | 迁移后退役自定义op | **HTTP部分确认。** embedded路径以p3 component运行HTTP程序后，host op 43..=50、53..=59、70..=72离开 `almide-wasm-run`。embedded host非HTTP的 `almide.*` 表层（fs、print、alloc计数器、有上限memory）不在范围内。无HTTP程序仍运行core module，alloc/size台账也测量它们。 |

<a id="32-one-artifact-two-runners"></a>

### 3.2 一个artifact，两个runner

对于使用HTTP的程序：

```
almide build app.almd --target wasm -o app.wasm   # a WASI 0.3 component
wasmtime run -S http app.wasm                     # a client, on any wasmtime ≥ 46
wasmtime serve app.wasm                           # a server
almide run app.almd --target wasm                 # the SAME bytes, on the embedded host
```

embedded路径不再是独立ABI，而成为stock artifact的runner，其transport、TLS、错误分类与native相同。

<a id="33-why-p3-for-the-server-superseding-adr-0020-53"></a>

### 3.3 为何server选择P3，取代ADR-0020 §5.3

ADR-0020 §5.3给出选择P2的三个理由：

1. “P2在固定wasmtime无需experimental flag即可load。”发现1已反驳：async-builtin P3 handler**完全无需flag**即可load，而同步版本无论有无 `-S p3` 都失败。
2. “P2是guest绝不交错的instance model。”正确，但ADR-0020自身支柱已使program状态不可跨instance观测：无可达 `var`（pillar 1）、跨request状态仅在 `kv`（pillar 2）、instance内闭合app（§5.2）。剩下是runtime自身重入（allocator、lazy-`let` slot、scratch buffer）。P3 shell在入口 `backpressure.inc`，response body完成后 `backpressure.dec`，以此串行化同instance的handler。该builtin在WASI 0.3 baseline内（wasmparser只用 `cm_async` 门控），合规host必须遵守。这是规范机制，用于ADR-0020原拟guest queue的位置。若step 8测量发现host忽略backpressure，就如ADR-0020所写回退guest queue。
3. “P2是方向中指名的interface。”方向现为WASI 0.3（owner裁定，2026-09-28）。

除裁定外还有选择P3的理由。Spin 3.6与 `wasmtime
serve` 复用P3 instance，所以app每instance构建一次而非每request一次。P2 artifact会在p3 client旁建立第二个component world（P2的 `wasi:http@0.2` client与P3接口不同），意味着又一个需保持同步的shim。

<a id="34-the-embedded-server-loop"></a>

### 3.4 embedded server loop

embedded host**不**使用 `wasmtime serve` 的hyper server。它保留 `almide-rt-core` 的 `http_server_core` 处理accept、request解析、`max_body_bytes`、`request_timeout_ms`、shutdown（ADR-0020 §5.5–5.7）。对每request构造 `wasi:http` request resource并调用component的 `handle` export。因此native与embedded继续共享server代码，这是C-367基础。`wasmtime serve` 是C-367 HTTP语义比较（status、header集合、body）下的stock host，hyper的reason phrase与 `date:` 是host装饰。

<a id="4-semantics-who-owns-what"></a>

## 4. 语义：谁拥有哪部分

<a id="41-the-rule"></a>

### 4.1 规则

- **guest拥有程序可作为值观测的部分**：URL分解、header有效性、body限制、deadline、取消、错误**文本**。在stdlib或shim写一次，所有路径运行。
- **host拥有transport及其信任**：DNS、socket、proxy、TLS、trust root、连接复用。在stock host上是host配置；native和embedded上则是 `almide-rt-core`，一次修正覆盖两条路径。

<a id="42-error-text"></a>

### 4.2 错误文本

每个transport失败归为 `wasi:http` 的 `error-code` case（`DNS-error`、`connection-refused`、`connection-timeout`、`TLS-certificate-error`、`HTTP-response-body-size` 等）。一个renderer把case转成 `err` 字符串。native（rt-core）与从任意host接收case的shim调用renderer。同一失败在native、embedded和所有stock host上读起来相同。只有 `internal-error(some(msg))` 携带自由文本，其相等仅保证在native与embedded之间。这改变native措辞（当时 `connection failed: Connection refused (os error 61)` 含OS专属errno），所以先在als修改C-328（§7 step 0）。

<a id="43-per-concern"></a>

### 4.3 按关注点划分

| 关注点（审计issue） | native / embedded（`almide-rt-core`） | stock p3 host | 一致机制 |
|---|---|---|---|
| streaming body | rt-core read loop | `consume-body` stream | shim用async `stream.read` 读入相同call-handle状态机 |
| `total_ms`（C-366） | rt-core deadline | `wasi:http` 中无 | guest侧：让 `wasi:clocks/monotonic-clock` 的 `wait-for` 与 `send` 在同一waitable set竞争；触发后 `subtask.cancel` 并drop body。guest在所有路径写 `request timeout: total_ms <n> exceeded` |
| `idle_ms` | rt-core每次read timeout | host支持时使用 `request-options` first-byte / between-bytes timeout | guest timer为权威；option只是提示（host可回答 `not-supported`） |
| connect timeout、无界response（#2825） | rt-core修复（`connect_timeout`、size cap） | host默认（wasmtime：600 s） | guest `total_ms` deadline和guest body-size limit限制所有路径 |
| 取消（C-366） | rt-core shutdown | `subtask.cancel` / drop response stream | server观测close；各路径都有fixture |
| proxy（#2819） | rt-core修复（`HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`） | host配置；wasmtime没有 | native/embedded承诺，stock明确在该行列为契约外 |
| TLS trust root（#2820） | rt-core修复（`SSL_CERT_FILE` / `SSL_CERT_DIR`、OS store） | host配置；wasmtime仅有webpki-roots | 同上。wasm guest不能、也不应自带trust store |
| header的CR/LF（#2822） | 拒绝 | 拒绝（`invalid-syntax`） | stdlib在任意路径发送前检查name/value，使各处 `err` 文本相同；`http.serve` response做相同检查 |
| 禁止header | 当时接受 | 拒绝（§2.7九个名称） | stdlib在所有路径拒绝相同九名。这是runtime `err` 而非编译拒绝，所以不是dialect epoch |
| URL解析（#2821） | 当时rt-core `parse_url` | guest必须拆为scheme / authority / path-with-query | stdlib基于 `url` module的单一parser；parts交给rt-core及 `request.set-*` |
| 1xx、chunk extension（#2824） | rt-core修复 | hyper处理 | native/embedded fixture |
| response header名 | server发送的原样 | 小写 | 全路径小写（native归一化） |
| server body限制（#2823） | `http_server_core` | shell读取request stream时计数（ADR-0020 §5.7） | 所有路径 `413` |

<a id="5-contracts"></a>

## 5. 契约

- **C-328**（client）从“native ⇄ embedded”扩为“native ⇄ embedded ⇄ stock p3 host”：status、小写header集合、body、error-code类别三者相等。分类错误的render文本三者相等，`internal-error` 则在native/embedded间相等。证据加入 `wasmtime run -S http` 下p3 fixture运行。
- **C-366**（call handle）加入stock p3路径。那里的 `total_ms`、`idle_ms`、`cancel` 由guest强制，文本相同。
- **C-367**（serve）保持ADR-0020 HTTP语义措辞，加入运行同artifact的 `wasmtime serve` 作为第三路径。
- 新契约规定header拒绝（CR/LF、九个禁止名）在所有路径为相同 `err`。

每个新增或改写条文先落入almide/als，再移动pin，然后实现（CLAUDE.md，“A new C-NNN lands in almide/als FIRST”）。

<a id="6-migration"></a>

## 6. 迁移

<a id="61-what-breaks-for-deployed-artifacts"></a>

### 6.1 对已部署artifact的破坏

- **不用HTTP的程序**：没有。逐字节保持preview-1 core module。
- **用HTTP的程序**：没有，因为以前 `almide build --target wasm` 拒绝它们（E081），不能构建。现在能构建成component。
- **`ALMIDE_COMPONENT_P3=1` 用户**：artifact不再需要 `-W component-model-async=y,component-model-more-async-builtins=y`。旧flag仍有效。环境开关对非HTTP保留含义（选择p3），对HTTP成为no-op。
- **runtime**：HTTP artifact需要WASI 0.3 host：wasmtime ≥ 46、jco、Spin 3.6+、wasmCloud 2.5+。wasmtime LTS 36不能运行。build行会说明，面向仅p1 runtime的E081 note不再适用。

<a id="62-proofstarget-availabilitytoml"></a>

### 6.2 `proofs/target-availability.toml`

- `p3-component` 路径（已在header词汇中，“尚无行声明”）成为测量路径，也是HTTP程序的**build**路径。
- `http.*` 的stock-p1行改为p3-component行，随§7 step 3–5落地缩减至空，`pending_port_ceiling` 随之下降。
- embedded的 `http.*` 行跟随p3-component，因为是同一artifact。
- `http.serve` 行从“stock p1 artifact无监听socket”移为handler export提供。

<a id="63-diagnostics"></a>

### 6.3 诊断

- **E081**仍表示“无构建路径提供此函数”。HTTP程序的理由指明WASI 0.3 component，而非preview 1。当时 `component_availability` 写作“在direct WASI 0.3 component中不可用。… `almide run --target wasm` 使用embedded host”。删除后句，因为embedded此时与component完全相同，不再有可引导用户去的其他路径。
- **E082**（两个emitter路径均拒绝程序形状）不变。
- `almide check --target wasm` 与build同样路由，E081在check时出现。

<a id="64-the-865-vm"></a>

### 6.4 #865 VM

`almide-wasm-vm` 用封闭指令集运行Critical profile程序交付的preview-1 artifact。Critical profile程序不获network capability，因此从不包含HTTP，继续构建为p1 core module。VM不需component支持，其封闭指令集不变。若VM未来运行component，应另立记录。

<a id="65-pins"></a>

### 6.5 pin

`proofs/wasi-pin-policy.toml` [runtime].flags改为 `-S http=y`。移除成对 `-W` 和 `-S p3=y`，因为最低46默认开启p3。`scripts/check-wasi-pins.sh`、`tests/component_p3_test.rs` 在同PR修改。embedded host的 `wasmtime-wasi` 与 `wasmtime-wasi-http` pin加入[runtime].crate_major（47），与 `wasmtime` 同步。wasmtime-wasi-http的p3标记experimental，p3专属修复不回移，所以pin随major前进而非patch。

<a id="7-implementation-plan"></a>

## 7. 实现计划

按以下顺序，每step一个带gate的PR。

| # | Step | Gate |
|---|---|---|
| 0 | als：重写C-328 / C-366 / C-367（§5），增加header拒绝契约，推进 `proofs/als-pin.txt` | `scripts/check-als-pin.sh` |
| 1 | **无🚝 shim**：`to_p3` 内所有同步 `stream.read/write`、`future.read/write` 改为 `[async-lower]` 加基于 `waitable-set.wait` 的共享“等至完成”helper（fs预取及http body pump已有该循环）。pin flag改为 `-S http=y` | **无 `-W`**的 `component_p3_test` green；`check-wasi-pins.sh`；test中关闭 `cm_more_async_builtins` 的wasmparser验证 |
| 2 | **错误分类**：rt-core把失败映射到 `error-code` case，rt-core一个renderer、shim有对应版本；shim render host case，不再用静态 `(p3 transport)` 文本 | 拒绝端口 / DNS / TLS fixture在native ⇄ embedded ⇄ `wasmtime run -S http` 读来相同 |
| 3 | **HTTP程序所需p3 shim覆盖**：`env.get` / `env.args` / `env.cwd`（`wasi:cli/environment@0.3.0`）、`env.sleep_ms`（monotonic `wait-for`）、`env.os` / `env.temp_dir` 为定义回答或带理由E081 | 移除这些op的p3-component行 |
| 4 | **shim的client完整性**：6个 `*_response`（`response.get-headers` → `fields.entries`）、`request_stream`、call handle（`start` / `poll` / `read_new` / `wait` / `cancel`、`request_stream_with_limits`），使用async `send`、async body read与monotonic deadline（§4.3） | 每项native ⇄ p3 cross fixture；行减少；`pending_port_ceiling` 下降 |
| 5 | **自托管SSE family**（`anthropic_streaming_call`、`openai_streaming_call` 及两个 `_with_limits`）：用call handle上的Almide实现，删除 `almide_rt_sse_*` intrinsic | 4行离开全部路径；interp bridge覆盖（3-way oracle） |
| 6 | **共享层审计**：stdlib URL parser（#2821）、CR/LF与禁止header拒绝（#2822）、guest body limit（#2823 client侧）；rt-core proxy（#2819）、trust root（#2820）、connect timeout/size cap（#2825）、1xx/chunk extension（#2824） | 各issue复现作为native/embedded fixture；stdlib检查也在p3运行 |
| 7 | **wasi:http上的embedded host**：`almide-wasm-run` 链接 `wasmtime_wasi::p3` + `wasmtime_wasi_http::p3`，用rt-core `send_request` hook（§2.2）；`almide run --target wasm` 运行HTTP程序的p3 component | **未注册**op 43..=50与53..=59时 `tests/embedded_cross_test.rs` green |
| 8 | **server export**（重新定位的ADR-0020 step 7）：`wasi:http/handler@0.3.0`，ADR-0020 §5.4形状规则、`backpressure` 串行化、shell `max_body_bytes`；embedded serve loop通过 `http_server_core` 驱动export（§3.4） | `tests/http_serve_cross_test.rs` 三路径：native、embedded、`wasmtime serve app.wasm`（无flag）；16并发probe显示每instance仅一个进行中handler |
| 9 | **默认切换**：`almide build --target wasm` 无需 `--component` 或环境开关就把HTTP程序构建为p3 component；重构 `target-availability.toml`（§6.2）；E081措辞（§6.3） | `scripts/check-target-availability.sh`；E081诊断fixture |
| 10 | **退役**host op 43..=50、53..=59、70..=72：删除 `http_call_host.rs`、`host_serve.rs` 及 `host.rs` 分支；把C-328 / C-366 / C-367证据移至component fixture | grep gate确认 `almide-wasm-run` 不注册HTTP op |
| 11 | **文档化**部署（`wasmtime run -S http`、`wasmtime serve`），写入CHEATSHEET / llms.txt / `docs/stdlib/http.md`，关闭 `docs/roadmap/on-hold/wasm-http-client.md` | `scripts/check-llm-surface.sh` |

step 1、2、6彼此独立。step 7需1、2。step 8需ADR-0020 step 0a–2（#2664、#2696、#2697、#2698）。step 9需3、4、7。step 10需7、8、9。这里不增加旧emitter代码（#1696）：shim作用于structural路径的module。

<a id="8-alternatives"></a>

## 8. 替代方案

- **保留host op并用新增op补缺失函数**（现状）。一个程序两套ABI。embedded继续提供stock host不能运行的答案，每新函数写两次：host一次、shim一次。
- **embedded用wasmtime-wasi-http默认transport（hyper + rustls）。** 少一个hook，但native/embedded不再共享client。C-328错误文本及#2819/#2820/#2824/#2825修复，会在本应同一路径的两边分歧。
- **先P2阶段**（`wasi:http@0.2`、`incoming-handler`）。owner裁定拒绝。它还是要shim的第二个interface：P2 client（`outgoing-handler`、`wasi:io` pollable）与P3（`client.send`、component-model stream）无共享。唯一优势、无需 `-W` 即load，已不存在（发现1）。
- **升级wasmtime**以获得无flag p3。不需要：flag属于guest问题（发现1）。最低46、pin 47不变。
- **所有程序都构建为component。** 对无收益程序造成1.3 KB → 18 KB成本，放弃wasmtime LTS 36，也拿走#865 VM的输入格式。
- **server用 `wasi:sockets`**（#2659选项1）。ADR-0020 §10已针对serving platform拒绝，理由仍成立。

<a id="9-refinements-the-evidence-forced"></a>

## 9. 证据迫使的细化

1. **“P3需要experimental flag”是Almide shim性质，不是P3性质。** 被门控的是🚝同步builtin；async-builtin handler在wasmtime 47无flag即可serve（§2.1）。这消除了升级wasmtime的理由与ADR-0020 §5.3前提。
2. **“无自定义op”指没有host提供的op。** 生成代码与guest shim之间op编号是从不离开artifact的内部ABI（§1.1）。
3. **embedded host保留rt-core作为transport。** 用 `send_request` hook而非hyper实现 `wasi:http`，使native/embedded继续共享一个client（§2.2、§8）。
4. **错误一致性移入guest**：一个 `error-code` 分类与renderer，native也采用（§4.2）。
5. **仅HTTP程序默认component**（§3.1第3点）。
6. **p3 shim须先提供env与args**，否则LLM client不能build（§2.3）。
7. **每instance串行化用baseline builtin `backpressure.inc/dec`**，不是guest queue（§3.3）。
8. **SSE函数成为call handle上的Almide**，不是shim代码（§3.1第1点）。

<a id="10-consequences"></a>

## 10. 后果

收益：

- 一个HTTP artifact适用于 `wasmtime run`、`wasmtime serve`、Spin、wasmCloud、jco、`almide run --target wasm`。
- 包括server在内的每个 `http` 函数都可构建到wasm，不再有只在一条wasm路径提供而另一条没有的函数。
- 审计修复每层只做一次：program观测在stdlib，transport在rt-core。
- `almide-wasm-run` 移除三类op及约320行host代码（`http_call_host.rs`、`host_serve.rs`）。

成本：

- embedded host约增加64个crate（tokio、hyper、wasmtime-wasi、wasmtime-wasi-http），依赖wasmtime的experimental p3 module。
- HTTP artifact是约30 KB的component，需要WASI 0.3 host；LTS runtime不适用。
- native transport错误措辞改变一次（§4.2）。
- stock host的proxy与trust root属于host而非Almide；契约会说明而不作承诺。
