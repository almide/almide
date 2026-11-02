> Description (fixed history): Stdlib redesign L2/L3 — process: one ProcessCommand record replaces the exec_in/exec_with_stdin/exec_status/exec_status_timeout axes, a start/poll/wait/stop handle (#2587) replaces spawn/is_alive, and the env/argv/sleep/stdin twins leave.
<a id="l2l3-process"></a>

# L2/L3: process

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-process.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-process.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-process.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, independent probe execution, host/child-process cleanup testing or current API verification. Preserve initial poll signatures versus later Result-inside-Option revision, initial run_in retention versus later removal, six-handle family gate versus later child_pid addition, and historical Int/Duration allocation. These are original proposals/reported observations, not proof of present process-tree cleanup or stock WASI support; private host capability and load-time rejection remain distinct. Original reversed-nesting wording is retained, not silently repaired. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-process.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-25: child processes, exiting oneself and pid belong to process).
> Sources: Rust https://doc.rust-lang.org/std/process/ / Go https://pkg.go.dev/os/exec / Python https://docs.python.org/3/library/subprocess.html / TS/JS https://nodejs.org/api/child_process.html / Gleam https://gleam-stdlib.hexdocs.pm/ (absent) / Elixir https://hexdocs.pm/elixir/System.html#cmd/3 / OCaml https://ocaml.org/manual/latest/api/Sys.html (`command`; `Unix.create_process` is in the unix library) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.system/exit-process.html (child processes use JVM ProcessBuilder) / Swift https://github.com/swiftlang/swift-subprocess
> Handle API comparisons use the sourced table in issue #2587's body.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Execute and receive stdout (failure is err) | `exec` | `Command::output` | `Cmd.Output` | `check_output` | `execFileSync` | — | `System.cmd` | — | `ProcessBuilder` | `Process` |
| Execute and receive exit code/output | `exec_status` | `Command::output` | `Cmd.CombinedOutput`+`ExitCode` | `run(capture_output=)` | `spawnSync` | — | `System.cmd` | `Sys.command` | `ProcessBuilder` | `Subprocess.run` |
| Specify working directory | `exec_in`・`run_in` | `current_dir` | `Cmd.Dir` | `cwd=` | `{cwd}` | — | `cd:` | — | `directory()` | `workingDirectory` |
| Supply stdin | `exec_with_stdin` | `stdin(piped)` | `Cmd.Stdin` | `input=` | `{input}` | — | — | — | `redirectInput` | `input:` |
| Stop on timeout | `exec_status_timeout` | — | `CommandContext` | `timeout=` | `{timeout}` | — | — | — | `waitFor(t)` | — |
| Run attached to terminal | `run`・`run_in`・`exec_attached`(deprecated) | `Command::status` | `Cmd.Run` | `run()` | `{stdio: 'inherit'}` | — | `into: IO.stream` | `Sys.command` | `inheritIO` | `Process.run` |
| Launch in background and check without waiting | `spawn`・`is_alive` | `spawn`+`try_wait` | `Start`+`Wait` | `Popen.poll` | `spawn` | — | `Port.open` | `Unix.create_process` | `start`+`isAlive` | inside `Subprocess.run` |
| Stop | `kill`(one signal) | `kill` | `Process.Kill`・`Cmd.Cancel` | `terminate`・`kill` | `kill` | — | `Port.close` | `Unix.kill` | `destroy` | `teardownSequence` |
| Exit oneself | `exit` | `process::exit` | `os.Exit` | `sys.exit` | `process.exit` | — | `System.halt` | `exit` | `exitProcess` | `exit` |
| Own pid | `pid` | `process::id` | `os.Getpid` | `os.getpid` | `process.pid` | — | `System.pid` | `Unix.getpid` | `ProcessHandle.current().pid()` | `ProcessInfo.processIdentifier` |
| argv / environment variables / sleep / stdin | `args`・`env`・`sleep`・`stdin_lines` | (`env`・`thread`・`io` modules) | (`os`・`time`) | (`sys`・`os`・`time`) | (together in `process`) | — | (`System`) | (`Sys`) | — | (`CommandLine`・`ProcessInfo`) |

<a id="完全性の規則"></a>

### Completeness rules

- **Collect child-process settings in one `ProcessCommand` record.** Allow any combination of {command, arguments, working directory, stdin string, timeout}; do not create combinations of function names (such as `exec_in_with_stdin`).
- **Limit execution to three forms.**
  1. Run in one operation and receive a result record: `output(c) -> ProcessStatus` (any exit code is ok; err only for failure to start or timeout).
  2. Nonblocking running handle: `start`, `poll`, `wait`, `stop`, `read_new_stdout`, `read_new_stderr`.
  3. Run attached to the terminal: `run`, `run_in` (distinct because it must remain in the terminal foreground; #2540).
- Only one shortcut: `exec(cmd, args) -> String` (stdout for exit code 0; otherwise stderr as err). Like `http.get`, the shape is “one shortcut + one result record”.
- Only one result record: `ProcessStatus { code, stdout, stderr }` (`code = -1` on signal termination, as currently).
- Do not own argv, environment variables, sleeping or stdin (R-L1-25; those belong to `env`/`io`).

<a id="l2-の判定"></a>

### L2 verdict

- Multiplying axes and gaps: only four cells of {stdout only, all} × {dir, stdin, timeout} have functions; `exec_status_in` and `exec_with_stdin_timeout` do not exist → R-process-02.
- Known defects: `spawn` leaves grandchildren, sends output to the parent's terminal and discards the exit code (#2587 reproduction) → R-process-03.
- Duplication/misplacement: `process.args`, `process.env` (a pure fn reading the environment), `process.sleep`, `process.stdin_lines` → R-process-01.
- Declaration gaps: `process.pid` and `process.is_alive` are pure `fn`s reading host state. Measured callable from pure fns (probe-io) → R-process-04.
- Deprecated without a deadline: `exec_attached` (#3851 A) → R-process-02.

<a id="案"></a>

## Proposals

<a id="r-process-01-argv環境変数待ちstdin-を持ち主に返す---採用"></a>

### R-process-01 Return argv, environment variables, sleeping and stdin to their owners — ○ Accepted

- Content: `process.args()` → `env.args()` (does not return argv[0]; L2-env completeness rules), `process.env(k)` → `env.get(k)`, `process.sleep(ms)` → `env.sleep_ms(ms)`, `process.stdin_lines()` → `string.lines(io.read_all()!)`.
- Supporting L0 rules: R-L0-01, R-L0-02, R-L0-13, R-L1-25.
- (a) Rust, Go, Python and Elixir do not place argv/environment variables in child-process modules (facts §3.10).
- (b) All destinations already work on wasm through `wasi:cli` and `wasi:clocks`. `process.env` and `process.stdin_lines` also work on wasm (ledger note dated 2026-09-21).
- (c) `process.args` and `env.args` differ only in whether argv[0] is included; mixing them up shifts results by one (#3851 “Dropped”).
- (d) Remove. `@deprecated(since=15, use="env.args")`, `use="env.get"`, `use="env.sleep_ms"`, `use="string.lines(io.read_all()!)"`, all with `removal_epoch = 16`. `almide fix` rewrites `process.env`, `process.sleep` and `process.stdin_lines`. Do not mechanically rewrite `process.args()`, since argv[0] inclusion shifts the result by one; emit only a hint: “`env.args()` does not include argv[0] (program name). Decrease indices by one.” Usage counts: `args` 5/10/30/4, `env` 13/0/20/0, `sleep` 4/0/12/0, `stdin_lines` 0/3/11/3.
- Proposed gate: synonym gate rule 5 (#3851 F).
- Issue: #3851 B (existing)

<a id="r-process-02-processcommand-と-output-で一度きりの実行をまとめる---採用"></a>

### R-process-02 Consolidate one-shot execution with `ProcessCommand` and `output` — ○ Accepted

- Content:
  ```almide
  type ProcessCommand = { cmd: String, args: List[String], dir: String = "", stdin: String = "", timeout_ms: Int = 0 }
  effect fn output(c: ProcessCommand) -> ProcessStatus
  ```
  `dir = ""` means the current working directory; `timeout_ms = 0` means no timeout. On timeout, stop the child with its whole process group and return err (reuse C-214 wording unchanged).
  - Remove: `exec_status` (→ `output(process.ProcessCommand { cmd, args })`), `exec_status_timeout` (→ `timeout_ms`), `exec_in`/`exec_with_stdin` (→ `output` + `code` check), `exec_attached` (deprecated since epoch 10; → `run`).
  - Retain: `exec` (shortcut), `run`, `run_in`, `exit`, `kill`.
  - Prefix the type name with `Process`, following `ProcessStatus`/`HttpRequest` (to avoid collisions with user types in generated Rust; rationale at the start of `stdlib/http.almd`).
- Syntax: the named record expression `process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo" }` permits omitting defaulted fields (measured, probe-process). Unnamed `{ cmd: …, args: … }` cannot omit them and yields E005. Standardize on the named form and use it in CHEATSHEET and `docs/stdlib/process.md` examples (L0 referral 2, resolved).
- Supporting L0 rule: R-L0-01 (one spelling for the same execution), CLAUDE.md “families as matrices”. Filling the matrix by function names yields 2^3 names; use record fields instead. This answers #2587's “decision 2”.
- (a) Collecting settings into one value is the majority approach (Rust `Command` builder, Go `exec.Cmd` fields, Python keyword arguments, Node options, Swift `Configuration`).
- (b) wasm: private host capability `almide:process/spawn` (ADR-0025). Stock WASI has no child processes (ledger `host-capability` row). `output` is one new `enum op` case adding dir/stdin to the existing `exec-status-timeout` op, following ADR-0025 §3 item 4 (WIT test, host arm and cross fixture in one change). Inspection uses ADR-0027 D4's fixed command set.
- (c) This eliminates inferable-but-absent names such as `exec_status_in`.
- (d) Add and remove. Mark `exec_status`, `exec_status_timeout`, `exec_in`, `exec_with_stdin` with `@deprecated(since=15, use="process.output")`, and `exec_attached` with `@deprecated(since=15, use="process.run")`, all with `removal_epoch = 16`. `almide fix` rewrites: `exec_status(c, a)` → `output(process.ProcessCommand { cmd: c, args: a })`, `exec_status_timeout(c, a, t)` → `output(process.ProcessCommand { cmd: c, args: a, timeout_ms: t })`, `exec_attached(c, a)` → `run(c, a)` (all one-to-one). `exec_in`/`exec_with_stdin` change return shape (`String` → `ProcessStatus`), so provide hints only (“if `code` from `output(process.ProcessCommand { cmd, args, dir })` is 0, use `stdout`”). Usage counts: `exec_status` 10/5/10/0, `exec_status_timeout` 9/8/14/0, `exec_in` 1/2/6/0, `exec_with_stdin` 1/5/6/0, `exec_attached` 1/1/4/0.
- Proposed gate: a family gate fixes `process` public functions to {shortcut `exec`, `output`, six handle functions, `run`/`run_in`, `exit`, `kill`, `pid`}. Adding a `ProcessCommand` field requires both `output` and `start` fixtures in the same PR.
- Issues: #2587 (existing; answer to decision 2), new (removals)

<a id="r-process-03-ハンドル2587を足しspawnis_alive-を置き換える---採用"></a>

### R-process-03 Add handles (#2587), replacing `spawn`/`is_alive` — ○ Accepted

- Content:
  ```almide
  effect fn start(c: ProcessCommand) -> ProcessChild          // 自分のプロセスグループで起動。stdin は stdin 欄、出力はランタイムが吸い上げる
  effect fn poll(c: ProcessChild) -> ProcessStatus?            // 待たない。終わっていれば片付けて結果
  effect fn wait(c: ProcessChild) -> ProcessStatus             // 終わるまで待つ
  effect fn stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus  // グループに SIGTERM → grace_ms 待つ → SIGKILL → 片付け。何度呼んでもよい
  effect fn read_new_stdout(c: ProcessChild) -> String         // 前回から届いた分。待たない
  effect fn read_new_stderr(c: ProcessChild) -> String
  ```
  Remove: `spawn` (→ `start`), `is_alive` (→ `poll(c)! == none`). Retain `kill(pid, signal)` for signaling pids that are not one's own children.
- Preserved code comments: start in its own process group, take stdin from the stdin field and drain output in the runtime; poll does not wait and cleans up/returns a result if finished; wait waits for completion; stop sends SIGTERM to the group, waits grace_ms, sends SIGKILL, then cleans up and may be called repeatedly; stdout reads only new output since the previous call without waiting.
- Spelling choice (R-L0-01, across modules): reuse existing names for the same roles. Nonblocking check → `http.poll`; wait for completion → `http.wait`; read changes since last time → `http.read_new`. Do not use #2587 draft names `try_wait`/`read_stdout`.
- Answers to #2587 decisions: 1 `grace_ms` is mandatory (the language has no default arguments). 2 `ProcessCommand` (R-process-02). 3 One `ProcessStatus` result type; no separate `Exit` (do not make two types for the same result; R-L0-01). 4 Dropping a handle does not stop the child; the runtime stops the child tree at program exit.
- Time arguments are milliseconds as `Int`. This aligns with this module, `env.sleep_ms`, `HttpLimits`, `net.tcp_read_timeout`, and L2-datetime's referral decision: “time quantities use `Int` with units in names; `Duration` is only for `fan` budgets/deadlines” (L0 referral 1, resolved).
- (a) Rust, Go, Python, Zig and .NET separate launch settings from running-child handles and provide nonblocking checks (#2587 table). Go, execa, trio and swift-subprocess stop via “gentle signal → grace period → force”.
- (b) wasm: add `enum op` cases (start/poll/wait/stop/read) to the same `almide:process/spawn` as R-process-02 (ADR-0025 §3 item 4). Stock WASI rejects it at load time (host-capability). ADR-0027 D3 verdict: `poll` is none while running, some after completion.
- (c) `spawn`'s three defects (remaining grandchildren, mixed output, lost exit code) are hard to uncover even when an LLM writes tests.
- (d) Add and remove. `spawn`: `@deprecated(since=15, use="process.start")`; `is_alive`: `@deprecated(since=15, use="process.poll")`; both `removal_epoch = 16`. Arguments change from pid to handle, so `almide fix` gives hints only, not rewrites (“start with `let c = process.start(process.ProcessCommand { cmd, args })!`, then check running state with `process.poll(c)! == none`”). Usage counts: `spawn` 10/4/8/0, `is_alive` 12/0/8/0.
- Proposed gate: directly fixture #2587 completion criteria (no grandchildren after `stop`, `poll` none → some, output not sent to parent's stdout, no child tree left after `exit`). Fix the handle verb set in the same style as `http_call_family_gate_test.rs`.
- Issue: #2587 (existing)

<a id="r-process-04-pid-を-effect-fn-にする---採用"></a>

### R-process-04 Make `pid` an effect fn — ○ Accepted

- Content: change `fn pid() -> Int` to `effect fn pid() -> Int` (`is_alive` is removed by R-process-03).
- Supporting L0 rules: R-L0-13 (environment-reading functions are effect), ADR-0022 D3 (pure fns do not read outside).
- (b) Checker-only change. (c) No need to memorize by module whether a function returning the same value is pure or effect.
- (d) Semantic change (pure → effect), no rename. Following epochs 8/13, include it in epoch 15 `breaks`. Calls from pure fns produce E006, with a hint to make the caller an `effect fn`. Usage count 2/12/11/0.
- Proposed gate: add “intrinsics reading host state are effect” to R-L0-G and remove `process.pid` from the allowlist.
- Issue: #3907

<a id="r-process-05-processpollwait-の形を-httppollwait-に合わせる---採用"></a>

### R-process-05 Align `process.poll`/`wait` shapes with `http.poll`/`wait` — ○ Accepted

- Current: compared with `http.poll(c) -> Result[HttpResponse, String]?` (exchange failure is err inside some), newly added `process.poll(c) -> ProcessStatus?` has the reverse nesting order and no place for err. It cannot express timeout of a child launched with `start` and `timeout_ms`.
- Proposal: `effect fn process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?`. Like `http.wait`, `wait`/`stop` return child failures (timeout, cleanup failure) as effect err. The single handle-family contract is “`poll` never waits; none while running, then some(ok(result)) or some(err(reason))”.
- Supporting L0 rules: R-L0-01 (one shape per role; R-process-03 borrowed `http.poll` spelling but reversed nesting), R-L0-17.
- (a) Rust `Child::try_wait` is `io::Result<Option<ExitStatus>>`; Python `Popen.poll()` is `None` while running. Both distinguish nonblocking checks from results.
- (c) An LLM knowing `match http.poll(c) { some(ok(r)) => …, some(err(e)) => …, none => … }` writes the same form for `process.poll`.
- (d) Only change the new function's declaration.
- Completeness rule: in handle-owning modules (http/process), `poll` is `Result[T, String]?`, `wait` is `T` (failure via effect err).
- Proposed gate: expand `http_call_family_gate_test.rs` into a “handle family gate” checking the same declaration shape for process.
- Issue: #3974

<a id="r-process-06-動いている子の-pid-を読む-processchild_pid-を足す---採用"></a>

### R-process-06 Add `process.child_pid` to read a running child's pid — ○ Accepted

- Proposal: `effect fn process.child_pid(c: ProcessChild) -> Int`.
- (a) Six languages provide this: Go `cmd.Process.Pid`, Python `Popen.pid`, Rust `Child::id`, Node `child.pid`, Kotlin `Process.pid()`, Swift `processIdentifier`.
- (b) Native uses the OS pid; wasm uses the pid returned by `almide:process/spawn` (R-L0-15's fourth path).
- (c) Removing `spawn` removes the way to obtain the pid for sending `kill` to a child. Sending SIGINT to a child and logging pids are common.
- (d) Add 1.
- Issue: #3974

<a id="r-process-07-run-も-processcommand-を受け取りrun_in-を撤去する---採用"></a>

### R-process-07 Make `run` also accept `ProcessCommand`; remove `run_in` — ○ Accepted

- Current: `process.run(cmd, args) -> Int` and `process.run_in(dir, cmd, args) -> Int`. R-process-02's completeness rule says “collect settings in one record, not function-name combinations”, but `run_in` still encodes the working-directory axis in its name. Standard functions have no default arguments (R-string-08), so passing settings as a record follows the rule.
- Proposal: `effect fn process.run(c: ProcessCommand) -> Int` (pass the terminal through when the stdin field is empty); remove `run_in`. Keep only one argument-only shortcut, `exec(cmd, args)`.
- Supporting L0 rules: R-process-02 completeness rule, CLAUDE.md “families as matrices”.
- (a) Rust `Command::status`, Go `Cmd.Run`, Python `run()` take the same settings value as other execution forms.
- (c) Prevent an LLM wanting a timeout on `run` from guessing `run_timeout`.
- (d) Semantic change (argument type changes make old calls type errors, satisfying R-L0-14.4) and 1 removal. `almide fix` rewrites `run(c, a)` to `run(ProcessCommand { cmd: c, args: a, ... })`, and `run_in(d, c, a)` to the form with `dir: d` added. Usage counts: `run` 1/6/7/0, `run_in` 2/5/8/0.
- Issue: #3974

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `process.exec` | `effect fn exec(cmd: String, args: List[String]) -> String` | Go `Output` / Python `check_output` / Node `execFileSync` | Retain (shortcut) | R-process-02 |
| `process.exec_in` | `effect fn exec_in(dir: String, cmd: String, args: List[String]) -> String` | Rust `current_dir` / Python `cwd=` | Remove (→ `output`) | R-process-02 |
| `process.exec_with_stdin` | `effect fn exec_with_stdin(cmd: String, args: List[String], input: String) -> String` | Python `input=` / Node `{input}` | Remove (→ `output`) | R-process-02 |
| `process.exec_status` | `effect fn exec_status(cmd: String, args: List[String]) -> ProcessStatus` | Rust `output` / Python `run` | Remove (→ `output`) | R-process-02 |
| `process.exec_status_timeout` | `effect fn exec_status_timeout(cmd: String, args: List[String], timeout_ms: Int) -> ProcessStatus` | Python `timeout=` / Go `CommandContext` | Remove (→ `timeout_ms` in `output`) | R-process-02 |
| `process.output` | `effect fn output(c: ProcessCommand) -> ProcessStatus`(proposed) | Rust `Command::output` / Go `Cmd.Output` | Add | R-process-02 |
| `process.run` | `effect fn run(cmd: String, args: List[String]) -> Int` | Rust `status` / Go `Run` / Python `run()` | Semantic change (`ProcessCommand` accepted) | R-process-07 |
| `process.run_in` | `effect fn run_in(dir: String, cmd: String, args: List[String]) -> Int` | Same as above + cwd | Remove → `run(ProcessCommand { dir: … })` | R-process-07 |
| `process.exec_attached` | `effect fn exec_attached(cmd: String, args: List[String]) -> Int`(deprecated) | — | Remove (→ `run`) | R-process-02 |
| `process.spawn` | `effect fn spawn(cmd: String, args: List[String]) -> Int` | Rust `spawn` / Node `spawn` / Go `Start` | Remove (→ `start`) | R-process-03 |
| `process.start` | `effect fn start(c: ProcessCommand) -> ProcessChild`(proposed) | Go `Start` / Rust `spawn` / Kotlin `start` | Add | R-process-03 |
| `process.poll` | `effect fn poll(c: ProcessChild) -> Result[ProcessStatus, String]?`(proposed, R-process-05) | Python `Popen.poll` / Rust `try_wait` | Add | R-process-03 |
| `process.wait` | `effect fn wait(c: ProcessChild) -> ProcessStatus`(proposed) | Rust・Go・Python `wait` | Add | R-process-03 |
| `process.stop` | `effect fn stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus`(proposed) | Go `Cancel`+`WaitDelay` / execa `forceKillAfterDelay` | Add | R-process-03 |
| `process.read_new_stdout` | `effect fn read_new_stdout(c: ProcessChild) -> String`(proposed) | — (`http.read_new` aligned) | Add | R-process-03 |
| `process.read_new_stderr` | `effect fn read_new_stderr(c: ProcessChild) -> String`(proposed) | Same as above | Add | R-process-03 |
| `process.is_alive` | `fn is_alive(pid: Int) -> Bool` | Kotlin `isAlive` / Rust `try_wait` | Remove (→ `poll`) | R-process-03 |
| `process.kill` | `effect fn kill(pid: Int, signal: Int) -> Unit` | Rust `kill` / Python `os.kill` / Node `process.kill` | Retain | R-process-03 |
| `process.exit` | `effect fn exit(code: Int) -> Never` | Rust・Go・Python・OCaml・Swift `exit` | Retain | — |
| `process.pid` | `fn pid() -> Int` | Rust `id` / Go `Getpid` / Node `pid` | Semantic change (make effect fn) | R-process-04 |
| `process.args` | `effect fn args() -> List[String]` | Rust `env::args` | Remove (→ `env.args`) | R-process-01 |
| `process.env` | `fn env(key: String) -> String?` | Rust `env::var` | Remove (→ `env.get`) | R-process-01 |
| `process.sleep` | `fn sleep(ms: Int) -> Unit` | Rust `thread::sleep` | Remove (→ `env.sleep_ms`) | R-process-01 |
| `process.stdin_lines` | `effect fn stdin_lines() -> List[String]` | Python `readlines` | Remove (→ `string.lines(io.read_all()!)`) | R-process-01 |
| `process.child_pid` | `effect fn child_pid(c: ProcessChild) -> Int`(proposed) | Go `Process.Pid` / Python `Popen.pid` / Rust `Child::id` | Add | R-process-06 |

<a id="l0-への差し戻し処理済み"></a>

## Referrals back to L0 (resolved)

1. **Argument type for real-time durations**: decided in the L2-datetime referral (not `Duration`). Time quantities consistently use `Int` with units in the name (`sleep_ms`, `timeout_ms`, `grace_ms`). `Duration` remains the type for `fan` budgets/deadlines per ADR-0001. This page's `timeout_ms: Int` and `grace_ms: Int` remain unchanged.
2. **Unnamed record expressions cannot omit defaulted fields** (measured E005 “expects Cmd but got { cmd, args }”): retain named record expressions `process.ProcessCommand { … }` for `ProcessCommand`. They permit omitted defaulted fields (measured). Filling defaults from the expected type is a language type-checking question, not standard-function design; handle it outside this redesign. The standard-library-side action is to include an E005 hint “write the named form `process.ProcessCommand { … }`” in the R-process-02 gate (fixture).
