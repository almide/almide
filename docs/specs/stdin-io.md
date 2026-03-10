# Standard I/O Specification

> Verified by: `exercises/stdlib-test/bmi_test.almd` (io.print + io.read_line usage pattern).

---

## 1. Overview

The `io` stdlib module provides functions for interactive standard input/output. All functions are effect functions — they require an `effect fn` context.

```almide
import io

effect fn main(_args: List[String]) -> Result[Unit, String] = {
  io.print("Enter your name: ")
  let name = io.read_line()
  println("Hello, ${name}!")
}
```

---

## 2. io Module Functions

### 2.1 io.print

```almide
io.print(s: String) -> Unit
```

Prints a string to stdout **without** a trailing newline. This complements the built-in `println` which always appends a newline.

Use case: prompts where user input should appear on the same line.

```almide
io.print("Enter weight (kg): ")   -- cursor stays on same line
let input = io.read_line()
```

### 2.2 io.read_line

```almide
io.read_line() -> String
```

Reads one line from stdin (blocking). Returns the line content without the trailing newline.

```almide
io.print("Name: ")
let name = io.read_line()    -- blocks until user presses Enter
```

### 2.3 io.read_all

```almide
io.read_all() -> String
```

Reads all remaining content from stdin until EOF. Returns the entire content as a single string.

```almide
let all_input = io.read_all()   -- reads until EOF (e.g., piped input)
```

---

## 3. process.stdin_lines

```almide
process.stdin_lines() -> Result[List[String], String]
```

Reads all lines from stdin and returns them as a list. This is in the `process` module (not `io`) because it is a batch operation that reads until EOF, similar to other `process` functions.

```almide
import process

effect fn main(_args: List[String]) -> Result[Unit, String] = {
  let lines = process.stdin_lines()
  println("Got ${int.to_string(list.length(lines))} lines")
}
```

---

## 4. Effect Function Requirement

All I/O functions are effect functions (`is_effect: true`). Calling them from a pure function produces a compile error:

```
error: effect function 'io.read_line' cannot be called from pure function 'my_func'
  hint: add 'effect' keyword to the enclosing function
```

---

## 5. Code Generation

### 5.1 Rust Target

| Function | Generated Code |
|----------|---------------|
| `io.print(s)` | `print!("{}", s); std::io::Write::flush(&mut std::io::stdout()).ok();` |
| `io.read_line()` | `std::io::BufRead` based line reading |
| `io.read_all()` | `std::io::Read::read_to_string` |
| `process.stdin_lines()` | `std::io::BufRead::lines()` collected into `Vec<String>` |

### 5.2 TypeScript Target

| Function | Deno | Node |
|----------|------|------|
| `io.print(s)` | `Deno.stdout.writeSync(...)` | `process.stdout.write(...)` |
| `io.read_line()` | `prompt()` | `fs.readSync(0, ...)` |
| `io.read_all()` | `Deno.readAllSync(Deno.stdin)` | `fs.readFileSync(0, 'utf8')` |

---

## 6. Relationship to println

`println` is a built-in (not in the `io` module) that prints a string followed by a newline. It is available without importing `io`:

```almide
println("hello")           -- built-in, always adds newline
io.print("hello")          -- io module, no newline
```
