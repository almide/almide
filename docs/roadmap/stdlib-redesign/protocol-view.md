<!-- description: Stdlib redesign — the protocol pyramid: the standard library's protocols layered after Swift, each type's conformances, the holes they expose and the decisions they force (R-L0-37) -->
# 標準ライブラリの protocol の階層

> Last updated: 2026-10-22
> 元の記録: [R-L0-37](L0-conventions.md)・[target-surface.md](target-surface.md)

R-L0-37 のとおり、標準の具体型を「準拠する protocol の組み合わせ」と「その型だけの関数」で書き表します。protocol の階層は Swift の標準ライブラリを手本にし、Swift にない層は理由を書いて足しています。準拠と穴は、[target-surface.md](target-surface.md)（リデザイン後の関数）から機械的に出しています。

- 値の型 31 種の関数 1,205 個のうち、822 個（68%）が protocol の要件か、数の型どうしの変換の族です。残りの 383 個が、その型だけの関数です。
- 準拠を決めると、要件のうちまだ無い関数が 3 個あります（穴）。既定の実装で埋まるものは数えていません。
- 手本にした Swift の protocol は、表の「Swift」の列に書いています。

## 階層

系統（土台・数・入れ物・分野）ごとの木です。下の protocol は上の protocol を含みます（継承）。ほかの系統の protocol も含むときは「＋」の後に書きます。

土台:

```
Eq
├─ Hash
└─ Ord
Repr
└─ Parse
Codec
```

数:

```
Additive  ＋ Eq
├─ Numeric
│  ├─ Signed
│  │  └─ FloatingPoint  ＋ Ord・Parse
│  │     └─ BinaryFloatingPoint
│  └─ Integer  ＋ Ord・Hash・Parse
│     ├─ FixedWidthInteger
│     ├─ SignedInteger  ＋ Signed
│     └─ UnsignedInteger
└─ TimeAmount  ＋ Ord
```

入れ物:

```
Sized
├─ Container
│  ├─ Membership
│  │  └─ SetAlgebra
│  ├─ Sequence
│  │  └─ Ordered
│  │     ├─ Indexed
│  │     └─ Text
│  ├─ Growable
│  └─ PriorityQueue
└─ KeyValue
```

分野:

```
HttpMessage
```

## protocol

| 層 | protocol | 含む | 要件（関数） | 既定の実装 | Swift | 準拠する型 |
|---|---|---|---|---|---|---|
| 土台 | `Eq` | — | — | — | Equatable | Bool・String・Bytes・List[A]・Set[A]・BigInt・Float・Float32・Float64・Duration・Compute・Uuid・Url・Option[A]・Result[A, E]・Value・Matrix・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 土台 | `Hash` | `Eq` | — | — | Hashable | Bool・String・Bytes・List[A]・Set[A]・BigInt・Duration・Compute・Uuid・Url・Option[A]・Result[A, E]・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 土台 | `Repr` | — | `to_string` | `to_string` | CustomStringConvertible | Bool・String・BigInt・Float・Float32・Float64・Uuid・Url・Value・Matrix・Regex・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 土台 | `Parse` | `Repr` | `parse` | — | LosslessStringConvertible | Bool・BigInt・Float・Float32・Float64・Uuid・Url・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 土台 | `Ord` | `Eq` | `compare` | `min`・`max`・`clamp` | Comparable | String・Bytes・BigInt・Float・Float32・Float64・Duration・Compute・Uuid・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 土台 | `Codec` | — | — | — | Codable | Bool・String・Bytes・List[A]・Deque[A]・Set[A]・Map[K, V]・BigInt・Float・Float32・Float64・Duration・Uuid・Url・Option[A]・Result[A, E]・Value・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `Additive` | `Eq` | — | — | AdditiveArithmetic | BigInt・Float・Float32・Float64・Duration・Compute・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `Numeric` | `Additive` | — | — | Numeric | BigInt・Float・Float32・Float64・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `Signed` | `Numeric` | `abs` | — | SignedNumeric | BigInt・Float・Float32・Float64・Int・Int8・Int16・Int32・Int64 |
| 数 | `Integer` | `Numeric`・`Ord`・`Hash`・`Parse` | `floor_div`・`mod`・`pow`・`is_even`・`is_odd`・`parse_radix`・`to_string_radix`・`band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr` | `is_even`・`is_odd` | BinaryInteger | BigInt・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `FixedWidthInteger` | `Integer` | `min_value`・`max_value`・`count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap` | — | FixedWidthInteger | Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `SignedInteger` | `Integer`・`Signed` | `sign` | — | SignedInteger | BigInt・Int・Int8・Int16・Int32・Int64 |
| 数 | `UnsignedInteger` | `Integer` | — | — | UnsignedInteger | UInt8・UInt16・UInt32・UInt64 |
| 数 | `FloatingPoint` | `Signed`・`Ord`・`Parse` | `nan`・`infinity`・`epsilon`・`min_value`・`max_value`・`is_nan`・`is_finite`・`is_infinite`・`floor`・`ceil`・`round`・`trunc`・`signum` | — | FloatingPoint | Float・Float32・Float64 |
| 数 | `BinaryFloatingPoint` | `FloatingPoint` | `to_bits`・`from_bits` | — | BinaryFloatingPoint | Float・Float32・Float64 |
| 数 | `TimeAmount` | `Additive`・`Ord` | `ns`・`us`・`ms`・`s`・`min`・`h` | — | DurationProtocol | Duration・Compute |
| 入れ物 | `Sized` | — | `len`・`is_empty` | `is_empty` | Collection（count・isEmpty） | String・Bytes・List[A]・Deque[A]・Set[A]・Heap[A]・Map[K, V] |
| 入れ物 | `Container` | `Sized` | `new`・`from_list`・`to_list`・`any`・`all`・`count`・`filter` | `any`・`all`・`count` | — | String・Bytes・List[A]・Deque[A]・Set[A]・Heap[A] |
| 入れ物 | `Membership` | `Container` | `contains` | — | Sequence.contains（Element: Equatable） | List[A]・Deque[A]・Set[A]・Heap[A] |
| 入れ物 | `Sequence` | `Container` | `fold`・`find` | `find` | Sequence | String・Bytes・List[A]・Deque[A]・Set[A] |
| 入れ物 | `Ordered` | `Sequence` | `first`・`last`・`reverse`・`take`・`drop`・`take_end`・`drop_end`・`slice` | — | BidirectionalCollection | String・Bytes・List[A]・Deque[A] |
| 入れ物 | `Indexed` | `Ordered` | `get`・`set` | — | RandomAccessCollection・MutableCollection | Bytes・List[A]・Deque[A] |
| 入れ物 | `Growable` | `Container` | `push`・`pop`・`insert`・`remove_at`・`clear` | — | RangeReplaceableCollection | String・Bytes・List[A]・Deque[A] |
| 入れ物 | `SetAlgebra` | `Membership` | `insert`・`remove`・`union`・`intersection`・`difference`・`symmetric_difference`・`is_subset`・`is_disjoint` | — | SetAlgebra | Set[A] |
| 入れ物 | `KeyValue` | `Sized` | `new`・`get`・`set`・`remove`・`contains`・`keys`・`values`・`update`・`upsert`・`merge`・`clear` | — | Dictionary（具体型） | Map[K, V] |
| 入れ物 | `PriorityQueue` | `Container` | `push`・`pop`・`peek`・`clear` | — | Heap（swift-collections） | Heap[A] |
| 入れ物 | `Text` | `Ordered` | `contains`・`starts_with`・`ends_with`・`index_of`・`split`・`repeat`・`pad_start`・`pad_end` | — | StringProtocol | String・Bytes |
| 分野 | `HttpMessage` | — | `header`・`header_values`・`headers`・`body`・`body_bytes`・`set_header`・`set_body`・`set_body_bytes` | — | — | HttpRequest・HttpResponse |

注:

- `Eq`: `==`・`!=`。関数ではなく演算子。`Fn` 以外のすべての値の型が自動で持つ。
- `Hash`: Map のキー・Set の要素になれる。浮動小数点数は持たない。
- `Repr`: `"${x}"` の中身。関数の形が `<mod>.to_string`。
- `Parse`: `parse(to_string(x)) == ok(x)` が成り立つ型だけが持つ。
- `Ord`: `<` など。`min`・`max`・`clamp` は `compare` から既定の実装で作る。
- `Codec`: `encode`・`decode`。導出で付く。
- `Additive`: `+`・`-` と 0。
- `Numeric`: `*`。
- `Signed`: 単項の `-`。
- `Integer`: `/`・`%`。`bigint` もここに入る（Swift の `BinaryInteger` は任意精度の整数も入れる設計）。数の型どうしの変換（`to_<T>`・`_checked`・`_saturating`・`_wrapping`）もここ（Swift の `init(_:)`・`init(exactly:)`・`init(clamping:)`・`init(truncatingIfNeeded:)`）。
- `FixedWidthInteger`: 幅の決まった整数。`count_trailing_zeros` は 0 のとき型の幅を返すので、幅のない `bigint` には置かない。
- `FloatingPoint`: `/`。順序は IEEE totalOrder（R-L0-16）。
- `TimeAmount`: `Duration`・`Compute`（R-duration-03）。
- `Sized`: `is_empty` は `len` から既定の実装。
- `Container`: 順序に頼らない問い合わせ。`filter` は同じ型を返す（Swift の `RangeReplaceableCollection.filter`）。Swift にない層で、`heap` のように中の順序を外に見せない入れ物のために置く（R-heap-05）。
- `Membership`: 要素が入っているか。要素が `Eq` のときだけ。
- `Sequence`: 決まった順に 1 つずつたどれる。
- `Ordered`: 両端から数えられる。
- `Indexed`: 位置で読み書きする。
- `Growable`: 足し引きできる。
- `KeyValue`: `contains` はキーを受け取る。
- `PriorityQueue`: Swift の `Heap` も `Sequence` ではない。
- `Text`: 部分の列で探す。`contains` は部分の列を受け取るので、`Membership` とは同じ型に載せられない。
- `HttpMessage`: R-http-12。

## 型ごとの準拠と穴

「準拠」は宣言する protocol です（含まれる protocol は省略）。「穴」は要件のうちまだ無い関数で、括弧はその要件を持つ protocol です。

| 型 | 準拠 | 関数 | その型だけ | 穴 |
|---|---|---|---|---|
| `Bool` | Parse・Hash・Codec | 2 | 0 | なし |
| `String` | Text・Growable・Ord・Repr・Hash・Codec | 54 | 27 | new（Container）、from_list（Container）、to_list（Container） |
| `Bytes` | Text・Indexed・Growable・Ord・Hash・Codec | 141 | 106 | なし |
| `List[A]` | Indexed・Growable・Membership・Hash・Codec | 73 | 46 | なし |
| `Deque[A]` | Indexed・Growable・Membership・Codec | 30 | 3 | なし |
| `Set[A]` | SetAlgebra・Sequence・Hash・Codec | 22 | 2 | なし |
| `Heap[A]` | PriorityQueue・Membership | 16 | 2 | なし |
| `Map[K, V]` | KeyValue・Codec | 22 | 9 | なし |
| `BigInt` | SignedInteger・Codec | 29 | 3 | なし |
| `Float` | BinaryFloatingPoint・Codec | 54 | 5 | なし |
| `Float32` | BinaryFloatingPoint・Codec | 51 | 3 | なし |
| `Float64` | BinaryFloatingPoint・Codec | 52 | 3 | なし |
| `Duration` | TimeAmount・Hash・Codec | 7 | 0 | なし |
| `Compute` | TimeAmount・Hash | 7 | 0 | なし |
| `Uuid` | Parse・Ord・Hash・Codec | 7 | 4 | なし |
| `Url` | Parse・Hash・Codec | 9 | 7 | なし |
| `Option[A]` | Hash・Codec | 13 | 13 | なし |
| `Result[A, E]` | Hash・Codec | 14 | 14 | なし |
| `Value` | Repr・Eq・Codec | 29 | 29 | なし |
| `Matrix` | Repr・Eq | 25 | 25 | なし |
| `Regex` | Repr | 15 | 15 | なし |
| `HttpRequest・HttpResponse` | HttpMessage | 34 | 26 | なし |
| `Int` | FixedWidthInteger・SignedInteger・Codec | 77 | 17 | なし |
| `Int8` | FixedWidthInteger・SignedInteger・Codec | 50 | 3 | なし |
| `Int16` | FixedWidthInteger・SignedInteger・Codec | 53 | 3 | なし |
| `Int32` | FixedWidthInteger・SignedInteger・Codec | 57 | 3 | なし |
| `Int64` | FixedWidthInteger・SignedInteger・Codec | 62 | 3 | なし |
| `UInt8` | FixedWidthInteger・UnsignedInteger・Codec | 39 | 3 | なし |
| `UInt16` | FixedWidthInteger・UnsignedInteger・Codec | 45 | 3 | なし |
| `UInt32` | FixedWidthInteger・UnsignedInteger・Codec | 52 | 3 | なし |
| `UInt64` | FixedWidthInteger・UnsignedInteger・Codec | 64 | 3 | なし |

## この階層で決めること

階層を決めると、次の点に答えが要ります。推奨は、Swift と、Almide の物差し（LLM が書いたコードがそのまま動くか）の両方から出しています。

| 問い | 事実 | 推奨 |
|---|---|---|
| `Bool` は `Ord` か | Swift は `Bool` を `Comparable` にしない（真偽に大小の意味はない）。Rust・Kotlin・OCaml・Python・Elixir は順序をつける。Almide はいま `false < true` が通る。 | 保留（R-bool-02）。`Ord` から外す方向で、R-L0-36 のとおり Dojo で測る（`sort_by` の鍵に `Bool` を使うコードがどれだけ落ちるか）。それまで `bool.compare`（R-bool-02）は保留。 |
| `Repr` のメソッド名 | 組み込みの `Repr` のメソッドは `repr`。一方でモジュールの関数は `int.to_string`。同じ役割に 2 つの綴り。 | 採用（R-L0-38・R-bool-03）。R-L0-22（`Ord.cmp` → `compare`）と同じく、`Repr` のメソッドを `to_string` にする。すると `<mod>.to_string` はすべての値の型にある（`bool.to_string` も、足す実装なしで生える）。 |
| `regex.parse` は `Parse` か | Swift の `Regex` は `LosslessStringConvertible` ではない。`regex.parse` は値の文字列表現を読むのではなく、パターンを組み立てる。 | `Regex` は `Parse` に入れない。名前の重なりは R-regex-02 の判断（Go `Compile`・Gleam `from_string` に合わせた）を、この観点で見直す。 |
| サイズつきの整数の算術 | `int` は `floor_div`・`mod`・`pow`・ビット演算・`abs`・`sign` を持つのに、`int8`〜`uint64` は持たない。`Integer`・`FixedWidthInteger` に入れると、これがそのまま穴になる。 | 採用（R-sized-10。R-sized-05 を見直し）。`Integer` の要件として全部の整数に置く。中身は既定の実装 1 つで、型ごとに書かない。 |
| `bigint` のビット演算 | Swift の `BinaryInteger` はビット演算を要件にする（任意精度でも 2 の補数として振る舞う）。`bigint` はいま持たない。 | 採用（R-bigint-07）。要件に含める。代わりに `bigint` を `Integer` から外す案は、`int` と `bigint` を同じ総称関数で扱えなくなるので採らない。 |
| `bytes.contains` の意味 | `Text` の `contains` は部分の列、`Membership` の `contains` は要素。Almide には重ね定義がないので、1 つの型は片方にしか入れない。 | `bytes` は `string` と同じ `Text` にする（`bytes.contains(b, pat)` のまま）。1 つの値は `bytes.any(b, (x) => x == v)` で書く。 |
| `string` は `Indexed` か | Swift の `String` は `BidirectionalCollection` 止まり（位置で読むのは O(1) ではない）。Almide の `string.get` はコードポイント単位。 | `Ordered` までにして、`string.get` は `string` だけの関数に残す。 |
| `deque` の `push`・`pop` | `Growable` の `push`・`pop` は末尾。`deque` は `push_back`・`pop_back` と綴る。 | 採用（R-deque-03）。`deque.push`・`deque.pop` を `Growable` の関数として置き、`push_back`・`pop_back` は置かない（`push_front`・`pop_front` は `deque` だけの関数に残す）。 |
| `int` の N bit の演算 | `int.wrap_add(a, b, bits)`・`rotate_left(a, n, bits)`・`bit_width`・`bshr_logical` は「`Int` の値を N bit の数として扱う」演算。固定幅の型では幅が型で決まるので、同じ名前を `bits` なしで置くと形が食い違う。 | `FixedWidthInteger` の要件に入れず、`int` だけの関数に残す。固定幅の型の折り返し演算が要るときは、`_wrapping` の変換（R-L0-34）と組み合わせる。名前を分けるかは、使われ方を測ってから決める。 |
| `string` の `new`・`from_list`・`to_list` | `Text` は `Container` を含むので、`string` もこの 3 つが要る。ところが `to_list` は `string.chars` と同じ役割で、`from_list` は `list.join(xs, "")` と同じになる。 | 保留。`chars` を `to_list` に改名するかどうか（Rust の `chars` に慣れた書き方が落ちるか）を Dojo で測ってから決める（R-L0-36）。 |
| `map`・`flat_map` | Swift は `Sequence.map` が常に `Array` を返し、`Optional`・`Result` の `map` は具体型の関数。入れ物そのものを引数にとる型（`F[_]`）を使わない。 | Swift に合わせて protocol にしない。型ごとの関数のまま。言語に `F[_]` は足さない。 |
| `list` の `new`・`from_list`・`to_list` | `Container` の要件だが、`list` は持たない（リテラル `[]` があり、`List` そのものなので）。総称関数の中で `T.new()` と書くには、`list` も持つ必要がある。 | 採用（R-list-20）。置く。中身は自明（`[]` と恒等）で、`list` を書くときに使う必要はない。 |
| 数の型どうしの変換 | Swift は変換先の型の総称イニシャライザ（`Int8(clamping: x)`）で 1 つにまとめる。Almide は変換元のモジュールに変換先ごとの名前（`int.to_int8_saturating`）を並べる。 | 名前の族のまま、R-L0-32・R-L0-34 の行列ゲートで守る。1 つの総称関数にするには綴りを変える必要があり、ここでは決めない。 |
| `map`（Map）は `Container` か | Swift の `Dictionary` は `(key, value)` の組の `Collection`。Almide の `map.filter` などは callback が 2 引数。 | `KeyValue` と `Sized` までにして、`Container` には入れない（callback の形が違うため）。 |

## 言語に要るもの

どれも今の Almide では書けません（実測は #4000）。どれも Swift が持つ機能です。入れ物そのものを引数にとる型（`F[_]`）は、Swift と同じく使いません。

| 機能 | 例 | 使うところ |
|---|---|---|
| protocol の継承 | `protocol Integer: Ord + Hash + Numeric + Parse` | 階層そのもの |
| 関連型 | `Container` の要素の型（`List[A]` は `A`、`Bytes` は `Int`） | 入れ物の層すべて |
| 既定の実装 | `compare` から `min`・`max`・`clamp`、`len` から `is_empty`、`fold` から `any`・`all`・`count` | 穴の多くが実装なしで埋まる |
| 受け手のない要件の呼び出し | 総称関数の中の `T.parse(s)`・`T.new()`・`T.max_value()`・`T.nan()` | `Parse`・`Container`・`FixedWidthInteger`・`FloatingPoint` |
| 条件つきの準拠 | `List[A]` は `A: Hash` のときだけ `Hash`、`contains` は要素が `Eq` のときだけ | `Hash`・`Membership` |

## その型だけの関数

- `String`（27）: `byte_len`、`byte_slice`、`capitalize`、`chars`、`from_codepoint`、`from_utf16`、`get`、`is_alpha`、`is_alphanumeric`、`is_digit`、`is_lower`、`is_upper`、`is_whitespace`、`last_index_of`、`lines`、`replace`、`replace_first`、`split_once`、`strip_prefix`、`strip_suffix`、`to_codepoint`、`to_lower`、`to_upper`、`to_utf16`、`trim`、`trim_end`、`trim_start`
- `Bytes`（106）: `append_bool`、`append_bytes`、`append_f16_be`、`append_f16_le`、`append_f32_be`、`append_f32_le`、`append_f64_be`、`append_f64_le`、`append_i16_be`、`append_i16_le`、`append_i32_be`、`append_i32_le`、`append_i64_be`、`append_i64_le`、`append_i8`、`append_u16_be`、`append_u16_le`、`append_u32_be`、`append_u32_le`、`append_u8`、`chunk`、`copy_from`、`copy_within`、`fill`、`from_string`、`get_bool`、`get_f16_be`、`get_f16_be_array`、`get_f16_le`、`get_f16_le_array`、`get_f32_be`、`get_f32_be_array`、`get_f32_le`、`get_f32_le_array`、`get_f64_be`、`get_f64_be_array`、`get_f64_le`、`get_f64_le_array`、`get_i16_be`、`get_i16_be_array`、`get_i16_le`、`get_i16_le_array`、`get_i32_be`、`get_i32_be_array`、`get_i32_le`、`get_i32_le_array`、`get_i64_be`、`get_i64_be_array`、`get_i64_le`、`get_i64_le_array`、`get_i8`、`get_u16_be`、`get_u16_be_array`、`get_u16_le`、`get_u16_le_array`、`get_u32_be`、`get_u32_be_array`、`get_u32_le`、`get_u32_le_array`、`get_u8`、`is_eof`、`is_valid_utf8`、`map`、`next_bool`、`next_bytes`、`next_f16_be`、`next_f16_le`、`next_f32_be`、`next_f32_le`、`next_f64_be`、`next_f64_le`、`next_i16_be`、`next_i16_le`、`next_i32_be`、`next_i32_le`、`next_i64_be`、`next_i64_le`、`next_i8`、`next_u16_be`、`next_u16_le`、`next_u32_be`、`next_u32_le`、`next_u8`、`put_bool`、`put_f16_be`、`put_f16_le`、`put_f32_be`、`put_f32_le`、`put_f64_be`、`put_f64_le`、`put_i16_be`、`put_i16_le`、`put_i32_be`、`put_i32_le`、`put_i64_be`、`put_i64_le`、`put_i8`、`put_u16_be`、`put_u16_le`、`put_u32_be`、`put_u32_le`、`put_u8`、`skip`、`to_string`、`to_string_lossy`、`xor`
- `List[A]`（46）: `binary_search`、`binary_search_by`、`binary_search_with`、`chunk`、`dedup`、`drop_while`、`enumerate`、`filter_map`、`find_index`、`find_map`、`flat_map`、`flatten`、`group_by`、`index_of`、`intersperse`、`iterate`、`join`、`map`、`max`、`max_by`、`max_with`、`min`、`min_by`、`min_with`、`partition`、`product`、`range`、`range_step`、`reduce`、`repeat`、`scan`、`sort`、`sort_by`、`sort_with`、`split_at`、`sum`、`swap`、`take_while`、`unique`、`unique_by`、`unzip`、`update`、`window`、`with_capacity`、`zip`、`zip_with`
- `Deque[A]`（3）: `map`、`pop_front`、`push_front`
- `Set[A]`（2）: `clear`、`map`
- `Heap[A]`（2）: `new_by`、`new_with`
- `Map[K, V]`（9）: `all`、`any`、`count`、`filter`、`find`、`fold`、`from_list`、`map`、`to_list`
- `BigInt`（3）: `clamp`、`max`、`min`
- `Float`（5）: `clamp`、`max`、`min`、`pow`、`to_fixed`
- `Float32`（3）: `clamp`、`max`、`min`
- `Float64`（3）: `clamp`、`max`、`min`
- `Uuid`（4）: `from_bytes`、`to_bytes`、`v4`、`v7`
- `Url`（7）: `build_query`、`decode_component`、`decode_form`、`encode_component`、`encode_form`、`parse_query`、`resolve`
- `Option[A]`（13）: `collect`、`collect_map`、`filter`、`flat_map`、`flatten`、`is_none`、`is_some`、`map`、`or_else`、`to_list`、`to_result`、`unwrap_or_else`、`zip`
- `Result[A, E]`（14）: `filter`、`flat_map`、`flatten`、`is_err`、`is_ok`、`map`、`map_err`、`or_else`、`partition`、`to_err_option`、`to_list`、`to_option`、`unwrap_or_else`、`zip`
- `Value`（29）: `array`、`as_array`、`as_bool`、`as_float`、`as_int`、`as_object`、`as_string`、`bool`、`float`、`get`、`get_array`、`get_bool`、`get_float`、`get_int`、`get_object`、`get_path`、`get_string`、`index`、`int`、`key`、`merge`、`null`、`object`、`omit`、`pick`、`remove_path`、`root`、`set_path`、`string`
- `Matrix`（25）: `broadcast_add_row`、`cols`、`concat_cols`、`concat_rows`、`dot_row`、`from_bytes_f16_le`、`from_bytes_f32_le`、`from_bytes_f64_le`、`from_lists`、`gather_rows`、`get`、`map`、`ones`、`rows`、`scale`、`shape`、`slice_rows`、`split_cols_even`、`to_bytes_f16_le`、`to_bytes_f32_le`、`to_bytes_f64_le`、`to_lists`、`transpose`、`zeros`、`zip_with`
- `Regex`（15）: `captures`、`captures_all`、`escape`、`find`、`find_all`、`index_of`、`is_full_match`、`is_match`、`new`、`parse`、`replace`、`replace_first`、`replace_first_with`、`replace_with`、`split`
- `HttpRequest・HttpResponse`（26）: `cancel`、`decode_json`、`get`、`get_response`、`json`、`method`、`mount`、`new_request`、`param`、`path`、`poll`、`query_params`、`read_new`、`redirect`、`request`、`response`、`route`、`router`、`send`、`serve`、`set_status`、`start`、`status`、`wait`、`with_headers`、`wrap`
- `Int`（17）: `bit_reverse`、`bit_width`、`bshr_logical`、`clamp`、`log2_ceil`、`log2_floor`、`max`、`min`、`next_power_of_two`、`prev_power_of_two`、`rotate_left`、`rotate_right`、`to_hex`、`wrap`、`wrap_add`、`wrap_mul`、`wrap_sub`
- `Int8`（3）: `clamp`、`max`、`min`
- `Int16`（3）: `clamp`、`max`、`min`
- `Int32`（3）: `clamp`、`max`、`min`
- `Int64`（3）: `clamp`、`max`、`min`
- `UInt8`（3）: `clamp`、`max`、`min`
- `UInt16`（3）: `clamp`、`max`、`min`
- `UInt32`（3）: `clamp`、`max`、`min`
- `UInt64`（3）: `clamp`、`max`、`min`
