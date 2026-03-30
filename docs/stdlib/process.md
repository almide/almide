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
