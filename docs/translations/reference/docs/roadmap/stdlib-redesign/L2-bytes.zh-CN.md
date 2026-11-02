> 描述（固定历史）：标准库重设计 L2/L3 — bytes：原地动词 push/put_/append_、唯一字节 I/O 族、生指针迁往 ffi，以及与 list/string 相同的序列名称。
<a id="l2l3-bytes"></a>

# L2/L3: bytes

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-bytes.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-bytes.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-bytes.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取官方来源或独立执行。(F)、计时、实测和实现陈述为原作者报告。保留早期i8/u8格省略与后来新增恢复、旧_at元组顺序与next_逆转、负slice夹取与后来abort、旧L3数量与后续新增行。RawPtr迁移/import是旧提案，不是强制安全边界或当前FFI契约。常数时间比较明确公开长度；原文结构检查不构成独立计时验证或普遍侧信道免疫。日文代码占位符指类型/值/下一位置或状态/宽度/同名。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md)

> 最后更新：2026-10-24
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源：Rust https://doc.rust-lang.org/std/vec/struct.Vec.html / https://doc.rust-lang.org/std/primitive.u32.html / Go https://pkg.go.dev/encoding/binary (F) / https://pkg.go.dev/bytes / Python https://docs.python.org/3/library/struct.html / https://docs.python.org/3/library/stdtypes.html#bytearray / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/DataView (F) / Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F) / Elixir https://elixir.hexdocs.pm/Kernel.html#binary_part/3 / OCaml https://ocaml.org/manual/latest/api/Bytes.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-byte-array/ / Swift https://developer.apple.com/documentation/foundation/data
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm` 两者；probe 位于 scratchpad 的 `probe-bytes/`。
> 数量：按 spec/tests/docs/stdlib/research 分别统计 `/usr/bin/grep -rEo '\bbytes\.<fn>\b'`（包含定义行和生成索引，故为上限）。

读法：`mut`（mute）= 原地改写调用方 `var` 的参数。`??`（空值合并）= 左侧 none/err 时采用右侧。`b[i] = v`（下标赋值）。LE/BE = 小端/大端（字节顺序）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 构造全零 | `new(n)` | `vec![0; n]` | `make([]byte, n)` | `bytes(n)` | `new Uint8Array(n)` | — | `:binary.copy(<<0>>, n)` | `Bytes.make` (F) | `ByteArray(n)` | `Data(count:)` |
| 字符串 ⇄ 字节 | `from_string` / `to_string`（Result）/ `to_string_lossy` | `into_bytes` / `String::from_utf8` | `[]byte(s)` / `string(b)` | `encode` / `decode` | `TextEncoder` / `TextDecoder` | `from_string` / `to_string`→Result (F) | binary 即字符串 | `of_string` / `to_string` (F) | `encodeToByteArray` / `decodeToString` | `Data(s.utf8)` / `String(decoding:)` |
| 读单字节（安全处理越界） | `get`→Option | `get`→Option | — | — | — | — | — | — | `getOrNull` | — |
| 写单字节（原地） | `b[i] = v`、`set_at`、`set_u8` | `b[i] = v` | `b[i] = v` | `b[i] = v` | `b[i] = v` | — | — | `set_uint8` (F) | `b[i] = v` | `b[i] = v` |
| 末尾追加单字节（原地） | `push`、`append_u8`、`write_u8` | `push` | `append` | `bytearray.append` | — | — | — | `Buffer.add_uint8` | — | `append` |
| 末尾追加字节序列（原地） | **—**（`concat` 返回副本） | `extend_from_slice` | `append(b, s...)` | `extend` / `+=` | `set` | `append`（副本）(F) | `<>` | `Buffer.add_bytes` | `+=` | `append(contentsOf:)` |
| 读取定宽整数/浮点数 | `read_<型>_<le/be>`（越界为 0）+ `read_uint16` 族（两族） | `u32::from_le_bytes` | `binary.LittleEndian.Uint32` (F) | `struct.unpack_from` | `DataView.getUint32(o, le)` (F) | 比特语法 | `<<x::little-32>>` | `get_int32_le` (F) | — | `load(as:)` |
| 按位置覆盖 | `set_<型>_<le/be>` + `set_uint16` 族 | `copy_from_slice(&x.to_le_bytes())` | `PutUint32` (F) | `struct.pack_into` | `DataView.setUint32` (F) | — | — | `set_int32_le` (F) | — | — |
| 末尾追加 | `append_<型>_<le/be>` + `write_*_be` + `write_uint16` 族 | `extend_from_slice` | `AppendUint32` (F) | `+= struct.pack` | — | 比特语法 | `<<b::binary, x::32>>` | `Buffer.add_int32_le` | — | — |
| 边读取边推进位置 | `next_<型>`、`next_bytes`、`skip`、`is_eof`（当前为 `read_<型>_at`、`take_at`、`eof`） | `Cursor` | `bytes.Reader` + `binary.Read` | `io.BytesIO` | — | 模式匹配 | 模式匹配 | — | `ByteBuffer`(JVM) | — |
| 子序列/拼接/搜索 | `slice`、`concat`、`index_of`、`contains`、`starts_with`、`ends_with`、`split`、`lines` | ● | `bytes.*` | ● | ● | `slice`、`starts_with` (F) | `binary_part`、`:binary.match` | `sub`、`cat` (F) | ● | ● |
| 头/尾 n 个 | **—**（`slice` 仅此） | `&b[..n]` | `b[:n]` | `b[:n]` | `subarray` | `slice` (F) | `binary_part` | `sub` | `take` / `takeLast` | `prefix` / `suffix` |
| 生指针 | `as_ptr`、`as_mut_ptr`、`data_ptr`、`from_raw_ptr`、`copy_to_ptr` | `as_ptr` 等 | `unsafe` | `ctypes` | — | — | — | — | — | `withUnsafeBytes` |

<a id="完全性の規則"></a>

### 完备性规则

1. **二进制读写统一为一族矩阵。** 轴为类型×字节序×操作。
   - 类型：`u8`、`u16`、`i16`、`u32`、`i32`、`i64`、`f16`、`f32`、`f64`、`bool`。
   - 字节序：至少两字节的类型都有 `_le`、`_be`。`u8`、`bool` 无字节序。
   - 操作：`get_<型>`（按位置读取，越界 none）、`next_<型>`（推进读取位置，返回 `(Option[値], 次の位置)`，R-bytes-13/R-L0-23）、`get_<型>_array`（仅至少两字节的类型，任一越界即 none）、`put_<型>`（按位置原地覆盖）、`append_<型>`（原地末尾追加）。
   - **有意省略**：`u8` 位置读取由 `get` 承担（同 `list.get`），`u8` 的 `append` 由 `push` 承担（同 `list.push`/`string.push` 的“末尾加一个”动词，R-bytes-02）。无 `u8`、`bool` 的 `_array`（可写 `bytes.slice(...) |> bytes.to_list`）。不提供 `i8`、`u64`。`i8` 可用 `bytes.read_u8(b, p) |> int.to_int8` 读取；`u64` 无法装入 `Int`（i64），故由 `i64` 承载相同比特。
   - **有符号/无符号写入体相同的格**（如 `append_i16_le` 与 `append_u16_le`）两者均保留，使其与读取名称对称。优先让人可由名称推测编写。R-L0-01 不将已声明且有 gate 保护的类型矩阵格视为同义词（“向 L0 回报（已解决）”第 1 项）。
2. **原地改写动词限于 R-L0-04 集合**：`push`、`clear`、`fill`、`append_<型>`、`put_<型>`、`copy_from`、`copy_within`。`set`、`insert`、`remove_at` 属副本侧。
3. **序列函数与 list/string 同拼写**：`compare`（所有有序类型均为 `<mod>.compare(a, b) -> Int`）、`slice`、`take`、`drop`、`take_end`、`drop_end`、`chunk`、`map`、`pad_start`、`pad_end`、`index_of`、`contains`、`starts_with`、`ends_with`、`split`、`lines`、`repeat`、`reverse`、`insert`、`remove_at`。
4. **可失败读取不返回哨兵（零/空字符串）**（R-L0-06 第 4 项）。无需原因则 Option，需要则 `Result[_, String]`。
5. **不放置内存不安全函数**（R-L1-26）。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`push` ≡ `append_u8` ≡ `write_u8`，`set_at` ≡ `set_u8`（实测三个均将 300 → `[44]`；`set_at`、`set_u8` 同体）→ R-bytes-02。`write_u32_be` ≡ `append_u32_be` 及另外三对 → R-bytes-03。
- 两族：后缀族（`read_u32_le`）与 `Endian` 参数族（`read_uint32(b, o, .le)`，12 函数、仅四宽度）→ R-bytes-04（#3851 D）。
- 动词方式：`set_<型>` 接受 `mut`，但 `bytes.set`、`list.set`、`map.set` 返回副本 → R-bytes-01。
- 缺口：`f16` 仅 LE 读取，缺 BE/写入；`bool` 无按位置覆盖；无原地追加字节序列函数 → R-bytes-03/R-bytes-05。
- 拼写偏差：`cmp`（有序类型为 `compare`）、`chunks`（list 为 `chunk`）、`map_each`（其他六模块为 `map`）、`pad_left`/`pad_right`（string 为 `pad_start`/`pad_end`）、缺 `take`/`drop` 族 → R-bytes-08。
- `_at` 两义：`read_u32_le_at` 等推进读取位置，`read_string_at(b, pos, len)` 却只按位置读取 → R-bytes-05。
- L0 通则：`get_or`（R-L0-06）、`eof`（R-L0-09）→ R-bytes-07。
- 生指针 → R-bytes-06（#3853）；长度前缀辅助函数 → R-bytes-05（#3852）。
- 哨兵：按位置 `read_<型>`、`read_bool`、`read_<型>_array` 越界返回零/false（实测 `read_u8(h, 99)` → 0，长度 4 的 `read_u32_le(h, 2)` → 0），违反 R-L0-06 第 4 项 → R-bytes-09。
- 越界写入：`put_<型>`（当前 `set_<型>`）、`set_at`、`set`、`remove_at` 不做事；`insert` 将位置夹取到 `0..len`。`b[i] = v` abort（实测 `Error: index out of bounds`）。违反 L0 位置写入越界 abort 规则 → R-bytes-11。
- 边界矩阵（R-L0-12）：`bytes.slice(b, -1, 2)` 返回 `[]`，`list.slice` 也返回 `[]`，只有 `string.slice("abc", -1, 2)` 返回 `"ab"`（native/wasm 均实测）。R-L0-12 将负索引视为越界并夹取至零，因此 string 正确，bytes/list 应对齐 → R-bytes-12（list 见 L2-list R-list-10）。

<a id="案"></a>

## 提案

<a id="r-bytes-01-位置を指定した上書きを-put_型-にする---採用"></a>

### R-bytes-01 按位置覆盖命名为 `put_<型>` — ○ 采纳

- 内容：将 `set_u8` 与 `set_<型>_<le/be>`（`u16`/`i16`/`u32`/`i32`/`i64`/`f32`/`f64` × LE/BE）共 15 函数改名为 `put_u8`、`put_u16_le` … `put_f64_be`，参数不变。越界位置由另案 R-bytes-11 改为 abort，与改名同窗口落地。
- L0 依据：R-L0-04（`set` 是副本动词，原地位置写入用 `put_<型>`）、R-L0-14。
- (a) Go 用 `PutUint32` (F)，OCaml/JS 用 `set_int32_le`/`setUint32` (F)。虽无多数，Almide 内 `bytes.set`（副本）与 `bytes.set_u8`（原地）同动词，符合 R-L0-00 条件 1，故改名。同 Go `Put`/`Append` 对。
- (b) 仅名称变更，本体（native intrinsic、wasm self-host）不变。
- (c) 将 `bytes.set(b, i, v)` 写为语句会静默无操作（实测 `bytes.set(b, 0, 9)` 后仍为 `[1, 2, 3]`）。分开动词可由名称判断 `set` 副本、`put_` 原地。
- (d) 改名：`@deprecated(since=15, use="bytes.put_<型>")`（`set_u32_le` 为 `use="bytes.put_u32_le"`）、`removal_epoch = 16`。`almide fix` 一对一将 `bytes.set_u32_le(b, p, v)` 改为 `bytes.put_u32_le(b, p, v)`。15 函数合计使用数：spec 178 / tests 10 / docs 78 / stdlib 1。
- gate：R-L0-G `mut` 动词检查（接受 `mut` 的公开函数名属于动词集）。矩阵 gate（R-bytes-03）检查全部 `put_` 格。
- Issue：#3898。

<a id="r-bytes-02-1-バイトの追記は-push1-バイトの上書きは-put_u8-だけにする---採用"></a>

### R-bytes-02 单字节追加仅用 `push`，覆盖仅用 `put_u8` — ○ 采纳

- 内容：撤除 `append_u8`、`write_u8`，统一为 `push`；撤除 `set_at`，统一为 `put_u8`。
- L0 依据：R-L0-01（同角色一种拼写，跨模块亦同）、R-L0-04、R-L0-14。此处裁决 #3851 C。
- (a) 末尾加一个元素的动词分为 Rust `push`、Swift `append`、Python `append`。Almide 内已有 `list.push(mut xs, x)`、`string.push(mut s, x)` 的 `push`，按 R-L0-01 保留相同角色相同拼写的 `push`。
- (b) 三者同体 `b.push(val as u8)`，两目标无影响。
- (c) `set_at` 替换为 `put_u8`；`put_u8` 是类型矩阵（R-bytes-03）中 `u8` 覆盖格，本就保留。R-bytes-11 使越界 abort 后，`put_u8` 与下标赋值 `b[i] = v` 同行为，两种都可写，但 `almide fix` 能维持调用形态一对一改写的只有 `put_u8`。
- (d) 撤除：`@deprecated(since=15, use="bytes.push")`（`append_u8`、`write_u8`）、`use="bytes.put_u8"`（`set_at`）、`removal_epoch = 16`。`almide fix` 一对一：`bytes.append_u8(b, v)` → `bytes.push(b, v)`；`bytes.set_at(b, i, v)` → `bytes.put_u8(b, i, v)`。使用数：`append_u8` 69/5/14/0、`write_u8` 8/1/3/0、`set_at` 44/8/5/0（spec/tests/docs/stdlib）。`push` 保留，spec 45/stdlib 40。
- gate：#3851 F 同义词 gate 规则 6（同模块本体相同的不同 intrinsic）。矩阵 gate 声明“`u8` 的 append 格 = `push`”。
- Issue：#3851 C（既有）。

<a id="r-bytes-03-書き足しを-append_-に揃え型--向き--操作の行列を閉じる---採用"></a>

### R-bytes-03 追加统一为 `append_`，补全类型×字节序×操作矩阵 — ○ 采纳

- 内容：
  - 撤除 `write_u32_be`、`write_i64_be`、`write_f64_be`，统一为 `append_u32_be`、`append_i64_be`、`append_f64_be`。
  - `write_bool` 改名 `append_bool`，新增 `put_bool(mut b, pos, val: Bool)`。
  - 补 `f16` 缺口：`get_f16_be`、`read_f16_be_at`、`get_f16_be_array`、`put_f16_le`、`put_f16_be`、`append_f16_le`、`append_f16_be`。
- L0 依据：R-L0-01、R-L0-04（追加动词 `append_<型>`）、CLAUDE.md“API 族按矩阵展开”。
- (a) 双字节序 `f16` 读写有 JS `getFloat16`/`setFloat16` (F)、Python `struct` 的 `e`。当前只有 LE 读取，BE 文件格式编写者必须自写。
- (b) 纯计算。`f16` 舍入（`Float` → 半精度）由单一 self-host 实现，使两目标比特相同。
- (c) 知道 `get_f16_le` 的 LLM 会按规则推测 `put_f16_le`，矩阵缺口会使推测落空。
- (d) 撤除 3/改名 1/新增 8。`@deprecated(since=15, use="bytes.append_u32_be")` 等（`write_bool` 为 `use="bytes.append_bool"`）、`removal_epoch = 16`。`almide fix` 一对一：`bytes.write_u32_be(b, v)` → `bytes.append_u32_be(b, v)`。四个 `write_*` 使用数：spec 16 / tests 8 / docs 16 / stdlib 0。
- gate：`tests/stdlib_bytes_matrix_test.rs`。按声明类型/字节序/操作生成全部格的名称和签名，要求与 interface JSON 完全一致（多/少均失败）。一个 fixture 在 native/wasm 对比各格往返（`append` → `read`、`put` → `read`）。
- Issue：#3851 C（既有 `write_*` 部分）＋新建（矩阵缺口）。

<a id="r-bytes-04-endian-引数の系統12-関数を撤去し接尾辞の系統を唯一にする---採用"></a>

### R-bytes-04 撤除 `Endian` 参数族（12 函数），仅保留后缀族 — ○ 采纳

- 内容：撤除 `read_uint16`、`read_uint32`、`read_int32`、`read_float32`、`write_uint16`、`write_uint32`、`write_int32`、`write_float32`、`set_uint16`、`set_uint32`、`set_int32`、`set_float32` 及 `type Endian`。删除 `stdlib/bytes.almd:552-569` 的“the new preferred surface”注释。这是 #3851 D 的 (a)。
- L0 依据：R-L0-01（同操作两族）、R-L0-04（`write_`/`set_` 不属于动词集；若采纳 (b)，需增加这两个动词或重命名整个矩阵）、R-L0-00（最少必要改名）。
- (a) 分为名称包含字节序（OCaml `get_int32_le` (F)、Rust `from_le_bytes`）与值传入（Go `binary.LittleEndian.Uint32` (F)、JS `getUint32(o, true)` (F)）。无多数，保留文档/用例更多的后缀族。
- (b) 两族落入相同函数体，无影响。
- (c) 同一读取目前有两种拼写及两种返回类型（`Int`/`UInt32`），源码注释与文档对何者正确存在分歧。L0 动词 `append_`/`put_` 仅适用后缀族。
- (d) 撤除。以 `@deprecated(since=15, use="bytes.get_u32_le")` 等指出各字节序后缀版（`read_uint32(b, o, LittleEndian)` → `bytes.get_u32_le(b, o) |> option.map(int.to_uint32)`）。`removal_epoch = 16`。`Endian` 值为字面量时 `almide fix` 改为后缀版；`Endian` 运行时决定时无法机械改写，仅提示按字节序选择 `bytes.get_u32_le` / `bytes.get_u32_be`。12 函数使用数：spec 103 / tests 15 / docs 46 / stdlib 1。
- gate：R-bytes-03 矩阵 gate 拒绝矩阵外二进制读写函数。
- Issue：#3851 D（既有）。

<a id="r-bytes-05-長さ前置きの補助関数と-read_string_at-を撤去しappend_bytes-を足す---採用"></a>

### R-bytes-05 撤除长度前缀辅助函数与 `read_string_at`，新增 `append_bytes` — ○ 采纳

- 内容：
  - 撤除 `read_string_be`、`read_string_be_at`、`write_string_be`（u32 BE 长度前缀）、`read_length_prefixed_strings_le`、`skip_length_prefixed_le`（u32 LE 长度前缀）、`read_string_at(b, pos, len)`。
  - 新增 `append_bytes(mut b: Bytes, src: Bytes) -> Unit`（原地末尾追加字节序列）。
- L0 依据：R-L0-01、R-L0-04（`append_<型>` 中的类型包括 `bytes`）、R-L0-14。此处裁决 #3852 的 bytes 项。
- (a) 标准库长度前缀字符串主要见 Java `DataOutput.writeUTF`（u16 BE），前缀宽度/字节序取决于格式。Almide 当前单项 u32 BE、批量 u32 LE 也不一致。八种语言有原地追加字节序列函数（Rust `extend_from_slice`、Go `append(b, s...)`、Python `extend` 等）。
- (b) 纯计算。
- (c) `read_string_at` 的 `_at` 与其他 `_at`（推进读取位置并返回 `(位置, Option)`）不同，无法按名称推测形态，也无法猜前缀宽度/字节序。可组合替换：
  - 读取：`let n = bytes.get_u32_be(b, p)?` 后 `bytes.slice(b, p + 4, p + 4 + n) |> bytes.to_string`。
  - 写入：`bytes.append_u32_be(b, bytes.len(sb))` 后 `bytes.append_bytes(b, sb)`。
  - `read_string_at(b, p, n)` → `bytes.slice(b, p, p + n) |> bytes.to_string_lossy`。部分越界时结果不同（旧为空字符串，新为范围内部分），提示中注明。
- (d) 撤除 6/新增 1。`@deprecated(since=15, use="…")`：`read_string_at` 为 `use="bytes.slice(b, p, p + n) |> bytes.to_string_lossy"`；五个前缀函数指向 `use="bytes.get_u32_be + bytes.slice"` 等组合；`removal_epoch = 16`。形态改变，`almide fix` 不改写，只提示 (c) 写法。六函数使用数：spec 35 / tests 14 / docs 33 / stdlib 5；stdlib 五处与撤除同一变更替换。
- gate：矩阵 gate（R-bytes-03）。R-L0-G 新增类型检查：以 `_at` 结尾的公开函数必须返回 `(Int, Option[T])`。
- Issue：#3852（既有）。

<a id="r-bytes-06-生ポインタを-ffi-に移す---採用"></a>

### R-bytes-06 生指针迁往 `ffi` — ○ 采纳

- 内容：遵循 #3853：`as_ptr`/`as_mut_ptr`/`data_ptr` → `ffi.address(b) -> RawPtr`（空则 null），`from_raw_ptr` → `ffi.read(ptr, len) -> Bytes`，`copy_to_ptr` → `ffi.write(b, ptr, cap) -> Int`。不自动 import `ffi`。
- L0 依据：R-L1-26、R-L0-01（数据地址有三种拼写）。
- (a) Rust 也有 `as_ptr`，但不安全操作限于 `unsafe`。Almide 没有 `unsafe`，因此分模块并要求 import。
- (b) 空 Bytes 当前结果不同：`as_ptr` 为 null，`data_ptr` 为 handle + 12。`ffi.address` 两目标统一为 null。撤除前确认 wasm 向 JS host 传地址的路径（#907、`spec/wasm_host_js/extern_data_ptr_lifetime.almd`）能通过 `ffi.address`。
- (d) 撤除（`ffi` 侧新增）。`@deprecated(since=15, use="ffi.address")`（`as_ptr`/`as_mut_ptr`/`data_ptr`）、`use="ffi.read"`（`from_raw_ptr`）、`use="ffi.write"`（`copy_to_ptr`）、`removal_epoch = 16`。`almide fix` 一对一改写并加 `import ffi`。`data_ptr` 返回类型由 `Int` 变为 `RawPtr`，产生类型错误处给提示。五函数使用数：spec 48 / tests 1 / docs 36 / stdlib 2。
- gate：检查 `bytes` 公开函数类型不出现 `RawPtr`。
- Issue：#3853（既有）。

<a id="r-bytes-07-get_or-を撤去しeof-を-is_eof-に改名する---採用"></a>

### R-bytes-07 撤除 `get_or`，`eof` 改为 `is_eof` — ○ 采纳

- 内容：`get_or(b, i, d)` → `bytes.get(b, i) ?? d`；`eof(b, pos)` → `is_eof(b, pos)`（同行为，`pos >= len` 或负数时 true）。
- L0 依据：R-L0-06（默认值版 `_or` 统一为 `??`）、R-L0-09（Bool 名称为 `is_`；`bytes.eof` 是 L0 指明的例外）。
- (b) stdlib 词法分析器使用 `get_or` 33 次。撤除前以 `research/benchmark/perf/` 词法分析器确认替换成 `bytes.get(b, i) ?? -1` 不减速（native `Option<i64>` 不装箱，预计无差）。
- (d) 撤除 1/改名 1。`@deprecated(since=15, use="??")`（`get_or`）、`@deprecated(since=15, use="bytes.is_eof")`（`eof`）、`removal_epoch = 16`。`almide fix`：`bytes.get_or(b, i, d)` → `bytes.get(b, i) ?? d`；`bytes.eof(b, p)` → `bytes.is_eof(b, p)`。使用数：`get_or` 69/7/11/33，`eof` 9/1/6/1（spec/tests/docs/stdlib）。
- gate：R-L0-G 的 R-L0-06/R-L0-09 规则。
- Issue：#3913。

<a id="r-bytes-08-列の関数を-liststring-と同じ綴りにする---採用"></a>

### R-bytes-08 序列函数与 list/string 同拼写 — ○ 采纳

- 内容：
  - 改名 `cmp` → `compare`（L0 修订：所有有序类型为 `<mod>.compare(a, b) -> Int`）、`chunks` → `chunk`、`map_each` → `map`、`pad_left` → `pad_start`、`pad_right` → `pad_end`。
  - 新增返回副本的 `take(b, n)`、`drop(b, n)`、`take_end(b, n)`、`drop_end(b, n)`，边界与 list/string 相同（`n` 超长返回全部；负 `n` 时 `take` 全部、`drop` 空，L2-string 完备性规则 1）。
  - `chunk` 同 `list.chunk`，大小≤0 时 abort（实测 `list.chunk(xs, 0)` 为 `Error: chunk size must be positive`，`bytes.chunks(b, 0)` 为 `[]`）。新名称不属于 R-L0-14“同名不能改义”。
- L0 依据：R-L0-01（跨模块同角色同拼写）、R-L0-12（有 `take`/`drop`/`take_end`/`drop_end` 的类型包括 bytes）、R-L0-06 第 3 项（仅违约 abort，零大小分块是违约）。
- (a) 首尾截取有 Kotlin `take`/`takeLast`、Swift `prefix`/`suffix` 等，各语言对字节序列和列表同拼写。
- (b) 纯计算。
- (c) 知道 `list.map` 的 LLM 会写 `bytes.map`，当前却是未定义错误。
- (d) 改名 5/新增 4。`@deprecated(since=15, use="bytes.compare")`（`cmp`）、`use="bytes.chunk"`（`chunks`）、`use="bytes.map"`（`map_each`）、`use="bytes.pad_start"`/`use="bytes.pad_end"`、`removal_epoch = 16`。`almide fix` 一对一改写。`chunks(b, 0)` 原返回 `[]`，`chunk(b, 0)` 会 abort，应在提示注明。使用数：四个改名函数（除 `cmp`）spec 30 / tests 0 / docs 17 / stdlib 4；`cmp` 为 20/1/6/1。
- gate：R-L0-12 边界矩阵测试（list/string/bytes 对照相同输入）纳入 bytes 五函数。将跨模块同义词 gate（#3851 F 规则 5）扩展至同角色不同拼写属于 L0 范围，此处只做矩阵测试。
- Issue：#3899。

<a id="r-bytes-09-位置指定の読みを番兵-0-を返す-read_型-から-option-を返す-get_型-に移す---採用"></a>

### R-bytes-09 按位置读取由零哨兵 `read_<型>` 改为返回 Option 的 `get_<型>` — ○ 采纳

- 内容：
  - `read_<型>_<le/be>`（14）及 `read_f16_le` 改为 `get_<型>_<le/be>(b, pos) -> Option[T]`，越界 none。
  - `read_bool(b, pos)` → `get_bool(b, pos) -> Option[Bool]`。
  - `read_<型>_array`（15）→ `get_<型>_array(b, pos, count) -> Option[List[T]]`，任一越界即 none。
  - 撤除 `read_u8(b, pos)`，统一为 `bytes.get(b, pos)`（同角色，已返回 Option）。
  - 推进位置版 `read_<型>_at` 已返回 Option，保持不变。
- L0 依据：R-L0-06 第 4 项（可失败构造/解码不返回哨兵，无需原因则 Option）。R-L0-14 第 4 项也允许同名返回类型 `Int` 改成 `Option[Int]`，但仍改名是因为动词不符。`read_` 属于推进位置的 `read_<型>_at` 族，按位置以 Option 提取的动词为 `get`。`get_` 与 R-L0-06“无后缀提取为 Option”（`list.get`、`bytes.get`）同动词。
- (a) 没有语言越界返回零。Go/JS/Python/Rust/Kotlin 为异常或 panic（JS DataView 为 RangeError (F)）。名称同 OCaml `get_int32_le`/`set_int32_le` (F)。
- (b) 纯计算。边界检查由单一 self-host 实现，使两目标同结果。
- (c) 当前漏查长度会将损坏文件静默读为零；Option 不写 `?` 或 `??` 就无法取值。
- (d) 改名 31/撤除 1。`@deprecated(since=15, use="bytes.get_u32_le")` 等（`read_u8` 为 `use="bytes.get"`）、`removal_epoch = 16`。`almide fix` 改形：`bytes.read_u32_le(b, p)` → `bytes.get_u32_le(b, p) ?? 0`（保持当前结果），提示建议用 `?` 传播失败。除 `read_u8` 的 31 函数合计：spec 194 / tests 3 / docs 159 / stdlib 59（`read_u8` 为 98/6/6/2）。stdlib 与 `matrix.from_bytes_*` 内部同一变更改写。
- gate：矩阵 gate（R-bytes-03）检查 `get_*` 返回 Option。R-L0-G 列出返回 `Int`/`Float`/`Bool`/`String`、名称以 `read_`/`get_`/`decode`/`parse` 开头的公开函数，通过边界输入确认不返回哨兵。
- Issue：#3897。

<a id="r-bytes-10-utf-8-の両方向は-bytes-が持つ---採用据え置き"></a>

### R-bytes-10 UTF-8 双向都由 bytes 承担 — ○ 采纳（保留）

- 内容：保留 `bytes.from_string`、`bytes.to_string`（Result）、`bytes.to_string_lossy` 为字符串/字节序列转换的唯一拼写。`string.to_bytes`/`string.from_bytes`（`List[Int]`）在 L2-string 撤除（L0 修订裁决）。
- L0 依据：R-L0-07 第 4 项（双向配对类型一致）、R-L0-01、R-L0-06 第 4 项（`to_string` 为带原因 Result，`to_string_lossy` 用 U+FFFD 替换字符必成功）。
- (a) 分为来源侧解码（Gleam `bit_array.to_string`→Result (F)、Python `decode`、Kotlin `decodeToString`、OCaml `Bytes.to_string` (F)）与目标侧（Rust `String::from_utf8`、Swift `String(decoding:)`）。保持 Almide 双向都在编码侧的形态（同 base64/hex）。
- (b) `bytes.from_string` 在 native 为 `Vec<u8>`，比 `string.to_bytes` 的 `Vec<i64>`（8 倍大小，#2078）更轻。
- (d) bytes 侧无变更。
- Issue：#2078（相关）。

<a id="r-bytes-11-範囲外の位置への書き込みを-abort-にする---採用"></a>

### R-bytes-11 越界位置写入改为 abort — ○ 采纳

- 内容：按位置写入将越界视为调用方违约而停止（abort）。
  - 原地覆盖 `put_<型>(mut b, pos, v)`（当前 `set_<型>`/`set_u8`/`set_at`）目前宽度越界则无操作；改为 `pos < 0` 或 `pos + 幅 > len` 时 abort。
  - 副本式位置写入 `bytes.set(b, i, v)`/`bytes.remove_at(b, i)` 目前越界返回原值，改为超出 `0 <= i < len` 时 abort。
  - `bytes.insert(b, pos, v)` 目前夹取到 `0..len`，改为同 `list.insert` 接受 `0..=len`，其外 abort。
- L0 依据：位置写入规则（R-L0-12 后、R-L0-13 前）、R-L0-06 第 3 项。
- (a) Go `PutUint32` panic，JS `DataView.setUint32` RangeError (F)，OCaml `set_int32_le` 异常 (F)，Rust `copy_from_slice` panic，没有语言静默丢弃越界写入。
- (b) 两目标采用相同 abort 路径（下标赋值 `b[i] = v` 已用），边界判断由单一 self-host 实现。
- (c) 当前 `put_` 与 `b[i] = v` 越界行为不同，写入静默消失，写坏文件也察觉不到。
- (d) 语义变更（缩小定义域）。范围内写入结果不变，不计 dialect epoch `breaks`（现有缩小定义域不触发 epoch 的处理）。名称与 R-bytes-01 同窗口改变，但本案自身无需 `@deprecated`。依赖越界无操作的调用可写 `if pos + 4 <= bytes.len(b) then bytes.put_u32_le(b, pos, v) else ()`。统计 spec/tests/stdlib 传越界位置的调用，在 PR 记录数量。
- gate：矩阵 gate（R-bytes-03）的全部 `put_` 格以及 `set`/`insert`/`remove_at`，在 native/wasm 设置 {-1, 0, len-宽度, len-宽度+1, len} 边界 fixture，确认 abort 位置一致（同 L2-list R-list-13 表）。可观察行为改变，契约账本须先进入 als。
- Issue：#3937。

<a id="r-bytes-12-slice-の負の開始位置を-0-に丸める---採用-r-l0-29-で-abort-に変更"></a>

### R-bytes-12 `slice` 负起点夹取至零 — ○ 采纳（→ R-L0-29 改为 abort）

- 第六轮注：负起点夹取已由 R-L0-29 改为 abort，正侧越界夹取到长度仍按本案。
- 内容：`bytes.slice(b, start, end)` 的负 `start` 夹取到零。目前 `bytes.slice(b, -1, 2)` 返回长度零，改为同 `string.slice("abc", -1, 2)` 返回前两字节。
- L0 依据：R-L0-12（越界索引夹取至端点，负索引属于越界）。
- (a) Python/JS 将负索引用于从末尾计数，R-L0-12 不采用。夹取规则同 Gleam/Elixir `slice`。
- (b) 纯计算，两目标同行为。
- (c) list/string/bytes 同参数却不同结果，会令在一种类型学到的边界行为在另一类型失效。
- (d) 语义变更（仅边界输入结果，与文档“capped”对齐的修正）。名称/类型不变，无需 `@deprecated`。统计 spec/tests 负起点调用，在 PR 记录。
- gate：R-L0-12 边界矩阵（list/string/bytes × `slice`/`take`/`drop`/`take_end`/`drop_end` × {-1, 0, len, len+1}）。
- Issue：#3944。

<a id="r-bytes-13-読み位置を進める族を-next_型-にし値-次の位置-を返す---採用"></a>

### R-bytes-13 推进位置族改为 `next_<型>`，返回 `(値, 次の位置)` — ○ 采纳

- 现状：`read_<型>_at(b, pos) -> (Int, Option[T])`（18）及 `take_at(b, pos, n) -> (Int, Option[Bytes])`，下一位置在前、值在后。
- 提案：`next_<型>(b, pos) -> (Option[T], Int)`、`next_bytes(b, pos, n) -> (Option[Bytes], Int)`。`skip(b, pos, n) -> Int`、`is_eof(b, pos) -> Bool` 名称/形态不变（仅返回状态、不返回值，非 R-L0-23 对象）。
- L0 依据：R-L0-23（推进状态函数为 `next_`、`(値, 次の状態)`），同 `random.next_int(rng, …) -> (Int, Rng)`。
- 读取失败不推进位置（同当前 `read_u32_le_at` 对短输入返回 `(0, none)`），改名后保留。`skip(b, pos, n)` 在 `n < 0` 时返回负位置（实测 `skip(a, 1, -2)` 为 `-1`），故 `n < 0` 视为违约 abort（R-L0-12/R-L0-06 第 3 项）。
- (a) Rust 常用 bytes 库 `get_i32_le` 推进位置，与 Almide 仅按位置读取的 `get_<型>`（R-bytes-09）反义。推进版用 `next_` 后 `get_` 含义唯一。
- (c) `let (v, pos) = bytes.next_u32_le(b, pos)` 与随机数 `let (n, rng) = random.next_int(rng, 0, 9)` 同形。
- (d) 改名/语义变更（元组顺序变化），旧写法类型错误，满足 R-L0-14.4。epoch 15 旧名加 `@deprecated`，epoch 16 撤除。`almide fix` 改名，解构顺序以提示指引。
- 完备性：`get_<型>`（按位置读）、`next_<型>`（推进）、`put_<型>`（按位置写）、`append_<型>`（末尾写）四列具有全部相同类型行（`u16`–`i64`、`f16`–`f64` 的 le/be 及 `bool`）。仅 `u8` 的 `get` 列为 `bytes.get`、`append` 列为 `bytes.push`（同角色已有函数，R-bytes-02/R-bytes-09）。`get_<型>_array` 仅至少两字节类型。
- gate：从 interface JSON 检查四列×类型矩阵（扩展 R-bytes-03 gate）。
- Issue：#3962。

<a id="r-bytes-14-bytesheap_savebytesheap_restore-を公開の面から外す---採用"></a>

### R-bytes-14 从公开表面移除 `bytes.heap_save`/`bytes.heap_restore` — ○ 采纳

- 现状：`stdlib/bytes.almd` 有公开 `fn heap_save() -> Int`、`fn heap_restore(checkpoint: Int) -> Unit`，本记录 L3 表也漏列（记录遗漏）。
- 提案：改内部名 `__heap_save`、`__heap_restore`，仅供 runtime fixture 使用。
- L0 依据：R-L1-26（标准库不放内存不安全函数）、R-L0-02（主语非 Bytes）、R-L0-15（返回值因目标而异，native 为零，wasm 为堆地址）。
- (a) 没有语言将堆回退作为标准公开 API。
- (b) wasm 回退后使用相应值会损坏。
- (c) `bytes` 自动 import，故出现在补全候选中；为“释放内存”而调用会只在 wasm 损坏。
- (d) 撤除 2（内部保留），epoch 15 加 `@deprecated`，epoch 16 退出公开。使用数：spec 四文件（全为 runtime 测试），stdlib 2。
- gate：R-bytes-03 拒绝不在矩阵/序列函数表中的 bytes 公开函数，也能阻止本次遗漏。
- Issue：#3970。

<a id="r-bytes-15-i8-の行を読み書きの行列に足す---採用"></a>

### R-bytes-15 读写矩阵新增 `i8` 行 — ○ 采纳

- 现状：类型 u8/u16/i16/u32/i32/i64/f16/f32/f64/bool，i8 有意省略，理由可用 `bytes.read_u8(b, p) |> int.to_int8`；但 R-bytes-09 撤除 `read_u8`，且 `int.to_int8` 返回 `Int8`（其他格为 `Int`）。
- 提案：新增 `get_i8(b, pos) -> Int?`、`next_i8(b, pos) -> (Int?, Int)`、`put_i8(mut b, pos, val: Int)`、`append_i8(mut b, val: Int)`。
- L0 依据：R-L0-01 矩阵例外（有/无符号同体格也都保留）、CLAUDE.md“族按矩阵”。
- (a) 有符号 8 位读取有 OCaml `Bytes.get_int8` (F)、JS `DataView.getInt8`、Python `struct` 的 `b`、Java `readByte`。
- (c) 有 `get_i16_le` 无 `get_i8` 会让按规则推测失效；手写符号转换 `if x >= 128 then x - 256 else x` 易错。
- (d) 新增 4。
- 完备性：类型轴 {u8, i8, u16, i16, u32, i32, i64, f16, f32, f64, bool}，只缺 u64，因放不进 Int，`get_i64_*` 承载相同比特。此名属于 R-L0-19 指引对象。
- Issue：#3970。

<a id="r-bytes-16-bytes-も--で連結しbytesconcat-を撤去する---採用"></a>

### R-bytes-16 `Bytes` 也用 `+` 拼接，撤除 `bytes.concat` — ○ 采纳

- 现状：`fn bytes.concat(a: Bytes, b: Bytes) -> Bytes`；`a + b` 被“operator '+' requires numeric, String, or List types”拒绝。list/string 用 `+` 拼接，无 `concat` 函数（R-list-12）。
- 提案：检查器及两目标 lowering 接受 `Bytes + Bytes -> Bytes`，撤除 `bytes.concat`。CHEATSHEET“`+` 为字符串和列表拼接”扩展为序列（String/List/Bytes）拼接。
- L0 依据：R-L0-00 第 1 项（三个序列类型中唯 Bytes 拼接写法不同）、R-L0-01。
- (a) Python `b1 + b2`、Kotlin `ByteArray.plus`、Swift `Data + Data`、Elixir `<>`；函数式有 Go `append`、OCaml `Bytes.cat`、Gleam `bit_array.append`。
- (b) native 拼接 `Vec<u8>`，wasm 只调用 self-host `bytes_concat`。
- (c) 实测 `a + b` 类型错误，学过 list/string `+` 的 LLM 也会用于 Bytes。
- (d) 语言变更（仅扩大接受范围）及撤除 1。`@deprecated(since=15, use="+")`，epoch 16 撤除。`almide fix`：`bytes.concat(a, b)` → `a + b`。`html.concat` 是 `SafeHtml` 类型，非对象。
- gate：`+` 类型矩阵（String/List/Bytes × 两目标）fixture，契约先入 als。
- Issue：#3970。

<a id="r-bytes-17-byteslines-を撤去するstringlines-と-r-の扱いが違う---採用"></a>

### R-bytes-17 撤除 `bytes.lines`（与 `string.lines` 的 `\r` 处理不同）— ○ 采纳

- 现状：`bytes.lines(bytes.from_string("a\r\nb\n"))` 首行为 `[97, 13]`（保留 `\r`），`string.lines("a\r\nb\n")` 首行为 `"a"`（去掉 `\r`），同动词不同义。
- 提案：撤除 `bytes.lines`。文本行用 `string.lines(bytes.to_string(b)!)`；保持字节按 `\n` 分割用 `bytes.split(b, bytes.from_string("\n"))`。
- L0 依据：R-L0-00 第 1 项（同动词因模块异义）、R-L0-14.4（不能同名改行为）。没有多数名称，不采用改名案。
- (a) 字节行分割去掉 `\r` 的有 Go `bufio.ScanLines`、Python `bytes.splitlines`、Rust `BufRead::lines`，未找到保留它的标准函数。
- (c) Windows 文件经 `fs.read_bytes` 分行会静默保留行尾 `\r`，导致比较不符。
- (d) 撤除，epoch 15 加 `@deprecated`，epoch 16 撤除。形态变更，不做 `almide fix`，提示两种写法。使用数 spec 9 / docs 1。
- Issue：#3970。

<a id="r-bytes-18-bytes-に高階関数の族を置くfilterfoldanyallcountfind---採用"></a>

### R-bytes-18 bytes 增加高阶函数族（`filter`/`fold`/`any`/`all`/`count`/`find`）— ○ 采纳

- 现状：bytes 高阶函数只有 `map`。
- 提案：与 list/deque 相同 D3 族：`fn bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`、`fold(b, init: B, f: (B, Int) -> B) -> B`、`any`/`all`（`(Int) -> Bool`）、`count(b, f) -> Int`、`find(b, f) -> Int?`。已有 `contains`（子序列含义，按 R-L0-01 随主语类型与 list `contains` 区分）。
- L0 依据：R-map-06（D3 族置于所有有序集合），仅有 `map` 的 bytes 不完整。
- (a) Rust `[u8]`、Kotlin `ByteArray`、Swift `[UInt8]`/`Data`、Python `bytes` 均可作为序列使用全族。
- (b) 纯计算。native `bytes.to_list |> list.fold` 会构造八倍大小的 `List[Int]`，直接 `fold` 在校验和等处理中更快。
- (c) 见过 `bytes.map` 的 LLM 也会写 `bytes.fold`/`bytes.filter`，当前为 E002。
- (d) 新增 6。
- 完备性：有序集合 list/deque/bytes 与 map/set 均有 D3 七项。string 无字符值（`chars` 返回 `List[String]`），因此不含。
- gate：R-map-06 矩阵增加 bytes 列。性质测试：各格与 `bytes.to_list(b) |> list.<同名>(f)` 同结果。
- Issue：#3970。

<a id="r-bytes-19-_array-の読みだけがある理由を規則に書く---採用文書だけ"></a>

### R-bytes-19 写明只有 `_array` 读取的理由 — ○ 采纳（仅文档）

- 提案：有意省略增加“写入 `put_`/`append_` 和推进读取 `next_` 不设 `_array`，用 `for` 逐项写入”，纳入矩阵 gate 声明。
- (a) 批量数组写入有 Python `struct.pack`、Go `binary.Write`，非多数。
- (c) 声明缺口后，R-L0-19 指引可将写 `append_f32_le_array` 的 LLM 引向正确形态。
- Issue：#3970。

<a id="r-bytes-20-firstlastpop-を足す---採用"></a>

### R-bytes-20 新增 `first`/`last`/`pop` — ○ 采纳

- 现状：bytes 有 `get`/`push`，无 `first`/`last`/`pop`，从 list（`first`/`last`/`pop`）、deque（`first`/`last`/`pop`）类推会得到 E002。
- 提案：`fn bytes.first(b: Bytes) -> Int?`、`fn bytes.last(b: Bytes) -> Int?`、`fn bytes.pop(mut b: Bytes) -> Int?`。
- L0 依据：R-L0-12（序列类型函数同拼写）、R-L0-06（可失败的无后缀提取为 Option）。
- (a) Rust `Vec<u8>` 的 `first`/`last`/`pop`、Python `bytearray.pop`、Kotlin `ByteArray.first`/`lastOrNull`、Swift `Data.first`/`last`/`popLast`。
- (b) 纯计算，`pop` 是原地改写函数，同 `push`。
- (d) 新增 3。
- 完备性：有 `push` 的序列（list/bytes）也有 `pop`/`first`/`last`。
- Issue：#3980。

<a id="r-bytes-21-bytesskip-を-next_-の形にする---不採用"></a>

### R-bytes-21 将 `bytes.skip` 改为 `next_` 形态 — × 否决

- 理由：`skip` 不读取值，只推进位置，没有 `(値, 次の状態)` 的值。返回 `(Unit, Int)` 只增加调用方丢弃的值。R-L0-27 已写明“不读取值的位置移动仅返回下一状态”。
- Issue：无。

<a id="r-bytes-22-byteschunk-は-size--0-で-abort---採用"></a>

### R-bytes-22 `bytes.chunk` 在 `size <= 0` 时 abort — ○ 采纳

- 现状：R-list-10 使 `list.chunk` 对 `n <= 0` abort，`bytes.chunk` 无规则。
- 提案：同 `list.chunk`，abort。
- L0 依据：R-L0-12（list/string/bytes 边界矩阵一致）。
- (d) 语义变更（不接受输入改为 abort）。
- Issue：#3985。

<a id="r-bytes-23-bytesxor-は長さが違うと-abort---採用"></a>

### R-bytes-23 `bytes.xor` 长度不同则 abort — ○ 采纳

- 现状：不同长度截短至较短者（实测 `almide 0.68.0 dev 437b8f127`，native/wasm 相同：`[1, 2, 3]` 与 `[1]` 得 `[0]`），不停止而返回较短结果。
- 提案：长度不同 abort，同 matrix 形状不匹配，调用前可用 `bytes.len` 检查。
- L0 依据：R-L0-06 第 3 项（调用方可确认的前提违反则 abort）。
- (d) 语义变更，当前截短返回的调用改为 abort，计 epoch 15 `breaks`。
- Issue：#3985。

<a id="r-bytes-24-bytesset-と-put_u8-を-1-つにする---不採用"></a>

### R-bytes-24 合并 `bytes.set` 与 `put_u8` — × 否决

- 理由：`set(b, i, v) -> Bytes` 返回副本，`put_u8(mut b, pos, v)` 原地改写，属于 R-L0-04 的配对（默认副本，原地固定动词）。`list.set` 也返回副本。
- Issue：无。

<a id="r-bytes-25-型つきの読み書きの行列に-get_u8append_u8-を戻す---採用r-bytes-02-の一部を見直し"></a>

### R-bytes-25 类型化读写矩阵恢复 `get_u8`/`append_u8` — ○ 采纳（重审 R-bytes-02 部分）

- 现状：类型化读写族 {`get_`, `put_`, `next_`, `append_`} × {类型}，仅 `u8` 列缺 `get_u8`/`append_u8`（有 `get_i8`/`put_u8`/`next_u8`），因 R-bytes-02 将 `append_u8` 归入 `push`，R-bytes-09 将 `get_u8` 归入 `get`。三个外部评审接连指出族缺口。
- 提案：新增 `fn bytes.get_u8(b: Bytes, pos: Int) -> Int?`，不撤除而保留 `append_u8`。`bytes.get`/`bytes.push` 作为容器读写保留（同 `list.get`/`list.push`）。
  - 同计算两个名称，但分别是容器族和类型化读写族的格。CLAUDE.md“族按矩阵”优先于 R-L0-01 不设同义词。
- (a) Go `binary.LittleEndian`、JS `DataView` 的 `getUint8`、OCaml `Bytes.get_uint8`。
- (d) 新增 1、取消撤除 1。
- 完备性：类型化读写四动词涵盖所有类型列，包括 `u8`。
- Issue：#3991。

<a id="r-bytes-26-bytesget_u8-と-getappend_u8-と-push-を-1-つにする---不採用役割の違いを明記"></a>

### R-bytes-26 合并 `bytes.get_u8`/`get` 与 `append_u8`/`push` — × 否决（明确角色差别）

- 理由：结果相同但所属族不同。`get`/`push` 属容器族（同 `list.get`/`deque.push`），`get_u8`/`append_u8` 属编码族（`get_u16_le`/`append_u16_le` … 的单字节格）。编码族仅移除 `u8` 会在 `get_<型>` 矩阵留洞，使推测的 `get_u8` 失效（R-bytes-25）。
- 约定：定义上返回同值（`get_u8(b, i) == get(b, i)`，`append_u8` 与 `push` 同追加）。矩阵 gate 也检查此一致性。
- Issue：无。

<a id="r-bytes-27-bytes-に-text-の要件をそろえる---採用"></a>

### R-bytes-27 为 `bytes` 补全 `Text` 要求 — ○ 采纳

- 内容：增加 `last_index_of`、`count_matches`、`replace`、`replace_first`、`strip_prefix`、`strip_suffix`，与 `string` 同名函数同形同义，位置按字节。
- 理由：protocol 层级中 `bytes` 与 `string` 同属 `Text`（按子序列搜索/替换）。Go `bytes`（`LastIndex`/`ReplaceAll`/`CutPrefix`）、Python `bytes`（`rfind`/`replace`/`removeprefix`）、Rust 切片（`strip_prefix`）、Swift `Collection`（`trimmingPrefix`/`replacing`）均有。
- (d) 新增 6。
- Issue：#4012。

<a id="r-bytes-28-bytesnewlen-を-byteszerosn-に改名しbytesnew-を空のバイト列にする---採用"></a>

### R-bytes-28 `bytes.new(len)` 改名 `bytes.zeros(n)`，`bytes.new()` 构造空字节序列 — ○ 采纳

- 现状：容器 `new()` 创建空容器（`list`/`string`/`deque`/`set`/`heap`/`map`），仅 `bytes.new(len)` 创建长度 `len` 的零字节序列，参数数目不同，不符合 protocol 的 `Container.new`。
- 提案：现有函数改为 `bytes.zeros(n: Int) -> Bytes`（同 `matrix.zeros`），`bytes.new() -> Bytes` 创建空字节序列。
- (a) 各语言长度 n 零缓冲区都传入长度（Rust `vec![0; n]`、Go `make([]byte, n)`、Python `bytes(n)`、JS `new Uint8Array(n)`、Swift `Data(count: n)`），空缓冲区是另一形态（Rust `Vec::new()`、Swift `Data()`）。
- 名称复用（R-L0-43 第 2 项）：旧 `bytes.new(10)` 参数数目不符而类型错误，不静默改义。
- (d) 改名/新增，`almide fix` 将 `bytes.new(n)` 改为 `bytes.zeros(n)`。
- Issue：#4012。

<a id="r-bytes-29-定数時間の比較-bytesis_equal_constant_time-を置く---採用"></a>

### R-bytes-29 增加常数时间比较 `bytes.is_equal_constant_time` — ○ 采纳

- 现状：`==` 在首个不同字节返回，用 `==` 比较 MAC/令牌可通过计时逐字节推测值。
- 提案：`bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`。长度不同立即 false（长度公开）；同长度读取全部字节，将 `a[i] xor b[i]` 做 OR，最后只与零比较一次。与 `string.is_equal_constant_time`（R-string-19）构成 {Bytes, String} 类型矩阵格（R-L0-01 例外）。
- native 为 `@intrinsic`（runtime 每次 `core::hint::black_box`）；wasm 固定循环，仅索引比较分支。`tests/constant_time_compare_structure_test.rs` 检查输出代码。
- L0 依据：R-L0-09（Bool 为 `is_`）、R-L0-30（对等双方 `a`/`b`）。
- (a) Go `subtle.ConstantTimeCompare`、Python `hmac.compare_digest`、Node `crypto.timingSafeEqual`、Java `MessageDigest.isEqual`。
- protocol（R-L0-37）：`String`/`Bytes` 同角色，因此只声明一次为 `Text` 要求（[protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) 的 `Text` 行，模块函数即要求本身）。
- 拼写（R-L0-00）：无多数（Go `ConstantTimeCompare`/`hmac.Equal`、Python `compare_digest`、Node `timingSafeEqual`、Erlang `hash_equals`、Rust `subtle` 的 `ct_eq`、Java `isEqual`）。`compare` 是 `Ord` 三值结果（R-L0-16），不用；采用 Bool `is_`（R-L0-09），直写“相等”和“常数时间”。
- (d) 新增。契约 C-423。Issue：#4106（R-hash-06）。

<a id="l3-関数表"></a>

## L3: 函数表

（146 函数 + 新增 13。判定：保留 58、改名 53、撤除 31、语义变更 4、新增 13。改名的 `put_` 15 项在 R-bytes-11 也改变越界行为）

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `append_f32_be` | `bytes.append_f32_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_f32_le` | `bytes.append_f32_le(mut b: Bytes, val: Float) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_f64_be` | `bytes.append_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_f64_le` | `bytes.append_f64_le(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_i16_be` | `bytes.append_i16_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_i16_le` | `bytes.append_i16_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_i32_be` | `bytes.append_i32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_i32_le` | `bytes.append_i32_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_i64_be` | `bytes.append_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_i64_le` | `bytes.append_i64_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_u16_be` | `bytes.append_u16_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_u16_le` | `bytes.append_u16_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_u32_be` | `bytes.append_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 保留 | — |
| `append_u32_le` | `bytes.append_u32_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 保留 | — |
| `append_u8` | `bytes.append_u8(mut b: Bytes, val: Int) -> Unit` | Rust push / Python append / Go append(b, x) | 保留 | R-bytes-25 |
| `as_mut_ptr` | `bytes.as_mut_ptr(b: Bytes) -> RawPtr` | Rust as_mut_ptr | 撤除 → `ffi.address` | R-bytes-06 |
| `as_ptr` | `bytes.as_ptr(b: Bytes) -> RawPtr` | Rust as_ptr / Go unsafe.SliceData | 撤除 → `ffi.address` | R-bytes-06 |
| `chunks` | `bytes.chunks(b: Bytes, size: Int) -> List[Bytes]` | Rust chunks / Kotlin chunked / Almide list.chunk | 改名 → `chunk` | R-bytes-08 |
| `clear` | `bytes.clear(mut b: Bytes) -> Unit` | Rust clear / Python bytearray.clear | 保留 | — |
| `cmp` | `bytes.cmp(a: Bytes, b: Bytes) -> Int` | Rust cmp / Go bytes.Compare / Gleam compare (F) / OCaml Bytes.compare (F) | 改名 → `compare` | R-bytes-08 |
| `concat` | `bytes.concat(a: Bytes, b: Bytes) -> Bytes` | Python a + b / Gleam bit_array.append (F) / OCaml Bytes.cat (F) / Go append(a, b...) | 撤除 → `+` | R-bytes-16 |
| `contains` | `bytes.contains(b: Bytes, pattern: Bytes) -> Bool` | Go bytes.Contains / Python in / Kotlin contains | 保留 | — |
| `copy_from` | `bytes.copy_from(mut dst: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` | Rust copy_from_slice / Go copy / OCaml Bytes.blit (F) | 保留 | — |
| `copy_to_ptr` | `bytes.copy_to_ptr(b: Bytes, ptr: RawPtr, cap: Int) -> Int` | Rust ptr::copy_nonoverlapping | 撤除 → `ffi.write` | R-bytes-06 |
| `data_ptr` | `bytes.data_ptr(b: Bytes) -> Int` | Go uintptr(unsafe.Pointer) | 撤除 → `ffi.address` | R-bytes-06 |
| `ends_with` | `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` | Go bytes.HasSuffix / Python endswith | 保留 | — |
| `eof` | `bytes.eof(b: Bytes, pos: Int) -> Bool` | Go io.EOF（错误值） / Python — / 多数：比较长度 | 改名 → `is_eof` | R-bytes-07 |
| `fill` | `bytes.fill(mut b: Bytes, val: Int) -> Unit` | Rust fill / JS fill / OCaml Bytes.fill (F) | 保留 | — |
| `from_list` | `bytes.from_list(xs: List[Int]) -> Bytes` | Rust Vec::from / Python bytes(list) / JS Uint8Array.from / Kotlin byteArrayOf | 保留 | — |
| `from_raw_ptr` | `bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes` | Rust slice::from_raw_parts / Go unsafe.Slice | 撤除 → `ffi.read` | R-bytes-06 |
| `from_string` | `bytes.from_string(s: String) -> Bytes` | Rust String::into_bytes / Go []byte(s) / Python s.encode() / JS TextEncoder / Gleam bit_array.from_string (F) / OCaml Bytes.of_string (F) | 保留 | R-bytes-10 |
| `get` | `bytes.get(b: Bytes, i: Int) -> Option[Int]` | Rust get → Option / Kotlin getOrNull / Swift — | 保留（兼作按位置读取 u8 格） | R-bytes-09 |
| `get_or` | `bytes.get_or(b: Bytes, i: Int, default: Int) -> Int` | Python get 无 / Kotlin getOrElse / 多数：运算符/通用函数 | 撤除 → `get(b, i) ?? d` | R-bytes-07 |
| `index_of` | `bytes.index_of(b: Bytes, pattern: Bytes) -> Option[Int]` | Go bytes.Index / Python b.find / JS indexOf | 保留 | — |
| `insert` | `bytes.insert(b: Bytes, pos: Int, val: Int) -> Bytes` | Python bytearray.insert / Rust Vec::insert（原地） | 语义变更（`0..=len` 之外 abort） | R-bytes-11 |
| `is_empty` | `bytes.is_empty(b: Bytes) -> Bool` | Rust is_empty / Kotlin isEmpty / Swift isEmpty | 保留 | — |
| `is_valid_utf8` | `bytes.is_valid_utf8(b: Bytes) -> Bool` | Go utf8.Valid / Rust str::from_utf8(..).is_ok() | 保留 | — |
| `len` | `bytes.len(b: Bytes) -> Int` | Rust len / Go len / Python len / JS length / Gleam byte_size / OCaml length | 保留 | — |
| `lines` | `bytes.lines(b: Bytes) -> List[Bytes]` | Go bufio.ScanLines / Python splitlines | 撤除（→ `string.lines(bytes.to_string(b)!)` 或 `bytes.split`） | R-bytes-17 |
| `map_each` | `bytes.map_each(b: Bytes, f: (Int) -> Int) -> Bytes` | Kotlin map / Python bytes(map(f, b)) / Almide list.map | 改名 → `map` | R-bytes-08 |
| `new` | `bytes.new(len: Int) -> Bytes` | Rust vec![0; n] / Go make([]byte, n) / Python bytes(n) / JS new Uint8Array(n) / OCaml Bytes.make (F) / Kotlin ByteArray(n) | 改名 → `zeros` | R-bytes-28 |
| `pad_left` | `bytes.pad_left(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padStart / Almide string.pad_start | 改名 → `pad_start` | R-bytes-08 |
| `pad_right` | `bytes.pad_right(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padEnd / Almide string.pad_end | 改名 → `pad_end` | R-bytes-08 |
| `push` | `bytes.push(mut b: Bytes, val: Int) -> Unit` | Rust Vec::push / Python bytearray.append / Almide list.push | 保留 | R-bytes-02 |
| `set_at` | `bytes.set_at(mut b: Bytes, i: Int, val: Int) -> Unit` | OCaml set_uint8 (F) / JS setUint8 (F) / 多数： b[i] = v | 撤除 → `put_u8`（越界 abort） | R-bytes-02、R-bytes-11 |
| `copy_within` | `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` | Rust copy_within / JS copyWithin | 保留 | — |
| `read_bool` | `bytes.read_bool(b: Bytes, pos: Int) -> Bool` | Python struct '?' / 多数： — (read_u8 != 0) | 改名 → `get_bool`（越界 none 而非 false） | R-bytes-09 |
| `read_bool_at` | `bytes.read_bool_at(b: Bytes, pos: Int) -> (Int, Option[Bool])` | — | 改名 → `next_bool`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_f16_le` | `bytes.read_f16_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint16 (F) / JS getFloat16(o, true) (F) / Python struct '<e' | 改名 → `get_f16_le`（越界 none 而非零） | R-bytes-09 |
| `read_f16_le_at` | `bytes.read_f16_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_f16_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_f16_le_array` | `bytes.read_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Ne') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f16_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_f32_be` | `bytes.read_f32_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint32 (F) / JS getFloat32(o, false) (F) / Python struct '>f' | 改名 → `get_f32_be`（越界 none 而非零） | R-bytes-09 |
| `read_f32_be_array` | `bytes.read_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nf') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_f32_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_f32_be_at` | `bytes.read_f32_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_f32_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_f32_le` | `bytes.read_f32_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint32 (F) / JS getFloat32(o, true) (F) / Python struct '<f' | 改名 → `get_f32_le`（越界 none 而非零） | R-bytes-09 |
| `read_f32_le_array` | `bytes.read_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nf') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f32_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_f32_le_at` | `bytes.read_f32_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_f32_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_f64_be` | `bytes.read_f64_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint64 (F) / JS getFloat64(o, false) (F) / Python struct '>d' | 改名 → `get_f64_be`（越界 none 而非零） | R-bytes-09 |
| `read_f64_be_array` | `bytes.read_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nd') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_f64_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_f64_be_at` | `bytes.read_f64_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_f64_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_f64_le` | `bytes.read_f64_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint64 (F) / JS getFloat64(o, true) (F) / Python struct '<d' | 改名 → `get_f64_le`（越界 none 而非零） | R-bytes-09 |
| `read_f64_le_array` | `bytes.read_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nd') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f64_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_f64_le_at` | `bytes.read_f64_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_f64_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_i16_be` | `bytes.read_i16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_int16_be (F) / JS getInt16(o, false) (F) / Python struct '>h' | 改名 → `get_i16_be`（越界 none 而非零） | R-bytes-09 |
| `read_i16_be_at` | `bytes.read_i16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_i16_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_i16_be_array` | `bytes.read_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nh') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i16_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_i16_le` | `bytes.read_i16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_int16_le (F) / JS getInt16(o, true) (F) / Python struct '<h' | 改名 → `get_i16_le`（越界 none 而非零） | R-bytes-09 |
| `read_i16_le_at` | `bytes.read_i16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_i16_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_i16_le_array` | `bytes.read_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nh') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i16_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_i32_be` | `bytes.read_i32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getInt32(o, false) (F) / Python struct '>i' | 改名 → `get_i32_be`（越界 none 而非零） | R-bytes-09 |
| `read_i32_be_array` | `bytes.read_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Ni') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i32_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_i32_be_at` | `bytes.read_i32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_i32_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_i32_le` | `bytes.read_i32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getInt32(o, true) (F) / Python struct '<i' | 改名 → `get_i32_le`（越界 none 而非零） | R-bytes-09 |
| `read_i32_le_array` | `bytes.read_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Ni') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i32_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_i32_le_at` | `bytes.read_i32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_i32_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_i64_be` | `bytes.read_i64_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint64 (F) / OCaml get_int64_be (F) / JS getBigInt64(o, false) (F) / Python struct '>q' | 改名 → `get_i64_be`（越界 none 而非零） | R-bytes-09 |
| `read_i64_be_array` | `bytes.read_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nq') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i64_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_i64_be_at` | `bytes.read_i64_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_i64_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_i64_le` | `bytes.read_i64_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint64 (F) / OCaml get_int64_le (F) / JS getBigInt64(o, true) (F) / Python struct '<q' | 改名 → `get_i64_le`（越界 none 而非零） | R-bytes-09 |
| `read_i64_le_array` | `bytes.read_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nq') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i64_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_i64_le_at` | `bytes.read_i64_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_i64_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_length_prefixed_strings_le` | `bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]` | —（格式专用） | 撤除（→ `get_u32_le` 与 `slice` 组合） | R-bytes-05 |
| `read_string_at` | `bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String` | Python b[p:p+n].decode / Rust from_utf8_lossy(&b[p..p+n]) | 撤除（→ `slice` ＋ `to_string_lossy`） | R-bytes-05 |
| `read_string_be` | `bytes.read_string_be(b: Bytes, pos: Int) -> String` | Java DataInput.readUTF（u16 BE） / 多数： — | 撤除（→ `get_u32_be` 与 `slice` 组合） | R-bytes-05 |
| `read_string_be_at` | `bytes.read_string_be_at(b: Bytes, pos: Int) -> (Int, Option[String])` | — | 撤除（→ `next_u32_be` 与 `next_bytes` 组合） | R-bytes-05 |
| `read_u16_be` | `bytes.read_u16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_uint16_be (F) / JS getUint16(o, false) (F) / Python struct '>H' | 改名 → `get_u16_be`（越界 none 而非零） | R-bytes-09 |
| `read_u16_be_at` | `bytes.read_u16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_u16_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_u16_be_array` | `bytes.read_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NH') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_u16_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_u16_le` | `bytes.read_u16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_uint16_le (F) / JS getUint16(o, true) (F) / Python struct '<H' | 改名 → `get_u16_le`（越界 none 而非零） | R-bytes-09 |
| `read_u16_le_array` | `bytes.read_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NH') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_u16_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_u16_le_at` | `bytes.read_u16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_u16_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_u32_be` | `bytes.read_u32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getUint32(o, false) (F) / Python struct '>I' | 改名 → `get_u32_be`（越界 none 而非零） | R-bytes-09 |
| `read_u32_be_array` | `bytes.read_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NI') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_u32_be_array`（越界 none 而非零） | R-bytes-09 |
| `read_u32_be_at` | `bytes.read_u32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_u32_be`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_u32_le` | `bytes.read_u32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getUint32(o, true) (F) / Python struct '<I' | 改名 → `get_u32_le`（越界 none 而非零） | R-bytes-09 |
| `read_u32_le_array` | `bytes.read_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NI') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_u32_le_array`（越界 none 而非零） | R-bytes-09 |
| `read_u32_le_at` | `bytes.read_u32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型；Go bytes.Reader + binary.Read) | 改名 → `next_u32_le`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `read_u8` | `bytes.read_u8(b: Bytes, pos: Int) -> Int` | OCaml get_uint8 (F) / JS getUint8 (F) / 多数： b[i] | 撤除 → `get`（同角色，越界 none） | R-bytes-09 |
| `read_u8_at` | `bytes.read_u8_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数：持有位置的 reader 类型) | 改名 → `next_u8`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `remove_at` | `bytes.remove_at(b: Bytes, pos: Int) -> Bytes` | Rust Vec::remove / Python del b[i]（原地） | 语义变更（越界 abort） | R-bytes-11 |
| `repeat` | `bytes.repeat(b: Bytes, n: Int) -> Bytes` | Go bytes.Repeat / Python b * n | 保留 | — |
| `reverse` | `bytes.reverse(b: Bytes) -> Bytes` | Rust reverse / Kotlin reversed | 保留 | — |
| `set` | `bytes.set(b: Bytes, i: Int, val: Int) -> Bytes` | Kotlin copyOf + set / 多数： 下标赋值（原地） | 语义变更（越界 abort） | R-bytes-11 |
| `set_f32_be` | `bytes.set_f32_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat32 (F) | 改名 → `put_f32_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_f32_le` | `bytes.set_f32_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat32 (F) | 改名 → `put_f32_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_f64_be` | `bytes.set_f64_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat64 (F) | 改名 → `put_f64_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_f64_le` | `bytes.set_f64_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat64 (F) | 改名 → `put_f64_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_i16_be` | `bytes.set_i16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setInt16 (F) | 改名 → `put_i16_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_i16_le` | `bytes.set_i16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setInt16 (F) | 改名 → `put_i16_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_i32_be` | `bytes.set_i32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setInt32 (F) | 改名 → `put_i32_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_i32_le` | `bytes.set_i32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setInt32 (F) | 改名 → `put_i32_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_i64_be` | `bytes.set_i64_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setBigInt64 (F) | 改名 → `put_i64_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_i64_le` | `bytes.set_i64_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setBigInt64 (F) | 改名 → `put_i64_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_u16_be` | `bytes.set_u16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setUint16 (F) | 改名 → `put_u16_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_u16_le` | `bytes.set_u16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setUint16 (F) | 改名 → `put_u16_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_u32_be` | `bytes.set_u32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setUint32 (F) | 改名 → `put_u32_be`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_u32_le` | `bytes.set_u32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setUint32 (F) | 改名 → `put_u32_le`（越界 abort） | R-bytes-01、R-bytes-11 |
| `set_u8` | `bytes.set_u8(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go b[i] = x / OCaml set_*_le・_be (F) / JS DataView setUint8 (F) | 改名 → `put_u8`（越界 abort） | R-bytes-01、R-bytes-11 |
| `skip` | `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | Go bytes.Reader.Seek / Python — | 语义变更（`n < 0` abort） | R-bytes-13、R-bytes-21 |
| `skip_length_prefixed_le` | `bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int` | —（格式专用） | 撤除（→ `get_u32_le` 与 `slice` 组合） | R-bytes-05 |
| `slice` | `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | Rust &b[a..b] / Go b[a:b] / Python b[a:b] / JS subarray・slice / OCaml Bytes.sub (F) | 语义变更（负位置 abort，R-L0-29） | R-bytes-12、R-L0-29 |
| `split` | `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` | Go bytes.Split / Python b.split | 保留 | — |
| `starts_with` | `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` | Go bytes.HasPrefix / Python startswith / Gleam starts_with (F) | 保留 | — |
| `take_at` | `bytes.take_at(b: Bytes, pos: Int, n: Int) -> (Int, Option[Bytes])` | Go bytes.Reader.Read / Python b[p:p+n] | 改名 → `next_bytes`（`(値, 次の位置)` 返回） | R-bytes-13 |
| `to_list` | `bytes.to_list(b: Bytes) -> List[Int]` | Python list(b) / JS Array.from / Kotlin toList | 保留 | — |
| `to_string` | `bytes.to_string(b: Bytes) -> Result[String, String]` | Rust String::from_utf8 → Result / Gleam bit_array.to_string → Result (F) / Python b.decode() / Kotlin decodeToString | 保留 | R-bytes-10 |
| `to_string_lossy` | `bytes.to_string_lossy(b: Bytes) -> String` | Rust String::from_utf8_lossy / Python decode(errors='replace') | 保留 | R-bytes-10 |
| `write_bool` | `bytes.write_bool(mut b: Bytes, val: Bool) -> Unit` | Python struct.pack('?') / 多数： — | 改名 → `append_bool` | R-bytes-03 |
| `write_f64_be` | `bytes.write_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64(math.Float64bits) (F) | 撤除 → `append_f64_be` | R-bytes-03 |
| `write_i64_be` | `bytes.write_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) | 撤除 → `append_i64_be` | R-bytes-03 |
| `write_string_be` | `bytes.write_string_be(mut b: Bytes, s: String) -> Unit` | Java DataOutput.writeUTF（u16 BE） / 多数： — | 撤除（→ `append_u32_be` ＋ `append_bytes`） | R-bytes-05 |
| `write_u32_be` | `bytes.write_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) | 撤除 → `append_u32_be` | R-bytes-03 |
| `write_u8` | `bytes.write_u8(mut b: Bytes, val: Int) -> Unit` | 同上 | 撤除 → `push` | R-bytes-02 |
| `xor` | `bytes.xor(a: Bytes, b: Bytes) -> Bytes` | — (多数： 编写循环) | 语义变更（长度不同 abort） | R-bytes-23 |
| `read_uint16` | `bytes.read_uint16(b: Bytes, offset: Int, endian: Endian) -> UInt16` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `read_uint32` | `bytes.read_uint32(b: Bytes, offset: Int, endian: Endian) -> UInt32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `read_int32` | `bytes.read_int32(b: Bytes, offset: Int, endian: Endian) -> Int32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `read_float32` | `bytes.read_float32(b: Bytes, offset: Int, endian: Endian) -> Float32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_uint16` | `bytes.write_uint16(mut b: Bytes, value: UInt16, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_uint32` | `bytes.write_uint32(mut b: Bytes, value: UInt32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_int32` | `bytes.write_int32(mut b: Bytes, value: Int32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_float32` | `bytes.write_float32(mut b: Bytes, value: Float32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_uint16` | `bytes.set_uint16(mut b: Bytes, offset: Int, value: UInt16, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_uint32` | `bytes.set_uint32(mut b: Bytes, offset: Int, value: UInt32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_int32` | `bytes.set_int32(mut b: Bytes, offset: Int, value: Int32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_float32` | `bytes.set_float32(mut b: Bytes, offset: Int, value: Float32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤除（→ 后缀族 `get_`/`append_`/`put_`） | R-bytes-04 |
| `put_f16_le` | `bytes.put_f16_le(mut b: Bytes, pos: Int, val: Float) -> Unit`（提案） | JS setFloat16 (F) / Python struct '<e' | 新增 | R-bytes-03 |
| `put_f16_be` | `bytes.put_f16_be(mut b: Bytes, pos: Int, val: Float) -> Unit`（提案） | JS setFloat16 (F) / Python struct '>e' | 新增 | R-bytes-03 |
| `append_f16_le` | `bytes.append_f16_le(mut b: Bytes, val: Float) -> Unit`（提案） | Python struct.pack('<e') | 新增 | R-bytes-03 |
| `append_f16_be` | `bytes.append_f16_be(mut b: Bytes, val: Float) -> Unit`（提案） | Python struct.pack('>e') | 新增 | R-bytes-03 |
| `get_f16_be` | `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]`（提案） | JS getFloat16(o, false) (F) / Python struct '>e' | 新增 | R-bytes-03 |
| `next_f16_be` | `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)`（提案） | — | 新增 | R-bytes-03、R-bytes-13 |
| `get_f16_be_array` | `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`（提案） | Python struct.unpack_from('>Ne') | 新增 | R-bytes-03 |
| `put_bool` | `bytes.put_bool(mut b: Bytes, pos: Int, val: Bool) -> Unit`（提案） | Python struct.pack_into('?') | 新增（越界 abort） | R-bytes-03、R-bytes-11 |
| `append_bytes` | `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit`（提案） | Rust extend_from_slice / Go append(b, src...) / Python bytearray.extend / OCaml Buffer.add_bytes | 新增 | R-bytes-05 |
| `take` | `bytes.take(b: Bytes, n: Int) -> Bytes`（提案） | Rust &b[..n] / Python b[:n] / Kotlin take / Almide list.take | 新增（负数 abort，R-L0-29） | R-bytes-08 |
| `drop` | `bytes.drop(b: Bytes, n: Int) -> Bytes`（提案） | Rust &b[n..] / Python b[n:] / Kotlin drop / Almide list.drop | 新增（负数 abort，R-L0-29） | R-bytes-08 |
| `take_end` | `bytes.take_end(b: Bytes, n: Int) -> Bytes`（提案） | Python b[-n:] / Kotlin takeLast / Almide list.take_end | 新增（负数 abort，R-L0-29） | R-bytes-08 |
| `drop_end` | `bytes.drop_end(b: Bytes, n: Int) -> Bytes`（提案） | Python b[:-n] / Kotlin dropLast / Almide list.drop_end | 新增（负数 abort，R-L0-29） | R-bytes-08 |
| `heap_save` | `bytes.heap_save() -> Int` | — | 撤除（改为内部名称） | R-bytes-14 |
| `heap_restore` | `bytes.heap_restore(checkpoint: Int) -> Unit` | — | 撤除（改为内部名称） | R-bytes-14 |
| `get_i8` | `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]`（提案） | OCaml `get_int8` (F) / JS `getInt8` | 新增 | R-bytes-15 |
| `next_i8` | `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)`（提案） | — | 新增 | R-bytes-15、R-bytes-13 |
| `put_i8` | `bytes.put_i8(mut b: Bytes, pos: Int, val: Int) -> Unit`（提案） | OCaml `set_int8` / JS `setInt8` | 新增 | R-bytes-15 |
| `append_i8` | `bytes.append_i8(mut b: Bytes, val: Int) -> Unit`（提案） | — | 新增 | R-bytes-15 |
| `filter` | `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`（提案） | Kotlin `filter` / Rust `iter().filter` | 新增 | R-bytes-18 |
| `fold` | `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B`（提案） | Kotlin `fold` / Rust `iter().fold` | 新增 | R-bytes-18 |
| `any` | `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool`（提案） | Kotlin `any` | 新增 | R-bytes-18 |
| `all` | `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool`（提案） | Kotlin `all` | 新增 | R-bytes-18 |
| `count` | `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int`（提案） | Kotlin `count {}` | 新增 | R-bytes-18 |
| `find` | `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]`（提案） | Kotlin `find` | 新增 | R-bytes-18 |
| `first` | （新增）`bytes.first(b: Bytes) -> Option[Int]` | Rust first / Swift first | 新增 | R-bytes-20 |
| `last` | （新增）`bytes.last(b: Bytes) -> Option[Int]` | Rust last / Swift last | 新增 | R-bytes-20 |
| `pop` | （新增）`bytes.pop(mut b: Bytes) -> Option[Int]` | Rust Vec::pop / Python bytearray.pop | 新增 | R-bytes-20 |
| `get_u8` | （新增）`bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]` | Go `LittleEndian.Uint8` / JS `getUint8` / OCaml `get_uint8` | 新增 | R-bytes-25 |
| `enumerate` | （新增）`bytes.enumerate(b: Bytes) -> List[(Int, Int)]` | Swift `enumerated` / Rust `enumerate` | 新增 | R-L0-42 |
| `zip` | （新增）`bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` | Swift `zip` / Rust `zip` | 新增 | R-L0-42 |
| `zip_with` | （新增）`bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 新增 | R-L0-42 |
| `filter_map` | （新增）`bytes.filter_map(b: Bytes, f: (Int) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 新增 | R-L0-42 |
| `find_map` | （新增）`bytes.find_map(b: Bytes, f: (Int) -> B?) -> B?` | Rust `find_map` | 新增 | R-L0-42 |
| `reduce` | （新增）`bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Int?` | Swift `reduce` / Rust `reduce` | 新增 | R-L0-42 |
| `scan` | （新增）`bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 新增 | R-L0-42 |
| `partition` | （新增）`bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` | Kotlin `partition` | 新增 | R-L0-42 |
| `group_by` | （新增）`bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 新增 | R-L0-42 |
| `sort` | （新增）`bytes.sort(b: Bytes) -> List[Int]` | Swift `sorted()` | 新增 | R-L0-42 |
| `sort_by` | （新增）`bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` | Kotlin `sortedBy` | 新增 | R-L0-42 |
| `sort_with` | （新增）`bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` | Swift `sorted(by:)` | 新增 | R-L0-42 |
| `min` | （新增）`bytes.min(b: Bytes) -> Int?` | Swift `min()` / Rust `min` | 新增 | R-L0-42 |
| `max` | （新增）`bytes.max(b: Bytes) -> Int?` | Swift `max()` / Rust `max` | 新增 | R-L0-42 |
| `min_by` | （新增）`bytes.min_by(b: Bytes, f: (Int) -> K) -> Int?` | Kotlin `minByOrNull` | 新增 | R-L0-42 |
| `max_by` | （新增）`bytes.max_by(b: Bytes, f: (Int) -> K) -> Int?` | Kotlin `maxByOrNull` | 新增 | R-L0-42 |
| `min_with` | （新增）`bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | Swift `min(by:)` | 新增 | R-L0-42 |
| `max_with` | （新增）`bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | Swift `max(by:)` | 新增 | R-L0-42 |
| `sum` | （新增）`bytes.sum(b: Bytes) -> Int` | Rust `sum` / Kotlin `sum` | 新增 | R-L0-42 |
| `product` | （新增）`bytes.product(b: Bytes) -> Int` | Rust `product` | 新增 | R-L0-42 |
| `find_index` | （新增）`bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Int?` | Swift `firstIndex(where:)` | 新增 | R-L0-42 |
| `take_while` | （新增）`bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | Swift `prefix(while:)` | 新增 | R-L0-42 |
| `drop_while` | （新增）`bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | Swift `drop(while:)` | 新增 | R-L0-42 |
| `window` | （新增）`bytes.window(b: Bytes, n: Int) -> List[Bytes]` | Kotlin `windowed` | 新增 | R-L0-42 |
| `split_at` | （新增）`bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` | Rust `split_at` | 新增 | R-L0-42 |
| `dedup` | （新增）`bytes.dedup(b: Bytes) -> Bytes` | Rust `dedup` | 新增 | R-L0-42 |
| `intersperse` | （新增）`bytes.intersperse(b: Bytes, sep: Int) -> Bytes` | Rust `intersperse` | 新增 | R-L0-42 |
| `unique` | （新增）`bytes.unique(b: Bytes) -> Bytes` | Kotlin `distinct` | 新增 | R-L0-42 |
| `unique_by` | （新增）`bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` | Kotlin `distinctBy` | 新增 | R-L0-42 |
| `last_index_of` | （新增）`bytes.last_index_of(b: Bytes, needle: Bytes) -> Int?` | Go `bytes.LastIndex` / Python `rfind` | 新增 | R-bytes-27 |
| `replace` | （新增）`bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | Go `bytes.ReplaceAll` / Python `replace` | 新增 | R-bytes-27 |
| `replace_first` | （新增）`bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | Go `bytes.Replace(…, 1)` / Python `replace(…, 1)` | 新增 | R-bytes-27 |
| `strip_prefix` | （新增）`bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Bytes?` | Rust `strip_prefix` / Go `CutPrefix` | 新增 | R-bytes-27 |
| `strip_suffix` | （新增）`bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Bytes?` | Rust `strip_suffix` / Go `CutSuffix` | 新增 | R-bytes-27 |
| `count_matches` | （新增）`bytes.count_matches(b: Bytes, needle: Bytes) -> Int` | Go `bytes.Count` / Python `count` | 新增 | R-bytes-27 |
| `new` | （新增）`bytes.new() -> Bytes` | Rust `Vec::new` / Swift `Data()` / Go `[]byte{}` | 新增 | R-bytes-28 |
| `is_equal_constant_time` | `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`（提案） | Go `subtle.ConstantTimeCompare` / Python `hmac.compare_digest` / Node `crypto.timingSafeEqual` | 新增 | R-bytes-29 |

<a id="l0-への差し戻し処理済み"></a>

## 向 L0 回报（已解决）

1. **R-L0-01 与矩阵格。** R-L0-01 已加入：类型矩阵格即便 runtime 本体相同也不视为同义词，前提是矩阵形态已声明且受 gate 保护。保留 `append_i16_le` 与 `append_u16_le`。#3851 F 同义词 gate 排除由矩阵声明（R-bytes-03）生成的格，而非按名称枚举排除。
2. **越界位置写入。** L0 已加入“按位置写入越界视为违约而 abort”（R-L0-12 后）。R-bytes-11 使 `put_<型>`、`set`、`insert`、`remove_at` 统一 abort。
