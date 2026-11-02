# fs

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fs.zh-CN.md)



> 原文：[docs/stdlib/fs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/fs.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名和代码内注释保留原文，在代码外附说明译文。这是生成索引的固定副本，本译文不是生成目标。 原文 fold_lines_range 正文按行首前一个字节决定归属，索引却简写为在范围内开始的行；保留原文差异。不跟随链接的说明针对 walk/glob 的遍历条目，指定的 dir 本身仍跟随链接。2026-10-25 的其他语言比较是原文测量记录，本译文未重新测量。

文件系统。import fs，effect。

### `_if_exists` 内容读取器——将不存在作为值

`read_text_if_exists` / `read_bytes_if_exists` / `read_lines_if_exists` 返回 `Result[Option[T], String]`：路径（或父目录）不存在时为 `ok(none)`，可读时为 `ok(some(content))`，实际失败（权限、路径是目录、IO）时为 `err(msg)`。分类在同一次读取内部完成，没有竞态——不同于事先检查 `fs.exists`。契约 C-215；支持原生和两条 wasm 路径，各目标可用性记录在路由清单中。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/config.toml"
  let cfg = fs.read_text_if_exists(path)! ?? "default"
  println(cfg)
  fs.write_text(path, "port = 8080")!
  println(fs.read_text_if_exists(path)! ?? "default")
  fs.remove_all(dir)!
}
```
```output
default
port = 8080
```

### 整文件读取器读取最多 1 GiB 的普通文件

`read_text` / `read_bytes` / `read_lines` 及其 `_if_exists` 对应版本读取最多 **1 GiB**（1073741824 字节）的**普通文件**；原生、嵌入式 wasm 宿主及标准 WASI 路径一致（契约 C-431，#4201）：

- 字符或块设备、FIFO 或套接字（`/dev/zero`、`/dev/null`、命名管道）返回错误 `fs.read_text("/dev/zero"): Not a regular file`。打开没有写入方的 FIFO 不会阻塞，判断的是已打开的描述符，所以检查后被替换的文件不能绕过检查。
- 更大的文件返回 `fs.read_bytes("big.bin"): File too large (the read
  limit is 1073741824 bytes)`：在读取任何字节前依据大小判断；若大小低报，也依据实际读取字节数判断。
- 目录保留自己的 `Is a directory (os error 21)`。

`_if_exists` 版本对此返回 `err`，绝不是 `ok(none)`。遍历超过上限的文件应使用 `fs.fold_lines` / `fs.for_each_line` 流式处理。读取标准输入使用 `io.read_all`，而不是 `fs.read_text("/dev/stdin")`。

### `fs.read_text(path: String) -> Result[String, String]`

将文件内容读为 UTF-8 字符串。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/config.toml", "port = 8080")!
  let text = fs.read_text("${dir}/config.toml")!
  println(text)
  fs.remove_all(dir)!
}
```
```output
port = 8080
```

### `fs.read_bytes(path: String) -> Result[Bytes, String]`

将文件内容读为 `Bytes`。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/image.png", bytes.from_list([137, 80, 78, 71]))!
  let data = fs.read_bytes("${dir}/image.png")!
  println("${bytes.to_list(data)}")
  fs.remove_all(dir)!
}
```
```output
[137, 80, 78, 71]
```

### `fs.write_text(path: String, content: String) -> Result[Unit, String]`

将字符串写入文件，创建或覆盖文件。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/output.txt", "hello")!
  println(fs.read_text("${dir}/output.txt")!)
  fs.remove_all(dir)!
}
```
```output
hello
```

### `fs.write_bytes(path: String, data: Bytes) -> Result[Unit, String]`

将 `Bytes` 写入文件，创建或覆盖文件。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/out.bin", bytes.from_list([0, 1, 2]))!
  println("${bytes.to_list(fs.read_bytes("${dir}/out.bin")!)}")
  fs.remove_all(dir)!
}
```
```output
[0, 1, 2]
```

### `fs.append_text(path: String, content: String) -> Result[Unit, String]`

向文件追加字符串，不存在则创建。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let log = "${dir}/log.txt"
  fs.append_text(log, "first line\n")!
  fs.append_text(log, "new line\n")!
  println("${fs.read_lines(log)!}")
  fs.remove_all(dir)!
}
```
```output
["first line", "new line"]
```

### `fs.append_bytes(path: String, data: Bytes) -> Result[Unit, String]`

向文件追加 `Bytes`，不存在则创建。所有目标都是真正的追加：以追加模式打开文件，绝不读取，因此不论文件大小，调用开销取决于追加的字节数（C-447）。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let out = "${dir}/out.bin"
  fs.append_bytes(out, bytes.from_list([1, 2]))!
  fs.append_bytes(out, bytes.from_list([3]))!
  println("${bytes.to_list(fs.read_bytes(out)!)}")
  fs.remove_all(dir)!
}
```
```output
[1, 2, 3]
```

### `fs.mkdir_p(path: String) -> Result[Unit, String]`

创建目录及全部父目录。

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

检查文件或目录是否存在。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/config.toml", "")!
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

将文件读为行列表。会将整个文件物化；大输入应改用 `fs.fold_lines` / `fs.for_each_line`（读取器缓冲为 O(最长行)，而非 O(文件)，不含累加器及回调的内存分配）。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "a,1\nb,2\n")!
  let lines = fs.read_lines("${dir}/data.csv")!
  println("${lines}")
  fs.remove_all(dir)!
}
```
```output
["a,1", "b,2"]
```

### `fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> Result[A, String]`

对文件的行进行 fold 而不物化它们；行语义与 `read_lines` 逐字节一致（契约 C-220）。这是聚合大型文件的默认形式。

```almd run
import fs

fn parse_row(line: String) -> Int = int.parse(line) ?? 0

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "1\n2\n3\n")!
  let total = fs.fold_lines("${dir}/data.csv", 0, (acc, line) => acc + parse_row(line))!
  println(int.to_string(total))
  fs.remove_all(dir)!
}
```
```output
6
```
**可失败的回调**使整个遍历可失败（ADR-0006，契约 C-274）：回调体用 `!` 传播时，选择首个 err 短路形式——失败行之后的行不会再调用回调，原生读取器也在那里停止。名字相同，只多一个 `!`。

```almd run
import fs

fn add_row(acc: Int, line: String) -> Int! = {
  guard line != "oops" else err("bad row: ${line}")
  acc + (int.parse(line) ?? 0)
}

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write_text(path, "1\n2\n3\n")!
  let stats = fs.fold_lines(path, 0, (acc, line) => add_row(acc, line)!)!
  println(int.to_string(stats))
  // first err wins; later lines are never visited
  fs.write_text(path, "1\noops\n3\n")!
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

不物化列表，按顺序访问文件每一行。回调可修改捕获的 `var`。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "a,1\nb,2\nc,3\n")!
  var count = 0
  fs.for_each_line("${dir}/data.csv", (line) => { count = count + 1 })!
  println(int.to_string(count))
  fs.remove_all(dir)!
}
```
```output
3
```
也接受与 `fold_lines` 相同的可失败回调形式：

```almd check
import fs

effect fn emit(line: String) -> Unit = {
  guard line != "" else err("empty line")
  println("got ${line}")
}

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write_text(path, "one\ntwo\n")!
  fs.for_each_line(path, (line) => emit(line)!)!
  fs.remove_all(dir)!
}
```

下面两个**分区**单元（`fold_lines_range` / `fold_lines_chunked`）刻意**没有**可失败形式：分区遍历没有定义“第一个”err（哪个分块先失败是线程调度可观察的结果），因此出错的分块体须自行处理错误。

### `fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> Result[A, String]`

仅 fold 属于字节范围 `[start, end)` 的行：一行归属于包含其首字节前一个字节的范围（第一行属于含字节 0 的范围）。因此对 `[0, size)`（大小取自 `fs.stat`）的分区进行 fold，会恰好访问每行一次。

```almd check
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write_text(path, "a\nbb\nccc\n")!
  let part = fs.fold_lines_range(path, 0, 4096, 0, (acc, line) => acc + 1)!
  println(int.to_string(part))
  // a partition of [0, file_size) visits every line exactly once
  let size = fs.stat(path)!.size
  let first = fs.fold_lines_range(path, 0, 4, 0, (acc, line) => acc + 1)!
  let second = fs.fold_lines_range(path, 4, size, 0, (acc, line) => acc + 1)!
  println("${first} + ${second}")
  fs.remove_all(dir)!
}
```

### `fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> Result[List[A], String]`

分块 fold，按分块顺序返回部分结果。符合条件的原生调用使用运行时线程池，以可用核心数为上限；累加器或闭包不可发送的调用走等价顺序路径。线程池耗尽时也可能就地运行。须自行合并部分结果；无论线程如何调度，结果都确定。回调必须是纯步骤函数（捕获可变状态会编译报错）。

```almd check
import fs

fn step(acc: Int, line: String) -> Int = acc + 1
fn combine(a: Int, b: Int) -> Int = a + b

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "a\nb\nc\nd\ne\n")!
  let partials = fs.fold_lines_chunked("${dir}/data.csv", 8, 0, step)!
  let stats = partials |> list.fold(0, (acc, m) => combine(acc, m))
  println(int.to_string(stats))
  fs.remove_all(dir)!
}
```

### `fs.remove(path: String) -> Result[Unit, String]`

删除文件。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/temp.txt", "scratch")!
  fs.remove("${dir}/temp.txt")!
  println("${fs.exists("${dir}/temp.txt")}")
  fs.remove_all(dir)!
}
```
```output
false
```

### `fs.list_dir(path: String) -> Result[List[String], String]`

列出目录条目。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write_text("${dir}/src/main.almd", "")!
  fs.write_text("${dir}/src/util.almd", "")!
  let entries = fs.list_dir("${dir}/src/")!
  println("${entries |> list.sort}")
  fs.remove_all(dir)!
}
```
```output
["main.almd", "util.almd"]
```

### `fs.is_dir(path: String) -> Bool`

检查路径是否为目录。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write_text("${dir}/readme.md", "")!
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

检查路径是否为普通文件。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write_text("${dir}/readme.md", "")!
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

将文件从 src 复制到 dst。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/a.txt", "hello")!
  fs.copy("${dir}/a.txt", "${dir}/b.txt")!
  println(fs.read_text("${dir}/b.txt")!)
  fs.remove_all(dir)!
}
```
```output
hello
```

### `fs.rename(src: String, dst: String) -> Result[Unit, String]`

重命名或移动文件。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/old.txt", "hello")!
  fs.rename("${dir}/old.txt", "${dir}/new.txt")!
  println("${fs.exists("${dir}/old.txt")} ${fs.exists("${dir}/new.txt")}")
  fs.remove_all(dir)!
}
```
```output
false true
```

### `fs.walk(dir: String) -> Result[List[String], String]`

递归列出目录树中的所有条目——文件和目录都包括。

符号链接作为条目列出，但绝不跟随（C-396）：指向树外目录的链接不会列出树外内容，链接环也不会令遍历重复。`fs.glob` 同样遍历。`dir` 本身按给定路径打开，其自身的符号链接会被跟随。

目前跟随链接不是目标，没有启用选项。对于其他人可写入的目录（上传或 spool 目录、解压的归档、克隆的仓库），不跟随是安全默认值。在测量过的遍历器中（2026-10-25），Go 的 `filepath.WalkDir` 也没有跟随选项，而 Python 的 `os.walk(followlinks=True)` 和 Rust walkdir 的 `follow_links(true)` 有。若要进入链接指向的目录，请自行将其路径交给 `fs.walk`。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src/util")!
  fs.write_text("${dir}/src/main.almd", "")!
  fs.write_text("${dir}/src/util/io.almd", "")!
  let all_files = fs.walk("${dir}/src")!
  println("${all_files |> list.map((f) => string.replace(f, dir, "")) |> list.sort}")
  fs.remove_all(dir)!
}
```
```output
["/src/main.almd", "/src/util", "/src/util/io.almd"]
```

### `fs.remove_all(path: String) -> Result[Unit, String]`

递归删除目录及其全部内容。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/build/obj")!
  fs.write_text("${dir}/build/obj/main.o", "")!
  fs.remove_all("${dir}/build/")!
  println("${fs.exists("${dir}/build")}")
  fs.remove_all(dir)!
}
```
```output
false
```

### `fs.temp_dir() -> String`

取得系统临时目录路径。

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

取得文件大小、类型和修改时间等元数据。

```almd check
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/file.txt", "hello")!
  let info = fs.stat("${dir}/file.txt")! // {size, is_dir, is_file, modified}
  println("${info.size} ${info.is_dir} ${info.is_file} ${info.modified > 0}")
  fs.remove_all(dir)!
}
```

### `fs.glob(pattern: String) -> Result[List[String], String]`

查找符合 glob 模式的路径。按段将模式与其字面前缀下的路径匹配（相对模式为当前目录）：`*` 匹配一个段内任意一串字符，`**` 匹配零个或多个完整段，其他均按字面解释。文件和目录都符合条件。结果排序并使用模式自身的前缀表示；相对模式相对于 cwd，开头不带 `./`。前缀目录不存在时返回 `[]`。

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

创建具有给定前缀的临时文件，返回路径。

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

创建具有给定前缀的临时目录，返回路径。

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

检查路径是否为符号链接。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/link", "a regular file, not a symlink")!
  println(if fs.is_symlink("${dir}/link") then "symlink" else "not a symlink")
  fs.remove_all(dir)!
}
```
```output
not a symlink
```



## 签名索引（37 个函数）

```
// Whole regular file (<= 1 GiB) as UTF-8; err if missing or not UTF-8.
// @since 0.5.0 or earlier
effect fs.read_text(path: String) -> String

// Whole regular file (<= 1 GiB) as Bytes; err if missing.
// @since 0.5.0 or earlier
effect fs.read_bytes(path: String) -> Bytes

// Overwrites or creates; err if no parent dir.
// @since unreleased
effect fs.write_text(path: String, content: String) -> Unit

// Writes data verbatim, replacing the file.
// @since 0.5.0 or earlier
effect fs.write_bytes(path: String, data: Bytes) -> Unit

// Appends content; creates the file if missing.
// @since unreleased
effect fs.append_text(path: String, content: String) -> Unit

// Appends data verbatim; creates the file if missing.
// @since unreleased
effect fs.append_bytes(path: String, data: Bytes) -> Unit

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

// File Bytes, or none if missing.
// @since 0.54.0
effect fs.read_bytes_if_exists(path: String) -> Option[Bytes]

// read_lines result, or none if missing.
// @since 0.54.0
effect fs.read_lines_if_exists(path: String) -> Option[List[String]]

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

// dir as an FsRoot; err if dir is not a directory.
// @since unreleased
effect fs.open_root(dir: String) -> FsRoot

// read_text of path inside root; err if it escapes.
// @since unreleased
effect fs.root_read_text(root: FsRoot, path: String) -> String

// read_bytes of path inside root; err if it escapes.
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

## 类型索引（1 个类型）

```
// Path metadata; modified is Unix seconds.
// @since 0.15.0 or earlier
type fs.FileStat = { size: Int, is_dir: Bool, is_file: Bool, modified: Int }
```



## 索引注释的说明译文

- `effect fs.read_text(path: String) -> String` — 将整个普通文件（<= 1 GiB）读为 UTF-8；不存在或非 UTF-8 则 err。 (@since 0.5.0 or earlier)
- `effect fs.read_bytes(path: String) -> Bytes` — 将整个普通文件（<= 1 GiB）读为 Bytes；不存在则 err。 (@since 0.5.0 or earlier)
- `effect fs.write_text(path: String, content: String) -> Unit` — 覆盖或创建；无父目录则 err。 (@since unreleased)
- `effect fs.write_bytes(path: String, data: Bytes) -> Unit` — 原样写入 data，替换文件。 (@since 0.5.0 or earlier)
- `effect fs.append_text(path: String, content: String) -> Unit` — 追加 content；不存在则创建。 (@since unreleased)
- `effect fs.append_bytes(path: String, data: Bytes) -> Unit` — 原样追加 data；不存在则创建。 (@since unreleased)
- `effect fs.mkdir_p(path: String) -> Unit` — 创建目录及父目录；已存在则 ok。 (@since 0.5.0 or earlier)
- `effect fs.exists(path: String) -> Bool` — 不存在或为悬空符号链接时为 false。 (@since 0.6.0 or earlier)
- `effect fs.read_lines(path: String) -> List[String]` — 普通文件（<= 1 GiB）的行，识别 CRLF；末尾换行不增加一行。 (@since 0.5.0 or earlier)
- `effect fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> A` — 对行进行流式 fold；不存在则 err。 (@since 0.57.0)
- `effect fs.for_each_line(path: String, f: (String) -> Unit) -> Unit` — 流式逐行调用 f；可能在文件中途 err。 (@since 0.57.0)
- `effect fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` — 对在字节 [start, end) 中开始的行进行 fold。 (@since 0.57.0)
- `effect fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` — 按顺序返回并行分块的部分结果；空则 []。 (@since 0.57.0)
- `effect fs.read_text_if_exists(path: String) -> Option[String]` — 文本，不存在则 none；非 UTF-8 则 err。 (@since 0.54.0)
- `effect fs.read_bytes_if_exists(path: String) -> Option[Bytes]` — 文件 Bytes，不存在则 none。 (@since 0.54.0)
- `effect fs.read_lines_if_exists(path: String) -> Option[List[String]]` — read_lines 结果，不存在则 none。 (@since 0.54.0)
- `effect fs.remove(path: String) -> Unit` — 删除文件或空目录；不存在则 err。 (@since 0.5.0 or earlier)
- `effect fs.list_dir(path: String) -> List[String]` — 排序后的名称（不是路径）；不是目录则 err。 (@since 0.5.0 or earlier)
- `effect fs.is_dir(path: String) -> Bool` — 目录则 true；跟随符号链接。 (@since 0.6.0 or earlier)
- `effect fs.is_file(path: String) -> Bool` — 普通文件则 true；跟随符号链接。 (@since 0.6.0 or earlier)
- `effect fs.copy(src: String, dst: String) -> Unit` — 将 src 复制到 dst；覆盖 dst。 (@since 0.5.0 or earlier)
- `effect fs.rename(src: String, dst: String) -> Unit` — 将 src 移到 dst；替换已有 dst。 (@since 0.5.0 or earlier)
- `effect fs.walk(dir: String) -> List[String]` — dir 下排序后的路径；列出链接但不跟随。 (@since 0.5.0 or earlier)
- `effect fs.remove_all(path: String) -> Unit` — 删除目录树或文件；不存在则 err。 (@since 0.5.0 or earlier)
- `effect fs.temp_dir() -> String` — 宿主临时目录；这是位置，不是安全的名称。 (@since 0.5.0 or earlier)
- `effect fs.stat(path: String) -> FileStat` — 大小、类型和 mtime；不存在则 err。 (@since 0.5.0 or earlier)
- `effect fs.glob(pattern: String) -> List[String]` — 排序后的 * 与 ** 匹配；不跟随链接。 (@since 0.6.0 or earlier)
- `effect fs.create_temp_file(prefix: String) -> String` — 随机名称的新私有文件，返回其路径。 (@since 0.6.0 or earlier)
- `effect fs.create_temp_dir(prefix: String) -> String` — 随机名称的新私有目录，返回其路径。 (@since 0.6.0 or earlier)
- `effect fs.is_symlink(path: String) -> Bool` — 符号链接则 true，即使悬空。 (@since 0.6.0 or earlier)
- `effect fs.open_root(dir: String) -> FsRoot` — 将 dir 作为 FsRoot；若不是目录则 err。 (@since unreleased)
- `effect fs.root_read_text(root: FsRoot, path: String) -> String` — 对 root 内的 path 执行 read_text；逃出范围则 err。 (@since unreleased)
- `effect fs.root_read_bytes(root: FsRoot, path: String) -> Bytes` — 对 root 内的 path 执行 read_bytes；逃出范围则 err。 (@since unreleased)
- `effect fs.root_write_text(root: FsRoot, path: String, content: String) -> Unit` — 向 root 内的 path 写入 content；逃出范围则 err。 (@since unreleased)
- `effect fs.root_list_dir(root: FsRoot, path: String) -> List[String]` — 对 root 内的 path 执行 list_dir；逃出范围则 err。 (@since unreleased)
- `effect fs.root_exists(root: FsRoot, path: String) -> Bool` — 对 root 内的 path 执行 exists；逃出范围则 err。 (@since unreleased)
- `effect fs.root_remove(root: FsRoot, path: String) -> Unit` — 对 root 内的 path 执行 remove；逃出范围则 err。 (@since unreleased)
- `type fs.FileStat = { size: Int, is_dir: Bool, is_file: Bool, modified: Int }` — 路径元数据；modified 为 Unix 秒。 (@since 0.15.0 or earlier)
