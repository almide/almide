> 说明（固定历史）：标准库重设计 L2/L3 — fs：Bytes 是唯一字节容器（read_bytes/write_bytes 保持名称并改用 Bytes，移除 _raw 重复项），size、mtime 归 stat，_if_exists 和流式族保留门禁。
<a id="l2l3-fs"></a>

# L2/L3: fs

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-fs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-fs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-fs.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、独立文件系统/安全probe或核实现行API。原比较、使用数、实测均为原作者主张。保留append_bytes先拒绝后采纳、write/append旧名与后来text版、提案plain fn与表中effect签名。根隔离受原文先检查后调用期间链接替换竞争的限制，前文不会逃逸不能当作无竞争安全保证。path.normalize的词法处理不是符号链接解析。保留日文占位符分别表示类型、根/路径段。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-25：临时目录属于 fs）。
> 来源：Rust https://doc.rust-lang.org/std/fs/ ・ Go https://pkg.go.dev/os ・ Python https://docs.python.org/3/library/pathlib.html ・ TS/JS https://nodejs.org/api/fs.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （stdlib 无 fs）・ Elixir https://hexdocs.pm/elixir/File.html ・ OCaml https://ocaml.org/manual/latest/api/In_channel.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io/java.io.-file/ ・ Swift https://developer.apple.com/documentation/foundation/filemanager

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 读取全部文本 | `read_text` | `read_to_string` | `os.ReadFile`+`string` | `read_text` | `readFile(p, "utf8")` | — | `File.read` | `In_channel.input_all` | `readText` | `String(contentsOfFile:)` |
| 读取全部字节 | `read_bytes`（List[Int]）・`read_bytes_raw`（Bytes）→ `read_bytes`（Bytes） 统一 | `fs::read` | `os.ReadFile` | `read_bytes` | `readFile(p)` | — | `File.read` | `In_channel.input_all` | `readBytes` | `Data(contentsOf:)` |
| 读取行列表 | `read_lines` | `read_to_string`+`lines` | `bufio.Scanner` | `read_text().splitlines()` | — | — | `File.stream!` | `In_channel.input_lines` | `readLines` | — |
| 读取，不存在为 none | `read_*_if_exists` | — | — | — | — | — | — | — | — | — |
| 流式折叠行 | `fold_lines`・`for_each_line`・`fold_lines_range`・`fold_lines_chunked` | `BufRead::lines` | `Scanner` | `for line in f` | `readline` | — | `File.stream!` | `In_channel.fold_lines` | `forEachLine`・`useLines` | — |
| 写入（覆盖） | `write`・`write_bytes`（List[Int]）・`write_bytes_raw` → `write`・`write_bytes`（Bytes） | `fs::write` | `os.WriteFile` | `write_text`・`write_bytes` | `writeFile` | — | `File.write` | `Out_channel.with_open_text` | `writeText`・`writeBytes` | `write(to:)` |
| 追加 | `append` | `OpenOptions::append` | `OpenFile(O_APPEND)` | `open("a")` | `appendFile` | — | `File.write(…, [:append])` | `open_gen [Open_append]` | `appendText` | `FileHandle.seekToEnd` |
| 创建目录 | `mkdir_p`（含父目录） | `create_dir`・`create_dir_all` | `Mkdir`・`MkdirAll` | `mkdir(parents=)` | `mkdir({recursive})` | — | `mkdir`・`mkdir_p` | `Sys.mkdir` | `mkdir`・`mkdirs` | `createDirectory` |
| 删除 | `remove`・`remove_all` | `remove_file`・`remove_dir_all` | `Remove`・`RemoveAll` | `unlink`・`shutil.rmtree` | `rm` | — | `rm`・`rm_rf` | `Sys.remove` | `delete`・`deleteRecursively` | `removeItem` |
| 复制、移动 | `copy`・`rename` | `copy`・`rename` | —・`Rename` | `shutil.copy`・`rename` | `copyFile`・`rename` | — | `cp`・`rename` | —・`Sys.rename` | `copyTo`・`renameTo` | `copyItem`・`moveItem` |
| 列表、遍历、glob | `list_dir`・`walk`・`glob` | `read_dir` | `ReadDir`・`WalkDir`・`Glob` | `iterdir`・`rglob`・`glob` | `readdir`・`glob` | — | `ls`・`Path.wildcard` | `Sys.readdir` | `listFiles`・`walk` | `contentsOfDirectory` |
| 存在与种类 | `exists`・`is_dir`・`is_file`・`is_symlink` | `exists`・`is_dir`・`is_file`・`is_symlink` | `Stat`+`IsDir` | `exists`・`is_dir`・`is_file`・`is_symlink` | `existsSync`・`stat` | — | `exists?`・`dir?`・`regular?` | `Sys.file_exists`・`is_directory` | `exists`・`isDirectory`・`isFile` | `fileExists` |
| 元数据 | `stat`・`file_size`・`modified_at` | `metadata` | `Stat` | `stat`・`getsize`・`getmtime` | `stat` | — | `stat` | `Unix.stat` | `length`・`lastModified` | `attributesOfItem` |
| 临时文件 | `temp_dir`・`create_temp_file`・`create_temp_dir` | `env::temp_dir` | `CreateTemp`・`MkdirTemp` | `tempfile` | `mkdtemp` | — | `System.tmp_dir` | `Filename.temp_file` | `createTempFile` | `temporaryDirectory` |
| 创建、读取符号链接 | — | `symlink`・`read_link` | `Symlink`・`Readlink` | `symlink_to`・`readlink` | `symlink`・`readlink` | — | `ln_s`・`read_link` | `Unix.symlink` | — | `createSymbolicLink` |
| 权限 | — | `set_permissions` | `Chmod` | `chmod` | `chmod` | — | `chmod` | `Unix.chmod` | `setExecutable` | `setAttributes` |
| 文件句柄 | — | `File` | `os.File` | `open` | `open` | — | `File.open` | `In_channel`・`Out_channel` | `File` | `FileHandle` |
| 解析真实路径 | — | `canonicalize` | `EvalSymlinks` | `resolve` | `realpath` | — | — | `Unix.realpath` | `canonicalPath` | `resolvingSymlinksInPath` |

<a id="完全性の規則"></a>

### 完备性规则

- **内容读取族：**{文本 `read_text`、行 `read_lines`、字节 `read_bytes`（`Bytes`）} 三种。每种恰有一个 `_if_exists` 版本（`Result[Option[T], String]`：不存在为 none，其余失败为 err）。元数据、目录操作、写入、删除不加此后缀（理由同当前 `tests/fs_if_exists_family_gate_test.rs` 门禁）。
- **行流式处理族：**限定为 {`fold_lines`、`for_each_line`、`fold_lines_range`、`fold_lines_chunked`}。可能失败的 callback 版本仅用于能确定首个 err 的两个顺序操作（`tests/fs_streaming_family_gate_test.rs`、ADR-0006）。
- **写入族：**{覆盖 `write`（String）、`write_bytes`（Bytes）、追加 `append`（String）}。不提供 Bytes 追加（下方 R-fs-04）。
- **字节容器只有 `Bytes`**（R-L0-07 第 4 条）。
- **元数据由 `stat` 负责。** 不提供仅取一个 `FileStat` 字段的函数（R-L0-01）。存在及种类谓词（`exists`、`is_dir`、`is_file`、`is_symlink`）在不存在时返回 false，与 `stat`（不存在时 err）不同，故保留。

<a id="l2-の判定"></a>

### L2 判定

- 类型不一致与重复：`read_bytes`、`write_bytes`、`read_bytes_if_exists` 是 `List[Int]`，同职责 `_raw` 版为 `Bytes` → R-fs-01（无后缀名称改为 `Bytes`，移除 `_raw` 版）。
- 重复：`file_size` ＝ `stat(p).size`，`modified_at` ＝ `stat(p).modified`（实测一致，probe-fs）→ R-fs-02。
- 跨模块重复：`env.temp_dir` ＝ `fs.temp_dir` → 保留 fs 侧（L2-env R-env-01）。
- 族门禁：R-fs-01 将 `_if_exists` 族从 4 → 3 → R-fs-03。
- 缺失职责（至少 5/9）：仅创建一级目录、创建/读取符号链接、权限、文件句柄、真实路径 → R-fs-04（拒绝及理由）。

<a id="案"></a>

## 提案

<a id="r-fs-01-バイトの読み書きを無印の名前のまま-bytes-にし_raw-版を撤去する---採用"></a>

### R-fs-01 字节读写保留无后缀名称并改用 `Bytes`，移除 `_raw` 版 — ○ 采纳

- 内容：
  - 改为 `fs.read_bytes(p) -> Bytes`、`fs.write_bytes(p, data: Bytes)`、`fs.read_bytes_if_exists(p) -> Bytes?`。名称不变，`List[Int]` 改为 `Bytes`。
  - 移除相同职责的 `fs.read_bytes_raw`（→ `fs.read_bytes`）、`fs.write_bytes_raw`（→ `fs.write_bytes`）、`fs.read_bytes_raw_if_exists`（→ `fs.read_bytes_if_exists`）。
- 名称：改变类型后，旧写法（传入/接收 `List[Int]`）必定产生类型错误，不会悄悄得到不同结果，因此是 R-L0-14 第 4 条允许的变化。`_raw` 只是表示“字节序列类型不同的版本”的后缀，`Bytes` 成为唯一容器后便无意义。io 也对齐相同形式（`io.read_bytes`、`io.write_bytes`，L2-io R-io-01）。
- 依据的 L0 规则：R-L0-07 第 4 条、R-L0-01、R-L0-14 第 4 条。
- (a) 9 种语言的文件字节均使用专用字节序列类型，没有语言返回数字列表。
- (b) `wasi:filesystem`。新 `read_bytes`、`write_bytes` 原样使用当前 `_raw` 实现。该实现已在 native、wasm 两者运行（没有壁条目）。
- (c) `fs.write_bytes` 只写 Int 低 8 位（`-1` 变为 255），越界值悄悄变化。`List[Int]` 每字节占用 8 字节（#2078 实测）。
- (d) 语义变化（类型变化）与移除。
  - 无后缀三函数：旧调用产生类型错误，通过提示和 `almide fix` 重写。将 `fs.read_bytes(p)!` 当 `List[Int]` 使用之处改为 `bytes.to_list(fs.read_bytes(p)!)`，`fs.write_bytes(p, xs)` 改为 `fs.write_bytes(p, bytes.from_list(xs))`。类型变化列入 epoch 15 的 `breaks`，数量写入 `summary`。使用数：`read_bytes` 8/13/20/2、`write_bytes` 12/1/13/3、`read_bytes_if_exists` 10/4/5/2。
  - `_raw` 三函数：`@deprecated(since=15, use="fs.read_bytes")`（`write_bytes_raw` 为 `use="fs.write_bytes"`，`read_bytes_raw_if_exists` 为 `use="fs.read_bytes_if_exists"`），`removal_epoch = 16`。仅名称一对一改名，由 `almide fix` 重写。
  - 无后缀类型变化在与 `_raw` 窗口相同的 epoch 15 引入。窗口期间 `_raw` 代码照常运行，只有用无后缀名称处理 `List[Int]` 的代码产生类型错误。
- 门禁提案：R-L0-G“拒绝有 Bytes 对应项的 `List[Int]` 字节 API”（与 L2-io R-io-01 相同检查）。
- issue：#3896

<a id="r-fs-02-file_size-と-modified_at-を撤去しstat-に一本化する---採用"></a>

### R-fs-02 移除 `file_size`、`modified_at`，统一到 `stat` — ○ 采纳

- 内容：`fs.file_size(p)` → `fs.stat(p)!.size`，`fs.modified_at(p)` → `fs.stat(p)!.modified`。同时将 `stat` 单行说明“Size, kind and mtime”对齐实际字段（`size`、`is_dir`、`is_file`、`modified`）（实测：`.mtime` 产生 E013）。
- 依据的 L0 规则：R-L0-01（不以两个拼写返回同一值）。
- (a) Rust、Go、Node、Elixir、OCaml 仅提供元数据结构体。另有独立函数的是 Python（`getsize`）与 Kotlin（`length`）。
- (b) 无变化（`wasi:filesystem` 的 `stat`）。
- (c) 移除后提示是机械的，只需记住 `stat`。
- (d) 移除。`@deprecated(since=15, use="fs.stat(path)!.size")`（`modified_at` 为 `use="fs.stat(path)!.modified"`），`removal_epoch = 16`。由 `almide fix` 重写。使用数：`file_size` 8/2/9/1、`modified_at` 3/0/8/1。
- 门禁提案：向同义词门禁规则 5 添加“返回值等于另一公开函数返回值的单个字段”无法通过实现体比较检测，因此用 R-L0-G 允许列表阻止（新增 projection 函数即失败）。
- issue：#3912

<a id="r-fs-03-2-つの族のゲートを新しい形に合わせる---採用"></a>

### R-fs-03 两个族门禁对齐新形式 — ○ 采纳

- 内容：将 `tests/fs_if_exists_family_gate_test.rs` 的族改为 {`read_text`、`read_lines`、`read_bytes`} 三个（移除 `read_bytes_raw_if_exists`，epoch 16 时 4 → 3）。窗口期间不计 `@deprecated` 行。门禁还确认 `read_bytes_if_exists` 类型为 `Bytes?`。行流式族门禁不变。
- 依据：CLAUDE.md“API families are extended by matrix”、R-L0-14。
- (b) 无影响。(c) 族形式与文档一致。(d) 仅门禁变化。
- issue：与 R-fs-01 相同
- issue：#3896

<a id="r-fs-04-59-以上が持つが-almide-に無い役割---不採用いまは足さない"></a>

### R-fs-04 至少 5/9 提供而 Almide 缺少的职责 — × 不采纳（目前不加）

- 理由：各职责要么现有函数已足够（`mkdir` 可用 `mkdir_p`，句柄可用行流式族与 `append`），要么无 WASI 路径（chmod），要么尚无需求（各项理由如下）。
- 仅创建一级目录的 `mkdir`：`mkdir_p` 足够。“已存在则 err”只用于锁，有两种反而易选错（R-L0-01）。
- 创建/读取符号链接：`wasi:filesystem` 有 `symlink-at`、`readlink-at`，但尚无使用需求。可通过 `is_symlink` 判断。有需求时成对新增 `symlink`、`read_link`。
- 权限（chmod）：WASI filesystem 无权限操作。R-L0-15 三条路径中，没有标准 WASI 接口，也没有添加临时 host op 的理由（使用需求）。有需求时以 package 提供（由 package 决定 native 权限与不具备此能力的 wasm 如何处理）。不加入标准库。
- 文件句柄：读取由行流式族满足，写入由 `append` 满足。新增句柄类型会给同一处理提供两种写法（与 L1 拒绝惰性迭代器理由相同）。
- 真实路径解析：`path.normalize` 在词法层面足够。跟随符号链接的版本需要 R-L0-15 实现（重复 `readlink-at`），有需求时再加。
- Bytes 追加：`append_<型>` 被 R-L0-04 保留为“向 `mut` 字节序列追加”的动词，因此 `fs.append_bytes` 冲突命名规则，也无使用示例。
- issue：无

<a id="r-fs-05-writeappend-を-write_textappend_text-にしappend_bytes-を足す---採用"></a>

### R-fs-05 `write`、`append` 改为 `write_text`、`append_text`，新增 `append_bytes` — ○ 采纳

- 当前：读取侧 `read_text`、`read_bytes`，写入侧 `write`、`write_bytes`，追加只有 `append`。按 `read_text` 的对应项写 `write_text` 会产生 E002，没有追加字节函数。
- 提案：改名 `write` → `write_text`、`append` → `append_text`，新增 `fn fs.append_bytes(path: String, data: Bytes) -> Unit`。读、写、追加都具备 `_text`、`_bytes` 两种。
- 依据的 L0 规则：R-L0-12（族按矩阵对齐）、R-L0-00（Almide 内相同职责拼写不一致）。
- (a) Python `write_text`/`write_bytes`，Kotlin `writeText`/`writeBytes`/`appendText`/`appendBytes`。Rust `fs::write`、Go `WriteFile` 按类型区分，故只有一个，而 Almide 不按类型分派。
- (d) 改名 2、新增 1。`@deprecated(since=15, use="fs.write_text")`、`use="fs.append_text"`、`removal_epoch = 16`，由 `almide fix` 重写。
- 完备性规则：`fs` 整文件读写为 {read、write、append} × {text、bytes} 六格。
- issue：#3979

<a id="r-fs-06-根を決めてその下だけを触る関数fsopen_rootfsroot_を足す---採用"></a>

### R-fs-06 新增限定根目录、只操作其下的函数（`fs.open_root`、`fs.root_*`）— ○ 采纳

- 当前：`fs.*` 接受任意 `String` 路径。将不可信输入拼到基准目录时，可借 `..`、绝对路径、符号链接逃逸（#4022，安全表第 10 行）。`path.from_string` 仅在字面拒绝 `..`，不检查符号链接。
- 提案：新增对应 Go 1.24 `os.Root` 的根类型 `FsRoot`，及仅操作根下的函数。
  - `effect fn open_root(dir: String) -> FsRoot`（非目录则 err）
  - `root_read_text(root, path) -> String`、`root_read_bytes(root, path) -> Bytes`、`root_write_text(root, path, content) -> Unit`、`root_list_dir(root, path) -> List[String]`、`root_exists(root, path) -> Bool`、`root_remove(root, path) -> Unit`
  - 若 `path` 是绝对路径、包含越过根的 `..`，或根下任一层是符号链接（无论指向哪里），则不触碰任何内容，返回 `err("fs: path escapes root: <path>")`。否则，对词法折叠 `.`、`..` 后的 `<根>/<段>` 调用普通 fs 函数并返回其结果。
  - 检查仅用 Almide 编写一次（`stdlib/fs.almd`），构建于普通 fs 函数之上。native 与 wasm 对同一路径以相同措辞拒绝。
- 名称：读取、写入、列表、存在、删除的最小集合。在现有动词 `read_text`、`read_bytes`、`write_text`（R-fs-05 新拼写）、`list_dir`、`exists`、`remove` 前加 `root_`。参数名：主体 `root`、路径 `path`（R-L0-30）、写入内容 `content`。均为返回副本的函数，故不受 R-L0-04 动词约束。`root_exists` 对根外查询返回 err 而非悄悄 false，因此以 `Result` 包装 `Bool`。
- 依据的 L0 规则：R-L0-30、R-L0-15（两 target 相同）、R-L0-47（默认保护不可信输入）。
- (a) Go 1.24 `os.Root`（`OpenRoot`、`Root.Open`、`ReadFile`、`WriteFile`、`Remove`）、Rust `cap-std` 的 `Dir`、Linux `openat2(RESOLVE_BENEATH)`、WASI preopen。Go 跟随保持在根内的符号链接，这里不跟随（同 `RESOLVE_NO_SYMLINKS`）。跟随需 `read_link`，fs 尚无此函数（R-fs-04）。
- (b) wasm：普通 fs 函数之上的 Almide，因此两 target 路径相同。
- (c) 拼接外来名称是 LLM 最容易写的形式。改为打开并传递根，即便忘写 `..` 或链接检查也不会逃逸。
- (d) 仅新增，既有函数语义不变。
- 剩余漏洞：检查在调用前执行，期间若目录被替换为链接就会发生竞争。封堵需基于描述符解析（native `openat2`、wasm preopen）。整个程序的上限为 `almide.toml` 的 `[permissions] fs`（docs/specs/effect-system.md）。
- 门禁：`spec/wasm_cross/fs_root_escape.almd`（`..`、绝对路径、嵌套）、`tests/fs_scope_test.rs`（根内链接指向外部时两 target 返回相同 err）、`spec/stdlib/fs_root_test.almd`。
- issue：#4022

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，标明 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `fs.read_text` | `effect fn read_text(path: String) -> String` | Rust `read_to_string` / Python `read_text` / Kotlin `readText` / Elixir `File.read` | 保留 | — |
| `fs.read_bytes` | `effect fn read_bytes(path: String) -> List[Int]` | Rust `fs::read` / Go `ReadFile` / Python `read_bytes` | 语义变化（`-> Bytes`；唯一字节读取 API） | R-fs-01 |
| `fs.read_bytes_raw` | `effect fn read_bytes_raw(path: String) -> Bytes` | 同上 | 移除（→ `read_bytes`） | R-fs-01 |
| `fs.read_lines` | `effect fn read_lines(path: String) -> List[String]` | Kotlin `readLines` / OCaml `input_lines` | 保留 | — |
| `fs.read_text_if_exists` | `effect fn read_text_if_exists(path: String) -> String?` | — （Almide 特有，ADR-0004 D4） | 保留 | R-fs-03 |
| `fs.read_bytes_if_exists` | `effect fn read_bytes_if_exists(path: String) -> List[Int]?` | — | 语义变化（`-> Bytes?`） | R-fs-01・R-fs-03 |
| `fs.read_lines_if_exists` | `effect fn read_lines_if_exists(path: String) -> List[String]?` | — | 保留 | R-fs-03 |
| `fs.read_bytes_raw_if_exists` | `effect fn read_bytes_raw_if_exists(path: String) -> Bytes?` | — | 移除（→ `read_bytes_if_exists`） | R-fs-01・R-fs-03 |
| `fs.fold_lines` | `effect fn fold_lines[A](path: String, init: A, f: (A, String) -> A) -> A` | OCaml `In_channel.fold_lines` / Kotlin `useLines` | 保留 | — |
| `fs.for_each_line` | `effect fn for_each_line(path: String, f: (String) -> Unit) -> Unit` | Kotlin `forEachLine` / Rust `lines()` | 保留 | — |
| `fs.fold_lines_range` | `effect fn fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` | —（Almide 特有） | 语义变化（负位置 abort） | R-L0-29、R-L0-46 |
| `fs.fold_lines_chunked` | `effect fn fold_lines_chunked[A](path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` | —（Almide 特有，`@concurrent`） | 保留 | — |
| `fs.write` | `effect fn write(path: String, content: String) -> Unit` | Rust `fs::write` / Go `WriteFile` / Python `write_text` / Elixir `File.write` | 改名 → `write_text` | R-fs-05 |
| `fs.write_bytes` | `effect fn write_bytes(path: String, bytes: List[Int]) -> Unit` | Python `write_bytes` / Kotlin `writeBytes` | 语义变化（`data: Bytes`） | R-fs-01 |
| `fs.write_bytes_raw` | `effect fn write_bytes_raw(path: String, data: Bytes) -> Unit` | 同上 | 移除（→ `write_bytes`） | R-fs-01 |
| `fs.append` | `effect fn append(path: String, content: String) -> Unit` | Node `appendFile` / Kotlin `appendText` | 改名 → `append_text` | R-fs-04、R-fs-05 |
| `fs.mkdir_p` | `effect fn mkdir_p(path: String) -> Unit` | Rust `create_dir_all` / Go `MkdirAll` / Elixir `mkdir_p` | 保留 | R-fs-04 |
| `fs.exists` | `effect fn exists(path: String) -> Bool` | Rust・Python `exists` / Elixir `exists?` | 保留（R-L0-09 动词集） | — |
| `fs.is_dir` | `effect fn is_dir(path: String) -> Bool` | Rust `is_dir` / Kotlin `isDirectory` | 保留 | — |
| `fs.is_file` | `effect fn is_file(path: String) -> Bool` | Rust `is_file` / Kotlin `isFile` | 保留 | — |
| `fs.is_symlink` | `effect fn is_symlink(path: String) -> Bool` | Rust `is_symlink` / Python `is_symlink` | 保留 | — |
| `fs.remove` | `effect fn remove(path: String) -> Unit` | Rust `remove_file` / Go `Remove` / Elixir `rm` | 保留 | — |
| `fs.remove_all` | `effect fn remove_all(path: String) -> Unit` | Rust `remove_dir_all` / Go `RemoveAll` / Elixir `rm_rf` | 保留 | — |
| `fs.list_dir` | `effect fn list_dir(path: String) -> List[String]` | Rust `read_dir` / Go `ReadDir` / Python `listdir` / Elixir `ls` | 保留（无多数） | — |
| `fs.walk` | `effect fn walk(dir: String) -> List[String]` | Go `WalkDir` / Python `walk` | 保留 | — |
| `fs.glob` | `effect fn glob(pattern: String) -> List[String]` | Go `Glob` / Python `glob` / Node `glob` | 保留 | — |
| `fs.copy` | `effect fn copy(src: String, dst: String) -> Unit` | Rust `copy` / Elixir `cp` | 保留 | — |
| `fs.rename` | `effect fn rename(src: String, dst: String) -> Unit` | Rust・Go・Python・Node `rename` | 保留 | — |
| `fs.stat` | `effect fn stat(path: String) -> FileStat` | Rust `metadata` / Go `Stat` / Python `stat` / Node `stat` | 保留（修正单行说明） | R-fs-02 |
| `fs.file_size` | `effect fn file_size(path: String) -> Int` | Python `getsize` / Kotlin `length` | 移除（→ `stat(p)!.size`） | R-fs-02 |
| `fs.modified_at` | `effect fn modified_at(path: String) -> Int` | Python `getmtime` / Kotlin `lastModified` | 移除（→ `stat(p)!.modified`） | R-fs-02 |
| `fs.temp_dir` | `effect fn temp_dir() -> String` | Rust `env::temp_dir` / Go `TempDir` / Elixir `System.tmp_dir` | 保留（`env.temp_dir` 被吸收） | L2-env R-env-01 |
| `fs.create_temp_file` | `effect fn create_temp_file(prefix: String) -> String` | Go `CreateTemp` / Python `mkstemp` | 保留 | — |
| `fs.create_temp_dir` | `effect fn create_temp_dir(prefix: String) -> String` | Go `MkdirTemp` / Python `mkdtemp` / Node `mkdtemp` | 保留 | — |
| `fs.append_bytes` | `effect fn append_bytes(path: String, data: Bytes) -> Unit`（提案） | Kotlin `appendBytes` / Python `open("ab")` | 新增 | R-fs-05 |
| `fs.open_root` | `effect fn open_root(dir: String) -> FsRoot`（提案） | Go `os.OpenRoot` / Rust cap-std `Dir::open_ambient_dir` | 新增 | R-fs-06 |
| `fs.root_read_text` | `effect fn root_read_text(root: FsRoot, path: String) -> String`（提案） | Go `Root.ReadFile` | 新增 | R-fs-06 |
| `fs.root_read_bytes` | `effect fn root_read_bytes(root: FsRoot, path: String) -> Bytes`（提案） | Go `Root.ReadFile` | 新增 | R-fs-06 |
| `fs.root_write_text` | `effect fn root_write_text(root: FsRoot, path: String, content: String) -> Unit`（提案） | Go `Root.WriteFile` | 新增 | R-fs-06 |
| `fs.root_list_dir` | `effect fn root_list_dir(root: FsRoot, path: String) -> List[String]`（提案） | Go `Root.Open` + `ReadDir` | 新增 | R-fs-06 |
| `fs.root_exists` | `effect fn root_exists(root: FsRoot, path: String) -> Bool`（提案） | Go `Root.Stat` | 新增 | R-fs-06 |
| `fs.root_remove` | `effect fn root_remove(root: FsRoot, path: String) -> Unit`（提案） | Go `Root.Remove` | 新增 | R-fs-06 |
