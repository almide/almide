> Description (fixed history): Stdlib redesign L2/L3 — matrix: finish the epoch-14 split (#3852): kernels to nn.kernels, fusion targets internal, Float32 preview retired, select_rows consolidated into gather_rows, and shape/length mismatches abort instead of returning zeros.
<a id="l2l3-matrix"></a>

# L2/L3: matrix

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-matrix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-matrix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-matrix.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh source retrieval or independent execution. Measurements, speed and implementation statements are original-author reports, not current verification. Preserve early abort-on-short-data versus R15 none, early retain-mul versus later operators-only, unary map versus later zip_with correction, old get signature versus Option proposal, and “46 functions/no additions” versus later appended additions. Accepted proposals and epochs are historical decisions, not proof of current delivery. Japanese code placeholders mean value, default value, width and type. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-matrix.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-22: retain `matrix` in the standard library and complete #3852's split).
> Sources: Rust https://docs.rs/ndarray/latest/ndarray/ (not standard) / Go https://pkg.go.dev/gonum.org/v1/gonum/mat (not standard) / Python https://numpy.org/doc/stable/reference/routines.linalg.html (not standard; only operator `@` is in the language) / TS/JS https://developer.mozilla.org/en-US/docs/Web/API/DOMMatrix (browser only) / Gleam https://gleam-stdlib.hexdocs.pm/ (no matrices) / Elixir https://hexdocs.pm/nx/Nx.html (external) / OCaml https://ocaml.org/manual/latest/api/Bigarray.html (container only) / Kotlin https://github.com/Kotlin/multik (external) / Swift https://developer.apple.com/documentation/accelerate (Apple only)

None of the nine languages has standard matrix operations ([facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) §§1.A/3.9). L1 R-L1-22 retains only general dense-matrix operations in the standard library because compiler rewrites (`@rewrite`, egg fusion) target `matrix` operations and packages cannot supply rewrite rules. “Other language” columns use spellings from de facto standard external libraries (numpy, gonum, ndarray, Nx).

Measurements (`almide 0.68.0 dev 437b8f127`):

| Expression | Result | Interpretation |
|---|---|---|
| `matrix.add(2×2, 1×3)` | `1×2` | Mismatched shapes silently compute only the overlap and shrink |
| `matrix.from_bytes_f32_le(4 バイト, 0, 2, 2)` | `2×2` filled entirely with `0.0` | Insufficient data makes everything zero, including readable initial elements (sentinel) |
| `matrix.from_bytes_f32_le(4 バイト, 0, 1, 1)` | `1.0` | Reads correctly when sufficient |
| `matrix.select_rows(m, [-1])` / `matrix.gather_rows(m, [-1])` | Row 0 / zero row | Two functions with the same role handle negative indices differently |
| `matrix.dot_row(m, 5, …)` (two-row matrix) | `0.0` | Returns zero for an out-of-range row |
| `matrix.from_lists([[1.0, 2.0], [3.0]])` | abort “rows must have equal length” | Unequal row lengths stop as a contract violation |
| `matrix.get(m, 9, 9)` | abort | Stops for out-of-range indices |

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust（ndarray） | Go（gonum） | Python（numpy） | TS/JS | Gleam | Elixir（Nx） | OCaml | Kotlin（multik） | Swift（Accelerate） |
|---|---|---|---|---|---|---|---|---|---|---|
| Construct filled with zeros/ones | `zeros` `ones` | `zeros` `ones` | `NewDense(r, c, nil)` | `zeros` `ones` | — | — | `broadcast(0, …)` | `Bigarray.Array2.create` | `zeros` `ones` | — |
| From/to lists | `from_lists` `to_lists` | `from_shape_vec` | `NewDense(r, c, data)` | `array` `tolist` | — | — | `tensor` `to_list` | — | `mk.ndarray` | — |
| Shape | `shape` `rows` `cols` | `shape` `nrows` `ncols` | `Dims` | `shape` | — | — | `shape` | `dim1` `dim2` | `shape` | — |
| Read an element | `get` (currently aborts out of range; R-matrix-08 makes it `Option`） | `m[[i, j]]`（panic） | `At`（panic） | `m[i, j]` (exception) | — | — | `m[i][j]` | `get` | `m[i, j]` | — |
| Transpose | `transpose` | `t()` | `T()` | `.T` | — | — | `transpose` | — | `transpose` | `vDSP_mtrans` |
| Elementwise sum/difference/quotient/negation/power | `add` `sub` `div` `neg` `pow` | `+` `-` `/` | `Add` `Sub` `DivElem` | `+` `-` `/` `**` | — | — | `add` `subtract` `divide` `pow` | — | `+` `-` `/` | `vDSP` |
| Elementwise product | — | `*` | `MulElem` | `*` | — | — | `multiply` | — | `*` | `vDSP_vmul` |
| Matrix product | `mul` | `dot` | `Mul` | `@` / `matmul` | — | — | `dot` | — | `dot` | `cblas_dgemm` |
| Scalar multiplication/application of a function | `scale` `map` | `* s` `mapv` | `Scale` `Apply` | `* s` `vectorize` | — | — | `multiply` `map` | — | `* s` `map` | — |
| Extract rows | `slice_rows` `gather_rows` `select_rows` | `slice` `select` | `Slice` | `m[a:b]` `m[idx]` | — | — | `slice` `take` | `sub` | `m[a..b]` | — |
| Concatenate/split | `append_rows` `concat_cols` `split_cols_even` | `concatenate` | `Stack` `Augment` | `vstack` `hstack` `split` | — | — | `concatenate` `split` | — | `cat` | — |
| Add a row vector/dot product with a row | `broadcast_add_row` `dot_row` | broadcasting `+` | — | broadcasting `+` `m[i] @ v` | — | — | `add`（broadcasting） | — | — | — |
| Byte conversions (f16/f32/f64) | `from_bytes_*_le` `to_bytes_*_le` | `from_shape_vec` + `bytemuck` | `binary.Read` | `frombuffer` `tobytes` | — | — | `from_binary` `to_binary` | — | — | — |
| Inference kernels | `linear_row` `conv1d` `linear_f32_row_no_bias` `select_rows_f32` | External | External | `torch` | — | — | `Axon` | — | — | — |
| 32-bit matrices | `Matrix[Float32]` (preview; only two constructors and four products) | `Array2<f32>` | — | `float32` | — | — | `{:f, 32}` | — | `D2Array<Float>` | `cblas_sgemm` |

<a id="完全性の規則"></a>

### Completeness rules

1. **Retain only general dense-matrix operations in the standard library** (R-L1-22, #3852). Inference kernels above and fusion targets (`fma`, `fma3`, `mul_scaled`) are not public. Retain `broadcast_add_row` and `split_cols_even` as general operations (corresponding to numpy/Nx broadcasting and `split`, without inference-specific assumptions; these are the two rulings requested by #3852).
2. **Only one element type, `Float`**. Publishing `Matrix[Float32]` requires the complete same operation table as `Matrix`, checked as a family matrix by a gate (CLAUDE.md family rule). There are no standard-library consumers, so do not offer it now (R-matrix-03).
3. **Shape/length/index mismatch aborts** (R-L0-06 items 3/4). Functions whose callers supply shapes (elementwise operations, matrix multiplication, row-vector addition, byte construction, row extraction, row dot product) abort on shape/length mismatch. Callers can check dimensions/length before calling through `rows`, `cols`, `bytes.len`, so mismatch is a contract violation (like `list.chunk(xs, 0)`). Do not return sentinel zeros or small matrices (R-L0-06 item 4). `from_lists` already follows this rule.
   - Only single-element `get`, as unsuffixed extraction, returns `Option` (R-L0-06 item 1, R-matrix-08).
   - Only `slice_rows` ranges follow R-L0-12 (clamp overruns to endpoints).
   - Matrix has no positional-write functions (all operations return copies), so no function falls under L0's “out-of-range writes abort”.
4. Remove aliases scheduled without an epoch (`concat_cols_many`, `row_dot`) within R-L0-14's window.

<a id="l2-の判定"></a>

### L2 verdict

- Five inference kernels → R-matrix-01 (`almide-graphics/nn`).
- Three fusion targets → R-matrix-02 (internal `__` names).
- Six 32-bit preview functions → R-matrix-03 (remove).
- Duplicates: `select_rows`/`gather_rows` → R-matrix-04; `concat_cols_many`/`row_dot` → R-matrix-06 (#3851 A).
- Sentinels and silent truncation → R-matrix-05.
- Missing role: elementwise multiplication (majority of external libraries). Naming needs a decision because `matrix.mul` means matrix multiplication; it can be written with `map` and `from_lists`, and no language has standard matrices, so do not add it (R-matrix-07 rejected).

<a id="案"></a>

## Proposals

<a id="r-matrix-01-推論用のカーネルを-almide-graphicsnn-へ移す---採用"></a>

### R-matrix-01 Move inference kernels to `almide-graphics/nn` — ○ Accepted

- Content: remove `linear_f32_row_no_bias`, `select_rows_f32`, `linear_row`, `linear_row_no_bias`, `conv1d` from the standard library; place them in `almide-graphics/nn` with identical names/arguments. Implement nn.kernels with general operations (`linear_row(x, w, b)` = `broadcast_add_row(mul(x, transpose(w)), b)`; the two f32 functions do the same after `from_bytes_f32_le`).
- Supporting L0 rules: L1 R-L1-22, epoch 14 ruling (NN kernels belong to nn.kernels).
- (a) No language has standard inference kernels.
- (b) nn.kernels is pure Almide (`src/kernels.almd`, no intrinsics), so runs on native/wasm. The two current f32 functions are intrinsics reading GGUF buffers directly and may become slower after moving. Performance belongs to nn.kernels, not the standard-surface ruling.
- (c) Sibling kernels (`linear_q8_0_row_no_bias`, etc.) already moved to nn.kernels in epoch 14. Leaving half in the standard library makes placement unpredictable for LLMs.
- (d) Remove 5. Follow #3852: release almide-graphics/nn first so hints such as `@deprecated(since=15, use="kernels.linear_row")` can name that version. Removal epoch 16. `almide fix`: `matrix.<fn>(…)` → add `import nn.kernels`, use `kernels.<fn>(…)`. Usage counts (spec/tests/docs/stdlib): `linear_f32_row_no_bias` 9/0/5/0, `select_rows_f32` 10/0/6/0, `linear_row` 8/0/4/0, `linear_row_no_bias` 2/0/4/1, `conv1d` 15/0/5/0. nn.kernels itself calls `matrix.linear_row` (2), `linear_row_no_bias` (2), `linear_f32_row_no_bias` (1); redirect to definitions within nn.kernels. Replace CHEATSHEET:1066's `linear_row` example.
- Proposed gate: regenerate `crates/almide-types/src/self_host_registry.rs`, `docs/stdlib/matrix.md` index, `since.toml`, semantics-manifest, inspection-ledger rows in the same change; verify reduced public count with `check_stdlib_fn_count`.
- Issue: #3852 (§1)

<a id="r-matrix-02-fusion-の対象を内部の名前にする---採用"></a>

### R-matrix-02 Make fusion targets internal names — ○ Accepted

- Content: remove `mul_scaled`, `fma`, `fma3` from the public surface, letting only `@rewrite`/egg rules emit `__mul_scaled`, `__fma`, `__fma3`. Public syntax is composition only (`matrix.scale(matrix.mul(a, b), s)`, `matrix.add(matrix.scale(a, ka), matrix.scale(b, kb))`).
- Supporting L0 rule: R-L0-01 (two spellings for the same computation); also its forwarding case (a function merely calling a combination of other public functions once).
- (b) Rewrites find the composition and replace it with internal functions, so speed is unchanged. `crates/almide-codegen/src/target.rs:81-85` states these rewrites are only for speed and specs do not depend on them.
- (c) Removes functions that leave LLMs wondering whether explicit use is faster or equivalent to not using them.
- (d) Remove 3 (bodies remain internal). `@deprecated(since=15, use="matrix.scale(matrix.mul(a, b), s)")`, etc.; removal epoch 16. `almide fix`: one-to-one expansion into composition. Usage counts: `mul_scaled` 3/0/3/2, `fma` 13/2/5/1, `fma3` 6/2/3/0.
- Proposed gate: rewrite existing tests such as `tests/matrix_fma_overlap_cost_test.rs` using compositions; check rewriting to internal functions.
- Issue: #3852 (§1)

<a id="r-matrix-03-32-bit-の試験版-matrixfloat32-を撤去する---採用"></a>

### R-matrix-03 Retire 32-bit preview `Matrix[Float32]` — ○ Accepted

- Content: remove `zeros_f32`, `ones_f32`, `mul_f32`, `mul_f32_scaled`, `mul_f32_t`, `mul_f32_t_scaled`. Read f32 data as `Matrix` (Float) with `from_bytes_f32_le`.
- Supporting L0 rule: completeness rule 2 (CLAUDE.md “expand families by matrix”). #3852 requested either completing the operation table or moving to nn.kernels. The latter is impossible (nn.kernels is pure Almide without intrinsics; no construction route for `Matrix[Float32]` outside the standard library). Completing the table requires about 30 cells, like `Matrix`, with no consumers except specs.
- (a) External libraries have the same operation table per element type (numpy dtype, ndarray generics). None offers only two constructors and four products.
- (b) Runtime SmallF32 storage (`cblas_sgemm`) may remain internal; remove only the public surface.
- (c) One can construct `Matrix[Float32]` but types do not match `add`, so LLM-written code immediately reaches a dead end.
- (d) Remove 6. `@deprecated(since=15, use="matrix.zeros")`, etc.; removal epoch 16. `almide fix` cannot mechanically rewrite because types change; use `removed_spellings.rs` hints pointing to `from_bytes_f32_le` and Float matrices. Usage counts: `zeros_f32` 2/0/3/0, `ones_f32` 18/8/3/0, `mul_f32` 3/5/3/0, `mul_f32_scaled` 2/1/3/0, `mul_f32_t` 2/0/3/0, `mul_f32_t_scaled` 2/0/3/0 (mostly `spec/stdlib/matrix_f32_test.almd`).
- Reopening condition: when an f32 consumer appears in the standard library or nn.kernels, reintroduce `Matrix[Float32]` with the same full operation table/gates as `Matrix`.
- Issue: #3852 (§1 Float32 preview)

<a id="r-matrix-04-select_rows-を撤去しgather_rows-に一本化する---採用"></a>

### R-matrix-04 Remove `select_rows`, consolidate on `gather_rows` — ○ Accepted

- Content: remove `select_rows`; R-matrix-05 makes `gather_rows` abort on out-of-range indices.
- Supporting L0 rule: R-L0-01 (same role, two functions differing only at boundaries).
- (c) Names cannot reveal boundary differences (negative index selects row 0 versus a zero row). CHEATSHEET:1066 names `gather_rows`.
- (d) Remove. `@deprecated(since=15, use="matrix.gather_rows")`, removal epoch 16. `almide fix`: rename (hint notes negative-index semantic change). Usage counts: `select_rows` 0/0/3/0, `gather_rows` 5/0/4/0.
- Issue: #3852 (§1)

<a id="r-matrix-05-形長さ添字の不一致を-abort-にする---採用"></a>

### R-matrix-05 Abort on shape/length/index mismatches — ○ Accepted

- Content: stop silently returning smaller results or zero; abort naming the mismatch in these cases:
  - `add`, `sub`, `div`: different shapes.
  - `mul`: `cols(a) != rows(b)`.
  - `broadcast_add_row`: `bias` length differs from column count.
  - `from_bytes_f16_le`, `from_bytes_f32_le`, `from_bytes_f64_le`: `offset + rows * cols * 幅` exceeds data length.
  - `gather_rows`: negative index or index at least row count.
  - `dot_row`: row out of range or `vec` length differs from column count.
- Supporting L0 rules: R-L0-06 item 3 (abort only for caller contract violations), item 4 (fallible construction does not return sentinels). Callers choose/supply shapes and can check before calling, so mismatch violates caller contracts. `from_lists` already follows this rule.
- (a) numpy, gonum, ndarray, Nx use exceptions/panics for shape mismatch. No external library computes only overlapping regions.
- (b) Add only pure computation checks; identical abort wording on both targets (fixed by wasm_cross fixtures).
- (c) LLM code with wrong shapes currently proceeds with small matrices/zeros and produces wrong answers much later. Stopping lets it fix the error locally.
- (d) Boundary semantic change. No correctly returned input result changes; only inputs previously yielding meaningless values abort (domain narrowing, not R-L0-14 item 4 “different meaning under same name”; like L0 positional writes, valid results are unchanged, so exclude from epoch `breaks`). No rename, no `@deprecated`. Abort names the mismatched shapes and function. Before introducing, count mismatched-shape calls in specs/tests/nn.kernels/downstream projects (nn.kernels `shape_mismatch()` uses `append_rows`, outside this change).
- Proposed gate: one mismatched-input abort fixture per function, new C-number in contract ledger (als first).
- Issue: #3927

<a id="r-matrix-06-撤去予定の別名-concat_cols_manyrow_dot-を-epoch-16-で撤去する---採用"></a>

### R-matrix-06 Remove scheduled aliases `concat_cols_many`/`row_dot` in epoch 16 — ○ Accepted

- Content: add `[[deprecation]]` blocks (removal epoch 16) for two functions already annotated `@deprecated(since=5)`.
- Supporting L0 rules: R-L0-14, same treatment as R-L0-11 (#3851 A).
- (d) Remove. Already annotated `@deprecated(since=5, use="matrix.concat_cols")` / `@deprecated(since=5, use="matrix.dot_row")`. `removal_epoch = 16`. `almide fix` renames one-to-one (`concat_cols_many` → `concat_cols`, `row_dot` → `dot_row`). Usage counts: `concat_cols_many` 1/2/5/1, `row_dot` 2/3/6/0.
- Issue: #3851 A

<a id="r-matrix-08-matrixget-は範囲外で-option-を返す---採用"></a>

### R-matrix-08 `matrix.get` returns `Option` out of range — ○ Accepted

- Content: `matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]`. In range returns `some(値)`, otherwise none.
- Supporting L0 rule: R-L0-06 item 1 (unsuffixed extraction is safe). `list.get`, `map.get`, `bytes.get` all return `Option`; make “unsuffixed `get` returns `Option`” exception-free.
- (a) Rust ndarray separates `get` returning `Option` from indexing `m[[i, j]]` panicking. Align Almide with the `get` side.
- (b) No impact.
- (c) If `get` stops in one module and returns none in another, names cannot tell whether to write `??`. Element-reading loops should ordinarily use matrix-wide operations like `matrix.map`/`matrix.to_lists`; per-element chains of `??` are not recommended.
- (d) Return-type change. Old calls type-error, not silently change meaning (R-L0-14 item 4). No rename, thus no `@deprecated`; type errors hint `matrix.get(m, i, j) ?? 既定値`. `almide fix` preserves current out-of-range stopping behavior by rewriting to `matrix.get(m, i, j) ?? panic("matrix.get: out of range")`. Record usage count in the PR.
- Issue: #3927

<a id="r-matrix-07-mul-を-matmul-に改名し要素ごとの積を足す---不採用-r-matrix-14-で--に置き換え"></a>

### R-matrix-07 Rename `mul` to `matmul`, add elementwise multiplication — × Rejected (→ replaced by `*` in R-matrix-14)

- Content: no change; `mul` remains matrix multiplication.
- Supporting L0 rules: R-L0-00 (no majority because none of nine standard libraries has matrices; even external libraries split among gonum `Mul`, Nx/ndarray `dot`, numpy `matmul`), R-L0-14 item 4 (cannot repurpose `mul` as elementwise multiplication).
- (c) Sixth-round addition: R-matrix-14 removed `mul`, making operator `*` matrix multiplication. Docs/CHEATSHEET wording: “`*` is matrix multiplication; use `map` for elementwise multiplication”. R-matrix-05 stops shape mismatches, so confusion is found at runtime.
- Issue: none

<a id="r-matrix-09-要素ごとの二項演算を-matrixzip_with-で書けるようにする---採用"></a>

### R-matrix-09 Express elementwise binary operations with `matrix.zip_with` — ○ Accepted

- Current: elementwise operations are only `add`/`sub`/`div`, not multiplication. Rejected R-matrix-07 says “use `map` for elementwise multiplication”, but `matrix.map` takes a unary function, so it cannot express the product of two matrices (new evidence for R-matrix-07).
- Proposal: `fn matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix`. Abort on shape mismatch, like R-matrix-05.
- Supporting L0 rule: R-L0-10 (`_with`, same spelling as `list.zip_with`).
- (a) Most external libraries have elementwise multiplication (numpy `*`, gonum `MulElem`, Nx `multiply`).
- (c) An LLM that mistook `matrix.mul` for elementwise multiplication now has a correction destination.
- (d) Add 1; preserve `mul` (matrix multiplication) meaning.
- Issue: #3972

<a id="r-matrix-11-行方向の連結を-concat_rowslistmatrix-にしconcat_cols-と形をそろえる---採用"></a>

### R-matrix-11 Use `concat_rows(List[Matrix])` for row concatenation, align with `concat_cols` — ○ Accepted

- Current: `matrix.append_rows(base, extra)` takes two values, `matrix.concat_cols(matrices: List[Matrix])` takes a list.
- Proposal: add `fn matrix.concat_rows(matrices: List[Matrix]) -> Matrix`, remove `append_rows`.
- Supporting L0 rules: R-L0-01 (same shape when only axes differ), R-L0-04 item 2 (`append_` reserved for in-place `mut` verbs).
- (a) numpy `vstack`/`hstack`, Nx `concatenate`, ndarray `concatenate` take lists on both axes.
- (d) Add 1/remove 1. `@deprecated(since=15, use="matrix.concat_rows")`; `almide fix` rewrites `append_rows(a, b)` to `concat_rows([a, b])`.
- Issue: #3972

<a id="r-matrix-12-to_bytes_f16_le-を足してバイト列の変換を対にする---採用"></a>

### R-matrix-12 Add `to_bytes_f16_le`, pairing byte conversions — ○ Accepted

- Proposal: `fn matrix.to_bytes_f16_le(m: Matrix) -> Bytes` (round to f16 using IEEE nearest-even).
- Supporting L0 rule: R-L0-07 item 4 (pair directions). Six cells: widths {f16, f32, f64} × directions {from, to}.
- (d) Add 1.
- Issue: #3972

<a id="r-matrix-13-matrixpow-を撤去する---採用"></a>

### R-matrix-13 Remove `matrix.pow` — ○ Accepted

- Current: `fn matrix.pow(m: Matrix, exp: Float) -> Matrix` is elementwise power, merely calling `matrix.map(m, (x) => float.pow(x, e))` once. Mathematical A^2 means matrix multiplication, also inviting misreading.
- Supporting L0 rule: R-L0-01 (a function calling another public function once is a synonym).
- (d) Remove. `almide fix` rewrites to `matrix.map(m, (x) => float.pow(x, e))`. Usage count 6/0/3/0.
- Issue: #3973

<a id="r-matrix-14-matrix-の-addsubmuldivneg-を公開しない演算子だけにする---採用"></a>

### R-matrix-14 Do not expose matrix `add`/`sub`/`mul`/`div`/`neg` (operators only) — ○ Accepted

- Current: `a + b`, `a - b`, `a * b` (matrix multiplication), `a / b` are desugared while the same computations remain public functions.
- Proposal: apply R-bigint-04's rule (R-L0-26: do not expose operator desugaring targets) to matrix and remove five functions. Desugar unary `-` after fixing #3965 (currently internal error).
- Supporting L0 rules: R-L0-01/R-L0-26. Applying only to bigint makes rules differ for types with operators.
- (c) Piping `a |> matrix.add(b)` has benefits, but one form `a + b` reduces generated-code variation.
- (d) Remove 5. `@deprecated(since=15, use="+")`, etc.; remove epoch 16. `almide fix` rewrites `matrix.add(a, b)` to `a + b`.
- Issue: #3973

<a id="r-matrix-15-from_bytes__le-はデータが足りないと-none-を返すr-matrix-05-の見直し---採用"></a>

### R-matrix-15 `from_bytes_*_le` returns none for insufficient data (revising R-matrix-05) — ○ Accepted

- Current: R-matrix-05 chose abort for insufficient data (currently all-zero sentinel), but byte readers such as `bytes.get_f32_le_array` return none when short. Switching external-file reads to Matrix changes a recoverable failure into program termination.
- Proposal: distinguish two cases. Negative `rows`/`cols`/`offset` is caller error and still aborts. Insufficient bytes for `rows × cols` elements from `offset` is an input problem: return none via `-> Matrix?`.
  - `fn matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` (same for f32/f64)
- Supporting L0 rule: R-L0-06 (fallible unsuffixed extraction returns Option). Same failure shape as bytes `get_<型>_array`.
- (d) Semantic change. Return types change, so old calls type-error rather than silently changing results (R-L0-14 item 4). `almide fix` gives a hint equivalent to `?? abort`, not a trailing `!`.
- Issue: #3978

<a id="l3-関数表"></a>

## L3: Function table

The 46 current `matrix` functions (no additions).

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `zeros` | `matrix.zeros(rows: Int, cols: Int) -> Matrix` | numpy `zeros` / ndarray `zeros` / Nx `broadcast` / multik `zeros` | Keep | — |
| `ones` | `matrix.ones(rows: Int, cols: Int) -> Matrix` | numpy `ones` / ndarray `ones` / multik `ones` | Keep | — |
| `shape` | `matrix.shape(m: Matrix) -> (Int, Int)` | numpy `shape` / ndarray `shape` / gonum `Dims` / Nx `shape` | Keep | — |
| `transpose` | `matrix.transpose(m: Matrix) -> Matrix` | numpy `.T` / ndarray `t()` / gonum `T()` / Nx `transpose` | Keep | — |
| `from_lists` | `matrix.from_lists(rows: List[List[Float]]) -> Matrix` | numpy `array` / Nx `tensor` / gonum `NewDense` | Keep | — |
| `select_rows` | `matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix` | numpy `m[idx]` / ndarray `select` / Nx `take` | Remove | R-matrix-04 |
| `append_rows` | `matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix` | numpy `vstack` / ndarray `concatenate(Axis(0))` / Nx `concatenate` | Remove → `concat_rows([a, b])` | R-matrix-11 |
| `linear_f32_row_no_bias` | `matrix.linear_f32_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix` | torch `F.linear` / External only | Remove | R-matrix-01（nn.kernels） |
| `select_rows_f32` | `matrix.select_rows_f32(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix` | torch `embedding` / External only | Remove | R-matrix-01（nn.kernels） |
| `to_lists` | `matrix.to_lists(m: Matrix) -> List[List[Float]]` | numpy `tolist` / Nx `to_list` | Keep | — |
| `get` | `matrix.get(m: Matrix, row: Int, col: Int) -> Float` | numpy `m[i, j]` (exception)/ Rust ndarray `get` → Option・`m[[i, j]]` → panic / gonum `At`（panic） | Semantic change (`Option[Float]` returned) | R-matrix-08 |
| `rows` | `matrix.rows(m: Matrix) -> Int` | ndarray `nrows` / numpy `shape[0]` | Keep | — |
| `cols` | `matrix.cols(m: Matrix) -> Int` | ndarray `ncols` / numpy `shape[1]` | Keep | — |
| `add` | `matrix.add(a: Matrix, b: Matrix) -> Matrix` | numpy `+` / gonum `Add` / Nx `add` | Remove → `a + b` | R-matrix-14 |
| `mul` | `matrix.mul(a: Matrix, b: Matrix) -> Matrix` | numpy `@`/`matmul` / gonum `Mul` / ndarray `dot` / Nx `dot` | Remove → `a * b` | R-matrix-14 |
| `scale` | `matrix.scale(m: Matrix, s: Float) -> Matrix` | numpy `* s` / gonum `Scale` / Nx `multiply` | Keep | — |
| `fma` | `matrix.fma(a: Matrix, ka: Float, b: Matrix, kb: Float) -> Matrix` | BLAS `axpby` / External only | Remove | R-matrix-02（internal `__fma`） |
| `fma3` | `matrix.fma3(a: Matrix, ka: Float, b: Matrix, kb: Float, c: Matrix, kc: Float) -> Matrix` | External only | Remove | R-matrix-02（internal `__fma3`） |
| `sub` | `matrix.sub(a: Matrix, b: Matrix) -> Matrix` | numpy `-` / gonum `Sub` / Nx `subtract` | Remove → `a - b` | R-matrix-14 |
| `div` | `matrix.div(a: Matrix, b: Matrix) -> Matrix` | numpy `/` / gonum `DivElem` / Nx `divide` | Remove → `a / b` | R-matrix-14 |
| `neg` | `matrix.neg(m: Matrix) -> Matrix` | numpy `-m` / Nx `negate` | Remove → `-m` | R-matrix-14 |
| `pow` | `matrix.pow(m: Matrix, exp: Float) -> Matrix` | numpy `**` / Nx `pow` | Remove → `matrix.map(m, (x) => float.pow(x, e))` | R-matrix-13 |
| `map` | `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` | ndarray `mapv` / gonum `Apply` / Nx `map` | Keep | — |
| `from_bytes_f32_le` | `matrix.from_bytes_f32_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f4')` / Nx `from_binary` | Semantic change (negative shapes abort; insufficient data returns none) | R-matrix-05、R-matrix-15 |
| `to_bytes_f64_le` | `matrix.to_bytes_f64_le(m: Matrix) -> Bytes` | numpy `astype('<f8').tobytes()` / Nx `to_binary` | Keep | — |
| `to_bytes_f32_le` | `matrix.to_bytes_f32_le(m: Matrix) -> Bytes` | numpy `astype('<f4').tobytes()` / Nx `to_binary` | Keep | — |
| `from_bytes_f16_le` | `matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f2')` | Semantic change (negative shapes abort; insufficient data returns none) | R-matrix-05、R-matrix-15 |
| `from_bytes_f64_le` | `matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f8')` | Semantic change (negative shapes abort; insufficient data returns none) | R-matrix-05、R-matrix-15 |
| `broadcast_add_row` | `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` | numpy `m + v`（broadcasting） / Nx `add` | Semantic change | R-matrix-05（length mismatch aborts） |
| `split_cols_even` | `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` | numpy `hsplit` / Nx `split` | Keep | — |
| `concat_cols` | `matrix.concat_cols(matrices: List[Matrix]) -> Matrix` | numpy `hstack` / ndarray `concatenate(Axis(1))` / gonum `Augment` | Keep | — |
| `concat_cols_many` | `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix`（`@deprecated`） | — | Remove | R-matrix-06 |
| `linear_row` | `matrix.linear_row(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix` | torch `F.linear` / Axon `dense` | Remove | R-matrix-01（nn.kernels） |
| `linear_row_no_bias` | `matrix.linear_row_no_bias(x: Matrix, weight: Matrix) -> Matrix` | torch `F.linear(bias=None)` | Remove | R-matrix-01（nn.kernels） |
| `slice_rows` | `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` | numpy `m[a:b]` / ndarray `slice` / gonum `Slice` | Semantic change (negative positions abort; positive side clamps) | R-L0-29、R-L0-46 |
| `conv1d` | `matrix.conv1d(input: Matrix, weight: Matrix, bias: List[Float], kernel: Int, stride: Int, padding: Int) -> Matrix` | torch `conv1d` / Axon `conv` | Remove | R-matrix-01（nn.kernels） |
| `gather_rows` | `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` | numpy `take(axis=0)` / Nx `take` / torch `index_select` | Semantic change | R-matrix-04・R-matrix-05（out of range aborts） |
| `dot_row` | `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` | numpy `m[r] @ v` / ndarray `row(r).dot(v)` | Semantic change | R-matrix-05（out of range aborts） |
| `row_dot` | `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float`（`@deprecated`） | — | Remove | R-matrix-06 |
| `zeros_f32` | `matrix.zeros_f32(rows: Int, cols: Int) -> Matrix[Float32]` | numpy `zeros(dtype=float32)` | Remove | R-matrix-03 |
| `ones_f32` | `matrix.ones_f32(rows: Int, cols: Int) -> Matrix[Float32]` | numpy `ones(dtype=float32)` | Remove | R-matrix-03 |
| `mul_f32` | `matrix.mul_f32(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm` | Remove | R-matrix-03 |
| `mul_f32_scaled` | `matrix.mul_f32_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（alpha） | Remove | R-matrix-03 |
| `mul_f32_t` | `matrix.mul_f32_t(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（transB） | Remove | R-matrix-03 |
| `mul_f32_t_scaled` | `matrix.mul_f32_t_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（alpha, transB） | Remove | R-matrix-03 |
| `mul_scaled` | `matrix.mul_scaled(a: Matrix, alpha: Float, b: Matrix) -> Matrix` | BLAS `dgemm`（alpha） | Remove | R-matrix-02（internal `__mul_scaled`） |
| `zip_with` | `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` | numpy `*` (elementwise) / gonum `MulElem` | Add | R-matrix-09 |
| `concat_rows` | `matrix.concat_rows(matrices: List[Matrix]) -> Matrix` | numpy `vstack` / Nx `concatenate` | Add | R-matrix-11 |
| `to_bytes_f16_le` | `matrix.to_bytes_f16_le(m: Matrix) -> Bytes` | numpy `astype("<f2").tobytes()` | Add | R-matrix-12 |

<a id="l0-への差し戻し処理済み"></a>

## Referrals back to L0 (resolved)

- `get` for fixed-shape containers: apply R-L0-06 item 1 without exception and return `Option` (R-matrix-08).
