<!-- description: Stdlib redesign L2/L3 — matrix: finish the epoch-14 cut (#3852): kernels to nn.kernels, fusion targets internal, the Float32 preview retired, select_rows folded into gather_rows, and shape or length mismatches abort instead of answering zeros -->
# L2/L3: matrix

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-22: `matrix` を標準に残し、#3852 の切り出しを終える）
> 出典: Rust https://docs.rs/ndarray/latest/ndarray/（標準にはない） ・ Go https://pkg.go.dev/gonum.org/v1/gonum/mat（標準にはない） ・ Python https://numpy.org/doc/stable/reference/routines.linalg.html（標準にはない。演算子 `@` だけが言語にある） ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/DOMMatrix（ブラウザだけ） ・ Gleam https://gleam-stdlib.hexdocs.pm/（行列なし） ・ Elixir https://hexdocs.pm/nx/Nx.html（外部） ・ OCaml https://ocaml.org/manual/latest/api/Bigarray.html（入れ物だけ） ・ Kotlin https://github.com/Kotlin/multik（外部） ・ Swift https://developer.apple.com/documentation/accelerate（Apple だけ）

9 言語とも、標準に行列の演算を持ちません（[facts/categories.md](facts/categories.md) 1.A・3.9 節）。L1 R-L1-22 は、コンパイラの書き換え規則（`@rewrite`、egg による fusion）が `matrix` の演算を対象にしていて、package は書き換え規則を持ち込めないことを理由に、汎用の密行列演算だけを標準に残すと決めました。この表の「他言語」の列は、各言語で事実上の標準になっている外部ライブラリ（numpy、gonum、ndarray、Nx）の綴りです。

実測（`almide 0.68.0 dev 437b8f127`）:

| 式 | 結果 | 読み取れること |
|---|---|---|
| `matrix.add(2×2, 1×3)` | `1×2` | 形が合わないと、重なる部分だけ計算して黙って小さくなる |
| `matrix.from_bytes_f32_le(4 バイト, 0, 2, 2)` | 全部 `0.0` の `2×2` | データが足りないと、読めた先頭の要素も含めて全部 0 になる（番兵の値） |
| `matrix.from_bytes_f32_le(4 バイト, 0, 1, 1)` | `1.0` | 足りていれば正しく読む |
| `matrix.select_rows(m, [-1])` / `matrix.gather_rows(m, [-1])` | 行 0 / 0 の行 | 同じ役割の 2 関数で、負の添字の扱いが違う |
| `matrix.dot_row(m, 5, …)`（2 行の行列） | `0.0` | 範囲外の行は 0 として答える |
| `matrix.from_lists([[1.0, 2.0], [3.0]])` | abort「rows must have equal length」 | 行の長さの不一致は契約違反として止まる |
| `matrix.get(m, 9, 9)` | abort | 添字の範囲外は止まる |

## L2: 役割の比較

| 役割 | Almide | Rust（ndarray） | Go（gonum） | Python（numpy） | TS/JS | Gleam | Elixir（Nx） | OCaml | Kotlin（multik） | Swift（Accelerate） |
|---|---|---|---|---|---|---|---|---|---|---|
| 0・1 で埋めて作る | `zeros` `ones` | `zeros` `ones` | `NewDense(r, c, nil)` | `zeros` `ones` | — | — | `broadcast(0, …)` | `Bigarray.Array2.create` | `zeros` `ones` | — |
| リストから・リストへ | `from_lists` `to_lists` | `from_shape_vec` | `NewDense(r, c, data)` | `array` `tolist` | — | — | `tensor` `to_list` | — | `mk.ndarray` | — |
| 形 | `shape` `rows` `cols` | `shape` `nrows` `ncols` | `Dims` | `shape` | — | — | `shape` | `dim1` `dim2` | `shape` | — |
| 要素を読む | `get`（いまは範囲外で abort。R-matrix-08 で `Option`） | `m[[i, j]]`（panic） | `At`（panic） | `m[i, j]`（例外） | — | — | `m[i][j]` | `get` | `m[i, j]` | — |
| 転置 | `transpose` | `t()` | `T()` | `.T` | — | — | `transpose` | — | `transpose` | `vDSP_mtrans` |
| 要素ごとの和・差・商・符号反転・累乗 | `add` `sub` `div` `neg` `pow` | `+` `-` `/` | `Add` `Sub` `DivElem` | `+` `-` `/` `**` | — | — | `add` `subtract` `divide` `pow` | — | `+` `-` `/` | `vDSP` |
| 要素ごとの積 | — | `*` | `MulElem` | `*` | — | — | `multiply` | — | `*` | `vDSP_vmul` |
| 行列の積 | `mul` | `dot` | `Mul` | `@` / `matmul` | — | — | `dot` | — | `dot` | `cblas_dgemm` |
| スカラー倍・関数を当てる | `scale` `map` | `* s` `mapv` | `Scale` `Apply` | `* s` `vectorize` | — | — | `multiply` `map` | — | `* s` `map` | — |
| 行の取り出し | `slice_rows` `gather_rows` `select_rows` | `slice` `select` | `Slice` | `m[a:b]` `m[idx]` | — | — | `slice` `take` | `sub` | `m[a..b]` | — |
| 連結・分割 | `append_rows` `concat_cols` `split_cols_even` | `concatenate` | `Stack` `Augment` | `vstack` `hstack` `split` | — | — | `concatenate` `split` | — | `cat` | — |
| 行ベクトルの加算・行と内積 | `broadcast_add_row` `dot_row` | ブロードキャスト `+` | — | ブロードキャスト `+` `m[i] @ v` | — | — | `add`（ブロードキャスト） | — | — | — |
| バイト列との変換（f16/f32/f64） | `from_bytes_*_le` `to_bytes_*_le` | `from_shape_vec` + `bytemuck` | `binary.Read` | `frombuffer` `tobytes` | — | — | `from_binary` `to_binary` | — | — | — |
| 推論用のカーネル | `linear_row` `conv1d` `linear_f32_row_no_bias` `select_rows_f32` | 外部 | 外部 | `torch` | — | — | `Axon` | — | — | — |
| 32 bit の行列 | `Matrix[Float32]`（試験版、構築子 2 と積 4 だけ） | `Array2<f32>` | — | `float32` | — | — | `{:f, 32}` | — | `D2Array<Float>` | `cblas_sgemm` |

### 完全性の規則

1. **標準に残すのは汎用の密行列演算だけ**（R-L1-22、#3852）: 上の表の推論用カーネルと、fusion の対象（`fma`・`fma3`・`mul_scaled`）は公開の面に置きません。`broadcast_add_row` と `split_cols_even` は汎用の演算として残します（numpy・Nx のブロードキャストと `split` に当たり、推論に固有の前提を持たないため。#3852 が判定を求めていた 2 件です）。
2. **要素の型は `Float` の 1 種だけ**: `Matrix[Float32]` を公開するなら、`Matrix` と同じ演算の表をすべて持たせ、ゲートで行列として検査することが条件です（CLAUDE.md の family の規則）。その消費者が標準の中にないので、いまは置きません（R-matrix-03）。
3. **形・長さ・添字の不一致は abort**（R-L0-06 の 3 と 4）: 呼び出し側が形を決めて渡す関数（要素ごとの演算、行列の積、行ベクトルの加算、バイト列からの構築、行の取り出し、行との内積）は、形や長さが合わなければ abort します。形も長さも、呼び出す前に呼び出し側が `rows`・`cols`・`bytes.len` で確かめられる値なので、合わないのは契約違反です（`list.chunk(xs, 0)` と同じ扱い）。0 や小さな行列のような番兵の値は返しません（R-L0-06 の 4）。`from_lists` はすでにこの規則で動いています。
   - 1 要素の取り出し `get` だけは、無印の取り出しなので `Option` を返します（R-L0-06 の 1、R-matrix-08）。
   - `slice_rows` の範囲だけは R-L0-12（はみ出しは端に丸める）に従います。
   - 位置を指定して書き込む関数は matrix にありません（全部コピーを返す演算です）。L0 の「範囲外の書き込みは abort」に当たる関数はありません。
4. 撤去予定のまま epoch が決まっていない別名（`concat_cols_many`・`row_dot`）は、R-L0-14 の窓で撤去します。

### L2 の判定

- 推論用のカーネル 5 関数 → R-matrix-01（`almide-graphics/nn` へ）。
- fusion の対象 3 関数 → R-matrix-02（内部の `__` 名へ）。
- 32 bit の試験版 6 関数 → R-matrix-03（撤去）。
- 重複: `select_rows` と `gather_rows` → R-matrix-04。`concat_cols_many`・`row_dot` → R-matrix-06（#3851 A）。
- 番兵の値と黙った切り詰め → R-matrix-05。
- 抜けている役割: 要素ごとの積（外部ライブラリの多数派が持つ）。`matrix.mul` の名前が行列の積に使われているので足す名前の判断が要りますが、`map` と `from_lists` で書け、標準に行列を持つ言語がないので足しません（R-matrix-07 ×）。

## 案

### R-matrix-01 推論用のカーネルを `almide-graphics/nn` へ移す — ○ 採用

- 内容: `linear_f32_row_no_bias`、`select_rows_f32`、`linear_row`、`linear_row_no_bias`、`conv1d` を標準から撤去し、`almide-graphics/nn` に同じ名前・同じ引数で置きます。nn.kernels の本体は汎用の演算で書きます（`linear_row(x, w, b)` = `broadcast_add_row(mul(x, transpose(w)), b)`、f32 の 2 関数は `from_bytes_f32_le` の後に同じ計算）。
- 根拠の L0 規則: L1 R-L1-22、epoch 14 の裁定（NN のカーネルは nn.kernels）。
- (a) 標準に推論のカーネルを持つ言語はありません。
- (b) nn.kernels は純粋な Almide の package（`src/mod.almd`、intrinsic なし）なので、native と wasm の両方で動きます。いまの f32 の 2 関数は GGUF のバッファを直接読む intrinsic で、移した後は速さが下がることがあります。速さは nn.kernels の側の課題とし、標準の面の判定には入れません。
- (c) epoch 14 で兄弟のカーネル（`linear_q8_0_row_no_bias` など）はすでに nn.kernels にあります。半分だけが標準に残っていると、LLM はどちらに置かれているかを当てられません。
- (d) 撤去 5。#3852 の手順どおり、先に nn.kernels の版を出し、`@deprecated(since=15, use="kernels.linear_row")` などのヒントがその版を名指しできるようにします。removal epoch 16。`almide fix`: `matrix.<fn>(…)` → `import nn.kernels` を足して `kernels.<fn>(…)`。使用数（spec/tests/docs/stdlib）: `linear_f32_row_no_bias` 9/0/5/0、`select_rows_f32` 10/0/6/0、`linear_row` 8/0/4/0、`linear_row_no_bias` 2/0/4/1、`conv1d` 15/0/5/0。nn.kernels 自身が `matrix.linear_row`（2）・`linear_row_no_bias`（2）・`linear_f32_row_no_bias`（1）を呼んでいるので、nn.kernels の中の定義に付け替えます。CHEATSHEET:1066 の `linear_row` の例を差し替えます。
- ゲート案: `crates/almide-types/src/self_host_registry.rs`・`docs/stdlib/matrix.md` の索引・`since.toml`・semantics-manifest・検品台帳の行を同じ変更で再生成し、`check_stdlib_fn_count` で公開数の減りを確かめます。
- issue: #3852（1 節）

### R-matrix-02 fusion の対象を内部の名前にする — ○ 採用

- 内容: `mul_scaled`、`fma`、`fma3` を公開の面から外し、`__mul_scaled`・`__fma`・`__fma3` として `@rewrite` と egg の規則だけが出力するようにします。公開の書き方は合成形だけになります（`matrix.scale(matrix.mul(a, b), s)`、`matrix.add(matrix.scale(a, ka), matrix.scale(b, kb))`）。
- 根拠の L0 規則: R-L0-01（同じ計算に綴りが 2 つ）。R-L0-01 の転送（ほかの公開関数の組み合わせを 1 回呼ぶだけの関数）にも当たります。
- (b) 書き換え規則は合成形を見つけて内部の関数に置き換えるので、速さは変わりません。`crates/almide-codegen/src/target.rs:81-85` が、この書き換えは速さのためだけで spec は依存しないと書いています。
- (c) 書いたほうが速いのか、書かなくても同じなのかを LLM が迷う関数が消えます。
- (d) 撤去 3（中身は内部に残る）。`@deprecated(since=15, use="matrix.scale(matrix.mul(a, b), s)")` など、removal epoch 16。`almide fix`: 合成形への 1 対 1 の展開。使用数: `mul_scaled` 3/0/3/2、`fma` 13/2/5/1、`fma3` 6/2/3/0。
- ゲート案: `tests/matrix_fma_overlap_cost_test.rs` などの既存テストを合成形で書き直し、合成形が内部の関数に書き換わることを検査します。
- issue: #3852（1 節）

### R-matrix-03 32 bit の試験版 `Matrix[Float32]` を撤去する — ○ 採用

- 内容: `zeros_f32`、`ones_f32`、`mul_f32`、`mul_f32_scaled`、`mul_f32_t`、`mul_f32_t_scaled` を撤去します。f32 のデータは `from_bytes_f32_le` で `Matrix`（Float）に読みます。
- 根拠の L0 規則: 完全性の規則 2（CLAUDE.md の「family は行列で広げる」）。#3852 は「演算の表を完成させるか、nn.kernels へ移すか」を求めていました。nn.kernels へは移せません（nn.kernels は intrinsic を持たない純粋な Almide の package で、`Matrix[Float32]` を作る手段が標準の外にないため）。表を完成させるには `Matrix` と同じ約 30 セルが要り、その消費者は spec のほかにありません。
- (a) 外部ライブラリは要素の型ごとに同じ演算の表を持ちます（numpy の dtype、ndarray のジェネリクス）。構築子 2 つと積 4 つだけという形は、どこにもありません。
- (b) runtime の SmallF32 の格納（`cblas_sgemm`）は内部に残してよく、公開の面だけを外します。
- (c) `Matrix[Float32]` を作れても `add` の型が合わず、LLM が書き始めるとすぐ行き止まりになります。
- (d) 撤去 6。`@deprecated(since=15, use="matrix.zeros")` など、removal epoch 16。`almide fix` は機械的に書き換えられない（型が変わる）ので、`removed_spellings.rs` のヒントで `from_bytes_f32_le` と Float の行列を示します。使用数: `zeros_f32` 2/0/3/0、`ones_f32` 18/8/3/0、`mul_f32` 3/5/3/0、`mul_f32_scaled` 2/1/3/0、`mul_f32_t` 2/0/3/0、`mul_f32_t_scaled` 2/0/3/0（多くは `spec/stdlib/matrix_f32_test.almd`）。
- 再開の条件: 標準の中か nn.kernels に f32 の消費者が現れたら、`Matrix[Float32]` を `Matrix` と同じ演算の表とゲートつきで出し直します。
- issue: #3852（1 節の Float32 preview）

### R-matrix-04 `select_rows` を撤去し、`gather_rows` に一本化する — ○ 採用

- 内容: `select_rows` を撤去します。`gather_rows` は R-matrix-05 で「範囲外の添字は abort」になります。
- 根拠の L0 規則: R-L0-01（同じ役割で端だけ違う 2 関数）。
- (c) 名前から端の違い（負の添字が行 0 か、0 の行か）を当てる手段がありません。CHEATSHEET:1066 が名指ししているのは `gather_rows` です。
- (d) 撤去。`@deprecated(since=15, use="matrix.gather_rows")`、removal epoch 16。`almide fix`: 名前の置き換え（負の添字の意味は変わるので、ヒントに一言書く）。使用数: `select_rows` 0/0/3/0、`gather_rows` 5/0/4/0。
- issue: #3852（1 節）

### R-matrix-05 形・長さ・添字の不一致を abort にする — ○ 採用

- 内容: 次の場合に、黙って小さな結果や 0 を返すのをやめ、何が合わないかを名指しして abort します。
  - `add`・`sub`・`div`: 2 つの形が違う。
  - `mul`: `cols(a) != rows(b)`。
  - `broadcast_add_row`: `bias` の長さが列の数と違う。
  - `from_bytes_f16_le`・`from_bytes_f32_le`・`from_bytes_f64_le`: `offset + rows * cols * 幅` がデータの長さを超える。
  - `gather_rows`: 添字が負か、行の数以上。
  - `dot_row`: 行が範囲外か、`vec` の長さが列の数と違う。
- 根拠の L0 規則: R-L0-06 の 3（abort は呼び出し側の契約違反だけ）と R-L0-06 の 4（失敗しうる構築は番兵を返さない）。形はどれも呼び出し側が決めて渡していて、呼び出す前に確かめられるので、合わないのは呼び出し側の契約違反です。`from_lists` はすでにこの規則で動いています。
- (a) numpy・gonum・ndarray・Nx は、形の不一致を例外か panic にします。重なる部分だけで計算する外部ライブラリはありません。
- (b) 純粋な計算の検査を足すだけで、abort の文言は両 target で同じにします（wasm_cross fixture で固定）。
- (c) 形を間違えた LLM のコードが、いまは小さな行列や 0 を返して先に進み、ずっと後で違う答えになります。止まれば、その場所で直せます。
- (d) 端の意味変更です。値を正しく返していた入力の結果は 1 つも変わらず、意味のない値を返していた入力だけが abort になります（定義域を狭める変更で、R-L0-14 の 4 の「同じ名前で別の意味」には当たりません。L0 の「位置を指定した書き込み」と同じく、正しい入力の結果は変わらないので、epoch の `breaks` には入れません）。名前は変えないので `@deprecated` は付けません。abort の文言が、合わなかった形と関数の名前を示します。入れる前に、spec・tests・nn.kernels・下流のプロジェクトで、不一致の形で呼んでいる箇所を数えます（nn.kernels の `shape_mismatch()` は `append_rows` を使っていて、この変更の対象外です）。
- ゲート案: 関数ごとに不一致の入力 1 つを abort の fixture にし、契約の台帳に新しい C 番号を置きます（als に先に置く）。
- issue: #3927

### R-matrix-06 撤去予定の別名 `concat_cols_many`・`row_dot` を epoch 16 で撤去する — ○ 採用

- 内容: すでに `@deprecated(since=5)` が付いている 2 関数に、`[[deprecation]]` ブロック（removal epoch 16）を置きます。
- 根拠の L0 規則: R-L0-14、R-L0-11 と同じ扱い（#3851 A）。
- (d) 撤去。`@deprecated(since=5, use="matrix.concat_cols")` と `@deprecated(since=5, use="matrix.dot_row")` はすでに付いています。`removal_epoch = 16`。`almide fix` は 1 対 1 の名前の置き換え（`concat_cols_many` → `concat_cols`、`row_dot` → `dot_row`）です。使用数: `concat_cols_many` 1/2/5/1、`row_dot` 2/3/6/0。
- issue: #3851 A

### R-matrix-08 `matrix.get` は範囲外で `Option` を返す — ○ 採用

- 内容: `matrix.get(m: Matrix, row: Int, col: Int) -> Option[Float]` にします。範囲内なら `some(値)`、範囲外なら none です。
- 根拠の L0 規則: R-L0-06 の 1（無印の取り出しは安全版）。`list.get`、`map.get`、`bytes.get` がすべて `Option` を返すので、「無印の `get` は `Option`」を例外なしにします。
- (a) Rust（ndarray）は `get` が `Option`、添字 `m[[i, j]]` が panic と、名前で 2 つを分けています。Almide は `get` の側を揃えます。
- (b) 影響なし。
- (c) モジュールによって `get` が止まったり none を返したりすると、`??` を書くべきかどうかを名前から決められません。要素を 1 つずつ読むループは、行列では `matrix.map`、`matrix.to_lists` などの全体の演算で書くのが基本で、1 要素ごとの `??` が並ぶ書き方は推奨しません。
- (d) 戻り型の変更です。旧い呼び出しは型エラーになり、黙って意味が変わることはありません（R-L0-14 の 4）。同じ名前のままなので `@deprecated` は付けず、型エラーにヒント「`matrix.get(m, i, j) ?? 既定値`」を出します。`almide fix` は、いまの挙動（範囲外で止まる）を保つ形 `matrix.get(m, i, j) ?? panic("matrix.get: out of range")` に書き換えます。使用数を PR に書きます。
- issue: #3927

### R-matrix-07 `mul` を `matmul` に改名し、要素ごとの積を足す — × 不採用（→ R-matrix-14 で `*` に置き換え）

- 内容: 変えません。`mul` は行列の積のままにします。
- 根拠の L0 規則: R-L0-00（9 言語の標準に行列がないので多数派がない。外部ライブラリでも gonum の `Mul` と Nx・ndarray の `dot` と numpy の `matmul` に割れている）、R-L0-14 の 4（`mul` を要素ごとの積に付け替えることはできない）。
- (c) 六巡目の追記: `mul` は R-matrix-14 で撤去し、行列の積は演算子 `*` になりました。doc と CHEATSHEET に書く文は「`*` は行列の積。要素ごとの積は `map` で」です。R-matrix-05 で形の不一致が止まるので、取り違えは実行時に見つかります。
- issue: なし

### R-matrix-09 要素ごとの二項演算を `matrix.zip_with` で書けるようにする — ○ 採用

- いま: 要素ごとの演算は `add`・`sub`・`div` だけで、要素ごとの積がありません。R-matrix-07（×）は「要素ごとの積は `map` で」と書くとしていますが、`matrix.map` は 1 引数の関数なので 2 つの行列の積は書けません（R-matrix-07 への新しい証拠）。
- 案: `fn matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix`。形が違えば R-matrix-05 と同じく abort します。
- 根拠の L0 規則: R-L0-10（`_with`。`list.zip_with` と同じ綴り）。
- (a) 外部ライブラリの多数派は要素ごとの積を持ちます（numpy `*`、gonum `MulElem`、Nx `multiply`）。
- (c) `matrix.mul` を要素ごとの積だと思って書いた LLM が、直す先の関数を持てます。
- (d) 追加 1。`mul`（行列の積）の意味は変えません。
- issue: #3972

### R-matrix-11 行方向の連結を `concat_rows(List[Matrix])` にし、`concat_cols` と形をそろえる — ○ 採用

- いま: `matrix.append_rows(base, extra)`（2 つを受け取る）と `matrix.concat_cols(matrices: List[Matrix])`（リストを受け取る）。
- 案: `fn matrix.concat_rows(matrices: List[Matrix]) -> Matrix` を足し、`append_rows` を撤去します。
- 根拠の L0 規則: R-L0-01（軸だけが違うものは同じ形）、R-L0-04 の 2（`append_` は `mut` のその場書き換えの動詞として予約されている）。
- (a) numpy `vstack`/`hstack`、Nx `concatenate`、ndarray `concatenate` はどれも両方の軸でリストを受け取ります。
- (d) 追加 1・撤去 1。`@deprecated(since=15, use="matrix.concat_rows")`、`almide fix` で `append_rows(a, b)` を `concat_rows([a, b])` に書き換えます。
- issue: #3972

### R-matrix-12 `to_bytes_f16_le` を足して、バイト列の変換を対にする — ○ 採用

- 案: `fn matrix.to_bytes_f16_le(m: Matrix) -> Bytes`（f16 への丸めは IEEE の最近接偶数）。
- 根拠の L0 規則: R-L0-07 の 4（向きの対はそろえる）。幅 {f16, f32, f64} × 向き {from, to} の 6 セルにします。
- (d) 追加 1。
- issue: #3972

### R-matrix-13 `matrix.pow` を撤去する — ○ 採用

- いま: `fn matrix.pow(m: Matrix, exp: Float) -> Matrix` は要素ごとの累乗で、`matrix.map(m, (x) => float.pow(x, e))` を 1 回呼ぶだけです。数学の記法 A^2 は行列の積なので、名前の読み違いも起きます。
- 根拠の L0 規則: R-L0-01（ほかの公開関数を 1 回呼ぶだけの関数は同義語）。
- (d) 撤去。`almide fix` が `matrix.map(m, (x) => float.pow(x, e))` に書き換えます。使用数 6/0/3/0。
- issue: #3973

### R-matrix-14 matrix の `add`/`sub`/`mul`/`div`/`neg` を公開しない（演算子だけにする） — ○ 採用

- いま: `a + b`・`a - b`・`a * b`（行列の積）・`a / b` は脱糖され、同じ計算が公開の関数としても残っています。
- 案: R-bigint-04 と同じ規則（R-L0-26: 演算子の脱糖先は公開しない）を matrix にも当てはめ、5 つを撤去します。単項 `-` は #3965（いまは内部エラー）を直したうえで脱糖します。
- 根拠の L0 規則: R-L0-01、R-L0-26。bigint だけに当てはめると、同じ「演算子を持つ型」で規則が割れます。
- (c) パイプで `a |> matrix.add(b)` と書く利点はありますが、`a + b` の 1 通りにしたほうが生成されるコードが揺れません。
- (d) 撤去 5。`@deprecated(since=15, use="+")` など、epoch 16 で撤去。`almide fix` で `matrix.add(a, b)` を `a + b` に書き換えます。
- issue: #3973

### R-matrix-15 `from_bytes_*_le` はデータが足りないと none を返す（R-matrix-05 の見直し） — ○ 採用

- いま: R-matrix-05 は、データが足りないとき（いまは全部 0 の番兵）を abort にすると決めました。ところが同じバイト列を読む `bytes.get_f32_le_array` などは、足りなければ none を返します。外から来たファイルを読む処理を Matrix 版に替えると、回復できた失敗がプログラムの終了に変わります。
- 案: 2 つを分けます。`rows`・`cols`・`offset` が負なのは呼び出し側の誤りなので abort のまま。`offset` から `rows × cols` 個を読むのにバイトが足りないのは入力の問題なので、`-> Matrix?` で none を返します。
  - `fn matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?`（f32・f64 も同じ）
- 根拠の L0 規則: R-L0-06（失敗しうる取り出しは無印で Option）。bytes の `get_<型>_array` と同じ失敗の形です。
- (d) 意味変更。戻り型が変わるので、古い呼び出しは型エラーになり、黙って結果は変わりません（R-L0-14 の 4）。`almide fix` は末尾に `!` ではなく `?? abort` 相当のヒントを出します。
- issue: #3978

## L3: 関数表

`matrix` の現行 46 関数です（追加はありません）。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `zeros` | `matrix.zeros(rows: Int, cols: Int) -> Matrix` | numpy `zeros` / ndarray `zeros` / Nx `broadcast` / multik `zeros` | 据え置き | — |
| `ones` | `matrix.ones(rows: Int, cols: Int) -> Matrix` | numpy `ones` / ndarray `ones` / multik `ones` | 据え置き | — |
| `shape` | `matrix.shape(m: Matrix) -> (Int, Int)` | numpy `shape` / ndarray `shape` / gonum `Dims` / Nx `shape` | 据え置き | — |
| `transpose` | `matrix.transpose(m: Matrix) -> Matrix` | numpy `.T` / ndarray `t()` / gonum `T()` / Nx `transpose` | 据え置き | — |
| `from_lists` | `matrix.from_lists(rows: List[List[Float]]) -> Matrix` | numpy `array` / Nx `tensor` / gonum `NewDense` | 据え置き | — |
| `select_rows` | `matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix` | numpy `m[idx]` / ndarray `select` / Nx `take` | 撤去 | R-matrix-04 |
| `append_rows` | `matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix` | numpy `vstack` / ndarray `concatenate(Axis(0))` / Nx `concatenate` | 撤去 → `concat_rows([a, b])` | R-matrix-11 |
| `linear_f32_row_no_bias` | `matrix.linear_f32_row_no_bias(x: Matrix, w_bytes: Bytes, w_offset: Int, w_rows: Int, w_cols: Int) -> Matrix` | torch `F.linear` / 外部のみ | 撤去 | R-matrix-01（nn.kernels） |
| `select_rows_f32` | `matrix.select_rows_f32(data: Bytes, offset: Int, cols: Int, row_ids: List[Int]) -> Matrix` | torch `embedding` / 外部のみ | 撤去 | R-matrix-01（nn.kernels） |
| `to_lists` | `matrix.to_lists(m: Matrix) -> List[List[Float]]` | numpy `tolist` / Nx `to_list` | 据え置き | — |
| `get` | `matrix.get(m: Matrix, row: Int, col: Int) -> Float` | numpy `m[i, j]`（例外）/ Rust ndarray `get` → Option・`m[[i, j]]` → panic / gonum `At`（panic） | 意味変更（`Option[Float]` を返す） | R-matrix-08 |
| `rows` | `matrix.rows(m: Matrix) -> Int` | ndarray `nrows` / numpy `shape[0]` | 据え置き | — |
| `cols` | `matrix.cols(m: Matrix) -> Int` | ndarray `ncols` / numpy `shape[1]` | 据え置き | — |
| `add` | `matrix.add(a: Matrix, b: Matrix) -> Matrix` | numpy `+` / gonum `Add` / Nx `add` | 撤去 → `a + b` | R-matrix-14 |
| `mul` | `matrix.mul(a: Matrix, b: Matrix) -> Matrix` | numpy `@`/`matmul` / gonum `Mul` / ndarray `dot` / Nx `dot` | 撤去 → `a * b` | R-matrix-14 |
| `scale` | `matrix.scale(m: Matrix, s: Float) -> Matrix` | numpy `* s` / gonum `Scale` / Nx `multiply` | 据え置き | — |
| `fma` | `matrix.fma(a: Matrix, ka: Float, b: Matrix, kb: Float) -> Matrix` | BLAS `axpby` / 外部のみ | 撤去 | R-matrix-02（内部の `__fma`） |
| `fma3` | `matrix.fma3(a: Matrix, ka: Float, b: Matrix, kb: Float, c: Matrix, kc: Float) -> Matrix` | 外部のみ | 撤去 | R-matrix-02（内部の `__fma3`） |
| `sub` | `matrix.sub(a: Matrix, b: Matrix) -> Matrix` | numpy `-` / gonum `Sub` / Nx `subtract` | 撤去 → `a - b` | R-matrix-14 |
| `div` | `matrix.div(a: Matrix, b: Matrix) -> Matrix` | numpy `/` / gonum `DivElem` / Nx `divide` | 撤去 → `a / b` | R-matrix-14 |
| `neg` | `matrix.neg(m: Matrix) -> Matrix` | numpy `-m` / Nx `negate` | 撤去 → `-m` | R-matrix-14 |
| `pow` | `matrix.pow(m: Matrix, exp: Float) -> Matrix` | numpy `**` / Nx `pow` | 撤去 → `matrix.map(m, (x) => float.pow(x, e))` | R-matrix-13 |
| `map` | `matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` | ndarray `mapv` / gonum `Apply` / Nx `map` | 据え置き | — |
| `from_bytes_f32_le` | `matrix.from_bytes_f32_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f4')` / Nx `from_binary` | 意味変更（負の形は abort、データ不足は none） | R-matrix-05、R-matrix-15 |
| `to_bytes_f64_le` | `matrix.to_bytes_f64_le(m: Matrix) -> Bytes` | numpy `astype('<f8').tobytes()` / Nx `to_binary` | 据え置き | — |
| `to_bytes_f32_le` | `matrix.to_bytes_f32_le(m: Matrix) -> Bytes` | numpy `astype('<f4').tobytes()` / Nx `to_binary` | 据え置き | — |
| `from_bytes_f16_le` | `matrix.from_bytes_f16_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f2')` | 意味変更（負の形は abort、データ不足は none） | R-matrix-05、R-matrix-15 |
| `from_bytes_f64_le` | `matrix.from_bytes_f64_le(data: Bytes, offset: Int, rows: Int, cols: Int) -> Option[Matrix]` | numpy `frombuffer(dtype='<f8')` | 意味変更（負の形は abort、データ不足は none） | R-matrix-05、R-matrix-15 |
| `broadcast_add_row` | `matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` | numpy `m + v`（ブロードキャスト） / Nx `add` | 意味変更 | R-matrix-05（長さ違いは abort） |
| `split_cols_even` | `matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` | numpy `hsplit` / Nx `split` | 据え置き | — |
| `concat_cols` | `matrix.concat_cols(matrices: List[Matrix]) -> Matrix` | numpy `hstack` / ndarray `concatenate(Axis(1))` / gonum `Augment` | 据え置き | — |
| `concat_cols_many` | `matrix.concat_cols_many(matrices: List[Matrix]) -> Matrix`（`@deprecated`） | — | 撤去 | R-matrix-06 |
| `linear_row` | `matrix.linear_row(x: Matrix, weight: Matrix, bias: List[Float]) -> Matrix` | torch `F.linear` / Axon `dense` | 撤去 | R-matrix-01（nn.kernels） |
| `linear_row_no_bias` | `matrix.linear_row_no_bias(x: Matrix, weight: Matrix) -> Matrix` | torch `F.linear(bias=None)` | 撤去 | R-matrix-01（nn.kernels） |
| `slice_rows` | `matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` | numpy `m[a:b]` / ndarray `slice` / gonum `Slice` | 意味変更（負の位置で abort。正の側は丸める） | R-L0-29、R-L0-46 |
| `conv1d` | `matrix.conv1d(input: Matrix, weight: Matrix, bias: List[Float], kernel: Int, stride: Int, padding: Int) -> Matrix` | torch `conv1d` / Axon `conv` | 撤去 | R-matrix-01（nn.kernels） |
| `gather_rows` | `matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` | numpy `take(axis=0)` / Nx `take` / torch `index_select` | 意味変更 | R-matrix-04・R-matrix-05（範囲外は abort） |
| `dot_row` | `matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` | numpy `m[r] @ v` / ndarray `row(r).dot(v)` | 意味変更 | R-matrix-05（範囲外は abort） |
| `row_dot` | `matrix.row_dot(m: Matrix, r: Int, vec: List[Float]) -> Float`（`@deprecated`） | — | 撤去 | R-matrix-06 |
| `zeros_f32` | `matrix.zeros_f32(rows: Int, cols: Int) -> Matrix[Float32]` | numpy `zeros(dtype=float32)` | 撤去 | R-matrix-03 |
| `ones_f32` | `matrix.ones_f32(rows: Int, cols: Int) -> Matrix[Float32]` | numpy `ones(dtype=float32)` | 撤去 | R-matrix-03 |
| `mul_f32` | `matrix.mul_f32(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm` | 撤去 | R-matrix-03 |
| `mul_f32_scaled` | `matrix.mul_f32_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（alpha） | 撤去 | R-matrix-03 |
| `mul_f32_t` | `matrix.mul_f32_t(a: Matrix[Float32], b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（transB） | 撤去 | R-matrix-03 |
| `mul_f32_t_scaled` | `matrix.mul_f32_t_scaled(a: Matrix[Float32], alpha: Float, b: Matrix[Float32]) -> Matrix[Float32]` | BLAS `sgemm`（alpha, transB） | 撤去 | R-matrix-03 |
| `mul_scaled` | `matrix.mul_scaled(a: Matrix, alpha: Float, b: Matrix) -> Matrix` | BLAS `dgemm`（alpha） | 撤去 | R-matrix-02（内部の `__mul_scaled`） |
| `zip_with` | `matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` | numpy `*`（要素ごと） / gonum `MulElem` | 追加 | R-matrix-09 |
| `concat_rows` | `matrix.concat_rows(matrices: List[Matrix]) -> Matrix` | numpy `vstack` / Nx `concatenate` | 追加 | R-matrix-11 |
| `to_bytes_f16_le` | `matrix.to_bytes_f16_le(m: Matrix) -> Bytes` | numpy `astype("<f2").tobytes()` | 追加 | R-matrix-12 |

## L0 への差し戻し（処理済み）

- 固定の形を持つ入れ物の `get`: R-L0-06 の 1 を例外なしに当てはめ、`Option` を返すことにしました（R-matrix-08）。
