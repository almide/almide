> 说明（固定历史）：标准库重设计 L2/L3 — process：用一个 ProcessCommand 记录替代 exec_in/exec_with_stdin/exec_status/exec_status_timeout 各轴，用 start/poll/wait/stop 句柄（#2587）替代 spawn/is_alive，移除 env/argv/sleep/stdin 的重复项。
<a id="l2l3-process"></a>

# L2/L3: process

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-process.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-process.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-process.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、独立执行probe、测试host/子进程清理或核实现行API。保留早期poll签名与后来Option内Result修订、初始保留run_in与后文移除、六句柄族门禁与后来新增child_pid、当时Int/Duration分配。这些是原提案/报告，不证明当前进程树清理或stockWASI支持；privatehostcapability与加载拒绝保持区分。原文嵌套相反的说法不擅自修正。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-process.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-25：子进程、自身退出、pid 属于 process）。
> 来源：Rust https://doc.rust-lang.org/std/process/ ・ Go https://pkg.go.dev/os/exec ・ Python https://docs.python.org/3/library/subprocess.html ・ TS/JS https://nodejs.org/api/child_process.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （没有）・ Elixir https://hexdocs.pm/elixir/System.html#cmd/3 ・ OCaml https://ocaml.org/manual/latest/api/Sys.html （`command`，`Unix.create_process` 属于 unix 库）・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.system/exit-process.html （子进程使用 JVM ProcessBuilder）・ Swift https://github.com/swiftlang/swift-subprocess
> 各语言句柄 API 的比较采用 #2587 正文中附来源的表。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 执行并取得 stdout（失败为 err） | `exec` | `Command::output` | `Cmd.Output` | `check_output` | `execFileSync` | — | `System.cmd` | — | `ProcessBuilder` | `Process` |
| 执行并取得退出码和输出 | `exec_status` | `Command::output` | `Cmd.CombinedOutput`+`ExitCode` | `run(capture_output=)` | `spawnSync` | — | `System.cmd` | `Sys.command` | `ProcessBuilder` | `Subprocess.run` |
| 指定工作目录 | `exec_in`・`run_in` | `current_dir` | `Cmd.Dir` | `cwd=` | `{cwd}` | — | `cd:` | — | `directory()` | `workingDirectory` |
| 传入 stdin | `exec_with_stdin` | `stdin(piped)` | `Cmd.Stdin` | `input=` | `{input}` | — | — | — | `redirectInput` | `input:` |
| 超时停止 | `exec_status_timeout` | — | `CommandContext` | `timeout=` | `{timeout}` | — | — | — | `waitFor(t)` | — |
| 连接终端执行 | `run`・`run_in`・`exec_attached`（已弃用） | `Command::status` | `Cmd.Run` | `run()` | `{stdio: 'inherit'}` | — | `into: IO.stream` | `Sys.command` | `inheritIO` | `Process.run` |
| 后台启动、不等待地检查 | `spawn`・`is_alive` | `spawn`+`try_wait` | `Start`+`Wait` | `Popen.poll` | `spawn` | — | `Port.open` | `Unix.create_process` | `start`+`isAlive` | `Subprocess.run` 内 |
| 停止 | `kill`（一次信号） | `kill` | `Process.Kill`・`Cmd.Cancel` | `terminate`・`kill` | `kill` | — | `Port.close` | `Unix.kill` | `destroy` | `teardownSequence` |
| 自身退出 | `exit` | `process::exit` | `os.Exit` | `sys.exit` | `process.exit` | — | `System.halt` | `exit` | `exitProcess` | `exit` |
| 自身 pid | `pid` | `process::id` | `os.Getpid` | `os.getpid` | `process.pid` | — | `System.pid` | `Unix.getpid` | `ProcessHandle.current().pid()` | `ProcessInfo.processIdentifier` |
| argv、环境变量、sleep、stdin | `args`・`env`・`sleep`・`stdin_lines` | （`env`・`thread`・`io` 中放置） | （`os`・`time`） | （`sys`・`os`・`time`） | （`process` 同置） | — | （`System`） | （`Sys`） | — | （`CommandLine`・`ProcessInfo`） |

<a id="完全性の規則"></a>

### 完备性规则

- **子进程设置集中在一个 `ProcessCommand` 记录中。** 允许 {命令、参数、工作目录、stdin 字符串、超时} 任意组合，不构造函数名组合（如 `exec_in_with_stdin`）。
- **执行形式限定为三种。**
  1. 一次执行并取得结果记录：`output(c) -> ProcessStatus`（任何退出码均为 ok；仅无法启动或超时为 err）。
  2. 不等待的运行句柄：`start`、`poll`、`wait`、`stop`、`read_new_stdout`、`read_new_stderr`。
  3. 连接终端执行：`run`、`run_in`（需要一直处于终端前台，故另设形式，#2540）。
- 仅保留一个捷径 `exec(cmd, args) -> String`（退出码 0 返回 stdout，否则将 stderr 作为 err）。与 `http.get` 一样，采用“一个捷径＋一个结果记录”。
- 结果记录只有 `ProcessStatus { code, stdout, stderr }` 一种（信号终止时 `code = -1`，与目前相同）。
- 不负责 argv、环境变量、等待、stdin（R-L1-25，属于 `env`、`io`）。

<a id="l2-の判定"></a>

### L2 判定

- 轴的乘积与缺口：{仅 stdout、全部} × {dir、stdin、timeout} 中只有四格成为函数，没有 `exec_status_in` 或 `exec_with_stdin_timeout` → R-process-02。
- 已知缺陷：`spawn` 遗留孙进程，把输出流向父进程终端，丢弃退出码（#2587 的复现）→ R-process-03。
- 重复与位置不当：`process.args`、`process.env`（纯 fn 读取环境的漏洞）、`process.sleep`、`process.stdin_lines` → R-process-01。
- 声明漏洞：`process.pid`、`process.is_alive` 是纯 `fn` 却读取 host 状态。实测可从纯 fn 调用（probe-io）→ R-process-04。
- 已弃用但无期限：`exec_attached`（#3851 A）→ R-process-02。

<a id="案"></a>

## 提案

<a id="r-process-01-argv環境変数待ちstdin-を持ち主に返す---採用"></a>

### R-process-01 将 argv、环境变量、等待、stdin 归还所属模块 — ○ 采纳

- 内容：`process.args()` → `env.args()`（不返回 argv[0]，见 L2-env 完备性规则）、`process.env(k)` → `env.get(k)`、`process.sleep(ms)` → `env.sleep_ms(ms)`、`process.stdin_lines()` → `string.lines(io.read_all()!)`。
- 依据的 L0 规则：R-L0-01、R-L0-02、R-L0-13、R-L1-25。
- (a) Rust、Go、Python、Elixir 不在子进程模块放 argv 或环境变量（facts §3.10）。
- (b) 保留侧均已通过 `wasi:cli`、`wasi:clocks` 支持 wasm。`process.env`、`process.stdin_lines` 在 wasm 也能运行（台账 2026-09-21 注记）。
- (c) `process.args` 与 `env.args` 仅是否包含 argv[0] 不同，混淆就错位一个（#3851 “Dropped”）。
- (d) 移除。`@deprecated(since=15, use="env.args")`、`use="env.get"`、`use="env.sleep_ms"`、`use="string.lines(io.read_all()!)"`，均为 `removal_epoch = 16`。`almide fix` 重写 `process.env`、`process.sleep`、`process.stdin_lines`。`process.args()` 因 argv[0] 有无导致结果错位一个，不机械重写，仅提示“`env.args()` 不包含 argv[0]（程序名），请将索引减一”。使用数：`args` 5/10/30/4、`env` 13/0/20/0、`sleep` 4/0/12/0、`stdin_lines` 0/3/11/3。
- 门禁提案：同义词门禁规则 5（#3851 F）。
- issue：#3851 B（已有）

<a id="r-process-02-processcommand-と-output-で一度きりの実行をまとめる---採用"></a>

### R-process-02 用 `ProcessCommand` 和 `output` 统一一次性执行 — ○ 采纳

- 内容：
  ```almide
  type ProcessCommand = { cmd: String, args: List[String], dir: String = "", stdin: String = "", timeout_ms: Int = 0 }
  effect fn output(c: ProcessCommand) -> ProcessStatus
  ```
  `dir = ""` 表示当前工作目录，`timeout_ms = 0` 表示无超时。超时时停止子进程及整个进程组并返回 err（原样沿用 C-214 措辞）。
  - 移除：`exec_status`（→ `output(process.ProcessCommand { cmd, args })`）、`exec_status_timeout`（→ `timeout_ms`）、`exec_in`、`exec_with_stdin`（→ `output` ＋ 检查 `code`）、`exec_attached`（epoch 10 起弃用，→ `run`）。
  - 保留：`exec`（捷径）、`run`、`run_in`、`exit`、`kill`。
  - 类型名按 `ProcessStatus`、`HttpRequest` 的先例添加 `Process` 前缀（避免生成 Rust 时与用户类型名冲突，见 `stdlib/http.almd` 开头理由）。
- 写法：带类型名的记录表达式 `process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo" }` 可省略带默认值的字段（实测，probe-process）。无类型名的 `{ cmd: …, args: … }` 不能省略，会产生 E005。标准写法确定为带类型名形式，CHEATSHEET 与 `docs/stdlib/process.md` 示例也采用此形式（退回 L0 的事项 2，已处理）。
- 依据的 L0 规则：R-L0-01（相同执行一个拼写）、CLAUDE.md“族按矩阵”。若用函数名填矩阵缺口就有 2^3 个，故用记录字段。这是对 #2587“待决定事项 2”的回答。
- (a) 将设置集中为一个值是多数做法（Rust `Command` builder、Go `exec.Cmd` 字段、Python 关键字参数、Node options、Swift `Configuration`）。
- (b) wasm：`almide:process/spawn` private host capability（ADR-0025）。stock WASI 无子进程（台账 `host-capability` 行）。`output` 是在现有 `exec-status-timeout` op 上添加 dir、stdin 的一个新 `enum op` case，遵循 ADR-0025 §3 第 4 条（WIT 测试、host 分支、cross fixture 在同一次变更中）。检验采用 ADR-0027 D4 的固定命令集合。
- (c) 消除 `exec_status_in` 这样能按规则推测、实际不存在的名称。
- (d) 新增与移除。给 `exec_status`、`exec_status_timeout`、`exec_in`、`exec_with_stdin` 加 `@deprecated(since=15, use="process.output")`，给 `exec_attached` 加 `@deprecated(since=15, use="process.run")`，均为 `removal_epoch = 16`。`almide fix` 重写：`exec_status(c, a)` → `output(process.ProcessCommand { cmd: c, args: a })`，`exec_status_timeout(c, a, t)` → `output(process.ProcessCommand { cmd: c, args: a, timeout_ms: t })`，`exec_attached(c, a)` → `run(c, a)`（均一对一）。`exec_in`、`exec_with_stdin` 返回形状变化（`String` → `ProcessStatus`），故仅提示（“`output(process.ProcessCommand { cmd, args, dir })` 的 `code` 为 0 时取 `stdout`”）。使用数：`exec_status` 10/5/10/0、`exec_status_timeout` 9/8/14/0、`exec_in` 1/2/6/0、`exec_with_stdin` 1/5/6/0、`exec_attached` 1/1/4/0。
- 门禁提案：族门禁将 `process` 公开函数集固定为 {捷径 `exec`、`output`、六个句柄函数、`run`、`run_in`、`exit`、`kill`、`pid`}。增加 `ProcessCommand` 字段时，要求同一 PR 包含 `output`、`start` 两者的 fixture。
- issue：#2587（已有，待决定事项 2 的回答）、新建（移除）

<a id="r-process-03-ハンドル2587を足しspawnis_alive-を置き換える---採用"></a>

### R-process-03 新增句柄（#2587），替代 `spawn`、`is_alive` — ○ 采纳

- 内容：
  ```almide
  effect fn start(c: ProcessCommand) -> ProcessChild          // 自分のプロセスグループで起動。stdin は stdin 欄、出力はランタイムが吸い上げる
  effect fn poll(c: ProcessChild) -> ProcessStatus?            // 待たない。終わっていれば片付けて結果
  effect fn wait(c: ProcessChild) -> ProcessStatus             // 終わるまで待つ
  effect fn stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus  // グループに SIGTERM → grace_ms 待つ → SIGKILL → 片付け。何度呼んでもよい
  effect fn read_new_stdout(c: ProcessChild) -> String         // 前回から届いた分。待たない
  effect fn read_new_stderr(c: ProcessChild) -> String
  ```
  移除：`spawn`（→ `start`）、`is_alive`（→ `poll(c)! == none`）。`kill(pid, signal)` 有向非自身子进程的 pid 发信号的职责，因此保留。
- 保留代码注释的译文：在独立进程组启动，stdin 使用 stdin 字段、输出由 runtime 吸收；poll 不等待，已结束则清理并返回结果；wait 等待结束；stop 向组发 SIGTERM、等待 grace_ms、发 SIGKILL、清理，可重复调用；stdout 读取上次以来的新内容，不等待。
- 拼写选择（R-L0-01，跨模块）：相同职责沿用已有拼写。不等待确认 → `http.poll`，等到结束 → `http.wait`，读取上次以来差量 → `http.read_new`。不采用 #2587 草案中的 `try_wait`、`read_stdout`。
- 对 #2587 待决定事项的回答：1 `grace_ms` 为必需参数（语言无默认参数）。2 使用 `ProcessCommand`（R-process-02）。3 结果只用 `ProcessStatus`，不另加 `Exit` 类型（不为同一结果建两种类型，R-L0-01）。4 丢弃句柄不停止子进程，由 runtime 在程序退出时停止子进程树。
- 时间参数采用 `Int` 毫秒。与本模块、`env.sleep_ms`、`HttpLimits`、`net.tcp_read_timeout` 对齐，也遵循 L2-datetime 退回事项的决定：“时间量采用名称含单位的 `Int`，`Duration` 仅用于 `fan` 预算与期限”（退回 L0 的事项 1，已处理）。
- (a) Rust、Go、Python、Zig、.NET 将启动设置与运行中子进程句柄分离，并提供不等待的检查（#2587 表）。Go、execa、trio、swift-subprocess 采用“温和信号 → 宽限 → 强制”停止方式。
- (b) wasm：向与 R-process-02 相同的 `almide:process/spawn` 添加 `enum op` case（start、poll、wait、stop、read）（ADR-0025 §3 第 4 条）。stock WASI 在加载时拒绝（host-capability）。ADR-0027 D3 verdict：`poll` 运行中为 none，结束后为 some。
- (c) `spawn` 三个缺陷（孙进程遗留、输出混杂、退出码消失）即便 LLM 编写测试也难发现。
- (d) 新增与移除。`spawn` 为 `@deprecated(since=15, use="process.start")`，`is_alive` 为 `@deprecated(since=15, use="process.poll")`，均为 `removal_epoch = 16`。参数从 pid 变成句柄，因此 `almide fix` 不重写，只提示（“通过 `let c = process.start(process.ProcessCommand { cmd, args })!` 启动，再以 `process.poll(c)! == none` 检查是否运行”）。使用数：`spawn` 10/4/8/0、`is_alive` 12/0/8/0。
- 门禁提案：直接将 #2587 完成条件制成 fixture（`stop` 后无孙进程遗留、`poll` 从 none → some、输出不进入父进程 stdout、`exit` 不遗留子进程树）。按 `http_call_family_gate_test.rs` 相同形式固定句柄动词集合。
- issue：#2587（已有）

<a id="r-process-04-pid-を-effect-fn-にする---採用"></a>

### R-process-04 将 `pid` 改为 effect fn — ○ 采纳

- 内容：`fn pid() -> Int` 改为 `effect fn pid() -> Int`（`is_alive` 由 R-process-03 移除）。
- 依据的 L0 规则：R-L0-13（读取环境的函数为 effect）、ADR-0022 D3（纯 fn 不读取外部）。
- (b) 仅检查器变化。(c) 不必按模块记住返回相同值的函数是纯还是 effect。
- (d) 语义变化（纯 → effect），名称不变。按 epoch 8、13 先例列入 epoch 15 的 `breaks`。纯 fn 调用处产生 E006，提示“将调用方改为 `effect fn`”。使用数 2/12/11/0。
- 门禁提案：将“读取 host 状态的 intrinsic 为 effect”纳入 R-L0-G，从允许列表移除 `process.pid`。
- issue：#3907

<a id="r-process-05-processpollwait-の形を-httppollwait-に合わせる---採用"></a>

### R-process-05 将 `process.poll`、`wait` 形状对齐 `http.poll`、`wait` — ○ 采纳

- 当前：相对于 `http.poll(c) -> Result[HttpResponse, String]?`（交换失败是 some 内的 err），新增的 `process.poll(c) -> ProcessStatus?` 嵌套顺序相反，无处放 err。通过带 `timeout_ms` 的 `start` 启动的子进程超时时无法表达结果。
- 提案：`effect fn process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?`。`wait`、`stop` 与 `http.wait` 一样，用 effect err 返回子进程失败（超时、清理失败）。句柄族约定只有一个：“`poll` 不等待，运行中 none，结束后 some(ok(结果)) 或 some(err(原因))”。
- 依据的 L0 规则：R-L0-01（相同职责一种形状，R-process-03 借用了 `http.poll` 拼写却颠倒嵌套）、R-L0-17。
- (a) Rust `Child::try_wait` 是 `io::Result<Option<ExitStatus>>`，Python `Popen.poll()` 运行中为 `None`。两者都区分不等待的检查与结果。
- (c) 学会 `match http.poll(c) { some(ok(r)) => …, some(err(e)) => …, none => … }` 的 LLM 对 `process.poll` 也会写相同形式。
- (d) 只改变新函数声明。
- 完备性规则：持有句柄的模块（http、process），`poll` 为 `Result[T, String]?`，`wait` 为 `T`（失败为 effect err）。
- 门禁提案：将 `http_call_family_gate_test.rs` 扩为“句柄族门禁”，检查 process 声明形状相同。
- issue：#3974

<a id="r-process-06-動いている子の-pid-を読む-processchild_pid-を足す---採用"></a>

### R-process-06 新增读取运行中子进程 pid 的 `process.child_pid` — ○ 采纳

- 提案：`effect fn process.child_pid(c: ProcessChild) -> Int`。
- (a) 六种语言提供：Go `cmd.Process.Pid`、Python `Popen.pid`、Rust `Child::id`、Node `child.pid`、Kotlin `Process.pid()`、Swift `processIdentifier`。
- (b) native 为 OS pid，wasm 为 `almide:process/spawn` 返回的 pid（R-L0-15 第四条路）。
- (c) 移除 `spawn` 后无法取得向子进程发 `kill` 的 pid。给子进程发 SIGINT、将 pid 写入日志很常见。
- (d) 新增 1。
- issue：#3974

<a id="r-process-07-run-も-processcommand-を受け取りrun_in-を撤去する---採用"></a>

### R-process-07 `run` 也接收 `ProcessCommand`，移除 `run_in` — ○ 采纳

- 当前：`process.run(cmd, args) -> Int` 与 `process.run_in(dir, cmd, args) -> Int`。R-process-02 完备性规则规定“设置集中于一个记录，不组合函数名”，但 `run_in` 仍将工作目录轴放在名称里。标准函数无默认参数（R-string-08），因此用记录传设置才符合规则。
- 提案：改为 `effect fn process.run(c: ProcessCommand) -> Int`（stdin 字段为空则直接传递终端），移除 `run_in`。仅保留 `exec(cmd, args)` 一个只传参数的捷径。
- 依据的 L0 规则：R-process-02 完备性规则、CLAUDE.md“族按矩阵”。
- (a) Rust `Command::status`、Go `Cmd.Run`、Python `run()` 接收与其他执行形式相同的设置值。
- (c) 可避免想给 `run` 加超时的 LLM 猜写 `run_timeout`。
- (d) 语义变化（参数类型改变，旧调用为类型错误，符合 R-L0-14.4）与移除 1 个。`almide fix` 将 `run(c, a)` 重写为 `run(ProcessCommand { cmd: c, args: a, ... })`，将 `run_in(d, c, a)` 重写为添加 `dir: d` 的形式。使用数：`run` 1/6/7/0、`run_in` 2/5/8/0。
- issue：#3974

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，标明 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `process.exec` | `effect fn exec(cmd: String, args: List[String]) -> String` | Go `Output` / Python `check_output` / Node `execFileSync` | 保留（捷径） | R-process-02 |
| `process.exec_in` | `effect fn exec_in(dir: String, cmd: String, args: List[String]) -> String` | Rust `current_dir` / Python `cwd=` | 移除（→ `output`） | R-process-02 |
| `process.exec_with_stdin` | `effect fn exec_with_stdin(cmd: String, args: List[String], input: String) -> String` | Python `input=` / Node `{input}` | 移除（→ `output`） | R-process-02 |
| `process.exec_status` | `effect fn exec_status(cmd: String, args: List[String]) -> ProcessStatus` | Rust `output` / Python `run` | 移除（→ `output`） | R-process-02 |
| `process.exec_status_timeout` | `effect fn exec_status_timeout(cmd: String, args: List[String], timeout_ms: Int) -> ProcessStatus` | Python `timeout=` / Go `CommandContext` | 移除（→ `output` 的 `timeout_ms`） | R-process-02 |
| `process.output` | `effect fn output(c: ProcessCommand) -> ProcessStatus`（提案） | Rust `Command::output` / Go `Cmd.Output` | 新增 | R-process-02 |
| `process.run` | `effect fn run(cmd: String, args: List[String]) -> Int` | Rust `status` / Go `Run` / Python `run()` | 语义变化（`ProcessCommand` 接收） | R-process-07 |
| `process.run_in` | `effect fn run_in(dir: String, cmd: String, args: List[String]) -> Int` | 同上 ＋ cwd | 移除 → `run(ProcessCommand { dir: … })` | R-process-07 |
| `process.exec_attached` | `effect fn exec_attached(cmd: String, args: List[String]) -> Int`（已弃用） | — | 移除（→ `run`） | R-process-02 |
| `process.spawn` | `effect fn spawn(cmd: String, args: List[String]) -> Int` | Rust `spawn` / Node `spawn` / Go `Start` | 移除（→ `start`） | R-process-03 |
| `process.start` | `effect fn start(c: ProcessCommand) -> ProcessChild`（提案） | Go `Start` / Rust `spawn` / Kotlin `start` | 新增 | R-process-03 |
| `process.poll` | `effect fn poll(c: ProcessChild) -> Result[ProcessStatus, String]?`（提案，R-process-05） | Python `Popen.poll` / Rust `try_wait` | 新增 | R-process-03 |
| `process.wait` | `effect fn wait(c: ProcessChild) -> ProcessStatus`（提案） | Rust・Go・Python `wait` | 新增 | R-process-03 |
| `process.stop` | `effect fn stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus`（提案） | Go `Cancel`+`WaitDelay` / execa `forceKillAfterDelay` | 新增 | R-process-03 |
| `process.read_new_stdout` | `effect fn read_new_stdout(c: ProcessChild) -> String`（提案） | — （`http.read_new` 对齐） | 新增 | R-process-03 |
| `process.read_new_stderr` | `effect fn read_new_stderr(c: ProcessChild) -> String`（提案） | 同上 | 新增 | R-process-03 |
| `process.is_alive` | `fn is_alive(pid: Int) -> Bool` | Kotlin `isAlive` / Rust `try_wait` | 移除（→ `poll`） | R-process-03 |
| `process.kill` | `effect fn kill(pid: Int, signal: Int) -> Unit` | Rust `kill` / Python `os.kill` / Node `process.kill` | 保留 | R-process-03 |
| `process.exit` | `effect fn exit(code: Int) -> Never` | Rust・Go・Python・OCaml・Swift `exit` | 保留 | — |
| `process.pid` | `fn pid() -> Int` | Rust `id` / Go `Getpid` / Node `pid` | 语义变化（改为 effect fn） | R-process-04 |
| `process.args` | `effect fn args() -> List[String]` | Rust `env::args` | 移除（→ `env.args`） | R-process-01 |
| `process.env` | `fn env(key: String) -> String?` | Rust `env::var` | 移除（→ `env.get`） | R-process-01 |
| `process.sleep` | `fn sleep(ms: Int) -> Unit` | Rust `thread::sleep` | 移除（→ `env.sleep_ms`） | R-process-01 |
| `process.stdin_lines` | `effect fn stdin_lines() -> List[String]` | Python `readlines` | 移除（→ `string.lines(io.read_all()!)`） | R-process-01 |
| `process.child_pid` | `effect fn child_pid(c: ProcessChild) -> Int`（提案） | Go `Process.Pid` / Python `Popen.pid` / Rust `Child::id` | 新增 | R-process-06 |

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0 的事项（已处理）

1. **实际时间长度的参数类型**：L2-datetime 退回事项已决定（不采用 `Duration`）。时间量统一使用名称含单位的 `Int`（`sleep_ms`、`timeout_ms`、`grace_ms`）。`Duration` 按 ADR-0001 仅保留用于 `fan` 预算与期限。本页 `timeout_ms: Int`、`grace_ms: Int` 不变。
2. **无类型名的匿名记录表达式不能省略带默认值字段**（实测 E005 “expects Cmd but got { cmd, args }”）：`ProcessCommand` 保持使用带类型名的 `process.ProcessCommand { … }`。此形式可省略带默认值字段（实测）。从期望类型补默认值属于语言类型检查而非标准函数设计，在本轮重设计范围外处理。标准库侧可做的是在 R-process-02 门禁（fixture）中包含 E005 提示“采用带类型名的 `process.ProcessCommand { … }`”。
