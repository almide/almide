# process

Child processes, and this program's own exit and pid. `import process`,
`effect fn`.

A child's settings are one record, `ProcessCommand`:

```almd
type ProcessCommand = {
  cmd: String,          // the program
  args: List[String],   // its argument list
  dir: String = "",     // working directory; "" keeps this program's
  stdin: String = "",   // text fed to its stdin; "" gives it none
  timeout_ms: Int = 0,  // deadline; 0 means none
}
```

Write the record with its type name, so the defaulted fields can be left out:
`process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo" }`.
Without the type name (`{ cmd: ..., args: ... }`) every field must be given.

A program and its arguments are always an argv list: nothing here runs a shell
string through `sh -c`, so no argument is ever re-split or expanded. A
`ProcessCommand` runs three ways:

| Call | Waits? | Output | Answer |
|---|---|---|---|
| `output(c)` | until it ends | captured | `ProcessStatus { code, stdout, stderr }` |
| `start(c)` | no | captured, read as it arrives | a `ProcessChild` to `poll` / `wait` / `stop` |
| `run(c)` | until it ends | on this terminal | the exit code |

and `exec(cmd, args)` is the one shortcut: stdout on exit 0, else an `err`
with stderr's text.

Every call here that can fail returns a `Result`; write `!` or handle the
`err` (ADR-0008). `poll`, `child_pid`, `read_new_stdout` and `read_new_stderr`
return a plain value, but as `effect fn`s their calls yield a `Result` too, so
they also take `!`: `process.child_pid(c)!`.

**Targets** ([ADR-0025](../adr/0025-wasm-process-is-a-private-host-capability.md)).
Native serves every function. `almide run --target wasm` and the wasm leg of
`almide test` serve `exec`, `output`, `run`, `kill`, `pid` and `exit` with
native's results — the same captured text, exit codes and err strings. The
`start` family (`start`, `poll`, `wait`, `stop`, `read_new_stdout`,
`read_new_stderr`, `child_pid`) is native only for now. `almide build
--target wasm` builds a program that starts a child, but its artifact imports
the private `almide:process/spawn` interface, so a stock runtime such as
`wasmtime run` refuses it at load; it runs on a host that implements that
import. A `--component` build refuses it with E081.

**`[permissions] proc`**: a project can list the commands this module may
start. Every call that starts a child must then name its command as a string
literal on the list — `exec`'s first argument, or the `cmd` field of the
`ProcessCommand` written in the call to `output`, `run` or `start` — or the
program does not compile:

```toml
[permissions]
proc = ["git", "cargo"]
```

## The shortcut

### `process.exec(cmd: String, args: List[String]) -> Result[String, String]`

Run `cmd` with `args` and return its stdout if it exits 0; otherwise an `err`
with its stderr text (which may be empty).

```almd check
import process

effect fn main() -> Unit = {
  let listing = process.exec("ls", ["-la"])!
  println(listing)
}
```

## Run once, capture everything

### `process.output(c: ProcessCommand) -> Result[ProcessStatus, String]`

Run `c` to its end and return its exit code with everything it wrote:
`ProcessStatus { code: Int, stdout: String, stderr: String }`, where `code` is
`-1` if a signal ended the child. Every exit code is `ok` — check `code`
yourself. The `err` cases are a child that cannot start (a missing program, a
`dir` that does not exist) and a passed deadline.

```almd check
import process

effect fn main() -> Unit = {
  let st = process.output(process.ProcessCommand { cmd: "git", args: ["status", "--short"], dir: "." })!
  if st.code == 0 then println(st.stdout) else println("git failed: ${st.stderr}")
}
```

`stdin` feeds the child text; without it the child's stdin is empty (not the
terminal):

```almd check
import process

effect fn main() -> Unit = {
  let st = process.output(process.ProcessCommand { cmd: "sort", args: [], stdin: "b\na\n" })!
  println(st.stdout)
}
```

**Deadlines.** `timeout_ms` is the one admissible timeout (C-214): starting a
process is already outside the byte-identity contract, so bounding it adds no
nondeterminism. If the deadline passes, the child is killed and the err is
exactly `exec timed out after <ms>ms`; whether it passes is a function of the
host. Callers that compare exit codes commonly map it to 124.

A captured child runs in its own process group, so a deadline kills its whole
tree — which makes it a background job for the controlling terminal. A child
that reads the terminal or changes its settings (`stty -echo`) is stopped by
the kernel with SIGTTIN / SIGTTOU; that stop is answered at once with an err
(`...: the child was stopped by SIGTTOU: it tried to use the terminal from a
background process group; run terminal programs with process.run`) rather than
by waiting out the deadline.

```almd check
import process

effect fn main() -> Unit = {
  match process.output(process.ProcessCommand { cmd: "cargo", args: ["build"], timeout_ms: 60000 }) {
    ok(st) => println("exit ${st.code}"),
    err(e) => println(e),
  }
}
```

## Run on this terminal

### `process.run(c: ProcessCommand) -> Result[Int, String]`

Run `c` ON this program's terminal: the child inherits stdout and stderr (and
stdin too, unless `c.stdin` is set) and stays in this program's process group,
so pagers, editors, `stty` and anything else that reads or reconfigures the
terminal work, and a long build streams its output as it runs. This program's
stdout is flushed before the child starts. Nothing is captured; the answer is
the exit code (`-1` if the child was killed by a signal). Any exit code is
`ok`; the `err` cases are a child that cannot start (the err names both `dir`
and `cmd` when `dir` does not exist) and a passed `timeout_ms`.

`output` captures the output and returns it; `run` attaches to the terminal
and returns the exit code.

```almd check
import process

effect fn main() -> Unit = {
  let code = process.run(process.ProcessCommand { cmd: "cargo", args: ["build"], dir: "." })!
  println("cargo build exited ${code}")
}
```

## Run in the background

`start` returns at once with a `ProcessChild` handle. The child runs in its own
process group; its stdout and stderr are captured by the runtime (never mixed
into this program's output) and read with `read_new_stdout` /
`read_new_stderr`. Native only for now (see Targets).

### `process.start(c: ProcessCommand) -> Result[ProcessChild, String]`

Start `c` and return at once; an `err` if it cannot start.

### `process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?`

Never blocks: `none` while `c` runs, then `some(ok(status))` — or
`some(err(e))` if it passed its deadline. The same shape as `http.poll`.

### `process.wait(c: ProcessChild) -> Result[ProcessStatus, String]`

Block until `c` ends; its outcome.

### `process.stop(c: ProcessChild, grace_ms: Int) -> Result[ProcessStatus, String]`

Send SIGTERM to `c`'s process group, give it `grace_ms` to end, then SIGKILL,
and reap it. The signals go to the whole group, so the child's own children
are stopped with it. Calling it again after the end returns the same outcome.

### `process.read_new_stdout(c: ProcessChild) -> String` / `process.read_new_stderr(c: ProcessChild) -> String`

What `c` wrote to that stream since the last read; never blocks.

### `process.child_pid(c: ProcessChild) -> Int`

The OS process id of `c`, for `process.kill` or a log line.

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "ping", args: ["-c", "3", "127.0.0.1"] })!
  let pid = process.child_pid(c)!
  println("started pid ${pid}")
  match process.poll(c)! {
    none => println("still running"),
    some(ok(st)) => println("exited ${st.code}"),
    some(err(e)) => println(e),
  }
  let st = process.wait(c)!
  let errs = process.read_new_stderr(c)!
  println(st.stdout)
  println(errs)
}
```

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "sleep", args: ["30"] })!
  let st = process.stop(c, 500)!
  println("stopped, code ${st.code}")
}
```

## Signals, exit and pid

### `process.kill(pid: Int, signal: Int) -> Result[Unit, String]`

Send `signal` to the process `pid` (e.g. 15 for SIGTERM, 9 for SIGKILL). The
signal goes through the kill(2) system call, never a `kill` program found on
`PATH`. `pid` must be in 1..=2147483647 — `0` (the caller's process group) and
`-1` (every process the user may signal) are refused — and `signal` in 0..=31
(0 only asks whether the process exists). Every err is a fixed text:
`process.kill(<pid>, <signal>): pid must be in 1..=2147483647`,
`…: signal must be in 0..=31`, or the kill(2) failure as `no such process`,
`operation not permitted` or `invalid signal`. Under `[permissions] proc`,
`kill` must be on the list. For a child of this program, `process.stop` also
waits and reaps it; `kill` is for any pid.

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "sleep", args: ["30"] })!
  process.kill(process.child_pid(c)!, 15)!
  let st = process.wait(c)!
  println("code ${st.code}")
}
```

### `process.exit(code: Int) -> Never`

Exit the process with the given status code, the POSIX exit status **0 to
255**. The code passes through unchanged on native and on
`almide run --target wasm`, so a wrapper can exit with its child's status,
including 126, 127 and 128+n (#2780). Any other code is a domain error with one
stderr line, `Error: exit code must be in 0..=255`, and exit 1, identically on
every target (C-350). Rust, Go, Python and Node truncate such a code to its low
8 bits instead, so `exit(256)` would report success.

A WASI preview-1 build (`almide build --target wasm`, run on a stock runtime
such as `wasmtime`) delivers only 0..=125. A stock runtime traps on
`proc_exit(126)` and above, so that build refuses 126..=255 itself with
`Error: a WASI preview-1 build cannot exit with a code in 126..=255` and exit 1.
It never exits with a silent 1.

```almd check
import process

effect fn main() -> Unit = {
  println("fatal: giving up")
  process.exit(1)
}
```

### `process.pid() -> Int`

The OS process id of this program.

```almd check
import process

effect fn main() -> Unit = {
  println(int.to_string(process.pid()))
}
```

## Kept until they move

`args`, `stdin_lines`, `env` and `sleep` belong to `env` and `io` in the
redesign; they stay here until their behavior contracts move.

### `process.args() -> List[String]`

The full argv, the program path first.

### `process.stdin_lines() -> Result[List[String], String]`

All lines of standard input; an `err` on input that is not UTF-8.

### `process.env(key: String) -> String?`

The value of an environment variable, or `none` if it is not set.

### `process.sleep(ms: Int) -> Unit`

Block for `ms` milliseconds; a negative `ms` is 0.

```almd check
import process

effect fn main() -> Unit = {
  let args = process.args()
  let home = process.env("HOME") ?? "/tmp"
  let lines = process.stdin_lines()!
  process.sleep(10)
  println("${list.len(args)} arg(s), home ${home}, ${list.len(lines)} line(s)")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (13 functions)

```
// Stdout on exit 0; else err with stderr text.
// @since 0.5.0 or earlier
effect process.exec(cmd: String, args: List[String]) -> String

// Runs c to its end: any exit code is ok; err if it cannot start or passes its deadline.
// @since unreleased
effect process.output(c: ProcessCommand) -> ProcessStatus

// Runs c on this terminal (its stdin too, unless c.stdin is set); the exit code, -1 if signalled.
// @since unreleased
effect process.run(c: ProcessCommand) -> Int

// Starts c in its own process group and returns at once; its output is captured.
// @since unreleased
effect process.start(c: ProcessCommand) -> ProcessChild

// Never blocks: none while c runs, then its outcome (err past its deadline).
// @since unreleased
effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]

// Blocks until c ends; its outcome.
// @since unreleased
effect process.wait(c: ProcessChild) -> ProcessStatus

// SIGTERM to c's group, grace_ms to end, then SIGKILL; again after the end is the same outcome.
// @since unreleased
effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus

// What c wrote to stdout since the last read; never blocks.
// @since unreleased
effect process.read_new_stdout(c: ProcessChild) -> String

// What c wrote to stderr since the last read; never blocks.
// @since unreleased
effect process.read_new_stderr(c: ProcessChild) -> String

// The OS process id of c, for process.kill.
// @since unreleased
effect process.child_pid(c: ProcessChild) -> Int

// Sends signal (0..=31) to pid (> 0) by kill(2); err names why.
// @since 0.12.3 or earlier
effect process.kill(pid: Int, signal: Int) -> Unit

// Ends with code 0..=255; others abort, exit 1.
// @since 0.5.0 or earlier
effect process.exit(code: Int) -> Never

// OS process ID of this program.
// @since 0.12.3 or earlier
effect process.pid() -> Int
```

## Type index (2 types)

```
// Exit code plus captured stdout and stderr; code -1 if a signal ended the child.
// @since 0.15.0 or earlier
type process.ProcessStatus = { code: Int, stdout: String, stderr: String }

// A child's settings: dir "" keeps this one, stdin "" gives it none, timeout_ms 0 means no deadline.
// @since unreleased
type process.ProcessCommand = { cmd: String, args: List[String], dir: String, stdin: String, timeout_ms: Int }
```

<!-- END GENERATED SIGNATURE INDEX -->
