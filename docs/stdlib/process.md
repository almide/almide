# process

Process execution. import process, effect.

### `process.exec(cmd: String, args: List[String]) -> Result[String, String]`

Execute a command and return its stdout as a string

```almd
let output = process.exec("ls", ["-la"])
```

### `process.exit(code: Int) -> Unit`

Exit the process with the given status code

```almd
process.exit(1)
```

### `process.stdin_lines() -> Result[List[String], String]`

Read all lines from standard input

```almd
let lines = process.stdin_lines()
```

### `process.exec_in(dir: String, cmd: String, args: List[String]) -> Result[String, String]`

Execute a command in a specific working directory

```almd
let output = process.exec_in("/tmp", "pwd", [])
```

### `process.exec_with_stdin(cmd: String, args: List[String], input: String) -> Result[String, String]`

Execute a command with input piped to its stdin

```almd
let output = process.exec_with_stdin("cat", [], "hello")
```

### `process.exec_status(cmd: String, args: List[String]) -> Result[{code: Int, stdout: String, stderr: String}, String]`

Execute a command and return exit code, stdout, and stderr

```almd
let r = process.exec_status("ls", []) // {code, stdout, stderr}
```

### `process.env(key: String) -> Option[String]`

Get the value of an environment variable, or None if not set

```almd
let home = process.env("HOME") ?? "/tmp"
```

### `process.spawn(cmd: String, args: List[String]) -> Result[Int, String]`

Spawn a child process without waiting, return its PID

```almd
let pid = process.spawn("node", ["server.js"])
```

### `process.kill(pid: Int, signal: Int) -> Result[Unit, String]`

Send a signal to a process by PID (e.g. 15 for SIGTERM, 9 for SIGKILL)

```almd
process.kill(pid, 15)
```

### `process.is_alive(pid: Int) -> Bool`

Check if a process with the given PID is still running

```almd
let running = process.is_alive(pid)
```

### `process.args() -> List[String]`

Get command-line arguments as a list of strings.

```almd
let args = process.args()
```

### `process.pid() -> Int`

Get the current process ID.

```almd
let my_pid = process.pid()
```
