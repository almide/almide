# matrix

2D matrix operations. `import matrix`.

> **Dialect epoch 14:** the neural-network kernels (attention, RoPE, the row
> norms, the activations, the Q1_0/Q8_0 loaders and the Qwen3 blocks — 25
> functions) moved to package [almide-graphics/nn](https://github.com/almide-graphics/nn) (module `nn.kernels`):
> `almide add almide-graphics/nn@v0.2.0 --trust github.com/almide-graphics`, `import nn.kernels`, then `kernels.gelu(m)` for
> `matrix.gelu(m)`, with the same arguments. Calling the old spelling is a
> check-time error that names the new one.
>
> **Dialect epoch 15:** the arithmetic functions became the operators
> (`add` is `a + b`; `sub`, `mul`, `div` likewise), the dense
> kernels `linear_row`, `linear_row_no_bias`, `linear_f32_row_no_bias`,
> `conv1d` and `select_rows_f32` moved to almide-graphics/nn, and the `_f32`
> product fusions are `*` with `matrix.transpose` / `matrix.scale`.

`Matrix` is a row-major, dense, `Float`-valued matrix (`f64`). All operations
treat the value as immutable; combinators return fresh matrices.

## Naming convention

- `<op>_rows` — operates on every row independently and returns a Matrix
  (`slice_rows`, `gather_rows`, `broadcast_add_row`).
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
| `matrix.from_bytes_f64_le(b, offset, rows, cols) -> Option[Matrix]` | Read flat f64 LE bytes; `none` if the data does not fit |
| `matrix.from_bytes_f32_le(b, offset, rows, cols) -> Option[Matrix]` | Read flat f32 LE (promoted to f64); `none` if the data does not fit |
| `matrix.from_bytes_f16_le(b, offset, rows, cols) -> Option[Matrix]` | Read flat IEEE-754 half (GGML weights); `none` if the data does not fit |

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
| `matrix.get(m, r, c) -> Option[Float]` | Element accessor; `none` out of range (C-282) |

## Arithmetic

All element-wise unless noted. `a * b` is the matrix product and requires
`cols(a) == rows(b)` (see **Shapes and domains** below); the element-wise
operators (`+` / `-` / `/`, and `broadcast_add_row`) zip-truncate to the
shorter operand instead, identically on both targets.

| Expression | Purpose |
|---|---|
| `a + b` | Element-wise sum |
| `a - b` | Element-wise difference |
| `a * b` | **Matrix multiplication** (not element-wise) |
| `a / b` | Element-wise quotient |
| `matrix.scale(m, s) -> Matrix` | Every cell times `s`; `matrix.scale(m, -1.0)` negates |
| `matrix.map(m, f) -> Matrix` | Apply `(Float) -> Float` to every cell, e.g. `(x) => float.pow(x, e)` |
| `matrix.concat_rows(ms: List[Matrix]) -> Matrix` | Rows of every member, top to bottom |

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

The dense kernels (`linear_row`, `conv1d`, ...) live in module nn.kernels of package almide-graphics/nn.

## Shapes and domains

**A shape precondition is a defined abort.** A kernel that indexes one
operand's extent against another's requires them equal, and a violation stops
the program with `Error: matrix shape mismatch` on stderr and exit code 1 —
on native and on both wasm legs, never a panic and never a truncated answer
(C-353):

| Call | Requires |
|---|---|
| `a * b` | `cols(a) == rows(b)` |
| `concat_cols(ms)` | every non-empty member's `rows` equal the first member's |
| `concat_rows(ms)` | every non-empty member's `cols` equal |
| `from_lists(rows)` | every row as wide as the first — a ragged list aborts with `Error: matrix rows must have equal length` |

An **empty** operand is exempt: each of these answers its degenerate shape
rather than aborting (`m * zeros(0, 0)` is the `rows(m)×0` matrix,
`concat_cols([m, zeros(0, 0)])` is `m`).

**A matrix with no rows has no columns** (C-161). A Matrix is a list of
equal-width rows, and its width is the width of row 0. So every 0-row matrix
is 0 × 0: `shape` answers `(0, 0)` and `cols` answers `0`, whatever width
produced it. `zeros(0, 3)`, `from_bytes_f32_le(b, 0, 0, 3)!`,
`gather_rows(m, [])` and `slice_rows(m, 5, 9)` are all 0 × 0 on both
targets. This keeps `from_lists(to_lists(m))` shape-preserving: `to_lists` of
a rowless matrix is `[]`, and `from_lists([])` is 0 × 0. It does not work the
other way round: an r × 0 matrix keeps its r rows (`zeros(3, 0)` is `(3, 0)`
and `to_lists` gives three empty rows).

**Row ranges and counts clamp** (C-354), the way `list.slice` does:
`slice_rows(m, start, end)` clamps both bounds into `0..rows(m)` (a negative
bound is 0, the negative-count rule of #4147), and `start >= end` is empty;
`split_cols_even(m, n)` answers the empty list for `n <= 0`.

**Negation is IEEE** (C-355): `-m` flips the sign bit, so a
`0.0` cell becomes `-0.0` and `1.0` over it is `-inf` on both targets.

## Byte decoders

Rows read from a byte buffer are a decode and a gather:
`matrix.gather_rows(matrix.from_bytes_f32_le(b, offset, rows, cols)!, ids)`
(the Q1_0 / Q8_0 quantized loaders moved to module nn.kernels of package almide-graphics/nn at dialect
epoch 14, and `select_rows_f32` at epoch 15).

The `from_bytes_f16_le` / `_f32_le` / `_f64_le` *decoders* keep a stored
`-0.0`: there the sign is the datum and survives (C-269). They answer `none`
unless `0 <= offset <= len(b)` and `rows * cols * width <= len(b) - offset`
(C-341), after a negative dimension clamps to 0 and an over-ceiling count
aborts with `Error: matrix dimensions too large` (C-161).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (27 functions)

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

<!-- END GENERATED SIGNATURE INDEX -->
