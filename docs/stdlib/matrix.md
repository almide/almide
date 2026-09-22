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

## Quantized loaders

GGUF-style block-quantized weights, decoded straight from the packed bytes.

| Signature | Purpose |
|---|---|
| `matrix.from_q1_0_bytes(data, offset, rows, cols) -> Matrix` | Q1_0: 18 B per 128 weights — fp16 scale + 128 sign bits |
| `matrix.select_rows_q1_0(data, offset, cols, row_ids) -> Matrix` | The row-subset twin (embedding lookup, no full decode) |
| `matrix.select_rows_q8_0_dq(data, offset, cols, row_ids) -> Matrix` | Q8_0: 34 B per 32 weights — fp16 scale + 32 int8 quants |
| `matrix.select_rows_f32(data, offset, cols, row_ids) -> Matrix` | Row subset of flat f32 LE bytes |

**A row whose bytes leave the buffer is the all-zero row** (per selected row), a
negative row id clamps to row 0 and a negative offset clamps to 0 — never a
panic, never a read past the buffer (C-229).

**The dequantization-zero ruling: an element of zero magnitude is `+0.0`.** A
quantized element is *magnitude* (the block's fp16 scale) × *direction* (the
sign bit / int8 quant); when the magnitude is zero the bytes encode no
direction, so the sign IEEE-754 would return is an artifact of how the
arithmetic was spelled — `-scale` yields `-0.0` exactly where `0.0 - scale`
yields `+0.0`. Since `-0.0 == 0.0`, nothing downstream notices, which is what
makes a stray sign bit worth ruling out rather than shrugging off. Non-zero
weights keep their exact sign and bits.

The `from_bytes_f16_le` / `_f32_le` / `_f64_le` *decoders* are deliberately
outside this rule: there a stored `-0.0` is the datum, so its sign is
information and survives (C-269).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (71 functions)

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

// Q1_0 1-bit decode; blocks past data are 0.0.
// @since 0.15.1 or earlier
matrix.from_q1_0_bytes(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix

// Rows at row_ids; id < 0 is row 0, past end zeros.
// @since 0.15.1 or earlier
matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix

// x times Q1_0-packed W^T.
// @since 0.15.1 or earlier
matrix.linear_q1_0_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix

// Elementwise x*sigmoid(x) of a, times b.
// @since 0.15.1 or earlier
matrix.silu_mul(a: Matrix, b: Matrix) -> Matrix

// Q1_0 rows at row_ids; past-data rows are 0.
// @since 0.15.1 or earlier
matrix.select_rows_q1_0(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix

// Interleaved RoPE, position = row; bad head sizes abort.
// @since 0.15.1 or earlier
matrix.rope_rotate(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float) -> Matrix

// RoPE with row p at start_pos + p; negative start is 0.
// @since 0.15.1 or earlier
matrix.rope_rotate_at(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float, start_pos: Int) -> Matrix

// base rows then extra; empty side gives other.
// @since 0.15.1 or earlier
matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix

// Qwen3 layer, Q1_0; new (h, k_cache, v_cache).
// @since 0.15.1 or earlier
matrix.qwen3_block_q1_0_kv(h: Matrix, k_cache: Matrix, v_cache: Matrix, w: Bytes, gamma_offs: List[Int], weight_offs: List[Int], start_pos: Int, n_q_heads: Int, n_kv_heads: Int, head_dim: Int, ffn_hidden: Int, rope_theta: Float, eps: Float) -> (Matrix, Matrix, Matrix)

// x times f32 LE W^T.
// @since 0.31.0 or earlier
matrix.linear_f32_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix

// f32 LE rows at row_ids; past-data rows are 0.
// @since 0.31.0 or earlier
matrix.select_rows_f32(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix

// NeoX RoPE: pairs (j, j + head_dim/2).
// @since 0.31.0 or earlier
matrix.rope_rotate_neox_at(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float, start_pos: Int) -> Matrix

// Qwen3 layer, f32; new (h, k_cache, v_cache).
// @since 0.31.0 or earlier
matrix.qwen3_block_f32_kv(h: Matrix, k_cache: Matrix, v_cache: Matrix, w: Bytes, gamma_offs: List[Int], weight_offs: List[Int], start_pos: Int, n_q_heads: Int, n_kv_heads: Int, head_dim: Int, ffn_hidden: Int, rope_theta: Float, eps: Float) -> (Matrix, Matrix, Matrix)

// x times Q8_0 W^T; all 0 if w_cols % 32 > 0.
// @since 0.31.0 or earlier
matrix.linear_q8_0_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix

// Q8_0 rows dequantized; 0 past data or if cols % 32 > 0.
// @since 0.31.0 or earlier
matrix.select_rows_q8_0_dq(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix

// Qwen3 layer, Q8_0; new (h, k_cache, v_cache).
// @since 0.31.0 or earlier
matrix.qwen3_block_q8_0_kv(h: Matrix, k_cache: Matrix, v_cache: Matrix, w: Bytes, gamma_offs: List[Int], weight_offs: List[Int], start_pos: Int, n_q_heads: Int, n_kv_heads: Int, head_dim: Int, ffn_hidden: Int, rope_theta: Float, eps: Float) -> (Matrix, Matrix, Matrix)

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

// f16 LE rows x cols at offset; all 0 if data short.
// @since 0.13.4 or earlier
matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix

// f64 LE rows x cols at offset; all 0 if data short.
// @since 0.13.4 or earlier
matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix

// bias added to each row; cols cut to bias.
// @since 0.13.4 or earlier
matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix

// Row-wise layer norm; cols cut to gamma/beta.
// @since 0.13.4 or earlier
matrix.layer_norm_rows(m: Matrix, gamma: List[Float], beta: List[Float], eps: Float) -> Matrix

// Row-wise RMS norm; cols cut to gamma.
// @since 0.15.0 or earlier
matrix.rms_norm_rows(m: Matrix, gamma: List[Float], eps: Float) -> Matrix

// silu(x Wg^T) * (x Wu^T); empty input: 0 x 0.
// @since 0.15.0 or earlier
matrix.swiglu_gate(x: Matrix, w_gate: Matrix, w_up: Matrix) -> Matrix

// Row-wise softmax, max-subtracted.
// @since 0.13.4 or earlier
matrix.softmax_rows(m: Matrix) -> Matrix

// Elementwise GELU, tanh approximation.
// @since 0.13.4 or earlier
matrix.gelu(m: Matrix) -> Matrix

// gelu(scale(add(mul(a, b), bias), alpha)).
// @since 0.15.0 or earlier
matrix.fused_gemm_bias_scale_gelu(a: Matrix, b: Matrix, bias: Matrix, alpha: Float) -> Matrix

// softmax_rows(scale(mul(q, kt), scale)).
// @since 0.15.0 or earlier
matrix.attention_weights(q: Matrix, kt: Matrix, scale: Float) -> Matrix

// mul(attention_weights(q, kt, scale), v).
// @since 0.15.0 or earlier
matrix.scaled_dot_product_attention(q: Matrix, kt: Matrix, v: Matrix, scale: Float) -> Matrix

// gelu(linear_row(x, weight, bias)).
// @since 0.15.0 or earlier
matrix.linear_row_gelu(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix

// linear_row(layer_norm_rows(x, ...), weight, bias).
// @since 0.15.0 or earlier
matrix.pre_norm_linear(x: Matrix, gamma: List[Float], beta: List[Float], eps: Float, weight: Matrix, bias: List[Float]) -> Matrix

// n slices cols/n wide, remainder dropped; [] if n = 0.
// @since 0.13.4 or earlier
matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]

// Side-by-side join; [] gives 0 x 0.
// @since 0.13.4 or earlier
matrix.concat_cols(matrices: List[Matrix]) -> Matrix

// Same as concat_cols; [] gives 0 x 0.
// @since 0.13.4 or earlier
matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix

// mask_val added where col > row.
// @since 0.13.4 or earlier
matrix.causal_mask_add(m: Matrix, mask_val: Float) -> Matrix

// Attention over n_heads; n_heads < 1 aborts.
// @since 0.13.4 or earlier
matrix.multi_head_attention(q: Matrix, k: Matrix, v: Matrix, n_heads: Int) -> Matrix

// Causal multi-head attention; n_heads < 1 aborts.
// @since 0.13.4 or earlier
matrix.masked_multi_head_attention(q: Matrix, k: Matrix, v: Matrix, n_heads: Int) -> Matrix

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

// Same as dot_row; 0.0 if r is out of range.
// @since 0.13.4 or earlier
matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float

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
```

<!-- END GENERATED SIGNATURE INDEX -->
