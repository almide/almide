说明：stdlib 重设计 L2/L3 — env 拥有 argv、环境变量、cwd、OS 名称和 sleep；process/args/fs/datetime 中重复项撤去，归入所属模块。
<a id="l2l3-env"></a>

# L2/L3: env

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-env.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-env.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-env.zh-CN.md)

> 非规范性历史重设计参照版，原本权威且不变。不自动同步、无新probe、未核实现行实现。保留remove初始Unit提案与后段实现/表Result、set表Unit与err说明差异。不擅选未定effect/Option调用形式。Secret显示/常数时间为原契约主张，不是普遍防泄漏或独立安全保证，未访问凭据。Duration拒绝为带日期决定，不与其他ADR时代强行统一。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md)

> 最后更新：2026-10-26
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-25 分工表是本页基础）。
> 来源：Rust https://doc.rust-lang.org/std/env/；Go https://pkg.go.dev/os；Python https://docs.python.org/3/library/os.html；TS/JS https://nodejs.org/api/process.html；Gleam https://gleam-stdlib.hexdocs.pm/（stdlib 无 env）；Elixir https://hexdocs.pm/elixir/System.html；OCaml https://ocaml.org/manual/latest/api/Sys.html；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.system/；Swift https://developer.apple.com/documentation/foundation/processinfo。
> 9 种语言名称取自已核实表：[facts/naming-conventions.md §20](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md)、[facts/categories.md §1.H、§3.10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| argv（参数） | `env.args`（不含 argv[0]）・`process.args`（包含）・`args.raw`（= env.args） | `env::args`（包含） | `os.Args`（包含） | `sys.argv`（包含） | `process.argv`（包含）・`Deno.args`（不含） | — | `System.argv`（不含） | `Sys.argv`（包含） | `main(args)`（不含） | `CommandLine.arguments`（包含） |
| 读一个环境变量 | `env.get`・`process.env` | `env::var` | `os.LookupEnv` | `os.environ.get` | `process.env.K` | — | `System.get_env/1` | `Sys.getenv_opt` | `System.getenv`(JVM) | `ProcessInfo.environment[k]` |
| 读全部环境变量 | — | `env::vars` | `os.Environ` | `os.environ` | `process.env` | — | `System.get_env/0` | `Unix.environment` | `System.getenv()`(JVM) | `ProcessInfo.environment` |
| 设置环境变量 | `env.set` | `env::set_var` | `os.Setenv` | `os.environ[k] = v` | `process.env.K = v` | — | `System.put_env` | `Unix.putenv` | — | `setenv`(libc) |
| 删除环境变量 | — | `env::remove_var` | `os.Unsetenv` | `del os.environ[k]` | `delete process.env.K` | — | `System.delete_env` | — | — | `unsetenv`(libc) |
| 工作目录 | `env.cwd` | `env::current_dir` | `os.Getwd` | `os.getcwd` | `process.cwd()` | — | `File.cwd` | `Sys.getcwd` | JVM `user.dir` | `FileManager.currentDirectoryPath` |
| OS 名称 | `env.os` | `env::consts::OS` | `runtime.GOOS` | `sys.platform` | `process.platform` | — | `:os.type` | `Sys.os_type` | — | `ProcessInfo.operatingSystemVersion` |
| 临时目录 | `env.temp_dir`・`fs.temp_dir` | `env::temp_dir` | `os.TempDir` | `tempfile.gettempdir` | `os.tmpdir()` | — | `System.tmp_dir` | `Filename.get_temp_dir_name` | JVM `java.io.tmpdir` | `FileManager.temporaryDirectory` |
| 等待（sleep） | `env.sleep_ms`・`process.sleep` | `thread::sleep` | `time.Sleep` | `time.sleep` | `setTimeout`（Node `timers/promises`） | — | `Process.sleep` | `Unix.sleep` | `Thread.sleep`(JVM) | `Thread.sleep` |
| 当前时间（实时间） | `env.unix_timestamp`・`env.millis`・`datetime.now` | `time::SystemTime` | `time.Now` | `time.time` | `Date.now()` | — | `System.os_time` | `Unix.time` | `Clock.System.now()` | `Date()` |

<a id="完全性の規則"></a>

### 完整性规则

- `env` 拥有“本进程从外部收到的东西”（R-L1-25）：argv、环境变量、工作目录、OS 名称、等待。
- 环境变量凑齐四项：{读一个 `get`、读全部 `vars`、设置 `set`、删除 `remove`}。有 `set` 无 `remove`，就无法在传给子进程前删除变量。
- 时钟属 `datetime`，临时目录属 `fs`，子进程/退出/pid 属 `process`，flag 读取属 `args`；`env` 不放它们的别名（R-L0-01）。
- 有意不放 argv[0]（程序名）。实测 native `almide run` 中为临时目录内 binary 路径，wasm 为 `wasm-harness`（probe-args）。不同运行方式值不同，可移植程序不能依赖（R-L0-15）。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`env.temp_dir`＝`fs.temp_dir`、`env.unix_timestamp`＝`datetime.now`、`process.env`＝`env.get`、`process.sleep`＝`env.sleep_ms`、`args.raw`＝`env.args` → R-env-01。
- 位置错误：`env.millis` 是时钟，应属 `datetime` → R-env-01。
- 缺失角色：读全部环境变量（7/9）、删除（6/9）→ R-env-02。
- 调用形式不一致：仅 `env.get` 可不用 `!` 直接写 `??` → R-env-03（L0 R-L0-17 解决）。

<a id="案"></a>

## 提案

<a id="r-env-01-os-まわりの持ち分を-1-つにする---採用"></a>

### R-env-01 统一 OS 周边分工 — ○ 采纳

- 内容：
  - 撤去 `env.temp_dir`（→ `fs.temp_dir()`）、`env.unix_timestamp`（→ `datetime.now()`）。
  - 移动 `env.millis` → `datetime.now_ms()`（L2-datetime 增加，同一 body）。
  - 集中至此（各页撤去）：`process.env(k)` → `env.get(k)`、`process.sleep(ms)` → `env.sleep_ms(ms)`、`args.raw()` → `env.args()`。
  - 撤去含 argv[0] 的 `process.args()`，替换为 `env.args()`。不加读取 argv[0] 的函数（见完整性规则）。
  - 修正 `env.sleep_ms` 过时说明“a negative ms hangs”。实测负值立即返回（probe-env，native/wasm 两者）。
- L0 依据：R-L0-01（同角色一个名称）、R-L0-02（主体类型/所属模块）、R-L0-13（`process.env` 仍是纯 fn 却读取环境的漏洞）、L1 R-L1-25。
- (a) argv/环境变量同模块的有 Rust `std::env`、Go `os`、Elixir `System`、OCaml `Sys`。仅 Elixir/OCaml 把时间放 `env`；Rust/Go 是别模块。
- (b) native/wasm：全部直接使用保留侧 body。`env.get`、`env.args`、`env.sleep_ms`、`env.cwd`、`env.os`、`env.temp_dir` 在 p3 component 通过 `wasi:cli/environment@0.3.0` 和 monotonic clock `wait-for`，embedded host 通过已有 op（`proofs/target-availability.toml` 无 wall 行）。`datetime.now_ms` 用 `wasi:clocks/wall-clock`。ADR-0027 D3 检验以“时钟是否在窗口内”、sleep“单调时钟差是否至少 k ms”的 verdict 行比较。
- (c) 文档同时有 `env.temp_dir` 和 `fs.temp_dir`（CHEATSHEET 用 fs，llms.txt 用 env）。统一后，不论 LLM 记住哪边都到同一答案。`env.args` 与 `process.args` 是否含 argv[0] 不同，易造成偏移一位的错误。
- (d) 撤去/移动。兼容：`env.temp_dir` 标 `@deprecated(since=15, use="fs.temp_dir")`，`env.unix_timestamp` 标 `use="datetime.now"`，`env.millis` 标 `use="datetime.now_ms"`，`[[deprecation]]` 中 `removal_epoch = 16`（`process.args`、`process.env`、`process.sleep` 在 L2-process R-process-01，`args.raw` 在 L2-args R-args-02 同式记录）。`almide fix` 一对一改写：`env.temp_dir()` → `fs.temp_dir()`、`env.unix_timestamp()` → `datetime.now()`、`env.millis()` → `datetime.now_ms()`。使用数（spec/tests/docs/stdlib）：`env.temp_dir` 9/0/22/3、`env.unix_timestamp` 5/0/16/5、`env.millis` 11/7/21/3。
- 门禁提案：同义词门禁规则 5（跨模块同 body，#3851 F）及 R-L0-01“只转发调用一次其他公开函数也算同角色”，防止 `args.raw` 类转发重现。
- issue：#3851 B（已有）。`env.millis` 移动追加到同 issue。

<a id="r-env-02-envvars-と-envremove-を足す---採用"></a>

### R-env-02 增加 `env.vars` 和 `env.remove` — ○ 采纳

- 内容：`effect fn vars() -> Map[String, String]`（全部环境变量，按键排序 map）、`effect fn remove(name: String) -> Unit`（从本进程及以后启动的子进程删除；不存在也可）。
- L0 依据：R-L0-15（只加 WASI 能承载的内容）、L1 标准 1（至少 5 种语言）。
- (a) 读全部 7/9，删除 6/9。名称分散（`vars`/`Environ`/`environ`/`get_env`）。为与 `set` 成对，选接近 Rust `vars`/`remove_var` 的 `vars`、`remove`。
- (b) `vars` 就是 `wasi:cli/environment` 的 `get-environment`。`remove` 同 `env.set`，通过 p1 shim guest 侧覆盖记录（op 37、#3223），wasm 行为相同。不需新 host op。结果在 ADR-0027 D2 固定 env 下确定。
- (c) “输出全部环境变量”的任务中，LLM 按其他语言习惯寻找一个函数；没有就写不出。
- (d) 增加，使用数 0。
- 门禁提案：family 矩阵测试确认 env 四项 get/vars/set/remove 都存在（`tests/env_variable_family_gate_test.rs`）。
- 实现（#3908）：`remove` 与 `set` 同样检查名字（空、`=`、NUL），所以同 `set` 一样返回 `Result[Unit, String]`（如 `env.remove: empty name`）。
- issue：#3908。

<a id="r-env-03-envget-を--で受ける形を自前の-effect-fn-と同じにする---採用r-l0-17-に従う"></a>

### R-env-03 让 `env.get` 用 `??` 接收的形式与自定义 effect fn 一致 — ○ 采纳（遵循 R-L0-17）

- 实测（probe-io）：即使返回 `String?` 的 effect fn，body 为 `@intrinsic` 或 Almide 时调用类型不同。
  - `env.get("X") ?? "d"` 通过；`env.get("X")!` 将 none 作为 err `"none"` 传播（`match env.get("X")! { some(..) => … }` 错误为 String 不匹配 some）。`io.read_line` 在 R-io-01 返回 `Option` 的版本也相同。
  - `args.option("x") ?? "d"` 错误，须写 `args.option("x")! ?? "d"`（#3522 形式）。
- 内容：遵循 L0 R-L0-17（调用类型只取决于声明），把 `env.get`、`io.read_line`、`args.option`、`args.positional_at` 调用类型统一成仅由 `effect fn … -> T?` 决定的一个形式。选直接 `??` 还是先 `!` 再 `??`，由 R-L0-17 issue 依 ADR-0002/ADR-0008 设计。本页函数只遵循结果，不改名/声明。
- L0 依据：R-L0-17。
- (a) 不适用（没有语言把 effect/Option 设为独立轴）。
- (b) 仅 checker 变更，两 target 相同。
- (c) 同声明因 body 写法而变调用方式，LLM 读声明也无法确定正确调用。
- (d) 语言变更。若统一结果改变现有调用，按 R-L0-17 记入 epoch 15 `breaks`，附提示（如“写 `env.get(k)! ?? d`”，依所选形式）及 `almide fix` 改写。函数名不变。
- 门禁提案：R-L0-17 的 fixture 检查所有标准 effect fn intrinsic/Almide 版接受 `??`、`!` 相同，加入 `env.get`/`args.option` 对。
- issue：同 R-L0-17（新建）。
- issue：#3900。

<a id="r-env-04-待ち時間タイムアウトの引数を-duration-にする---不採用"></a>

### R-env-04 把等待/timeout 参数改为 `Duration` — × 不采纳

- 内容：把 `env.sleep_ms(ms: Int)`、`net.tcp_set_timeout(stream, timeout_ms: Int)`、`net.tcp_read_timeout`、`unix.select(xs, timeout_ms: Int)`、`process.stop(c, grace_ms: Int)` 时间参数改 `Duration`，把 `sleep_ms` 改名 `sleep`。
- 理由：9 种语言中用时间类型接收等待量的仅 Go（`time.Duration`）、Rust（`Duration`）、Swift（`Duration`）、Kotlin（`delay` 的 `Duration` 版）4 种，未达 5 种。JS/Elixir/Kotlin 接收毫秒整数，Python/OCaml 接收秒数。Almide 决定用名称带单位的 `Int` 表达量（`sleep_ms`、`add_seconds`；L2-datetime“接收时间量函数的类型”）。按 ADR-0001，`Duration` 用于 `fan` 预算/期限，无数值读取函数（S7）。只让等待函数用 `Duration`，会让持有 `Int` 毫秒的调用者多包 `duration.ms(x)`；也没有证据表明比当前单位名称更少误读。
- 重审条件：决定给 `Duration` 加读取函数时（重审 ADR-0001 S7）。
- issue：无。

<a id="r-env-05-資格情報を-secretstring-で読む-envsecret-を足す---採用2026-10-26"></a>

### R-env-05 增加以 `Secret[String]` 读取凭据的 `env.secret` — ○ 采纳（2026-10-26）

- 当前：API token/webhook key 由 `env.get` 作为 `String` 读取。`String` 在日志、assert 失败等所有显示中原样输出，`==` 在首个不同字节返回，因此比较时间泄漏 token（安全表第 20 行）。
- 提案：`effect fn env.secret(name: String) -> Secret[String]?`。把 `env.get(name)` 同一值用 `secret.new` 包装返回。各处显示为 `Secret(***)`（C-425），`==` 为常数时间比较（C-426）；`env.get` 保持。
- 依据：秘密从产生处起就是 `Secret`（L2-secret）。`hash.hmac_sha256` 的 key 已变 `Secret[Bytes]`（L2-hash R-hash-07），来源也应返回 `Secret`。
- 契约：C-438（`spec/wasm_cross/env_secret.almd`）。
- issue：无。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当时，显示 mut） | 其他语言主要名称 | 判定 | 提案 |
|---|---|---|---|---|
| `env.args` | `effect fn args() -> List[String]` | Rust `env::args` / Go `os.Args` / Python `sys.argv` / Elixir `System.argv`（argv[0] 不包含的是 Elixir/Kotlin/Deno） | 保持 | R-env-01（集中于此） |
| `env.get` | `effect fn get(name: String) -> String?` | Rust `var` / Go `LookupEnv` / Python `environ.get` / OCaml `getenv_opt` / Elixir `get_env` | 保持 | R-env-01、R-env-03 |
| `env.set` | `effect fn set(name: String, value: String) -> Unit` | Rust `set_var` / Go `Setenv` / Elixir `put_env` | 保持（空名字、名字中的 `=` 或 NUL、值中的 NUL 为 err；#4200、C-421） | — |
| `env.cwd` | `effect fn cwd() -> String` | Rust `current_dir` / Go `Getwd` / Python `getcwd` / OCaml `getcwd` | 保持 | — |
| `env.os` | `fn os() -> String` | Rust `consts::OS` / Go `GOOS` / OCaml `os_type` | 保持（各 target 常量；C-189） | — |
| `env.sleep_ms` | `effect fn sleep_ms(ms: Int) -> Unit` | Rust `thread::sleep` / Go `time.Sleep` / Python `time.sleep` / Elixir `Process.sleep` | 保持（仅修正说明） | R-env-01 |
| `env.temp_dir` | `effect fn temp_dir() -> String` | Rust `env::temp_dir` / Go `os.TempDir` / Elixir `System.tmp_dir` | 撤去（→ `fs.temp_dir`） | R-env-01 |
| `env.unix_timestamp` | `effect fn unix_timestamp() -> Int` | Python `time.time` / Elixir `os_time` | 撤去（→ `datetime.now`） | R-env-01 |
| `env.millis` | `effect fn millis() -> Int` | JS `Date.now()` / Elixir `os_time(:millisecond)` | 撤去（→ `datetime.now_ms` 移动） | R-env-01 |
| `env.vars` | `effect fn vars() -> Map[String, String]` | Rust `vars` / Go `Environ` / Python `environ` / Elixir `get_env/0` | 增加 | R-env-02 |
| `env.remove` | `effect fn remove(name: String) -> Result[Unit, String]` | Rust `remove_var` / Go `Unsetenv` / Elixir `delete_env` | 增加 | R-env-02 |
| `env.secret` | `effect fn secret(name: String) -> Secret[String]?` | — | 增加（值产生即为 `Secret`） | R-env-05 |

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0（已处理）

1. **返回 Option 的 effect fn 调用类型**（R-env-03）：L0 已有 R-L0-17（调用类型只取决于声明）。停止 `@intrinsic` body 为 `Option[T]`、Almide body 为 `Result[Option[T], String]` 的双重处理，统一为声明决定的形式。具体选哪种由新 R-L0-17 issue 设计。影响 `env.get`、`io.read_line`、`args.option`、`args.positional_at` 及用户 effect fn。R-env-03 已标 ○。
