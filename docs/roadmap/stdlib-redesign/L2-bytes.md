<!-- description: Stdlib redesign L2/L3 — bytes: in-place verbs (push / put_ / append_), one byte-IO family, raw pointers out to ffi, and the same sequence names as list and string -->
# L2/L3: bytes

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/vec/struct.Vec.html ・ https://doc.rust-lang.org/std/primitive.u32.html ・ Go https://pkg.go.dev/encoding/binary (F) ・ https://pkg.go.dev/bytes ・ Python https://docs.python.org/3/library/struct.html ・ https://docs.python.org/3/library/stdtypes.html#bytearray ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/DataView (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F) ・ Elixir https://elixir.hexdocs.pm/Kernel.html#binary_part/3 ・ OCaml https://ocaml.org/manual/latest/api/Bytes.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-byte-array/ ・ Swift https://developer.apple.com/documentation/foundation/data
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm` の両方。プローブは scratchpad の `probe-bytes/`。
> 件数: `/usr/bin/grep -rEo '\bbytes\.<fn>\b'` を spec / tests / docs / stdlib / research ごとに数えた値（定義行と生成された索引を含むので上限値）。

読み方: `mut`（読み: ミュート）= 呼び出し元の `var` をその場で書き換える引数。`??`（読み: ナル合体）= 左が none / err なら右を使う演算子。`b[i] = v`（読み: 添字代入）。LE / BE = リトルエンディアン / ビッグエンディアン（バイトの並び順）。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 0 埋めで作る | `new(n)` | `vec![0; n]` | `make([]byte, n)` | `bytes(n)` | `new Uint8Array(n)` | — | `:binary.copy(<<0>>, n)` | `Bytes.make` (F) | `ByteArray(n)` | `Data(count:)` |
| 文字列 ⇄ バイト列 | `from_string` / `to_string`（Result）/ `to_string_lossy` | `into_bytes` / `String::from_utf8` | `[]byte(s)` / `string(b)` | `encode` / `decode` | `TextEncoder` / `TextDecoder` | `from_string` / `to_string`→Result (F) | binary が文字列 | `of_string` / `to_string` (F) | `encodeToByteArray` / `decodeToString` | `Data(s.utf8)` / `String(decoding:)` |
| 1 バイト読む（範囲外を安全に） | `get`→Option | `get`→Option | — | — | — | — | — | — | `getOrNull` | — |
| 1 バイト書く（その場） | `b[i] = v`、`set_at`、`set_u8` | `b[i] = v` | `b[i] = v` | `b[i] = v` | `b[i] = v` | — | — | `set_uint8` (F) | `b[i] = v` | `b[i] = v` |
| 末尾に 1 バイト足す（その場） | `push`、`append_u8`、`write_u8` | `push` | `append` | `bytearray.append` | — | — | — | `Buffer.add_uint8` | — | `append` |
| 末尾にバイト列を足す（その場） | **—**（`concat` はコピー） | `extend_from_slice` | `append(b, s...)` | `extend` / `+=` | `set` | `append`（コピー）(F) | `<>` | `Buffer.add_bytes` | `+=` | `append(contentsOf:)` |
| 幅つき整数・浮動小数の読み | `read_<型>_<le/be>`（範囲外は 0）+ `read_uint16` 系（2 系統） | `u32::from_le_bytes` | `binary.LittleEndian.Uint32` (F) | `struct.unpack_from` | `DataView.getUint32(o, le)` (F) | ビット構文 | `<<x::little-32>>` | `get_int32_le` (F) | — | `load(as:)` |
| 位置を指定して上書き | `set_<型>_<le/be>` + `set_uint16` 系 | `copy_from_slice(&x.to_le_bytes())` | `PutUint32` (F) | `struct.pack_into` | `DataView.setUint32` (F) | — | — | `set_int32_le` (F) | — | — |
| 末尾に書き足す | `append_<型>_<le/be>` + `write_*_be` + `write_uint16` 系 | `extend_from_slice` | `AppendUint32` (F) | `+= struct.pack` | — | ビット構文 | `<<b::binary, x::32>>` | `Buffer.add_int32_le` | — | — |
| 読み位置を進めながら読む | `read_<型>_at`、`take_at`、`skip`、`eof` | `Cursor` | `bytes.Reader` + `binary.Read` | `io.BytesIO` | — | パターン一致 | パターン一致 | — | `ByteBuffer`(JVM) | — |
| 部分列・連結・検索 | `slice`、`concat`、`index_of`、`contains`、`starts_with`、`ends_with`、`split`、`lines` | ● | `bytes.*` | ● | ● | `slice`、`starts_with` (F) | `binary_part`、`:binary.match` | `sub`、`cat` (F) | ● | ● |
| 先頭・末尾から n 個 | **—**（`slice` だけ） | `&b[..n]` | `b[:n]` | `b[:n]` | `subarray` | `slice` (F) | `binary_part` | `sub` | `take` / `takeLast` | `prefix` / `suffix` |
| 生ポインタ | `as_ptr`、`as_mut_ptr`、`data_ptr`、`from_raw_ptr`、`copy_to_ptr` | `as_ptr` ほか | `unsafe` | `ctypes` | — | — | — | — | — | `withUnsafeBytes` |

### 完全性の規則

1. **バイナリの読み書きは 1 系統の行列にする。** 行列の軸は「型 × 向き × 操作」です。
   - 型: `u8`、`u16`、`i16`、`u32`、`i32`、`i64`、`f16`、`f32`、`f64`、`bool`。
   - 向き: 2 バイト以上の型は `_le` と `_be` の両方を持ちます。`u8` と `bool` は向きを持ちません。
   - 操作: `get_<型>`（位置を指定して読む。範囲外は none）、`read_<型>_at`（読み位置を進める版。`(次の位置, Option[値])` を返す）、`get_<型>_array`（2 バイト以上の型だけ。1 つでも範囲外なら none）、`put_<型>`（位置を指定して上書き、その場）、`append_<型>`（末尾に書き足す、その場）。
   - **意図した抜け**: `u8` の位置読みは `get` が持ちます（`list.get` と同じ）。`u8` の `append` は `push` が持ちます（`list.push`・`string.push` と同じ「末尾に 1 つ足す」動詞。R-bytes-02）。`u8` と `bool` の `_array` は持ちません（`bytes.slice(...) |> bytes.to_list` で書けます）。`i8` と `u64` は持ちません。`i8` は `bytes.read_u8(b, p) |> int.to_int8` で読めます。`u64` は `Int`（i64）に入りきらないので `i64` が同じビット列を担います。
   - **書き込みで符号の有無が同じ本体になるセル**（`append_i16_le` と `append_u16_le` など）は、読みの名前と対称にするために両方残します。名前から推測して書けることを優先します。R-L0-01 は、宣言されゲートで守られた型の行列のセルを同義語に数えません（「L0 への差し戻し（処理済み）」1）。
2. **その場書き換えの動詞は R-L0-04 の集合だけ**: `push`、`clear`、`fill`、`append_<型>`、`put_<型>`、`copy_from`、`copy_within`。`set`・`insert`・`remove_at` はコピー側です。
3. **列の関数は list・string と同じ綴り**: `compare`（順序のある型はすべて `<mod>.compare(a, b) -> Int`）、`slice`、`take`、`drop`、`take_end`、`drop_end`、`chunk`、`map`、`pad_start`、`pad_end`、`index_of`、`contains`、`starts_with`、`ends_with`、`split`、`lines`、`repeat`、`reverse`、`insert`、`remove_at`。
4. **失敗しうる読みは番兵（0・空文字列）を返さない**（R-L0-06 の 4）。理由を返す必要がなければ Option、あれば `Result[_, String]`。
5. **メモリ安全でない関数を置かない**（R-L1-26）。

### L2 の判定

- 重複: `push` ≡ `append_u8` ≡ `write_u8`、`set_at` ≡ `set_u8`（実測: 3 つとも 300 → `[44]`。`set_at` と `set_u8` は本体が同じ）→ R-bytes-02。`write_u32_be` ≡ `append_u32_be` ほか 3 組 → R-bytes-03。
- 2 系統: 接尾辞の系統（`read_u32_le`）と `Endian` 引数の系統（`read_uint32(b, o, .le)`、12 関数、4 幅だけ）→ R-bytes-04（#3851 D）。
- 動詞の様式: `set_<型>` は `mut` なのに、`bytes.set`・`list.set`・`map.set` はコピー → R-bytes-01。
- 穴: `f16` は LE の読みだけで、BE と書き込みがない。`bool` は位置を指定した上書きがない。バイト列をその場で足す関数がない → R-bytes-03、R-bytes-05。
- 綴りのずれ: `cmp`（順序のある型は `compare`）、`chunks`（list は `chunk`）、`map_each`（他の 6 モジュールは `map`）、`pad_left`/`pad_right`（string は `pad_start`/`pad_end`）、`take`/`drop` 系がない → R-bytes-08。
- `_at` の意味が 2 つ: `read_u32_le_at` などは「読み位置を進める版」、`read_string_at(b, pos, len)` は位置を指定して読むだけ → R-bytes-05。
- L0 の一般規則: `get_or`（R-L0-06）、`eof`（R-L0-09）→ R-bytes-07。
- 生ポインタ → R-bytes-06（#3853）。長さ前置きの補助関数 → R-bytes-05（#3852）。
- 番兵: 位置指定の `read_<型>`・`read_bool`・`read_<型>_array` は、範囲外で 0（または false）を返します（実測: `read_u8(h, 99)` → 0、`read_u32_le(h, 2)`（長さ 4）→ 0）。R-L0-06 の 4（失敗しうる読みは番兵を返さない）に反します → R-bytes-09。
- 範囲外の書き込み: `put_<型>`（いまの `set_<型>`）・`set_at`・`set`・`remove_at` は何もせず、`insert` は位置を `0..len` に丸めます。`b[i] = v` は abort します（実測: `Error: index out of bounds`）。L0 の「位置を指定した書き込み」の規則（範囲外は abort）に反します → R-bytes-11。
- 境界の行列（R-L0-12）: `bytes.slice(b, -1, 2)` は `[]`、`list.slice` も `[]`、`string.slice("abc", -1, 2)` だけが `"ab"` を返します（native・wasm とも実測）。R-L0-12 は負の添字を範囲外として 0 に丸めるので、string の挙動が正しく、bytes と list をそれに合わせます → R-bytes-12（list は L2-list R-list-10）。

## 案

### R-bytes-01 位置を指定した上書きを `put_<型>` にする — ○ 採用

- 内容: `set_u8` と `set_<型>_<le/be>`（`u16`・`i16`・`u32`・`i32`・`i64`・`f32`・`f64` × LE/BE）の 15 関数を、`put_u8`、`put_u16_le` … `put_f64_be` に改名します。引数は変えません。範囲外の位置の挙動は、別の案 R-bytes-11 で abort にします（改名と同じ窓で入れます）。
- 根拠の L0 規則: R-L0-04（`set` はコピー側の動詞。その場の位置書きは `put_<型>`）、R-L0-14。
- (a) 位置を指定して上書きする名前は、Go が `PutUint32`(F)、OCaml と JS が `set_int32_le`/`setUint32`(F) です。多数派はありませんが、Almide の中で `bytes.set`（コピー）と `bytes.set_u8`（その場）が同じ動詞なので、R-L0-00 の条件 1 で改名します。Go の `Put`/`Append` の対と同じ形になります。
- (b) 名前だけの変更です。本体（native の intrinsic と wasm の self-host）はそのままです。
- (c) `bytes.set(b, i, v)` を文として書くと、黙って何も起きません（実測: `bytes.set(b, 0, 9)` の後も `[1, 2, 3]`）。動詞を分けると、`set` はコピー、`put_` はその場、と名前から読めます。
- (d) 改名。`@deprecated(since=15, use="bytes.put_<型>")`（`set_u32_le` なら `use="bytes.put_u32_le"`）、`removal_epoch = 16`。`almide fix` は `bytes.set_u32_le(b, p, v)` → `bytes.put_u32_le(b, p, v)` と 1 対 1 で書き換えます。使用数（15 関数の合計）: spec 178 / tests 10 / docs 78 / stdlib 1。
- ゲート案: R-L0-G の `mut` 動詞検査（`mut` を取る公開関数の名前が動詞集合に入る）。行列ゲート（R-bytes-03）で `put_` の全セルを確かめます。
- issue: #3898

### R-bytes-02 1 バイトの追記は `push`、1 バイトの上書きは `put_u8` だけにする — ○ 採用

- 内容: `append_u8` と `write_u8` を撤去して `push` に寄せます。`set_at` を撤去して `put_u8` に寄せます。
- 根拠の L0 規則: R-L0-01（同じ役割に綴りは 1 つ。モジュールをまたいでも）、R-L0-04、R-L0-14。#3851 C の判断をここで決めます。
- (a) 1 要素を末尾に足す動詞は Rust `push`、Swift `append`、Python `append` で割れています。Almide の中では `list.push(mut xs, x)` と `string.push(mut s, x)` がすでに `push` です。同じ役割に同じ綴りを使う R-L0-01 に従い、`push` を残します。
- (b) 3 つとも同じ本体です（`b.push(val as u8)`）。両 target で影響はありません。
- (c) `set_at` の置き換え先は `put_u8` にします。`put_u8` は型の行列（R-bytes-03）の `u8` の上書きのセルで、どのみち残る名前です。R-bytes-11 で範囲外は abort になるので、`put_u8` と添字代入 `b[i] = v` は同じ挙動になります。どちらで書いても同じですが、`almide fix` が呼び出しの形のまま 1 対 1 で書き換えられるのは `put_u8` です。
- (d) 撤去。`@deprecated(since=15, use="bytes.push")`（`append_u8`・`write_u8`）、`use="bytes.put_u8"`（`set_at`）、`removal_epoch = 16`。`almide fix` は `bytes.append_u8(b, v)` → `bytes.push(b, v)`、`bytes.set_at(b, i, v)` → `bytes.put_u8(b, i, v)` と 1 対 1 で書き換えます。使用数: `append_u8` 69/5/14/0、`write_u8` 8/1/3/0、`set_at` 44/8/5/0（spec/tests/docs/stdlib）。`push` は spec 45・stdlib 40 で残ります。
- ゲート案: #3851 F の同義語ゲート規則 6（同じモジュールで本体が同じ別 intrinsic）。行列ゲートは「`u8` の append セル = `push`」を宣言として持ちます。
- issue: #3851 C（既存）

### R-bytes-03 書き足しを `append_` に揃え、型 × 向き × 操作の行列を閉じる — ○ 採用

- 内容:
  - `write_u32_be`・`write_i64_be`・`write_f64_be` を撤去して `append_u32_be`・`append_i64_be`・`append_f64_be` に寄せます。
  - `write_bool` を `append_bool` に改名し、`put_bool(mut b, pos, val: Bool)` を追加します。
  - `f16` の穴を埋めます: `get_f16_be`、`read_f16_be_at`、`get_f16_be_array`、`put_f16_le`、`put_f16_be`、`append_f16_le`、`append_f16_be`。
- 根拠の L0 規則: R-L0-01、R-L0-04（書き足しの動詞は `append_<型>`）、CLAUDE.md の「API の族は行列で広げる」。
- (a) `f16` の読み書きを両方の向きで持つのは JS `getFloat16`/`setFloat16`(F)、Python `struct` の `e` です。いまは LE の読みだけなので、BE のファイル形式を書く人は自前で書くことになります。
- (b) 純粋な計算です。`f16` の丸め（`Float` → 半精度）は self-host の 1 本の実装にして、両 target で同じビット列にします。
- (c) `get_f16_le` を知っている LLM は `put_f16_le` を規則から推測して書きます。行列に穴があると、その推測が外れます。
- (d) 撤去 3・改名 1・追加 8。`@deprecated(since=15, use="bytes.append_u32_be")` など（`write_bool` は `use="bytes.append_bool"`）、`removal_epoch = 16`。`almide fix` は `bytes.write_u32_be(b, v)` → `bytes.append_u32_be(b, v)` と 1 対 1 で書き換えます。使用数（`write_*` 4 関数）: spec 16 / tests 8 / docs 16 / stdlib 0。
- ゲート案: 行列ゲート `tests/stdlib_bytes_matrix_test.rs`。上の型・向き・操作の宣言から全セルの名前とシグネチャを生成し、interface JSON と完全に一致することを確かめます（多い・少ないのどちらも失敗）。各セルの往復（`append` → `read`、`put` → `read`）を native と wasm で突き合わせる fixture を 1 本置きます。
- issue: #3851 C（既存。`write_*` の部分）＋ 新規（行列の穴）

### R-bytes-04 `Endian` 引数の系統（12 関数）を撤去し、接尾辞の系統を唯一にする — ○ 採用

- 内容: `read_uint16`・`read_uint32`・`read_int32`・`read_float32`・`write_uint16`・`write_uint32`・`write_int32`・`write_float32`・`set_uint16`・`set_uint32`・`set_int32`・`set_float32` と `type Endian` を撤去します。`stdlib/bytes.almd:552-569` の「the new preferred surface」というコメントも消します。#3851 D の (a) です。
- 根拠の L0 規則: R-L0-01（同じ操作に 2 系統）、R-L0-04（`write_` と `set_` は L0 の動詞集合に入っていない。(b) を採ると、その動詞を集合に足すか、行列全体を改名し直すことになる）、R-L0-00（改名は必要最小限）。
- (a) 向きを名前に入れる言語（OCaml `get_int32_le`(F)、Rust `from_le_bytes`）と、値で渡す言語（Go `binary.LittleEndian.Uint32`(F)、JS `getUint32(o, true)`(F)）に割れています。多数派がないので、文書と使用の多い接尾辞の系統を据え置きます。
- (b) 両系統とも同じ本体に落ちます。影響はありません。
- (c) いまは同じ読みに 2 つの綴りと 2 つの戻り型（`Int` と `UInt32`）があり、ソースのコメントと文書が「どちらが正か」で食い違っています。L0 の動詞（`append_`/`put_`）は接尾辞の系統にしか当てはまりません。
- (d) 撤去。`@deprecated(since=15, use="bytes.get_u32_le")` のように、向きごとの接尾辞版を示します（`read_uint32(b, o, LittleEndian)` → `bytes.get_u32_le(b, o) |> option.map(int.to_uint32)`）。`removal_epoch = 16`。`Endian` の値がリテラルの呼び出しは `almide fix` が接尾辞版に書き換えます。`Endian` の値が実行時に決まる呼び出しは機械的に書き換えられないので、ヒント「向きごとに `bytes.get_u32_le` / `bytes.get_u32_be` を使い分ける」だけにします。使用数（12 関数）: spec 103 / tests 15 / docs 46 / stdlib 1。
- ゲート案: R-bytes-03 の行列ゲートが、行列の外にあるバイナリ読み書き関数を拒否します。
- issue: #3851 D（既存）

### R-bytes-05 長さ前置きの補助関数と `read_string_at` を撤去し、`append_bytes` を足す — ○ 採用

- 内容:
  - 撤去: `read_string_be`、`read_string_be_at`、`write_string_be`（u32 BE の長さ前置き）、`read_length_prefixed_strings_le`、`skip_length_prefixed_le`（u32 LE の長さ前置き）、`read_string_at(b, pos, len)`。
  - 追加: `append_bytes(mut b: Bytes, src: Bytes) -> Unit`（末尾にバイト列を書き足す、その場）。
- 根拠の L0 規則: R-L0-01、R-L0-04（`append_<型>` の型に `bytes` を入れる）、R-L0-14。#3852 の bytes の項をここで決めます。
- (a) 長さ前置きの文字列を標準に持つのは Java の `DataOutput.writeUTF`（u16 BE）くらいで、前置きの幅と向きは形式ごとに違います。いまの Almide も、単体は u32 BE、まとめ読みは u32 LE で食い違っています。バイト列をその場で足す関数は 8 言語が持っています（Rust `extend_from_slice`、Go `append(b, s...)`、Python `extend` など）。
- (b) 純粋な計算です。
- (c) `read_string_at` の `_at` は、他の `_at`（読み位置を進めて `(位置, Option)` を返す）と意味が違います。名前から形を推測できません。前置きの幅と向きも名前からは当てられません。置き換えは次の組み合わせで書けます。
  - 読み: `let n = bytes.get_u32_be(b, p)?` のあと `bytes.slice(b, p + 4, p + 4 + n) |> bytes.to_string`
  - 書き: `bytes.append_u32_be(b, bytes.len(sb))` のあと `bytes.append_bytes(b, sb)`
  - `read_string_at(b, p, n)` → `bytes.slice(b, p, p + n) |> bytes.to_string_lossy`。範囲が一部だけはみ出したときの結果が違います（旧: 空文字列、新: はみ出さない部分）。ヒントにそう書きます。
- (d) 撤去 6・追加 1。`@deprecated(since=15, use="…")` は、`read_string_at` が `use="bytes.slice(b, p, p + n) |> bytes.to_string_lossy"`、長さ前置きの 5 関数が `use="bytes.get_u32_be + bytes.slice"` などの組み合わせを示し、`removal_epoch = 16`。形が変わるので `almide fix` は書き換えず、上の (c) の書き方をヒントに出します。使用数（6 関数）: spec 35 / tests 14 / docs 33 / stdlib 5。stdlib の 5 件は撤去と同じ変更で置き換えます。
- ゲート案: 行列ゲート（R-bytes-03）。`_at` で終わる公開関数が `(Int, Option[T])` を返すことを型から検査する規則を、R-L0-G に足します。
- issue: #3852（既存）

### R-bytes-06 生ポインタを `ffi` に移す — ○ 採用

- 内容: #3853 の案どおりです。`as_ptr`・`as_mut_ptr`・`data_ptr` → `ffi.address(b) -> RawPtr`（空なら null）、`from_raw_ptr` → `ffi.read(ptr, len) -> Bytes`、`copy_to_ptr` → `ffi.write(b, ptr, cap) -> Int`。`ffi` は自動 import しません。
- 根拠の L0 規則: R-L1-26、R-L0-01（「データの番地」に綴りが 3 つ）。
- (a) Rust も `as_ptr` を持ちますが、安全でない操作は `unsafe` の中だけです。Almide には `unsafe` がないので、モジュールを分けて import を要求します。
- (b) いまは空の Bytes で答えが食い違います（`as_ptr` は null、`data_ptr` は handle + 12）。`ffi.address` は両 target で null に揃えます。wasm から JS host に番地を渡す経路（#907、`spec/wasm_host_js/extern_data_ptr_lifetime.almd`）が `ffi.address` で通ることを、撤去の前に確かめます。
- (d) 撤去（`ffi` 側は追加）。`@deprecated(since=15, use="ffi.address")`（`as_ptr`・`as_mut_ptr`・`data_ptr`）、`use="ffi.read"`（`from_raw_ptr`）、`use="ffi.write"`（`copy_to_ptr`）、`removal_epoch = 16`。`almide fix` は 1 対 1 で書き換え、`import ffi` を足します。`data_ptr` は戻り型が `Int` から `RawPtr` に変わるので、型エラーになる箇所にはヒントを出します。使用数（5 関数）: spec 48 / tests 1 / docs 36 / stdlib 2。
- ゲート案: `bytes` の公開関数の型に `RawPtr` が現れないことを検査します。
- issue: #3853（既存）

### R-bytes-07 `get_or` を撤去し、`eof` を `is_eof` に改名する — ○ 採用

- 内容: `get_or(b, i, d)` → `bytes.get(b, i) ?? d`。`eof(b, pos)` → `is_eof(b, pos)`（挙動は同じ。`pos >= len` または負で true）。
- 根拠の L0 規則: R-L0-06（既定値版 `_or` は `??` に一本化）、R-L0-09（Bool を返す名前は `is_`。`bytes.eof` は L0 が名指しした例外）。
- (b) `get_or` は stdlib の字句解析器で 33 回使われています。`bytes.get(b, i) ?? -1` に置き換えたときに速さが落ちないことを、撤去の前に `research/benchmark/perf/` の字句解析器で確かめます（native の `Option<i64>` は箱に入らないので、差は出ない見込みです）。
- (d) 撤去 1・改名 1。`@deprecated(since=15, use="??")`（`get_or`）、`@deprecated(since=15, use="bytes.is_eof")`（`eof`）、`removal_epoch = 16`。`almide fix` は `bytes.get_or(b, i, d)` → `bytes.get(b, i) ?? d`、`bytes.eof(b, p)` → `bytes.is_eof(b, p)` と書き換えます。使用数: `get_or` 69/7/11/33、`eof` 9/1/6/1（spec/tests/docs/stdlib）。
- ゲート案: R-L0-G の R-L0-06・R-L0-09 の規則。
- issue: #3913

### R-bytes-08 列の関数を list・string と同じ綴りにする — ○ 採用

- 内容:
  - 改名: `cmp` → `compare`（L0 改訂: 順序のある型はすべて `<mod>.compare(a, b) -> Int`）、`chunks` → `chunk`、`map_each` → `map`、`pad_left` → `pad_start`、`pad_right` → `pad_end`。
  - 追加: `take(b, n)`、`drop(b, n)`、`take_end(b, n)`、`drop_end(b, n)`（すべてコピーを返す）境界の答えは list・string と同じにします（`n` が長さを超えたら全部、負の `n` は `take` が全部・`drop` が空。L2-string の完全性の規則 1）。
  - `chunk` は、`list.chunk` と同じく大きさ 0 以下で abort します（実測: `list.chunk(xs, 0)` は `Error: chunk size must be positive`、`bytes.chunks(b, 0)` は `[]`）。名前が新しくなるので、R-L0-14 の「同じ名前で意味を変えない」には当たりません。
- 根拠の L0 規則: R-L0-01（同じ役割に同じ綴り。モジュールをまたいでも）、R-L0-12（`take`・`drop`・`take_end`・`drop_end` を持つ型に bytes が入っている）、R-L0-06 の 3（abort は契約違反だけ。大きさ 0 の分割は契約違反）。
- (a) 列を先頭・末尾から切る関数は Kotlin `take`/`takeLast`、Swift `prefix`/`suffix` などで、どの言語でもバイト列とリストで同じ綴りを使います。
- (b) 純粋な計算です。
- (c) `list.map` を知っている LLM は `bytes.map` と書きます。いまはそれが未定義エラーになります。
- (d) 改名 5・追加 4。`@deprecated(since=15, use="bytes.compare")`（`cmp`）、`use="bytes.chunk"`（`chunks`）、`use="bytes.map"`（`map_each`）、`use="bytes.pad_start"`・`use="bytes.pad_end"`、`removal_epoch = 16`。`almide fix` は 1 対 1 で書き換えます。`chunks(b, 0)` は `[]` を返していましたが、`chunk(b, 0)` は abort するので、ヒントにそう書きます。使用数: 改名 4 関数（`cmp` 以外）spec 30 / tests 0 / docs 17 / stdlib 4、`cmp` 20/1/6/1。
- ゲート案: R-L0-12 の境界行列テスト（list・string・bytes で同じ入力を突き合わせる）に bytes の 5 関数を入れます。モジュールをまたぐ同義語ゲート（#3851 F 規則 5）を「同じ役割の綴りの違い」にも広げるのは L0 の範囲なので、ここでは行列テストだけにします。
- issue: #3899

### R-bytes-09 位置指定の読みを、番兵 0 を返す `read_<型>` から Option を返す `get_<型>` に移す — ○ 採用

- 内容:
  - `read_<型>_<le/be>`（14）と `read_f16_le` を `get_<型>_<le/be>(b, pos) -> Option[T]` に改名します。範囲外は none です。
  - `read_bool(b, pos)` → `get_bool(b, pos) -> Option[Bool]`。
  - `read_<型>_array`（15）→ `get_<型>_array(b, pos, count) -> Option[List[T]]`。1 つでも範囲外なら none です。
  - `read_u8(b, pos)` は撤去して `bytes.get(b, pos)` に寄せます（同じ役割で、すでに Option を返します）。
  - 読み位置を進める版 `read_<型>_at` は、いまも Option を返しているので据え置きます。
- 根拠の L0 規則: R-L0-06 の 4（失敗しうる構築・復号は番兵を返さない。理由が要らなければ Option）。R-L0-14 の 4 は、戻り型を `Int` から `Option[Int]` に変える同じ名前の変更も許します。それでも名前を移すのは、動詞が合っていないからです。`read_` は読み位置を進める `read_<型>_at` の族の動詞で、位置を指定して Option で取り出す動詞は `get` です。名前の `get_` は R-L0-06 の「無印の取り出しは Option」（`list.get`、`bytes.get`）と同じ動詞です。
- (a) 範囲外を 0 で返す言語はありません。Go・JS・Python・Rust・Kotlin は例外か panic です（JS DataView は RangeError(F)）。名前は OCaml の `get_int32_le`/`set_int32_le`(F) と同じ形になります。
- (b) 純粋な計算です。境界の判定を self-host の 1 本の実装にして、両 target で同じ答えにします。
- (c) いまは長さを確かめ忘れると、壊れたファイルを黙って 0 として読みます。Option なら `?` か `??` を書かないと値が取り出せません。
- (d) 改名 31・撤去 1。`@deprecated(since=15, use="bytes.get_u32_le")` など（`read_u8` は `use="bytes.get"`）、`removal_epoch = 16`。`almide fix` の書き換えは形が変わります: `bytes.read_u32_le(b, p)` → `bytes.get_u32_le(b, p) ?? 0`（いまと同じ結果になる形）。ヒントには「`?` で失敗を伝えるほうがよい」と出します。使用数（`read_u8` を除く 31 関数の合計）は spec 194 / tests 3 / docs 159 / stdlib 59 です（`read_u8` は 98/6/6/2）。stdlib と `matrix.from_bytes_*` の内部は同じ変更で書き換えます。
- ゲート案: 行列ゲート（R-bytes-03）が `get_*` の戻り型が Option であることを確かめます。R-L0-G に「戻り型が `Int`・`Float`・`Bool`・`String` で、名前が `read_`・`get_`・`decode`・`parse` で始まる公開関数」を一覧にし、番兵を返さないことを境界入力で確かめるテストを置きます。
- issue: #3897

### R-bytes-10 UTF-8 の両方向は bytes が持つ — ○ 採用（据え置き）

- 内容: `bytes.from_string`、`bytes.to_string`（Result）、`bytes.to_string_lossy` を、文字列とバイト列の変換の唯一の綴りとして据え置きます。`string.to_bytes`・`string.from_bytes`（`List[Int]`）は L2-string で撤去します（L0 改訂の決定）。
- 根拠の L0 規則: R-L0-07 の 4（向きの対は型を揃える）、R-L0-01、R-L0-06 の 4（`to_string` は理由つきの Result、`to_string_lossy` は置換文字 U+FFFD で必ず成功）。
- (a) 復号を元の側に置く言語（Gleam `bit_array.to_string`→Result(F)、Python `decode`、Kotlin `decodeToString`、OCaml `Bytes.to_string`(F)）と、先の側に置く言語（Rust `String::from_utf8`、Swift `String(decoding:)`）に割れています。符号化の両方向を符号化の側に置く Almide の形（base64・hex と同じ）を保ちます。
- (b) `bytes.from_string` は native で `Vec<u8>` です。`string.to_bytes` の `Vec<i64>`（8 倍の大きさ、#2078）より軽い経路です。
- (d) bytes 側は変更なし。
- issue: #2078（関連）

### R-bytes-11 範囲外の位置への書き込みを abort にする — ○ 採用

- 内容: 位置を指定した書き込みは、範囲外の位置を呼び出し側の契約違反として止めます（abort）。
  - その場の上書き `put_<型>(mut b, pos, v)`（いまの `set_<型>`・`set_u8`・`set_at`）: いまは書き込む幅が範囲からはみ出すと何もしません。`pos < 0` または `pos + 幅 > len` で abort にします。
  - コピーの位置書き `bytes.set(b, i, v)`・`bytes.remove_at(b, i)`: いまは範囲外でそのまま返します。`0 <= i < len` の外で abort にします。
  - `bytes.insert(b, pos, v)`: いまは位置を `0..len` に丸めます。`list.insert` と同じく `0..=len` を受け付け、その外は abort にします。
- 根拠の L0 規則: L0 の「位置を指定した書き込み」（R-L0-12 の後ろ、R-L0-13 の前）、R-L0-06 の 3。
- (a) Go の `PutUint32` は panic、JS の `DataView.setUint32` は RangeError(F)、OCaml の `set_int32_le` は例外(F)、Rust の `copy_from_slice` は panic です。範囲外を黙って捨てる言語はありません。
- (b) 両 target で同じ abort の経路（添字代入 `b[i] = v` が使っているもの）を使います。境界の判定を self-host の 1 本の実装にします。
- (c) いまは `put_` と `b[i] = v` で範囲外の挙動が違い、書き込みが黙って消えます。壊れたファイルを書いても気づけません。
- (d) 意味変更（定義域を狭める変更）。範囲内の書き込みの結果は変わらないので、dialect epoch の `breaks` には入れません（定義域を狭めても epoch にしない、という既存の扱い）。名前は R-bytes-01 の改名と同じ窓で変わりますが、この案自体は `@deprecated` を要しません。範囲外を何もしない挙動に頼っていた呼び出しは、`if pos + 4 <= bytes.len(b) then bytes.put_u32_le(b, pos, v) else ()` のように書きます。影響の件数は spec・tests・stdlib で範囲外の位置を渡している呼び出しを数えて PR に書きます。
- ゲート案: 行列ゲート（R-bytes-03）の `put_` の全セルと `set`・`insert`・`remove_at` に、境界入力 {-1, 0, len-幅, len-幅+1, len} の fixture を native と wasm で置き、abort の位置が同じであることを確かめます（L2-list R-list-13 と同じ表）。観測できる挙動の変更なので、契約の台帳を als に先に入れます。
- issue: #3937

### R-bytes-12 `slice` の負の開始位置を 0 に丸める — ○ 採用

- 内容: `bytes.slice(b, start, end)` の負の `start` を 0 に丸めます。いまは `bytes.slice(b, -1, 2)` が長さ 0 を返します。`string.slice("abc", -1, 2)` と同じく、先頭の 2 バイトを返すようにします。
- 根拠の L0 規則: R-L0-12（範囲外の添字は端に丸める。負の添字は範囲外）。
- (a) Python・JS は負の添字を末尾からの位置に使いますが、R-L0-12 はそれを採りません。丸めの規則は Gleam・Elixir の `slice` と同じです。
- (b) 純粋な計算です。両 target で同じ挙動にそろえます。
- (c) list・string・bytes で同じ引数の結果が違うと、ある型で覚えた端の挙動が別の型で外れます。
- (d) 意味変更（端の入力の結果だけ。文書の「capped」に合わせる修正）。名前と型は変えないので、`@deprecated` は要りません。影響の件数は spec・tests で負の開始位置を渡している呼び出しを数えて PR に書きます。
- ゲート案: R-L0-12 の境界行列テスト（list・string・bytes × `slice`・`take`・`drop`・`take_end`・`drop_end` × {-1, 0, len, len+1}）。
- issue: #3944

## L3: 関数表

（146 関数 + 追加 13。判定の内訳: 据え置き 58・改名 53・撤去 31・意味変更 4・追加 13。改名の `put_` 15 件は、R-bytes-11 で範囲外の挙動も変わります）

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `append_f32_be` | `bytes.append_f32_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_f32_le` | `bytes.append_f32_le(mut b: Bytes, val: Float) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_f64_be` | `bytes.append_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_f64_le` | `bytes.append_f64_le(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_i16_be` | `bytes.append_i16_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_i16_le` | `bytes.append_i16_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_i32_be` | `bytes.append_i32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_i32_le` | `bytes.append_i32_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_i64_be` | `bytes.append_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_i64_le` | `bytes.append_i64_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_u16_be` | `bytes.append_u16_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_u16_le` | `bytes.append_u16_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_u32_be` | `bytes.append_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | 据え置き | — |
| `append_u32_le` | `bytes.append_u32_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | 据え置き | — |
| `append_u8` | `bytes.append_u8(mut b: Bytes, val: Int) -> Unit` | Rust push / Python append / Go append(b, x) | 撤去 → `push` | R-bytes-02 |
| `as_mut_ptr` | `bytes.as_mut_ptr(b: Bytes) -> RawPtr` | Rust as_mut_ptr | 撤去 → `ffi.address` | R-bytes-06 |
| `as_ptr` | `bytes.as_ptr(b: Bytes) -> RawPtr` | Rust as_ptr / Go unsafe.SliceData | 撤去 → `ffi.address` | R-bytes-06 |
| `chunks` | `bytes.chunks(b: Bytes, size: Int) -> List[Bytes]` | Rust chunks / Kotlin chunked / Almide list.chunk | 改名 → `chunk` | R-bytes-08 |
| `clear` | `bytes.clear(mut b: Bytes) -> Unit` | Rust clear / Python bytearray.clear | 据え置き | — |
| `cmp` | `bytes.cmp(a: Bytes, b: Bytes) -> Int` | Rust cmp / Go bytes.Compare / Gleam compare (F) / OCaml Bytes.compare (F) | 改名 → `compare` | R-bytes-08 |
| `concat` | `bytes.concat(a: Bytes, b: Bytes) -> Bytes` | Python a + b / Gleam bit_array.append (F) / OCaml Bytes.cat (F) / Go append(a, b...) | 据え置き | — |
| `contains` | `bytes.contains(b: Bytes, pattern: Bytes) -> Bool` | Go bytes.Contains / Python in / Kotlin contains | 据え置き | — |
| `copy_from` | `bytes.copy_from(mut dst: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` | Rust copy_from_slice / Go copy / OCaml Bytes.blit (F) | 据え置き | — |
| `copy_to_ptr` | `bytes.copy_to_ptr(b: Bytes, ptr: RawPtr, cap: Int) -> Int` | Rust ptr::copy_nonoverlapping | 撤去 → `ffi.write` | R-bytes-06 |
| `data_ptr` | `bytes.data_ptr(b: Bytes) -> Int` | Go uintptr(unsafe.Pointer) | 撤去 → `ffi.address` | R-bytes-06 |
| `ends_with` | `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` | Go bytes.HasSuffix / Python endswith | 据え置き | — |
| `eof` | `bytes.eof(b: Bytes, pos: Int) -> Bool` | Go io.EOF（エラー値） / Python — / 多数: 長さと比べる | 改名 → `is_eof` | R-bytes-07 |
| `fill` | `bytes.fill(mut b: Bytes, val: Int) -> Unit` | Rust fill / JS fill / OCaml Bytes.fill (F) | 据え置き | — |
| `from_list` | `bytes.from_list(xs: List[Int]) -> Bytes` | Rust Vec::from / Python bytes(list) / JS Uint8Array.from / Kotlin byteArrayOf | 据え置き | — |
| `from_raw_ptr` | `bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes` | Rust slice::from_raw_parts / Go unsafe.Slice | 撤去 → `ffi.read` | R-bytes-06 |
| `from_string` | `bytes.from_string(s: String) -> Bytes` | Rust String::into_bytes / Go []byte(s) / Python s.encode() / JS TextEncoder / Gleam bit_array.from_string (F) / OCaml Bytes.of_string (F) | 据え置き | R-bytes-10 |
| `get` | `bytes.get(b: Bytes, i: Int) -> Option[Int]` | Rust get → Option / Kotlin getOrNull / Swift — | 据え置き（位置読みの u8 セルを兼ねる） | R-bytes-09 |
| `get_or` | `bytes.get_or(b: Bytes, i: Int, default: Int) -> Int` | Python get なし / Kotlin getOrElse / 多数: 演算子・汎用関数 | 撤去 → `get(b, i) ?? d` | R-bytes-07 |
| `index_of` | `bytes.index_of(b: Bytes, pattern: Bytes) -> Option[Int]` | Go bytes.Index / Python b.find / JS indexOf | 据え置き | — |
| `insert` | `bytes.insert(b: Bytes, pos: Int, val: Int) -> Bytes` | Python bytearray.insert / Rust Vec::insert（その場） | 意味変更（`0..=len` の外は abort） | R-bytes-11 |
| `is_empty` | `bytes.is_empty(b: Bytes) -> Bool` | Rust is_empty / Kotlin isEmpty / Swift isEmpty | 据え置き | — |
| `is_valid_utf8` | `bytes.is_valid_utf8(b: Bytes) -> Bool` | Go utf8.Valid / Rust str::from_utf8(..).is_ok() | 据え置き | — |
| `len` | `bytes.len(b: Bytes) -> Int` | Rust len / Go len / Python len / JS length / Gleam byte_size / OCaml length | 据え置き | — |
| `lines` | `bytes.lines(b: Bytes) -> List[Bytes]` | Go bufio.ScanLines / Python splitlines | 据え置き | — |
| `map_each` | `bytes.map_each(b: Bytes, f: (Int) -> Int) -> Bytes` | Kotlin map / Python bytes(map(f, b)) / Almide list.map | 改名 → `map` | R-bytes-08 |
| `new` | `bytes.new(len: Int) -> Bytes` | Rust vec![0; n] / Go make([]byte, n) / Python bytes(n) / JS new Uint8Array(n) / OCaml Bytes.make (F) / Kotlin ByteArray(n) | 据え置き | — |
| `pad_left` | `bytes.pad_left(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padStart / Almide string.pad_start | 改名 → `pad_start` | R-bytes-08 |
| `pad_right` | `bytes.pad_right(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padEnd / Almide string.pad_end | 改名 → `pad_end` | R-bytes-08 |
| `push` | `bytes.push(mut b: Bytes, val: Int) -> Unit` | Rust Vec::push / Python bytearray.append / Almide list.push | 据え置き | R-bytes-02 |
| `set_at` | `bytes.set_at(mut b: Bytes, i: Int, val: Int) -> Unit` | OCaml set_uint8 (F) / JS setUint8 (F) / 多数: b[i] = v | 撤去 → `put_u8`（範囲外は abort） | R-bytes-02、R-bytes-11 |
| `copy_within` | `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` | Rust copy_within / JS copyWithin | 据え置き | — |
| `read_bool` | `bytes.read_bool(b: Bytes, pos: Int) -> Bool` | Python struct '?' / 多数: — (read_u8 != 0) | 改名 → `get_bool`（範囲外は false ではなく none） | R-bytes-09 |
| `read_bool_at` | `bytes.read_bool_at(b: Bytes, pos: Int) -> (Int, Option[Bool])` | — | 据え置き | — |
| `read_f16_le` | `bytes.read_f16_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint16 (F) / JS getFloat16(o, true) (F) / Python struct '<e' | 改名 → `get_f16_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f16_le_at` | `bytes.read_f16_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_f16_le_array` | `bytes.read_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Ne') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f16_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_be` | `bytes.read_f32_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint32 (F) / JS getFloat32(o, false) (F) / Python struct '>f' | 改名 → `get_f32_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_be_array` | `bytes.read_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nf') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_f32_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_be_at` | `bytes.read_f32_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_f32_le` | `bytes.read_f32_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint32 (F) / JS getFloat32(o, true) (F) / Python struct '<f' | 改名 → `get_f32_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_le_array` | `bytes.read_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nf') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f32_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_le_at` | `bytes.read_f32_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_f64_be` | `bytes.read_f64_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint64 (F) / JS getFloat64(o, false) (F) / Python struct '>d' | 改名 → `get_f64_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_be_array` | `bytes.read_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nd') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_f64_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_be_at` | `bytes.read_f64_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_f64_le` | `bytes.read_f64_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint64 (F) / JS getFloat64(o, true) (F) / Python struct '<d' | 改名 → `get_f64_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_le_array` | `bytes.read_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nd') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f64_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_le_at` | `bytes.read_f64_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_i16_be` | `bytes.read_i16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_int16_be (F) / JS getInt16(o, false) (F) / Python struct '>h' | 改名 → `get_i16_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i16_be_at` | `bytes.read_i16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_i16_be_array` | `bytes.read_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nh') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i16_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i16_le` | `bytes.read_i16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_int16_le (F) / JS getInt16(o, true) (F) / Python struct '<h' | 改名 → `get_i16_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i16_le_at` | `bytes.read_i16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_i16_le_array` | `bytes.read_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nh') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i16_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_be` | `bytes.read_i32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getInt32(o, false) (F) / Python struct '>i' | 改名 → `get_i32_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_be_array` | `bytes.read_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Ni') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i32_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_be_at` | `bytes.read_i32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_i32_le` | `bytes.read_i32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getInt32(o, true) (F) / Python struct '<i' | 改名 → `get_i32_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_le_array` | `bytes.read_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Ni') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i32_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_le_at` | `bytes.read_i32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_i64_be` | `bytes.read_i64_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint64 (F) / OCaml get_int64_be (F) / JS getBigInt64(o, false) (F) / Python struct '>q' | 改名 → `get_i64_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_be_array` | `bytes.read_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nq') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i64_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_be_at` | `bytes.read_i64_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_i64_le` | `bytes.read_i64_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint64 (F) / OCaml get_int64_le (F) / JS getBigInt64(o, true) (F) / Python struct '<q' | 改名 → `get_i64_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_le_array` | `bytes.read_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nq') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i64_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_le_at` | `bytes.read_i64_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_length_prefixed_strings_le` | `bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]` | —（形式専用） | 撤去（→ `get_u32_le` と `slice` の組み合わせ） | R-bytes-05 |
| `read_string_at` | `bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String` | Python b[p:p+n].decode / Rust from_utf8_lossy(&b[p..p+n]) | 撤去（→ `slice` ＋ `to_string_lossy`） | R-bytes-05 |
| `read_string_be` | `bytes.read_string_be(b: Bytes, pos: Int) -> String` | Java DataInput.readUTF（u16 BE） / 多数: — | 撤去（→ `get_u32_be` と `slice` の組み合わせ） | R-bytes-05 |
| `read_string_be_at` | `bytes.read_string_be_at(b: Bytes, pos: Int) -> (Int, Option[String])` | — | 撤去（→ `read_u32_be_at` と `take_at` の組み合わせ） | R-bytes-05 |
| `read_u16_be` | `bytes.read_u16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_uint16_be (F) / JS getUint16(o, false) (F) / Python struct '>H' | 改名 → `get_u16_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_be_at` | `bytes.read_u16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_u16_be_array` | `bytes.read_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NH') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_u16_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_le` | `bytes.read_u16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_uint16_le (F) / JS getUint16(o, true) (F) / Python struct '<H' | 改名 → `get_u16_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_le_array` | `bytes.read_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NH') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_u16_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_le_at` | `bytes.read_u16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_u32_be` | `bytes.read_u32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getUint32(o, false) (F) / Python struct '>I' | 改名 → `get_u32_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_be_array` | `bytes.read_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NI') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_u32_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_be_at` | `bytes.read_u32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_u32_le` | `bytes.read_u32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getUint32(o, true) (F) / Python struct '<I' | 改名 → `get_u32_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_le_array` | `bytes.read_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NI') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_u32_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_le_at` | `bytes.read_u32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 据え置き | — |
| `read_u8` | `bytes.read_u8(b: Bytes, pos: Int) -> Int` | OCaml get_uint8 (F) / JS getUint8 (F) / 多数: b[i] | 撤去 → `get`（同じ役割。範囲外は none） | R-bytes-09 |
| `read_u8_at` | `bytes.read_u8_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型) | 据え置き | — |
| `remove_at` | `bytes.remove_at(b: Bytes, pos: Int) -> Bytes` | Rust Vec::remove / Python del b[i]（その場） | 意味変更（範囲外は abort） | R-bytes-11 |
| `repeat` | `bytes.repeat(b: Bytes, n: Int) -> Bytes` | Go bytes.Repeat / Python b * n | 据え置き | — |
| `reverse` | `bytes.reverse(b: Bytes) -> Bytes` | Rust reverse / Kotlin reversed | 据え置き | — |
| `set` | `bytes.set(b: Bytes, i: Int, val: Int) -> Bytes` | Kotlin copyOf + set / 多数: 添字代入（その場） | 意味変更（範囲外は abort） | R-bytes-11 |
| `set_f32_be` | `bytes.set_f32_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat32 (F) | 改名 → `put_f32_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_f32_le` | `bytes.set_f32_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat32 (F) | 改名 → `put_f32_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_f64_be` | `bytes.set_f64_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat64 (F) | 改名 → `put_f64_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_f64_le` | `bytes.set_f64_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat64 (F) | 改名 → `put_f64_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_i16_be` | `bytes.set_i16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setInt16 (F) | 改名 → `put_i16_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_i16_le` | `bytes.set_i16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setInt16 (F) | 改名 → `put_i16_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_i32_be` | `bytes.set_i32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setInt32 (F) | 改名 → `put_i32_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_i32_le` | `bytes.set_i32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setInt32 (F) | 改名 → `put_i32_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_i64_be` | `bytes.set_i64_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setBigInt64 (F) | 改名 → `put_i64_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_i64_le` | `bytes.set_i64_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setBigInt64 (F) | 改名 → `put_i64_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_u16_be` | `bytes.set_u16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setUint16 (F) | 改名 → `put_u16_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_u16_le` | `bytes.set_u16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setUint16 (F) | 改名 → `put_u16_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_u32_be` | `bytes.set_u32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setUint32 (F) | 改名 → `put_u32_be`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_u32_le` | `bytes.set_u32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setUint32 (F) | 改名 → `put_u32_le`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `set_u8` | `bytes.set_u8(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go b[i] = x / OCaml set_*_le・_be (F) / JS DataView setUint8 (F) | 改名 → `put_u8`（範囲外は abort） | R-bytes-01、R-bytes-11 |
| `skip` | `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | Go bytes.Reader.Seek / Python —  | 据え置き | — |
| `skip_length_prefixed_le` | `bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int` | —（形式専用） | 撤去（→ `get_u32_le` と `slice` の組み合わせ） | R-bytes-05 |
| `slice` | `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | Rust &b[a..b] / Go b[a:b] / Python b[a:b] / JS subarray・slice / OCaml Bytes.sub (F) | 意味変更（負の開始位置を 0 に丸める） | R-bytes-12 |
| `split` | `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` | Go bytes.Split / Python b.split | 据え置き | — |
| `starts_with` | `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` | Go bytes.HasPrefix / Python startswith / Gleam starts_with (F) | 据え置き | — |
| `take_at` | `bytes.take_at(b: Bytes, pos: Int, n: Int) -> (Int, Option[Bytes])` | Go bytes.Reader.Read / Python b[p:p+n] | 据え置き | — |
| `to_list` | `bytes.to_list(b: Bytes) -> List[Int]` | Python list(b) / JS Array.from / Kotlin toList | 据え置き | — |
| `to_string` | `bytes.to_string(b: Bytes) -> Result[String, String]` | Rust String::from_utf8 → Result / Gleam bit_array.to_string → Result (F) / Python b.decode() / Kotlin decodeToString | 据え置き | R-bytes-10 |
| `to_string_lossy` | `bytes.to_string_lossy(b: Bytes) -> String` | Rust String::from_utf8_lossy / Python decode(errors='replace') | 据え置き | R-bytes-10 |
| `write_bool` | `bytes.write_bool(mut b: Bytes, val: Bool) -> Unit` | Python struct.pack('?') / 多数: — | 改名 → `append_bool` | R-bytes-03 |
| `write_f64_be` | `bytes.write_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64(math.Float64bits) (F) | 撤去 → `append_f64_be` | R-bytes-03 |
| `write_i64_be` | `bytes.write_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) | 撤去 → `append_i64_be` | R-bytes-03 |
| `write_string_be` | `bytes.write_string_be(mut b: Bytes, s: String) -> Unit` | Java DataOutput.writeUTF（u16 BE） / 多数: — | 撤去（→ `append_u32_be` ＋ `append_bytes`） | R-bytes-05 |
| `write_u32_be` | `bytes.write_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) | 撤去 → `append_u32_be` | R-bytes-03 |
| `write_u8` | `bytes.write_u8(mut b: Bytes, val: Int) -> Unit` | 同上 | 撤去 → `push` | R-bytes-02 |
| `xor` | `bytes.xor(a: Bytes, b: Bytes) -> Bytes` | — (多数: ループで書く) | 据え置き | — |
| `read_uint16` | `bytes.read_uint16(b: Bytes, offset: Int, endian: Endian) -> UInt16` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `read_uint32` | `bytes.read_uint32(b: Bytes, offset: Int, endian: Endian) -> UInt32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `read_int32` | `bytes.read_int32(b: Bytes, offset: Int, endian: Endian) -> Int32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `read_float32` | `bytes.read_float32(b: Bytes, offset: Int, endian: Endian) -> Float32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_uint16` | `bytes.write_uint16(mut b: Bytes, value: UInt16, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_uint32` | `bytes.write_uint32(mut b: Bytes, value: UInt32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_int32` | `bytes.write_int32(mut b: Bytes, value: Int32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `write_float32` | `bytes.write_float32(mut b: Bytes, value: Float32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_uint16` | `bytes.set_uint16(mut b: Bytes, offset: Int, value: UInt16, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_uint32` | `bytes.set_uint32(mut b: Bytes, offset: Int, value: UInt32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_int32` | `bytes.set_int32(mut b: Bytes, offset: Int, value: Int32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `set_float32` | `bytes.set_float32(mut b: Bytes, offset: Int, value: Float32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | 撤去（→ 接尾辞の系統 `get_`/`append_`/`put_`） | R-bytes-04 |
| `put_f16_le` | `bytes.put_f16_le(mut b: Bytes, pos: Int, val: Float) -> Unit`（案） | JS setFloat16 (F) / Python struct '<e' | 追加 | R-bytes-03 |
| `put_f16_be` | `bytes.put_f16_be(mut b: Bytes, pos: Int, val: Float) -> Unit`（案） | JS setFloat16 (F) / Python struct '>e' | 追加 | R-bytes-03 |
| `append_f16_le` | `bytes.append_f16_le(mut b: Bytes, val: Float) -> Unit`（案） | Python struct.pack('<e') | 追加 | R-bytes-03 |
| `append_f16_be` | `bytes.append_f16_be(mut b: Bytes, val: Float) -> Unit`（案） | Python struct.pack('>e') | 追加 | R-bytes-03 |
| `get_f16_be` | `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]`（案） | JS getFloat16(o, false) (F) / Python struct '>e' | 追加 | R-bytes-03 |
| `read_f16_be_at` | `bytes.read_f16_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])`（案） | — | 追加 | R-bytes-03 |
| `get_f16_be_array` | `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`（案） | Python struct.unpack_from('>Ne') | 追加 | R-bytes-03 |
| `put_bool` | `bytes.put_bool(mut b: Bytes, pos: Int, val: Bool) -> Unit`（案） | Python struct.pack_into('?') | 追加（範囲外は abort） | R-bytes-03、R-bytes-11 |
| `append_bytes` | `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit`（案） | Rust extend_from_slice / Go append(b, src...) / Python bytearray.extend / OCaml Buffer.add_bytes | 追加 | R-bytes-05 |
| `take` | `bytes.take(b: Bytes, n: Int) -> Bytes`（案） | Rust &b[..n] / Python b[:n] / Kotlin take / Almide list.take | 追加 | R-bytes-08 |
| `drop` | `bytes.drop(b: Bytes, n: Int) -> Bytes`（案） | Rust &b[n..] / Python b[n:] / Kotlin drop / Almide list.drop | 追加 | R-bytes-08 |
| `take_end` | `bytes.take_end(b: Bytes, n: Int) -> Bytes`（案） | Python b[-n:] / Kotlin takeLast / Almide list.take_end | 追加 | R-bytes-08 |
| `drop_end` | `bytes.drop_end(b: Bytes, n: Int) -> Bytes`（案） | Python b[:-n] / Kotlin dropLast / Almide list.drop_end | 追加 | R-bytes-08 |

## L0 への差し戻し（処理済み）

1. **R-L0-01 と行列のセル。** R-L0-01 に「型の行列のセルは、実行時の本体が同じでも同義語に数えない。ただし行列の形が宣言され、ゲートで守られていること」が入りました。`append_i16_le` と `append_u16_le` は両方残します。#3851 F の同義語ゲートは、行列ゲート（R-bytes-03）の宣言から生まれたセルを除外します（名前の列挙では除外しません）。
2. **位置指定の書き込みの範囲外。** L0 に「位置を指定した書き込みは、範囲外を契約違反として abort する」（R-L0-12 の後ろ）が入りました。`put_<型>`・`set`・`insert`・`remove_at` を R-bytes-11 で abort にそろえます。
