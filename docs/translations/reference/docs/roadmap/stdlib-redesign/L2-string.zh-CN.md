描述：标准库重设计 L2/L3 — string：统一为一个 join（list.join），UTF-8 编解码全部移至 bytes，撤除 run_length_encode 和 length，新增 compare/byte_len/UTF-16
<a id="l2l3-string"></a>

# L2/L3: string

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-string.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-string.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-string.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md)

> 原文历史注记：这是带日期的重新设计调查与提案记录，不是独立复核的文档、当前实现证据或新的安全及性能保证。(F) 表示原文自身的文档核查，探针观察限于列明的开发版本。采纳提案和迁移 epoch 本身不证明完成。R-string-12 的首个码点方案被 R-string-14 改为恰好一个码点；R-string-13 拒绝谓词 count 的决定被 R-string-18 的采纳取代。旧 chars 配方与后续 to_list 改名并存。开头50个函数／48个返回副本的函数，以及结尾50 + 4 = 54行，虽与后续表格增补不一致，仍按原文保留。R-string-15 的双参数 slice/drop 等价主张与边界规则并存，但负 slice 起始位置与负 drop 数量的处理不同，不应将等价性推广至这些输入。常量时间比较明确在字节长度不同时立即返回，因此其时间主张不表示与长度无关；HMAC 验证被指向单独 API。

> 最后更新：2026-10-24
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源: Rust https://doc.rust-lang.org/std/primitive.str.html (F) ・ Go https://pkg.go.dev/strings (F) ・ Python https://docs.python.org/3/library/stdtypes.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/string.html (F) ・ Elixir https://elixir.hexdocs.pm/String.html (F) ・ OCaml https://ocaml.org/manual/latest/api/String.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/ (部分 F) ・ Swift https://developer.apple.com/documentation/swift/string （页面通过 JS 渲染，无法获取。Swift 栏无 F 标记）
> 实测：`almide 0.68.0 (dev, 437b8f127)`，探针位于 `scratchpad/sr/probe-string/`（a〜e.almd）

阅读约定（首次出现）：
- `|>`（读作：管道）= 将左侧的值插入右侧函数的第 1 个参数
- `??`（读作：空值合并）= 左侧为 none / err 时使用右侧
- `mut`（读作：mut）= 原地改写调用方 `var` 的参数
- (F) = 本次获取官方文档并核实过的单元格

`string` 目前有 50 个函数（与 `list` 一样，在所有文件中无需 import 即可使用）。全部是纯计算，不使用宿主功能。native 与 wasm 的差异由契约台账保障（C-xxx 的 string 族，包括 `string.split("")` 和 RLE 以码点为单位的契约等）。

---

<a id="l2-役割の比較"></a>

## L2：角色对比

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 长度（字符数） | `len`（码点数） | `chars().count()` | `utf8.RuneCountInString` | `len` | `length`（UTF-16 单元） | `length`（字素） (F) | `length`（字素） (F) | — | `length`（UTF-16） | `count`（字素） |
| 长度（字节数） | — | `len` (F) | `len` | `len(s.encode())` | — (F: `TextEncoder` 必需) | `byte_size` (F) | `byte_size`（Kernel）(F) | `length` (F) | `encodeToByteArray().size` | `utf8.count` |
| 判空 | `is_empty` | `is_empty` | — | — | — | `is_empty` (F) | — | `is_empty` | `isEmpty` | `isEmpty` |
| 包含子串 | `contains` | `contains` (F) | `Contains` (F) | `in` | `includes` | `contains` (F) | `contains?` (F) | `includes`（5.5 起）| `contains` | `contains` |
| 前缀、后缀匹配 | `starts_with` / `ends_with` | `starts_with` / `ends_with` | `HasPrefix` / `HasSuffix` (F) | `startswith` / `endswith` (F) | `startsWith` / `endsWith` | `starts_with` / `ends_with` (F) | `starts_with?` / `ends_with?` (F) | `starts_with` / `ends_with` (F) | `startsWith` / `endsWith` | `hasPrefix` / `hasSuffix` |
| 查找位置 | `index_of` / `last_index_of` | `find` / `rfind` | `Index` / `LastIndex` (F) | `find` / `rfind` (F) | `indexOf` / `lastIndexOf` | — | — | `index_opt` / `rindex_opt` (F) | `indexOf` / `lastIndexOf` | `firstRange(of:)` |
| 出现次数 | `count` | `matches().count()` (F) | `Count` (F) | `count` (F) | — | — | `count` (F) | — | — | — |
| 分割 | `split` / `split_once` / `lines` | `split` / `split_once` / `lines` (F) | `Split` / `Cut` / `Lines` (F) | `split` / `partition` / `splitlines` (F) | `split` (F) | `split` / `split_once` (F) | `split` / `split_at` (F) | `split_on_char` (F) | `split` / `lines` (F) | `split(separator:)` |
| 按空白分割 | — | `split_whitespace` (F) | `Fields` (F) | `split()` (F) | — (F) | — (F) | `split/1` (F) | — (F) | — | — |
| 连接 | `join`（与 `list.join` 重复） | `[..].join` | `Join` (F) | `sep.join` (F) | `Array.join` | `join` (F) | `Enum.join` | `concat` (F) | `joinToString` | `joined(separator:)` |
| 去除（空白） | `trim` / `trim_start` / `trim_end` | `trim` / `trim_start` / `trim_end` | `TrimSpace` (F) | `strip` / `lstrip` / `rstrip` (F) | `trim` / `trimStart` / `trimEnd` (F) | `trim` / `trim_start` / `trim_end` (F) | `trim` / `trim_leading` / `trim_trailing` (F) | `trim` (F) | `trim` / `trimStart` / `trimEnd` | `trimmingCharacters`（Foundation）|
| 去除前缀、后缀 | `strip_prefix` / `strip_suffix`（Option） | `strip_prefix`（Option）(F) | `CutPrefix` / `TrimPrefix` (F) | `removeprefix`（不存在则保持原样）(F) | — | `remove_prefix` (F) | `replace_prefix` (F) | — | `removePrefix` | — |
| 替换 | `replace` / `replace_first` | `replace` / `replacen` | `ReplaceAll` / `Replace(n)` (F) | `replace(count)` (F) | `replaceAll` / `replace` (F) | `replace` (F) | `replace` (F) | — | `replace` / `replaceFirst` | `replacing` |
| 切片 | `slice` / `take` / `drop` / `take_end` / `drop_end` | `[a..b]` / `get(a..b)` | `s[a:b]` | `s[a:b]` | `slice` | `slice` / `drop_start` / `drop_end` / `crop` (F) | `slice` (F) | `sub` | `substring` / `take` / `drop` | `prefix` / `dropFirst` |
| 按字节范围切片 | `byte_slice` | `get(a..b)` (F) | `s[a:b]` | — | — | — | `byte_slice` (F) | `sub` | — | — |
| 取出 1 个字符 | `get` / `first` / `last` | `chars().nth` | — | `s[i]` | `at` (F) | `first` / `last` (F) | `at` / `first` / `last` (F) | `get` | `get` / `first` / `last` | `first` / `last` |
| 字符列表 | `chars` | `chars` (F) | `[]rune(s)` | `list(s)` | `[...s]` | `to_graphemes` (F) | `graphemes` / `codepoints` (F) | — | `toList` | `Array(s)` |
| 大写、小写 | `to_upper` / `to_lower` | `to_uppercase` / `to_lowercase` (F) | `ToUpper` / `ToLower` (F) | `upper` / `lower` (F) | `toUpperCase` / `toLowerCase` | `uppercase` / `lowercase` (F) | `upcase` / `downcase` (F) | `uppercase_ascii` | `uppercase` / `lowercase` | `uppercased` / `lowercased` |
| 首字符大写 | `capitalize`（其余保持原样） | — | —（`Title` 已弃用 (F)） | `capitalize`（其余改为小写）(F) | — (F) | `capitalise`（其余改为小写）(F) | `capitalize`（其余改为小写）(F) | `capitalize_ascii`（其余保持原样）(F) | `replaceFirstChar` | `capitalized`（Foundation）|
| 填充 | `pad_start` / `pad_end` | — | — | `rjust` / `ljust`（仅 1 个字符）(F) | `padStart` / `padEnd`（可使用多个字符）(F) | `pad_start` / `pad_end`（可使用多个字符）(F) | `pad_leading` / `pad_trailing` (F) | — | `padStart` / `padEnd`（Char） | `padding`（Foundation）|
| 重复、反转 | `repeat` / `reverse` | `repeat` (F) / `chars().rev()` | `Repeat` (F) | `*` / `[::-1]` | `repeat` | `repeat` / `reverse` (F) | `duplicate` / `reverse` (F) | — | `repeat` / `reversed` | `String(repeating:)` / `reversed` |
| 字符类别 | `is_digit` / `is_alpha` / `is_alphanumeric` / `is_whitespace` / `is_upper` / `is_lower` | `char::is_*` | `unicode.Is*` | `isdigit` / `isalpha` / `isspace` … (F) | — | — | — | — | `Char.isDigit` … | `Character.isNumber` … |
| 比较（返回 Int） | — | `cmp`（Ord）(F) | `Compare` (F) | — | `localeCompare`（依赖区域设置）(F) | `compare`（Order）(F) | — | `compare` (F) | `compareTo` (F) | — |
| 码点 | `codepoint` / `from_codepoint` | `char as u32` / `char::from_u32` | `rune` | `ord` / `chr` | `codePointAt` / `fromCodePoint` (F) | `utf_codepoint` / `from_utf_codepoints` (F) | `codepoints` / `to_charlist` (F) | `Uchar` | `codePointAt` (F) | `unicodeScalars` |
| 与 UTF-8 字节序列转换 | `to_bytes` / `from_bytes`（List[Int]）和 `bytes.from_string` / `bytes.to_string`（Bytes） | `as_bytes` / `String::from_utf8` | `[]byte(s)` / `string(b)` | `encode` / `decode` (F) | `TextEncoder` / `TextDecoder` | `bit_array.from_string` / `to_string` | 字符串本身就是 binary | 字符串本身就是字节序列 | `encodeToByteArray` / `decodeToString` (F) | `utf8` / `String(decoding:as:)` |
| 与 UTF-16 转换 | — | `encode_utf16` / `String::from_utf16` (F) | `unicode/utf16` | `encode("utf-16")` (F) | 内部表示（`charCodeAt` / `fromCharCode`）(F) | — | `:unicode` | `get_utf_16le_uchar` 等 (F) | 内部表示 | `utf16` / `String(decoding:as:)` |
| 游程编码 | `run_length_encode` | — | — | — | — | — | — | — | — | — |
| 原地添加、清空 | `push` / `clear`（`mut`） | `push_str` / `clear` | `strings.Builder` | — | — | — | — | `Buffer` | `StringBuilder` | `append` / `removeAll` |

<a id="完全性の規則"></a>

### 完整性规则

1. **边界族（R-L0-12）：** `slice`、`take`、`drop`、`take_end`、`drop_end` 与 `list` 同样为 5 个函数，对相同的边界输入返回相同答案（`n` 为负：`take` 返回全部，`drop` 返回空。a.almd 实测：`take("abc",-1)="abc"`、`drop("abc",-1)=""`）。list 也给出相同答案（probe-take 实测：`list.take([1, 2, 3], -1)` 有 3 个元素，`list.drop` 为空。`take_end`、`drop_end` 亦然）。负的 `n` 是数量而非下标，因此不属于 R-L0-12 的“负下标截断为 0”的适用范围。list 与 string 的答案一致，若修改，就会在名称不变时悄然改变结果（R-L0-14 第 4 项禁止的形式），因此维持现状。新添加的 `bytes.take` 等也遵循这一答案。`bytes` 也纳入同一矩阵。`slice` 的负起始位置按 R-L0-12（负下标越界，截断为 0）截断为 0（`slice("abc", -1, 2) = "ab"`）。string 目前已如此，list 和 bytes 将与其对齐（L2-list R-list-10、L2-bytes R-bytes-12）。
2. **取值族（R-L0-06）：** 可能失败的取值（`get`、`first`、`last`、`index_of`、`last_index_of`、`strip_prefix`、`strip_suffix`、`codepoint`、`byte_slice`、`split_once`）不带后缀，返回 `Option`。目前没有缺项，也没有 `_or` 版本。
3. **原地修改（R-L0-04）：** 只有 `push` 和 `clear` 接受 `mut`，两者都使用规定的动词。其余 48 个函数返回副本。
4. **编码的归属（R-L0-07）：** 与 UTF-8 字节序列之间的双向转换都放在 `bytes` 中（`bytes.from_string` / `bytes.to_string` / `bytes.to_string_lossy` 形成与 `bytes.from_list` / `bytes.to_list` 同样形状的配对）。`string` 中只保留不经过 `Bytes` 的表示（单个码点、UTF-16 单元序列）。
5. **有意不提供的功能：** `split` 族止于 `{split, split_once, lines}`（依照 `stdlib/string.almd:24-25` 的注释，不提供 `rsplit_once` 和 `splitn`）。格式化（printf）在 L1 中判为 ×。不提供字符类型（Char）；1 个字符是长度为 1 的 String。

<a id="l2-の判定"></a>

### L2 判定

- **重复：** `string.join` 和 `list.join` 是相同计算（`stdlib/string_join.almd:50` 写着“identical”；a.almd 探针也输出相同结果）。它们有各自的 intrinsic，因此当前同义词门禁无法发现 → R-string-01。
- **方向与类型不一致：** `string.to_bytes` 返回 `List[Int]`，`bytes.from_string` 返回 `Bytes`（#2078 的原因 1：1 字节变成 8 字节）。反向也有 `string.from_bytes`（List[Int]，替换损坏的序列）、`bytes.to_string`（Result）、`bytes.to_string_lossy` 这 3 种 → R-string-02。
- **单一用途的函数：** `run_length_encode`（#3852）→ R-string-03。
- **未确定撤除日期的 deprecated：** `string.length`（#3851 A）→ R-string-04。
- **返回失败哨兵值的构造：** `from_codepoint` 对无效值（代理项或越界值）返回 `""`（实测：U+D800 返回 `""`）。违反 R-L0-06 第 4 项（可能失败的构造不返回哨兵值）→ R-string-11。
- **缺失的角色（5 种以上语言）：** 比较 `compare`（6 种语言）→ R-string-05。字节数（6 种语言有专用名称；虽有 `byte_slice`，却无法不经复制获取确定其范围所需的字节数）→ R-string-06。UTF-16（R-L1-08）→ R-string-07。
- **不添加的角色：** 按空白分割（4/9）→ R-string-08 ×。去除指定字符、`splitn`、将码点列成列表 → R-string-10 ×。
- **输入其他语言名称时的引导：** 写 `string.split_whitespace` 时，E002 目前会推荐 `string.is_whitespace`（c.almd 实测；含义完全不同）→ R-string-09。
- **与 R-L0-05 的关系：** 将 `string.trim(s)` 或 `string.to_upper(s)` 写成语句，目前不会报错（b.almd 实测：显示 `[  hi  ]`）。string 的 48 个复制函数都可能出现这一事故，尤其是 `trim`、`replace`、`to_upper`、`to_lower`，容易因其他语言的习惯而被当成原地修改（Rust 的 `String` 或 Swift 的 `mutating` 函数，不包括 Python、JS 的不可变字符串）。R-L0-05 的 E042 扩展将拦截此类情况。本页不另立新提案。

---

<a id="案"></a>

## 提案

<a id="r-string-01-stringjoin-を撤去し-listjoin-に一本化する---採用"></a>

### R-string-01 撤除 `string.join`，统一为 `list.join` — ○ 采纳

- 内容：为 `string.join(list, sep)` 添加 `@deprecated(since=15, use="list.join")`，在 epoch 16 撤除。`xs |> list.join(", ")` 成为唯一写法。
- 依据的 L0 规则：R-L0-01（同一角色只有一种名称）、R-L0-02（放在主语 = 第 1 个参数的类型 `List[String]` 所属模块中）。
- (a) 多数语言放在主语类型一侧（Rust 的 slice `join`、JS 的 `Array.join`、Kotlin 的 `joinToString`、Swift 的 `joined`、Elixir 的 `Enum.join`）。也有放在分隔符一侧（Python `sep.join`）或字符串模块（Go、Gleam、OCaml `concat`）的，但 Almide 的归属由 R-L0-02 决定。
- (b) native 与 wasm 相同（都是纯计算；`list.join` 已纳入两条执行路径的契约）。
- (c) `xs |> list.join(",")` 的形式由管道开头的类型确定模块。保留 `string.join` 就仍然存在“两种都能写”的选择。
- (d) 撤除。`removal_epoch = 16`。由于是一对一改名，`almide fix` 将 `string.join(xs, sep)` → `list.join(xs, sep)`。使用数（`\bstring\.join\b`，包含定义和生成文档的上限）：spec 104 / tests 26 / docs 13 / stdlib 5（`path.almd:45,120` 等）/ research 28。`list.join` 为 spec 390 / tests 91 / docs 30 / stdlib 24。
- 门禁提案：在 R-L0-01 同义词门禁的规则 6（相同函数体的不同 intrinsic）中加入跨模块比较即可检出（`almide_rt_string_join` 和 `almide_rt_list_join` 都是 `xs.join(sep)`）。
- issue: #3947

<a id="r-string-02-utf-8-の変換は両方向とも-bytes-に置きstringto_bytes-と-stringfrom_bytes-を撤去する---採用"></a>

### R-string-02 将 UTF-8 双向转换都放在 `bytes` 中，撤除 `string.to_bytes` 和 `string.from_bytes` — ○ 采纳

- 内容：
  - 为 `string.to_bytes(s) -> List[Int]` 添加 `@deprecated(since=15, use="bytes.from_string")`。需要 List[Int] 时提示 `bytes.from_string(s) |> bytes.to_list`。
  - 为 `string.from_bytes(xs: List[Int]) -> String`（损坏序列替换为 U+FFFD）添加 `@deprecated(since=15, use="bytes.to_string_lossy")`。提示 `bytes.from_list(xs) |> bytes.to_string_lossy`。
  - 保留的规范名称：`bytes.from_string`（String → Bytes）、`bytes.to_string`（Bytes → Result[String, String]）、`bytes.to_string_lossy`（Bytes → String）。
- 依据的 L0 规则：R-L0-07 第 2 项（容器构造使用目标类型的 `from_<元>`（元 指来源））和第 4 项（双向配对的类型也要一致）、R-L0-01。R-L0-14 第 4 项也允许保留名称、将返回类型改为 `Bytes`（使旧写法产生类型错误的修改）。仍然连名称一起撤除，是因为 UTF-8 双向转换的归属已定为 `bytes`（L2-bytes R-bytes-10）；若将 `string.to_bytes` 保留为 `Bytes` 版本，就会与 `bytes.from_string` 成为同一角色的两个名称（R-L0-01）。
- (a) Rust（`as_bytes` / `String::from_utf8`）与 Kotlin（`encodeToByteArray` / `decodeToString`）放在源类型一侧；Gleam 的 `bit_array.from_string` / `bit_array.to_string` 采用“字节序列一侧拥有双向转换”的形式。Almide 的 `bytes` 已有 `from_list` / `to_list` 配对，因此将 `from_string` / `to_string` 也放在旁边最为一致。
- (b) native：`bytes.from_string` 使用 `Vec<u8>`（消除 #2078 的 1 字节 = 8 字节问题）。wasm：两者都是纯计算，无需宿主。
- (c) 目前“想要字节序列”有两个答案（List[Int] 和 Bytes），性能相差 8 倍。LLM 会选择名称较短的 `string.to_bytes`。
- (d) 撤除 2 个。`removal_epoch = 16`。使用数：`string.to_bytes` spec 27 / tests 3 / docs 11 / stdlib 3（全是注释）/ research 6；`string.from_bytes` spec 41 / tests 2 / docs 11 / stdlib 3（`http_url_decode.almd:41` 中 1 处调用及注释）/ research 2。由于改写会改变形状（添加 `|> bytes.to_list`），`almide fix` 根据“该位置是否接收 List[Int]”选择两种改写形式。
- 门禁提案：在 R-L0-07 门禁（对照 `to_<T>` 与 `from_<T>`，确保同一转换不出现在两处）中登记“String ⇄ UTF-8 字节序列”这一组。也检查 `string` 的公开函数中不存在以 `List[Int]` 为返回类型或参数类型、且含义为 UTF-8 的函数。
- issue：#2078（待办事项 4 的答案）。撤除本身是新增事项。

<a id="r-string-03-stringrun_length_encode-を撤去する---採用"></a>

### R-string-03 撤除 `string.run_length_encode` — ○ 采纳

- 内容：`@deprecated(since=15, use="string.chars + list.fold")`，在 `removal_epoch = 16` 撤除。替代方案不是单个函数，因此 `almide fix` 不改写，而是在 `removed_spellings.rs` 中放置两行 fold（`string.chars(s) |> list.fold([], ...)`）作为提示。
- 依据的 L0 规则：R-L0-00 的前提（9 种语言均未在标准库提供）、L1“添加”标准的反面（0/9）。
- (a) 0/9。这是练习题的答案，不是字符串基本操作。也没有对应的解码函数。
- (b) 撤除后，contract（`string.split("")` 和 RLE 的码点单位）中的 RLE 行将移除。fixture `spec/wasm_cross/string_split_rle_codepoint.almd` 仅保留 split 的行。
- (c) 公开接口少一个函数，LLM 候选就少一个“含义狭窄的名称”。
- (d) 撤除。使用数：spec 8 / tests 2 / docs 11 / stdlib 1（定义）/ research 0。
- 门禁提案：无（一次性撤除）。
- issue：#3852（第 2 节表格的第 1 行）

<a id="r-string-04-stringlength-の撤去を-epoch-16-に載せる---採用"></a>

### R-string-04 将 `string.length` 的撤除安排在 epoch 16 — ○ 采纳

- 内容：已有 `@deprecated(since=3, use="string.len")`（E052 警告和 `almide fix` 已生效，d.almd 实测）。在 `proofs/dialect-epochs.toml` 中放置 `[[deprecation]]`，并设为 `removal_epoch = 16`。保留 CHEATSHEET:1077 的 `→ WRONG` 行。
- 依据的 L0 规则：R-L0-11、R-L0-14
- (d) 撤除。since 保持当前的 3（窗口已经开启，不重新设为 since=15 的窗口），`removal_epoch = 16`。`almide fix` 将 `string.length(s)` → `string.len(s)` 进行一对一改写。撤除后的提示：“`string.len(s)`”。使用数：spec 6 / tests 7 / docs 8 / stdlib 1（定义）。
- issue: #3851 A

<a id="r-string-05-stringcomparea-b---int-を足す---採用"></a>

### R-string-05 添加 `string.compare(a, b) -> Int` — ○ 采纳

- 内容：返回负、0、正的 Int。顺序与 `<` 一致（UTF-8 字节顺序 = 码点顺序；e.almd 实测：`"B" < "a"` 为 true）。可直接传给接受比较函数的函数，如 `list.sort_with(xs, string.compare)`。
- 依据的 L0 规则：R-L0-16（有序类型提供 `compare(a, b) -> Int`）、R-L0-10（`_with` 接受比较函数；L2-list 将添加 `list.sort_with`，因此需要比较函数组件）。
- (a) 6 种语言提供（Rust `cmp`、Go `Compare` (F)、Gleam `compare` (F)、OCaml `compare` (F)、Kotlin `compareTo` (F)、JS `localeCompare` (F，依赖区域设置，含义不同)）。`compare` 这一名称有 3 种语言使用，最多；Almide 的 `bytes.cmp` 属于少数。多数返回 Int（Go、OCaml、Kotlin、JS），Almide 的 `bytes.cmp` 也返回 Int。
- (b) 纯计算。不读取区域设置，因此 native 与 wasm 相同。
- (c) 降序或复合键排序时，LLM 会编写比较函数。若每次都要求写 `(a, b) => if a < b then -1 else if a > b then 1 else 0`，就会出现符号混淆。
- (d) 添加。
- 门禁提案：使用随机字符串对，在 native、wasm、interp 三端验证 `string.compare(a, b) < 0` ⇔ `a < b`。
- issue: #3902

<a id="r-string-06-stringbyte_lens---int-を足す---採用"></a>

### R-string-06 添加 `string.byte_len(s) -> Int` — ○ 采纳

- 内容：不经复制返回 UTF-8 字节数。
- 依据的 L0 规则：R-L0-11（长度的名称为 `len`）、R-L0-02（主语为 String）。`byte_` 与现有 `byte_slice` 使用相同前缀。
- (a) 6 种语言有专用名称（Rust `len` (F)、Go `len`、Gleam `byte_size` (F)、Elixir `byte_size` (F)、OCaml `length` (F)、Swift `utf8.count`）。
- (b) native 使用 `s.len()`，wasm 只需直接读取字符串的长度字段。
- (c) 确定 `string.byte_slice(s, a, b)` 的范围需要字节数，但目前只能通过 `bytes.from_string(s) |> bytes.len` 复制整个字符串。
- (d) 添加。
- 门禁提案：将 `string.byte_len(s) == bytes.len(bytes.from_string(s))` 作为性质测试。
- issue: #3945

<a id="r-string-07-utf-16-との変換を足すlatin-1-は足さない---採用"></a>

### R-string-07 添加 UTF-16 转换（不添加 Latin-1）— ○ 采纳

- 内容：
  - `string.to_utf16(s: String) -> List[Int]`（UTF-16 编码单元 0..65535，无 BOM）
  - `string.from_utf16(units: List[Int]) -> Result[String, String]`（未配对的代理项或越界值返回 `err("invalid UTF-16 at index N")`）
  - 不添加 Latin-1。
- 依据的 L0 规则：R-L1-08（作为 string 函数添加）、R-L0-07 第 4 项（配对类型一致：两者均为 `List[Int]`）、R-L0-06 第 1 项（有原因的失败使用 Result）。
- (a) 8 种语言提供 UTF-16（L1）。Latin-1 有 Python、JS、Kotlin、Swift、Elixir 这 5 种语言提供，但都作为“传入字符编码名称的通用编码器”的一个选项，没有专用函数。没有理由在 Almide 中引入通用字符编码表，因此判为 ×。
- (b) 纯计算，native 与 wasm 相同。UTF-16 单元序列不是 `Bytes`（单位为 16 位），因此放在 `string` 而不是 `bytes` 中。
- (c) 处理 JS、Windows API、JSON 的 `\uXXXX` 时，LLM 会以 `encode_utf16` / `charCodeAt` 的语义为前提编写代码。
- (d) 添加。现有 `from_codepoint` 也将在 R-string-11 中统一为用 `Option` 表示失败的形式。
- 门禁提案：为 `string.from_utf16(string.to_utf16(s)) == ok(s)` 往返、代理对（如 U+1F980）、孤立代理项的 err 设置 fixture，确保 native 与 wasm 一致。
- issue: #3948

<a id="r-string-08-stringsplit_whitespace-を足す---不採用"></a>

### R-string-08 添加 `string.split_whitespace` — × 不采纳

- 理由：只有 4/9 提供专用函数（Rust (F)、Go `Fields` (F)、Python `split()` (F)、Elixir `split/1` (F)），未达到 L1 的“5 种以上语言”。已确认 Gleam、JS、OCaml 不提供 (F)。可写成 `regex.split("\\s+", string.trim(s))`（参数顺序遵循 regex 的判定）。
- 改用 R-string-09 的提示，将写出此名称的 LLM 引导至正确写法。
- 依据的 L0 规则：R-L0-00、L1 判定标准

<a id="r-string-09-他言語の綴りへのヒントを意味の合う書き方に直す---採用"></a>

### R-string-09 将其他语言名称的提示修正为含义相符的写法 — ○ 采纳

- 内容：在 E002 的“Did you mean”表（`crates/almide-frontend/src/stdlib.rs:156` 的 `ALIASES`）中添加 string 行。
  - `split_whitespace` / `fields` / `words` → `regex.split("\\s+", string.trim(s))`
  - `to_utf8` / `as_bytes` / `encode` → `bytes.from_string`、`decode` / `from_utf8` → `bytes.to_string`
  - `cmp` / `compare_to` / `locale_compare` → `string.compare`
  - `byte_size` / `byte_length` → `string.byte_len`
  - `pad_left` / `pad_right` / `rjust` / `ljust` → `string.pad_start` / `string.pad_end`
  - `strip` / `lstrip` / `rstrip` → `string.trim` / `trim_start` / `trim_end`
  - `remove_prefix` / `trim_prefix` → `string.strip_prefix`
- 依据的 L0 规则：R-L0-01（只有一种名称；不创建别名，通过错误引导）。
- (a) 表中名称来自上方 L2 表格中的各语言名称。
- (b) 仅修改检查器。
- (c) 目前写 `string.split_whitespace` 会被推荐 `string.is_whitespace`（c.almd 实测）。按名称相似度选择的候选，有时含义相反。
- (d) 添加（仅诊断，不改变语言表面接口）。
- 门禁提案：添加测试，检查表中所有行均满足“键未定义，目标是有效的公开函数或正确表达式”（表中仍留有指向当前有效函数的键。此测试与 L2-map 的 R-map-06 相同）。
- issue: #3905

<a id="r-string-10-指定文字の-trimsplitnコードポイントのリスト化を足す---不採用"></a>

### R-string-10 添加指定字符的 trim、`splitn`、码点列表化 — × 不采纳

- 指定字符的 trim（Python `strip(chars)`、Rust `trim_matches`、Go `Trim(s, cutset)`、Kotlin `trim(chars)`、Elixir `trim(s, to_trim)`、Swift `trimmingCharacters`）：6 种语言提供，但语义分为“字符集合”（Python、Go）、“重复子串”（Elixir）、“模式”（Rust）。无论与哪种对齐，按其他语言习惯编写的 LLM 都会出错。可用 `strip_prefix` / `strip_suffix` 和 `regex.replace` 表达。
- `splitn`：维持 `stdlib/string.almd:24-25` 的判断（拆成两部分的用途由 `split_once` 负责）。
- 码点列表化（Gleam `to_utf_codepoints`、Elixir `to_charlist` 等）：用 `string.chars(s) |> list.filter_map(string.codepoint)` 一行即可表达，因此不满足 L1 标准第 3 项（现有函数无法涵盖）。
- 依据的 L0 规则：R-L0-00、L1 判定标准

---

<a id="r-string-11-stringfrom_codepoint-を-optionstring-を返す形にする---採用"></a>

### R-string-11 使 `string.from_codepoint` 返回 `Option[String]` — ○ 采纳

- 内容：将 `string.from_codepoint(n: Int) -> String` 改为 `string.from_codepoint(n: Int) -> Option[String]`。无效值（代理项 U+D800〜U+DFFF、负数、大于 U+10FFFF 的值）返回 none。目前返回 `""`。名称不变。
- 依据的 L0 规则：R-L0-06 第 4 项（可能失败的构造不返回哨兵值；没有原因则用 `Option`）、R-L0-14 第 4 项（若修改一定使旧写法产生类型错误，附带提示和 `almide fix` 即可保持名称不变）。
- (a) Rust 的 `char::from_u32` 返回 `Option`，Gleam 的 `utf_codepoint` 返回 `Result`，Swift 的 `Unicode.Scalar(_:)` 是可能失败的初始化器（nil）。Python 的 `chr` 和 JS 的 `fromCodePoint` 抛出异常。没有语言返回空字符串。
- (b) 纯计算。native 与 wasm 使用相同边界判断。
- (c) 目前无效值变成 `""`，直接拼接进字符串，出错也无法察觉。若用 `Option`，必须写 `??` 或 `?` 才能取出值。
- (d) 语义变更（返回类型 `String` → `Option[String]`）。旧写法在将结果用作 `String` 的位置一定产生类型错误，不会悄然得到不同结果（R-L0-14 第 4 项）。不加 `@deprecated`，列入 epoch 15 的 `breaks`。`almide fix` 将 `string.from_codepoint(n)` → `string.from_codepoint(n) ?? ""`（保持当前结果），并提示“最好用 `?` 传递失败”。使用数将在 spec、tests、docs、stdlib 中统计并写入 PR。
- 门禁提案：R-L0-G 的哨兵检查（列出返回类型为 `String`、`Int` 且名称以 `from_`、`parse`、`decode` 开头的公开函数，检查其对边界输入不返回哨兵值；与 L2-bytes R-bytes-09 相同的测试）。在 native 和 wasm 中设置 `from_codepoint(0xD800) == none` 与 `from_codepoint(0x1F980) == some("🦀")` 的 fixture。
- issue: #3946

<a id="r-string-15-stringslice-の終わりの既定値を外し2-引数の形は-drop-に一本化する---採用"></a>

### R-string-15 移除 `string.slice` 的结束位置默认值，将 2 参数形式统一为 `drop` — ○ 采纳

- 现状：`fn string.slice(s: String, start: Int, end: Int = 末尾) -> String`（末尾 指末端） 是标准函数中唯一带默认参数的函数。`string.slice(s, n)` 与 `string.drop(s, n)` 返回相同值。`list.slice` 和 `bytes.slice` 只接受 3 个参数，以 2 个参数调用会产生 E004。
- 提案：移除 `end` 的默认值，改为 `fn string.slice(s: String, start: Int, end: Int) -> String`。2 参数调用产生 E004，并提示 `string.drop(s, n)`。不为 list 和 bytes 添加默认值。
- 依据的 L0 规则：R-L0-01（2 参数 `slice` 是 `drop` 的别名）、R-L0-12（list、string、bytes 的边界矩阵形状一致）。
- (a) JS、Python、Kotlin（`substring(a)`）提供可省略结束位置的 slice，但 Almide 已有 `drop`。若为全部 3 种类型添加默认值，就会产生 3 组 `slice(n)` 与 `drop(n)` 的同义词。
- (b) 仅修改检查器和声明。
- (c) 实测：`string.slice("abcdef", 2)` 返回 `cdef`，而 `list.slice([1,2,3], 1)` 产生 E004。在 string 学到的 2 参数形式，套用到 list 和 bytes 就会失败。
- (d) 语义变更（缩小接受的形式）。旧的 2 参数调用产生 E004，而非悄然改变结果，因此满足 R-L0-14.4。列入 epoch 15 的 `breaks`，`almide fix` 将 `string.slice(s, n)` 改写为 `string.drop(s, n)`。使用数（2 参数）：spec 6 / docs 1 / stdlib 2 / research 4 / examples 1。
- 完整性规则：拥有 `slice`、`take`、`drop`、`take_end`、`drop_end` 的类型（list、string、bytes），同名函数的参数数量一致。标准公开函数不设置默认参数。
- 门禁提案：在 R-L0-12 的边界矩阵测试中添加“3 种类型的同名函数参数数量一致”，并检查 interface JSON 中带默认值的参数数为 0。
- issue: #3969

<a id="r-string-12-stringcodepoint-を-stringto_codepoint-に改名する---採用"></a>

### R-string-12 将 `string.codepoint` 改名为 `string.to_codepoint` — ○ 采纳

- 现状：反向是 `string.from_codepoint(n) -> String?`。往返配对为 `codepoint` 与 `from_codepoint`，写 `to_codepoint` 会产生 E002。
- 提案：`fn string.to_codepoint(s: String) -> Int?`（首个字符的码点，空字符串返回 none）。与 `from_codepoint` 构成 `to_`/`from_` 配对（与 `float.to_bits`/`from_bits` 相同）。
- 依据的 L0 规则：R-L0-25（转换动词表：往返配对用 `to_<X>`/`from_<X>`）。
- (a) JS `codePointAt`/`fromCodePoint`、Python `ord`/`chr`、Rust `char as u32`/`char::from_u32` 的名称分散，没有多数派。按 Almide 内部的配对决定。
- (d) 改名 1 个。`@deprecated(since=15, use="string.to_codepoint")`、`removal_epoch = 16`，由 `almide fix` 改写。
- issue: #3979

<a id="r-string-13-stringcount-を述語で数える形にする---不採用"></a>

### R-string-13 使 `string.count` 按谓词计数 — × 不采纳

- 理由：字符串“计数”的多数含义是子串出现次数（Rust `matches().count()`、Go `strings.Count`、Python `str.count`、Elixir `String.count`……）。string 不属于 D3 族（list、map、set、deque、heap、bytes 的高阶函数）的对象。若要按谓词计数，写 `string.chars(s) |> list.count(f)`。
- issue：无

<a id="r-string-14-to_codepoint-は-1-コードポイントちょうどのときだけ-some---採用r-string-12-の意味を決める"></a>

### R-string-14 `to_codepoint` 仅在恰好 1 个码点时返回 some — ○ 采纳（确定 R-string-12 的语义）

- 现状：R-string-12 规定为“首个字符的码点；为空则 none”。`to_codepoint("ab")` 返回 97，悄然丢弃第 2 个字符。反向 `from_codepoint(n) -> String?` 构造 1 个字符，因此不构成往返配对。
- 提案：仅对恰好 1 个码点的字符串返回 some，其他情况（空、2 个以上）返回 none。要读取首个字符，先用 `string.get(s, 0)` 再转换。
- (a) Python `ord("ab")` 抛出异常，Rust 的 `char` 是单字符类型。JS `codePointAt(0)` 返回首个字符，但它是接受位置参数的另一种函数。
- (d) 语义变更（与改名同在 epoch 15 引入，因此从一开始就确定为新名称的语义）。
- issue: #3991

<a id="r-string-16-string-に-sequencegrowable-の要件をそろえる---採用"></a>

### R-string-16 为 `string` 补齐 `Sequence`、`Growable` 的要求 — ○ 采纳

- 现状：`string` 属于 `Text`（包含 `Ordered`、`Sequence`、`Container`）和 `Growable`，但没有 `filter`、`fold`、`pop`、`insert`、`remove_at`。
- 提案：提供这 5 个函数。元素为 1 字符（1 码点）的 `String`，位置以码点为单位（与 R-string 的约定相同）。
  - `filter(s, f: (String) -> Bool) -> String`、`fold(s, init: B, f: (B, String) -> B) -> B`
  - `pop(mut s) -> String?`（取出末尾 1 字符）、`insert(s, i: Int, sub: String) -> String`、`remove_at(s, i: Int) -> String`（与 `list` 一样，分别超出 `0..=len`、`0..len` 时 abort）
- (a) Swift 的 `String` 是 `RangeReplaceableCollection`，提供 `filter`（返回 `String`）、`reduce`、`popLast`、`insert(_:at:)`、`remove(at:)`。Kotlin 也为 `String` 提供 `filter`、`fold`。
- `new`、`from_list`、`to_list` 已由 R-string-17 决定。
- (d) 添加 5 个。
- issue: #4000

<a id="r-string-17-stringchars-を-to_list-にしnewfrom_list-を置く---採用r-string-16-の保留を解く"></a>

### R-string-17 将 `string.chars` 改为 `to_list`，并提供 `new`、`from_list` — ○ 采纳（解决 R-string-16 的待定项）

- 内容：将 `string.chars(s)` 改名为 `string.to_list(s)`，并提供 `string.new() -> String`（`""`）和 `string.from_list(xs: List[String]) -> String`（拼接）。`string` 具备 `Container` 的全部要求。
- 理由：在 protocol 层次中，`string` 是 `Text`（包含 `Ordered`、`Sequence`、`Container`）。避免让 `to_list` 与 `chars` 成为同一角色的两种名称。Swift 使用 `Array(s)`，Kotlin 使用 `toList()`。对习惯 Rust `chars` 的写法，E002 提示 `string.to_list`。
- (d) 改名和添加。在 epoch 15 为 `chars` 标注 `@deprecated(since=15, use="to_list")`，在 epoch 16 撤除。
- issue: #4004

<a id="r-string-18-部分文字列を数える-stringcount-を-count_matches-にしcount-は-container-の述語の形にする---採用"></a>

### R-string-18 将统计子串的 `string.count` 改为 `count_matches`，`count` 改为 `Container` 的谓词形式 — ○ 采纳

- 现状：`string.count(s, sub)` 返回子串出现次数。其他 6 个容器的 `count(xs, f)` 是符合谓词的元素数量。同名却不同义。第一轮以“参数类型不同，混淆会产生类型错误”为由维持现状，但根据 R-L0-36，仅有类型错误不足以构成保留理由。在 protocol 层次中，`string` 也是 `Container`，因此 `count` 不能同时有两种含义。
- 提案：将统计子串的函数改名为 `count_matches(s, needle) -> Int`，作为 `Text` 的要求（`bytes` 也提供）。`string.count(s, f: (String) -> Bool) -> Int` 按 `Container` 的谓词形式提供。
- (a) 按谓词计数的 `count` 存在于 Kotlin、Swift（`count(where:)`）、Rust（`filter().count()`）、Elixir、Gleam 这 5 种语言；以 `count` 统计子串的有 Go、Python 这 2 种语言。
- (d) 改名和添加。epoch 15 为 `string.count(s, sub)`（传入字符串的形式）标注 `@deprecated(use="count_matches")`，epoch 16 仅保留谓词形式。`almide fix` 将传入字符串的调用改写为 `count_matches`。
- issue: #4012

<a id="r-string-19-定数時間の比較-stringis_equal_constant_time-を置く---採用"></a>

### R-string-19 提供常量时间比较 `string.is_equal_constant_time` — ○ 采纳

- 现状：webhook 的签名头和令牌以 `String` 到达。用 `==` 比较时在首个不同字节处返回，因此可从耗时猜测值。
- 提案：`string.is_equal_constant_time(a: String, b: String) -> Bool`。比较 UTF-8 字节。字节长度不同则立即返回 false。与 `bytes.is_equal_constant_time`（R-bytes-29）机制相同，是 {Bytes, String} 类型矩阵中的单元格。HMAC 验证使用 `hash.is_valid_hmac_sha256`（R-hash-06）。
- 依据的 L0 规则：R-L0-09、R-L0-30。
- (a) Python `secrets.compare_digest`（也接受 str）、Erlang `crypto:hash_equals`。
- protocol（R-L0-37）：`String` 与 `Bytes` 拥有相同角色，因此作为 `Text` 的要求仅声明一次（[protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) 的 `Text` 行；模块函数就是要求本身）。
- 名称（R-L0-00）：没有多数派（Go `ConstantTimeCompare`、`hmac.Equal`，Python `compare_digest`，Node `timingSafeEqual`，Erlang `hash_equals`，Rust `subtle` 的 `ct_eq`，Java `isEqual`）。`compare` 表示 `Ord` 的三值（R-L0-16），因此不用；改用 Bool 的 `is_`（R-L0-09），直接写明“相等”和“常量时间”。
- (d) 添加。契约 C-423。issue：#4106

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要名称 | 判定 | 提案 |
|---|---|---|---|---|
| `trim` | `(s: String) -> String` | Rust/JS/Gleam/Kotlin `trim` / Python `strip` / Go `TrimSpace` | 维持现状 | — |
| `split` | `(s: String, sep: String) -> List[String]` | 9/9 `split` 系列（Go `Split`）。`split("", ",")` 为 `[""]`（与 Rust 相同） | 维持现状 | — |
| `split_once` | `(s: String, sep: String) -> Option[(String, String)]` | Rust/Gleam `split_once` / Go `Cut` / Python `partition` | 维持现状 | — |
| `join` | `(list: List[String], sep: String) -> String` | Go/Gleam `Join` / Python `sep.join` / Rust・JS 在序列一侧 | 撤除（→ `list.join`） | R-string-01 |
| `len` | `(s: String) -> Int` | Python `len` / Gleam・Elixir `length`（字素）/ Swift `count` | 维持现状 | — |
| `length` | `(s: String) -> Int`（已弃用） | — | 撤除（→ `string.len`） | R-string-04 |
| `contains` | `(s: String, sub: String) -> Bool` | Rust/Gleam/Kotlin/Swift `contains` / JS `includes` | 维持现状 | — |
| `starts_with` | `(s: String, prefix: String) -> Bool` | Rust/Gleam/OCaml `starts_with` / Go `HasPrefix` | 维持现状 | — |
| `ends_with` | `(s: String, suffix: String) -> Bool` | Rust/Gleam/OCaml `ends_with` / Go `HasSuffix` | 维持现状 | — |
| `slice` | `(s: String, start: Int, end: Int) -> String` | Gleam/Elixir/JS `slice` / Python `s[a:b]` | 语义变更（`end` 的默认值移除；2 参数用 `drop`） | R-string-15 |
| `byte_slice` | `(s: String, start: Int, end: Int) -> Option[String]` | Elixir `byte_slice` / Rust `get(a..b)` | 维持现状 | — |
| `pad_start` | `(s: String, n: Int, ch: String) -> String` | JS/Gleam/Kotlin `padStart` / Python `rjust`。`ch` 仅使用最前面的 1 个字符（实测：`pad_start("7",5,"ab")="aaaa7"`） | 维持现状 | — |
| `to_bytes` | `(s: String) -> List[Int]` | Rust `as_bytes` / Kotlin `encodeToByteArray` / Gleam `bit_array.from_string` | 撤除（→ `bytes.from_string`） | R-string-02 |
| `capitalize` | `(s: String) -> String` | Python/Elixir/Gleam 将其余字符转为小写，OCaml `capitalize_ascii` 保持其余不变 (F)。无多数派，故维持现状 | 维持现状 | — |
| `to_upper` | `(s: String) -> String` | Rust `to_uppercase` / Go `ToUpper` / Python `upper` / Gleam `uppercase` | 维持现状 | — |
| `to_lower` | `(s: String) -> String` | Rust `to_lowercase` / Go `ToLower` / Python `lower` / Gleam `lowercase` | 维持现状 | — |
| `replace` | `(s: String, from: String, to: String) -> String` | Rust/Python/Gleam `replace` / JS・Go `replaceAll`/`ReplaceAll` | 维持现状 | — |
| `get` | `(s: String, i: Int) -> Option[String]` | JS `at` / Elixir `at` / Rust `chars().nth` | 维持现状 | — |
| `lines` | `(s: String) -> List[String]` | Rust/Kotlin `lines` / Python `splitlines` / Go `Lines` | 维持现状 | — |
| `chars` | `(s: String) -> List[String]` | Rust `chars` / Gleam `to_graphemes` / Elixir `graphemes` | 改名 → `to_list` | R-string-17 |
| `index_of` | `(s: String, needle: String) -> Option[Int]` | Rust `find` / Go `Index` / JS `indexOf` / OCaml `index_opt` | 维持现状 | — |
| `repeat` | `(s: String, n: Int) -> String` | Rust/JS/Gleam `repeat` / Go `Repeat` / Elixir `duplicate` | 维持现状 | — |
| `from_bytes` | `(bytes: List[Int]) -> String` | Rust `String::from_utf8_lossy` / Kotlin `decodeToString` | 撤除（→ `bytes.from_list(xs) \|> bytes.to_string_lossy`） | R-string-02 |
| `is_digit` | `(s: String) -> Bool` | Python `isdigit` / Rust `is_ascii_digit`。空时为 false | 维持现状 | — |
| `is_alpha` | `(s: String) -> Bool` | Python `isalpha` / Rust `is_alphabetic`。空时为 false | 维持现状 | — |
| `is_alphanumeric` | `(s: String) -> Bool` | Python `isalnum` / Rust `is_alphanumeric`。空时为 false | 维持现状 | — |
| `is_whitespace` | `(s: String) -> Bool` | Python `isspace`（空时为 false）/ Rust `is_whitespace`。Almide 为空时返回 true（与文档一致，可用于判断空行） | 维持现状 | — |
| `is_upper` | `(s: String) -> Bool` | Python `isupper`（忽略非字母字符的语义也相同） | 维持现状 | — |
| `is_lower` | `(s: String) -> Bool` | Python `islower` | 维持现状 | — |
| `codepoint` | `(s: String) -> Option[Int]` | Python `ord` / JS `codePointAt` / Gleam `utf_codepoint_to_int` | 改名 → `to_codepoint`（不恰好为 1 个码点则返回 none） | R-string-12、R-string-14 |
| `from_codepoint` | `(n: Int) -> String` | Python `chr` / JS `fromCodePoint` / Rust `char::from_u32`（Option）。无效值返回 `""`（实测：U+D800 返回 `""`） | 语义变更（返回 `Option[String]`） | R-string-11 |
| `pad_end` | `(s: String, n: Int, ch: String) -> String` | JS/Gleam/Kotlin `padEnd` / Python `ljust`。`ch` 为空时使用空格 | 维持现状 | — |
| `trim_start` | `(s: String) -> String` | Rust/JS/Gleam `trim_start` / Python `lstrip` | 维持现状 | — |
| `trim_end` | `(s: String) -> String` | Rust/JS/Gleam `trim_end` / Python `rstrip` | 维持现状 | — |
| `count` | `(s: String, sub: String) -> Int` | Python `count` / Go `Count`（sub 为空时为 len+1，Almide 相同 (F)）。`list.count`（谓词）动词相同，但参数类型不同，混淆会产生类型错误 | 改名 → `count_matches` | R-string-18 |
| `is_empty` | `(s: String) -> Bool` | Rust/Gleam/OCaml `is_empty` / Kotlin `isEmpty` | 维持现状 | — |
| `reverse` | `(s: String) -> String` | Gleam/Elixir `reverse`（字素单位）/ Rust `chars().rev()`。Almide 以码点为单位 | 维持现状 | — |
| `strip_prefix` | `(s: String, prefix: String) -> Option[String]` | Rust `strip_prefix`（Option）/ Python `removeprefix` / Gleam `remove_prefix` | 维持现状 | — |
| `strip_suffix` | `(s: String, suffix: String) -> Option[String]` | Rust `strip_suffix` / Python `removesuffix` | 维持现状 | — |
| `replace_first` | `(s: String, from: String, to: String) -> String` | Kotlin `replaceFirst` / Rust `replacen(..,1)` / JS `replace` | 维持现状 | — |
| `last_index_of` | `(s: String, needle: String) -> Option[Int]` | Rust `rfind` / Go `LastIndex` / JS `lastIndexOf` | 维持现状 | — |
| `first` | `(s: String) -> Option[String]` | Gleam/Elixir/Kotlin `first` | 维持现状 | — |
| `last` | `(s: String) -> Option[String]` | Gleam/Elixir/Kotlin `last` | 维持现状 | — |
| `take` | `(s: String, n: Int) -> String` | Kotlin `take` / Swift `prefix` | 维持现状 | — |
| `take_end` | `(s: String, n: Int) -> String` | Kotlin `takeLast` / Swift `suffix` | 维持现状 | — |
| `drop` | `(s: String, n: Int) -> String` | Kotlin `drop` / Gleam `drop_start` | 维持现状 | — |
| `drop_end` | `(s: String, n: Int) -> String` | Kotlin `dropLast` / Gleam `drop_end` | 维持现状 | — |
| `run_length_encode` | `(s: String) -> List[(String, Int)]` | 0/9 | 撤除（→ `string.chars` 和 `list.fold`） | R-string-03 |
| `push` | `(mut s: String, suffix: String) -> Unit` | Rust `push_str` / Swift `append` | 维持现状 | — |
| `clear` | `(mut s: String) -> Unit` | Rust `clear` / Swift `removeAll` | 维持现状 | — |
| `compare`（添加） | `(a: String, b: String) -> Int` | Go `Compare` / Gleam・OCaml `compare` / Rust `cmp` / Kotlin `compareTo` | 添加 | R-string-05（R-L0-16） |
| `byte_len`（添加） | `(s: String) -> Int` | Rust `len` / Go `len` / Gleam・Elixir `byte_size` / Swift `utf8.count` | 添加 | R-string-06 |
| `to_utf16`（添加） | `(s: String) -> List[Int]` | Rust `encode_utf16` / JS `charCodeAt` / Swift `utf16` | 添加 | R-string-07 |
| `from_utf16`（添加） | `(units: List[Int]) -> Result[String, String]` | Rust `String::from_utf16`（Result）/ JS `fromCharCode` | 添加 | R-string-07 |
| `filter` | （添加）`(s: String, f: (String) -> Bool) -> String` | Swift `filter` / Kotlin `filter` | 添加 | R-string-16 |
| `fold` | （添加）`(s: String, init: B, f: (B, String) -> B) -> B` | Swift `reduce` / Kotlin `fold` | 添加 | R-string-16 |
| `pop` | （添加）`(mut s: String) -> String?` | Swift `popLast` / Rust `pop` | 添加 | R-string-16 |
| `insert` | （添加）`(s: String, i: Int, sub: String) -> String` | Swift `insert(contentsOf:at:)` / Rust `insert_str` | 添加 | R-string-16 |
| `remove_at` | （添加）`(s: String, i: Int) -> String` | Swift `remove(at:)` / Rust `remove` | 添加 | R-string-16 |
| `new` | （添加）`() -> String` | Swift `String()` / Rust `String::new` | 添加 | R-string-17 |
| `from_list` | （添加）`(xs: List[String]) -> String` | Swift `String(chars)` / Kotlin `joinToString("")` | 添加 | R-string-17 |
| `enumerate` | （添加）`string.enumerate(s: String) -> List[(Int, String)]` | Swift `enumerated` / Rust `enumerate` | 添加 | R-L0-42 |
| `zip` | （添加）`string.zip(s: String, ys: List[B]) -> List[(String, B)]` | Swift `zip` / Rust `zip` | 添加 | R-L0-42 |
| `zip_with` | （添加）`string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 添加 | R-L0-42 |
| `filter_map` | （添加）`string.filter_map(s: String, f: (String) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 添加 | R-L0-42 |
| `find_map` | （添加）`string.find_map(s: String, f: (String) -> B?) -> B?` | Rust `find_map` | 添加 | R-L0-42 |
| `reduce` | （添加）`string.reduce(s: String, f: (String, String) -> String) -> String?` | Swift `reduce` / Rust `reduce` | 添加 | R-L0-42 |
| `scan` | （添加）`string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 添加 | R-L0-42 |
| `partition` | （添加）`string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` | Kotlin `partition` | 添加 | R-L0-42 |
| `group_by` | （添加）`string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 添加 | R-L0-42 |
| `sort` | （添加）`string.sort(s: String) -> List[String]` | Swift `sorted()` | 添加 | R-L0-42 |
| `sort_by` | （添加）`string.sort_by(s: String, f: (String) -> B) -> List[String]` | Kotlin `sortedBy` | 添加 | R-L0-42 |
| `sort_with` | （添加）`string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` | Swift `sorted(by:)` | 添加 | R-L0-42 |
| `min` | （添加）`string.min(s: String) -> String?` | Swift `min()` / Rust `min` | 添加 | R-L0-42 |
| `max` | （添加）`string.max(s: String) -> String?` | Swift `max()` / Rust `max` | 添加 | R-L0-42 |
| `min_by` | （添加）`string.min_by(s: String, f: (String) -> K) -> String?` | Kotlin `minByOrNull` | 添加 | R-L0-42 |
| `max_by` | （添加）`string.max_by(s: String, f: (String) -> K) -> String?` | Kotlin `maxByOrNull` | 添加 | R-L0-42 |
| `min_with` | （添加）`string.min_with(s: String, cmp: (String, String) -> Int) -> String?` | Swift `min(by:)` | 添加 | R-L0-42 |
| `max_with` | （添加）`string.max_with(s: String, cmp: (String, String) -> Int) -> String?` | Swift `max(by:)` | 添加 | R-L0-42 |
| `find_index` | （添加）`string.find_index(s: String, f: (String) -> Bool) -> Int?` | Swift `firstIndex(where:)` | 添加 | R-L0-42 |
| `take_while` | （添加）`string.take_while(s: String, f: (String) -> Bool) -> String` | Swift `prefix(while:)` | 添加 | R-L0-42 |
| `drop_while` | （添加）`string.drop_while(s: String, f: (String) -> Bool) -> String` | Swift `drop(while:)` | 添加 | R-L0-42 |
| `chunk` | （添加）`string.chunk(s: String, n: Int) -> List[String]` | Kotlin `chunked` | 添加 | R-L0-42 |
| `window` | （添加）`string.window(s: String, n: Int) -> List[String]` | Kotlin `windowed` | 添加 | R-L0-42 |
| `split_at` | （添加）`string.split_at(s: String, n: Int) -> (String, String)` | Rust `split_at` | 添加 | R-L0-42 |
| `dedup` | （添加）`string.dedup(s: String) -> String` | Rust `dedup` | 添加 | R-L0-42 |
| `intersperse` | （添加）`string.intersperse(s: String, sep: String) -> String` | Rust `intersperse` | 添加 | R-L0-42 |
| `unique` | （添加）`string.unique(s: String) -> String` | Kotlin `distinct` | 添加 | R-L0-42 |
| `unique_by` | （添加）`string.unique_by(s: String, f: (String) -> K) -> String` | Kotlin `distinctBy` | 添加 | R-L0-42 |
| `count` | （添加）`(s: String, f: (String) -> Bool) -> Int` | Kotlin `count {}` / Swift `count(where:)` | 添加 | R-string-18、R-L0-42 |
| `is_equal_constant_time` | `string.is_equal_constant_time(a: String, b: String) -> Bool`（提案） | Python `secrets.compare_digest` / Erlang `crypto:hash_equals` | 添加 | R-string-19 |

当前 50 个函数 + 添加 4 个 = 54 行。

---

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0 的事项（已处理）

1. **比较函数的名称和类型。** L0 已加入 R-L0-16（有序类型提供 `<mod>.compare(a, b) -> Int`）。`string.compare` 将由 R-string-05 添加。`bytes.cmp` 将改名为 `bytes.compare`（L2-bytes R-bytes-08）。
2. **构造函数如何表示失败。** L0 已加入 R-L0-06 第 4 项（可能失败的构造、转换不返回哨兵值），R-L0-14 第 4 项允许“使旧写法产生类型错误的同名修改”。`string.from_codepoint` 保持名称不变，改为返回 `Option[String]`（R-string-11）。
