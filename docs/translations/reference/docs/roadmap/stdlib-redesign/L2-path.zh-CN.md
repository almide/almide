<!-- description: 标准库重新设计 L2/L3 — path：纯词法路径文本；保留十个函数，增加 components 与 relative_to（6/9 种语言具备的职责） -->
# L2/L3: path

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-path.zh-CN.md)

> [日文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-path.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照，不自动同步。已采纳提案、测量与跨语言比较是原文历史记录，并非独立重跑的测试或当前实现完成的主张。未执行所述命令。
> 页首“保留十个函数”及最初“无违规”与后文采纳的 SafePath 移除和新增并存。“只接受和返回 String”规则也与 List/Option/Bool 签名并存，未擅自修正。词法上“拒绝 ..”的提示不是文件系统限制、符号链接或路径遍历安全保证。

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源：Rust https://doc.rust-lang.org/std/path/struct.Path.html ；Go https://pkg.go.dev/path/filepath ；Python https://docs.python.org/3/library/pathlib.html ；TS/JS https://nodejs.org/api/path.html ；Gleam https://gleam-stdlib.hexdocs.pm/ （没有）；Elixir https://hexdocs.pm/elixir/Path.html ；OCaml https://ocaml.org/manual/latest/api/Filename.html ；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io.path/ ；Swift https://github.com/apple/swift-system （`FilePath`）

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 连接 | `join` | `join` | `Join` | `/`、`joinpath` | `join` | — | `join` | `concat` | `resolve` | `appending` |
| 父路径 | `dirname` | `parent` | `Dir` | `parent` | `dirname` | — | `dirname` | `dirname` | `parent` | `removingLastComponent` |
| 最后名称 | `basename` | `file_name` | `Base` | `name` | `basename` | — | `basename` | `basename` | `name` | `lastComponent` |
| 扩展名 | `extension` | `extension` | `Ext` | `suffix` | `extname` | — | `extname` | `extension` | `extension` | `extension` |
| 去除扩展名后的名称 | `stem` | `file_stem` | — | `stem` | — | — | `rootname`+`basename` | `remove_extension` | `nameWithoutExtension` | `stem` |
| 是否绝对路径 | `is_absolute` | `is_absolute` | `IsAbs` | `is_absolute` | `isAbsolute` | — | `type` | `is_relative` | `isAbsolute` | `isAbsolute` |
| 规范化（词法） | `normalize` | — | `Clean` | `normpath` | `normalize` | — | `expand`（也使用 cwd） | — | `normalize` | `lexicallyNormalized` |
| 拆成组成部分 | — | `components` | `SplitList`（含义不同）、`strings.Split` | `parts` | `parse` | — | `split` | — | `iterator` | `components` |
| 转为相对路径 | — | `strip_prefix` | `Rel` | `relative_to` | `relative` | — | `relative_to` | — | `relativeTo` | — |
| 防路径遍历的安全类型 | `SafePath`（`from_string`、`trusted`、`to_string`） | — | `os.Root`（1.24） | — | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### 完备性规则

- `path` 仅在字符串上进行词法计算（纯）。查看文件系统的操作（解析真实路径、存在性）归 `fs`。
- 分解／组装配对：{`join` ↔ `components`}、{`dirname`、`basename` ↔ 用 `join` 组装}、{`stem`、`extension`}。
- 失败表示（R-L0-06 第 1、4 项，不返回哨兵）：可能不存在的部分用 `Option`（`extension`）；有理由的拒绝用 `Result`（`from_string` 的 `..`）。`dirname("a")` 返回的 `""` 是“空父路径”这个值，并非“无父路径”，所以不是哨兵（与 Python 相同）。
- 顺序：`path` 的值是 `String`。`SafePath` 是无序类型，因此不设 `path.compare`（R-L0-16 仅针对有序类型）。

<a id="l2-の判定"></a>

### L2 判定

- 缺失职责：拆分组成部分（6/9）、转为相对路径（6/9）→ R-path-01。
- 重复／违反规则：无（`is_absolute` 符合 R-L0-09；`from_string` 遵循 R-L0-07 的方向）。
- 实测（probe-path）：`normalize("a/../../b/./c")` 得到 `../b/c`，`join("a", "/b")` 得到 `/b`，`extension(".bashrc")` 为 none，`from_string("a/../b")` 为 err。与文档一致。

<a id="案"></a>

## 提案

<a id="r-path-01-components-と-relative_to-を足す---採用"></a>

### R-path-01 增加 `components` 与 `relative_to` — ○ 采纳

- 内容：
  - `fn components(p: String) -> List[String]`——非空组成部分的序列；绝对路径首项为 `"/"`。用 `path.join` 折叠后，回到经过 `normalize` 的同一路径。
  - `fn relative_to(p: String, base: String) -> String?`——词法上位于 `base` 之下时，返回相对路径；否则 none。不生成 `..`。
- 写法：组成部分用 `components`（Rust、Swift），相对路径用 `relative_to`（Python、Elixir，Kotlin `relativeTo`）。两者均无多数派，因此选择既有且含义不冲突的写法。不用 `split`，因为与 Python 的 `os.path.split`（分成两部分）冲突。
- 依据的 L0 规则：L1 判定标准 1（至少五种语言）、R-L0-06（不在其下则 none）、R-L0-02（字符串路径函数归 path）。
- (a) 6/9 种语言可拆分组成部分，6/9 种可转为相对路径。
- (b) 纯计算，native 与 wasm 相同。
- (c) 没有它时，LLM 会写 `string.split(p, "/")`，为开头 `/` 或连续 `/` 生成空元素。
- (d) 新增，使用数 0。
- 检查提案：将 `components` 与 `join` 的往返、`relative_to` 与 `join` 的往返（`join(base, relative_to(p, base)!) == normalize(p)`）设为性质测试。
- issue：#3936。

<a id="r-path-02-safepathfrom_stringtrustedto_stringを撤去する---採用"></a>

### R-path-02 移除 `SafePath`（`from_string`、`trusted`、`to_string`）— ○ 采纳

- 当前：`path.from_string(s) -> Result[SafePath, String]`、`path.trusted(s) -> SafePath`、`path.to_string(p: SafePath) -> String`。其他 path 函数全部接受 `String`，没有一个标准函数接受 `SafePath`。
- 提案：移除三个函数与 `type SafePath`。将来重新设计为 `fs` 能接受的形式时，再添加安全路径类型（docs/roadmap/on-hold/secure-by-design.md）。
- 依据的 L0 规则：R-L0-06 第 4 项的思路（值没有名称承诺的性质）、R-L0-00 第 1 项（其他模块将 String 转成类型化值的可失败函数用 `parse`，只有这里用 `from_string`）、R-L0-01（`html.raw` 与 `path.trusted` 是同一职责的两种写法）。
- (a) 只有 Go 的 `os.Root`（1.24）在标准库提供安全路径类型，而且与文件系统操作一体。没有语言仅靠词法处理便让类型自称“安全”。
- (c) 实测：`path.from_string("/etc/passwd")` 与 `"a/%2e%2e/b"` 都为 `ok`。只拒绝 `..` 组成部分。LLM 若用它解决“从用户输入安全构建路径”，就会在误以为安全时放行绝对路径。
- (d) 移除 3 项。epoch 15 标记 `@deprecated`，epoch 16 移除。由于形式改变，不做 `almide fix`，而给出提示：“若要拒绝 `..`，请检查 `path.components(p) |> list.contains("..")`。”
- 完备性规则：`path` 公开函数只接受和返回 `String`。
- issue：#3971。

<a id="r-path-03-with_extension-を足す---採用"></a>

### R-path-03 增加 `with_extension` — ○ 采纳

- 当前：有读取扩展名的 `extension` 和去除扩展名的名称 `stem`，却无替换扩展名的函数。
- 提案：`fn path.with_extension(p: String, ext: String) -> String`。`ext` 为 `""` 时移除扩展名。
- 依据：L1 添加标准（至少五种语言）。
- (a) Rust `with_extension`、Python `with_suffix`、Swift `appendingPathExtension`/`deletingPathExtension`、Kotlin（`resolveSibling`）、Elixir `Path.rootname` + 连接、OCaml `remove_extension` + 连接。
- (b) 纯字符串计算。
- (d) 新增 1 项。
- issue：#3985。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `path.from_string` | `fn from_string(s: String) -> Result[SafePath, String]` | 接近 Go `os.OpenRoot` | 连同 `SafePath` 移除 | R-path-02 |
| `path.trusted` | `fn trusted(s: String) -> SafePath` | — | 连同 `SafePath` 移除 | R-path-02 |
| `path.to_string` | `fn to_string(p: SafePath) -> String` | Rust `to_str` / Python `str(p)` | 连同 `SafePath` 移除 | R-path-02 |
| `path.join` | `fn join(base: String, child: String) -> String` | Rust、Node、Elixir `join` / Go `Join` | 保持 | — |
| `path.dirname` | `fn dirname(p: String) -> String` | Node、Elixir、OCaml `dirname` / Go `Dir` / Rust `parent` | 保持 | — |
| `path.basename` | `fn basename(p: String) -> String` | Node、Elixir、OCaml `basename` / Go `Base` / Rust `file_name` | 保持 | — |
| `path.extension` | `fn extension(p: String) -> String?` | Rust、OCaml、Kotlin `extension` / Python `suffix` / Node `extname` | 保持 | — |
| `path.is_absolute` | `fn is_absolute(p: String) -> Bool` | Rust、Python `is_absolute` / Go `IsAbs` | 保持 | — |
| `path.stem` | `fn stem(p: String) -> String` | Rust `file_stem` / Python `stem` / Swift `stem` | 保持 | — |
| `path.normalize` | `fn normalize(p: String) -> String` | Go `Clean` / Python `normpath` / Node `normalize` | 保持 | — |
| `path.components` | `fn components(p: String) -> List[String]`（提案） | Rust `components` / Python `parts` / Elixir `split` | 添加 | R-path-01 |
| `path.relative_to` | `fn relative_to(p: String, base: String) -> String?`（提案） | Python、Elixir `relative_to` / Go `Rel` / Node `relative` | 添加 | R-path-01 |
| `path.with_extension` | `fn with_extension(p: String, ext: String) -> String`（提案） | Rust `with_extension` / Python `with_suffix` | 添加 | R-path-03 |
