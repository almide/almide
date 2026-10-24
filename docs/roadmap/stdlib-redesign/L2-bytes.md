<!-- description: Stdlib redesign L2/L3 — bytes: in-place verbs (push / put_ / append_), one byte-IO family, raw pointers out to ffi, and the same sequence names as list and string -->
# L2/L3: bytes

> Last updated: 2026-10-24
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
| 読み位置を進めながら読む | `next_<型>`、`next_bytes`、`skip`、`is_eof`（いまは `read_<型>_at`、`take_at`、`eof`） | `Cursor` | `bytes.Reader` + `binary.Read` | `io.BytesIO` | — | パターン一致 | パターン一致 | — | `ByteBuffer`(JVM) | — |
| 部分列・連結・検索 | `slice`、`concat`、`index_of`、`contains`、`starts_with`、`ends_with`、`split`、`lines` | ● | `bytes.*` | ● | ● | `slice`、`starts_with` (F) | `binary_part`、`:binary.match` | `sub`、`cat` (F) | ● | ● |
| 先頭・末尾から n 個 | **—**（`slice` だけ） | `&b[..n]` | `b[:n]` | `b[:n]` | `subarray` | `slice` (F) | `binary_part` | `sub` | `take` / `takeLast` | `prefix` / `suffix` |
| 生ポインタ | `as_ptr`、`as_mut_ptr`、`data_ptr`、`from_raw_ptr`、`copy_to_ptr` | `as_ptr` ほか | `unsafe` | `ctypes` | — | — | — | — | — | `withUnsafeBytes` |

### 完全性の規則

1. **バイナリの読み書きは 1 系統の行列にする。** 行列の軸は「型 × 向き × 操作」です。
   - 型: `u8`、`u16`、`i16`、`u32`、`i32`、`i64`、`f16`、`f32`、`f64`、`bool`。
   - 向き: 2 バイト以上の型は `_le` と `_be` の両方を持ちます。`u8` と `bool` は向きを持ちません。
   - 操作: `get_<型>`（位置を指定して読む。範囲外は none）、`next_<型>`（読み位置を進める版。`(Option[値], 次の位置)` を返す。R-bytes-13・R-L0-23）、`get_<型>_array`（2 バイト以上の型だけ。1 つでも範囲外なら none）、`put_<型>`（位置を指定して上書き、その場）、`append_<型>`（末尾に書き足す、その場）。
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

### R-bytes-12 `slice` の負の開始位置を 0 に丸める — ○ 採用（→ R-L0-29 で abort に変更）

- 六巡目の注: 負の開始位置を 0 に丸める部分は、R-L0-29 で abort に変わりました。正の側のはみ出しを長さまで丸める部分は、この案のままです。
- 内容: `bytes.slice(b, start, end)` の負の `start` を 0 に丸めます。いまは `bytes.slice(b, -1, 2)` が長さ 0 を返します。`string.slice("abc", -1, 2)` と同じく、先頭の 2 バイトを返すようにします。
- 根拠の L0 規則: R-L0-12（範囲外の添字は端に丸める。負の添字は範囲外）。
- (a) Python・JS は負の添字を末尾からの位置に使いますが、R-L0-12 はそれを採りません。丸めの規則は Gleam・Elixir の `slice` と同じです。
- (b) 純粋な計算です。両 target で同じ挙動にそろえます。
- (c) list・string・bytes で同じ引数の結果が違うと、ある型で覚えた端の挙動が別の型で外れます。
- (d) 意味変更（端の入力の結果だけ。文書の「capped」に合わせる修正）。名前と型は変えないので、`@deprecated` は要りません。影響の件数は spec・tests で負の開始位置を渡している呼び出しを数えて PR に書きます。
- ゲート案: R-L0-12 の境界行列テスト（list・string・bytes × `slice`・`take`・`drop`・`take_end`・`drop_end` × {-1, 0, len, len+1}）。
- issue: #3944

### R-bytes-13 読み位置を進める族を `next_<型>` にし、`(値, 次の位置)` を返す — ○ 採用

- いま: `read_<型>_at(b, pos) -> (Int, Option[T])`（18）と `take_at(b, pos, n) -> (Int, Option[Bytes])`。次の位置が先、値が後です。
- 案: `next_<型>(b, pos) -> (Option[T], Int)`、`next_bytes(b, pos, n) -> (Option[Bytes], Int)`。`skip(b, pos, n) -> Int` と `is_eof(b, pos) -> Bool` は名前も形も据え置きます（状態だけを返し、値を返さないので R-L0-23 の対象外です）。
- 根拠の L0 規則: R-L0-23（状態を進める関数は `next_`、`(値, 次の状態)`）。`random.next_int(rng, …) -> (Int, Rng)` と同じ形になります。
- 読みに失敗したときは位置を進めません（いまの `read_u32_le_at` が短い入力で `(0, none)` を返すのと同じ）。改名後もこの約束を保ちます。`skip(b, pos, n)` は `n < 0` で負の位置を返すので（実測: `skip(a, 1, -2)` が `-1`）、`n < 0` を契約違反として abort にします（R-L0-12、R-L0-06 の 3）。
- (a) Rust でよく使われる bytes ライブラリの `get_i32_le` は読み位置を進める版で、Almide の `get_<型>`（位置を指定して読むだけ、R-bytes-09）と意味が逆です。進める版を `next_` にすれば、`get_` の意味は 1 つに決まります。
- (c) `let (v, pos) = bytes.next_u32_le(b, pos)` と、乱数の `let (n, rng) = random.next_int(rng, 0, 9)` が同じ形で書けます。
- (d) 改名と意味変更です（タプルの順序が変わる）。旧い書き方は型エラーで止まるので、R-L0-14.4 を満たします。epoch 15 で旧名を `@deprecated`、epoch 16 で撤去します。`almide fix` は名前を書き換え、分解の順序はヒントで案内します。
- 完全性の規則: `get_<型>`（位置を指定して読む）・`next_<型>`（進める）・`put_<型>`（位置を指定して書く）・`append_<型>`（末尾に書く）の 4 列が、同じ型の行（`u16`〜`i64`・`f16`〜`f64` の le/be と `bool`）をすべて持ちます。`u8` の行だけは、`get` 列が `bytes.get`、`append` 列が `bytes.push` です（同じ役割の関数がすでにあるため。R-bytes-02・R-bytes-09）。`get_<型>_array` は 2 バイト以上の型だけです。
- ゲート案: 4 列 × 型の行列を interface JSON から検査するテスト（R-bytes-03 の行列ゲートを広げる）。
- issue: #3962

### R-bytes-14 `bytes.heap_save`・`bytes.heap_restore` を公開の面から外す — ○ 採用

- いま: `stdlib/bytes.almd` に公開の `fn heap_save() -> Int` と `fn heap_restore(checkpoint: Int) -> Unit` があります。この記録の L3 表にも載っていませんでした（記録の漏れ）。
- 案: 2 関数を内部の名前（`__heap_save`・`__heap_restore`）にし、ランタイムの fixture だけが使うようにします。
- 根拠の L0 規則: R-L1-26（メモリ安全でない関数を標準に置かない）、R-L0-02（主語が Bytes ではない）、R-L0-15（戻り値が target で違う。native は 0、wasm はヒープの番地）。
- (a) ヒープの巻き戻しを標準の公開 API に持つ言語はありません。
- (b) wasm で巻き戻した後の値を使うと壊れます。
- (c) `bytes` は自動 import なので、補完の候補に出ます。「メモリを解放したい」と思って呼ぶと、wasm でだけ壊れます。
- (d) 撤去 2（内部に残す）。epoch 15 で `@deprecated`、epoch 16 で公開から外します。使用数: spec 4 ファイル（すべてランタイムの試験）、stdlib 2。
- ゲート案: R-bytes-03 の行列ゲートが「行列と列の関数の表にない bytes の公開関数」を拒むようにします（今回の漏れも止まります）。
- issue: #3970

### R-bytes-15 `i8` の行を読み書きの行列に足す — ○ 採用

- いま: 行列の型は u8・u16・i16・u32・i32・i64・f16・f32・f64・bool で、i8 は意図した抜けでした。理由は「`bytes.read_u8(b, p) |> int.to_int8` で読める」ですが、`read_u8` は R-bytes-09 で撤去され、`int.to_int8` は `Int8` を返します（ほかのセルは `Int`）。
- 案: `get_i8(b, pos) -> Int?`、`next_i8(b, pos) -> (Int?, Int)`、`put_i8(mut b, pos, val: Int)`、`append_i8(mut b, val: Int)` を足します。
- 根拠の L0 規則: R-L0-01 の行列の例外（符号の有無で本体が同じセルも両方置く）、CLAUDE.md「族は行列で広げる」。
- (a) 符号つき 8 bit の読みは OCaml `Bytes.get_int8` (F)、JS `DataView.getInt8`、Python `struct` の `b`、Java `readByte` が持ちます。
- (c) `get_i16_le` があって `get_i8` がないと、規則から推測した名前が外れます。符号の変換を手で書くと `if x >= 128 then x - 256 else x` を誤りやすくなります。
- (d) 追加 4。
- 完全性の規則: 型の軸は {u8, i8, u16, i16, u32, i32, i64, f16, f32, f64, bool}。u64 だけが抜けで、理由は「Int に入らない。`get_i64_*` が同じビット列」です。この名前は R-L0-19 の案内の対象です。
- issue: #3970

### R-bytes-16 `Bytes` も `+` で連結し、`bytes.concat` を撤去する — ○ 採用

- いま: `fn bytes.concat(a: Bytes, b: Bytes) -> Bytes`。`a + b` は「operator '+' requires numeric, String, or List types」で拒まれます。list と string は `+` で連結し、`concat` 関数を持ちません（R-list-12）。
- 案: 検査器と両 target の lowering で `Bytes + Bytes -> Bytes` を受け付け、`bytes.concat` を撤去します。CHEATSHEET の「`+` は文字列とリストの連結」を「列（String・List・Bytes）の連結」に広げます。
- 根拠の L0 規則: R-L0-00 の 1（列の 3 つの型のうち Bytes だけ連結の書き方が違う）、R-L0-01。
- (a) Python `b1 + b2`、Kotlin `ByteArray.plus`、Swift `Data + Data`、Elixir `<>`。関数で書くのは Go `append`、OCaml `Bytes.cat`、Gleam `bit_array.append` です。
- (b) native は `Vec<u8>` の連結、wasm は self-host の `bytes_concat` を呼ぶだけです。
- (c) 実測で `a + b` が型エラーです。list と string で `+` を覚えた LLM は Bytes にも書きます。
- (d) 言語の変更（受理が広がるだけ）と撤去 1。`@deprecated(since=15, use="+")`、epoch 16 で撤去。`almide fix`: `bytes.concat(a, b)` → `a + b`。`html.concat` は `SafeHtml` の型なので対象外です。
- ゲート案: `+` の型の行列（String・List・Bytes × 両 target）の fixture。als に先に契約を入れます。
- issue: #3970

### R-bytes-17 `bytes.lines` を撤去する（`string.lines` と `\r` の扱いが違う） — ○ 採用

- いま: `bytes.lines(bytes.from_string("a\r\nb\n"))` の最初の行は `[97, 13]`（`\r` を残す）、`string.lines("a\r\nb\n")` の最初の行は `"a"`（`\r` を取る）。同じ動詞で挙動が違います。
- 案: `bytes.lines` を撤去します。テキストの行は `string.lines(bytes.to_string(b)!)`、バイトのまま `\n` で分けるときは `bytes.split(b, bytes.from_string("\n"))` と書きます。
- 根拠の L0 規則: R-L0-00 の 1（同じ動詞がモジュールで違う意味）、R-L0-14.4（同じ名前のまま挙動を変えることはできない）。新しい名前に移す案は、名前の多数派がないので取りません。
- (a) バイト列の行分割で `\r` を取るのは Go `bufio.ScanLines`、Python `bytes.splitlines`、Rust `BufRead::lines` です。残す標準の関数は見当たりません。
- (c) Windows 由来のファイルを `fs.read_bytes` から行に分けると、行末の `\r` が黙って残り、比較が合わなくなります。
- (d) 撤去。epoch 15 で `@deprecated`、epoch 16 で撤去。形が変わるので `almide fix` はせず、ヒントで 2 つの書き方を示します。使用数: spec 9 / docs 1。
- issue: #3970

### R-bytes-18 bytes に高階関数の族を置く（`filter`・`fold`・`any`・`all`・`count`・`find`） — ○ 採用

- いま: bytes の高階関数は `map` だけです。
- 案: list・deque と同じ D3 の族を置きます。`fn bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`、`fold(b, init: B, f: (B, Int) -> B) -> B`、`any`・`all`（`(Int) -> Bool`）、`count(b, f) -> Int`、`find(b, f) -> Int?`。`contains` はすでにあります（部分列の意味。R-L0-01 で list の `contains` と主語の型どおりに違う）。
- 根拠の L0 規則: R-map-06（D3 の族は順序の決まったコレクションすべてに置く）。bytes は `map` だけを持つ半端な形になっていました。
- (a) Rust の `[u8]`、Kotlin `ByteArray`、Swift `[UInt8]`・`Data`、Python `bytes` は列として族をすべて使えます。
- (b) 純粋な計算です。`bytes.to_list |> list.fold` は native で 8 倍の大きさの `List[Int]` を作るので、直接の `fold` は検査和のような処理で速くなります。
- (c) `bytes.map` を見た LLM は `bytes.fold`・`bytes.filter` も書きます。いまは E002 です。
- (d) 追加 6。
- 完全性の規則: 順序の決まったコレクション（list・deque・bytes）と集合系（map・set）は D3 の 7 つを持ちます。string は文字を値として持たない（`chars` が `List[String]` を返す）ので対象外です。
- ゲート案: R-map-06 の行列に bytes の列を足します。性質テスト: どのセルも `bytes.to_list(b) |> list.<同名>(f)` と同じ結果です。
- issue: #3970

### R-bytes-19 `_array` の読みだけがある理由を規則に書く — ○ 採用（文書だけ）

- 案: 完全性の規則の「意図した抜け」に「書き込み（`put_`・`append_`）と読み位置を進める版（`next_`）の `_array` は持たない。`for` で 1 つずつ書く」を足し、行列ゲートの宣言に入れます。
- (a) 配列をまとめて書くのは Python `struct.pack`、Go `binary.Write` で、多数派ではありません。
- (c) 抜けが宣言されていれば、R-L0-19 の案内で `append_f32_le_array` を書いた LLM を正しい形に導けます。
- issue: #3970

### R-bytes-20 `first`・`last`・`pop` を足す — ○ 採用

- いま: bytes は `get`・`push` を持つのに、`first`・`last`・`pop` がありません。list（`first`・`last`・`pop`）と deque（`first`・`last`・`pop`）から類推すると E002 になります。
- 案: `fn bytes.first(b: Bytes) -> Int?`、`fn bytes.last(b: Bytes) -> Int?`、`fn bytes.pop(mut b: Bytes) -> Int?`。
- 根拠の L0 規則: R-L0-12（列の型は同じ綴りの関数を持つ）、R-L0-06（失敗しうる取り出しは無印で Option）。
- (a) Rust `Vec<u8>` の `first`・`last`・`pop`、Python `bytearray.pop`、Kotlin `ByteArray.first`/`lastOrNull`、Swift `Data.first`/`last`/`popLast`。
- (b) 純粋な計算です。`pop` はその場で書き換える関数で、`push` と同じ扱いです。
- (d) 追加 3。
- 完全性の規則: `push` を持つ列の型（list・bytes）は `pop`・`first`・`last` も持ちます。
- issue: #3980

### R-bytes-21 `bytes.skip` を `next_` の形にする — × 不採用

- 理由: `skip` は値を読まずに位置だけを進めるので、`(値, 次の状態)` の値がありません。`(Unit, Int)` を返す形は、読む側が捨てる値を増やすだけです。R-L0-27 に「値を読まない位置の移動は次の状態だけを返す」と書きました。
- issue: なし

### R-bytes-22 `bytes.chunk` は `size <= 0` で abort — ○ 採用

- いま: `list.chunk` は R-list-10 で `n <= 0` を abort にしましたが、`bytes.chunk` には決まりがありません。
- 案: `list.chunk` と同じく abort にします。
- 根拠の L0 規則: R-L0-12（境界の行列は list・string・bytes で同じ）。
- (d) 意味変更（受け付けない入力を abort にする）。
- issue: #3985

### R-bytes-23 `bytes.xor` は長さが違うと abort — ○ 採用

- いま: 長さが違うと、短いほうに切り詰めます（実測 `almide 0.68.0 dev 437b8f127`、native と wasm で同じ: `[1, 2, 3]` と `[1]` で `[0]`）。止まらずに、短い結果を返します。
- 案: 長さが違えば abort にします。matrix の「形の不一致は abort」と同じ扱いで、長さは呼び出す前に `bytes.len` で確かめられます。
- 根拠の L0 規則: R-L0-06 の 3（呼び出し側が確かめられる前提の違反は abort）。
- (d) 意味変更。いまは切り詰めて返している呼び出しが abort になるので、epoch 15 の `breaks` に入れます。
- issue: #3985

### R-bytes-24 `bytes.set` と `put_u8` を 1 つにする — × 不採用

- 理由: `set(b, i, v) -> Bytes` はコピーを返す関数、`put_u8(mut b, pos, v)` はその場で書き換える関数で、R-L0-04（コピーが既定、その場書き換えは決まった動詞）の対です。`list.set` もコピーです。
- issue: なし

### R-bytes-25 型つきの読み書きの行列に `get_u8`・`append_u8` を戻す — ○ 採用（R-bytes-02 の一部を見直し）

- いま: 型つきの読み書きの族は {`get_`, `put_`, `next_`, `append_`} × {型} ですが、`u8` の列だけ `get_u8` と `append_u8` がありません（`get_i8`・`put_u8`・`next_u8` はあります）。R-bytes-02 で `append_u8` を `push` に、R-bytes-09 で `get_u8` を `get` に寄せたためです。3 つの外部レビューが続けて族の穴として挙げました。
- 案: `fn bytes.get_u8(b: Bytes, pos: Int) -> Int?` を足し、`append_u8` を撤去せずに残します。`bytes.get`・`bytes.push` は入れ物としての読み書き（`list.get`・`list.push` と同じ形）として残します。
  - 同じ計算が 2 つの名前を持ちますが、2 つの族（入れ物の族と、型つきの読み書きの族）のそれぞれのセルです。CLAUDE.md の「族は行列で揃える」を、R-L0-01（同義語を置かない）より先に当てます。
- (a) Go `binary.LittleEndian` と JS `DataView` の `getUint8`、OCaml `Bytes.get_uint8`。
- (d) 追加 1、撤去の取り消し 1。
- 完全性の規則: 型つきの読み書きの 4 つの動詞は、`u8` を含むすべての型の列を持ちます。
- issue: #3991

### R-bytes-26 `bytes.get_u8` と `get`、`append_u8` と `push` を 1 つにする — × 不採用（役割の違いを明記）

- 理由: 2 つは結果が同じですが、属する族が違います。`get`・`push` は入れ物の族（`list.get`・`deque.push` と同じ形）、`get_u8`・`append_u8` は符号化の族（`get_u16_le`・`append_u16_le` … の 1 バイトのセル）です。符号化の族から `u8` だけを抜くと、`get_<型>` の行列に穴ができ、規則から推して書いた `get_u8` が外れます（R-bytes-25）。
- 約束: 2 つは定義として同じ値を返します（`get_u8(b, i) == get(b, i)`、`append_u8` と `push` は同じ書き足し）。行列のゲートで、この一致も確かめます。
- issue: なし

### R-bytes-27 `bytes` に `Text` の要件をそろえる — ○ 採用

- 内容: `last_index_of`・`count_matches`・`replace`・`replace_first`・`strip_prefix`・`strip_suffix` を置きます。形と意味は `string` の同じ関数と同じで、位置はバイト単位です。
- 理由: protocol の階層で `bytes` は `string` と同じ `Text`（部分の列で探す・置き換える）です。Go の `bytes` パッケージ（`LastIndex`・`ReplaceAll`・`CutPrefix`）、Python の `bytes`（`rfind`・`replace`・`removeprefix`）、Rust のスライス（`strip_prefix`）、Swift の `Collection`（`trimmingPrefix`・`replacing`）が持っています。
- (d) 追加 6。
- issue: #4012

### R-bytes-28 `bytes.new(len)` を `bytes.zeros(n)` に改名し、`bytes.new()` を空のバイト列にする — ○ 採用

- いま: 入れ物の `new()` は空の入れ物を作ります（`list`・`string`・`deque`・`set`・`heap`・`map`）。`bytes.new(len)` だけが「長さ `len` の 0 で埋めたバイト列」を作り、引数の数が違います。protocol の階層の `Container.new` の形に合いません。
- 案: いまの関数を `bytes.zeros(n: Int) -> Bytes` に改名し（`matrix.zeros` と同じ綴り）、`bytes.new() -> Bytes` を空のバイト列にします。
- (a) 0 で埋めた長さ n のバッファは、どの言語も「長さを渡す」形です（Rust `vec![0; n]`、Go `make([]byte, n)`、Python `bytes(n)`、JS `new Uint8Array(n)`、Swift `Data(count: n)`）。空のバッファは別の形です（Rust `Vec::new()`、Swift `Data()`）。
- 名前の使い直し（R-L0-43 の 2）: 古い `bytes.new(10)` は引数の数が合わず型エラーになるので、黙って意味が変わりません。
- (d) 改名と追加です。`almide fix` が `bytes.new(n)` を `bytes.zeros(n)` に書き換えます。
- issue: #4012

### R-bytes-29 定数時間の比較 `bytes.is_equal_constant_time` を置く — ○ 採用

- いま: `==` は最初に違うバイトで返ります。MAC やトークンを `==` で比べると、時間から値を 1 バイトずつ当てられます。
- 案: `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`。長さが違えばすぐ false（長さは公開）。同じ長さなら全部のバイトを読み、`a[i] xor b[i]` の OR を最後に 1 回だけ 0 と比べます。`string.is_equal_constant_time`（R-string-19）と {Bytes, String} の型の行列のセルです（R-L0-01 の行列の例外）。
- native は `@intrinsic`（runtime で毎回 `core::hint::black_box`）、wasm は添字の比較だけが分岐の固定のループです。`tests/constant_time_compare_structure_test.rs` が出力したコードを検査します。
- 根拠の L0 規則: R-L0-09（Bool は `is_`）、R-L0-30（対等な 2 つは `a`・`b`）。
- (a) Go `subtle.ConstantTimeCompare`、Python `hmac.compare_digest`、Node `crypto.timingSafeEqual`、Java `MessageDigest.isEqual`。
- protocol（R-L0-37）: `String` と `Bytes` が同じ役割で持つので、`Text` の要件として 1 回だけ宣言します（[protocol-view](protocol-view.md) の `Text` の行。モジュールの関数は要件そのもの）。
- 綴り（R-L0-00）: 多数派はありません（Go `ConstantTimeCompare`・`hmac.Equal`、Python `compare_digest`、Node `timingSafeEqual`、Erlang `hash_equals`、Rust `subtle` の `ct_eq`、Java `isEqual`）。`compare` は `Ord` の 3 値（R-L0-16）なので使わず、Bool の `is_`（R-L0-09）に「等しい」と「定数時間」をそのまま書きます。
- (d) 追加。契約 C-421。issue: #4106（R-hash-06）

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
| `append_u8` | `bytes.append_u8(mut b: Bytes, val: Int) -> Unit` | Rust push / Python append / Go append(b, x) | 据え置き | R-bytes-25 |
| `as_mut_ptr` | `bytes.as_mut_ptr(b: Bytes) -> RawPtr` | Rust as_mut_ptr | 撤去 → `ffi.address` | R-bytes-06 |
| `as_ptr` | `bytes.as_ptr(b: Bytes) -> RawPtr` | Rust as_ptr / Go unsafe.SliceData | 撤去 → `ffi.address` | R-bytes-06 |
| `chunks` | `bytes.chunks(b: Bytes, size: Int) -> List[Bytes]` | Rust chunks / Kotlin chunked / Almide list.chunk | 改名 → `chunk` | R-bytes-08 |
| `clear` | `bytes.clear(mut b: Bytes) -> Unit` | Rust clear / Python bytearray.clear | 据え置き | — |
| `cmp` | `bytes.cmp(a: Bytes, b: Bytes) -> Int` | Rust cmp / Go bytes.Compare / Gleam compare (F) / OCaml Bytes.compare (F) | 改名 → `compare` | R-bytes-08 |
| `concat` | `bytes.concat(a: Bytes, b: Bytes) -> Bytes` | Python a + b / Gleam bit_array.append (F) / OCaml Bytes.cat (F) / Go append(a, b...) | 撤去 → `+` | R-bytes-16 |
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
| `lines` | `bytes.lines(b: Bytes) -> List[Bytes]` | Go bufio.ScanLines / Python splitlines | 撤去（→ `string.lines(bytes.to_string(b)!)` か `bytes.split`） | R-bytes-17 |
| `map_each` | `bytes.map_each(b: Bytes, f: (Int) -> Int) -> Bytes` | Kotlin map / Python bytes(map(f, b)) / Almide list.map | 改名 → `map` | R-bytes-08 |
| `new` | `bytes.new(len: Int) -> Bytes` | Rust vec![0; n] / Go make([]byte, n) / Python bytes(n) / JS new Uint8Array(n) / OCaml Bytes.make (F) / Kotlin ByteArray(n) | 改名 → `zeros` | R-bytes-28 |
| `pad_left` | `bytes.pad_left(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padStart / Almide string.pad_start | 改名 → `pad_start` | R-bytes-08 |
| `pad_right` | `bytes.pad_right(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padEnd / Almide string.pad_end | 改名 → `pad_end` | R-bytes-08 |
| `push` | `bytes.push(mut b: Bytes, val: Int) -> Unit` | Rust Vec::push / Python bytearray.append / Almide list.push | 据え置き | R-bytes-02 |
| `set_at` | `bytes.set_at(mut b: Bytes, i: Int, val: Int) -> Unit` | OCaml set_uint8 (F) / JS setUint8 (F) / 多数: b[i] = v | 撤去 → `put_u8`（範囲外は abort） | R-bytes-02、R-bytes-11 |
| `copy_within` | `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` | Rust copy_within / JS copyWithin | 据え置き | — |
| `read_bool` | `bytes.read_bool(b: Bytes, pos: Int) -> Bool` | Python struct '?' / 多数: — (read_u8 != 0) | 改名 → `get_bool`（範囲外は false ではなく none） | R-bytes-09 |
| `read_bool_at` | `bytes.read_bool_at(b: Bytes, pos: Int) -> (Int, Option[Bool])` | — | 改名 → `next_bool`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_f16_le` | `bytes.read_f16_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint16 (F) / JS getFloat16(o, true) (F) / Python struct '<e' | 改名 → `get_f16_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f16_le_at` | `bytes.read_f16_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_f16_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_f16_le_array` | `bytes.read_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Ne') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f16_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_be` | `bytes.read_f32_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint32 (F) / JS getFloat32(o, false) (F) / Python struct '>f' | 改名 → `get_f32_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_be_array` | `bytes.read_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nf') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_f32_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_be_at` | `bytes.read_f32_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_f32_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_f32_le` | `bytes.read_f32_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint32 (F) / JS getFloat32(o, true) (F) / Python struct '<f' | 改名 → `get_f32_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_le_array` | `bytes.read_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nf') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f32_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f32_le_at` | `bytes.read_f32_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_f32_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_f64_be` | `bytes.read_f64_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint64 (F) / JS getFloat64(o, false) (F) / Python struct '>d' | 改名 → `get_f64_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_be_array` | `bytes.read_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nd') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_f64_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_be_at` | `bytes.read_f64_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_f64_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_f64_le` | `bytes.read_f64_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint64 (F) / JS getFloat64(o, true) (F) / Python struct '<d' | 改名 → `get_f64_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_le_array` | `bytes.read_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nd') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_f64_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_f64_le_at` | `bytes.read_f64_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_f64_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_i16_be` | `bytes.read_i16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_int16_be (F) / JS getInt16(o, false) (F) / Python struct '>h' | 改名 → `get_i16_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i16_be_at` | `bytes.read_i16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_i16_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_i16_be_array` | `bytes.read_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nh') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i16_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i16_le` | `bytes.read_i16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_int16_le (F) / JS getInt16(o, true) (F) / Python struct '<h' | 改名 → `get_i16_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i16_le_at` | `bytes.read_i16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_i16_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_i16_le_array` | `bytes.read_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nh') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i16_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_be` | `bytes.read_i32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getInt32(o, false) (F) / Python struct '>i' | 改名 → `get_i32_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_be_array` | `bytes.read_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Ni') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i32_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_be_at` | `bytes.read_i32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_i32_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_i32_le` | `bytes.read_i32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getInt32(o, true) (F) / Python struct '<i' | 改名 → `get_i32_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_le_array` | `bytes.read_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Ni') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i32_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i32_le_at` | `bytes.read_i32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_i32_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_i64_be` | `bytes.read_i64_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint64 (F) / OCaml get_int64_be (F) / JS getBigInt64(o, false) (F) / Python struct '>q' | 改名 → `get_i64_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_be_array` | `bytes.read_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nq') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_i64_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_be_at` | `bytes.read_i64_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_i64_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_i64_le` | `bytes.read_i64_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint64 (F) / OCaml get_int64_le (F) / JS getBigInt64(o, true) (F) / Python struct '<q' | 改名 → `get_i64_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_le_array` | `bytes.read_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nq') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_i64_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_i64_le_at` | `bytes.read_i64_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_i64_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_length_prefixed_strings_le` | `bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]` | —（形式専用） | 撤去（→ `get_u32_le` と `slice` の組み合わせ） | R-bytes-05 |
| `read_string_at` | `bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String` | Python b[p:p+n].decode / Rust from_utf8_lossy(&b[p..p+n]) | 撤去（→ `slice` ＋ `to_string_lossy`） | R-bytes-05 |
| `read_string_be` | `bytes.read_string_be(b: Bytes, pos: Int) -> String` | Java DataInput.readUTF（u16 BE） / 多数: — | 撤去（→ `get_u32_be` と `slice` の組み合わせ） | R-bytes-05 |
| `read_string_be_at` | `bytes.read_string_be_at(b: Bytes, pos: Int) -> (Int, Option[String])` | — | 撤去（→ `next_u32_be` と `next_bytes` の組み合わせ） | R-bytes-05 |
| `read_u16_be` | `bytes.read_u16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_uint16_be (F) / JS getUint16(o, false) (F) / Python struct '>H' | 改名 → `get_u16_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_be_at` | `bytes.read_u16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_u16_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_u16_be_array` | `bytes.read_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NH') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_u16_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_le` | `bytes.read_u16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_uint16_le (F) / JS getUint16(o, true) (F) / Python struct '<H' | 改名 → `get_u16_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_le_array` | `bytes.read_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NH') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_u16_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u16_le_at` | `bytes.read_u16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_u16_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_u32_be` | `bytes.read_u32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getUint32(o, false) (F) / Python struct '>I' | 改名 → `get_u32_be`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_be_array` | `bytes.read_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NI') / Go binary.Read(r, BigEndian, []T) | 改名 → `get_u32_be_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_be_at` | `bytes.read_u32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_u32_be`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_u32_le` | `bytes.read_u32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getUint32(o, true) (F) / Python struct '<I' | 改名 → `get_u32_le`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_le_array` | `bytes.read_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NI') / Go binary.Read(r, LittleEndian, []T) | 改名 → `get_u32_le_array`（範囲外は 0 ではなく none） | R-bytes-09 |
| `read_u32_le_at` | `bytes.read_u32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型。Go bytes.Reader + binary.Read) | 改名 → `next_u32_le`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `read_u8` | `bytes.read_u8(b: Bytes, pos: Int) -> Int` | OCaml get_uint8 (F) / JS getUint8 (F) / 多数: b[i] | 撤去 → `get`（同じ役割。範囲外は none） | R-bytes-09 |
| `read_u8_at` | `bytes.read_u8_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (多数: 位置を持つ reader 型) | 改名 → `next_u8`（`(値, 次の位置)` を返す） | R-bytes-13 |
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
| `skip` | `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | Go bytes.Reader.Seek / Python — | 意味変更（`n < 0` は abort） | R-bytes-13、R-bytes-21 |
| `skip_length_prefixed_le` | `bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int` | —（形式専用） | 撤去（→ `get_u32_le` と `slice` の組み合わせ） | R-bytes-05 |
| `slice` | `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | Rust &b[a..b] / Go b[a:b] / Python b[a:b] / JS subarray・slice / OCaml Bytes.sub (F) | 意味変更（負の位置は abort。R-L0-29） | R-bytes-12、R-L0-29 |
| `split` | `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` | Go bytes.Split / Python b.split | 据え置き | — |
| `starts_with` | `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` | Go bytes.HasPrefix / Python startswith / Gleam starts_with (F) | 据え置き | — |
| `take_at` | `bytes.take_at(b: Bytes, pos: Int, n: Int) -> (Int, Option[Bytes])` | Go bytes.Reader.Read / Python b[p:p+n] | 改名 → `next_bytes`（`(値, 次の位置)` を返す） | R-bytes-13 |
| `to_list` | `bytes.to_list(b: Bytes) -> List[Int]` | Python list(b) / JS Array.from / Kotlin toList | 据え置き | — |
| `to_string` | `bytes.to_string(b: Bytes) -> Result[String, String]` | Rust String::from_utf8 → Result / Gleam bit_array.to_string → Result (F) / Python b.decode() / Kotlin decodeToString | 据え置き | R-bytes-10 |
| `to_string_lossy` | `bytes.to_string_lossy(b: Bytes) -> String` | Rust String::from_utf8_lossy / Python decode(errors='replace') | 据え置き | R-bytes-10 |
| `write_bool` | `bytes.write_bool(mut b: Bytes, val: Bool) -> Unit` | Python struct.pack('?') / 多数: — | 改名 → `append_bool` | R-bytes-03 |
| `write_f64_be` | `bytes.write_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64(math.Float64bits) (F) | 撤去 → `append_f64_be` | R-bytes-03 |
| `write_i64_be` | `bytes.write_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) | 撤去 → `append_i64_be` | R-bytes-03 |
| `write_string_be` | `bytes.write_string_be(mut b: Bytes, s: String) -> Unit` | Java DataOutput.writeUTF（u16 BE） / 多数: — | 撤去（→ `append_u32_be` ＋ `append_bytes`） | R-bytes-05 |
| `write_u32_be` | `bytes.write_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) | 撤去 → `append_u32_be` | R-bytes-03 |
| `write_u8` | `bytes.write_u8(mut b: Bytes, val: Int) -> Unit` | 同上 | 撤去 → `push` | R-bytes-02 |
| `xor` | `bytes.xor(a: Bytes, b: Bytes) -> Bytes` | — (多数: ループで書く) | 意味変更（長さが違うと abort） | R-bytes-23 |
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
| `next_f16_be` | `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)`（案） | — | 追加 | R-bytes-03、R-bytes-13 |
| `get_f16_be_array` | `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]`（案） | Python struct.unpack_from('>Ne') | 追加 | R-bytes-03 |
| `put_bool` | `bytes.put_bool(mut b: Bytes, pos: Int, val: Bool) -> Unit`（案） | Python struct.pack_into('?') | 追加（範囲外は abort） | R-bytes-03、R-bytes-11 |
| `append_bytes` | `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit`（案） | Rust extend_from_slice / Go append(b, src...) / Python bytearray.extend / OCaml Buffer.add_bytes | 追加 | R-bytes-05 |
| `take` | `bytes.take(b: Bytes, n: Int) -> Bytes`（案） | Rust &b[..n] / Python b[:n] / Kotlin take / Almide list.take | 追加（負の数は abort。R-L0-29） | R-bytes-08 |
| `drop` | `bytes.drop(b: Bytes, n: Int) -> Bytes`（案） | Rust &b[n..] / Python b[n:] / Kotlin drop / Almide list.drop | 追加（負の数は abort。R-L0-29） | R-bytes-08 |
| `take_end` | `bytes.take_end(b: Bytes, n: Int) -> Bytes`（案） | Python b[-n:] / Kotlin takeLast / Almide list.take_end | 追加（負の数は abort。R-L0-29） | R-bytes-08 |
| `drop_end` | `bytes.drop_end(b: Bytes, n: Int) -> Bytes`（案） | Python b[:-n] / Kotlin dropLast / Almide list.drop_end | 追加（負の数は abort。R-L0-29） | R-bytes-08 |
| `heap_save` | `bytes.heap_save() -> Int` | — | 撤去（内部の名前にする） | R-bytes-14 |
| `heap_restore` | `bytes.heap_restore(checkpoint: Int) -> Unit` | — | 撤去（内部の名前にする） | R-bytes-14 |
| `get_i8` | `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]`（案） | OCaml `get_int8` (F) / JS `getInt8` | 追加 | R-bytes-15 |
| `next_i8` | `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)`（案） | — | 追加 | R-bytes-15、R-bytes-13 |
| `put_i8` | `bytes.put_i8(mut b: Bytes, pos: Int, val: Int) -> Unit`（案） | OCaml `set_int8` / JS `setInt8` | 追加 | R-bytes-15 |
| `append_i8` | `bytes.append_i8(mut b: Bytes, val: Int) -> Unit`（案） | — | 追加 | R-bytes-15 |
| `filter` | `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`（案） | Kotlin `filter` / Rust `iter().filter` | 追加 | R-bytes-18 |
| `fold` | `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B`（案） | Kotlin `fold` / Rust `iter().fold` | 追加 | R-bytes-18 |
| `any` | `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool`（案） | Kotlin `any` | 追加 | R-bytes-18 |
| `all` | `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool`（案） | Kotlin `all` | 追加 | R-bytes-18 |
| `count` | `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int`（案） | Kotlin `count {}` | 追加 | R-bytes-18 |
| `find` | `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]`（案） | Kotlin `find` | 追加 | R-bytes-18 |
| `first` | （追加）`bytes.first(b: Bytes) -> Option[Int]` | Rust first / Swift first | 追加 | R-bytes-20 |
| `last` | （追加）`bytes.last(b: Bytes) -> Option[Int]` | Rust last / Swift last | 追加 | R-bytes-20 |
| `pop` | （追加）`bytes.pop(mut b: Bytes) -> Option[Int]` | Rust Vec::pop / Python bytearray.pop | 追加 | R-bytes-20 |
| `get_u8` | （追加）`bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]` | Go `LittleEndian.Uint8` / JS `getUint8` / OCaml `get_uint8` | 追加 | R-bytes-25 |
| `enumerate` | （追加）`bytes.enumerate(b: Bytes) -> List[(Int, Int)]` | Swift `enumerated` / Rust `enumerate` | 追加 | R-L0-42 |
| `zip` | （追加）`bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` | Swift `zip` / Rust `zip` | 追加 | R-L0-42 |
| `zip_with` | （追加）`bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 追加 | R-L0-42 |
| `filter_map` | （追加）`bytes.filter_map(b: Bytes, f: (Int) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 追加 | R-L0-42 |
| `find_map` | （追加）`bytes.find_map(b: Bytes, f: (Int) -> B?) -> B?` | Rust `find_map` | 追加 | R-L0-42 |
| `reduce` | （追加）`bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Int?` | Swift `reduce` / Rust `reduce` | 追加 | R-L0-42 |
| `scan` | （追加）`bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 追加 | R-L0-42 |
| `partition` | （追加）`bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` | Kotlin `partition` | 追加 | R-L0-42 |
| `group_by` | （追加）`bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 追加 | R-L0-42 |
| `sort` | （追加）`bytes.sort(b: Bytes) -> List[Int]` | Swift `sorted()` | 追加 | R-L0-42 |
| `sort_by` | （追加）`bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` | Kotlin `sortedBy` | 追加 | R-L0-42 |
| `sort_with` | （追加）`bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` | Swift `sorted(by:)` | 追加 | R-L0-42 |
| `min` | （追加）`bytes.min(b: Bytes) -> Int?` | Swift `min()` / Rust `min` | 追加 | R-L0-42 |
| `max` | （追加）`bytes.max(b: Bytes) -> Int?` | Swift `max()` / Rust `max` | 追加 | R-L0-42 |
| `min_by` | （追加）`bytes.min_by(b: Bytes, f: (Int) -> K) -> Int?` | Kotlin `minByOrNull` | 追加 | R-L0-42 |
| `max_by` | （追加）`bytes.max_by(b: Bytes, f: (Int) -> K) -> Int?` | Kotlin `maxByOrNull` | 追加 | R-L0-42 |
| `min_with` | （追加）`bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | Swift `min(by:)` | 追加 | R-L0-42 |
| `max_with` | （追加）`bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | Swift `max(by:)` | 追加 | R-L0-42 |
| `sum` | （追加）`bytes.sum(b: Bytes) -> Int` | Rust `sum` / Kotlin `sum` | 追加 | R-L0-42 |
| `product` | （追加）`bytes.product(b: Bytes) -> Int` | Rust `product` | 追加 | R-L0-42 |
| `find_index` | （追加）`bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Int?` | Swift `firstIndex(where:)` | 追加 | R-L0-42 |
| `take_while` | （追加）`bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | Swift `prefix(while:)` | 追加 | R-L0-42 |
| `drop_while` | （追加）`bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | Swift `drop(while:)` | 追加 | R-L0-42 |
| `window` | （追加）`bytes.window(b: Bytes, n: Int) -> List[Bytes]` | Kotlin `windowed` | 追加 | R-L0-42 |
| `split_at` | （追加）`bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` | Rust `split_at` | 追加 | R-L0-42 |
| `dedup` | （追加）`bytes.dedup(b: Bytes) -> Bytes` | Rust `dedup` | 追加 | R-L0-42 |
| `intersperse` | （追加）`bytes.intersperse(b: Bytes, sep: Int) -> Bytes` | Rust `intersperse` | 追加 | R-L0-42 |
| `unique` | （追加）`bytes.unique(b: Bytes) -> Bytes` | Kotlin `distinct` | 追加 | R-L0-42 |
| `unique_by` | （追加）`bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` | Kotlin `distinctBy` | 追加 | R-L0-42 |
| `last_index_of` | （追加）`bytes.last_index_of(b: Bytes, needle: Bytes) -> Int?` | Go `bytes.LastIndex` / Python `rfind` | 追加 | R-bytes-27 |
| `replace` | （追加）`bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | Go `bytes.ReplaceAll` / Python `replace` | 追加 | R-bytes-27 |
| `replace_first` | （追加）`bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | Go `bytes.Replace(…, 1)` / Python `replace(…, 1)` | 追加 | R-bytes-27 |
| `strip_prefix` | （追加）`bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Bytes?` | Rust `strip_prefix` / Go `CutPrefix` | 追加 | R-bytes-27 |
| `strip_suffix` | （追加）`bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Bytes?` | Rust `strip_suffix` / Go `CutSuffix` | 追加 | R-bytes-27 |
| `count_matches` | （追加）`bytes.count_matches(b: Bytes, needle: Bytes) -> Int` | Go `bytes.Count` / Python `count` | 追加 | R-bytes-27 |
| `new` | （追加）`bytes.new() -> Bytes` | Rust `Vec::new` / Swift `Data()` / Go `[]byte{}` | 追加 | R-bytes-28 |
| `is_equal_constant_time` | `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`（案） | Go `subtle.ConstantTimeCompare` / Python `hmac.compare_digest` / Node `crypto.timingSafeEqual` | 追加 | R-bytes-29 |

## L0 への差し戻し（処理済み）

1. **R-L0-01 と行列のセル。** R-L0-01 に「型の行列のセルは、実行時の本体が同じでも同義語に数えない。ただし行列の形が宣言され、ゲートで守られていること」が入りました。`append_i16_le` と `append_u16_le` は両方残します。#3851 F の同義語ゲートは、行列ゲート（R-bytes-03）の宣言から生まれたセルを除外します（名前の列挙では除外しません）。
2. **位置指定の書き込みの範囲外。** L0 に「位置を指定した書き込みは、範囲外を契約違反として abort する」（R-L0-12 の後ろ）が入りました。`put_<型>`・`set`・`insert`・`remove_at` を R-bytes-11 で abort にそろえます。
