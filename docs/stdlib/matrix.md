# matrix

2D matrix operations. `import matrix`.

> **Dialect epoch 14:** the neural-network kernels (attention, RoPE, the row
> norms, the activations, the Q1_0/Q8_0 loaders and the Qwen3 blocks — 25
> functions) moved to package [almide/nnlib](https://github.com/almide/nnlib):
> `almide add almide/nnlib@v0.1.0`, `import nnlib`, then `nnlib.gelu(m)` for
> `matrix.gelu(m)`, with the same arguments. Calling the old spelling is a
> check-time error that names the new one.

`Matrix` is a row-major, dense, `Float`-valued matrix (`f64`). All operations
treat the value as immutable; combinators return fresh matrices.

## Naming convention

- `<op>_rows` — operates on every row independently and returns a Matrix
  (`slice_rows`, `gather_rows`, `broadcast_add_row`, `linear_row`).
- `<op>_cols` — column-wise variant (`split_cols_even`, `concat_cols`).
- `<verb>_<noun>` — first word is what the function does
  (`from_lists`, `to_bytes_f64_le`, `dot_row`).
- Suffix `_<dtype>_le` for explicit endianness on byte conversion.

## Constructors

| Signature | Purpose |
|---|---|
| `matrix.zeros(rows: Int, cols: Int) -> Matrix` | Zero-filled |
| `matrix.ones(rows: Int, cols: Int) -> Matrix` | One-filled |
| `matrix.from_lists(rows: List[List[Float]]) -> Matrix` | From nested lists |
| `matrix.from_bytes_f64_le(data, offset, rows, cols) -> Matrix` | Read flat f64 LE bytes |
| `matrix.from_bytes_f32_le(data, offset, rows, cols) -> Matrix` | Read flat f32 LE (promoted to f64) |
| `matrix.from_bytes_f16_le(data, offset, rows, cols) -> Matrix` | Read flat IEEE-754 half (GGML weights) |

## Conversions

| Signature | Purpose |
|---|---|
| `matrix.to_lists(m) -> List[List[Float]]` | Materialise as nested lists |
| `matrix.to_bytes_f64_le(m) -> Bytes` | Flat f64 LE (symmetric to `from_bytes_f64_le`) |
| `matrix.to_bytes_f32_le(m) -> Bytes` | Flat f32 LE (each cell demoted) |
| `matrix.transpose(m) -> Matrix` | Transpose |

## Inspection

| Signature | Purpose |
|---|---|
| `matrix.shape(m) -> (Int, Int)` | `(rows, cols)` |
| `matrix.rows(m) -> Int` | Row count |
| `matrix.cols(m) -> Int` | Column count |
| `matrix.get(m, r, c) -> Float` | Element accessor |

## Arithmetic

All element-wise unless noted. `matrix.mul` is the matrix product and requires
`cols(a) == rows(b)` (see **Shapes and domains** below); the element-wise pairs
(`add` / `sub` / `div`, and `broadcast_add_row`) zip-truncate to
the shorter operand instead, identically on both targets.

| Signature | Purpose |
|---|---|
| `matrix.add(a, b) -> Matrix` | `a + b` |
| `matrix.sub(a, b) -> Matrix` | `a - b` |
| `matrix.mul(a, b) -> Matrix` | **Matrix multiplication** (not element-wise) |
| `matrix.div(a, b) -> Matrix` | `a / b` |
| `matrix.scale(m, s) -> Matrix` | `m * s` |
| `matrix.neg(m) -> Matrix` | `-m` |
| `matrix.pow(m, exp) -> Matrix` | `m^exp` (fractional exponent has known WASM bug) |
| `matrix.map(m, f) -> Matrix` | Apply `(Float) -> Float` to every cell |

## Slicing & assembly

| Signature | Purpose |
|---|---|
| `matrix.slice_rows(m, start, end) -> Matrix` | Half-open row range |
| `matrix.gather_rows(m, indices: List[Int]) -> Matrix` | Pick rows by index list (e.g. token-embedding lookup) |
| `matrix.split_cols_even(m, n) -> List[Matrix]` | Split columns into `n` equal chunks |
| `matrix.concat_cols(matrices: List[Matrix]) -> Matrix` | Column-wise concat (deprecated alias: `concat_cols_many`) |
| `matrix.dot_row(m, r, vec: List[Float]) -> Float` | Dot product of row `r` with `vec` (deprecated alias: `row_dot`) |

## Neural-network primitives

| Signature | Purpose |
|---|---|
| `matrix.broadcast_add_row(m, row: Matrix) -> Matrix` | Add a (1×cols) row to every row of m |
| `matrix.linear_row(x, w, b) -> Matrix` | Affine transform `x · w + b` |
| `matrix.linear_row_no_bias(x, w) -> Matrix` | `x · w` |
| `matrix.conv1d(input, weight, bias, kernel, stride, padding) -> Matrix` | 1D convolution |

## Shapes and domains

**A shape precondition is a defined abort.** A kernel that indexes one
operand's extent against another's requires them equal, and a violation stops
the program with `Error: matrix shape mismatch` on stderr and exit code 1 —
the same one-line abort the element index (`Error: matrix index out of bounds`) already use, on
native and on both wasm legs, never a panic and never a truncated answer
(C-353):

| Call | Requires |
|---|---|
| `mul(a, b)`, and the `mul_scaled` / `mul_f32*` fusions over it | `cols(a) == rows(b)` |
| `linear_row(x, w, bias)` | `cols(x) == cols(w)`, `len(bias) == rows(w)` |
| `linear_row_no_bias(x, w)`, `linear_f32_row_no_bias(x, …, w_cols)` | `cols(x) == cols(w)` |
| `conv1d(input, weight, bias, kernel, …)` | `cols(weight) == cols(input) * kernel`, `len(bias) == rows(weight)` |
| `concat_cols(ms)` | every non-empty member's `rows` equal the first member's |
| `append_rows(base, extra)` | `cols(extra) == cols(base)` |
| `from_lists(rows)` | every row as wide as the first — a ragged list aborts with `Error: matrix rows must have equal length` |

An **empty** operand is exempt: each of these answers its degenerate shape
rather than aborting (`mul(m, zeros(0, 0))` is the `rows(m)×0` matrix,
`concat_cols([m, zeros(0, 0)])` is `m`).

**A matrix with no rows has no columns** (C-161). A Matrix is a list of
equal-width rows, and its width is the width of row 0. So every 0-row matrix
is 0 × 0: `shape` answers `(0, 0)` and `cols` answers `0`, whatever width
produced it. `zeros(0, 3)`, `from_bytes_f32_le(b, 0, 0, 3)`,
`select_rows_f32(b, 0, 3, [])` and `slice_rows(m, 5, 9)` are all 0 × 0 on both
targets. This keeps `from_lists(to_lists(m))` shape-preserving: `to_lists` of
a rowless matrix is `[]`, and `from_lists([])` is 0 × 0. It does not work the
other way round: an r × 0 matrix keeps its r rows (`zeros(3, 0)` is `(3, 0)`
and `to_lists` gives three empty rows).

**Row ranges and counts clamp** (C-354), the way `list.slice` does:
`slice_rows(m, start, end)` clamps both bounds into `0..rows(m)` (a negative
bound is 0, the negative-count rule of #4147), and `start >= end` is empty;
`split_cols_even(m, n)` answers the empty list for `n <= 0`; `conv1d`'s
`kernel` and `padding` clamp at 0. Its `stride` is a step, not a width: a
stride below 1 aborts with `Error: stride must be positive`.

**`neg` is IEEE negation** (C-355): `matrix.neg` flips the sign bit, so
`neg(0.0)` is `-0.0` and `1.0 / neg(0.0)` is `-inf` on both targets.

## Byte-resident rows

Rows read straight from a byte buffer (the Q1_0 / Q8_0 quantized loaders
moved to package almide/nnlib at dialect epoch 14).

| Signature | Purpose |
|---|---|
| `matrix.select_rows_f32(data, offset, cols, row_ids) -> Matrix` | Row subset of flat f32 LE bytes |

**A row whose bytes leave the buffer is the all-zero row** (per selected row), a
negative row id clamps to row 0 and a negative offset clamps to 0 — never a
panic, never a read past the buffer (C-229).

The `from_bytes_f16_le` / `_f32_le` / `_f64_le` *decoders* keep a stored
`-0.0`: there the sign is the datum and survives (C-269).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (49 functions)

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

// Matrix of equal-length rows; [] is 0 x 0.
// @since 0.9.3 or earlier
matrix.from_lists(rows: List[List[Float]]) -> Matrix

// Rows at row_ids; id < 0 is row 0, past end zeros.
// @since 0.15.1 or earlier
matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix

// base rows then extra; empty side gives other.
// @since 0.15.1 or earlier
matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix

// x times f32 LE W^T.
// @since 0.31.0 or earlier
matrix.linear_f32_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix

// f32 LE rows at row_ids; past-data rows are 0.
// @since 0.31.0 or earlier
matrix.select_rows_f32(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix

// Rows as lists; r x 0 gives r empty rows.
// @since 0.9.3 or earlier
matrix.to_lists(m: Matrix) -> List[List[Float]]

// Element at (row, col); out of range aborts.
// @since 0.9.3 or earlier
matrix.get(m: Matrix, row: Int, col: Int) -> Float

// Row count; 0 for an empty matrix.
// @since 0.9.3 or earlier
matrix.rows(m: Matrix) -> Int

// Width of row 0; 0 when there are no rows.
// @since 0.9.3 or earlier
matrix.cols(m: Matrix) -> Int

// Elementwise a + b; shapes truncate to overlap.
// @since 0.9.3 or earlier
matrix.add(a: Matrix, b: Matrix) -> Matrix

// Matrix product; cols(a) == rows(b) is not checked.
// @since 0.9.3 or earlier
matrix.mul(a: Matrix, b: Matrix) -> Matrix

// Every element times s.
// @since 0.9.3 or earlier
matrix.scale(m: Matrix, s: Float) -> Matrix

// a*ka + b*kb in one pass; shapes truncate.
// @since 0.14.3 or earlier
matrix.fma(a: Matrix, ka: Float, b: Matrix, kb: Float) -> Matrix

// a*ka + b*kb + c*kc; shapes truncate.
// @since 0.14.3 or earlier
matrix.fma3(a: Matrix, ka: Float, b: Matrix, kb: Float, c: Matrix, kc: Float) -> Matrix

// Elementwise a - b; shapes truncate to overlap.
// @since 0.13.4 or earlier
matrix.sub(a: Matrix, b: Matrix) -> Matrix

// Elementwise a / b; x/0.0 is inf or NaN.
// @since 0.13.4 or earlier
matrix.div(a: Matrix, b: Matrix) -> Matrix

// Every element negated.
// @since 0.13.4 or earlier
matrix.neg(m: Matrix) -> Matrix

// Elementwise x^exp; x < 0, fractional exp: NaN.
// @since 0.13.4 or earlier
matrix.pow(m: Matrix, exp: Float) -> Matrix

// f applied to every element.
// @since 0.9.3 or earlier
matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix

// f32 LE rows x cols at offset; all 0 if data short.
// @since 0.13.4 or earlier
matrix.from_bytes_f32_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix

// Row-major f64 LE, 8 bytes per element.
// @since 0.13.4 or earlier
matrix.to_bytes_f64_le(m: Matrix) -> Bytes

// Row-major f32 LE, 4 bytes each (rounded).
// @since 0.13.4 or earlier
matrix.to_bytes_f32_le(m: Matrix) -> Bytes

// Row-major f16 LE, 2 bytes each (rounded to nearest even; overflow is inf).
// @since unreleased
matrix.to_bytes_f16_le(m: Matrix) -> Bytes

// f16 LE rows x cols at offset; all 0 if data short.
// @since 0.13.4 or earlier
matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix

// f64 LE rows x cols at offset; all 0 if data short.
// @since 0.13.4 or earlier
matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix

// bias added to each row; cols cut to bias.
// @since 0.13.4 or earlier
matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix

// n slices cols/n wide, remainder dropped; [] if n = 0.
// @since 0.13.4 or earlier
matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]

// Side-by-side join; [] gives 0 x 0.
// @since 0.13.4 or earlier
matrix.concat_cols(matrices: List[Matrix]) -> Matrix

// Alias of matrix.concat_cols.
// @since 0.13.4 or earlier
matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix   (deprecated — use matrix.concat_cols)

// x W^T + bias; empty x or W gives 0 x 0.
// @since 0.13.4 or earlier
matrix.linear_row(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix

// x W^T; empty x or W gives 0 x 0.
// @since 0.13.4 or earlier
matrix.linear_row_no_bias(x: Matrix, weight: Matrix) -> Matrix

// Rows [start, end), end clamped to the row count.
// @since 0.13.4 or earlier
matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix

// 1-D conv over rows; [] if input too short.
// @since 0.13.4 or earlier
matrix.conv1d(input: Matrix, weight: Matrix, bias: List[Float], kernel: Int, stride: Int, padding: Int) -> Matrix

// Rows at indices; out of range is a zero row.
// @since 0.13.4 or earlier
matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix

// Row r dot vec; 0.0 if r is out of range.
// @since 0.13.4 or earlier
matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float

// Alias of matrix.dot_row.
// @since 0.13.4 or earlier
matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float   (deprecated — use matrix.dot_row)

// Float32 rows x cols of 0.0; negatives clamp.
// @since 0.14.0 or earlier
matrix.zeros_f32(rows: Int, cols: Int) -> Matrix[Float32]

// Float32 rows x cols of 1.0; negatives clamp.
// @since 0.14.0 or earlier
matrix.ones_f32(rows: Int, cols: Int) -> Matrix[Float32]

// Float32 product; cols(a) == rows(b) is not checked.
// @since 0.14.0 or earlier
matrix.mul_f32(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]

// alpha * (a x b), Float32.
// @since 0.14.0 or earlier
matrix.mul_f32_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]

// a x transpose(b), Float32.
// @since 0.14.0 or earlier
matrix.mul_f32_t(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]

// alpha * (a x transpose(b)), Float32.
// @since 0.14.0 or earlier
matrix.mul_f32_t_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]

// Same as scale(mul(a, b), alpha).
// @since 0.14.0 or earlier
matrix.mul_scaled(a: Matrix, alpha: Float, b: Matrix) -> Matrix

// f over each pair of same-position elements; a shape mismatch aborts.
// @since unreleased
matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix

// Rows of every matrix, top to bottom; empty members skipped, a width mismatch aborts.
// @since unreleased
matrix.concat_rows(xs: List[Matrix]) -> Matrix
```

<!-- END GENERATED SIGNATURE INDEX -->
