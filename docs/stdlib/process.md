# process

Process execution. import process, effect.

### `process.exec(cmd: String, args: List[String]) -> Result[String, String]`

Execute a command and return its stdout as a string

```almd check
import process

effect fn main() -> Unit = {
  let output = process.exec("ls", ["-la"])!
  println(output)
}
```

### `process.exit(code: Int) -> Unit`

Exit the process with the given status code. The code is **0 to 125** on every
target; any other code is a domain error — one stderr line
`Error: exit code must be in 0..=125` and exit 1, identically everywhere
(C-350). 126, 127 and 128+n are values a shell generates itself, POSIX carries
only the low 8 bits of a status to its parent, and the shipped wasm artifact's
`proc_exit` is specified over the same range — so 0..=125 is what a build can
actually deliver.

```almd check
import process

effect fn main() -> Unit = {
  println("fatal: giving up")
  process.exit(1)
}
```

### `process.stdin_lines() -> Result[List[String], String]`

Read all lines from standard input

```almd check
import process

effect fn main() -> Unit = {
  let lines = process.stdin_lines()!
  println("${list.len(lines)} line(s)")
}
```

### `process.exec_in(dir: String, cmd: String, args: List[String]) -> Result[String, String]`

Execute a command in a specific working directory

```almd check
import process

effect fn main() -> Unit = {
  let output = process.exec_in("/tmp", "pwd", [])!
  println(output)
}
```

### `process.exec_with_stdin(cmd: String, args: List[String], input: String) -> Result[String, String]`

Execute a command with input piped to its stdin

```almd check
import process

effect fn main() -> Unit = {
  let output = process.exec_with_stdin("cat", [], "hello")!
  println(output)
}
```

### `process.exec_status(cmd: String, args: List[String]) -> Result[{code: Int, stdout: String, stderr: String}, String]`

Execute a command and return exit code, stdout, and stderr

```almd check
import process

effect fn main() -> Unit = {
  let r = process.exec_status("ls", [])! // {code, stdout, stderr}
  println("exit ${r.code}")
  println(r.stdout)
}
```

### `process.env(key: String) -> Option[String]`

Get the value of an environment variable, or None if not set

```almd check
import process

effect fn main() -> Unit = {
  let home = process.env("HOME") ?? "/tmp"
  println(home)
}
```

### `process.spawn(cmd: String, args: List[String]) -> Result[Int, String]`

Spawn a child process without waiting, return its PID

```almd check
import process

effect fn main() -> Unit = {
  let pid = process.spawn("node", ["server.js"])!
  println("started pid ${pid}")
}
```

### `process.kill(pid: Int, signal: Int) -> Result[Unit, String]`

Send a signal to a process by PID (e.g. 15 for SIGTERM, 9 for SIGKILL)

```almd check
import process

effect fn main() -> Unit = {
  let pid = process.spawn("sleep", ["30"])!
  process.kill(pid, 15)!
}
```

### `process.is_alive(pid: Int) -> Bool`

Check if a process with the given PID is still running

```almd check
import process

effect fn main() -> Unit = {
  let pid = process.spawn("sleep", ["30"])!
  let running = process.is_alive(pid)
  println(if running then "running" else "exited")
  process.kill(pid, 15)!
}
```

### `process.args() -> List[String]`

Get command-line arguments as a list of strings.

```almd check
import process

effect fn main() -> Unit = {
  let args = process.args()
  println("${args}")
}
```

### `process.pid() -> Int`

Get the current process ID.

```almd check
import process

effect fn main() -> Unit = {
  let my_pid = process.pid()
  println(int.to_string(my_pid))
}
```


### `process.exec_status_timeout(cmd: String, args: List[String], timeout_ms: Int) -> Result[ProcessStatus, String]`

`exec_status` with a deadline — the one admissible timeout (C-214): spawning a
process is already outside the byte-identity contract, so bounding it adds no
nondeterminism. If the deadline fires the child is killed and the err is
exactly `exec timed out after <ms>ms`; whether it fires is a function of the
host. A fired deadline is commonly mapped to exit code 124 by callers that
compare exit codes.

```almd check
import process

effect fn main() -> Unit = {
  match process.exec_status_timeout("cargo", ["build"], 60000) {
    ok(st) => println(int.to_string(st.code)),
    err(e) => println(e),   // "exec timed out after 60000ms" on a hang
  }
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (14 functions)

```
// Stdout on exit 0; else err with stderr text.
effect process.exec(cmd: String, args: List[String]) -> String

// Ends with code 0..=125; others abort, exit 1.
effect process.exit(code: Int) -> Never

// Full argv, program name at index 0.
process.args() -> List[String]

// All stdin lines; err on non-UTF-8 input.
effect process.stdin_lines() -> List[String]

// exec in dir; err is stderr, maybe empty.
effect process.exec_in(dir: String, cmd: String, args: List[String]) -> String

// exec with input piped in; err may be empty.
effect process.exec_with_stdin(cmd: String, args: List[String], input: String) -> String

// Code and output; code -1 if killed by signal.
effect process.exec_status(cmd: String, args: List[String]) -> ProcessStatus

// exec_status, but kills cmd and errs at timeout_ms.
effect process.exec_status_timeout(cmd: String, args: List[String], timeout_ms: Int) -> ProcessStatus

// OS process ID of this program.
process.pid() -> Int

// Variable value, or none if unset.
process.env(key: String) -> Option[String]

// Pid of cmd started in the background.
effect process.spawn(cmd: String, args: List[String]) -> Int

// Sends signal to pid; err is kill's stderr.
effect process.kill(pid: Int, signal: Int) -> Unit

// Blocks ms milliseconds; negative is 0.
process.sleep(ms: Int) -> Unit

// True while pid exists, zombies included.
process.is_alive(pid: Int) -> Bool
```

## Type index (1 types)

```
type process.ProcessStatus = { code: Int, stdout: String, stderr: String }
```

<!-- END GENERATED SIGNATURE INDEX -->
