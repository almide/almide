# L1 事実集: 9 言語の標準ライブラリ — カテゴリ（モジュール）単位の比較

調べた日: 2026-10-21。目的は「事実を集めること」。ここには決定や提案は書かない。

## 0. 読み方

### 0.1 記号

| 記号 | 意味 |
|---|---|
| ● | 標準ライブラリに入っている（すべてのプラットフォーム、または言語の共通部分） |
| ◐ | 入っているが条件つき: JVM の JDK だけ（Kotlin）、Apple OS だけ（Swift）、Erlang/OTP 側のモジュール（Elixir）、コンパイラ同梱の別ライブラリ unix/str/threads（OCaml）、Deno std / Bun / Node だけ（JS）など。セルに理由を書く |
| pkg: | 標準ではない。言語チームの公式パッケージだけがある |
| — | 標準にない |
| [F] | このセッションで公式ドキュメントを WebFetch して確かめたセル |

「数（●/◐）」列 = 9 言語のうち ● の数 / ◐ の数。pkg は数えない。
Almide 列は 2026-10 時点の `docs/stdlib/README.md`（44 モジュール 999 関数）から。

### 0.2 「標準」の範囲（言語ごと）

| 言語 | 標準として数えた範囲 | 数えなかったもの（pkg か注記のみ） |
|---|---|---|
| Rust | `std` crate（stable）。unstable は注記 | rust-lang org の crate（regex, log）は pkg |
| Go | `pkg.go.dev/std` の全パッケージ | `golang.org/x/...` |
| Python | 標準ライブラリ（3.15 の module index） | PyPI |
| TS/JS | ECMAScript 組み込み + ランタイム共通の Web API（fetch, URL, crypto.subtle 等）を ●。Node 組み込み / Deno `@std` / Bun 組み込みだけにあるものは ◐ | npm |
| Gleam | `gleam_stdlib` だけ | gleam-lang org の公式パッケージ（gleam_json 等）は pkg |
| Elixir | Elixir 本体のモジュール（+ 同梱アプリ Logger / ExUnit / EEx）を ●。Erlang/OTP（stdlib, kernel, crypto, inets, ssl…）は Elixir から依存なしで呼べるので ◐(OTP) | Hex パッケージ |
| OCaml | `Stdlib` を ●。コンパイラ同梱の unix / str / threads を ◐ | Jane Street の Base/Core は注記だけ（数えない） |
| Kotlin | `kotlin-stdlib` を ●（Common か全ターゲット）。JVM 専用の stdlib パッケージや JDK クラスは ◐(JVM) | kotlinx.* / Ktor は pkg |
| Swift | Swift 標準ライブラリ + ツールチェーン同梱の Foundation（swift-foundation, Linux/Windows でも同梱）を ●。Apple OS だけのフレームワークは ◐(Apple) | swift-collections などは pkg |

### 0.3 参照した公式ドキュメントの根（ルート）URL

| 言語 | ルート URL | 確認した版 |
|---|---|---|
| Rust | https://doc.rust-lang.org/std/ [F] | 1.99.1 |
| Go | https://pkg.go.dev/std [F] | go1.27.3 |
| Python | https://docs.python.org/3/library/ ・ https://docs.python.org/3/py-modindex.html [F] | 3.15.0 |
| TS/JS | ECMAScript: https://tc39.es/ecma262/ ・ 完了提案: https://github.com/tc39/proposals/blob/main/finished-proposals.md [F] ・ MDN: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects ・ Node: https://nodejs.org/api/ [F] ・ Deno std: https://jsr.io/@std [F] ・ Bun: https://bun.com/docs/runtime/utils [F] | Node 26.12, Deno std 43 パッケージ |
| Gleam | https://gleam-stdlib.hexdocs.pm/ （旧 https://hexdocs.pm/gleam_stdlib/ ）[F] ・ 公式 org: https://github.com/gleam-lang [F] | gleam_stdlib 1.0.5 |
| Elixir | https://hexdocs.pm/elixir/api-reference.html [F] ・ Erlang stdlib: https://www.erlang.org/doc/apps/stdlib/api-reference.html [F] | Elixir 1.20.4 / OTP 29.1.2 |
| OCaml | https://ocaml.org/manual/latest/api/index.html [F] ・ Core: https://ocaml.org/p/core/latest | OCaml 5.5 |
| Kotlin | https://kotlinlang.org/api/core/kotlin-stdlib/ [F] ・ 公式ライブラリ: https://kotlinlang.org/docs/api-references.html [F] | 2.4 |
| Swift | https://developer.apple.com/documentation/swift/swift-standard-library （本文は取得できず）・ https://github.com/swiftlang/swift-foundation [F] ・ swift-collections: https://swiftpackageindex.com/apple/swift-collections/documentation [F] | swift-collections 1.7.3 |
| WASI | https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md [F] ・ https://github.com/WebAssembly/wasi-clocks [F] | — |

**取得で確かめたこと（[F] の中身）**
- Rust std のモジュール一覧（stable / unstable。`random`, `f16`, `f128` は unstable）。
- Go std の全パッケージ一覧。`uuid` が std にある（`uuid.NewV4`, `NewV7`, `Parse`）こと（https://pkg.go.dev/uuid ）。`simd` もある。
- Python 3.15 の module index（`compression`, `tomllib`, `zoneinfo`, `uuid` がある）。
- TC39 完了提案の年: Temporal = 2027 版、Uint8Array base64/hex = 2026、Iterator helpers = 2025、Set methods = 2025、Upsert(Map.getOrInsert) = 2026、Math.sumPrecise = 2026。
- Deno `@std` の 43 パッケージ一覧。`@std/encoding` は hex / base32 / base58 / base64 / base64url / ascii85 / varint。`@std/data-structures` に BinaryHeap, BinarySearchTree, (Readonly)Deque など。
- Node 26 の API 一覧（SQLite, Zlib, UDP/datagram, TTY, Test runner, Web Crypto, FFI など）。
- Bun の組み込み: gzip/deflate/zstd, escapeHTML, randomUUIDv7, stringWidth/stripANSI, TOML/YAML 読み込み。
- gleam_stdlib 1.0.5 のモジュールは 19 個だけ: bit_array, bool, bytes_tree, dict, dynamic, dynamic/decode, float, function, int, io, list, option, order, pair, result, set, string, string_tree, uri。regex と iterator は stdlib から外れて公式パッケージ `gleam_regexp`, `gleam_yielder` になった。gleam-lang org に json, crypto, time, http, httpc, erlang, otp がある。
- Elixir 1.20 のモジュール一覧（`JSON`, `Duration`, `OptionParser`, `IO.ANSI`, `Base`, `URI`, `Version`, `Regex`, `Calendar.TimeZoneDatabase` がある）。
- Erlang/OTP 29 stdlib の一覧（`argparse`, `json`, `queue`, `gb_trees`, `gb_sets`, `base64`, `uri_string`, `calendar`, `rand`, `zip`, `erl_tar`, `zstd`, `io_ansi`, `digraph` がある）。
- OCaml 5.5 Stdlib の一覧（`Dynarray`, `Pqueue`(5.4〜), `Iarray`, `Pair`, `Repr`, `Domain`, `Effect`, `Atomic`, `Arg`, `Buffer`, `Digest`, `Seq` がある）。外部: unix, str, threads。
- Kotlin stdlib 2.4 のパッケージと対応プラットフォーム（`kotlin.uuid` と `kotlin.io.encoding` は Common+JVM、`kotlin.time` は全ターゲットで `Instant`/`Clock` が 2.3 から、`kotlin.system` は JVM+Native だけ）。公式 kotlinx 一覧: coroutines, serialization, kotlinx-io, kotlinx-datetime, collections.immutable, Ktor。
- swift-collections のモジュール: DequeModule, HeapModule, OrderedCollections, BitCollections, HashTreeCollections ほか。
- swift-foundation は Linux/Windows のツールチェーンに同梱（FoundationEssentials / FoundationInternationalization）。
- WASI: Phase 3 = cli, clocks, filesystem, sockets, random, http。Phase 2 = clocks の timezone。Phase 1 = crypto, tls, kv-store, native-threads。

[F] の付いていないセルは記憶による（よく知られた API）。不確かなものは脚注に「未確認」と書いた。

---

## 1. カテゴリ比較表

列の並び: Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift

### 1.A 数値と基本型

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 ●/◐ |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 整数（既定の Int） | `int` | `i64` 等 primitive の inherent メソッド | builtin `int` + `strconv`, `math` | `int` | `Number` / `BigInt` | `gleam/int` [F] | `Integer` [F] | `Int` [F] | `Int`/`Long`（kotlin） | `Int` | 9/0 |
| 固定幅整数 8〜64 bit | `int8` `int16` `int32` `int64` `uint8` `uint16` `uint32` `uint64`（8 モジュール） | `i8`〜`u64` primitive（各型のメソッド。`std::i8` 等の旧モジュールは非推奨） | builtin 型 `int8`〜`uint64` + `math/bits`, `strconv`, `encoding/binary` | —（`struct`, `array`, `ctypes` で表現だけ） | ◐ 型付き配列 `Int8Array`〜`BigUint64Array`, `BigInt.asIntN/asUintN` | —（Int のみ。bit array の幅指定だけ） | —（任意精度整数。`<<x::8>>` で幅指定） | ◐ `Int32` `Int64` `Nativeint` [F]（8/16 bit 型はない） | `Byte` `Short` `Int` `Long` `UByte` `UShort` `UInt` `ULong` | `Int8`〜`UInt64` 構造体 | 4/2 |
| 128 bit 整数 | — | `i128`/`u128` | — | （int は任意精度） | — | — | （任意精度） | — | — | `Int128`/`UInt128`（Swift 6） | 2/0 |
| 浮動小数（既定） | `float` | `f64` | `float64` + `math` | `float` | `Number` | `gleam/float` [F] | `Float` [F] | `Float` [F] | `Double` | `Double` | 9/0 |
| 32 bit 浮動小数 | `float32`, `float64` モジュール | `f32`, `f64`（定数モジュール + primitive）[F] | `float32` | —（`struct`/`array` 'f' のみ） | ◐ `Float32Array`, `Math.fround`, `Float16Array`(2025) | — | —（bitstring `::float-32`） | —（`Int32.float_of_bits` のみ） | `Float` | `Float`, `Float16`, `Float80` | 4/1 |
| 任意精度整数 BigInt | — | — | `math/big` (Int) [F] | `int` | `BigInt` | ◐ Int（Erlang ターゲットでは任意精度、JS では違う） | `Integer`（任意精度） | —（Zarith は外部） | ◐ `java.math.BigInteger`(JVM) | — | 4/2 |
| 10 進小数 / 有理数 | — | — | `math/big` (Rat, Float) | `decimal`, `fractions` [F] | — | — | —（Decimal は Hex パッケージ） | — | ◐ `java.math.BigDecimal`(JVM) | `Decimal`(Foundation) | 3/1 |
| 複素数 | — | — | builtin `complex128` + `math/cmplx` [F] | `complex` + `cmath` [F] | — | — | — | `Complex` [F] | — | pkg: swift-numerics | 3/0 |
| 数学関数 | `math` | `f64` メソッド | `math` | `math` | `Math` | ◐ `gleam/float` の一部だけ（三角関数なし） | ◐ `:math`(OTP) | `Float` | `kotlin.math` [F] | Foundation（Glibc/Darwin 経由）+ pkg: swift-numerics | 6/2 |
| 統計 | — | — | — | `statistics` | — | — | — | — | — | — | 1/0 |
| ビット演算ヘルパ（popcount 等） | `int` の一部 | 整数メソッド `count_ones` 等 | `math/bits` | `int.bit_count` | `Math.clz32` | `gleam/int` bitwise_* | `Bitwise` [F] | `Int` の land/lor | `Int.countOneBits` 等 | `nonzeroBitCount` 等 | 9/0 |
| SIMD | — | `arch`（stable）, `simd`（unstable）[F] | `simd`, `simd/archsimd` [F] | — | — | — | — | — | — | `SIMD2`〜`SIMD64` 型 | 3/0 |
| 行列 / 線形代数 | `matrix`（46 関数） | — | — | —（`@` 演算子だけあり実装なし） | —（`DOMMatrix` はブラウザだけ） | — | —（Nx は外部） | ◐ `Bigarray`（多次元配列の入れ物。演算なし） | — | ◐ `simd` フレームワーク（Apple） | 0/2 |

### 1.B 文字列とテキスト

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 文字列 | `string` | `str`, `string` | `strings`, `strconv` | `str`, `string` | `String` | `gleam/string` [F] | `String` [F] | `String` [F] | `kotlin.text` | `String` | 9/0 |
| 文字型 / Unicode 分類 | `string.is_digit` 等、`codepoint`（文字型モジュールはない） | `char` [F] | `unicode`, `unicode/utf8` [F] | `unicodedata` | `String.prototype.codePointAt`, `Intl.Segmenter` | `string.to_utf_codepoints`, `to_graphemes` | `String.graphemes`, `:unicode`(OTP) | `Char`, `Uchar` [F] | `Char`（`category`） | `Character`, `Unicode.Scalar` | 9/0 |
| UTF-16 / Latin-1 等の符号化 | — | `String::from_utf16`, `encode_utf16` | `unicode/utf16` [F] | `codecs` | `TextDecoder`（utf-16le 等）, Node `Buffer` latin1 | — | ◐ `:unicode.characters_to_binary`(OTP) | `Bytes.get_utf_16le_uchar` 等 | ◐ `java.nio.charset`(JVM)（Common は UTF-8 のみ） | `String(decoding:as: UTF16.self)`, `String.Encoding` | 6/2 |
| 文字列ビルダー | —（`+` と `list.join`） | `String`（可変） | `strings.Builder`, `bytes.Buffer` | `io.StringIO` | —（配列 join が慣用） | `gleam/string_tree` [F] | iodata（`IO.iodata_to_binary`） | `Buffer` [F] | `StringBuilder` | `String`（可変） | 8/0 |
| 書式化 printf | 文字列補間、`string.pad_*` | `fmt`（`format!`） | `fmt` | `str.format`, f-string, `string.templatelib` | テンプレートリテラル, ◐ Deno `@std/fmt` printf | — | `inspect`, ◐ `:io_lib.format` | `Printf`, `Format` [F] | ◐ `String.format`(JVM) | ◐ `String(format:)`(Foundation) | 5/3 |
| テキスト整形（折返し・差分・幅） | — | — | `text/tabwriter` [F] | `textwrap`, `difflib` | ◐ Deno `@std/text`, Bun `stringWidth`/`wrapAnsi` | — | — | — | — | — | 2/1 |
| 正規表現 | `regex` | pkg: `regex`（rust-lang） | `regexp` [F] | `re` | `RegExp` | pkg: `gleam_regexp` [F] | `Regex` [F] | ◐ `Str`（同梱ライブラリ）[F] | `Regex` | `Regex`（5.7〜）, `NSRegularExpression` | 6/1 |
| テンプレートエンジン | —（`html` は安全な連結だけ） | — | `text/template`, `html/template` [F] | `string.Template` | — | — | EEx（同梱アプリ） | — | — | — | 3/0 |
| ロケール / 国際化書式 | — | — | — | `locale`, `gettext` | `Intl` | — | — | — | ◐ JVM | `Locale`, FormatStyle（FoundationInternationalization） | 3/1 |

### 1.C コレクション

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| リスト / 可変長配列 | `list` | `Vec`, `vec`, `slice` | slice + `slices` | `list` | `Array` | `gleam/list`（連結リスト）[F] | `List`, `Enum` | `List`, `Array`, `Dynarray` [F] | `List`/`ArrayList` | `Array` | 9/0 |
| ハッシュマップ | `map`（挿入順を保つ） | `HashMap` | builtin map + `maps` | `dict`（挿入順） | `Map`（挿入順） | `gleam/dict` [F] | `Map` | `Hashtbl` | `Map`（`mapOf` は挿入順） | `Dictionary` | 9/0 |
| 挿入順マップ | `map` | — | — | `dict` | `Map` | — | `Keyword`（リスト） | — | `LinkedHashMap`（既定） | pkg: `OrderedDictionary` [F] | 4/0 |
| ソート済みマップ（木） | — | `BTreeMap`, `BTreeSet` | — | — | ◐ Deno `RedBlackTree`/`BinarySearchTree` [F] | — | ◐ `:gb_trees`, `:gb_sets`(OTP) [F] | `Map`, `Set`（平衡木・順序あり） | ◐ `sortedMapOf`→`TreeMap`(JVM) | — | 2/3 |
| 集合 | `set`（挿入順） | `HashSet` | —（`map[T]struct{}`） | `set` | `Set` + 2025 集合演算 | `gleam/set` [F] | `MapSet` | `Set` | `Set` | `Set` | 8/0 |
| 両端キュー / キュー | — | `VecDeque` | ◐ `container/list`（双方向リスト）[F] | `collections.deque`, `queue` | ◐ Deno `@std/data-structures` Deque [F] | —（旧 `gleam/queue` は stdlib にない [F]） | ◐ `:queue`(OTP) [F] | `Queue`, `Stack` | `ArrayDeque` | pkg: `Deque` [F] | 4/3 |
| ヒープ / 優先度キュー | — | `BinaryHeap` | `container/heap` [F] | `heapq` | ◐ Deno `BinaryHeap` [F] | — | —（`gb_sets` で代用） | `Pqueue`（5.4〜）[F] | ◐ `java.util.PriorityQueue`(JVM) | pkg: `Heap` [F] | 4/2 |
| 連結リスト | （`list` は配列ベース） | `LinkedList` | `container/list`, `container/ring` | — | — | `gleam/list` | `List` | `List` | ◐ `LinkedList`(JVM) | — | 5/1 |
| ビットセット | — | — | — | — | — | — | — | — | ◐ `java.util.BitSet`(JVM) | pkg: `BitSet` [F] | 0/1 |
| 多重集合 / カウンタ | — | — | — | `collections.Counter` | — | — | — | — | — | — | 1/0 |
| 遅延シーケンス / イテレータ | —（list は即時。`fs.fold_lines` 系だけ逐次） | `iter` [F] | `iter`（1.23〜）[F] | `itertools`, generator | Iterator helpers（ES2025）[F] | pkg: `gleam_yielder`（旧 `gleam/iterator`）[F] | `Stream` [F] | `Seq` [F] | `Sequence`（`kotlin.sequences`） | `Sequence`, `LazySequence` | 8/0 |
| 範囲 Range | 構文 `0..n` | `ops::Range` | `for range n` | `range` | — | `list.range` | `Range` [F] | — | `kotlin.ranges` | `Range` | 7/0 |
| 比較・順序 | — | `cmp` | `cmp` | `functools.cmp_to_key` | — | `gleam/order` [F] | — | `compare` | `kotlin.comparisons` | `Comparable` | 7/0 |
| タプル / ペア | 構文 | タプル | — | タプル | — | `gleam/pair` [F] | `Tuple` | `Pair`（5.5）[F] | `Pair`, `Triple` | タプル | 7/0 |
| Option / Result | `option`, `result` | `option`, `result` | —（多値返し） | —（例外） | — | `gleam/option`, `gleam/result` [F] | —（`{:ok, v}` 慣習） | `Option`, `Result`, `Either` [F] | `T?`, `Result` | `Optional`, `Result` | 6/0 |
| エラー | `error` | `error` | `errors` [F] | 例外（builtins） | `Error` | —（Result） | `Exception` | `Printexc` | `Throwable` | `Error` | 8/0 |

### 1.D バイト列・符号化・データ形式

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| バイト列 | `bytes`（146 関数、LE/BE 読み書き込み） | `Vec<u8>`, `slice` | `bytes`, `encoding/binary` | `bytes`, `bytearray`, `struct` | `Uint8Array`, `DataView`, ◐ Node `Buffer` | `gleam/bit_array`, `bytes_tree` [F] | binary, ◐ `:binary` | `Bytes` | `ByteArray` | `[UInt8]`, `Data`(Foundation) | 9/0 |
| Hex | `hex`（3 関数）+ `int.to_hex` | — | `encoding/hex` [F] | `bytes.hex()`, `binascii` | `Uint8Array.toHex/fromHex`（2026）[F], ◐ Deno `@std/encoding` | `bit_array.base16_encode` | `Base.encode16` [F] | —（`Digest.to_hex` だけ） | `HexFormat`, `toHexString`（kotlin.text） | — | 7/0 |
| Base64 | `base64`（4 関数） | — | `encoding/base64` [F] | `base64` | `btoa/atob`, `Uint8Array.toBase64`（2026）[F] | `bit_array.base64_encode` | `Base` [F] | — | `kotlin.io.encoding.Base64`（Common+JVM）[F] | `Data.base64EncodedString`(Foundation) | 7/0 |
| Base32 | — | — | `encoding/base32` [F] | `base64.b32encode` | ◐ Deno `@std/encoding` [F] | — | `Base.encode32` | — | — | — | 3/1 |
| Ascii85 / Base85 / Base58 | — | — | `encoding/ascii85` [F] | `base64.b85encode`, `a85encode` | ◐ Deno（ascii85, base58）[F] | — | — | — | — | — | 2/1 |
| JSON | `json`（15 関数） | — | `encoding/json` [F] | `json` | `JSON` | pkg: `gleam_json` [F] | `JSON`（1.18〜）[F], ◐ `:json`(OTP 27〜)[F] | — | pkg: kotlinx.serialization-json [F] | `JSONEncoder`/`JSONDecoder`(Foundation) | 5/0 |
| 動的値（JSON 風データモデル） | `value`（20 関数） | — | `any` + `reflect` | dict/list | object | `gleam/dynamic`, `dynamic/decode` [F]（読み取り専用の解読器） | 項（term）そのもの | — | pkg: `JsonElement` | `Any` + `JSONSerialization` | 6/0 |
| CSV | — | — | `encoding/csv` [F] | `csv` | ◐ Deno `@std/csv` [F] | — | — | — | — | — | 2/1 |
| TOML | — | — | — | `tomllib`（読み込みのみ）[F] | ◐ Deno `@std/toml`, Bun [F] | — | — | — | — | — | 1/1 |
| YAML | — | — | — | — | ◐ Deno `@std/yaml`, Bun [F] | — | — | — | — | — | 0/1 |
| XML | — | — | `encoding/xml` [F] | `xml.*` | ◐ Deno `@std/xml` [F]（`DOMParser` はブラウザ） | — | ◐ `:xmerl`(OTP) | — | ◐ `javax.xml`(JVM) | `XMLParser`(Foundation) | 3/3 |
| INI / dotenv | — | — | — | `configparser` | ◐ Deno `@std/ini`, `@std/dotenv` [F] | — | — | — | — | ◐ `PropertyListEncoder`(plist) | 1/2 |
| バイナリ直列化（gob/pickle/cbor 等） | — | — | `encoding/gob` [F] | `pickle`, `marshal` | `structuredClone`, ◐ Deno `@std/cbor`, `@std/msgpack` [F] | — | ◐ `:erlang.term_to_binary` | `Marshal` [F] | pkg: serialization-cbor/protobuf [F] | `PropertyListEncoder` | 5/2 |
| URL | `url`（6 関数）+ `http.url_decode` | — | `net/url` [F] | `urllib.parse` | `URL`, `URLSearchParams` | `gleam/uri` [F] | `URI` [F] | — | ◐ `java.net.URI`(JVM) | `URL`, `URLComponents`(Foundation) | 6/1 |
| HTML エスケープ | `html`（`SafeHtml` 型） | — | `html`, `html/template` [F] | `html` | ◐ Deno `@std/html`, `Bun.escapeHTML` [F] | — | — | — | pkg: kotlinx.html（ビルダー、未確認） | — | 2/1 |
| MIME タイプ | — | — | `mime`, `mime/multipart` [F] | `mimetypes`, `email` | ◐ Deno `@std/media-types` [F] | — | — | — | — | — | 2/1 |
| UUID | — | — | `uuid`（v4/v7）[F] | `uuid` [F] | `crypto.randomUUID()`, ◐ Deno `@std/uuid`, `Bun.randomUUIDv7` [F] | — | — | —（Core_unix に Uuid） | `kotlin.uuid.Uuid`（Common+JVM）[F] | `UUID`(Foundation) | 5/0 |
| ULID | — | — | — | — | ◐ Deno `@std/ulid` [F] | — | — | — | — | — | 0/1 |
| バージョン（semver） | — | — | — | — | ◐ Deno `@std/semver`, `Bun.semver` [F] | — | `Version` [F] | — | — | — | 1/1 |

### 1.E ハッシュ・暗号・乱数

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 非暗号ハッシュ（FNV, CRC32, Adler） | `hash.fnv1a32`, `zlib.crc32` | `hash`（SipHash の `DefaultHasher`） | `hash/fnv`, `crc32`, `crc64`, `adler32`, `maphash` [F] | `zlib.crc32`, `adler32`, `hash()` | ◐ `Bun.hash`（ECMAScript にはない） | — | ◐ `:erlang.phash2`, `:erlang.crc32` | `Hashtbl.hash` | `hashCode`, ◐ `java.util.zip.CRC32`(JVM) | `Hasher` | 6/2 |
| SHA-256 | `hash.sha256` | — | `crypto/sha256` [F] | `hashlib` | `crypto.subtle.digest` | pkg: `gleam_crypto` [F] | ◐ `:crypto`(OTP) | —（`Digest` は MD5 と BLAKE2、SHA なし。BLAKE2 は 5.2〜、未確認） | ◐ `MessageDigest`(JVM) | ◐ CryptoKit(Apple) / pkg: swift-crypto | 3/3 |
| MD5 / SHA-1 / SHA-512 / SHA-3 | — | — | `crypto/md5`, `sha1`, `sha512`, `sha3` [F] | `hashlib`（md5, sha1, sha2, sha3, blake2） | `crypto.subtle`（SHA-1/384/512）, ◐ `node:crypto`, Deno `@std/crypto` | pkg: `gleam_crypto` | ◐ `:crypto` | `Digest`（MD5） | ◐ JVM | ◐ CryptoKit / pkg | 4/3 |
| HMAC / KDF | — | — | `crypto/hmac`, `hkdf`, `pbkdf2` [F] | `hmac`, `hashlib.pbkdf2_hmac` | `crypto.subtle`（HMAC, PBKDF2, HKDF） | pkg: `gleam_crypto` | ◐ `:crypto.mac` | — | ◐ `javax.crypto.Mac`(JVM) | ◐ CryptoKit / pkg | 3/2 |
| 暗号（AES, 署名, TLS） | — | — | `crypto/aes`, `ed25519`, `tls`, `mlkem` 等 [F] | `ssl`（暗号プリミティブはない） | `crypto.subtle` | — | ◐ `:crypto`, `:ssl` | — | ◐ JVM | ◐ CryptoKit / pkg | 3/3 |
| 疑似乱数 | `random`（4 関数） | —（`random` は unstable [F]） | `math/rand/v2` [F] | `random` | `Math.random` | `int.random`, `float.random` | `Enum.random`, ◐ `:rand` [F] | `Random` [F] | `kotlin.random` [F] | `Int.random(in:)` | 8/0 |
| シード付き（再現できる）乱数 | — | — | `rand.New(rand.NewPCG(..))` | `random.Random(seed)` | ◐ Deno `@std/random`（seeded） | — | ◐ `:rand.seed` | `Random.State` | `Random(seed)` | —（stdlib に seed 付き生成器はない） | 4/2 |
| 暗号用乱数（CSPRNG） | — | — | `crypto/rand` [F] | `secrets` | `crypto.getRandomValues` | pkg: `gleam_crypto` | ◐ `:crypto.strong_rand_bytes` | — | ◐ `SecureRandom`(JVM) | `SystemRandomNumberGenerator`（CSPRNG） | 4/2 |

### 1.F 圧縮・アーカイブ

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| DEFLATE / zlib / gzip | `zlib` | — | `compress/flate`, `gzip`, `zlib` [F] | `zlib`, `gzip` | `CompressionStream`（gzip/deflate/deflate-raw）, ◐ `node:zlib`, `Bun.gzipSync` [F] | — | ◐ `:zlib`(erts) | — | ◐ `java.util.zip`(JVM) | ◐ Compression(Apple) | 3/3 |
| その他の圧縮（bzip2, lzma, zstd, brotli, lzw） | — | — | `compress/bzip2`（展開のみ）, `lzw` [F] | `bz2`, `lzma`, `compression.zstd` [F] | ◐ `node:zlib`（brotli, zstd）, `Bun.zstd*` [F] | — | ◐ `:zstd`(OTP 29) [F] | — | — | ◐ Compression(Apple) | 2/3 |
| アーカイブ（tar, zip） | — | — | `archive/tar`, `archive/zip` [F] | `tarfile`, `zipfile` | ◐ Deno `@std/tar` [F] | — | ◐ `:zip`, `:erl_tar`(OTP) [F] | — | ◐ `java.util.zip`(JVM) | — | 2/3 |

### 1.G 時間

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 経過時間の型 Duration（実時間） | `duration`（6 関数、構築子と演算のみ） | `time::Duration` | `time.Duration` | `datetime.timedelta` | `Temporal.Duration`（ES2027 予定 [F]） | pkg: `gleam_time`（`gleam/time/duration`）[F] | `Duration`（1.17〜）[F] | —（Core: `Time_ns.Span`） | `kotlin.time.Duration` [F] | `Duration`（5.7〜） | 7/0 |
| 決定的な計算時間（Almide `compute`） | `compute` | — | — | —（`time.process_time` は CPU 時間） | — | — | —（最も近いもの: BEAM の reductions 数。値の型ではない） | —（`Sys.time` は CPU 時間） | — | — | 0/0 |
| 単調時計・ストップウォッチ | `datetime.monotonic_ns` | `time::Instant` | `time.Now`（monotonic 付き）, `time.Since` | `time.monotonic`, `perf_counter` | `performance.now()` | pkg: `gleam_time` | `System.monotonic_time` | ◐ `Unix.gettimeofday`（単調ではない。Stdlib に単調時計なし） | `TimeSource.Monotonic`, `measureTime` [F] | `ContinuousClock`, `SuspendingClock` | 7/1 |
| 現在時刻（Unix 時刻） | `datetime.now`, `env.unix_timestamp`, `env.millis` | `SystemTime` | `time.Now` | `time.time` | `Date.now` | pkg: `gleam_time` | `System.os_time`, `DateTime.utc_now` | ◐ `Unix.time` | `kotlin.time.Clock.System`（2.3〜）[F] | `Date()`(Foundation) | 7/1 |
| 暦の日付・時刻 | `datetime`（Int タイムスタンプ + 部品取り出し） | — | `time.Time` | `datetime`, `calendar` | `Date`, `Temporal.PlainDate*`（2027） | pkg: `gleam_time`（calendar） | `Date`, `Time`, `NaiveDateTime`, `DateTime`, `Calendar` [F] | ◐ `Unix.gmtime/localtime` | pkg: kotlinx-datetime [F] | `Calendar`, `DateComponents`(Foundation) | 5/1 |
| タイムゾーン（IANA） | —（UTC とオフセットのみ） | — | `time.LoadLocation`, `time/tzdata` [F] | `zoneinfo` [F] | `Intl.DateTimeFormat`(timeZone), `Temporal.ZonedDateTime` | — | ◐ `DateTime` + `Calendar.TimeZoneDatabase`（既定は UTC のみ [F]、tz データは外部） | — | pkg: kotlinx-datetime `TimeZone`, ◐ `java.time.ZoneId`(JVM) | `TimeZone`(Foundation) | 4/2 |
| 日時の書式・解析 | `datetime.format`, `parse_iso`, `to_iso` | — | `time.Format`, `time.Parse` | `strftime`, `fromisoformat` | `toISOString`, `Intl` | pkg | `Calendar.strftime`, `DateTime.from_iso8601` | — | `Instant.parse`（ISO）[F] | `ISO8601DateFormatter`, FormatStyle | 6/0 |
| スリープ | `env.sleep_ms`, `process.sleep` | `thread::sleep` | `time.Sleep` | `time.sleep` | `setTimeout`, ◐ `Bun.sleep` | —（pkg: gleam_erlang `process.sleep`） | `Process.sleep` | ◐ `Unix.sleepf` | ◐ `Thread.sleep`(JVM), pkg: coroutines `delay` | `Task.sleep` | 6/2 |
| タイマー（後で実行） | — | — | `time.AfterFunc`, `time.Ticker` | `threading.Timer`, `sched` | `setTimeout`, `setInterval` | — | `:timer`(OTP), `Process.send_after` | — | pkg: coroutines | `Task` + `Clock.sleep` | 4/1 |

### 1.H OS・ファイル・プロセス

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| ファイルシステム | `fs`（34 関数） | `fs` [F] | `os`, `io/fs` [F] | `os`, `shutil`, `pathlib` | ◐ `node:fs`, `Deno.*`, `@std/fs`, `Bun.file` | —（simplifile は外部） | `File` [F] | `Sys`, `In_channel`, `Out_channel` [F], ◐ `Unix` | ◐ `kotlin.io`(JVM の `File`), pkg: kotlinx-io | `FileManager`(Foundation) | 6/2 |
| パス操作 | `path` | `path` [F] | `path`, `path/filepath` [F] | `os.path`, `pathlib` | ◐ `node:path`, `@std/path` | — | `Path` [F] | `Filename` [F] | ◐ `kotlin.io.path`(JVM)[F] | `URL`(file), pkg: swift-system `FilePath` | 6/2 |
| 一時ファイル | `fs.temp_dir`, `create_temp_file`, `create_temp_dir`, `env.temp_dir` | `env::temp_dir`（ディレクトリ名のみ） | `os.CreateTemp`, `os.MkdirTemp` | `tempfile` | ◐ `Deno.makeTempFile`, `fs.mkdtemp` | — | `System.tmp_dir` | `Filename.temp_file` | ◐ `createTempFile`(JVM) | `FileManager.temporaryDirectory` | 6/2 |
| glob | `fs.glob` | — | `filepath.Glob`, `fs.Glob` | `glob`, `fnmatch` | ◐ `fs.glob`(Node 22〜), `Bun.Glob`, Deno `expandGlob` | — | `Path.wildcard` | — | ◐ `PathMatcher`(JVM) | — | 3/2 |
| ファイル監視 | — | — | — | — | ◐ `fs.watch`, `Deno.watchFs` | — | — | — | ◐ `WatchService`(JVM) | ◐ FSEvents(Apple) | 0/3 |
| 標準入出力 | `io`, `println`, `eprintln` | `io` | `os.Stdin`, `bufio`, `fmt` | `sys.stdin`, `print`, `input` | `console`, ◐ `process.stdin`, `Deno.stdin` | `gleam/io`（出力のみ。stdin 読みはない）[F] | `IO` [F] | `In_channel`, `Out_channel`, `print_*` | `println`, `readln` | `print`, `readLine` | 8/1 |
| 端末 / ANSI 色 / TTY | — | `io::IsTerminal` | —（x/term は外部） | `curses`, `tty`, `termios`, `os.isatty` | ◐ `node:tty`, `util.styleText`, `@std/fmt/colors`, `Bun.stripANSI` | — | `IO.ANSI` [F], ◐ `io_ansi`(OTP)[F] | ◐ `Unix.isatty` | — | — | 3/2 |
| 環境変数 | `env.get`, `env.set` | `env::var` | `os.Getenv` | `os.environ` | `process.env`, `Deno.env` | — | `System.get_env` | `Sys.getenv` | ◐ `System.getenv`(JVM)（Common には無い） | `ProcessInfo.environment` | 7/1 |
| コマンドライン引数（生） | `env.args`, `args.raw` | `env::args` | `os.Args` | `sys.argv` | `process.argv`, `Deno.args`, `Bun.argv` | — | `System.argv` | `Sys.argv` | `main(args)` | `CommandLine.arguments` | 8/0 |
| フラグ解析 | `args`（6 関数: flag, option, positional） | — | `flag` [F] | `argparse`, `getopt`, `optparse` [F] | ◐ `util.parseArgs`(Node), `@std/cli` [F] | — | `OptionParser` [F], ◐ `argparse`(OTP)[F] | `Arg` [F] | —（kotlinx-cli は公式 API 一覧に無い [F]） | pkg: swift-argument-parser | 4/1 |
| 作業ディレクトリ | `env.cwd` | `env::current_dir` | `os.Getwd` | `os.getcwd` | `process.cwd()` | — | `File.cwd` | `Sys.getcwd` | ◐ JVM | `FileManager.currentDirectoryPath` | 7/1 |
| 子プロセス実行 | `process`（17 関数） | `process` [F] | `os/exec` [F] | `subprocess` | ◐ `child_process`, `Deno.Command`, `Bun.spawn` | — | `System.cmd`, `Port` [F] | `Sys.command`, ◐ `Unix.create_process` | ◐ `ProcessBuilder`(JVM) | `Process`(Foundation) | 6/3 |
| 終了コード exit | `process.exit` | `process::exit` | `os.Exit` | `sys.exit` | `process.exit` | — | `System.halt` | `exit` | `exitProcess`（JVM+Native [F]） | `exit` | 8/0 |
| シグナル | `process.kill`（送信のみ） | — | `os/signal` [F] | `signal` | ◐ `process.on('SIGINT')`, `Deno.addSignalListener` | — | `System.trap_signal` | `Sys.signal` | — | ◐ Dispatch `DispatchSource` | 4/2 |
| OS 情報・ユーザー | `env.os` | `env::consts` | `runtime.GOOS`, `os/user` [F] | `platform`, `os`, `getpass`, `pwd` | ◐ `node:os` | — | `:os.type`, `System` | `Sys.os_type` | ◐ JVM | `ProcessInfo` | 5/2 |
| ロギング | — | pkg: `log`（rust-lang） | `log`, `log/slog` [F] | `logging` | `console.*`, ◐ Deno `@std/log` [F] | — | `Logger`（同梱アプリ） | — | — | pkg: swift-log, ◐ `os.Logger`(Apple) | 5/0 |
| データベース（SQLite 等） | — | — | `database/sql`（接続口のみ）[F] | `sqlite3` | ◐ `node:sqlite` [F], `bun:sqlite` | — | ◐ `:ets`, `:dets`, mnesia(OTP) | — | — | — | 2/2 |

### 1.I 並行・非同期

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| スレッド / 並列実行 | 言語機能 `fan`（stdlib モジュールではない） | `thread` [F] | goroutine（言語）+ `runtime` | `threading`, `multiprocessing`, `concurrent.futures` | ◐ `worker_threads`, Web Worker | —（pkg: gleam_otp, gleam_erlang） | `Task`, `Process`, `Agent` [F] | `Domain` [F], ◐ `Thread` | ◐ `kotlin.concurrent`(JVM+Native)[F], pkg: coroutines | `Task`（Concurrency）, `Thread`(Foundation) | 6/2 |
| チャネル / メッセージ | — | `sync::mpsc` | `chan`（言語） | `queue.Queue`, `asyncio.Queue` | `MessageChannel` | —（pkg: gleam_erlang `Subject`） | `send`/`receive`, `GenServer` | ◐ `Event`(threads) | pkg: coroutines `Channel` | `AsyncStream` | 6/1 |
| ロック・アトミック | — | `sync`（Mutex, Arc, Atomic*） | `sync`, `sync/atomic` | `threading.Lock` | `Atomics`, `SharedArrayBuffer` | — | ◐ `:atomics`(OTP) | `Mutex`, `Atomic`, `Condition`, `Semaphore` [F] | `kotlin.concurrent.atomics` [F] | `Synchronization`（`Atomic`, `Mutex`） | 7/1 |
| async / await と実行器 | — | `future`, `task`（実行器はない） | — | `asyncio` | `Promise`, async 関数 | — | `Task.async/await` | `Effect`（実行器はない） | 言語 `suspend` + pkg: coroutines | `async`/`await`, `Task`, actor | 5/0 |
| 取り消し / 締め切り | `fan.timeout`（言語） | — | `context` [F] | `asyncio.timeout` | `AbortController`, `AbortSignal.timeout` | — | `Task.yield/shutdown` | — | pkg: coroutines `withTimeout` | `Task.cancel` | 5/0 |

### 1.J ネットワーク

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| TCP | `net.tcp_*` | `net::TcpStream`, `TcpListener` [F] | `net` [F] | `socket`, `socketserver` | ◐ `node:net`, `Deno.connect`, `Bun.connect` | — | ◐ `:gen_tcp`(OTP kernel) | ◐ `Unix` ソケット | ◐ `java.net.Socket`(JVM), pkg: ktor-network | —（pkg: swift-nio） | 3/4 |
| UDP | — | `net::UdpSocket` | `net` | `socket` | ◐ `node:dgram` [F], Deno | — | ◐ `:gen_udp` | ◐ `Unix` | ◐ `DatagramSocket`(JVM) | — | 3/4 |
| Unix ドメインソケット | `net.unix_*`, `net.shm_*` | `os::unix::net` | `net` | `socket`(AF_UNIX) | ◐ `node:net`(path) | — | ◐ `:gen_tcp` `{:local, path}` | ◐ `Unix` | ◐ JVM 16〜 | — | 3/4 |
| DNS 名前解決 | —（connect 内で暗黙） | `ToSocketAddrs` | `net.LookupHost` | `socket.getaddrinfo` | ◐ `node:dns`, `Deno.resolveDns` | — | ◐ `:inet.gethostbyname` | ◐ `Unix.getaddrinfo` | ◐ `InetAddress`(JVM) | — | 3/4 |
| IP アドレス型 | — | `net::IpAddr` | `net/netip` [F] | `ipaddress` | ◐ `net.isIP`, `BlockList` | — | ◐ `:inet.parse_address` | ◐ `Unix.inet_addr_of_string` | ◐ JVM | — | 3/4 |
| HTTP クライアント | `http.get` ほか | — | `net/http` [F] | `urllib.request`, `http.client` | `fetch` | pkg: `gleam_httpc`, `gleam_fetch`（型は `gleam_http`）[F] | ◐ `:httpc`(OTP inets) | — | ◐ `java.net.http`(JVM), pkg: Ktor | `URLSession`(Foundation) | 4/2 |
| HTTP サーバ | `http.serve` | — | `net/http` | `http.server`（開発用） | ◐ `node:http`, `Deno.serve`, `Bun.serve`, `@std/http` | pkg: `gleam_httpd`? (org に `httpd` リポジトリ [F]、パッケージ名は未確認) | ◐ `:httpd`(inets) | — | pkg: Ktor | — | 2/2 |
| TLS | （http 内部のみ） | — | `crypto/tls` [F] | `ssl` | ◐ `node:tls` | — | ◐ `:ssl` | — | ◐ JVM | ◐ Network.framework(Apple) | 2/4 |
| WebSocket | — | — | — | — | `WebSocket`（Node 22〜 クライアント, Deno, Bun） | — | — | — | pkg: Ktor | ◐ `URLSessionWebSocketTask` | 1/1 |
| メール / SMTP | — | — | `net/mail`, `net/smtp` [F] | `email`, `smtplib` | — | — | — | — | — | — | 2/0 |

### 1.K テスト

| カテゴリ | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| テスト枠組み・アサート | `testing` + 言語 `test` ブロック | `#[test]`, `assert!`（言語+std） | `testing` [F] | `unittest`, `doctest` | ◐ `node:test`, `Deno.test`, `@std/assert`, `bun:test` | —（gleeunit は外部） | ExUnit（同梱） | — | pkg: `kotlin.test` [F] | XCTest / swift-testing（ツールチェーン同梱） | 5/1 |
| ベンチマーク | CLI `almide bench` | （unstable `#[bench]`） | `testing.B` | `timeit` | ◐ `Deno.bench` | — | — | — | — | — | 2/1 |
| プロパティテスト | — | — | `testing/quick` [F] | — | — | — | — | — | — | — | 1/0 |

---

## 2. 「何言語が標準に持つか」の集計（Almide に無いものだけ、多い順）

● = 無条件に標準、◐ = 条件つき（0.1 参照）。「● + ◐ ≥ 3」のものを上に並べた。

| Almide に無いカテゴリ | ● | ◐ | 合計 | 備考 |
|---|---:|---:|---:|---|
| 遅延シーケンス / イテレータ | 8 | 0 | 8 | Gleam も公式 pkg（gleam_yielder） |
| UTF-16 / Latin-1 等の符号化 | 6 | 2 | 8 | |
| 文字列ビルダー | 8 | 0 | 8 | JS だけ無い |
| 書式化 printf | 5 | 3 | 8 | |
| ロック・アトミック | 7 | 1 | 8 | |
| スレッド（モジュールとして） | 6 | 2 | 8 | Almide は言語機能 `fan` で並列 |
| 両端キュー / キュー | 4 | 3 | 7 | Swift は公式 pkg |
| UDP | 3 | 4 | 7 | |
| DNS 名前解決（明示） | 3 | 4 | 7 | |
| IP アドレス型 | 3 | 4 | 7 | |
| チャネル / メッセージ | 6 | 1 | 7 | |
| CSPRNG | 4 | 2 | 6 | Gleam・Swift(pkg) も公式 pkg |
| シード付き乱数 | 4 | 2 | 6 | |
| ヒープ / 優先度キュー | 4 | 2 | 6 | Swift は公式 pkg |
| タイムゾーン（IANA） | 4 | 2 | 6 | Kotlin は公式 pkg |
| 任意精度整数 | 4 | 2 | 6 | |
| バイナリ直列化 | 5 | 2 | 7 | |
| XML | 3 | 3 | 6 | |
| 非暗号ハッシュの充実（CRC64, maphash 等） | — | — | — | Almide は fnv1a32 と crc32 だけ |
| MD5 / SHA-1 / SHA-512 / SHA-3 | 4 | 3 | 7 | |
| ソート済みマップ | 2 | 3 | 5 | |
| UUID | 5 | 0 | 5 | Go は 1.27 で std 入り [F] |
| ロギング | 5 | 0 | 5 | Rust/Swift は公式 pkg |
| HMAC / KDF | 3 | 2 | 5 | |
| 暗号（AES, 署名, TLS） | 3 | 3 | 6 | |
| async / await | 5 | 0 | 5 | Almide は `fan` が別設計 |
| 取り消し / 締め切り | 5 | 0 | 5 | Almide は `fan.timeout` |
| 端末 / ANSI / TTY | 3 | 2 | 5 | |
| その他の圧縮（bzip2, zstd 等） | 2 | 3 | 5 | |
| アーカイブ（tar, zip） | 2 | 3 | 5 | |
| フラグ解析の本格版（ヘルプ生成・型付き） | 4 | 1 | 5 | Almide `args` は最小の読み取り器 |
| 10 進小数 / 有理数 | 3 | 1 | 4 | |
| Base32 | 3 | 1 | 4 | |
| タイマー | 4 | 1 | 5 | |
| データベース（SQLite 等） | 2 | 2 | 4 | |
| 挿入順マップ（独立型として） | 4 | 0 | 4 | Almide の `map` 自体が挿入順 |
| SIMD | 3 | 0 | 3 | |
| 複素数 | 3 | 0 | 3 | |
| テンプレートエンジン | 3 | 0 | 3 | |
| ロケール / 国際化書式 | 3 | 1 | 4 | |
| CSV | 2 | 1 | 3 | |
| MIME タイプ | 2 | 1 | 3 | |
| Ascii85 / Base58 | 2 | 1 | 3 | |
| ファイル監視 | 0 | 3 | 3 | すべて条件つき |
| 128 bit 整数 | 2 | 0 | 2 | |
| TOML | 1 | 1 | 2 | |
| INI / dotenv | 1 | 2 | 3 | |
| semver | 1 | 1 | 2 | |
| WebSocket | 1 | 1 | 2 | |
| メール / SMTP | 2 | 0 | 2 | |
| 統計 | 1 | 0 | 1 | |
| YAML | 0 | 1 | 1 | Deno/Bun だけ |
| ビットセット | 0 | 1 | 1 | Swift は公式 pkg |
| 多重集合 Counter | 1 | 0 | 1 | |

Almide にあって、他言語の標準ではまれなもの:

| Almide モジュール | 標準に持つ言語数 ●/◐ | 事実 |
|---|---|---|
| `matrix` | 0/2 | 9 言語とも標準に行列演算はない。OCaml `Bigarray` は入れ物だけ、Swift `simd` は Apple だけ |
| `compute` | 0/0 | 決定的な計算時間を値の型として持つ標準は無い |
| `html`（SafeHtml 型） | 2/1 | エスケープ関数は Go/Python/Deno/Bun。「安全な HTML を別の型にする」設計は Go `html/template` の `template.HTML` 型だけが近い |
| `hex` を独立モジュール | — | 独立モジュールは Go `encoding/hex` だけ。多くは base64 と同じ場所 |
| `zlib` | 3/3 | Rust・Gleam・OCaml には無い |
| `testing`（アサート集） | 5/1 | |

---

## 3. Almide の「分け方・まとめ方」が多数派と違って見える点 — 9 言語の事実

### 3.1 固定幅整数を 8 モジュールに分けている（int8〜uint64）

Almide: 型ごとにモジュール（`int8` 20 関数 … `uint64` 26 関数）。中身は幅変換（`to_<型>`, `to_<型>_checked`, `to_<型>_saturating`）, `to_string`, `min_value`/`max_value`。さらに `int` の中にも `to_int8`, `to_uint8_checked`, `from_uint64` 等と、型を返さない `int.to_u8`, `int.to_u32`（戻り値は Int）がある。

| 言語 | 構成の事実 |
|---|---|
| Rust | `i8`〜`u128` は primitive 型。メソッドは型に直接付く（`i8::MAX`, `checked_add`, `saturating_sub`, `wrapping_mul`, `to_le_bytes`）。型変換は `as`（切り詰め / float→int は飽和）, `From`（損失なし）, `TryFrom`（失敗あり）の 3 系統。古い `std::i8` 等のモジュールは非推奨 [F: std 一覧に i8 モジュールは出ない] |
| Go | 型は言語組み込み。関数は用途別パッケージ: 定数 `math.MaxInt8`、ビット操作 `math/bits`、文字列化 `strconv.FormatInt(int64(x),10)`、バイト化 `encoding/binary`。型ごとのパッケージはない。変換は `int8(x)`（切り詰めのみ） |
| Python | 固定幅整数の型はない（int は任意精度）。幅は `struct`, `array` の型コード、`ctypes.c_int8` で表すだけ |
| TS/JS | Number（f64）と BigInt。幅は `Int8Array` 等の型付き配列、`BigInt.asIntN(n, x)`、`x | 0`、`x >>> 0` |
| Gleam | Int のみ。幅は bit array の `<<x:size(8)>>` |
| Elixir | 任意精度整数のみ。幅は `<<x::signed-8>>` |
| OCaml | `Int`（63 bit）, `Int32`, `Int64`, `Nativeint` の 4 モジュール [F]。8/16 bit はない（`Bytes.get_int8`, `get_int16_le` で読むだけ） |
| Kotlin | `Byte`〜`ULong` の型に変換メンバー（`toByte()`, `toUInt()`）。切り詰めのみで checked/saturating の標準関数はない（`coerceIn` で手書き） |
| Swift | `Int8`〜`UInt64`（+ `Int128`）の構造体。変換は初期化子 3 種: `init(truncatingIfNeeded:)`（切り詰め）, `init(clamping:)`（飽和）, `init(exactly:)`（失敗で nil）。Almide の「三つ組」に一番近い |

### 3.2 `float32` / `float64` モジュールと `float`

Almide: `float`（46 関数）が既定。`float32`, `float64` は各 10 関数の幅変換。

| 言語 | 事実 |
|---|---|
| Rust | `f32`, `f64` は primitive（メソッドは型に付く）。`std::f32`, `std::f64` モジュールは定数（`consts::PI`）だけ [F] |
| Go | `float32`, `float64` 型。`math` は float64 だけ。`math.Float32bits` 等 |
| Python / Gleam / Elixir / OCaml | 浮動小数は 64 bit の 1 種だけ。32 bit は配列やバイト列の表現だけ |
| JS | Number（64 bit）。`Math.fround`, `Float32Array`, `Float16Array`（2025）[F] |
| Kotlin | `Float`, `Double` 型にメンバー |
| Swift | `Float`, `Double`, `Float16`, `Float80` 型にメンバー |

→ 型ごとに「モジュール」を持つ言語は 9 言語中 0。型にメソッドを付けるか（Rust, Kotlin, Swift）、用途別パッケージ（Go）。Almide の float32/float64 は UFCS で `x.to_int64()` と書けるので、使い方は「型にメソッド」型に近い。

### 3.3 `compute` と `duration`（と `datetime`, `env` の時間関数）

Almide: `duration`（実時間、6 関数）, `compute`（決定的計算時間、6 関数）, `datetime`（Int タイムスタンプ、22 関数, `now` と `monotonic_ns`）, `env.unix_timestamp`, `env.millis`, `env.sleep_ms`, `process.sleep`。

| 言語 | 時間の置き場所 |
|---|---|
| Rust | `std::time` 1 モジュールに `Duration`, `Instant`, `SystemTime`。sleep は `std::thread::sleep` |
| Go | `time` 1 パッケージに Duration, Time, Sleep, Timer, Location（tz） |
| Python | `time`（時計・sleep）, `datetime`（暦・timedelta）, `zoneinfo`（tz）, `calendar` |
| JS | `Date`, `performance.now`, `setTimeout`、新しく `Temporal`（Duration, Instant, ZonedDateTime …、2027 版予定）[F] |
| Gleam | stdlib に時間なし。公式 pkg `gleam_time`（duration, timestamp, calendar）[F] |
| Elixir | `Duration`, `Date`, `Time`, `DateTime`, `NaiveDateTime`, `Calendar`（別モジュール）[F] + `System.monotonic_time`, `Process.sleep` |
| OCaml | Stdlib に時計は `Sys.time`（CPU 時間）だけ。実時間は `Unix`（同梱）。Core に `Time_ns` |
| Kotlin | `kotlin.time` に `Duration`, `TimeSource`, `Instant`, `Clock` [F]。暦と tz は pkg kotlinx-datetime |
| Swift | stdlib に `Duration`, `Clock` プロトコル（`ContinuousClock`, `SuspendingClock`）。暦は Foundation `Date`, `Calendar`, `TimeZone` |

事実:
- 「計算量の時計」を実時間と型で分けている言語はない。近いもの: BEAM の reductions 数（Elixir `Process.info(pid, :reductions)`）、CPU 時間（Python `time.process_time`, OCaml `Sys.time`）。どれも Duration とは別の値で、予算の型ではない。
- Swift の `Clock` プロトコルは「時計ごとに Instant 型が違い、Duration は共有」という形。Almide は時計ごとに量の型自体が違う（`Compute` と `Duration` は混ぜられない）。
- 多数派（Rust, Go, Kotlin）は「経過時間・時刻・sleep」を 1 モジュールにまとめる。Almide は現在時刻が `datetime` と `env` に、sleep が `env` と `process` に重複している。

### 3.4 引数: `args` / `env.args` / `process`

Almide: `env.args()`（argv[1..]）, `args.raw()`（README の一行説明は「env.args と同じ、argv[0] を除く」。ただし args.md の本文は「argv[0] を含む」と書いてあり、記述が食い違っている）, `args.flag/option/positional`。

| 言語 | 生の argv | フラグ解析 |
|---|---|---|
| Rust | `std::env::args` | なし（clap は外部） |
| Go | `os.Args` | `flag` |
| Python | `sys.argv` | `argparse`（+ 旧 `getopt`, `optparse`） |
| JS | `process.argv`, `Deno.args`, `Bun.argv` | `util.parseArgs`(Node), `@std/cli`(Deno) |
| Gleam | なし（外部 `argv`） | なし |
| Elixir | `System.argv` | `OptionParser`（+ OTP `argparse`） |
| OCaml | `Sys.argv` | `Arg`（Core は `Command`） |
| Kotlin | `main(args)` の引数 | なし（kotlinx-cli は公式 API 一覧に出ていない [F]） |
| Swift | `CommandLine.arguments` | 公式 pkg swift-argument-parser |

→ 生の argv は「環境/OS」側（Rust `env`, Go `os`, Python `sys`, Elixir `System`, OCaml `Sys`）。フラグ解析は別モジュール（Go `flag`, Python `argparse`, Elixir `OptionParser`, OCaml `Arg`）。この 2 段構成は Almide の `env.args` + `args` と同じ形。違いは、Almide の `args` も生の argv を返す関数（`raw`）を持っていて、二重になっている点。

### 3.5 `value` と `json`

Almide: `value`（データモデル 20 関数: 構築・取り出し・`stringify`・`pick`/`omit`・キー名の camel/snake 変換）, `json`（`parse`, `stringify`, `get_*`, `JsonPath`）。`value.stringify` と `json.stringify` が重複。

| 言語 | 事実 |
|---|---|
| Go | `encoding/json` が `any`（map[string]any 等）に解析する。専用の値型はない |
| Python | `json` が dict/list/str に解析する。専用の値型はない |
| JS | `JSON` が普通のオブジェクトに解析する |
| Gleam | stdlib `gleam/dynamic` + `gleam/dynamic/decode`（型のない値を型付きに解読する）[F]。JSON 本体は公式 pkg `gleam_json`。値を作る側は gleam_json の `json.object` 等 |
| Elixir | `JSON`（1.18〜）が map/list に解析する。値は普通の項 |
| Kotlin | pkg kotlinx.serialization: `JsonElement`（`JsonObject`, `JsonArray`, `JsonPrimitive`）が JSON 専用の値型 |
| Swift | `JSONDecoder` は型に直接（Codable）。型のない形は `JSONSerialization`（Any） |
| Rust / OCaml | 標準に無い（serde_json, yojson は外部） |

→ 「形式に依存しない動的値の型」を標準に置いているのは Gleam（`dynamic`）だけ。ただし Gleam の dynamic は読み取り専用で、構築子は JSON パッケージ側にある。JSON 専用の値型を持つのは Kotlin（pkg）。

### 3.6 `hex` と `base64` を別モジュールにしている

| 言語 | 置き場所 |
|---|---|
| Go | 別パッケージだが `encoding/` の下に並ぶ: `encoding/hex`, `base32`, `base64`, `ascii85`, `pem`, `csv`, `json`, `xml`, `binary` [F] |
| Python | 1 モジュール `base64`（b16, b32, b64, b85, a85）+ `binascii` + `bytes.hex()` |
| JS | `Uint8Array` のメソッド `toBase64/fromBase64/toHex/fromHex`（2026）[F]。Deno は 1 パッケージ `@std/encoding` [F] |
| Gleam | 両方とも `gleam/bit_array` の関数（`base16_encode`, `base64_encode`, `base64_url_encode`） |
| Elixir | 1 モジュール `Base`（encode16/32/64） |
| Kotlin | Base64 は `kotlin.io.encoding.Base64`、hex は `kotlin.text.HexFormat`（別の場所） |
| Swift | Base64 は Foundation `Data` のメソッド。hex は標準に無い |
| Rust / OCaml | どちらも標準に無い |

→ 「hex と base64 を 1 か所（`Base`, `base64`, `@std/encoding`, `bit_array`）」が 5 言語。別々だが共通の親を持つのが Go。完全にばらばらなのは Kotlin。

### 3.7 `hash`（FNV と SHA-256 を同居）と `zlib.crc32`

| 言語 | 非暗号ハッシュと暗号ハッシュの置き場所 |
|---|---|
| Go | 非暗号は `hash/*`（fnv, crc32, crc64, adler32, maphash）、暗号は `crypto/*`（sha256 等）。共通インターフェース `hash.Hash` [F] |
| Python | 暗号は `hashlib`、CRC/Adler は `zlib` と `binascii`、`hash()` は組み込み |
| JS | 暗号は `crypto.subtle.digest`、非暗号は標準に無い（Bun.hash だけ） |
| Elixir | `:crypto.hash`(OTP)、`:erlang.phash2`, `:erlang.crc32` |
| OCaml | `Digest`（MD5, BLAKE2）、`Hashtbl.hash` |
| Rust | `std::hash` は Hasher トレイトと SipHash だけ。暗号は無い |

→ Almide の「FNV と SHA-256 が同じ `hash`、CRC32 は `zlib`」は、Python の分け方（crc32 は zlib）に近く、Go の分け方（非暗号と暗号を別ツリー）とは違う。

### 3.8 `html`

標準でエスケープを持つのは Go `html`（+ `html/template`）, Python `html`, Deno `@std/html`, Bun `escapeHTML`。「エスケープ済み文字列を別の型にする」のは Go `html/template` の `template.HTML` 型（信頼済みの印）。Rust, Gleam, Elixir（Phoenix.HTML は外部）, OCaml, Kotlin（kotlinx.html は pkg）, Swift には標準で無い。

### 3.9 `matrix`

9 言語とも標準に行列演算は無い（2 章の表）。数値計算は外部（numpy, ndarray, gonum, Nx, Owl, multik, swift-numerics の一部）。Almide は epoch 14 で NN 用の 25 関数を `almide/nnlib` パッケージへ移した（matrix.md 冒頭）。

### 3.10 `env` に入っているもの

Almide `env`: unix_timestamp, args, get, set, cwd, millis, sleep_ms, temp_dir, os（9 関数）。

| 言語 | 「env」相当に入るもの |
|---|---|
| Rust `std::env` | args, vars, var, set_var, current_dir, temp_dir, consts（OS 名）。時刻や sleep は入らない |
| Go `os` | Args, Getenv, Setenv, Getwd, TempDir, Exit, ファイル操作 |
| Python `os` / `sys` | environ, getcwd / argv, exit, platform |
| Elixir `System` | argv, get_env, cwd, tmp_dir, os_time, monotonic_time, cmd, halt（時刻も入る） |
| OCaml `Sys` | argv, getenv, getcwd, command, time（CPU）, os_type |

→ Rust `env` と Almide `env` は中身がほぼ同じだが、Rust は時刻・sleep を `time`/`thread` に置く。時刻を同居させているのは Elixir `System` と OCaml `Sys`。

### 3.11 `io`（stdin/stdout）と `println`

全 9 言語に標準入出力がある。Gleam `gleam/io` は出力のみで stdin を読む関数がない [F]。Almide `io.write_bytes(List[Int])` と `io.write(Bytes)` の 2 系統がある。

### 3.12 `net`（TCP + Unix ソケット + 共有メモリ）

標準に「Unix ドメインソケットと共有メモリ」を TCP と同じモジュールに置くのは Rust（`std::os::unix::net` は別モジュール）・Go（`net` に TCP/UDP/Unix が同居、共有メモリは無い）。共有メモリ（shm）を標準に持つのは Python `multiprocessing.shared_memory` と `mmap`。UDP を持たずに Unix ソケットと shm を持つ組み合わせは 9 言語に無い。

### 3.13 Almide 内部の重複（事実のみ）

| 機能 | Almide 内の場所 |
|---|---|
| argv | `env.args`, `args.raw` |
| 現在時刻 | `datetime.now`, `env.unix_timestamp`, `env.millis` |
| sleep | `env.sleep_ms`（負で hang）, `process.sleep`（負は 0） |
| 一時ディレクトリ | `env.temp_dir`, `fs.temp_dir` |
| JSON 文字列化 | `value.stringify`, `json.stringify` |
| URL デコード | `url.decode_component`, `http.url_decode` |
| 32/8 bit への切り詰め | `int.to_u32`/`int.to_u8`（Int を返す）, `int.to_uint32`/`int.to_uint8`（UInt 型を返す） |
| hex | `hex.encode`（Bytes）, `int.to_hex`（Int） |
| CRC32 | `zlib.crc32`（`hash` ではない） |

---

## 4. Almide に無いカテゴリが WASM/WASI でホスト機能を要るか

WASI の公式インターフェース（Phase 3 = p2/p3 に入るもの）: `wasi:cli`（引数・環境変数・stdin/stdout/stderr・exit・terminal 判定）, `wasi:clocks`（monotonic-clock, wall-clock/system-clock）, `wasi:filesystem`, `wasi:sockets`（tcp, udp, ip-name-lookup）, `wasi:random`（random, insecure, insecure-seed）, `wasi:http`（incoming/outgoing handler）。Phase 2: clocks の `timezone`。Phase 1: `wasi-crypto`, `wasi-tls`, `wasi-kv-store`, `wasi-native-threads` [F]。プロセス生成とシグナルは WASI に無い。

| 無いカテゴリ | 必要なもの | WASI での状況 |
|---|---|---|
| 遅延シーケンス / イテレータ | 純粋計算 | 不要 |
| 文字列ビルダー | 純粋計算 | 不要 |
| printf / テキスト整形 | 純粋計算 | 不要 |
| UTF-16 / Latin-1 符号化 | 純粋計算 | 不要 |
| 両端キュー・ヒープ・ソート済みマップ・ビットセット・Counter | 純粋計算 | 不要 |
| 任意精度整数・10 進小数・複素数・統計 | 純粋計算 | 不要 |
| Base32 / Ascii85 / Base58 | 純粋計算 | 不要 |
| CSV / TOML / YAML / XML / INI | 純粋計算（読み書きは文字列） | 不要 |
| バイナリ直列化（cbor, msgpack） | 純粋計算 | 不要 |
| MIME タイプ表 | 純粋計算（表を埋め込む） | 不要 |
| UUID v4 | 乱数 | `wasi:random` |
| UUID v7 / ULID | 乱数 + 現在時刻 | `wasi:random` + `wasi:clocks` wall-clock |
| MD5 / SHA-1 / SHA-512 / SHA-3 / HMAC / KDF | 純粋計算 | 不要（`wasi-crypto` は Phase 1 で使えない） |
| AES・署名 | 純粋計算（鍵の乱数は `wasi:random`） | 不要（ただし定数時間実装が課題） |
| CSPRNG | 乱数 | `wasi:random/random`（暗号用と明記） |
| シード付き乱数 | 純粋計算 | 不要（シードの初期値に `insecure-seed` を使う道もある） |
| 他の圧縮（bzip2, zstd, lzma）・tar/zip | 純粋計算 | 不要 |
| タイムゾーン（IANA） | tz データ（埋め込みなら純粋）。ローカル tz の取得はホスト | 埋め込み: 不要。ローカル tz: `wasi:clocks/timezone`（Phase 2、まだ安定でない） |
| タイマー（後で実行） | 時計 + 実行器 | `wasi:clocks` monotonic-clock の `subscribe`/p3 の async |
| ロギング | 出力先は stderr / ファイル | `wasi:cli` stderr、`wasi:filesystem`（logging 自体の新インターフェースは wasi-logging が Phase 1 系、未確認） |
| 端末 / ANSI / TTY | 色付けは純粋。TTY 判定はホスト | `wasi:cli/terminal-stdout` 等 |
| フラグ解析の本格版 | 引数（既に `env.args`） | `wasi:cli/environment` |
| シグナル | ホスト | WASI に無い |
| ファイル監視 | ホスト | WASI に無い |
| データベース（SQLite） | ファイル | `wasi:filesystem`（SQLite を wasm に同梱する場合） |
| UDP | ソケット | `wasi:sockets/udp` |
| DNS 名前解決 | ソケット | `wasi:sockets/ip-name-lookup` |
| IP アドレス型 | 純粋計算 | 不要 |
| TLS | ソケット + 暗号 | `wasi-tls` は Phase 1。`wasi:http` 経由なら https はホストが処理 |
| WebSocket | ソケット | `wasi:sockets/tcp`（+ TLS） |
| メール / SMTP | ソケット | `wasi:sockets/tcp` |
| スレッド / ロック / アトミック / チャネル | 実行器 | `wasi-native-threads` は Phase 1。p3 の async（stream/future）は協調的並行。共有メモリスレッドは無い |
| async / 取り消し | 実行器 | p3 の async で表現（Almide は `fan` で既に持つ） |
| 128 bit 整数・SIMD | 純粋計算 | 不要（SIMD は wasm simd128 命令） |
| 行列（既にある） | 純粋計算 | 不要 |

---

## 5. 注記・確かめきれなかったこと

- Swift 標準ライブラリの本文ページは JavaScript 描画のため取得できなかった。Swift のセル（`Duration` 5.7, `Regex` 5.7, `Int128` 6.0, `Synchronization` モジュール, `SystemRandomNumberGenerator` が CSPRNG）は記憶による。
- OCaml `Digest` の BLAKE2 対応（5.2〜）は Pqueue ページの取得では確かめられなかった（記憶）。
- Kotlin `HexFormat`（kotlin.text）、`Base64` の安定化時期は確かめていない（存在は kotlin.io.encoding パッケージ一覧で確認 [F]）。
- Gleam の HTTP サーバ用公式パッケージ: gleam-lang org に `httpd` リポジトリがあることは確認 [F]、パッケージ名と用途は未確認。
- Elixir の `JSON` が 1.18 から、`Duration` が 1.17 からという版数は記憶。存在は 1.20 のモジュール一覧で確認 [F]。
- Go の `uuid` パッケージ: std に存在し v4/v7 を作ることは確認 [F]。追加された版（1.27 と推定）は確認できていない。
- Python 3.15 の module index には削除済みモジュール（cgi, imp 等）も旧版記録として残っている。表では使っていない。
- 「数」は 0.2 の範囲の決め方に強く依存する。特に JS（Deno/Bun/Node を含めるか）、Elixir（OTP を含めるか）、Kotlin（JVM を含めるか）で ◐ が増減する。
