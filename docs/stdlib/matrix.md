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
matrix.zeros(rows: Int, cols: Int) -> Matrix
matrix.ones(rows: Int, cols: Int) -> Matrix
matrix.shape(m: Matrix) -> ()
matrix.transpose(m: Matrix) -> Matrix
matrix.from_lists(rows: List[List[Float]]) -> Matrix
matrix.from_q1_0_bytes(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix
matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix
matrix.linear_q1_0_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix
matrix.silu_mul(a: Matrix, b: Matrix) -> Matrix
matrix.select_rows_q1_0(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix
matrix.rope_rotate(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float) -> Matrix
matrix.rope_rotate_at(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float, start_pos: Int) -> Matrix
matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix
matrix.qwen3_block_q1_0_kv(h: Matrix, k_cache: Matrix, v_cache: Matrix, w: Bytes, gamma_offs: List[Int], weight_offs: List[Int], start_pos: Int, n_q_heads: Int, n_kv_heads: Int, head_dim: Int, ffn_hidden: Int, rope_theta: Float, eps: Float) -> ()
matrix.linear_f32_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix
matrix.select_rows_f32(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix
matrix.rope_rotate_neox_at(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float, start_pos: Int) -> Matrix
matrix.qwen3_block_f32_kv(h: Matrix, k_cache: Matrix, v_cache: Matrix, w: Bytes, gamma_offs: List[Int], weight_offs: List[Int], start_pos: Int, n_q_heads: Int, n_kv_heads: Int, head_dim: Int, ffn_hidden: Int, rope_theta: Float, eps: Float) -> ()
matrix.linear_q8_0_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix
matrix.select_rows_q8_0_dq(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix
matrix.qwen3_block_q8_0_kv(h: Matrix, k_cache: Matrix, v_cache: Matrix, w: Bytes, gamma_offs: List[Int], weight_offs: List[Int], start_pos: Int, n_q_heads: Int, n_kv_heads: Int, head_dim: Int, ffn_hidden: Int, rope_theta: Float, eps: Float) -> ()
matrix.to_lists(m: Matrix) -> List[List[Float]]
matrix.get(m: Matrix, row: Int, col: Int) -> Float
matrix.rows(m: Matrix) -> Int
matrix.cols(m: Matrix) -> Int
matrix.add(a: Matrix, b: Matrix) -> Matrix
matrix.mul(a: Matrix, b: Matrix) -> Matrix
matrix.scale(m: Matrix, s: Float) -> Matrix
matrix.fma(a: Matrix, ka: Float, b: Matrix, kb: Float) -> Matrix
matrix.fma3(a: Matrix, ka: Float, b: Matrix, kb: Float, c: Matrix, kc: Float) -> Matrix
matrix.sub(a: Matrix, b: Matrix) -> Matrix
matrix.div(a: Matrix, b: Matrix) -> Matrix
matrix.neg(m: Matrix) -> Matrix
matrix.pow(m: Matrix, exp: Float) -> Matrix
matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix
matrix.from_bytes_f32_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix
matrix.to_bytes_f64_le(m: Matrix) -> Bytes
matrix.to_bytes_f32_le(m: Matrix) -> Bytes
matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix
matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix
matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix
matrix.layer_norm_rows(m: Matrix, gamma: List[Float], beta: List[Float], eps: Float) -> Matrix
matrix.rms_norm_rows(m: Matrix, gamma: List[Float], eps: Float) -> Matrix
matrix.swiglu_gate(x: Matrix, w_gate: Matrix, w_up: Matrix) -> Matrix
matrix.softmax_rows(m: Matrix) -> Matrix
matrix.gelu(m: Matrix) -> Matrix
matrix.fused_gemm_bias_scale_gelu(a: Matrix, b: Matrix, bias: Matrix, alpha: Float) -> Matrix
matrix.attention_weights(q: Matrix, kt: Matrix, scale: Float) -> Matrix
matrix.scaled_dot_product_attention(q: Matrix, kt: Matrix, v: Matrix, scale: Float) -> Matrix
matrix.linear_row_gelu(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix
matrix.pre_norm_linear(x: Matrix, gamma: List[Float], beta: List[Float], eps: Float, weight: Matrix, bias: List[Float]) -> Matrix
matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]
matrix.concat_cols(matrices: List[Matrix]) -> Matrix
matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix
matrix.causal_mask_add(m: Matrix, mask_val: Float) -> Matrix
matrix.multi_head_attention(q: Matrix, k: Matrix, v: Matrix, n_heads: Int) -> Matrix
matrix.masked_multi_head_attention(q: Matrix, k: Matrix, v: Matrix, n_heads: Int) -> Matrix
matrix.linear_row(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix
matrix.linear_row_no_bias(x: Matrix, weight: Matrix) -> Matrix
matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix
matrix.conv1d(input: Matrix, weight: Matrix, bias: List[Float], kernel: Int, stride: Int, padding: Int) -> Matrix
matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix
matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float
matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float
matrix.zeros_f32(rows: Int, cols: Int) -> ?
matrix.ones_f32(rows: Int, cols: Int) -> ?
matrix.mul_f32(a: ?, b: ?) -> ?
matrix.mul_f32_scaled(a: ?, alpha: Float, b: ?) -> ?
matrix.mul_f32_t(a: ?, b: ?) -> ?
matrix.mul_f32_t_scaled(a: ?, alpha: Float, b: ?) -> ?
matrix.mul_scaled(a: Matrix, alpha: Float, b: Matrix) -> Matrix
```

<!-- END GENERATED SIGNATURE INDEX -->
