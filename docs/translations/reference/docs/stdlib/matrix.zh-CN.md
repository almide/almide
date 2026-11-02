# matrix

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](matrix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](matrix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](matrix.zh-CN.md)



> 原文：[matrix.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/matrix.md)。这是面向读者的非规范性固定快照，不参与自动生成或自动同步。签名、类型、`@since` 及代码（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行或验证示例、性能、安全性或跨目标一致性。

> 形状、空值、符号及范围规则均为固定原文的主张。原文称 slice_rows(m, 5, 9) 为空，但未写明 m 的行数条件；不据此另作普遍空值保证。也保留 split_cols_even 正文 n <= 0 与索引 n = 0 的说明差异。

二维矩阵操作。自动导入；`import matrix` 可省略。

> **方言纪元14：** 神经网络内核（attention、RoPE、行归一化、激活函数、Q1_0/Q8_0 加载器和 Qwen3 块，共25个函数）移至包 [almide-graphics/nn](https://github.com/almide-graphics/nn)（模块 `nn.kernels`）：执行 `almide add almide-graphics/nn@v0.2.0 --trust github.com/almide-graphics`，使用 `import nn.kernels`，然后用相同参数的 `kernels.gelu(m)` 代替 `matrix.gelu(m)`。调用旧写法会在检查时产生指出新名称的错误。
>
> **方言纪元15：** 算术函数成为运算符（`add` 为 `a + b`；`sub`、`mul`、`div` 同理）；稠密内核 `linear_row`、`linear_row_no_bias`、`linear_f32_row_no_bias`、`conv1d` 和 `select_rows_f32` 移至 almide-graphics/nn；`_f32` 乘积融合操作用 `*` 配合 `matrix.transpose` / `matrix.scale` 表示。

`Matrix` 是行优先、稠密、以 `Float` 为值的矩阵（`f64`）。所有操作都将值视为不可变，组合操作返回新矩阵。

## 命名约定

- `<op>_rows`：独立操作每行并返回 Matrix（`slice_rows`、`gather_rows`、`broadcast_add_row`）。
- `<op>_cols`：按列版本（`split_cols_even`、`concat_cols`）。
- `<verb>_<noun>`：第一个词表示函数所做的事（`from_lists`、`to_bytes_f64_le`、`dot_row`）。
- 字节转换的后缀 `_<dtype>_le` 显式指定字节序。

## 构造

| 签名 | 用途 |
|---|---|
| `matrix.zeros(rows: Int, cols: Int) -> Matrix` | 填零 |
| `matrix.ones(rows: Int, cols: Int) -> Matrix` | 填1 |
| `matrix.from_lists(rows: List[List[Float]]) -> Matrix` | 从嵌套列表构造 |
| `matrix.from_bytes_f64_le(b, offset, rows, cols) -> Option[Matrix]` | 读取平坦 f64 LE 字节；数据容纳不下时为 `none` |
| `matrix.from_bytes_f32_le(b, offset, rows, cols) -> Option[Matrix]` | 读取平坦 f32 LE（提升为 f64）；数据容纳不下时为 `none` |
| `matrix.from_bytes_f16_le(b, offset, rows, cols) -> Option[Matrix]` | 读取平坦 IEEE-754 半精度（GGML 权重）；数据容纳不下时为 `none` |

## 转换

| 签名 | 用途 |
|---|---|
| `matrix.to_lists(m) -> List[List[Float]]` | 实体化为嵌套列表 |
| `matrix.to_bytes_f64_le(m) -> Bytes` | 平坦 f64 LE（与 `from_bytes_f64_le` 对称） |
| `matrix.to_bytes_f32_le(m) -> Bytes` | 平坦 f32 LE（逐单元降低精度） |
| `matrix.transpose(m) -> Matrix` | 转置 |

## 检查

| 签名 | 用途 |
|---|---|
| `matrix.shape(m) -> (Int, Int)` | `(rows, cols)` |
| `matrix.rows(m) -> Int` | 行数 |
| `matrix.cols(m) -> Int` | 列数 |
| `matrix.get(m, r, c) -> Option[Float]` | 元素访问；越界为 `none`（C-282） |

## 算术

除非另有说明，均为逐元素操作。`a * b` 是矩阵乘积，要求 `cols(a) == rows(b)`（见下方**形状与定义域**）；逐元素运算符（`+` / `-` / `/` 和 `broadcast_add_row`）则 zip 截断到较短操作数，两个目标行为相同。

| 表达式 | 用途 |
|---|---|
| `a + b` | 逐元素求和 |
| `a - b` | 逐元素求差 |
| `a * b` | **矩阵乘法**（非逐元素） |
| `a / b` | 逐元素求商 |
| `matrix.scale(m, s) -> Matrix` | 每个单元乘 `s`；`matrix.scale(m, -1.0)` 取负 |
| `matrix.map(m, f) -> Matrix` | 对每个单元应用 `(Float) -> Float`，如 `(x) => float.pow(x, e)` |
| `matrix.concat_rows(ms: List[Matrix]) -> Matrix` | 每个成员的行，从上到下 |

## 切片与组装

| 签名 | 用途 |
|---|---|
| `matrix.slice_rows(m, start, end) -> Matrix` | 半开行范围 |
| `matrix.gather_rows(m, indices: List[Int]) -> Matrix` | 按索引列表选行（如词元嵌入查找） |
| `matrix.split_cols_even(m, n) -> List[Matrix]` | 将列分成 `n` 个等宽块 |
| `matrix.concat_cols(matrices: List[Matrix]) -> Matrix` | 按列拼接（已弃用别名：`concat_cols_many`） |
| `matrix.dot_row(m, r, vec: List[Float]) -> Float` | 行 `r` 与 `vec` 的点积（已弃用别名：`row_dot`） |

## 神经网络基本操作

| 签名 | 用途 |
|---|---|
| `matrix.broadcast_add_row(m, row: List[Float]) -> Matrix` | 将偏置列表加到 m 的每行（zip 截断） |

稠密内核（`linear_row`、`conv1d` 等）位于包 almide-graphics/nn 的 nn.kernels 模块。

## 形状与定义域

**违反形状前置条件会产生定义明确的中止。** 以内核一个操作数的范围索引另一个操作数时，要求相应范围相等。违反时向 stderr 输出 `Error: matrix shape mismatch` 并以退出码1停止程序；原生和两个 wasm 通道均如此，绝不 panic，也不返回截断答案（C-353）：

| 调用 | 要求 |
|---|---|
| `a * b` | `cols(a) == rows(b)` |
| `concat_cols(ms)` | 每个非空成员的 `rows` 与第一个成员相同 |
| `concat_rows(ms)` | 每个非空成员的 `cols` 相等 |
| `from_lists(rows)` | 每行与第一行等宽；参差列表以 `Error: matrix rows must have equal length` 中止 |

**空**操作数豁免：这些操作返回其退化形状，而不发生中止（`m * zeros(0, 0)` 是 `rows(m)×0` 矩阵，`concat_cols([m, zeros(0, 0)])` 是 `m`）。

**没有行的矩阵也没有列**（C-161）。Matrix 是等宽行的列表，其宽度取自行0。因此任何0行矩阵均为0 × 0：无论生成时宽度是多少，`shape` 返回 `(0, 0)`，`cols` 返回 `0`。`zeros(0, 3)`、`from_bytes_f32_le(b, 0, 0, 3)!`、`gather_rows(m, [])` 和 `slice_rows(m, 5, 9)` 在两个目标上均为0 × 0。这使 `from_lists(to_lists(m))` 保持形状：无行矩阵的 `to_lists` 为 `[]`，`from_lists([])` 为0 × 0。反向并不成立：r × 0 矩阵保留 r 行（`zeros(3, 0)` 是 `(3, 0)`，`to_lists` 给出三个空行）。

**行范围和计数会限幅**（C-354），如同 `list.slice`：`slice_rows(m, start, end)` 将两个边界限制在 `0..rows(m)` 内（负边界为0，即 #4147 的负计数规则），`start >= end` 为空；`split_cols_even(m, n)` 在 `n <= 0` 时返回空列表。

**取负遵循 IEEE**（C-355）：`-m` 翻转符号位，因此 `0.0` 单元变为 `-0.0`，两个目标上 `1.0` 除以它都得到 `-inf`。

## 字节解码器

从字节缓冲区读取行是解码与收集的组合：`matrix.gather_rows(matrix.from_bytes_f32_le(b, offset, rows, cols)!, ids)`（Q1_0 / Q8_0 量化加载器在方言纪元14移至包 almide-graphics/nn 的 nn.kernels 模块，`select_rows_f32` 在纪元15移入）。

`from_bytes_f16_le` / `_f32_le` / `_f64_le` **解码器**保留存储的 `-0.0`：这里符号本身就是数据，因此保留（C-269）。先将负维度限制为0，超出上限的计数以 `Error: matrix dimensions too large` 中止（C-161）；在此之后，除非 `0 <= offset <= len(b)` 且 `rows * cols * width <= len(b) - offset`，否则返回 `none`（C-341）。


## 原文固定快照 API 索引（27个函数）

```
// rows x cols of 0.0; negatives clamp to 0.
// @since 0.9.3 or earlier
matrix.zeros(rows: Int, cols: Int) -> Matrix

// rows x cols of 1.0; negatives clamp to 0.
// @since 0.9.3 or earlier
matrix.ones(rows: Int, cols: Int) -> Matrix

// (rows, cols); (0, 0) with no rows.
// @since 0.9.3 or earlier
matrix.shape(m: Matrix) -> (Int, Int)

// cols x rows copy; r x 0 becomes 0 x 0.
// @since 0.9.3 or earlier
matrix.transpose(m: Matrix) -> Matrix

// Matrix of the equal-length rows xs; [] is 0 x 0.
// @since 0.9.3 or earlier
matrix.from_lists(xs: List[List[Float]]) -> Matrix

// Rows as lists; r x 0 gives r empty rows.
// @since 0.9.3 or earlier
matrix.to_lists(m: Matrix) -> List[List[Float]]

// Element at (row, col); none out of range.
// @since 0.9.3 or earlier
matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]

// Row count; 0 for an empty matrix.
// @since 0.9.3 or earlier
matrix.rows(m: Matrix) -> Int

// Width of row 0; 0 when there are no rows.
// @since 0.9.3 or earlier
matrix.cols(m: Matrix) -> Int

// Every element times s.
// @since 0.9.3 or earlier
matrix.scale(m: Matrix, s: Float) -> Matrix

// f applied to every element.
// @since 0.9.3 or earlier
matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix

// f32 LE rows x cols of b at offset; none if b is short.
// @since 0.13.4 or earlier
matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]

// Row-major f64 LE, 8 bytes per element.
// @since 0.13.4 or earlier
matrix.to_bytes_f64_le(m: Matrix) -> Bytes

// Row-major f32 LE, 4 bytes each (rounded).
// @since 0.13.4 or earlier
matrix.to_bytes_f32_le(m: Matrix) -> Bytes

// Row-major f16 LE, 2 bytes each (rounded to nearest even; overflow is inf).
// @since unreleased
matrix.to_bytes_f16_le(m: Matrix) -> Bytes

// f16 LE rows x cols of b at offset; none if b is short.
// @since 0.13.4 or earlier
matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]

// f64 LE rows x cols of b at offset; none if b is short.
// @since 0.13.4 or earlier
matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]

// bias added to each row; cols cut to bias.
// @since 0.13.4 or earlier
matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix

// n slices cols/n wide, remainder dropped; [] if n = 0.
// @since 0.13.4 or earlier
matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]

// Side-by-side join; [] gives 0 x 0.
// @since 0.13.4 or earlier
matrix.concat_cols(xs: List[Matrix]) -> Matrix

// Alias of matrix.concat_cols.
// @since 0.13.4 or earlier
matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix   (deprecated — use matrix.concat_cols)

// Rows [start, end), each bound clamped to 0..row count.
// @since 0.13.4 or earlier
matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix

// Rows at indices; out of range is a zero row.
// @since 0.13.4 or earlier
matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix

// Row r dot vec; 0.0 if r is out of range.
// @since 0.13.4 or earlier
matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float

// Alias of matrix.dot_row.
// @since 0.13.4 or earlier
matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float   (deprecated — use matrix.dot_row)

// f over each pair of same-position elements; a shape mismatch aborts.
// @since unreleased
matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix

// Rows of every matrix, top to bottom; empty members skipped, a width mismatch aborts.
// @since unreleased
matrix.concat_rows(xs: List[Matrix]) -> Matrix
```

## API 说明译文（按上述签名顺序）

- `matrix.zeros(rows: Int, cols: Int) -> Matrix`：rows × cols 个0.0；负值限为0。
- `matrix.ones(rows: Int, cols: Int) -> Matrix`：rows × cols 个1.0；负值限为0。
- `matrix.shape(m: Matrix) -> (Int, Int)`：(rows, cols)；无行时为 (0, 0)。
- `matrix.transpose(m: Matrix) -> Matrix`：cols × rows 的副本；r × 0 变成0 × 0。
- `matrix.from_lists(xs: List[List[Float]]) -> Matrix`：由等长行 xs 构成的矩阵；[] 为0 × 0。
- `matrix.to_lists(m: Matrix) -> List[List[Float]]`：行作为列表；r × 0 给出 r 个空行。
- `matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]`：(row, col) 处元素；越界为 none。
- `matrix.rows(m: Matrix) -> Int`：行数；空矩阵为0。
- `matrix.cols(m: Matrix) -> Int`：行0的宽度；无行时为0。
- `matrix.scale(m: Matrix, s: Float) -> Matrix`：每个元素乘 s。
- `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix`：对每个元素应用 f。
- `matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]`：从 b 的 offset 起读取 f32 LE 的 rows × cols；b 不足时为 none。
- `matrix.to_bytes_f64_le(m: Matrix) -> Bytes`：行优先 f64 LE，每元素8字节。
- `matrix.to_bytes_f32_le(m: Matrix) -> Bytes`：行优先 f32 LE，每元素4字节（舍入）。
- `matrix.to_bytes_f16_le(m: Matrix) -> Bytes`：行优先 f16 LE，每元素2字节（最近偶数舍入；溢出为 inf）。
- `matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]`：从 b 的 offset 起读取 f16 LE 的 rows × cols；b 不足时为 none。
- `matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]`：从 b 的 offset 起读取 f64 LE 的 rows × cols；b 不足时为 none。
- `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix`：对每行加 bias，列截断到 bias 的长度。
- `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]`：n 个宽为 cols/n 的切片，丢弃余数；n = 0 时为 []。
- `matrix.concat_cols(xs: List[Matrix]) -> Matrix`：并排连接；[] 得到0 × 0。
- `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix   (deprecated — use matrix.concat_cols)`：matrix.concat_cols 的别名。
- `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix`：行 [start, end)，每个边界限制在0..行数内。
- `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix`：indices 处的行；越界为全零行。
- `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float`：行 r 与 vec 的点积；r 越界为0.0。
- `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float   (deprecated — use matrix.dot_row)`：matrix.dot_row 的别名。
- `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix`：对相同位置的各元素对应用 f；形状不匹配时中止。
- `matrix.concat_rows(xs: List[Matrix]) -> Matrix`：各矩阵的行从上到下排列；跳过空成员，宽度不匹配时中止。
