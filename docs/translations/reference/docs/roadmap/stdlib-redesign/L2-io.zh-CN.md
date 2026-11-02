说明：stdlib 重设计 L2/L3 — io：stdin/stdout；read_line 保留名称，用 Option 区分 EOF；read_bytes/write_bytes 同 fs 使用 Bytes；按 ADR-0022 将写入函数改为 effect fn。
<a id="l2l3-io"></a>

# L2/L3: io

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-io.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-io.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-io.zh-CN.md)

> 非规范性历史重设计资料，原本权威且不变。不自动同步、无新probe、未核实现行API。保留提案顺序：R-io-01/02旧字节API保留/改名，R-io-04取消，后段R-io-05改为句柄统一。旧迁移片段不是现行指令。虽标题已处理，effect/Option调用形式仍委托R-L0-17。EOF空Bytes区别于被拒行/Int哨兵，不构成普遍I/O安全保证。F标记和使用数为原调查。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-io.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)。
> 来源：Rust https://doc.rust-lang.org/std/io/；Go https://pkg.go.dev/bufio；Python https://docs.python.org/3/library/sys.html#sys.stdin；TS/JS https://nodejs.org/api/process.html#processstdin；Gleam https://gleam-stdlib.hexdocs.pm/gleam/io.html（仅输出）；Elixir https://hexdocs.pm/elixir/IO.html；OCaml https://ocaml.org/manual/latest/api/In_channel.html；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io/readln-or-null.html；Swift https://developer.apple.com/documentation/swift/readline(strippingnewline:)。
> (F) = 此次调查获取并核实的单元。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 读一行（区分 EOF） | `io.read_line_opt` | `read_line`（0 为 EOF） | `Scanner.Scan` | `readline()`（"" 为 EOF） | `readline` 的 `line` | — | `IO.gets`（`:eof`） | `In_channel.input_line`（option） | `readlnOrNull`（null）(F) | `readLine()`（nil） |
| 读一行（EOF 为 ""） | `io.read_line` | — | — | `sys.stdin.readline()` | — | — | — | — | — | — |
| 读全部 | `io.read_all` | `read_to_string` | `io.ReadAll` | `sys.stdin.read()` | — | — | `IO.read(:eof)` | `In_channel.input_all` | `System.in.readAllBytes` | `FileHandle.readDataToEndOfFile` |
| 所有行列表 | `process.stdin_lines` | `lines()` | `Scanner` | `readlines()` | — | — | `IO.stream` | `input_lines` | `generateSequence(::readLine)` | — |
| 读字节 | `io.read_byte`（-1 为 EOF）・`io.read_n_bytes`（List[Int]） | `read(&mut buf)` | `Read(p)` | `sys.stdin.buffer.read(n)` | `stdin.read(n)` | — | `IO.binread(n)` | `In_channel.input` | `System.in.read` | `FileHandle.readData(ofLength:)` |
| 无换行写字符串 | `io.print` | `print!` | `fmt.Print` | `print(end="")` | `process.stdout.write` | `io.print` | `IO.write` | `print_string` | `print` | `print(terminator: "")` |
| 写字节 | `io.write`（Bytes）・`io.write_bytes`（List[Int]） | `write_all` | `os.Stdout.Write` | `sys.stdout.buffer.write` | `stdout.write(buf)` | — | `IO.binwrite` | `Out_channel.output_bytes` | `System.out.write` | `FileHandle.write` |

<a id="完全性の規則"></a>

### 完整性规则

- 标准输入读取三项：{一行 `read_line`、全部 `read_all`、最多 n 字节 `read_bytes`}。全部行列表可写成 `string.lines(io.read_all()!)`，不另设函数（R-L0-01 的可组合形式）。
- 失败表示（R-L0-06 第 4 项，不返回失败哨兵）：一行在 EOF 为 none；字节在 EOF 为空 `Bytes`（同 `net.tcp_read`，如 Rust `read` 返回 0）。不用 `-1` 或 `""` 标记 EOF。
- 字节序列仅用 `Bytes`（R-L0-07 第 4 项）；读写名统一为与 fs 相同的 `read_bytes`、`write_bytes`（L2-fs R-fs-01）。
- 写出函数均为 effect fn。ADR-0022 D1 已规定 `io.print`、`io.write`、`io.write_bytes` 是 effect fn，从纯 fn 调用为 E006。
- 标准错误写出由内建 `eprintln` 拥有。

<a id="l2-の判定"></a>

### L2 判定

- 违反规则（失败哨兵）：`io.read_line` 的 EOF/空行均为 `""`，`io.read_byte` 的 EOF 为 `-1` → R-io-01。
- 重复：`io.read_line_opt` 与 `io.read_line` 同角色（一行读取）→ R-io-01（无后缀名改为 `Option`）。
- 类型差异与重复：`io.read_n_bytes`、`io.write_bytes` 用 `List[Int]`，同角色 `io.write` 用 `Bytes` → R-io-01。
- 位置错误：`process.stdin_lines`（标准输入属 io）→ R-io-01。
- 声明漏洞：`io.write`、`io.write_bytes` 声明为纯 `fn`；实测从纯 fn 调用 `io.write` 不报错（probe-io），违反 ADR-0022 D1 → R-io-02。

<a id="案"></a>

## 提案

<a id="r-io-01-読み取りをeof-を区別する-1-行bytesにそろえる---採用"></a>

### R-io-01 读取统一成“区分 EOF 的一行”和 Bytes — ○ 采纳

- 内容：
  - 一行：保留 `io.read_line` 名称，返回从 `String` 改成 `Option[String]`（EOF 为 none）。撤去 `io.read_line_opt`（→ `io.read_line`）。
  - 读字节：加 `effect fn read_bytes(n: Int) -> Bytes`（最多 n 字节，EOF 及 `n <= 0` 为空），撤去 `io.read_byte`（→ `io.read_bytes(1)`）与 `io.read_n_bytes`（→ `io.read_bytes(n)`）。
  - 写字节：保留 `io.write_bytes`，参数从 `List[Int]` 改 `Bytes`。撤去同角色 `io.write`（`Bytes`）（→ `io.write_bytes`）。读取/写入构成 `read_bytes`、`write_bytes` 对，与 fs 同形。
  - 全部行：撤去 `process.stdin_lines()`（→ `string.lines(io.read_all()!)`，L2-process 行）。
- L0 依据：R-L0-06 第 1/4 项（无后缀名为安全版，不返回失败哨兵）、R-L0-07 第 4 项（`Bytes` 是唯一字节容器）、R-L0-01（同角色一个名称）、R-L0-14 第 4 项（旧写法必定类型错误的变更可保留同名）。
- (a) Swift `readLine`、Kotlin `readlnOrNull`(F)、OCaml `input_line` 用 `Option`（nil/null/none）表示 EOF，Elixir 也返回独立值 `:eof`。仅 Python `readline()` 把 EOF 表示为 `""`。
- (b) 标准输入 `wasi:cli/stdin`，标准输出 `wasi:cli/stdout`。现有 body 均可在 native/embedded host 运行（无 wall 行）。新 `read_line` 用 `read_line_opt` body；`read_bytes` 用 `read_n_bytes` body 并将返回改 `Bytes`；新 `write_bytes` 用 `io.write` body。可在 ADR-0027 D2 固定 stdin 下确定性检验。
- (c) `while` 中读 `io.read_line()` 遇 `""` 停止的代码会在空行提前停下，LLM 察觉不到。返回变 `Option` 后，与 `""` 比较会类型错误，必定促使改写。
- (d) 语义变化（类型）与撤去。
  - `io.read_line`：返回 `Option[String]`，旧 `String` 用法变类型错误，R-L0-14 第 4 项允许。提示“`io.read_line()` 在 EOF 返回 none；`io.read_line()! ?? ""` 为旧行为”，附 `almide fix` 改写。加入 epoch 15 `breaks`，计数写入 `summary`。
  - `io.read_line_opt`：`@deprecated(since=15, use="io.read_line")`、`removal_epoch = 16`。`almide fix` 一对一把 `io.read_line_opt()` 改 `io.read_line()`。
  - `io.read_byte`：`@deprecated(since=15, use="io.read_bytes(1)")`；`io.read_n_bytes`：`@deprecated(since=15, use="io.read_bytes(n)")`，均 `removal_epoch = 16`。`read_byte` 形状变化（`Int` → `Bytes`），只给提示：“`bytes.get_u8(io.read_bytes(1)!, 0)` 为 none 则 EOF”。`read_n_bytes` 由 `almide fix` 改为 `bytes.to_list(io.read_bytes(n)!)`。
  - `io.write_bytes`：参数变 `Bytes`，旧写法类型错误；`almide fix` 改为 `io.write_bytes(bytes.from_list(xs))`。
  - `io.write`：`@deprecated(since=15, use="io.write_bytes")`、`removal_epoch = 16`，`almide fix` 一对一改写。
  - `process.stdin_lines`：在 L2-process R-process-01 行处理。
  - 使用数：`io.read_line` 5/24/20/8、`io.read_byte` 1/16/13/1、`io.read_n_bytes` 7/16/23/8、`io.write_bytes` 7/4/21/1、`io.write` 12/8/27/3。CHEATSHEET:885-890 的 `io.read_line` 说明改为“返回 `Option[String]`，EOF 为 none”（`check-llm-surface.sh`）。
- 门禁提案：R-L0-G 加“拒绝 `List[Int]` 字节 API”，逐渐缩减 io/fs/string 允许列表。native/wasm 添加 EOF fixture，区分空行后的 EOF：none 与 some("")。
- issue：#3896。

<a id="r-io-02-iowrite_bytes-を-effect-fn-に直す---採用"></a>

### R-io-02 将 `io.write_bytes` 修正为 effect fn — ○ 采纳

- 内容：`fn write_bytes(data: …) -> Unit` 改为 `effect fn write_bytes(data: Bytes) -> Unit`（类型变化见 R-io-01）。窗口期间保留的 `io.write` 也在同 epoch 改 `effect fn`。
- L0 依据：R-L0-13（接触环境的函数为 effect）、ADR-0022 D1（纯 fn 仅允许 6 个内建，不含 `io.write`、`io.write_bytes`）。
- (a) 不适用（具备 effect 类型的语言少）。
- (b) 仅 checker 变化。
- (c) 不读声明就不知道 `io.print` 为 effect、`io.write` 为 pure。ADR 与实现不同，读文档的 LLM 代码就与检查结果不符。
- (d) 语义变化（pure → effect），不改名。先例为 dialect epoch 8/13（argv 读取改 effect），同样写入 `breaks`。纯 fn 调用处成为 E006，提示“将调用方改成 `effect fn`”。在 epoch 15 `breaks` 计数。使用数 12/8/27/3（`io.write`）、7/4/21/1（`io.write_bytes`）。
- 门禁提案：对照 ADR-0022 D1 名字集合与 `io` 声明的 `effect` 栏。
- issue：#3907。

<a id="r-io-03-改行なしで標準エラーに書く-ioeprint-を足しprint-の綴りから-ioprint-へ案内する---採用"></a>

### R-io-03 增加无换行标准错误 `io.eprint`，将 `print` 引导到 `io.print` — ○ 采纳

- 当前：无换行输出仅有标准输出 `io.print`，标准错误没有。写 `print("x")` 会提示“是否 `println`？”——增加换行会改变输出。
- 提案：增加 `effect fn io.eprint(s: String) -> Unit`。未定义 `print`、`eprint` 引导至 `io.print`、`io.eprint`（R-L0-19）。
- (a) 9/9 均有无换行标准错误（Rust `eprint!` (F)、Gleam `io.print_error` (F)、Go `fmt.Fprint(os.Stderr, …)` 等）。
- (b) `wasi:cli/stderr`，同 `eprintln` 路径。
- (c) LLM 几乎总将进度显示（用 `\r` 重写）放标准错误。
- (d) 增加与诊断。
- 完整性规则：输出为 {stdout, stderr} × {有/无换行} 四单元。有换行归内建，无换行归 `io`。
- issue：#3974。

<a id="r-io-04-標準入力をバイト列で全部読む-ioread_all_bytes-を足す---取り消し十三巡目ioread_to_endiostdin-で書く--r-io-05"></a>

### R-io-04 增加读取全部 stdin 字节的 `io.read_all_bytes` — × 取消（第十三轮，用 `io.read_to_end(io.stdin())` → R-io-05）

- 当前：`io.read_all() -> String`（全部文本）和 `io.read_bytes(n) -> Bytes`（最多 n 字节），缺少读取全部字节的函数。
- 提案：`effect fn io.read_all_bytes() -> Bytes`。
- (a) 8/9 具备（Python `sys.stdin.buffer.read()` (F)、Go `io.ReadAll(os.Stdin)`、Rust `read_to_end` 等）。
- (b) `wasi:cli/stdin`，同 `read_all` 路径，不转换为 UTF-8。
- (c) 当时经过 `io.read_all()` 会将无效 UTF-8 变成替换字符，静默改变结果。
- (d) 增加 1。
- 完整性规则：stdin 读取为 {一行 `read_line`、全部文本 `read_all`、全部字节 `read_all_bytes`、最多 n 字节 `read_bytes`}。
- issue：#3974。

<a id="r-io-05-標準入出力を取っ手にしバイトの読み書きを-ioreadiowrite-に一本化する---採用"></a>

### R-io-05 标准流改为句柄，字节读写统一为 `io.read`、`io.write` — ○ 采纳

- 内容：
  ```almide
  type Stdin    // Read
  type Stdout   // Write
  type Stderr   // Write
  effect fn stdin() -> Stdin
  effect fn stdout() -> Stdout
  effect fn stderr() -> Stderr
  ```
  - 与其他句柄同样读写：`io.read(io.stdin(), n)`、`io.read_to_end(io.stdin())`、`io.write(io.stdout(), data)`（R-L0-44）。
  - 不设 R-io-01 的 `io.read_bytes`、R-io-04 的 `io.read_all_bytes`；撤去 `io.write_bytes`（→ `io.write(io.stdout(), data)`）。
  - 保留文本快捷方式 `io.read_line`、`io.read_all`、`io.print`、`io.eprint`。9 种语言均有不显式写句柄即可读一行/写字符串的快捷方式（`readLine`、`print`）。
- (a) 标准输入/输出与其他句柄遵循同一读写类型的有 6 种：Rust `io::stdin()`（`Read`）、Go `os.Stdin`（`io.Reader`）、Python `sys.stdin.buffer`、Elixir `:stdio`、OCaml `stdin`（channel）、Kotlin `System.in`（`InputStream`）。分成三类型与 Rust 相同，向 stdin 写入会类型错误。
- (d) 撤去 `io.write_bytes`（R-io-01 原计划也改其类型）并增加新 API。`almide fix` 把 `io.write_bytes(d)` 改成 `io.write(io.stdout(), d)`。
- issue：#4015。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当时，显示 mut） | 其他语言主要名称 | 判定 | 提案 |
|---|---|---|---|---|
| `io.read_line` | `effect fn read_line() -> String` | Swift `readLine` / Kotlin `readlnOrNull`(F) / OCaml `input_line` | 语义变化（`-> String?` 返回，唯一一行读取） | R-io-01 |
| `io.read_line_opt` | `effect fn read_line_opt() -> String?` | 同上 | 撤去（→ `read_line`） | R-io-01 |
| `io.read_all` | `effect fn read_all() -> String` | Rust `read_to_string` / Go `io.ReadAll` / OCaml `input_all` | 保持 | — |
| `io.read_byte` | `effect fn read_byte() -> Int` | Rust `read` / Python `read(1)` | 撤去（→ `io.read(io.stdin(), 1)`） | R-io-01・R-io-05 |
| `io.read_n_bytes` | `effect fn read_n_bytes(n: Int) -> List[Int]` | Python `buffer.read(n)` / Elixir `binread(n)` | 撤去（→ `io.read(io.stdin(), n)`） | R-io-01・R-io-05 |
| `io.print` | `effect fn print(s: String) -> Unit` | Rust `print!` / Go `fmt.Print` / Gleam `io.print` | 保持 | — |
| `io.write` | `fn write(data: Bytes) -> Unit` | Rust `write_all` / Go `Stdout.Write` / Elixir `binwrite` | 语义变化（`Write` 的泛型函数 `write(h, data)`，旧单参数调用为类型错误） | R-L0-44・R-io-05 |
| `io.write_bytes` | `fn write_bytes(data: List[Int]) -> Unit` | Python `buffer.write` / Kotlin `writeBytes` | 撤去（→ `io.write(io.stdout(), data)`） | R-io-01・R-io-02・R-io-05 |
| `io.eprint` | `effect fn eprint(s: String) -> Unit`（提案） | Rust `eprint!` (F) / Gleam `io.print_error` (F) / OCaml `prerr_string` | 增加 | R-io-03 |
| `io.stdin` | （增加）`effect fn io.stdin() -> Stdin` | Rust `io::stdin` / Go `os.Stdin` / Python `sys.stdin` | 增加 | R-io-05 |
| `io.stdout` | （增加）`effect fn io.stdout() -> Stdout` | Rust `io::stdout` / Go `os.Stdout` / Python `sys.stdout` | 增加 | R-io-05 |
| `io.stderr` | （增加）`effect fn io.stderr() -> Stderr` | Rust `io::stderr` / Go `os.Stderr` / Python `sys.stderr` | 增加 | R-io-05 |
| `io.read` | （增加）`effect fn io.read[H: Read](h: H, max: Int) -> Bytes` | Rust `Read::read` / Go `Reader.Read` / Python `read(n)` | 增加 | R-L0-44 |
| `io.read_exact` | （增加）`effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes` | Rust `read_exact` / Go `io.ReadFull` | 增加 | R-L0-44 |
| `io.read_to_end` | （增加）`effect fn io.read_to_end[H: Read](h: H) -> Bytes` | Rust `read_to_end` / Go `io.ReadAll` / Python `read()` | 增加 | R-L0-44 |
| `io.close` | （增加）`effect fn io.close[H: Close](h: H) -> Unit` | Rust（drop）/ Go `Closer.Close` / Python `close` | 增加 | R-L0-44 |

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0（已处理）

1. **无后缀名被返回失败哨兵的版本占用时**：由 R-L0-14 第 4 项确定。返回从 `String` 改 `Option[String]` 必使旧写法类型错误，所以保留无后缀名改成安全版。`io.read_line` 返回 `Option[String]`，撤去 `read_line_opt`（R-io-01），不设 `_opt` 后缀。
2. **返回 `Option` 的 effect fn 对 `!`、`??` 的处理**：由 R-L0-17（调用类型只取决于声明）确定。`io.read_line` 调用方式不依赖 body 写法，仅取决于 `effect fn read_line() -> String?`。具体统一成哪种，由 R-L0-17 issue 设计（同 L2-env R-env-03）。
