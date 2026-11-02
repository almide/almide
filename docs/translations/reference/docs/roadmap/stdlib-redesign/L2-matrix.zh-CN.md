> 描述（固定历史）：标准库重设计 L2/L3 — matrix：完成 epoch 14 的拆分（#3852）：kernels 迁往 nn.kernels，融合目标内部化，撤除 Float32 预览，select_rows 统一为 gather_rows，形状/长度不匹配由返回零改为 abort。
<a id="l2l3-matrix"></a>

# L2/L3: matrix

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-matrix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-matrix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-matrix.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取来源或独立执行。实测、速度与实现陈述为原作者报告，不是当前验证。保留早期数据不足abort与R15 none、早期保留mul与后来仅运算符、一元map与后来zip_with修正、旧get签名与Option提案、46函数无新增与后续新增行的差异。采纳提案与epoch属于历史决定，不证明当前已交付。日文代码占位符指值、默认值、宽度、类型。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-matrix.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-22：标准库保留 `matrix`，完成 #3852 的拆分）。
> 来源：Rust https://docs.rs/ndarray/latest/ndarray/（非标准库）/ Go https://pkg.go.dev/gonum.org/v1/gonum/mat（非标准库）/ Python https://numpy.org/doc/stable/reference/routines.linalg.html（非标准库；只有运算符 `@` 属于语言）/ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/DOMMatrix（仅浏览器）/ Gleam https://gleam-stdlib.hexdocs.pm/（无矩阵）/ Elixir https://hexdocs.pm/nx/Nx.html（外部）/ OCaml https://ocaml.org/manual/latest/api/Bigarray.html（仅容器）/ Kotlin https://github.com/Kotlin/multik（外部）/ Swift https://developer.apple.com/documentation/accelerate（仅 Apple）

九种语言都没有标准矩阵运算（[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) §§1.A/3.9）。L1 R-L1-22 因编译器重写（`@rewrite`、egg 融合）以 `matrix` 运算为目标，而包无法提供重写规则，决定标准库只保留通用稠密矩阵运算。“其他语言”栏采用事实标准的外部库（numpy、gonum、ndarray、Nx）拼写。

实测（`almide 0.68.0 dev 437b8f127`）：

| 表达式 | 结果 | 可读出的结论 |
|---|---|---|
| `matrix.add(2×2, 1×3)` | `1×2` | 形状不匹配时仅计算重叠部分，静默缩小 |
| `matrix.from_bytes_f32_le(4 バイト, 0, 2, 2)` | 全部 `0.0` 的 `2×2` | 数据不足时全部变成 0，包括可读取的开头元素（哨兵值） |
| `matrix.from_bytes_f32_le(4 バイト, 0, 1, 1)` | `1.0` | 数据足够时正确读取 |
| `matrix.select_rows(m, [-1])` / `matrix.gather_rows(m, [-1])` | 第 0 行 / 全零行 | 同一角色的两个函数对负索引处理不同 |
| `matrix.dot_row(m, 5, …)`（两行矩阵） | `0.0` | 越界行返回 0 |
| `matrix.from_lists([[1.0, 2.0], [3.0]])` | abort“rows must have equal length” | 行长度不一致作为违约停止 |
| `matrix.get(m, 9, 9)` | abort | 索引越界时停止 |

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust（ndarray） | Go（gonum） | Python（numpy） | TS/JS | Gleam | Elixir（Nx） | OCaml | Kotlin（multik） | Swift（Accelerate） |
|---|---|---|---|---|---|---|---|---|---|---|
| 构造填满 0/1 的矩阵 | `zeros` `ones` | `zeros` `ones` | `NewDense(r, c, nil)` | `zeros` `ones` | — | — | `broadcast(0, …)` | `Bigarray.Array2.create` | `zeros` `ones` | — |
| 列表与矩阵互转 | `from_lists` `to_lists` | `from_shape_vec` | `NewDense(r, c, data)` | `array` `tolist` | — | — | `tensor` `to_list` | — | `mk.ndarray` | — |
| 形状 | `shape` `rows` `cols` | `shape` `nrows` `ncols` | `Dims` | `shape` | — | — | `shape` | `dim1` `dim2` | `shape` | — |
| 读取元素 | `get`（当前越界 abort；R-matrix-08 改为 `Option`） | `m[[i, j]]`（panic） | `At`（panic） | `m[i, j]`（异常） | — | — | `m[i][j]` | `get` | `m[i, j]` | — |
| 转置 | `transpose` | `t()` | `T()` | `.T` | — | — | `transpose` | — | `transpose` | `vDSP_mtrans` |
| 逐元素和/差/商/取负/幂 | `add` `sub` `div` `neg` `pow` | `+` `-` `/` | `Add` `Sub` `DivElem` | `+` `-` `/` `**` | — | — | `add` `subtract` `divide` `pow` | — | `+` `-` `/` | `vDSP` |
| 逐元素乘积 | — | `*` | `MulElem` | `*` | — | — | `multiply` | — | `*` | `vDSP_vmul` |
| 矩阵乘积 | `mul` | `dot` | `Mul` | `@` / `matmul` | — | — | `dot` | — | `dot` | `cblas_dgemm` |
| 标量乘法/应用函数 | `scale` `map` | `* s` `mapv` | `Scale` `Apply` | `* s` `vectorize` | — | — | `multiply` `map` | — | `* s` `map` | — |
| 提取行 | `slice_rows` `gather_rows` `select_rows` | `slice` `select` | `Slice` | `m[a:b]` `m[idx]` | — | — | `slice` `take` | `sub` | `m[a..b]` | — |
| 拼接/拆分 | `append_rows` `concat_cols` `split_cols_even` | `concatenate` | `Stack` `Augment` | `vstack` `hstack` `split` | — | — | `concatenate` `split` | — | `cat` | — |
| 行向量加法/与行求点积 | `broadcast_add_row` `dot_row` | 广播 `+` | — | 广播 `+` `m[i] @ v` | — | — | `add`（广播） | — | — | — |
| 字节转换（f16/f32/f64） | `from_bytes_*_le` `to_bytes_*_le` | `from_shape_vec` + `bytemuck` | `binary.Read` | `frombuffer` `tobytes` | — | — | `from_binary` `to_binary` | — | — | — |
| 推理 kernels | `linear_row` `conv1d` `linear_f32_row_no_bias` `select_rows_f32` | 外部 | 外部 | `torch` | — | — | `Axon` | — | — | — |
| 32 位矩阵 | `Matrix[Float32]`（预览；只有两个构造器与四种乘积） | `Array2<f32>` | — | `float32` | — | — | `{:f, 32}` | — | `D2Array<Float>` | `cblas_sgemm` |

<a id="完全性の規則"></a>

### 完备性规则

1. **标准库只保留通用稠密矩阵运算**（R-L1-22、#3852）。上述推理 kernels 及融合目标（`fma`、`fma3`、`mul_scaled`）不公开。`broadcast_add_row` 与 `split_cols_even` 保留为通用运算（对应 numpy/Nx 的广播与 `split`，不预设推理用途；这是 #3852 要求裁决的两项）。
2. **元素类型只有 `Float` 一种**。要公开 `Matrix[Float32]`，必须具备与 `Matrix` 完全相同的运算表，并通过 gate 按族矩阵检查（CLAUDE.md 的 family 规则）。标准库没有使用方，因此目前不提供（R-matrix-03）。
3. **形状/长度/索引不匹配时 abort**（R-L0-06 第 3、4 项）。由调用方指定形状的函数（逐元素运算、矩阵乘法、行向量加法、字节构造、行提取、行点积）遇到形状/长度不匹配即 abort。调用前可用 `rows`、`cols`、`bytes.len` 检查维度/长度，因此不匹配属于违约（与 `list.chunk(xs, 0)` 相同）。不得返回哨兵零或小矩阵（R-L0-06 第 4 项）。`from_lists` 已遵循此规则。
   - 只有单元素 `get` 作为无后缀提取返回 `Option`（R-L0-06 第 1 项、R-matrix-08）。
   - 只有 `slice_rows` 的范围遵循 R-L0-12（超出端点时夹取）。
   - matrix 没有位置写入函数（所有运算返回副本），故没有函数属于 L0 的“越界写入 abort”。
4. 对已计划撤除但未定 epoch 的别名（`concat_cols_many`、`row_dot`），按 R-L0-14 的窗口撤除。

<a id="l2-の判定"></a>

### L2 判定

- 五个推理 kernels → R-matrix-01（`almide-graphics/nn`）。
- 三个融合目标 → R-matrix-02（内部 `__` 名称）。
- 六个 32 位预览函数 → R-matrix-03（撤除）。
- 重复：`select_rows`/`gather_rows` → R-matrix-04；`concat_cols_many`/`row_dot` → R-matrix-06（#3851 A）。
- 哨兵与静默截短 → R-matrix-05。
- 缺少的角色：逐元素乘法（外部库多数具备）。因 `matrix.mul` 表示矩阵乘法，需要命名裁决；可以用 `map` 与 `from_lists` 编写，且没有语言的标准库提供矩阵，因此不新增（R-matrix-07 否决）。

<a id="案"></a>

## 提案

<a id="r-matrix-01-推論用のカーネルを-almide-graphicsnn-へ移す---採用"></a>

### R-matrix-01 推理 kernels 迁往 `almide-graphics/nn` — ○ 采纳

- 内容：从标准库撤除 `linear_f32_row_no_bias`、`select_rows_f32`、`linear_row`、`linear_row_no_bias`、`conv1d`，以相同名称/参数放入 `almide-graphics/nn`。nn.kernels 用通用运算实现（`linear_row(x, w, b)` = `broadcast_add_row(mul(x, transpose(w)), b)`；两个 f32 函数先经 `from_bytes_f32_le`，之后相同）。
- L0 依据：L1 R-L1-22、epoch 14 裁决（NN kernels 属于 nn.kernels）。
- (a) 没有语言在标准库提供推理 kernels。
- (b) nn.kernels 是纯 Almide（`src/kernels.almd`，无 intrinsics），因此可在 native/wasm 运行。当前两个 f32 函数是直接读取 GGUF 缓冲区的 intrinsic，迁移后可能变慢。性能属于 nn.kernels 的议题，不属于标准表面裁决。
- (c) 同族 kernels（`linear_q8_0_row_no_bias` 等）已在 epoch 14 迁往 nn.kernels。半数留在标准库会使 LLM 无法预测所在位置。
- (d) 撤除 5 个。按 #3852 先发布 almide-graphics/nn，让 `@deprecated(since=15, use="kernels.linear_row")` 等提示能指向该版本。撤除 epoch 16。`almide fix`：`matrix.<fn>(…)` → 添加 `import nn.kernels`，使用 `kernels.<fn>(…)`。用例数（spec/tests/docs/stdlib）：`linear_f32_row_no_bias` 9/0/5/0、`select_rows_f32` 10/0/6/0、`linear_row` 8/0/4/0、`linear_row_no_bias` 2/0/4/1、`conv1d` 15/0/5/0。nn.kernels 自身调用 `matrix.linear_row`（2）、`linear_row_no_bias`（2）、`linear_f32_row_no_bias`（1），改为 nn.kernels 内定义。替换 CHEATSHEET:1066 的 `linear_row` 示例。
- gate 提案：在同一变更中重新生成 `crates/almide-types/src/self_host_registry.rs`、`docs/stdlib/matrix.md` 索引、`since.toml`、semantics-manifest、inspection-ledger 行；以 `check_stdlib_fn_count` 检查公开数减少。
- Issue：#3852（§1）

<a id="r-matrix-02-fusion-の対象を内部の名前にする---採用"></a>

### R-matrix-02 融合目标改为内部名称 — ○ 采纳

- 内容：从公开表面撤除 `mul_scaled`、`fma`、`fma3`，只允许 `@rewrite`/egg 规则生成 `__mul_scaled`、`__fma`、`__fma3`。公开写法仅为组合（`matrix.scale(matrix.mul(a, b), s)`、`matrix.add(matrix.scale(a, ka), matrix.scale(b, kb))`）。
- L0 依据：R-L0-01（同一计算两种写法），也属于其转发情形（只是调用其他公开函数组合一次的函数）。
- (b) 重写发现组合并替换为内部函数，因此速度不变。`crates/almide-codegen/src/target.rs:81-85` 明言这些重写只为速度，spec 不依赖它们。
- (c) 消除让 LLM 猜测“显式使用是否更快、不使用是否相同”的函数。
- (d) 撤除 3 个（函数体内部保留）。`@deprecated(since=15, use="matrix.scale(matrix.mul(a, b), s)")` 等，撤除 epoch 16。`almide fix` 一对一展开为组合。用例数：`mul_scaled` 3/0/3/2、`fma` 13/2/5/1、`fma3` 6/2/3/0。
- gate 提案：将 `tests/matrix_fma_overlap_cost_test.rs` 等现有测试改为组合，检查重写到内部函数。
- Issue：#3852（§1）

<a id="r-matrix-03-32-bit-の試験版-matrixfloat32-を撤去する---採用"></a>

### R-matrix-03 撤除 32 位预览 `Matrix[Float32]` — ○ 采纳

- 内容：撤除 `zeros_f32`、`ones_f32`、`mul_f32`、`mul_f32_scaled`、`mul_f32_t`、`mul_f32_t_scaled`。f32 数据通过 `from_bytes_f32_le` 读为 `Matrix`（Float）。
- L0 依据：完备性规则 2（CLAUDE.md“按矩阵展开族”）。#3852 要求补全运算表或迁往 nn.kernels。后者不可能（nn.kernels 是没有 intrinsics 的纯 Almide，标准库外无构造 `Matrix[Float32]` 的路径）。补全需像 `Matrix` 一样约 30 格，除 spec 外没有使用方。
- (a) 外部库对每种元素类型提供相同运算表（numpy dtype、ndarray 泛型），没有库仅提供两个构造器与四种乘积。
- (b) runtime 的 SmallF32 存储（`cblas_sgemm`）可保留内部，只撤除公开表面。
- (c) 能构造 `Matrix[Float32]`，但与 `add` 类型不符，LLM 编写的代码马上走入死路。
- (d) 撤除 6 个。`@deprecated(since=15, use="matrix.zeros")` 等，撤除 epoch 16。因类型变化，`almide fix` 不能机械重写；使用 `removed_spellings.rs` 提示，指向 `from_bytes_f32_le` 与 Float 矩阵。用例数：`zeros_f32` 2/0/3/0、`ones_f32` 18/8/3/0、`mul_f32` 3/5/3/0、`mul_f32_scaled` 2/1/3/0、`mul_f32_t` 2/0/3/0、`mul_f32_t_scaled` 2/0/3/0（大多在 `spec/stdlib/matrix_f32_test.almd`）。
- 重启条件：标准库或 nn.kernels 出现 f32 使用方时，按与 `Matrix` 相同的完整运算表/gates 重新引入 `Matrix[Float32]`。
- Issue：#3852（§1 Float32 preview）

<a id="r-matrix-04-select_rows-を撤去しgather_rows-に一本化する---採用"></a>

### R-matrix-04 撤除 `select_rows`，统一为 `gather_rows` — ○ 采纳

- 内容：撤除 `select_rows`；R-matrix-05 使 `gather_rows` 遇越界索引时 abort。
- L0 依据：R-L0-01（同一角色有两个仅边界行为不同的函数）。
- (c) 名称无法透露边界差异（负索引取第 0 行或全零行）。CHEATSHEET:1066 使用 `gather_rows`。
- (d) 撤除。`@deprecated(since=15, use="matrix.gather_rows")`，撤除 epoch 16。`almide fix` 改名（提示注明负索引语义变化）。用例数：`select_rows` 0/0/3/0、`gather_rows` 5/0/4/0。
- Issue：#3852（§1）

<a id="r-matrix-05-形長さ添字の不一致を-abort-にする---採用"></a>

### R-matrix-05 形状/长度/索引不匹配时 abort — ○ 采纳

- 内容：以下情况不再静默返回较小结果或零，而是 abort 并指出不匹配：
  - `add`、`sub`、`div`：形状不同。
  - `mul`：`cols(a) != rows(b)`。
  - `broadcast_add_row`：`bias` 长度不等于列数。
  - `from_bytes_f16_le`、`from_bytes_f32_le`、`from_bytes_f64_le`：`offset + rows * cols * 幅` 超过数据长度。
  - `gather_rows`：负索引或索引不小于行数。
  - `dot_row`：行越界或 `vec` 长度不等于列数。
- L0 依据：R-L0-06 第 3 项（仅调用方违约时 abort）、第 4 项（可能失败的构造不返回哨兵）。形状由调用方选择/指定，能在调用前检查，故不匹配是调用方违约。`from_lists` 已遵循此规则。
- (a) numpy、gonum、ndarray、Nx 对形状不匹配使用异常/panic，没有外部库只计算重叠部分。
- (b) 仅增加纯计算检查，两目标的 abort 文案相同（由 wasm_cross fixtures 固定）。
- (c) LLM 代码形状错误时，目前会继续带着小矩阵/零运行，很晚才产生错误答案。及时停止能局部修正。
- (d) 边界语义变更。原来正确返回的输入结果不变，只有此前返回无意义值的输入会 abort（缩小定义域，不是 R-L0-14 第 4 项“同名异义”；如 L0 的位置写入，有效结果不变，故不计入 epoch `breaks`）。不改名、不加 `@deprecated`。abort 指出不匹配形状和函数。引入前统计 specs/tests/nn.kernels/下游中的形状不匹配调用（nn.kernels 的 `shape_mismatch()` 使用 `append_rows`，不在本次范围）。
- gate 提案：每个函数一条不匹配输入 abort fixture，契约账本新增 C 编号（als 优先）。
- Issue：#3927

<a id="r-matrix-06-撤去予定の別名-concat_cols_manyrow_dot-を-epoch-16-で撤去する---採用"></a>

### R-matrix-06 epoch 16 撤除已计划别名 `concat_cols_many`/`row_dot` — ○ 采纳

- 内容：对两个已有 `@deprecated(since=5)` 的函数增加 `[[deprecation]]` 区块（撤除 epoch 16）。
- L0 依据：R-L0-14，与 R-L0-11（#3851 A）相同处理。
- (d) 撤除。已有 `@deprecated(since=5, use="matrix.concat_cols")` / `@deprecated(since=5, use="matrix.dot_row")`。`removal_epoch = 16`。`almide fix` 一对一改名（`concat_cols_many` → `concat_cols`、`row_dot` → `dot_row`）。用例数：`concat_cols_many` 1/2/5/1、`row_dot` 2/3/6/0。
- Issue：#3851 A

<a id="r-matrix-08-matrixget-は範囲外で-option-を返す---採用"></a>

### R-matrix-08 `matrix.get` 越界返回 `Option` — ○ 采纳

- 内容：`matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]`。范围内返回 `some(値)`，否则 none。
- L0 依据：R-L0-06 第 1 项（无后缀提取安全）。`list.get`、`map.get`、`bytes.get` 都返回 `Option`，使“无后缀 `get` 返回 `Option`”没有例外。
- (a) Rust ndarray 区分返回 `Option` 的 `get` 与会 panic 的索引 `m[[i, j]]`。Almide 对齐 `get` 一侧。
- (b) 无影响。
- (c) 若某模块的 `get` 停止、另一模块返回 none，名称无法说明是否需写 `??`。逐元素读取循环通常应采用 `matrix.map`/`matrix.to_lists` 等全矩阵运算，不推荐每个元素串接 `??`。
- (d) 返回类型变更。旧调用产生类型错误，不会静默改义（R-L0-14 第 4 项）。不改名，故无 `@deprecated`；类型错误提示 `matrix.get(m, i, j) ?? 既定値`。`almide fix` 改为 `matrix.get(m, i, j) ?? panic("matrix.get: out of range")`，保留现有越界停止行为。用例数在 PR 中记录。
- Issue：#3927

<a id="r-matrix-07-mul-を-matmul-に改名し要素ごとの積を足す---不採用-r-matrix-14-で--に置き換え"></a>

### R-matrix-07 将 `mul` 改名为 `matmul` 并新增逐元素乘法 — × 否决（→ R-matrix-14 改为 `*`）

- 内容：不变，`mul` 保持矩阵乘法。
- L0 依据：R-L0-00（九种标准库都无矩阵，因此无多数；外部也分为 gonum `Mul`、Nx/ndarray `dot`、numpy `matmul`），R-L0-14 第 4 项（不能将 `mul` 改作逐元素乘法）。
- (c) 第六轮追加：R-matrix-14 撤除 `mul`，令运算符 `*` 表示矩阵乘法。docs/CHEATSHEET 文案：“`*` 是矩阵乘法；逐元素乘法用 `map`”。R-matrix-05 在形状不匹配时停止，因此混淆会在运行时被发现。
- Issue：无

<a id="r-matrix-09-要素ごとの二項演算を-matrixzip_with-で書けるようにする---採用"></a>

### R-matrix-09 用 `matrix.zip_with` 表达逐元素二元运算 — ○ 采纳

- 现状：逐元素运算只有 `add`/`sub`/`div`，没有乘法。否决案 R-matrix-07 称“逐元素乘法用 `map`”，但 `matrix.map` 接受一元函数，不能表达两个矩阵的乘积（R-matrix-07 的新证据）。
- 提案：`fn matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix`。与 R-matrix-05 一样，形状不匹配时 abort。
- L0 依据：R-L0-10（`_with`，与 `list.zip_with` 同拼写）。
- (a) 多数外部库有逐元素乘法（numpy `*`、gonum `MulElem`、Nx `multiply`）。
- (c) 将 `matrix.mul` 误认为逐元素乘法的 LLM 有了修正去向。
- (d) 新增 1 个，保留 `mul`（矩阵乘法）的含义。
- Issue：#3972

<a id="r-matrix-11-行方向の連結を-concat_rowslistmatrix-にしconcat_cols-と形をそろえる---採用"></a>

### R-matrix-11 行拼接改为 `concat_rows(List[Matrix])`，对齐 `concat_cols` — ○ 采纳

- 现状：`matrix.append_rows(base, extra)` 接受两个值，`matrix.concat_cols(matrices: List[Matrix])` 接受列表。
- 提案：新增 `fn matrix.concat_rows(matrices: List[Matrix]) -> Matrix`，撤除 `append_rows`。
- L0 依据：R-L0-01（仅轴不同则形状一致）、R-L0-04 第 2 项（`append_` 留给原地 `mut` 动词）。
- (a) numpy `vstack`/`hstack`、Nx `concatenate`、ndarray `concatenate` 两轴都接受列表。
- (d) 新增 1 个/撤除 1 个。`@deprecated(since=15, use="matrix.concat_rows")`；`almide fix` 把 `append_rows(a, b)` 改为 `concat_rows([a, b])`。
- Issue：#3972

<a id="r-matrix-12-to_bytes_f16_le-を足してバイト列の変換を対にする---採用"></a>

### R-matrix-12 新增 `to_bytes_f16_le`，字节转换成对 — ○ 采纳

- 提案：`fn matrix.to_bytes_f16_le(m: Matrix) -> Bytes`（按 IEEE nearest-even 舍入至 f16）。
- L0 依据：R-L0-07 第 4 项（方向配对）。宽度 {f16, f32, f64} × 方向 {from, to} 共六格。
- (d) 新增 1 个。
- Issue：#3972

<a id="r-matrix-13-matrixpow-を撤去する---採用"></a>

### R-matrix-13 撤除 `matrix.pow` — ○ 采纳

- 现状：`fn matrix.pow(m: Matrix, exp: Float) -> Matrix` 是逐元素幂，只调用 `matrix.map(m, (x) => float.pow(x, e))` 一次。数学上的 A^2 表示矩阵乘法，容易误读。
- L0 依据：R-L0-01（仅调用其他公开函数一次的函数是同义词）。
- (d) 撤除。`almide fix` 改为 `matrix.map(m, (x) => float.pow(x, e))`。用例数 6/0/3/0。
- Issue：#3973

<a id="r-matrix-14-matrix-の-addsubmuldivneg-を公開しない演算子だけにする---採用"></a>

### R-matrix-14 不公开矩阵 `add`/`sub`/`mul`/`div`/`neg`（仅运算符）— ○ 采纳

- 现状：`a + b`、`a - b`、`a * b`（矩阵乘法）、`a / b` 会脱糖，同时相同计算仍作为公开函数存在。
- 提案：将 R-bigint-04 的规则（R-L0-26：不公开运算符脱糖目标）应用于 matrix，撤除五个函数。一元 `-` 在修复 #3965（目前 internal error）后脱糖。
- L0 依据：R-L0-01/R-L0-26。只应用于 bigint 会令具备运算符的类型规则不一。
- (c) `a |> matrix.add(b)` 的管道写法有优势，但统一为 `a + b` 能减少生成代码的变体。
- (d) 撤除 5 个。`@deprecated(since=15, use="+")` 等，epoch 16 撤除。`almide fix` 将 `matrix.add(a, b)` 改为 `a + b`。
- Issue：#3973

<a id="r-matrix-15-from_bytes__le-はデータが足りないと-none-を返すr-matrix-05-の見直し---採用"></a>

### R-matrix-15 `from_bytes_*_le` 数据不足时返回 none（修订 R-matrix-05）— ○ 采纳

- 现状：R-matrix-05 选择数据不足时 abort（当前是全零哨兵），但 `bytes.get_f32_le_array` 等字节读取器在不足时返回 none。外部文件读取改用 Matrix 后，可恢复失败变成程序终止。
- 提案：区分两种情况。`rows`/`cols`/`offset` 为负是调用方错误，仍 abort。从 `offset` 开始不足以读取 `rows × cols` 个元素的字节是输入问题，通过 `-> Matrix?` 返回 none。
  - `fn matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?`（f32/f64 相同）
- L0 依据：R-L0-06（可失败的无后缀提取返回 Option）。与 bytes 的 `get_<型>_array` 失败形态相同。
- (d) 语义变更。返回类型变化，旧调用产生类型错误而不是静默改变结果（R-L0-14 第 4 项）。`almide fix` 给出等同于 `?? abort` 的提示，而非末尾 `!`。
- Issue：#3978

<a id="l3-関数表"></a>

## L3：函数表

当前 `matrix` 的 46 个函数（无新增）。

| 函数 | 签名（当前，显示 mut） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `zeros` | `matrix.zeros(rows: Int, cols: Int) -> Matrix` | numpy `zeros` / ndarray `zeros` / Nx `broadcast` / multik `zeros` | 保留 | — |
| `ones` | `matrix.ones(rows: Int, cols: Int) -> Matrix` | numpy `ones` / ndarray `ones` / multik `ones` | 保留 | — |
| `shape` | `matrix.shape(m: Matrix) -> (Int, Int)` | numpy `shape` / ndarray `shape` / gonum `Dims` / Nx `shape` | 保留 | — |
| `transpose` | `matrix.transpose(m: Matrix) -> Matrix` | numpy `.T` / ndarray `t()` / gonum `T()` / Nx `transpose` | 保留 | — |
| `from_lists` | `matrix.from_lists(rows: List[List[Float]]) -> Matrix` | numpy `array` / Nx `tensor` / gonum `NewDense` | 保留 | — |
| `select_rows` | `matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix` | numpy `m[idx]` / ndarray `select` / Nx `take` | 撤除 | R-matrix-04 |
| `append_rows` | `matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix` | numpy `vstack` / ndarray `concatenate(Axis(0))` / Nx `concatenate` | 撤除 → `concat_rows([a, b])` | R-matrix-11 |
| `linear_f32_row_no_bias` | `matrix.linear_f32_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix` | torch `F.linear` / 仅外部 | 撤除 | R-matrix-01（nn.kernels） |
| `select_rows_f32` | `matrix.select_rows_f32(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix` | torch `embedding` / 仅外部 | 撤除 | R-matrix-01（nn.kernels） |
| `to_lists` | `matrix.to_lists(m: Matrix) -> List[List[Float]]` | numpy `tolist` / Nx `to_list` | 保留 | — |
| `get` | `matrix.get(m: Matrix, row: Int, col: Int) -> Float` | numpy `m[i, j]`（异常）/ Rust ndarray `get` → Option・`m[[i, j]]` → panic / gonum `At`（panic） | 语义变更（`Option[Float]` 返回） | R-matrix-08 |
| `rows` | `matrix.rows(m: Matrix) -> Int` | ndarray `nrows` / numpy `shape[0]` | 保留 | — |
| `cols` | `matrix.cols(m: Matrix) -> Int` | ndarray `ncols` / numpy `shape[1]` | 保留 | — |
| `add` | `matrix.add(a: Matrix, b: Matrix) -> Matrix` | numpy `+` / gonum `Add` / Nx `add` | 撤除 → `a + b` | R-matrix-14 |
| `mul` | `matrix.mul(a: Matrix, b: Matrix) -> Matrix` | numpy `@`/`matmul` / gonum `Mul` / ndarray `dot` / Nx `dot` | 撤除 → `a * b` | R-matrix-14 |
| `scale` | `matrix.scale(m: Matrix, s: Float) -> Matrix` | numpy `* s` / gonum `Scale` / Nx `multiply` | 保留 | — |
| `fma` | `matrix.fma(a: Matrix, ka: Float, b: Matrix, kb: Float) -> Matrix` | BLAS `axpby` / 仅外部 | 撤除 | R-matrix-02（内部 `__fma`） |
| `fma3` | `matrix.fma3(a: Matrix, ka: Float, b: Matrix, kb: Float, c: Matrix, kc: Float) -> Matrix` | 仅外部 | 撤除 | R-matrix-02（内部 `__fma3`） |
| `sub` | `matrix.sub(a: Matrix, b: Matrix) -> Matrix` | numpy `-` / gonum `Sub` / Nx `subtract` | 撤除 → `a - b` | R-matrix-14 |
| `div` | `matrix.div(a: Matrix, b: Matrix) -> Matrix` | numpy `/` / gonum `DivElem` / Nx `divide` | 撤除 → `a / b` | R-matrix-14 |
| `neg` | `matrix.neg(m: Matrix) -> Matrix` | numpy `-m` / Nx `negate` | 撤除 → `-m` | R-matrix-14 |
| `pow` | `matrix.pow(m: Matrix, exp: Float) -> Matrix` | numpy `**` / Nx `pow` | 撤除 → `matrix.map(m, (x) => float.pow(x, e))` | R-matrix-13 |
| `map` | `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` | ndarray `mapv` / gonum `Apply` / Nx `map` | 保留 | — |
| `from_bytes_f32_le` | `matrix.from_bytes_f32_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f4')` / Nx `from_binary` | 语义变更（负形状 abort、数据不足 none） | R-matrix-05、R-matrix-15 |
| `to_bytes_f64_le` | `matrix.to_bytes_f64_le(m: Matrix) -> Bytes` | numpy `astype('<f8').tobytes()` / Nx `to_binary` | 保留 | — |
| `to_bytes_f32_le` | `matrix.to_bytes_f32_le(m: Matrix) -> Bytes` | numpy `astype('<f4').tobytes()` / Nx `to_binary` | 保留 | — |
| `from_bytes_f16_le` | `matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f2')` | 语义变更（负形状 abort、数据不足 none） | R-matrix-05、R-matrix-15 |
| `from_bytes_f64_le` | `matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f8')` | 语义变更（负形状 abort、数据不足 none） | R-matrix-05、R-matrix-15 |
| `broadcast_add_row` | `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` | numpy `m + v`（广播） / Nx `add` | 语义变更 | R-matrix-05（长度不同 abort） |
| `split_cols_even` | `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` | numpy `hsplit` / Nx `split` | 保留 | — |
| `concat_cols` | `matrix.concat_cols(matrices: List[Matrix]) -> Matrix` | numpy `hstack` / ndarray `concatenate(Axis(1))` / gonum `Augment` | 保留 | — |
| `concat_cols_many` | `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix`（`@deprecated`） | — | 撤除 | R-matrix-06 |
| `linear_row` | `matrix.linear_row(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix` | torch `F.linear` / Axon `dense` | 撤除 | R-matrix-01（nn.kernels） |
| `linear_row_no_bias` | `matrix.linear_row_no_bias(x: Matrix, weight: Matrix) -> Matrix` | torch `F.linear(bias=None)` | 撤除 | R-matrix-01（nn.kernels） |
| `slice_rows` | `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` | numpy `m[a:b]` / ndarray `slice` / gonum `Slice` | 语义变更（负位置 abort；正侧夹取） | R-L0-29、R-L0-46 |
| `conv1d` | `matrix.conv1d(input: Matrix, weight: Matrix, bias: List[Float], kernel: Int, stride: Int, padding: Int) -> Matrix` | torch `conv1d` / Axon `conv` | 撤除 | R-matrix-01（nn.kernels） |
| `gather_rows` | `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` | numpy `take(axis=0)` / Nx `take` / torch `index_select` | 语义变更 | R-matrix-04・R-matrix-05（越界 abort） |
| `dot_row` | `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` | numpy `m[r] @ v` / ndarray `row(r).dot(v)` | 语义变更 | R-matrix-05（越界 abort） |
| `row_dot` | `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float`（`@deprecated`） | — | 撤除 | R-matrix-06 |
| `zeros_f32` | `matrix.zeros_f32(rows: Int, cols: Int) -> Matrix[Float32]` | numpy `zeros(dtype=float32)` | 撤除 | R-matrix-03 |
| `ones_f32` | `matrix.ones_f32(rows: Int, cols: Int) -> Matrix[Float32]` | numpy `ones(dtype=float32)` | 撤除 | R-matrix-03 |
| `mul_f32` | `matrix.mul_f32(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm` | 撤除 | R-matrix-03 |
| `mul_f32_scaled` | `matrix.mul_f32_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（alpha） | 撤除 | R-matrix-03 |
| `mul_f32_t` | `matrix.mul_f32_t(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（transB） | 撤除 | R-matrix-03 |
| `mul_f32_t_scaled` | `matrix.mul_f32_t_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（alpha, transB） | 撤除 | R-matrix-03 |
| `mul_scaled` | `matrix.mul_scaled(a: Matrix, alpha: Float, b: Matrix) -> Matrix` | BLAS `dgemm`（alpha） | 撤除 | R-matrix-02（内部 `__mul_scaled`） |
| `zip_with` | `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` | numpy `*`（逐元素） / gonum `MulElem` | 新增 | R-matrix-09 |
| `concat_rows` | `matrix.concat_rows(matrices: List[Matrix]) -> Matrix` | numpy `vstack` / Nx `concatenate` | 新增 | R-matrix-11 |
| `to_bytes_f16_le` | `matrix.to_bytes_f16_le(m: Matrix) -> Bytes` | numpy `astype("<f2").tobytes()` | 新增 | R-matrix-12 |

<a id="l0-への差し戻し処理済み"></a>

## 向 L0 回报（已解决）

- 固定形状容器的 `get`：无例外应用 R-L0-06 第 1 项，返回 `Option`（R-matrix-08）。
