# matrix

2D matrix operations. `import matrix`.

`Matrix` is a row-major, dense, `Float`-valued matrix (`f64`). All operations
treat the value as immutable; combinators return fresh matrices.

## Naming convention

- `<op>_rows` — operates on every row independently and returns a Matrix
  (`softmax_rows`, `layer_norm_rows`, `slice_rows`, `gather_rows`,
  `broadcast_add_row`, `linear_row`).
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

All element-wise unless noted. Both operands of binary ops must have the same shape.

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
| `matrix.gelu(m) -> Matrix` | GELU activation (tanh approximation, NaN-safe clamp) |
| `matrix.softmax_rows(m) -> Matrix` | Numerically-stable row softmax |
| `matrix.layer_norm_rows(m, gamma, beta, eps) -> Matrix` | Per-row LayerNorm |
| `matrix.causal_mask_add(m, mask_val) -> Matrix` | Add `mask_val` at upper-triangular positions |
| `matrix.multi_head_attention(...) -> Matrix` | Standard MHA |
| `matrix.masked_multi_head_attention(...) -> Matrix` | Causal MHA |
| `matrix.conv1d(input, weight, bias, kernel, stride, padding) -> Matrix` | 1D convolution |
