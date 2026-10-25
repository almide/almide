# fs

File system. import fs, effect.

### The `_if_exists` content readers — absence as a value

`read_text_if_exists` / `read_bytes_if_exists` / `read_lines_if_exists` /
`read_bytes_raw_if_exists` return `Result[Option[T], String]`:
`ok(none)` when the path (or a parent) does not exist, `ok(some(content))`
when readable, `err(msg)` for real failures (permission, a directory at the
path, IO). Race-free — the classification happens inside the one read
(unlike an `fs.exists` pre-check). Contract C-215; serves on native and both wasm legs
(the wasm render walls honestly).

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/config.toml"
  let cfg = fs.read_text_if_exists(path)! ?? "default"
  println(cfg)
  fs.write(path, "port = 8080")!
  println(fs.read_text_if_exists(path)! ?? "default")
  fs.remove_all(dir)!
}
```
```output
default
port = 8080
```

### The whole-file readers read regular files of at most 1 GiB

`read_text` / `read_bytes` / `read_lines` / `read_bytes_raw` and their
`_if_exists` twins read a **regular file** of at most **1 GiB**
(1073741824 bytes), the same on native, the embedded wasm host and the stock
WASI legs (contract C-431, #4201):

- a character or block device, a FIFO or a socket (`/dev/zero`, `/dev/null`,
  a named pipe) errs `fs.read_text("/dev/zero"): Not a regular file`. The
  open does not block on a FIFO with no writer, and the opened descriptor is
  what is judged, so a file swapped in after a check cannot slip past;
- a larger file errs `fs.read_bytes("big.bin"): File too large (the read
  limit is 1073741824 bytes)` — from its size before a byte is read, and from
  the bytes actually read when the size under-reports;
- a directory keeps its own `Is a directory (os error 21)`.

The `_if_exists` twins answer these as `err`, never `ok(none)`. To walk a
file larger than the limit, stream it with `fs.fold_lines` /
`fs.for_each_line`. Reading standard input is `io.read_all`, not
`fs.read_text("/dev/stdin")`.

### `fs.read_text(path: String) -> Result[String, String]`

Read file contents as a UTF-8 string

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/config.toml", "port = 8080")!
  let text = fs.read_text("${dir}/config.toml")!
  println(text)
  fs.remove_all(dir)!
}
```
```output
port = 8080
```

### `fs.read_bytes(path: String) -> Result[List[Int], String]`

Read file contents as a list of bytes

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/image.png", [137, 80, 78, 71])!
  let bytes = fs.read_bytes("${dir}/image.png")!
  println("${bytes}")
  fs.remove_all(dir)!
}
```
```output
[137, 80, 78, 71]
```

### `fs.write(path: String, content: String) -> Result[Unit, String]`

Write a string to a file, creating or overwriting it

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/output.txt", "hello")!
  println(fs.read_text("${dir}/output.txt")!)
  fs.remove_all(dir)!
}
```
```output
hello
```

### `fs.write_bytes(path: String, bytes: List[Int]) -> Result[Unit, String]`

Write a list of bytes to a file

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/out.bin", [0, 1, 2])!
  println("${fs.read_bytes("${dir}/out.bin")!}")
  fs.remove_all(dir)!
}
```
```output
[0, 1, 2]
```

### `fs.append(path: String, content: String) -> Result[Unit, String]`

Append a string to a file, creating it if it doesn't exist

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let log = "${dir}/log.txt"
  fs.append(log, "first line\n")!
  fs.append(log, "new line\n")!
  println("${fs.read_lines(log)!}")
  fs.remove_all(dir)!
}
```
```output
["first line", "new line"]
```

### `fs.mkdir_p(path: String) -> Result[Unit, String]`

Create a directory and all parent directories

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/data/cache/images")!
  println("${fs.is_dir("${dir}/data/cache/images")}")
  fs.remove_all(dir)!
}
```
```output
true
```

### `fs.exists(path: String) -> Bool`

Check if a file or directory exists

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/config.toml", "")!
  println(if fs.exists("${dir}/config.toml") then "found" else "missing")
  println(if fs.exists("${dir}/other.toml") then "found" else "missing")
  fs.remove_all(dir)!
}
```
```output
found
missing
```

### `fs.read_lines(path: String) -> Result[List[String], String]`

Read a file as a list of lines. Materializes the whole file — for large
inputs use `fs.fold_lines` / `fs.for_each_line` instead (O(longest line)
memory, not O(file)).

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/data.csv", "a,1\nb,2\n")!
  let lines = fs.read_lines("${dir}/data.csv")!
  println("${lines}")
  fs.remove_all(dir)!
}
```
```output
["a,1", "b,2"]
```

### `fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> Result[A, String]`

Fold over a file's lines without materializing them — line semantics
byte-match `read_lines` (contract C-220). The default shape for aggregating
a large file.

```almd run
import fs

fn parse_row(line: String) -> Int = int.parse(line) ?? 0

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/data.csv", "1\n2\n3\n")!
  let total = fs.fold_lines("${dir}/data.csv", 0, (acc, line) => acc + parse_row(line))!
  println(int.to_string(total))
  fs.remove_all(dir)!
}
```
```output
6
```

A **fallible callback** makes the whole walk fallible (ADR-0006, contract
C-274): a callback body that propagates with `!` selects the first-err
short-circuit form — the callback is never invoked for a line after the one
that failed, and the native reader stops there too. Same name, one extra `!`.

```almd run
import fs

fn add_row(acc: Int, line: String) -> Int! = {
  guard line != "oops" else err("bad row: ${line}")
  acc + (int.parse(line) ?? 0)
}

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write(path, "1\n2\n3\n")!
  let stats = fs.fold_lines(path, 0, (acc, line) => add_row(acc, line)!)!
  println(int.to_string(stats))
  // first err wins; later lines are never visited
  fs.write(path, "1\noops\n3\n")!
  let failed = fs.fold_lines(path, 0, (acc, line) => add_row(acc, line)!)
  match failed {
    ok(n) => println(int.to_string(n)),
    err(e) => println("error: ${e}"),
  }
  fs.remove_all(dir)!
}
```
```output
6
error: bad row: oops
```

### `fs.for_each_line(path: String, f: (String) -> Unit) -> Result[Unit, String]`

Visit each line of a file in order without materializing the list. The
callback may mutate captured `var`s.

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/data.csv", "a,1\nb,2\nc,3\n")!
  var count = 0
  fs.for_each_line("${dir}/data.csv", (line) => { count = count + 1 })!
  println(int.to_string(count))
  fs.remove_all(dir)!
}
```
```output
3
```

It takes the same fallible callback form as `fold_lines`:

```almd check
import fs

effect fn emit(line: String) -> Unit = {
  guard line != "" else err("empty line")
  println("got ${line}")
}

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write(path, "one\ntwo\n")!
  fs.for_each_line(path, (line) => emit(line)!)!
  fs.remove_all(dir)!
}
```

The two **partitioned** cells below (`fold_lines_range` / `fold_lines_chunked`)
deliberately have **no** fallible form: a partitioned walk has no defined
"first" err (which chunk fails first is a thread-schedule observable), so an
erring chunk body handles its own error.

### `fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> Result[A, String]`

Fold exactly the lines owned by the byte range `[start, end)` — a line
belongs to the range containing the byte before its first byte — so folding
a partition of `[0, file_size)` visits every line exactly once.

```almd check
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write(path, "a\nbb\nccc\n")!
  let part = fs.fold_lines_range(path, 0, 4096, 0, (acc, line) => acc + 1)!
  println(int.to_string(part))
  // a partition of [0, file_size) visits every line exactly once
  let size = fs.file_size(path)!
  let first = fs.fold_lines_range(path, 0, 4, 0, (acc, line) => acc + 1)!
  let second = fs.fold_lines_range(path, 4, size, 0, (acc, line) => acc + 1)!
  println("${first} + ${second}")
  fs.remove_all(dir)!
}
```

### `fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> Result[List[A], String]`

Chunk-parallel fold: one range worker per chunk on real threads inside the
runtime, partials returned in chunk order. Merge the partials yourself — the
result is deterministic whatever the thread schedule. The callback must be a
pure step function (capturing mutable state is a compile error).

```almd check
import fs

fn step(acc: Int, line: String) -> Int = acc + 1
fn combine(a: Int, b: Int) -> Int = a + b

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/data.csv", "a\nb\nc\nd\ne\n")!
  let partials = fs.fold_lines_chunked("${dir}/data.csv", 8, 0, step)!
  let stats = partials |> list.fold(0, (acc, m) => combine(acc, m))
  println(int.to_string(stats))
  fs.remove_all(dir)!
}
```

### `fs.remove(path: String) -> Result[Unit, String]`

Delete a file

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/temp.txt", "scratch")!
  fs.remove("${dir}/temp.txt")!
  println("${fs.exists("${dir}/temp.txt")}")
  fs.remove_all(dir)!
}
```
```output
false
```

### `fs.list_dir(path: String) -> Result[List[String], String]`

List entries in a directory

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write("${dir}/src/main.almd", "")!
  fs.write("${dir}/src/util.almd", "")!
  let entries = fs.list_dir("${dir}/src/")!
  println("${entries |> list.sort}")
  fs.remove_all(dir)!
}
```
```output
["main.almd", "util.almd"]
```

### `fs.is_dir(path: String) -> Bool`

Check if a path is a directory

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write("${dir}/readme.md", "")!
  println(if fs.is_dir("${dir}/src") then "directory" else "not a directory")
  println(if fs.is_dir("${dir}/readme.md") then "directory" else "not a directory")
  fs.remove_all(dir)!
}
```
```output
directory
not a directory
```

### `fs.is_file(path: String) -> Bool`

Check if a path is a regular file

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write("${dir}/readme.md", "")!
  println(if fs.is_file("${dir}/readme.md") then "file" else "not a file")
  println(if fs.is_file("${dir}/src") then "file" else "not a file")
  fs.remove_all(dir)!
}
```
```output
file
not a file
```

### `fs.copy(src: String, dst: String) -> Result[Unit, String]`

Copy a file from src to dst

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/a.txt", "hello")!
  fs.copy("${dir}/a.txt", "${dir}/b.txt")!
  println(fs.read_text("${dir}/b.txt")!)
  fs.remove_all(dir)!
}
```
```output
hello
```

### `fs.rename(src: String, dst: String) -> Result[Unit, String]`

Rename or move a file

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/old.txt", "hello")!
  fs.rename("${dir}/old.txt", "${dir}/new.txt")!
  println("${fs.exists("${dir}/old.txt")} ${fs.exists("${dir}/new.txt")}")
  fs.remove_all(dir)!
}
```
```output
false true
```

### `fs.walk(dir: String) -> Result[List[String], String]`

Recursively list every entry — files AND directories — in a directory tree

A symlink is listed as an entry and never followed (C-396): a link to a
directory outside the tree lists nothing outside it, and a link loop cannot
repeat the walk. `fs.glob` walks the same way. `dir` itself is opened as
written, its own symlinks followed.

Following links is a non-goal for now: there is no opt-in. Not following is
the safe default for a directory others can write to (an upload or spool
directory, an extracted archive, a cloned repository). Of the walkers measured
(2026-10-25), Go's `filepath.WalkDir` has no follow option either, while
Python's `os.walk(followlinks=True)` and Rust's walkdir `follow_links(true)`
do. To descend into a linked directory, name it to `fs.walk` yourself.

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src/util")!
  fs.write("${dir}/src/main.almd", "")!
  fs.write("${dir}/src/util/io.almd", "")!
  let all_files = fs.walk("${dir}/src")!
  println("${all_files |> list.map((f) => string.replace(f, dir, "")) |> list.sort}")
  fs.remove_all(dir)!
}
```
```output
["/src/main.almd", "/src/util", "/src/util/io.almd"]
```

### `fs.remove_all(path: String) -> Result[Unit, String]`

Recursively delete a directory and all its contents

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/build/obj")!
  fs.write("${dir}/build/obj/main.o", "")!
  fs.remove_all("${dir}/build/")!
  println("${fs.exists("${dir}/build")}")
  fs.remove_all(dir)!
}
```
```output
false
```

### `fs.file_size(path: String) -> Result[Int, String]`

Get file size in bytes

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/data.bin", [0, 1, 2, 3, 4])!
  let size = fs.file_size("${dir}/data.bin")!
  println(int.to_string(size))
  fs.remove_all(dir)!
}
```
```output
5
```

### `fs.temp_dir() -> String`

Get the system temporary directory path

```almd run
import fs

effect fn main() -> Unit = {
  let tmp = fs.temp_dir()
  println("${fs.is_dir(tmp)}")
}
```
```output
true
```

### `fs.stat(path: String) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]`

Get file metadata: size, type, and modification time

```almd check
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/file.txt", "hello")!
  let info = fs.stat("${dir}/file.txt")! // {size, is_dir, is_file, modified}
  println("${info.size} ${info.is_dir} ${info.is_file} ${info.modified > 0}")
  fs.remove_all(dir)!
}
```

### `fs.glob(pattern: String) -> Result[List[String], String]`

Find the paths matching a glob pattern. The pattern is matched segment by
segment against the paths below its literal prefix (the current directory
when the pattern is relative): `*` matches any run of characters within one
segment, `**` matches zero or more whole segments, everything else is
literal. Files and directories both qualify. The answer is sorted and spelled
with the pattern's own prefix — cwd-relative, no leading `./`, for a relative
pattern — and a prefix directory that does not exist answers `[]`.

```almd check
import fs

effect fn main() -> Unit = {
  // From the repo root: ["src/deep/util.almd", "src/main.almd"] — every
  // .almd at any depth under src/, sorted; "src/*.almd" would stop at depth 1
  // and "*.almd" lists only the current directory.
  let files = fs.glob("src/**/*.almd")!
  for f in files {
    println(f)
  }
}
```

### `fs.create_temp_file(prefix: String) -> Result[String, String]`

Create a temporary file with a given prefix, return its path

```almd run
import fs

effect fn main() -> Unit = {
  let path = fs.create_temp_file("almide-")!
  println("${fs.is_file(path)} ${string.contains(path, "almide-")}")
  fs.remove(path)!
}
```
```output
true true
```

### `fs.create_temp_dir(prefix: String) -> Result[String, String]`

Create a temporary directory with a given prefix, return its path

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("build-")!
  println("${fs.is_dir(dir)} ${string.contains(dir, "build-")}")
  fs.remove_all(dir)!
}
```
```output
true true
```

### `fs.is_symlink(path: String) -> Bool`

Check if a path is a symbolic link

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/link", "a regular file, not a symlink")!
  println(if fs.is_symlink("${dir}/link") then "symlink" else "not a symlink")
  fs.remove_all(dir)!
}
```
```output
not a symlink
```

### `fs.modified_at(path: String) -> Result[Int, String]`

Get file modification time as Unix timestamp (seconds)

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write("${dir}/file.txt", "hello")!
  let ts = fs.modified_at("${dir}/file.txt")!
  println("${ts > 0}")
  fs.remove_all(dir)!
}
```
```output
true
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (41 functions)

```
// Whole regular file (<= 1 GiB) as UTF-8; err if missing or not UTF-8.
// @since 0.5.0 or earlier
effect fs.read_text(path: String) -> String

// Bytes as Ints 0..255; regular file <= 1 GiB; err if missing.
// @since 0.5.0 or earlier
effect fs.read_bytes(path: String) -> List[Int]

// Overwrites or creates; err if no parent dir.
// @since 0.5.0 or earlier
effect fs.write(path: String, content: String) -> Unit

// Writes low byte of each Int (-1 -> 255).
// @since 0.5.0 or earlier
effect fs.write_bytes(path: String, bytes: List[Int]) -> Unit

// Writes data verbatim, replacing the file.
// @since 0.13.4 or earlier
effect fs.write_bytes_raw(path: String, data: Bytes) -> Unit

// Appends content; creates the file if missing.
// @since 0.5.0 or earlier
effect fs.append(path: String, content: String) -> Unit

// Creates dir and parents; ok if it exists.
// @since 0.5.0 or earlier
effect fs.mkdir_p(path: String) -> Unit

// False if missing or a dangling symlink.
// @since 0.6.0 or earlier
effect fs.exists(path: String) -> Bool

// CRLF-aware lines of a regular file <= 1 GiB; a final newline adds no line.
// @since 0.5.0 or earlier
effect fs.read_lines(path: String) -> List[String]

// Streamed fold over lines; err if missing.
// @since 0.57.0
effect fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> A

// Calls f per line, streamed; may err mid-file.
// @since 0.57.0
effect fs.for_each_line(path: String, f: (String) -> Unit) -> Unit

// Folds lines starting in bytes [start, end).
// @since 0.57.0
effect fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A

// Parallel chunk partials, in order; [] if empty.
// @since 0.57.0
effect fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]

// Text, or none if missing; err if not UTF-8.
// @since 0.54.0
effect fs.read_text_if_exists(path: String) -> Option[String]

// Bytes as Ints, or none if missing.
// @since 0.54.0
effect fs.read_bytes_if_exists(path: String) -> Option[List[Int]]

// read_lines result, or none if missing.
// @since 0.54.0
effect fs.read_lines_if_exists(path: String) -> Option[List[String]]

// File Bytes, or none if missing.
// @since 0.54.0
effect fs.read_bytes_raw_if_exists(path: String) -> Option[Bytes]

// Deletes a file or empty dir; err if missing.
// @since 0.5.0 or earlier
effect fs.remove(path: String) -> Unit

// Sorted names (not paths); err if not a dir.
// @since 0.5.0 or earlier
effect fs.list_dir(path: String) -> List[String]

// True for a dir; follows symlinks.
// @since 0.6.0 or earlier
effect fs.is_dir(path: String) -> Bool

// True for a regular file; follows symlinks.
// @since 0.6.0 or earlier
effect fs.is_file(path: String) -> Bool

// Copies src to dst; overwrites dst.
// @since 0.5.0 or earlier
effect fs.copy(src: String, dst: String) -> Unit

// Moves src to dst; replaces an existing dst.
// @since 0.5.0 or earlier
effect fs.rename(src: String, dst: String) -> Unit

// Sorted paths below dir; links listed, not followed.
// @since 0.5.0 or earlier
effect fs.walk(dir: String) -> List[String]

// Deletes a tree or file; err if missing.
// @since 0.5.0 or earlier
effect fs.remove_all(path: String) -> Unit

// Size in bytes; err if missing.
// @since 0.5.0 or earlier
effect fs.file_size(path: String) -> Int

// Host temp dir; a location, not a safe name.
// @since 0.5.0 or earlier
effect fs.temp_dir() -> String

// Size, kind and mtime; err if missing.
// @since 0.5.0 or earlier
effect fs.stat(path: String) -> FileStat

// Sorted * and ** matches; links not followed.
// @since 0.6.0 or earlier
effect fs.glob(pattern: String) -> List[String]

// New private file, random name; its path.
// @since 0.6.0 or earlier
effect fs.create_temp_file(prefix: String) -> String

// New private dir, random name; its path.
// @since 0.6.0 or earlier
effect fs.create_temp_dir(prefix: String) -> String

// True for a symlink, even a dangling one.
// @since 0.6.0 or earlier
effect fs.is_symlink(path: String) -> Bool

// Mtime in Unix seconds; err if missing.
// @since 0.6.0 or earlier
effect fs.modified_at(path: String) -> Int

// Whole regular file (<= 1 GiB) as Bytes; err if missing.
// @since 0.13.4 or earlier
effect fs.read_bytes_raw(path: String) -> Bytes

// dir as an FsRoot; err if dir is not a directory.
// @since unreleased
effect fs.open_root(dir: String) -> FsRoot

// read_text of path inside root; err if it escapes.
// @since unreleased
effect fs.root_read_text(root: FsRoot, path: String) -> String

// read_bytes_raw of path inside root; err if it escapes.
// @since unreleased
effect fs.root_read_bytes(root: FsRoot, path: String) -> Bytes

// write of content to path inside root; err if it escapes.
// @since unreleased
effect fs.root_write_text(root: FsRoot, path: String, content: String) -> Unit

// list_dir of path inside root; err if it escapes.
// @since unreleased
effect fs.root_list_dir(root: FsRoot, path: String) -> List[String]

// exists of path inside root; err if it escapes.
// @since unreleased
effect fs.root_exists(root: FsRoot, path: String) -> Bool

// remove of path inside root; err if it escapes.
// @since unreleased
effect fs.root_remove(root: FsRoot, path: String) -> Unit
```

## Type index (1 types)

```
// Path metadata; modified is Unix seconds.
// @since 0.15.0 or earlier
type fs.FileStat = { size: Int, is_dir: Bool, is_file: Bool, modified: Int }
```

<!-- END GENERATED SIGNATURE INDEX -->
