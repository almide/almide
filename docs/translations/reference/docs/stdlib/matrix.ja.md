# matrix

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](matrix.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](matrix.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](matrix.zh-CN.md)



> 原文：[matrix.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/matrix.md)。読者向けの非規範的な固定スナップショットです。自動生成・自動同期の対象ではありません。署名・型・`@since` とコード（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例・性能・安全性・ターゲット間の一致は、この翻訳作業では独立実行・検証していません。

> 形状・空値・符号・範囲の規則は固定原文の主張です。原文の slice_rows(m, 5, 9) が空になるという記述には m の行数条件が明記されておらず、一般的な空値保証として追加解釈しません。split_cols_even の本文 n <= 0 と索引 n = 0 の説明差も保持しています。

2次元行列の操作。自動インポートされ、`import matrix` は任意です。

> **方言エポック14：** ニューラルネットワークのカーネル（attention、RoPE、行の正規化、活性化、Q1_0/Q8_0 ローダー、Qwen3 ブロックの25関数）はパッケージ [almide-graphics/nn](https://github.com/almide-graphics/nn)（モジュール `nn.kernels`）へ移動しました。`almide add almide-graphics/nn@v0.2.0 --trust github.com/almide-graphics`、`import nn.kernels` とし、`matrix.gelu(m)` の代わりに同じ引数で `kernels.gelu(m)` を使います。古い表記の呼び出しは、新しい表記を示す検査時エラーになります。
>
> **方言エポック15：** 算術関数は演算子になりました（`add` は `a + b`、`sub`、`mul`、`div` も同様）。密なカーネル `linear_row`、`linear_row_no_bias`、`linear_f32_row_no_bias`、`conv1d`、`select_rows_f32` は almide-graphics/nn に移動し、`_f32` 積の融合操作は `matrix.transpose` / `matrix.scale` を伴う `*` です。

`Matrix` は行優先の密な `Float` 値の行列（`f64`）です。すべての操作は値を不変として扱い、組み合わせ操作は新しい行列を返します。

<a id="naming-convention"></a>

## 命名規則

- `<op>_rows` — 各行を独立に処理し Matrix を返します（`slice_rows`、`gather_rows`、`broadcast_add_row`）。
- `<op>_cols` — 列方向の版（`split_cols_even`、`concat_cols`）。
- `<verb>_<noun>` — 最初の語は関数の動作（`from_lists`、`to_bytes_f64_le`、`dot_row`）。
- 接尾辞 `_<dtype>_le` はバイト変換のエンディアンを明示します。

<a id="constructors"></a>

## 構築

| 署名 | 目的 |
|---|---|
| `matrix.zeros(rows: Int, cols: Int) -> Matrix` | ゼロ埋め |
| `matrix.ones(rows: Int, cols: Int) -> Matrix` | 1埋め |
| `matrix.from_lists(rows: List[List[Float]]) -> Matrix` | 入れ子のリストから構築 |
| `matrix.from_bytes_f64_le(b, offset, rows, cols) -> Option[Matrix]` | 平坦な f64 LE バイトを読む。データが収まらなければ `none` |
| `matrix.from_bytes_f32_le(b, offset, rows, cols) -> Option[Matrix]` | 平坦な f32 LE を読む（f64 に拡張）。データが収まらなければ `none` |
| `matrix.from_bytes_f16_le(b, offset, rows, cols) -> Option[Matrix]` | 平坦な IEEE-754 半精度を読む（GGML 重み）。データが収まらなければ `none` |

<a id="conversions"></a>

## 変換

| 署名 | 目的 |
|---|---|
| `matrix.to_lists(m) -> List[List[Float]]` | 入れ子のリストとして実体化 |
| `matrix.to_bytes_f64_le(m) -> Bytes` | 平坦な f64 LE（`from_bytes_f64_le` と対称） |
| `matrix.to_bytes_f32_le(m) -> Bytes` | 平坦な f32 LE（各セルの精度を縮小） |
| `matrix.transpose(m) -> Matrix` | 転置 |

<a id="inspection"></a>

## 調査

| 署名 | 目的 |
|---|---|
| `matrix.shape(m) -> (Int, Int)` | `(rows, cols)` |
| `matrix.rows(m) -> Int` | 行数 |
| `matrix.cols(m) -> Int` | 列数 |
| `matrix.get(m, r, c) -> Option[Float]` | 要素取得。範囲外は `none`（C-282） |

<a id="arithmetic"></a>

## 算術

特記がなければ要素ごとです。`a * b` は行列積で、`cols(a) == rows(b)` が必要です（下記の**形状と定義域**を参照）。一方、要素ごとの演算子（`+` / `-` / `/` および `broadcast_add_row`）は短いオペランドに合わせて zip で切り詰め、両ターゲットで同一に動作します。

| 式 | 目的 |
|---|---|
| `a + b` | 要素ごとの和 |
| `a - b` | 要素ごとの差 |
| `a * b` | **行列乗算**（要素ごとではない） |
| `a / b` | 要素ごとの商 |
| `matrix.scale(m, s) -> Matrix` | 各セルを `s` 倍。`matrix.scale(m, -1.0)` は符号反転 |
| `matrix.map(m, f) -> Matrix` | 各セルに `(Float) -> Float` を適用。例：`(x) => float.pow(x, e)` |
| `matrix.concat_rows(ms: List[Matrix]) -> Matrix` | 各要素の行を上から下へ連結 |

<a id="slicing--assembly"></a>

## 切り出しと組み立て

| 署名 | 目的 |
|---|---|
| `matrix.slice_rows(m, start, end) -> Matrix` | 半開の行範囲 |
| `matrix.gather_rows(m, indices: List[Int]) -> Matrix` | 添字リストで行を選ぶ（トークン埋め込み参照など） |
| `matrix.split_cols_even(m, n) -> List[Matrix]` | 列を `n` 個の等しい塊に分割 |
| `matrix.concat_cols(matrices: List[Matrix]) -> Matrix` | 列方向に連結（非推奨別名：`concat_cols_many`） |
| `matrix.dot_row(m, r, vec: List[Float]) -> Float` | 行 `r` と `vec` の内積（非推奨別名：`row_dot`） |

<a id="neural-network-primitives"></a>

## ニューラルネットワークの基本操作

| 署名 | 目的 |
|---|---|
| `matrix.broadcast_add_row(m, row: List[Float]) -> Matrix` | m の各行にバイアスリストを加算（zip で切り詰め） |

密なカーネル（`linear_row`、`conv1d` など）はパッケージ almide-graphics/nn のモジュール nn.kernels にあります。

<a id="shapes-and-domains"></a>

## 形状と定義域

**形状の前提条件違反は定義済みの中断です。** 一方のオペランドの範囲を他方の添字として使うカーネルでは、その範囲が等しい必要があります。違反すると stderr に `Error: matrix shape mismatch` を出して終了コード1で停止します。ネイティブと wasm の両経路で、panic にも切り詰めた結果にもなりません（C-353）。

| 呼び出し | 必要条件 |
|---|---|
| `a * b` | `cols(a) == rows(b)` |
| `concat_cols(ms)` | 空でない各要素の `rows` が最初の要素と等しい |
| `concat_rows(ms)` | 空でない各要素の `cols` が等しい |
| `from_lists(rows)` | すべての行が最初の行と同じ幅。不揃いのリストは `Error: matrix rows must have equal length` で中断 |

**空**のオペランドは例外です。これらは中断せず退化した形状を返します（`m * zeros(0, 0)` は `rows(m)×0` 行列、`concat_cols([m, zeros(0, 0)])` は `m`）。

**行がない行列には列もありません**（C-161）。Matrix は幅の等しい行のリストであり、幅は行0の幅です。そのため0行の行列はすべて0 × 0です。生成時の幅にかかわらず `shape` は `(0, 0)`、`cols` は `0` を返します。`zeros(0, 3)`、`from_bytes_f32_le(b, 0, 0, 3)!`、`gather_rows(m, [])`、`slice_rows(m, 5, 9)` は両ターゲットでいずれも0 × 0です。これにより `from_lists(to_lists(m))` の形状が保たれます。行のない行列の `to_lists` は `[]`、`from_lists([])` は0 × 0です。逆は同じではありません。r × 0 行列は r 行を保ちます（`zeros(3, 0)` は `(3, 0)`、`to_lists` は空行3つを返します）。

**行範囲と個数は範囲内に制限します**（C-354）。`list.slice` と同様、`slice_rows(m, start, end)` は両境界を `0..rows(m)` に制限します（負の境界は0。#4147 の負の個数の規則）。`start >= end` は空で、`split_cols_even(m, n)` は `n <= 0` なら空リストを返します。

**符号反転は IEEE に従います**（C-355）。`-m` は符号ビットを反転するため、セルの `0.0` は `-0.0` になり、両ターゲットでそれを分母とした `1.0` は `-inf` になります。

<a id="byte-decoders"></a>

## バイトデコーダー

バイトバッファからの行の読み出しは、デコードと収集を組み合わせます：`matrix.gather_rows(matrix.from_bytes_f32_le(b, offset, rows, cols)!, ids)`（Q1_0 / Q8_0 量子化ローダーは方言エポック14、`select_rows_f32` はエポック15でパッケージ almide-graphics/nn のモジュール nn.kernels に移動しました）。

`from_bytes_f16_le` / `_f32_le` / `_f64_le` の**デコーダー**は格納された `-0.0` を保持します。ここでは符号そのものがデータであり、残ります（C-269）。負の次元を0に制限し、上限を超える個数なら `Error: matrix dimensions too large` で中断した後（C-161）、`0 <= offset <= len(b)` かつ `rows * cols * width <= len(b) - offset` でない限り `none` を返します（C-341）。


<a id="signature-index-27-functions"></a>

## 原文固定スナップショットの API 索引（27関数）

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

## API 説明の訳（上記署名順）

- `matrix.zeros(rows: Int, cols: Int) -> Matrix`：rows × cols の0.0。負数は0に制限。
- `matrix.ones(rows: Int, cols: Int) -> Matrix`：rows × cols の1.0。負数は0に制限。
- `matrix.shape(m: Matrix) -> (Int, Int)`：(rows, cols)。行がなければ (0, 0)。
- `matrix.transpose(m: Matrix) -> Matrix`：cols × rows のコピー。r × 0 は0 × 0になります。
- `matrix.from_lists(xs: List[List[Float]]) -> Matrix`：等長の行 xs からなる行列。[] は0 × 0。
- `matrix.to_lists(m: Matrix) -> List[List[Float]]`：行をリストにします。r × 0 は r 個の空行。
- `matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]`：(row, col) の要素。範囲外なら none。
- `matrix.rows(m: Matrix) -> Int`：行数。空の行列なら0。
- `matrix.cols(m: Matrix) -> Int`：行0の幅。行がなければ0。
- `matrix.scale(m: Matrix, s: Float) -> Matrix`：全要素を s 倍。
- `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix`：全要素に f を適用。
- `matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]`：b の offset から f32 LE の rows × cols。b が短ければ none。
- `matrix.to_bytes_f64_le(m: Matrix) -> Bytes`：行優先の f64 LE。要素あたり8バイト。
- `matrix.to_bytes_f32_le(m: Matrix) -> Bytes`：行優先の f32 LE。各4バイト（丸め）。
- `matrix.to_bytes_f16_le(m: Matrix) -> Bytes`：行優先の f16 LE。各2バイト（最近接偶数丸め、オーバーフローは inf）。
- `matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]`：b の offset から f16 LE の rows × cols。b が短ければ none。
- `matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]`：b の offset から f64 LE の rows × cols。b が短ければ none。
- `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix`：各行に bias を加え、列数を bias に合わせて切り詰めます。
- `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]`：幅 cols/n の n 個の切片。余りは捨てます。n = 0 なら []。
- `matrix.concat_cols(xs: List[Matrix]) -> Matrix`：横並びに結合。[] は0 × 0。
- `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix   (deprecated — use matrix.concat_cols)`：matrix.concat_cols の別名。
- `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix`：行 [start, end)。各境界を0..行数に制限。
- `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix`：indices の行。範囲外ならゼロ行。
- `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float`：行 r と vec の内積。r が範囲外なら0.0。
- `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float   (deprecated — use matrix.dot_row)`：matrix.dot_row の別名。
- `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix`：同位置の各要素対に f を適用。形状不一致は中断。
- `matrix.concat_rows(xs: List[Matrix]) -> Matrix`：各行列の行を上から下へ並べます。空の要素は飛ばし、幅不一致は中断。
